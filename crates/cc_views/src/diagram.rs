//! The single-line diagram of a distribution board with its load schedule:
//! the supply and main breaker, the busbar and one branch per circuit with
//! its breaker, conductor and load, then a table of the circuits (from
//! `cc_geom::sizing`). Sizes in paper millimeters; `label` translates the
//! headings (keys `diagram-*`).

use crate::{Anchor, Drawing2D, LineStyle, format};
use cc_core::{ElementId, Project};
use cc_geom::ModelGeometry;
use cc_geom::sizing::CircuitSizing;
use glam::DVec2;

/// Standard breaker ratings for the main breaker, A.
const MAINS: [f64; 12] = [16.0, 20.0, 25.0, 32.0, 40.0, 50.0, 63.0, 70.0, 80.0, 100.0, 125.0, 160.0];
/// Vertical spacing of the circuit branches, mm.
const STEP: f64 = 9.0;
const ROW: f64 = 6.0;
const TEXT: f64 = 2.2;

/// Column widths of the load schedule, mm.
const COLUMNS: [f64; 9] = [16.0, 48.0, 16.0, 14.0, 22.0, 18.0, 18.0, 20.0, 16.0];

pub fn panel_diagram(
    project: &Project,
    geom: &ModelGeometry,
    panel: ElementId,
    scale: u32,
    label: &dyn Fn(&str) -> String,
) -> Drawing2D {
    let mut d = Drawing2D::default();
    let k = scale.max(1) as f64 / 1000.0;
    let at = |x: f64, y: f64| DVec2::new(x, y) * k;
    let Some(board) = project.element(panel) else { return d };
    let name = if board.mark.is_empty() { project.type_name(board.type_id).to_string() } else { board.mark.clone() };
    let mut circuits: Vec<CircuitSizing> = cc_geom::sizing::size_circuits(project, geom)
        .into_iter()
        .filter(|c| project.system(c.system).is_some_and(|s| s.panel == Some(panel)))
        .collect();
    circuits.sort_by(|a, b| {
        let code = |c: &CircuitSizing| project.system(c.system).map_or(String::new(), |s| s.code.clone());
        let (x, y) = (code(a), code(b));
        match (x.parse::<f64>(), y.parse::<f64>()) {
            (Ok(p), Ok(q)) => p.total_cmp(&q),
            _ => x.cmp(&y),
        }
    });
    let id = Some(panel);
    d.text_at(at(0.0, 0.0), format!("{} – {name}", label("diagram-title")), 3.5, Anchor::LeftBottom, 0.0);

    // Supply and main breaker.
    let power: f64 = circuits.iter().map(|c| c.power).sum();
    let current: f64 = circuits.iter().map(|c| c.current).sum();
    let main = MAINS.iter().copied().find(|m| *m >= current).unwrap_or(200.0);
    let x = 12.0;
    d.line(at(x, -6.0), at(x, -12.0), LineStyle::Cut, id);
    breaker(&mut d, at(x, -12.0), at(x, -16.0), k, id);
    d.text_at(
        at(x + 4.0, -15.0),
        format!("{} {} A", label("diagram-main"), format::number(main, 0)),
        TEXT,
        Anchor::LeftBottom,
        0.0,
    );
    d.line(at(x, -16.0), at(x, -20.0), LineStyle::Cut, id);
    // Busbar and branches.
    let n = circuits.len().max(1) as f64;
    let bottom = -20.0 - STEP * n;
    d.line(at(x, -20.0), at(x, bottom), LineStyle::Cut, id);
    for (i, c) in circuits.iter().enumerate() {
        let y = -20.0 - STEP * (i as f64 + 0.5);
        let system = project.system(c.system);
        d.line(at(x, y), at(x + 10.0, y), LineStyle::Projection, id);
        breaker(&mut d, at(x + 10.0, y), at(x + 15.0, y), k, id);
        d.line(at(x + 15.0, y), at(x + 44.0, y), LineStyle::Projection, id);
        // Conductor ticks: phase, neutral and earth.
        for t in [0.0, 1.2, 2.4] {
            let base = at(x + 27.0 + t, y);
            d.line(base + at(-0.6, -1.0), base + at(0.6, 1.0), LineStyle::Thin, id);
        }
        d.text_at(at(x + 12.5, y + 2.0), format!("{} A", format::number(c.breaker, 0)), 2.0, Anchor::CenterBottom, 0.0);
        let section = format!("{} mm²", format::number(c.conductor, 1));
        d.text_at(at(x + 33.0, y + 1.0), section, 2.0, Anchor::LeftBottom, 0.0);
        let code = system.map_or(String::new(), |s| s.code.clone());
        let title = system.map_or(String::new(), |s| s.name.clone());
        let text = format!("{code}  {title}  {} VA", format::number(c.power, 0));
        d.text_at(at(x + 46.0, y - 1.0), text, TEXT, Anchor::LeftBottom, 0.0);
    }

    // Load schedule.
    let top = bottom - 10.0;
    d.text_at(at(0.0, top + 2.0), label("diagram-loads"), 3.0, Anchor::LeftBottom, 0.0);
    let headers = [
        "diagram-circuit",
        "diagram-description",
        "diagram-voltage",
        "diagram-points",
        "diagram-power",
        "diagram-current",
        "diagram-breaker",
        "diagram-conductor",
        "diagram-drop",
    ];
    let mut rows: Vec<[String; 9]> = vec![headers.map(label)];
    for c in &circuits {
        let system = project.system(c.system);
        let points = project.system_members(c.system).filter(|e| project.mep_data(e).is_some()).count();
        rows.push([
            system.map_or(String::new(), |s| s.code.clone()),
            system.map_or(String::new(), |s| s.name.clone()),
            format::number(c.voltage, 0),
            points.to_string(),
            format::number(c.power, 0),
            format::number(c.current, 1),
            format::number(c.breaker, 0),
            format::number(c.conductor, 1),
            c.drop.map_or("–".into(), |v| format::number(v, 2)),
        ]);
    }
    rows.push([
        label("diagram-total"),
        String::new(),
        String::new(),
        String::new(),
        format::number(power, 0),
        format::number(current, 1),
        format::number(main, 0),
        String::new(),
        String::new(),
    ]);
    let width: f64 = COLUMNS.iter().sum();
    let height = ROW * rows.len() as f64;
    for r in 0..=rows.len() {
        let y = top - ROW * r as f64;
        let style = if r == 1 || r == rows.len() - 1 || r == 0 || r == rows.len() {
            LineStyle::Projection
        } else {
            LineStyle::Thin
        };
        d.line(at(0.0, y), at(width, y), style, None);
    }
    let mut cx = 0.0;
    for w in COLUMNS.iter().chain(std::iter::once(&0.0)) {
        d.line(at(cx, top), at(cx, top - height), LineStyle::Thin, None);
        cx += w;
    }
    for (r, row) in rows.iter().enumerate() {
        let y = top - ROW * (r as f64 + 1.0) + 1.8;
        let mut cx = 0.0;
        for (c, text) in row.iter().enumerate() {
            d.text_at(at(cx + 1.5, y), text.clone(), TEXT, Anchor::LeftBottom, 0.0);
            cx += COLUMNS[c];
        }
    }
    d
}

