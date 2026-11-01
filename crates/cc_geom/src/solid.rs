use crate::polygon;
use cc_core::{Category, ElementId};
use glam::{DVec2, DVec3};

/// A planar polygon face with an outward normal.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    pub verts: Vec<DVec3>,
    pub normal: DVec3,
    /// A facet of a curved surface: edges shared with neighboring smooth
    /// facets are not outlines (only silhouettes, see [`Solid::smooth_edges`]).
    pub smooth: bool,
}

/// Unit normal of a polygon's winding (Newell's method); zero when it has no area.
fn newell(verts: &[DVec3]) -> DVec3 {
    let n = verts.len();
    (0..n)
        .map(|i| {
            let (a, b) = (verts[i], verts[(i + 1) % n]);
            DVec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y))
        })
        .sum::<DVec3>()
        .normalize_or_zero()
}

/// Smooth facets meeting at less than this angle (cosine) read as one surface.
const SMOOTH_COS: f64 = 0.85;

impl Face {
    pub fn new(verts: Vec<DVec3>, normal: DVec3) -> Self {
        Self { verts, normal: normal.normalize_or_zero(), smooth: false }
    }

    /// Orthonormal in-plane basis `(u, v)`.
    fn basis(&self) -> (DVec3, DVec3) {
        // A face built without a normal (its points on one line, or a normal
        // that was zero) gets the normal of its points; with none, any plane.
        let n = if self.normal.length_squared() > 0.5 { self.normal } else { newell(&self.verts) };
        let n = if n.is_finite() && n != DVec3::ZERO { n } else { DVec3::Z };
        let helper = if n.z.abs() < 0.9 { DVec3::Z } else { DVec3::X };
        let u = n.cross(helper).normalize();
        (u, n.cross(u))
    }

    /// Triangles as index triples into `verts`.
    pub fn triangulate(&self) -> Vec<[usize; 3]> {
        if self.verts.len() == 4 {
            return vec![[0, 1, 2], [0, 2, 3]];
        }
        let (u, v) = self.basis();
        let flat: Vec<DVec2> = self.verts.iter().map(|p| DVec2::new(p.dot(u), p.dot(v))).collect();
        polygon::triangulate(&flat)
    }

    pub fn centroid(&self) -> DVec3 {
        self.verts.iter().copied().sum::<DVec3>() / self.verts.len().max(1) as f64
    }

    /// Area of the planar polygon.
    pub fn area(&self) -> f64 {
        let n = self.verts.len();
        let sum: DVec3 = (0..n).map(|i| self.verts[i].cross(self.verts[(i + 1) % n])).sum();
        sum.dot(self.normal).abs() * 0.5
    }
}

/// Boundary representation of one element (or a part of one).
#[derive(Debug, Clone)]
pub struct Solid {
    pub element: ElementId,
    pub category: Category,
    pub color: [u8; 3],
    /// 0 (opaque) to 1 (invisible), from the material.
    pub transparency: f64,
    pub faces: Vec<Face>,
    /// A part the element carries that is not its body (a roof's framing):
    /// drawn, but left out of its volume, joins and exported body.
    pub secondary: bool,
    /// The detail levels of the views that draw it (a component's piece).
    pub detail: cc_core::component::DetailLevels,
    /// The material's appearance color and surface pattern (the textures style).
    pub appearance: [u8; 3],
    pub surface: cc_core::PatternFill,
}

#[derive(Debug, Clone, Copy)]
pub struct Ray {
    pub origin: DVec3,
    pub dir: DVec3,
}

impl Solid {
    pub fn new(element: ElementId, category: Category, color: [u8; 3]) -> Self {
        Self {
            element,
            category,
            color,
            transparency: 0.0,
            faces: Vec::new(),
            secondary: false,
            detail: Default::default(),
            appearance: color,
            surface: Default::default(),
        }
    }

