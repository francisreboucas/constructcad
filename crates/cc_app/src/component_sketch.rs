//! Sketching pieces in the component modeling environment: an outline
//! drawn in the plan, front or side view (lines with optional arcs, a
//! rectangle or a circle), extruded or revolved into a solid or void piece;
//! a path that a rectangular or round section is swept along; or two
//! outlines blended into each other.
//!
//! A point that lands on a reference plane seen edge-on is locked to it:
//! its coordinate is written as the plane's name, so the piece follows the
//! plane when a parameter moves it. Other coordinates are rounded to the
//! millimeter.

use crate::component_env::{EnvTool, Pick};
use crate::paint::Xf;
use crate::state::AppState;
use crate::t;
use crate::theme::{self, PALETTE};
use cc_engine::commands::component_piece::{ComponentPieceAdd, ExprArg, SketchPointArg};
use cc_views::{ComponentView, ComponentViewKind};
use egui::{Painter, Stroke, Ui};
use glam::DVec2;

/// How clicks make the outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SketchMode {
    /// Points one by one, closed by clicking the first again; a radius makes
    /// the next segment an arc.
    #[default]
    Line,
    /// Two opposite corners.
    Rect,
    /// The center, then a point on the circle.
    Circle,
}

/// What the outline becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PieceKind {
    #[default]
    Extrusion,
    /// Turned about an axis picked after the outline closes.
    Revolution,
    /// A section carried along an open path (the path is what is drawn).
    Sweep,
    /// A bottom outline, then a top one.
    Blend,
}

/// A point of the sketch: where it is and how its coordinates are written.
#[derive(Debug, Clone, PartialEq)]
pub struct SketchPt {
    pub pos: DVec2,
    pub u: String,
    pub v: String,
    /// The segment to the next point is an arc of this radius (empty: straight).
    pub radius: String,
}

/// The sketch being drawn.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sketch {
    pub mode: SketchMode,
    pub kind: PieceKind,
    pub void: bool,
    /// Radius of the next segment's arc (0: straight).
    pub arc_radius: f64,
    /// The view the sketch is drawn in (set by its first point).
    pub view: Option<ComponentViewKind>,
    pub points: Vec<SketchPt>,
    /// A closed outline of a revolution, waiting for its axis.
    pub closed: bool,
    /// A blend's bottom outline, once drawn.
    pub bottom: Vec<SketchPt>,
    /// A sweep's section: width (or diameter) and height, round or not.
    pub section: (f64, f64),
    pub round: bool,
}

impl Sketch {
    pub fn new(kind: PieceKind) -> Self {
        Self { kind, section: (0.05, 0.05), ..Self::default() }
    }

    /// The same tool, ready for the next outline.
    fn restart(&self) -> Self {
        Self { view: None, points: Vec::new(), closed: false, bottom: Vec::new(), ..self.clone() }
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
    let r = (v * 1000.0).round() / 1000.0;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r}")
}

/// `e + d`, folded into a number when `e` is one.
pub fn offset(e: &str, d: f64) -> String {
    if d.abs() < 1e-12 {
        return e.to_string();
    }
    if let Ok(v) = e.trim().parse::<f64>() {
        return mm(v + d);
    }
    if d > 0.0 { format!("{e} + {}", mm(d)) } else { format!("{e} - {}", mm(-d)) }
}

/// The point at `p` in `view`, locked to the planes whose lines pass within
/// `tol`: a vertical line locks u, a horizontal one v.
pub fn snap(view: &ComponentView, p: DVec2, tol: f64) -> SketchPt {
    let mut pos = p;
    let (mut u, mut v) = (None, None);
    let (mut du, mut dv) = (tol, tol);
    for plane in &view.planes {
        let vertical = (plane.b.x - plane.a.x).abs() < 1e-9;
        if vertical && (p.x - plane.a.x).abs() <= du {
            du = (p.x - plane.a.x).abs();
            pos.x = plane.a.x;
            u = Some(plane.name.clone());
        } else if !vertical && (p.y - plane.a.y).abs() <= dv {
            dv = (p.y - plane.a.y).abs();
            pos.y = plane.a.y;
            v = Some(plane.name.clone());
        }
    }
    let u = u.unwrap_or_else(|| {
        pos.x = (pos.x * 1000.0).round() / 1000.0;
        mm(pos.x)
    });
    let v = v.unwrap_or_else(|| {
        pos.y = (pos.y * 1000.0).round() / 1000.0;
        mm(pos.y)
    });
    SketchPt { pos, u, v, radius: String::new() }
}

