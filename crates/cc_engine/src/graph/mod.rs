//! Visual programming graphs: nodes joined by wires, evaluated as a
//! dataflow over the engine's commands.
//!
//! Every command of the catalog is a node (its parameters are the inputs,
//! its result the outputs), next to built-in nodes (inputs, math, lists,
//! geometry as JSON values) and Lua nodes (pure functions run in the
//! sandbox). A node given lists where it takes single values runs once per
//! item ([`lace`]). Elements an edit node creates are bound to its call
//! ([`cc_core::graph_binding`]), so running the graph again changes them in
//! place, makes the new ones and deletes the ones no longer made ([`eval`]).
//!
//! A graph is a `.ccgraph` file (JSON, [`Graph`]); files are hostile input:
//! [`Graph::from_json`] bounds sizes, drops dangling wires and refuses
//! cycles.

pub mod builtin;
pub mod eval;
pub mod lace;
pub mod ports;

use cc_core::Uid;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub use eval::{Evaluator, NodeResult, RunReport};
pub use lace::Lacing;
pub use ports::{Port, PortType};

/// Current `.ccgraph` format version.
pub const GRAPH_FORMAT: u32 = 1;
/// Most nodes in a graph.
pub const MAX_NODES: usize = 5_000;
/// Most wires in a graph.
pub const MAX_WIRES: usize = 20_000;
/// Largest text in a node (Lua code, notes), in bytes.
pub const MAX_TEXT: usize = 256 * 1024;
/// Largest `.ccgraph` file, in bytes.
pub const MAX_FILE: usize = 32 * 1024 * 1024;
/// Most items a list value may hold.
pub const MAX_LIST: usize = 100_000;
/// Most node calls in one run.
pub const MAX_CALLS: usize = 100_000;

/// When a graph runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    /// When asked to.
    #[default]
    Manual,
    /// After every change of the graph.
    Automatic,
}

/// What a node is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// An engine command (built-in, or a plugin's `<id>__<name>`).
    Command(String),
    /// A built-in node ([`builtin::catalog`]).
    Builtin(String),
    /// A Lua function; its inputs and outputs are the node's `inputs` and
    /// `outputs`.
    Lua(String),
}

/// A box of the graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Unique in the graph; bindings refer to it.
    pub id: u64,
    pub kind: NodeKind,
    /// Position on the canvas.
    #[serde(default)]
    pub pos: [f32; 2],
    /// A label shown instead of the node's name; on an input node, the name
    /// of the graph input it is.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub lacing: Lacing,
    /// Values of inputs left unwired (and the value of input nodes).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, Value>,
    /// Lua nodes: names of the inputs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<String>,
    /// Lua nodes: names of the outputs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<String>,
    /// Shows all its inputs (a command node otherwise shows the required,
    /// wired and set ones).
    #[serde(default, skip_serializing_if = "is_default")]
    pub expanded: bool,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

impl Node {
    pub fn new(id: u64, kind: NodeKind, pos: [f32; 2]) -> Self {
        let lua = matches!(kind, NodeKind::Lua(_));
        Self {
            id,
            kind,
            pos,
            name: String::new(),
            lacing: Lacing::default(),
            values: BTreeMap::new(),
            inputs: if lua { vec!["x".into()] } else { Vec::new() },
            outputs: if lua { vec!["result".into()] } else { Vec::new() },
            expanded: false,
        }
    }
}

/// An output of one node joined to an input of another.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Wire {
    /// Node id and output name.
    pub from: (u64, String),
    /// Node id and input name.
    pub to: (u64, String),
}

/// A text on the canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub pos: [f32; 2],
    pub text: String,
}

/// Nodes framed together under a title.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub title: String,
    pub nodes: BTreeSet<u64>,
    /// Color as `[r, g, b]`.
    #[serde(default)]
    pub color: [u8; 3],
}

/// A visual programming graph (`.ccgraph`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    #[serde(default = "format")]
    pub format: u32,
    /// Persistent id: the elements it made are bound to it.
    #[serde(default)]
    pub uid: Uid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub run_mode: RunMode,
    /// Whether its commands may read and write files.
    #[serde(default)]
    pub allow_files: bool,
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub wires: Vec<Wire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Note>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<Group>,
}

fn format() -> u32 {
    GRAPH_FORMAT
}

impl Default for Graph {
    fn default() -> Self {
        Self::new("")
    }
}

