//! Views of a component definition for the modeling environment: the
//! pieces seen from above, the front, the right or in a 3D axonometric,
//! with the reference planes seen edge-on as dash-dot lines named at one
//! end, the reference lines of that view and the dimensions between planes
//! (labeled ones show their parameter).
//!
//! Coordinates are the view's own (u, v) in meters, as [`SketchPlane`]
//! defines them: plan x, y; front x, z; side y, z. Lines carry no element:
//! the environment picks planes and dimensions from the lists returned.

use crate::{Anchor, Drawing2D, LineStyle, format};
use cc_core::component::{ComponentDef, SketchPlane, WALL_THICKNESS};
use cc_core::{Dimension, DimensionKind, ViewId};
use glam::{DVec2, DVec3};
use std::collections::BTreeMap;

/// Thickness of the wall doors and windows are shown in.
const PREVIEW_WALL: f64 = 0.15;

/// The views of the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ComponentViewKind {
    #[default]
    Plan,
    Front,
    Side,
    /// From the front, right and above.
    ThreeD,
}

impl ComponentViewKind {
    pub const ALL: [ComponentViewKind; 4] =
        [ComponentViewKind::Plan, ComponentViewKind::Front, ComponentViewKind::Side, ComponentViewKind::ThreeD];

    /// The sketch plane drawn in this view (none in 3D).
    pub fn plane(self) -> Option<SketchPlane> {
        match self {
            ComponentViewKind::Plan => Some(SketchPlane::Plan),
            ComponentViewKind::Front => Some(SketchPlane::Front),
            ComponentViewKind::Side => Some(SketchPlane::Side),
            ComponentViewKind::ThreeD => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            ComponentViewKind::Plan => "comp-view-plan",
            ComponentViewKind::Front => "comp-view-front",
            ComponentViewKind::Side => "comp-view-side",
            ComponentViewKind::ThreeD => "comp-view-3d",
        }
    }

    /// A model point in this view: (u, v) and the depth toward the viewer.
    pub fn project(self, p: DVec3) -> DVec3 {
        match self {
            ComponentViewKind::Plan => p,
            ComponentViewKind::Front => DVec3::new(p.x, p.z, -p.y),
            ComponentViewKind::Side => DVec3::new(p.y, p.z, p.x),
            ComponentViewKind::ThreeD => {
                let eye = DVec3::new(1.0, -1.0, 1.0).normalize();
                let right = (-eye).cross(DVec3::Z).normalize();
                let up = right.cross(-eye);
                DVec3::new(p.dot(right), p.dot(up), p.dot(eye))
            }
        }
    }
}

/// A reference plane seen edge-on: its name and the ends of its line.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewPlane {
    pub name: String,
    pub a: DVec2,
    pub b: DVec2,
    /// One of the origin planes (cannot move).
    pub origin: bool,
}

/// A dimension between planes as drawn: where its text sits.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewDimension {
    pub index: usize,
    pub text_pos: DVec2,
    pub text_angle: f64,
    pub value: f64,
    pub label: Option<String>,
}

/// A view of a component: its drawing and what can be picked in it.
#[derive(Debug, Clone, Default)]
pub struct ComponentView {
    pub drawing: Drawing2D,
    pub planes: Vec<ViewPlane>,
    pub dimensions: Vec<ViewDimension>,
    /// Equalities (EQ) drawn in this view, by index: where each "EQ" text sits.
    pub equals: Vec<ViewDimension>,
    /// Each piece's edges in this view (by index in the definition), for picking.
    pub pieces: Vec<(usize, Vec<(DVec2, DVec2)>)>,
    /// Box of the pieces and planes, for zoom to fit.
    pub bounds: Option<(DVec2, DVec2)>,
    /// Pieces that do not resolve, with why.
    pub errors: Vec<String>,
}

