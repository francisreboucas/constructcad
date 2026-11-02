use crate::polygon::line_intersect;
use crate::{Face, Solid};
use cc_core::arc::{ARC_STEP, Arc, segments};
use cc_core::{Category, ElementId};
use glam::{DVec2, DVec3};

/// A rectangular hole through a wall, in wall-local coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Opening {
    pub element: ElementId,
    pub u0: f64,
    pub u1: f64,
    pub z0: f64,
    pub z1: f64,
}

/// A vertical slice of a wall between opening boundaries.
///
/// Its footprint is a quad whose left (+v) edge spans `left` and right (-v)
/// edge spans `right` along the wall axis; end columns may be mitered.
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub left: (f64, f64),
    pub right: (f64, f64),
    /// Solid z-intervals (absolute elevations), bottom to top.
    pub solid: Vec<(f64, f64)>,
}

impl Column {
    pub fn is_solid_at(&self, z: f64) -> bool {
        self.solid.iter().any(|&(a, b)| a <= z && z <= b)
    }

    pub fn top(&self) -> Option<f64> {
        self.solid.last().map(|s| s.1)
    }
}

/// Resolved wall geometry: frame, joins, openings and columns.
///
/// Wall-local coordinates are `u`, the distance along the body centerline
/// from its start, and `v`, the distance to its left. A straight wall's
/// frame is `origin`, `dir` and `normal`; an arc wall's centerline is `arc`
/// (`dir` and `normal` are then its frame at the start), and its faces and
/// layer boundaries are concentric arcs.
#[derive(Debug, Clone)]
pub struct WallGeom {
    pub element: ElementId,
    pub origin: DVec2,
    pub dir: DVec2,
    /// Left normal (+v).
    pub normal: DVec2,
    pub length: f64,
    pub half: f64,
    pub base_z: f64,
    pub top_z: f64,
    /// Axial extents `(left, right)` of the start/end faces after joins.
    pub start_ext: (f64, f64),
    pub end_ext: (f64, f64),
    pub start_joined: bool,
    pub end_joined: bool,
    pub openings: Vec<Opening>,
    pub columns: Vec<Column>,
    /// Layer boundaries as `(v_from, v_to, material)`, from the left face inward.
    pub layers: Vec<(f64, f64, String)>,
    /// Stretches of a face where another wall meets it in a T, as
    /// `(side, u0, u1)` with side +1 for the left face and -1 for the right.
    pub face_gaps: Vec<(f64, f64, f64)>,
    /// The body centerline of an arc wall.
    pub arc: Option<Arc>,
    /// A top that follows what the wall is attached to: segments
    /// `(u0, u1, z0, z1)` along the centerline, the top running straight
    /// from `z0` to `z1` (empty: the flat `top_z`).
    pub top_profile: Vec<(f64, f64, f64, f64)>,
}

/// How one end of a wall is cut by a neighbor.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EndJoin {
    /// Distance along the ray (pointing into this wall) of the ray-left and ray-right faces.
    pub ray_left: f64,
    pub ray_right: f64,
    /// For a T-join, the wall whose face this end meets.
    pub host: Option<usize>,
}

/// Minimal wall description used while resolving joins.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WallLine {
    pub start: DVec2,
    pub end: DVec2,
    pub half: f64,
    /// Whether each end may join (a disallowed end stays square and is ignored by neighbors).
    pub join_start: bool,
    pub join_end: bool,
    /// Bulge of the centerline (0 for a straight wall; see [`cc_core::arc`]).
    pub bulge: f64,
}

/// A wall face (or any line parallel to a wall's centerline) in plan.
#[derive(Debug, Clone, Copy)]
enum Curve {
    /// Through `p` along the unit direction `d`.
    Line {
        p: DVec2,
        d: DVec2,
    },
    Circle {
        c: DVec2,
        r: f64,
    },
}

/// Where two curves cross: the crossing nearest `near`.
fn meet(a: Curve, b: Curve, near: DVec2) -> Option<DVec2> {
    let nearest = |pts: [DVec2; 2]| {
        pts.into_iter().filter(|p| p.is_finite()).min_by(|x, y| x.distance(near).total_cmp(&y.distance(near)))
    };
    match (a, b) {
        (Curve::Line { p: p1, d: d1 }, Curve::Line { p: p2, d: d2 }) => {
            line_intersect(p1, d1, p2, d2).map(|t| p1 + d1 * t).filter(|p| p.is_finite())
        }
        (Curve::Line { p, d }, Curve::Circle { c, r }) | (Curve::Circle { c, r }, Curve::Line { p, d }) => {
            // |p + t d - c|² = r² with |d| = 1.
            let w = p - c;
            let b = d.dot(w);
            let disc = b * b - (w.length_squared() - r * r);
            if disc < 0.0 {
                return None;
            }
            let s = disc.sqrt();
            nearest([p + d * (-b - s), p + d * (-b + s)])
        }
        (Curve::Circle { c: c1, r: r1 }, Curve::Circle { c: c2, r: r2 }) => {
            let d = c2 - c1;
            let dist = d.length();
            if dist < 1e-9 {
                return None;
            }
            let a = (r1 * r1 - r2 * r2 + dist * dist) / (2.0 * dist);
            let h2 = r1 * r1 - a * a;
            if h2 < 0.0 {
                return None;
            }
            let m = c1 + d * (a / dist);
            let off = d.perp() * (h2.sqrt() / dist);
            nearest([m + off, m - off])
        }
    }
}

