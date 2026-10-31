//! Profiles of component pieces: closed outlines of straight segments,
//! arcs and parts of ellipses drawn on one of the component's planes (plan, front or side), with
//! every coordinate an [`crate::expr`] expression, and the solids made from
//! them: an extrusion across the plane or a revolution about an axis in it.
//!
//! A profile's plane is placed at `offset` along the axis normal to it (Z
//! for the plan, Y for the front, X for the side), and an extrusion runs
//! `depth` from there along that axis. The solids come out as closed meshes
//! with outward faces, in the component's frame (X to the right, Y to the
//! back, Z up).

use crate::arc::Arc;
use crate::expr::{self, ExprError};
use crate::sketch::{Sketch, SketchCurve, SketchError};
use crate::sketch_edit;
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

/// Arc segments per full turn.
const ARC_SEGMENTS: f64 = 32.0;
/// Segments of a full revolution.
const REVOLUTION_SEGMENTS: f64 = 48.0;
/// Distance under which a point lies on a revolution axis or two points coincide.
const EPS: f64 = 1e-9;

/// The plane a profile is drawn on, as seen in the matching view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SketchPlane {
    /// Seen from above: U along X, V along Y; placed and extruded along Z.
    #[default]
    Plan,
    /// Seen from the front (-Y): U along X, V along Z; placed and extruded along Y.
    Front,
    /// Seen from the right (+X): U along Y, V along Z; placed and extruded along X.
    Side,
}

impl SketchPlane {
    pub const ALL: [SketchPlane; 3] = [SketchPlane::Plan, SketchPlane::Front, SketchPlane::Side];

    pub fn key(self) -> &'static str {
        match self {
            SketchPlane::Plan => "comp-plane-plan",
            SketchPlane::Front => "comp-plane-front",
            SketchPlane::Side => "comp-plane-side",
        }
    }

    /// The point at `u`, `v` on the plane placed at `w` on its normal axis.
    pub fn point(self, u: f64, v: f64, w: f64) -> DVec3 {
        match self {
            SketchPlane::Plan => DVec3::new(u, v, w),
            SketchPlane::Front => DVec3::new(u, w, v),
            SketchPlane::Side => DVec3::new(w, u, v),
        }
    }

    /// World directions of U, V and the extrusion (the plane's normal axis).
    fn axes(self) -> (DVec3, DVec3, DVec3) {
        match self {
            SketchPlane::Plan => (DVec3::X, DVec3::Y, DVec3::Z),
            SketchPlane::Front => (DVec3::X, DVec3::Z, DVec3::Y),
            SketchPlane::Side => (DVec3::Y, DVec3::Z, DVec3::X),
        }
    }
}

/// A vertex of a profile. The segment to the next vertex is straight, an
/// arc of `radius` when it is not empty (a positive radius bulges to the
/// right of the direction of travel, outwards in a counter-clockwise
/// profile; a negative one to the left), or a part of `ellipse`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProfilePoint {
    pub u: String,
    pub v: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub radius: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ellipse: Option<ProfileEllipse>,
}

/// The ellipse a profile segment runs along, from its vertex to the next:
/// center (`u`, `v`), semi-axes `rx` and `ry` along its own axes, turned
/// `rotation` degrees; counter-clockwise unless `clockwise`. The vertices
/// should lie on it (each is taken at the ellipse's parameter of its
/// direction from the center).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProfileEllipse {
    pub u: String,
    pub v: String,
    pub rx: String,
    pub ry: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rotation: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clockwise: bool,
}

impl ProfilePoint {
    pub fn new(u: impl Into<String>, v: impl Into<String>) -> Self {
        Self { u: u.into(), v: v.into(), ..Self::default() }
    }

    pub fn arc(u: impl Into<String>, v: impl Into<String>, radius: impl Into<String>) -> Self {
        Self { u: u.into(), v: v.into(), radius: radius.into(), ellipse: None }
    }

    /// Every expression of the point, with a field label (an arc's radius
    /// and an ellipse's rotation only when set).
    pub fn expressions(&self) -> Vec<(&'static str, &String)> {
        let mut out = vec![("u", &self.u), ("v", &self.v)];
        if !self.radius.trim().is_empty() {
            out.push(("radius", &self.radius));
        }
        if let Some(e) = &self.ellipse {
            out.extend([("u", &e.u), ("v", &e.v), ("rx", &e.rx), ("ry", &e.ry)]);
            if !e.rotation.trim().is_empty() {
                out.push(("rotation", &e.rotation));
            }
        }
        out
    }

