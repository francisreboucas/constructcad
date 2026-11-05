//! Structural framing joins: a beam end inside a structural column stops at
//! the column's face, and beam ends meet other beams the way walls do (a
//! mitered L corner, a butt T against the side of a beam, a collinear pair
//! running through a junction of three or more).

use crate::polygon;
use crate::wall::{EndJoin, WallLine, resolve_join};
use cc_core::ElementId;
use glam::DVec2;

/// How close to a column outline a beam end must lie to frame into it.
const COLUMN_TOL: f64 = 0.01;

/// What a beam end is framed into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeamEnd {
    /// Free: the end is square at its location point.
    Free,
    /// Cut back to the face of a structural column.
    Column(ElementId),
    /// Mitered or butted against other beams.
    Beam,
}

/// Resolved beam geometry: frame, section and the ends after joins.
#[derive(Debug, Clone, PartialEq)]
pub struct BeamGeom {
    pub element: ElementId,
    pub origin: DVec2,
    pub dir: DVec2,
    /// Left normal (+v).
    pub normal: DVec2,
    /// Length between the location points.
    pub length: f64,
    pub half: f64,
    /// Bottom and top elevations at the start location point (`u = 0`).
    pub z_range: (f64, f64),
    /// Rise per meter along the axis (sloped beams).
    pub slope: f64,
    /// Solid parts of the section as `(min, max)` rectangles: `x` across the
    /// beam (`v`), `y` up from the bottom of the section, square to the axis.
    pub section: Vec<(DVec2, DVec2)>,
    /// Axial extents `(left, right)` of the start/end faces after joins.
    pub start_ext: (f64, f64),
    pub end_ext: (f64, f64),
    pub start: BeamEnd,
    pub end: BeamEnd,
}

impl BeamGeom {
    /// World plan position of beam-local `(u, v)`.
    pub fn world(&self, u: f64, v: f64) -> DVec2 {
        self.origin + self.dir * u + self.normal * v
    }

    /// The footprint corners in beam-local `(u, v)`, counter-clockwise.
    pub fn local_footprint(&self) -> [DVec2; 4] {
        let h = self.half;
        [
            DVec2::new(self.start_ext.1, -h),
            DVec2::new(self.end_ext.1, -h),
            DVec2::new(self.end_ext.0, h),
            DVec2::new(self.start_ext.0, h),
        ]
    }

    /// Plan footprint of the body, counter-clockwise.
    pub fn footprint(&self) -> [DVec2; 4] {
        self.local_footprint().map(|p| self.world(p.x, p.y))
    }

    /// Length of the body along its (sloped) axis, the mean of its two sides.
    pub fn cut_length(&self) -> f64 {
        ((self.end_ext.0 - self.start_ext.0) + (self.end_ext.1 - self.start_ext.1)) * 0.5 * self.stretch()
    }

    /// Length along the slope per meter in plan.
    pub fn stretch(&self) -> f64 {
        self.slope.hypot(1.0)
    }

    /// Bottom and top elevations at `u` along the axis.
    pub fn z_at(&self, u: f64) -> (f64, f64) {
        (self.z_range.0 + self.slope * u, self.z_range.1 + self.slope * u)
    }

    /// Lowest and highest elevations of the body.
    pub fn z_bounds(&self) -> (f64, f64) {
        let lo = self.start_ext.0.min(self.start_ext.1);
        let hi = self.end_ext.0.max(self.end_ext.1);
        let (a, b) = (self.z_at(lo), self.z_at(hi));
        (a.0.min(b.0), a.1.max(b.1))
    }

    /// Where the start (`at_end` false) or end face crosses the line `v`.
    fn face_u(&self, at_end: bool, v: f64) -> f64 {
        let (left, right) = if at_end { self.end_ext } else { self.start_ext };
        let t = if self.half > 0.0 { (v + self.half) / (2.0 * self.half) } else { 0.5 };
        right + (left - right) * t
    }

    /// Each solid part of the section: its plan footprint (counter-clockwise)
    /// and its bottom and top heights above the section's bottom, measured
    /// vertically.
    pub fn parts(&self) -> Vec<([DVec2; 4], (f64, f64))> {
        let k = self.stretch();
        self.section
            .iter()
            .map(|(a, b)| {
                let corners = [
                    (self.face_u(false, a.x), a.x),
                    (self.face_u(true, a.x), a.x),
                    (self.face_u(true, b.x), b.x),
                    (self.face_u(false, b.x), b.x),
                ];
                (corners.map(|(u, v)| self.world(u, v)), (a.y * k, b.y * k))
            })
            .collect()
    }

