//! Curtain walls in the app: the Curtain Grid tool (add and remove grid
//! lines, merge cells across a segment, swap panels by clicking a curtain
//! wall in a plan, elevation or section), its options bar, the grid rows of
//! the properties palette and the curtain sections of the Type Properties
//! dialog. Every change runs an engine command (`curtain_grid_set`,
//! `curtain_panel_set`, `type_set`).

use crate::i18n::tr;
use crate::paint::Xf;
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use crate::theme::{self, PALETTE};
use cc_core::curtain::{CurtainPanel, CurtainSystem, GridAxis, GridJustification, GridLayout, LINE_TOL, Mullion};
use cc_core::{Category, Element, ElementId, ElementKind, ElementType, ElementTypeId, TypeData, View};
use cc_engine::Ref;
use cc_engine::commands::curtain::{Axis, CurtainGridSet, CurtainPanelSet, PanelArg, PanelKind};
use cc_geom::WallGeom;
use cc_geom::curtain::CurtainGeom;
use cc_views::format;
use egui::{DragValue, Pos2, Stroke, Ui};
use glam::DVec2;

/// What a click of the Curtain Grid tool does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GridMode {
    /// Put in a grid line (vertical or horizontal, see [`CurtainOptions::axis`]).
    #[default]
    AddLine,
    /// Take out the line nearest the click.
    RemoveLine,
    /// Merge the cells on both sides of the clicked segment, or split them again.
    Segment,
    /// Give the clicked panel [`CurtainOptions::panel`].
    Panel,
}

impl GridMode {
    pub const ALL: [GridMode; 4] = [GridMode::AddLine, GridMode::RemoveLine, GridMode::Segment, GridMode::Panel];

    pub fn key(self) -> &'static str {
        match self {
            GridMode::AddLine => "curtain-mode-add",
            GridMode::RemoveLine => "curtain-mode-remove",
            GridMode::Segment => "curtain-mode-segment",
            GridMode::Panel => "curtain-mode-panel",
        }
    }
}

/// The panel the tool puts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelChoice {
    /// The type's panel again.
    #[default]
    Type,
    Glass,
    Solid,
    Empty,
    Component(ElementTypeId),
}

/// Options of the Curtain Grid tool.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurtainOptions {
    pub mode: GridMode,
    /// Direction of new lines (plans only add vertical ones).
    pub axis: GridAxis,
    pub panel: PanelChoice,
}

impl Default for CurtainOptions {
    fn default() -> Self {
        CurtainOptions { mode: GridMode::default(), axis: GridAxis::Vertical, panel: PanelChoice::default() }
    }
}

/// A point on a curtain wall: its distance along the wall (`u`, as the
/// layout measures it) and its elevation.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Hit {
    wall: ElementId,
    u: f64,
    z: f64,
}

/// How a view sees walls: plans from above at their cut height, elevations
/// and sections through their projection.
#[derive(Debug, Clone, Copy)]
enum Look {
    Plan { z: f64 },
    Projected { origin: DVec2, right: DVec2 },
}

impl Look {
    fn of(state: &AppState, view: &View) -> Option<Look> {
        if view.is_plan() {
            let cut = match view.kind {
                cc_core::ViewKind::FloorPlan { cut_height, .. } | cc_core::ViewKind::CeilingPlan { cut_height, .. } => {
                    cut_height
                }
                _ => 1.2,
            };
            let level = view.level().and_then(|l| state.project().level(l)).map_or(0.0, |l| l.elevation);
            return Some(Look::Plan { z: level + cut });
        }
        view.projection().map(|p| Look::Projected { origin: p.origin, right: p.right })
    }

    /// Where the point of wall `g` at `(u, z)` falls in the drawing.
    fn to_drawing(self, g: &WallGeom, u: f64, z: f64, v: f64) -> DVec2 {
        let p = g.world(u, v);
        match self {
            Look::Plan { .. } => p,
            Look::Projected { origin, right } => DVec2::new((p - origin).dot(right), z),
        }
    }
}

/// The curtain walls of the view's geometry, with their grids.
fn curtains(state: &AppState, view: &View) -> Vec<(WallGeom, CurtainGeom)> {
    state.session.geom_for(view.id).walls.values().filter_map(|g| g.curtain.clone().map(|c| (g.clone(), c))).collect()
}

