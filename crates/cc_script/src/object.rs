//! Scripted component pieces: running the Lua of a
//! [`cc_core::ComponentShape::Script`] piece.
//!
//! A script declares its parameters and hotspots at its top level and
//! defines `geometry(p)`, which returns the pieces:
//!
//! ```lua
//! param.length  { "depth", label = "Depth", default = 0.6 }
//! param.choice  { "finish", { {"plain", "Plain"}, {"blue", "Blue"} }, default = "plain" }
//! param.boolean { "doors", default = true, instance = true }
//! hotspot       { "size", 0.9, 0.6, 0, label = "Size", shape = "diamond", color = "#ffccddee" }
//!
//! function geometry(p)
//!   if not p.doors then return {} end
//!   return { box { 0, 0, 0, p.size.x, p.depth, 0.9, material = "Wood" } }
//! end
//! ```
//!
//! Scripts are pure: they see the parameter values and nothing else (no
//! engine commands, files or model), so the same values always give the
//! same pieces and results can be cached. [`declare`] reads the
//! declarations, [`evaluate`] runs `geometry`, and [`ScriptCache`] is the
//! [`ScriptEval`] geometry uses: results by source and values, and the last
//! pieces that worked for each owner.

use crate::convert::{from_lua, to_lua};
use crate::sandbox::{BUDGET_SPENT, Limits, Sandbox};
use cc_core::component::{ComponentItem, ComponentShape, DetailLevels, ProfilePoint, SketchPlane};
use cc_core::component_script::{
    Hotspot, HotspotShape, MAX_CHOICES, MAX_HOTSPOTS, MAX_TEXT_LEN, ParamChoice, ScriptEval, ScriptInput, ScriptOwner,
    ScriptResult, parse_color,
};
use cc_core::{ComponentParam, ParamKind};
use mlua::{Function, Value as LuaValue};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

/// What one evaluation may use: less than a plugin command, since scripts
/// come with project files.
pub const LIMITS: Limits = Limits { instructions: 20_000_000, memory: 64 * 1024 * 1024 };
/// Most pieces a script returns.
pub const MAX_PIECES: usize = 10_000;
/// Most points (outline points and mesh vertices) of all its pieces.
pub const MAX_POINTS: usize = 1_000_000;
/// Most parameters a script declares.
pub const MAX_PARAMS: usize = 256;
/// Most nested lists of pieces flattened.
const MAX_NESTING: usize = 8;

/// Results kept by [`ScriptCache`].
const CACHE_SIZE: usize = 4096;
/// Owners whose last pieces are kept; past that the oldest are forgotten.
const OWNERS: usize = 65_536;

/// Why a script failed, with the line when Lua gave one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError {
    pub message: String,
    pub line: Option<u32>,
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl ScriptError {
    fn new(message: impl Into<String>) -> ScriptError {
        ScriptError { message: message.into(), line: None }
    }

    /// From a Lua error: `script:12: message` gives line 12.
    fn lua(e: &mlua::Error) -> ScriptError {
        let text = match e {
            mlua::Error::CallbackError { cause, .. } => cause.to_string(),
            e => e.to_string(),
        };
        if text.contains(BUDGET_SPENT) {
            return ScriptError::new(BUDGET_SPENT);
        }
        if matches!(e, mlua::Error::MemoryError(_)) || text.contains("not enough memory") {
            return ScriptError::new("the script used too much memory");
        }
        // The first line holds the message; the rest is the traceback.
        let first = text.lines().next().unwrap_or_default().trim();
        let first =
            first.strip_prefix("runtime error: ").or_else(|| first.strip_prefix("syntax error: ")).unwrap_or(first);
        if let Some(rest) = first.find(CHUNK).map(|i| &first[i + CHUNK.len()..])
            && let Some((line, message)) = rest.split_once(':')
            && let Ok(line) = line.trim().parse()
        {
            return ScriptError { message: message.trim().to_string(), line: Some(line) };
        }
        ScriptError::new(first)
    }
}

/// The chunk name errors carry (`script:LINE:`).
const CHUNK: &str = "script:";

/// What a script declares.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Declarations {
    /// Marked `from_script`.
    pub params: Vec<ComponentParam>,
    pub hotspots: Vec<Hotspot>,
}

/// The functions scripts see: declarations and piece builders. Returns the
/// function that hands the declarations over and the one that ends them.
const PRELUDE: &str = r#"
local decl = { params = {}, hotspots = {} }
local seen = {}
local declaring = true
local function check_id(id, what)
  if not declaring then error("parameters and hotspots are declared outside geometry()", 3) end
  if type(id) ~= "string" or not id:match("^[%a_][%w_]*$") then
    error(what .. " ids are names: letters, digits and _, not starting with a digit", 3)
  end
  if seen[id] then error("'" .. id .. "' is declared twice", 3) end
  seen[id] = true