    /// Add the body to `s`: one prism per part, following the slope.
    pub fn add_to(&self, s: &mut crate::Solid) {
        let (origin, dir, slope, base) = (self.origin, self.dir, self.slope, self.z_range.0);
        let z = move |p: DVec2| base + slope * (p - origin).dot(dir);
        for (footprint, (z0, z1)) in self.parts() {
            s.add_prism(&footprint, |p| z(p) + z0, |p| z(p) + z1);
        }
    }

    /// Whether both end faces are square to the axis.
    pub fn is_square(&self) -> bool {
        (self.start_ext.0 - self.start_ext.1).abs() < 1e-9 && (self.end_ext.0 - self.end_ext.1).abs() < 1e-9
    }

    pub fn volume(&self) -> f64 {
        self.parts().iter().map(|(f, (z0, z1))| polygon::area(f) * (z1 - z0)).sum()
    }
}

/// A beam as seen by the join resolution.
#[derive(Debug, Clone)]
pub(crate) struct BeamInput {
    pub element: ElementId,
    pub start: DVec2,
    pub end: DVec2,
    pub half: f64,
    /// Bottom and top elevations at the start.
    pub z_range: (f64, f64),
    pub slope: f64,
    pub section: Vec<(DVec2, DVec2)>,
    pub join_start: bool,
    pub join_end: bool,
}

impl BeamInput {
    /// Lowest and highest elevations along the whole beam.
    fn envelope(&self) -> (f64, f64) {
        let rise = self.slope * self.start.distance(self.end);
        (self.z_range.0 + rise.min(0.0), self.z_range.1 + rise.max(0.0))
    }
}

/// A structural column as seen by the join resolution.
#[derive(Debug, Clone)]
pub(crate) struct ColumnInput {
    pub element: ElementId,
    /// Plan outline (convex).
    pub outline: Vec<DVec2>,
    pub z_range: (f64, f64),
}

fn overlap(a: (f64, f64), b: (f64, f64)) -> bool {
    a.0 < b.1 - 1e-6 && b.0 < a.1 - 1e-6
}

/// Distance from `p` to the boundary of `poly`.
fn distance_to_outline(poly: &[DVec2], p: DVec2) -> f64 {
    (0..poly.len())
        .map(|i| {
            let (q, _) = polygon::closest_on_segment(poly[i], poly[(i + 1) % poly.len()], p);
            q.distance(p)
        })
        .fold(f64::INFINITY, f64::min)
}

/// Where the line through `q` along `d` leaves `outline` (the largest exit).
fn exit(outline: &[DVec2], q: DVec2, d: DVec2) -> Option<f64> {
    polygon::clip_line(outline, q, d).into_iter().map(|(_, t1)| t1).reduce(f64::max)
}

/// The end of `beam` at `p` (ray `da` into the beam) framed into a structural
/// column whose outline holds it: each side of the beam stops where it leaves
/// the column.
fn column_join(beam: &BeamInput, columns: &[ColumnInput], p: DVec2, da: DVec2) -> Option<(ElementId, EndJoin)> {
    let length = beam.start.distance(beam.end);
    let na = da.perp();
    columns
        .iter()
        .filter(|c| c.outline.len() >= 3 && overlap(c.z_range, beam.envelope()))
        .filter(|c| polygon::contains(&c.outline, p) || distance_to_outline(&c.outline, p) < COLUMN_TOL)
        .find_map(|c| {
            let center = exit(&c.outline, p, da)?;
            let side = |off: f64| exit(&c.outline, p + na * off, da).unwrap_or(center);
            let (left, right) = (side(beam.half), side(-beam.half));
            // A beam that never leaves the column keeps its whole length.
            (left.max(right) < length - 1e-3)
                .then_some((c.element, EndJoin { ray_left: left.max(0.0), ray_right: right.max(0.0), host: None }))
        })
}

