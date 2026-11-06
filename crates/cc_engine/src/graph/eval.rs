//! Running a graph: nodes in dataflow order, lists mapped over, the
//! elements edit nodes create bound to their calls.
//!
//! A run is one undo step. For each call of a command that creates
//! elements, the elements made by the same call in an earlier run (the same
//! node and lacing path) take the new content and keep their ids, persistent
//! ids, marks, comments and phases, so what refers to them stays; new calls
//! make new elements, and after the run the elements of calls no longer
//! made are deleted. Running again with nothing changed changes nothing.

use super::builtin::{self, Builtin, Eval};
use super::lace::{self, Arg};
use super::ports::{Port, PortType, depth, type_of_schema};
use super::{Graph, MAX_CALLS, Node, NodeKind};
use crate::{CommandKind, ErrorCode, Session};
use cc_core::graph_binding::GraphBinding;
use cc_core::{ElementId, Uid};
use cc_script::{Limits, Sandbox};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

/// Commands a graph never runs: they replace the document, move in its
/// history, change a session setting or run graphs themselves.
const REFUSED: [&str; 7] =
    ["project_new", "project_open", "undo", "redo", "language_set", "graph_run", "graph_release"];

/// Lua instructions per call of a Lua node.
const LUA_INSTRUCTIONS: u64 = 50_000_000;

/// What a node is, resolved against the session's catalog.
#[derive(Clone)]
pub enum DefKind {
    Command {
        name: String,
        kind: CommandKind,
    },
    Builtin(Builtin),
    Lua,
    /// A node that cannot run here (unknown, refused, a file command in a
    /// graph that does not allow them), and why.
    Missing(String),
}

/// A node's ports and what running it does.
#[derive(Clone)]
pub struct NodeDef {
    pub title: String,
    pub group: String,
    pub description: String,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub kind: DefKind,
}

impl NodeDef {
    /// Whether running it may change the model.
    pub fn edits(&self) -> bool {
        match &self.kind {
            DefKind::Command { kind, .. } => *kind != CommandKind::Query,
            DefKind::Builtin(b) => matches!(b.eval, Eval::Edit(_)),
            _ => false,
        }
    }
}

/// The ports of a command, from its catalog entry.
pub fn command_def(info: &crate::CommandInfo) -> NodeDef {
    let schema = &info.input_schema;
    let required: BTreeSet<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut inputs: Vec<Port> = schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|props| {
            props
                .iter()
                .map(|(name, s)| {
                    let mut port = Port::new(name, type_of_schema(name, s));
                    port.required = required.contains(name.as_str());
                    port.description = s
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    port
                })
                .collect()
        })
        .unwrap_or_default();
    inputs.sort_by_key(|p| !p.required);
    let mut outputs = Vec::new();
    if info.kind == CommandKind::Edit && info.name.ends_with("_create") {
        outputs.push(Port::new("element", PortType::Element).describe("The id of the element made."));
    }
    outputs.push(Port::new("result", PortType::Any).describe("The command's whole result."));
    NodeDef {
        title: info.name.clone(),
        group: info.group.clone(),
        description: info.description.clone(),
        inputs,
        outputs,
        kind: DefKind::Command { name: info.name.clone(), kind: info.kind },
    }
}

fn builtin_def(b: Builtin) -> NodeDef {
    NodeDef {
        title: b.name.into(),
        group: b.group.into(),
        description: b.description.into(),
        inputs: b.inputs.clone(),
        outputs: b.outputs.clone(),
        kind: DefKind::Builtin(b),
    }
}

fn missing(title: &str, why: String) -> NodeDef {
    NodeDef {
        title: title.into(),
        group: String::new(),
        description: why.clone(),
        inputs: Vec::new(),
        outputs: Vec::new(),
        kind: DefKind::Missing(why),
    }
}