end
local function declare(kind)
  return function(t)
    if type(t) ~= "table" then error("param." .. kind .. ' takes a table: { "id", default = ... }', 2) end
    local id = t[1] or t.id
    check_id(id, "parameter")
    local choices = nil
    if kind == "choice" then
      choices = {}
      for i, c in ipairs(t[2] or t.choices or {}) do
        if type(c) ~= "table" then error("choices are { key, label } pairs", 2) end
        choices[i] = { key = c[1] or c.key, label = c[2] or c.label }
      end
    end
    decl.params[#decl.params + 1] = {
      id = id, kind = kind, label = t.label, default = t.default, instance = t.instance == true, choices = choices,
    }
  end
end
param = {}
for _, k in ipairs { "length", "angle", "number", "text", "boolean", "choice", "material" } do
  param[k] = declare(k)
end
function hotspot(t)
  if type(t) ~= "table" then error('hotspot takes a table: { "id", x, y, z }', 2) end
  local id = t[1] or t.id
  check_id(id, "hotspot")
  decl.hotspots[#decl.hotspots + 1] = {
    id = id, x = t[2] or t.x or 0, y = t[3] or t.y or 0, z = t[4] or t.z or 0,
    label = t.label, shape = t.shape, color = t.color,
  }
end

local point_lists = { points = true, path = true, profile = true, bottom = true, top = true }
local function point(p)
  if type(p) ~= "table" then return p end
  local out = {}
  for k, v in pairs(p) do if type(k) == "string" then out[k] = v end end
  out.u = p[1] or p.u or p.x
  out.v = p[2] or p.v or p.y
  out.x, out.y = nil, nil
  return out
end
local function builder(kind, names)
  return function(t)
    if type(t) ~= "table" then error(kind .. " takes a table", 2) end
    local out = { kind = kind }
    for k, v in pairs(t) do if type(k) == "string" then out[k] = v end end
    for i, n in ipairs(names) do if t[i] ~= nil then out[n] = t[i] end end
    if kind ~= "extrusion" then
      for k in pairs(point_lists) do
        if type(out[k]) == "table" then
          local list = {}
          for i, p in ipairs(out[k]) do list[i] = point(p) end
          out[k] = list
        end
      end
    end
    return out
  end
end
box = builder("box", { "x", "y", "z", "width", "depth", "height" })
cylinder = builder("cylinder", { "x", "y", "z", "radius", "height" })
extrusion = builder("extrusion", { "points", "z", "height" })
profile = builder("profile", { "points", "offset", "depth" })
revolution = builder("revolution", { "points", "offset", "axis", "angle" })
sweep = builder("sweep", { "path", "profile", "offset" })
blend = builder("blend", { "bottom", "top", "offset", "depth" })
mesh = builder("mesh", { "vertices", "faces" })
math.randomseed(0)
return function() return decl end, function() declaring = false end
"#;

/// A sandbox with the script loaded and its top level run.
struct Loaded {
    sandbox: Sandbox,
    declarations: Declarations,
    end_declarations: Function,
}

fn load(source: &str) -> Result<Loaded, ScriptError> {
    let sandbox = Sandbox::new(LIMITS).map_err(|e| ScriptError::lua(&e))?;
    let lua = &sandbox.lua;
    let (get, end): (Function, Function) =
        lua.load(PRELUDE).set_name("=prelude").call(()).map_err(|e| ScriptError::lua(&e))?;
    sandbox.reset_budget();
    lua.load(source).set_name(format!("={}", CHUNK.trim_end_matches(':'))).exec().map_err(|e| ScriptError::lua(&e))?;
    let decl: LuaValue = get.call(()).map_err(|e| ScriptError::lua(&e))?;
    let decl = from_lua(lua, &decl).map_err(ScriptError::new)?;
    let declarations = declarations(&decl)?;
    Ok(Loaded { sandbox, declarations, end_declarations: end })
}

/// Read what `source` declares (its top level runs; `geometry` does not).
pub fn declare(source: &str) -> Result<Declarations, ScriptError> {
    load(source).map(|l| l.declarations)
}

/// Run `geometry(p)` with the values of `input` and return its pieces.
pub fn evaluate(source: &str, input: &ScriptInput) -> Result<Vec<ComponentItem>, ScriptError> {
    let loaded = load(source)?;
    let lua = &loaded.sandbox.lua;
    let geometry: Function = match lua.globals().get::<LuaValue>("geometry") {
        Ok(LuaValue::Function(f)) => f,
        _ => return Err(ScriptError::new("the script defines no geometry(p) function")),
    };
    loaded.end_declarations.call::<()>(()).map_err(|e| ScriptError::lua(&e))?;
    let p = to_lua(lua, &params_value(&loaded.declarations, input)).map_err(|e| ScriptError::lua(&e))?;
    let out: LuaValue = geometry.call(p).map_err(|e| ScriptError::lua(&e))?;
    let out = from_lua(lua, &out).map_err(|e| ScriptError::new(format!("geometry() returned {e}")))?;
    pieces(&out)
}

