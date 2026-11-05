//! The `ccad` table scripts and plugins see.
//!
//! - `ccad.run(name, args)`: run an engine command; returns its result, or
//!   raises an error.
//! - `ccad.try(name, args)`: the same, returning `result` or `nil, err`
//!   (`err.code`, `err.message`) instead of raising.
//! - `ccad.commands()`: the commands (name, kind, group, description);
//!   `ccad.describe(name)` one of them with its parameters' `schema`.
//! - `ccad.null` (JSON null), `ccad.array(t)` (marks `t`, even empty, as a
//!   list), `ccad.json.encode(v)` / `ccad.json.decode(s)`, `ccad.version`,
//!   `ccad.api` (the plugin API version).
//!
//! The functions that reach the session exist only while a script or a
//! plugin command runs: the Lua wrappers stay (a plugin may keep them in
//! locals when it loads) and call the session's functions through a private
//! table filled for the run.

use crate::convert::{from_lua, to_lua};
use cc_engine::{CommandKind, EngineError, ErrorCode, Session};
use mlua::{Lua, LuaSerdeExt, Table, Value as LuaValue};
use serde_json::Value;
use std::cell::RefCell;

/// The plugin API version this build provides (`api` in `plugin.toml`).
pub const API_VERSION: u32 = 1;

/// Makes the wrappers over the private table of the run's functions.
const PRELUDE: &str = r#"
local ccad, impl = ...
local error = error
local function get(name)
  local f = impl[name]
  if not f then error("ccad." .. name .. " is only available while a script or plugin command runs", 3) end
  return f
end
function ccad.run(name, args) return get("run")(name, args) end
function ccad.try(name, args) return get("try")(name, args) end
function ccad.commands() return get("commands")() end
function ccad.describe(name) return get("describe")(name) end
"#;

/// What the commands a run calls may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Access {
    /// Commands that change the model.
    pub edits: bool,
    /// Commands that read or write files.
    pub files: bool,
    /// Commands that replace the document or move in its history (open,
    /// new, undo, redo), which a plugin command, one undo step, cannot.
    pub history: bool,
    /// The language (a session-wide setting).
    pub language: bool,
}

impl Access {
    /// Everything: a script the user runs.
    pub const ALL: Access = Access { edits: true, files: true, history: true, language: true };

    /// Why `name` (of kind `kind`) may not run, if it may not.
    fn refuses(&self, name: &str, kind: Option<CommandKind>) -> Option<String> {
        if !self.history && matches!(name, "project_new" | "project_open" | "undo" | "redo") {
            return Some(format!("{name} cannot run inside a plugin command"));
        }
        if !self.language && name == "language_set" {
            return Some(format!("{name} cannot run inside a plugin command"));
        }
        match kind {
            Some(CommandKind::Edit) if !self.edits => {
                Some(format!("{name} changes the model, which a query command may not do"))
            }
            Some(CommandKind::File) if !self.files => {
                Some(format!("{name} works with files, which needs the plugin's \"files\" permission"))
            }
            _ => None,
        }
    }
}

/// The `ccad` table and the private table of the run's functions.
pub struct Api {
    pub ccad: Table,
    imp: Table,
}

impl Api {
    /// Make the `ccad` global in `lua`.
    pub fn install(lua: &Lua) -> mlua::Result<Api> {
        let ccad = lua.create_table()?;
        let imp = lua.create_table()?;
        ccad.raw_set("null", LuaValue::NULL)?;
        ccad.raw_set("version", env!("CARGO_PKG_VERSION"))?;
        ccad.raw_set("api", API_VERSION)?;
        ccad.raw_set(
            "array",
            lua.create_function(|lua, t: Option<Table>| {
                let t = match t {
                    Some(t) => t,
                    None => lua.create_table()?,
                };
                t.set_metatable(Some(lua.array_metatable()))?;
                Ok(t)
            })?,
        )?;
        let json = lua.create_table()?;
        json.raw_set(
            "encode",
            lua.create_function(|lua, v: LuaValue| {
                let value = from_lua(lua, &v).map_err(mlua::Error::runtime)?;
                Ok(serde_json::to_string(&value).unwrap_or_default())
            })?,
        )?;
        json.raw_set(
            "decode",
            lua.create_function(|lua, s: mlua::String| {
                let value: Value = serde_json::from_slice(&s.as_bytes()).map_err(mlua::Error::runtime)?;
                to_lua(lua, &value)
            })?,
        )?;
        ccad.raw_set("json", json)?;
        lua.load(PRELUDE).set_name("=ccad").call::<()>((ccad.clone(), imp.clone()))?;
        lua.globals().raw_set("ccad", ccad.clone())?;
        Ok(Api { ccad, imp })
    }

