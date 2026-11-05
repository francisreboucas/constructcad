//! Hatch patterns: the line work of a material [`Pattern`] clipped to a
//! polygon. Lines are anchored to the drawing origin so neighbouring
//! regions line up; `spacing` is the pattern's base spacing in drawing
//! units (callers derive it from paper millimeters and the view scale).

use cc_core::Pattern;
use cc_geom::polygon;
use glam::DVec2;

/// Most lines one region may produce (keeps huge regions at fine spacing cheap).
const MAX_LINES: usize = 4000;

/// Segments of `pattern` inside `poly`. `Solid` and `None` give no lines
/// (solid patterns are filled by the caller).
pub fn hatch(pattern: Pattern, poly: &[DVec2], spacing: f64) -> Vec<(DVec2, DVec2)> {
    if poly.len() < 3 || !(spacing.is_finite() && spacing > 1e-9) {
        return Vec::new();
    }
    let (lo, hi) = bounds(poly);
    let mut out = Vec::new();
    let parallel = |out: &mut Vec<(DVec2, DVec2)>, angle_deg: f64, step: f64| {
        let dir = DVec2::from_angle(angle_deg.to_radians());
        let n = dir.perp();
        // Offsets of lines crossing the bounding box, on the global grid.
        let corners = [lo, hi, DVec2::new(lo.x, hi.y), DVec2::new(hi.x, lo.y)];
        let (o0, o1) =
            corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), c| (a.min(c.dot(n)), b.max(c.dot(n))));
        let first = (o0 / step).ceil() as i64;
        let last = (o1 / step).floor() as i64;
        for k in first..=last {
            if out.len() >= MAX_LINES {
                return;
            }
            let origin = n * (k as f64 * step);
            for (t0, t1) in polygon::clip_line(poly, origin, dir) {
                out.push((origin + dir * t0, origin + dir * t1));
            }
        }
    };
    match pattern {
        Pattern::None | Pattern::Solid => {}
        Pattern::Diagonal => parallel(&mut out, 45.0, spacing),
        Pattern::DiagonalBack => parallel(&mut out, 135.0, spacing),
        Pattern::Crosshatch => {
            parallel(&mut out, 45.0, spacing);
            parallel(&mut out, 135.0, spacing);
        }
        Pattern::Horizontal => parallel(&mut out, 0.0, spacing),
        Pattern::Vertical => parallel(&mut out, 90.0, spacing),
        Pattern::Grid => {
            parallel(&mut out, 0.0, spacing);
            parallel(&mut out, 90.0, spacing);
        }
        Pattern::Brick => {
            // Courses of height `spacing`, bricks twice as long, joints staggered.
            let course = spacing;
            parallel(&mut out, 0.0, course);
            let first = (lo.y / course).floor() as i64;
            let last = (hi.y / course).ceil() as i64;
            for row in first..last {
                let (y0, y1) = (row as f64 * course, (row + 1) as f64 * course);
                let shift = if row.rem_euclid(2) == 0 { 0.0 } else { course };
                let mut k = ((lo.x - shift) / (2.0 * course)).floor() as i64;
                while (k as f64) * 2.0 * course + shift <= hi.x && out.len() < MAX_LINES {
                    let x = k as f64 * 2.0 * course + shift;
                    for (t0, t1) in polygon::clip_line(poly, DVec2::new(x, 0.0), DVec2::Y) {
                        let (a, b) = (t0.max(y0), t1.min(y1));
                        if b > a {
                            out.push((DVec2::new(x, a), DVec2::new(x, b)));
                        }
                    }
                    k += 1;
                }
            }
        }
        Pattern::Sand => dots(&mut out, poly, lo, hi, spacing * 0.5, false),
        Pattern::Earth => weave(&mut out, poly, lo, hi, spacing * 2.0),
        Pattern::Concrete => dots(&mut out, poly, lo, hi, spacing, true),
        Pattern::Insulation => {
            // Zig-zag running along X, one wave per `spacing`.
            let amp = spacing * 0.5;
            let rows_first = (lo.y / spacing).floor() as i64;
            let rows_last = (hi.y / spacing).ceil() as i64;
            for row in rows_first..rows_last {
                let y = row as f64 * spacing;
                let mut k = (lo.x / spacing).floor() as i64;
                while (k as f64) * spacing <= hi.x && out.len() < MAX_LINES {
                    let x = k as f64 * spacing;
                    let a = DVec2::new(x, y);
                    let b = DVec2::new(x + spacing * 0.5, y + amp * 2.0);
                    let c = DVec2::new(x + spacing, y);
                    for (p, q) in [(a, b), (b, c)] {
                        clip_segment(&mut out, poly, p, q);
                    }
                    k += 1;
                }
            }
        }
    }
    out
}

fn bounds(poly: &[DVec2]) -> (DVec2, DVec2) {
    poly.iter()
        .fold((DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)), |(lo, hi), p| (lo.min(*p), hi.max(*p)))
}