/// The curtain wall under drawing point `p`, within `tol` (drawing units).
fn pick(look: Look, walls: &[(WallGeom, CurtainGeom)], p: DVec2, tol: f64) -> Option<Hit> {
    let mut best: Option<(f64, Hit)> = None;
    for (g, c) in walls {
        let (Some(&u0), Some(&u1)) = (c.layout.u.first(), c.layout.u.last()) else { continue };
        let (Some(&z0), Some(&z1)) = (c.layout.z.first(), c.layout.z.last()) else { continue };
        let found = match look {
            Look::Plan { z } => {
                let (u, v) = g.local(p);
                let reach = g.half.max(c.thickness * 0.5) + tol;
                (z >= z0 && z <= z1 && v.abs() <= reach && u >= u0 - tol && u <= u1 + tol)
                    .then(|| (v.abs(), Hit { wall: g.element, u: u.clamp(u0, u1), z }))
            }
            Look::Projected { right, .. } => {
                if p.y < z0 - tol || p.y > z1 + tol {
                    continue;
                }
                let look_dir = right.perp();
                // Where along the wall the drawing's x falls: sampled, then bisected (arcs may cross it twice).
                let x = |u: f64| look.to_drawing(g, u, 0.0, 0.0).x - p.x;
                const STEPS: usize = 64;
                let mut nearest: Option<(f64, f64)> = None;
                for i in 0..STEPS {
                    let (mut a, mut b) =
                        (u0 + (u1 - u0) * i as f64 / STEPS as f64, u0 + (u1 - u0) * (i + 1) as f64 / STEPS as f64);
                    let (fa, fb) = (x(a), x(b));
                    if !(fa.is_finite() && fb.is_finite()) || fa * fb > 0.0 {
                        continue;
                    }
                    for _ in 0..40 {
                        let m = (a + b) * 0.5;
                        if x(a) * x(m) <= 0.0 { b = m } else { a = m }
                    }
                    let u = (a + b) * 0.5;
                    let depth = g.world(u, 0.0).dot(look_dir);
                    if nearest.is_none_or(|(d, _)| depth < d) {
                        nearest = Some((depth, u));
                    }
                }
                nearest.map(|(depth, u)| (depth, Hit { wall: g.element, u, z: p.y.clamp(z0, z1) }))
            }
        };
        if let Some((d, hit)) = found
            && best.as_ref().is_none_or(|(b, _)| d < *b)
        {
            best = Some((d, hit));
        }
    }
    best.map(|(_, h)| h)
}

/// The interior line nearest a hit, within `tol` in the drawing: its axis,
/// index and position (`u`, or the elevation).
fn nearest_line(look: Look, g: &WallGeom, c: &CurtainGeom, hit: Hit, tol: f64) -> Option<(GridAxis, usize, f64)> {
    let at = look.to_drawing(g, hit.u, hit.z, 0.0);
    let mut best: Option<(f64, (GridAxis, usize, f64))> = None;
    let mut consider = |d: f64, line: (GridAxis, usize, f64)| {
        if d <= tol && best.as_ref().is_none_or(|(b, _)| d < *b) {
            best = Some((d, line));
        }
    };
    for (i, &u) in c.layout.interior(GridAxis::Vertical).iter().enumerate() {
        consider(look.to_drawing(g, u, hit.z, 0.0).distance(at), (GridAxis::Vertical, i, u));
    }
    if matches!(look, Look::Projected { .. }) {
        for (j, &z) in c.layout.interior(GridAxis::Horizontal).iter().enumerate() {
            consider((z - hit.z).abs(), (GridAxis::Horizontal, j, z));
        }
    }
    best.map(|(_, l)| l)
}

/// The piece of a line between the lines across it that holds the hit.
fn segment_of(c: &CurtainGeom, axis: GridAxis, hit: Hit) -> Option<(usize, (f64, f64))> {
    let across = match axis {
        GridAxis::Vertical => &c.layout.z,
        GridAxis::Horizontal => &c.layout.u,
    };
    let v = match axis {
        GridAxis::Vertical => hit.z,
        GridAxis::Horizontal => hit.u,
    };
    let k = across.windows(2).position(|w| w[0] <= v && v <= w[1])?;
    Some((k, (across[k], across[k + 1])))
}

/// Whether the segment of line `at` over `piece` is removed on wall `w`.
fn segment_removed(w: &cc_core::Wall, axis: GridAxis, at: f64, piece: (f64, f64), base: f64) -> bool {
    // Stored as the line's position (heights above the base) and the piece's middle.
    let (at, piece) = match axis {
        GridAxis::Vertical => (at, (piece.0 - base, piece.1 - base)),
        GridAxis::Horizontal => (at - base, piece),
    };
    w.curtain
        .removed
        .iter()
        .any(|g| g.axis == axis && (g.at - at).abs() < LINE_TOL * 2.0 && g.along > piece.0 && g.along < piece.1)
}

fn axis_arg(axis: GridAxis) -> Axis {
    match axis {
        GridAxis::Vertical => Axis::Vertical,
        GridAxis::Horizontal => Axis::Horizontal,
    }
}

fn grid_command(element: ElementId, axis: GridAxis) -> CurtainGridSet {
    CurtainGridSet {
        element: element.0,
        axis: axis_arg(axis),
        reset: false,
        lines: None,
        move_lines: Vec::new(),
        remove: Vec::new(),
        add: Vec::new(),
        remove_segments: Vec::new(),
        restore_segments: Vec::new(),
    }
}

