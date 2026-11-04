//! Commands added at run time (plugins). An [`Extension`] set on a
//! [`Session`] lists its commands next to the built-in catalog and runs them
//! when [`Session::execute`] is asked for one of their names, so plugin
//! commands reach the GUI, the command line and the MCP server like the
//! built-in ones.
//!
//! A command of an extension is one undo step however many commands it
//! runs, and a failed one leaves nothing behind.

use crate::error::Result;
use crate::registry::CommandInfo;
use crate::session::Session;
use serde_json::Value;

/// Separates an extension's id from its command's name in the full name
/// (`<id>__<name>`). Built-in names never contain it.
pub const SEPARATOR: &str = "__";

/// A source of commands beyond the built-in catalog.
pub trait Extension {
    /// The commands it adds, with their full names.
    fn commands(&self) -> Vec<CommandInfo>;

    /// Run the command `name` (one of [`Extension::commands`]) on `session`,
    /// or `None` when the name is not one of its own. The extension is taken
    /// out of the session while it runs, so commands it runs through
    /// [`Session::execute`] reach the built-in catalog only.
    fn run(&mut self, session: &mut Session, name: &str, args: Value) -> Option<Result<Value>>;

    /// Messages for the user since the last call (what plugins printed).
    fn take_messages(&mut self) -> Vec<String> {
        Vec::new()
    }
}

/// Whether `name` could be the full name of an extension command.
pub fn is_extension_name(name: &str) -> bool {
    name.contains(SEPARATOR)
}
