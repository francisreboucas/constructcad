//! Visual programming: the graph editor, a panel under the views.
//!
//! The graph (`cc_engine::graph::Graph`) is edited here and run through the
//! engine's `graph_run` command, like every model change of the app; the
//! nodes come from `graph_nodes` (built-in nodes, the Lua node and a node per
//! engine command). The canvas is drawn with egui's painter: nodes, ports
//! colored by type, wires as curves, the value or error of each node under
//! it; an inspector on the right edits the selected node's inputs, Lua code
//! and lacing.

use crate::state::AppState;
use crate::t;
use crate::theme::{self, PALETTE};
use cc_engine::commands::graph::node_catalog;
use cc_engine::graph::{Graph, Lacing, Node, NodeKind, Port, PortType, RunMode, Wire};
use egui::epaint::CubicBezierShape;
use egui::{Align2, Color32, FontId, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

/// Node width, header height and port row height, in graph units.
const NODE_W: f32 = 190.0;
const HEADER: f32 = 24.0;
const ROW: f32 = 20.0;
const PORT_R: f32 = 5.0;
/// Graph undo steps kept.
const MAX_UNDO: usize = 100;
/// Matches shown by the node search.
const MAX_MATCHES: usize = 60;

/// A node of the catalog, as the editor shows it.
#[derive(Clone)]
struct Def {
    kind: NodeKind,
    name: String,
    group: String,
    description: String,
    inputs: Vec<Port>,
    outputs: Vec<Port>,
}

/// What a drag on the canvas is doing.
#[derive(Clone, Default)]
enum Drag {
    #[default]
    None,
    /// Moving the selected nodes (the step is taken once).
    Nodes,
    /// A wire from an output, to be dropped on an input.
    FromOutput(u64, String),
    /// A wire from an input, to be dropped on an output.
    FromInput(u64, String),
    /// Box selection from a corner (screen).
    Box(Pos2),
    Pan,
}

/// The node search, opened at a point of the graph.
struct Search {
    at: Pos2,
    screen: Pos2,
    query: String,
    focus: bool,
}

/// An action that would discard unsaved changes of the graph.
#[derive(Clone)]
enum Pending {
    New,
    Open,
    Close,
}

/// The graph editor's state (`AppState::graph`).
pub struct GraphEditor {
    pub open: bool,
    pub graph: Graph,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    /// The last run's result (`graph_run`), or why it could not run.
    pub report: Option<Result<Value, String>>,
    pan: Vec2,
    zoom: f32,
    pub selected: BTreeSet<u64>,
    drag: Drag,
    search: Option<Search>,
    undo: Vec<Graph>,
    redo: Vec<Graph>,
    catalog: Option<Vec<Def>>,
    /// Whether the pointer is over the panel: its keys go to the graph.
    pub hovered: bool,
    /// Changed since the last run (automatic runs).
    changed: bool,
    /// The input node being edited inline (one undo step per edit).
    editing: Option<u64>,
    /// Text of the inspector's value fields, by node and input.
    buffers: HashMap<(u64, String), String>,
    clipboard: Option<(Vec<Node>, Vec<Wire>)>,
    confirm: Option<Pending>,
}

impl Default for GraphEditor {
    fn default() -> Self {
        Self {
            open: false,
            graph: Graph::new(""),
            path: None,
            dirty: false,
            report: None,
            pan: vec2(40.0, 40.0),
            zoom: 1.0,
            selected: BTreeSet::new(),
            drag: Drag::None,
            search: None,
            undo: Vec::new(),
            redo: Vec::new(),
            catalog: None,
            hovered: false,
            changed: false,
            editing: None,
            buffers: HashMap::new(),
            clipboard: None,
            confirm: None,
        }
    }
}

impl GraphEditor {
    /// Record the graph before a change (one undo step).
    fn step(&mut self) {
        self.undo.push(self.graph.clone());
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.changed = true;
    }

    fn undo(&mut self) {
        if let Some(g) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.graph, g));
            self.after_history();
        }
    }

    fn redo(&mut self) {
        if let Some(g) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.graph, g));
            self.after_history();
        }
    }

    fn after_history(&mut self) {
        self.selected.retain(|id| self.graph.node(*id).is_some());
        self.buffers.clear();
        self.dirty = true;
        self.changed = true;
    }

    /// Replace the graph (new, opened), forgetting its history.
    fn replace(&mut self, graph: Graph, path: Option<PathBuf>) {
        self.graph = graph;
        self.path = path;
        self.dirty = false;
        self.report = None;
        self.selected.clear();
        self.undo.clear();
        self.redo.clear();
        self.buffers.clear();
        self.changed = false;
        self.pan = vec2(40.0, 40.0);
        self.zoom = 1.0;
    }

    fn def(&self, node: &Node) -> Option<Def> {
        let catalog = self.catalog.as_ref()?;
        match &node.kind {
            NodeKind::Lua(_) => Some(Def {
                kind: node.kind.clone(),
                name: "lua".into(),
                group: "Script".into(),
                description: catalog
                    .iter()
                    .find(|d| d.name == "lua")
                    .map(|d| d.description.clone())
                    .unwrap_or_default(),
                inputs: node.inputs.iter().map(|n| Port::new(n, PortType::Any)).collect(),
                outputs: node.outputs.iter().map(|n| Port::new(n, PortType::Any)).collect(),
            }),
            kind => catalog.iter().find(|d| &d.kind == kind).cloned(),
        }
    }

    /// The inputs a node shows: a command node's required, wired and set
    /// ones unless expanded; every input of other nodes. A node unknown here
    /// shows the ports its wires use.
    fn ports(&self, node: &Node) -> (Vec<Port>, Vec<Port>) {
        let Some(def) = self.def(node) else {
            let ins = self.graph.wires.iter().filter(|w| w.to.0 == node.id).map(|w| Port::new(&w.to.1, PortType::Any));
            let outs =
                self.graph.wires.iter().filter(|w| w.from.0 == node.id).map(|w| Port::new(&w.from.1, PortType::Any));
            let mut outs: Vec<Port> = outs.collect();
            outs.dedup_by(|a, b| a.name == b.name);
            return (ins.collect(), outs);
        };
        let command = matches!(node.kind, NodeKind::Command(_));
        let inputs = def
            .inputs
            .into_iter()
            .filter(|p| {
                !command
                    || node.expanded
                    || p.required
                    || node.values.contains_key(&p.name)
                    || self.graph.wire_into(node.id, &p.name).is_some()
            })
            .collect();
        (inputs, def.outputs)
    }

    fn result(&self, node: u64) -> Option<&Value> {
        match &self.report {
            Some(Ok(r)) => r.get("nodes").and_then(|n| n.get(node.to_string())),
            _ => None,
        }
    }

    /// The node's rectangle in graph units (the body; the value line is under it).
    fn node_rect(&self, node: &Node) -> Rect {
        let (ins, outs) = self.ports(node);
        let mut rows = ins.len().max(outs.len()) as f32;
        if inline_kind(node).is_some() {
            rows += 1.0;
        }
        Rect::from_min_size(pos2(node.pos[0], node.pos[1]), vec2(NODE_W, HEADER + ROW * rows.max(1.0) + 6.0))
    }
}