    /// An empty solid colored and made transparent like material `name`.
    pub fn of_material(element: ElementId, category: Category, project: &cc_core::Project, name: &str) -> Self {
        let mut s = Self {
            transparency: project.material_transparency(name),
            ..Self::new(element, category, project.material_color(name))
        };
        s.set_appearance(project, name);
        s
    }

    /// Take the appearance color and surface pattern of material `name`
    /// (what the textures style shows).
    pub fn set_appearance(&mut self, project: &cc_core::Project, name: &str) {
        if let Some(m) = project.material(name) {
            self.appearance = m.appearance.color;
            self.surface = m.graphics.surface;
        }
    }

    pub fn bounds(&self) -> Option<(DVec3, DVec3)> {
        let mut it = self.faces.iter().flat_map(|f| f.verts.iter().copied());
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
    }

    /// Volume via the divergence theorem (faces must be closed and outward).
    pub fn volume(&self) -> f64 {
        self.faces.iter().map(|f| f.centroid().dot(f.normal) * f.area()).sum::<f64>() / 3.0
    }

    /// Visible outline edges grouped by face: the parts of each face boundary
    /// not shared with a coplanar neighbor face (so split faces read as one).
    pub fn feature_edges(&self) -> Vec<Vec<(DVec3, DVec3)>> {
        let mut all: Vec<(usize, DVec3, DVec3)> = Vec::new();
        for (fi, f) in self.faces.iter().enumerate() {
            let n = f.verts.len();
            all.extend((0..n).map(|i| (fi, f.verts[i], f.verts[(i + 1) % n])));
        }
        // Small solids compare every pair of faces; larger ones only edges
        // whose boxes touch (see `EdgeGrid`).
        let grid = (all.len() > GRID_MIN_EDGES).then(|| EdgeGrid::new(&all));
        let mut seen = vec![usize::MAX; all.len()];
        let mut candidates = Vec::new();
        let mut out = vec![Vec::new(); self.faces.len()];
        for (ei, &(fi, a, b)) in all.iter().enumerate() {
            let (Some(face), Some(face_out)) = (self.faces.get(fi), out.get_mut(fi)) else { continue };
            let ab = b - a;
            let len2 = ab.length_squared();
            if len2 < 1e-12 {
                continue;
            }
            // Edges of other faces with the same normal (or smooth neighbors) hide what they overlap.
            let joins = |fj: usize, other: &Face| {
                let cos = other.normal.dot(face.normal);
                fj != fi && (cos >= 0.9999 || (face.smooth && other.smooth && cos > SMOOTH_COS))
            };
            let mut covered: Vec<(f64, f64)> = Vec::new();
            let mut cover = |c: DVec3, d: DVec3| {
                let off = |p: DVec3| (p - a).cross(ab).length_squared() / len2;
                if off(c) > 1e-10 || off(d) > 1e-10 {
                    return;
                }
                let (tc, td) = ((c - a).dot(ab) / len2, (d - a).dot(ab) / len2);
                let (lo, hi) = (tc.min(td).max(0.0), tc.max(td).min(1.0));
                if hi - lo > 1e-9 {
                    covered.push((lo, hi));
                }
            };
            match &grid {
                Some(grid) => {
                    grid.near(a, b, ei, &mut seen, &mut candidates);
                    for &cj in &candidates {
                        if let Some(&(fj, c, d)) = all.get(cj)
                            && self.faces.get(fj).is_some_and(|other| joins(fj, other))
                        {
                            cover(c, d);
                        }
                    }
                }
                None => {
                    for (_, other) in self.faces.iter().enumerate().filter(|(fj, other)| joins(*fj, other)) {
                        let m = other.verts.len();
                        (0..m).for_each(|k| cover(other.verts[k], other.verts[(k + 1) % m]));
                    }
                }
            }
            covered.sort_by(|x, y| x.0.total_cmp(&y.0));
            let mut t = 0.0;
            for (lo, hi) in covered {
                if lo > t + 1e-9 {
                    face_out.push((a + ab * t, a + ab * lo));
                }
                t = t.max(hi);
            }
            if t < 1.0 - 1e-9 {
                face_out.push((a + ab * t, b));
            }
        }
        out
    }

