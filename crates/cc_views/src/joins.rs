//! Joined geometry in cut views (plans and sections): the outline of the
//! element that is cut stops where the cutting element begins, the cutting
//! element is painted over it, and when both are cut in the same material
//! the line between them goes, so they read as one piece.

use crate::{Drawing2D, LineStyle, Prim};
use cc_core::{ElementId, Project};
use glam::DVec2;
use std::collections::BTreeMap;

/// Distance under which a point is on a line, in drawing units (meters).
const EPS: f64 = 1e-6;
/// How far to each side of a line piece the region is sampled.
const SIDE: f64 = 1e-4;

/// Where a piece of line lies with respect to a cut region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    Outside,
    /// On the region's outline: inside on one side only.
    Boundary,
    /// Inside, including on an edge shared by two of the region's polygons.
    Inside,
}

fn place(region: &[Vec<DVec2>], a: DVec2, b: DVec2) -> Place {
    let inside = |p: DVec2| region.iter().any(|poly| cc_geom::polygon::contains(poly, p));
    let mid = (a + b) / 2.0;
    let n = (b - a).perp().normalize_or_zero() * SIDE;
    match (inside(mid + n), inside(mid - n)) {
        (true, true) => Place::Inside,
        (false, false) => Place::Outside,
        _ => Place::Boundary,
    }
}

/// Split `a`–`b` where it crosses or starts to run along an edge of `region`.
fn split(region: &[Vec<DVec2>], a: DVec2, b: DVec2) -> Vec<(DVec2, DVec2)> {
    let d = b - a;
    let len2 = d.length_squared();
    if len2 < EPS * EPS {
        return Vec::new();
    }
    let mut ts = vec![0.0, 1.0];
    for poly in region {
        for (i, &p) in poly.iter().enumerate() {
            let q = poly[(i + 1) % poly.len()];
            let e = q - p;
            let denom = d.perp_dot(e);
            if denom.abs() > 1e-12 {
                let t = (p - a).perp_dot(e) / denom;
                let u = (p - a).perp_dot(d) / denom;
                if (-1e-9..=1.0 + 1e-9).contains(&u) {
                    ts.push(t);
                }
            } else if d.perp_dot(p - a).abs() / len2.sqrt() < EPS {
                // Collinear: the edge's ends bound the shared stretch.
                ts.push((p - a).dot(d) / len2);
                ts.push((q - a).dot(d) / len2);
            }
        }
    }
    ts.retain(|t| t.is_finite() && (0.0..=1.0).contains(t));
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    ts.windows(2).map(|w| (a + d * w[0], a + d * w[1])).filter(|(p, q)| p.distance(*q) > EPS).collect()
}

/// Keep the pieces of `element`'s lines (of the given styles) that `keep`
/// accepts by their place against `region`.
fn trim_lines(
    d: &mut Drawing2D,
    element: ElementId,
    region: &[Vec<DVec2>],
    styles: &[LineStyle],
    keep: impl Fn(LineStyle, Place) -> bool,
) {
    let prims = std::mem::take(&mut d.prims);
    for prim in prims {
        match prim {
            Prim::Line { a, b, style, element: Some(id), pen } if id == element && styles.contains(&style) => {
                for (p, q) in split(region, a, b) {
                    if keep(style, place(region, p, q)) {
                        d.prims.push(Prim::Line { a: p, b: q, style, element: Some(id), pen });
                    }
                }
            }
            other => d.prims.push(other),
        }
    }
}

