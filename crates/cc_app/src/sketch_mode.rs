//! Sketch mode: the outline of a floor, ceiling, roof, foundation slab or
//! mass drawn with lines, rectangles, polygons, circles, arcs and ellipses
//! (see [`cc_core::sketch`]), in the active plan. The ribbon shows the
//! drawing tools and *Finish*/*Cancel* while the sketch is open; in its
//! *Select* mode a click picks a curve, whose grips stretch it and whose
//! values the properties palette edits; a window picks the curves inside it
//! (or, dragged to the left, those it touches) and a double click the chain
//! of joined curves. The modify tools move, copy, rotate and mirror the
//! picked curves, and offset, trim, extend and split curves (see
//! [`cc_core::sketch_edit`]). *Finish* creates the element (or draws the
//! edited element's outline again, *Edit Boundary*) through the engine's
//! `curves`; the sketch's own changes undo inside it.

use crate::paint::{HIGHLIGHT, PREVIEW, Xf};
use crate::state::AppState;
use crate::t;
use crate::theme::{self, PALETTE};
use crate::tools::{Tool, draw_snap, snap_at, temp_dimension};
use cc_core::arc::Arc;
use cc_core::sketch::{JOIN_TOL, Sketch, SketchCurve, SketchError};
use cc_core::sketch_edit;
use cc_core::{Category, ElementId, ElementTypeId, LevelId, PlanTransform};
use cc_engine::Ref;
use cc_engine::commands::sketch::CurveArg;
use egui::{Color32, Painter, Response, Stroke, Ui};
use glam::DVec2;
use std::collections::BTreeSet;

/// Color of the sketch's curves.
const SKETCH: Color32 = Color32::from_rgb(200, 40, 160);
/// Most sketch states kept for undo.
const MAX_UNDO: usize = 200;
/// Pick distance on screen, points.
const PICK_PX: f64 = 6.0;

/// What finishing the sketch makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A new element of `category` and type.
    New { category: Category, type_id: ElementTypeId },
    /// A new outline for an element (*Edit Boundary*).
    Edit { id: ElementId },
}

/// How clicks draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Draw {
    /// Pick curves to change them.
    #[default]
    Select,
    /// Two points, chained by default.
    Line,
    /// Two opposite corners.
    Rect,
    /// The center, then a corner.
    Polygon,
    /// The center, then the middle of a side.
    PolygonOut,
    /// The center, then a point on it.
    Circle,
    /// The two ends, then a point on the arc.
    Arc3,
    /// The center, the start, then the end.
    ArcCenter,
    /// From the free end of a curve, along it, to the end.
    ArcTangent,
    /// The center, the end of an axis, then the other axis's half length.
    Ellipse,
    /// Move the picked curves: base point, then target.
    Move,
    /// Copies of the picked curves: base point, then targets.
    Copy,
    /// Rotate the picked curves: center, reference direction, final direction.
    Rotate,
    /// Mirror the picked curves across a line of the sketch.
    MirrorPick,
    /// Mirror the picked curves across an axis drawn with two points.
    MirrorAxis,
    /// A copy of a curve, or of its chain, at a distance.
    Offset,
    /// Two curves trimmed or extended to meet.
    TrimCorner,
    /// Curves trimmed or extended to a reference curve.
    TrimExtend,
    /// A curve split in two.
    Split,
}

impl Draw {
    /// The tools that act on the picked curves.
    pub fn on_selection(self) -> bool {
        matches!(self, Draw::Move | Draw::Copy | Draw::Rotate | Draw::MirrorPick | Draw::MirrorAxis)
    }

    /// The modify tools.
    fn modifies(self) -> bool {
        self.on_selection() || matches!(self, Draw::Offset | Draw::TrimCorner | Draw::TrimExtend | Draw::Split)
    }
}

/// A grip of a selected curve: `which` is an end (0, 1), the middle of an
/// arc (2), a center (3), or a point on a circle or an ellipse's axis (4, 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Grip {
    curve: usize,
    which: u8,
}

/// An open sketch.
#[derive(Debug, Clone)]
pub struct Session {
    pub target: Target,
    pub curves: Vec<SketchCurve>,
    pub draw: Draw,
    /// Points picked for the curve being drawn.
    pub picks: Vec<DVec2>,
    /// Lines continue from the last end.
    pub chain: bool,
    /// Sides of the next polygon.
    pub sides: usize,
    pub selected: BTreeSet<usize>,
    /// The curve picked first by *Trim corner* (with where it was clicked)
    /// or the reference of *Trim/Extend*.
    picked: Option<(usize, DVec2)>,
    /// Where a selection window started.
    boxing: Option<DVec2>,
    undo: Vec<Vec<SketchCurve>>,
    redo: Vec<Vec<SketchCurve>>,
    /// The grip being dragged, and where the curves were when it started.
    drag: Option<(Grip, Vec<SketchCurve>)>,
}

impl Session {
    pub fn new(target: Target, curves: Vec<SketchCurve>) -> Self {
        Self {
            target,
            curves,
            draw: Draw::Line,
            picks: Vec::new(),
            chain: true,
            sides: 6,
            selected: BTreeSet::new(),
            picked: None,
            boxing: None,
            undo: Vec::new(),
            redo: Vec::new(),
            drag: None,
        }
    }