fn panel_arg(choice: PanelChoice) -> Option<PanelArg> {
    let arg = |kind| PanelArg { kind, thickness: None, material: None, component: None };
    match choice {
        PanelChoice::Type => None,
        PanelChoice::Glass => Some(arg(PanelKind::Glass)),
        PanelChoice::Solid => Some(arg(PanelKind::Solid)),
        PanelChoice::Empty => Some(arg(PanelKind::Empty)),
        PanelChoice::Component(id) => Some(PanelArg { component: Some(Ref::Id(id.0)), ..arg(PanelKind::Component) }),
    }
}

/// What a click at `hit` would do, with the drawing lines that show it.
enum Action {
    Grid(CurtainGridSet),
    Panel(CurtainPanelSet),
}

fn plan_action(
    state: &AppState,
    look: Look,
    g: &WallGeom,
    c: &CurtainGeom,
    hit: Hit,
    tol: f64,
) -> Option<(Action, Vec<[DVec2; 2]>)> {
    let opts = state.curtain;
    let plan = matches!(look, Look::Plan { .. });
    let (z0, z1) = (c.layout.z.first().copied()?, c.layout.z.last().copied()?);
    let (u0, u1) = (c.layout.u.first().copied()?, c.layout.u.last().copied()?);
    let half = g.half.max(c.thickness * 0.5);
    // A vertical line at `u` and a horizontal one at `z`, as drawn in this view.
    let vertical = |u: f64, za: f64, zb: f64| match look {
        Look::Plan { .. } => [g.world(u, -half), g.world(u, half)],
        Look::Projected { .. } => [look.to_drawing(g, u, za, 0.0), look.to_drawing(g, u, zb, 0.0)],
    };
    let horizontal = |z: f64, ua: f64, ub: f64| -> Vec<[DVec2; 2]> {
        let n = if g.arc.is_some() { 24 } else { 1 };
        (0..n)
            .map(|i| {
                let a = ua + (ub - ua) * i as f64 / n as f64;
                let b = ua + (ub - ua) * (i + 1) as f64 / n as f64;
                [look.to_drawing(g, a, z, 0.0), look.to_drawing(g, b, z, 0.0)]
            })
            .collect()
    };
    let base = g.base_z;
    match opts.mode {
        GridMode::AddLine => {
            let axis = if plan { GridAxis::Vertical } else { opts.axis };
            let (position, lines) = match axis {
                GridAxis::Vertical => (hit.u, vec![vertical(hit.u, z0, z1)]),
                GridAxis::Horizontal => (hit.z - base, horizontal(hit.z, u0, u1)),
            };
            let (lo, hi) = match axis {
                GridAxis::Vertical => (u0, u1),
                GridAxis::Horizontal => (0.0, z1 - base),
            };
            if position <= lo + LINE_TOL || position >= hi - LINE_TOL {
                return None;
            }
            let mut command = grid_command(hit.wall, axis);
            command.add = vec![position];
            Some((Action::Grid(command), lines))
        }
        GridMode::RemoveLine => {
            let (axis, i, at) = nearest_line(look, g, c, hit, tol)?;
            let lines = match axis {
                GridAxis::Vertical => vec![vertical(at, z0, z1)],
                GridAxis::Horizontal => horizontal(at, u0, u1),
            };
            let mut command = grid_command(hit.wall, axis);
            command.remove = vec![i];
            Some((Action::Grid(command), lines))
        }
        GridMode::Segment => {
            if plan {
                return None;
            }
            let (axis, i, at) = nearest_line(look, g, c, hit, tol)?;
            let (k, piece) = segment_of(c, axis, hit)?;
            let lines = match axis {
                GridAxis::Vertical => vec![vertical(at, piece.0, piece.1)],
                GridAxis::Horizontal => horizontal(at, piece.0, piece.1),
            };
            let Some(ElementKind::Wall(w)) = state.project().element(hit.wall).map(|e| &e.kind) else { return None };
            let mut command = grid_command(hit.wall, axis);
            if segment_removed(w, axis, at, piece, base) {
                command.restore_segments = vec![[i, k]];
            } else {
                command.remove_segments = vec![[i, k]];
            }
            Some((Action::Grid(command), lines))
        }
        GridMode::Panel => {
            let column = c.layout.u.windows(2).position(|w| w[0] <= hit.u && hit.u <= w[1])?;
            let row = c.layout.z.windows(2).position(|w| w[0] <= hit.z && hit.z <= w[1])?;
            let cell = c.layout.panel_at(column, row)?;
            let (ua, ub) = cell.u;
            let (za, zb) = if plan { (hit.z, hit.z) } else { cell.z };
            let mut lines = if plan {
                vec![[g.world(ua, 0.0), g.world(ub, 0.0)]]
            } else {
                let mut l = horizontal(za, ua, ub);
                l.extend(horizontal(zb, ua, ub));
                l.push(vertical(ua, za, zb));
                l.push(vertical(ub, za, zb));
                l
            };
            if plan {
                lines.push(vertical(ua, za, zb));
                lines.push(vertical(ub, za, zb));
            }
            let command =
                CurtainPanelSet { element: hit.wall.0, cells: vec![[column, row]], panel: panel_arg(opts.panel) };
            Some((Action::Panel(command), lines))
        }
    }
}

