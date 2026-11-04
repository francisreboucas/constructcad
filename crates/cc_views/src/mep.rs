//! Building services in plans: pipes and conduits as bands in their
//! system's color with diameter, slope and circuit tags, and electrical
//! devices as the usual plan symbols (sizes in paper millimeters).
//!
//! Symbols follow common Brazilian practice: an outlet is a triangle,
//! hollow when low, half filled at medium height and filled when high; a
//! ceiling light is a circle with its circuit above its power; a switch is
//! a small filled circle; a board is a rectangle with a filled half.

use crate::{Anchor, Drawing2D, LineStyle, format};
use cc_core::{DeviceKind, Element, ElementKind, LevelId, MepData, MepDiscipline, Project};
use cc_geom::ModelGeometry;
use glam::DVec2;

/// Ink of filled symbols.
const INK: [u8; 4] = [40, 40, 40, 255];
/// Outlets below this height are low, below [`MEDIUM`] medium, else high.
const LOW: f64 = 0.6;
const MEDIUM: f64 = 1.6;
/// Conduits lower than this are in the floor (drawn dashed).
const IN_FLOOR: f64 = 0.3;

/// Pipes or conduits of `level` and, in electrical plans, the device symbols.
pub(crate) fn services(
    d: &mut Drawing2D,
    project: &Project,
    geom: &ModelGeometry,
    level: LevelId,
    cut_height: f64,
    discipline: MepDiscipline,
    scale: u32,
) {
    let mm = scale as f64 / 1000.0;
    for e in project.elements.values() {
        match (&e.kind, discipline) {
            (ElementKind::Pipe(_), MepDiscipline::Plumbing) | (ElementKind::Conduit(_), MepDiscipline::Electrical) => {
                // The level's runs and the risers of other levels passing through it.
                if let Some(s) = project.segment_at(e, level) {
                    segment(d, project, geom, e, &s, cut_height, mm);
                }
            }
            (ElementKind::Component(c), MepDiscipline::Electrical) if e.level == level => {
                if let Some(MepData::Device { device, power }) = project.mep_data(e) {
                    let circuit = c.system.and_then(|s| project.system(s)).map(|s| s.label().to_string());
                    let frame = Frame { origin: c.position, angle: c.rotation_deg.to_radians(), mm };
                    symbol(d, &frame, device, c.base_offset, power, circuit.as_deref(), Some(e.id));
                }
            }
            _ => {}
        }
    }
    let category = match discipline {
        MepDiscipline::Plumbing => cc_core::Category::Pipe,
        MepDiscipline::Electrical => cc_core::Category::Conduit,
    };
    for f in geom.mep.fittings.iter().filter(|f| f.level == level && f.category == category) {
        fitting(d, project, f, mm);
    }
}

/// A fitting in plan: a dot in the system's color on pipes; on conduits, a
/// box where three or more runs meet (elbows and couplings show as the runs).
fn fitting(d: &mut Drawing2D, project: &Project, f: &cc_geom::mep_net::Fitting, mm: f64) {
    use cc_geom::mep_net::FittingKind as K;
    let id = f.runs.first().copied();
    let c = DVec2::new(f.point.x, f.point.y);
    match (f.category, f.kind) {
        (cc_core::Category::Conduit, K::Tee | K::Cross) => {
            let h = 1.5 * mm;
            let square =
                vec![c + DVec2::new(-h, -h), c + DVec2::new(h, -h), c + DVec2::new(h, h), c + DVec2::new(-h, h)];
            d.fill(square.clone(), [255, 255, 255, 255], id);
            d.polyline(&square, true, LineStyle::Projection, id);
        }
        (cc_core::Category::Conduit, _) => {}
        _ => {
            let color = f.system.and_then(|s| project.system(s)).map_or([128, 128, 128], |s| s.color());
            let pts = circle(c, f.diameter().max(0.8 * mm) * 0.75);
            d.fill(pts.clone(), [color[0], color[1], color[2], 255], id);
            d.polyline(&pts, true, LineStyle::Thin, id);
        }
    }
}