    /// Keep the curves as they are for undo, before a change.
    fn remember(&mut self) {
        self.undo.push(self.curves.clone());
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn add(&mut self, curves: Vec<SketchCurve>) {
        let curves: Vec<SketchCurve> = curves.into_iter().filter(SketchCurve::is_valid).collect();
        if curves.is_empty() {
            return;
        }
        self.remember();
        self.curves.extend(curves);
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.curves, prev));
            self.selected.clear();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.curves, next));
            self.selected.clear();
        }
    }

    /// Remove the selected curves.
    pub fn delete_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.remember();
        let selected = std::mem::take(&mut self.selected);
        let mut i = 0;
        self.curves.retain(|_| {
            let keep = !selected.contains(&i);
            i += 1;
            keep
        });
    }

    /// Change curve `i` (one undo step).
    pub fn set_curve(&mut self, i: usize, curve: SketchCurve) {
        if !curve.is_valid() || self.curves.get(i) == Some(&curve) {
            return;
        }
        self.remember();
        if let Some(c) = self.curves.get_mut(i) {
            *c = curve;
        }
    }

    /// Change some curves and add others, in one undo step.
    fn change(&mut self, changes: &[(usize, SketchCurve)], added: &[SketchCurve]) {
        if changes.iter().map(|(_, c)| c).chain(added).any(|c| !c.is_valid()) {
            return;
        }
        self.remember();
        for &(i, c) in changes {
            if let Some(slot) = self.curves.get_mut(i) {
                *slot = c;
            }
        }
        self.curves.extend_from_slice(added);
    }

    /// The picked curves moved by `t` (one undo step), or copies of them;
    /// returns the copies' indices.
    pub fn transform_selected(&mut self, t: &PlanTransform, copy: bool) -> BTreeSet<usize> {
        let moved: Vec<(usize, SketchCurve)> = self
            .selected
            .iter()
            .filter_map(|&i| {
                let mut c = *self.curves.get(i)?;
                c.transform(t);
                Some((i, c))
            })
            .collect();
        if moved.is_empty() {
            return BTreeSet::new();
        }
        let start = self.curves.len();
        if copy {
            let added: Vec<SketchCurve> = moved.iter().map(|(_, c)| *c).collect();
            self.change(&[], &added);
            (start..self.curves.len()).collect()
        } else {
            self.change(&moved, &[]);
            BTreeSet::new()
        }
    }

    /// Ends, centers and middles to snap to.
    fn snap_points(&self) -> Vec<DVec2> {
        let mut out = Vec::new();
        for c in &self.curves {
            if let Some((a, b)) = c.ends() {
                out.extend([a, b]);
            }
            match *c {
                SketchCurve::Line { a, b } => out.push((a + b) / 2.0),
                SketchCurve::Arc { a, b, bulge } => {
                    if let Some(arc) = Arc::from_bulge(a, b, bulge) {
                        out.extend([arc.center, arc.point(arc.length() / 2.0)]);
                    }
                }
                SketchCurve::Circle { center, .. } | SketchCurve::Ellipse { center, .. } => out.push(center),
            }
        }
        out
    }

    /// The curve nearest `p` within `tol`.
    fn curve_at(&self, p: DVec2, tol: f64) -> Option<usize> {
        self.curves
            .iter()
            .enumerate()
            .map(|(i, c)| (i, distance(c, p)))
            .filter(|(_, d)| *d <= tol)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }
}

/// Distance from `p` to a curve (through its chords).
fn distance(c: &SketchCurve, p: DVec2) -> f64 {
    c.points()
        .windows(2)
        .map(|w| cc_geom::polygon::closest_on_segment(w[0], w[1], p).0.distance(p))
        .fold(f64::INFINITY, f64::min)
}

/// The grips of curve `i`.
fn grips(c: &SketchCurve, curve: usize) -> Vec<(Grip, DVec2)> {
    let g = |which| Grip { curve, which };
    match *c {
        SketchCurve::Line { a, b } => vec![(g(0), a), (g(1), b)],
        SketchCurve::Arc { a, b, bulge } => {
            let mut out = vec![(g(0), a), (g(1), b)];
            if let Some(arc) = Arc::from_bulge(a, b, bulge) {
                out.push((g(2), arc.point(arc.length() / 2.0)));
            }
            out
        }
        SketchCurve::Circle { center, radius } => vec![(g(3), center), (g(4), center + DVec2::X * radius)],
        SketchCurve::Ellipse { center, radii, rotation_deg, .. } => {
            let turn = DVec2::from_angle(rotation_deg.to_radians());
            let mut out = vec![
                (g(3), center),
                (g(4), center + turn.rotate(DVec2::X * radii.x)),
                (g(5), center + turn.rotate(DVec2::Y * radii.y)),
            ];
            if let Some((a, b)) = c.ends() {
                out.extend([(g(0), a), (g(1), b)]);
            }
            out
        }
    }
}

/// Curve `c` with its grip `which` moved to `p`.
fn moved(c: &SketchCurve, which: u8, p: DVec2) -> SketchCurve {
    match (*c, which) {
        (SketchCurve::Line { b, .. }, 0) => SketchCurve::Line { a: p, b },
        (SketchCurve::Line { a, .. }, 1) => SketchCurve::Line { a, b: p },
        (SketchCurve::Arc { b, bulge, .. }, 0) => SketchCurve::Arc { a: p, b, bulge },
        (SketchCurve::Arc { a, bulge, .. }, 1) => SketchCurve::Arc { a, b: p, bulge },
        (SketchCurve::Arc { a, b, bulge }, 2) => {
            SketchCurve::Arc { a, b, bulge: Arc::bulge_through(a, p, b).unwrap_or(bulge) }
        }
        (SketchCurve::Circle { radius, .. }, 3) => SketchCurve::Circle { center: p, radius },
        (SketchCurve::Circle { center, .. }, 4) => SketchCurve::Circle { center, radius: center.distance(p) },
        (SketchCurve::Ellipse { radii, rotation_deg, start_deg, sweep_deg, .. }, 3) => {
            SketchCurve::Ellipse { center: p, radii, rotation_deg, start_deg, sweep_deg }
        }
        (SketchCurve::Ellipse { center, radii, start_deg, sweep_deg, .. }, 4) => {
            let d = p - center;
            SketchCurve::Ellipse {
                center,
                radii: DVec2::new(d.length(), radii.y),
                rotation_deg: d.y.atan2(d.x).to_degrees(),
                start_deg,
                sweep_deg,
            }
        }
        (SketchCurve::Ellipse { center, radii, rotation_deg, start_deg, sweep_deg }, 5) => {
            let axis = DVec2::from_angle(rotation_deg.to_radians()).perp();
            SketchCurve::Ellipse {
                center,
                radii: DVec2::new(radii.x, (p - center).dot(axis).abs()),
                rotation_deg,
                start_deg,
                sweep_deg,
            }
        }
        _ => *c,
    }
}

/// The direction a curve leaves its end `at` (0: its start, 1: its end),
/// pointing away from it.
fn end_tangent(c: &SketchCurve, at: u8) -> Option<DVec2> {
    let pts = c.points();
    let (p, q) = match at {
        0 => (*pts.get(1)?, *pts.first()?),
        _ => (*pts.get(pts.len().checked_sub(2)?)?, *pts.last()?),
    };
    (q - p).try_normalize()
}

/// The element whose boundary *Edit Boundary* would open: one selected
/// floor, ceiling, roof or mass.
pub fn boundary_target(state: &AppState) -> Option<ElementId> {
    let mut sel = state.selection.iter();
    let (Some(id), None) = (sel.next(), sel.next()) else { return None };
    state.project().element(*id).filter(|e| e.sketched_profile().is_some()).map(|e| e.id)
}

/// Open the sketch of the selected element's outline (*Edit Boundary*).
pub fn edit_boundary(state: &mut AppState) {
    let Some(id) = boundary_target(state) else { return };
    let Some(e) = state.project().element(id) else { return };
    let (category, type_id) = (e.category(), e.type_id);
    let curves = match (e.drawn_sketch(), e.sketched_profile()) {
        (Some(s), _) => s.curves.clone(),
        (None, Some((outline, openings))) => Sketch::from_polygons(outline, openings).curves,
        _ => return,
    };
    // The element's level shows its outline: open its plan if another view is active.
    let level = e.level;
    if state.active_level() != Some(level)
        && let Some(view) = state.project().views.values().find(|v| v.level() == Some(level)).map(|v| v.id)
    {
        state.open_view(view);
    }
    state.set_tool(Tool::Sketch { category, type_id });
    let mut session = Session::new(Target::Edit { id }, curves);
    session.draw = Draw::Select;
    state.sketch = Some(session);
    state.selection.clear();
}