/// The `p` table: every number, booleans as booleans, texts, hotspots as
/// `{ x, y, z }`.
fn params_value(decl: &Declarations, input: &ScriptInput) -> Value {
    let booleans: BTreeSet<&str> =
        decl.params.iter().filter(|p| p.kind == ParamKind::Boolean).map(|p| p.name.as_str()).collect();
    let mut map = Map::new();
    for (name, v) in &input.numbers {
        let value = if booleans.contains(name.as_str()) { json!(*v != 0.0) } else { json!(v) };
        map.insert(name.clone(), value);
    }
    for (name, v) in &input.texts {
        map.insert(name.clone(), json!(v));
    }
    for (name, p) in &input.hotspots {
        map.insert(name.clone(), json!({ "x": p.x, "y": p.y, "z": p.z }));
    }
    Value::Object(map)
}

fn text(v: Option<&Value>, what: &str) -> Result<String, ScriptError> {
    match v {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(s)) if s.len() <= MAX_TEXT_LEN => Ok(s.clone()),
        Some(Value::String(_)) => Err(ScriptError::new(format!("{what} is longer than {MAX_TEXT_LEN} bytes"))),
        Some(_) => Err(ScriptError::new(format!("{what} must be text"))),
    }
}

fn number(v: Option<&Value>, what: &str) -> Result<f64, ScriptError> {
    match v {
        None | Some(Value::Null) => Ok(0.0),
        Some(Value::Number(n)) => n.as_f64().ok_or_else(|| ScriptError::new(format!("{what} must be a number"))),
        Some(_) => Err(ScriptError::new(format!("{what} must be a number"))),
    }
}

fn declarations(decl: &Value) -> Result<Declarations, ScriptError> {
    let list = |key: &str| decl.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    let params = list("params");
    if params.len() > MAX_PARAMS {
        return Err(ScriptError::new(format!("more than {MAX_PARAMS} parameters")));
    }
    let mut out = Declarations::default();
    for p in &params {
        let name = text(p.get("id"), "a parameter id")?;
        let kind_name = text(p.get("kind"), "a parameter kind")?;
        let kind = ParamKind::from_name(&kind_name).ok_or_else(|| ScriptError::new("unknown parameter kind"))?;
        let what = format!("the default of '{name}'");
        let mut param = ComponentParam {
            name: name.clone(),
            kind,
            instance: p.get("instance").and_then(Value::as_bool).unwrap_or(false),
            label: text(p.get("label"), &format!("the label of '{name}'"))?,
            from_script: true,
            ..Default::default()
        };
        let default = p.get("default");
        match kind {
            ParamKind::Length | ParamKind::Angle | ParamKind::Number => param.value = number(default, &what)?,
            ParamKind::Boolean => match default {
                None | Some(Value::Null) => {}
                Some(Value::Bool(b)) => param.value = f64::from(u8::from(*b)),
                Some(_) => return Err(ScriptError::new(format!("{what} must be true or false"))),
            },
            ParamKind::Text | ParamKind::Material => param.text = text(default, &what)?,
            ParamKind::Choice => {
                let choices = p.get("choices").and_then(Value::as_array).cloned().unwrap_or_default();
                if choices.is_empty() || choices.len() > MAX_CHOICES {
                    return Err(ScriptError::new(format!("'{name}' needs 1 to {MAX_CHOICES} choices")));
                }
                for c in &choices {
                    let key = text(c.get("key"), "a choice key")?;
                    if key.is_empty() || param.choices.iter().any(|o| o.key == key) {
                        return Err(ScriptError::new(format!("the choices of '{name}' need distinct, non-empty keys")));
                    }
                    let label = text(c.get("label"), "a choice label")?;
                    param.choices.push(ParamChoice { key, label });
                }
                param.text = text(default, &what)?;
                if param.text.is_empty() {
                    param.text = param.choices[0].key.clone();
                } else if !param.accepts_text(&param.text) {
                    return Err(ScriptError::new(format!("{what} is not one of its choices")));
                }
            }
        }
        if !param.value.is_finite() {
            return Err(ScriptError::new(format!("{what} is not a finite number")));
        }
        out.params.push(param);
    }
    let hotspots = list("hotspots");
    if hotspots.len() > MAX_HOTSPOTS {
        return Err(ScriptError::new(format!("more than {MAX_HOTSPOTS} hotspots")));
    }
    for h in &hotspots {
        let name = text(h.get("id"), "a hotspot id")?;
        let position = [
            number(h.get("x"), &format!("the x of '{name}'"))?,
            number(h.get("y"), &format!("the y of '{name}'"))?,
            number(h.get("z"), &format!("the z of '{name}'"))?,
        ];
        let shape = match text(h.get("shape"), "a hotspot shape")?.as_str() {
            "" => HotspotShape::Circle,
            s => HotspotShape::from_name(s).ok_or_else(|| {
                let names: Vec<&str> = HotspotShape::ALL.iter().map(|s| s.name()).collect();
                ScriptError::new(format!("unknown hotspot shape '{s}' (use {})", names.join(", ")))
            })?,
        };
        let color = match text(h.get("color"), "a hotspot color")?.as_str() {
            "" => [0, 255, 255, 255],
            c => {
                parse_color(c).ok_or_else(|| ScriptError::new(format!("'{c}' is not a #RRGGBB or #RRGGBBAA color")))?
            }
        };
        let label = text(h.get("label"), "a hotspot label")?;
        out.hotspots.push(Hotspot { name, label, position, shape, color });
    }
    Ok(out)
}