/// The definition of `node` in `graph`, against `session`'s commands.
pub fn node_def(session: &Session, graph: &Graph, node: &Node) -> NodeDef {
    match &node.kind {
        NodeKind::Builtin(name) => match builtin::find(name) {
            Some(b) => builtin_def(b),
            None => missing(name, format!("unknown node: {name}")),
        },
        NodeKind::Lua(_) => NodeDef {
            title: "lua".into(),
            group: "Script".into(),
            description: "A Lua function of its inputs.".into(),
            inputs: node.inputs.iter().map(|n| Port::new(n, PortType::Any)).collect(),
            outputs: node.outputs.iter().map(|n| Port::new(n, PortType::Any)).collect(),
            kind: DefKind::Lua,
        },
        NodeKind::Command(name) => {
            if REFUSED.contains(&name.as_str()) {
                return missing(name, format!("{name} cannot run in a graph"));
            }
            match session.describe(name) {
                Some(info) if info.kind == CommandKind::File && !graph.allow_files => {
                    missing(name, format!("{name} works with files, which this graph does not allow"))
                }
                Some(info) => command_def(&info),
                None => missing(name, format!("unknown command: {name}")),
            }
        }
    }
}

/// How one node fared in a run.
#[derive(Debug, Clone, Default, Serialize)]
pub struct NodeResult {
    /// Its outputs by name.
    pub outputs: BTreeMap<String, Value>,
    /// Why it failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Not run, and why: an input failed or has no value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
    /// Reused from the previous run (nothing it depends on changed).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cached: bool,
}

/// What a run did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RunReport {
    pub nodes: BTreeMap<u64, NodeResult>,
    /// The values of the graph's `output` nodes, by name.
    pub outputs: BTreeMap<String, Value>,
    pub created: usize,
    pub changed: usize,
    pub deleted: usize,
    /// Nodes that failed.
    pub errors: usize,
    /// What Lua nodes printed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<String>,
}

struct Cached {
    key: u64,
    outputs: BTreeMap<String, Value>,
}

/// Runs graphs; keeps each node's last outputs, so a run after a change of
/// the graph alone re-runs only the nodes it affects.
#[derive(Default)]
pub struct Evaluator {
    cache: HashMap<(Uid, u64), Cached>,
    /// The document revision after the last run: the cache holds while the
    /// model has not changed since.
    revision: Option<u64>,
    lua: Option<Sandbox>,
}

/// The elements made by earlier runs, by call.
type Bound = HashMap<(u64, Vec<u32>), Vec<(u32, ElementId)>>;

struct Run<'a> {
    graph: &'a Graph,
    report: RunReport,
    bound: Bound,
    /// Calls that made elements in this run: node, path, index.
    made: BTreeSet<(u64, Vec<u32>, u32)>,
    budget: usize,
}

