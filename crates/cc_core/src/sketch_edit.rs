//! Editing the curves of a sketch: where two curves (or the lines, circles
//! and ellipses they lie on) cross, a curve trimmed or extended to a point,
//! split in two, and offset alone or along the chain of curves joined to it.
//! The sketch tools of the app use these; `None` means the edit makes no
//! valid curve (closed curves have no end to trim, ellipses no offset that
//! is an ellipse, curves that never meet).

use crate::arc::{Arc, MAX_BULGE};
use crate::sketch::{JOIN_TOL, SketchCurve};
use glam::DVec2;
use std::f64::consts::{PI, TAU};

/// Steps around a circle or an ellipse when looking for crossings.
const SAMPLES: usize = 720;

/// The whole line, circle or ellipse a curve lies on.
#[derive(Debug, Clone, Copy)]
enum Carrier {
    Line {
        a: DVec2,
        dir: DVec2,
    },
    /// Circles have equal radii.
    Ellipse {
        center: DVec2,
        radii: DVec2,
        turn: DVec2,
    },
}

impl Carrier {
    fn of(c: &SketchCurve) -> Option<Carrier> {
        let line = |a: DVec2, b: DVec2| Some(Carrier::Line { a, dir: (b - a).try_normalize()? });
        match *c {
            SketchCurve::Line { a, b } => line(a, b),
            SketchCurve::Arc { a, b, bulge } => match Arc::from_bulge(a, b, bulge) {
                Some(arc) => {
                    Some(Carrier::Ellipse { center: arc.center, radii: DVec2::splat(arc.radius), turn: DVec2::X })
                }
                None => line(a, b),
            },
            SketchCurve::Circle { center, radius } => {
                Some(Carrier::Ellipse { center, radii: DVec2::splat(radius), turn: DVec2::X })
            }
            SketchCurve::Ellipse { center, radii, rotation_deg, .. } => {
                Some(Carrier::Ellipse { center, radii, turn: DVec2::from_angle(rotation_deg.to_radians()) })
            }
        }
    }

    /// 0 on the carrier, changing sign across it.
    fn level(&self, p: DVec2) -> f64 {
        match *self {
            Carrier::Line { a, dir } => dir.perp_dot(p - a),
            Carrier::Ellipse { center, radii, turn } => (unturn(turn, p - center) / radii).length() - 1.0,
        }
    }
}

/// `v` turned back by the rotation `turn`.
fn unturn(turn: DVec2, v: DVec2) -> DVec2 {
    DVec2::new(turn.x, -turn.y).rotate(v)
}

/// The point of an ellipse at the parameter angle `t`.
fn ellipse_at(center: DVec2, radii: DVec2, turn: DVec2, t: f64) -> DVec2 {
    center + turn.rotate(DVec2::new(radii.x * t.cos(), radii.y * t.sin()))
}

/// Where the lines, circles and ellipses curves `a` and `b` lie on cross
/// (lines run on past their ends, arcs round their circle, partial
/// ellipses round the whole ellipse). Tangent contacts may be missed.
pub fn crossings(a: &SketchCurve, b: &SketchCurve) -> Vec<DVec2> {
    let (Some(ca), Some(cb)) = (Carrier::of(a), Carrier::of(b)) else { return Vec::new() };
    let mut out: Vec<DVec2> = Vec::new();
    match (ca, cb) {
        (Carrier::Line { a: p, dir: d }, Carrier::Line { a: q, dir: e }) => {
            let denom = d.perp_dot(e);
            if denom.abs() > 1e-12 {
                out.push(p + d * ((q - p).perp_dot(e) / denom));
            }
        }
        // Round the ellipse (or circle), where the other's level changes sign.
        (Carrier::Ellipse { center, radii, turn }, other) | (other, Carrier::Ellipse { center, radii, turn }) => {
            let at = |t: f64| ellipse_at(center, radii, turn, t);
            let f = |t: f64| other.level(at(t));
            for i in 0..SAMPLES {
                let (mut t0, mut t1) = (TAU * i as f64 / SAMPLES as f64, TAU * (i + 1) as f64 / SAMPLES as f64);
                let (mut f0, f1) = (f(t0), f(t1));
                // On the other curve, or nearly: the samples at 2π and 0 are a few
                // 1e-16 apart, and a crossing between them changes no sign.
                if f0.abs() <= 1e-12 {
                    out.push(at(t0));
                    continue;
                }
                if f0.is_nan() || f1.is_nan() || f0 * f1 >= 0.0 {
                    continue;
                }
                for _ in 0..60 {
                    let tm = (t0 + t1) / 2.0;
                    let fm = f(tm);
                    if fm * f0 <= 0.0 {
                        t1 = tm;
                    } else {
                        (t0, f0) = (tm, fm);
                    }
                }
                out.push(at((t0 + t1) / 2.0));
            }
        }
    }
    let mut unique: Vec<DVec2> = Vec::new();
    for p in out.into_iter().filter(|p| p.is_finite()) {
        if !unique.iter().any(|q| q.distance(p) <= JOIN_TOL) {
            unique.push(p);
        }
    }
    unique
}