    pub fn expressions_mut(&mut self) -> Vec<&mut String> {
        let mut out = vec![&mut self.u, &mut self.v, &mut self.radius];
        if let Some(e) = &mut self.ellipse {
            out.extend([&mut e.u, &mut e.v, &mut e.rx, &mut e.ry, &mut e.rotation]);
        }
        out
    }
}

/// The segment from a profile's vertex to the next, in numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    Line,
    /// An arc of `radius` (signed like [`ProfilePoint::radius`]).
    Arc {
        radius: f64,
    },
    Ellipse {
        center: DVec2,
        radii: DVec2,
        rotation_deg: f64,
        clockwise: bool,
    },
}

/// The vertices of `points` and the segments from each to the next, in numbers.
pub fn vertices(
    points: &[ProfilePoint],
    lookup: &dyn Fn(&str) -> Option<f64>,
) -> Result<Vec<(DVec2, Segment)>, ProfileError> {
    let ev =
        |field: &'static str, text: &str| expr::eval(text, lookup).map_err(|error| ProfileError::Expr { field, error });
    let mut out = Vec::with_capacity(points.len());
    for p in points {
        let segment = match (&p.ellipse, p.radius.trim().is_empty()) {
            (Some(e), _) => {
                let rotation_deg = if e.rotation.trim().is_empty() { 0.0 } else { ev("rotation", &e.rotation)? };
                Segment::Ellipse {
                    center: DVec2::new(ev("u", &e.u)?, ev("v", &e.v)?),
                    radii: DVec2::new(ev("rx", &e.rx)?, ev("ry", &e.ry)?),
                    rotation_deg,
                    clockwise: e.clockwise,
                }
            }
            (None, false) => Segment::Arc { radius: ev("radius", &p.radius)? },
            (None, true) => Segment::Line,
        };
        out.push((DVec2::new(ev("u", &p.u)?, ev("v", &p.v)?), segment));
    }
    Ok(out)
}

/// The inner points of the segment from `a` to `b`.
pub(crate) fn segment_points(a: DVec2, b: DVec2, segment: Segment, out: &mut Vec<DVec2>) -> Result<(), ProfileError> {
    match segment {
        Segment::Line => Ok(()),
        Segment::Arc { radius } => arc_points(a, b, radius, out),
        Segment::Ellipse { center, radii, rotation_deg, clockwise } => {
            let turn = DVec2::from_angle(rotation_deg.to_radians());
            if !(center.is_finite() && radii.is_finite() && turn.is_finite()) || radii.min_element() <= EPS {
                return Err(ProfileError::Degenerate);
            }
            let param = |p: DVec2| {
                let local = DVec2::new(turn.x, -turn.y).rotate(p - center) / radii;
                local.y.atan2(local.x)
            };
            let t0 = param(a);
            let mut sweep = (param(b) - t0).rem_euclid(TAU);
            if clockwise {
                sweep -= TAU;
            }
            let steps = ((sweep.abs() / TAU) * ARC_SEGMENTS).ceil().max(2.0) as usize;
            for k in 1..steps {
                let t = t0 + sweep * k as f64 / steps as f64;
                out.push(center + turn.rotate(DVec2::new(radii.x * t.cos(), radii.y * t.sin())));
            }
            Ok(())
        }
    }
}

/// Why a profile could not be resolved.
#[derive(Debug, Clone, PartialEq)]
pub enum ProfileError {
    Expr {
        field: &'static str,
        error: ExprError,
    },
    /// Fewer than 3 distinct points, no area, an arc shorter than its chord,
    /// a zero size or a profile crossing its revolution axis.
    Degenerate,
}