/// The inline editor an input node shows.
fn inline_kind(node: &Node) -> Option<&str> {
    match &node.kind {
        NodeKind::Builtin(n) if matches!(n.as_str(), "number" | "integer" | "slider" | "boolean" | "text") => {
            Some(n.as_str())
        }
        _ => None,
    }
}

fn type_color(t: &PortType) -> Color32 {
    match t {
        PortType::Number | PortType::Integer => PALETTE.accent,
        PortType::Point => PALETTE.success,
        PortType::Element | PortType::Ref => PALETTE.warning,
        PortType::Object | PortType::List(_) => PALETTE.temporary,
        PortType::Bool | PortType::Text => PALETTE.text_muted,
        PortType::Any => PALETTE.text_faint,
    }
}

/// Open or close the panel.
pub fn toggle(state: &mut AppState) {
    let g = &mut state.graph;
    if g.open {
        if g.dirty {
            g.confirm = Some(Pending::Close);
        } else {
            g.open = false;
        }
        return;
    }
    g.open = true;
    // Plugins may have changed the commands since the last time.
    g.catalog = None;
}

fn ensure_catalog(state: &mut AppState) {
    if state.graph.catalog.is_some() {
        return;
    }
    let defs = node_catalog(&state.session)
        .into_iter()
        .filter_map(|n| {
            let kind: NodeKind = serde_json::from_value(n.kind).ok()?;
            Some(Def {
                kind,
                name: n.name,
                group: n.group,
                description: n.description,
                inputs: n.inputs,
                outputs: n.outputs,
            })
        })
        .collect();
    state.graph.catalog = Some(defs);
}

/// Run the graph through the engine (one undo step).
pub fn run(state: &mut AppState) {
    let graph = match serde_json::to_value(&state.graph.graph) {
        Ok(v) => v,
        Err(e) => {
            state.graph.report = Some(Err(e.to_string()));
            return;
        }
    };
    let result = state.session.execute("graph_run", json!({ "graph": graph })).map_err(|e| e.message);
    if let Ok(r) = &result {
        state.message = status_text(r);
    }
    state.graph.report = Some(result);
    state.graph.changed = false;
}

fn status_text(r: &Value) -> String {
    let n = |k: &str| r.get(k).and_then(Value::as_u64).unwrap_or(0);
    let mut s = t!("graph-status", "created" => n("created"), "changed" => n("changed"), "deleted" => n("deleted"));
    if n("errors") > 0 {
        s = format!("{s} · {}", t!("graph-errors", "count" => n("errors")));
    }
    s
}

fn file_dialog(state: &AppState) -> rfd::FileDialog {
    let dialog = rfd::FileDialog::new().add_filter(t!("file-filter-graph"), &["ccgraph", "json"]);
    match state.prefs.macro_dir() {
        Some(dir) => dialog.set_directory(dir),
        None => dialog,
    }
}

fn new_graph(state: &mut AppState) {
    state.graph.replace(Graph::new(&t!("graph-untitled")), None);
}

fn open_graph(state: &mut AppState) {
    let Some(path) = file_dialog(state).pick_file() else { return };
    open_path(state, path);
}

/// Open the graph file at `path` in the editor.
pub fn open_path(state: &mut AppState, path: PathBuf) {
    let read = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| Graph::from_json(&t));
    match read {
        Ok((graph, repairs)) => {
            state.graph.replace(graph, Some(path));
            state.graph.open = true;
            state.graph.catalog = None;
            if repairs.total() > 0 {
                state.message = t!("graph-repaired", "count" => repairs.total());
            }
        }
        Err(e) => state.dialogs.error = Some(t!("graph-open-failed", "error" => e)),
    }
}

fn save_graph(state: &mut AppState, save_as: bool) -> bool {
    let path = match (&state.graph.path, save_as) {
        (Some(p), false) => p.clone(),
        _ => {
            let name =
                if state.graph.graph.name.is_empty() { t!("graph-untitled") } else { state.graph.graph.name.clone() };
            let Some(mut p) = file_dialog(state).set_file_name(format!("{name}.ccgraph")).save_file() else {
                return false;
            };
            if p.extension().is_none() {
                p.set_extension("ccgraph");
            }
            p
        }
    };
    match std::fs::write(&path, state.graph.graph.to_json()) {
        Ok(()) => {
            state.graph.path = Some(path);
            state.graph.dirty = false;
            true
        }
        Err(e) => {
            state.dialogs.error = Some(t!("graph-save-failed", "error" => e));
            false
        }
    }
}