/// The point of `points` nearest `to`.
pub fn nearest(points: &[DVec2], to: DVec2) -> Option<DVec2> {
    points.iter().copied().min_by(|a, b| a.distance(to).total_cmp(&b.distance(to)))
}

/// The parameter angle of `p` on an ellipse, from the angle `t0` in the
/// direction `sign`, measured to the nearer side of the gap of a part
/// turning `sweep` (radians, like [`Arc::locate`]).
fn ellipse_param(center: DVec2, radii: DVec2, turn: DVec2, t0: f64, sweep: f64, p: DVec2) -> f64 {
    let local = unturn(turn, p - center) / radii;
    let t = local.y.atan2(local.x);
    let sign = if sweep >= 0.0 { 1.0 } else { -1.0 };
    let mid = sweep.abs() / 2.0;
    ((t - t0) * sign - mid + PI).rem_euclid(TAU) - PI + mid
}

/// Where `p` lies along an open curve from its start: meters along lines
/// and arcs, radians of the parameter along ellipses; before the start it
/// is negative and past the end greater than [`span`], where the curve runs
/// on. `None` for closed curves.
pub fn param(c: &SketchCurve, p: DVec2) -> Option<f64> {
    if c.is_closed() {
        return None;
    }
    match *c {
        SketchCurve::Line { a, b } => Some((p - a).dot((b - a).try_normalize()?)),
        SketchCurve::Arc { a, b, bulge } => match Arc::from_bulge(a, b, bulge) {
            Some(arc) => Some(arc.locate(p).0),
            None => param(&SketchCurve::Line { a, b }, p),
        },
        SketchCurve::Ellipse { center, radii, rotation_deg, start_deg, sweep_deg } => {
            let turn = DVec2::from_angle(rotation_deg.to_radians());
            Some(ellipse_param(center, radii, turn, start_deg.to_radians(), sweep_deg.to_radians(), p))
        }
        SketchCurve::Circle { .. } => None,
    }
}

/// The parameter of an open curve's end (see [`param`]).
pub fn span(c: &SketchCurve) -> Option<f64> {
    if c.is_closed() {
        return None;
    }
    match *c {
        SketchCurve::Line { a, b } => Some(a.distance(b)),
        SketchCurve::Arc { a, b, bulge } => {
            Some(Arc::from_bulge(a, b, bulge).map_or(a.distance(b), |arc| arc.length()))
        }
        SketchCurve::Ellipse { sweep_deg, .. } => Some(sweep_deg.to_radians().abs()),
        SketchCurve::Circle { .. } => None,
    }
}

/// The part of an open curve between the parameters `u0 < u1` (see
/// [`param`]); beyond its ends the curve runs on, so this also extends it.
pub fn between(c: &SketchCurve, u0: f64, u1: f64) -> Option<SketchCurve> {
    if !u0.is_finite() || !u1.is_finite() || u1 <= u0 || c.is_closed() {
        return None;
    }
    let out = match *c {
        SketchCurve::Line { a, b } => {
            let dir = (b - a).try_normalize()?;
            SketchCurve::Line { a: a + dir * u0, b: a + dir * u1 }
        }
        SketchCurve::Arc { a, b, bulge } => match Arc::from_bulge(a, b, bulge) {
            Some(arc) => {
                let bulge = arc.bulge_between(u0, u1);
                if bulge.is_nan() || bulge.abs() > MAX_BULGE {
                    return None;
                }
                SketchCurve::Arc { a: arc.point(u0), b: arc.point(u1), bulge }
            }
            None => return between(&SketchCurve::Line { a, b }, u0, u1),
        },
        SketchCurve::Ellipse { center, radii, rotation_deg, start_deg, sweep_deg } => {
            if u1 - u0 >= TAU - 1e-6 {
                return None;
            }
            let sign = if sweep_deg >= 0.0 { 1.0 } else { -1.0 };
            SketchCurve::Ellipse {
                center,
                radii,
                rotation_deg,
                start_deg: start_deg + sign * u0.to_degrees(),
                sweep_deg: sign * (u1 - u0).to_degrees(),
            }
        }
        SketchCurve::Circle { .. } => return None,
    };
    out.is_valid().then_some(out)
}

