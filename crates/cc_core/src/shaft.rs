//! Shaft openings: a vertical opening with an outline in plan, from its
//! level (plus `base_offset`) up to a level or a height, that cuts every
//! floor, ceiling and roof it passes through. The cuts are written into
//! those elements (`cuts`) by every edit (see [`crate::EditCtx`]), so plans,
//! schedules, 3D views and IFC export see the holes like any opening.
//!
//! A shaft cuts an element only where its outline lies inside the element's
//! outline, clear of its own openings.

use crate::{Element, ElementKind, Project, WallTop};
use glam::DVec2;
use serde::{Deserialize, Serialize};

/// Most points of a shaft's outline.
pub const MAX_OUTLINE: usize = 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shaft {
    /// Closed outline in plan (either winding).
    pub outline: Vec<DVec2>,
    /// Bottom of the shaft above its level.
    pub base_offset: f64,
    /// Height when `top` is unconnected.
    pub height: f64,
    pub top: WallTop,
    /// The curves the outline was drawn with (see [`crate::sketch`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch: Option<crate::sketch::Sketch>,
}

impl Shaft {
    pub fn is_valid(&self) -> bool {
        self.outline.len() >= 3
            && self.outline.len() <= MAX_OUTLINE
            && self.outline.iter().all(|p| p.is_finite())
            && self.base_offset.is_finite()
            && self.height.is_finite()
            && area(&self.outline) > 1e-6
    }
}

fn area(p: &[DVec2]) -> f64 {
    let n = p.len();
    (0..n).map(|i| p[i].perp_dot(p[(i + 1) % n])).sum::<f64>().abs() * 0.5
}

/// Whether `p` lies inside polygon `poly` (even-odd rule).
fn inside(poly: &[DVec2], p: DVec2) -> bool {
    let n = poly.len();
    let mut c = false;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            c = !c;
        }
    }
    c
}

fn edges(poly: &[DVec2]) -> impl Iterator<Item = (DVec2, DVec2)> + '_ {
    let n = poly.len();
    (0..n).map(move |i| (poly[i], poly[(i + 1) % n]))
}

/// Whether segments `ab` and `cd` touch or cross.
fn meet(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let side = |p: DVec2, q: DVec2, r: DVec2| (q - p).perp_dot(r - p);
    let (d1, d2, d3, d4) = (side(c, d, a), side(c, d, b), side(a, b, c), side(a, b, d));
    let eps = 1e-9;
    if ((d1 > eps && d2 < -eps) || (d1 < -eps && d2 > eps)) && ((d3 > eps && d4 < -eps) || (d3 < -eps && d4 > eps)) {
        return true;
    }
    let on = |p: DVec2, q: DVec2, r: DVec2| {
        side(p, q, r).abs() <= eps
            && r.x >= p.x.min(q.x) - eps
            && r.x <= p.x.max(q.x) + eps
            && r.y >= p.y.min(q.y) - eps
            && r.y <= p.y.max(q.y) + eps
    };
    on(c, d, a) || on(c, d, b) || on(a, b, c) || on(a, b, d)
}

/// Whether `outline` lies inside `boundary` and clear of each of `holes`
/// (no shared edges or points either): a hole it can cut.
pub fn fits(outline: &[DVec2], boundary: &[DVec2], holes: &[Vec<DVec2>]) -> bool {
    if outline.len() < 3 || boundary.len() < 3 {
        return false;
    }
    let crosses = |other: &[DVec2]| edges(outline).any(|(a, b)| edges(other).any(|(c, d)| meet(a, b, c, d)));
    if !outline.iter().all(|p| inside(boundary, *p)) || crosses(boundary) {
        return false;
    }
    holes.iter().all(|h| {
        h.len() < 3 || (!crosses(h) && !outline.iter().any(|p| inside(h, *p)) && !h.iter().any(|p| inside(outline, *p)))
    })
}

impl Project {
    /// Bottom and top elevation of a shaft (the top at least 1 cm above).
    pub fn shaft_span(&self, element: &Element, shaft: &Shaft) -> (f64, f64) {
        let base = self.level_elevation(element.level) + shaft.base_offset;
        let top = match shaft.top {
            WallTop::Unconnected => base + shaft.height,
            WallTop::Level { level, offset } => self.level_elevation(level) + offset,
        };
        (base, top.max(base + 0.01))
    }