/// The four corners of the rectangle with opposite corners `a` and `b`,
/// counter-clockwise.
pub fn rect_points(a: &SketchPt, b: &SketchPt) -> Vec<SketchPt> {
    let pt = |x: &SketchPt, y: &SketchPt| SketchPt {
        pos: DVec2::new(x.pos.x, y.pos.y),
        u: x.u.clone(),
        v: y.v.clone(),
        radius: String::new(),
    };
    let mut pts = vec![pt(a, a), pt(b, a), pt(b, b), pt(a, b)];
    // Counter-clockwise, so arcs bulge outwards.
    let area: f64 = (0..4).map(|i| pts[i].pos.perp_dot(pts[(i + 1) % 4].pos)).sum();
    if area < 0.0 {
        pts.reverse();
    }
    pts
}

/// A circle about `center` through a point `r` away: two half arcs.
pub fn circle_points(center: &SketchPt, r: f64) -> Vec<SketchPt> {
    let r = (r * 1000.0).round() / 1000.0;
    let radius = mm(r);
    [-1.0, 1.0]
        .iter()
        .map(|s| SketchPt {
            pos: center.pos + DVec2::new(s * r, 0.0),
            u: offset(&center.u, s * r),
            v: center.v.clone(),
            radius: radius.clone(),
        })
        .collect()
}

fn args(points: &[SketchPt]) -> Vec<SketchPointArg> {
    points.iter().map(|p| SketchPointArg::new(p.u.clone(), p.v.clone(), &p.radius)).collect()
}

fn plane_name(kind: ComponentViewKind) -> &'static str {
    match kind {
        ComponentViewKind::Front => "front",
        ComponentViewKind::Side => "side",
        _ => "plan",
    }
}

/// Make the piece of a closed sketch (`axis`: the revolution's axis). The
/// new piece is selected and the tool starts a new sketch of the same kind.
fn create(state: &mut AppState, sketch: &Sketch, axis: Option<String>) {
    let (Some(view), Some(env)) = (sketch.view, state.component_env.as_ref()) else { return };
    let kind = match sketch.kind {
        PieceKind::Extrusion => "extrusion",
        PieceKind::Revolution => "revolution",
        PieceKind::Sweep => "sweep",
        PieceKind::Blend => "blend",
    };
    // A sweep's drawn points are its path; a blend's are its top over the bottom drawn first.
    let (points, path, top, end) = match sketch.kind {
        PieceKind::Sweep => {
            let (w, h) = sketch.section;
            (section_points(w, h, sketch.round), Some(args(&sketch.points)), None, None)
        }
        PieceKind::Blend => (args(&sketch.bottom), None, Some(args(&sketch.points)), Some(ExprArg::Number(0.3))),
        _ => (args(&sketch.points), None, None, None),
    };
    let command = ComponentPieceAdd {
        type_ref: cc_engine::Ref::Id(env.type_id.0),
        kind: Some(kind.into()),
        plane: plane_name(view).into(),
        points,
        path,
        top,
        start: None,
        end,
        axis: axis.map(ExprArg::Text),
        angle: None,
        void: sketch.void,
        name: None,
        material: None,
    };
    let out = crate::actions::run_command(state, command);
    if let Some(env) = state.component_env.as_mut() {
        if let Some(i) = out.and_then(|o| o["index"].as_u64()) {
            env.pick = Some(Pick::Piece(i as usize));
        }
        env.tool = EnvTool::Sketch(sketch.restart());
    }
}

fn set(state: &mut AppState, sketch: Sketch) {
    if let Some(env) = state.component_env.as_mut() {
        env.tool = EnvTool::Sketch(sketch);
    }
}