/// What the tool would do at drawing point `raw` (within `tol` drawing
/// units): the lines that show it, and the command, run when `click`.
/// `None` when no curtain wall is there or the mode does nothing there.
fn act(state: &mut AppState, view: &View, raw: DVec2, tol: f64, click: bool) -> Option<Vec<[DVec2; 2]>> {
    let look = Look::of(state, view)?;
    let walls = curtains(state, view);
    let Some(hit) = pick(look, &walls, raw, tol) else {
        if click {
            state.message = t!("hint-curtain-grid");
        }
        return None;
    };
    let (g, c) = walls.iter().find(|(g, _)| g.element == hit.wall)?;
    let Some((action, lines)) = plan_action(state, look, g, c, hit, tol) else {
        if click && matches!(look, Look::Plan { .. }) && matches!(state.curtain.mode, GridMode::Segment) {
            state.message = t!("msg-curtain-elevation");
        }
        return None;
    };
    if click {
        let done = match action {
            Action::Grid(command) => crate::actions::run_command(state, command),
            Action::Panel(command) => crate::actions::run_command(state, command),
        };
        if done.is_some() {
            state.selection = [hit.wall].into_iter().collect();
        }
    }
    Some(lines)
}

/// The Curtain Grid tool: hovering a curtain wall shows what a click does
/// (the line to add or remove, the segment, the panel); clicking does it.
pub fn grid_tool(resp: &egui::Response, painter: &egui::Painter, state: &mut AppState, view: &View, xf: &Xf) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let tol = 8.0 / xf.scale.max(1e-9);
    let removing = matches!(state.curtain.mode, GridMode::RemoveLine | GridMode::Segment);
    let Some(lines) = act(state, view, raw, tol, resp.clicked()) else { return };
    for [a, b] in &lines {
        let (a, b): (Pos2, Pos2) = (xf.to_screen(*a), xf.to_screen(*b));
        if removing {
            painter.line_segment([a, b], Stroke::new(2.5, PALETTE.danger));
        } else {
            painter.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(2.0, PALETTE.accent), 6.0, 3.0));
        }
    }
}

/// The panels a curtain panel may become: the kinds, then the component
/// types (doors, windows and other components).
fn panel_choices(state: &AppState) -> Vec<(PanelChoice, String)> {
    let mut out = vec![
        (PanelChoice::Type, t!("curtain-panel-type")),
        (PanelChoice::Glass, t!("panel-glass")),
        (PanelChoice::Solid, t!("panel-solid")),
        (PanelChoice::Empty, t!("panel-empty")),
    ];
    for category in [Category::Door, Category::Window, Category::Component] {
        for ty in state.project().types_of(category) {
            if matches!(ty.data, TypeData::Component { .. }) {
                out.push((PanelChoice::Component(ty.id), ty.name.clone()));
            }
        }
    }
    out
}

/// Whether the project has a curtain wall type.
pub fn has_curtain_type(state: &AppState) -> bool {
    state.project().types_of(Category::Wall).any(|t| t.curtain().is_some())
}

/// The wall tool's list entry that makes a curtain type.
pub fn new_type_label() -> String {
    format!("+ {}", t!("tpl-type-curtain"))
}

/// The project's first curtain wall type, made (glass between steel
/// mullions) when it has none.
pub fn new_curtain_type(state: &mut AppState) -> Option<ElementTypeId> {
    if let Some(t) = state.project().types_of(Category::Wall).find(|t| t.curtain().is_some()) {
        return Some(t.id);
    }
    let command = cc_engine::commands::curtain::CurtainTypeCreate { name: t!("tpl-type-curtain"), curtain: None };
    let out = crate::actions::run_command(state, command)?;
    out["id"].as_u64().map(ElementTypeId)
}

/// The options bar of the Curtain Grid tool.
pub fn options_bar(ui: &mut Ui, state: &mut AppState) {
    for mode in GridMode::ALL {
        ui.selectable_value(&mut state.curtain.mode, mode, tr(mode.key()));
    }
    ui.separator();
    match state.curtain.mode {
        GridMode::AddLine => {
            ui.selectable_value(&mut state.curtain.axis, GridAxis::Vertical, t!("curtain-axis-vertical"));
            ui.selectable_value(&mut state.curtain.axis, GridAxis::Horizontal, t!("curtain-axis-horizontal"));
            ui.separator();
        }
        GridMode::Panel => {
            ui.label(t!("prop-panel"));
            let choices = panel_choices(state);
            let current = choices.iter().find(|c| c.0 == state.curtain.panel).map(|c| c.1.clone()).unwrap_or_default();
            egui::ComboBox::from_id_salt("curtain_panel").selected_text(current).width(170.0).show_ui(ui, |ui| {
                for (choice, name) in choices {
                    ui.selectable_value(&mut state.curtain.panel, choice, name);
                }
            });
            ui.separator();
        }
        _ => {}
    }
    ui.label(theme::muted(t!("hint-curtain-grid")));
}