/// Keys while the pointer is over the panel; returns whether the graph took them.
pub fn keys(ctx: &egui::Context, state: &mut AppState, undo: bool, redo: bool, save: bool, save_as: bool) -> bool {
    if !state.graph.open || !state.graph.hovered {
        return false;
    }
    if undo {
        state.graph.undo();
    }
    if redo {
        state.graph.redo();
    }
    if save || save_as {
        save_graph(state, save_as);
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
        run(state);
    }
    if ctx.memory(|m| m.focused().is_some()) {
        return true;
    }
    let cmd = |i: &mut egui::InputState, k| i.consume_key(Modifiers::COMMAND, k);
    let (copy, paste, all) = ctx.input_mut(|i| (cmd(i, Key::C), cmd(i, Key::V), cmd(i, Key::A)));
    let (delete, escape) =
        ctx.input(|i| (i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace), i.key_pressed(Key::Escape)));
    let g = &mut state.graph;
    if all {
        g.selected = g.graph.nodes.iter().map(|n| n.id).collect();
    }
    if copy {
        let nodes: Vec<Node> = g.graph.nodes.iter().filter(|n| g.selected.contains(&n.id)).cloned().collect();
        let wires: Vec<Wire> = g
            .graph
            .wires
            .iter()
            .filter(|w| g.selected.contains(&w.from.0) && g.selected.contains(&w.to.0))
            .cloned()
            .collect();
        g.clipboard = Some((nodes, wires));
    }
    if paste && let Some((nodes, wires)) = g.clipboard.clone() {
        g.step();
        let mut ids = BTreeMap::new();
        g.selected.clear();
        for mut n in nodes {
            let id = g.graph.next_id();
            ids.insert(n.id, id);
            n.id = id;
            n.pos = [n.pos[0] + 30.0, n.pos[1] + 30.0];
            g.selected.insert(id);
            g.graph.nodes.push(n);
        }
        for w in wires {
            if let (Some(&a), Some(&b)) = (ids.get(&w.from.0), ids.get(&w.to.0)) {
                let _ = g.graph.connect((a, w.from.1), (b, w.to.1));
            }
        }
    }
    if delete && !g.selected.is_empty() {
        g.step();
        for id in std::mem::take(&mut g.selected) {
            g.graph.remove_node(id);
        }
    }
    if escape {
        if g.search.is_some() {
            g.search = None;
        } else {
            g.selected.clear();
        }
    }
    true
}

/// The panel: toolbar, canvas and inspector.
pub fn show(ui: &mut Ui, state: &mut AppState) {
    ensure_catalog(state);
    state.graph.hovered = ui.rect_contains_pointer(ui.max_rect());
    toolbar(ui, state);
    ui.separator();
    egui::Panel::right("graph-inspector")
        .resizable(true)
        .default_size(300.0)
        .size_range(220.0..=520.0)
        .frame(egui::Frame::new().fill(PALETTE.panel).inner_margin(egui::Margin::same(8)))
        .show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| inspector(ui, state));
        });
    egui::CentralPanel::no_frame().show(ui, |ui| canvas(ui, state));
    confirm(ui.ctx(), state);
    // Automatic runs, once nothing is being dragged or typed.
    let g = &state.graph;
    let idle = matches!(g.drag, Drag::None) && !ui.ctx().memory(|m| m.focused().is_some());
    if g.graph.run_mode == RunMode::Automatic && g.changed && idle {
        run(state);
    }
}

fn toolbar(ui: &mut Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        let title =
            if state.graph.graph.name.is_empty() { t!("graph-untitled") } else { state.graph.graph.name.clone() };
        let dirty = if state.graph.dirty { " *" } else { "" };
        ui.label(theme::title(format!("{title}{dirty}")));
        ui.separator();
        if ui.button(t!("action-new")).clicked() {
            if state.graph.dirty {
                state.graph.confirm = Some(Pending::New);
            } else {
                new_graph(state);
            }
        }
        if ui.button(t!("action-open")).clicked() {
            if state.graph.dirty {
                state.graph.confirm = Some(Pending::Open);
            } else {
                open_graph(state);
            }
        }
        if ui.button(t!("action-save")).clicked() {
            save_graph(state, false);
        }
        if ui.button(t!("action-save-as")).clicked() {
            save_graph(state, true);
        }
        ui.separator();
        let g = &mut state.graph;
        if ui.add_enabled(!g.undo.is_empty(), egui::Button::new(t!("action-undo"))).clicked() {
            g.undo();
        }
        if ui.add_enabled(!g.redo.is_empty(), egui::Button::new(t!("action-redo"))).clicked() {
            g.redo();
        }
        if ui.button(t!("action-zoom-fit")).clicked() {
            fit(g, ui.available_rect_before_wrap().size());
        }
        ui.separator();
        let run_button =
            egui::Button::new(egui::RichText::new(t!("graph-run")).color(Color32::WHITE)).fill(PALETTE.accent);
        if ui.add(run_button).on_hover_text("F5").clicked() {
            run(state);
        }
        let mut auto = state.graph.graph.run_mode == RunMode::Automatic;
        if ui.checkbox(&mut auto, t!("graph-auto")).on_hover_text(t!("graph-auto-hint")).changed() {
            state.graph.step();
            state.graph.graph.run_mode = if auto { RunMode::Automatic } else { RunMode::Manual };
        }
        ui.separator();
        match &state.graph.report {
            Some(Ok(r)) => {
                let errors = r.get("errors").and_then(Value::as_u64).unwrap_or(0) > 0;
                let text = status_text(r);
                ui.label(if errors { egui::RichText::new(text).color(PALETTE.danger) } else { theme::muted(text) });
            }
            Some(Err(e)) => {
                ui.label(egui::RichText::new(e.clone()).color(PALETTE.danger));
            }
            None => {}
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(t!("action-close")).clicked() {
                toggle(state);
            }
        });
    });
}

/// Frame every node in a canvas of `size`.
fn fit(g: &mut GraphEditor, size: Vec2) {
    let mut bounds = Rect::NOTHING;
    for n in &g.graph.nodes {
        bounds = bounds.union(g.node_rect(n));
    }
    if !bounds.is_positive() {
        g.pan = vec2(40.0, 40.0);
        g.zoom = 1.0;
        return;
    }
    let bounds = bounds.expand(40.0);
    let zoom = (size.x / bounds.width()).min((size.y.max(100.0) - 20.0) / bounds.height()).clamp(0.25, 1.5);
    g.zoom = zoom;
    g.pan = -bounds.min.to_vec2();
}

fn confirm(ctx: &egui::Context, state: &mut AppState) {
    let Some(pending) = state.graph.confirm.clone() else { return };
    let r = crate::dialog::Dialog::new("graph-discard", t!("graph-discard-title")).show(
        ctx,
        |ui| {
            ui.label(t!("graph-discard-body"));
        },
        |f| {
            f.danger(t!("action-discard"));
            f.secondary(t!("action-cancel"));
        },
    );
    match r.clicked {
        Some(0) => {
            state.graph.confirm = None;
            state.graph.dirty = false;
            match pending {
                Pending::New => new_graph(state),
                Pending::Open => open_graph(state),
                Pending::Close => state.graph.open = false,
            }
        }
        Some(_) => state.graph.confirm = None,
        None if r.dismissed => state.graph.confirm = None,
        None => {}
    }
}

struct View {
    origin: Pos2,
    pan: Vec2,
    zoom: f32,
}