/// What reading a graph file fixed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Repairs {
    /// Wires dropped (dangling, repeated, a second wire into one input, or
    /// closing a cycle).
    pub wires: usize,
    /// Nodes dropped (repeated ids, oversized text).
    pub nodes: usize,
}

impl Repairs {
    pub fn total(&self) -> usize {
        self.wires + self.nodes
    }
}

impl Graph {
    pub fn new(name: &str) -> Self {
        Self {
            format: GRAPH_FORMAT,
            uid: Uid::generate(),
            name: name.into(),
            run_mode: RunMode::Manual,
            allow_files: false,
            nodes: Vec::new(),
            wires: Vec::new(),
            notes: Vec::new(),
            groups: Vec::new(),
        }
    }

    /// Read a graph (a file is hostile input) and fix what can be fixed.
    pub fn from_json(text: &str) -> Result<(Graph, Repairs), String> {
        if text.len() > MAX_FILE {
            return Err(format!("the graph file is larger than {} MB", MAX_FILE / (1024 * 1024)));
        }
        let mut graph: Graph = serde_json::from_str(text).map_err(|e| format!("invalid graph file: {e}"))?;
        if graph.format > GRAPH_FORMAT {
            return Err(format!("unsupported graph format {} (maximum {GRAPH_FORMAT})", graph.format));
        }
        let repairs = graph.repair()?;
        Ok((graph, repairs))
    }

    /// Read a graph from a JSON value (a command's argument).
    pub fn from_value(value: Value) -> Result<(Graph, Repairs), String> {
        let mut graph: Graph = serde_json::from_value(value).map_err(|e| format!("invalid graph: {e}"))?;
        let repairs = graph.repair()?;
        Ok((graph, repairs))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// Make the graph consistent: bounded sizes, unique node ids, wires
    /// between existing nodes, one wire per input, no cycles.
    pub fn repair(&mut self) -> Result<Repairs, String> {
        if self.nodes.len() > MAX_NODES {
            return Err(format!("the graph has more than {MAX_NODES} nodes"));
        }
        if self.wires.len() > MAX_WIRES {
            return Err(format!("the graph has more than {MAX_WIRES} wires"));
        }
        let mut repairs = Repairs::default();
        if self.uid.is_nil() {
            self.uid = Uid::generate();
        }
        let mut ids = BTreeSet::new();
        let before = self.nodes.len();
        self.nodes.retain(|n| {
            let text = match &n.kind {
                NodeKind::Lua(code) => code.len(),
                NodeKind::Command(s) | NodeKind::Builtin(s) => s.len(),
            };
            let pos_ok = n.pos.iter().all(|c| c.is_finite());
            text <= MAX_TEXT && n.inputs.len() <= 64 && n.outputs.len() <= 64 && pos_ok && ids.insert(n.id)
        });
        repairs.nodes = before - self.nodes.len();
        for n in &mut self.nodes {
            dedup_names(&mut n.inputs);
            dedup_names(&mut n.outputs);
        }
        self.notes.retain(|n| n.text.len() <= MAX_TEXT && n.pos.iter().all(|c| c.is_finite()));
        for g in &mut self.groups {
            g.nodes.retain(|id| ids.contains(id));
        }
        let before = self.wires.len();
        let wires = std::mem::take(&mut self.wires);
        let mut inputs = BTreeSet::new();
        for w in wires {
            let ok = w.from.0 != w.to.0
                && ids.contains(&w.from.0)
                && ids.contains(&w.to.0)
                && !inputs.contains(&w.to)
                && !self.reaches(w.to.0, w.from.0);
            if ok {
                inputs.insert(w.to.clone());
                self.wires.push(w);
            }
        }
        repairs.wires = before - self.wires.len();
        Ok(repairs)
    }

    pub fn node(&self, id: u64) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: u64) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// A free node id.
    pub fn next_id(&self) -> u64 {
        self.nodes.iter().map(|n| n.id).max().map_or(1, |m| m.saturating_add(1))
    }

    /// The wire into an input, if any.
    pub fn wire_into(&self, node: u64, input: &str) -> Option<&Wire> {
        self.wires.iter().find(|w| w.to.0 == node && w.to.1 == input)
    }