/// Leave the sketch without changing the model.
pub fn cancel(state: &mut AppState) {
    state.sketch = None;
    state.set_tool(Tool::Select);
}

/// Make the element (or its new outline) from the sketch; a sketch that
/// does not close shows where and stays open.
pub fn finish(state: &mut AppState) {
    let Some(session) = state.sketch.clone() else { return };
    let sketch = Sketch { curves: session.curves.clone() };
    let allows_openings = match session.target {
        Target::New { category, .. } => matches!(category, Category::Floor | Category::Roof),
        Target::Edit { id } => {
            matches!(state.project().element(id).map(|e| e.category()), Some(Category::Floor | Category::Roof))
        }
    };
    let checked = if allows_openings { sketch.profile().map(|_| ()) } else { sketch.outline().map(|_| ()) };
    if let Err(e) = checked {
        state.message = error_text(&e);
        return;
    }
    let curves: Vec<CurveArg> = session.curves.iter().map(CurveArg::of).collect();
    let out = match session.target {
        Target::Edit { id } => {
            let command = cc_engine::commands::sketch::SketchSet { id: id.0, curves };
            crate::actions::run_command(state, command).map(|_| id)
        }
        Target::New { category, type_id } => {
            // The type picked in the type selector since the sketch opened.
            let type_id = match state.tool {
                Tool::Sketch { type_id, .. } => type_id,
                _ => type_id,
            };
            let Some(level) = state.active_level() else {
                state.message = t!("msg-open-plan");
                return;
            };
            create(state, category, type_id, level, curves)
        }
    };
    if let Some(id) = out {
        state.sketch = None;
        state.set_tool(Tool::Select);
        state.selection = BTreeSet::from([id]);
    }
}

/// Run the command that makes a new element from `curves`.
fn create(
    state: &mut AppState,
    category: Category,
    type_id: ElementTypeId,
    level: LevelId,
    curves: Vec<CurveArg>,
) -> Option<ElementId> {
    use cc_engine::commands::create::{CeilingCreate, FloorCreate, RoofCreate, SketchArgs};
    let sketch = SketchArgs {
        curves: Some(curves.clone()),
        level: Some(Ref::Id(level.0)),
        type_ref: Some(Ref::Id(type_id.0)),
        ..Default::default()
    };
    let out = match category {
        Category::Floor => crate::actions::run_command(state, FloorCreate { sketch, openings: Vec::new() }),
        Category::Ceiling => crate::actions::run_command(state, CeilingCreate { sketch }),
        Category::Foundation => {
            crate::actions::run_command(state, cc_engine::commands::foundation::FoundationSlabCreate { sketch })
        }
        Category::Mass => crate::actions::run_command(
            state,
            cc_engine::commands::mass::MassCreate {
                curves: Some(curves),
                level: Some(Ref::Id(level.0)),
                type_ref: Some(Ref::Id(type_id.0)),
                ..Default::default()
            },
        ),
        _ => crate::actions::run_command(state, RoofCreate { sketch, overhang: None, holes: Vec::new() }),
    };
    out.and_then(|o| o["id"].as_u64()).map(ElementId)
}

/// A sketch error as a message.
pub fn error_text(e: &SketchError) -> String {
    crate::i18n::tr(e.key())
}

/// Escape: drop the curve being drawn, then leave the drawing tool, then the selection.
pub fn escape(state: &mut AppState) {
    let Some(s) = state.sketch.as_mut() else { return };
    if !s.picks.is_empty() || s.picked.is_some() {
        s.picks.clear();
        s.picked = None;
    } else if s.draw != Draw::Select {
        s.draw = Draw::Select;
    } else {
        s.selected.clear();
    }
}

/// Pick a drawing or modify tool; those acting on the picked curves need some.
pub fn set_draw(state: &mut AppState, draw: Draw) {
    let Some(s) = state.sketch.as_mut() else { return };
    if draw.on_selection() && s.selected.is_empty() {
        state.message = t!("msg-select-first");
        return;
    }
    s.draw = draw;
    s.picks.clear();
    s.picked = None;
}

/// Add lines along the loop of the level's walls (their exterior faces;
/// interior faces for ceilings).
pub fn pick_walls(state: &mut AppState) {
    let Some(level) = state.active_level() else {
        state.message = t!("msg-open-plan");
        return;
    };
    let category = match state.sketch.as_ref().map(|s| s.target) {
        Some(Target::New { category, .. }) => category,
        Some(Target::Edit { id }) => state.project().element(id).map_or(Category::Floor, |e| e.category()),
        None => return,
    };
    let outward = if category == Category::Ceiling { -1.0 } else { 1.0 };
    match crate::tools::walls_loop(state.project(), level, outward) {
        Some(pts) => {
            let lines = Sketch::from_polygons(&pts, &[]).curves;
            if let Some(s) = state.sketch.as_mut() {
                s.add(lines);
            }
        }
        None => state.message = t!("msg-no-wall-loop"),
    }
}

/// The sketch in a plan: its curves, the tool's preview, snapping, picks and grips.
pub fn canvas(ui: &Ui, resp: &Response, painter: &Painter, state: &mut AppState, level: LevelId, xf: &Xf) {
    let Some(mut s) = state.sketch.take() else { return };
    let tol = PICK_PX / xf.scale;
    // The curves; the selected ones highlighted; where the sketch does not close.
    for (i, c) in s.curves.iter().enumerate() {
        let color = if s.selected.contains(&i) { HIGHLIGHT } else { SKETCH };
        let pts: Vec<egui::Pos2> = c.points().iter().map(|p| xf.to_screen(*p)).collect();
        painter.add(egui::Shape::line(pts, Stroke::new(1.8, color)));
    }
    if let Err(e) = (Sketch { curves: s.curves.clone() }).loops()
        && let Some(p) = e.point()
        && s.picks.is_empty()
    {
        painter.circle_stroke(xf.to_screen(p), 6.0, Stroke::new(1.5, PALETTE.danger));
    }
    let raw = resp.hover_pos().map(|p| xf.to_world(p));
    let shift = ui.input(|i| i.modifiers.shift);
    let extra = s.snap_points();
    let anchor = s.picks.last().copied();
    let snapped = raw.map(|r| snap_at(state, Some(level), r, anchor, &extra, xf.scale, shift));
    if s.draw == Draw::Select {
        select(ui, resp, painter, &mut s, raw, snapped.map(|x| x.point), xf, tol);
    } else if s.draw.modifies() {
        if let (Some(raw), Some(sn)) = (raw, snapped) {
            draw_snap(painter, xf, sn);
            if let Some(message) = modify(resp, painter, &mut s, &state.modify, raw, sn.point, xf, tol) {
                state.message = message;
            }
        }
    } else if let Some(sn) = snapped {
        draw_snap(painter, xf, sn);
        draw(resp, painter, &mut s, sn.point, xf, tol);
        if let Some(message) = arc_tangent_hint(&s, sn.point, tol) {
            state.message = message;
        }
    }
    state.sketch = Some(s);
}