impl View {
    fn screen(&self, p: Pos2) -> Pos2 {
        self.origin + (p.to_vec2() + self.pan) * self.zoom
    }
    fn graph(&self, s: Pos2) -> Pos2 {
        ((s - self.origin) / self.zoom - self.pan).to_pos2()
    }
    fn rect(&self, r: Rect) -> Rect {
        Rect::from_min_max(self.screen(r.min), self.screen(r.max))
    }
}

/// What lies under a point of the canvas.
enum Hit {
    Output(u64, String),
    Input(u64, String),
    Node(u64),
    Empty,
}

fn hit(g: &GraphEditor, view: &View, p: Pos2) -> Hit {
    let reach = (PORT_R + 4.0) * view.zoom.max(0.6);
    for n in g.graph.nodes.iter().rev() {
        let r = view.rect(g.node_rect(n));
        if !r.expand(reach).contains(p) {
            continue;
        }
        let (ins, outs) = g.ports(n);
        for (i, port) in outs.iter().enumerate() {
            if port_pos(view, r, i, false).distance(p) <= reach {
                return Hit::Output(n.id, port.name.clone());
            }
        }
        for (i, port) in ins.iter().enumerate() {
            if port_pos(view, r, i, true).distance(p) <= reach {
                return Hit::Input(n.id, port.name.clone());
            }
        }
        if r.contains(p) {
            return Hit::Node(n.id);
        }
    }
    Hit::Empty
}

fn port_pos(view: &View, r: Rect, i: usize, input: bool) -> Pos2 {
    let y = r.min.y + (HEADER + ROW * (i as f32 + 0.5)) * view.zoom;
    pos2(if input { r.min.x } else { r.max.x }, y)
}

fn wire_shape(a: Pos2, b: Pos2, zoom: f32, stroke: Stroke) -> egui::Shape {
    let dx = ((b.x - a.x).abs() * 0.5).max(40.0 * zoom);
    let points = [a, a + vec2(dx, 0.0), b - vec2(dx, 0.0), b];
    egui::Shape::CubicBezier(CubicBezierShape::from_points_stroke(points, false, Color32::TRANSPARENT, stroke))
}

fn canvas(ui: &mut Ui, state: &mut AppState) {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, PALETTE.canvas);
    let pointer = ui.input(|i| i.pointer.hover_pos());

    // Zoom about the pointer.
    if resp.hovered()
        && let Some(p) = pointer
    {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let g = &mut state.graph;
            let view = View { origin: rect.min, pan: g.pan, zoom: g.zoom };
            let at = view.graph(p);
            g.zoom = (g.zoom * (scroll * 0.0015).exp()).clamp(0.25, 2.5);
            g.pan = (p - rect.min) / g.zoom - at.to_vec2();
        }
    }
    let view = View { origin: rect.min, pan: state.graph.pan, zoom: state.graph.zoom };

    // Grid.
    let step = 40.0 * view.zoom;
    if step > 8.0 {
        let start = view.screen(pos2(0.0, 0.0));
        let grid = Stroke::new(1.0, PALETTE.hover);
        let mut x = rect.min.x + (start.x - rect.min.x).rem_euclid(step);
        while x < rect.max.x {
            painter.line_segment([pos2(x, rect.min.y), pos2(x, rect.max.y)], grid);
            x += step;
        }
        let mut y = rect.min.y + (start.y - rect.min.y).rem_euclid(step);
        while y < rect.max.y {
            painter.line_segment([pos2(rect.min.x, y), pos2(rect.max.x, y)], grid);
            y += step;
        }
    }

    interact(ui, state, &resp, &view, rect);
    let g = &state.graph;

    // Wires.
    let mut port_at: HashMap<(u64, String, bool), (Pos2, PortType)> = HashMap::new();
    for n in &g.graph.nodes {
        let r = view.rect(g.node_rect(n));
        let (ins, outs) = g.ports(n);
        for (i, p) in ins.iter().enumerate() {
            port_at.insert((n.id, p.name.clone(), true), (port_pos(&view, r, i, true), p.ty.clone()));
        }
        for (i, p) in outs.iter().enumerate() {
            port_at.insert((n.id, p.name.clone(), false), (port_pos(&view, r, i, false), p.ty.clone()));
        }
    }
    for w in &g.graph.wires {
        let a = port_at.get(&(w.from.0, w.from.1.clone(), false));
        let b = port_at.get(&(w.to.0, w.to.1.clone(), true));
        if let (Some((a, ty)), Some((b, _))) = (a, b) {
            painter.add(wire_shape(*a, *b, view.zoom, Stroke::new(2.0 * view.zoom.max(0.5), type_color(ty))));
        }
    }
    if let Some(p) = pointer {
        let dragging = match &g.drag {
            Drag::FromOutput(n, port) => port_at.get(&(*n, port.clone(), false)).map(|(a, _)| (*a, p)),
            Drag::FromInput(n, port) => port_at.get(&(*n, port.clone(), true)).map(|(b, _)| (p, *b)),
            _ => None,
        };
        if let Some((a, b)) = dragging {
            painter.add(wire_shape(a, b, view.zoom, Stroke::new(2.0, PALETTE.accent)));
        }
        if let Drag::Box(start) = g.drag {
            let r = Rect::from_two_pos(start, p);
            painter.rect_filled(r, 0.0, PALETTE.accent_soft.gamma_multiply(0.5));
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, PALETTE.accent), StrokeKind::Inside);
        }
    }

    // Nodes.
    let text_size = 12.5 * view.zoom;
    let labels = view.zoom >= 0.45;
    let mut inline = Vec::new();
    for n in &g.graph.nodes {
        let r = view.rect(g.node_rect(n));
        if !r.expand(40.0).intersects(rect) {
            continue;
        }
        let result = g.result(n.id);
        let error = result.and_then(|r| r.get("error")).and_then(Value::as_str);
        let skipped = result.and_then(|r| r.get("skipped")).and_then(Value::as_str);
        let def = g.def(n);
        let missing = def.is_none();
        let radius = 6.0 * view.zoom;
        painter.rect_filled(r, radius, PALETTE.panel);
        let header = Rect::from_min_size(r.min, vec2(r.width(), HEADER * view.zoom));
        let head_fill = match &n.kind {
            NodeKind::Command(_) => PALETTE.accent_soft,
            NodeKind::Lua(_) => PALETTE.surface,
            NodeKind::Builtin(_) => PALETTE.hover,
        };
        painter.rect_filled(header, egui::CornerRadius { nw: radius as u8, ne: radius as u8, sw: 0, se: 0 }, head_fill);
        let outline = if g.selected.contains(&n.id) {
            Stroke::new(2.0, PALETTE.accent)
        } else if error.is_some() || missing {
            Stroke::new(1.5, PALETTE.danger)
        } else if skipped.is_some() {
            Stroke::new(1.0, PALETTE.warning)
        } else {
            Stroke::new(1.0, PALETTE.border_strong)
        };
        painter.rect_stroke(r, radius, outline, StrokeKind::Inside);
        if labels {
            let title = match (&n.name, &n.kind) {
                (name, _) if !name.is_empty() => name.clone(),
                (_, NodeKind::Command(c)) => c.clone(),
                (_, NodeKind::Builtin(b)) => b.clone(),
                (_, NodeKind::Lua(_)) => "lua".into(),
            };
            painter.text(
                header.left_center() + vec2(8.0 * view.zoom, 0.0),
                Align2::LEFT_CENTER,
                title,
                FontId::new(text_size, theme::semibold()),
                PALETTE.text,
            );
        }
        let (ins, outs) = g.ports(n);
        for (i, p) in ins.iter().enumerate() {
            let at = port_pos(&view, r, i, true);
            let wired = g.graph.wire_into(n.id, &p.name).is_some();
            let color = type_color(&p.ty);
            if wired {
                painter.circle_filled(at, PORT_R * view.zoom, color);
            } else {
                painter.circle(at, PORT_R * view.zoom, PALETTE.panel, Stroke::new(1.5, color));
            }
            if labels {
                let label = if p.required { format!("{} *", p.name) } else { p.name.clone() };
                let color = if wired || n.values.contains_key(&p.name) { PALETTE.text } else { PALETTE.text_muted };
                painter.text(
                    at + vec2(9.0 * view.zoom, 0.0),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(text_size * 0.92),
                    color,
                );
            }
        }
        for (i, p) in outs.iter().enumerate() {
            let at = port_pos(&view, r, i, false);
            painter.circle_filled(at, PORT_R * view.zoom, type_color(&p.ty));
            if labels {
                painter.text(
                    at - vec2(9.0 * view.zoom, 0.0),
                    Align2::RIGHT_CENTER,
                    &p.name,
                    FontId::proportional(text_size * 0.92),
                    PALETTE.text_muted,
                );
            }
        }
        if let Some(kind) = inline_kind(n) {
            let rows = ins.len().max(outs.len()) as f32;
            let top = r.min.y + (HEADER + ROW * rows.max(0.0)) * view.zoom + 2.0;
            let field = Rect::from_min_size(
                pos2(r.min.x + 8.0 * view.zoom, top),
                vec2(r.width() - 16.0 * view.zoom, (ROW - 2.0) * view.zoom),
            );
            inline.push((n.id, kind.to_string(), field));
        }
        // The value, error or reason under the node.
        if labels {
            let (text, color) = match (error, skipped, result) {
                (Some(e), _, _) => (e.to_string(), PALETTE.danger),
                (_, Some(s), _) => (s.to_string(), PALETTE.warning),
                (_, _, Some(res)) => {
                    let first = outs.first().and_then(|p| res.get("outputs").and_then(|o| o.get(&p.name)));
                    (first.map(summary).unwrap_or_default(), PALETTE.text_muted)
                }
                _ if missing => (t!("graph-node-missing"), PALETTE.danger),
                _ => (String::new(), PALETTE.text_muted),
            };
            if !text.is_empty() {
                let text: String = text.chars().take(80).collect();
                painter.text(
                    pos2(r.min.x + 2.0, r.max.y + 3.0 * view.zoom),
                    Align2::LEFT_TOP,
                    text,
                    FontId::monospace(text_size * 0.85),
                    color,
                );
            }
        }
    }
    if g.graph.nodes.is_empty() {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            t!("graph-empty-hint"),
            FontId::proportional(14.0),
            PALETTE.text_faint,
        );
    }
    // Inline editors of input nodes (drawn over the canvas, so they get the clicks).
    if view.zoom >= 0.6 {
        for (id, kind, field) in inline {
            inline_editor(ui, state, id, &kind, field);
        }
    }
    search(ui, state, &view);
}