/// Resolve the joins of every beam: column faces first, then beam to beam
/// among beams whose depths overlap.
pub(crate) fn resolve_beams(beams: &[BeamInput], columns: &[ColumnInput]) -> Vec<BeamGeom> {
    let ends = |b: &BeamInput| {
        let dir = (b.end - b.start).try_normalize().unwrap_or(DVec2::X);
        [(true, b.join_start, b.start, dir), (false, b.join_end, b.end, -dir)]
    };
    // Ends framed into columns; they take no part in beam joins.
    let framed: Vec<[Option<(ElementId, EndJoin)>; 2]> = beams
        .iter()
        .map(|b| ends(b).map(|(_, allowed, p, da)| allowed.then(|| column_join(b, columns, p, da)).flatten()))
        .collect();
    let lines: Vec<WallLine> = beams
        .iter()
        .zip(&framed)
        .map(|(b, f)| WallLine {
            start: b.start,
            end: b.end,
            half: b.half,
            join_start: b.join_start && f[0].is_none(),
            join_end: b.join_end && f[1].is_none(),
            bulge: 0.0,
            drawn: [b.start, b.end],
        })
        .collect();
    beams
        .iter()
        .enumerate()
        .map(|(i, b)| {
            // The beams this one can meet: those at a common depth.
            let near: Vec<usize> =
                (0..beams.len()).filter(|&j| j == i || overlap(beams[j].envelope(), b.envelope())).collect();
            let local: Vec<WallLine> = near.iter().map(|&j| lines[j]).collect();
            let me = near.iter().position(|&j| j == i).unwrap_or(0);
            let resolve = |k: usize, at_start: bool| match framed[i][k] {
                Some((column, join)) => (BeamEnd::Column(column), Some(join)),
                None => match resolve_join(&local, me, at_start) {
                    Some(join) => (BeamEnd::Beam, Some(join)),
                    None => (BeamEnd::Free, None),
                },
            };
            let (start, start_join) = resolve(0, true);
            let (end, end_join) = resolve(1, false);
            beam_geom(b, (start, start_join), (end, end_join))
        })
        .collect()
}