impl WallLine {
    /// The centerline arc, when the wall bulges and both faces keep a positive radius.
    pub(crate) fn arc(&self) -> Option<Arc> {
        Arc::from_bulge(self.start, self.end, self.bulge).filter(|a| a.radius - self.half > 1e-3)
    }

    fn chord_dir(&self) -> DVec2 {
        (self.end - self.start).try_normalize().unwrap_or(DVec2::X)
    }

    fn length(&self) -> f64 {
        self.arc().map_or(self.start.distance(self.end), |a| a.length())
    }

    /// The end point and the unit direction into the wall there.
    fn end_ray(&self, at_start: bool) -> (DVec2, DVec2) {
        match (self.arc(), at_start) {
            (Some(a), true) => (self.start, a.tangent(0.0)),
            (Some(a), false) => (self.end, -a.tangent(a.length())),
            (None, true) => (self.start, self.chord_dir()),
            (None, false) => (self.end, -self.chord_dir()),
        }
    }

    /// `(u, v)` of `p`: along the centerline from the start and to its left.
    fn locate(&self, p: DVec2) -> (f64, f64) {
        match self.arc() {
            Some(a) => a.locate(p),
            None => {
                let d = self.chord_dir();
                ((p - self.start).dot(d), (p - self.start).dot(d.perp()))
            }
        }
    }

    fn tangent_at(&self, u: f64) -> DVec2 {
        self.arc().map_or(self.chord_dir(), |a| a.tangent(u))
    }

    /// The curve `v` to the left of the centerline.
    fn face(&self, v: f64) -> Curve {
        match self.arc() {
            Some(a) => Curve::Circle { c: a.center, r: a.radius_at(v) },
            None => {
                let d = self.chord_dir();
                Curve::Line { p: self.start + d.perp() * v, d }
            }
        }
    }

    /// Offsets to the left of the centerline of the ray-left and ray-right
    /// faces at one end (the ray runs into the wall).
    fn ray_faces(&self, at_start: bool) -> (f64, f64) {
        if at_start { (self.half, -self.half) } else { (-self.half, self.half) }
    }

    /// Distance into the wall from one end of the face point `p`.
    fn depth(&self, at_start: bool, p: DVec2) -> f64 {
        let u = self.locate(p).0;
        if at_start { u } else { self.length() - u }
    }
}

const JOIN_TOL: f64 = 0.01;

/// How far short of another wall's face an end may stop and still join it.
const GAP_TOL: f64 = 0.05;

/// Distance into wall `a` (from the end at `p`, ray `da`) where its face
/// `va` meets `face`: the exact crossing of the two curves, or for curves
/// that miss each other the crossing of `a`'s tangent face line with
/// `fallback` (a line through a point along a direction).
fn face_depth(
    a: &WallLine,
    at_start: bool,
    va: f64,
    face: Curve,
    fallback: (DVec2, DVec2),
    p: DVec2,
    da: DVec2,
) -> Option<f64> {
    if let Some(x) = meet(a.face(va), face, p) {
        return Some(a.depth(at_start, x));
    }
    // The face of `a` along its tangent at the end, measured along the ray.
    let side = if at_start { va } else { -va };
    line_intersect(p + da.perp() * side, da, fallback.0, fallback.1)
}

/// The end of `walls[me]` (`at_start` selects it) meeting the face of
/// `walls[host]`: a T-join cut along that face.
fn t_join(walls: &[WallLine], me: usize, at_start: bool, host: usize) -> Option<EndJoin> {
    let a = walls[me];
    let b = walls[host];
    let (p, da) = a.end_ray(at_start);
    let max_ext = 10.0 * a.half.max(0.05);
    let clamp = |t: f64| t.clamp(-max_ext, max_ext);
    let (ub, _) = b.locate(p);
    let db = b.tangent_at(ub);
    let nb = db.perp();
    // The face on the side this wall comes from.
    let side = if da.dot(nb) >= 0.0 { 1.0 } else { -1.0 };
    let face = b.face(side * b.half);
    let fallback = (b.locate_point(ub, side * b.half), db);
    let (vl, vr) = a.ray_faces(at_start);
    let left = face_depth(&a, at_start, vl, face, fallback, p, da)?;
    let right = face_depth(&a, at_start, vr, face, fallback, p, da)?;
    Some(EndJoin { ray_left: clamp(left), ray_right: clamp(right), host: Some(host) })
}

impl WallLine {
    /// The point `u` along the centerline and `v` to its left.
    fn locate_point(&self, u: f64, v: f64) -> DVec2 {
        match self.arc() {
            Some(a) => a.at(u, v),
            None => {
                let d = self.chord_dir();
                self.start + d * u + d.perp() * v
            }
        }
    }
}