// ------------------------------------------------------------ properties palette

/// The grid of a selected curtain wall: its columns and rows, panel count,
/// glazed area and mullion length, and resetting a direction to the type's lines.
pub fn palette_rows(ui: &mut Ui, state: &mut AppState, el: &Element, w: &cc_core::Wall) {
    let Some(c) = state.session.geom().walls.get(&el.id).and_then(|g| g.curtain.clone()) else { return };
    let id = el.id;
    section(ui, "curtain-grid", t!("section-curtain-grid"), |ui| {
        let l = &c.layout;
        readonly_row(ui, &t!("prop-curtain-cells"), format!("{} × {}", l.columns(), l.rows()));
        readonly_row(ui, &t!("col-panel-count"), l.panel_count().to_string());
        readonly_row(ui, &t!("col-glazed-area"), format::area(l.glazed_area()));
        readonly_row(ui, &t!("col-mullion-length"), format::length(l.mullion_length()));
        let swapped = w.curtain.panels.len();
        if swapped > 0 {
            readonly_row(ui, &t!("prop-curtain-swapped"), swapped.to_string());
        }
        for (axis, own, label) in [
            (GridAxis::Vertical, w.curtain.vertical.is_some(), t!("prop-curtain-vertical")),
            (GridAxis::Horizontal, w.curtain.horizontal.is_some(), t!("prop-curtain-horizontal")),
        ] {
            let removed = w.curtain.removed.iter().any(|g| g.axis == axis);
            let value = if own || removed { t!("curtain-lines-own") } else { t!("curtain-lines-type") };
            let reset = row(ui, &label, |ui| {
                ui.horizontal(|ui| {
                    ui.label(value);
                    ui.add_enabled(own || removed, egui::Button::new(t!("curtain-reset")).small()).clicked()
                })
                .inner
            });
            if reset {
                let mut command = grid_command(id, axis);
                command.reset = true;
                crate::actions::run_command(state, command);
            }
        }
        if swapped > 0 {
            let reset = row(ui, &t!("prop-panel"), |ui| ui.button(t!("curtain-panels-reset")).clicked());
            if reset {
                let cells = (0..l.columns()).flat_map(|i| (0..l.rows()).map(move |j| [i, j])).collect();
                crate::actions::run_command(state, CurtainPanelSet { element: id.0, cells, panel: None });
            }
        }
    });
}

// ------------------------------------------------------------ type dialog

fn finish(state: &mut AppState, r: &egui::Response) {
    if r.drag_stopped() || r.lost_focus() {
        state.session.doc.break_merge();
    }
}

/// Change the curtain system of type `tid`, merging edits of the same field.
fn update(state: &mut AppState, tid: ElementTypeId, field: &str, f: impl FnOnce(&mut CurtainSystem)) {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (tid, "curtain", field).hash(&mut h);
    crate::engine_edit::element_type(state, tid, Some(h.finish()), |t| {
        if let TypeData::Wall { curtain: Some(c), .. } = &mut t.data {
            f(c);
        }
    });
}

fn grid_rows(
    ui: &mut Ui,
    state: &mut AppState,
    tid: ElementTypeId,
    axis: GridAxis,
    grid: cc_core::curtain::CurtainGrid,
) {
    let name = match axis {
        GridAxis::Vertical => "v",
        GridAxis::Horizontal => "h",
    };
    fn grid_of(c: &mut CurtainSystem, axis: GridAxis) -> &mut cc_core::curtain::CurtainGrid {
        match axis {
            GridAxis::Vertical => &mut c.vertical,
            GridAxis::Horizontal => &mut c.horizontal,
        }
    }
    let mut layout = grid.layout;
    row(ui, &t!("prop-grid-layout"), |ui| {
        egui::ComboBox::from_id_salt(("grid-layout", name)).selected_text(tr(layout.key())).show_ui(ui, |ui| {
            for option in GridLayout::ALL {
                ui.selectable_value(&mut layout, option, tr(option.key()));
            }
        })
    });
    if layout != grid.layout {
        update(state, tid, &format!("{name}-layout"), |c| grid_of(c, axis).layout = layout);
        state.session.doc.break_merge();
    }
    match grid.layout {
        GridLayout::None => {}
        GridLayout::FixedNumber => {
            let mut count = grid.count;
            let r = row(ui, &t!("prop-grid-count"), |ui| ui.add(DragValue::new(&mut count).range(1..=200)));
            if r.changed() {
                update(state, tid, &format!("{name}-count"), |c| grid_of(c, axis).count = count);
            }
            finish(state, &r);
        }
        GridLayout::FixedDistance | GridLayout::MaxSpacing => {
            let mut spacing = grid.spacing;
            let r = row(ui, &t!("prop-grid-spacing"), |ui| {
                ui.add(crate::properties::length_drag(&mut spacing).speed(0.01).range(0.1..=50.0))
            });
            if r.changed() {
                update(state, tid, &format!("{name}-spacing"), |c| grid_of(c, axis).spacing = spacing);
            }
            finish(state, &r);
        }
    }
    if grid.layout == GridLayout::FixedDistance {
        let mut justification = grid.justification;
        row(ui, &t!("prop-grid-justify"), |ui| {
            egui::ComboBox::from_id_salt(("grid-justify", name)).selected_text(tr(justification.key())).show_ui(
                ui,
                |ui| {
                    for option in GridJustification::ALL {
                        ui.selectable_value(&mut justification, option, tr(option.key()));
                    }
                },
            )
        });
        if justification != grid.justification {
            update(state, tid, &format!("{name}-justify"), |c| grid_of(c, axis).justification = justification);
            state.session.doc.break_merge();
        }
    }
}

