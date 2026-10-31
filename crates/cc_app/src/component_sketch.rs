//! Sketching pieces in the component modeling environment. *Extrusion*,
//! *Revolution*, *Sweep* and *Blend* open the same sketch as outlines in
//! plans ([`crate::sketch_mode`]), in the plan, front or side view: its
//! drawing and modify tools, picked curves and their grips, its own undo.
//! *Finish* turns the curves into the piece's profile points: an extrusion
//! is made at once, a revolution then asks for its axis, a sweep's curves
//! are its open path (the section comes from the options bar), and a blend
//! takes a bottom outline, then a top one. *Edit Sketch* opens a piece's
//! profile again.
//!
//! A coordinate that lies on a reference plane seen edge-on is written as
//! the plane's name, so the piece follows the plane when a parameter moves
//! it; a point on a circle or an ellipse is written from its center. Other
//! numbers are rounded to the micrometer. Editing keeps the expressions of
//! the points that did not move.

use crate::component_env::Pick;
use crate::paint::Xf;
use crate::sketch_mode::{Draw, Session, Target};
use crate::state::AppState;
use crate::t;
use crate::theme::{self, PALETTE};
use crate::tools::Snap;
use cc_core::ComponentShape;
use cc_core::profile::{self, ProfilePoint, Segment};
use cc_core::sketch::SketchCurve;
use cc_engine::commands::component_piece::{ComponentPieceAdd, ComponentPieceSet, EllipseArg, ExprArg, SketchPointArg};
use cc_views::{ComponentView, ComponentViewKind};
use egui::{Painter, Response, Stroke, Ui};
use glam::DVec2;

/// What the sketch becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PieceKind {
    #[default]
    Extrusion,
    /// Turned about an axis picked after the outline is finished.
    Revolution,
    /// A section carried along an open path (the path is what is drawn).
    Sweep,
    /// A bottom outline, then a top one.
    Blend,
}

/// A piece being sketched: what it becomes and where it stands.
#[derive(Debug, Clone, PartialEq)]
pub struct PieceSketch {
    pub kind: PieceKind,
    pub void: bool,
    /// The view the sketch is drawn in (set by its first curve, or the
    /// edited piece's plane).
    pub view: Option<ComponentViewKind>,
    /// The piece whose sketch is edited.
    pub edit: Option<usize>,
    /// The edited outline's (or path's) points as they were, and a blend's top.
    pub old: Vec<ProfilePoint>,
    pub old_top: Vec<ProfilePoint>,
    /// A blend's bottom outline, once finished.
    pub bottom: Option<Vec<SketchCurve>>,
    /// The outline of a revolution is finished and waits for its axis.
    pub axis: bool,
    /// A sweep's section: width (or diameter) and height, round or not.
    pub section: (f64, f64),
    pub round: bool,
}

impl PieceSketch {
    pub fn new(kind: PieceKind) -> Self {
        Self {
            kind,
            void: false,
            view: None,
            edit: None,
            old: Vec::new(),
            old_top: Vec::new(),
            bottom: None,
            axis: false,
            section: (0.05, 0.05),
            round: false,
        }
    }
}

/// A sweep's section about the path: a `w` by `h` rectangle, or a circle of
/// diameter `w`, centered on it (u to the left of the path, v along the normal).
pub fn section_points(w: f64, h: f64, round: bool) -> Vec<SketchPointArg> {
    let n = |v: f64| ExprArg::Text(mm(v));
    if round {
        let r = w / 2.0;
        return vec![
            SketchPointArg { u: n(-r), v: n(0.0), radius: Some(n(r)), ellipse: None },
            SketchPointArg { u: n(r), v: n(0.0), radius: Some(n(r)), ellipse: None },
        ];
    }
    let (a, b) = (w / 2.0, h / 2.0);
    [(-a, -b), (a, -b), (a, b), (-a, b)]
        .iter()
        .map(|(u, v)| SketchPointArg { u: n(*u), v: n(*v), radius: None, ellipse: None })
        .collect()
}

/// A number rounded to the millimeter, as an expression.
fn mm(v: f64) -> String {
    rounded(v, 1000.0)
}

/// A number rounded to the micrometer, as an expression.
fn um(v: f64) -> String {
    rounded(v, 1e6)
}