/// The outline of `points` in plane coordinates, arcs divided into segments,
/// counter-clockwise.
pub fn outline(points: &[ProfilePoint], lookup: &dyn Fn(&str) -> Option<f64>) -> Result<Vec<DVec2>, ProfileError> {
    let corners = vertices(points, lookup)?;
    let n = corners.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (a, segment) = corners[i];
        let b = corners[(i + 1) % n].0;
        out.push(a);
        segment_points(a, b, segment, &mut out)?;
    }
    out.dedup_by(|a, b| a.distance(*b) < EPS);
    while out.len() > 1 && out.first().zip(out.last()).is_some_and(|(f, l)| f.distance(*l) < EPS) {
        out.pop();
    }
    let area = signed_area(&out);
    if out.len() < 3 || area.abs() < 1e-12 || !area.is_finite() {
        return Err(ProfileError::Degenerate);
    }
    if area < 0.0 {
        out.reverse();
    }
    Ok(out)
}

fn arc_points(a: DVec2, b: DVec2, radius: f64, out: &mut Vec<DVec2>) -> Result<(), ProfileError> {
    let chord = a.distance(b);
    let r = radius.abs();
    if chord < EPS {
        return Ok(());
    }
    if !(r.is_finite() && r >= chord / 2.0 - 1e-9) {
        return Err(ProfileError::Degenerate);
    }
    let half = ((chord / 2.0) / r).clamp(-1.0, 1.0).asin();
    let dir = (b - a) / chord;
    let left = dir.perp();
    // Bulging right, the center is on the left and the arc turns counter-clockwise.
    let sign = radius.signum();
    let center = (a + b) * 0.5 + left * (sign * r * half.cos());
    let start = (a - center).to_angle();
    let sweep = 2.0 * half * sign;
    let steps = ((2.0 * half / TAU) * ARC_SEGMENTS).ceil().max(2.0) as usize;
    for k in 1..steps {
        let t = start + sweep * k as f64 / steps as f64;
        out.push(center + DVec2::from_angle(t) * r);
    }
    Ok(())
}

fn signed_area(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| poly[i].perp_dot(poly[(i + 1) % n])).sum::<f64>() / 2.0
}

/// The closed solid of a counter-clockwise `outline` on `plane`, from `offset`
/// along the plane's normal to `offset + depth`.
pub fn extrude(plane: SketchPlane, outline: &[DVec2], offset: f64, depth: f64) -> Vec<Vec<DVec3>> {
    let (u, v, d) = plane.axes();
    let at = |p: DVec2, w: f64| plane.point(p.x, p.y, w);
    let near: Vec<DVec3> = outline.iter().map(|p| at(*p, offset)).collect();
    let far: Vec<DVec3> = outline.iter().map(|p| at(*p, offset + depth)).collect();
    // A counter-clockwise outline faces U x V; the near cap faces against the extrusion.
    let along = u.cross(v).dot(d) > 0.0;
    let mut faces = Vec::with_capacity(outline.len() + 2);
    let reversed = |f: &[DVec3]| f.iter().rev().copied().collect::<Vec<_>>();
    faces.push(if along { reversed(&near) } else { near.clone() });
    faces.push(if along { far.clone() } else { reversed(&far) });
    let n = outline.len();
    for i in 0..n {
        let j = (i + 1) % n;
        let quad = vec![near[i], near[j], far[j], far[i]];
        faces.push(if along { quad } else { reversed(&quad) });
    }
    faces
}