/// Which mullions of a curtain system.
#[derive(Clone, Copy)]
enum Which {
    Vertical,
    Horizontal,
    Border,
}

fn mullion_of(c: &mut CurtainSystem, which: Which) -> &mut Option<Mullion> {
    match which {
        Which::Vertical => &mut c.vertical_mullion,
        Which::Horizontal => &mut c.horizontal_mullion,
        Which::Border => &mut c.border_mullion,
    }
}

fn mullion_rows(
    ui: &mut Ui,
    state: &mut AppState,
    tid: ElementTypeId,
    (which, title): (Which, &str),
    mullion: Option<Mullion>,
) {
    let name = match which {
        Which::Vertical => "mv",
        Which::Horizontal => "mh",
        Which::Border => "mb",
    };
    let mut on = mullion.is_some();
    let label = t!("prop-mullions-on");
    // Named with its section, so each of the three switches can be told apart.
    let switch = format!("{title}: {label}");
    let r = row(ui, &label, |ui| {
        let r = ui.checkbox(&mut on, "");
        r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, &switch));
        r
    });
    if r.changed() {
        let frame = mullion.clone();
        update(state, tid, &format!("{name}-on"), |c| {
            let fallback = c.vertical_mullion.clone().or(c.border_mullion.clone()).or(c.horizontal_mullion.clone());
            *mullion_of(c, which) = if on { frame.or(fallback).or(Some(Mullion::default())) } else { None };
        });
        state.session.doc.break_merge();
    }
    let Some(m) = mullion else { return };
    for (label, field, value) in [(t!("col-width"), "w", m.width), (t!("prop-depth"), "d", m.depth)] {
        let mut v = value;
        let r = row(ui, &label, |ui| ui.add(crate::properties::length_drag(&mut v).speed(0.002).range(0.005..=2.0)));
        if r.changed() {
            update(state, tid, &format!("{name}-{field}"), |c| {
                if let Some(m) = mullion_of(c, which) {
                    if field == "w" { m.width = v } else { m.depth = v }
                }
            });
        }
        finish(state, &r);
    }
    let mut material = m.material.clone();
    if row(ui, &t!("prop-material"), |ui| {
        crate::dialogs::material_combo(ui, state, ("mullion-mat", name), &mut material)
    }) {
        update(state, tid, &format!("{name}-mat"), |c| {
            if let Some(m) = mullion_of(c, which) {
                m.material = material;
            }
        });
        state.session.doc.break_merge();
    }
}

fn panel_rows(ui: &mut Ui, state: &mut AppState, tid: ElementTypeId, panel: &CurtainPanel) {
    // Kinds and component types; the type's own panel is no choice here.
    let choices: Vec<_> = panel_choices(state).into_iter().filter(|c| c.0 != PanelChoice::Type).collect();
    let current = match panel {
        CurtainPanel::Glass { .. } => PanelChoice::Glass,
        CurtainPanel::Solid { .. } => PanelChoice::Solid,
        CurtainPanel::Empty => PanelChoice::Empty,
        CurtainPanel::Component { component } => PanelChoice::Component(*component),
    };
    let mut choice = current;
    let text = choices.iter().find(|c| c.0 == current).map(|c| c.1.clone()).unwrap_or_default();
    row(ui, &t!("prop-panel"), |ui| {
        egui::ComboBox::from_id_salt("curtain-type-panel").selected_text(text).show_ui(ui, |ui| {
            for (option, name) in choices {
                ui.selectable_value(&mut choice, option, name);
            }
        })
    });
    if choice != current {
        let (thickness, material) = (panel.thickness(), panel.material().map(str::to_string));
        update(state, tid, "panel", |c| {
            let thickness = Some(thickness).filter(|t| *t > 0.0).unwrap_or(0.024);
            let material = material.unwrap_or_default();
            c.panel = match choice {
                PanelChoice::Glass | PanelChoice::Type => CurtainPanel::Glass { thickness, material },
                PanelChoice::Solid => CurtainPanel::Solid { thickness, material },
                PanelChoice::Empty => CurtainPanel::Empty,
                PanelChoice::Component(component) => CurtainPanel::Component { component },
            };
        });
        state.session.doc.break_merge();
    }
    if let CurtainPanel::Glass { thickness, material } | CurtainPanel::Solid { thickness, material } = panel {
        let mut v = *thickness;
        let r = row(ui, &t!("prop-thickness"), |ui| {
            ui.add(crate::properties::length_drag(&mut v).speed(0.001).range(0.001..=2.0))
        });
        if r.changed() {
            update(state, tid, "panel-thk", |c| {
                if let CurtainPanel::Glass { thickness, .. } | CurtainPanel::Solid { thickness, .. } = &mut c.panel {
                    *thickness = v;
                }
            });
        }
        finish(state, &r);
        let mut m = material.clone();
        if row(ui, &t!("prop-material"), |ui| crate::dialogs::material_combo(ui, state, "panel-mat", &mut m)) {
            update(state, tid, "panel-mat", |c| {
                if let CurtainPanel::Glass { material, .. } | CurtainPanel::Solid { material, .. } = &mut c.panel {
                    *material = m;
                }
            });
            state.session.doc.break_merge();
        }
    }
}