/// The tangent arc needs a free end to start from.
fn arc_tangent_hint(s: &Session, p: DVec2, tol: f64) -> Option<String> {
    (s.draw == Draw::ArcTangent && s.picks.is_empty() && free_end(s, p, tol).is_none())
        .then(|| t!("hint-sketch-arc-tangent"))
}

/// A free end of the sketch near `p`: the curve, which end and where.
fn free_end(s: &Session, p: DVec2, tol: f64) -> Option<(usize, u8, DVec2)> {
    let ends: Vec<(usize, u8, DVec2)> = s
        .curves
        .iter()
        .enumerate()
        .filter_map(|(i, c)| c.ends().map(|(a, b)| [(i, 0, a), (i, 1, b)]))
        .flatten()
        .collect();
    ends.iter()
        .copied()
        .filter(|(i, _, q)| q.distance(p) <= tol && !ends.iter().any(|(j, _, r)| j != i && r.distance(*q) <= JOIN_TOL))
        .min_by(|a, b| a.2.distance(p).total_cmp(&b.2.distance(p)))
}

/// Picking and dragging grips in the *Select* mode.
#[allow(clippy::too_many_arguments)]
fn select(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    s: &mut Session,
    raw: Option<DVec2>,
    snapped: Option<DVec2>,
    xf: &Xf,
    tol: f64,
) {
    let grip_list: Vec<(Grip, DVec2)> =
        s.selected.iter().filter_map(|&i| s.curves.get(i).map(|c| grips(c, i))).flatten().collect();
    for (_, p) in &grip_list {
        let c = xf.to_screen(*p);
        painter.rect_filled(egui::Rect::from_center_size(c, egui::vec2(7.0, 7.0)), 1.0, PALETTE.paper);
        painter.rect_stroke(
            egui::Rect::from_center_size(c, egui::vec2(7.0, 7.0)),
            1.0,
            Stroke::new(1.2, HIGHLIGHT),
            egui::StrokeKind::Middle,
        );
    }
    let Some(raw) = raw else { return };
    // Dragging a grip: ends joined to it move along, so loops stay closed.
    // Dragged elsewhere, a window picks curves.
    if resp.drag_started_by(egui::PointerButton::Primary) {
        match grip_list.iter().find(|(_, p)| p.distance(raw) <= tol * 1.5) {
            Some((grip, _)) => s.drag = Some((*grip, s.curves.clone())),
            None => s.boxing = Some(raw),
        }
    }
    if let Some(start) = s.boxing {
        let rect = egui::Rect::from_two_pos(xf.to_screen(start), xf.to_screen(raw));
        // Dragged to the left, the window picks the curves it touches.
        let touching = xf.to_screen(raw).x < xf.to_screen(start).x;
        painter.rect_filled(rect, 0.0, HIGHLIGHT.gamma_multiply(0.08));
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, HIGHLIGHT), egui::StrokeKind::Middle);
        if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
            let picked: BTreeSet<usize> = s
                .curves
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    let pts: Vec<egui::Pos2> = c.points().iter().map(|p| xf.to_screen(*p)).collect();
                    if touching {
                        pts.windows(2).any(|w| segment_touches(w[0], w[1], rect))
                    } else {
                        pts.iter().all(|p| rect.contains(*p))
                    }
                })
                .map(|(i, _)| i)
                .collect();
            if ui.input(|i| i.modifiers.command) {
                s.selected.extend(picked);
            } else {
                s.selected = picked;
            }
            s.boxing = None;
        }
        return;
    }
    if let Some((grip, before)) = s.drag.clone() {
        let p = snapped.unwrap_or(raw);
        let mut curves = before.clone();
        if let Some(c) = before.get(grip.curve) {
            let at = grips(c, grip.curve).into_iter().find(|(g, _)| *g == grip).map(|(_, q)| q);
            if let (Some(at), true) = (at, grip.which <= 1) {
                // Every curve end at the grip's point follows it.
                for (k, other) in before.iter().enumerate() {
                    if let Some((a, b)) = other.ends() {
                        let mut c2 = *other;
                        if a.distance(at) <= JOIN_TOL {
                            c2 = moved(&c2, 0, p);
                        }
                        if b.distance(at) <= JOIN_TOL {
                            c2 = moved(&c2, 1, p);
                        }
                        curves[k] = c2;
                    }
                }
            } else {
                curves[grip.curve] = moved(c, grip.which, p);
            }
        }
        if curves.iter().all(SketchCurve::is_valid) {
            s.curves = curves;
        }
        if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
            if s.curves != before {
                s.undo.push(before);
                s.redo.clear();
            }
            s.drag = None;
        }
        return;
    }
    // Hover and click pick a curve (Ctrl adds or removes it).
    let hit = s.curve_at(raw, tol);
    if let Some(i) = hit
        && !s.selected.contains(&i)
        && let Some(c) = s.curves.get(i)
    {
        let pts: Vec<egui::Pos2> = c.points().iter().map(|p| xf.to_screen(*p)).collect();
        painter.add(egui::Shape::line(pts, Stroke::new(2.4, HIGHLIGHT.gamma_multiply(0.6))));
    }
    if resp.double_clicked()
        && let Some(i) = hit
    {
        // The chain of curves joined to it.
        s.selected = sketch_edit::chain(&s.curves, i).0.into_iter().map(|(k, _)| k).collect();
    } else if resp.clicked() {
        let ctrl = ui.input(|i| i.modifiers.command);
        match (hit, ctrl) {
            (Some(i), true) => {
                if !s.selected.remove(&i) {
                    s.selected.insert(i);
                }
            }
            (Some(i), false) => s.selected = BTreeSet::from([i]),
            (None, false) => s.selected.clear(),
            (None, true) => {}
        }
    }
}