    /// Edges between smooth facets (hidden by [`Self::feature_edges`]), per
    /// face, with the neighbor's normal: a view draws one as a silhouette
    /// where the face is seen and its neighbor is not.
    pub fn smooth_edges(&self) -> Vec<Vec<(DVec3, DVec3, DVec3)>> {
        let mut out = vec![Vec::new(); self.faces.len()];
        for (fi, face) in self.faces.iter().enumerate().filter(|(_, f)| f.smooth) {
            let n = face.verts.len();
            for i in 0..n {
                let (a, b) = (face.verts[i], face.verts[(i + 1) % n]);
                let shared = self.faces.iter().enumerate().find_map(|(fj, other)| {
                    if fj == fi || !other.smooth || other.normal.dot(face.normal) <= SMOOTH_COS {
                        return None;
                    }
                    let m = other.verts.len();
                    (0..m)
                        .any(|k| {
                            let (c, d) = (other.verts[k], other.verts[(k + 1) % m]);
                            c.distance(b) < 1e-9 && d.distance(a) < 1e-9
                        })
                        .then_some(other.normal)
                });
                if let (Some(normal), Some(list)) = (shared, out.get_mut(fi)) {
                    list.push((a, b, normal));
                }
            }
        }
        out
    }

    /// Nearest hit distance along `ray`, if any.
    pub fn raycast(&self, ray: Ray) -> Option<f64> {
        self.raycast_within(ray, f64::INFINITY)
    }

