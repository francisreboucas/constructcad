//! Imported CAD drawings (underlays) in the view they belong to: pictures,
//! then thin lines in their own pen (grey, black or the layer colors, faded
//! when the element asks), fills and texts, all below the model's linework
//! and picked only when nothing of the model is under the pointer.

use crate::drawing::{Anchor, Drawing2D, LineStyle, Pen, Prim, halftone};
use cc_core::cad::{CadColorMode, CadHAlign, CadImport, CadPrim, CadVAlign, text_width};
use cc_core::{ElementId, ElementKind, Project, ViewId};
use glam::DVec2;

/// The grey of [`CadColorMode::Grey`].
pub const GREY: [u8; 3] = [128, 128, 128];

/// Chord error of tessellated arcs on paper, millimeters.
const CHORD_MM: f64 = 0.05;

/// Draw the imported drawings of `view` under what `d` holds.
pub fn underlays(d: &mut Drawing2D, project: &Project, view: ViewId, scale: u32) {
    let mut under = Drawing2D::default();
    for e in project.elements.values() {
        let ElementKind::CadImport(c) = &e.kind else { continue };
        if c.view == view {
            draw(&mut under, c, e.id, scale);
            d.background.insert(e.id);
        }
    }
    if !under.prims.is_empty() {
        d.prims.splice(0..0, under.prims);
    }
}

/// The color a primitive is drawn in.
fn color_of(c: &CadImport, prim: &CadPrim) -> [u8; 3] {
    let base = match c.color_mode {
        CadColorMode::Grey => GREY,
        CadColorMode::Black => [0, 0, 0],
        CadColorMode::Layers => {
            let rgb = prim.color().or_else(|| c.drawing.layer_of(prim).map(|l| l.color)).unwrap_or([0, 0, 0]);
            // Colors meant for a dark screen (white, yellow, cyan) darkened for white paper.
            let luma = 0.299 * f64::from(rgb[0]) + 0.587 * f64::from(rgb[1]) + 0.114 * f64::from(rgb[2]);
            if luma > 190.0 { rgb.map(|v| (f64::from(v) * 0.55).round() as u8) } else { rgb }
        }
    };
    if c.halftone { halftone(base) } else { base }
}

/// Segments for an arc of `radius` (view units) and `sweep` radians at `1:scale`.
fn segments(radius: f64, sweep: f64, scale: u32) -> usize {
    let tol = CHORD_MM * 1e-3 * f64::from(scale.max(1));
    let step = if radius > tol { 2.0 * (1.0 - tol / radius).clamp(-1.0, 1.0).acos() } else { sweep.abs() };
    let step = step.max(1e-3);
    ((sweep.abs() / step).ceil() as usize).clamp(2, 256)
}

/// Points of an arc in drawing units, tessellated for its size in the view.
fn arc_points(center: DVec2, radius: f64, start: f64, sweep: f64, factor: f64, scale: u32) -> Vec<DVec2> {
    let n = segments(radius * factor, sweep, scale);
    (0..=n).map(|i| center + DVec2::from_angle(start + sweep * i as f64 / n as f64) * radius).collect()
}

