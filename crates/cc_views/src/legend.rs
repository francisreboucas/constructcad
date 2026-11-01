use crate::{Drawing2D, LineStyle, cut::POCHE};
use cc_core::{Category, ElementTypeId, Project, TypeData};
use glam::DVec2;

/// Legend: one row per type with a 2D symbol and its name.
///
/// Rows are spaced in paper units so the legend reads the same at any scale.
pub fn legend(project: &Project, types: &[ElementTypeId], scale: u32) -> Drawing2D {
    let mut d = Drawing2D::default();
    let mm = scale as f64 / 1000.0; // one paper millimeter in model meters
    let row = 16.0 * mm;
    let sample = 1.2_f64.max(22.0 * mm);
    let mut y = 0.0;
    for t in types.iter().filter_map(|id| project.element_type(*id)).filter(|t| t.category().is_model()) {
        // Component outlines in plan, centered on the row.
        let device = match &t.data {
            TypeData::Component { def, .. } => def.mep.and_then(|m| m.device().map(|d| (d, m.power()))),
            _ => None,
        };
        // Doors and windows (of any kind of type) show their category's symbol.
        let opening =
            t.opening_size().map(|(w, _)| w).filter(|_| matches!(t.category(), Category::Door | Category::Window));
        let width = &opening.unwrap_or(0.0);
        let plan: Vec<Vec<DVec2>> = match &t.data {
            // Devices are shown by their plan symbol.
            TypeData::Component { .. } if device.is_some() || opening.is_some() => Vec::new(),
            TypeData::Component { def, .. } => def
                .resolve(&Default::default())
                .0
                .into_iter()
                .filter(|p| p.in_plan && !p.void)
                .map(|p| p.footprint)
                .collect(),
            _ => Vec::new(),
        };
        let plan_bounds = plan.iter().flatten().fold(None, |acc: Option<(DVec2, DVec2)>, p| {
            Some(acc.map_or((*p, *p), |(lo, hi)| (lo.min(*p), hi.max(*p))))
        });
        // Door swings and tall component outlines extend above the row; leave room for them.
        let above = match opening {
            Some(width) if t.category() == Category::Door => width + 0.075,
            _ => plan_bounds.map_or(0.0, |(lo, hi)| (hi.y - lo.y) / 2.0),
        };
        y -= above;
        let o = DVec2::new(0.0, y);
        match &t.data {
            TypeData::Wall { layers, .. } | TypeData::Floor { layers } | TypeData::Roof { layers, .. } => {
                let total: f64 = layers.iter().map(|l| l.thickness).sum();
                let mut v = total * 0.5;
                for layer in layers {
                    let c = project.material_color(&layer.material);
                    let rect = vec![
                        o + DVec2::new(0.0, v - layer.thickness),
                        o + DVec2::new(sample, v - layer.thickness),
                        o + DVec2::new(sample, v),
                        o + DVec2::new(0.0, v),
                    ];
                    d.fill(rect.clone(), [c[0], c[1], c[2], 255], None);
                    d.polyline(&rect, true, LineStyle::Thin, None);
                    v -= layer.thickness;
                }
                let h = total * 0.5;
                d.line(o + DVec2::new(0.0, h), o + DVec2::new(sample, h), LineStyle::Cut, None);
                d.line(o + DVec2::new(0.0, -h), o + DVec2::new(sample, -h), LineStyle::Cut, None);
            }
            _ if opening.is_some() && t.category() == Category::Door => {
                let h = 0.075;
                let jamb = |d: &mut Drawing2D, x0: f64, x1: f64| {
                    let r =
                        [o + DVec2::new(x0, -h), o + DVec2::new(x1, -h), o + DVec2::new(x1, h), o + DVec2::new(x0, h)];
                    d.fill(r.to_vec(), POCHE, None);
                    d.polyline(&r, true, LineStyle::Cut, None);
                };
                jamb(&mut d, 0.0, 0.2);
                jamb(&mut d, 0.2 + width, 0.4 + width);
                let hinge = o + DVec2::new(0.2, h);
                d.line(hinge, hinge + DVec2::Y * *width, LineStyle::Projection, None);
                d.arc(hinge, *width, std::f64::consts::FRAC_PI_2, 0.0, LineStyle::Thin, None);
            }
            _ if opening.is_some() => {
                let h = 0.075;
                for x in [0.0, 0.2 + width] {
                    let r = [
                        o + DVec2::new(x, -h),
                        o + DVec2::new(x + 0.2, -h),
                        o + DVec2::new(x + 0.2, h),
                        o + DVec2::new(x, h),
                    ];
                    d.fill(r.to_vec(), POCHE, None);
                    d.polyline(&r, true, LineStyle::Cut, None);
                }
                for v in [-h, h] {
                    d.line(o + DVec2::new(0.2, v), o + DVec2::new(0.2 + width, v), LineStyle::Projection, None);
                }
                for v in [-0.01, 0.01] {
                    d.line(o + DVec2::new(0.2, v), o + DVec2::new(0.2 + width, v), LineStyle::Thin, None);
                }
            }
            TypeData::Ceiling { thickness, .. } => {
                let r = [
                    o,
                    o + DVec2::new(sample, 0.0),
                    o + DVec2::new(sample, *thickness),
                    o + DVec2::new(0.0, *thickness),
                ];
                d.polyline(&r, true, LineStyle::Projection, None);
            }
            TypeData::Column { section, .. } => {
                let c = o + DVec2::new(0.4, 0.0);
                for part in cc_geom::section::column_parts(*section, c, 0.0) {
                    d.fill(part, POCHE, None);
                }
                for outline in cc_geom::section::column_loops(*section, c, 0.0) {
                    d.polyline(&outline, true, LineStyle::Cut, None);
                }
            }
            TypeData::Beam { width, height, shape, .. } => {
                // Section of the beam.
                let c = o + DVec2::new(width * 0.5, height * 0.5);
                for (a, b) in cc_geom::section::shape_rects(*shape, *width, *height) {
                    let r = vec![c + a, c + DVec2::new(b.x, a.y), c + b, c + DVec2::new(a.x, b.y)];
                    d.fill(r, POCHE, None);
                }
                for outline in cc_geom::section::shape_loops(*shape, *width, *height) {
                    let pts: Vec<DVec2> = outline.iter().map(|p| c + *p).collect();
                    d.polyline(&pts, true, LineStyle::Cut, None);
                }
            }
            TypeData::Rebar { .. } => {
                // A bar, drawn as a heavy line.
                d.line(o, o + DVec2::new(sample, 0.0), LineStyle::Cut, None);
            }
            TypeData::Pipe { diameter, .. } | TypeData::Conduit { diameter, .. } => {
                // A run of the pipe in plan, at least half a millimeter wide on paper.
                let h = diameter.max(0.5 * mm) * 0.5;
                let r = [
                    o + DVec2::new(0.0, -h),
                    o + DVec2::new(sample, -h),
                    o + DVec2::new(sample, h),
                    o + DVec2::new(0.0, h),
                ];
                d.fill(r.to_vec(), POCHE, None);
                d.polyline(&r, true, LineStyle::Projection, None);
            }
            TypeData::Foundation { kind, width, thickness, .. } => {
                // Section: a pad or strip its width wide, a slab as a band.
                let w = if *kind == cc_core::FoundationKind::Slab { sample } else { *width };
                let r = [o, o + DVec2::new(w, 0.0), o + DVec2::new(w, *thickness), o + DVec2::new(0.0, *thickness)];
                d.fill(r.to_vec(), POCHE, None);
                d.polyline(&r, true, LineStyle::Cut, None);
            }
            TypeData::Stair { tread, max_riser, .. } => {
                // A few steps in profile.
                let mut pts = vec![o];
                for k in 0..4 {
                    let (x, y) = (k as f64 * tread, (k + 1) as f64 * max_riser);
                    pts.push(o + DVec2::new(x, y));
                    pts.push(o + DVec2::new(x + tread, y));
                }
                d.polyline(&pts, false, LineStyle::Projection, None);
            }
            TypeData::Ramp { thickness, rules, .. } => {
                // The slab in profile, at its steepest slope.
                let rise = sample * rules.max_slope.clamp(0.0, 1.0);
                let up = DVec2::new(sample, rise);
                let t = DVec2::new(0.0, *thickness);
                let r = [o, o + up, o + up - t, o - t];
                d.fill(r.to_vec(), POCHE, None);
                d.polyline(&r, true, LineStyle::Cut, None);
            }
            TypeData::Railing { height, .. } => {
                d.line(o, o + DVec2::new(sample, 0.0), LineStyle::Thin, None);
                d.line(o + DVec2::new(0.0, *height), o + DVec2::new(sample, *height), LineStyle::Projection, None);
                for k in 0..=3 {
                    let x = sample * k as f64 / 3.0;
                    d.line(o + DVec2::new(x, 0.0), o + DVec2::new(x, *height), LineStyle::Thin, None);
                }
            }
            TypeData::Sweep { profile, material } => {
                // The profile's section, enlarged to read at the legend's scale.
                let (lo, hi) = profile
                    .iter()
                    .fold((DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)), |(lo, hi), p| {
                        (lo.min(*p), hi.max(*p))
                    });
                let size = (hi - lo).max_element();
                if size.is_finite() && size > 1e-9 {
                    let k = (10.0 * mm / size).max(1.0);
                    let at = o - DVec2::new(0.0, (hi.y - lo.y) * k / 2.0);
                    let pts: Vec<DVec2> = profile.iter().map(|p| at + (*p - lo) * k).collect();
                    let c = project.material_color(material);
                    d.fill(pts.clone(), [c[0], c[1], c[2], 255], None);
                    d.polyline(&pts, true, LineStyle::Cut, None);
                }
            }
            TypeData::Component { .. } if device.is_some() => {
                if let Some((kind, power)) = device {
                    // Wall devices with the wall along the top of the symbol; outlets low.
                    let f = crate::mep::Frame { origin: o + DVec2::new(4.0 * mm, 2.0 * mm), angle: 0.0, mm };
                    crate::mep::symbol(&mut d, &f, kind, 0.3, power, None, None);
                }
            }
            TypeData::Component { .. } => {
                if let Some((lo, hi)) = plan_bounds {
                    let shift = o - DVec2::new(lo.x, (lo.y + hi.y) / 2.0);
                    for outline in &plan {
                        let pts: Vec<DVec2> = outline.iter().map(|p| *p + shift).collect();
                        d.polyline(&pts, true, LineStyle::Projection, None);
                    }
                }
            }
            // Filtered out above: annotation types have no legend symbol.
            TypeData::Door { .. } | TypeData::Window { .. } => {}
            TypeData::Room {}
            | TypeData::Grid { .. }
            | TypeData::Dimension { .. }
            | TypeData::Text { .. }
            | TypeData::CadImport {}
            | TypeData::Mass { .. } => {}
        }
        let width = plan_bounds.map_or(0.0, |(lo, hi)| hi.x - lo.x);
        let text_x = sample.max(width).max(t.opening_size().map_or(0.0, |(w, _)| w + 0.4)) + 6.0 * mm;
        d.text(DVec2::new(text_x, y - 1.2 * mm), &t.name, 2.5);
        y -= row + if plan_bounds.is_some() { above } else { 0.0 };
    }
    d
}