fn inline_editor(ui: &mut Ui, state: &mut AppState, id: u64, kind: &str, field: Rect) {
    let Some(node) = state.graph.graph.node(id) else { return };
    let value = node.values.get("value").cloned().unwrap_or(json!(0));
    let mut changed: Option<Value> = None;
    let mut active = false;
    match kind {
        "boolean" => {
            let mut b = value.as_bool().unwrap_or(false);
            if ui.put(field, egui::Checkbox::without_text(&mut b)).changed() {
                changed = Some(json!(b));
            }
        }
        "text" => {
            let mut s = value.as_str().map(str::to_string).unwrap_or_default();
            let r = ui.put(field, egui::TextEdit::singleline(&mut s));
            active = r.has_focus();
            if r.changed() {
                changed = Some(json!(s));
            }
        }
        _ => {
            let mut x = value.as_f64().unwrap_or(0.0);
            if kind == "slider" {
                let min = node.values.get("min").and_then(Value::as_f64).unwrap_or(0.0);
                let max = node.values.get("max").and_then(Value::as_f64).unwrap_or(10.0);
                let step = node.values.get("step").and_then(Value::as_f64).filter(|s| *s > 0.0).unwrap_or(0.1);
                let r = ui.put(field, egui::Slider::new(&mut x, min..=max.max(min)).step_by(step));
                active = r.dragged() || r.has_focus();
                if r.changed() {
                    changed = Some(json!(x));
                }
            } else {
                let integer = kind == "integer";
                let drag = egui::DragValue::new(&mut x).speed(if integer { 0.2 } else { 0.05 });
                let drag = if integer { drag.fixed_decimals(0) } else { drag };
                let r = ui.put(field, drag);
                active = r.dragged() || r.has_focus();
                if r.changed() {
                    #[allow(clippy::cast_possible_truncation)]
                    let whole = json!(x.round().clamp(-9.0e15, 9.0e15) as i64);
                    changed = Some(if integer { whole } else { json!(x) });
                }
            }
        }
    }
    let g = &mut state.graph;
    if let Some(v) = changed {
        if g.editing != Some(id) {
            g.step();
            g.editing = Some(id);
        }
        g.changed = true;
        g.dirty = true;
        if let Some(n) = g.graph.node_mut(id) {
            n.values.insert("value".into(), v);
        }
    } else if !active && g.editing == Some(id) {
        g.editing = None;
    }
}