/// The open curve `c` ending at `at` (a point on it or where it runs on),
/// keeping the side of `at` where `keep` is: trimmed or extended.
pub fn trim_to(c: &SketchCurve, keep: DVec2, at: DVec2) -> Option<SketchCurve> {
    let (uk, ux, end) = (param(c, keep)?, param(c, at)?, span(c)?);
    if uk < ux { between(c, 0.0, ux) } else { between(c, ux, end) }
}

/// Two curves trimmed or extended to meet where they cross, each keeping
/// the side clicked (`k1`, `k2`); the crossing nearest the clicks.
pub fn trim_corner(c1: &SketchCurve, k1: DVec2, c2: &SketchCurve, k2: DVec2) -> Option<(SketchCurve, SketchCurve)> {
    let xs = crossings(c1, c2);
    let x = xs
        .iter()
        .copied()
        .min_by(|a, b| (a.distance(k1) + a.distance(k2)).total_cmp(&(b.distance(k1) + b.distance(k2))))?;
    Some((trim_to(c1, k1, x)?, trim_to(c2, k2, x)?))
}

/// Curve `c` trimmed or extended to the curve `boundary`, keeping the side
/// clicked (`keep`); the crossing nearest the click.
pub fn trim_extend(c: &SketchCurve, keep: DVec2, boundary: &SketchCurve) -> Option<SketchCurve> {
    let x = nearest(&crossings(c, boundary), keep)?;
    trim_to(c, keep, x)
}

/// Curve `c` split in two at the point of it nearest `p`. A circle or a
/// whole ellipse splits into halves, from `p` and from the point opposite.
pub fn split(c: &SketchCurve, p: DVec2) -> Option<(SketchCurve, SketchCurve)> {
    let pair = match *c {
        SketchCurve::Circle { center, radius } => {
            let dir = (p - center).try_normalize()?;
            let (a, b) = (center + dir * radius, center - dir * radius);
            (SketchCurve::Arc { a, b, bulge: 1.0 }, SketchCurve::Arc { a: b, b: a, bulge: 1.0 })
        }
        SketchCurve::Ellipse { center, radii, rotation_deg, sweep_deg, .. } if c.is_closed() => {
            let turn = DVec2::from_angle(rotation_deg.to_radians());
            let local = unturn(turn, p - center) / radii;
            let t = local.y.atan2(local.x).to_degrees();
            let half = if sweep_deg >= 0.0 { 180.0 } else { -180.0 };
            let part = |start_deg| SketchCurve::Ellipse { center, radii, rotation_deg, start_deg, sweep_deg: half };
            (part(t), part(t + half))
        }
        _ => {
            let (u, end) = (param(c, p)?, span(c)?);
            if u <= 0.0 || u >= end {
                return None;
            }
            (between(c, 0.0, u)?, between(c, u, end)?)
        }
    };
    (pair.0.is_valid() && pair.1.is_valid()).then_some(pair)
}

/// The side of curve `c` that `p` is on, looking along it: 1 left, -1
/// right. Circles run counter-clockwise, so their inside is their left.
fn side(c: &SketchCurve, p: DVec2) -> f64 {
    let left = match *c {
        SketchCurve::Line { a, b } => (b - a).perp_dot(p - a) >= 0.0,
        SketchCurve::Arc { a, b, bulge } => match Arc::from_bulge(a, b, bulge) {
            Some(arc) => arc.locate(p).1 >= 0.0,
            None => (b - a).perp_dot(p - a) >= 0.0,
        },
        SketchCurve::Circle { center, radius } => center.distance(p) <= radius,
        SketchCurve::Ellipse { .. } => true,
    };
    if left { 1.0 } else { -1.0 }
}

