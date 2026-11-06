//! Building pads (`cc_core::pad`) graded into their terrain's surface.
//!
//! For a pad at absolute elevation `z_p`, with `d` the plan distance from its
//! outline (0 inside), the graded surface is `F = max(L, min(T, C))`, where
//! `T` is the surface before the pad, `C = z_p + d / cut` the cut slope and
//! `L = z_p - d / fill` the fill slope. Pads apply one after another.
//!
//! Only the surface's triangles near the pad (within the farthest distance a
//! slope can reach) are rebuilt: a constrained Delaunay triangulation of
//! their vertices, with their edges as constraints (split where the new ones
//! cross them), plus samples along rays out of the outline, the daylight
//! points where the slopes meet the ground (found by bisection) and the
//! outline and daylight lines as constraints. Every new triangle lies inside
//! one old triangle, so the old surface is linear on it, and its vertices
//! take `F`: cut and fill volumes are integrated exactly between the two.

use crate::polygon;
use glam::{DVec2, DVec3};
use spade::{ConstrainedDelaunayTriangulation, HasPosition, Point2, Triangulation};

/// Most offset rings (sample distances along each ray, doubling from [`FIRST_RING`]).
pub const MAX_RINGS: usize = 16;
/// Most vertices a pad adds to the surface: past it, the slopes are sampled more coarsely.
pub const MAX_PAD_VERTICES: usize = 50_000;
/// The first ring's distance from the outline, meters.
const FIRST_RING: f64 = 0.25;
/// Angle between the rays of a convex corner's fan, degrees.
const CORNER_STEP_DEG: f64 = 15.0;
/// Spacing of the rays along the outline's edges, meters (before coarsening).
const EDGE_STEP: f64 = 1.0;
/// Bisection steps when looking for a daylight point.
const BISECTION_STEPS: usize = 40;
/// Elevations closer than this are equal.
const Z_TOL: f64 = 1e-9;
/// Ground graded by less than this is unchanged (daylight points, found
/// within [`Z_TOL`], stay on it).
const CHANGED_TOL: f64 = 1e-6;

/// Something about a pad the user should know (shown with the element).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadWarning {
    /// The slopes were sampled more coarsely than usual (a large pad or long slopes).
    Coarse,
    /// The grading failed; the terrain is left as it was before this pad.
    Failed,
    /// The pad or its slopes reach past the terrain's boundary, where they stop.
    OffTerrain,
}

impl PadWarning {
    /// A stable code (engine output and translation keys `pad-warning-<code>`).
    pub fn code(self) -> &'static str {
        match self {
            PadWarning::Coarse => "coarse",
            PadWarning::Failed => "failed",
            PadWarning::OffTerrain => "off-terrain",
        }
    }
}

/// A slope sample: from a point of the outline out to where its slope meets
/// the ground (or the terrain's edge).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlopeRay {
    pub origin: DVec2,
    pub end: DVec2,
    /// The ground stands above the pad here (else it lies below or level).
    pub cut: bool,
    /// The outline edge it leaves from (`None` at a corner).
    pub edge: Option<usize>,
}

/// A graded pad.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PadGeom {
    /// The outline, counter-clockwise.
    pub outline: Vec<DVec2>,
    /// Absolute elevation.
    pub z: f64,
    /// Volume of earth removed and added.
    pub cut: f64,
    pub fill: f64,
    /// Area in plan.
    pub area: f64,
    /// Where the slopes meet the ground.
    pub daylight: Vec<Vec<DVec2>>,
    /// The slope samples, in order around the outline.
    pub rays: Vec<SlopeRay>,
    pub warnings: Vec<PadWarning>,
}

/// A surface being graded: triangles (absolute z, counter-clockwise from
/// above) and the elevation the original ground had at each of their vertices.
#[derive(Debug, Clone, Default)]
pub struct Surface {
    pub triangles: Vec<[DVec3; 3]>,
    pub original: Vec<[f64; 3]>,
}

impl Surface {
    pub fn new(triangles: Vec<[DVec3; 3]>) -> Self {
        let original = triangles.iter().map(|t| t.map(|p| p.z)).collect();
        Self { triangles, original }
    }

    /// The original ground where grading changed it (triangles at its elevations).
    pub fn changed(&self) -> Vec<[DVec3; 3]> {
        self.triangles
            .iter()
            .zip(&self.original)
            .filter(|(t, o)| (0..3).any(|i| (t[i].z - o[i]).abs() > CHANGED_TOL))
            .map(|(t, o)| [0, 1, 2].map(|i| t[i].truncate().extend(o[i])))
            .collect()
    }
}

/// What a pad grades: its outline, absolute elevation and slope ratios.
#[derive(Debug, Clone)]
pub struct PadInput<'a> {
    pub outline: &'a [DVec2],
    pub z: f64,
    pub cut_slope: f64,
    pub fill_slope: f64,
}