/// A short text for a value.
fn summary(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Number(n) => match n.as_f64() {
            Some(x) if x.fract() == 0.0 => format!("{x:.0}"),
            Some(x) => format!("{x:.3}"),
            None => n.to_string(),
        },
        Value::Array(a) if a.len() == 2 && a.iter().all(Value::is_number) => {
            format!("[{}, {}]", summary(&a[0]), summary(&a[1]))
        }
        Value::Array(a) => match a.first() {
            Some(first) => format!("[{}] {}…", a.len(), summary(first)),
            None => "[0]".into(),
        },
        Value::Object(m) => match (m.get("category"), m.get("id")) {
            (Some(c), Some(id)) => format!("{} {}", c.as_str().unwrap_or_default(), id),
            _ => "{…}".into(),
        },
        Value::String(s) => format!("\"{s}\""),
        Value::Bool(b) => b.to_string(),
    }
}

fn interact(ui: &mut Ui, state: &mut AppState, resp: &egui::Response, view: &View, rect: Rect) {
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let shift = ui.input(|i| i.modifiers.shift || i.modifiers.command);
    // Pan with the middle or right button.
    if resp.dragged_by(egui::PointerButton::Middle) || resp.dragged_by(egui::PointerButton::Secondary) {
        state.graph.pan += resp.drag_delta() / view.zoom;
        state.graph.drag = Drag::Pan;
        return;
    }
    if resp.drag_started_by(egui::PointerButton::Primary) {
        let origin = ui.input(|i| i.pointer.press_origin()).or(pointer).unwrap_or(rect.center());
        state.graph.search = None;
        match hit(&state.graph, view, origin) {
            Hit::Output(n, p) => state.graph.drag = Drag::FromOutput(n, p),
            Hit::Input(n, p) => {
                // Pick up the wire that ends here, or start one from the input.
                let g = &mut state.graph;
                match g.graph.wire_into(n, &p).cloned() {
                    Some(w) => {
                        g.step();
                        g.graph.wires.retain(|x| x != &w);
                        g.drag = Drag::FromOutput(w.from.0, w.from.1);
                    }
                    None => g.drag = Drag::FromInput(n, p),
                }
            }
            Hit::Node(n) => {
                let g = &mut state.graph;
                if !g.selected.contains(&n) {
                    if !shift {
                        g.selected.clear();
                    }
                    g.selected.insert(n);
                }
                g.step();
                g.drag = Drag::Nodes;
            }
            Hit::Empty => state.graph.drag = Drag::Box(origin),
        }
    }
    if resp.dragged_by(egui::PointerButton::Primary) && matches!(state.graph.drag, Drag::Nodes) {
        let d = resp.drag_delta() / view.zoom;
        let g = &mut state.graph;
        for n in g.graph.nodes.iter_mut().filter(|n| g.selected.contains(&n.id)) {
            n.pos = [n.pos[0] + d.x, n.pos[1] + d.y];
        }
    }
    if resp.drag_stopped() {
        let drag = std::mem::take(&mut state.graph.drag);
        let at = pointer.unwrap_or(rect.center());
        match drag {
            Drag::FromOutput(n, p) => {
                if let Hit::Input(m, q) = hit(&state.graph, view, at) {
                    connect(state, (n, p), (m, q));
                }
            }
            Drag::FromInput(m, q) => {
                if let Hit::Output(n, p) = hit(&state.graph, view, at) {
                    connect(state, (n, p), (m, q));
                }
            }
            Drag::Box(start) => {
                let r = Rect::from_two_pos(start, at);
                let g = &mut state.graph;
                if !shift {
                    g.selected.clear();
                }
                let inside: Vec<u64> =
                    g.graph.nodes.iter().filter(|n| r.intersects(view.rect(g.node_rect(n)))).map(|n| n.id).collect();
                g.selected.extend(inside);
            }
            Drag::Nodes => {
                // Nodes clicked without moving leave no step behind.
                let g = &mut state.graph;
                if g.undo.last().is_some_and(|last| last.nodes == g.graph.nodes) {
                    g.undo.pop();
                }
                g.changed = g.graph.run_mode == RunMode::Automatic && g.changed;
            }
            Drag::Pan | Drag::None => {}
        }
    }
    if resp.clicked() {
        let at = pointer.unwrap_or(rect.center());
        let g = &mut state.graph;
        g.search = None;
        match hit(g, view, at) {
            Hit::Node(n) | Hit::Output(n, _) | Hit::Input(n, _) => {
                if !shift {
                    g.selected.clear();
                }
                g.selected.insert(n);
                select_elements(state, n);
            }
            Hit::Empty => g.selected.clear(),
        }
    }
    let open_search = resp.double_clicked() || (resp.secondary_clicked() && !matches!(state.graph.drag, Drag::Pan));
    if open_search
        && let Some(p) = pointer
        && matches!(hit(&state.graph, view, p), Hit::Empty)
    {
        state.graph.search = Some(Search { at: view.graph(p), screen: p, query: String::new(), focus: true });
    }
    if !resp.dragged() && matches!(state.graph.drag, Drag::Pan) {
        state.graph.drag = Drag::None;
    }
}

fn connect(state: &mut AppState, from: (u64, String), to: (u64, String)) {
    let g = &mut state.graph;
    let ty = |node: u64, port: &str, input: bool| {
        g.graph.node(node).and_then(|n| {
            let (ins, outs) = g.ports(n);
            let list = if input { ins } else { outs };
            list.into_iter().find(|p| p.name == port).map(|p| p.ty)
        })
    };
    if let (Some(a), Some(b)) = (ty(from.0, &from.1, false), ty(to.0, &to.1, true))
        && !a.fits(&b)
    {
        state.message = t!("graph-wire-type", "from" => a.label(), "to" => b.label());
        return;
    }
    g.step();
    if let Err(e) = g.graph.connect(from, to) {
        g.undo.pop();
        state.message = t!("graph-wire-refused", "error" => e);
    }
}

/// Select in the views the elements a node made.
fn select_elements(state: &mut AppState, node: u64) {
    let uid = state.graph.graph.uid;
    let ids: BTreeSet<cc_core::ElementId> =
        state.project().graph_bindings.iter().filter(|(_, b)| b.of_node(uid, node)).map(|(id, _)| *id).collect();
    if !ids.is_empty() {
        state.selection = ids;
    }
}