/// The curtain sections of the Type Properties dialog: both grids, the
/// panel and the three kinds of mullions.
pub fn type_sections(ui: &mut Ui, state: &mut AppState, ty: &ElementType) {
    let Some(system) = ty.curtain().cloned() else { return };
    let tid = ty.id;
    section(ui, "curtain-vertical", t!("section-curtain-vertical"), |ui| {
        grid_rows(ui, state, tid, GridAxis::Vertical, system.vertical)
    });
    section(ui, "curtain-horizontal", t!("section-curtain-horizontal"), |ui| {
        grid_rows(ui, state, tid, GridAxis::Horizontal, system.horizontal)
    });
    section(ui, "curtain-panel", t!("section-curtain-panel"), |ui| panel_rows(ui, state, tid, &system.panel));
    for (id, title, which, m) in [
        ("curtain-mv", t!("section-mullions-vertical"), Which::Vertical, system.vertical_mullion.clone()),
        ("curtain-mh", t!("section-mullions-horizontal"), Which::Horizontal, system.horizontal_mullion.clone()),
        ("curtain-mb", t!("section-mullions-border"), Which::Border, system.border_mullion.clone()),
    ] {
        let name = title.clone();
        section(ui, id, title, |ui| mullion_rows(ui, state, tid, (which, &name), m));
    }
    section(ui, "curtain-size", t!("section-dimensions"), |ui| {
        readonly_row(ui, &t!("prop-thickness"), format::length(system.thickness()));
    });
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use serde_json::json;

    /// A template project with a 6 m curtain wall of the template's curtain
    /// type along x, and its geometry built.
    fn curtain_state() -> (AppState, ElementId) {
        let mut state = AppState::new(cc_core::Document::new(cc_core::Project::from_template()));
        let ty = new_curtain_type(&mut state).unwrap();
        let out = state
            .session
            .execute("wall_create", json!({"start": [0.0, 0.0], "end": [6.0, 0.0], "type": ty.0, "height": 3.0}))
            .unwrap();
        let id = ElementId(out["id"].as_u64().unwrap());
        state.session.geometry();
        (state, id)
    }

    /// An elevation facing the wall (its right along x).
    fn elevation(state: &AppState) -> View {
        let facing = |v: &&View| {
            matches!(v.kind, cc_core::ViewKind::Elevation { .. })
                && v.projection().is_some_and(|p| p.right.x.abs() > 0.9)
        };
        state.project().views.values().find(facing).cloned().unwrap()
    }

    fn plan(state: &AppState) -> View {
        state.project().views.values().find(|v| matches!(v.kind, cc_core::ViewKind::FloorPlan { .. })).cloned().unwrap()
    }

    fn wall(state: &AppState, id: ElementId) -> cc_core::Wall {
        match &state.project().element(id).unwrap().kind {
            ElementKind::Wall(w) => w.clone(),
            _ => panic!("not a wall"),
        }
    }

    fn layout(state: &AppState, id: ElementId) -> (WallGeom, CurtainGeom) {
        let g = state.session.geom().walls.get(&id).unwrap().clone();
        let c = g.curtain.clone().unwrap();
        (g, c)
    }

    /// The drawing point of the wall at `(u, z)` in `view`.
    fn at(state: &AppState, view: &View, id: ElementId, u: f64, z: f64) -> DVec2 {
        let (g, _) = layout(state, id);
        Look::of(state, view).unwrap().to_drawing(&g, u, z, 0.0)
    }

    fn click(state: &mut AppState, view: &View, p: DVec2) -> bool {
        let done = act(state, view, p, 0.1, true).is_some();
        state.session.geometry();
        done
    }

    #[test]
    fn the_wall_tool_makes_a_curtain_type_once() {
        let mut state = AppState::new(cc_core::Document::new(cc_core::Project::from_template()));
        assert!(!has_curtain_type(&state));
        let id = new_curtain_type(&mut state).unwrap();
        let curtain = state.project().element_type(id).unwrap();
        assert_eq!(curtain.name, t!("tpl-type-curtain"));
        assert!(curtain.curtain().unwrap().panel.is_glazed());
        assert!(curtain.thickness() > 0.0);
        // Its glass and mullions use the template's materials.
        let materials = curtain.data.materials();
        assert!(materials.iter().all(|m| state.project().materials.iter().any(|x| &x.name == m)), "{materials:?}");
        // The second time, the same type.
        assert_eq!(new_curtain_type(&mut state), Some(id));
        assert!(has_curtain_type(&state));
    }

    #[test]
    fn lines_are_added_and_removed_in_elevations_and_plans() {
        let (mut state, id) = curtain_state();
        let view = elevation(&state);
        let base = layout(&state, id).0.base_z;
        let before = layout(&state, id).1.layout.interior(GridAxis::Vertical).len();

        state.curtain.mode = GridMode::AddLine;
        let p = at(&state, &view, id, 2.2, base + 1.0);
        assert!(click(&mut state, &view, p));
        let (_, c) = layout(&state, id);
        assert_eq!(c.layout.interior(GridAxis::Vertical).len(), before + 1);
        assert!(c.layout.interior(GridAxis::Vertical).iter().any(|u| (u - 2.2).abs() < 1e-6));
        assert!(wall(&state, id).curtain.vertical.is_some());
        assert_eq!(state.selection.iter().copied().collect::<Vec<_>>(), vec![id]);

        // A horizontal line at the clicked height.
        state.curtain.axis = GridAxis::Horizontal;
        let rows = c.layout.rows();
        let p = at(&state, &view, id, 2.5, base + 0.7);
        assert!(click(&mut state, &view, p));
        assert_eq!(layout(&state, id).1.layout.rows(), rows + 1);

        // Removing takes the nearest line out.
        state.curtain.mode = GridMode::RemoveLine;
        let p = at(&state, &view, id, 2.21, base + 2.0);
        assert!(click(&mut state, &view, p));
        assert!(!layout(&state, id).1.layout.interior(GridAxis::Vertical).iter().any(|u| (u - 2.2).abs() < 1e-6));

        // In a plan, lines are vertical whatever the axis.
        let plan = plan(&state);
        state.curtain.mode = GridMode::AddLine;
        let p = layout(&state, id).0.world(4.1, 0.0);
        let columns = layout(&state, id).1.layout.columns();
        assert!(click(&mut state, &plan, p));
        assert_eq!(layout(&state, id).1.layout.columns(), columns + 1);

        // Far from any curtain wall nothing happens.
        assert!(!click(&mut state, &plan, DVec2::new(50.0, 50.0)));
        assert_eq!(state.message, t!("hint-curtain-grid"));
    }

    #[test]
    fn segments_merge_and_split_and_panels_swap() {
        let (mut state, id) = curtain_state();
        let view = elevation(&state);
        let (g, c) = layout(&state, id);
        let line = c.layout.interior(GridAxis::Vertical)[0];
        let row_z = (c.layout.z[0] + c.layout.z[1]) * 0.5;
        let panels = c.layout.panel_count();

        state.curtain.mode = GridMode::Segment;
        let p = at(&state, &view, id, line + 0.01, row_z);
        assert!(click(&mut state, &view, p));
        assert_eq!(wall(&state, id).curtain.removed.len(), 1);
        assert_eq!(layout(&state, id).1.layout.panel_count(), panels - 1);
        // The same segment again splits the cells.
        assert!(click(&mut state, &view, p));
        assert!(wall(&state, id).curtain.removed.is_empty());
        assert_eq!(layout(&state, id).1.layout.panel_count(), panels);

        // Segments are not picked in plans.
        let plan = plan(&state);
        assert!(!click(&mut state, &plan, g.world(line, 0.0)));
        assert_eq!(state.message, t!("msg-curtain-elevation"));

        // A solid panel, then the type's again.
        state.curtain.mode = GridMode::Panel;
        state.curtain.panel = PanelChoice::Solid;
        let p = at(&state, &view, id, line * 0.5, row_z);
        assert!(click(&mut state, &view, p));
        let swaps = wall(&state, id).curtain.panels;
        assert_eq!(swaps.len(), 1);
        assert!(matches!(swaps[0].panel, CurtainPanel::Solid { .. }));
        state.curtain.panel = PanelChoice::Type;
        assert!(click(&mut state, &view, p));
        assert!(wall(&state, id).curtain.panels.is_empty());

        // Undo takes back one click at a time.
        state.session.execute("undo", json!({})).unwrap();
        assert_eq!(wall(&state, id).curtain.panels.len(), 1);
    }

    #[test]
    fn panel_choices_list_the_kinds_and_component_types() {
        let (state, _) = curtain_state();
        let choices = panel_choices(&state);
        assert_eq!(choices[0].0, PanelChoice::Type);
        assert!(choices.iter().any(|c| matches!(c.0, PanelChoice::Component(_))), "doors and windows are choices");
        for (choice, _) in &choices {
            if let PanelChoice::Component(id) = choice {
                assert!(matches!(state.project().element_type(*id).unwrap().data, TypeData::Component { .. }));
            }
        }
    }
}
