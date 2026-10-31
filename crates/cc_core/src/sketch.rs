//! Sketches: the curves an outline is drawn with (lines, arcs, circles and
//! ellipses; rectangles and polygons are drawn as lines). A floor, ceiling,
//! roof, foundation slab or mass keeps the sketch it was drawn from, so its
//! boundary can be edited again curve by curve; its outline (the polygon the
//! geometry is built from) is the sketch's loops, arcs and ellipses split
//! into short chords.
//!
//! [`Sketch::profile`] chains the curves into closed loops: the one around
//! all the others is the outline, the others are openings in it. Ends meet
//! within [`JOIN_TOL`]; open ends, curves branching at a point, loops that
//! cross themselves or each other, and loops apart from or nested in each
//! other are errors, with the point where they happen.

use crate::arc::{ARC_STEP, Arc, segments};
use crate::transform::PlanTransform;
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

/// Ends closer than this (meters) join.
pub const JOIN_TOL: f64 = 1e-4;

/// Most curves a sketch holds (a guard against crafted files).
pub const MAX_CURVES: usize = 2000;

/// Most points the loops of a sketch make.
pub const MAX_POINTS: usize = 8192;

/// A curve of a sketch, in plan coordinates (meters).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SketchCurve {
    Line {
        a: DVec2,
        b: DVec2,
    },
    /// A circular arc from `a` to `b`, by its bulge (see [`crate::arc`]).
    Arc {
        a: DVec2,
        b: DVec2,
        bulge: f64,
    },
    Circle {
        center: DVec2,
        radius: f64,
    },
    /// An ellipse with semi-axes `radii` (along its own x and y), its x axis
    /// turned `rotation_deg` from the plan's; a part of it from the angle
    /// `start_deg` (of its parameter) turning by `sweep_deg`, or all of it
    /// when the sweep is a full turn.
    Ellipse {
        center: DVec2,
        radii: DVec2,
        rotation_deg: f64,
        #[serde(default)]
        start_deg: f64,
        #[serde(default = "full_turn")]
        sweep_deg: f64,
    },
}

fn full_turn() -> f64 {
    360.0
}

impl SketchCurve {
    /// A whole ellipse.
    pub fn ellipse(center: DVec2, radii: DVec2, rotation_deg: f64) -> Self {
        SketchCurve::Ellipse { center, radii, rotation_deg, start_deg: 0.0, sweep_deg: 360.0 }
    }

    /// Finite numbers and a size: lines and arcs with distinct ends, positive radii.
    pub fn is_valid(&self) -> bool {
        match *self {
            SketchCurve::Line { a, b } => a.is_finite() && b.is_finite() && a.distance(b) > JOIN_TOL,
            SketchCurve::Arc { a, b, bulge } => {
                a.is_finite() && b.is_finite() && bulge.is_finite() && a.distance(b) > JOIN_TOL
            }
            SketchCurve::Circle { center, radius } => center.is_finite() && radius.is_finite() && radius > JOIN_TOL,
            SketchCurve::Ellipse { center, radii, rotation_deg, start_deg, sweep_deg } => {
                center.is_finite()
                    && radii.is_finite()
                    && radii.min_element() > JOIN_TOL
                    && rotation_deg.is_finite()
                    && start_deg.is_finite()
                    && sweep_deg.is_finite()
                    && sweep_deg.abs() > 1e-6
            }
        }
    }

    /// Whether the curve closes on itself (a circle, a whole ellipse).
    pub fn is_closed(&self) -> bool {
        match *self {
            SketchCurve::Circle { .. } => true,
            SketchCurve::Ellipse { sweep_deg, .. } => sweep_deg.abs() >= 360.0 - 1e-9,
            _ => false,
        }
    }

    /// The start and end of an open curve.
    pub fn ends(&self) -> Option<(DVec2, DVec2)> {
        if self.is_closed() {
            return None;
        }
        match *self {
            SketchCurve::Line { a, b } | SketchCurve::Arc { a, b, .. } => Some((a, b)),
            SketchCurve::Ellipse { start_deg, sweep_deg, .. } => Some((
                self.ellipse_point(start_deg.to_radians()),
                self.ellipse_point((start_deg + sweep_deg).to_radians()),
            )),
            SketchCurve::Circle { .. } => None,
        }
    }