/// The pieces `geometry` returned: a list of pieces, or of lists of pieces.
fn pieces(out: &Value) -> Result<Vec<ComponentItem>, ScriptError> {
    let mut items = Vec::new();
    let mut points = 0usize;
    match out {
        Value::Null => {}
        Value::Object(m) if m.is_empty() => {}
        Value::Object(_) => collect(out, 0, &mut items, &mut points)?,
        Value::Array(_) => collect(out, 0, &mut items, &mut points)?,
        _ => return Err(ScriptError::new("geometry() must return a list of pieces")),
    }
    Ok(items)
}

fn collect(v: &Value, depth: usize, items: &mut Vec<ComponentItem>, points: &mut usize) -> Result<(), ScriptError> {
    if depth > MAX_NESTING {
        return Err(ScriptError::new("pieces nested too deep"));
    }
    match v {
        Value::Array(list) => list.iter().try_for_each(|v| collect(v, depth + 1, items, points)),
        Value::Object(m) if m.is_empty() => Ok(()),
        Value::Object(m) => {
            if items.len() >= MAX_PIECES {
                return Err(ScriptError::new(format!("more than {MAX_PIECES} pieces")));
            }
            let item = piece(m, items.len() + 1, points)?;
            items.push(item);
            Ok(())
        }
        _ => Err(ScriptError::new("geometry() must return pieces made with box, cylinder, extrusion, …")),
    }
}

/// A number as an expression (exact: Rust prints the shortest text that reads back).
fn expr(v: Option<&Value>, what: &str, piece: usize) -> Result<String, ScriptError> {
    match v {
        Some(Value::Number(n)) => match n.as_f64() {
            Some(f) if f.is_finite() => Ok(format!("{f}")),
            _ => Err(ScriptError::new(format!("piece {piece}: {what} is not a finite number"))),
        },
        None | Some(Value::Null) => Err(ScriptError::new(format!("piece {piece}: {what} is missing"))),
        Some(_) => Err(ScriptError::new(format!("piece {piece}: {what} must be a number"))),
    }
}

fn opt_expr(v: Option<&Value>, what: &str, piece: usize) -> Result<String, ScriptError> {
    match v {
        None | Some(Value::Null) => Ok("0".into()),
        v => expr(v, what, piece),
    }
}

fn plane(v: Option<&Value>, piece: usize) -> Result<SketchPlane, ScriptError> {
    match v.and_then(Value::as_str).unwrap_or("plan") {
        "plan" => Ok(SketchPlane::Plan),
        "front" => Ok(SketchPlane::Front),
        "side" => Ok(SketchPlane::Side),
        other => Err(ScriptError::new(format!("piece {piece}: unknown plane '{other}' (use plan, front or side)"))),
    }
}

fn profile_points(
    v: Option<&Value>,
    what: &str,
    piece: usize,
    points: &mut usize,
) -> Result<Vec<ProfilePoint>, ScriptError> {
    let list = v
        .and_then(Value::as_array)
        .ok_or_else(|| ScriptError::new(format!("piece {piece}: {what} must be a list of points")))?;
    *points += list.len();
    if *points > MAX_POINTS {
        return Err(ScriptError::new(format!("more than {MAX_POINTS} points")));
    }
    list.iter()
        .map(|p| {
            let radius = match p.get("radius") {
                None | Some(Value::Null) => String::new(),
                r => expr(r, "a point's radius", piece)?,
            };
            Ok(ProfilePoint {
                u: expr(p.get("u"), "a point's u", piece)?,
                v: expr(p.get("v"), "a point's v", piece)?,
                radius,
                ellipse: None,
            })
        })
        .collect()
}