impl Evaluator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget the cached outputs (the next run runs every node).
    pub fn clear(&mut self) {
        self.cache.clear();
        self.revision = None;
    }

    /// Run `graph` on `session` as one undo step; `inputs` set the graph's
    /// named input nodes.
    pub fn run(&mut self, session: &mut Session, graph: &Graph, inputs: &BTreeMap<String, Value>) -> RunReport {
        if self.revision != Some(session.doc.revision()) {
            self.cache.clear();
        }
        let depth = session.doc.undo_depth();
        session.doc.break_merge();
        let mut run = Run {
            graph,
            report: RunReport::default(),
            bound: bound_of(session, graph.uid),
            made: BTreeSet::new(),
            budget: MAX_CALLS,
        };
        // Nodes whose elements stay as they are: cached, failed or skipped.
        let mut keep: BTreeSet<u64> = BTreeSet::new();
        for id in graph.order() {
            let Some(node) = graph.node(id) else { continue };
            let def = node_def(session, graph, node);
            let result = self.run_node(session, &mut run, node, &def, inputs);
            if result.cached || result.error.is_some() || result.skipped.is_some() {
                keep.insert(id);
            }
            if result.error.is_some() {
                run.report.errors += 1;
            }
            run.report.nodes.insert(id, result);
        }
        let stale: Vec<ElementId> = session
            .project()
            .graph_bindings
            .iter()
            .filter(|(_, b)| b.graph == graph.uid)
            .filter(|(_, b)| {
                graph.node(b.node).is_none()
                    || (!keep.contains(&b.node) && !run.made.contains(&(b.node, b.path.clone(), b.index)))
            })
            .map(|(id, _)| *id)
            .collect();
        if !stale.is_empty() {
            let removed = crate::commands::edit(session, "graph".into(), |e| {
                let mut n = 0;
                for id in &stale {
                    if e.project().elements.contains_key(id) {
                        e.remove_element(*id);
                        n += 1;
                    }
                }
                Ok(n)
            });
            run.report.deleted += removed.unwrap_or(0);
        }
        let name = if graph.name.is_empty() { "graph".to_string() } else { format!("graph: {}", graph.name) };
        session.doc.squash_since(depth, name);
        self.revision = Some(session.doc.revision());
        if let Some(lua) = &self.lua {
            run.report.messages.extend(lua.take_output());
        }
        run.report
    }

    fn run_node(
        &mut self,
        session: &mut Session,
        run: &mut Run,
        node: &Node,
        def: &NodeDef,
        inputs: &BTreeMap<String, Value>,
    ) -> NodeResult {
        if let DefKind::Missing(why) = &def.kind {
            return NodeResult { error: Some(why.clone()), ..Default::default() };
        }
        // A named input node takes the run's value.
        let mut node = node.clone();
        if let DefKind::Builtin(b) = &def.kind
            && b.input
            && let Some(v) = inputs.get(&node.name).filter(|_| !node.name.is_empty())
        {
            node.values.insert("value".into(), v.clone());
        }
        // Gather the inputs.
        let mut args = Vec::with_capacity(def.inputs.len());
        for port in &def.inputs {
            let value = match run.graph.wire_into(node.id, &port.name) {
                Some(w) => match run.report.nodes.get(&w.from.0) {
                    Some(r) if r.error.is_none() && r.skipped.is_none() => match r.outputs.get(&w.from.1) {
                        Some(v) => Some(v.clone()),
                        None => return skipped(format!("{} has no output \"{}\"", w.from.0, w.from.1)),
                    },
                    _ => return skipped(format!("the node wired into \"{}\" did not run", port.name)),
                },
                None => node.values.get(&port.name).cloned().filter(|v| !v.is_null()).or_else(|| port.default.clone()),
            };
            if value.is_none() && port.required {
                return skipped(format!("input \"{}\" has no value", port.name));
            }
            args.push((port, value));
        }
        if let DefKind::Builtin(b) = &def.kind
            && b.name == "output"
        {
            let name = if node.name.is_empty() { format!("output{}", node.id) } else { node.name.clone() };
            run.report.outputs.insert(name, args.first().and_then(|(_, v)| v.clone()).unwrap_or(Value::Null));
        }
        // Reuse the last outputs when nothing changed.
        let key = cache_key(&node, &args);
        if let Some(c) = self.cache.get(&(run.graph.uid, node.id))
            && c.key == key
        {
            return NodeResult { outputs: c.outputs.clone(), cached: true, ..Default::default() };
        }
        let laced: Vec<Arg> = args
            .into_iter()
            .map(|(port, value)| {
                let rank = port.ty.rank();
                let value = match (value, rank) {
                    // A single item where a list is taken: a list of one.
                    (Some(v), Some(r))
                        if matches!(port.ty, PortType::List(_)) && depth(&v) + 1 == r && !v.is_array() =>
                    {
                        Some(Value::Array(vec![v]))
                    }
                    (v, _) => v,
                };
                let levels = match (&value, rank) {
                    (Some(v), Some(r)) => depth(v).saturating_sub(r),
                    _ => 0,
                };
                Arg { value, levels }
            })
            .collect();
        let names: Vec<String> = def.inputs.iter().map(|p| p.name.clone()).collect();
        let outputs = def.outputs.len();
        let mut budget = run.budget;
        let result = {
            let mut call = |values: Vec<Option<Value>>, path: &[u32]| -> Result<Vec<Value>, String> {
                self.call(session, run, &node, def, &names, values, path)
            };
            lace::map(laced, node.lacing, outputs, &mut budget, &mut call)
        };
        run.budget = budget;
        match result {
            Ok(values) => {
                let outputs: BTreeMap<String, Value> = def.outputs.iter().map(|p| p.name.clone()).zip(values).collect();
                self.cache.insert((run.graph.uid, node.id), Cached { key, outputs: outputs.clone() });
                NodeResult { outputs, ..Default::default() }
            }
            Err(e) => {
                self.cache.remove(&(run.graph.uid, node.id));
                NodeResult { error: Some(e), ..Default::default() }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn call(
        &mut self,
        session: &mut Session,
        run: &mut Run,
        node: &Node,
        def: &NodeDef,
        names: &[String],
        values: Vec<Option<Value>>,
        path: &[u32],
    ) -> Result<Vec<Value>, String> {
        match &def.kind {
            DefKind::Builtin(b) => match b.eval {
                Eval::Pure(f) => f(&values, node),
                Eval::Query(f) | Eval::Edit(f) => f(session, &values, node),
            },
            DefKind::Lua => self.lua(node, names, values),
            DefKind::Command { name, kind } => {
                let mut args = Map::new();
                for (n, v) in names.iter().zip(values) {
                    if let Some(v) = v.filter(|v| !v.is_null()) {
                        args.insert(n.clone(), v);
                    }
                }
                let result = if *kind == CommandKind::Edit {
                    bound_call(session, run, node.id, name, Value::Object(args), path)?
                } else {
                    session.execute(name, Value::Object(args)).map_err(|e| e.message)?
                };
                Ok(def
                    .outputs
                    .iter()
                    .map(|p| match p.name.as_str() {
                        "element" => result.get("id").cloned().unwrap_or(Value::Null),
                        _ => result.clone(),
                    })
                    .collect())
            }
            DefKind::Missing(why) => Err(why.clone()),
        }
    }

    fn lua(&mut self, node: &Node, names: &[String], values: Vec<Option<Value>>) -> Result<Vec<Value>, String> {
        let NodeKind::Lua(code) = &node.kind else { return Err("not a Lua node".into()) };
        if let Some(bad) = names.iter().chain(&node.outputs).find(|n| !is_identifier(n)) {
            return Err(format!("\"{bad}\" is not a valid Lua name"));
        }
        if self.lua.is_none() {
            let sandbox = Sandbox::new(Limits { instructions: LUA_INSTRUCTIONS, ..Limits::default() })
                .map_err(|e| e.to_string())?;
            setup_lua(&sandbox.lua).map_err(|e| e.to_string())?;
            self.lua = Some(sandbox);
        }
        let Some(sandbox) = &self.lua else { return Err("no Lua state".into()) };
        sandbox.reset_budget();
        let lua = &sandbox.lua;
        let source =
            format!("local {} = ...\n{code}", if names.is_empty() { "_".to_string() } else { names.join(", ") });
        let mut args = mlua::MultiValue::new();
        for v in &values {
            args.push_back(
                cc_script::convert::to_lua(lua, v.as_ref().unwrap_or(&Value::Null)).map_err(|e| e.to_string())?,
            );
        }
        let returned: mlua::MultiValue =
            lua.load(source).set_name(format!("=node {}", node.id)).call(args).map_err(|e| lua_message(&e))?;
        let returned: Vec<mlua::Value> = returned.into_iter().collect();
        let mut out = Vec::with_capacity(node.outputs.len());
        // Several outputs: one value each, or a table with their names.
        let by_name = node.outputs.len() > 1 && returned.len() == 1;
        if by_name && let Some(mlua::Value::Table(t)) = returned.first() {
            for n in &node.outputs {
                let v: mlua::Value = t.get(n.as_str()).map_err(|e| e.to_string())?;
                out.push(cc_script::convert::from_lua(lua, &v)?);
            }
            return Ok(out);
        }
        for i in 0..node.outputs.len() {
            let v = returned.get(i).cloned().unwrap_or(mlua::Value::Nil);
            out.push(cc_script::convert::from_lua(lua, &v)?);
        }
        Ok(out)
    }
}

fn skipped(why: String) -> NodeResult {
    NodeResult { skipped: Some(why), ..Default::default() }
}

fn cache_key(node: &Node, args: &[(&Port, Option<Value>)]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(&(&node.kind, &node.lacing, &node.values, &node.inputs, &node.outputs))
        .unwrap_or_default()
        .hash(&mut h);
    for (p, v) in args {
        p.name.hash(&mut h);
        serde_json::to_string(v).unwrap_or_default().hash(&mut h);
    }
    h.finish()
}

fn bound_of(session: &Session, graph: Uid) -> Bound {
    let mut bound: Bound = HashMap::new();
    for (id, b) in &session.project().graph_bindings {
        if b.graph == graph {
            bound.entry((b.node, b.path.clone())).or_default().push((b.index, *id));
        }
    }
    for list in bound.values_mut() {
        list.sort();
    }
    bound
}

/// Run an edit command for one call of a node: the elements this call made
/// in an earlier run take the new content in place.
fn bound_call(
    session: &mut Session,
    run: &mut Run,
    node: u64,
    name: &str,
    args: Value,
    path: &[u32],
) -> Result<Value, String> {
    let old: Vec<ElementId> = run
        .bound
        .remove(&(node, path.to_vec()))
        .unwrap_or_default()
        .into_iter()
        .map(|(_, id)| id)
        .filter(|id| session.project().elements.contains_key(id))
        .collect();
    let first = session.project().next_id();
    let mut result = session.execute(name, args.clone());
    if let Err(e) = &result
        && !old.is_empty()
        && e.code != ErrorCode::Internal
    {
        // The old elements may stand in the way (a room where one is):
        // try again without them, and keep them if that fails too.
        let depth = session.doc.undo_depth();
        let removed = crate::commands::edit(session, "graph".into(), |e| {
            for id in &old {
                e.remove_element(*id);
            }
            Ok(())
        });
        if removed.is_ok() {
            match session.execute(name, args) {
                Ok(r) => {
                    run.report.deleted += old.len();
                    let made = new_elements(session, first);
                    return bind(session, run, node, path, &[], &made, r);
                }
                Err(_) => session.undo_to(depth),
            }
        }
    }
    let r = result.as_mut().map_err(|e| e.message.clone())?.take();
    let made = new_elements(session, first);
    bind(session, run, node, path, &old, &made, r)
}

fn new_elements(session: &Session, first: u64) -> Vec<ElementId> {
    session.project().elements.range(ElementId(first)..).map(|(id, _)| *id).collect()
}

/// Bind what a call made: each new element takes the place of the old one
/// at the same index (when of the same category), the rest are new, and old
/// ones left over are deleted.
fn bind(
    session: &mut Session,
    run: &mut Run,
    node: u64,
    path: &[u32],
    old: &[ElementId],
    made: &[ElementId],
    mut result: Value,
) -> Result<Value, String> {
    let graph = run.graph.uid;
    let path = path.to_vec();
    let mut renamed: Vec<(u64, u64)> = Vec::new();
    let mut created = 0;
    let mut changed = 0;
    let outcome = crate::commands::edit(session, "graph".into(), |e| {
        for (k, &new) in made.iter().enumerate() {
            let index = u32::try_from(k).unwrap_or(u32::MAX);
            let binding = GraphBinding { graph, node, path: path.clone(), index };
            let target = old.get(k).copied().filter(|o| {
                let p = e.project();
                matches!((p.elements.get(o), p.elements.get(&new)), (Some(a), Some(b)) if a.category() == b.category())
            });
            let id = match target {
                Some(o) => {
                    let (Some(prev), Some(fresh)) =
                        (e.project().elements.get(&o).cloned(), e.project().elements.get(&new).cloned())
                    else {
                        continue;
                    };
                    let mut next = fresh;
                    next.id = prev.id;
                    next.uid = prev.uid;
                    next.mark = prev.mark.clone();
                    next.comments = prev.comments.clone();
                    next.phase_created = prev.phase_created;
                    next.phase_demolished = prev.phase_demolished;
                    e.remove_element(new);
                    if next != prev {
                        changed += 1;
                    }
                    if let Some(slot) = e.element_mut(o) {
                        *slot = next;
                    }
                    renamed.push((new.0, o.0));
                    o
                }
                None => {
                    created += 1;
                    new
                }
            };
            e.set_graph_binding(id, Some(binding));
        }
        let mut deleted = 0;
        for o in old
            .iter()
            .skip(made.len())
            .chain(old.iter().take(made.len()).filter(|o| !renamed.iter().any(|r| r.1 == o.0)))
        {
            if e.project().elements.contains_key(o) {
                e.remove_element(*o);
                deleted += 1;
            }
        }
        Ok(deleted)
    });
    let deleted = outcome.map_err(|e| e.message)?;
    run.report.created += created;
    run.report.changed += changed;
    run.report.deleted += deleted;
    for k in 0..made.len() {
        run.made.insert((node, path.clone(), u32::try_from(k).unwrap_or(u32::MAX)));
    }
    if !renamed.is_empty() {
        rename_ids(&mut result, &renamed);
        if let Some(id) = result.get("id").and_then(Value::as_u64)
            && renamed.iter().any(|r| r.1 == id)
            && let Ok(fresh) = session.execute("element_get", json!({ "id": id }))
        {
            result = fresh;
        }
    }
    Ok(result)
}

/// Replace ids of elements that took an old element's place.
fn rename_ids(v: &mut Value, renamed: &[(u64, u64)]) {
    fn walk(v: &mut Value, renamed: &[(u64, u64)], depth: usize) {
        if depth > 32 {
            return;
        }
        match v {
            Value::Object(m) => {
                for (k, item) in m.iter_mut() {
                    if matches!(k.as_str(), "id" | "ids" | "element" | "elements") {
                        swap(item, renamed);
                    } else {
                        walk(item, renamed, depth + 1);
                    }
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|i| walk(i, renamed, depth + 1)),
            _ => {}
        }
    }
    fn swap(v: &mut Value, renamed: &[(u64, u64)]) {
        match v {
            Value::Number(n) => {
                if let Some(r) = n.as_u64().and_then(|x| renamed.iter().find(|r| r.0 == x)) {
                    *v = json!(r.1);
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|i| swap(i, renamed)),
            _ => {}
        }
    }
    walk(v, renamed, 0);
}

fn setup_lua(lua: &mlua::Lua) -> mlua::Result<()> {
    use mlua::LuaSerdeExt;
    let ccad = lua.create_table()?;
    ccad.raw_set("null", mlua::Value::NULL)?;
    ccad.raw_set(
        "array",
        lua.create_function(|lua, t: Option<mlua::Table>| {
            let t = match t {
                Some(t) => t,
                None => lua.create_table()?,
            };
            t.set_metatable(Some(lua.array_metatable()))?;
            Ok(t)
        })?,
    )?;
    lua.globals().raw_set("ccad", ccad)
}

fn lua_message(e: &mlua::Error) -> String {
    match e {
        mlua::Error::CallbackError { cause, .. } => lua_message(cause),
        mlua::Error::RuntimeError(m) | mlua::Error::SyntaxError { message: m, .. } => m.clone(),
        other => other.to_string(),
    }
}

const LUA_KEYWORDS: [&str; 22] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in", "local", "nil",
    "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// Whether `name` can be a Lua local's name.
pub fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !LUA_KEYWORDS.contains(&name)
}