/// The solid swept by a counter-clockwise `outline` on `plane` (placed
/// `offset` along its normal) turning `angle` degrees about the line
/// `u = axis` of the plane (parallel to V). The outline must lie on one side
/// of the axis; it may touch it.
pub fn revolve(
    plane: SketchPlane,
    outline: &[DVec2],
    offset: f64,
    axis: f64,
    angle: f64,
) -> Result<Vec<Vec<DVec3>>, ProfileError> {
    let (min_u, max_u) =
        outline.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.x), hi.max(p.x)));
    // Radial distances from the axis, on the side the outline lies.
    let side = if min_u >= axis - 1e-9 {
        1.0
    } else if max_u <= axis + 1e-9 {
        -1.0
    } else {
        return Err(ProfileError::Degenerate);
    };
    let sweep = angle.to_radians();
    if !(sweep.abs() > 1e-6 && sweep.is_finite()) {
        return Err(ProfileError::Degenerate);
    }
    let sweep = sweep.clamp(-TAU, TAU);
    let full = (sweep.abs() - TAU).abs() < 1e-9;
    let (u, v, _) = plane.axes();
    let radial = u * side;
    // Turning the radial direction towards `tangent` keeps the faces of a
    // counter-clockwise (radius, height) outline outwards.
    let tangent = radial.cross(v) * sweep.signum();
    let base = plane.point(axis, 0.0, offset);
    // A mirrored outline (on the negative side) runs clockwise in (radius, height).
    let mut profile: Vec<DVec2> = outline.iter().map(|p| DVec2::new((p.x - axis) * side, p.y)).collect();
    if signed_area(&profile) < 0.0 {
        profile.reverse();
    }
    if sweep < 0.0 {
        profile.reverse();
    }
    let steps = ((sweep.abs() / TAU) * REVOLUTION_SEGMENTS).ceil().max(3.0) as usize;
    let at = |p: DVec2, k: usize| {
        let t = sweep.abs() * k as f64 / steps as f64;
        base + (radial * t.cos() + tangent * t.sin()) * p.x.max(0.0) + v * p.y
    };
    let n = profile.len();
    let mut faces = Vec::with_capacity(n * steps + 2);
    for i in 0..n {
        let (a, b) = (profile[i], profile[(i + 1) % n]);
        if a.x <= 1e-9 && b.x <= 1e-9 {
            continue;
        }
        for k in 0..steps {
            let next = if full && k + 1 == steps { 0 } else { k + 1 };
            let mut face = vec![at(a, k), at(b, k), at(b, next), at(a, next)];
            face.dedup_by(|p, q| p.distance(*q) < EPS);
            if face.len() >= 3 {
                faces.push(face);
            }
        }
    }
    if !full {
        // The outline itself closes both ends: against the turn at the start, along it at the end.
        faces.push((0..n).rev().map(|i| at(profile[i], 0)).collect());
        faces.push((0..n).map(|i| at(profile[i], steps)).collect());
    }
    Ok(faces)
}

/// The point of an ellipse at the parameter angle `t` (radians).
fn ellipse_at(center: DVec2, radii: DVec2, rotation_deg: f64, t: f64) -> DVec2 {
    center + DVec2::from_angle(rotation_deg.to_radians()).rotate(DVec2::new(radii.x * t.cos(), radii.y * t.sin()))
}

/// The vertices and segments that draw the sketch `curves`: their one
/// closed loop when `closed`, or else the open path they make (its last
/// vertex ends it, with a straight segment that is not drawn). Arcs over
/// half a turn split in two, circles and whole ellipses into halves.
pub fn from_curves(curves: &[SketchCurve], closed: bool) -> Result<Vec<(DVec2, Segment)>, SketchError> {
    if closed {
        Sketch { curves: curves.to_vec() }.outline()?;
    }
    let Some(first) = curves.first() else { return Err(SketchError::Empty) };
    if let Some(i) = curves.iter().position(|c| !c.is_valid()) {
        return Err(SketchError::InvalidCurve(i));
    }
    let (links, _) = sketch_edit::chain(curves, 0);
    if let Some(apart) = (0..curves.len()).find(|i| !links.iter().any(|(k, _)| k == i)) {
        let at = curves.get(apart).and_then(|c| c.points().first().copied()).unwrap_or(DVec2::ZERO);
        return Err(SketchError::Apart(at));
    }
    let mut out = Vec::with_capacity(curves.len() + 2);
    for &(k, rev) in &links {
        let Some(c) = curves.get(k) else { continue };
        let c = if rev { c.reversed() } else { *c };
        match c {
            SketchCurve::Line { a, .. } => out.push((a, Segment::Line)),
            SketchCurve::Arc { a, b, bulge } => match Arc::from_bulge(a, b, bulge) {
                Some(arc) => {
                    let radius = arc.radius * arc.sign();
                    out.push((a, Segment::Arc { radius }));
                    // A radius draws the shorter arc: a longer one goes in two halves.
                    if arc.sweep.abs() > std::f64::consts::PI + 1e-9 {
                        out.push((arc.point(arc.length() / 2.0), Segment::Arc { radius }));
                    }
                }
                None => out.push((a, Segment::Line)),
            },
            SketchCurve::Circle { center, radius } => {
                out.push((center - DVec2::X * radius, Segment::Arc { radius }));
                out.push((center + DVec2::X * radius, Segment::Arc { radius }));
            }
            SketchCurve::Ellipse { center, radii, rotation_deg, start_deg, sweep_deg } => {
                let segment = Segment::Ellipse { center, radii, rotation_deg, clockwise: sweep_deg < 0.0 };
                let at = |deg: f64| ellipse_at(center, radii, rotation_deg, deg.to_radians());
                out.push((at(start_deg), segment));
                // Whole, or more than half of it: the segment needs a vertex on the way.
                if sweep_deg.abs() > 180.0 + 1e-9 {
                    out.push((at(start_deg + sweep_deg / 2.0), segment));
                }
            }
        }
    }
    if !closed && let Some(&(k, rev)) = links.last() {
        let end = curves.get(k).and_then(SketchCurve::ends).map(|(a, b)| if rev { a } else { b });
        out.push((end.unwrap_or(first.points().last().copied().unwrap_or(DVec2::ZERO)), Segment::Line));
    }
    Ok(out)
}