fn piece(m: &Map<String, Value>, index: usize, points: &mut usize) -> Result<ComponentItem, ScriptError> {
    let get = |k: &str| m.get(k);
    let e = |k: &str| expr(get(k), k, index);
    let o = |k: &str| opt_expr(get(k), k, index);
    let kind = get("kind").and_then(Value::as_str).unwrap_or("");
    let shape = match kind {
        "box" => ComponentShape::Box {
            x: o("x")?,
            y: o("y")?,
            z: o("z")?,
            width: e("width")?,
            depth: e("depth")?,
            height: e("height")?,
        },
        "cylinder" => {
            ComponentShape::Cylinder { x: o("x")?, y: o("y")?, z: o("z")?, radius: e("radius")?, height: e("height")? }
        }
        "extrusion" => {
            let list = get("points")
                .and_then(Value::as_array)
                .ok_or_else(|| ScriptError::new(format!("piece {index}: points must be a list of {{x, y}}")))?;
            *points += list.len();
            if *points > MAX_POINTS {
                return Err(ScriptError::new(format!("more than {MAX_POINTS} points")));
            }
            let pts = list
                .iter()
                .map(|p| Ok((expr(p.get(0), "a point's x", index)?, expr(p.get(1), "a point's y", index)?)))
                .collect::<Result<Vec<_>, ScriptError>>()?;
            ComponentShape::Extrusion { points: pts, z: o("z")?, height: e("height")? }
        }
        "profile" => ComponentShape::Profile {
            plane: plane(get("plane"), index)?,
            points: profile_points(get("points"), "points", index, points)?,
            offset: o("offset")?,
            depth: e("depth")?,
        },
        "revolution" => ComponentShape::Revolution {
            plane: plane(get("plane"), index)?,
            points: profile_points(get("points"), "points", index, points)?,
            offset: o("offset")?,
            axis: o("axis")?,
            angle: match get("angle") {
                None | Some(Value::Null) => "360".into(),
                v => expr(v, "angle", index)?,
            },
        },
        "sweep" => ComponentShape::Sweep {
            plane: plane(get("plane"), index)?,
            path: profile_points(get("path"), "path", index, points)?,
            offset: o("offset")?,
            profile: profile_points(get("profile"), "profile", index, points)?,
        },
        "blend" => ComponentShape::Blend {
            plane: plane(get("plane"), index)?,
            bottom: profile_points(get("bottom"), "bottom", index, points)?,
            top: profile_points(get("top"), "top", index, points)?,
            offset: o("offset")?,
            depth: e("depth")?,
        },
        "mesh" => mesh(m, index, points)?,
        _ => return Err(ScriptError::new(format!("piece {index} was not made with box, cylinder, extrusion, …"))),
    };
    let flag = |k: &str, default: bool| get(k).and_then(Value::as_bool).unwrap_or(default);
    let mut detail = DetailLevels::default();
    if let Some(levels) = get("detail").and_then(Value::as_array) {
        detail = DetailLevels { coarse: false, medium: false, fine: false };
        for l in levels {
            match l.as_str() {
                Some("coarse") => detail.coarse = true,
                Some("medium") => detail.medium = true,
                Some("fine") => detail.fine = true,
                _ => return Err(ScriptError::new(format!("piece {index}: detail levels are coarse, medium and fine"))),
            }
        }
    }
    Ok(ComponentItem {
        name: text(get("name"), "a piece name")?,
        shape,
        material: text(get("material"), "a piece material")?,
        in_plan: flag("in_plan", true),
        void: flag("void", false),
        plan_only: flag("plan_only", false),
        detail,
    })
}

/// A mesh: `vertices` `{x, y, z}` and `faces` of 1-based vertex indices.
fn mesh(m: &Map<String, Value>, index: usize, points: &mut usize) -> Result<ComponentShape, ScriptError> {
    let err = |what: &str| ScriptError::new(format!("piece {index}: {what}"));
    let vertices =
        m.get("vertices").and_then(Value::as_array).ok_or_else(|| err("vertices must be a list of {x, y, z}"))?;
    let faces = m.get("faces").and_then(Value::as_array).ok_or_else(|| err("faces must be lists of vertex numbers"))?;
    *points += vertices.len();
    if *points > MAX_POINTS || faces.len() > MAX_POINTS {
        return Err(ScriptError::new(format!("more than {MAX_POINTS} points")));
    }
    let coord = |v: &Value, i: usize| v.get(i).and_then(Value::as_f64).filter(|c| c.is_finite());
    let vertices = vertices
        .iter()
        .map(|v| Some([coord(v, 0)?, coord(v, 1)?, coord(v, 2)?]))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| err("each vertex is {x, y, z} of finite numbers"))?;
    let n = vertices.len();
    let mut corners = 0usize;
    let faces = faces
        .iter()
        .map(|f| {
            let f = f.as_array()?;
            corners += f.len();
            f.iter()
                .map(|i| i.as_u64().filter(|i| *i >= 1 && (*i as usize) <= n).and_then(|i| u32::try_from(i - 1).ok()))
                .collect::<Option<Vec<u32>>>()
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| err("faces list vertex numbers from 1 to the number of vertices"))?;
    if corners > MAX_POINTS {
        return Err(ScriptError::new(format!("more than {MAX_POINTS} points")));
    }
    Ok(ComponentShape::Mesh { vertices, faces })
}

/// A cached result: the source and values it is for (to rule out hash
/// collisions) and the pieces or the error.
struct Entry {
    source: u64,
    input: ScriptInput,
    result: Result<Arc<Vec<ComponentItem>>, String>,
}

#[derive(Default)]
struct Inner {
    results: HashMap<u64, Entry>,
    order: VecDeque<u64>,
    last_good: HashMap<ScriptOwner, Arc<Vec<ComponentItem>>>,
    runs: u64,
}

/// The [`ScriptEval`] geometry uses: runs a script only for source and
/// values it has not seen, and keeps the last pieces that worked for each
/// owner to show while a script fails.
#[derive(Default)]
pub struct ScriptCache {
    inner: Mutex<Inner>,
}

fn hash_of(source: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut h);
    h.finish()
}

