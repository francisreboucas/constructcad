//! Sweeps and blends of component pieces.
//!
//! A sweep carries a closed profile along an open path drawn on one of the
//! sketch planes: the profile's `u` runs to the left of the path (in the
//! path's plane) and its `v` along the plane's normal, and at each corner
//! of the path the section is mitered (it lies in the plane that bisects
//! the corner). A blend joins two closed outlines drawn on the same sketch
//! plane at two positions along its normal (a bottom and a top), the faces
//! between them ruled from points spaced evenly around each outline.
//! Both come out as closed meshes with outward faces, in the component's
//! frame.

use crate::expr::{self, ExprError};
use crate::profile::{ProfileError, ProfilePoint, SketchPlane};
use glam::{DVec2, DVec3};

/// Points of each blend outline after resampling: at least this many.
const BLEND_SAMPLES: usize = 48;
/// The miter scale at a sharp corner stops growing here (about 150° turns).
const MAX_MITER: f64 = 4.0;
/// Most points of a path or outline after arcs are divided.
const MAX_POINTS: usize = 4096;

/// An open path of lines, arcs and parts of ellipses (a point's `radius`
/// makes the segment to the next point an arc, positive bulging to the
/// right of travel; its `ellipse` a part of that ellipse).
pub fn path(points: &[ProfilePoint], lookup: &dyn Fn(&str) -> Option<f64>) -> Result<Vec<DVec2>, ProfileError> {
    let corners = crate::profile::vertices(points, lookup)?;
    let mut out = Vec::with_capacity(corners.len());
    for (i, (a, segment)) in corners.iter().enumerate() {
        out.push(*a);
        if let Some((b, _)) = corners.get(i + 1) {
            crate::profile::segment_points(*a, *b, *segment, &mut out)?;
        }
        if out.len() > MAX_POINTS {
            return Err(ProfileError::Degenerate);
        }
    }
    out.dedup_by(|a, b| a.distance(*b) < 1e-9);
    if out.len() < 2 || out.iter().any(|p| !p.is_finite()) {
        return Err(ProfileError::Degenerate);
    }
    Ok(out)
}

/// Flip every face when the mesh's signed volume is negative (inward faces).
fn outward(mut faces: Vec<Vec<DVec3>>) -> Result<Vec<Vec<DVec3>>, ProfileError> {
    let volume: f64 = faces
        .iter()
        .map(|f| {
            let Some(o) = f.first() else { return 0.0 };
            (1..f.len().saturating_sub(1)).map(|i| o.dot(f[i].cross(f[i + 1]))).sum::<f64>()
        })
        .sum();
    if !volume.is_finite() || volume.abs() < 1e-12 {
        return Err(ProfileError::Degenerate);
    }
    if volume < 0.0 {
        faces.iter_mut().for_each(|f| f.reverse());
    }
    Ok(faces)
}

/// The solid of a counter-clockwise `profile` carried along `path` (on
/// `plane`, placed `offset` along its normal), mitered at the corners.
pub fn sweep(
    plane: SketchPlane,
    path: &[DVec2],
    offset: f64,
    profile: &[DVec2],
) -> Result<Vec<Vec<DVec3>>, ProfileError> {
    let n = path.len();
    if n < 2 || profile.len() < 3 {
        return Err(ProfileError::Degenerate);
    }
    let dirs: Vec<DVec2> = path.windows(2).filter_map(|w| (w[1] - w[0]).try_normalize()).collect();
    if dirs.len() != n - 1 {
        return Err(ProfileError::Degenerate);
    }
    // At each point: the bisecting direction and the miter scale.
    let rings: Vec<Vec<DVec3>> = (0..n)
        .map(|i| {
            let (before, after) = (dirs.get(i.wrapping_sub(1)).copied(), dirs.get(i).copied());
            let (d, scale) = match (before, after) {
                (Some(a), Some(b)) => {
                    let d = (a + b).try_normalize().unwrap_or(b);
                    (d, (1.0 / d.dot(b).max(1.0 / MAX_MITER)).min(MAX_MITER))
                }
                (Some(a), None) => (a, 1.0),
                (None, Some(b)) => (b, 1.0),
                (None, None) => (DVec2::X, 1.0),
            };
            let left = d.perp();
            profile
                .iter()
                .map(|q| {
                    let p = path[i] + left * (q.x * scale);
                    plane.point(p.x, p.y, offset + q.y)
                })
                .collect()
        })
        .collect();
    let m = profile.len();
    let mut faces = Vec::with_capacity(m * (n - 1) + 2);
    if let (Some(first), Some(last)) = (rings.first(), rings.last()) {
        faces.push(first.clone());
        faces.push(last.iter().rev().copied().collect());
    }
    for w in rings.windows(2) {
        for j in 0..m {
            let k = (j + 1) % m;
            faces.push(vec![w[0][j], w[1][j], w[1][k], w[0][k]]);
        }
    }
    outward(faces)
}

