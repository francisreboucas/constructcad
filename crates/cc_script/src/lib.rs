//! ConstructCAD's sandboxed Lua 5.4.
//!
//! [`sandbox`] is the restricted state every Lua run uses: no files, no
//! loading of code, an instruction budget and a memory limit. [`convert`]
//! turns JSON values into Lua values and back, bounded in depth and size.
//! [`object`] runs the scripts of scripted component pieces.
//! The crate depends only on `cc_core`, so geometry can run scripts without
//! the engine; `cc_lua` builds scripts and plugins over engine commands on
//! top of it.

pub mod convert;
pub mod object;
pub mod sandbox;

pub use object::{Declarations, ScriptCache, ScriptError, declare, evaluate, shared};
pub use sandbox::{Limits, Sandbox};