/// A band of `width` from `a` to `b` in `color`, with a fine outline (dashed when hidden).
fn band(d: &mut Drawing2D, a: DVec2, b: DVec2, width: f64, color: [u8; 3], style: LineStyle, e: &Element) {
    let Some(dir) = (b - a).try_normalize() else { return };
    let n = dir.perp() * (width * 0.5);
    let pts = vec![a - n, b - n, b + n, a + n];
    d.fill(pts.clone(), [color[0], color[1], color[2], 255], Some(e.id));
    let outline = if style == LineStyle::Hidden { LineStyle::Hidden } else { LineStyle::Thin };
    d.polyline(&pts, true, outline, Some(e.id));
}

fn circle(c: DVec2, r: f64) -> Vec<DVec2> {
    (0..24).map(|i| c + DVec2::from_angle(i as f64 / 24.0 * std::f64::consts::TAU) * r).collect()
}

/// Tag text of a segment: its systems' codes and its diameter.
fn tag(project: &Project, e: &Element, diameter: f64) -> String {
    let codes: Vec<&str> = e.systems().iter().filter_map(|s| project.system(*s)).map(|s| s.label()).collect();
    let size = if format::imperial() {
        format!("Ø{}\"", format::inches(diameter))
    } else {
        format!("Ø{}", format::number(diameter * 1000.0, 0))
    };
    match e.kind {
        // Conduits carry circuit numbers; pipes their system code.
        ElementKind::Conduit(_) if !codes.is_empty() => format!("{size} ({})", codes.join(", ")),
        _ if !codes.is_empty() => format!("{} {size}", codes.join(", ")),
        _ => size,
    }
}

/// Run `e` with its elevations `s` above the plan's level.
fn segment(
    d: &mut Drawing2D,
    project: &Project,
    geom: &ModelGeometry,
    e: &Element,
    s: &cc_core::MepSegment,
    cut_height: f64,
    mm: f64,
) {
    let Some(g) = geom.segments.get(&e.id) else { return };
    let color = project.system_color(e);
    let conduit = matches!(e.kind, ElementKind::Conduit(_));
    // Pipes above the cut plane are dashed; conduits only when they run in the floor.
    let above = !conduit && s.start_offset.min(s.end_offset) > cut_height;
    let style = if above || (conduit && s.start_offset.max(s.end_offset) < IN_FLOOR) {
        LineStyle::Hidden
    } else {
        LineStyle::Projection
    };
    let text = tag(project, e, g.diameter);
    if s.is_vertical() {
        // A riser: a circle (at least 2 mm across) with its tag beside it.
        let r = (g.diameter * 0.5).max(mm);
        let pts = circle(s.start, r);
        d.fill(pts.clone(), [color[0], color[1], color[2], 255], Some(e.id));
        d.polyline(&pts, true, style, Some(e.id));
        d.tag_text(s.start + DVec2::new(r + mm, r + mm), text, 2.0, Anchor::LeftBottom, 0.0, Some(e.id));
        return;
    }
    // At least 0.8 mm on paper, so the system's color reads.
    let width = g.diameter.max(0.8 * mm);
    if conduit && style == LineStyle::Hidden {
        d.line(s.start, s.end, style, Some(e.id));
    } else {
        band(d, s.start, s.end, width, color, style, e);
    }
    // Tags along runs at least 15 mm long on paper, read from below or the right.
    if s.plan_length() < 15.0 * mm {
        return;
    }
    let dir = (s.end - s.start) / s.plan_length();
    let (dir, angle) = {
        let a = dir.to_angle();
        if dir.x < -1e-9 || (dir.x.abs() <= 1e-9 && dir.y < 0.0) { (-dir, a + std::f64::consts::PI) } else { (dir, a) }
    };
    let mid = (s.start + s.end) * 0.5 + dir.perp() * (width * 0.5 + 0.8 * mm);
    let slope = s.slope();
    let text = if matches!(e.kind, ElementKind::Pipe(_)) && slope.abs() > 1e-4 {
        format!("{text}  i = {}", format::slope(slope.abs()))
    } else {
        text
    };
    d.tag_text(mid, text, 2.0, Anchor::CenterBottom, angle, Some(e.id));
}

/// Where a symbol is drawn: the device's insertion point and rotation.
/// Wall devices stand out towards their local -Y.
pub(crate) struct Frame {
    pub origin: DVec2,
    pub angle: f64,
    /// One paper millimeter in model meters.
    pub mm: f64,
}