/// A breaker between `a` and `b`: a small box with a diagonal.
fn breaker(d: &mut Drawing2D, a: DVec2, b: DVec2, k: f64, id: Option<ElementId>) {
    let Some(dir) = (b - a).try_normalize() else { return };
    let n = dir.perp() * (1.2 * k);
    let pts = [a - n, b - n, b + n, a + n];
    d.polyline(&pts, true, LineStyle::Projection, id);
    d.line(a - n, b + n, LineStyle::Projection, id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Prim;
    use cc_core::{Component, Document, Element, ElementKind, MepSystem, SystemId, SystemKind};

    #[test]
    fn diagram_lists_the_circuits_of_its_board() {
        let mut doc = Document::new(Project::from_template());
        let panel = doc.edit("t", |e| {
            let level = e.project().levels[0].id;
            let find = |e: &cc_core::EditCtx, device: cc_core::DeviceKind| {
                e.project()
                    .types
                    .values()
                    .find(|t| matches!(&t.data, cc_core::TypeData::Component { def, .. } if def.mep.and_then(|m| m.device()) == Some(device)))
                    .map(|t| t.id)
                    .unwrap()
            };
            let (panel_t, outlet_t) = (find(e, cc_core::DeviceKind::Panel), find(e, cc_core::DeviceKind::Outlet));
            let add = |e: &mut cc_core::EditCtx, type_id, system| {
                let id = ElementId(e.alloc_id());
                let c = Component {
                    position: DVec2::ZERO,
                    rotation_deg: 0.0,
                    base_offset: 1.3,
                    overrides: Default::default(),
                    properties: Vec::new(),
                    system,
                    ..Default::default()
                };
                e.add_element(Element { id, level, type_id, mark: String::new(), comments: String::new(), uid: Default::default(), kind: ElementKind::Component(c), phase_created: Default::default(), phase_demolished: None  });
                id
            };
            let panel = add(e, panel_t, None);
            let circuit = SystemId(e.alloc_id());
            e.systems_mut().insert(circuit, MepSystem { panel: Some(panel), ..MepSystem::new("Tomadas", "1", SystemKind::Power) });
            add(e, outlet_t, Some(circuit));
            add(e, outlet_t, Some(circuit));
            panel
        });
        let geom = ModelGeometry::build(&doc.project);
        let d = panel_diagram(&doc.project, &geom, panel, 50, &|k| k.to_string());
        let texts: Vec<&str> = d
            .prims
            .iter()
            .filter_map(|p| match p {
                Prim::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        // 200 VA at 127 V: a 6 A breaker, 2.5 mm², a 16 A main.
        assert!(texts.iter().any(|t| t.starts_with("1  Tomadas  200")), "{texts:?}");
        assert!(texts.contains(&"diagram-main 16 A"), "{texts:?}");
        assert!(texts.contains(&format::number(2.5, 1).as_str()));
        assert!(texts.contains(&"diagram-total"));
    }
}
