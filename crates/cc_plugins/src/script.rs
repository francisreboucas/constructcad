//! Lua scripts: a file of Lua code run once over a session (`ccad lua`).
//! The script gets its arguments as `ccad.args` (and as `...`), and what it
//! returns is its result.

use crate::api::{Access, Api, engine_error};
use crate::convert::{from_lua, to_lua};
use crate::sandbox::{Limits, Sandbox};
use cc_engine::{EngineError, ErrorCode, Session};
use mlua::Value as LuaValue;
use serde_json::Value;

/// Largest script accepted, in bytes.
pub const MAX_SCRIPT: usize = 4 * 1024 * 1024;

/// How a script runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptOptions {
    pub limits: Limits,
    pub access: Access,
}

impl Default for ScriptOptions {
    fn default() -> Self {
        Self { limits: Limits::default(), access: Access::ALL }
    }
}

/// What a script did: its result (or error) and the lines it printed.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptOutput {
    pub result: Result<Value, EngineError>,
    pub output: Vec<String>,
}

/// Run the Lua `source` (named `name` in errors) over `session` with `args`.
/// Each command it runs is its own undo step.
pub fn run_script(
    session: &mut Session,
    source: &str,
    name: &str,
    args: Value,
    options: ScriptOptions,
) -> ScriptOutput {
    if source.len() > MAX_SCRIPT {
        let error =
            EngineError::new(ErrorCode::Script, format!("{name}: the script is larger than {MAX_SCRIPT} bytes"));
        return ScriptOutput { result: Err(error), output: Vec::new() };
    }
    let sandbox = match Sandbox::new(options.limits) {
        Ok(s) => s,
        Err(e) => return ScriptOutput { result: Err(engine_error(&e)), output: Vec::new() },
    };
    let result = run_in(&sandbox, session, source, name, &args, options.access);
    ScriptOutput { result: result.map_err(|e| engine_error(&e)), output: sandbox.take_output() }
}

fn run_in(
    sandbox: &Sandbox,
    session: &mut Session,
    source: &str,
    name: &str,
    args: &Value,
    access: Access,
) -> mlua::Result<Value> {
    let lua = &sandbox.lua;
    let api = Api::install(lua)?;
    let lua_args = to_lua(lua, args)?;
    api.ccad.raw_set("args", lua_args.clone())?;
    // Text only: binary chunks could crash the interpreter.
    let chunk = lua.load(source).set_name(format!("@{name}")).set_mode(mlua::ChunkMode::Text).into_function()?;
    sandbox.reset_budget();
    api.with_session(lua, session, access, |lua| {
        let out: LuaValue = chunk.call(lua_args)?;
        from_lua(lua, &out).map_err(|e| mlua::Error::runtime(format!("the script's result: {e}")))
    })
}