fn rounded(v: f64, per: f64) -> String {
    let r = (v * per).round() / per;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r}")
}

/// `e + d`, folded into a number when `e` is one.
pub fn offset(e: &str, d: f64) -> String {
    if d.abs() < 1e-12 {
        return e.to_string();
    }
    if let Ok(v) = e.trim().parse::<f64>() {
        return um(v + d);
    }
    if d > 0.0 { format!("{e} + {}", um(d)) } else { format!("{e} - {}", um(-d)) }
}

/// The point at `p` in `view`: on the planes whose lines pass within `tol`
/// (a vertical line fixes u, a horizontal one v), elsewhere rounded to the
/// millimeter; and whether it lies on a plane.
pub fn snap(view: &ComponentView, p: DVec2, tol: f64) -> (DVec2, bool) {
    let mut pos = p;
    let (mut du, mut dv) = (tol, tol);
    let (mut on_u, mut on_v) = (false, false);
    for plane in &view.planes {
        let vertical = (plane.b.x - plane.a.x).abs() < 1e-9;
        if vertical && (p.x - plane.a.x).abs() <= du {
            du = (p.x - plane.a.x).abs();
            pos.x = plane.a.x;
            on_u = true;
        } else if !vertical && (p.y - plane.a.y).abs() <= dv {
            dv = (p.y - plane.a.y).abs();
            pos.y = plane.a.y;
            on_v = true;
        }
    }
    if !on_u {
        pos.x = (pos.x * 1000.0).round() / 1000.0;
    }
    if !on_v {
        pos.y = (pos.y * 1000.0).round() / 1000.0;
    }
    (pos, on_u || on_v)
}

/// The name of the plane seen edge-on at `value` along u (`along_u`, a
/// vertical line) or v.
fn plane_at(view: &ComponentView, value: f64, along_u: bool) -> Option<String> {
    view.planes
        .iter()
        .find(|pl| {
            let vertical = (pl.b.x - pl.a.x).abs() < 1e-9;
            let at = if along_u { pl.a.x } else { pl.a.y };
            vertical == along_u && (at - value).abs() < 1e-6
        })
        .map(|pl| pl.name.clone())
}

/// The expression of a coordinate: a plane's name, or a number.
fn coordinate(view: &ComponentView, value: f64, along_u: bool) -> String {
    plane_at(view, value, along_u).unwrap_or_else(|| um(value))
}

/// Writes the vertices of a profile as expressions.
struct Writer<'a> {
    view: &'a ComponentView,
    /// The sketch's circles and whole ellipses.
    rounds: Vec<SketchCurve>,
    /// The points the profile had, with their positions and segments.
    old: Vec<(ProfilePoint, DVec2, Segment)>,
}

impl Writer<'_> {
    /// A coordinate of `p`: a plane, from the center of a circle or an
    /// ellipse it lies on (when that center is on a plane), or a number.
    fn coordinate(&self, p: DVec2, along_u: bool) -> String {
        let value = if along_u { p.x } else { p.y };
        if let Some(name) = plane_at(self.view, value, along_u) {
            return name;
        }
        for c in &self.rounds {
            let (SketchCurve::Circle { center, .. } | SketchCurve::Ellipse { center, .. }) = *c else { continue };
            let across = |d: DVec2| cc_core::sketch_edit::crossings(c, &SketchCurve::Line { a: p, b: p + d });
            let on = across(DVec2::X).iter().chain(&across(DVec2::Y)).any(|q| q.distance(p) < 1e-6);
            let at = if along_u { center.x } else { center.y };
            if on && let Some(name) = plane_at(self.view, at, along_u) {
                return offset(&name, value - at);
            }
        }
        um(value)
    }

    fn point(&self, p: DVec2, segment: Segment) -> SketchPointArg {
        // A point that did not move keeps what it was written as.
        if let Some((old, _, _)) = self.old.iter().find(|(_, q, s)| q.distance(p) < 1e-6 && same(*s, segment)) {
            return SketchPointArg::of(old);
        }
        let text = ExprArg::Text;
        let (u, v) = (self.coordinate(p, true), self.coordinate(p, false));
        match segment {
            Segment::Line => SketchPointArg::new(u, v, ""),
            Segment::Arc { radius } => {
                // Rounded up, so it still spans the rounded ends.
                let r = (radius.abs() * 1e6).ceil() / 1e6;
                SketchPointArg::new(u, v, &um(r * radius.signum()))
            }
            Segment::Ellipse { center, radii, rotation_deg, clockwise } => SketchPointArg {
                ellipse: Some(EllipseArg {
                    u: text(coordinate(self.view, center.x, true)),
                    v: text(coordinate(self.view, center.y, false)),
                    rx: text(um(radii.x)),
                    ry: text(um(radii.y)),
                    rotation: (rotation_deg.abs() > 1e-9).then(|| text(um(rotation_deg))),
                    clockwise,
                }),
                ..SketchPointArg::new(u, v, "")
            },
        }
    }
}

