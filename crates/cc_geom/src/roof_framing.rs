//! Roof framing: the purlins, rafters and battens under a roof's sloped
//! faces, from its type's framing and covering (see `cc_core::roofing`).
//! Each sloped underside of the roof's body is framed on its own: battens
//! along the eaves at the tile's gauge, right under the roof; rafters down
//! the slope under the battens; purlins along the eaves under the rafters,
//! spread evenly from eave to ridge no farther apart than the purlin step.
//! Steel framing has purlins only, right under the sheets. Members are
//! boxes with their height across the roof plane. Coplanar faces are framed
//! together (a face with openings comes as several polygons).

use crate::Solid;
use cc_core::roofing::{FramingSystem, MemberSize, RoofFraming, TileKind};
use glam::{DVec2, DVec3};

/// Most members one roof gets (a guard against huge or crafted roofs).
const MAX_MEMBERS: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemberKind {
    Purlin,
    Rafter,
    Batten,
    /// A member of a truss: top chord, tie, king post or strut.
    Truss,
}

/// The part of a truss a member is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrussPart {
    /// A top chord, along a roof face.
    Chord,
    /// The level tie (bottom chord).
    Tie,
    /// The king post, under the apex.
    Post,
    /// A strut, from the post's foot to a chord.
    Strut,
}

/// A framing member: its axis (along the roof plane), section, the
/// in-plane direction its width runs along and the direction its height
/// runs down (away from the roof), with the depth below the roof's
/// underside where it starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Member {
    pub kind: MemberKind,
    pub a: DVec3,
    pub b: DVec3,
    pub size: MemberSize,
    /// Which part of its truss, for truss members.
    pub truss: Option<TrussPart>,
    side: DVec3,
    down: DVec3,
    depth: f64,
}

impl Member {
    pub fn length(&self) -> f64 {
        self.a.distance(self.b)
    }

    /// The member's eight corners: the near end (top left, top right,
    /// bottom right, bottom left), then the far end.
    fn corners(&self) -> [DVec3; 8] {
        let half = self.side * (self.size.width / 2.0);
        let top = self.down * self.depth;
        let bottom = self.down * (self.depth + self.size.height);
        let end = |p: DVec3| [p - half + top, p + half + top, p + half + bottom, p - half + bottom];
        let (n, f) = (end(self.a), end(self.b));
        [n[0], n[1], n[2], n[3], f[0], f[1], f[2], f[3]]
    }

    /// The member's box as polygons, faces turned outward.
    pub fn faces(&self) -> Vec<crate::Face> {
        let mut s = Solid::new(cc_core::ElementId(0), cc_core::Category::Beam, [0; 3]);
        self.add_to(&mut s);
        s.faces
    }

    /// Add the member's box to `solid`, faces turned outward.
    pub fn add_to(&self, solid: &mut Solid) {
        let c = self.corners();
        let center = c.iter().copied().sum::<DVec3>() / 8.0;
        let quads = [[0, 1, 2, 3], [7, 6, 5, 4], [0, 4, 5, 1], [1, 5, 6, 2], [2, 6, 7, 3], [3, 7, 4, 0]];
        let faces = quads.map(|q| {
            let mut f: Vec<DVec3> = q.iter().map(|i| c[*i]).collect();
            let n = (f[1] - f[0]).cross(f[2] - f[0]);
            let mid = f.iter().copied().sum::<DVec3>() / 4.0;
            if n.dot(mid - center) < 0.0 {
                f.reverse();
            }
            f
        });
        solid.add_polygons(faces);
    }
}

/// The framing of one roof and the figures of its covering.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoofFramingGeom {
    pub members: Vec<Member>,
    /// True area of the sloped faces (what the covering covers).
    pub sloped_area: f64,
    /// The gentlest and steepest slopes of the faces, rise over run.
    pub slopes: Option<(f64, f64)>,
    /// How many trusses carry the purlins.
    pub trusses: usize,
}