/// A point location over some triangles of a surface: a uniform grid of buckets.
struct Index<'a> {
    tris: &'a [[DVec3; 3]],
    lo: DVec2,
    cell: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<u32>>,
}

/// Most cells along each side of an index's grid.
const MAX_CELLS: usize = 512;
/// Smallest barycentric weight a new point keeps in its triangle (see [`Index::settle`]).
const SETTLE: f64 = 1e-9;

impl<'a> Index<'a> {
    fn new(tris: &'a [[DVec3; 3]], which: &[usize]) -> Self {
        let mut lo = DVec2::splat(f64::INFINITY);
        let mut hi = DVec2::splat(f64::NEG_INFINITY);
        for t in which.iter().filter_map(|&i| tris.get(i)) {
            for p in t {
                lo = lo.min(p.truncate());
                hi = hi.max(p.truncate());
            }
        }
        if !(lo.is_finite() && hi.is_finite()) {
            return Self { tris, lo: DVec2::ZERO, cell: 1.0, nx: 0, ny: 0, cells: Vec::new() };
        }
        let size = (hi - lo).max(DVec2::splat(1e-9));
        let n = which.len().max(1) as f64;
        let cell = ((size.x * size.y / n).sqrt() * 1.5).max(size.x / MAX_CELLS as f64).max(size.y / MAX_CELLS as f64);
        let cell = if cell.is_finite() && cell > 0.0 { cell } else { 1.0 };
        let nx = ((size.x / cell).floor() as usize + 1).min(MAX_CELLS);
        let ny = ((size.y / cell).floor() as usize + 1).min(MAX_CELLS);
        let mut index = Self { tris, lo, cell, nx, ny, cells: vec![Vec::new(); nx * ny] };
        for &i in which {
            let Some(t) = tris.get(i) else { continue };
            let tlo = t[0].truncate().min(t[1].truncate()).min(t[2].truncate());
            let thi = t[0].truncate().max(t[1].truncate()).max(t[2].truncate());
            let (x0, y0) = index.cell_of(tlo);
            let (x1, y1) = index.cell_of(thi);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    if let Some(c) = index.cells.get_mut(y * nx + x) {
                        c.push(i as u32);
                    }
                }
            }
        }
        index
    }

    fn cell_of(&self, p: DVec2) -> (usize, usize) {
        let c = (p - self.lo) / self.cell;
        let clamp = |v: f64, n: usize| (v.floor().max(0.0) as usize).min(n.saturating_sub(1));
        (clamp(c.x, self.nx), clamp(c.y, self.ny))
    }

    /// The triangle holding `p` (within `tol` of barycentric weight) and
    /// `p`'s weights in it; the best one when several are close.
    fn locate(&self, p: DVec2, tol: f64) -> Option<(usize, [f64; 3])> {
        if self.cells.is_empty() || !p.is_finite() {
            return None;
        }
        let (x, y) = self.cell_of(p);
        let mut best: Option<(usize, [f64; 3], f64)> = None;
        for &i in self.cells.get(y * self.nx + x)? {
            let Some(t) = self.tris.get(i as usize) else { continue };
            let Some(w) = barycentric(t, p) else { continue };
            let worst = w[0].min(w[1]).min(w[2]);
            if worst >= -tol && best.is_none_or(|b| worst > b.2) {
                best = Some((i as usize, w, worst));
            }
        }
        best.map(|(i, w, _)| (i, w))
    }

    /// `p` moved off the near-degenerate spots of its triangle: onto a
    /// vertex within [`SETTLE`] (in barycentric weight) of it, else at least
    /// that far inside from the edges. Points a rounding error off an edge
    /// would make slivers, along which constraints split badly.
    fn settle(&self, p: DVec2) -> Option<DVec2> {
        let (i, w) = self.locate(p, 1e-7)?;
        let t = self.tris.get(i)?;
        let small = w.iter().filter(|w| **w < SETTLE).count();
        if small >= 2 {
            let k = (0..3).max_by(|a, b| w[*a].total_cmp(&w[*b]))?;
            return Some(t[k].truncate());
        }
        if small == 0 {
            return Some(p);
        }
        let w = w.map(|w| w.max(SETTLE));
        let sum = w[0] + w[1] + w[2];
        Some((t[0].truncate() * w[0] + t[1].truncate() * w[1] + t[2].truncate() * w[2]) / sum)
    }

    fn z_at(&self, p: DVec2) -> Option<f64> {
        let (i, w) = self.locate(p, 1e-7)?;
        let t = self.tris.get(i)?;
        Some(w[0] * t[0].z + w[1] * t[1].z + w[2] * t[2].z)
    }
}