/// Merge the cut regions of joined elements drawn in `d`.
pub(crate) fn merge(project: &Project, d: &mut Drawing2D) {
    if project.joins.is_empty() {
        return;
    }
    // Cut regions are the opaque fills of each element.
    let mut regions: BTreeMap<ElementId, Vec<Vec<DVec2>>> = BTreeMap::new();
    for prim in &d.prims {
        if let Prim::Fill { pts, color, element: Some(id), .. } = prim
            && color[3] > 0
        {
            regions.entry(*id).or_default().push(pts.clone());
        }
    }
    let material = |id: ElementId| {
        let e = project.element(id)?;
        project.element_type(e.type_id).map(|t| t.data.cut_material().to_string())
    };
    for &(cutting, cut) in &project.joins {
        let (Some(by), Some(own)) = (regions.get(&cutting), regions.get(&cut)) else { continue };
        let same = material(cutting).is_some_and(|m| Some(m) == material(cut));
        // The cut element's outline, hatch and layer lines stop inside the
        // cutting one; its outline along the cutting one's goes when they merge.
        trim_lines(d, cut, by, &[LineStyle::Cut, LineStyle::Thin, LineStyle::Hatch], |style, place| match place {
            Place::Inside => false,
            Place::Boundary => !same && style == LineStyle::Cut,
            Place::Outside => true,
        });
        if same {
            trim_lines(d, cutting, own, &[LineStyle::Cut], |_, place| place != Place::Boundary);
        }
        // The cutting element is painted over the one it cuts.
        let Some(last) = d.prims.iter().rposition(|p| p.element() == Some(cut)) else { continue };
        let mut moved = Vec::new();
        let mut kept = Vec::new();
        for (i, prim) in std::mem::take(&mut d.prims).into_iter().enumerate() {
            if i < last && prim.element() == Some(cutting) { moved.push(prim) } else { kept.push(prim) }
        }
        let at = kept.iter().rposition(|p| p.element() == Some(cut)).map_or(kept.len(), |i| i + 1);
        kept.splice(at..at, moved);
        d.prims = kept;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<DVec2> {
        vec![DVec2::new(x0, y0), DVec2::new(x1, y0), DVec2::new(x1, y1), DVec2::new(x0, y1)]
    }

    #[test]
    fn lines_split_at_a_region_and_are_placed() {
        let region = vec![rect(0.0, 0.0, 2.0, 1.0)];
        // Crossing the region: outside, inside, outside.
        let pieces = split(&region, DVec2::new(-1.0, 0.5), DVec2::new(3.0, 0.5));
        let places: Vec<Place> = pieces.iter().map(|(a, b)| place(&region, *a, *b)).collect();
        assert_eq!(places, [Place::Outside, Place::Inside, Place::Outside]);
        // Running along the bottom edge, past both ends.
        let pieces = split(&region, DVec2::new(-1.0, 0.0), DVec2::new(3.0, 0.0));
        let places: Vec<Place> = pieces.iter().map(|(a, b)| place(&region, *a, *b)).collect();
        assert_eq!(places, [Place::Outside, Place::Boundary, Place::Outside]);
        // An edge shared by two polygons of one region is inside it.
        let strips = vec![rect(0.0, 0.0, 1.0, 1.0), rect(1.0, 0.0, 2.0, 1.0)];
        assert_eq!(place(&strips, DVec2::new(1.0, 0.2), DVec2::new(1.0, 0.8)), Place::Inside);
    }

    /// A slab with a beam hanging from it, cut across the beam.
    fn slab_and_beam(joined: bool) -> (Drawing2D, ElementId, ElementId, f64) {
        use cc_core::{Category, Document, Element, ElementKind, TypeData};
        let mut doc = Document::new(Project::from_template());
        let (mut floor, mut beam) = (ElementId(0), ElementId(0));
        doc.edit("slab and beam", |e| {
            let level = e.project().levels[0].id;
            let floor_t = e.project().default_type(Category::Floor).unwrap();
            let beam_t = e.project().default_type(Category::Beam).unwrap();
            // Both in the slab's cut material, so they merge.
            let concrete = e.project().element_type(floor_t).unwrap().data.cut_material().to_string();
            if let Some(TypeData::Beam { material, .. }) = e.type_mut(beam_t).map(|t| &mut t.data) {
                *material = concrete;
            }
            let element = |e: &mut cc_core::EditCtx, type_id, kind| {
                let id = ElementId(e.alloc_id());
                e.add_element(Element {
                    id,
                    level,
                    type_id,
                    mark: String::new(),
                    comments: String::new(),
                    uid: Default::default(),
                    kind,
                    phase_created: Default::default(),
                    phase_demolished: None,
                });
                id
            };
            let boundary = vec![DVec2::new(0.0, 0.0), DVec2::new(8.0, 0.0), DVec2::new(8.0, 6.0), DVec2::new(0.0, 6.0)];
            floor = element(e, floor_t, ElementKind::Floor(cc_core::Floor::new(boundary, 0.0)));
            let kind = ElementKind::Beam(cc_core::Beam::new(DVec2::new(0.0, 3.0), DVec2::new(8.0, 3.0), 0.0));
            beam = element(e, beam_t, kind);
            if joined {
                e.set_joined(beam, floor, true);
            }
        });
        let geom = cc_geom::ModelGeometry::build(&doc.project);
        let mut d = crate::projected::section(
            &doc.project,
            &geom,
            (DVec2::new(4.0, 7.0), DVec2::new(4.0, -1.0)),
            2.0,
            50,
            cc_core::DetailLevel::Medium,
        );
        merge(&doc.project, &mut d);
        let slab = geom.volume(floor) / 48.0;
        (d, floor, beam, slab)
    }

    /// Cut lines along the slab's underside over the beam's width.
    fn lines_between(d: &Drawing2D, beam: ElementId, slab: f64) -> usize {
        let xs: Vec<f64> = d
            .prims
            .iter()
            .filter_map(|p| match p {
                Prim::Fill { pts, element: Some(id), .. } if *id == beam => Some(pts.iter().map(|q| q.x)),
                _ => None,
            })
            .flatten()
            .collect();
        let (x0, x1) = xs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
        d.prims
            .iter()
            .filter(|p| match p {
                Prim::Line { a, b, style: LineStyle::Cut, .. } => {
                    let mid = (*a + *b) / 2.0;
                    (a.y + slab).abs() < 1e-6 && (b.y + slab).abs() < 1e-6 && mid.x > x0 + 1e-3 && mid.x < x1 - 1e-3
                }
                _ => false,
            })
            .count()
    }

    #[test]
    fn joined_slab_and_beam_read_as_one_piece() {
        let (d, _, beam, slab) = slab_and_beam(false);
        assert!(lines_between(&d, beam, slab) > 0, "apart, the slab's underside crosses the beam");
        let (d, floor, beam, slab) = slab_and_beam(true);
        assert_eq!(lines_between(&d, beam, slab), 0, "joined, no line between them");
        // Both are still drawn and filled.
        for id in [floor, beam] {
            assert!(d.prims.iter().any(|p| matches!(p, Prim::Fill { element: Some(e), .. } if *e == id)));
            assert!(
                d.prims
                    .iter()
                    .any(|p| matches!(p, Prim::Line { style: LineStyle::Cut, element: Some(e), .. } if *e == id))
            );
        }
    }
}
