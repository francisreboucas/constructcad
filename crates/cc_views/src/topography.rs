//! Terrain in floor plans: contour lines (majors heavier) and the boundary
//! below the cut plane; where the cut plane crosses the surface, the earth
//! above it hatched in the type's material and outlined as cut.
//!
//! Building pads: their outline with their elevation, ticks across their
//! slopes (long and short in turn, the short ones from the top of the
//! slope), the daylight line, and the existing ground's contours dashed
//! where the pads changed it.

use crate::drawing::{Anchor, Drawing2D, LineStyle};
use cc_core::topography::{DEFAULT_CONTOUR_INTERVAL, DEFAULT_MAJOR_EVERY};
use cc_core::{LevelId, Project, TypeData};
use cc_geom::ModelGeometry;
use cc_geom::pad::PadGeom;
use cc_geom::topography::{MAX_CONTOURS, TopoGeom, chain, contours, contours_of, slice};
use glam::{DVec2, DVec3};

/// The terrain surfaces of `level` in its floor plan cut at `cut_z`.
pub(crate) fn draw(d: &mut Drawing2D, project: &Project, geom: &ModelGeometry, level: LevelId, cut_z: f64, scale: u32) {
    let spacing = crate::cut::spacing(scale);
    for (id, g) in &geom.topographies {
        let Some(e) = project.element(*id).filter(|e| e.level == level) else { continue };
        let (material, interval, major_every) = match project.element_type(e.type_id).map(|t| &t.data) {
            Some(TypeData::Topography { material, contour_interval, major_every }) => {
                (material.as_str(), *contour_interval, *major_every)
            }
            _ => ("", DEFAULT_CONTOUR_INTERVAL, DEFAULT_MAJOR_EVERY),
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
        // The existing ground where pads changed it.
        let range = g
            .existing
            .iter()
            .flatten()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.z), hi.max(p.z)));
        for c in
            contours_of(&g.existing, range, interval, major_every, MAX_CONTOURS).into_iter().filter(|c| c.z < cut_z)
        {
            d.polyline(&c.points, false, LineStyle::Hidden, id);
        }
        draw_boundary(d, g, cut_z, id);
        for (pad, _) in project.pads_of(e.id) {
            if let Some(pg) = geom.pads.get(&pad.id) {
                draw_pad(d, pg, scale, Some(pad.id));
            }
        }
    }
}

/// Spacing of the slope ticks on paper, millimeters.
const TICK_MM: f64 = 2.5;

/// A pad: outline and elevation, slope ticks and daylight line.
fn draw_pad(d: &mut Drawing2D, g: &PadGeom, scale: u32, id: Option<cc_core::ElementId>) {
    d.polyline(&g.outline, true, LineStyle::Projection, id);
    if let Some(at) = centroid(&g.outline) {
        d.tag_text(at, crate::format::elevation(g.z), 2.5, Anchor::Center, 0.0, id);
    }
    for line in &g.daylight {
        d.polyline(line, false, LineStyle::Thin, id);
    }
    let step = TICK_MM * scale.max(1) as f64 / 1000.0;
    let n = g.outline.len();
    for i in 0..n {
        let (a, b) = (g.outline[i], g.outline[(i + 1) % n]);
        let len = a.distance(b);
        let Some(u) = (b - a).try_normalize() else { continue };
        let normal = DVec2::new(u.y, -u.x);
        let count = ((len / step).floor() as usize).min(100_000);
        for k in 0..count {
            let p = a + u * ((k as f64 + 0.5) * len / count as f64);
            let Some(reach) = daylight_hit(&g.daylight, p, normal) else { continue };
            let cut = g
                .rays
                .iter()
                .filter(|r| r.edge == Some(i) || r.edge.is_none())
                .min_by(|x, y| x.origin.distance(p).total_cmp(&y.origin.distance(p)))
                .is_some_and(|r| r.cut);
            tick(d, p, normal, reach, k % 2 == 1, cut, id);
        }
    }
    // Convex corners: the fan's rays, all long.
    for r in g.rays.iter().filter(|r| r.edge.is_none()) {
        if r.end.distance(r.origin) > 1e-6 {
            d.line(r.origin, r.end, LineStyle::Thin, id);
        }
    }
}

