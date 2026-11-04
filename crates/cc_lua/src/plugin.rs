//! Plugins: a folder with a `plugin.toml` manifest and Lua code that adds
//! commands to the session.
//!
//! ```toml
//! id = "room_tools"        # lowercase letters, digits and _, no "__"
//! name = "Room tools"
//! version = "0.1.0"
//! api = 1                  # the plugin API it was written for
//! description = "…"
//! authors = ["…"]
//! license = "MIT"
//! homepage = "https://…"
//! main = "main.lua"        # the default
//! permissions = ["files"]  # optional: run commands that read or write files
//! ```
//!
//! `main.lua` registers commands with `ccad.command { name, title,
//! description, kind, params, run }` while it loads; `require("a.b")` loads
//! `a/b.lua` from the plugin's folder. A command `walls` of the plugin
//! `room_tools` is `room_tools__walls` in the catalog. Each plugin has its
//! own sandboxed Lua state.

use crate::api::{API_VERSION, Access, Api, engine_error};
use crate::convert::{from_lua, to_lua};
use crate::sandbox::{Limits, Sandbox};
use cc_engine::extension::SEPARATOR;
use cc_engine::{CommandInfo, CommandKind, EngineError, Extension, Session};
use mlua::{Function, Lua, Table, Value as LuaValue};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// The manifest's file name.
pub const MANIFEST: &str = "plugin.toml";
/// Largest manifest, and largest Lua file of a plugin, in bytes.
const MAX_MANIFEST: u64 = 64 * 1024;
const MAX_SOURCE: u64 = 4 * 1024 * 1024;
/// Most commands one plugin registers, and most plugins in a folder.
const MAX_COMMANDS: usize = 256;
const MAX_PLUGINS: usize = 256;
/// Longest full command name (a model tool name).
const MAX_NAME: usize = 64;

/// A plugin's `plugin.toml`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default = "api_one")]
    pub api: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default = "main_lua")]
    pub main: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

fn api_one() -> u32 {
    1
}

fn main_lua() -> String {
    "main.lua".into()
}

/// What a plugin may do beyond querying and editing the model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Permissions {
    /// Run commands that read or write files (import, export, save).
    pub files: bool,
}

impl Permissions {
    pub const NONE: Permissions = Permissions { files: false };

    fn and(self, other: Permissions) -> Permissions {
        Permissions { files: self.files && other.files }
    }
}

/// A plugin that could not be loaded, or a plugin command refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginError {
    /// The plugin's folder.
    pub dir: PathBuf,
    pub message: String,
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.dir.display(), self.message)
    }
}

impl std::error::Error for PluginError {}

/// Whether `s` is a valid id or command name: a lowercase letter, then
/// lowercase letters, digits and single underscores, not ending in one.
fn valid_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !s.contains(SEPARATOR)
        && !s.ends_with('_')
}

impl Manifest {
    /// Read and check a manifest.
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let m: Manifest = toml::from_str(text).map_err(|e| format!("{MANIFEST}: {e}"))?;
        m.check()?;
        Ok(m)
    }

    fn check(&self) -> Result<(), String> {
        if !valid_name(&self.id) || self.id.len() > 32 {
            return Err(format!(
                "id \"{}\": up to 32 lowercase letters, digits and single underscores, starting with a letter",
                self.id
            ));
        }
        if self.name.trim().is_empty() || self.name.len() > 100 {
            return Err("name: 1 to 100 characters".into());
        }
        if self.version.trim().is_empty() || self.version.len() > 32 {
            return Err("version: 1 to 32 characters".into());
        }
        if self.api == 0 || self.api > API_VERSION {
            return Err(format!("api {}: this version of ConstructCAD provides plugin API {API_VERSION}", self.api));
        }
        if module_path(&self.main).is_none() {
            return Err(format!("main \"{}\": a .lua file inside the plugin's folder", self.main));
        }
        for p in &self.permissions {
            if p != "files" {
                return Err(format!("unknown permission \"{p}\" (known: files)"));
            }
        }
        Ok(())
    }

    /// The permissions it asks for.
    pub fn requested(&self) -> Permissions {
        Permissions { files: self.permissions.iter().any(|p| p == "files") }
    }
}

/// A relative path made of plain names (letters, digits, `_`, `-`), ending
/// in `.lua`: no `..`, no absolute paths, no drive letters.
fn module_path(rel: &str) -> Option<PathBuf> {
    let stem = rel.strip_suffix(".lua")?;
    let mut path = PathBuf::new();
    for part in stem.split('/') {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return None;
        }
        path.push(part);
    }
    path.set_extension("lua");
    Some(path)
}

