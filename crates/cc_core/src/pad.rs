//! Building pads: a flat area at an elevation, graded into the terrain that
//! hosts it, with cut and fill slopes running out from its edges to the
//! existing ground. The terrain's surface becomes the graded one
//! (`cc_geom::pad`); the pad itself has no solid.
//!
//! A pad lives on its terrain's level (kept equal by every edit, see
//! [`crate::EditCtx`]), is deleted with it and moves with it.

use crate::{ElementId, ElementKind, Project};
use glam::DVec2;
use serde::{Deserialize, Serialize};

/// Most points of a pad's outline.
pub const MAX_OUTLINE: usize = 1024;
/// Gentlest and steepest slope ratios (horizontal run per unit of rise).
pub const MIN_SLOPE: f64 = 0.1;
pub const MAX_SLOPE: f64 = 100.0;
/// Ratios of a new pad type's cut and fill slopes.
pub const DEFAULT_CUT_SLOPE: f64 = 1.0;
pub const DEFAULT_FILL_SLOPE: f64 = 1.5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pad {
    /// The terrain it grades.
    pub host: ElementId,
    /// Closed outline in plan (either winding).
    pub outline: Vec<DVec2>,
    /// Elevation of the pad above the terrain's level.
    pub elevation: f64,
    /// Horizontal run per unit of rise of the cut slope; `None` uses the type's.
    #[serde(default)]
    pub cut_slope: Option<f64>,
    /// Horizontal run per unit of rise of the fill slope; `None` uses the type's.
    #[serde(default)]
    pub fill_slope: Option<f64>,
    /// The curves the outline was drawn with (see [`crate::sketch`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch: Option<crate::sketch::Sketch>,
}

/// Whether `r` is a slope ratio a pad keeps.
pub fn slope_ok(r: f64) -> bool {
    r.is_finite() && (MIN_SLOPE..=MAX_SLOPE).contains(&r)
}

/// `r` brought into the slope range (`default` when not a number).
pub fn clamp_slope(r: f64, default: f64) -> f64 {
    if r.is_finite() { r.clamp(MIN_SLOPE, MAX_SLOPE) } else { default }
}

impl Pad {
    /// An outline of three to `MAX_OUTLINE` finite points enclosing an area,
    /// and a finite elevation. Bad slope overrides are repaired instead.
    pub fn is_valid(&self) -> bool {
        self.outline.len() >= 3
            && self.outline.len() <= MAX_OUTLINE
            && self.outline.iter().all(|p| p.is_finite())
            && crate::mass::area(&self.outline) > 1e-6
            && self.elevation.is_finite()
    }

    /// Drop slope overrides out of range; returns how many were dropped.
    pub fn repair(&mut self) -> usize {
        let mut repairs = 0;
        for s in [&mut self.cut_slope, &mut self.fill_slope] {
            if s.is_some_and(|r| !slope_ok(r)) {
                *s = None;
                repairs += 1;
            }
        }
        repairs
    }

    /// The cut and fill ratios in effect: the pad's own, else its type's.
    pub fn slopes(&self, project: &Project, type_id: crate::ElementTypeId) -> (f64, f64) {
        let (tc, tf) = match project.element_type(type_id).map(|t| &t.data) {
            Some(crate::TypeData::Pad { cut_slope, fill_slope }) => (*cut_slope, *fill_slope),
            _ => (DEFAULT_CUT_SLOPE, DEFAULT_FILL_SLOPE),
        };
        let pick = |own: Option<f64>, ty: f64, d: f64| clamp_slope(own.filter(|r| slope_ok(*r)).unwrap_or(ty), d);
        (pick(self.cut_slope, tc, DEFAULT_CUT_SLOPE), pick(self.fill_slope, tf, DEFAULT_FILL_SLOPE))
    }
}

impl Project {
    /// The pads of terrain `host`, in id (creation) order.
    pub fn pads_of(&self, host: ElementId) -> impl Iterator<Item = (&crate::Element, &Pad)> {
        self.elements.values().filter_map(move |e| match &e.kind {
            ElementKind::Pad(p) if p.host == host => Some((e, p)),
            _ => None,
        })
    }