/// Curve `c` moved `v` to its left (to its right when negative); `None`
/// for ellipses and when an arc's or a circle's radius would vanish.
fn offset_left(c: &SketchCurve, v: f64) -> Option<SketchCurve> {
    let out = match *c {
        SketchCurve::Line { a, b } => {
            let n = (b - a).try_normalize()?.perp() * v;
            SketchCurve::Line { a: a + n, b: b + n }
        }
        SketchCurve::Arc { a, b, bulge } => match Arc::from_bulge(a, b, bulge) {
            Some(arc) => {
                if arc.radius_at(v) <= JOIN_TOL {
                    return None;
                }
                SketchCurve::Arc { a: arc.at(0.0, v), b: arc.at(arc.length(), v), bulge }
            }
            None => return offset_left(&SketchCurve::Line { a, b }, v),
        },
        SketchCurve::Circle { center, radius } => SketchCurve::Circle { center, radius: radius - v },
        SketchCurve::Ellipse { .. } => return None,
    };
    out.is_valid().then_some(out)
}

/// A copy of curve `c` `distance` away from it, on the side of `toward`.
pub fn offset(c: &SketchCurve, distance: f64, toward: DVec2) -> Option<SketchCurve> {
    offset_left(c, distance.abs() * side(c, toward))
}

/// The curves joined end to end with curve `i`, in order along the chain,
/// each with whether it runs backwards in it, and whether the chain
/// closes. The chain stops where more than two curves meet.
pub fn chain(curves: &[SketchCurve], i: usize) -> (Vec<(usize, bool)>, bool) {
    let Some(first) = curves.get(i) else { return (Vec::new(), false) };
    if first.is_closed() {
        return (vec![(i, false)], true);
    }
    let ends =
        |k: usize, rev: bool| curves.get(k).and_then(SketchCurve::ends).map(|(a, b)| if rev { (b, a) } else { (a, b) });
    // The one other curve with an end at `p`, and whether that end is its start.
    let next = |p: DVec2, from: usize| -> Option<(usize, bool)> {
        let mut found = curves.iter().enumerate().filter(|(k, _)| *k != from).filter_map(|(k, c)| {
            let (a, b) = c.ends()?;
            if a.distance(p) <= JOIN_TOL {
                Some((k, true))
            } else if b.distance(p) <= JOIN_TOL {
                Some((k, false))
            } else {
                None
            }
        });
        match (found.next(), found.next()) {
            (Some(one), None) => Some(one),
            _ => None,
        }
    };
    let mut links = vec![(i, false)];
    let mut closed = false;
    // Forward from the end, then back from the start.
    while links.len() <= curves.len() {
        let Some(&(k, rev)) = links.last() else { break };
        let Some((_, end)) = ends(k, rev) else { break };
        let Some((j, at_start)) = next(end, k) else { break };
        if j == i {
            closed = at_start;
            break;
        }
        if links.iter().any(|(l, _)| *l == j) {
            break;
        }
        links.push((j, !at_start));
    }
    if !closed {
        while links.len() <= curves.len() {
            let Some(&(k, rev)) = links.first() else { break };
            let Some((start, _)) = ends(k, rev) else { break };
            let Some((j, at_start)) = next(start, k) else { break };
            if links.iter().any(|(l, _)| *l == j) {
                break;
            }
            // Its end meets the chain's start: it runs forwards.
            links.insert(0, (j, at_start));
        }
    }
    (links, closed)
}