impl Frame {
    /// A point `x`, `y` paper millimeters from the origin in the device's frame.
    fn at(&self, x: f64, y: f64) -> DVec2 {
        self.origin + DVec2::from_angle(self.angle).rotate(DVec2::new(x, y) * self.mm)
    }
}

/// The plan symbol of an electrical device mounted `height` above its level.
pub(crate) fn symbol(
    d: &mut Drawing2D,
    f: &Frame,
    device: DeviceKind,
    height: f64,
    power: f64,
    circuit: Option<&str>,
    id: Option<cc_core::ElementId>,
) {
    let style = LineStyle::Projection;
    let label = |d: &mut Drawing2D, at: DVec2, text: &str| d.tag_text(at, text, 2.0, Anchor::Center, 0.0, id);
    match device {
        DeviceKind::Outlet | DeviceKind::DedicatedOutlet => {
            // Base on the wall, apex into the room.
            let (l, r, apex) = (f.at(-1.75, 0.0), f.at(1.75, 0.0), f.at(0.0, -3.0));
            let tri = vec![l, r, apex];
            if height >= MEDIUM || device == DeviceKind::DedicatedOutlet {
                d.fill(tri.clone(), INK, id);
            } else if height >= LOW {
                d.fill(vec![l, f.at(0.0, 0.0), apex], INK, id);
            }
            d.polyline(&tri, true, style, id);
            let text = match (device, circuit) {
                (DeviceKind::DedicatedOutlet, Some(c)) => format!("{c} · {} VA", format::number(power, 0)),
                (DeviceKind::DedicatedOutlet, None) => format!("{} VA", format::number(power, 0)),
                (_, Some(c)) => c.to_string(),
                (_, None) => String::new(),
            };
            if !text.is_empty() {
                label(d, f.at(0.0, -5.0), &text);
            }
        }
        DeviceKind::Switch => {
            let pts = circle(f.at(0.0, -1.5), 1.0 * f.mm);
            d.fill(pts.clone(), INK, id);
            d.polyline(&pts, true, style, id);
            if let Some(c) = circuit {
                label(d, f.at(0.0, -4.0), c);
            }
        }
        DeviceKind::CeilingLight => {
            let r = 3.0;
            d.polyline(&circle(f.origin, r * f.mm), true, style, id);
            d.line(f.at(-r, 0.0), f.at(r, 0.0), LineStyle::Thin, id);
            if let Some(c) = circuit {
                label(d, f.origin + DVec2::new(0.0, 1.4 * f.mm), c);
            }
            label(d, f.origin - DVec2::new(0.0, 1.4 * f.mm), &format::number(power, 0));
        }
        DeviceKind::WallLight => {
            // A half circle against the wall.
            let r = 2.5;
            let pts: Vec<DVec2> = (0..=12)
                .map(|i| {
                    let t = std::f64::consts::PI * (1.0 + i as f64 / 12.0);
                    f.at(r * t.cos(), r * t.sin())
                })
                .collect();
            d.polyline(&pts, true, style, id);
            if let Some(c) = circuit {
                label(d, f.at(0.0, -4.5), c);
            }
        }
        DeviceKind::Panel => {
            let (w, h) = (3.0, 2.5);
            let rect = vec![f.at(-w, 0.0), f.at(w, 0.0), f.at(w, -h), f.at(-w, -h)];
            d.fill(vec![f.at(-w, 0.0), f.at(w, 0.0), f.at(-w, -h)], INK, id);
            d.polyline(&rect, true, LineStyle::Cut, id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Prim;
    use cc_core::{Category, Document, ElementId, MepSegment, SystemKind, TypeData, ViewKind};

    fn system(p: &Project, kind: SystemKind) -> cc_core::SystemId {
        *p.systems.iter().find(|(_, s)| s.kind == kind).unwrap().0
    }

    #[test]
    fn plumbing_plan_draws_pipes_with_tags_and_hides_them_elsewhere() {
        let mut doc = Document::new(Project::from_template());
        let level = doc.project.levels[0].id;
        let sewer = system(&doc.project, SystemKind::Sanitary);
        let pipe_t = doc
            .project
            .types_of(Category::Pipe)
            .find(|t| matches!(t.data, TypeData::Pipe { diameter, .. } if (diameter - 0.1).abs() < 1e-9))
            .unwrap()
            .id;
        let (pipe, riser) = doc.edit("t", |e| {
            let mut run = MepSegment::new(DVec2::ZERO, DVec2::new(5.0, 0.0), -0.3);
            run.end_offset = -0.35;
            run.systems = vec![sewer];
            let mut up = MepSegment::new(DVec2::new(5.0, 0.0), DVec2::new(5.0, 0.0), -0.35);
            up.end_offset = 3.0;
            up.systems = vec![sewer];
            let add = |e: &mut cc_core::EditCtx, kind| {
                let id = ElementId(e.alloc_id());
                e.add_element(cc_core::Element {
                    id,
                    level,
                    type_id: pipe_t,
                    mark: String::new(),
                    comments: String::new(),
                    uid: Default::default(),
                    kind,
                    phase_created: Default::default(),
                    phase_demolished: None,
                });
                id
            };
            (add(e, ElementKind::Pipe(run)), add(e, ElementKind::Pipe(up)))
        });
        let geom = ModelGeometry::build(&doc.project);
        // The run and the elbow where the riser leaves it.
        assert_eq!(geom.solids_of(pipe).count(), 2);
        let plan = |mep| {
            cc_core::View::new(
                cc_core::ViewId(999),
                "",
                50,
                ViewKind::FloorPlan {
                    level,
                    cut_height: 1.2,
                    structural: false,
                    structure_tags: false,
                    mep,
                    underlay: Default::default(),
                },
            )
        };
        let d = crate::generate(&doc.project, &geom, &plan(Some(MepDiscipline::Plumbing))).unwrap();
        let texts: Vec<&str> = d
            .prims
            .iter()
            .filter_map(|p| match p {
                Prim::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.iter().any(|t| t.starts_with("ES Ø100") && t.contains("i = 1")), "{texts:?}");
        assert!(texts.contains(&"ES Ø100"), "{texts:?}");
        assert_eq!(d.hit_test(DVec2::new(2.5, 0.0), 0.01), Some(pipe));
        assert!(d.hit_test(DVec2::new(5.0, 0.0), 0.01).is_some());
        let colored = SystemKind::Sanitary.default_color();
        assert!(d.prims.iter().any(|p| matches!(p, Prim::Fill { color, element: Some(id), .. }
            if *id == riser && color[..3] == colored)));
        // Architectural and electrical plans leave pipes out.
        for mep in [None, Some(MepDiscipline::Electrical)] {
            let d = crate::generate(&doc.project, &geom, &plan(mep)).unwrap();
            assert!(d.prims.iter().all(|p| p.element() != Some(pipe)));
        }
        // The riser reaches the first floor: its plan shows it, without the ground floor's run.
        let first = doc.project.levels[1].id;
        let upstairs = cc_core::View {
            kind: ViewKind::FloorPlan {
                level: first,
                cut_height: 1.2,
                structural: false,
                structure_tags: false,
                mep: Some(MepDiscipline::Plumbing),
                underlay: Default::default(),
            },
            ..plan(None)
        };
        let d = crate::generate(&doc.project, &geom, &upstairs).unwrap();
        assert_eq!(d.hit_test(DVec2::new(5.0, 0.0), 0.01), Some(riser));
        assert!(d.prims.iter().all(|p| p.element() != Some(pipe)));
    }

    #[test]
    fn outlet_symbol_fills_with_height() {
        let fills = |height: f64| {
            let mut d = Drawing2D::default();
            let f = Frame { origin: DVec2::ZERO, angle: 0.0, mm: 0.05 };
            symbol(&mut d, &f, DeviceKind::Outlet, height, 100.0, Some("3"), None);
            let texts = d.prims.iter().filter(|p| matches!(p, Prim::Text { text, .. } if text == "3")).count();
            assert_eq!(texts, 1);
            d.prims.iter().filter(|p| matches!(p, Prim::Fill { .. })).count()
        };
        assert_eq!(fills(0.3), 0);
        assert_eq!(fills(1.1), 1);
        assert_eq!(fills(2.0), 1);
    }
}