    /// Like [`Self::raycast`], for a caller that already has a hit at
    /// `limit`: faces whose boxes the ray meets beyond it are skipped (a
    /// result beyond `limit` is not then the nearest).
    pub fn raycast_within(&self, ray: Ray, limit: f64) -> Option<f64> {
        let mut best: Option<f64> = None;
        for face in &self.faces {
            let Some(first) = face.verts.first() else { continue };
            let (lo, hi) = face.verts.iter().fold((*first, *first), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
            if ray_box(ray, lo, hi).is_none_or(|t| t > best.map_or(limit, |b| b.min(limit))) {
                continue;
            }
            for [a, b, c] in face.triangulate() {
                if let Some(t) = ray_triangle(ray, face.verts[a], face.verts[b], face.verts[c]) {
                    best = Some(best.map_or(t, |bt| bt.min(t)));
                }
            }
        }
        best
    }

    /// Append planar polygons, each with the normal of its winding (Newell's
    /// method); degenerate polygons are dropped.
    pub fn add_polygons(&mut self, polygons: impl IntoIterator<Item = Vec<DVec3>>) {
        for verts in polygons {
            let n = verts.len();
            let normal: DVec3 = (0..n)
                .map(|i| {
                    let (a, b) = (verts[i], verts[(i + 1) % n]);
                    DVec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y))
                })
                .sum();
            if n >= 3 && normal.length_squared() > 1e-20 && normal.is_finite() {
                self.faces.push(Face::new(verts, normal));
            }
        }
    }

    /// Append an axis-aligned (in the local frame) box.
    ///
    /// `origin` is the frame origin, `u`/`v` horizontal unit axes, Z is up.
    #[allow(clippy::too_many_arguments)]
    pub fn add_box(&mut self, origin: DVec2, u: DVec2, v: DVec2, u0: f64, u1: f64, v0: f64, v1: f64, z0: f64, z1: f64) {
        if u1 - u0 < 1e-6 || v1 - v0 < 1e-6 || z1 - z0 < 1e-6 {
            return;
        }
        let p = |a: f64, b: f64, z: f64| {
            let q = origin + u * a + v * b;
            DVec3::new(q.x, q.y, z)
        };
        let u3 = DVec3::new(u.x, u.y, 0.0);
        let v3 = DVec3::new(v.x, v.y, 0.0);
        let f = &mut self.faces;
        f.push(Face::new(vec![p(u0, v0, z1), p(u1, v0, z1), p(u1, v1, z1), p(u0, v1, z1)], DVec3::Z));
        f.push(Face::new(vec![p(u0, v0, z0), p(u0, v1, z0), p(u1, v1, z0), p(u1, v0, z0)], -DVec3::Z));
        f.push(Face::new(vec![p(u0, v0, z0), p(u1, v0, z0), p(u1, v0, z1), p(u0, v0, z1)], -v3));
        f.push(Face::new(vec![p(u1, v1, z0), p(u0, v1, z0), p(u0, v1, z1), p(u1, v1, z1)], v3));
        f.push(Face::new(vec![p(u0, v1, z0), p(u0, v0, z0), p(u0, v0, z1), p(u0, v1, z1)], -u3));
        f.push(Face::new(vec![p(u1, v0, z0), p(u1, v1, z0), p(u1, v1, z1), p(u1, v0, z1)], u3));
    }

    /// Append a prism like [`Self::add_prism`] with openings: `holes` are
    /// outlines inside `footprint` (either winding).
    pub fn add_prism_with_holes(
        &mut self,
        footprint: &[DVec2],
        holes: &[Vec<DVec2>],
        bottom: impl Fn(DVec2) -> f64,
        top: impl Fn(DVec2) -> f64,
    ) {
        if holes.is_empty() {
            self.add_prism(footprint, bottom, top);
            return;
        }
        let fp = polygon::ccw(footprint);
        if fp.len() < 3 {
            return;
        }
        let holes: Vec<Vec<DVec2>> = holes
            .iter()
            .filter(|h| h.len() >= 3)
            .map(|h| {
                let mut h = polygon::ccw(h);
                h.reverse();
                h
            })
            .collect();
        let lift = |p: DVec2, z: f64| DVec3::new(p.x, p.y, z);
        for t in polygon::triangulate_with_holes(&fp, &holes) {
            let top_pts: Vec<DVec3> = t.iter().map(|&p| lift(p, top(p))).collect();
            let bottom_pts: Vec<DVec3> = t.iter().rev().map(|&p| lift(p, bottom(p))).collect();
            let n = |pts: &[DVec3]| (pts[1] - pts[0]).cross(pts[2] - pts[0]);
            let (tn, bn) = (n(&top_pts), n(&bottom_pts));
            self.faces.push(Face::new(top_pts, tn));
            self.faces.push(Face::new(bottom_pts, bn));
        }
        // Sides: the outline counter-clockwise, the holes clockwise, all facing out of the solid.
        for ring in std::iter::once(&fp).chain(holes.iter()) {
            let n = ring.len();
            for i in 0..n {
                let (a, b) = (ring[i], ring[(i + 1) % n]);
                let out = -(b - a).normalize_or_zero().perp();
                self.faces.push(Face::new(
                    vec![lift(a, bottom(a)), lift(b, bottom(b)), lift(b, top(b)), lift(a, top(a))],
                    DVec3::new(out.x, out.y, 0.0),
                ));
            }
        }
    }

    /// Append a prism between two copies of a polygon: `bottom(p)`/`top(p)` give
    /// the z of each footprint vertex (planar surfaces only).
    pub fn add_prism(&mut self, footprint: &[DVec2], bottom: impl Fn(DVec2) -> f64, top: impl Fn(DVec2) -> f64) {
        let fp = polygon::ccw(footprint);
        let n = fp.len();
        if n < 3 {
            return;
        }
        let lift = |p: DVec2, z: f64| DVec3::new(p.x, p.y, z);
        let plane_normal = |pts: &[DVec3]| -> DVec3 {
            let sum: DVec3 = (0..pts.len()).map(|i| pts[i].cross(pts[(i + 1) % pts.len()])).sum();
            sum.normalize_or_zero()
        };
        let top_pts: Vec<DVec3> = fp.iter().map(|&p| lift(p, top(p))).collect();
        let mut bottom_pts: Vec<DVec3> = fp.iter().map(|&p| lift(p, bottom(p))).collect();
        let top_n = plane_normal(&top_pts);
        bottom_pts.reverse();
        let bottom_n = plane_normal(&bottom_pts);
        self.faces.push(Face::new(top_pts, top_n));
        self.faces.push(Face::new(bottom_pts, bottom_n));
        for i in 0..n {
            let (a, b) = (fp[i], fp[(i + 1) % n]);
            let out = -(b - a).normalize_or_zero().perp();
            self.faces.push(Face::new(
                vec![lift(a, bottom(a)), lift(b, bottom(b)), lift(b, top(b)), lift(a, top(a))],
                DVec3::new(out.x, out.y, 0.0),
            ));
        }
    }
}

