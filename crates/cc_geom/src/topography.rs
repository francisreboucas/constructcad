//! Terrain surfaces (`cc_core::topography`): a constrained Delaunay
//! triangulation (`spade`) of the measured points inside the boundary, with
//! the boundary's edges as constraints, so non-convex boundaries keep no
//! triangle outside. Boundary vertices take the elevation all the points
//! give them, those beyond the boundary too (linear over the points'
//! triangulation, the nearest point's beyond it). The surface closes down to a flat base into a solid; contours cut it
//! at multiples of an interval.

use crate::polygon;
use crate::solid::{Face, Solid};
use cc_core::topography::Topography;
use cc_core::{Element, Project};
use glam::{DVec2, DVec3};
use spade::{
    ConstrainedDelaunayTriangulation, DelaunayTriangulation, FloatTriangulation, HasPosition, Point2, Triangulation,
};

/// Most contour lines drawn for one surface: the interval doubles until fewer are left.
pub const MAX_CONTOURS: usize = 500;

/// Something about a surface the user should know (shown with the element).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopoWarning {
    /// Fewer than three points inside the boundary: the surface is flat or a plane through them.
    FewPoints,
    /// The points inside the boundary lie on one line.
    Collinear,
    /// Some boundary edges cross others and were left out of the surface.
    BoundaryCrossing,
    /// The triangulation failed; the surface is flat at the level.
    Failed,
}

impl TopoWarning {
    /// A stable code (engine output and translation keys `topo-warning-<code>`).
    pub fn code(self) -> &'static str {
        match self {
            TopoWarning::FewPoints => "few-points",
            TopoWarning::Collinear => "collinear",
            TopoWarning::BoundaryCrossing => "boundary-crossing",
            TopoWarning::Failed => "failed",
        }
    }
}

/// A built terrain surface, in absolute elevations.
#[derive(Debug, Clone, PartialEq)]
pub struct TopoGeom {
    /// Surface triangles, counter-clockwise seen from above.
    pub triangles: Vec<[DVec3; 3]>,
    /// The boundary, counter-clockwise, with each vertex's elevation.
    pub boundary: Vec<DVec3>,
    pub min_z: f64,
    pub max_z: f64,
    /// Elevation of the flat base.
    pub base_z: f64,
    /// Area in plan.
    pub area: f64,
    /// Volume between the surface and the base.
    pub volume: f64,
    pub warnings: Vec<TopoWarning>,
}

/// A contour line: the surface cut at elevation `z`.
#[derive(Debug, Clone, PartialEq)]
pub struct Contour {
    pub z: f64,
    /// Every `major_every`-th contour (elevations that are multiples of the major interval).
    pub major: bool,
    /// Plan points; closed when the first and last coincide.
    pub points: Vec<DVec2>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Vertex {
    position: Point2<f64>,
    z: f64,
}

impl HasPosition for Vertex {
    type Scalar = f64;
    fn position(&self) -> Point2<f64> {
        self.position
    }
}

/// Coordinates spade accepts: tiny values are zero (it refuses them otherwise).
fn spade_vertex(p: DVec2, z: f64) -> Vertex {
    let s = |v: f64| if v.abs() < 1e-30 { 0.0 } else { v };
    Vertex { position: Point2::new(s(p.x), s(p.y)), z }
}

/// Elevation of each boundary vertex from the points (relative z): linear
/// over their triangulation, the nearest point's beyond it (so with one or
/// two points too), 0 with none.
fn boundary_elevations(boundary: &[DVec2], points: &[DVec3], warnings: &mut Vec<TopoWarning>) -> Vec<f64> {
    let mut dt = DelaunayTriangulation::<Vertex>::new();
    for p in points {
        // A point spade refuses (out of its range) is left out.
        let _ = dt.insert(spade_vertex(p.truncate(), p.z));
    }
    if dt.num_vertices() < 3 {
        warnings.push(TopoWarning::FewPoints);
    } else if dt.num_inner_faces() == 0 {
        warnings.push(TopoWarning::Collinear);
    }
    let linear = dt.barycentric();
    boundary
        .iter()
        .map(|b| {
            let at = spade_vertex(*b, 0.0).position;
            linear
                .interpolate(|v| v.data().z, at)
                .filter(|z| z.is_finite())
                .or_else(|| dt.nearest_neighbor(at).map(|v| v.data().z))
                .unwrap_or(0.0)
        })
        .collect()
}

/// Whether `p` lies inside `poly` or on its edges (within the point tolerance).
fn inside_or_on(poly: &[DVec2], p: DVec2) -> bool {
    let n = poly.len();
    polygon::contains(poly, p) || (0..n).any(|i| on_segment(poly[i], poly[(i + 1) % n], p).is_some())
}

/// Where `p` lies along segment `a`–`b` (0 at `a`, 1 at `b`), when it is
/// on the segment within the point tolerance.
fn on_segment(a: DVec2, b: DVec2, p: DVec2) -> Option<f64> {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-300)).clamp(0.0, 1.0);
    ((a + ab * t).distance(p) <= cc_core::topography::POINT_TOL).then_some(t)
}

