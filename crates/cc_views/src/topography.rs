//! Terrain in floor plans: contour lines (majors heavier) and the boundary
//! below the cut plane; where the cut plane crosses the surface, the earth
//! above it hatched in the type's material and outlined as cut.

use crate::drawing::{Drawing2D, LineStyle};
use cc_core::{LevelId, Project, TypeData};
use cc_geom::ModelGeometry;
use cc_geom::topography::{MAX_CONTOURS, TopoGeom, chain, contours, slice};
use glam::{DVec2, DVec3};

/// The terrain surfaces of `level` in its floor plan cut at `cut_z`.
pub(crate) fn draw(
    d: &mut Drawing2D,
    project: &Project,
    geom: &ModelGeometry,
    level: LevelId,
    cut_z: f64,
    spacing: f64,
) {
    for (id, g) in &geom.topographies {
        let Some(e) = project.element(*id).filter(|e| e.level == level) else { continue };
        let (material, interval, major_every) = match project.element_type(e.type_id).map(|t| &t.data) {
            Some(TypeData::Topography { material, contour_interval, major_every }) => {
                (material.as_str(), *contour_interval, *major_every)
            }
            _ => ("", 1.0, 5),
        };
        let id = Some(*id);
        // The earth above the cut, hatched.
        if g.max_z > cut_z {
            if g.min_z >= cut_z {
                let outline: Vec<DVec2> = g.boundary.iter().map(|p| p.truncate()).collect();
                crate::cut::cut_fill(d, project, material, outline, spacing, id);
            } else {
                for piece in g.triangles.iter().filter_map(|t| above(t, cut_z)) {
                    crate::cut::cut_fill(d, project, material, piece, spacing, id);
                }
                for line in chain(slice(&g.triangles, cut_z)) {
                    d.polyline(&line, false, LineStyle::Cut, id);
                }
            }
        }
        // Contours below the cut.
        for c in contours(g, interval, major_every, MAX_CONTOURS).into_iter().filter(|c| c.z < cut_z) {
            let style = if c.major { LineStyle::Projection } else { LineStyle::Thin };
            d.polyline(&c.points, false, style, id);
        }
        draw_boundary(d, g, cut_z, id);
    }
}

/// The boundary: cut where it stands above the cut plane, seen below it.
fn draw_boundary(d: &mut Drawing2D, g: &TopoGeom, cut_z: f64, id: Option<cc_core::ElementId>) {
    let n = g.boundary.len();
    for i in 0..n {
        let (a, b) = (g.boundary[i], g.boundary[(i + 1) % n]);
        let style = |z: f64| if z > cut_z { LineStyle::Cut } else { LineStyle::Projection };
        if (a.z > cut_z) == (b.z > cut_z) {
            d.line(a.truncate(), b.truncate(), style(a.z), id);
        } else {
            let m = a.truncate().lerp(b.truncate(), (cut_z - a.z) / (b.z - a.z));
            d.line(a.truncate(), m, style(a.z), id);
            d.line(m, b.truncate(), style(b.z), id);
        }
    }
}

/// The part of triangle `t` above `z`, in plan (none when it is all below).
fn above(t: &[DVec3; 3], z: f64) -> Option<Vec<DVec2>> {
    let mut out = Vec::with_capacity(4);
    for i in 0..3 {
        let (a, b) = (t[i], t[(i + 1) % 3]);
        if a.z >= z {
            out.push(a.truncate());
        }
        if (a.z >= z) != (b.z >= z) {
            out.push(a.truncate().lerp(b.truncate(), (z - a.z) / (b.z - a.z)));
        }
    }
    (out.len() >= 3).then_some(out)
}

#[cfg(test)]
mod tests {
    use crate::drawing::{LineStyle, Prim};
    use cc_core::topography::Topography;
    use cc_core::{Document, Element, ElementId, ElementKind, Project};
    use cc_geom::ModelGeometry;
    use glam::{DVec2, DVec3};