/// Whether two segments are the same (an arc of the same radius, the same ellipse).
fn same(a: Segment, b: Segment) -> bool {
    match (a, b) {
        (Segment::Line, Segment::Line) => true,
        (Segment::Arc { radius: r }, Segment::Arc { radius: s }) => (r - s).abs() < 1e-6,
        (
            Segment::Ellipse { center: c, radii: r, rotation_deg: t, clockwise: w },
            Segment::Ellipse { center: d, radii: s, rotation_deg: u, clockwise: x },
        ) => c.distance(d) < 1e-6 && (r - s).abs().max_element() < 1e-6 && (t - u).abs() < 1e-6 && w == x,
        _ => false,
    }
}

/// The profile points that draw `curves` (closed, or an open path), or why not.
fn written(
    def: &cc_core::ComponentDef,
    view: &ComponentView,
    curves: &[SketchCurve],
    closed: bool,
    old: &[ProfilePoint],
) -> Result<Vec<SketchPointArg>, String> {
    let vertices = profile::from_curves(curves, closed).map_err(|e| crate::sketch_mode::error_text(&e))?;
    let scope = def.scope(&Default::default(), &[]);
    let lookup = |name: &str| scope.get(name);
    let old = profile::vertices(old, &lookup)
        .map(|v| old.iter().cloned().zip(v).map(|(p, (at, s))| (p, at, s)).collect())
        .unwrap_or_default();
    let rounds = curves.iter().copied().filter(SketchCurve::is_closed).collect();
    let writer = Writer { view, rounds, old };
    Ok(vertices.iter().map(|&(p, s)| writer.point(p, s)).collect())
}

/// The sketch plane of a view.
fn plane_name(kind: ComponentViewKind) -> &'static str {
    match kind {
        ComponentViewKind::Front => "front",
        ComponentViewKind::Side => "side",
        _ => "plan",
    }
}

fn view_of(plane: profile::SketchPlane) -> ComponentViewKind {
    match plane {
        profile::SketchPlane::Plan => ComponentViewKind::Plan,
        profile::SketchPlane::Front => ComponentViewKind::Front,
        profile::SketchPlane::Side => ComponentViewKind::Side,
    }
}

/// The open sketch's piece, if it is one.
pub fn piece(state: &AppState) -> Option<&PieceSketch> {
    state.sketch.as_ref().and_then(|s| s.piece.as_ref())
}

/// Start sketching a new piece of `kind`.
pub fn start(state: &mut AppState, kind: PieceKind) {
    let void = piece(state).is_some_and(|p| p.void);
    let mut session = Session::new(Target::Piece, Vec::new());
    session.piece = Some(PieceSketch { void, ..PieceSketch::new(kind) });
    state.sketch = Some(session);
    if let Some(env) = state.component_env.as_mut() {
        env.tool = crate::component_env::EnvTool::Select;
    }
}