/// Edges of a solid binned in a uniform grid by their bounding boxes, grown
/// by a margin above the collinearity tolerance of [`Solid::feature_edges`]:
/// only edges sharing a cell can overlap, so dense meshes are not quadratic.
struct EdgeGrid {
    cell: f64,
    cells: std::collections::HashMap<[i64; 3], Vec<usize>>,
    /// Edges spanning too many cells, compared with every edge.
    big: Vec<usize>,
    count: usize,
}

const GRID_MARGIN: f64 = 1e-4;
const GRID_MAX_CELLS: f64 = 64.0;
/// Solids with up to this many edges compare every pair of faces (cheaper than binning).
const GRID_MIN_EDGES: usize = 1500;

impl EdgeGrid {
    fn new(edges: &[(usize, DVec3, DVec3)]) -> Self {
        let total: f64 = edges.iter().map(|(_, a, b)| a.distance(*b)).filter(|l| l.is_finite()).sum();
        let cell = (total / edges.len().max(1) as f64).max(1e-3);
        let mut grid = Self { cell, cells: Default::default(), big: Vec::new(), count: edges.len() };
        for (i, &(_, a, b)) in edges.iter().enumerate() {
            match grid.range(a, b) {
                Some((lo, hi)) => {
                    for key in cell_keys(lo, hi) {
                        grid.cells.entry(key).or_default().push(i);
                    }
                }
                None => grid.big.push(i),
            }
        }
        grid
    }

    /// Cells covered by the grown box of `a`-`b`; `None` when it spans too
    /// many (or its coordinates are not usable).
    fn range(&self, a: DVec3, b: DVec3) -> Option<([i64; 3], [i64; 3])> {
        let lo = ((a.min(b) - DVec3::splat(GRID_MARGIN)) / self.cell).floor();
        let hi = ((a.max(b) + DVec3::splat(GRID_MARGIN)) / self.cell).floor();
        let usable = |v: DVec3| v.is_finite() && v.abs().max_element() < 1e15;
        let span = hi - lo + DVec3::ONE;
        if !usable(lo) || !usable(hi) || span.x * span.y * span.z > GRID_MAX_CELLS {
            return None;
        }
        Some(([lo.x as i64, lo.y as i64, lo.z as i64], [hi.x as i64, hi.y as i64, hi.z as i64]))
    }

    /// The edges that may overlap edge `query` (`a`-`b`), into `out`;
    /// `seen` (one slot per edge) marks those already listed for it.
    fn near(&self, a: DVec3, b: DVec3, query: usize, seen: &mut [usize], out: &mut Vec<usize>) {
        out.clear();
        let Some((lo, hi)) = self.range(a, b) else {
            out.extend(0..self.count);
            return;
        };
        let cells = cell_keys(lo, hi).filter_map(|key| self.cells.get(&key));
        for &i in cells.flatten().chain(&self.big) {
            if let Some(mark) = seen.get_mut(i)
                && *mark != query
            {
                *mark = query;
                out.push(i);
            }
        }
    }
}

fn cell_keys(lo: [i64; 3], hi: [i64; 3]) -> impl Iterator<Item = [i64; 3]> {
    (lo[0]..=hi[0]).flat_map(move |x| (lo[1]..=hi[1]).flat_map(move |y| (lo[2]..=hi[2]).map(move |z| [x, y, z])))
}

