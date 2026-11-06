//! Visual programming graphs (see `crate::graph`): run a `.ccgraph`, check
//! it, list the nodes a graph can use, and see or release the elements a
//! graph made.

use crate::error::{EngineError, Result};
use crate::graph::eval::{DefKind, node_def};
use crate::graph::{Evaluator, Graph, MAX_FILE, Port, builtin};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use cc_core::{ElementId, Uid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// The graph a command works on: a file or the graph itself.
fn load(session: &Session, path: Option<&str>, graph: Option<&Value>) -> Result<(Graph, usize)> {
    let (mut g, repairs) = match (path, graph) {
        (_, Some(v)) => Graph::from_value(v.clone()).map_err(EngineError::invalid)?,
        (Some(p), None) => {
            let size = std::fs::metadata(p).map_err(|e| EngineError::invalid(format!("{p}: {e}")))?.len();
            if usize::try_from(size).map_or(true, |s| s > MAX_FILE) {
                return Err(EngineError::invalid(format!("{p}: the graph file is too large")));
            }
            let text = std::fs::read_to_string(p).map_err(|e| EngineError::invalid(format!("{p}: {e}")))?;
            Graph::from_json(&text).map_err(|e| EngineError::invalid(format!("{p}: {e}")))?
        }
        (None, None) => return Err(EngineError::invalid("give the graph's `path` or the `graph` itself")),
    };
    // A command run by another command (a plugin's) has no say over files.
    if session.called_by_command() {
        g.allow_files = false;
    }
    Ok((g, repairs.total()))
}

fn uid(text: &str) -> Result<Uid> {
    u128::from_str_radix(text.trim(), 16)
        .ok()
        .map(Uid)
        .filter(|u| !u.is_nil())
        .ok_or_else(|| EngineError::invalid(format!("{text} is not a graph uid")))
}

/// Run a visual programming graph (a `.ccgraph` file, or the graph as
/// JSON) as one undo step. Elements its nodes create are bound to the node
/// and the item that made them: running the graph again changes them in
/// place (keeping their ids), makes the new ones and deletes the ones it no
/// longer makes. Returns each node's outputs or error, the values of its
/// `output` nodes and how many elements were created, changed and deleted.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct GraphRun {
    /// Path of the `.ccgraph` file.
    #[serde(default)]
    pub path: Option<String>,
    /// The graph itself (the contents of a `.ccgraph` file), instead of `path`.
    #[serde(default)]
    pub graph: Option<Value>,
    /// Values of the graph's input nodes, by their names.
    #[serde(default)]
    pub inputs: BTreeMap<String, Value>,
    /// Leave each node's outputs out of the result (only errors, outputs and counts).
    #[serde(default)]
    pub brief: bool,
}

impl Command for GraphRun {
    const NAME: &'static str = "graph_run";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (graph, repairs) = load(s, self.path.as_deref(), self.graph.as_ref())?;
        let report = Evaluator::new().run(s, &graph, &self.inputs);
        let mut v = serde_json::to_value(&report).map_err(|e| EngineError::invalid(e.to_string()))?;
        if self.brief
            && let Some(nodes) = v.get_mut("nodes").and_then(Value::as_object_mut)
        {
            nodes.retain(|_, n| n.get("error").is_some() || n.get("skipped").is_some());
            for n in nodes.values_mut() {
                if let Some(o) = n.as_object_mut() {
                    o.remove("outputs");
                }
            }
        }
        if let Some(o) = v.as_object_mut() {
            o.insert("graph".into(), json!(graph.uid.to_string()));
            o.insert("repairs".into(), json!(repairs));
        }
        Ok(v)
    }
}

/// Check a graph without running it: nodes that cannot run here (unknown
/// commands or nodes, file commands the graph does not allow), wires to
/// ports that do not exist or whose types do not fit, required inputs with
/// no value, and what reading the file fixed.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct GraphCheck {
    /// Path of the `.ccgraph` file.
    #[serde(default)]
    pub path: Option<String>,
    /// The graph itself, instead of `path`.
    #[serde(default)]
    pub graph: Option<Value>,
}

impl Command for GraphCheck {
    const NAME: &'static str = "graph_check";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (graph, repairs) = load(s, self.path.as_deref(), self.graph.as_ref())?;
        Ok(
            json!({ "problems": problems(s, &graph), "repairs": repairs, "nodes": graph.nodes.len(), "wires": graph.wires.len() }),
        )
    }
}

/// The problems `graph_check` reports, as `{node, message}`.
pub fn problems(s: &Session, graph: &Graph) -> Vec<Value> {
    let mut out = Vec::new();
    let defs: BTreeMap<u64, _> = graph.nodes.iter().map(|n| (n.id, node_def(s, graph, n))).collect();
    for n in &graph.nodes {
        let Some(def) = defs.get(&n.id) else { continue };
        if let DefKind::Missing(why) = &def.kind {
            out.push(json!({ "node": n.id, "message": why }));
            continue;
        }
        for port in def.inputs.iter().filter(|p| p.required) {
            if graph.wire_into(n.id, &port.name).is_none()
                && !n.values.contains_key(&port.name)
                && port.default.is_none()
            {
                out.push(json!({ "node": n.id, "message": format!("input \"{}\" has no value", port.name) }));
            }
        }
    }
    for w in &graph.wires {
        let from = defs.get(&w.from.0).and_then(|d| d.outputs.iter().find(|p| p.name == w.from.1));
        let to = defs.get(&w.to.0).and_then(|d| d.inputs.iter().find(|p| p.name == w.to.1));
        match (from, to) {
            (Some(a), Some(b)) if !a.ty.fits(&b.ty) => out.push(json!({
                "node": w.to.0,
                "message": format!("a {} is wired into \"{}\", which takes a {}", a.ty.label(), b.name, b.ty.label()),
            })),
            (Some(_), Some(_)) => {}
            (None, _) if defs.get(&w.from.0).is_some_and(|d| !matches!(d.kind, DefKind::Missing(_))) => {
                out.push(json!({ "node": w.from.0, "message": format!("no output \"{}\"", w.from.1) }))
            }
            (_, None) if defs.get(&w.to.0).is_some_and(|d| !matches!(d.kind, DefKind::Missing(_))) => {
                out.push(json!({ "node": w.to.0, "message": format!("no input \"{}\"", w.to.1) }))
            }
            _ => {}
        }
    }
    out
}