    /// Pads whose level differs from their terrain's, with the level they take.
    pub(crate) fn pad_levels(&self) -> Vec<(ElementId, crate::LevelId)> {
        self.elements
            .values()
            .filter_map(|e| {
                let ElementKind::Pad(p) = &e.kind else { return None };
                let host = self.element(p.host)?;
                (host.level != e.level).then_some((e.id, host.level))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad() -> Pad {
        Pad {
            host: ElementId(1),
            outline: vec![DVec2::ZERO, DVec2::new(4.0, 0.0), DVec2::new(4.0, 3.0), DVec2::new(0.0, 3.0)],
            elevation: 1.0,
            cut_slope: None,
            fill_slope: None,
            sketch: None,
        }
    }

    #[test]
    fn validity_needs_an_area_and_a_finite_elevation() {
        let mut p = pad();
        assert!(p.is_valid());
        p.elevation = f64::NAN;
        assert!(!p.is_valid());
        let mut p = pad();
        p.outline.truncate(2);
        assert!(!p.is_valid());
    }

    #[test]
    fn repair_drops_bad_overrides() {
        let mut p = Pad { cut_slope: Some(0.0), fill_slope: Some(2.0), ..pad() };
        assert_eq!(p.repair(), 1);
        assert_eq!((p.cut_slope, p.fill_slope), (None, Some(2.0)));
        p.fill_slope = Some(f64::INFINITY);
        assert_eq!(p.repair(), 1);
        assert_eq!(p.repair(), 0);
    }

    /// A document with a terrain (on the first level) and a pad on it, plus a second level.
    fn site() -> (crate::Document, ElementId, ElementId, crate::LevelId) {
        use crate::{Element, ElementType, ElementTypeId, TypeData};
        let mut p = Project::from_template();
        let level = p.levels[0].id;
        let other = p.levels[1].id;
        let add_type = |p: &mut Project, name: &str, data: TypeData| {
            let id = ElementTypeId(p.alloc_id());
            p.types.insert(
                id,
                ElementType { id, name: name.into(), data, info: Default::default(), uid: Default::default() },
            );
            id
        };
        let tt = add_type(
            &mut p,
            "Terrain",
            TypeData::Topography { material: String::new(), contour_interval: 1.0, major_every: 5 },
        );
        let pt = add_type(&mut p, "Pad", TypeData::Pad { cut_slope: 1.0, fill_slope: 1.5 });
        let element = |p: &mut Project, type_id, kind| {
            let id = ElementId(p.alloc_id());
            let e = Element {
                id,
                level,
                type_id,
                mark: String::new(),
                comments: String::new(),
                kind,
                uid: Default::default(),
                phase_created: Default::default(),
                phase_demolished: None,
            };
            p.elements.insert(id, e);
            id
        };
        let boundary =
            vec![DVec2::new(-20.0, -20.0), DVec2::new(20.0, -20.0), DVec2::new(20.0, 20.0), DVec2::new(-20.0, 20.0)];
        let terrain = element(
            &mut p,
            tt,
            ElementKind::Topography(crate::topography::Topography { boundary, ..Default::default() }),
        );
        let pad = element(&mut p, pt, ElementKind::Pad(Pad { host: terrain, ..pad() }));
        (crate::Document::new(p), terrain, pad, other)
    }

    #[test]
    fn repair_drops_a_pad_without_a_terrain() {
        let (doc, terrain, pad, _) = site();
        let mut p = doc.project.clone();
        assert_eq!(p.repair(), 0);
        // A host that is not a terrain.
        if let Some(ElementKind::Pad(x)) = p.elements.get_mut(&pad).map(|e| &mut e.kind) {
            x.host = pad;
        }
        assert!(p.repair() > 0);
        assert!(p.element(pad).is_none());
        // A missing host.
        let mut p = doc.project.clone();
        p.elements.remove(&terrain);
        assert!(p.repair() > 0);
        assert!(p.element(pad).is_none());
    }

    #[test]
    fn repair_clamps_the_type_ratios() {
        let (doc, _, _, _) = site();
        let mut p = doc.project.clone();
        for t in p.types.values_mut() {
            if let crate::TypeData::Pad { cut_slope, fill_slope } = &mut t.data {
                (*cut_slope, *fill_slope) = (0.0, f64::NAN);
            }
        }
        assert_eq!(p.repair(), 2);
        let ratios: Vec<_> = p
            .types
            .values()
            .filter_map(|t| match t.data {
                crate::TypeData::Pad { cut_slope, fill_slope } => Some((cut_slope, fill_slope)),
                _ => None,
            })
            .collect();
        assert_eq!(ratios, vec![(MIN_SLOPE, DEFAULT_FILL_SLOPE)]);
    }

    #[test]
    fn pads_go_with_their_terrain() {
        let (mut doc, terrain, pad, other) = site();
        // Moved with it.
        doc.edit("move", |ctx| {
            ctx.transform_elements(&[terrain], &crate::PlanTransform::translate(DVec2::new(2.0, 1.0)))
        });
        let Some(ElementKind::Pad(p)) = doc.project.element(pad).map(|e| &e.kind) else { panic!("pad") };
        assert_eq!(p.outline[0], DVec2::new(2.0, 1.0));
        // To its level.
        doc.edit("level", |ctx| {
            if let Some(e) = ctx.element_mut(terrain) {
                e.level = other;
            }
        });
        assert_eq!(doc.project.element(pad).map(|e| e.level), Some(other));
        // Copied with it, onto the copy.
        let copies = doc.edit("copy", |ctx| {
            ctx.copy_elements(&[terrain], &[crate::PlanTransform::translate(DVec2::new(50.0, 0.0))])
        });
        assert_eq!(copies.len(), 2);
        let copy = copies.iter().find(|id| doc.project.pads_of(**id).next().is_some());
        assert!(copy.is_some());
        // Deleted with it.
        doc.edit("delete", |ctx| ctx.remove_element(terrain));
        assert!(doc.project.element(pad).is_none());
    }

    #[test]
    fn ron_round_trip() {
        let (doc, _, pad, _) = site();
        let text = crate::persist::to_ron(&doc.project).unwrap_or_default();
        let back = crate::persist::from_ron(&text).map(|p| p.element(pad).cloned());
        assert_eq!(back.ok().flatten(), doc.project.element(pad).cloned());
    }

    #[test]
    fn slopes_are_clamped() {
        assert_eq!(clamp_slope(0.0, 1.0), MIN_SLOPE);
        assert_eq!(clamp_slope(1e9, 1.0), MAX_SLOPE);
        assert_eq!(clamp_slope(f64::NAN, 1.5), 1.5);
    }
}
