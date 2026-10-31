//! Conceptual masses: volumes for massing studies. A mass is a base outline
//! in plan extruded `height` up from its level (plus `base_offset`), or
//! lofted to a different top outline. Mass floors cut it at the levels it
//! lists: each gives the floor outline at that level, with its area and
//! perimeter, for gross floor areas before any wall or floor is drawn.

use crate::LevelId;
use glam::DVec2;
use serde::{Deserialize, Serialize};

/// Largest outline a mass keeps (points).
pub const MAX_OUTLINE: usize = 4096;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mass {
    /// Base outline in plan (closed, either winding).
    pub outline: Vec<DVec2>,
    /// Offset of the base from the level elevation.
    #[serde(default)]
    pub base_offset: f64,
    /// Height of the mass above its base.
    pub height: f64,
    /// Top outline of a loft; `None` extrudes the base outline.
    #[serde(default)]
    pub top: Option<Vec<DVec2>>,
    /// Levels where mass floors are taken (those the mass spans).
    #[serde(default)]
    pub floors: Vec<LevelId>,
    /// The curves the outline was drawn with (see [`crate::sketch`]); dropped
    /// when the outline is edited without them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch: Option<crate::sketch::Sketch>,
}

impl Mass {
    /// Outlines of at least three finite points (bounded), a finite positive
    /// height and a finite base offset.
    pub fn is_valid(&self) -> bool {
        let outline_ok = |o: &[DVec2]| o.len() >= 3 && o.len() <= MAX_OUTLINE && o.iter().all(|p| p.is_finite());
        outline_ok(&self.outline)
            && self.top.as_deref().is_none_or(outline_ok)
            && self.height.is_finite()
            && self.height > 1e-6
            && self.base_offset.is_finite()
    }

    /// The outline at `t` of the height (0 at the base, 1 at the top),
    /// counter-clockwise: the base outline of an extrusion, or the loft's
    /// section between its outlines.
    pub fn section(&self, t: f64) -> Vec<DVec2> {
        let base = crate::sweep::counter_clockwise(&self.outline);
        match &self.top {
            None => base,
            Some(top) => crate::sweep::blend_section(&base, &crate::sweep::counter_clockwise(top), t.clamp(0.0, 1.0)),
        }
    }
}

/// A mass floor: the outline of a mass at a level, its area and perimeter.
#[derive(Debug, Clone, PartialEq)]
pub struct MassFloor {
    pub level: LevelId,
    pub outline: Vec<DVec2>,
    pub area: f64,
    pub perimeter: f64,
}

/// Area of a closed polygon (either winding).
pub fn area(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| poly[i].perp_dot(poly[(i + 1) % n])).sum::<f64>().abs() * 0.5
}

/// Perimeter of a closed polygon.
pub fn perimeter(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| poly[i].distance(poly[(i + 1) % n])).sum()
}

/// A face of a mass, for push/pull.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MassFace {
    Top,
    Bottom,
    /// The side over edge `i` of the base outline (from point `i` to the next).
    Side(usize),
}

impl Mass {
    /// The mass with `face` moved `d` outward (inward when negative): the top
    /// changes the height, the bottom the base offset and the height, and a
    /// side moves its edge of the outline, the edges beside it keeping their
    /// directions. Sides of a loft are not moved. `None` when the result is
    /// not a valid mass (no height left, a side pushed through the others).
    pub fn pushed(&self, face: MassFace, d: f64) -> Option<Mass> {
        if !d.is_finite() {
            return None;
        }
        let mut m = self.clone();
        match face {
            MassFace::Top => m.height += d,
            MassFace::Bottom => {
                m.base_offset -= d;
                m.height += d;
            }
            MassFace::Side(i) => {
                if m.top.is_some() {
                    return None;
                }
                m.outline = push_edge(&m.outline, i, d)?;
            }
        }
        (m.is_valid() && m.height >= 0.01).then_some(m)
    }
}

/// Where the lines through `p` along `d1` and through `q` along `d2` meet.
fn meet(p: DVec2, d1: DVec2, q: DVec2, d2: DVec2) -> Option<DVec2> {
    let den = d1.perp_dot(d2);
    (den.abs() > 1e-9).then(|| p + d1 * ((q - p).perp_dot(d2) / den))
}

fn signed_area(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| poly[i].perp_dot(poly[(i + 1) % n])).sum::<f64>() * 0.5
}