impl RoofFramingGeom {
    /// Total length of the members of `kind`.
    pub fn length(&self, kind: MemberKind) -> f64 {
        self.members.iter().filter(|m| m.kind == kind).map(Member::length).sum::<f64>() + 0.0
    }

    /// Total length of every member.
    pub fn total_length(&self) -> f64 {
        self.members.iter().map(Member::length).sum::<f64>() + 0.0
    }

    /// Volume of every member.
    pub fn volume(&self) -> f64 {
        self.members.iter().map(|m| m.length() * m.size.width * m.size.height).sum::<f64>() + 0.0
    }
}

/// Where the line at `c` across axis `k` (0: the line x = c, 1: y = c)
/// crosses the polygons `polys` (the pieces of one face): the parts inside,
/// as ranges along the other axis, pieces that touch merged.
fn chords(polys: &[Vec<DVec2>], k: usize, c: f64) -> Vec<(f64, f64)> {
    let mut parts: Vec<(f64, f64)> = polys.iter().flat_map(|p| crossings(p, k, c)).collect();
    parts.sort_by(|a, b| a.0.total_cmp(&b.0));
    merge(parts).into_iter().filter(|(a, b)| b - a > 1e-3).collect()
}

/// Ranges sorted by their start, overlapping or touching ones joined.
fn merge(parts: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (a, b) in parts {
        match out.last_mut() {
            Some(last) if a <= last.1 + 1e-6 => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// Where the line at `c` across axis `k` crosses one polygon.
fn crossings(poly: &[DVec2], k: usize, c: f64) -> Vec<(f64, f64)> {
    let along = |p: DVec2| if k == 0 { p.y } else { p.x };
    let across = |p: DVec2| if k == 0 { p.x } else { p.y };
    let n = poly.len();
    let mut hits: Vec<f64> = (0..n)
        .filter_map(|i| {
            let (p, q) = (*poly.get(i)?, *poly.get((i + 1) % n)?);
            let (u, v) = (across(p), across(q));
            // Half-open, so a vertex on the line counts once.
            ((u <= c) != (v <= c)).then(|| along(p) + (c - u) / (v - u) * (along(q) - along(p)))
        })
        .collect();
    hits.sort_by(f64::total_cmp);
    hits.as_chunks::<2>().0.iter().map(|[a, b]| (*a, *b)).collect()
}

/// Positions from `lo` to `hi` (inset by `margin`) no farther apart than
/// `step`, spread evenly with both ends included.
fn spread(lo: f64, hi: f64, margin: f64, step: f64) -> Vec<f64> {
    let (a, b) = (lo + margin, hi - margin);
    if b - a < 1e-6 {
        return vec![(lo + hi) / 2.0];
    }
    let gaps = ((b - a) / step).ceil().clamp(1.0, MAX_MEMBERS as f64) as usize;
    (0..=gaps).map(|i| a + (b - a) * i as f64 / gaps as f64).collect()
}

/// Positions every `step` from `lo + start` while inside `hi - margin`.
fn every(lo: f64, hi: f64, start: f64, step: f64, margin: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let mut c = lo + start;
    while c <= hi - margin && out.len() < MAX_MEMBERS {
        out.push(c);
        c += step;
    }
    out
}

/// Where the vertical plane `p · r = c` crosses the pieces of a planar face:
/// the segments inside it, ordered along `across`, pieces that touch merged.
fn plane_cuts(polys: &[Vec<DVec3>], r: DVec3, c: f64, across: DVec3) -> Vec<(DVec3, DVec3)> {
    let mut parts: Vec<(DVec3, DVec3)> = Vec::new();
    for poly in polys {
        let n = poly.len();
        let mut hits: Vec<DVec3> = (0..n)
            .filter_map(|i| {
                let (p, q) = (*poly.get(i)?, *poly.get((i + 1) % n)?);
                let (u, v) = (p.dot(r) - c, q.dot(r) - c);
                ((u <= 0.0) != (v <= 0.0)).then(|| p + (q - p) * (u / (u - v)))
            })
            .collect();
        hits.sort_by(|a, b| a.dot(across).total_cmp(&b.dot(across)));
        parts.extend(hits.as_chunks::<2>().0.iter().map(|[a, b]| (*a, *b)));
    }
    parts.sort_by(|a, b| a.0.dot(across).total_cmp(&b.0.dot(across)));
    let mut out: Vec<(DVec3, DVec3)> = Vec::new();
    for (a, b) in parts {
        match out.last_mut() {
            Some(last) if a.dot(across) <= last.1.dot(across) + 1e-6 => {
                if b.dot(across) > last.1.dot(across) {
                    last.1 = b;
                }
            }
            _ => out.push((a, b)),
        }
    }
    out.retain(|(a, b)| a.distance(*b) > 0.05);
    out
}

/// The two highest top chords rising toward each other (the two sides of a
/// ridge), moved down under their faces, cross past each other or stop short:
/// end both where their lines cross, so they meet at the truss's apex.
fn meet_at_apex(cuts: &mut [(DVec3, DVec3)], across: DVec3) {
    // Each chord with its high end last.
    for (a, b) in cuts.iter_mut() {
        if a.z > b.z {
            std::mem::swap(a, b);
        }
    }
    let rising = |k: usize, cuts: &[(DVec3, DVec3)]| cuts.get(k).map(|(a, b)| (b.dot(across) - a.dot(across)).signum());
    let mut order: Vec<usize> = (0..cuts.len()).collect();
    order.sort_by(|i, j| cuts[*j].1.z.total_cmp(&cuts[*i].1.z));
    let Some(&i) = order.first() else { return };
    let Some(&j) = order.iter().skip(1).find(|&&k| rising(k, cuts) != rising(i, cuts)) else { return };
    let (a1, b1) = cuts[i];
    let (a2, b2) = cuts[j];
    let flat = |p: DVec3| DVec2::new(p.dot(across), p.z);
    let (p, d1) = (flat(a1), flat(b1) - flat(a1));
    let (q, d2) = (flat(a2), flat(b2) - flat(a2));
    let den = d1.perp_dot(d2);
    if den.abs() < 1e-9 {
        return;
    }
    let t = (q - p).perp_dot(d2) / den;
    let apex2 = p + d1 * t;
    // Only a small extension: chords that do not belong together stay as they are.
    let reach = |b: DVec3| flat(b).distance(apex2);
    if reach(b1) > 1.0 || reach(b2) > 1.0 {
        return;
    }
    let lift = |a: DVec3, b: DVec3| a + (b - a) * ((apex2.x - a.dot(across)) / (b.dot(across) - a.dot(across)));
    cuts[i].1 = lift(a1, b1);
    cuts[j].1 = lift(a2, b2);
}

/// A sloped underside of the roof: the pieces of one plane, its outward
/// normal, its eave direction and its area.
struct SlopedFace {
    pieces: Vec<Vec<DVec3>>,
    normal: DVec3,
    along: DVec3,
    area: f64,
}

/// Trusses under the purlins: plane frames across the ridge (the eave
/// direction of the largest sloped face), spread evenly along it no farther
/// apart than the truss spacing. Only the faces sloping across those planes
/// carry them (not hip ends or wings running the other way), and chords
/// apart from each other make separate trusses. In each, the top chords
/// follow the roof's faces `depth` under them (under the purlins), the tie
/// runs level at the height of the chords' lowest end, a king post drops
/// from their highest point to the tie, and struts run from its foot to the
/// middle of each sloped chord.
fn trusses(faces: &[SlopedFace], framing: &RoofFraming, depth: f64, out: &mut RoofFramingGeom) {
    let Some(largest) = faces.iter().max_by(|a, b| a.area.total_cmp(&b.area)) else { return };
    let r = DVec3::new(largest.along.x, largest.along.y, 0.0).normalize_or_zero();
    let across = DVec3::new(-r.y, r.x, 0.0);
    if r == DVec3::ZERO {
        return;
    }
    // The faces sloping across the trusses, moved down to the chords' top.
    let chords_at: Vec<Vec<Vec<DVec3>>> = faces
        .iter()
        .filter(|f| f.along.dot(r).abs() > 0.99)
        .map(|f| f.pieces.iter().map(|v| v.iter().map(|p| *p + f.normal * depth).collect()).collect())
        .collect();
    let (lo, hi) = chords_at
        .iter()
        .flatten()
        .flatten()
        .map(|p| p.dot(r))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(x), hi.max(x)));
    if (hi - lo).is_nan() || hi - lo <= 0.3 {
        return;
    }
    let t = framing.truss;
    let put = |out: &mut RoofFramingGeom, a: DVec3, b: DVec3, down: DVec3, depth: f64, part: TrussPart| {
        if out.members.len() < MAX_MEMBERS && a.distance(b) > 0.05 {
            let truss = Some(part);
            out.members.push(Member { kind: MemberKind::Truss, a, b, size: t, truss, side: r, down, depth });
        }
    };
    let span = |cut: &(DVec3, DVec3)| {
        let (u, v) = (cut.0.dot(across), cut.1.dot(across));
        (u.min(v), u.max(v))
    };
    for c in spread(lo, hi, 0.15, framing.truss_spacing.max(0.3)) {
        let mut all: Vec<(DVec3, DVec3)> = chords_at.iter().flat_map(|f| plane_cuts(f, r, c, across)).collect();
        all.sort_by(|a, b| span(a).0.total_cmp(&span(b).0));
        // Chords that meet, or nearly do, belong to one truss.
        let mut groups: Vec<(f64, Vec<(DVec3, DVec3)>)> = Vec::new();
        for cut in all {
            let (s0, s1) = span(&cut);
            match groups.last_mut() {
                Some((end, g)) if s0 <= *end + 0.3 => {
                    *end = end.max(s1);
                    g.push(cut);
                }
                _ => groups.push((s1, vec![cut])),
            }
        }
        for (_, mut cuts) in groups {
            meet_at_apex(&mut cuts, across);
            let ends = cuts.iter().flat_map(|(a, b)| [*a, *b]);
            let Some(low) = ends.clone().map(|p| p.z).reduce(f64::min) else { continue };
            let (s0, s1) = ends
                .clone()
                .map(|p| p.dot(across))
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), s| (a.min(s), b.max(s)));
            let Some(apex) = ends.max_by(|a, b| a.z.total_cmp(&b.z)) else { continue };
            if s1 - s0 < 1.0 {
                continue;
            }
            // In the truss plane, at `s` across and height `z`.
            let point = |s: f64, z: f64| r * c + across * s + DVec3::Z * z;
            // Top chords, their height running down from the roof's side.
            for (a, b) in &cuts {
                let d = (*b - *a).normalize_or_zero();
                let mut down = r.cross(d).normalize_or_zero();
                if down.z > 0.0 {
                    down = -down;
                }
                put(out, *a, *b, down, 0.0, TrussPart::Chord);
            }
            // The tie, level, its top at the chords' lowest end.
            put(out, point(s0, low), point(s1, low), -DVec3::Z, 0.0, TrussPart::Tie);
            // The king post and the struts, centered on their lines.
            let foot = point(apex.dot(across), low);
            if apex.z - low > 0.3 {
                put(out, foot, apex, across, -t.height / 2.0, TrussPart::Post);
                for (a, b) in &cuts {
                    let mid = (*a + *b) / 2.0;
                    if (b.z - a.z).abs() > 0.05 && mid.distance(foot) > 0.3 {
                        let d = (mid - foot).normalize_or_zero();
                        put(out, foot, mid, r.cross(d).normalize_or_zero(), -t.height / 2.0, TrussPart::Strut);
                    }
                }
            }
            out.trusses += 1;
        }
    }
}