/// Barycentric weights of `p` in the plan triangle of `t`.
fn barycentric(t: &[DVec3; 3], p: DVec2) -> Option<[f64; 3]> {
    let (a, b, c) = (t[0].truncate(), t[1].truncate(), t[2].truncate());
    let (v0, v1, v2) = (b - a, c - a, p - a);
    let den = v0.perp_dot(v1);
    if den.abs() < 1e-18 || !den.is_finite() {
        return None;
    }
    let u = v2.perp_dot(v1) / den;
    let v = v0.perp_dot(v2) / den;
    Some([1.0 - u - v, u, v])
}

/// Plan distance from `p` to polygon `poly` (0 inside).
fn distance(poly: &[DVec2], p: DVec2) -> f64 {
    if polygon::contains(poly, p) {
        return 0.0;
    }
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-300)).clamp(0.0, 1.0);
            (a + ab * t).distance(p)
        })
        .fold(f64::INFINITY, f64::min)
}

/// `∫ max(g, 0)` over the plan triangle `p`, `g` linear with the vertex values given.
fn positive_integral(p: [DVec2; 3], g: [f64; 3]) -> f64 {
    let mut poly: Vec<(DVec2, f64)> = Vec::with_capacity(4);
    for i in 0..3 {
        let (a, ga) = (p[i], g[i]);
        let (b, gb) = (p[(i + 1) % 3], g[(i + 1) % 3]);
        if ga >= 0.0 {
            poly.push((a, ga));
        }
        if (ga >= 0.0) != (gb >= 0.0) {
            let s = ga / (ga - gb);
            poly.push((a.lerp(b, s), 0.0));
        }
    }
    let Some(&(o, go)) = poly.first() else { return 0.0 };
    poly.windows(2)
        .skip(1)
        .map(|w| ((w[0].0 - o).perp_dot(w[1].0 - o) * 0.5).abs() * (go + w[0].1 + w[1].1) / 3.0)
        .sum()
}

/// The outline counter-clockwise, without repeated consecutive points.
fn clean_outline(outline: &[DVec2]) -> Vec<DVec2> {
    let mut out: Vec<DVec2> = Vec::with_capacity(outline.len());
    for p in outline.iter().filter(|p| p.is_finite()) {
        if out.last().is_none_or(|q| q.distance(*p) > 1e-9) {
            out.push(*p);
        }
    }
    while out.len() > 1 && out[0].distance(out[out.len() - 1]) <= 1e-9 {
        out.pop();
    }
    if polygon::signed_area(&out) < 0.0 {
        out.reverse();
    }
    out
}

/// A ray out of the outline: origin, unit direction and the edge it leaves from.
type Ray = (DVec2, DVec2, Option<usize>);