/// The drawing tools: preview at `p`, and a click adds a point or the curve.
fn draw(resp: &Response, painter: &Painter, s: &mut Session, p: DVec2, xf: &Xf, tol: f64) {
    let preview = |painter: &Painter, curves: &[SketchCurve]| {
        for c in curves.iter().filter(|c| c.is_valid()) {
            let pts: Vec<egui::Pos2> = c.points().iter().map(|q| xf.to_screen(*q)).collect();
            painter.add(egui::Shape::line(pts, Stroke::new(1.2, PREVIEW)));
        }
    };
    let picks = s.picks.clone();
    let shape = next_curves(s, &picks, p, tol);
    match (&shape, picks.last()) {
        (Some(curves), _) => preview(painter, curves),
        (None, Some(&last)) => preview(painter, &[SketchCurve::Line { a: last, b: p }]),
        _ => {}
    }
    if let (Some(&a), Draw::Line | Draw::Rect) = (picks.first(), s.draw) {
        temp_dimension(painter, xf, a, p, None, None, 0.0);
    }
    if !resp.clicked() {
        return;
    }
    let needed = match s.draw {
        Draw::Line | Draw::Rect | Draw::Polygon | Draw::PolygonOut | Draw::Circle | Draw::ArcTangent => 2,
        Draw::Arc3 | Draw::ArcCenter | Draw::Ellipse => 3,
        _ => return,
    };
    if picks.len() + 1 < needed {
        // The tangent arc starts at a free end of the sketch.
        let start = match s.draw {
            Draw::ArcTangent => match free_end(s, p, tol) {
                Some((_, _, q)) => q,
                None => return,
            },
            _ => p,
        };
        if picks.last().is_none_or(|l| l.distance(start) > JOIN_TOL) {
            s.picks.push(start);
        }
        return;
    }
    let Some(curves) = shape else { return };
    s.add(curves);
    s.picks.clear();
    // Lines and tangent arcs go on from their end, until the chain closes.
    let closed = |s: &Session| (Sketch { curves: s.curves.clone() }).loops().is_ok();
    if matches!(s.draw, Draw::Line | Draw::ArcTangent) && s.chain && !closed(s) {
        s.picks.push(p);
    }
}

/// The curves the picks and the point `p` make with the active tool.
fn next_curves(s: &Session, picks: &[DVec2], p: DVec2, tol: f64) -> Option<Vec<SketchCurve>> {
    use cc_engine::commands::sketch::CurveArg as A;
    let deg = |v: DVec2| v.y.atan2(v.x).to_degrees();
    match (s.draw, picks) {
        (Draw::Line, [a]) => Some(vec![SketchCurve::Line { a: *a, b: p }]),
        (Draw::Rect, [a]) => A::Rectangle { corner: a.to_array(), opposite: p.to_array(), rotation: 0.0 }.curves().ok(),
        (Draw::Polygon | Draw::PolygonOut, [c]) => {
            let sides = s.sides.clamp(3, 64);
            let inscribed = s.draw == Draw::Polygon;
            // Circumscribed, the cursor marks the middle of a side: the first corner is half a side further round.
            let half = if inscribed { 0.0 } else { 180.0 / sides as f64 };
            let shape = A::Polygon {
                center: c.to_array(),
                radius: c.distance(p),
                sides,
                inscribed,
                rotation: deg(p - *c) + half,
            };
            shape.curves().ok()
        }
        (Draw::Circle, [c]) => Some(vec![SketchCurve::Circle { center: *c, radius: c.distance(p) }]),
        (Draw::Arc3, [a]) => Some(vec![SketchCurve::Line { a: *a, b: p }]),
        (Draw::Arc3, [a, b]) => {
            Arc::bulge_through(*a, p, *b).map(|bulge| vec![SketchCurve::Arc { a: *a, b: *b, bulge }])
        }
        (Draw::ArcCenter, [c]) => Some(vec![SketchCurve::Circle { center: *c, radius: c.distance(p) }]),
        (Draw::ArcCenter, [c, start]) => {
            let r = c.distance(*start);
            let (u, v) = (*start - *c, p - *c);
            let end = *c + v.normalize_or_zero() * r;
            // The arc turns towards the cursor the short way round.
            let sweep = u.angle_to(v);
            let bulge = (sweep / 4.0).tan();
            (sweep.abs() > 1e-6).then(|| vec![SketchCurve::Arc { a: *start, b: end, bulge }])
        }
        (Draw::ArcTangent, [a]) => {
            let (i, which, _) = free_end(s, *a, tol)?;
            let tangent = end_tangent(s.curves.get(i)?, which)?;
            Arc::bulge_tangent(*a, tangent, p).map(|bulge| vec![SketchCurve::Arc { a: *a, b: p, bulge }])
        }
        (Draw::Ellipse, [c]) => Some(vec![SketchCurve::Line { a: *c, b: p }]),
        (Draw::Ellipse, [c, axis]) => {
            let d = *axis - *c;
            let ry = (p - *c).dot(d.perp().normalize_or_zero()).abs();
            Some(vec![SketchCurve::ellipse(*c, DVec2::new(d.length(), ry), deg(d))])
        }
        _ => None,
    }
}