fn beam_geom(b: &BeamInput, start: (BeamEnd, Option<EndJoin>), end: (BeamEnd, Option<EndJoin>)) -> BeamGeom {
    let dir = (b.end - b.start).try_normalize().unwrap_or(DVec2::X);
    let length = b.start.distance(b.end);
    let mut g = BeamGeom {
        element: b.element,
        origin: b.start,
        dir,
        normal: dir.perp(),
        length,
        half: b.half,
        z_range: b.z_range,
        slope: b.slope,
        section: b.section.clone(),
        start_ext: (0.0, 0.0),
        end_ext: (length, length),
        start: BeamEnd::Free,
        end: BeamEnd::Free,
    };
    // At the start the ray runs along +u and ray-left is the beam's left;
    // at the end the ray runs along -u and ray-left is the beam's right.
    let start_ext = start.1.map_or((0.0, 0.0), |j| (j.ray_left, j.ray_right));
    let end_ext = end.1.map_or((length, length), |j| (length - j.ray_right, length - j.ray_left));
    // Joins that would turn the body inside out leave it whole.
    if end_ext.0 - start_ext.0 > 1e-4 && end_ext.1 - start_ext.1 > 1e-4 {
        (g.start_ext, g.end_ext, g.start, g.end) = (start_ext, end_ext, start.0, end.0);
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beam(id: u64, a: (f64, f64), b: (f64, f64)) -> BeamInput {
        BeamInput {
            element: ElementId(id),
            start: DVec2::new(a.0, a.1),
            end: DVec2::new(b.0, b.1),
            half: 0.1,
            z_range: (2.6, 3.0),
            slope: 0.0,
            section: vec![(DVec2::new(-0.1, 0.0), DVec2::new(0.1, 0.4))],
            join_start: true,
            join_end: true,
        }
    }

    /// A 0.4 m square column centered at `c`, 0 to 3 m.
    fn column(id: u64, c: (f64, f64)) -> ColumnInput {
        let c = DVec2::new(c.0, c.1);
        let outline = [(-0.2, -0.2), (0.2, -0.2), (0.2, 0.2), (-0.2, 0.2)].map(|(x, y)| c + DVec2::new(x, y)).to_vec();
        ColumnInput { element: ElementId(id), outline, z_range: (0.0, 3.0) }
    }

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn beam_between_columns_stops_at_their_faces() {
        let g =
            &resolve_beams(&[beam(1, (0.0, 0.0), (5.0, 0.0))], &[column(10, (0.0, 0.0)), column(11, (5.0, 0.0))])[0];
        assert!(close(g.start_ext, (0.2, 0.2)) && close(g.end_ext, (4.8, 4.8)), "{g:?}");
        assert_eq!((g.start, g.end), (BeamEnd::Column(ElementId(10)), BeamEnd::Column(ElementId(11))));
        assert!((g.cut_length() - 4.6).abs() < 1e-9);
        assert!((g.volume() - 4.6 * 0.2 * 0.4).abs() < 1e-9);
    }

    #[test]
    fn a_rotated_column_cuts_the_beam_along_its_face() {
        let mut c = column(10, (0.0, 0.0));
        let a = 10f64.to_radians();
        let rot = DVec2::from_angle(a);
        c.outline = c.outline.iter().map(|p| rot.rotate(*p)).collect();
        let g = &resolve_beams(&[beam(1, (0.0, 0.0), (5.0, 0.0))], &[c])[0];
        // The face x·cos a + y·sin a = 0.2 crosses y = ±0.1 at different x.
        let x = |y: f64| (0.2 - y * a.sin()) / a.cos();
        assert!(close(g.start_ext, (x(0.1), x(-0.1))), "{:?}", g.start_ext);
    }

    #[test]
    fn columns_frame_beams_only_where_they_overlap_in_height() {
        let mut c = column(10, (0.0, 0.0));
        c.z_range = (0.0, 2.6); // The beam sits on top of the column.
        let g = &resolve_beams(&[beam(1, (0.0, 0.0), (5.0, 0.0))], &[c])[0];
        assert_eq!(g.start, BeamEnd::Free);
        assert!(close(g.start_ext, (0.0, 0.0)));
    }

    #[test]
    fn beams_meeting_at_a_corner_are_mitered() {
        let beams = [beam(1, (0.0, 0.0), (4.0, 0.0)), beam(2, (4.0, 0.0), (4.0, 3.0))];
        let g = resolve_beams(&beams, &[]);
        assert_eq!(g[0].end, BeamEnd::Beam);
        assert!(close(g[0].end_ext, (3.9, 4.1)), "{:?}", g[0].end_ext);
        assert!(close(g[1].start_ext, (0.1, -0.1)), "{:?}", g[1].start_ext);
        // The two bodies tile the corner: 4.0 + 3.0 m of axis at 0.2 m wide.
        let area: f64 = g.iter().map(|b| polygon::area(&b.footprint())).sum();
        assert!((area - 7.0 * 0.2).abs() < 1e-9, "{area}");
    }

    #[test]
    fn a_beam_ending_on_another_stops_at_its_side() {
        let beams = [beam(1, (0.0, 0.0), (6.0, 0.0)), beam(2, (3.0, 0.0), (3.0, 4.0))];
        let g = resolve_beams(&beams, &[]);
        assert!(close(g[1].start_ext, (0.1, 0.1)), "{:?}", g[1].start_ext);
        assert_eq!((g[0].start, g[0].end), (BeamEnd::Free, BeamEnd::Free));
    }

    #[test]
    fn beams_at_different_depths_do_not_join() {
        let mut beams = [beam(1, (0.0, 0.0), (4.0, 0.0)), beam(2, (4.0, 0.0), (4.0, 3.0))];
        beams[1].z_range = (1.0, 1.4);
        let g = resolve_beams(&beams, &[]);
        assert_eq!((g[0].end, g[1].start), (BeamEnd::Free, BeamEnd::Free));
    }

    #[test]
    fn beams_meeting_at_a_column_frame_into_it_instead_of_each_other() {
        let beams = [beam(1, (0.0, 0.0), (4.0, 0.0)), beam(2, (4.0, 0.0), (4.0, 3.0))];
        let g = resolve_beams(&beams, &[column(10, (4.0, 0.0))]);
        assert!(close(g[0].end_ext, (3.8, 3.8)) && close(g[1].start_ext, (0.2, 0.2)), "{g:?}");
    }

    #[test]
    fn disallowed_ends_stay_square() {
        let mut beams = [beam(1, (0.0, 0.0), (4.0, 0.0)), beam(2, (4.0, 0.0), (4.0, 3.0))];
        beams[0].join_start = false;
        beams[0].join_end = false;
        let g = resolve_beams(&beams, &[column(10, (0.0, 0.0))]);
        assert!(close(g[0].start_ext, (0.0, 0.0)) && close(g[0].end_ext, (4.0, 4.0)));
        // The other beam no longer miters against it.
        assert_eq!(g[1].start, BeamEnd::Free);
    }

    #[test]
    fn a_beam_inside_a_column_keeps_its_length() {
        let g = &resolve_beams(&[beam(1, (-0.1, 0.0), (0.1, 0.0))], &[column(10, (0.0, 0.0))])[0];
        assert!(close(g.start_ext, (0.0, 0.0)) && close(g.end_ext, (0.2, 0.2)), "{g:?}");
    }
}