/// The points of two outlines matched one to one, both starting at their
/// point farthest along +U from their center (so they line up): vertex to
/// vertex when they have as many (an exact loft between similar shapes, such
/// as an outline and its inset), else both resampled evenly.
fn matched(bottom: &[DVec2], top: &[DVec2]) -> (Vec<DVec2>, Vec<DVec2>) {
    if bottom.len() == top.len() {
        return (from_start(bottom), from_start(top));
    }
    let count = bottom.len().max(top.len()).clamp(BLEND_SAMPLES, MAX_POINTS);
    (resample(bottom, count), resample(top, count))
}

/// `outline` starting from its point farthest along +U from its center.
fn from_start(outline: &[DVec2]) -> Vec<DVec2> {
    let n = outline.len();
    if n == 0 {
        return Vec::new();
    }
    let center = outline.iter().copied().sum::<DVec2>() / n as f64;
    // Ties (an edge facing +U) take the lowest V, so both outlines pick the same corner.
    let key = |p: DVec2| (p - center).x - (p - center).y * 1e-9;
    let start = (0..n).max_by(|a, b| key(outline[*a]).total_cmp(&key(outline[*b]))).unwrap_or(0);
    (0..n).map(|i| outline[(start + i) % n]).collect()
}

/// `outline` (closed) resampled to `count` points evenly spaced along it,
/// starting from its point farthest along +U from its center (so two
/// outlines line up).
fn resample(outline: &[DVec2], count: usize) -> Vec<DVec2> {
    let n = outline.len();
    let center = outline.iter().copied().sum::<DVec2>() / n as f64;
    let start = (0..n).max_by(|a, b| (outline[*a] - center).x.total_cmp(&(outline[*b] - center).x)).unwrap_or(0);
    let ring: Vec<DVec2> = (0..=n).map(|i| outline[(start + i) % n]).collect();
    let lengths: Vec<f64> = ring.windows(2).map(|w| w[0].distance(w[1])).collect();
    let total: f64 = lengths.iter().sum();
    let mut out = Vec::with_capacity(count);
    let (mut seg, mut walked) = (0, 0.0);
    for i in 0..count {
        let target = total * i as f64 / count as f64;
        while seg + 1 < lengths.len() && walked + lengths[seg] < target {
            walked += lengths[seg];
            seg += 1;
        }
        let len = lengths.get(seg).copied().unwrap_or(0.0);
        let t = if len > 1e-12 { ((target - walked) / len).clamp(0.0, 1.0) } else { 0.0 };
        out.push(ring[seg].lerp(ring[seg + 1], t));
    }
    out
}

/// `outline` in counter-clockwise order.
pub fn counter_clockwise(outline: &[DVec2]) -> Vec<DVec2> {
    let n = outline.len();
    let signed: f64 = (0..n).map(|i| outline[i].perp_dot(outline[(i + 1) % n])).sum();
    if signed < 0.0 { outline.iter().rev().copied().collect() } else { outline.to_vec() }
}

/// The section of [`blend`] between counter-clockwise `bottom` and `top` at
/// `t` (0 at the bottom, 1 at the top): its outlines resampled alike and
/// joined point by point, as the blend's faces are.
pub fn blend_section(bottom: &[DVec2], top: &[DVec2], t: f64) -> Vec<DVec2> {
    if bottom.len() < 3 || top.len() < 3 {
        return bottom.to_vec();
    }
    let (b, u) = matched(bottom, top);
    b.iter().zip(&u).map(|(p, q)| p.lerp(*q, t)).collect()
}