fn key_of(source: u64, input: &ScriptInput) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut h);
    for (k, v) in &input.numbers {
        k.hash(&mut h);
        v.to_bits().hash(&mut h);
    }
    input.texts.hash(&mut h);
    for (k, p) in &input.hotspots {
        k.hash(&mut h);
        p.to_array().map(f64::to_bits).hash(&mut h);
    }
    h.finish()
}

/// The cache shared by everything in the process that resolves components.
/// Scripts are pure, so sharing it only saves runs; the last pieces that
/// worked are kept per owner.
pub fn shared() -> &'static ScriptCache {
    static SHARED: std::sync::OnceLock<ScriptCache> = std::sync::OnceLock::new();
    SHARED.get_or_init(ScriptCache::new)
}

impl ScriptCache {
    pub fn new() -> ScriptCache {
        ScriptCache::default()
    }

    /// How many times a script ran (not counting cached results).
    pub fn runs(&self) -> u64 {
        self.inner.lock().map_or(0, |i| i.runs)
    }

    fn cached(&self, key: u64, source: u64, input: &ScriptInput) -> Option<Result<Arc<Vec<ComponentItem>>, String>> {
        let inner = self.inner.lock().ok()?;
        let e = inner.results.get(&key)?;
        (e.source == source && &e.input == input).then(|| e.result.clone())
    }
}

impl ScriptEval for ScriptCache {
    fn eval(&self, source: &str, input: &ScriptInput, owner: ScriptOwner) -> ScriptResult {
        let source_hash = hash_of(source);
        let key = key_of(source_hash, input);
        let result = match self.cached(key, source_hash, input) {
            Some(r) => r,
            None => {
                // Run without the lock: other threads may run scripts too.
                let r = evaluate(source, input).map(Arc::new).map_err(|e| e.to_string());
                if let Ok(mut inner) = self.inner.lock() {
                    inner.runs += 1;
                    if inner.results.len() >= CACHE_SIZE
                        && let Some(old) = inner.order.pop_front()
                    {
                        inner.results.remove(&old);
                    }
                    let entry = Entry { source: source_hash, input: input.clone(), result: r.clone() };
                    if inner.results.insert(key, entry).is_none() {
                        inner.order.push_back(key);
                    }
                }
                r
            }
        };
        let Ok(mut inner) = self.inner.lock() else {
            return match result {
                Ok(items) => ScriptResult { items: Some(items), error: None },
                Err(error) => ScriptResult { items: None, error: Some(error) },
            };
        };
        match result {
            Ok(items) => {
                if inner.last_good.len() >= OWNERS && !inner.last_good.contains_key(&owner) {
                    inner.last_good.clear();
                }
                inner.last_good.insert(owner, items.clone());
                ScriptResult { items: Some(items), error: None }
            }
            Err(error) => ScriptResult { items: inner.last_good.get(&owner).cloned(), error: Some(error) },
        }
    }
}