fn search(ui: &mut Ui, state: &mut AppState, view: &View) {
    let Some(s) = &mut state.graph.search else { return };
    let Some(catalog) = &state.graph.catalog else { return };
    let words: Vec<String> = s.query.split_whitespace().map(str::to_lowercase).collect();
    let matches: Vec<&Def> = catalog
        .iter()
        .filter(|d| {
            let hay = format!("{} {} {}", d.name, d.group, d.description).to_lowercase();
            words.iter().all(|w| hay.contains(w))
        })
        .take(MAX_MATCHES)
        .collect();
    let mut chosen: Option<NodeKind> = None;
    let mut close = false;
    let area = egui::Area::new(egui::Id::new("graph-search")).fixed_pos(s.screen).order(egui::Order::Foreground).show(
        ui.ctx(),
        |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(340.0);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut s.query)
                        .hint_text(t!("graph-search-hint"))
                        .desired_width(f32::INFINITY),
                );
                r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, t!("graph-search-hint")));
                if s.focus {
                    r.request_focus();
                    s.focus = false;
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    chosen = matches.first().map(|d| d.kind.clone());
                }
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    close = true;
                }
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for d in &matches {
                        let label = format!("{}   {}", d.name, d.group);
                        let r = ui.selectable_label(false, label).on_hover_text(&d.description);
                        if r.clicked() {
                            chosen = Some(d.kind.clone());
                        }
                    }
                });
            });
        },
    );
    let pressed_outside = ui.input(|i| i.pointer.any_pressed())
        && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p));
    if close || pressed_outside {
        state.graph.search = None;
        return;
    }
    if let Some(kind) = chosen {
        let at = s.at;
        let _ = view;
        state.graph.search = None;
        add_node(state, kind, at);
    }
}

fn add_node(state: &mut AppState, kind: NodeKind, at: Pos2) {
    let g = &mut state.graph;
    g.step();
    let id = g.graph.next_id();
    let kind = match kind {
        NodeKind::Lua(_) => NodeKind::Lua("return x".into()),
        k => k,
    };
    g.graph.nodes.push(Node::new(id, kind, [at.x, at.y]));
    g.selected = BTreeSet::from([id]);
}

fn inspector(ui: &mut Ui, state: &mut AppState) {
    let single = (state.graph.selected.len() == 1).then(|| state.graph.selected.iter().next().copied()).flatten();
    match single.and_then(|id| state.graph.graph.node(id).cloned()) {
        Some(node) => node_inspector(ui, state, node),
        None => graph_inspector(ui, state),
    }
}

fn graph_inspector(ui: &mut Ui, state: &mut AppState) {
    ui.label(theme::title(t!("graph-properties")));
    ui.add_space(4.0);
    let mut name = state.graph.graph.name.clone();
    ui.label(theme::caption(t!("graph-name")));
    if ui.text_edit_singleline(&mut name).changed() {
        state.graph.step();
        state.graph.graph.name = name;
    }
    let mut files = state.graph.graph.allow_files;
    if ui.checkbox(&mut files, t!("graph-allow-files")).on_hover_text(t!("graph-allow-files-hint")).changed() {
        state.graph.step();
        state.graph.graph.allow_files = files;
    }
    ui.add_space(6.0);
    let uid = state.graph.graph.uid;
    let count = state.project().graph_bindings.values().filter(|b| b.graph == uid).count();
    ui.label(theme::muted(t!("graph-element-count", "count" => count)));
    ui.horizontal(|ui| {
        if ui.add_enabled(count > 0, egui::Button::new(t!("graph-select-elements"))).clicked() {
            state.selection =
                state.project().graph_bindings.iter().filter(|(_, b)| b.graph == uid).map(|(id, _)| *id).collect();
        }
        if ui
            .add_enabled(count > 0, egui::Button::new(t!("graph-release")))
            .on_hover_text(t!("graph-release-hint"))
            .clicked()
        {
            crate::actions::run_command(
                state,
                cc_engine::commands::graph::GraphRelease { graph: uid.to_string(), elements: None },
            );
        }
    });
    ui.add_space(8.0);
    ui.label(theme::muted(t!("graph-help")));
    if let Some(Ok(r)) = &state.graph.report
        && let Some(lines) = r.get("messages").and_then(Value::as_array)
        && !lines.is_empty()
    {
        ui.add_space(8.0);
        ui.label(theme::caption(t!("graph-messages")));
        for l in lines.iter().filter_map(Value::as_str).take(200) {
            ui.monospace(l);
        }
    }
}