/// Read a file of the plugin, refusing large ones and ones outside its folder
/// (through links).
fn read_inside(dir: &Path, rel: &Path, max: u64) -> Result<String, String> {
    let path = dir.join(rel);
    let shown = rel.display();
    let root = dir.canonicalize().map_err(|e| format!("{}: {e}", dir.display()))?;
    let real = path.canonicalize().map_err(|e| format!("{shown}: {e}"))?;
    if !real.starts_with(&root) {
        return Err(format!("{shown} is outside the plugin's folder"));
    }
    let len = std::fs::metadata(&real).map_err(|e| format!("{shown}: {e}"))?.len();
    if len > max {
        return Err(format!("{shown} is larger than {max} bytes"));
    }
    std::fs::read_to_string(&real).map_err(|e| format!("{shown}: {e}"))
}

/// A command a plugin registered.
struct PluginCommand {
    short: String,
    info: CommandInfo,
    required: Vec<String>,
    func: Function,
}

/// A loaded plugin.
pub struct Plugin {
    pub manifest: Manifest,
    pub dir: PathBuf,
    granted: Permissions,
    sandbox: Sandbox,
    api: Api,
    commands: Vec<PluginCommand>,
    /// What it printed while loading, until a host takes it.
    printed: Vec<String>,
}

impl std::fmt::Debug for Plugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plugin").field("id", &self.manifest.id).field("dir", &self.dir).finish_non_exhaustive()
    }
}

impl Plugin {
    /// Load the plugin in `dir`: read its manifest, run its main file and
    /// keep the commands it registers. It is granted no permission yet.
    pub fn load(dir: &Path, limits: Limits) -> Result<Plugin, PluginError> {
        let fail = |message: String| PluginError { dir: dir.to_path_buf(), message };
        let manifest =
            Manifest::parse(&read_inside(dir, Path::new(MANIFEST), MAX_MANIFEST).map_err(fail)?).map_err(fail)?;
        let main = module_path(&manifest.main).ok_or_else(|| fail("bad main".into()))?;
        let source = read_inside(dir, &main, MAX_SOURCE).map_err(fail)?;
        let sandbox = Sandbox::new(limits).map_err(|e| fail(engine_error(&e).message))?;
        let (api, commands) = setup(&sandbox, dir, &manifest, &source).map_err(|e| fail(engine_error(&e).message))?;
        let printed = sandbox.take_output();
        Ok(Plugin { manifest, dir: dir.to_path_buf(), granted: Permissions::NONE, sandbox, api, commands, printed })
    }

    /// Grant permissions (only those the manifest asks for are kept).
    pub fn grant(&mut self, permissions: Permissions) {
        self.granted = permissions.and(self.manifest.requested());
    }

    pub fn granted(&self) -> Permissions {
        self.granted
    }

    /// Its commands' catalog entries (full names).
    pub fn commands(&self) -> impl Iterator<Item = &CommandInfo> {
        self.commands.iter().map(|c| &c.info)
    }

    /// Run its command `short` (the name without the plugin's id).
    fn call(&mut self, session: &mut Session, short: &str, args: Value) -> Option<Result<Value, EngineError>> {
        let command = self.commands.iter().find(|c| c.short == short)?;
        let name = command.info.name.clone();
        let kind = command.info.kind;
        if kind == CommandKind::File && !self.granted.files {
            let message = format!(
                "{name} works with files: the plugin {} was not granted the \"files\" permission",
                self.manifest.id
            );
            return Some(Err(EngineError::rejected(message)));
        }
        let args = match args {
            Value::Null => json!({}),
            Value::Object(_) => args,
            _ => return Some(Err(EngineError::invalid(format!("{name}: the arguments must be an object")))),
        };
        if let Some(missing) = command.required.iter().find(|k| args.get(k.as_str()).is_none()) {
            return Some(Err(EngineError::invalid(format!("{name}: missing field `{missing}`"))));
        }
        let access =
            Access { edits: kind != CommandKind::Query, files: self.granted.files, history: false, language: false };
        let func = command.func.clone();
        self.sandbox.reset_budget();
        let result = self.api.with_session(&self.sandbox.lua, session, access, |lua| {
            let out: LuaValue = func.call(to_lua(lua, &args)?)?;
            from_lua(lua, &out).map_err(|e| mlua::Error::runtime(format!("{name}: the result: {e}")))
        });
        Some(result.map_err(|e| engine_error(&e)))
    }
}

/// What `ccad.command` collected while the plugin loaded.
type Registered = Rc<RefCell<Vec<PluginCommand>>>;