    /// Bottom and top elevation of a floor, ceiling or roof (a roof's top
    /// left open: a shaft cuts it when it rises past its base).
    fn slab_span(&self, element: &Element) -> Option<(f64, f64)> {
        let level = self.level_elevation(element.level);
        let thk = self.element_type(element.type_id).map_or(0.0, |t| t.thickness());
        match &element.kind {
            ElementKind::Floor(f) => Some((level + f.height_offset - thk, level + f.height_offset)),
            ElementKind::Ceiling(c) => Some((level + c.height_offset, level + c.height_offset + thk)),
            ElementKind::Roof(r) => Some((level + r.base_offset, f64::INFINITY)),
            _ => None,
        }
    }

    /// The outlines of the shafts that cut `element` (a floor, ceiling or
    /// roof): those that pass through its thickness and lie inside its
    /// outline, clear of its own openings and of one another.
    pub fn shaft_cuts(&self, element: &Element) -> Vec<Vec<DVec2>> {
        let Some((bottom, top)) = self.slab_span(element) else { return Vec::new() };
        let (boundary, own): (&[DVec2], &[Vec<DVec2>]) = match &element.kind {
            ElementKind::Floor(f) => (&f.boundary, &f.openings),
            ElementKind::Ceiling(c) => (&c.boundary, &[]),
            ElementKind::Roof(r) => (&r.footprint, &r.holes),
            _ => return Vec::new(),
        };
        let mut cuts: Vec<Vec<DVec2>> = Vec::new();
        for e in self.elements.values() {
            let ElementKind::Shaft(s) = &e.kind else { continue };
            if !s.is_valid() || self.phase_state_conflict(element, e) {
                continue;
            }
            let (low, high) = self.shaft_span(e, s);
            if high <= bottom + 1e-6 || low >= top - 1e-6 {
                continue;
            }
            let mut blocking: Vec<Vec<DVec2>> = own.to_vec();
            blocking.extend(cuts.iter().cloned());
            if fits(&s.outline, boundary, &blocking) {
                cuts.push(s.outline.clone());
            }
        }
        cuts
    }

    /// A shaft demolished before an element was built, or built after it
    /// was demolished, does not cut it.
    fn phase_state_conflict(&self, host: &Element, shaft: &Element) -> bool {
        let order = |id: crate::PhaseId| self.phases.iter().position(|p| p.id == id);
        let built = |e: &Element| order(e.phase_created);
        let gone = |e: &Element| e.phase_demolished.and_then(order);
        matches!((gone(shaft), built(host)), (Some(g), Some(b)) if g < b)
            || matches!((built(shaft), gone(host)), (Some(b), Some(g)) if b > g)
    }
}

impl Element {
    /// The shaft cuts kept in a floor, ceiling or roof.
    pub fn shaft_cuts_mut(&mut self) -> Option<&mut Vec<Vec<DVec2>>> {
        match &mut self.kind {
            ElementKind::Floor(f) => Some(&mut f.cuts),
            ElementKind::Ceiling(c) => Some(&mut c.cuts),
            ElementKind::Roof(r) => Some(&mut r.cuts),
            _ => None,
        }
    }

    /// The shaft cuts of a floor, ceiling or roof.
    pub fn shaft_cuts(&self) -> &[Vec<DVec2>] {
        match &self.kind {
            ElementKind::Floor(f) => &f.cuts,
            ElementKind::Ceiling(c) => &c.cuts,
            ElementKind::Roof(r) => &r.cuts,
            _ => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, s: f64) -> Vec<DVec2> {
        vec![DVec2::new(x, y), DVec2::new(x + s, y), DVec2::new(x + s, y + s), DVec2::new(x, y + s)]
    }

    #[test]
    fn a_shaft_fits_inside_and_clear_of_openings() {
        let slab = square(0.0, 0.0, 10.0);
        assert!(fits(&square(2.0, 2.0, 1.0), &slab, &[]));
        // Across the edge, or touching it: no.
        assert!(!fits(&square(9.5, 2.0, 1.0), &slab, &[]));
        assert!(!fits(&square(9.0, 2.0, 1.0), &slab, &[]));
        // Overlapping, around or inside an opening: no; beside it: yes.
        let hole = vec![square(2.5, 2.5, 1.0)];
        assert!(!fits(&square(2.0, 2.0, 1.0), &slab, &hole));
        assert!(!fits(&square(2.0, 2.0, 3.0), &slab, &hole));
        assert!(!fits(&square(2.7, 2.7, 0.2), &slab, &hole));
        assert!(fits(&square(5.0, 5.0, 1.0), &slab, &hole));
    }
}