/// Copies of the chain of curve `i` (see [`chain`]) `distance` away on the
/// side of `toward`, neighbors meeting where their copies cross (or joined
/// by a line where they do not); `None` when a curve has no offset.
pub fn offset_chain(curves: &[SketchCurve], i: usize, distance: f64, toward: DVec2) -> Option<Vec<SketchCurve>> {
    let (links, closed) = chain(curves, i);
    let oriented: Vec<SketchCurve> = links
        .iter()
        .map(|&(k, rev)| curves.get(k).map(|c| if rev { c.reversed() } else { *c }))
        .collect::<Option<_>>()?;
    let at = links.iter().position(|&(k, _)| k == i)?;
    let v = distance.abs() * side(oriented.get(at)?, toward);
    let mut out: Vec<SketchCurve> = oriented.iter().map(|c| offset_left(c, v)).collect::<Option<_>>()?;
    let n = out.len();
    let corners = match (n, closed) {
        (0 | 1, _) => 0,
        (_, true) => n,
        (_, false) => n - 1,
    };
    let mut bridges = Vec::new();
    for k in 0..corners {
        let j = (k + 1) % n;
        let corner = oriented.get(k)?.ends()?.1;
        let (end, start) = (out.get(k)?.ends()?.1, out.get(j)?.ends()?.0);
        if end.distance(start) <= JOIN_TOL {
            continue;
        }
        let met = nearest(&crossings(&out[k], &out[j]), corner).and_then(|x| {
            let before = between(&out[k], 0.0, param(&out[k], x)?)?;
            let after = between(&out[j], param(&out[j], x)?, span(&out[j])?)?;
            Some((before, after))
        });
        match met {
            Some((before, after)) => {
                out[k] = before;
                out[j] = after;
            }
            None => bridges.push(SketchCurve::Line { a: end, b: start }),
        }
    }
    out.extend(bridges.into_iter().filter(SketchCurve::is_valid));
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sketch::Sketch;

    fn p(x: f64, y: f64) -> DVec2 {
        DVec2::new(x, y)
    }

    fn line(a: DVec2, b: DVec2) -> SketchCurve {
        SketchCurve::Line { a, b }
    }

    fn close(a: DVec2, b: DVec2) -> bool {
        a.distance(b) < 1e-6
    }

    #[test]
    fn curves_cross_where_their_carriers_do() {
        let xs = crossings(&line(p(0.0, 0.0), p(1.0, 0.0)), &line(p(3.0, -1.0), p(3.0, 1.0)));
        assert!(xs.len() == 1 && close(xs[0], p(3.0, 0.0)), "{xs:?}");
        let circle = SketchCurve::Circle { center: p(0.0, 0.0), radius: 2.0 };
        let xs = crossings(&line(p(-5.0, 1.0), p(-4.0, 1.0)), &circle);
        assert_eq!(xs.len(), 2);
        assert!(xs.iter().all(|x| (x.length() - 2.0).abs() < 1e-9 && (x.y - 1.0).abs() < 1e-9), "{xs:?}");
        // An ellipse 3 × 1 and a circle of radius 2: four crossings.
        let e = SketchCurve::ellipse(p(0.0, 0.0), p(3.0, 1.0), 0.0);
        let xs = crossings(&e, &circle);
        assert_eq!(xs.len(), 4, "{xs:?}");
        assert!(xs.iter().all(|x| (x.length() - 2.0).abs() < 1e-9));
        // A crossing where the sampling closes round the circle (between the
        // points at 2π and 0, a few 1e-16 apart) is found too.
        let unit = SketchCurve::Circle { center: p(0.0, 0.0), radius: 1.0 };
        let xs = crossings(&unit, &line(p(-2.0, -1e-16), p(2.0, -1e-16)));
        assert_eq!(xs.len(), 2, "{xs:?}");
    }

    #[test]
    fn corners_trim_and_extend() {
        // One line overshoots the corner, the other falls short of it.
        let a = line(p(0.0, 0.0), p(5.0, 0.0));
        let b = line(p(4.0, 1.0), p(4.0, 3.0));
        let (a2, b2) = trim_corner(&a, p(1.0, 0.0), &b, p(4.0, 2.0)).unwrap();
        assert_eq!(a2, line(p(0.0, 0.0), p(4.0, 0.0)));
        assert!(matches!(b2, SketchCurve::Line { a, b } if close(a, p(4.0, 0.0)) && close(b, p(4.0, 3.0))), "{b2:?}");
        // Parallel lines never meet.
        assert!(trim_corner(&a, p(1.0, 0.0), &line(p(0.0, 1.0), p(5.0, 1.0)), p(1.0, 1.0)).is_none());
        // An arc extended round its circle to a line.
        let arc = SketchCurve::Arc { a: p(1.0, 0.0), b: p(0.0, 1.0), bulge: (PI / 8.0).tan() };
        let out = trim_extend(&arc, p(0.7, 0.7), &line(p(-2.0, 0.0), p(2.0, 0.0)).reversed()).unwrap();
        assert!(matches!(out, SketchCurve::Arc { a, b, .. } if close(a, p(1.0, 0.0)) && close(b, p(0.0, 1.0))));
        let wall = line(p(-2.0, 0.6), p(2.0, 0.6));
        let out = trim_extend(&arc, p(0.9, 0.2), &wall).unwrap();
        let (_, end) = out.ends().unwrap();
        assert!((end.y - 0.6).abs() < 1e-9 && (end.length() - 1.0).abs() < 1e-9, "{out:?}");
    }

    #[test]
    fn splitting_keeps_the_loop() {
        let (a, b) = split(&line(p(0.0, 0.0), p(4.0, 0.0)), p(1.0, 0.3)).unwrap();
        assert_eq!((a, b), (line(p(0.0, 0.0), p(1.0, 0.0)), line(p(1.0, 0.0), p(4.0, 0.0))));
        assert!(split(&line(p(0.0, 0.0), p(4.0, 0.0)), p(5.0, 0.0)).is_none());
        // A circle splits into halves that still make a loop of the same area.
        let (h1, h2) = split(&SketchCurve::Circle { center: p(0.0, 0.0), radius: 1.0 }, p(0.0, 3.0)).unwrap();
        let outline = Sketch { curves: vec![h1, h2] }.outline().unwrap();
        let area = crate::mass::area(&outline).abs();
        assert!((area - PI).abs() < 0.01, "{area}");
        let e = SketchCurve::ellipse(p(0.0, 0.0), p(2.0, 1.0), 30.0);
        let (e1, e2) = split(&e, p(5.0, 0.0)).unwrap();
        assert!(Sketch { curves: vec![e1, e2] }.outline().is_ok());
    }

    #[test]
    fn chains_offset_with_their_corners() {
        let rect = vec![
            line(p(0.0, 0.0), p(4.0, 0.0)),
            line(p(4.0, 2.0), p(4.0, 0.0)), // runs backwards
            line(p(4.0, 2.0), p(0.0, 2.0)),
            line(p(0.0, 2.0), p(0.0, 0.0)),
        ];
        let (links, closed) = chain(&rect, 2);
        assert!(closed && links.len() == 4, "{links:?}");
        let out = offset_chain(&rect, 0, 1.0, p(2.0, -3.0)).unwrap();
        let outline = Sketch { curves: out }.outline().unwrap();
        let area = crate::mass::area(&outline).abs();
        assert!((area - 6.0 * 4.0).abs() < 1e-9, "{outline:?}");
        // Inwards, an arc end gets the concentric arc.
        let mut rounded = rect.clone();
        rounded[1] = SketchCurve::Arc { a: p(4.0, 0.0), b: p(4.0, 2.0), bulge: 1.0 };
        let out = offset_chain(&rounded, 0, 0.5, p(2.0, 1.0)).unwrap();
        let sketch = Sketch { curves: out };
        assert!(sketch.outline().is_ok(), "{:?}", sketch.outline());
        assert!(
            sketch.curves.iter().any(|c| matches!(c, SketchCurve::Arc { a, b, .. }
            if close(*a, p(4.0, 0.5)) && close(*b, p(4.0, 1.5)))),
            "{:?}",
            sketch.curves
        );
        // An open chain: two lines.
        let open = vec![line(p(0.0, 0.0), p(2.0, 0.0)), line(p(2.0, 0.0), p(2.0, 2.0))];
        let out = offset_chain(&open, 1, 1.0, p(5.0, 1.0)).unwrap();
        assert_eq!(out, vec![line(p(0.0, -1.0), p(3.0, -1.0)), line(p(3.0, -1.0), p(3.0, 2.0))]);
        assert!(offset(&SketchCurve::ellipse(p(0.0, 0.0), p(2.0, 1.0), 0.0), 1.0, p(0.0, 0.0)).is_none());
        assert_eq!(
            offset(&SketchCurve::Circle { center: p(0.0, 0.0), radius: 2.0 }, 0.5, p(9.0, 0.0)),
            Some(SketchCurve::Circle { center: p(0.0, 0.0), radius: 2.5 })
        );
    }
}
