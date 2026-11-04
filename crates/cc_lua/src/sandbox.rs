//! The sandboxed Lua state scripts and plugins run in.
//!
//! Only the `table`, `string`, `math`, `utf8` and `coroutine` libraries are
//! loaded, and the base library without the functions that reach files or
//! load code (`dofile`, `loadfile`, `load`, `require`) or the collector:
//! a script touches the project only through engine commands, and files only
//! through the commands that read and write them, when it may. Each run has
//! an instruction budget (a hook counts them in every coroutine) and the
//! state a memory limit; `pcall`, `xpcall` and `coroutine.resume` pass the
//! budget's error on, so a script cannot catch it and keep going.

use mlua::{HookTriggers, Lua, LuaOptions, StdLib, Value as LuaValue, VmState};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// How much a script may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Lua instructions per run (a script, or one plugin command).
    pub instructions: u64,
    /// Memory of the Lua state, in bytes.
    pub memory: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { instructions: 500_000_000, memory: 256 * 1024 * 1024 }
    }
}

/// The message of the error raised when the instruction budget is spent.
pub const BUDGET_SPENT: &str = "the script ran too long (instruction limit reached)";

/// Lines of `print` output kept per run, and their total size.
const MAX_LINES: usize = 10_000;
const MAX_OUTPUT: usize = 1024 * 1024;

/// Instructions between two checks of the budget.
const STEP: u32 = 10_000;

/// Passes the budget's error on through the functions that catch errors.
const PRELUDE: &str = r#"
local spent = ...
local raw_error = error
local raw_pcall, raw_xpcall, raw_resume = pcall, xpcall, coroutine.resume
local function check(...)
  local message = spent()
  if message then raw_error(message, 0) end
  return ...
end
pcall = function(...) return check(raw_pcall(...)) end
xpcall = function(...) return check(raw_xpcall(...)) end
coroutine.resume = function(...) return check(raw_resume(...)) end
"#;

/// A sandboxed Lua state with its budget and captured output.
pub struct Sandbox {
    pub lua: Lua,
    limits: Limits,
    spent: Rc<Cell<u64>>,
    output: Rc<RefCell<Output>>,
}

#[derive(Default)]
struct Output {
    lines: Vec<String>,
    bytes: usize,
    dropped: bool,
}

impl Sandbox {
    pub fn new(limits: Limits) -> mlua::Result<Sandbox> {
        let libs = StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8 | StdLib::COROUTINE;
        let lua = Lua::new_with(libs, LuaOptions::new())?;
        lua.set_memory_limit(limits.memory)?;
        let globals = lua.globals();
        for name in ["dofile", "loadfile", "load", "require", "collectgarbage"] {
            globals.raw_set(name, LuaValue::Nil)?;
        }
        if let Ok(string) = globals.get::<mlua::Table>("string") {
            string.raw_set("dump", LuaValue::Nil)?;
        }

        let spent = Rc::new(Cell::new(0u64));
        let counter = spent.clone();
        let limit = limits.instructions;
        lua.set_global_hook(HookTriggers::new().every_nth_instruction(STEP), move |_, _| {
            let n = counter.get().saturating_add(u64::from(STEP));
            counter.set(n);
            if n > limit { Err(mlua::Error::runtime(BUDGET_SPENT)) } else { Ok(VmState::Continue) }
        })?;
        let counter = spent.clone();
        let check = lua.create_function(move |_, ()| Ok((counter.get() > limit).then_some(BUDGET_SPENT)))?;
        lua.load(PRELUDE).set_name("=sandbox").call::<()>(check)?;

        let output = Rc::new(RefCell::new(Output::default()));
        let sink = output.clone();
        let print = lua.create_function(move |_, args: mlua::MultiValue| {
            let parts: Vec<String> =
                args.iter().map(|v| v.to_string().unwrap_or_else(|_| v.type_name().into())).collect();
            let line = parts.join("\t");
            let mut out = sink.borrow_mut();
            if out.lines.len() >= MAX_LINES || out.bytes + line.len() > MAX_OUTPUT {
                out.dropped = true;
            } else {
                out.bytes += line.len();
                out.lines.push(line);
            }
            Ok(())
        })?;
        globals.raw_set("print", print)?;
        drop(globals);
        Ok(Sandbox { lua, limits, spent, output })
    }

    /// Start a new run: a full instruction budget.
    pub fn reset_budget(&self) {
        self.spent.set(0);
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// The lines printed since the last call (a last line notes any dropped).
    pub fn take_output(&self) -> Vec<String> {
        let mut out = self.output.borrow_mut();
        let mut lines = std::mem::take(&mut out.lines);
        if out.dropped {
            lines.push(format!("(output after {} lines or {} bytes left out)", MAX_LINES, MAX_OUTPUT));
        }
        *out = Output::default();
        lines
    }
}