/// The boundary counter-clockwise, without repeated consecutive points.
fn clean_boundary(boundary: &[DVec2]) -> Vec<DVec2> {
    let mut out: Vec<DVec2> = Vec::with_capacity(boundary.len());
    for p in boundary {
        if out.last().is_none_or(|q| q.distance(*p) > cc_core::topography::POINT_TOL) {
            out.push(*p);
        }
    }
    while out.len() > 1 && out[0].distance(out[out.len() - 1]) <= cc_core::topography::POINT_TOL {
        out.pop();
    }
    if polygon::signed_area(&out) < 0.0 {
        out.reverse();
    }
    out
}

/// Splits the points that reach the boundary off the inner ones: a point on
/// an edge becomes a boundary vertex with its own elevation (so the sides
/// of the solid follow the surface's edge), and one on a boundary vertex
/// only sets that vertex's elevation. Returns the boundary, its relative
/// elevations and the points strictly inside.
fn split_on_boundary(boundary: &[DVec2], boundary_z: &[f64], inside: &[DVec3]) -> (Vec<DVec2>, Vec<f64>, Vec<DVec3>) {
    let tol = cc_core::topography::POINT_TOL;
    let n = boundary.len();
    let mut corner_z: Vec<Option<f64>> = vec![None; n];
    let mut on_edge: Vec<Vec<(f64, DVec3)>> = vec![Vec::new(); n];
    let mut inner = Vec::with_capacity(inside.len());
    for p in inside {
        let q = p.truncate();
        if let Some(i) = boundary.iter().position(|b| b.distance(q) <= tol) {
            corner_z[i].get_or_insert(p.z);
            continue;
        }
        let edge = (0..n).find_map(|i| on_segment(boundary[i], boundary[(i + 1) % n], q).map(|t| (i, t)));
        match edge {
            Some((i, t)) => on_edge[i].push((t, *p)),
            None => inner.push(*p),
        }
    }
    let mut out_b = Vec::with_capacity(n);
    let mut out_z = Vec::with_capacity(n);
    for i in 0..n {
        out_b.push(boundary[i]);
        out_z.push(corner_z[i].unwrap_or(boundary_z.get(i).copied().unwrap_or(0.0)));
        let mut along = std::mem::take(&mut on_edge[i]);
        along.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, p) in along {
            // Points closer than the tolerance along the edge count once.
            if out_b.last().is_some_and(|l: &DVec2| l.distance(p.truncate()) <= tol) {
                continue;
            }
            out_b.push(p.truncate());
            out_z.push(p.z);
        }
    }
    (out_b, out_z, inner)
}

/// Surface triangles (relative z) of the points inside `boundary` (ccw)
/// with the boundary elevations `boundary_z`.
fn surface(
    boundary: &[DVec2],
    boundary_z: &[f64],
    inner: &[DVec3],
    warnings: &mut Vec<TopoWarning>,
) -> Vec<[DVec3; 3]> {
    let mut cdt = ConstrainedDelaunayTriangulation::<Vertex>::new();
    let mut handles = Vec::with_capacity(boundary.len());
    for (p, z) in boundary.iter().zip(boundary_z) {
        match cdt.insert(spade_vertex(*p, *z)) {
            Ok(h) => handles.push(h),
            Err(_) => {
                warnings.push(TopoWarning::Failed);
                return Vec::new();
            }
        }
    }
    for p in inner {
        let _ = cdt.insert(spade_vertex(p.truncate(), p.z));
    }
    let n = handles.len();
    let mut crossing = false;
    for i in 0..n {
        let (a, b) = (handles[i], handles[(i + 1) % n]);
        if a != b && cdt.try_add_constraint(a, b).is_empty() && !cdt.exists_constraint(a, b) {
            crossing = true;
        }
    }
    if crossing {
        warnings.push(TopoWarning::BoundaryCrossing);
    }
    let point = |v: spade::handles::VertexHandle<Vertex, (), spade::CdtEdge<()>, ()>| {
        let p = v.position();
        DVec3::new(p.x, p.y, v.data().z)
    };
    cdt.inner_faces()
        .map(|f| f.vertices().map(point))
        .filter(|t| polygon::contains(boundary, ((t[0] + t[1] + t[2]) / 3.0).truncate()))
        .collect()
}