/// Resolve the join at one end of `walls[me]` (`at_start` selects the end).
/// Faces of arc walls are their true circles; where two faces do not cross
/// the end is cut along the tangent of the arc at the end instead.
pub(crate) fn resolve_join(walls: &[WallLine], me: usize, at_start: bool) -> Option<EndJoin> {
    let a = walls[me];
    if !(if at_start { a.join_start } else { a.join_end }) {
        return None;
    }
    let (p, da) = a.end_ray(at_start);
    let max_ext = 10.0 * a.half.max(0.05);
    let clamp = |t: f64| t.clamp(-max_ext, max_ext);
    let square = || Some(EndJoin { ray_left: 0.0, ray_right: 0.0, host: None });

    // Other wall ends at the same point (that allow joining), as rays into each wall.
    let ends: Vec<(usize, DVec2, bool)> = walls
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != me)
        .filter_map(|(i, b)| {
            if b.start.distance(p) < JOIN_TOL && b.join_start {
                Some((i, b.end_ray(true).1, true))
            } else if b.end.distance(p) < JOIN_TOL && b.join_end {
                Some((i, b.end_ray(false).1, false))
            } else {
                None
            }
        })
        .collect();

    // L-join: exactly one other wall ends here.
    if ends.len() == 1 {
        let (bi, db, b_start) = ends[0];
        let b = walls[bi];
        if da.perp_dot(db).abs() < 1e-6 {
            // Collinear (or tangent): a seamless butt when the thicknesses match.
            return if (a.half - b.half).abs() < 1e-4 && da.dot(db) < 0.0 { square() } else { None };
        }
        let nb = db.perp();
        // Pair this wall's ray-left face with the neighbor's ray-right face, and vice versa.
        let (vl, vr) = a.ray_faces(at_start);
        let (bl, br) = b.ray_faces(b_start);
        let left = face_depth(&a, at_start, vl, b.face(br), (p - nb * b.half, db), p, da)?;
        let right = face_depth(&a, at_start, vr, b.face(bl), (p + nb * b.half, db), p, da)?;
        return Some(EndJoin { ray_left: clamp(left), ray_right: clamp(right), host: None });
    }

    // Three or more ends: the thickest collinear pair runs through, the
    // others meet its face.
    if ends.len() >= 2 {
        let mut all: Vec<(usize, DVec2)> = vec![(me, da)];
        all.extend(ends.iter().map(|&(i, d, _)| (i, d)));
        let mut through: Option<(usize, usize, f64)> = None;
        for (x, &(i, di)) in all.iter().enumerate() {
            for &(j, dj) in all.iter().skip(x + 1) {
                if di.dot(dj) < -0.9999 {
                    let weight = walls[i].half + walls[j].half;
                    if through.is_none_or(|t| weight > t.2 + 1e-9) {
                        through = Some((i, j, weight));
                    }
                }
            }
        }
        let (i, j, _) = through?;
        if me == i || me == j {
            return square();
        }
        return t_join(walls, me, at_start, i);
    }

    // T-join: the end lies on (or within a few centimeters of) another wall,
    // away from its ends.
    let mut best: Option<(usize, f64)> = None;
    for (i, b) in walls.iter().enumerate() {
        if i == me {
            continue;
        }
        let len = b.length();
        let (along, v) = b.locate(p);
        let off = v.abs();
        let db = b.tangent_at(along);
        if along <= JOIN_TOL || along >= len - JOIN_TOL || off > b.half + GAP_TOL || da.perp_dot(db).abs() < 1e-6 {
            continue;
        }
        if best.is_none_or(|(_, o)| off < o) {
            best = Some((i, off));
        }
    }
    best.and_then(|(i, _)| t_join(walls, me, at_start, i))
}

/// Subtract `cut` from the sorted disjoint `intervals`.
fn subtract(intervals: &[(f64, f64)], cut: (f64, f64)) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for &(a, b) in intervals {
        if cut.1 <= a || cut.0 >= b {
            out.push((a, b));
            continue;
        }
        if cut.0 > a + 1e-9 {
            out.push((a, cut.0));
        }
        if cut.1 < b - 1e-9 {
            out.push((cut.1, b));
        }
    }
    out
}

impl WallGeom {
    pub(crate) fn new(
        element: ElementId,
        line: WallLine,
        base_z: f64,
        top_z: f64,
        start: Option<EndJoin>,
        end: Option<EndJoin>,
        layers: Vec<(f64, f64, String)>,
    ) -> Self {
        let arc = line.arc();
        let (_, dir) = line.end_ray(true);
        let length = line.length();
        // At the start the ray runs along +u and ray-left is the wall's left;
        // at the end the ray runs along -u and ray-left is the wall's right.
        let start_ext = start.map_or((0.0, 0.0), |j| (j.ray_left, j.ray_right));
        let end_ext = end.map_or((length, length), |j| (length - j.ray_right, length - j.ray_left));
        Self {
            element,
            origin: line.start,
            dir,
            normal: dir.perp(),
            length,
            half: line.half,
            base_z,
            top_z,
            start_ext,
            end_ext,
            start_joined: start.is_some(),
            end_joined: end.is_some(),
            openings: Vec::new(),
            columns: Vec::new(),
            layers,
            face_gaps: Vec::new(),
            arc,
            top_profile: Vec::new(),
        }
    }