    /// The point of an ellipse at parameter angle `t` (radians).
    fn ellipse_point(&self, t: f64) -> DVec2 {
        match *self {
            SketchCurve::Ellipse { center, radii, rotation_deg, .. } => {
                center
                    + DVec2::from_angle(rotation_deg.to_radians())
                        .rotate(DVec2::new(radii.x * t.cos(), radii.y * t.sin()))
            }
            _ => DVec2::ZERO,
        }
    }

    /// The curve as points from its start to its end (closed curves end
    /// where they start), arcs and ellipses split every [`ARC_STEP`].
    pub fn points(&self) -> Vec<DVec2> {
        match *self {
            SketchCurve::Line { a, b } => vec![a, b],
            SketchCurve::Arc { a, b, bulge } => match Arc::from_bulge(a, b, bulge) {
                Some(arc) => {
                    let mut pts = arc.points(0.0, arc.length(), 0.0, ARC_STEP);
                    // Exact ends, so chained curves meet.
                    if let Some(last) = pts.last_mut() {
                        *last = b;
                    }
                    if let Some(first) = pts.first_mut() {
                        *first = a;
                    }
                    pts
                }
                None => vec![a, b],
            },
            SketchCurve::Circle { center, radius } => {
                let n = segments(TAU, ARC_STEP);
                (0..=n).map(|i| center + DVec2::from_angle(TAU * i as f64 / n as f64) * radius).collect()
            }
            SketchCurve::Ellipse { start_deg, sweep_deg, .. } => {
                let sweep = sweep_deg.clamp(-360.0, 360.0).to_radians();
                let n = segments(sweep, ARC_STEP);
                let t0 = start_deg.to_radians();
                (0..=n).map(|i| self.ellipse_point(t0 + sweep * i as f64 / n as f64)).collect()
            }
        }
    }

    /// The same curve run the other way.
    pub fn reversed(&self) -> Self {
        match *self {
            SketchCurve::Line { a, b } => SketchCurve::Line { a: b, b: a },
            SketchCurve::Arc { a, b, bulge } => SketchCurve::Arc { a: b, b: a, bulge: -bulge },
            SketchCurve::Ellipse { center, radii, rotation_deg, start_deg, sweep_deg } => SketchCurve::Ellipse {
                center,
                radii,
                rotation_deg,
                start_deg: start_deg + sweep_deg,
                sweep_deg: -sweep_deg,
            },
            c @ SketchCurve::Circle { .. } => c,
        }
    }

    /// Move, rotate or mirror the curve.
    pub fn transform(&mut self, t: &PlanTransform) {
        match self {
            SketchCurve::Line { a, b } => {
                *a = t.apply(*a);
                *b = t.apply(*b);
            }
            SketchCurve::Arc { a, b, bulge } => {
                *a = t.apply(*a);
                *b = t.apply(*b);
                // A mirror turns the arc the other way between the same ends.
                if t.mirror {
                    *bulge = -*bulge;
                }
            }
            SketchCurve::Circle { center, .. } => *center = t.apply(*center),
            SketchCurve::Ellipse { center, rotation_deg, start_deg, sweep_deg, .. } => {
                *center = t.apply(*center);
                *rotation_deg = t.apply_angle_deg(*rotation_deg);
                // Mirrored, the parameter runs the other way.
                if t.mirror {
                    *start_deg = -*start_deg;
                    *sweep_deg = -*sweep_deg;
                }
            }
        }
    }
}

/// Why a sketch makes no outline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SketchError {
    /// No curves.
    Empty,
    /// A curve with invalid numbers or no size.
    InvalidCurve(usize),
    /// Too many curves or points.
    TooLarge,
    /// A curve ends here without meeting another.
    OpenEnd(DVec2),
    /// More than two curves meet here.
    Branch(DVec2),
    /// Loops cross or touch here (themselves or each other).
    Crossing(DVec2),
    /// A loop lies outside the others (more than one outline), near here.
    Apart(DVec2),
    /// A loop lies inside an opening, near here.
    Nested(DVec2),
    /// The outline has openings where none are allowed, near here.
    Openings(DVec2),
    /// A loop encloses no area, near here.
    NoArea(DVec2),
}