/// Merge what a script declares into a definition's parameters and
/// hotspots: new ones are added, those still declared keep their values
/// when the kind (and, for choices, the value) still fits, and those the
/// script no longer declares are removed. Parameters made by hand stay.
/// Fails when a declaration has the name of a parameter made by hand.
pub fn sync(params: &mut Vec<ComponentParam>, hotspots: &mut Vec<Hotspot>, decl: &Declarations) -> Result<(), String> {
    for d in &decl.params {
        if params.iter().any(|p| p.name == d.name && !p.from_script) {
            return Err(format!("'{}' is already a parameter of the component", d.name));
        }
    }
    let old: BTreeMap<String, ComponentParam> =
        params.iter().filter(|p| p.from_script).map(|p| (p.name.clone(), p.clone())).collect();
    params.retain(|p| !p.from_script);
    for d in &decl.params {
        let mut p = d.clone();
        if let Some(o) = old.get(&d.name).filter(|o| o.kind == d.kind) {
            if p.kind.is_text() {
                if p.accepts_text(&o.text) {
                    p.text = o.text.clone();
                }
            } else {
                p.value = o.value;
            }
        }
        params.push(p);
    }
    let old: BTreeMap<String, [f64; 3]> = hotspots.iter().map(|h| (h.name.clone(), h.position)).collect();
    *hotspots = decl
        .hotspots
        .iter()
        .map(|h| Hotspot { position: old.get(&h.name).copied().unwrap_or(h.position), ..h.clone() })
        .collect();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;

    const CABINET: &str = r##"
param.length  { "depth", label = "Depth", default = 0.6 }
param.choice  { "finish", { {"plain", "Plain"}, {"blue", "Blue"} }, default = "blue" }
param.length  { "width", default = 0.9, instance = true }
param.boolean { "doors", default = true, instance = true }
param.material { "body", default = "Wood" }
hotspot { "size", 0.9, 0.6, 0, label = "Size", shape = "diamond", color = "#ffccddee" }

function geometry(p)
  if not p.doors then return {} end
  local pieces = { box { 0, 0, 0, p.width, p.depth, 0.9, name = "body", material = p.body } }
  if p.finish == "blue" then
    pieces[#pieces + 1] = cylinder { x = p.size.x, y = p.size.y, radius = 0.05, height = 1 }
  end
  return pieces
end
"##;

    fn input(decl: &Declarations) -> ScriptInput {
        let def = cc_core::ComponentDef {
            params: decl.params.clone(),
            hotspots: decl.hotspots.clone(),
            ..Default::default()
        };
        ScriptInput {
            numbers: def.values(&BTreeMap::new()),
            texts: def.texts(&BTreeMap::new()),
            hotspots: def.hotspot_positions(&BTreeMap::new()),
        }
    }

    #[test]
    fn declarations_are_read() {
        let d = declare(CABINET).unwrap();
        let names: Vec<&str> = d.params.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["depth", "finish", "width", "doors", "body"]);
        assert!(d.params.iter().all(|p| p.from_script));
        assert_eq!(d.params[0].label, "Depth");
        assert_eq!(d.params[0].value, 0.6);
        assert_eq!(d.params[1].text, "blue");
        assert_eq!(d.params[1].choices.len(), 2);
        assert!(d.params[2].instance && !d.params[0].instance);
        assert_eq!((d.params[3].kind, d.params[3].value), (ParamKind::Boolean, 1.0));
        assert_eq!(d.params[4].text, "Wood");
        let h = &d.hotspots[0];
        assert_eq!(
            (h.name.as_str(), h.position, h.shape, h.color),
            ("size", [0.9, 0.6, 0.0], HotspotShape::Diamond, [0xff, 0xcc, 0xdd, 0xee])
        );
    }

    #[test]
    fn geometry_returns_pieces() {
        let d = declare(CABINET).unwrap();
        let mut input = input(&d);
        let items = evaluate(CABINET, &input).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "body");
        assert_eq!(items[0].material, "Wood");
        assert!(matches!(&items[0].shape, ComponentShape::Box { width, .. } if width == "0.9"));
        assert!(matches!(&items[1].shape, ComponentShape::Cylinder { x, .. } if x == "0.9"));
        // Booleans arrive as booleans, texts as text, hotspots as points.
        input.numbers.insert("doors".into(), 0.0);
        assert!(evaluate(CABINET, &input).unwrap().is_empty());
        input.numbers.insert("doors".into(), 1.0);
        input.texts.insert("finish".into(), "plain".into());
        input.hotspots.insert("size".into(), DVec3::new(2.0, 0.0, 0.0));
        assert_eq!(evaluate(CABINET, &input).unwrap().len(), 1);
    }

    #[test]
    fn every_builder_makes_a_piece_that_resolves() {
        let src = r#"
function geometry(p)
  return {
    box { 0, 0, 0, 1, 1, 1 },
    cylinder { 0, 0, 0, 0.5, 1 },
    extrusion { { {0, 0}, {1, 0}, {0, 1} }, 0, 1 },
    profile { { {0, 0}, {1, 0}, {1, 1, radius = 0.5}, {0, 1} }, 0, 0.2, plane = "front" },
    revolution { { {0.1, 0}, {0.5, 0}, {0.5, 1}, {0.1, 1} }, plane = "front" },
    sweep { path = { {0, 0}, {2, 0} }, profile = { {0, 0}, {0.1, 0}, {0.1, 0.1}, {0, 0.1} } },
    blend { { {0, 0}, {1, 0}, {1, 1}, {0, 1} }, { {0.2, 0.2}, {0.8, 0.2}, {0.8, 0.8}, {0.2, 0.8} }, 0, 1 },
    mesh { { {0, 0, 0}, {1, 0, 0}, {0, 1, 0}, {0, 0, 1} }, { {1, 3, 2}, {1, 2, 4}, {2, 3, 4}, {3, 1, 4} } },
    { box { 2, 0, 0, 1, 1, 1, void = true, detail = { "fine" } } },
  }
end
"#;
        let items = evaluate(src, &ScriptInput::default()).unwrap();
        assert_eq!(items.len(), 9);
        assert!(items[8].void && items[8].detail.fine && !items[8].detail.coarse);
        let def = cc_core::ComponentDef { items, ..Default::default() };
        let (parts, errors) = def.resolve(&BTreeMap::new());
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(parts.len(), 9);
    }

    #[test]
    fn mistakes_are_errors_with_lines() {
        let err = |src: &str| evaluate(src, &ScriptInput::default()).unwrap_err();
        let e = err("param.length { \"a\" }\nparam.number { \"a\" }\nfunction geometry() end");
        assert_eq!(e.line, Some(2), "{e:?}");
        assert!(e.message.contains("declared twice"), "{e:?}");
        assert!(err("x = = 1").line.is_some());
        assert!(err("return 1").message.contains("geometry"));
        assert!(err("function geometry() error('nope') end").message.contains("nope"));
        assert!(err("function geometry() param.length { 'late' } end").message.contains("outside geometry"));
        assert!(err("function geometry() return { box { 0, 0, 0, 1, 1 } } end").message.contains("height"));
        assert!(err("function geometry() return { 1 } end").message.contains("pieces"));
        assert!(
            err("function geometry() return { mesh { { {0,0,0} }, { {1, 2, 3} } } } end").message.contains("vertex")
        );
        assert!(declare("hotspot { 'h', shape = 'hexagon' }").is_err());
        assert!(declare("hotspot { 'h', color = 'red' }").is_err());
        assert!(declare("param.choice { 'c', { {'a', 'A'} }, default = 'b' }").is_err());
        assert!(declare("param.choice { 'c', {} }").is_err());
        assert!(declare("param.boolean { 'b', default = 1 }").is_err());
        assert!(declare("param.length { '1x' }").is_err());
    }

    #[test]
    fn scripts_cannot_escape_or_run_forever() {
        let err = |src: &str| evaluate(src, &ScriptInput::default()).unwrap_err().message;
        assert_eq!(err("while true do end"), BUDGET_SPENT);
        assert_eq!(err("function geometry() while true do end end"), BUDGET_SPENT);
        assert_eq!(
            err("function geometry() while true do pcall(function() while true do end end) end end"),
            BUDGET_SPENT
        );
        assert_eq!(err("local s = ('x'):rep(100 * 1024 * 1024)"), "the script used too much memory");
        for name in ["io", "os", "require", "load", "dofile", "debug", "package", "ccad"] {
            let src = format!("assert({name} == nil, '{name}') function geometry() return {{}} end");
            assert!(evaluate(&src, &ScriptInput::default()).is_ok(), "{name} is reachable");
        }
        let many =
            "function geometry() local t = {} for i = 1, 10001 do t[i] = box { 0, 0, 0, 1, 1, 1 } end return t end";
        assert!(err(many).contains("pieces"));
    }

    #[test]
    fn random_numbers_repeat() {
        let src = "function geometry() return { box { 0, 0, 0, math.random() + 0.1, 1, 1 } } end";
        let a = evaluate(src, &ScriptInput::default()).unwrap();
        let b = evaluate(src, &ScriptInput::default()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn the_cache_runs_once_per_values_and_keeps_the_last_good_pieces() {
        let cache = ScriptCache::new();
        let owner = ScriptOwner::default();
        let src = "param.length { 'w', default = 1 } function geometry(p) assert(p.w > 0, 'w must be positive') return { box { 0, 0, 0, p.w, 1, 1 } } end";
        let mut input = ScriptInput { numbers: BTreeMap::from([("w".to_string(), 1.0)]), ..Default::default() };
        let first = cache.eval(src, &input, owner);
        assert!(first.error.is_none());
        cache.eval(src, &input, owner);
        cache.eval(src, &input, ScriptOwner { piece: 3, ..owner });
        assert_eq!(cache.runs(), 1);
        input.numbers.insert("w".into(), -1.0);
        let failed = cache.eval(src, &input, owner);
        assert!(failed.error.as_deref().is_some_and(|e| e.contains("w must be positive")), "{failed:?}");
        assert_eq!(failed.items, first.items, "the last good pieces are kept");
        let other = cache.eval(src, &input, ScriptOwner { piece: 9, ..owner });
        assert!(other.items.is_none());
        assert_eq!(cache.runs(), 2);
    }

    #[test]
    fn sync_keeps_values_of_compatible_declarations() {
        let mut params = vec![ComponentParam::number("handmade", 1.0, ParamKind::Length)];
        let mut hotspots = Vec::new();
        sync(&mut params, &mut hotspots, &declare(CABINET).unwrap()).unwrap();
        assert_eq!(params.len(), 6);
        let set = |params: &mut Vec<ComponentParam>, name: &str, f: &dyn Fn(&mut ComponentParam)| {
            if let Some(p) = params.iter_mut().find(|p| p.name == name) {
                f(p);
            }
        };
        set(&mut params, "depth", &|p| p.value = 0.7);
        set(&mut params, "finish", &|p| p.text = "plain".into());
        set(&mut params, "width", &|p| p.value = 2.0);
        hotspots[0].position = [5.0, 0.0, 0.0];
        // Width becomes a number, the choice loses "plain", depth only changes its label, doors goes away.
        let edited = CABINET
            .replace("param.length  { \"width\", default = 0.9", "param.text { \"width\", default = \"wide\"")
            .replace("{\"plain\", \"Plain\"}, ", "")
            .replace("label = \"Depth\"", "label = \"Deep\"")
            .replace("param.boolean { \"doors\", default = true, instance = true }", "");
        sync(&mut params, &mut hotspots, &declare(&edited).unwrap()).unwrap();
        let p = |name: &str| params.iter().find(|p| p.name == name).unwrap();
        assert_eq!((p("depth").value, p("depth").label.as_str()), (0.7, "Deep"));
        assert_eq!(p("finish").text, "blue");
        assert_eq!(p("width").kind, ParamKind::Text);
        assert!(params.iter().all(|p| p.name != "doors"));
        assert_eq!(p("handmade").value, 1.0);
        assert_eq!(hotspots[0].position, [5.0, 0.0, 0.0]);
        // A declaration may not take the name of a hand-made parameter.
        assert!(sync(&mut params, &mut hotspots, &declare("param.length { 'handmade' }").unwrap()).is_err());
    }
}