/// The sketch tool in one view: preview, snapping and clicks.
#[allow(clippy::too_many_arguments)]
pub fn handle(
    state: &mut AppState,
    sketch: Sketch,
    view: &ComponentView,
    kind: ComponentViewKind,
    xf: &Xf,
    painter: &Painter,
    hover: Option<DVec2>,
    clicked: bool,
    tol: f64,
) {
    if kind == ComponentViewKind::ThreeD || sketch.view.is_some_and(|v| v != kind) {
        return;
    }
    let stroke = Stroke::new(1.5, PALETTE.accent);
    let line = |a: DVec2, b: DVec2| painter.line_segment([xf.to_screen(a), xf.to_screen(b)], stroke);
    let n = sketch.bottom.len();
    for i in 0..n {
        let (a, b) = (sketch.bottom[i].pos, sketch.bottom[(i + 1) % n].pos);
        painter.line_segment([xf.to_screen(a), xf.to_screen(b)], Stroke::new(1.0, PALETTE.accent.gamma_multiply(0.6)));
    }
    for w in sketch.points.windows(2) {
        line(w[0].pos, w[1].pos);
    }
    let Some(p) = hover else { return };
    // A closed revolution outline waits for its axis: a vertical plane, or any vertical line.
    if sketch.closed {
        if let (Some(first), Some(last)) = (sketch.points.first(), sketch.points.last()) {
            line(last.pos, first.pos);
        }
        let s = snap(view, p, tol);
        let (lo, hi) = view.bounds.unwrap_or((DVec2::splat(-1.0), DVec2::splat(1.0)));
        painter.line_segment(
            [xf.to_screen(DVec2::new(s.pos.x, lo.y)), xf.to_screen(DVec2::new(s.pos.x, hi.y))],
            Stroke::new(1.0, PALETTE.accent.gamma_multiply(0.6)),
        );
        if clicked {
            create(state, &sketch, Some(s.u));
        }
        return;
    }
    let s = snap(view, p, tol);
    // Locked coordinates show as a square, free ones as a circle.
    let at = xf.to_screen(s.pos);
    let locked = s.u.parse::<f64>().is_err() || s.v.parse::<f64>().is_err();
    if locked {
        painter.rect_stroke(
            egui::Rect::from_center_size(at, egui::vec2(9.0, 9.0)),
            0.0,
            stroke,
            egui::StrokeKind::Middle,
        );
    } else {
        painter.circle_stroke(at, 4.0, stroke);
    }
    let mut sketch = sketch;
    match sketch.mode {
        SketchMode::Line => {
            let near = |q: &SketchPt| xf.to_screen(q.pos).distance(at) < 8.0;
            let closes = if sketch.kind == PieceKind::Sweep {
                sketch.points.len() >= 2 && sketch.points.last().is_some_and(near)
            } else {
                sketch.points.len() >= 2 && sketch.points.first().is_some_and(near)
            };
            if let Some(last) = sketch.points.last() {
                line(last.pos, s.pos);
            }
            if !clicked {
                return;
            }
            if closes {
                finish(state, sketch);
                return;
            }
            if let Some(last) = sketch.points.last_mut()
                && sketch.arc_radius > 0.0
            {
                last.radius = mm(sketch.arc_radius);
            }
            sketch.view = Some(kind);
            sketch.points.push(s);
            set(state, sketch);
        }
        SketchMode::Rect => match sketch.points.first() {
            Some(a) => {
                let pts = rect_points(a, &s);
                for i in 0..4 {
                    line(pts[i].pos, pts[(i + 1) % 4].pos);
                }
                if clicked && (s.pos - a.pos).abs().min_element() > 1e-3 {
                    sketch.points = pts;
                    finish(state, sketch);
                }
            }
            None if clicked => {
                sketch.view = Some(kind);
                sketch.points.push(s);
                set(state, sketch);
            }
            None => {}
        },
        SketchMode::Circle => match sketch.points.first() {
            Some(c) => {
                let r = c.pos.distance(p);
                painter.circle_stroke(xf.to_screen(c.pos), (r * xf.scale) as f32, stroke);
                if clicked && r > 1e-3 {
                    sketch.points = circle_points(c, r);
                    finish(state, sketch);
                }
            }
            None if clicked => {
                sketch.view = Some(kind);
                sketch.points.push(s);
                set(state, sketch);
            }
            None => {}
        },
    }
}

/// A finished outline: an extrusion or sweep is made at once, a revolution
/// asks for its axis, a blend for its top outline after the bottom one.
fn finish(state: &mut AppState, sketch: Sketch) {
    match sketch.kind {
        PieceKind::Extrusion | PieceKind::Sweep => create(state, &sketch, None),
        PieceKind::Revolution => set(state, Sketch { closed: true, ..sketch }),
        PieceKind::Blend if sketch.bottom.is_empty() => {
            set(state, Sketch { bottom: sketch.points.clone(), points: Vec::new(), ..sketch });
        }
        PieceKind::Blend => create(state, &sketch, None),
    }
}

#[cfg(test)]
pub fn finish_for_tests(state: &mut AppState, sketch: Sketch) {
    finish(state, sketch);
}