impl SketchError {
    /// Translation key of the message.
    pub fn key(&self) -> &'static str {
        match self {
            SketchError::Empty => "sketch-empty",
            SketchError::InvalidCurve(_) => "sketch-invalid-curve",
            SketchError::TooLarge => "sketch-too-large",
            SketchError::OpenEnd(_) => "sketch-open-end",
            SketchError::Branch(_) => "sketch-branch",
            SketchError::Crossing(_) => "sketch-crossing",
            SketchError::Apart(_) => "sketch-apart",
            SketchError::Nested(_) => "sketch-nested",
            SketchError::Openings(_) => "sketch-openings",
            SketchError::NoArea(_) => "sketch-no-area",
        }
    }

    /// Where it happens, to show it.
    pub fn point(&self) -> Option<DVec2> {
        match *self {
            SketchError::OpenEnd(p)
            | SketchError::Branch(p)
            | SketchError::Crossing(p)
            | SketchError::Apart(p)
            | SketchError::Nested(p)
            | SketchError::Openings(p)
            | SketchError::NoArea(p) => Some(p),
            _ => None,
        }
    }
}

impl std::fmt::Display for SketchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let at = |p: &DVec2| format!(" at ({:.3}, {:.3})", p.x, p.y);
        match self {
            SketchError::Empty => write!(f, "the sketch has no curves"),
            SketchError::InvalidCurve(i) => write!(f, "curve {i} has invalid numbers or no size"),
            SketchError::TooLarge => write!(f, "the sketch has too many curves or points"),
            SketchError::OpenEnd(p) => write!(f, "a curve ends without meeting another{}", at(p)),
            SketchError::Branch(p) => write!(f, "more than two curves meet{}", at(p)),
            SketchError::Crossing(p) => write!(f, "the loops cross{}", at(p)),
            SketchError::Apart(p) => write!(f, "a loop lies outside the outline{}", at(p)),
            SketchError::Nested(p) => write!(f, "a loop lies inside an opening{}", at(p)),
            SketchError::Openings(p) => write!(f, "this outline takes no openings{}", at(p)),
            SketchError::NoArea(p) => write!(f, "a loop encloses no area{}", at(p)),
        }
    }
}

/// An outline and the openings in it, from a sketch.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Profile {
    pub outline: Vec<DVec2>,
    pub openings: Vec<Vec<DVec2>>,
}

/// The curves an outline is drawn with.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Sketch {
    pub curves: Vec<SketchCurve>,
}

impl Sketch {
    /// Lines around `outline` and each of `openings`.
    pub fn from_polygons(outline: &[DVec2], openings: &[Vec<DVec2>]) -> Self {
        let mut curves = Vec::new();
        for poly in std::iter::once(outline).chain(openings.iter().map(Vec::as_slice)) {
            let n = poly.len();
            for i in 0..n {
                let (Some(&a), Some(&b)) = (poly.get(i), poly.get((i + 1) % n)) else { continue };
                if a.distance(b) > JOIN_TOL {
                    curves.push(SketchCurve::Line { a, b });
                }
            }
        }
        Self { curves }
    }

    /// Move, rotate or mirror every curve.
    pub fn transform(&mut self, t: &PlanTransform) {
        self.curves.iter_mut().for_each(|c| c.transform(t));
    }

    /// Valid curves and not too many of them.
    pub fn is_valid(&self) -> bool {
        self.curves.len() <= MAX_CURVES && self.curves.iter().all(SketchCurve::is_valid)
    }