/// The solid between a counter-clockwise `bottom` outline on `plane` at
/// `offset` and a `top` outline `depth` farther along its normal.
pub fn blend(
    plane: SketchPlane,
    bottom: &[DVec2],
    top: &[DVec2],
    offset: f64,
    depth: f64,
) -> Result<Vec<Vec<DVec3>>, ProfileError> {
    if bottom.len() < 3 || top.len() < 3 || depth.abs() < 1e-6 || !depth.is_finite() {
        return Err(ProfileError::Degenerate);
    }
    let (b, t) = matched(bottom, top);
    let count = b.len();
    let low: Vec<DVec3> = b.iter().map(|p| plane.point(p.x, p.y, offset)).collect();
    let high: Vec<DVec3> = t.iter().map(|p| plane.point(p.x, p.y, offset + depth)).collect();
    let mut faces = Vec::with_capacity(count + 2);
    faces.push(low.iter().rev().copied().collect());
    faces.push(high.clone());
    for j in 0..count {
        let k = (j + 1) % count;
        faces.push(vec![low[j], low[k], high[k], high[j]]);
    }
    outward(faces)
}

/// Signed volume of a closed mesh (for tests and checks).
pub fn volume(faces: &[Vec<DVec3>]) -> f64 {
    faces
        .iter()
        .map(|f| {
            let Some(o) = f.first() else { return 0.0 };
            (1..f.len().saturating_sub(1)).map(|i| o.dot(f[i].cross(f[i + 1]))).sum::<f64>()
        })
        .sum::<f64>()
        / 6.0
}

/// Evaluate an expression of a piece, naming the field on failure.
pub fn eval_field(field: &'static str, text: &str, lookup: &dyn Fn(&str) -> Option<f64>) -> Result<f64, ProfileError> {
    expr::eval(text, lookup).map_err(|error: ExprError| ProfileError::Expr { field, error })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(s: f64) -> Vec<DVec2> {
        vec![DVec2::new(-s, -s), DVec2::new(s, -s), DVec2::new(s, s), DVec2::new(-s, s)]
    }

    #[test]
    fn a_straight_sweep_is_a_prism_and_corners_are_mitered() {
        // A 0.1 square section along 2 m of X: 0.02 m³.
        let faces = sweep(SketchPlane::Plan, &[DVec2::ZERO, DVec2::new(2.0, 0.0)], 0.0, &square(0.05)).unwrap();
        assert!((volume(&faces) - 0.02).abs() < 1e-9, "{}", volume(&faces));
        // An L of two 1 m legs: the mitered corner keeps the volume of 2 m of section
        // (the miter adds what the inner side loses).
        let path = [DVec2::ZERO, DVec2::new(1.0, 0.0), DVec2::new(1.0, 1.0)];
        let faces = sweep(SketchPlane::Plan, &path, 0.0, &square(0.05)).unwrap();
        assert!((volume(&faces) - 0.02).abs() < 1e-9, "{}", volume(&faces));
        // A path drawn in the front view stands in X/Z; v lifts along -Y there... any plane gives a solid.
        let faces = sweep(SketchPlane::Front, &[DVec2::ZERO, DVec2::new(0.0, 1.0)], 0.2, &square(0.05)).unwrap();
        assert!((volume(&faces) - 0.01).abs() < 1e-9);
        assert!(sweep(SketchPlane::Plan, &[DVec2::ZERO, DVec2::ZERO], 0.0, &square(0.05)).is_err());
    }

    #[test]
    fn blends_join_outlines_and_keep_volume_for_equal_ones() {
        // Equal squares: a prism of 0.2 × 0.2 × 1.
        let faces = blend(SketchPlane::Plan, &square(0.1), &square(0.1), 0.0, 1.0).unwrap();
        assert!((volume(&faces) - 0.04).abs() < 1e-9, "{}", volume(&faces));
        // Square to a smaller square: a frustum, (A1 + A2 + sqrt(A1 A2)) h / 3.
        let faces = blend(SketchPlane::Plan, &square(0.1), &square(0.05), 0.0, 1.0).unwrap();
        let (a1, a2) = (0.04_f64, 0.01_f64);
        assert!((volume(&faces) - (a1 + a2 + (a1 * a2).sqrt()) / 3.0).abs() < 1e-6, "{}", volume(&faces));
        // A square into a circle-like outline of many points: still closed and outward.
        let circle: Vec<DVec2> =
            (0..40).map(|i| DVec2::from_angle(i as f64 / 40.0 * std::f64::consts::TAU) * 0.1).collect();
        let faces = blend(SketchPlane::Plan, &square(0.1), &circle, 0.0, 0.5).unwrap();
        assert!(volume(&faces) > 0.0);
        assert!(blend(SketchPlane::Plan, &square(0.1), &square(0.1), 0.0, 0.0).is_err());
    }
}