/// `outline` with edge `i` (from point `i` to the next) moved `d` outward,
/// the edges beside it keeping their directions (they stretch or shrink).
/// `None` for a bad edge or when the outline would fold over.
pub fn push_edge(outline: &[DVec2], i: usize, d: f64) -> Option<Vec<DVec2>> {
    let n = outline.len();
    if n < 3 || i >= n || !d.is_finite() {
        return None;
    }
    let at = |k: usize| outline.get(k % n).copied();
    let (prev, a, b, next) = (at(i + n - 1)?, at(i)?, at(i + 1)?, at(i + 2)?);
    let before = signed_area(outline);
    let u = (b - a).try_normalize()?;
    // Outward is to the right of a counter-clockwise outline.
    let out = if before > 0.0 { DVec2::new(u.y, -u.x) } else { DVec2::new(-u.y, u.x) };
    let shift = out * d;
    let na = meet(a + shift, u, prev, a - prev).unwrap_or(a + shift);
    let nb = meet(b + shift, u, b, next - b).unwrap_or(b + shift);
    let mut moved = outline.to_vec();
    *moved.get_mut(i)? = na;
    *moved.get_mut((i + 1) % n)? = nb;
    let after = signed_area(&moved);
    (after * before > 0.0 && after.abs() > 1e-6 && moved.iter().all(|p| p.is_finite())).then_some(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: f64, d: f64) -> Vec<DVec2> {
        vec![DVec2::ZERO, DVec2::new(w, 0.0), DVec2::new(w, d), DVec2::new(0.0, d)]
    }

    #[test]
    fn sections_of_extrusions_and_lofts() {
        let mut m = Mass {
            outline: rect(10.0, 6.0),
            base_offset: 0.0,
            height: 9.0,
            top: None,
            floors: Vec::new(),
            sketch: None,
        };
        assert!(m.is_valid());
        assert!((area(&m.section(0.5)) - 60.0).abs() < 1e-9);
        // A loft from 10 x 6 to 4 x 6: halfway the section is 7 x 6.
        m.top = Some(rect(4.0, 6.0).into_iter().map(|p| p + DVec2::new(3.0, 0.0)).collect());
        assert!((area(&m.section(0.5)) - 42.0).abs() < 1e-6, "{}", area(&m.section(0.5)));
        assert!((area(&m.section(1.0)) - 24.0).abs() < 1e-6);
        assert!((perimeter(&rect(10.0, 6.0)) - 32.0).abs() < 1e-12);
        // Bad data is invalid.
        m.height = 0.0;
        assert!(!m.is_valid());
        m.height = 3.0;
        m.outline = vec![DVec2::ZERO, DVec2::X];
        assert!(!m.is_valid());
        m.outline = rect(1.0, 1.0);
        m.top = Some(vec![DVec2::new(f64::NAN, 0.0); 3]);
        assert!(!m.is_valid());
    }

    #[test]
    fn faces_push_and_pull() {
        let m = Mass {
            outline: rect(10.0, 6.0),
            base_offset: 0.0,
            height: 9.0,
            top: None,
            floors: Vec::new(),
            sketch: None,
        };
        assert_eq!(m.pushed(MassFace::Top, 3.0).map(|m| m.height), Some(12.0));
        let b = m.pushed(MassFace::Bottom, 1.0).unwrap();
        assert_eq!((b.base_offset, b.height), (-1.0, 10.0));
        // The east side (edge 1, from (10,0) to (10,6)) out by 2: 12 x 6.
        let e = m.pushed(MassFace::Side(1), 2.0).unwrap();
        assert!((area(&e.outline) - 72.0).abs() < 1e-9 && e.outline[1] == DVec2::new(12.0, 0.0));
        // The north side in by 4, on a clockwise outline: 10 x 2.
        let cw = Mass { outline: m.outline.iter().rev().copied().collect(), ..m.clone() };
        let w = cw.pushed(MassFace::Side(0), -4.0).unwrap();
        assert!((area(&w.outline) - 20.0).abs() < 1e-9, "{:?}", w.outline);
        // A trapezoid's slanted neighbors keep their directions.
        let trap = vec![DVec2::ZERO, DVec2::new(10.0, 0.0), DVec2::new(8.0, 4.0), DVec2::new(2.0, 4.0)];
        let up = push_edge(&trap, 2, 1.0).unwrap();
        assert!((up[2] - DVec2::new(7.5, 5.0)).length() < 1e-9 && (up[3] - DVec2::new(2.5, 5.0)).length() < 1e-9);
        // Too far in: nothing left, or folded; and lofts keep their sides.
        assert!(m.pushed(MassFace::Top, -9.0).is_none());
        assert!(m.pushed(MassFace::Side(1), -10.0).is_none());
        assert!(m.pushed(MassFace::Side(9), 1.0).is_none());
        let loft = Mass { top: Some(rect(4.0, 4.0)), ..m };
        assert!(loft.pushed(MassFace::Side(0), 1.0).is_none());
        assert!(loft.pushed(MassFace::Top, 1.0).is_some());
    }
}