    /// The top of the profile segment that holds `mid`, carried on to `u`
    /// (the corners of a mitered end lie a little beyond); `None` with a
    /// flat top.
    pub fn profile_top(&self, mid: f64, u: f64) -> Option<f64> {
        let (u0, u1, z0, z1) =
            *self.top_profile.iter().find(|s| s.0 - 1e-9 <= mid && mid <= s.1 + 1e-9).or_else(|| {
                if mid < self.top_profile.first()?.0 { self.top_profile.first() } else { self.top_profile.last() }
            })?;
        let t = if (u1 - u0).abs() > 1e-12 { (u - u0) / (u1 - u0) } else { 0.0 };
        Some(z0 + (z1 - z0) * t)
    }

    /// Follow a top profile: the columns split where its segments meet, so
    /// each one's top is a single plane.
    pub(crate) fn set_top_profile(&mut self, profile: Vec<(f64, f64, f64, f64)>) {
        let cuts: Vec<f64> = profile.iter().skip(1).map(|s| s.0).collect();
        let mut columns = Vec::with_capacity(self.columns.len() + cuts.len());
        for c in std::mem::take(&mut self.columns) {
            let mut c = c;
            for &u in &cuts {
                let (lo, hi) = (c.left.0.max(c.right.0), c.left.1.min(c.right.1));
                if u > lo + 1e-6 && u < hi - 1e-6 {
                    columns.push(Column { left: (c.left.0, u), right: (c.right.0, u), solid: c.solid.clone() });
                    c = Column { left: (u, c.left.1), right: (u, c.right.1), solid: c.solid };
                }
            }
            columns.push(c);
        }
        self.columns = columns;
        self.top_profile = profile;
    }

    /// The solid intervals of column `c` at `u`, its top following the profile.
    fn intervals_at(&self, c: &Column, u: f64) -> Vec<(f64, f64)> {
        let mut out = c.solid.clone();
        let mid = (c.left.0.max(c.right.0) + c.left.1.min(c.right.1)) * 0.5;
        if let (Some(last), Some(z)) = (out.last_mut(), self.profile_top(mid, u)) {
            last.1 = z.max(last.0 + 0.01);
        }
        out
    }

    /// World plan position of wall-local `(u, v)`.
    pub fn world(&self, u: f64, v: f64) -> DVec2 {
        match &self.arc {
            Some(a) => a.at(u, v),
            None => self.origin + self.dir * u + self.normal * v,
        }
    }

    /// Wall-local `(u, v)` of a plan point (arc walls: along the arc, the
    /// distance from its start measured to the nearer side of its gap).
    pub fn local(&self, p: DVec2) -> (f64, f64) {
        match &self.arc {
            Some(a) => a.locate(p),
            None => ((p - self.origin).dot(self.dir), (p - self.origin).dot(self.normal)),
        }
    }

    /// Direction of the centerline at `u`.
    pub fn tangent(&self, u: f64) -> DVec2 {
        self.arc.map_or(self.dir, |a| a.tangent(u))
    }

    /// Left normal of the centerline at `u`.
    pub fn normal_at(&self, u: f64) -> DVec2 {
        self.tangent(u).perp()
    }

    /// A straight frame `(origin, dir, normal)` matching the wall at `u`:
    /// `origin + dir * x + normal * y` is the wall point `(x, y)` for `x`
    /// near `u` (exact for straight walls; the tangent frame for arcs).
    pub fn frame_at(&self, u: f64) -> (DVec2, DVec2, DVec2) {
        match &self.arc {
            None => (self.origin, self.dir, self.normal),
            Some(_) => {
                let t = self.tangent(u);
                (self.world(u, 0.0) - t * u, t, t.perp())
            }
        }
    }

    /// The line `v` to the left of the centerline between `u0` and `u1`,
    /// as a polyline (two points for straight walls, short chords of the arc otherwise).
    pub fn path(&self, u0: f64, u1: f64, v: f64) -> Vec<DVec2> {
        match &self.arc {
            Some(a) => a.points(u0, u1, v, ARC_STEP),
            None => vec![self.world(u0, v), self.world(u1, v)],
        }
    }

    fn world3(&self, u: f64, v: f64, z: f64) -> DVec3 {
        let p = self.world(u, v);
        DVec3::new(p.x, p.y, z)
    }

    /// Range of u where both faces exist (where openings may be placed).
    pub fn inner_range(&self) -> (f64, f64) {
        (self.start_ext.0.max(self.start_ext.1), self.end_ext.0.min(self.end_ext.1))
    }

    /// The parts of the face on `side` (+1 left, -1 right) between `u0` and
    /// `u1` that no T-joined wall covers.
    pub fn face_segments(&self, side: f64, u0: f64, u1: f64) -> Vec<(f64, f64)> {
        let (lo, hi) = (u0.min(u1), u0.max(u1));
        let mut parts = vec![(lo, hi)];
        for &(s, g0, g1) in &self.face_gaps {
            if s == side {
                parts = subtract(&parts, (g0, g1));
            }
        }
        parts.retain(|(a, b)| b - a > 1e-6);
        parts
    }

    /// Plan outline, counter-clockwise, of the part of the wall whose left
    /// face spans `left` and right face spans `right`.
    pub fn outline(&self, left: (f64, f64), right: (f64, f64)) -> Vec<DVec2> {
        let h = self.half;
        let mut pts = self.path(right.0, right.1, -h);
        pts.extend(self.path(left.1, left.0, h));
        pts
    }