/// The roof's sloped undersides (facing down, neither flat nor vertical),
/// coplanar faces together.
fn sloped_faces(roof: &Solid) -> Vec<SlopedFace> {
    let mut out: Vec<SlopedFace> = Vec::new();
    for face in &roof.faces {
        let n = face.normal.normalize_or_zero();
        if n.z > -1e-6 || n.z < -0.9999 || face.verts.len() < 3 {
            continue;
        }
        let up = (DVec3::Z - n * n.z).normalize_or_zero();
        let along = up.cross(n).normalize_or_zero();
        let Some(&p) = face.verts.first() else { continue };
        if up == DVec3::ZERO || along == DVec3::ZERO {
            continue;
        }
        let same = |f: &SlopedFace| {
            f.normal.dot(n) > 1.0 - 1e-9
                && f.pieces.first().and_then(|v| v.first()).is_some_and(|q| (p - *q).dot(n).abs() < 1e-6)
        };
        match out.iter_mut().find(|f| same(f)) {
            Some(f) => {
                f.pieces.push(face.verts.clone());
                f.area += face.area();
            }
            None => out.push(SlopedFace { pieces: vec![face.verts.clone()], normal: n, along, area: face.area() }),
        }
    }
    out
}

/// The framing under a roof's body `roof` (its solid) for `framing` and
/// `covering`.
pub fn roof_framing(roof: &Solid, framing: &RoofFraming, covering: Option<TileKind>) -> RoofFramingGeom {
    let mut out = RoofFramingGeom::default();
    let sloped = sloped_faces(roof);
    for face in &sloped {
        let n = face.normal;
        let slope = n.truncate().length() / -n.z;
        out.slopes = Some(out.slopes.map_or((slope, slope), |(lo, hi)| (lo.min(slope), hi.max(slope))));
        out.sloped_area += face.area;
        if framing.system == FramingSystem::None {
            continue;
        }
        let (along, up) = (face.along, n.cross(face.along));
        let Some(&o) = face.pieces.first().and_then(|v| v.first()) else { continue };
        let polys: Vec<Vec<DVec2>> = face
            .pieces
            .iter()
            .map(|v| v.iter().map(|p| DVec2::new((*p - o).dot(along), (*p - o).dot(up))).collect())
            .collect();
        let lo = polys.iter().flatten().copied().fold(DVec2::splat(f64::INFINITY), DVec2::min);
        let hi = polys.iter().flatten().copied().fold(DVec2::splat(f64::NEG_INFINITY), DVec2::max);
        let at = |x: f64, t: f64| o + along * x + up * t;
        let mut put = |kind: MemberKind, size: MemberSize, depth: f64, a: DVec3, b: DVec3, side: DVec3| {
            if out.members.len() < MAX_MEMBERS {
                out.members.push(Member { kind, a, b, size, truss: None, side, down: n, depth });
            }
        };
        // Members along the eaves at heights `ts` up the slope, `depth` under the roof.
        let eave_lines =
            |kind,
             size: MemberSize,
             depth,
             ts: Vec<f64>,
             put: &mut dyn FnMut(MemberKind, MemberSize, f64, DVec3, DVec3, DVec3)| {
                for t in ts {
                    for (x0, x1) in chords(&polys, 1, t) {
                        put(kind, size, depth, at(x0, t), at(x1, t), up);
                    }
                }
            };
        match framing.system {
            FramingSystem::None => {}
            FramingSystem::Steel => {
                let ts = spread(lo.y, hi.y, 0.1, framing.purlin_step(covering));
                eave_lines(MemberKind::Purlin, framing.purlin, 0.0, ts, &mut put);
            }
            FramingSystem::Timber | FramingSystem::TimberTrusses => {
                let (bh, rh) = (framing.batten.height, framing.rafter.height);
                let gauge = framing.batten_step(covering);
                let ts = every(lo.y, hi.y, 0.05, gauge, 0.02);
                eave_lines(MemberKind::Batten, framing.batten, 0.0, ts, &mut put);
                for x in every(lo.x, hi.x, 0.05, framing.rafter_spacing.max(0.1), 0.02) {
                    for (t0, t1) in chords(&polys, 0, x) {
                        put(MemberKind::Rafter, framing.rafter, bh, at(x, t0), at(x, t1), along);
                    }
                }
                let ts = spread(lo.y, hi.y, 0.1, framing.purlin_step(covering));
                eave_lines(MemberKind::Purlin, framing.purlin, bh + rh, ts, &mut put);
            }
        }
    }
    if framing.system == FramingSystem::TimberTrusses {
        let depth = framing.batten.height + framing.rafter.height + framing.purlin.height;
        trusses(&sloped, framing, depth, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::{Category, ElementId};

    /// A shed roof's underside: 10 m along the eave, 4 m in plan up a 50 %
    /// slope (4.47 m along it), facing down.
    fn shed() -> Solid {
        let mut s = Solid::new(ElementId(1), Category::Roof, [0; 3]);
        let p = |x: f64, y: f64| DVec3::new(x, y, 0.5 * y);
        s.add_polygons(vec![vec![p(0.0, 0.0), p(0.0, 4.0), p(10.0, 4.0), p(10.0, 0.0)]]);
        s
    }

    #[test]
    fn sloped_faces_are_framed() {
        let roof = shed();
        assert!(roof.faces[0].normal.z < 0.0, "the test face looks down");
        let along_slope = 4.0 * 1.25f64.sqrt();
        let mut f = RoofFraming::timber("Madeira".into());
        // No framing: the figures only.
        let g = roof_framing(&roof, &f, Some(TileKind::CeramicFrench));
        assert!(g.members.is_empty());
        assert!((g.sloped_area - 10.0 * along_slope).abs() < 1e-9);
        assert!((g.slopes.unwrap().0 - 0.5).abs() < 1e-9);

        f.system = FramingSystem::Timber;
        let g = roof_framing(&roof, &f, Some(TileKind::CeramicFrench));
        let count = |k| g.members.iter().filter(|m| m.kind == k).count();
        // Purlins at most 1.5 m apart over 4.27 m: 4 lines of 10 m.
        assert_eq!(count(MemberKind::Purlin), 4);
        assert!((g.length(MemberKind::Purlin) - 40.0).abs() < 1e-6);
        // Rafters every 0.5 m from 5 cm in: 20, each the slope's length.
        assert_eq!(count(MemberKind::Rafter), 20);
        assert!((g.length(MemberKind::Rafter) - 20.0 * along_slope).abs() < 1e-6);
        // Battens at the French tile's 33 cm gauge.
        assert_eq!(count(MemberKind::Batten), ((along_slope - 0.07) / 0.33).floor() as usize + 1);
        // Stacked under the roof: battens, then rafters, then purlins, lower each time.
        let z = |k: MemberKind| {
            let m = g.members.iter().find(|m| m.kind == k).unwrap();
            m.corners().iter().map(|c| c.z - 0.5 * c.y).fold(f64::INFINITY, f64::min)
        };
        assert!(z(MemberKind::Batten) > z(MemberKind::Rafter) && z(MemberKind::Rafter) > z(MemberKind::Purlin));
        let expected = g.members.iter().map(|m| m.length() * m.size.width * m.size.height).sum::<f64>();
        assert!((g.volume() - expected).abs() < 1e-12);
        let mut s = Solid::new(ElementId(1), Category::Roof, [0; 3]);
        g.members[0].add_to(&mut s);
        let m = &g.members[0];
        assert!((s.volume() - m.length() * m.size.width * m.size.height).abs() < 1e-9, "closed, outward box");

        // Steel: purlins only, at the sheet's span when narrower.
        f.system = FramingSystem::Steel;
        f.purlin_spacing = 3.0;
        let g = roof_framing(&roof, &f, Some(TileKind::FiberCement));
        assert!(g.members.iter().all(|m| m.kind == MemberKind::Purlin));
        assert_eq!(g.members.len(), 4, "1.69 m spans over 4.27 m: 4 purlins");
    }

    #[test]
    fn helpers() {
        let square = [DVec2::ZERO, DVec2::new(2.0, 0.0), DVec2::new(2.0, 2.0), DVec2::new(0.0, 2.0)];
        assert_eq!(chords(&[square.to_vec()], 1, 1.0), vec![(0.0, 2.0)]);
        assert!(chords(&[square.to_vec()], 0, 3.0).is_empty());
        // Two halves of the square make one chord.
        let halves = [vec![square[0], square[1], square[2]], vec![square[0], square[2], square[3]]];
        assert_eq!(chords(&halves, 1, 0.5), vec![(0.0, 2.0)]);
        assert_eq!(spread(0.0, 1.0, 0.1, 0.5), vec![0.1, 0.5, 0.9]);
        assert_eq!(spread(0.0, 0.1, 0.1, 0.5), vec![0.05]);
        assert_eq!(every(0.0, 1.0, 0.1, 0.4, 0.05).len(), 3);
    }

    /// A gable's undersides: 10 m along the ridge (X), 8 m across, rising
    /// at 50 % to a ridge 2 m up at y = 4.
    fn gable() -> Solid {
        let mut s = Solid::new(ElementId(1), Category::Roof, [0; 3]);
        let p = |x: f64, y: f64| DVec3::new(x, y, 2.0 - 0.5 * (y - 4.0).abs());
        s.add_polygons(vec![
            vec![p(0.0, 0.0), p(0.0, 4.0), p(10.0, 4.0), p(10.0, 0.0)],
            vec![p(0.0, 4.0), p(0.0, 8.0), p(10.0, 8.0), p(10.0, 4.0)],
        ]);
        s
    }

    #[test]
    fn trusses_carry_the_purlins() {
        let roof = gable();
        assert!(roof.faces.iter().all(|f| f.normal.z < 0.0), "undersides look down");
        let f = RoofFraming { system: FramingSystem::TimberTrusses, ..RoofFraming::timber(String::new()) };
        let g = roof_framing(&roof, &f, Some(TileKind::CeramicColonial));
        // At most 3 m apart over 10 m (inset 15 cm): 5 trusses.
        assert_eq!(g.trusses, 5);
        let truss: Vec<&Member> = g.members.iter().filter(|m| m.kind == MemberKind::Truss).collect();
        // Each: two top chords, the tie, the king post and two struts.
        assert_eq!(truss.len(), 5 * 6);
        // Every member lies in one of the trusses' vertical planes across the ridge.
        assert!(truss.iter().all(|m| (m.a.x - m.b.x).abs() < 1e-9));
        // The ties run level across the 8 m, below the chords' top.
        let depth = f.batten.height + f.rafter.height + f.purlin.height;
        let ties: Vec<&&Member> =
            truss.iter().filter(|m| (m.a.z - m.b.z).abs() < 1e-9 && (m.a.y - m.b.y).abs() > 7.0).collect();
        assert_eq!(ties.len(), 5);
        assert!(ties[0].a.z < -depth + 1e-6 + 0.0 + 1.0, "at the eaves' height, under the purlins");
        // The king posts rise from the tie to the apex at the ridge.
        let posts: Vec<&&Member> =
            truss.iter().filter(|m| (m.a.y - m.b.y).abs() < 1e-9 && (m.a.z - m.b.z).abs() > 1.0).collect();
        assert_eq!(posts.len(), 5);
        assert!(posts.iter().all(|m| (m.a.y - 4.0).abs() < 1e-6));
        // Purlins, rafters and battens are still there, above the trusses.
        assert!(
            g.length(MemberKind::Purlin) > 0.0
                && g.length(MemberKind::Rafter) > 0.0
                && g.length(MemberKind::Batten) > 0.0
        );
        // A shed (one face) gets half trusses: a chord, the tie, the post at the high end and a strut.
        let g = roof_framing(&shed(), &f, None);
        assert!(g.trusses > 0);
        assert_eq!(g.members.iter().filter(|m| m.kind == MemberKind::Truss).count(), g.trusses * 4);
    }

    /// A face with openings comes as several polygons (here the shed's face
    /// split in two triangles): it is framed as one face, as if whole.
    #[test]
    fn pieces_of_a_face_are_framed_together() {
        let p = |x: f64, y: f64| DVec3::new(x, y, 0.5 * y);
        let mut split = Solid::new(ElementId(1), Category::Roof, [0; 3]);
        split.add_polygons(vec![
            vec![p(0.0, 0.0), p(0.0, 4.0), p(10.0, 4.0)],
            vec![p(0.0, 0.0), p(10.0, 4.0), p(10.0, 0.0)],
        ]);
        let f = RoofFraming { system: FramingSystem::TimberTrusses, ..RoofFraming::timber(String::new()) };
        let (whole, split) = (
            roof_framing(&shed(), &f, Some(TileKind::CeramicFrench)),
            roof_framing(&split, &f, Some(TileKind::CeramicFrench)),
        );
        assert_eq!(split.members.len(), whole.members.len());
        assert_eq!(split.trusses, whole.trusses);
        for k in [MemberKind::Purlin, MemberKind::Rafter, MemberKind::Batten, MemberKind::Truss] {
            assert!((split.length(k) - whole.length(k)).abs() < 1e-6, "{k:?}");
        }
        assert!((split.sloped_area - whole.sloped_area).abs() < 1e-9);
    }

    /// A wing running the other way, and a second roof apart, do not join
    /// the trusses: each truss spans one gable, its tie no longer than 8 m.
    #[test]
    fn trusses_span_one_gable() {
        let mut roof = gable();
        // A wing from the gable's side (y 8 to 14), its ridge at x = 5 along y.
        let w = |x: f64, y: f64| DVec3::new(x, y, 2.0 - 0.5 * (x - 5.0).abs());
        // A second gable 2 m apart (y 10 to 18), beyond the wing's end in x.
        let g = |x: f64, y: f64| DVec3::new(x, y, 2.0 - 0.5 * (y - 14.0).abs());
        roof.add_polygons(vec![
            vec![w(3.0, 8.0), w(3.0, 14.0), w(5.0, 14.0), w(5.0, 8.0)],
            vec![w(5.0, 8.0), w(5.0, 14.0), w(7.0, 14.0), w(7.0, 8.0)],
            vec![g(8.0, 10.0), g(8.0, 14.0), g(10.0, 14.0), g(10.0, 10.0)],
            vec![g(8.0, 14.0), g(8.0, 18.0), g(10.0, 18.0), g(10.0, 14.0)],
        ]);
        assert!(roof.faces.iter().all(|f| f.normal.z < 0.0), "undersides look down");
        let f = RoofFraming { system: FramingSystem::TimberTrusses, ..RoofFraming::timber(String::new()) };
        let g = roof_framing(&roof, &f, None);
        let truss: Vec<&Member> = g.members.iter().filter(|m| m.kind == MemberKind::Truss).collect();
        assert!(truss.iter().all(|m| (m.a.x - m.b.x).abs() < 1e-9), "in planes across the main ridge");
        let longest = truss.iter().map(|m| m.length()).fold(0.0, f64::max);
        assert!(longest < 8.01, "a tie spans one gable: {longest}");
        // 5 over the main gable, and the planes past x = 8 also carry the second one.
        assert!(g.trusses > 5, "{}", g.trusses);
    }

    #[test]
    fn spread_is_bounded() {
        assert_eq!(spread(0.0, 1e12, 0.0, 0.1).len(), MAX_MEMBERS + 1);
    }
}