/// Install the API, `require` and `ccad.command`, and run the main file.
fn setup(sandbox: &Sandbox, dir: &Path, manifest: &Manifest, source: &str) -> mlua::Result<(Api, Vec<PluginCommand>)> {
    let lua = &sandbox.lua;
    let api = Api::install(lua)?;
    let plugin = lua.create_table()?;
    plugin.raw_set("id", manifest.id.as_str())?;
    plugin.raw_set("name", manifest.name.as_str())?;
    plugin.raw_set("version", manifest.version.as_str())?;
    api.ccad.raw_set("plugin", plugin)?;
    lua.globals().raw_set("require", require_fn(lua, dir, &manifest.id)?)?;

    let registered: Registered = Rc::new(RefCell::new(Vec::new()));
    let sink = registered.clone();
    let id = manifest.id.clone();
    let requested = manifest.requested();
    let command = lua.create_function(move |lua, spec: Table| {
        let c = register(lua, &id, requested, &spec)?;
        let mut list = sink.borrow_mut();
        if list.len() >= MAX_COMMANDS {
            return Err(mlua::Error::runtime(format!("a plugin registers at most {MAX_COMMANDS} commands")));
        }
        if list.iter().any(|o| o.short == c.short) {
            return Err(mlua::Error::runtime(format!("command \"{}\" is registered twice", c.short)));
        }
        list.push(c);
        Ok(())
    })?;
    api.ccad.raw_set("command", command)?;

    sandbox.reset_budget();
    let chunk_name = format!("@{}/{}", manifest.id, manifest.main);
    lua.load(source).set_name(chunk_name).set_mode(mlua::ChunkMode::Text).exec()?;

    // Commands are registered while the plugin loads only.
    let late = lua.create_function(|_, _: mlua::MultiValue| -> mlua::Result<()> {
        Err(mlua::Error::runtime("ccad.command can only be called while the plugin loads"))
    })?;
    api.ccad.raw_set("command", late)?;
    let commands = std::mem::take(&mut *registered.borrow_mut());
    Ok((api, commands))
}

/// Read one `ccad.command { … }` table.
fn register(lua: &Lua, id: &str, requested: Permissions, spec: &Table) -> mlua::Result<PluginCommand> {
    let err = |m: String| mlua::Error::runtime(format!("ccad.command: {m}"));
    let short: String = spec.get::<Option<String>>("name")?.ok_or_else(|| err("needs a name".into()))?;
    let full = format!("{id}{SEPARATOR}{short}");
    if !valid_name(&short) || full.len() > MAX_NAME {
        return Err(err(format!(
            "name \"{short}\": lowercase letters, digits and single underscores, starting with a letter \
             ({MAX_NAME} characters at most with the plugin's id)"
        )));
    }
    let title: Option<String> = spec.get("title")?;
    let description: String = spec.get::<Option<String>>("description")?.unwrap_or_default();
    let kind = match spec.get::<Option<String>>("kind")?.as_deref() {
        None | Some("edit") => CommandKind::Edit,
        Some("query") => CommandKind::Query,
        Some("file") if requested.files => CommandKind::File,
        Some("file") => {
            return Err(err(format!("{short}: kind \"file\" needs the \"files\" permission in {MANIFEST}")));
        }
        Some(other) => return Err(err(format!("{short}: kind \"{other}\" (query, edit or file)"))),
    };
    let func: Function =
        spec.get::<Option<Function>>("run")?.ok_or_else(|| err(format!("{short}: needs a run function")))?;
    let mut schema = match spec.get::<LuaValue>("params")? {
        LuaValue::Nil => json!({ "type": "object", "properties": {} }),
        v => from_lua(lua, &v).map_err(|e| err(format!("{short}: params: {e}")))?,
    };
    let Some(obj) = schema.as_object_mut() else {
        return Err(err(format!("{short}: params must be a JSON Schema object")));
    };
    obj.insert("type".into(), json!("object"));
    obj.entry("properties").or_insert_with(|| json!({}));
    let required: Vec<String> = obj
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    let description = match title {
        Some(t) if !t.trim().is_empty() && description.is_empty() => t,
        _ => description,
    };
    let info = CommandInfo { name: full, kind, group: format!("Plugin: {id}"), description, input_schema: schema };
    Ok(PluginCommand { short, info, required, func })
}

/// `require(name)`: the module `a.b` is `a/b.lua` in the plugin's folder,
/// run once (its result is kept).
fn require_fn(lua: &Lua, dir: &Path, id: &str) -> mlua::Result<Function> {
    let dir = dir.to_path_buf();
    let id = id.to_string();
    // `None` while the module loads (to catch a module requiring itself).
    let cache: Rc<RefCell<HashMap<String, Option<LuaValue>>>> = Rc::default();
    lua.create_function(move |lua, name: String| {
        if name.len() > 128 {
            return Err(mlua::Error::runtime("require: module name too long"));
        }
        if let Some(entry) = cache.borrow().get(&name) {
            return match entry {
                Some(v) => Ok(v.clone()),
                None => Err(mlua::Error::runtime(format!("require: \"{name}\" requires itself"))),
            };
        }
        let rel = format!("{}.lua", name.replace('.', "/"));
        let path =
            module_path(&rel).ok_or_else(|| mlua::Error::runtime(format!("require: bad module name \"{name}\"")))?;
        let source = read_inside(&dir, &path, MAX_SOURCE).map_err(|e| mlua::Error::runtime(format!("require: {e}")))?;
        cache.borrow_mut().insert(name.clone(), None);
        let loaded = lua
            .load(source)
            .set_name(format!("@{id}/{rel}"))
            .set_mode(mlua::ChunkMode::Text)
            .call::<LuaValue>(name.clone());
        let value = match loaded {
            Ok(LuaValue::Nil) => LuaValue::Boolean(true),
            Ok(v) => v,
            Err(e) => {
                cache.borrow_mut().remove(&name);
                return Err(e);
            }
        };
        cache.borrow_mut().insert(name, Some(value.clone()));
        Ok(value)
    })
}

