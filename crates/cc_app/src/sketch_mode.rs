//! Sketch mode: the outline of a floor, ceiling, roof, foundation slab or
//! mass drawn with lines, rectangles, polygons, circles, arcs and ellipses
//! (see [`cc_core::sketch`]), in the active plan. The ribbon shows the
//! drawing tools and *Finish*/*Cancel* while the sketch is open; in its
//! *Select* mode a click picks a curve, whose grips stretch it and whose
//! values the properties palette edits. *Finish* creates the element (or
//! draws the edited element's outline again, *Edit Boundary*) through the
//! engine's `curves`; the sketch's own changes undo inside it.

use crate::paint::{HIGHLIGHT, PREVIEW, Xf};
use crate::state::AppState;
use crate::t;
use crate::theme::{self, PALETTE};
use crate::tools::{Tool, draw_snap, snap_at, temp_dimension};
use cc_core::arc::Arc;
use cc_core::sketch::{JOIN_TOL, Sketch, SketchCurve, SketchError};
use cc_core::{Category, ElementId, ElementTypeId, LevelId};
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
    if !s.picks.is_empty() {
        s.picks.clear();
    } else if s.draw != Draw::Select {
        s.draw = Draw::Select;
    } else {
        s.selected.clear();
    }
}

pub fn set_draw(state: &mut AppState, draw: Draw) {
    if let Some(s) = state.sketch.as_mut() {
        s.draw = draw;
        s.picks.clear();
    }
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
    if resp.drag_started_by(egui::PointerButton::Primary)
        && let Some((grip, _)) = grip_list.iter().find(|(_, p)| p.distance(raw) <= tol * 1.5)
    {
        s.drag = Some((*grip, s.curves.clone()));
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
    if resp.clicked() {
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
        Draw::Select => return,
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
        crate::ribbon::env_group(ui, state, t!("group-sketch-tools"), &[SketchPickWalls, Delete]);
        crate::ribbon::env_group(ui, state, t!("group-mode"), &[SketchFinish, SketchCancel]);
    });
}

/// The options bar: chaining, polygon sides and the hint.
pub fn options(ui: &mut Ui, state: &mut AppState) {
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
    };
    ui.label(theme::muted(hint));
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