/// Rays out of `outline` (ccw), in order around it: a fan at each convex
/// corner (every `corner_step` radians), the bisector at each concave one,
/// and along each edge every `step` meters.
fn rays(outline: &[DVec2], step: f64, corner_step: f64) -> Vec<Ray> {
    let n = outline.len();
    let normal = |u: DVec2| DVec2::new(u.y, -u.x);
    let mut out = Vec::new();
    for i in 0..n {
        let (prev, a, b) = (outline[(i + n - 1) % n], outline[i], outline[(i + 1) % n]);
        let (u_prev, u) = ((a - prev).normalize_or_zero(), (b - a).normalize_or_zero());
        let (n_prev, n_i) = (normal(u_prev), normal(u));
        let turn = u_prev.perp_dot(u);
        if turn > 1e-12 {
            let angle = n_prev.angle_to(n_i);
            let steps = ((angle / corner_step).ceil() as usize).clamp(1, 64);
            for k in 0..=steps {
                let dir = DVec2::from_angle(angle * k as f64 / steps as f64).rotate(n_prev);
                out.push((a, dir, None));
            }
        } else if turn < -1e-12 {
            out.push((a, (n_prev + n_i).normalize_or(n_i), None));
        } else {
            out.push((a, n_i, None));
        }
        let len = a.distance(b);
        let m = ((len / step).ceil() as usize).clamp(1, 1 << 16);
        for k in 1..m {
            out.push((a.lerp(b, k as f64 / m as f64), n_i, Some(i)));
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Regime {
    Cut,
    Fill,
    Ground,
}

#[derive(Debug, Clone, Copy, Default)]
struct V {
    position: Point2<f64>,
}

impl HasPosition for V {
    type Scalar = f64;
    fn position(&self) -> Point2<f64> {
        self.position
    }
}

/// Coordinates spade accepts: tiny values are zero (it refuses them otherwise).
fn vertex(p: DVec2) -> V {
    let s = |v: f64| if v.abs() < 1e-30 { 0.0 } else { v };
    V { position: Point2::new(s(p.x), s(p.y)) }
}

fn key(p: DVec2) -> (i64, i64) {
    ((p.x * 1e9).round() as i64, (p.y * 1e9).round() as i64)
}

/// Grade `pad` into `surface`. A failure leaves the surface as it was, with a warning.
pub fn grade(surface: &mut Surface, pad: &PadInput) -> PadGeom {
    let outline = clean_outline(pad.outline);
    let mut geom = PadGeom {
        outline: outline.clone(),
        z: pad.z,
        area: polygon::signed_area(&outline).abs(),
        ..Default::default()
    };
    let ok = |r: f64| r.is_finite() && r > 0.0;
    if outline.len() < 3 || !pad.z.is_finite() || !ok(pad.cut_slope) || !ok(pad.fill_slope) {
        geom.warnings.push(PadWarning::Failed);
        return geom;
    }
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| grade_inner(surface, pad, &outline)));
    match built {
        Ok(Some((next, g))) => {
            *surface = next;
            PadGeom { outline, z: pad.z, area: geom.area, ..g }
        }
        Ok(None) => {
            geom.warnings.push(PadWarning::OffTerrain);
            geom
        }
        Err(_) => {
            geom.warnings.push(PadWarning::Failed);
            geom
        }
    }
}

/// The graded surface and the pad's figures, or `None` when the pad does not touch the surface.
fn grade_inner(surface: &Surface, pad: &PadInput, outline: &[DVec2]) -> Option<(Surface, PadGeom)> {
    let (z_p, cut, fill) = (pad.z, pad.cut_slope, pad.fill_slope);
    let zs = surface.triangles.iter().flatten().map(|p| p.z);
    let (min_z, max_z) = zs.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), z| (lo.min(z), hi.max(z)));
    if !(min_z.is_finite() && max_z.is_finite()) {
        return None;
    }
    // Past this distance neither slope reaches the ground: C > max_z, L < min_z.
    let reach = (cut * (max_z - z_p)).max(fill * (z_p - min_z)).max(0.0) + 1e-6;
    let (mut lo, mut hi) = (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY));
    for p in outline {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let (lo, hi) = (lo - DVec2::splat(reach), hi + DVec2::splat(reach));
    let region: Vec<usize> = surface
        .triangles
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            let tlo = t[0].truncate().min(t[1].truncate()).min(t[2].truncate());
            let thi = t[0].truncate().max(t[1].truncate()).max(t[2].truncate());
            tlo.x <= hi.x && tlo.y <= hi.y && thi.x >= lo.x && thi.y >= lo.y
        })
        .map(|(i, _)| i)
        .collect();
    if region.is_empty() {
        return None;
    }
    let index = Index::new(&surface.triangles, &region);
    let mut warnings = Vec::new();

    // Sample distances along each ray: 0, the rings, the reach.
    let mut rings = vec![0.0];
    let mut d = FIRST_RING;
    while d < reach && rings.len() <= MAX_RINGS {
        rings.push(d);
        d *= 2.0;
    }
    if d < reach {
        warnings.push(PadWarning::Coarse);
    }
    rings.push(reach);
    // Coarser rays until the samples fit.
    let (mut step, mut corner) = (EDGE_STEP, CORNER_STEP_DEG.to_radians());
    let mut all = rays(outline, step, corner);
    for _ in 0..24 {
        if all.len() * (rings.len() + 2) <= MAX_PAD_VERTICES {
            break;
        }
        (step, corner) = (step * 2.0, (corner * 2.0).min(std::f64::consts::FRAC_PI_2));
        all = rays(outline, step, corner);
        if !warnings.contains(&PadWarning::Coarse) {
            warnings.push(PadWarning::Coarse);
        }
    }

    let t_at = |p: DVec2| index.z_at(p);
    let regime = |p: DVec2| -> Option<Regime> {
        let t = t_at(p)?;
        let d = distance(outline, p);
        Some(if t > z_p + d / cut + Z_TOL {
            Regime::Cut
        } else if t < z_p - d / fill - Z_TOL {
            Regime::Fill
        } else {
            Regime::Ground
        })
    };
    // The last distance in (a, b) still like `a` (bisection on `same`).
    let bisect = |mut a: f64, mut b: f64, same: &dyn Fn(f64) -> bool| {
        for _ in 0..BISECTION_STEPS {
            let m = 0.5 * (a + b);
            if same(m) {
                a = m;
            } else {
                b = m;
            }
        }
        0.5 * (a + b)
    };

    let mut points: Vec<DVec2> = Vec::new();
    let mut out_rays: Vec<Option<SlopeRay>> = Vec::with_capacity(all.len());
    let mut off = false;
    for &(o, dir, edge) in &all {
        let Some(t0) = t_at(o) else {
            off = true;
            out_rays.push(None);
            continue;
        };
        let at = |s: f64| o + dir * s;
        let mut end: Option<DVec2> = None;
        let mut prev = (0.0, regime(o).unwrap_or(Regime::Ground));
        if prev.1 == Regime::Ground {
            end = Some(o);
        }
        for &s in rings.iter().skip(1) {
            let r = regime(at(s));
            match r {
                None => {
                    // Off the terrain: stop at its edge.
                    let s_edge = bisect(prev.0, s, &|m| t_at(at(m)).is_some());
                    let p = at(s_edge);
                    if prev.1 != Regime::Ground {
                        off = true;
                        end.get_or_insert(p);
                        points.push(p);
                    }
                    break;
                }
                Some(r) if r != prev.1 => {
                    let r0 = prev.1;
                    let s_x = bisect(prev.0, s, &|m| regime(at(m)) == Some(r0));
                    let p = at(s_x);
                    points.push(p);
                    if r == Regime::Ground {
                        end.get_or_insert(p);
                    }
                }
                Some(_) => {}
            }
            if r != Some(Regime::Ground) {
                points.push(at(s));
            }
            prev = (s, r.unwrap_or(Regime::Ground));
        }
        let end = end.unwrap_or_else(|| at(prev.0));
        out_rays.push(Some(SlopeRay { origin: o, end, cut: t0 > z_p + Z_TOL, edge }));
    }
    if off {
        warnings.push(PadWarning::OffTerrain);
    }

    // Daylight lines: the rays' ends, joined between neighbors.
    let mut daylight: Vec<Vec<DVec2>> = Vec::new();
    let mut line: Vec<DVec2> = Vec::new();
    for r in &out_rays {
        match r {
            Some(r) => {
                if line.last().is_none_or(|q: &DVec2| q.distance(r.end) > 1e-9) {
                    line.push(r.end);
                }
            }
            None => {
                if line.len() > 1 {
                    daylight.push(std::mem::take(&mut line));
                }
                line.clear();
            }
        }
    }
    let closed = out_rays.iter().all(Option::is_some);
    if closed && line.len() > 2 {
        line.push(line[0]);
    }
    if line.len() > 1 {
        daylight.push(line);
    }

    // The triangulation of the region.
    let mut cdt = ConstrainedDelaunayTriangulation::<V>::new();
    let mut handles = std::collections::HashMap::new();
    let mut handle = |cdt: &mut ConstrainedDelaunayTriangulation<V>, p: DVec2| {
        if !p.is_finite() {
            return None;
        }
        if let Some(h) = handles.get(&key(p)) {
            return Some(*h);
        }
        let h = cdt.insert(vertex(p)).ok()?;
        handles.insert(key(p), h);
        Some(h)
    };
    let mut edges = std::collections::HashSet::new();
    let mut constraints = Vec::new();
    for &i in &region {
        let Some(t) = surface.triangles.get(i) else { continue };
        let hs = [0, 1, 2].map(|k| handle(&mut cdt, t[k].truncate()));
        for k in 0..3 {
            let (a, b) = (t[k].truncate(), t[(k + 1) % 3].truncate());
            let e = if key(a) < key(b) { (key(a), key(b)) } else { (key(b), key(a)) };
            if let (Some(ha), Some(hb)) = (hs[k], hs[(k + 1) % 3])
                && edges.insert(e)
            {
                constraints.push((ha, hb));
            }
        }
    }
    for (a, b) in constraints {
        if a != b {
            cdt.add_constraint_and_split(a, b, |p| V { position: p });
        }
    }
    for p in &points {
        if let Some(p) = index.settle(*p) {
            handle(&mut cdt, p);
        }
    }
    // The outline between neighboring ray origins on the terrain, and the daylight lines.
    let mut lines: Vec<(DVec2, DVec2)> = Vec::new();
    let origins: Vec<Option<DVec2>> = out_rays.iter().map(|r| r.map(|r| r.origin)).collect();
    let n = origins.len();
    for i in 0..n {
        if let (Some(Some(a)), Some(Some(b))) = (origins.get(i), origins.get((i + 1) % n)) {
            lines.push((*a, *b));
        }
    }
    for l in &daylight {
        lines.extend(l.windows(2).map(|w| (w[0], w[1])));
    }
    for (a, b) in lines {
        if a.distance(b) <= 1e-9 || t_at(0.5 * (a + b)).is_none() {
            continue;
        }
        let (Some(a), Some(b)) = (index.settle(a), index.settle(b)) else { continue };
        if let (Some(ha), Some(hb)) = (handle(&mut cdt, a), handle(&mut cdt, b))
            && ha != hb
        {
            cdt.add_constraint_and_split(ha, hb, |p| V { position: p });
        }
    }

    // Each vertex: the surface before (and the original ground) and after.
    let mut values: Vec<Option<Option<(f64, f64, f64)>>> = vec![None; cdt.num_vertices()];
    let mut value = |i: usize, p: DVec2| -> Option<(f64, f64, f64)> {
        if let Some(Some(v)) = values.get(i) {
            return *v;
        }
        let v = index.locate(p, 1e-7).and_then(|(ti, w)| {
            let t = surface.triangles.get(ti)?;
            let o = surface.original.get(ti)?;
            let before = w[0] * t[0].z + w[1] * t[1].z + w[2] * t[2].z;
            let original = w[0] * o[0] + w[1] * o[1] + w[2] * o[2];
            let d = distance(outline, p);
            let after = (z_p - d / fill).max(before.min(z_p + d / cut));
            Some((before, original, after))
        });
        if let Some(slot) = values.get_mut(i) {
            *slot = Some(v);
        }
        v
    };
    let mut keep = vec![true; surface.triangles.len()];
    for &i in &region {
        if let Some(k) = keep.get_mut(i) {
            *k = false;
        }
    }
    let mut next = Surface::default();
    for (t, o) in surface.triangles.iter().zip(&surface.original).zip(&keep).filter(|(_, k)| **k).map(|(x, _)| x) {
        next.triangles.push(*t);
        next.original.push(*o);
    }
    let (mut cut_v, mut fill_v) = (0.0, 0.0);
    for f in cdt.inner_faces() {
        let vs = f.vertices();
        let ps = vs.map(|v| DVec2::new(v.position().x, v.position().y));
        let centroid = (ps[0] + ps[1] + ps[2]) / 3.0;
        if index.locate(centroid, 1e-9).is_none() {
            continue;
        }
        let [Some(a), Some(b), Some(c)] = [0, 1, 2].map(|k| value(vs[k].fix().index(), ps[k])) else { continue };
        let vals = [a, b, c];
        if (ps[1] - ps[0]).perp_dot(ps[2] - ps[0]).abs() < 1e-18 {
            continue;
        }
        cut_v += positive_integral(ps, vals.map(|v| v.0 - v.2));
        fill_v += positive_integral(ps, vals.map(|v| v.2 - v.0));
        next.triangles.push([0, 1, 2].map(|k| ps[k].extend(vals[k].2)));
        next.original.push(vals.map(|v| v.1));
    }
    let rays: Vec<SlopeRay> = out_rays.into_iter().flatten().collect();
    let geom = PadGeom { cut: cut_v, fill: fill_v, daylight, rays, warnings, ..Default::default() };
    Some((next, geom))
}