/// The loaded plugins, as the session's [`Extension`].
#[derive(Debug, Default)]
pub struct PluginHost {
    plugins: Vec<Plugin>,
    messages: Vec<String>,
}

impl PluginHost {
    pub fn new() -> PluginHost {
        PluginHost::default()
    }

    /// Add a loaded plugin; refused if one with its id is already there.
    pub fn add(&mut self, plugin: Plugin) -> Result<(), PluginError> {
        if self.plugins.iter().any(|p| p.manifest.id == plugin.manifest.id) {
            return Err(PluginError {
                dir: plugin.dir.clone(),
                message: format!("another plugin with the id \"{}\" is already loaded", plugin.manifest.id),
            });
        }
        if self.plugins.len() >= MAX_PLUGINS {
            return Err(PluginError { dir: plugin.dir.clone(), message: format!("at most {MAX_PLUGINS} plugins") });
        }
        let id = plugin.manifest.id.clone();
        let mut plugin = plugin;
        for line in std::mem::take(&mut plugin.printed) {
            self.messages.push(format!("[{id}] {line}"));
        }
        self.plugins.push(plugin);
        Ok(())
    }

    /// Load the plugins of `folder`: the folder itself when it holds a
    /// manifest, else each of its subfolders that does (in name order).
    /// `grant` decides the permissions of each. Returns the plugins that
    /// could not be loaded.
    pub fn load_folder(
        &mut self,
        folder: &Path,
        limits: Limits,
        grant: impl Fn(&Manifest) -> Permissions,
    ) -> Vec<PluginError> {
        let mut errors = Vec::new();
        let dirs: Vec<PathBuf> = if folder.join(MANIFEST).is_file() {
            vec![folder.to_path_buf()]
        } else {
            match std::fs::read_dir(folder) {
                Ok(entries) => {
                    let mut dirs: Vec<PathBuf> = entries
                        .filter_map(|e| e.ok().map(|e| e.path()))
                        .filter(|p| p.join(MANIFEST).is_file())
                        .take(MAX_PLUGINS)
                        .collect();
                    dirs.sort();
                    dirs
                }
                Err(e) => {
                    errors.push(PluginError { dir: folder.to_path_buf(), message: e.to_string() });
                    Vec::new()
                }
            }
        };
        for dir in dirs {
            match Plugin::load(&dir, limits) {
                Ok(mut plugin) => {
                    plugin.grant(grant(&plugin.manifest));
                    if let Err(e) = self.add(plugin) {
                        errors.push(e);
                    }
                }
                Err(e) => errors.push(e),
            }
        }
        errors
    }

    pub fn plugins(&self) -> &[Plugin] {
        &self.plugins
    }

    /// A summary of the plugins for listings (`ccad plugins`).
    pub fn summary(&self) -> Value {
        Value::Array(
            self.plugins
                .iter()
                .map(|p| {
                    json!({
                        "id": p.manifest.id,
                        "name": p.manifest.name,
                        "version": p.manifest.version,
                        "description": p.manifest.description,
                        "dir": p.dir,
                        "requested": p.manifest.requested(),
                        "granted": p.granted,
                        "commands": p.commands().map(|c| json!({"name": c.name, "kind": c.kind, "description": c.description})).collect::<Vec<_>>(),
                    })
                })
                .collect(),
        )
    }
}

impl Extension for PluginHost {
    fn commands(&self) -> Vec<CommandInfo> {
        self.plugins.iter().flat_map(|p| p.commands().cloned()).collect()
    }

    fn run(&mut self, session: &mut Session, name: &str, args: Value) -> Option<cc_engine::Result<Value>> {
        let (id, short) = name.split_once(SEPARATOR)?;
        let plugin = self.plugins.iter_mut().find(|p| p.manifest.id == id)?;
        let result = plugin.call(session, short, args);
        for line in plugin.sandbox.take_output() {
            self.messages.push(format!("[{id}] {line}"));
        }
        result
    }

    fn take_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.messages)
    }
}