/// Whether the segment `a`–`b` (screen points) touches `rect`.
fn segment_touches(a: egui::Pos2, b: egui::Pos2, rect: egui::Rect) -> bool {
    // Clip the segment's parameter to the rectangle's slabs.
    let (mut t0, mut t1) = (0.0_f32, 1.0_f32);
    let d = b - a;
    for (p, q) in [(-d.x, a.x - rect.min.x), (d.x, rect.max.x - a.x), (-d.y, a.y - rect.min.y), (d.y, rect.max.y - a.y)]
    {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    t0 <= t1
}

/// A curve of the sketch drawn over it.
fn paint_curve(painter: &Painter, xf: &Xf, c: &SketchCurve, width: f32, color: Color32) {
    let pts: Vec<egui::Pos2> = c.points().iter().map(|p| xf.to_screen(*p)).collect();
    painter.add(egui::Shape::line(pts, Stroke::new(width, color)));
}

/// The line of the sketch under the cursor, as a mirror axis.
fn mirror_axis(s: &Session, p: DVec2, tol: f64) -> Option<(DVec2, DVec2)> {
    s.curves
        .iter()
        .filter_map(|c| match *c {
            SketchCurve::Line { a, b } => Some((a, b, distance(c, p))),
            _ => None,
        })
        .filter(|(_, _, d)| *d <= tol)
        .min_by(|x, y| x.2.total_cmp(&y.2))
        .map(|(a, b, _)| (a, b))
}

/// The modify tools at the cursor (`raw`, snapped `p`): the transforms of
/// the picked curves preview and apply them; offset, trim and split act on
/// the curves clicked. Returns a message when a click does nothing.
#[allow(clippy::too_many_arguments)]
fn modify(
    resp: &Response,
    painter: &Painter,
    s: &mut Session,
    opts: &crate::modify::ModifyOptions,
    raw: DVec2,
    p: DVec2,
    xf: &Xf,
    tol: f64,
) -> Option<String> {
    let click = resp.clicked();
    let hit = s.curve_at(raw, tol);
    let hover = |painter: &Painter, i: Option<usize>| {
        if let Some(c) = i.and_then(|i| s.curves.get(i)) {
            paint_curve(painter, xf, c, 2.4, HIGHLIGHT.gamma_multiply(0.6));
        }
    };
    if s.draw.on_selection() {
        if let Some(&last) = s.picks.last() {
            painter.line_segment([xf.to_screen(last), xf.to_screen(p)], Stroke::new(1.0, PREVIEW));
        }
        let t = match (s.draw, s.picks.as_slice()) {
            (Draw::Move | Draw::Copy, [base]) => {
                temp_dimension(painter, xf, *base, p, None, None, 0.0);
                Some(PlanTransform::translate(p - *base))
            }
            (Draw::Rotate, [center, from]) => {
                Some(PlanTransform::rotate_about(*center, (*from - *center).angle_to(p - *center)))
            }
            (Draw::MirrorAxis, [a]) => (a.distance(p) > JOIN_TOL).then(|| PlanTransform::mirror_across(*a, p - *a)),
            (Draw::MirrorPick, _) => {
                let axis = mirror_axis(s, raw, tol);
                if let Some((a, b)) = axis {
                    paint_curve(painter, xf, &SketchCurve::Line { a, b }, 2.4, HIGHLIGHT.gamma_multiply(0.6));
                }
                axis.map(|(a, b)| PlanTransform::mirror_across(a, b - a))
            }
            _ => None,
        };
        if let Some(t) = &t {
            for c in s.selected.iter().filter_map(|&i| s.curves.get(i)) {
                let mut c = *c;
                c.transform(t);
                paint_curve(painter, xf, &c, 1.2, PREVIEW);
            }
        }
        if !click {
            return None;
        }
        let Some(t) = t else {
            if s.draw == Draw::MirrorPick {
                return Some(t!("hint-sketch-mirror-pick"));
            }
            if s.picks.last().is_none_or(|l| l.distance(p) > JOIN_TOL) {
                s.picks.push(p);
            }
            return None;
        };
        let copy = match s.draw {
            Draw::Copy => true,
            Draw::Rotate => opts.rotate_copy,
            Draw::MirrorPick | Draw::MirrorAxis => opts.mirror_copy,
            _ => false,
        };
        let copies = s.transform_selected(&t, copy);
        // Copying several times goes on from the same base point.
        if s.draw == Draw::Copy && opts.multiple {
            return None;
        }
        if copy {
            s.selected = copies;
        }
        s.picks.clear();
        s.draw = Draw::Select;
        return None;
    }
    match s.draw {
        Draw::Offset => {
            hover(painter, hit);
            let i = hit?;
            let out = if s.chain {
                sketch_edit::offset_chain(&s.curves, i, opts.offset, raw)
            } else {
                s.curves.get(i).and_then(|c| sketch_edit::offset(c, opts.offset, raw)).map(|c| vec![c])
            };
            match out {
                Some(curves) => {
                    for c in &curves {
                        paint_curve(painter, xf, c, 1.2, PREVIEW);
                    }
                    if click {
                        s.add(curves);
                    }
                    None
                }
                None => click.then(|| t!("sketch-no-offset")),
            }
        }
        Draw::TrimCorner | Draw::TrimExtend => {
            let Some((first, at)) = s.picked else {
                hover(painter, hit);
                if click && let Some(i) = hit {
                    s.picked = Some((i, raw));
                }
                return None;
            };
            if let Some(c) = s.curves.get(first) {
                paint_curve(painter, xf, c, 2.4, HIGHLIGHT);
            }
            let j = hit.filter(|j| *j != first)?;
            let (c1, c2) = (*s.curves.get(first)?, *s.curves.get(j)?);
            hover(painter, Some(j));
            let changes = if s.draw == Draw::TrimCorner {
                sketch_edit::trim_corner(&c1, at, &c2, raw).map(|(a, b)| vec![(first, a), (j, b)])
            } else {
                sketch_edit::trim_extend(&c2, raw, &c1).map(|b| vec![(j, b)])
            };
            match changes {
                Some(changes) => {
                    for (_, c) in &changes {
                        paint_curve(painter, xf, c, 1.2, PREVIEW);
                    }
                    if click {
                        s.change(&changes, &[]);
                        // Trim/Extend goes on with the same reference.
                        if s.draw == Draw::TrimCorner {
                            s.picked = None;
                        }
                    }
                    None
                }
                None if !click => None,
                None if (c1.is_closed() && s.draw == Draw::TrimCorner) || c2.is_closed() => Some(t!("sketch-no-trim")),
                None => Some(t!("sketch-no-meet")),
            }
        }
        Draw::Split => {
            hover(painter, hit);
            let i = hit?;
            let c = *s.curves.get(i)?;
            let Some((a, b)) = sketch_edit::split(&c, p) else {
                return click.then(|| t!("sketch-no-split"));
            };
            if let Some((_, at)) = a.ends() {
                painter.circle_stroke(xf.to_screen(at), 4.0, Stroke::new(1.5, HIGHLIGHT));
            }
            if click {
                s.change(&[(i, a)], &[b]);
                s.selected.clear();
            }
            None
        }
        _ => None,
    }
}

// --- Ribbon, options bar and properties ---

/// The sketch's ribbon: mode, drawing tools and tools.
pub fn ribbon(ui: &mut Ui, state: &mut AppState) {
    use crate::commands::UiCommand::*;
    // The tool's name says what the sketch makes (the edited element's category opens it).
    let what = crate::i18n::tr(state.tool.key());
    let title = match state.sketch.as_ref().map(|s| s.target) {
        Some(Target::Edit { .. }) => t!("sketch-title-edit", "category" => what),
        _ => t!("sketch-title-new", "category" => what),
    };
    ui.horizontal(|ui| {
        ui.label(theme::title(title));
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        crate::ribbon::env_group(ui, state, t!("group-selection"), &[Select]);
        crate::ribbon::env_stacks(
            ui,
            state,
            t!("group-draw"),
            &[
                &[SketchLine, SketchRect, SketchPolygon],
                &[SketchPolygonOut, SketchCircle, SketchEllipse],
                &[SketchArc3, SketchArcCenter, SketchArcTangent],
            ],
        );
        crate::ribbon::env_stacks(
            ui,
            state,
            t!("group-modify"),
            &[&[Move, CopyElements, Rotate], &[MirrorPick, MirrorDraw, Offset], &[TrimCorner, TrimExtend, Split]],
        );
        crate::ribbon::env_group(ui, state, t!("group-sketch-tools"), &[SketchPickWalls, Delete]);
        crate::ribbon::env_group(ui, state, t!("group-mode"), &[SketchFinish, SketchCancel]);
    });
}

/// The options bar: chaining, polygon sides, the modify tools' options and the hint.
pub fn options(ui: &mut Ui, state: &mut AppState) {
    let opts = &mut state.modify;
    let Some(s) = state.sketch.as_mut() else { return };
    match s.draw {
        Draw::Line => {
            ui.checkbox(&mut s.chain, t!("sketch-chain"));
            ui.separator();
        }
        Draw::Polygon | Draw::PolygonOut => {
            ui.label(t!("sketch-sides"));
            ui.add(egui::DragValue::new(&mut s.sides).range(3..=64));
            ui.separator();
        }
        Draw::Copy => {
            ui.checkbox(&mut opts.multiple, t!("opt-multiple"));
            ui.separator();
        }
        Draw::Rotate => {
            ui.checkbox(&mut opts.rotate_copy, t!("opt-copy"));
            ui.separator();
            ui.label(t!("prop-rotation"));
            ui.add(egui::DragValue::new(&mut opts.angle).speed(1.0).suffix("°"));
            // About the center clicked, or the middle of the picked curves.
            if ui.button(t!("action-apply")).clicked()
                && let Some(center) = s.picks.first().copied().or_else(|| selection_center(s))
            {
                let copies = s.transform_selected(
                    &PlanTransform::rotate_about(center, opts.angle.to_radians()),
                    opts.rotate_copy,
                );
                if opts.rotate_copy {
                    s.selected = copies;
                }
                s.picks.clear();
                s.draw = Draw::Select;
            }
            ui.separator();
        }
        Draw::MirrorPick | Draw::MirrorAxis => {
            ui.checkbox(&mut opts.mirror_copy, t!("opt-copy"));
            ui.separator();
        }
        Draw::Offset => {
            ui.label(t!("opt-distance"));
            ui.add(crate::properties::length_drag(&mut opts.offset).range(0.001..=100.0));
            ui.checkbox(&mut s.chain, t!("sketch-chain"));
            ui.separator();
        }
        _ => {}
    }
    let hint = match (s.draw, s.picks.len()) {
        (Draw::Select, _) => t!("hint-sketch-select"),
        (Draw::Line, 0) => t!("hint-sketch-line"),
        (Draw::Line, _) => t!("hint-sketch-line-next"),
        (Draw::Rect, 0) => t!("hint-sketch-rect"),
        (Draw::Rect, _) => t!("hint-sketch-rect-next"),
        (Draw::Polygon | Draw::PolygonOut | Draw::Circle | Draw::ArcCenter | Draw::Ellipse, 0) => {
            t!("hint-sketch-center")
        }
        (Draw::Polygon, _) => t!("hint-sketch-corner"),
        (Draw::PolygonOut, _) => t!("hint-sketch-side"),
        (Draw::Circle, _) => t!("hint-sketch-radius"),
        (Draw::Arc3, 0) => t!("hint-sketch-arc-start"),
        (Draw::Arc3, 1) => t!("hint-sketch-arc-end"),
        (Draw::Arc3, _) => t!("hint-sketch-arc-through"),
        (Draw::ArcCenter, 1) => t!("hint-sketch-arc-start"),
        (Draw::ArcCenter, _) => t!("hint-sketch-arc-end"),
        (Draw::ArcTangent, 0) => t!("hint-sketch-arc-tangent"),
        (Draw::ArcTangent, _) => t!("hint-sketch-arc-end"),
        (Draw::Ellipse, 1) => t!("hint-sketch-axis"),
        (Draw::Ellipse, _) => t!("hint-sketch-other-axis"),
        (Draw::Move | Draw::Copy, 0) => t!("hint-base-point"),
        (Draw::Move | Draw::Copy, _) => t!("hint-sketch-move-target"),
        (Draw::Rotate, 0) => t!("hint-sketch-rotate-center"),
        (Draw::Rotate, 1) => t!("hint-sketch-rotate-from"),
        (Draw::Rotate, _) => t!("hint-rotate-target"),
        (Draw::MirrorPick, _) => t!("hint-sketch-mirror-pick"),
        (Draw::MirrorAxis, _) => t!("hint-mirror-draw"),
        (Draw::Offset, _) => t!("hint-sketch-offset"),
        (Draw::TrimCorner, _) if s.picked.is_some() => t!("hint-sketch-trim-corner-second"),
        (Draw::TrimCorner, _) => t!("hint-sketch-trim-corner"),
        (Draw::TrimExtend, _) if s.picked.is_some() => t!("hint-sketch-trim-extend-second"),
        (Draw::TrimExtend, _) => t!("hint-sketch-trim-extend"),
        (Draw::Split, _) => t!("hint-sketch-split"),
    };
    ui.label(theme::muted(hint));
}

/// The middle of the picked curves' extents.
fn selection_center(s: &Session) -> Option<DVec2> {
    let pts = s.selected.iter().filter_map(|&i| s.curves.get(i)).flat_map(|c| c.points());
    let (lo, hi) =
        pts.fold(None, |acc: Option<(DVec2, DVec2)>, p| Some(acc.map_or((p, p), |(lo, hi)| (lo.min(p), hi.max(p)))))?;
    Some((lo + hi) / 2.0)
}

/// The selected curve's values in the properties palette.
pub fn properties(ui: &mut Ui, state: &mut AppState) {
    use crate::properties::length_drag;
    use crate::table::{readonly_row, row, section};
    let Some(s) = state.sketch.as_mut() else { return };
    let mut sel = s.selected.iter();
    let (Some(&i), None) = (sel.next(), sel.next()) else {
        crate::table::padded(ui, |ui| {
            let text = match s.selected.len() {
                0 => t!("sketch-props-none"),
                n => t!("sketch-props-many", "count" => n),
            };
            ui.label(egui::RichText::new(text).color(PALETTE.text_faint));
        });
        return;
    };
    let Some(mut c) = s.curves.get(i).copied() else { return };
    let before = c;
    let point = |ui: &mut Ui, label: &str, p: &mut DVec2| {
        row(ui, label, |ui| {
            // X and Y share the value column.
            let w = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(40.0);
            ui.spacing_mut().interact_size.x = w;
            ui.add(length_drag(&mut p.x));
            ui.add(length_drag(&mut p.y));
        });
    };
    let title = match c {
        SketchCurve::Line { .. } => t!("sketch-line"),
        SketchCurve::Arc { .. } => t!("sketch-arc"),
        SketchCurve::Circle { .. } => t!("sketch-circle"),
        SketchCurve::Ellipse { .. } => t!("sketch-ellipse"),
    };
    section(ui, "sketch-curve", title, |ui| match &mut c {
        SketchCurve::Line { a, b } => {
            point(ui, &t!("prop-start"), a);
            point(ui, &t!("prop-end"), b);
            readonly_row(ui, &t!("prop-length"), cc_views::format::length(a.distance(*b)));
        }
        SketchCurve::Arc { a, b, bulge } => {
            point(ui, &t!("prop-start"), a);
            point(ui, &t!("prop-end"), b);
            if let Some(arc) = Arc::from_bulge(*a, *b, *bulge) {
                let mut radius = arc.radius;
                if row(ui, &t!("prop-radius"), |ui| ui.add(length_drag(&mut radius)).changed())
                    && let Some(v) = Arc::bulge_radius(*a, *b, radius, *bulge > 0.0, bulge.abs() > 1.0)
                {
                    *bulge = v;
                }
                readonly_row(ui, &t!("prop-length"), cc_views::format::length(arc.length()));
                readonly_row(ui, &t!("prop-angle"), cc_views::format::angle(arc.sweep.abs().to_degrees()));
            }
        }
        SketchCurve::Circle { center, radius } => {
            point(ui, &t!("prop-center"), center);
            row(ui, &t!("prop-radius"), |ui| ui.add(length_drag(radius)));
        }
        SketchCurve::Ellipse { center, radii, rotation_deg, .. } => {
            point(ui, &t!("prop-center"), center);
            row(ui, &t!("sketch-radius-x"), |ui| ui.add(length_drag(&mut radii.x)));
            row(ui, &t!("sketch-radius-y"), |ui| ui.add(length_drag(&mut radii.y)));
            row(ui, &t!("prop-rotation"), |ui| ui.add(egui::DragValue::new(rotation_deg).speed(1.0).suffix("°")));
        }
    });
    if c != before {
        s.set_curve(i, c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(draw: Draw) -> Session {
        let mut s = Session::new(Target::New { category: Category::Mass, type_id: ElementTypeId(1) }, Vec::new());
        s.draw = draw;
        s
    }

    #[test]
    fn tools_make_their_curves() {
        let p = |x: f64, y: f64| DVec2::new(x, y);
        let s = session(Draw::Rect);
        assert_eq!(next_curves(&s, &[p(0.0, 0.0)], p(4.0, 2.0), 0.1).unwrap().len(), 4);
        let mut s = session(Draw::PolygonOut);
        s.sides = 4;
        let square = next_curves(&s, &[p(0.0, 0.0)], p(1.0, 0.0), 0.1).unwrap();
        // Circumscribed with a side's middle at (1, 0): a 2 m square.
        let sketch = Sketch { curves: square };
        let outline = sketch.outline().unwrap();
        assert!(outline.iter().any(|q| q.distance(p(1.0, 1.0)) < 1e-9), "{outline:?}");
        let s = session(Draw::Arc3);
        let arc = next_curves(&s, &[p(1.0, 0.0), p(-1.0, 0.0)], p(0.0, 1.0), 0.1).unwrap();
        assert!(matches!(arc[0], SketchCurve::Arc { bulge, .. } if (bulge - 1.0).abs() < 1e-9));
        let s = session(Draw::Ellipse);
        let e = next_curves(&s, &[p(0.0, 0.0), p(0.0, 3.0)], p(2.0, 1.0), 0.1).unwrap();
        assert!(matches!(e[0], SketchCurve::Ellipse { radii, rotation_deg, .. }
            if (radii - p(3.0, 2.0)).length() < 1e-9 && (rotation_deg - 90.0).abs() < 1e-9));
    }

    #[test]
    fn tangent_arcs_continue_a_line() {
        let mut s = session(Draw::ArcTangent);
        s.curves.push(SketchCurve::Line { a: DVec2::ZERO, b: DVec2::new(4.0, 0.0) });
        // From the line's end, turning back to (4, 2): a half circle.
        let arc = next_curves(&s, &[DVec2::new(4.0, 0.0)], DVec2::new(4.0, 2.0), 0.1).unwrap();
        assert!(matches!(arc[0], SketchCurve::Arc { bulge, .. } if (bulge - 1.0).abs() < 1e-9), "{arc:?}");
        // Only free ends start one.
        assert!(free_end(&s, DVec2::new(4.0, 0.0), 0.1).is_some());
        s.curves.push(SketchCurve::Line { a: DVec2::new(4.0, 0.0), b: DVec2::new(4.0, 4.0) });
        assert!(free_end(&s, DVec2::new(4.0, 0.0), 0.1).is_none());
    }

    #[test]
    fn picked_curves_move_copy_and_change_in_one_step() {
        let mut s = session(Draw::Select);
        let rect = CurveArg::Rectangle { corner: [0.0, 0.0], opposite: [4.0, 2.0], rotation: 0.0 }.curves().unwrap();
        s.add(rect);
        s.selected = BTreeSet::from([0, 1]);
        let copies = s.transform_selected(&PlanTransform::translate(DVec2::new(0.0, 10.0)), true);
        assert_eq!((s.curves.len(), copies), (6, BTreeSet::from([4, 5])));
        assert_eq!(s.curves[4], SketchCurve::Line { a: DVec2::new(0.0, 10.0), b: DVec2::new(4.0, 10.0) });
        // Moved, not copied: one undo step brings them back.
        s.transform_selected(&PlanTransform::rotate_about(DVec2::ZERO, std::f64::consts::PI), false);
        assert!(matches!(s.curves[0], SketchCurve::Line { b, .. } if b.distance(DVec2::new(-4.0, 0.0)) < 1e-9));
        s.undo();
        assert_eq!(s.curves[0], SketchCurve::Line { a: DVec2::ZERO, b: DVec2::new(4.0, 0.0) });
        // A change with an invalid curve changes nothing.
        let before = s.curves.clone();
        s.change(&[(0, SketchCurve::Line { a: DVec2::ZERO, b: DVec2::ZERO })], &[]);
        assert_eq!(s.curves, before);
        // Windows touch the segments that cross them, even with both ends outside.
        let r = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(10.0, 10.0));
        assert!(segment_touches(egui::pos2(-5.0, 5.0), egui::pos2(15.0, 5.0), r));
        assert!(!segment_touches(egui::pos2(-5.0, 15.0), egui::pos2(15.0, 12.0), r));
        // Tools on the picked curves need some.
        s.selected.clear();
        assert!(Draw::Move.on_selection() && !Draw::Offset.on_selection() && Draw::Split.modifies());
    }

    #[test]
    fn grips_keep_loops_closed_and_undo() {
        let mut s = session(Draw::Select);
        let rect = CurveArg::Rectangle { corner: [0.0, 0.0], opposite: [4.0, 2.0], rotation: 0.0 }.curves().unwrap();
        s.add(rect);
        assert!(s.can_undo());
        let circle = SketchCurve::Circle { center: DVec2::new(2.0, 1.0), radius: 0.5 };
        assert_eq!(
            moved(&circle, 4, DVec2::new(2.0, 1.8)),
            SketchCurve::Circle { center: DVec2::new(2.0, 1.0), radius: 0.8 }
        );
        s.set_curve(0, SketchCurve::Line { a: DVec2::ZERO, b: DVec2::new(5.0, 0.0) });
        assert!((Sketch { curves: s.curves.clone() }).loops().is_err(), "an end moved alone opens the loop");
        s.undo();
        assert!((Sketch { curves: s.curves.clone() }).loops().is_ok());
        s.selected.insert(0);
        s.delete_selected();
        assert_eq!(s.curves.len(), 3);
        s.undo();
        s.undo();
        assert!(s.curves.is_empty());
        s.redo();
        assert_eq!(s.curves.len(), 4);
    }
}