    /// Exact plan area of [`Self::outline`] (arcs included).
    pub fn outline_area(&self, left: (f64, f64), right: (f64, f64)) -> f64 {
        let Some(a) = &self.arc else { return crate::polygon::area(&self.outline(left, right)) };
        let h = self.half;
        // Green's theorem: an arc of radius r from p0 to p1 turning by dφ adds
        // (r² dφ + c × (p1 - p0)) / 2; a segment p → q adds (p × q) / 2.
        let arc_part = |u0: f64, u1: f64, v: f64| {
            let r = a.radius_at(v);
            let (p0, p1) = (a.at(u0, v), a.at(u1, v));
            (r * r * (a.angle_at(u1) - a.angle_at(u0)) + a.center.perp_dot(p1 - p0)) * 0.5
        };
        let seg = |p: DVec2, q: DVec2| p.perp_dot(q) * 0.5;
        let sum = arc_part(right.0, right.1, -h)
            + seg(a.at(right.1, -h), a.at(left.1, h))
            + arc_part(left.1, left.0, h)
            + seg(a.at(left.0, h), a.at(right.0, -h));
        sum.abs()
    }

    /// Plan footprint of a column, counter-clockwise.
    pub fn column_polygon(&self, c: &Column) -> Vec<DVec2> {
        self.outline(c.left, c.right)
    }

    /// Full plan footprint of the wall.
    pub fn footprint(&self) -> Vec<DVec2> {
        self.outline((self.start_ext.0, self.end_ext.0), (self.start_ext.1, self.end_ext.1))
    }

    /// Cut openings and split the wall into columns.
    pub(crate) fn set_openings(&mut self, mut openings: Vec<Opening>) {
        let (lo, hi) = self.inner_range();
        let eps = 1e-4;
        for o in &mut openings {
            o.u0 = o.u0.max(lo + eps);
            o.u1 = o.u1.min(hi - eps);
            o.z0 = o.z0.max(self.base_z);
            o.z1 = o.z1.min(self.top_z);
        }
        openings.retain(|o| o.u1 - o.u0 > eps && o.z1 - o.z0 > eps);

        let mut bounds: Vec<f64> = openings.iter().flat_map(|o| [o.u0, o.u1]).collect();
        bounds.sort_by(f64::total_cmp);
        bounds.dedup_by(|a, b| (*a - *b).abs() < 1e-6);

        let mut cols = Vec::with_capacity(bounds.len() + 1);
        for i in 0..=bounds.len() {
            let left = (
                if i == 0 { self.start_ext.0 } else { bounds[i - 1] },
                if i == bounds.len() { self.end_ext.0 } else { bounds[i] },
            );
            let right = (
                if i == 0 { self.start_ext.1 } else { bounds[i - 1] },
                if i == bounds.len() { self.end_ext.1 } else { bounds[i] },
            );
            let mid = (left.0.max(right.0) + left.1.min(right.1)) * 0.5;
            let mut solid = vec![(self.base_z, self.top_z)];
            for o in openings.iter().filter(|o| o.u0 <= mid && mid <= o.u1) {
                solid = subtract(&solid, (o.z0, o.z1));
            }
            cols.push(Column { left, right, solid });
        }
        self.columns = cols;
        self.openings = openings;
    }

    /// Net volume: each column's plan area times its solid height. Computed
    /// analytically because the body solid omits end caps at joins (and
    /// meshes arcs with chords).
    pub fn volume(&self) -> f64 {
        self.columns
            .iter()
            .map(|c| {
                let mid = (c.left.0.max(c.right.0) + c.left.1.min(c.right.1)) * 0.5;
                let height: f64 = self.intervals_at(c, mid).iter().map(|(z0, z1)| z1 - z0).sum();
                self.outline_area(c.left, c.right) * height
            })
            .sum()
    }

    pub fn column_at(&self, u: f64) -> Option<&Column> {
        self.columns.iter().find(|c| c.left.0.max(c.right.0) <= u && u <= c.left.1.min(c.right.1))
    }

    /// Number of mesh segments along a column: one for straight walls.
    fn segments(&self, c: &Column) -> usize {
        match &self.arc {
            Some(a) => {
                let du = (c.left.1 - c.left.0).abs().max((c.right.1 - c.right.0).abs());
                segments(du / a.radius.max(1e-9), ARC_STEP)
            }
            None => 1,
        }
    }