    /// A 10 x 4 m slope rising from 0 to 4 m along x, on the ground level.
    fn slope() -> (Project, ElementId) {
        let mut doc = Document::new(Project::from_template());
        let level = doc.project.levels[0].id;
        let ty = doc.edit("type", |e| {
            let id = cc_core::ElementTypeId(e.alloc_id());
            e.add_type(cc_core::ElementType {
                id,
                name: "Terreno".into(),
                data: cc_core::TypeData::Topography { material: "Terra".into(), contour_interval: 1.0, major_every: 5 },
                info: Default::default(),
                uid: Default::default(),
            })
        });
        let points = vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(10.0, 0.0, 4.0),
            DVec3::new(10.0, 4.0, 4.0),
            DVec3::new(0.0, 4.0, 0.0),
            DVec3::new(5.0, 2.0, 2.0),
        ];
        let boundary = vec![DVec2::ZERO, DVec2::new(10.0, 0.0), DVec2::new(10.0, 4.0), DVec2::new(0.0, 4.0)];
        let id = doc.edit("t", |e| {
            let id = ElementId(e.alloc_id());
            e.add_element(Element {
                id,
                level,
                type_id: ty,
                mark: String::new(),
                comments: String::new(),
                uid: Default::default(),
                kind: ElementKind::Topography(Topography { points, boundary, base_depth: 2.0, sketch: None }),
                phase_created: Default::default(),
                phase_demolished: None,
            });
            id
        });
        (doc.project, id)
    }

    fn fills(d: &crate::Drawing2D, id: ElementId) -> Vec<Vec<DVec2>> {
        d.prims
            .iter()
            .filter_map(|p| match p {
                Prim::Fill { pts, element: Some(e), cut: true, .. } if *e == id => Some(pts.clone()),
                _ => None,
            })
            .collect()
    }

    fn lines(d: &crate::Drawing2D, id: ElementId, style: LineStyle) -> Vec<(DVec2, DVec2)> {
        d.prims
            .iter()
            .filter_map(|p| match p {
                Prim::Line { a, b, style: s, element: Some(e), .. } if *e == id && *s == style => Some((*a, *b)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn plans_hatch_the_earth_above_the_cut_and_draw_contours_below() {
        let (project, id) = slope();
        let geom = ModelGeometry::build(&project);
        let level = project.levels[0].id;
        // Cut at 1.2 m: the earth beyond x = 3 is above it.
        let d = crate::plan(&project, &geom, level, 1.2, false, 100);
        let filled = fills(&d, id);
        assert!(!filled.is_empty());
        assert!(filled.iter().flatten().all(|p| p.x >= 3.0 - 1e-9), "{filled:?}");
        let area: f64 = filled.iter().map(|f| cc_geom::polygon::area(f)).sum();
        assert!((area - 7.0 * 4.0).abs() < 1e-6, "{area}");
        // The cut runs across at x = 3; contours 0 and 1 below it (1 m apart).
        let cut = lines(&d, id, LineStyle::Cut);
        assert!(cut.iter().any(|(a, b)| (a.x - 3.0).abs() < 1e-9 && (b.x - 3.0).abs() < 1e-9));
        let thin = lines(&d, id, LineStyle::Thin);
        assert!(thin.iter().any(|(a, b)| (a.x - 2.5).abs() < 1e-9 && (b.x - 2.5).abs() < 1e-9), "{thin:?}");
        assert!(!thin.iter().any(|(a, _)| a.x > 3.0 + 1e-9));
        // Above the whole surface (cut at 5 m): no hatch, every contour drawn.
        let d = crate::plan(&project, &geom, level, 5.0, false, 100);
        assert!(fills(&d, id).is_empty());
        assert!(!lines(&d, id, LineStyle::Thin).is_empty());
        assert!(lines(&d, id, LineStyle::Cut).is_empty());
        // Ceiling plans leave the terrain out.
        let d = crate::plan(&project, &geom, level, 1.2, true, 100);
        assert!(fills(&d, id).is_empty() && lines(&d, id, LineStyle::Projection).is_empty());
    }
}