/// The surface of terrain `t` of element `e`, or `None` when its boundary is invalid.
pub fn topo_geom(project: &Project, e: &Element, t: &Topography) -> Option<TopoGeom> {
    if !t.is_valid() {
        return None;
    }
    let level = project.level_elevation(e.level);
    let boundary = clean_boundary(&t.boundary);
    if boundary.len() < 3 {
        return None;
    }
    let points = cc_core::topography::clean_points(&t.points);
    let inner: Vec<DVec3> = points.iter().copied().filter(|p| inside_or_on(&boundary, p.truncate())).collect();
    let mut warnings = Vec::new();
    // Spade is trusted not to panic, but a model rebuilds on every edit:
    // a failure becomes a flat surface and a warning, never a crash.
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut w = Vec::new();
        // Points surveyed beyond the boundary still say how high it runs.
        let boundary_z = boundary_elevations(&boundary, &points, &mut w);
        let (edge, boundary_z, inner) = split_on_boundary(&boundary, &boundary_z, &inner);
        let tris = surface(&edge, &boundary_z, &inner, &mut w);
        (edge, boundary_z, tris, w)
    }));
    let (boundary, boundary_z, mut triangles) = match built {
        Ok((edge, boundary_z, tris, w)) if !tris.is_empty() => {
            warnings = w;
            (edge, boundary_z, tris)
        }
        Ok((_, _, _, w)) => {
            warnings = w;
            if !warnings.contains(&TopoWarning::Failed) {
                warnings.push(TopoWarning::Failed);
            }
            let (boundary_z, tris) = flat_surface(&boundary);
            (boundary, boundary_z, tris)
        }
        Err(_) => {
            warnings.push(TopoWarning::Failed);
            let (boundary_z, tris) = flat_surface(&boundary);
            (boundary, boundary_z, tris)
        }
    };
    for tri in &mut triangles {
        tri.iter_mut().for_each(|p| p.z += level);
    }
    let boundary: Vec<DVec3> = boundary.iter().zip(&boundary_z).map(|(p, z)| p.extend(z + level)).collect();
    let zs = triangles.iter().flatten().map(|p| p.z).chain(boundary.iter().map(|p| p.z));
    let (min_z, max_z) = zs.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), z| (lo.min(z), hi.max(z)));
    if !(min_z.is_finite() && max_z.is_finite()) {
        return None;
    }
    let base_z = min_z - t.base_depth;
    let plan_area = |t: &[DVec3; 3]| (t[1] - t[0]).truncate().perp_dot((t[2] - t[0]).truncate()) * 0.5;
    let area = triangles.iter().map(plan_area).sum();
    let volume = triangles.iter().map(|t| plan_area(t) * ((t[0].z + t[1].z + t[2].z) / 3.0 - base_z)).sum();
    Some(TopoGeom { triangles, boundary, min_z, max_z, base_z, area, volume, warnings })
}

/// A flat surface at relative elevation 0 over `boundary` (ccw): the
/// boundary's own triangles and zero elevations.
fn flat_surface(boundary: &[DVec2]) -> (Vec<f64>, Vec<[DVec3; 3]>) {
    let tris = polygon::triangulate(boundary)
        .into_iter()
        .filter_map(|[a, b, c]| {
            Some([boundary.get(a)?.extend(0.0), boundary.get(b)?.extend(0.0), boundary.get(c)?.extend(0.0)])
        })
        .map(|t| if (t[1] - t[0]).truncate().perp_dot((t[2] - t[0]).truncate()) < 0.0 { [t[0], t[2], t[1]] } else { t })
        .collect();
    (vec![0.0; boundary.len()], tris)
}