/// The parts of segment `p`-`q` inside `poly`.
fn clip_segment(out: &mut Vec<(DVec2, DVec2)>, poly: &[DVec2], p: DVec2, q: DVec2) {
    let d = q - p;
    let len = d.length();
    if len < 1e-12 {
        return;
    }
    let dir = d / len;
    for (t0, t1) in polygon::clip_line(poly, p, dir) {
        let (a, b) = (t0.max(0.0), t1.min(len));
        if b > a {
            out.push((p + dir * a, p + dir * b));
        }
    }
}

/// Earth: a basket weave turned 45°. Square blocks of `cell` along the
/// diagonals, each with three parallel strokes stopping short of its sides,
/// along one diagonal in a block and the other in the next.
fn weave(out: &mut Vec<(DVec2, DVec2)>, poly: &[DVec2], lo: DVec2, hi: DVec2, cell: f64) {
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let (u_axis, v_axis) = (DVec2::new(s, s), DVec2::new(-s, s));
    // The box's extent in the turned frame.
    let corners = [lo, hi, DVec2::new(lo.x, hi.y), DVec2::new(hi.x, lo.y)];
    let range = |axis: DVec2| {
        corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), c| (a.min(c.dot(axis)), b.max(c.dot(axis))))
    };
    let ((u0, u1), (v0, v1)) = (range(u_axis), range(v_axis));
    let (i0, i1) = ((u0 / cell).floor() as i64, (u1 / cell).ceil() as i64);
    let (j0, j1) = ((v0 / cell).floor() as i64, (v1 / cell).ceil() as i64);
    let gap = cell * 0.12;
    for i in i0..i1 {
        for j in j0..j1 {
            if out.len() >= MAX_LINES {
                return;
            }
            let (ua, va) = (i as f64 * cell, j as f64 * cell);
            let at = |u: f64, v: f64| u_axis * u + v_axis * v;
            for k in 1..=3 {
                let t = k as f64 * cell / 4.0;
                let (p, q) = if (i + j).rem_euclid(2) == 0 {
                    (at(ua + gap, va + t), at(ua + cell - gap, va + t))
                } else {
                    (at(ua + t, va + gap), at(ua + t, va + cell - gap))
                };
                clip_segment(out, poly, p, q);
            }
        }
    }
}

/// Deterministic jitter in [0, 1) for a grid cell.
fn jitter(i: i64, j: i64, salt: u64) -> f64 {
    let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (j as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ salt;
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Scattered dots (tiny segments) and, for concrete, small triangles.
fn dots(out: &mut Vec<(DVec2, DVec2)>, poly: &[DVec2], lo: DVec2, hi: DVec2, step: f64, triangles: bool) {
    let dot = step * 0.06;
    let (i0, i1) = ((lo.x / step).floor() as i64, (hi.x / step).ceil() as i64);
    let (j0, j1) = ((lo.y / step).floor() as i64, (hi.y / step).ceil() as i64);
    for i in i0..i1 {
        for j in j0..j1 {
            if out.len() >= MAX_LINES {
                return;
            }
            let p = DVec2::new((i as f64 + jitter(i, j, 1)) * step, (j as f64 + jitter(i, j, 2)) * step);
            if !polygon::contains(poly, p) {
                continue;
            }
            if triangles && jitter(i, j, 3) < 0.25 {
                let r = step * 0.22;
                let a = jitter(i, j, 4) * std::f64::consts::TAU;
                let pts: Vec<DVec2> =
                    (0..3).map(|k| p + DVec2::from_angle(a + k as f64 * std::f64::consts::TAU / 3.0) * r).collect();
                for k in 0..3 {
                    clip_segment(out, poly, pts[k], pts[(k + 1) % 3]);
                }
            } else {
                out.push((p, p + DVec2::new(dot, dot * 0.3)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(s: f64) -> Vec<DVec2> {
        vec![DVec2::ZERO, DVec2::new(s, 0.0), DVec2::new(s, s), DVec2::new(0.0, s)]
    }

    #[test]
    fn lines_stay_inside_and_follow_the_spacing() {
        let poly = square(1.0);
        let h = hatch(Pattern::Horizontal, &poly, 0.1);
        // y = 0.1 .. 0.9 (the edges at 0 and 1 are on the boundary).
        assert!((9..=11).contains(&h.len()), "{}", h.len());
        for (a, b) in hatch(Pattern::Crosshatch, &poly, 0.1).iter().chain(&h) {
            for p in [a, b] {
                assert!(p.x > -1e-9 && p.x < 1.0 + 1e-9 && p.y > -1e-9 && p.y < 1.0 + 1e-9, "{p}");
            }
        }
        for pattern in Pattern::ALL {
            let lines = hatch(pattern, &poly, 0.1);
            let empty = matches!(pattern, Pattern::None | Pattern::Solid);
            assert_eq!(lines.is_empty(), empty, "{pattern:?}");
            assert!(lines.iter().all(|(a, b)| a.is_finite() && b.is_finite()));
        }
    }

    #[test]
    fn hostile_input_is_bounded() {
        assert!(hatch(Pattern::Grid, &square(1.0), 0.0).is_empty());
        assert!(hatch(Pattern::Grid, &square(1.0), f64::NAN).is_empty());
        assert!(hatch(Pattern::Grid, &square(1e6), 1e-3).len() <= MAX_LINES * 2);
        assert!(hatch(Pattern::Sand, &square(1e6), 1e-3).len() <= MAX_LINES);
    }
}