    /// The closed loops the curves make, each as points without the closing
    /// repeat.
    pub fn loops(&self) -> Result<Vec<Vec<DVec2>>, SketchError> {
        if self.curves.is_empty() {
            return Err(SketchError::Empty);
        }
        if self.curves.len() > MAX_CURVES {
            return Err(SketchError::TooLarge);
        }
        if let Some(i) = self.curves.iter().position(|c| !c.is_valid()) {
            return Err(SketchError::InvalidCurve(i));
        }
        let mut loops: Vec<Vec<DVec2>> = Vec::new();
        // Closed curves are loops of their own.
        for c in self.curves.iter().filter(|c| c.is_closed()) {
            let mut pts = c.points();
            pts.pop();
            loops.push(pts);
        }
        // Open curves meet at nodes: ends within the tolerance.
        let open: Vec<(SketchCurve, DVec2, DVec2)> =
            self.curves.iter().filter_map(|c| c.ends().map(|(a, b)| (*c, a, b))).collect();
        let mut nodes: Vec<DVec2> = Vec::new();
        let node_of = |p: DVec2, nodes: &mut Vec<DVec2>| match nodes.iter().position(|q| q.distance(p) <= JOIN_TOL) {
            Some(i) => i,
            None => {
                nodes.push(p);
                nodes.len() - 1
            }
        };
        let ends: Vec<(usize, usize)> =
            open.iter().map(|(_, a, b)| (node_of(*a, &mut nodes), node_of(*b, &mut nodes))).collect();
        let mut at: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
        for (i, (s, e)) in ends.iter().enumerate() {
            at[*s].push(i);
            at[*e].push(i);
        }
        for (n, curves) in at.iter().enumerate() {
            let p = nodes.get(n).copied().unwrap_or_default();
            match curves.len() {
                2 => {}
                0 | 1 => return Err(SketchError::OpenEnd(p)),
                _ => return Err(SketchError::Branch(p)),
            }
        }
        // Walk each loop from an unused curve.
        let mut used = vec![false; open.len()];
        for first in 0..open.len() {
            if used[first] {
                continue;
            }
            let mut pts: Vec<DVec2> = Vec::new();
            let (mut curve, mut from) = (first, ends[first].0);
            // Every curve is used once: the walk is bounded by their count.
            for _ in 0..open.len() {
                used[curve] = true;
                let (c, _, _) = open[curve];
                let (s, e) = ends[curve];
                let (c, next) = if s == from { (c, e) } else { (c.reversed(), s) };
                let mut cp = c.points();
                // The joint is written once: the next curve starts there.
                cp.pop();
                pts.extend(cp);
                from = next;
                let Some(&following) = at[from].iter().find(|&&k| k != curve) else { break };
                if used[following] {
                    break;
                }
                curve = following;
            }
            loops.push(pts);
        }
        if loops.iter().map(Vec::len).sum::<usize>() > MAX_POINTS {
            return Err(SketchError::TooLarge);
        }
        if let Some(p) = crossing(&loops) {
            return Err(SketchError::Crossing(p));
        }
        for l in &loops {
            if l.len() < 3 || area(l) < 1e-6 {
                return Err(SketchError::NoArea(l.first().copied().unwrap_or_default()));
            }
        }
        Ok(loops)
    }

    /// The outline (the loop around all the others) and the openings in it.
    pub fn profile(&self) -> Result<Profile, SketchError> {
        let mut loops = self.loops()?;
        // The largest loop is the only one that can hold all the others.
        let Some(outer) = (0..loops.len()).max_by(|&a, &b| area(&loops[a]).total_cmp(&area(&loops[b]))) else {
            return Err(SketchError::Empty);
        };
        let outline = loops.swap_remove(outer);
        for (i, l) in loops.iter().enumerate() {
            let p = l.first().copied().unwrap_or_default();
            if !contains(&outline, p) {
                return Err(SketchError::Apart(p));
            }
            if loops.iter().enumerate().any(|(j, other)| j != i && contains(other, p)) {
                return Err(SketchError::Nested(p));
            }
        }
        Ok(Profile { outline, openings: loops })
    }

    /// [`Self::profile`] for outlines that take no openings.
    pub fn outline(&self) -> Result<Vec<DVec2>, SketchError> {
        let p = self.profile()?;
        match p.openings.first().and_then(|o| o.first()) {
            Some(&at) => Err(SketchError::Openings(at)),
            None => Ok(p.outline),
        }
    }

