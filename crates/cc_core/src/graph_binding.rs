//! Elements made by visual programming graphs.
//!
//! A graph (`cc_engine::graph`) run creates elements through engine
//! commands; each one is bound to the call that made it, so the next run
//! changes it in place instead of adding another. Bindings live in
//! [`crate::Project::graph_bindings`], keyed by element, and are tracked by
//! undo like the other project settings; removing an element drops its
//! binding, and copies of an element are never bound.

use crate::Uid;
use serde::{Deserialize, Serialize};

/// Deepest lacing path kept (list nesting of a graph value).
pub const MAX_PATH: usize = 16;

/// The call of a graph node that made an element.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GraphBinding {
    /// The graph (its file's `uid`).
    pub graph: Uid,
    /// The node's id inside the graph.
    pub node: u64,
    /// The indices of the call through the lists its inputs were mapped
    /// over (empty for a single call).
    #[serde(default)]
    pub path: Vec<u32>,
    /// Which of the elements that call made (most calls make one).
    #[serde(default)]
    pub index: u32,
}

impl GraphBinding {
    /// Whether it can be kept (a file is hostile input).
    pub fn is_valid(&self) -> bool {
        !self.graph.is_nil() && self.path.len() <= MAX_PATH
    }

    /// Whether it was made by `node` of `graph`.
    pub fn of_node(&self, graph: Uid, node: u64) -> bool {
        self.graph == graph && self.node == node
    }
}
