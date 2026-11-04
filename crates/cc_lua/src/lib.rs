//! Lua scripts and plugins for ConstructCAD.
//!
//! Lua 5.4 runs sandboxed ([`sandbox`]): no files, no loading of code, an
//! instruction budget and a memory limit. Lua code reaches the project only
//! through the engine's commands (`ccad.run`, see [`api`]), the same ones the
//! GUI, the command line and the MCP server use.
//!
//! - [`run_script`] runs a script once (`ccad lua file.lua`).
//! - [`plugin`] loads plugins (a folder with `plugin.toml` and `main.lua`)
//!   whose commands join the session's catalog through [`PluginHost`], an
//!   engine [`cc_engine::Extension`]: each plugin command is one undo step
//!   and leaves nothing behind when it fails.

pub mod api;
pub mod convert;
pub mod plugin;
pub mod sandbox;
pub mod script;

pub use api::{API_VERSION, Access};
pub use plugin::{Manifest, Permissions, Plugin, PluginError, PluginHost};
pub use sandbox::Limits;
pub use script::{ScriptOptions, ScriptOutput, run_script};