    /// Whether the sketch still makes `outline` and `openings` (the same
    /// points, in order from any start and either way round): an outline
    /// edited without its sketch leaves it behind.
    pub fn makes(&self, outline: &[DVec2], openings: &[Vec<DVec2>]) -> bool {
        let Ok(p) = self.profile() else { return false };
        same_loop(&p.outline, outline)
            && p.openings.len() == openings.len()
            && p.openings.iter().all(|o| openings.iter().any(|q| same_loop(o, q)))
    }
}

/// Area of a closed polygon (either winding).
fn area(pts: &[DVec2]) -> f64 {
    let n = pts.len();
    (0..n).map(|i| pts[i].perp_dot(pts[(i + 1) % n])).sum::<f64>().abs() / 2.0
}

/// Whether `p` is inside polygon `poly` (even-odd rule).
fn contains(poly: &[DVec2], p: DVec2) -> bool {
    let n = poly.len();
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}

/// The same closed loop: equal points from some start, either way round.
fn same_loop(a: &[DVec2], b: &[DVec2]) -> bool {
    let n = a.len();
    if n != b.len() || n == 0 {
        return false;
    }
    let near = |p: DVec2, q: DVec2| p.distance(q) <= JOIN_TOL;
    let Some(start) = b.iter().position(|q| near(*q, a[0])) else { return false };
    let forward = (0..n).all(|i| near(a[i], b[(start + i) % n]));
    let backward = (0..n).all(|i| near(a[i], b[(start + n - i) % n]));
    forward || backward
}

/// A point where segments of the loops cross or touch, other than where
/// neighbors of one loop meet.
fn crossing(loops: &[Vec<DVec2>]) -> Option<DVec2> {
    struct Seg {
        a: DVec2,
        b: DVec2,
        lo: DVec2,
        hi: DVec2,
        lp: usize,
        i: usize,
        n: usize,
    }
    let segs: Vec<Seg> = loops
        .iter()
        .enumerate()
        .flat_map(|(lp, l)| {
            let n = l.len();
            (0..n).map(move |i| {
                let (a, b) = (l[i], l[(i + 1) % n]);
                Seg { a, b, lo: a.min(b), hi: a.max(b), lp, i, n }
            })
        })
        .collect();
    let mut order: Vec<usize> = (0..segs.len()).collect();
    order.sort_by(|&x, &y| segs[x].lo.x.total_cmp(&segs[y].lo.x));
    // Sweep along x: only segments whose x ranges overlap are compared.
    for (k, &i) in order.iter().enumerate() {
        let s = &segs[i];
        for &j in order.iter().skip(k + 1) {
            let t = &segs[j];
            if t.lo.x > s.hi.x + JOIN_TOL {
                break;
            }
            if t.lo.y > s.hi.y + JOIN_TOL || s.lo.y > t.hi.y + JOIN_TOL {
                continue;
            }
            let neighbors = s.lp == t.lp && ((s.i + 1) % s.n == t.i || (t.i + 1) % t.n == s.i);
            if let Some(p) = meet(s.a, s.b, t.a, t.b, neighbors) {
                return Some(p);
            }
        }
    }
    None
}