/// A tick across a slope `reach` long from `p` along `dir`: the whole slope,
/// or (`short`) its third from the top (the pad's edge for fill, the
/// daylight for cut).
fn tick(d: &mut Drawing2D, p: DVec2, dir: DVec2, reach: f64, short: bool, cut: bool, id: Option<cc_core::ElementId>) {
    let far = p + dir * reach;
    let (a, b) = match (short, cut) {
        (false, _) => (p, far),
        (true, false) => (p, p + dir * (reach / 3.0)),
        (true, true) => (far, far - dir * (reach / 3.0)),
    };
    d.line(a, b, LineStyle::Thin, id);
}

/// Area centroid of a simple polygon (none without an area).
fn centroid(poly: &[DVec2]) -> Option<DVec2> {
    let n = poly.len();
    let (mut a, mut c) = (0.0, DVec2::ZERO);
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let w = p.perp_dot(q);
        a += w;
        c += (p + q) * w;
    }
    (a.abs() > 1e-12).then(|| c / (3.0 * a)).filter(|c| c.is_finite())
}

/// How far from `p` along `dir` the first daylight line is met (none when
/// it is not, or right at `p`).
fn daylight_hit(lines: &[Vec<DVec2>], p: DVec2, dir: DVec2) -> Option<f64> {
    let mut best: Option<f64> = None;
    for w in lines.iter().flat_map(|l| l.windows(2)) {
        let (a, b) = (w[0], w[1]);
        let e = b - a;
        let den = dir.perp_dot(e);
        if den.abs() < 1e-12 {
            continue;
        }
        let t = (a - p).perp_dot(e) / den;
        let s = (a - p).perp_dot(dir) / den;
        if t > 1e-6 && (-1e-9..=1.0 + 1e-9).contains(&s) && best.is_none_or(|x| t < x) {
            best = Some(t);
        }
    }
    best
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

    #[test]
    fn pads_draw_their_slopes_between_the_edge_and_the_daylight() {
        let (mut project, terrain) = slope();
        let level = project.levels[0].id;
        // A pad across the middle, at 2 m: cut on the high side, fill on the low one.
        let ty = cc_core::ElementTypeId(project.alloc_id());
        project.types.insert(
            ty,
            cc_core::ElementType {
                id: ty,
                name: "Pad".into(),
                data: cc_core::TypeData::Pad { cut_slope: 1.0, fill_slope: 1.0 },
                info: Default::default(),
                uid: Default::default(),
            },
        );
        let outline = vec![DVec2::new(4.0, 1.0), DVec2::new(6.0, 1.0), DVec2::new(6.0, 3.0), DVec2::new(4.0, 3.0)];
        let pad = ElementId(project.alloc_id());
        let mut e = project.element(terrain).unwrap().clone();
        e.id = pad;
        e.type_id = ty;
        e.kind = ElementKind::Pad(cc_core::pad::Pad {
            host: terrain,
            outline: outline.clone(),
            elevation: 2.0,
            cut_slope: None,
            fill_slope: None,
            sketch: None,
        });
        project.elements.insert(pad, e);
        let geom = ModelGeometry::build(&project);
        let g = &geom.pads[&pad];
        let reach = g.rays.iter().map(|r| r.end.distance(r.origin)).fold(0.0, f64::max);
        assert!(reach > 0.5 && reach < 2.0, "{reach}");
        let d = crate::plan(&project, &geom, level, 5.0, false, 50);
        let dist = |p: DVec2| {
            let n = outline.len();
            if cc_geom::polygon::contains(&outline, p) {
                return 0.0;
            }
            (0..n)
                .map(|i| cc_geom::polygon::closest_on_segment(outline[i], outline[(i + 1) % n], p).0.distance(p))
                .fold(f64::INFINITY, f64::min)
        };
        // Ticks and daylight lie in the slope band.
        let thin = lines(&d, pad, LineStyle::Thin);
        assert!(thin.len() > 8, "{}", thin.len());
        for (a, b) in &thin {
            assert!(dist(*a) <= reach + 1e-6 && dist(*b) <= reach + 1e-6, "{a} {b}");
        }
        // The existing ground's contours, dashed, only where the pad changed it.
        let hidden = lines(&d, terrain, LineStyle::Hidden);
        assert!(!hidden.is_empty());
        for (a, b) in &hidden {
            assert!(dist(*a) <= reach + 1e-6 && dist(*b) <= reach + 1e-6, "{a} {b}");
        }
        // The outline and its elevation.
        assert_eq!(lines(&d, pad, LineStyle::Projection).len(), 4);
        assert!(
            d.prims
                .iter()
                .any(|p| matches!(p, Prim::Text { element: Some(e), text, .. } if *e == pad && text.contains('2')))
        );
    }
}