/// Open the sketch of piece `index` (*Edit Sketch*).
pub fn edit(state: &mut AppState, index: usize) {
    let Some((_, def)) = crate::component_env::component(state) else { return };
    let Some(item) = def.items.get(index) else { return };
    let scope = def.scope(&Default::default(), &[]);
    let lookup = |name: &str| scope.get(name);
    let mut piece = PieceSketch { void: item.void, edit: Some(index), ..PieceSketch::new(PieceKind::Extrusion) };
    let (plane, outline, closed) = match &item.shape {
        ComponentShape::Profile { plane, points, .. } => (*plane, points, true),
        ComponentShape::Revolution { plane, points, .. } => {
            piece.kind = PieceKind::Revolution;
            (*plane, points, true)
        }
        ComponentShape::Sweep { plane, path, profile: section, .. } => {
            piece.kind = PieceKind::Sweep;
            // The section as it is, unless the options change it.
            if let Ok(o) = profile::outline(section, &lookup) {
                let (lo, hi) =
                    o.iter().fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
                piece.section = ((hi - lo).x, (hi - lo).y);
                piece.round = section.len() == 2 && section.iter().all(|p| !p.radius.trim().is_empty());
            }
            (*plane, path, false)
        }
        ComponentShape::Blend { plane, bottom, top, .. } => {
            piece.kind = PieceKind::Blend;
            piece.old_top = top.clone();
            (*plane, bottom, true)
        }
        _ => return,
    };
    let Ok(vertices) = profile::vertices(outline, &lookup) else {
        state.message = t!("sketch-piece-unreadable");
        return;
    };
    let mut session = Session::new(Target::Piece, profile::to_curves(&vertices, closed));
    session.draw = Draw::Select;
    session.piece = Some(PieceSketch { view: Some(view_of(plane)), old: outline.clone(), ..piece });
    state.sketch = Some(session);
}

/// *Finish*: the piece from the curves, or the next step (a revolution's
/// axis, a blend's top); a sketch that does not make a profile stays open.
pub fn finish(state: &mut AppState) {
    let Some(mut session) = state.sketch.clone() else { return };
    let Some(mut piece) = session.piece.clone() else { return };
    let Some((_, def)) = crate::component_env::component(state) else { return };
    let Some(kind) = piece.view else {
        state.message = t!("sketch-empty");
        return;
    };
    // The curves must make a profile before anything else.
    if let Err(e) = profile::from_curves(&session.curves, piece.kind != PieceKind::Sweep) {
        state.message = crate::sketch_mode::error_text(&e);
        return;
    }
    match piece.kind {
        PieceKind::Revolution if piece.edit.is_none() && !piece.axis => {
            piece.axis = true;
            session.draw = Draw::Select;
            session.selected.clear();
            session.piece = Some(piece);
            state.sketch = Some(session);
            state.message = t!("comp-sketch-hint-axis");
            return;
        }
        PieceKind::Blend if piece.bottom.is_none() => {
            let scope = def.scope(&Default::default(), &[]);
            let lookup = |name: &str| scope.get(name);
            let top =
                profile::vertices(&piece.old_top, &lookup).map(|v| profile::to_curves(&v, true)).unwrap_or_default();
            piece.bottom = Some(std::mem::replace(&mut session.curves, top));
            session.selected.clear();
            session.picks.clear();
            session.draw = if piece.edit.is_some() { Draw::Select } else { Draw::Line };
            session.piece = Some(piece);
            state.sketch = Some(session);
            state.message = t!("comp-sketch-hint-top");
            return;
        }
        _ => {}
    }
    let view = cc_views::component_view(&def, &Default::default(), kind, crate::component_env::VIEW_SCALE);
    create(state, &def, &view, &session.curves, &piece, None);
}