/// Where segments `a`-`b` and `c`-`d` cross or touch; `neighbors` share an
/// end, which does not count unless they overlap.
fn meet(a: DVec2, b: DVec2, c: DVec2, d: DVec2, neighbors: bool) -> Option<DVec2> {
    let r = b - a;
    let s = d - c;
    let den = r.perp_dot(s);
    if den.abs() < 1e-12 {
        // Parallel: overlapping collinear segments meet.
        if (c - a).perp_dot(r).abs() > 1e-9 * r.length().max(1.0) {
            return None;
        }
        let len2 = r.length_squared().max(1e-18);
        let (t0, t1) = ((c - a).dot(r) / len2, (d - a).dot(r) / len2);
        let (lo, hi) = (t0.min(t1).max(0.0), t0.max(t1).min(1.0));
        let overlap = (hi - lo) * r.length();
        return (overlap > JOIN_TOL || (!neighbors && hi >= lo)).then(|| a + r * (lo + hi) / 2.0);
    }
    let t = (c - a).perp_dot(s) / den;
    let u = (c - a).perp_dot(r) / den;
    let eps = JOIN_TOL / r.length().max(JOIN_TOL);
    let eps_u = JOIN_TOL / s.length().max(JOIN_TOL);
    if t < -eps || t > 1.0 + eps || u < -eps_u || u > 1.0 + eps_u {
        return None;
    }
    let p = a + r * t;
    // Neighbors meet at their shared end.
    if neighbors && (p.distance(a).min(p.distance(b)) <= JOIN_TOL) && (p.distance(c).min(p.distance(d)) <= JOIN_TOL) {
        return None;
    }
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(a: (f64, f64), b: (f64, f64)) -> SketchCurve {
        SketchCurve::Line { a: DVec2::new(a.0, a.1), b: DVec2::new(b.0, b.1) }
    }

    fn square(x: f64, y: f64, w: f64) -> Vec<SketchCurve> {
        vec![
            line((x, y), (x + w, y)),
            line((x + w, y), (x + w, y + w)),
            line((x + w, y + w), (x, y + w)),
            line((x, y + w), (x, y)),
        ]
    }

    #[test]
    fn lines_in_any_order_and_direction_close() {
        let mut curves = square(0.0, 0.0, 4.0);
        curves.swap(0, 2);
        curves[1] = curves[1].reversed();
        let p = Sketch { curves }.profile().unwrap();
        assert_eq!(p.outline.len(), 4);
        assert!((area(&p.outline) - 16.0).abs() < 1e-9);
        assert!(p.openings.is_empty());
    }

    #[test]
    fn arcs_and_circles_make_curved_loops() {
        // A 4 m wide stadium: two half circles of radius 2 between straight sides.
        let curves = vec![
            line((0.0, 0.0), (6.0, 0.0)),
            SketchCurve::Arc { a: DVec2::new(6.0, 0.0), b: DVec2::new(6.0, 4.0), bulge: 1.0 },
            line((6.0, 4.0), (0.0, 4.0)),
            SketchCurve::Arc { a: DVec2::new(0.0, 4.0), b: DVec2::new(0.0, 0.0), bulge: 1.0 },
            SketchCurve::Circle { center: DVec2::new(3.0, 2.0), radius: 1.0 },
        ];
        let p = Sketch { curves }.profile().unwrap();
        let expected = 24.0 + std::f64::consts::PI * 4.0;
        assert!((area(&p.outline) - expected).abs() < 0.05, "{}", area(&p.outline));
        assert_eq!(p.openings.len(), 1, "the circle is an opening");
        assert!((area(&p.openings[0]) - std::f64::consts::PI).abs() < 0.01);
    }

    #[test]
    fn ellipses_whole_and_half() {
        let whole = Sketch { curves: vec![SketchCurve::ellipse(DVec2::ZERO, DVec2::new(3.0, 2.0), 30.0)] };
        let p = whole.outline().unwrap();
        assert!((area(&p) - std::f64::consts::PI * 6.0).abs() < 0.05);
        // Half an ellipse closed by its long axis.
        let half = SketchCurve::Ellipse {
            center: DVec2::ZERO,
            radii: DVec2::new(3.0, 2.0),
            rotation_deg: 0.0,
            start_deg: 0.0,
            sweep_deg: 180.0,
        };
        let (a, b) = half.ends().unwrap();
        assert!(a.distance(DVec2::new(3.0, 0.0)) < 1e-9 && b.distance(DVec2::new(-3.0, 0.0)) < 1e-9);
        let p = Sketch { curves: vec![half, SketchCurve::Line { a: b, b: a }] }.outline().unwrap();
        assert!((area(&p) - std::f64::consts::PI * 3.0).abs() < 0.05);
    }

    #[test]
    fn errors_say_where() {
        let mut open = square(0.0, 0.0, 4.0);
        open.pop();
        let Err(SketchError::OpenEnd(p)) = Sketch { curves: open }.profile() else { panic!("open") };
        assert!(p.distance(DVec2::new(0.0, 4.0)) < 1e-9 || p.distance(DVec2::ZERO) < 1e-9, "{p}");
        let mut branch = square(0.0, 0.0, 4.0);
        branch.push(line((0.0, 0.0), (2.0, 2.0)));
        branch.push(line((2.0, 2.0), (4.0, 0.0)));
        assert!(matches!(Sketch { curves: branch }.profile(), Err(SketchError::Branch(_))));
        let bow = vec![
            line((0.0, 0.0), (4.0, 4.0)),
            line((4.0, 4.0), (4.0, 0.0)),
            line((4.0, 0.0), (0.0, 4.0)),
            line((0.0, 4.0), (0.0, 0.0)),
        ];
        assert_eq!(Sketch { curves: bow }.profile(), Err(SketchError::Crossing(DVec2::new(2.0, 2.0))));
        let mut apart = square(0.0, 0.0, 4.0);
        apart.extend(square(10.0, 0.0, 1.0));
        assert!(matches!(Sketch { curves: apart }.profile(), Err(SketchError::Apart(_))));
        let mut nested = square(0.0, 0.0, 10.0);
        nested.extend(square(2.0, 2.0, 6.0));
        nested.extend(square(4.0, 4.0, 1.0));
        assert!(matches!(Sketch { curves: nested.clone() }.profile(), Err(SketchError::Nested(_))));
        nested.truncate(8);
        assert!(matches!(Sketch { curves: nested }.outline(), Err(SketchError::Openings(_))));
        assert_eq!(Sketch::default().profile(), Err(SketchError::Empty));
        let bad = vec![SketchCurve::Circle { center: DVec2::ZERO, radius: f64::NAN }];
        assert_eq!(Sketch { curves: bad }.profile(), Err(SketchError::InvalidCurve(0)));
    }

    #[test]
    fn transforms_keep_the_shape() {
        let sketch = Sketch {
            curves: vec![
                line((0.0, 0.0), (4.0, 0.0)),
                SketchCurve::Arc { a: DVec2::new(4.0, 0.0), b: DVec2::new(0.0, 0.0), bulge: 0.5 },
                SketchCurve::Ellipse {
                    center: DVec2::new(2.0, 0.6),
                    radii: DVec2::new(0.5, 0.2),
                    rotation_deg: 20.0,
                    start_deg: 0.0,
                    sweep_deg: 360.0,
                },
            ],
        };
        let before = sketch.profile().unwrap();
        for t in [
            PlanTransform::translate(DVec2::new(3.0, -2.0)),
            PlanTransform::rotate_about(DVec2::new(1.0, 1.0), 0.7),
            PlanTransform::mirror_across(DVec2::new(0.0, 5.0), DVec2::new(1.0, 2.0)),
        ] {
            let mut s = sketch.clone();
            s.transform(&t);
            let after = s.profile().unwrap();
            let moved: Vec<DVec2> = before.outline.iter().map(|p| t.apply(*p)).collect();
            assert!(same_loop(&after.outline, &moved), "{t:?}");
            let hole: Vec<DVec2> = before.openings[0].iter().map(|p| t.apply(*p)).collect();
            assert!((area(&after.openings[0]) - area(&hole)).abs() < 1e-9);
            assert!(after.openings[0].iter().all(|p| hole.iter().any(|q| q.distance(*p) < 1e-9)), "{t:?}");
        }
    }

    #[test]
    fn makes_its_outline() {
        let s = Sketch { curves: square(0.0, 0.0, 4.0) };
        let p = s.profile().unwrap();
        let mut rotated = p.outline.clone();
        rotated.rotate_left(2);
        rotated.reverse();
        assert!(s.makes(&rotated, &[]));
        rotated[0].x += 0.1;
        assert!(!s.makes(&rotated, &[]));
        let back = Sketch::from_polygons(&p.outline, &[]);
        assert!(back.makes(&p.outline, &[]));
    }
}