fn grow(b: &mut Option<(DVec2, DVec2)>, p: DVec2) {
    if p.is_finite() {
        *b = Some(b.map_or((p, p), |(lo, hi)| (lo.min(p), hi.max(p))));
    }
}

/// The component `def` (with instance `overrides`) in `kind`, drawn at a
/// view `scale` (for text and dimension sizes).
pub fn component_view(
    def: &ComponentDef,
    overrides: &BTreeMap<String, f64>,
    kind: ComponentViewKind,
    scale: u32,
) -> ComponentView {
    let mut out = ComponentView::default();
    let extra = [(WALL_THICKNESS, PREVIEW_WALL)];
    let scope = def.scope(overrides, &extra);
    let mm = f64::from(scale.max(1)) / 1000.0;

    // Pieces: solids with voids cut, then the voids themselves, dashed.
    let solids = cc_geom::resolve_component_in(def, overrides, &extra);
    let (all, errors) = def.resolve_in(overrides, &extra);
    out.errors = errors
        .iter()
        .map(|(i, e)| format!("{}: {e}", def.items.get(*i).map_or(String::new(), |it| it.name.clone())))
        .collect();
    let pieces = solids
        .iter()
        .filter(|p| !p.plan_only || kind == ComponentViewKind::Plan)
        .map(|p| (p.item, false, cc_geom::component::faces_of(p)))
        .chain(all.iter().filter(|p| p.void).map(|p| (p.item, true, cc_geom::component::faces_of(p))));
    for (item, void, faces) in pieces {
        let turned: Vec<Vec<DVec3>> = faces.iter().map(|f| f.iter().map(|p| kind.project(*p)).collect()).collect();
        let mut edges = Vec::new();
        for (a, b) in cc_geom::plan_edges(&turned) {
            let (a, b) = (a.truncate(), b.truncate());
            grow(&mut out.bounds, a);
            grow(&mut out.bounds, b);
            out.drawing.line(a, b, if void { LineStyle::Hidden } else { LineStyle::Projection }, None);
            edges.push((a, b));
        }
        // A solid cut by voids comes in parts: they all pick the same piece.
        match out.pieces.iter_mut().find(|(i, _)| *i == item) {
            Some((_, e)) => e.extend(edges),
            None => out.pieces.push((item, edges)),
        }
    }
    let Some(view) = kind.plane() else {
        return out;
    };

    // Planes seen edge-on in this view, across the pieces with a margin.
    let shown: Vec<(cc_core::RefPlane, bool, f64)> = def
        .all_ref_planes()
        .into_iter()
        .filter_map(|p| {
            let vertical = p.axis.in_view(view)?;
            Some((p.clone(), vertical, scope.get(&p.name)?))
        })
        .collect();
    for (_, vertical, at) in &shown {
        grow(&mut out.bounds, if *vertical { DVec2::new(*at, 0.0) } else { DVec2::new(0.0, *at) });
    }
    let (lo, hi) = out.bounds.unwrap_or((DVec2::splat(-0.5), DVec2::splat(0.5)));
    let margin = ((hi - lo).max_element() * 0.15).max(0.1);
    let (lo, hi) = (lo - DVec2::splat(margin), hi + DVec2::splat(margin));
    let origin = |name: &str| cc_core::component_ref::ORIGIN_PLANES.iter().any(|(n, _)| *n == name);
    for (plane, vertical, at) in &shown {
        let (a, b) = if *vertical {
            (DVec2::new(*at, lo.y), DVec2::new(*at, hi.y))
        } else {
            (DVec2::new(lo.x, *at), DVec2::new(hi.x, *at))
        };
        out.drawing.line(a, b, LineStyle::Center, None);
        // The view fits the planes' ends and names too.
        grow(&mut out.bounds, a);
        grow(&mut out.bounds, b + (b - a).normalize_or_zero() * 12.0 * mm);
        // The name past the far end, along the line.
        let (pos, angle) =
            if *vertical { (b + DVec2::Y * mm, std::f64::consts::FRAC_PI_2) } else { (b + DVec2::X * mm, 0.0) };
        out.drawing.text_at(pos, plane.name.clone(), 2.0, Anchor::LeftBottom, angle);
        out.planes.push(ViewPlane { name: plane.name.clone(), a, b, origin: origin(&plane.name) });
    }

    // Reference lines of this view.
    for line in def.ref_lines.iter().filter(|l| l.plane == view) {
        let ev = |e: &str| cc_core::expr::eval(e, &|n: &str| scope.get(n)).ok();
        let (Some(su), Some(sv), Some(eu), Some(ev2)) =
            (ev(&line.start.0), ev(&line.start.1), ev(&line.end.0), ev(&line.end.1))
        else {
            continue;
        };
        let (a, b) = (DVec2::new(su, sv), DVec2::new(eu, ev2));
        out.drawing.line(a, b, LineStyle::Center, None);
        out.drawing.text_at(b + DVec2::splat(mm), line.name.clone(), 2.0, Anchor::LeftBottom, 0.0);
    }

    // Dimensions between planes: the label's parameter is written with the value.
    for (index, d) in def.dimensions.iter().enumerate().filter(|(_, d)| d.view == view) {
        let (Ok(value), Some(from), Some(to), Some(axis)) = (
            def.dimension_value(&scope, index),
            scope.get(&d.from),
            scope.get(&d.to),
            def.ref_plane(&d.from).and_then(|p| p.axis.in_view(view)),
        ) else {
            continue;
        };
        let (a, b, dir) = if axis {
            (DVec2::new(from, d.place), DVec2::new(to, d.place), DVec2::X)
        } else {
            (DVec2::new(d.place, from), DVec2::new(d.place, to), DVec2::Y)
        };
        let dim = Dimension {
            view: ViewId(0),
            a,
            b,
            offset: 0.0,
            kind: DimensionKind::Linear { dir },
            refs: Default::default(),
        };
        let Some(layout) = crate::dimension_layout(&dim, scale) else { continue };
        for (p, q, style) in layout.lines {
            out.drawing.line(p, q, style, None);
        }
        let text = match &d.label {
            Some(label) => format!("{label} = {}", format::meters(value)),
            None => format::meters(value),
        };
        out.drawing.text_at(layout.text_pos, text, 2.5, Anchor::CenterBottom, layout.text_angle);
        grow(&mut out.bounds, layout.text_pos);
        grow(&mut out.bounds, a);
        grow(&mut out.bounds, b);
        out.dimensions.push(ViewDimension {
            index,
            text_pos: layout.text_pos,
            text_angle: layout.text_angle,
            value,
            label: d.label.clone(),
        });
    }
    // Equalities: two equal dimensions marked EQ.
    for (index, q) in def.equals.iter().enumerate().filter(|(_, q)| q.view == view) {
        let (Some(a), Some(m), Some(b), Some(vertical)) = (
            scope.get(&q.a),
            scope.get(&q.middle),
            scope.get(&q.b),
            def.ref_plane(&q.a).and_then(|p| p.axis.in_view(view)),
        ) else {
            continue;
        };
        for (from, to) in [(a, m), (m, b)] {
            let (pa, pb, dir) = if vertical {
                (DVec2::new(from, q.place), DVec2::new(to, q.place), DVec2::X)
            } else {
                (DVec2::new(q.place, from), DVec2::new(q.place, to), DVec2::Y)
            };
            let dim = Dimension {
                view: ViewId(0),
                a: pa,
                b: pb,
                offset: 0.0,
                kind: DimensionKind::Linear { dir },
                refs: Default::default(),
            };
            let Some(layout) = crate::dimension_layout(&dim, scale) else { continue };
            for (p, q2, style) in layout.lines {
                out.drawing.line(p, q2, style, None);
            }
            out.drawing.text_at(layout.text_pos, "EQ", 2.5, Anchor::CenterBottom, layout.text_angle);
            grow(&mut out.bounds, layout.text_pos);
            out.equals.push(ViewDimension {
                index,
                text_pos: layout.text_pos,
                text_angle: layout.text_angle,
                value: (to - from).abs(),
                label: Some("EQ".into()),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::{ComponentItem, ComponentParam, ComponentShape, ParamKind, PlaneAxis, RefDimension, RefPlane};

    fn table() -> ComponentDef {
        ComponentDef {
            params: vec![ComponentParam { name: "Width".into(), value: 1.2, kind: ParamKind::Length, instance: false }],
            ref_planes: vec![
                RefPlane { name: "Left".into(), axis: PlaneAxis::X, at: "-Width/2".into() },
                RefPlane { name: "Right".into(), axis: PlaneAxis::X, at: "Left + Width".into() },
                RefPlane { name: "Top".into(), axis: PlaneAxis::Z, at: "0.75".into() },
            ],
            dimensions: vec![RefDimension {
                view: SketchPlane::Plan,
                from: "Left".into(),
                to: "Right".into(),
                label: Some("Width".into()),
                place: -0.6,
            }],
            items: vec![ComponentItem {
                name: "Top".into(),
                shape: ComponentShape::Box {
                    x: "Left".into(),
                    y: "-0.4".into(),
                    z: "Top - 0.03".into(),
                    width: "Right - Left".into(),
                    depth: "0.8".into(),
                    height: "0.03".into(),
                },
                material: String::new(),
                in_plan: true,
                void: false,
                plan_only: false,
                detail: Default::default(),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn views_show_the_planes_seen_edge_on_and_labeled_dimensions() {
        let def = table();
        let plan = component_view(&def, &BTreeMap::new(), ComponentViewKind::Plan, 20);
        assert!(plan.errors.is_empty(), "{:?}", plan.errors);
        // Plan: the x planes (CenterX, Left, Right) and the y one (CenterY), not the heights.
        let names: Vec<&str> = plan.planes.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["CenterX", "CenterY", "Left", "Right"]);
        let left = &plan.planes[2];
        assert!((left.a.x + 0.6).abs() < 1e-12 && (left.b.x + 0.6).abs() < 1e-12 && !left.origin);
        assert!(plan.planes[0].origin);
        // The pieces' outline: the table top from -0.6 to 0.6.
        let (lo, hi) = plan.bounds.unwrap();
        assert!(lo.x <= -0.6 && hi.x >= 0.6);
        assert_eq!(plan.dimensions.len(), 1);
        assert!((plan.dimensions[0].value - 1.2).abs() < 1e-12);
        assert!(
            plan.drawing
                .prims
                .iter()
                .any(|p| matches!(p, crate::Prim::Text { text, .. } if text.starts_with("Width = ")))
        );
        // Front: x planes and the heights.
        let front = component_view(&def, &BTreeMap::new(), ComponentViewKind::Front, 20);
        let names: Vec<&str> = front.planes.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["CenterX", "Base", "Left", "Right", "Top"]);
        assert!(front.dimensions.is_empty());
        // Side: y planes and heights; 3D: only the pieces.
        let side = component_view(&def, &BTreeMap::new(), ComponentViewKind::Side, 20);
        assert_eq!(side.planes.len(), 3);
        let three = component_view(&def, &BTreeMap::new(), ComponentViewKind::ThreeD, 20);
        assert!(three.planes.is_empty() && !three.drawing.prims.is_empty());
        // A wider type moves the right plane.
        let wide = BTreeMap::from([("Width".to_string(), 2.0)]);
        let mut def = def;
        def.params[0].instance = true;
        let plan = component_view(&def, &wide, ComponentViewKind::Plan, 20);
        assert!((plan.planes[3].a.x - 1.0).abs() < 1e-12);
    }
}