fn node_inspector(ui: &mut Ui, state: &mut AppState, node: Node) {
    let id = node.id;
    let def = state.graph.def(&node);
    let title = match &node.kind {
        NodeKind::Command(c) => c.clone(),
        NodeKind::Builtin(b) => b.clone(),
        NodeKind::Lua(_) => "lua".into(),
    };
    ui.label(theme::title(title));
    if let Some(d) = &def {
        ui.label(theme::muted(format!("{} — {}", d.group, d.description)));
    } else {
        ui.label(egui::RichText::new(t!("graph-node-missing")).color(PALETTE.danger));
    }
    ui.add_space(6.0);
    ui.label(theme::caption(t!("graph-name")));
    let mut name = node.name.clone();
    if ui.text_edit_singleline(&mut name).on_hover_text(t!("graph-name-hint")).changed() {
        set_node(state, id, |n| n.name = name);
    }
    if !matches!(node.kind, NodeKind::Lua(_)) {
        ui.horizontal(|ui| {
            ui.label(theme::caption(t!("graph-lacing")));
            let mut lacing = node.lacing;
            egui::ComboBox::from_id_salt(("lacing", id)).selected_text(lacing_label(lacing)).show_ui(ui, |ui| {
                for l in [Lacing::Shortest, Lacing::Longest, Lacing::Cross] {
                    ui.selectable_value(&mut lacing, l, lacing_label(l));
                }
            });
            if lacing != node.lacing {
                set_node(state, id, |n| n.lacing = lacing);
            }
        });
    }
    if matches!(node.kind, NodeKind::Command(_)) {
        let mut expanded = node.expanded;
        if ui.checkbox(&mut expanded, t!("graph-show-all-inputs")).changed() {
            set_node(state, id, |n| n.expanded = expanded);
        }
    }
    if let NodeKind::Lua(code) = &node.kind {
        lua_fields(ui, state, &node, code);
    }
    if matches!(&node.kind, NodeKind::Builtin(b) if b == "select_elements") {
        let n = node.values.get("uids").and_then(Value::as_array).map_or(0, Vec::len);
        ui.label(theme::muted(t!("graph-element-count", "count" => n)));
        if ui.button(t!("graph-use-selection")).clicked() {
            let uids: Vec<Value> = state
                .selection
                .iter()
                .filter_map(|e| state.project().elements.get(e).map(|el| json!(el.uid.to_string())))
                .collect();
            set_node(state, id, |n| {
                n.values.insert("uids".into(), Value::Array(uids));
            });
        }
    }
    if let Some(d) = &def
        && !d.inputs.is_empty()
    {
        ui.add_space(8.0);
        ui.label(theme::caption(t!("graph-inputs")));
        for port in &d.inputs {
            input_field(ui, state, &node, port);
        }
    }
    // Outputs of the last run.
    if let Some(res) = state.graph.result(id).cloned() {
        ui.add_space(8.0);
        if let Some(e) = res.get("error").and_then(Value::as_str) {
            ui.label(egui::RichText::new(e).color(PALETTE.danger));
        }
        if let Some(s) = res.get("skipped").and_then(Value::as_str) {
            ui.label(egui::RichText::new(s).color(PALETTE.warning));
        }
        if let Some(outputs) = res.get("outputs").and_then(Value::as_object) {
            ui.label(theme::caption(t!("graph-outputs")));
            for (name, v) in outputs {
                ui.label(egui::RichText::new(name).strong());
                let mut text = serde_json::to_string_pretty(v).unwrap_or_default();
                if text.len() > 4000 {
                    let cut = text.char_indices().nth(4000).map_or(text.len(), |(i, _)| i);
                    text.truncate(cut);
                    text.push('…');
                }
                ui.add(egui::Label::new(egui::RichText::new(text).monospace().size(11.0)).wrap());
            }
        }
    }
}

fn lacing_label(l: Lacing) -> String {
    match l {
        Lacing::Shortest => t!("graph-lacing-shortest"),
        Lacing::Longest => t!("graph-lacing-longest"),
        Lacing::Cross => t!("graph-lacing-cross"),
    }
}

fn set_node(state: &mut AppState, id: u64, f: impl FnOnce(&mut Node)) {
    let g = &mut state.graph;
    g.step();
    if let Some(n) = g.graph.node_mut(id) {
        f(n);
    }
}

fn lua_fields(ui: &mut Ui, state: &mut AppState, node: &Node, code: &str) {
    let id = node.id;
    for (label, outputs) in [(t!("graph-inputs"), false), (t!("graph-outputs"), true)] {
        ui.label(theme::caption(label));
        let key = (id, if outputs { "#outputs" } else { "#inputs" }.to_string());
        let current = if outputs { node.outputs.join(", ") } else { node.inputs.join(", ") };
        let buffer = state.graph.buffers.entry(key.clone()).or_insert(current);
        let r = ui.text_edit_singleline(buffer).on_hover_text(t!("graph-lua-names-hint"));
        if r.lost_focus() {
            let names: Vec<String> =
                buffer.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            state.graph.buffers.remove(&key);
            if names.iter().all(|n| cc_engine::graph::eval::is_identifier(n)) {
                let before = if outputs { &node.outputs } else { &node.inputs };
                if &names != before {
                    set_node(state, id, |n| {
                        if outputs {
                            n.outputs = names;
                        } else {
                            n.inputs = names;
                        }
                    });
                    // Wires to ports that are gone go too.
                    let (ins, outs) =
                        state.graph.ports(&state.graph.graph.node(id).cloned().unwrap_or_else(|| node.clone()));
                    state.graph.graph.wires.retain(|w| {
                        (w.to.0 != id || ins.iter().any(|p| p.name == w.to.1))
                            && (w.from.0 != id || outs.iter().any(|p| p.name == w.from.1))
                    });
                }
            } else {
                state.message = t!("graph-lua-bad-name");
            }
        }
    }
    ui.label(theme::caption(t!("graph-code")));
    let key = (id, "#code".to_string());
    let buffer = state.graph.buffers.entry(key.clone()).or_insert_with(|| code.to_string());
    let r = ui.add(
        egui::TextEdit::multiline(buffer).code_editor().desired_rows(8).desired_width(f32::INFINITY).lock_focus(true),
    );
    if r.lost_focus() {
        let text = buffer.clone();
        state.graph.buffers.remove(&key);
        if text != code {
            set_node(state, id, |n| n.kind = NodeKind::Lua(text));
        }
    }
}

fn input_field(ui: &mut Ui, state: &mut AppState, node: &Node, port: &Port) {
    let id = node.id;
    let wired = state.graph.graph.wire_into(id, &port.name).is_some();
    ui.horizontal(|ui| {
        let label = if port.required { format!("{} *", port.name) } else { port.name.clone() };
        ui.label(egui::RichText::new(label).color(type_color(&port.ty))).on_hover_text(format!(
            "{}\n{}",
            port.ty.label(),
            port.description
        ));
        if wired {
            ui.label(theme::muted(t!("graph-wired")));
        }
    });
    if wired {
        return;
    }
    let key = (id, port.name.clone());
    let current = node.values.get(&port.name).map(|v| v.to_string()).unwrap_or_default();
    let hint = port.default.as_ref().map(Value::to_string).unwrap_or_default();
    let buffer = state.graph.buffers.entry(key.clone()).or_insert(current.clone());
    let r = ui.add(
        egui::TextEdit::singleline(buffer)
            .hint_text(hint)
            .desired_width(f32::INFINITY)
            .font(egui::TextStyle::Monospace),
    );
    if r.lost_focus() {
        let text = buffer.trim().to_string();
        state.graph.buffers.remove(&key);
        if text == current {
            return;
        }
        // JSON, else a text.
        let value = if text.is_empty() {
            None
        } else {
            Some(serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text)))
        };
        set_node(state, id, |n| match value {
            Some(v) => {
                n.values.insert(port.name.clone(), v);
            }
            None => {
                n.values.remove(&port.name);
            }
        });
    }
}

/// Open the node search at a point of the graph (`screen` on the canvas).
#[cfg(test)]
pub(crate) fn open_search(state: &mut AppState, at: Pos2, screen: Pos2) {
    state.graph.search = Some(Search { at, screen, query: String::new(), focus: true });
}
