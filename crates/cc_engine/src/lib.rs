//! The ConstructCAD engine: every user-facing operation as a named command
//! that runs on a [`Session`] without a GUI.
//!
//! A command is a struct of typed parameters (deserialized from JSON, with a
//! JSON Schema generated from the same type) and a `run` method. The
//! [`catalog`] lists them; [`Session::execute`] runs one by name. The
//! command line (`ccad`), the MCP server and, area by area, the desktop app
//! all go through this one catalog, so an operation behaves the same
//! wherever it is called from.
//!
//! Lengths are in meters, angles in degrees, points are `[x, y]` in plan.
//! Edits go through `Document::edit`, so each command is one undo step.
//! Levels, types, views and materials can be named by id or by name;
//! elements by id.

mod defaults;
pub mod dimensions;
mod error;
pub mod i18n;
pub mod reference;
mod refs;
mod registry;
pub mod script;
mod session;
pub mod skp;

pub mod commands;

/// The fields of an enum value's variant as `*_get` shows them in `params`.
pub fn params_of<T: serde::Serialize>(value: &T) -> serde_json::Value {
    commands::variant(serde_json::to_value(value).unwrap_or_default()).1
}

/// An element's `params` as `element_get` shows them (an imported drawing
/// without the drawing itself, which is not a parameter).
pub fn element_params(kind: &cc_core::ElementKind) -> serde_json::Value {
    commands::cad::params(kind)
}

pub use defaults::{
    conduit_offset, default_roof_base, default_top, device_height, drain_slope, next_number, pipe_offset, walls_loop,
};
pub use error::{EngineError, ErrorCode, Result};
pub use refs::{Ref, category_name};
pub use registry::{Command, CommandInfo, CommandKind, catalog, describe};
pub use session::{Session, new_project};