/// The closed solid: the surface on top, a flat base, sides along the boundary.
pub fn add_solid(s: &mut Solid, g: &TopoGeom) {
    for t in &g.triangles {
        let normal = (t[1] - t[0]).cross(t[2] - t[0]);
        if normal.length_squared() > 1e-24 && normal.is_finite() {
            let mut f = Face::new(t.to_vec(), normal);
            f.smooth = true;
            s.faces.push(f);
        }
    }
    let base: Vec<DVec3> = g.boundary.iter().rev().map(|p| p.truncate().extend(g.base_z)).collect();
    s.add_polygons([base]);
    let n = g.boundary.len();
    s.add_polygons((0..n).map(|i| {
        let (a, b) = (g.boundary[i], g.boundary[(i + 1) % n]);
        vec![a.truncate().extend(g.base_z), b.truncate().extend(g.base_z), b, a]
    }));
}

/// Contour lines of `g` every `interval` (doubled until at most `max`
/// remain), every `major_every`-th one a major.
pub fn contours(g: &TopoGeom, interval: f64, major_every: u32, max: usize) -> Vec<Contour> {
    if !(interval.is_finite() && interval > 1e-6) || g.triangles.is_empty() {
        return Vec::new();
    }
    // Majors are the multiples of the major interval, whatever the step.
    let major_step = interval * major_every.max(1) as f64;
    let max = max.max(1) as f64;
    let mut step = interval;
    // Bounded: 64 doublings outgrow any finite range.
    for _ in 0..64 {
        if ((g.max_z / step).floor() - (g.min_z / step).ceil() + 1.0) <= max {
            break;
        }
        step *= 2.0;
    }
    let (first, last) = ((g.min_z / step).ceil(), (g.max_z / step).floor());
    if !(first.is_finite() && last.is_finite()) || last - first + 1.0 > max {
        return Vec::new();
    }
    let mut out = Vec::new();
    for k in first as i64..=last as i64 {
        let z = k as f64 * step;
        let segments = slice(&g.triangles, z);
        let m = z / major_step;
        let major = (m - m.round()).abs() < 1e-6;
        out.extend(chain(segments).into_iter().map(|points| Contour { z, major, points }));
    }
    out
}

/// Segments where the plane at `z` cuts `triangles` (vertices exactly at
/// `z` count as just above, so shared edges give no duplicates).
pub fn slice(triangles: &[[DVec3; 3]], z: f64) -> Vec<(DVec2, DVec2)> {
    let mut out = Vec::new();
    for t in triangles {
        let above = t.map(|p| p.z >= z);
        let mut pts = Vec::with_capacity(2);
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            if above[i] != above[(i + 1) % 3] {
                let s = (z - a.z) / (b.z - a.z);
                pts.push(a.truncate().lerp(b.truncate(), s));
            }
        }
        if let [a, b] = pts[..]
            && a.distance(b) > 1e-12
        {
            out.push((a, b));
        }
    }
    out
}