    /// Exterior faces of the wall body (arc faces as smooth facets).
    pub fn to_solid(&self, color: [u8; 3]) -> Solid {
        let mut s = Solid::new(self.element, Category::Wall, color);
        let h = self.half;
        let curved = self.arc.is_some();
        let flat = |v: DVec2| DVec3::new(v.x, v.y, 0.0);
        let quad = |u0: f64, v0: f64, u1: f64, v1: f64, z0: f64, z1: f64, n: DVec3| {
            let verts = vec![
                self.world3(u0, v0, z0),
                self.world3(u1, v1, z0),
                self.world3(u1, v1, z1),
                self.world3(u0, v0, z1),
            ];
            Face::new(verts, n)
        };
        // Faces of an arc are facets of one smooth surface.
        let side = |mut f: Face| {
            f.smooth = curved;
            f
        };
        // A quad whose top runs from `za` (at u0) to `zb` (at u1).
        let slanted = |u0: f64, v0: f64, u1: f64, v1: f64, z0: f64, za: f64, zb: f64, n: DVec3| {
            let verts = vec![
                self.world3(u0, v0, z0),
                self.world3(u1, v1, z0),
                self.world3(u1, v1, zb),
                self.world3(u0, v0, za),
            ];
            Face::new(verts, n)
        };
        for (ci, c) in self.columns.iter().enumerate() {
            let n = self.segments(c);
            let at = |range: (f64, f64), i: usize| range.0 + (range.1 - range.0) * i as f64 / n as f64;
            let last = c.solid.len().saturating_sub(1);
            for (k, &(z0, z1)) in c.solid.iter().enumerate() {
                // The top interval follows the profile; others are level.
                let top = |u: f64| if k == last { self.intervals_at(c, u).last().map_or(z1, |iv| iv.1) } else { z1 };
                for i in 0..n {
                    let (l0, l1) = (at(c.left, i), at(c.left, i + 1));
                    let (r0, r1) = (at(c.right, i), at(c.right, i + 1));
                    let poly = [self.world(r0, -h), self.world(r1, -h), self.world(l1, h), self.world(l0, h)];
                    let tops = [top(r0), top(r1), top(l1), top(l0)];
                    let upper: Vec<DVec3> = poly.iter().zip(tops).map(|(p, z)| DVec3::new(p.x, p.y, z)).collect();
                    let up = (upper[1] - upper[0]).cross(upper[2] - upper[0]).try_normalize().unwrap_or(DVec3::Z);
                    s.faces.push(Face::new(upper, up));
                    s.faces.push(Face::new(poly.iter().rev().map(|p| DVec3::new(p.x, p.y, z0)).collect(), -DVec3::Z));
                    let right_n = -flat(self.normal_at((r0 + r1) * 0.5));
                    let left_n = flat(self.normal_at((l0 + l1) * 0.5));
                    s.faces.push(side(slanted(r0, -h, r1, -h, z0, tops[0], tops[1], right_n)));
                    s.faces.push(side(slanted(l1, h, l0, h, z0, tops[2], tops[3], left_n)));
                }
            }
            // Faces on the boundary with the next column where only one side is solid.
            if let Some(next) = self.columns.get(ci + 1) {
                let u = c.left.1;
                let along = flat(self.tangent(u));
                let (mine, theirs) = (self.intervals_at(c, u), self.intervals_at(next, u));
                for &iv in &mine {
                    let exposed = theirs
                        .iter()
                        .fold(vec![iv], |acc, &cut| acc.iter().flat_map(|&i| subtract(&[i], cut)).collect());
                    for (z0, z1) in exposed {
                        s.faces.push(quad(u, -h, u, h, z0, z1, along));
                    }
                }
                for &iv in &theirs {
                    let exposed =
                        mine.iter().fold(vec![iv], |acc, &cut| acc.iter().flat_map(|&i| subtract(&[i], cut)).collect());
                    for (z0, z1) in exposed {
                        s.faces.push(quad(u, h, u, -h, z0, z1, -along));
                    }
                }
            }
        }
        // End caps, omitted where another wall closes the joint.
        let cap = |s: &mut Solid, col: &Column, (ul, ur): (f64, f64), outward: f64| {
            let e = self.world(ul, h) - self.world(ur, -h);
            let mut n = e.perp().normalize_or_zero();
            if n.dot(self.tangent((ul + ur) * 0.5) * outward) < 0.0 {
                n = -n;
            }
            let n3 = DVec3::new(n.x, n.y, 0.0);
            let (at_r, at_l) = (self.intervals_at(col, ur), self.intervals_at(col, ul));
            for (&(z0, zr), &(_, zl)) in at_r.iter().zip(&at_l) {
                s.faces.push(slanted(ur, -h, ul, h, z0, zr, zl, n3));
            }
        };
        if !self.start_joined
            && let Some(c) = self.columns.first()
        {
            cap(&mut s, c, self.start_ext, -1.0);
        }
        if !self.end_joined
            && let Some(c) = self.columns.last()
        {
            cap(&mut s, c, self.end_ext, 1.0);
        }
        s
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn line(a: (f64, f64), b: (f64, f64)) -> WallLine {
        WallLine {
            start: DVec2::new(a.0, a.1),
            end: DVec2::new(b.0, b.1),
            half: 0.1,
            join_start: true,
            join_end: true,
            bulge: 0.0,
        }
    }

    fn geom(walls: &[WallLine], i: usize) -> WallGeom {
        let s = resolve_join(walls, i, true);
        let e = resolve_join(walls, i, false);
        let mut g = WallGeom::new(ElementId(i as u64), walls[i], 0.0, 3.0, s, e, vec![]);
        g.set_openings(vec![]);
        g
    }

    #[test]
    fn free_wall_volume() {
        let g = geom(&[line((0.0, 0.0), (5.0, 0.0))], 0);
        assert!((g.to_solid([0; 3]).volume() - 5.0 * 0.2 * 3.0).abs() < 1e-9);
    }

    #[test]
    fn l_join_miters_both_walls() {
        let walls = [line((0.0, 0.0), (4.0, 0.0)), line((4.0, 0.0), (4.0, 3.0))];
        let a = geom(&walls, 0);
        // Wall A runs +X with left face towards +Y (the inside of the corner).
        assert!((a.end_ext.0 - 3.9).abs() < 1e-9, "{:?}", a.end_ext);
        assert!((a.end_ext.1 - 4.1).abs() < 1e-9, "{:?}", a.end_ext);
        assert!(a.end_joined && !a.start_joined);
        let b = geom(&walls, 1);
        // Wall B runs +Y with its left face towards -X (also the inside).
        assert!((b.start_ext.0 - 0.1).abs() < 1e-9 && (b.start_ext.1 - (-0.1)).abs() < 1e-9, "{:?}", b.start_ext);
    }

    #[test]
    fn t_join_stops_at_face() {
        let walls = [line((0.0, 0.0), (6.0, 0.0)), line((3.0, 0.0), (3.0, 4.0))];
        let b = geom(&walls, 1);
        assert!((b.start_ext.0 - 0.1).abs() < 1e-9 && (b.start_ext.1 - 0.1).abs() < 1e-9);
        let a = geom(&walls, 0);
        assert!(!a.start_joined && !a.end_joined);
    }

    #[test]
    fn opening_removes_volume() {
        let walls = [line((0.0, 0.0), (5.0, 0.0))];
        let mut g = geom(&walls, 0);
        g.set_openings(vec![Opening { element: ElementId(9), u0: 1.0, u1: 2.0, z0: 1.0, z1: 2.0 }]);
        assert_eq!(g.columns.len(), 3);
        assert_eq!(g.columns[1].solid, vec![(0.0, 1.0), (2.0, 3.0)]);
        let v = g.to_solid([0; 3]).volume();
        assert!((v - (3.0 - 0.2)).abs() < 1e-9, "{v}");
    }

    #[test]
    fn disallowed_ends_stay_square_and_neighbors_ignore_them() {
        // An L corner at (4, 0).
        let mut walls = [line((0.0, 0.0), (4.0, 0.0)), line((4.0, 0.0), (4.0, 3.0))];
        assert!(resolve_join(&walls, 0, false).is_some());
        assert!(resolve_join(&walls, 1, true).is_some());
        walls[0].join_end = false;
        assert!(resolve_join(&walls, 0, false).is_none());
        // The neighbor no longer miters against it either.
        assert!(resolve_join(&walls, 1, true).is_none());
    }

    #[test]
    fn ends_at_the_face_or_slightly_short_join_in_a_t() {
        // Half thickness 0.1: an end at the face (0.1) and one 3 cm short (0.13) both join.
        for y in [0.0, 0.1, 0.13] {
            let walls = [line((0.0, 0.0), (6.0, 0.0)), line((3.0, y), (3.0, 2.0))];
            let j = resolve_join(&walls, 1, true).unwrap_or_else(|| panic!("no join at {y}"));
            assert_eq!(j.host, Some(0));
            // Cut along the face y = 0.1, measured from the end along the wall.
            assert!((j.ray_left - (0.1 - y)).abs() < 1e-9 && (j.ray_right - (0.1 - y)).abs() < 1e-9, "{y}: {j:?}");
        }
        // Too far away: no join.
        let walls = [line((0.0, 0.0), (6.0, 0.0)), line((3.0, 0.3), (3.0, 2.0))];
        assert!(resolve_join(&walls, 1, true).is_none());
    }

    #[test]
    fn junctions_of_three_or_four_walls_run_one_pair_through() {
        // Collinear pair meeting at (2, 0) plus a wall going up.
        let walls = [line((0.0, 0.0), (2.0, 0.0)), line((2.0, 0.0), (4.0, 0.0)), line((2.0, 0.0), (2.0, 2.0))];
        let a = resolve_join(&walls, 0, false).unwrap();
        let b = resolve_join(&walls, 1, true).unwrap();
        assert_eq!((a.ray_left, a.ray_right, a.host), (0.0, 0.0, None));
        assert_eq!((b.ray_left, b.ray_right, b.host), (0.0, 0.0, None));
        let c = resolve_join(&walls, 2, true).unwrap();
        assert!(c.host.is_some());
        assert!((c.ray_left - 0.1).abs() < 1e-9 && (c.ray_right - 0.1).abs() < 1e-9);
        // A plus: the thicker pair runs through.
        let mut plus = [
            line((0.0, 0.0), (2.0, 0.0)),
            line((2.0, 0.0), (4.0, 0.0)),
            line((2.0, -2.0), (2.0, 0.0)),
            line((2.0, 0.0), (2.0, 2.0)),
        ];
        plus[2].half = 0.15;
        plus[3].half = 0.15;
        assert_eq!(resolve_join(&plus, 2, false).map(|j| j.host), Some(None));
        assert_eq!(resolve_join(&plus, 0, false).and_then(|j| j.host), Some(2));
        // Collinear walls of the same thickness meet without a seam; different ones keep it.
        let pair = [line((0.0, 0.0), (2.0, 0.0)), line((2.0, 0.0), (4.0, 0.0))];
        assert!(resolve_join(&pair, 0, false).is_some());
        let mut pair = pair;
        pair[1].half = 0.2;
        assert!(resolve_join(&pair, 0, false).is_none());
    }

    fn arc(a: (f64, f64), b: (f64, f64), bulge: f64) -> WallLine {
        WallLine { bulge, ..line(a, b) }
    }

    /// Bulge of a quarter circle (sweep 90°).
    fn quarter() -> f64 {
        (std::f64::consts::PI / 8.0).tan()
    }

    #[test]
    fn free_arc_wall_volume_is_exact() {
        // A quarter circle of radius 4: 2π m long, 0.2 thick, 3 high.
        let g = geom(&[arc((4.0, 0.0), (0.0, 4.0), quarter())], 0);
        let expected = 2.0 * std::f64::consts::PI * 0.2 * 3.0;
        assert!((g.length - 2.0 * std::f64::consts::PI).abs() < 1e-9);
        assert!((g.volume() - expected).abs() < 1e-9, "{} vs {expected}", g.volume());
        // The meshed body is close (chords of 3° lose a little).
        let meshed = g.to_solid([0; 3]).volume();
        assert!((meshed - expected).abs() < 2e-3 * expected, "{meshed}");
        // Faces are concentric arcs: every footprint point is 3.9 or 4.1 from the center.
        for p in g.footprint() {
            let r = p.length();
            assert!((r - 3.9).abs() < 1e-9 || (r - 4.1).abs() < 1e-9, "{p}");
        }
        let (u, v) = g.local(DVec2::from_angle(0.3) * 4.05);
        assert!((u - 1.2).abs() < 1e-9 && (v - -0.05).abs() < 1e-9, "{u} {v}");
    }

    #[test]
    fn arc_and_straight_walls_meet_at_shared_corners() {
        // A straight wall along +X ending where a quarter circle (center at the origin) starts.
        let walls = [line((0.0, 0.0), (4.0, 0.0)), arc((4.0, 0.0), (0.0, 4.0), quarter())];
        let (a, b) = (geom(&walls, 0), geom(&walls, 1));
        assert!(a.end_joined && b.start_joined);
        let fa = a.footprint();
        let fb = b.footprint();
        // The end corners of the straight wall are corners of the arc wall's outline.
        let shared = fa.iter().filter(|p| fb.iter().any(|q| q.distance(**p) < 1e-9)).count();
        assert_eq!(shared, 2, "{fa:?} / {fb:?}");
        // Inner corner on the inner circle and the line y = 0.1, outer on y = -0.1, r = 4.1.
        let inner = DVec2::new((3.9f64 * 3.9 - 0.01).sqrt(), 0.1);
        let outer = DVec2::new((4.1f64 * 4.1 - 0.01).sqrt(), -0.1);
        assert!(fa.iter().any(|p| p.distance(inner) < 1e-9) && fa.iter().any(|p| p.distance(outer) < 1e-9));
        // Joined, the two pieces fill the corner exactly: area of each analytic outline.
        let total = a.outline_area((a.start_ext.0, a.end_ext.0), (a.start_ext.1, a.end_ext.1))
            + b.outline_area((b.start_ext.0, b.end_ext.0), (b.start_ext.1, b.end_ext.1));
        assert!(total > 0.2 * (4.0 + 2.0 * std::f64::consts::PI) - 0.05, "{total}");
    }

    #[test]
    fn straight_wall_meets_the_face_of_an_arc_wall() {
        // A half circle of radius 4 through (0, 4) and a wall from the center up to it.
        let walls = [arc((4.0, 0.0), (-4.0, 0.0), 1.0), line((0.0, 0.0), (0.0, 4.0))];
        let j = resolve_join(&walls, 1, false).unwrap();
        assert_eq!(j.host, Some(0));
        let cut = 4.0 - (3.9f64 * 3.9 - 0.01).sqrt();
        assert!((j.ray_left - cut).abs() < 1e-9 && (j.ray_right - cut).abs() < 1e-9, "{j:?}");
        // The arc's inner face opens there.
        let b = geom(&walls, 1);
        assert!((b.end_ext.0 - (4.0 - cut)).abs() < 1e-9);
        // Two arcs continuing each other (a full circle) butt square.
        let ring = [arc((4.0, 0.0), (-4.0, 0.0), 1.0), arc((-4.0, 0.0), (4.0, 0.0), 1.0)];
        let j = resolve_join(&ring, 0, false).unwrap();
        assert_eq!((j.ray_left, j.ray_right), (0.0, 0.0));
        let total: f64 = (0..2).map(|i| geom(&ring, i).volume()).sum();
        let annulus = std::f64::consts::PI * (4.1 * 4.1 - 3.9 * 3.9) * 3.0;
        assert!((total - annulus).abs() < 1e-9, "{total} vs {annulus}");
    }

    #[test]
    fn faces_open_where_walls_meet_them() {
        let walls = [line((0.0, 0.0), (6.0, 0.0)), line((3.0, 0.1), (3.0, 2.0))];
        let mut host = geom(&walls, 0);
        host.face_gaps.push((1.0, 2.9, 3.1));
        assert_eq!(host.face_segments(1.0, 0.0, 6.0), vec![(0.0, 2.9), (3.1, 6.0)]);
        assert_eq!(host.face_segments(-1.0, 0.0, 6.0), vec![(0.0, 6.0)]);
    }
}