    /// Whether `to` can be reached from `from` along wires (bounded by the
    /// node count).
    pub fn reaches(&self, from: u64, to: u64) -> bool {
        let mut seen = BTreeSet::new();
        let mut pending = vec![from];
        while let Some(n) = pending.pop() {
            if n == to {
                return true;
            }
            if !seen.insert(n) || seen.len() > MAX_NODES {
                continue;
            }
            pending.extend(self.wires.iter().filter(|w| w.from.0 == n).map(|w| w.to.0));
        }
        false
    }

    /// Join an output to an input, replacing the input's wire; refused when
    /// it would close a cycle.
    pub fn connect(&mut self, from: (u64, String), to: (u64, String)) -> Result<(), String> {
        if from.0 == to.0 || self.reaches(to.0, from.0) {
            return Err("the wire would make a loop".into());
        }
        if self.node(from.0).is_none() || self.node(to.0).is_none() {
            return Err("no such node".into());
        }
        if self.wires.len() >= MAX_WIRES {
            return Err(format!("the graph has {MAX_WIRES} wires already"));
        }
        self.wires.retain(|w| w.to != to);
        self.wires.push(Wire { from, to });
        Ok(())
    }

    /// Remove a node and its wires.
    pub fn remove_node(&mut self, id: u64) {
        self.nodes.retain(|n| n.id != id);
        self.wires.retain(|w| w.from.0 != id && w.to.0 != id);
        for g in &mut self.groups {
            g.nodes.remove(&id);
        }
    }

    /// The nodes in an order where every node comes after the nodes wired
    /// into it (the graph has no cycles once repaired; a cycle's nodes are
    /// left out).
    pub fn order(&self) -> Vec<u64> {
        let mut incoming: BTreeMap<u64, usize> = self.nodes.iter().map(|n| (n.id, 0)).collect();
        for w in &self.wires {
            if let Some(c) = incoming.get_mut(&w.to.0) {
                *c += 1;
            }
        }
        // Of the nodes ready to run, the oldest (lowest id) first: nodes that do
        // not depend on each other run in the order they were added.
        let mut ready: std::collections::BinaryHeap<std::cmp::Reverse<u64>> =
            self.nodes.iter().filter(|n| incoming.get(&n.id) == Some(&0)).map(|n| std::cmp::Reverse(n.id)).collect();
        let mut out = Vec::with_capacity(self.nodes.len());
        while let Some(std::cmp::Reverse(n)) = ready.pop() {
            out.push(n);
            for w in self.wires.iter().filter(|w| w.from.0 == n) {
                if let Some(c) = incoming.get_mut(&w.to.0) {
                    *c = c.saturating_sub(1);
                    if *c == 0 {
                        ready.push(std::cmp::Reverse(w.to.0));
                    }
                }
            }
        }
        out
    }
}

fn dedup_names(names: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    names.retain(|n| !n.is_empty() && n.len() <= 64 && seen.insert(n.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_files_are_repaired() {
        let text = r#"{
            "nodes": [
                {"id": 1, "kind": {"builtin": "number"}},
                {"id": 2, "kind": {"builtin": "add"}},
                {"id": 2, "kind": {"builtin": "add"}},
                {"id": 3, "kind": {"builtin": "add"}}
            ],
            "wires": [
                {"from": [1, "value"], "to": [2, "a"]},
                {"from": [1, "value"], "to": [2, "a"]},
                {"from": [9, "value"], "to": [2, "b"]},
                {"from": [2, "result"], "to": [3, "a"]},
                {"from": [3, "result"], "to": [2, "b"]}
            ]
        }"#;
        let (g, repairs) = Graph::from_json(text).unwrap();
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.wires.len(), 2);
        assert_eq!(repairs, Repairs { wires: 3, nodes: 1 });
        assert!(!g.uid.is_nil());
        assert_eq!(g.order(), vec![1, 2, 3]);
        assert!(Graph::from_json("[").is_err());
        assert!(Graph::from_json(r#"{"format": 99}"#).is_err());
    }

    #[test]
    fn connecting_refuses_loops() {
        let mut g = Graph::new("t");
        g.nodes.push(Node::new(1, NodeKind::Builtin("add".into()), [0.0, 0.0]));
        g.nodes.push(Node::new(2, NodeKind::Builtin("add".into()), [0.0, 0.0]));
        assert!(g.connect((1, "result".into()), (2, "a".into())).is_ok());
        assert!(g.connect((2, "result".into()), (1, "a".into())).is_err());
        // A second wire into an input replaces the first.
        assert!(g.connect((1, "result".into()), (2, "a".into())).is_ok());
        assert_eq!(g.wires.len(), 1);
    }
}