/// One imported drawing, in view coordinates.
pub fn draw(d: &mut Drawing2D, c: &CadImport, id: ElementId, scale: u32) {
    let f = c.factor();
    if !(f.is_finite() && f > 0.0) {
        return;
    }
    let place = c.placement();
    let to = |p: DVec2| place.apply(p * f);
    let element = Some(id);
    for image in &c.drawing.images {
        if !c.layer_visible(image.layer) {
            continue;
        }
        let origin = to(image.origin);
        d.prims.push(Prim::Image {
            origin,
            u: to(image.origin + image.u) - origin,
            v: to(image.origin + image.v) - origin,
            image: image.image.clone(),
            element,
            faded: c.halftone,
        });
    }
    for prim in &c.drawing.prims {
        if !c.layer_visible(prim.layer()) {
            continue;
        }
        let pen = Some(Pen { color: Some(color_of(c, prim)), ..Default::default() });
        let line = |d: &mut Drawing2D, a: DVec2, b: DVec2| {
            let (a, b) = (to(a), to(b));
            if a.distance_squared(b) > 1e-14 {
                d.prims.push(Prim::Line { a, b, style: LineStyle::Thin, element, pen });
            }
        };
        match prim {
            CadPrim::Polyline { pts, bulges, closed, .. } => {
                let n = pts.len();
                // Points (two equal ends) are snap targets only.
                let segs = if *closed { n } else { n.saturating_sub(1) };
                for i in 0..segs {
                    let (Some(a), Some(b)) = (pts.get(i), pts.get((i + 1) % n)) else { continue };
                    match cc_core::arc::Arc::from_bulge(*a, *b, bulges.get(i).copied().unwrap_or(0.0)) {
                        Some(arc) => {
                            let pts = arc_points(arc.center, arc.radius, arc.start_angle, arc.sweep, f, scale);
                            for w in pts.windows(2) {
                                line(d, w[0], w[1]);
                            }
                        }
                        None => line(d, *a, *b),
                    }
                }
            }
            CadPrim::Arc { center, radius, start, sweep, .. } => {
                let pts = arc_points(*center, *radius, *start, *sweep, f, scale);
                for w in pts.windows(2) {
                    line(d, w[0], w[1]);
                }
            }
            CadPrim::Fill { pts, .. } => {
                let color = color_of(c, prim);
                let pts: Vec<DVec2> = pts.iter().map(|p| to(*p)).collect();
                if pts.len() >= 3 {
                    d.prims.push(Prim::Fill { pts, color: [color[0], color[1], color[2], 255], element, cut: false });
                }
            }
            CadPrim::Text { pos, height, rotation, text, h, v, .. } => {
                let dir = place.apply_vector(DVec2::from_angle(*rotation));
                let height = height * f;
                let up = dir.perp() * height;
                let w = text_width(text, height);
                let along = match h {
                    CadHAlign::Left => 0.0,
                    CadHAlign::Center => -0.5 * w,
                    CadHAlign::Right => -w,
                };
                let across = match v {
                    CadVAlign::Baseline | CadVAlign::Bottom => 0.0,
                    CadVAlign::Middle => -0.5,
                    CadVAlign::Top => -1.0,
                };
                let at = to(*pos) + dir * along + up * across;
                let size = height * 1000.0 / f64::from(scale.max(1));
                d.prims.push(Prim::Text {
                    pos: at,
                    text: text.clone(),
                    size,
                    anchor: Anchor::LeftBottom,
                    angle: dir.y.atan2(dir.x),
                    element,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::cad::{CadDrawing, CadLayer, CadUnit};
    use std::sync::Arc;

    fn import() -> CadImport {
        CadImport {
            view: ViewId(1),
            name: "a.dxf".into(),
            drawing: Arc::new(CadDrawing {
                layers: vec![
                    CadLayer { name: "0".into(), color: [255, 0, 0], visible: true },
                    CadLayer { name: "Off".into(), color: [0, 0, 0], visible: false },
                ],
                prims: vec![
                    CadPrim::Polyline {
                        layer: 0,
                        color: None,
                        closed: false,
                        pts: vec![DVec2::ZERO, DVec2::new(1000.0, 0.0)],
                        bulges: vec![],
                    },
                    CadPrim::Arc { layer: 1, color: None, center: DVec2::ZERO, radius: 100.0, start: 0.0, sweep: 1.0 },
                    CadPrim::Text {
                        layer: 0,
                        color: Some([0, 0, 255]),
                        pos: DVec2::new(0.0, 500.0),
                        height: 250.0,
                        rotation: 0.0,
                        text: "AB".into(),
                        h: CadHAlign::Right,
                        v: CadVAlign::Baseline,
                    },
                ],
                extents: None,
                units: Some(CadUnit::Millimeters),
                images: vec![],
            }),
            units: CadUnit::Millimeters,
            scale: 1.0,
            position: DVec2::new(2.0, 3.0),
            rotation_deg: 90.0,
            mirrored: false,
            hidden_layers: ["Off".to_string()].into(),
            color_mode: CadColorMode::Layers,
            halftone: false,
        }
    }

    #[test]
    fn draws_placed_lines_texts_and_layer_colors() {
        let mut d = Drawing2D::default();
        draw(&mut d, &import(), ElementId(9), 100);
        let lines: Vec<_> = d.prims.iter().filter(|p| matches!(p, Prim::Line { .. })).collect();
        assert_eq!(lines.len(), 1, "the arc's layer is hidden");
        match lines[0] {
            Prim::Line { a, b, pen, .. } => {
                // 1000 mm along X, turned 90° and moved to (2, 3): (2, 3)-(2, 4).
                assert!(a.distance(DVec2::new(2.0, 3.0)) < 1e-12 && b.distance(DVec2::new(2.0, 4.0)) < 1e-12);
                assert_eq!(pen.and_then(|p| p.color), Some([255, 0, 0]));
            }
            p => panic!("{p:?}"),
        }
        let text = d.prims.iter().find_map(|p| match p {
            Prim::Text { pos, size, angle, text, .. } => Some((*pos, *size, *angle, text.clone())),
            _ => None,
        });
        let (pos, size, angle, text) = text.unwrap();
        assert_eq!(text, "AB");
        // 0.25 m tall at 1:100 is 2.5 mm; right-aligned, so it starts 2 × 0.6 × 0.25 m before (along +Y).
        assert!((size - 2.5).abs() < 1e-9);
        assert!((angle - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!(pos.distance(DVec2::new(2.0 - 0.5, 3.0 - 0.3)) < 1e-9, "{pos}");
    }

    #[test]
    fn model_elements_win_picking_over_underlays() {
        let mut d = Drawing2D::default();
        draw(&mut d, &import(), ElementId(9), 100);
        d.background.insert(ElementId(9));
        // An underlay line at x = 2 and a model line close by.
        assert_eq!(d.hit_test(DVec2::new(2.0, 3.5), 0.05), Some(ElementId(9)));
        d.line(DVec2::new(2.03, 3.0), DVec2::new(2.03, 4.0), LineStyle::Cut, Some(ElementId(1)));
        assert_eq!(d.hit_test(DVec2::new(2.0, 3.5), 0.05), Some(ElementId(1)));
    }

    #[test]
    fn grey_halftone_and_tessellation() {
        let mut c = import();
        c.color_mode = CadColorMode::Grey;
        c.halftone = true;
        c.hidden_layers.clear();
        let mut d = Drawing2D::default();
        draw(&mut d, &c, ElementId(9), 50);
        let arc_lines = d.prims.iter().filter(|p| matches!(p, Prim::Line { .. })).count() - 1;
        assert!(arc_lines >= 2, "{arc_lines}");
        assert!(d.prims.iter().all(|p| match p {
            Prim::Line { pen, .. } => pen.and_then(|p| p.color) == Some(halftone(GREY)),
            _ => true,
        }));
        // Finer at a larger scale on paper, coarser when small.
        assert!(segments(10.0, std::f64::consts::TAU, 50) > segments(0.1, std::f64::consts::TAU, 50));
        assert_eq!(segments(1e-9, 1.0, 100), 2);
    }
}