/// Join segments that share ends into polylines (bounded by the segment count).
pub fn chain(segments: Vec<(DVec2, DVec2)>) -> Vec<Vec<DVec2>> {
    use std::collections::HashMap;
    let key = |p: DVec2| ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64);
    let mut at: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, (a, b)) in segments.iter().enumerate() {
        at.entry(key(*a)).or_default().push(i);
        at.entry(key(*b)).or_default().push(i);
    }
    let mut used = vec![false; segments.len()];
    let mut lines = Vec::new();
    for start in 0..segments.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let (a, b) = segments[start];
        let mut line = std::collections::VecDeque::from([a, b]);
        // Grow at the back, then at the front.
        for front in [false, true] {
            for _ in 0..segments.len() {
                let end = if front { line[0] } else { line[line.len() - 1] };
                let Some(&next) = at.get(&key(end)).and_then(|list| list.iter().find(|&&i| !used[i])) else {
                    break;
                };
                used[next] = true;
                let (p, q) = segments[next];
                let far = if key(p) == key(end) { q } else { p };
                if front {
                    line.push_front(far);
                } else {
                    line.push_back(far);
                }
            }
        }
        lines.push(line.into_iter().collect());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::{Category, Document, ElementId, ElementKind, ElementTypeId};

    fn terrain(points: Vec<DVec3>, boundary: Vec<DVec2>) -> (Project, Element) {
        let doc = Document::new(Project::from_template());
        let project = doc.project;
        let level = project.levels[0].id;
        let e = Element {
            id: ElementId(9999),
            level,
            type_id: ElementTypeId(0),
            mark: String::new(),
            comments: String::new(),
            uid: Default::default(),
            kind: ElementKind::Topography(Topography { points, boundary, base_depth: 2.0, sketch: None }),
            phase_created: Default::default(),
            phase_demolished: None,
        };
        (project, e)
    }

    fn geom(points: Vec<DVec3>, boundary: Vec<DVec2>) -> TopoGeom {
        let (p, e) = terrain(points, boundary);
        let ElementKind::Topography(t) = &e.kind else { panic!() };
        topo_geom(&p, &e, t).unwrap()
    }

    fn rect(w: f64, h: f64) -> Vec<DVec2> {
        vec![DVec2::ZERO, DVec2::new(w, 0.0), DVec2::new(w, h), DVec2::new(0.0, h)]
    }

    /// z = 0.1 x + 0.2 y sampled on a grid strictly inside a 10 x 10 square.
    fn plane_points() -> Vec<DVec3> {
        let mut pts = Vec::new();
        for i in 1..10 {
            for j in 1..10 {
                let (x, y) = (i as f64, j as f64);
                pts.push(DVec3::new(x, y, 0.1 * x + 0.2 * y));
            }
        }
        pts
    }

    #[test]
    fn the_surface_passes_through_the_points_and_the_volume_is_exact() {
        let g = geom(plane_points(), rect(10.0, 10.0));
        assert!(g.warnings.is_empty(), "{:?}", g.warnings);
        // Every point is a vertex of the surface.
        for p in plane_points() {
            assert!(g.triangles.iter().flatten().any(|q| q.distance(p) < 1e-9), "{p}");
        }
        // The boundary is on the plane (the points' plane extends linearly...
        // inside their hull; the corners take the nearest point's elevation).
        assert!((g.area - 100.0).abs() < 1e-9, "{}", g.area);
        // Volume = sum of plan area x mean height above the base.
        let mut s = Solid::new(ElementId(1), Category::Topography, [0, 0, 0]);
        add_solid(&mut s, &g);
        assert!((s.volume() - g.volume).abs() < 1e-6, "{} {}", s.volume(), g.volume);
        assert!((g.base_z - (g.min_z - 2.0)).abs() < 1e-12);
    }

    #[test]
    fn an_inclined_plane_over_its_own_corners_has_an_exact_volume() {
        // Points at the corners: the surface is the plane z = x / 10.
        let pts = vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(10.0, 0.0, 1.0),
            DVec3::new(10.0, 10.0, 1.0),
            DVec3::new(0.0, 10.0, 0.0),
            DVec3::new(5.0, 5.0, 0.5),
        ];
        // The boundary sits just inside the corners so they count as inside.
        let g =
            geom(pts, vec![DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0), DVec2::new(10.0, 10.0), DVec2::new(0.0, 10.0)]);
        // Mean height above the base (min 0 - 2): 0.5 + 2 over 100 m².
        assert!((g.volume - 250.0).abs() < 1e-6, "{}", g.volume);
        assert!((g.min_z, g.max_z) == (0.0, 1.0), "{} {}", g.min_z, g.max_z);
    }

    #[test]
    fn a_non_convex_boundary_keeps_no_triangle_outside() {
        let l = vec![
            DVec2::ZERO,
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 4.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(4.0, 10.0),
            DVec2::new(0.0, 10.0),
        ];
        let pts = vec![
            DVec3::new(2.0, 2.0, 1.0),
            DVec3::new(8.0, 2.0, 2.0),
            DVec3::new(2.0, 8.0, 3.0),
            DVec3::new(7.0, 7.0, 99.0),
        ];
        let g = geom(pts, l.clone());
        // 100 - 36 = 64 m², and the point in the notch is left out.
        assert!((g.area - 64.0).abs() < 1e-9, "{}", g.area);
        assert!(g.triangles.iter().flatten().all(|p| p.z < 50.0));
        for t in &g.triangles {
            assert!(polygon::contains(&l, ((t[0] + t[1] + t[2]) / 3.0).truncate()));
        }
    }

    #[test]
    fn degenerate_inputs_give_a_surface_and_a_warning() {
        let b = rect(10.0, 10.0);
        let g = geom(Vec::new(), b.clone());
        assert_eq!(g.warnings, [TopoWarning::FewPoints]);
        assert!((g.area - 100.0).abs() < 1e-9 && g.max_z == 0.0);
        let g = geom(vec![DVec3::new(5.0, 5.0, 3.0)], b.clone());
        assert_eq!(g.warnings, [TopoWarning::FewPoints]);
        assert!(g.triangles.iter().flatten().all(|p| (p.z - 3.0).abs() < 1e-9));
        let line: Vec<_> = (1..9).map(|i| DVec3::new(i as f64, i as f64, i as f64)).collect();
        let g = geom(line, b.clone());
        assert_eq!(g.warnings, [TopoWarning::Collinear]);
        assert!((g.area - 100.0).abs() < 1e-9);
        // Bad and repeated points are dropped; a point outside only lifts the boundary.
        let bad = vec![
            DVec3::new(f64::NAN, 1.0, 0.0),
            DVec3::new(2.0, 2.0, 1.0),
            DVec3::new(2.0, 2.0, 5.0),
            DVec3::new(1e-200, 3.0, 1.0),
            DVec3::new(50.0, 50.0, 1.0),
        ];
        let g = geom(bad, b.clone());
        assert!((g.area - 100.0).abs() < 1e-9);
        // A boundary that crosses itself (a bow tie) still builds.
        let bow = vec![DVec2::ZERO, DVec2::new(10.0, 10.0), DVec2::new(10.0, 0.0), DVec2::new(0.0, 10.0)];
        let (p, e) = terrain(plane_points(), bow);
        let ElementKind::Topography(t) = &e.kind else { panic!() };
        if let Some(g) = topo_geom(&p, &e, t) {
            assert!(g.min_z.is_finite());
        }
    }

    #[test]
    fn contours_of_an_inclined_plane_are_straight_and_parallel() {
        // z = x over a 10 x 4 rectangle, points on the boundary corners.
        let pts = vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(10.0, 0.0, 10.0),
            DVec3::new(10.0, 4.0, 10.0),
            DVec3::new(0.0, 4.0, 0.0),
            DVec3::new(5.0, 2.0, 5.0),
        ];
        let g = geom(pts, rect(10.0, 4.0));
        let c = contours(&g, 1.0, 5, MAX_CONTOURS);
        // Elevations 0 to 10: 1..=9 are lines across. The highest, 10, runs
        // along the edge x = 10; the lowest, 0, gives no line (vertices at a
        // contour's height count as above it, so nothing lies below 0).
        assert!(c.iter().any(|c| c.z == 10.0 && c.points.iter().all(|p| (p.x - 10.0).abs() < 1e-9)), "{c:?}");
        assert!(c.iter().all(|c| c.z != 0.0), "{c:?}");
        let inner: Vec<_> = c.iter().filter(|c| c.z > 0.5 && c.z < 9.5).collect();
        assert_eq!(inner.len(), 9);
        for line in inner {
            assert!(line.points.iter().all(|p| (p.x - line.z).abs() < 1e-9), "{line:?}");
            let ys: Vec<f64> = line.points.iter().map(|p| p.y).collect();
            assert!(ys.iter().cloned().fold(f64::INFINITY, f64::min) < 1e-9);
            assert!(ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max) > 4.0 - 1e-9);
            assert_eq!(line.major, line.z == 5.0);
        }
        // A cap doubles the interval.
        let few = contours(&g, 1.0, 5, 4);
        let zs: std::collections::BTreeSet<i64> = few.iter().map(|c| c.z as i64).collect();
        assert!(zs.len() <= 4, "{zs:?}");
    }

    #[test]
    fn the_boundary_takes_the_elevations_of_points_beyond_it() {
        // z = 0.1 x + 0.2 y surveyed on a grid reaching 5 m past a 10 x 10
        // lot: its corners lie inside the points' hull, so they take the
        // plane's own elevations, as does every point of the surface.
        let plane = |x: f64, y: f64| 0.1 * x + 0.2 * y;
        let mut pts = Vec::new();
        for i in -5..=15 {
            for j in -5..=15 {
                let (x, y) = (i as f64 + 0.5, j as f64 + 0.25);
                pts.push(DVec3::new(x, y, plane(x, y)));
            }
        }
        let g = geom(pts, rect(10.0, 10.0));
        assert!(g.warnings.is_empty(), "{:?}", g.warnings);
        assert_eq!(g.boundary.len(), 4);
        for b in &g.boundary {
            assert!((b.z - plane(b.x, b.y)).abs() < 1e-9, "{b}");
        }
        assert!(g.triangles.iter().flatten().all(|p| (p.z - plane(p.x, p.y)).abs() < 1e-9));
        // Points beyond the lot are not vertices of the surface.
        assert!(g.triangles.iter().flatten().all(|p| (0.0..=10.0).contains(&p.x) && (0.0..=10.0).contains(&p.y)));
        assert!(g.min_z.abs() < 1e-9 && (g.max_z - 3.0).abs() < 1e-9, "{} {}", g.min_z, g.max_z);
    }

    #[test]
    fn beyond_the_points_hull_the_boundary_takes_the_nearest_point() {
        // Two points: no triangle to interpolate over, so each corner takes
        // the elevation of the point nearest it (a warning says so), and the
        // surface still passes through both.
        let pts = vec![DVec3::new(2.0, 5.0, 1.0), DVec3::new(8.0, 5.0, 4.0)];
        let g = geom(pts.clone(), rect(10.0, 10.0));
        assert_eq!(g.warnings, [TopoWarning::FewPoints]);
        for b in &g.boundary {
            let want = if b.x < 5.0 { 1.0 } else { 4.0 };
            assert!((b.z - want).abs() < 1e-9, "{b}");
        }
        for p in pts {
            assert!(g.triangles.iter().flatten().any(|q| q.distance(p) < 1e-9), "{p}");
        }
    }

    #[test]
    fn a_point_on_a_boundary_edge_keeps_the_solid_closed() {
        // The corners give 0; the point on the bottom edge lifts it to 3.
        let pts = vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(10.0, 10.0, 0.0),
            DVec3::new(0.0, 10.0, 0.0),
            DVec3::new(5.0, 0.0, 3.0),
            DVec3::new(5.0, 5.0, 1.0),
        ];
        let g = geom(pts, rect(10.0, 10.0));
        assert!(g.boundary.iter().any(|p| p.distance(DVec3::new(5.0, 0.0, 3.0)) < 1e-9), "{:?}", g.boundary);
        let mut s = Solid::new(ElementId(1), Category::Topography, [0, 0, 0]);
        add_solid(&mut s, &g);
        assert!((s.volume() - g.volume).abs() < 1e-6, "{} {}", s.volume(), g.volume);
        // Every edge of the closed solid is shared by exactly two faces.
        let key = |p: DVec3| ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64, (p.z * 1e6).round() as i64);
        let mut edges: std::collections::HashMap<_, i32> = std::collections::HashMap::new();
        for f in &s.faces {
            let n = f.verts.len();
            for i in 0..n {
                let (a, b) = (key(f.verts[i]), key(f.verts[(i + 1) % n]));
                *edges.entry(if a < b { (a, b) } else { (b, a) }).or_default() += 1;
            }
        }
        assert!(edges.values().all(|c| *c == 2), "{edges:?}");
    }

    #[test]
    fn majors_stay_on_the_major_interval_when_the_step_doubles() {
        // z = x from 0 to 100: 101 contours at 1 m, so a cap of 60 doubles to 2 m.
        let pts = vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(100.0, 0.0, 100.0),
            DVec3::new(100.0, 4.0, 100.0),
            DVec3::new(0.0, 4.0, 0.0),
        ];
        let g = geom(pts, rect(100.0, 4.0));
        let c = contours(&g, 1.0, 5, 60);
        assert!(c.iter().all(|c| (c.z / 2.0).fract() == 0.0));
        for line in &c {
            assert_eq!(line.major, (line.z / 5.0).fract() == 0.0, "{}", line.z);
        }
        assert!(c.iter().any(|c| c.major && c.z == 10.0));
        // No cap at all still ends.
        assert!(contours(&g, 1.0, 5, 0).len() <= 4);
    }

    #[test]
    fn chaining_joins_segments_end_to_end() {
        let s = vec![
            (DVec2::new(1.0, 0.0), DVec2::new(2.0, 0.0)),
            (DVec2::new(0.0, 0.0), DVec2::new(1.0, 0.0)),
            (DVec2::new(3.0, 0.0), DVec2::new(2.0, 0.0)),
        ];
        let lines = chain(s);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].len(), 4);
    }
}