/// Make (or change) the piece; `axis`: a new revolution's axis.
fn create(
    state: &mut AppState,
    def: &cc_core::ComponentDef,
    view: &ComponentView,
    curves: &[SketchCurve],
    piece: &PieceSketch,
    axis: Option<String>,
) {
    let Some(env) = state.component_env.as_ref() else { return };
    let type_ref = cc_engine::Ref::Id(env.type_id.0);
    let Some(kind) = piece.view else { return };
    // The drawn curves are a blend's top over the bottom drawn first, a sweep's path.
    let old = if piece.kind == PieceKind::Blend { &piece.old_top } else { &piece.old };
    let drawn = written(def, view, curves, piece.kind != PieceKind::Sweep, old);
    let bottom = piece.bottom.as_ref().map(|b| written(def, view, b, true, &piece.old)).transpose();
    let (drawn, bottom) = match (drawn, bottom) {
        (Ok(d), Ok(b)) => (d, b),
        (Err(e), _) | (_, Err(e)) => {
            state.message = e;
            return;
        }
    };
    let (points, path, top) = match piece.kind {
        PieceKind::Sweep => {
            let (w, h) = piece.section;
            (section_points(w, h, piece.round), Some(drawn), None)
        }
        PieceKind::Blend => (bottom.unwrap_or_default(), None, Some(drawn)),
        _ => (drawn, None, None),
    };
    let out = match piece.edit {
        Some(index) => {
            let command = ComponentPieceSet {
                type_ref,
                index,
                name: None,
                material: None,
                void: Some(piece.void),
                in_plan: None,
                points: Some(points),
                path,
                top,
                start: None,
                end: None,
                axis: None,
                angle: None,
            };
            crate::actions::run_command(state, command).map(|_| index)
        }
        None => {
            let name = match piece.kind {
                PieceKind::Extrusion => "extrusion",
                PieceKind::Revolution => "revolution",
                PieceKind::Sweep => "sweep",
                PieceKind::Blend => "blend",
            };
            let command = ComponentPieceAdd {
                type_ref,
                kind: Some(name.into()),
                plane: plane_name(kind).into(),
                points,
                path,
                top,
                start: None,
                end: (piece.kind == PieceKind::Blend).then_some(ExprArg::Number(0.3)),
                axis: axis.map(ExprArg::Text),
                angle: None,
                void: piece.void,
                name: None,
                material: None,
            };
            crate::actions::run_command(state, command).and_then(|o| o["index"].as_u64()).map(|i| i as usize)
        }
    };
    if let Some(index) = out {
        state.sketch = None;
        if let Some(env) = state.component_env.as_mut() {
            env.pick = Some(Pick::Piece(index));
        }
    }
}

/// The piece sketch in one of the environment's views: drawn in the view
/// it started in, snapping to the sketch's points and the reference planes.
#[allow(clippy::too_many_arguments)]
pub fn canvas(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    view: &ComponentView,
    kind: ComponentViewKind,
    xf: &Xf,
    tol: f64,
) {
    let Some(piece) = piece(state).cloned() else { return };
    if kind == ComponentViewKind::ThreeD || piece.view.is_some_and(|v| v != kind) {
        return;
    }
    let raw = resp.hover_pos().map(|p| xf.to_world(p));
    let faint = Stroke::new(1.0, PALETTE.accent.gamma_multiply(0.6));
    // A blend's bottom outline under its top.
    for c in piece.bottom.iter().flatten() {
        painter.add(egui::Shape::line(c.points().iter().map(|p| xf.to_screen(*p)).collect(), faint));
    }
    // A finished revolution outline waits for its axis: a vertical plane, or any vertical line.
    if piece.axis {
        let curves = state.sketch.as_ref().map(|s| s.curves.clone()).unwrap_or_default();
        for c in &curves {
            let pts = c.points().iter().map(|p| xf.to_screen(*p)).collect();
            painter.add(egui::Shape::line(pts, Stroke::new(1.5, PALETTE.accent)));
        }
        let Some(p) = raw else { return };
        let (at, _) = snap(view, p, tol);
        let (lo, hi) = view.bounds.unwrap_or((DVec2::splat(-1.0), DVec2::splat(1.0)));
        painter.line_segment(
            [xf.to_screen(DVec2::new(at.x, lo.y - 1.0)), xf.to_screen(DVec2::new(at.x, hi.y + 1.0))],
            faint,
        );
        if resp.clicked()
            && let Some((_, def)) = crate::component_env::component(state)
        {
            create(state, &def, view, &curves, &piece, Some(coordinate(view, at.x, true)));
        }
        return;
    }
    let Some(mut s) = state.sketch.take() else { return };
    // The sketch's own points first, then the planes and the millimeter grid.
    let snapped = raw.map(|r| {
        let own = s
            .snap_points()
            .into_iter()
            .filter(|q| q.distance(r) <= tol)
            .min_by(|a, b| a.distance(r).total_cmp(&b.distance(r)));
        match own {
            Some(q) => Snap::plain(q, true),
            None => {
                let (at, locked) = snap(view, r, tol);
                Snap::plain(at, locked)
            }
        }
    });
    let message = crate::sketch_mode::interact(ui, resp, painter, &mut s, &state.modify, raw, snapped, xf);
    // The first curve fixes the sketch's view.
    if !s.curves.is_empty()
        && let Some(p) = s.piece.as_mut()
        && p.view.is_none()
    {
        p.view = Some(kind);
    }
    state.sketch = Some(s);
    if let Some(m) = message {
        state.message = m;
    }
}

