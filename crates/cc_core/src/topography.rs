//! Terrain: a topography surface from measured points (x, y, z) inside a
//! closed boundary drawn in plan. The geometry (`cc_geom::topography`)
//! triangulates the points with the boundary, whose vertices take the
//! elevation the points give them, and closes the surface down to a flat base
//! `base_depth` below its lowest point, so sections cut the earth as a solid.
//! Points outside the boundary are kept but take no part in the surface.

use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

/// Most measured points a topography keeps.
pub const MAX_POINTS: usize = 20_000;
/// Most points of a topography's boundary.
pub const MAX_OUTLINE: usize = 4096;
/// Points closer than this in plan are one point (the first is kept).
pub const POINT_TOL: f64 = 1e-6;

fn default_base_depth() -> f64 {
    2.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Topography {
    /// Measured points; z is relative to the element's level elevation.
    pub points: Vec<DVec3>,
    /// Closed boundary in plan (either winding).
    pub boundary: Vec<DVec2>,
    /// Depth of the flat base below the lowest point of the surface.
    #[serde(default = "default_base_depth")]
    pub base_depth: f64,
    /// The curves the boundary was drawn with (see [`crate::sketch`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch: Option<crate::sketch::Sketch>,
}

impl Default for Topography {
    fn default() -> Self {
        Self { points: Vec::new(), boundary: Vec::new(), base_depth: default_base_depth(), sketch: None }
    }
}

impl Topography {
    /// A boundary of three to `MAX_OUTLINE` finite points enclosing an area.
    /// Points and the base depth are repaired instead (see [`Self::repair`]).
    pub fn is_valid(&self) -> bool {
        self.boundary.len() >= 3
            && self.boundary.len() <= MAX_OUTLINE
            && self.boundary.iter().all(|p| p.is_finite())
            && crate::mass::area(&self.boundary) > 1e-6
    }

    /// Whether the points and base depth need no repair.
    pub fn points_ok(&self) -> bool {
        self.base_depth.is_finite() && self.base_depth > 1e-6 && clean_points(&self.points).len() == self.points.len()
    }

    /// Drop non-finite points, points over `MAX_POINTS` and points that
    /// repeat another in plan, and reset a bad base depth; returns how many
    /// repairs were made.
    pub fn repair(&mut self) -> usize {
        let mut repairs = 0;
        if !(self.base_depth.is_finite() && self.base_depth > 1e-6) {
            self.base_depth = default_base_depth();
            repairs += 1;
        }
        let clean = clean_points(&self.points);
        repairs += self.points.len() - clean.len();
        self.points = clean;
        repairs
    }
}

/// The finite points of `points`, the first of those that coincide in plan
/// (within [`POINT_TOL`]), at most [`MAX_POINTS`].
pub fn clean_points(points: &[DVec3]) -> Vec<DVec3> {
    let cell = |v: f64| (v / POINT_TOL).round() as i64;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for p in points.iter().filter(|p| p.is_finite()) {
        if out.len() >= MAX_POINTS {
            break;
        }
        // Neighboring cells too, so points just across a cell edge count as one.
        let (cx, cy) = (cell(p.x), cell(p.y));
        let near = (-1..=1).any(|dx| (-1..=1).any(|dy| seen.contains(&(cx + dx, cy + dy))));
        if near && out.iter().any(|q: &DVec3| q.truncate().distance(p.truncate()) < POINT_TOL) {
            continue;
        }
        seen.insert((cx, cy));
        out.push(*p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<DVec2> {
        vec![DVec2::ZERO, DVec2::new(10.0, 0.0), DVec2::new(10.0, 10.0), DVec2::new(0.0, 10.0)]
    }

    #[test]
    fn repair_drops_bad_and_repeated_points_and_resets_the_depth() {
        let mut t = Topography {
            points: vec![
                DVec3::new(1.0, 1.0, 0.5),
                DVec3::new(f64::NAN, 1.0, 0.0),
                DVec3::new(1.0, 1.0, 9.0),
                DVec3::new(2.0, 2.0, f64::INFINITY),
                DVec3::new(3.0, 3.0, 1.0),
            ],
            boundary: square(),
            base_depth: -1.0,
            sketch: None,
        };
        assert!(t.is_valid() && !t.points_ok());
        assert_eq!(t.repair(), 4);
        assert_eq!(t.points, vec![DVec3::new(1.0, 1.0, 0.5), DVec3::new(3.0, 3.0, 1.0)]);
        assert_eq!(t.base_depth, 2.0);
        assert!(t.points_ok());
    }

    #[test]
    fn a_boundary_needs_an_area() {
        let mut t = Topography { boundary: square(), ..Default::default() };
        assert!(t.is_valid());
        t.boundary = vec![DVec2::ZERO, DVec2::X, DVec2::new(2.0, 0.0)];
        assert!(!t.is_valid());
        t.boundary = vec![DVec2::ZERO, DVec2::X, DVec2::new(f64::NAN, 1.0)];
        assert!(!t.is_valid());
    }

    #[test]
    fn points_are_bounded() {
        let points: Vec<_> = (0..MAX_POINTS + 10).map(|i| DVec3::new(i as f64, 0.0, 0.0)).collect();
        assert_eq!(clean_points(&points).len(), MAX_POINTS);
    }
}