/// The sketch's options: how clicks draw, solid or void, the arc radius.
pub fn options(ui: &mut Ui, state: &mut AppState) {
    let Some(EnvTool::Sketch(mut sketch)) = state.component_env.as_ref().map(|e| e.tool.clone()) else { return };
    let before = sketch.clone();
    if sketch.kind == PieceKind::Sweep {
        sketch.mode = SketchMode::Line;
    }
    ui.horizontal(|ui| {
        let modes: &[(SketchMode, &str)] = if sketch.kind == PieceKind::Sweep {
            &[]
        } else {
            &[
                (SketchMode::Line, "comp-sketch-line"),
                (SketchMode::Rect, "comp-sketch-rect"),
                (SketchMode::Circle, "comp-sketch-circle"),
            ]
        };
        for (mode, key) in modes.iter().copied() {
            if ui.selectable_label(sketch.mode == mode, crate::i18n::tr(key)).clicked() && sketch.mode != mode {
                sketch = Sketch { mode, ..sketch.restart() };
            }
        }
        if sketch.kind == PieceKind::Sweep {
            ui.label(t!("comp-sketch-section"));
            ui.selectable_value(&mut sketch.round, false, t!("comp-sketch-rect"));
            ui.selectable_value(&mut sketch.round, true, t!("comp-sketch-circle"));
            ui.add(crate::properties::length_drag(&mut sketch.section.0).range(0.001..=100.0))
                .on_hover_text(t!("comp-sketch-section-width"));
            if !sketch.round {
                ui.label("×");
                ui.add(crate::properties::length_drag(&mut sketch.section.1).range(0.001..=100.0))
                    .on_hover_text(t!("comp-sketch-section-height"));
            }
        }
        ui.separator();
        ui.selectable_value(&mut sketch.void, false, t!("comp-sketch-solid"));
        ui.selectable_value(&mut sketch.void, true, t!("comp-sketch-void"));
        if sketch.mode == SketchMode::Line {
            ui.separator();
            ui.label(t!("comp-sketch-arc"));
            ui.add(crate::properties::length_drag(&mut sketch.arc_radius).range(0.0..=1000.0))
                .on_hover_text(t!("comp-sketch-arc-tip"));
        }
        ui.separator();
        let hint = match (sketch.closed, sketch.mode, sketch.points.is_empty()) {
            (true, ..) => t!("comp-sketch-hint-axis"),
            _ if sketch.kind == PieceKind::Sweep => t!("comp-sketch-hint-path"),
            _ if sketch.kind == PieceKind::Blend && !sketch.bottom.is_empty() => t!("comp-sketch-hint-top"),
            (_, SketchMode::Line, true) => t!("comp-sketch-hint-line"),
            (_, SketchMode::Line, false) => t!("comp-sketch-hint-line-next"),
            (_, SketchMode::Rect, _) => t!("comp-sketch-hint-rect"),
            (_, SketchMode::Circle, _) => t!("comp-sketch-hint-circle"),
        };
        ui.label(theme::muted(hint));
    });
    if sketch != before {
        set(state, sketch);
    }
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
        let s = snap(&v, DVec2::new(-0.603, -0.398), 0.01);
        assert_eq!((s.u.as_str(), s.v.as_str()), ("Left", "Front"));
        assert_eq!(s.pos, DVec2::new(-0.6, -0.4));
        let s = snap(&v, DVec2::new(0.12345, -0.398), 0.01);
        assert_eq!((s.u.as_str(), s.v.as_str()), ("0.123", "Front"));
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
    fn rectangles_and_circles_keep_their_locks() {
        let v = view();
        let a = snap(&v, DVec2::new(-0.6, -0.4), 0.01);
        let b = snap(&v, DVec2::new(0.2, 0.3), 0.01);
        let r = rect_points(&b, &a);
        let us: Vec<&str> = r.iter().map(|p| p.u.as_str()).collect();
        let vs: Vec<&str> = r.iter().map(|p| p.v.as_str()).collect();
        assert!(us.contains(&"Left") && us.contains(&"0.2") && vs.contains(&"Front") && vs.contains(&"0.3"));
        // Counter-clockwise whichever corner came first.
        let area: f64 = (0..4).map(|i| r[i].pos.perp_dot(r[(i + 1) % 4].pos)).sum();
        assert!(area > 0.0);
        let c = circle_points(&a, 0.05);
        assert_eq!((c[0].u.as_str(), c[1].u.as_str()), ("Left - 0.05", "Left + 0.05"));
        assert_eq!(c[0].radius, "0.05");
        assert_eq!(offset("0.5", -0.25), "0.25");
    }
}