/// The piece's options in the options bar: solid or void, a sweep's
/// section, and what the sketch is waiting for.
pub fn options(ui: &mut Ui, state: &mut AppState) {
    let Some(piece) = state.sketch.as_mut().and_then(|s| s.piece.as_mut()) else { return };
    ui.selectable_value(&mut piece.void, false, t!("comp-sketch-solid"));
    ui.selectable_value(&mut piece.void, true, t!("comp-sketch-void"));
    ui.separator();
    if piece.kind == PieceKind::Sweep {
        ui.label(t!("comp-sketch-section"));
        ui.selectable_value(&mut piece.round, false, t!("comp-sketch-rect"));
        ui.selectable_value(&mut piece.round, true, t!("comp-sketch-circle"));
        ui.add(crate::properties::length_drag(&mut piece.section.0).range(0.001..=100.0))
            .on_hover_text(t!("comp-sketch-section-width"));
        if !piece.round {
            ui.label("×");
            ui.add(crate::properties::length_drag(&mut piece.section.1).range(0.001..=100.0))
                .on_hover_text(t!("comp-sketch-section-height"));
        }
        ui.separator();
    }
    let hint = if piece.axis {
        Some(t!("comp-sketch-hint-axis"))
    } else if piece.bottom.is_some() {
        Some(t!("comp-sketch-hint-top"))
    } else if piece.view.is_none() {
        Some(t!("comp-sketch-hint-view"))
    } else {
        None
    };
    if let Some(hint) = hint {
        ui.label(theme::muted(hint));
        ui.separator();
    }
}

#[cfg(test)]
pub(crate) fn finish_for_tests(state: &mut AppState) {
    finish(state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_views::component_view::ViewPlane;

    fn view() -> ComponentView {
        let plane = |name: &str, a: (f64, f64), b: (f64, f64)| ViewPlane {
            name: name.into(),
            a: DVec2::new(a.0, a.1),
            b: DVec2::new(b.0, b.1),
            origin: false,
        };
        ComponentView {
            planes: vec![plane("Left", (-0.6, -1.0), (-0.6, 1.0)), plane("Front", (-1.0, -0.4), (1.0, -0.4))],
            ..Default::default()
        }
    }

    #[test]
    fn points_lock_to_planes_within_reach() {
        let v = view();
        assert_eq!(snap(&v, DVec2::new(-0.603, -0.398), 0.01), (DVec2::new(-0.6, -0.4), true));
        assert_eq!(snap(&v, DVec2::new(0.12345, 0.398), 0.01), (DVec2::new(0.123, 0.398), false));
    }

    #[test]
    fn sweep_sections_are_centered_on_the_path() {
        let rect = section_points(0.1, 0.04, false);
        assert_eq!(rect.len(), 4);
        assert_eq!(rect[0].u, ExprArg::Text("-0.05".into()));
        assert_eq!(rect[2].v, ExprArg::Text("0.02".into()));
        let round = section_points(0.1, 0.0, true);
        assert_eq!(round.len(), 2);
        assert_eq!(round[1].radius, Some(ExprArg::Text("0.05".into())));
    }

    #[test]
    fn points_are_written_on_planes_from_centers_or_as_they_were() {
        let v = view();
        let circle = SketchCurve::Circle { center: DVec2::new(-0.6, 0.2), radius: 0.05 };
        let old = ProfilePoint::new("Left + W", "0.2");
        let writer = Writer { view: &v, rounds: vec![circle], old: vec![(old, DVec2::new(0.4, 0.2), Segment::Line)] };
        // On the circle: from its center, which is on the plane Left.
        let p = writer.point(DVec2::new(-0.65, 0.2), Segment::Arc { radius: 0.05 });
        assert_eq!((p.u, p.radius), (ExprArg::Text("Left - 0.05".into()), Some(ExprArg::Text("0.05".into()))));
        // On a plane; then a point that stayed put keeps its expression.
        let p = writer.point(DVec2::new(0.25, -0.4), Segment::Line);
        assert_eq!((p.u, p.v), (ExprArg::Text("0.25".into()), ExprArg::Text("Front".into())));
        let p = writer.point(DVec2::new(0.4, 0.2), Segment::Line);
        assert_eq!(p.u, ExprArg::Text("Left + W".into()));
        assert_eq!(offset("0.5", -0.25), "0.25");
    }
}