/// The pad of terrain `terrain` whose outline holds plan point `p` (the last
/// applied wins), so a click on the graded earth picks the pad there.
pub fn pad_at(project: &cc_core::Project, terrain: cc_core::ElementId, p: DVec2) -> Option<cc_core::ElementId> {
    project.pads_of(terrain).filter(|(_, pad)| polygon::contains(&pad.outline, p)).map(|(e, _)| e.id).last()
}

/// The boundary of `triangles` (their unshared edges) as one loop,
/// counter-clockwise, or `None` when they do not make exactly one.
pub fn mesh_boundary(triangles: &[[DVec3; 3]]) -> Option<Vec<DVec3>> {
    use std::collections::HashMap;
    type Key = (i64, i64);
    let mut count: HashMap<(Key, Key), (usize, DVec2, DVec2)> = HashMap::new();
    let mut z: HashMap<(i64, i64), f64> = HashMap::new();
    for t in triangles {
        for k in 0..3 {
            let (a, b) = (t[k].truncate(), t[(k + 1) % 3].truncate());
            z.insert(key(a), t[k].z);
            let e = if key(a) < key(b) { (key(a), key(b)) } else { (key(b), key(a)) };
            count.entry(e).or_insert((0, a, b)).0 += 1;
        }
    }
    let open: Vec<(DVec2, DVec2)> = count.values().filter(|c| c.0 == 1).map(|c| (c.1, c.2)).collect();
    let mut loops = crate::topography::chain(open);
    if loops.len() != 1 {
        return None;
    }
    let mut l = loops.pop()?;
    if l.len() > 1 && key(l[0]) == key(l[l.len() - 1]) {
        l.pop();
    }
    if l.len() < 3 {
        return None;
    }
    if polygon::signed_area(&l) < 0.0 {
        l.reverse();
    }
    l.iter().map(|p| z.get(&key(*p)).map(|z| p.extend(*z))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plane z = x / 10 over [-50, 50]², as a grid of triangles every 5 m.
    fn plane() -> Surface {
        let f = |x: f64, y: f64| DVec3::new(x, y, x / 10.0);
        let mut tris = Vec::new();
        for i in -10..10 {
            for j in -10..10 {
                let (x0, y0, x1, y1) = (i as f64 * 5.0, j as f64 * 5.0, i as f64 * 5.0 + 5.0, j as f64 * 5.0 + 5.0);
                tris.push([f(x0, y0), f(x1, y0), f(x1, y1)]);
                tris.push([f(x0, y0), f(x1, y1), f(x0, y1)]);
            }
        }
        Surface::new(tris)
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<DVec2> {
        vec![DVec2::new(x0, y0), DVec2::new(x1, y0), DVec2::new(x1, y1), DVec2::new(x0, y1)]
    }

    fn z_at(s: &Surface, p: DVec2) -> Option<f64> {
        let all: Vec<usize> = (0..s.triangles.len()).collect();
        Index::new(&s.triangles, &all).z_at(p)
    }

    fn plan_area(s: &Surface) -> f64 {
        s.triangles.iter().map(|t| (t[1] - t[0]).truncate().perp_dot((t[2] - t[0]).truncate()) * 0.5).sum()
    }

    #[test]
    fn a_band_across_an_inclined_plane_has_exact_slopes_and_volumes() {
        // A pad across the whole terrain in y (its corners are off it), from
        // x = -10 to 10 at z = 0: ground 1 m above its high edge, 1 m below its low one.
        let mut s = plane();
        let outline = rect(-10.0, -60.0, 10.0, 60.0);
        let (cut, fill) = (1.0, 1.5);
        let g = grade(&mut s, &PadInput { outline: &outline, z: 0.0, cut_slope: cut, fill_slope: fill });
        assert!(g.warnings.contains(&PadWarning::OffTerrain), "{:?}", g.warnings);
        // Daylight at Δz·s / (1 - s/10) from the edges.
        let (dc, df) = (1.0 * cut / (1.0 - cut / 10.0), 1.0 * fill / (1.0 - fill / 10.0));
        for r in g.rays.iter().filter(|r| r.edge.is_some() && r.origin.y.abs() < 49.0) {
            let d = r.end.distance(r.origin);
            let want = if r.cut { dc } else { df };
            assert!((d - want).abs() < 1e-6, "{r:?} {d} {want}");
        }
        // Inside: flat; on the slopes: z_p ± d/s; beyond: the ground.
        let at = |x: f64| z_at(&s, DVec2::new(x, 3.3)).unwrap_or(f64::NAN);
        assert!(at(0.0).abs() < 1e-9 && at(-9.0).abs() < 1e-9 && at(9.0).abs() < 1e-9);
        assert!((at(10.5) - 0.5 / cut).abs() < 1e-9, "{}", at(10.5));
        assert!((at(-10.5) + 0.5 / fill).abs() < 1e-9, "{}", at(-10.5));
        assert!((at(20.0) - 2.0).abs() < 1e-9 && (at(-20.0) + 2.0).abs() < 1e-9);
        // Volumes per meter of width: the pad's own triangles plus the slope's.
        let w = 100.0;
        let want_cut = w * (0.5 * 10.0 * 1.0 + 0.5 * 1.0 * dc);
        let want_fill = w * (0.5 * 10.0 * 1.0 + 0.5 * 1.0 * df);
        assert!((g.cut - want_cut).abs() < 1e-6, "{} {}", g.cut, want_cut);
        assert!((g.fill - want_fill).abs() < 1e-6, "{} {}", g.fill, want_fill);
        // Nothing lost or overlapping.
        assert!((plan_area(&s) - 10_000.0).abs() < 1e-6, "{}", plan_area(&s));
        // The original ground is kept where it changed.
        assert!(s.changed().iter().flatten().all(|p| (p.z - p.x / 10.0).abs() < 1e-9));
        assert!(mesh_boundary(&s.triangles).is_some_and(|b| b.len() >= 4));
    }

    #[test]
    fn single_sided_pads_only_cut_or_fill() {
        let outline = rect(-5.0, -5.0, 5.0, 5.0);
        let mut s = plane();
        let g = grade(&mut s, &PadInput { outline: &outline, z: -3.0, cut_slope: 1.0, fill_slope: 1.5 });
        assert!(g.cut > 0.0 && g.fill.abs() < 1e-9, "{} {}", g.cut, g.fill);
        assert!(g.rays.iter().all(|r| r.cut));
        let mut s = plane();
        let g = grade(&mut s, &PadInput { outline: &outline, z: 3.0, cut_slope: 1.0, fill_slope: 1.5 });
        assert!(g.fill > 0.0 && g.cut.abs() < 1e-9, "{} {}", g.cut, g.fill);
        assert!(g.warnings.is_empty(), "{:?}", g.warnings);
        assert!(g.daylight.len() == 1 && g.daylight[0].first() == g.daylight[0].last());
    }

    #[test]
    fn a_convex_corner_grades_a_cone() {
        // Flat ground at 0, a pad 2 m up: at the corner the fill slope is a cone.
        let flat = Surface::new(plane().triangles.iter().map(|t| t.map(|p| p.truncate().extend(0.0))).collect());
        let mut s = flat;
        let outline = rect(-5.0, -5.0, 5.0, 5.0);
        let g = grade(&mut s, &PadInput { outline: &outline, z: 2.0, cut_slope: 1.0, fill_slope: 1.5 });
        // The daylight on the bisector is 3 m from the corner.
        let corner = DVec2::new(5.0, 5.0);
        let bis = g
            .rays
            .iter()
            .filter(|r| r.edge.is_none() && r.origin == corner)
            .min_by(|a, b| {
                let ang = |r: &&SlopeRay| (r.end - r.origin).angle_to(DVec2::ONE).abs();
                ang(a).total_cmp(&ang(b))
            })
            .copied();
        let Some(bis) = bis else { panic!("no corner ray") };
        assert!((bis.end.distance(corner) - 3.0).abs() < 1e-6, "{bis:?}");
        let at = z_at(&s, corner + DVec2::ONE.normalize() * 2.0).unwrap_or(f64::NAN);
        assert!((at - (2.0 - 2.0 / 1.5)).abs() < 1e-6, "{at}");
    }

    #[test]
    fn overlapping_pads_apply_in_order() {
        let mut s = plane();
        let a = rect(-5.0, -5.0, 5.0, 5.0);
        let b = rect(0.0, 0.0, 8.0, 8.0);
        grade(&mut s, &PadInput { outline: &a, z: 0.0, cut_slope: 1.0, fill_slope: 1.5 });
        grade(&mut s, &PadInput { outline: &b, z: 0.4, cut_slope: 1.0, fill_slope: 1.5 });
        // The second pad wins where they overlap.
        assert!((z_at(&s, DVec2::new(2.0, 2.0)).unwrap_or(f64::NAN) - 0.4).abs() < 1e-9);
        assert!(z_at(&s, DVec2::new(-4.0, -4.0)).unwrap_or(f64::NAN).abs() < 1e-9);
        assert!((plan_area(&s) - 10_000.0).abs() < 1e-6);
    }

    #[test]
    fn measured_points_outside_the_changed_area_stay() {
        let mut s = plane();
        let before: Vec<DVec3> = s.triangles.iter().flatten().copied().collect();
        let outline = rect(-5.0, -5.0, 5.0, 5.0);
        grade(&mut s, &PadInput { outline: &outline, z: 0.0, cut_slope: 1.0, fill_slope: 1.5 });
        let after: std::collections::HashSet<_> =
            s.triangles.iter().flatten().map(|p| (key(p.truncate()), (p.z * 1e9).round() as i64)).collect();
        for p in before.iter().filter(|p| p.truncate().distance(DVec2::ZERO) > 20.0) {
            assert!(after.contains(&(key(p.truncate()), (p.z * 1e9).round() as i64)), "{p}");
        }
    }

    #[test]
    fn degenerate_pads_never_panic() {
        let cases: Vec<(Vec<DVec2>, f64, f64)> = vec![
            (rect(0.0, 0.0, 1e-12, 1e-12), 0.0, 1.0),
            (vec![DVec2::new(f64::NAN, 0.0), DVec2::X, DVec2::Y], 0.0, 1.0),
            (rect(500.0, 500.0, 510.0, 510.0), 0.0, 1.0),
            (rect(-5.0, -5.0, 5.0, 5.0), f64::INFINITY, 1.0),
            (rect(-5.0, -5.0, 5.0, 5.0), 0.0, 0.0),
            (rect(-5.0, -5.0, 5.0, 5.0), 1e6, 100.0),
        ];
        for (outline, z, slope) in cases {
            let mut s = plane();
            let g = grade(&mut s, &PadInput { outline: &outline, z, cut_slope: slope, fill_slope: slope });
            assert!(!g.warnings.is_empty() || g.cut + g.fill >= 0.0);
            assert!((plan_area(&s) - 10_000.0).abs() < 1e-6, "{outline:?} {}", plan_area(&s));
        }
        // A pad off the terrain says so.
        let mut s = plane();
        let far = rect(500.0, 500.0, 510.0, 510.0);
        let g = grade(&mut s, &PadInput { outline: &far, z: 0.0, cut_slope: 1.0, fill_slope: 1.0 });
        assert_eq!(g.warnings, [PadWarning::OffTerrain]);
    }

    #[test]
    fn positive_parts_integrate_exactly() {
        let p = [DVec2::ZERO, DVec2::X, DVec2::Y];
        assert!((positive_integral(p, [1.0, 1.0, 1.0]) - 0.5).abs() < 1e-12);
        assert_eq!(positive_integral(p, [-1.0, -1.0, -1.0]), 0.0);
        // g = 1 - 2x: positive for x < 1/2.
        let g = [1.0, -1.0, 1.0];
        let whole = positive_integral(p, g) - positive_integral(p, g.map(|v| -v));
        assert!((whole - 0.5 * (1.0 - 2.0 / 3.0)).abs() < 1e-12, "{whole}");
    }
}