/// Distance along `ray` at which it enters the box `lo`-`hi` (grown by a
/// small margin; 0 from inside), if it meets the box ahead.
pub(crate) fn ray_box(ray: Ray, lo: DVec3, hi: DVec3) -> Option<f64> {
    const MARGIN: f64 = 1e-6;
    let (lo, hi) = (lo - DVec3::splat(MARGIN), hi + DVec3::splat(MARGIN));
    let (mut near, mut far) = (0.0_f64, f64::INFINITY);
    for axis in 0..3 {
        let (o, d) = (ray.origin[axis], ray.dir[axis]);
        if d == 0.0 {
            if o < lo[axis] || o > hi[axis] {
                return None;
            }
            continue;
        }
        let (a, b) = ((lo[axis] - o) / d, (hi[axis] - o) / d);
        near = near.max(a.min(b));
        far = far.min(a.max(b));
    }
    (near <= far).then_some(near)
}

/// Möller–Trumbore, two-sided.
fn ray_triangle(ray: Ray, a: DVec3, b: DVec3, c: DVec3) -> Option<f64> {
    let e1 = b - a;
    let e2 = c - a;
    let p = ray.dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = ray.origin - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = ray.dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-9).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression (issue #51): a prism over a footprint with no area (its
    /// points on one line) has caps without a normal; triangulating them for
    /// the 3D view or a pick in it panicked inside earcutr.
    #[test]
    fn degenerate_faces_triangulate_without_panicking() {
        let line: Vec<DVec2> = (0..5).map(|i| DVec2::new(f64::from(i), 0.0)).collect();
        let mut s = Solid::new(ElementId(1), Category::Floor, [0; 3]);
        s.add_prism(&line, |_| 0.0, |_| 1.0);
        for f in &s.faces {
            f.triangulate();
        }
        let ray = Ray { origin: DVec3::new(2.0, 0.0, 5.0), dir: -DVec3::Z };
        s.raycast(ray);
        // A face built with no normal triangulates in the plane of its points.
        let pentagon: Vec<DVec3> = (0..5)
            .map(|i| f64::from(i) * std::f64::consts::TAU / 5.0)
            .map(|a| DVec3::new(a.cos(), a.sin(), 1.0))
            .collect();
        assert_eq!(Face::new(pentagon, DVec3::ZERO).triangulate().len(), 3);
        assert!(Face::new(vec![DVec3::splat(f64::NAN); 6], DVec3::Z).triangulate().is_empty());
    }

    #[test]
    fn box_volume_and_raycast() {
        let mut s = Solid::new(ElementId(1), Category::Wall, [0; 3]);
        s.add_box(DVec2::ZERO, DVec2::X, DVec2::Y, 0.0, 2.0, 0.0, 3.0, 0.0, 4.0);
        assert!((s.volume() - 24.0).abs() < 1e-9);
        let hit = s.raycast(Ray { origin: DVec3::new(1.0, -5.0, 1.0), dir: DVec3::Y }).unwrap();
        assert!((hit - 5.0).abs() < 1e-9);
    }

    #[test]
    fn split_faces_hide_shared_edges() {
        let mut s = Solid::new(ElementId(1), Category::Wall, [0; 3]);
        // Two boxes side by side form one 2x1x1 block: its outline has 12 edges.
        s.add_box(DVec2::ZERO, DVec2::X, DVec2::Y, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0);
        s.add_box(DVec2::ZERO, DVec2::X, DVec2::Y, 1.0, 2.0, 0.0, 1.0, 0.0, 1.0);
        // Drop the two internal faces at x = 1 (+u of the first box, -u of the second).
        s.faces.remove(10);
        s.faces.remove(5);
        let len: f64 = s.feature_edges().iter().flatten().map(|(a, b)| a.distance(*b)).sum();
        // Each visible edge is reported by both adjacent faces: 2 * (4*2 + 8*1).
        assert!((len - 2.0 * (8.0 + 8.0)).abs() < 1e-9, "{len}");
    }

    /// [`Solid::feature_edges`] comparing every pair of edges.
    fn feature_edges_brute(s: &Solid) -> Vec<Vec<(DVec3, DVec3)>> {
        let edges = |f: &Face| {
            let n = f.verts.len();
            (0..n).map(move |i| (f.verts[i], f.verts[(i + 1) % n])).collect::<Vec<_>>()
        };
        let all: Vec<Vec<(DVec3, DVec3)>> = s.faces.iter().map(edges).collect();
        let mut out = Vec::new();
        for (fi, face) in s.faces.iter().enumerate() {
            let mut face_out = Vec::new();
            for &(a, b) in &all[fi] {
                let ab = b - a;
                let len2 = ab.length_squared();
                if len2 < 1e-12 {
                    continue;
                }
                let mut covered: Vec<(f64, f64)> = Vec::new();
                for (fj, other) in s.faces.iter().enumerate() {
                    let cos = other.normal.dot(face.normal);
                    let smooth = face.smooth && other.smooth && cos > SMOOTH_COS;
                    if fj == fi || (cos < 0.9999 && !smooth) {
                        continue;
                    }
                    for &(c, d) in &all[fj] {
                        let off = |p: DVec3| (p - a).cross(ab).length_squared() / len2;
                        if off(c) > 1e-10 || off(d) > 1e-10 {
                            continue;
                        }
                        let (tc, td) = ((c - a).dot(ab) / len2, (d - a).dot(ab) / len2);
                        let (lo, hi) = (tc.min(td).max(0.0), tc.max(td).min(1.0));
                        if hi - lo > 1e-9 {
                            covered.push((lo, hi));
                        }
                    }
                }
                covered.sort_by(|x, y| x.0.total_cmp(&y.0));
                let mut t = 0.0;
                for (lo, hi) in covered {
                    if lo > t + 1e-9 {
                        face_out.push((a + ab * t, a + ab * lo));
                    }
                    t = t.max(hi);
                }
                if t < 1.0 - 1e-9 {
                    face_out.push((a + ab * t, b));
                }
            }
            out.push(face_out);
        }
        out
    }

    #[test]
    fn feature_edges_match_the_pairwise_comparison() {
        // A faceted sphere (dense, small edges) on a long split slab (edges
        // too long for the grid, and T-junctions along them).
        let mut s = Solid::new(ElementId(1), Category::Wall, [0; 3]);
        let n = 24;
        let p = |i: usize, j: usize| {
            let (th, ph) = (i as f64 / n as f64 * std::f64::consts::TAU, j as f64 / n as f64 * std::f64::consts::PI);
            DVec3::new(ph.sin() * th.cos(), ph.sin() * th.sin(), 1.0 + ph.cos())
        };
        let mut polys = vec![];
        for i in 0..n {
            for j in 0..n {
                polys.push(vec![p(i, j), p(i + 1, j), p(i + 1, j + 1)]);
                polys.push(vec![p(i, j), p(i + 1, j + 1), p(i, j + 1)]);
            }
        }
        s.add_polygons(polys);
        s.add_box(DVec2::new(-40.0, -1.0), DVec2::X, DVec2::Y, 0.0, 30.0, 0.0, 2.0, -0.2, 0.0);
        s.add_box(DVec2::new(-40.0, -1.0), DVec2::X, DVec2::Y, 30.0, 80.0, 0.0, 2.0, -0.2, 0.0);
        for f in s.faces.iter_mut().skip(n * n) {
            f.smooth = true;
        }
        assert_eq!(s.feature_edges(), feature_edges_brute(&s));
    }

    #[test]
    fn prism_volume() {
        let mut s = Solid::new(ElementId(1), Category::Floor, [0; 3]);
        let sq = [DVec2::new(0.0, 0.0), DVec2::new(0.0, 2.0), DVec2::new(2.0, 2.0), DVec2::new(2.0, 0.0)];
        s.add_prism(&sq, |_| -0.5, |_| 0.0);
        assert!((s.volume() - 2.0).abs() < 1e-9);
    }
}