    /// Run `f` with the session's functions available to the Lua code.
    pub fn with_session<R>(
        &self,
        lua: &Lua,
        session: &mut Session,
        access: Access,
        f: impl FnOnce(&Lua) -> mlua::Result<R>,
    ) -> mlua::Result<R> {
        let cell = RefCell::new(session);
        let cell = &cell;
        let result = lua.scope(|scope| {
            let run = scope.create_function(move |lua, (name, args): (String, LuaValue)| {
                let mut session = cell.try_borrow_mut().map_err(mlua::Error::runtime)?;
                match execute(lua, &mut session, access, &name, &args)? {
                    Ok(v) => to_lua(lua, &v),
                    Err(e) => Err(mlua::Error::external(e)),
                }
            })?;
            let try_run = scope.create_function(move |lua, (name, args): (String, LuaValue)| {
                let mut session = cell.try_borrow_mut().map_err(mlua::Error::runtime)?;
                match execute(lua, &mut session, access, &name, &args)? {
                    Ok(v) => Ok((to_lua(lua, &v)?, LuaValue::Nil)),
                    Err(e) => {
                        let err = lua.create_table()?;
                        err.raw_set("code", code_name(e.code))?;
                        err.raw_set("message", e.message)?;
                        Ok((LuaValue::Nil, LuaValue::Table(err)))
                    }
                }
            })?;
            let commands = scope.create_function(move |lua, ()| {
                let session = cell.try_borrow().map_err(mlua::Error::runtime)?;
                let list: Vec<Value> = session
                    .commands()
                    .into_iter()
                    .map(|c| {
                        serde_json::json!({ "name": c.name, "kind": c.kind, "group": c.group, "description": c.description })
                    })
                    .collect();
                to_lua(lua, &Value::Array(list))
            })?;
            let describe = scope.create_function(move |lua, name: String| {
                let session = cell.try_borrow().map_err(mlua::Error::runtime)?;
                match session.describe(&name) {
                    Some(c) => to_lua(
                        lua,
                        &serde_json::json!({
                            "name": c.name, "kind": c.kind, "group": c.group,
                            "description": c.description, "schema": c.input_schema,
                        }),
                    ),
                    None => Ok(LuaValue::Nil),
                }
            })?;
            self.imp.raw_set("run", run)?;
            self.imp.raw_set("try", try_run)?;
            self.imp.raw_set("commands", commands)?;
            self.imp.raw_set("describe", describe)?;
            let result = f(lua);
            self.imp.clear()?;
            result
        });
        // Leave nothing that points at the finished scope, even after an error.
        let _ = self.imp.clear();
        result
    }
}

/// Run one command for the Lua code: `Err` for errors of the Lua side (bad
/// arguments), `Ok(Err)` for the command's own failure.
fn execute(
    lua: &Lua,
    session: &mut Session,
    access: Access,
    name: &str,
    args: &LuaValue,
) -> mlua::Result<Result<Value, EngineError>> {
    let args = match args {
        LuaValue::Nil => Value::Object(Default::default()),
        other => from_lua(lua, other).map_err(|e| mlua::Error::runtime(format!("{name}: arguments: {e}")))?,
    };
    if let Some(reason) = access.refuses(name, session.kind_of(name)) {
        return Ok(Err(EngineError::rejected(reason)));
    }
    Ok(session.execute(name, args))
}

/// An error code as its JSON name.
pub fn code_name(code: ErrorCode) -> String {
    serde_json::to_value(code).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

/// Longest error message kept from a Lua error (tracebacks can be long).
const MAX_MESSAGE: usize = 4000;

/// A Lua error as an engine error: a command's error raised through
/// `ccad.run` keeps its code; anything else is a script error.
pub fn engine_error(e: &mlua::Error) -> EngineError {
    fn find(e: &mlua::Error) -> Option<EngineError> {
        match e {
            mlua::Error::CallbackError { cause, .. } | mlua::Error::WithContext { cause, .. } => find(cause),
            mlua::Error::ExternalError(x) => x.downcast_ref::<EngineError>().cloned(),
            _ => None,
        }
    }
    if let Some(found) = find(e) {
        return found;
    }
    let mut message = match e {
        mlua::Error::RuntimeError(m) | mlua::Error::SyntaxError { message: m, .. } => m.clone(),
        mlua::Error::MemoryError(_) => "the script used too much memory".to_string(),
        other => other.to_string(),
    };
    // The message, without the stack traceback Lua appends.
    if let Some(cut) = message.find("\nstack traceback:") {
        message.truncate(cut);
    }
    if message.len() > MAX_MESSAGE {
        let mut cut = MAX_MESSAGE;
        while !message.is_char_boundary(cut) {
            cut -= 1;
        }
        message.truncate(cut);
        message.push('…');
    }
    EngineError::new(ErrorCode::Script, message)
}