/// The sketch curves that draw `vertices` (the segment from the last vertex
/// back to the first only when `closed`).
pub fn to_curves(vertices: &[(DVec2, Segment)], closed: bool) -> Vec<SketchCurve> {
    let n = vertices.len();
    let segments = if closed { n } else { n.saturating_sub(1) };
    (0..segments)
        .filter_map(|i| {
            let (a, segment) = *vertices.get(i)?;
            let b = vertices.get((i + 1) % n)?.0;
            let curve = match segment {
                Segment::Line => SketchCurve::Line { a, b },
                Segment::Arc { radius } => match Arc::bulge_radius(a, b, radius.abs(), radius > 0.0, false) {
                    Some(bulge) => SketchCurve::Arc { a, b, bulge },
                    None => SketchCurve::Line { a, b },
                },
                Segment::Ellipse { center, radii, rotation_deg, clockwise } => {
                    let turn = DVec2::from_angle(rotation_deg.to_radians());
                    let param = |p: DVec2| {
                        let local = DVec2::new(turn.x, -turn.y).rotate(p - center) / radii;
                        local.y.atan2(local.x)
                    };
                    let t0 = param(a);
                    let mut sweep = (param(b) - t0).rem_euclid(TAU);
                    if clockwise {
                        sweep -= TAU;
                    }
                    SketchCurve::Ellipse {
                        center,
                        radii,
                        rotation_deg,
                        start_deg: t0.to_degrees(),
                        sweep_deg: sweep.to_degrees(),
                    }
                }
            };
            curve.is_valid().then_some(curve)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Profile points that write `vertices` as plain numbers.
    fn written(vertices: &[(DVec2, Segment)]) -> Vec<ProfilePoint> {
        let n = |v: f64| format!("{v}");
        vertices
            .iter()
            .map(|(p, s)| match *s {
                Segment::Line => ProfilePoint::new(n(p.x), n(p.y)),
                Segment::Arc { radius } => ProfilePoint::arc(n(p.x), n(p.y), n(radius)),
                Segment::Ellipse { center, radii, rotation_deg, clockwise } => ProfilePoint {
                    ellipse: Some(ProfileEllipse {
                        u: n(center.x),
                        v: n(center.y),
                        rx: n(radii.x),
                        ry: n(radii.y),
                        rotation: n(rotation_deg),
                        clockwise,
                    }),
                    ..ProfilePoint::new(n(p.x), n(p.y))
                },
            })
            .collect()
    }

    fn area_of(points: &[ProfilePoint]) -> f64 {
        signed_area(&outline(points, &none).unwrap())
    }

    #[test]
    fn sketched_curves_become_profile_points_and_back() {
        use std::f64::consts::PI;
        let p = DVec2::new;
        // A circle: two half arcs, the circle's area.
        let circle = from_curves(&[SketchCurve::Circle { center: p(1.0, 1.0), radius: 0.5 }], true).unwrap();
        assert_eq!(circle.len(), 2);
        assert!((area_of(&written(&circle)) - PI * 0.25).abs() < 0.01);
        // A whole ellipse, turned: its area.
        let e = SketchCurve::ellipse(p(0.0, 0.0), p(2.0, 1.0), 30.0);
        let halves = from_curves(&[e], true).unwrap();
        assert!((area_of(&written(&halves)) / (PI * 2.0) - 1.0).abs() < 0.01);
        // Half an ellipse closed by its diameter, run clockwise: half the area.
        let half = SketchCurve::Ellipse {
            center: p(0.0, 0.0),
            radii: p(2.0, 1.0),
            rotation_deg: 0.0,
            start_deg: 180.0,
            sweep_deg: -180.0,
        };
        let loop_ = [half, SketchCurve::Line { a: p(2.0, 0.0), b: p(-2.0, 0.0) }];
        let v = from_curves(&loop_, true).unwrap();
        assert!((area_of(&written(&v)) / PI - 1.0).abs() < 0.01, "{v:?}");
        let back = to_curves(&v, true);
        assert!(matches!(back[0], SketchCurve::Ellipse { sweep_deg, .. } if (sweep_deg + 180.0).abs() < 1e-9));
        // A three-quarter arc closed by a line: split in two, the area kept.
        let arc = SketchCurve::Arc { a: p(1.0, 0.0), b: p(0.0, -1.0), bulge: (3.0 * PI / 8.0).tan() };
        let loop_ = [arc, SketchCurve::Line { a: p(0.0, -1.0), b: p(1.0, 0.0) }];
        let v = from_curves(&loop_, true).unwrap();
        assert_eq!(v.len(), 3);
        let expected = 0.75 * PI + 0.5;
        assert!((area_of(&written(&v)) / expected - 1.0).abs() < 0.01);
        let back = Sketch { curves: to_curves(&v, true) };
        assert!((crate::mass::area(&back.outline().unwrap()) / expected - 1.0).abs() < 0.01);
        // An open path keeps its last point; curves apart are refused.
        let path = [
            SketchCurve::Line { a: p(0.0, 0.0), b: p(1.0, 0.0) },
            SketchCurve::Line { a: p(1.0, 0.0), b: p(1.0, 1.0) },
        ];
        let v = from_curves(&path, false).unwrap();
        assert_eq!(v.iter().map(|x| x.0).collect::<Vec<_>>(), vec![p(0.0, 0.0), p(1.0, 0.0), p(1.0, 1.0)]);
        let apart = [path[0], SketchCurve::Line { a: p(5.0, 0.0), b: p(6.0, 0.0) }];
        assert!(matches!(from_curves(&apart, false), Err(SketchError::Apart(_))));
        // Points with an ellipse keep it through serialization, and older files load.
        let points = written(&halves);
        let ron = ron::to_string(&points).unwrap();
        assert_eq!(ron::from_str::<Vec<ProfilePoint>>(&ron).unwrap(), points);
        let old: ProfilePoint = ron::from_str("(u: \"1\", v: \"2\")").unwrap();
        assert_eq!(old, ProfilePoint::new("1", "2"));
    }

    fn volume(faces: &[Vec<DVec3>]) -> f64 {
        faces.iter().map(|f| (1..f.len() - 1).map(|i| f[0].dot(f[i].cross(f[i + 1]))).sum::<f64>()).sum::<f64>() / 6.0
    }

    fn none(_: &str) -> Option<f64> {
        None
    }

    fn square(size: &str) -> Vec<ProfilePoint> {
        vec![
            ProfilePoint::new("0", "0"),
            ProfilePoint::new(size, "0"),
            ProfilePoint::new(size, size),
            ProfilePoint::new("0", size),
        ]
    }

    #[test]
    fn extrusions_on_every_plane_are_closed_and_outward() {
        let outline = outline(&square("2"), &none).unwrap();
        for plane in SketchPlane::ALL {
            let faces = extrude(plane, &outline, 0.5, 3.0);
            assert!((volume(&faces) - 12.0).abs() < 1e-9, "{plane:?}: {}", volume(&faces));
        }
        // Front: U is X, V is Z, extruded to the back from y = 0.5.
        let faces = extrude(SketchPlane::Front, &outline, 0.5, 3.0);
        let all: Vec<DVec3> = faces.iter().flatten().copied().collect();
        let max = all.iter().fold(DVec3::splat(f64::NEG_INFINITY), |m, p| m.max(*p));
        assert_eq!(max, DVec3::new(2.0, 3.5, 2.0));
        // Side: U is Y, placed and extruded along X.
        let faces = extrude(SketchPlane::Side, &outline, -1.5, 3.0);
        let min = faces.iter().flatten().fold(DVec3::splat(f64::INFINITY), |m, p| m.min(*p));
        let max = faces.iter().flatten().fold(DVec3::splat(f64::NEG_INFINITY), |m, p| m.max(*p));
        assert_eq!((min, max), (DVec3::new(-1.5, 0.0, 0.0), DVec3::new(1.5, 2.0, 2.0)));
        // A profile written clockwise comes out the same.
        let mut cw = square("2");
        cw.reverse();
        assert_eq!(super::outline(&cw, &none).unwrap().len(), 4);
    }

    #[test]
    fn arcs_bulge_by_the_sign_of_their_radius() {
        // A 2 x 1 rectangle whose top edge (right to left) is a half circle bulging up.
        let mut pts = vec![
            ProfilePoint::new("0", "0"),
            ProfilePoint::new("2", "0"),
            ProfilePoint::arc("2", "1", "1"),
            ProfilePoint::new("0", "1"),
        ];
        let o = outline(&pts, &none).unwrap();
        let area = signed_area(&o);
        let half_disc = std::f64::consts::PI / 2.0;
        // The polygon of the arc falls a little short of the circle.
        assert!(area < 2.0 + half_disc && area > 2.0 + half_disc * 0.98, "{area}");
        assert!(o.iter().any(|p| (p.y - 2.0).abs() < 1e-9));
        // A negative radius cuts the half circle out instead.
        pts[2].radius = "-1".into();
        let area = signed_area(&outline(&pts, &none).unwrap());
        assert!(area > 2.0 - half_disc && area < 2.0 - half_disc * 0.98, "{area}");
        // A radius shorter than half the chord cannot join the points.
        pts[2].radius = "0.5".into();
        assert_eq!(outline(&pts, &none), Err(ProfileError::Degenerate));
        pts[2].radius = "R".into();
        assert!(matches!(outline(&pts, &none), Err(ProfileError::Expr { field: "radius", .. })));
    }

    #[test]
    fn revolutions_make_closed_solids_of_the_right_volume() {
        // A 1 x 2 rectangle touching the axis: a cylinder of radius 1, height 2.
        let rect = vec![
            ProfilePoint::new("0", "0"),
            ProfilePoint::new("1", "0"),
            ProfilePoint::new("1", "2"),
            ProfilePoint::new("0", "2"),
        ];
        let o = outline(&rect, &none).unwrap();
        let polygon_area = |n: f64| 0.5 * n * (TAU / n).sin();
        for plane in SketchPlane::ALL {
            let faces = revolve(plane, &o, 0.0, 0.0, 360.0).unwrap();
            let expected = polygon_area(REVOLUTION_SEGMENTS) * 2.0;
            assert!((volume(&faces) - expected).abs() < 1e-9, "{plane:?}: {}", volume(&faces));
        }
        // A quarter turn the other way, and an outline on the negative side of the axis.
        let quarter = revolve(SketchPlane::Front, &o, 0.0, 0.0, -90.0).unwrap();
        let expected = 12.0 * 0.5 * (TAU / 48.0).sin() * 2.0;
        assert!((volume(&quarter) - expected).abs() < 1e-9, "{}", volume(&quarter));
        let left: Vec<DVec2> = o.iter().map(|p| DVec2::new(3.0 - p.x, p.y)).rev().collect();
        let ring = revolve(SketchPlane::Front, &left, 0.0, 3.0, 360.0).unwrap();
        assert!(volume(&ring) > 0.0);
        // Around the vertical axis of the front plane: the cylinder stands on the floor.
        let faces = revolve(SketchPlane::Front, &o, 0.0, 0.0, 360.0).unwrap();
        let top = faces.iter().flatten().fold(f64::NEG_INFINITY, |m, p| m.max(p.z));
        assert!((top - 2.0).abs() < 1e-12);
        // Crossing the axis is refused.
        assert_eq!(revolve(SketchPlane::Plan, &o, 0.0, 0.5, 360.0), Err(ProfileError::Degenerate));
    }
}