/// A node of the catalog as `graph_nodes` lists it.
#[derive(Debug, Clone, Serialize)]
pub struct NodeInfo {
    /// The node's `kind` in a graph file (`{"command": name}`, `{"builtin": name}` or `{"lua": code}`).
    pub kind: Value,
    pub name: String,
    pub group: String,
    pub description: String,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    /// Whether it may change the model.
    pub edits: bool,
}

/// Every node a graph can use on this session: built-in nodes, a Lua node,
/// then a node per command.
pub fn node_catalog(s: &Session) -> Vec<NodeInfo> {
    let graph = Graph::new("");
    let mut out: Vec<NodeInfo> = builtin::catalog()
        .into_iter()
        .map(|b| {
            let edits = matches!(b.eval, builtin::Eval::Edit(_));
            NodeInfo {
                kind: json!({ "builtin": b.name }),
                name: b.name.into(),
                group: b.group.into(),
                description: b.description.into(),
                inputs: b.inputs,
                outputs: b.outputs,
                edits,
            }
        })
        .collect();
    out.push(NodeInfo {
        kind: json!({ "lua": "return x" }),
        name: "lua".into(),
        group: "Script".into(),
        description: "A Lua function: its inputs are locals (the node's `inputs`), it returns its outputs (the node's `outputs`) in order, or one table with their names. It runs in the sandbox, without access to the model.".into(),
        inputs: vec![Port::new("x", crate::graph::PortType::Any)],
        outputs: vec![Port::new("result", crate::graph::PortType::Any)],
        edits: false,
    });
    for info in s.commands() {
        let node = crate::graph::Node::new(0, crate::graph::NodeKind::Command(info.name.clone()), [0.0, 0.0]);
        let def = node_def(s, &graph, &node);
        if matches!(def.kind, DefKind::Missing(_)) && info.kind != CommandKind::File {
            continue;
        }
        let edits = def.edits() || info.kind != CommandKind::Query;
        let d = crate::graph::eval::command_def(&info);
        out.push(NodeInfo {
            kind: json!({ "command": info.name }),
            name: info.name,
            group: info.group,
            description: info.description,
            inputs: d.inputs,
            outputs: d.outputs,
            edits,
        });
    }
    out
}

/// The nodes a graph can use: built-in nodes (inputs, math, logic, lists,
/// geometry, element parameters, outputs), the Lua node and a node per
/// engine command, each with its inputs and outputs (name, type, whether
/// required, default). Filter by words of the name or description, or by
/// group.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct GraphNodes {
    /// Words that must all appear in the node's name, group or description.
    #[serde(default)]
    pub search: Option<String>,
    /// Only this group (`Math`, `Lists`, `Creating elements`…).
    #[serde(default)]
    pub group: Option<String>,
}

impl Command for GraphNodes {
    const NAME: &'static str = "graph_nodes";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let words: Vec<String> = self.search.unwrap_or_default().split_whitespace().map(str::to_lowercase).collect();
        let nodes: Vec<NodeInfo> = node_catalog(s)
            .into_iter()
            .filter(|n| self.group.as_ref().is_none_or(|g| n.group.eq_ignore_ascii_case(g)))
            .filter(|n| {
                let hay = format!("{} {} {}", n.name, n.group, n.description).to_lowercase();
                words.iter().all(|w| hay.contains(w))
            })
            .collect();
        serde_json::to_value(nodes).map_err(|e| EngineError::invalid(e.to_string()))
    }
}

/// The elements a graph made, with the node and item (lacing path) that
/// made each.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct GraphElements {
    /// The graph's uid (its file's `uid`, also in `graph_run`'s result).
    pub graph: String,
}

impl Command for GraphElements {
    const NAME: &'static str = "graph_elements";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let g = uid(&self.graph)?;
        let list: Vec<Value> = s
            .project()
            .graph_bindings
            .iter()
            .filter(|(_, b)| b.graph == g)
            .map(|(id, b)| json!({ "id": id.0, "node": b.node, "path": b.path, "index": b.index }))
            .collect();
        Ok(Value::Array(list))
    }
}

/// Release elements from the graph that made them: they become ordinary
/// elements, and the next run of the graph makes new ones in their place.
/// Without `elements`, every element of the graph is released.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct GraphRelease {
    /// The graph's uid.
    pub graph: String,
    /// Ids of the elements to release (default: all of the graph's).
    #[serde(default)]
    pub elements: Option<Vec<u64>>,
}

impl Command for GraphRelease {
    const NAME: &'static str = "graph_release";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let g = uid(&self.graph)?;
        let ids: Vec<ElementId> = s
            .project()
            .graph_bindings
            .iter()
            .filter(|(id, b)| b.graph == g && self.elements.as_ref().is_none_or(|l| l.contains(&id.0)))
            .map(|(id, _)| *id)
            .collect();
        let n = ids.len();
        super::edit(s, "graph_release".into(), |e| {
            for id in ids {
                e.set_graph_binding(id, None);
            }
            Ok(())
        })?;
        let mut out = Map::new();
        out.insert("released".into(), json!(n));
        Ok(Value::Object(out))
    }
}
