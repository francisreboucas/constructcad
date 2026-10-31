//! View generation for ConstructCAD.
//!
//! Each [`cc_core::View`] is a query over the model. [`generate`] produces a
//! resolution-independent [`Drawing2D`] for graphical views; [`schedule`]
//! computes tables.

mod annotate;
pub mod cad;
pub mod component_view;
mod cut;
mod diagram;
mod drawing;
pub mod dxf;
pub mod export;
pub mod fonts;
pub mod format;
pub mod graphics;
pub mod hatch;
mod joins;
mod legend;
mod mep;
pub mod notes;
mod plan;
mod projected;
mod schedule;
mod sheet;

pub use annotate::{DimensionLayout, HIGHLIGHT_ONLY, dimension_layout};
pub use component_view::{ComponentView, ComponentViewKind, component_view};
pub use drawing::{Anchor, Drawing2D, LineStyle, Pen, Prim, ResolvedPen, TagHandle, halftone};
pub use legend::legend;
pub use plan::{is_structural, plan, plan_of};
pub use projected::{Frame, axonometric, elevation, section};
pub use schedule::{Cell, Row, Table, field_value, schedule, schedule_where, to_csv};
pub use sheet::{MARGIN, PlacedViewport, Sheet, TITLE_H, TITLE_W, TitleLabels, compose as compose_sheet};

use cc_core::{Project, View, ViewKind};
use cc_geom::ModelGeometry;

/// Drawing for any graphical view (`None` for schedules and sheets), with
/// the headings of diagrams left as their translation keys.
pub fn generate(project: &Project, geom: &ModelGeometry, view: &View) -> Option<Drawing2D> {
    generate_labeled(project, geom, view, &|key| key.to_string())
}

/// Drawing for any graphical view, with the view's graphics applied;
/// `label` translates the headings drawn by the view itself (diagrams).
pub fn generate_labeled(
    project: &Project,
    geom: &ModelGeometry,
    view: &View,
    label: &dyn Fn(&str) -> String,
) -> Option<Drawing2D> {
    generate_temporary(project, geom, view, label, None)
}

/// Drawing for any graphical view with its graphics and, when given, the
/// elements temporarily hidden or isolated in it.
pub fn generate_temporary(
    project: &Project,
    geom: &ModelGeometry,
    view: &View,
    label: &dyn Fn(&str) -> String,
    temporary: Option<&cc_core::TemporaryVisibility>,
) -> Option<Drawing2D> {
    let mut d = match &view.kind {
        ViewKind::FloorPlan { level, cut_height, .. } | ViewKind::CeilingPlan { level, cut_height } => {
            let ceiling = matches!(view.kind, ViewKind::CeilingPlan { .. });
            let (structural, tags, services) = match view.kind {
                ViewKind::FloorPlan { structural, structure_tags, mep, .. } => {
                    (structural, structural || structure_tags, mep)
                }
                _ => (false, false, None),
            };
            let mut d = plan_of(project, geom, *level, *cut_height, ceiling, structural, view.scale);
            joins::merge(project, &mut d);
            if let Some(discipline) = services {
                // The architecture stays in the background of a services plan.
                fade(&mut d);
                mep::services(&mut d, project, geom, *level, *cut_height, discipline, view.scale);
            }
            annotate::plan_annotations(&mut d, project, geom, *level, ceiling, structural, view);
            if tags {
                annotate::structure_tags(&mut d, project, geom, *level, view.scale, &view.tag_offsets);
            }
            d
        }
        ViewKind::Elevation { direction } => elevation(project, geom, *direction),
        ViewKind::Section { start, end, depth } => {
            let mut d = section(project, geom, *start, *end, *depth, view.scale);
            joins::merge(project, &mut d);
            d
        }
        ViewKind::ThreeD { camera, analytical: true, .. } => projected::analytical(project, geom, camera),
        ViewKind::ThreeD { camera, rebar: true, .. } => projected::rebar(geom, camera),
        ViewKind::ThreeD { camera, .. } => axonometric(project, geom, camera),
        ViewKind::Legend { types } => legend(project, types, view.scale),
        ViewKind::Diagram { panel } => diagram::panel_diagram(project, geom, *panel, view.scale, label),
        ViewKind::Schedule { .. } | ViewKind::Sheet { .. } => return None,
    };
    // Imported CAD drawings, under everything (not in 3D views or diagrams).
    if hosts_cad(&view.kind) {
        cad::underlays(&mut d, project, view.id, view.scale);
    }
    annotate::dimensions(&mut d, project, view.id, view.scale);
    notes::notes(&mut d, project, view.id, view.scale);
    graphics::apply(&mut d, &mut graphics::Resolver::new(project, geom, view, temporary), view.scale);
    Some(d)
}

/// Whether a view can hold imported CAD drawings: plans, ceiling plans,
/// elevations, sections and legends (2D views with a drawing plane).
pub fn hosts_cad(kind: &ViewKind) -> bool {
    matches!(
        kind,
        ViewKind::FloorPlan { .. }
            | ViewKind::CeilingPlan { .. }
            | ViewKind::Elevation { .. }
            | ViewKind::Section { .. }
            | ViewKind::Legend { .. }
    )
}

/// Draw everything so far faint: lines in the faint pen (dashes kept),
/// fills mixed two thirds with white.
fn fade(d: &mut Drawing2D) {
    for prim in &mut d.prims {
        match prim {
            Prim::Line { style, .. }
                if matches!(style, LineStyle::Cut | LineStyle::Projection | LineStyle::Thin | LineStyle::Hatch) =>
            {
                *style = LineStyle::Faint;
            }
            Prim::Fill { color, .. } => {
                for c in color.iter_mut().take(3) {
                    *c = (f64::from(*c) + (255.0 - f64::from(*c)) * 0.66).round() as u8;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::{Category, ViewCategory};
    use glam::DVec2;

    fn sample() -> (Project, ModelGeometry) {
        let project = cc_core::from_ron(include_str!("../../../examples/casa_simples.ccad")).unwrap();
        let geom = ModelGeometry::build(&project);
        (project, geom)
    }

    #[test]
    fn ground_plan_shows_rooms_grids_and_dimensions() {
        let (project, geom) = sample();
        let level = project.levels[0].id;
        let view = project.views_of(ViewCategory::FloorPlan).find(|v| v.level() == Some(level)).unwrap();
        let d = generate(&project, &geom, view).unwrap();
        let texts: Vec<&str> = d
            .prims
            .iter()
            .filter_map(|p| match p {
                Prim::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        for expected in ["Sala", "Quarto", "1", "A", "B"] {
            assert!(texts.contains(&expected), "missing {expected:?} in {texts:?}");
        }
        // The overall dimension of the south wall.
        assert!(texts.contains(&format::number(9.0, 2).as_str()), "{texts:?}");
        assert!(d.prims.iter().any(|p| matches!(p, Prim::Line { style: LineStyle::Center, .. })));
        // Door and window marks are tagged.
        assert!(texts.iter().filter(|t| **t == "2").count() >= 2);

        // The upper plan has the grids but neither the rooms nor the dimensions.
        let upper =
            project.views_of(ViewCategory::FloorPlan).find(|v| v.level() == Some(project.levels[1].id)).unwrap();
        let d = generate(&project, &geom, upper).unwrap();
        let has = |s: &str| d.prims.iter().any(|p| matches!(p, Prim::Text { text, .. } if text == s));
        assert!(has("A") && !has("Sala") && !has(&format::number(9.0, 2)));
    }

    #[test]
    fn room_schedule_lists_enclosed_rooms() {
        let (project, geom) = sample();
        let table = schedule(&project, &geom, Category::Room, &Default::default());
        assert_eq!(table.rows.len(), 2);
        // Interior of the left room: 5 m minus half walls, by 6 m minus half walls.
        let area = (5.0 - 0.075 - 0.075) * (6.0 - 0.15);
        assert_eq!(table.rows[0].cells[3], Cell::Text(format::area(area)));
    }

    /// Zoom to fit a plan frames the model, not the elevation markers around it
    /// nor a section's far clip line.
    #[test]
    fn plans_fit_to_the_model() {
        let (project, geom) = sample();
        let plan = project.views.values().find(|v| matches!(v.kind, ViewKind::FloorPlan { .. })).unwrap();
        let d = generate(&project, &geom, plan).unwrap();
        let (lo, hi) = d.bounds().unwrap();
        let (flo, fhi) = d.fit_bounds().unwrap();
        let (mlo, mhi) = geom.bounds().unwrap();
        // The markers stand outside on every side; the fitted box holds the model.
        assert!((flo - lo).min_element() > 0.5 && (hi - fhi).min_element() > 0.5, "{lo} {hi} / {flo} {fhi}");
        assert!(flo.x <= mlo.x && flo.y <= mlo.y && fhi.x >= mhi.x && fhi.y >= mhi.y);
        // Other views fit to everything drawn.
        let elevation = project.views.values().find(|v| matches!(v.kind, ViewKind::Elevation { .. })).unwrap();
        let d = generate(&project, &geom, elevation).unwrap();
        assert_eq!(d.fit_bounds(), d.bounds());
    }

    #[test]
    fn sheet_places_views_at_scale_and_exports() {
        let (project, geom) = sample();
        let sheet = project.views_of(ViewCategory::Sheet).next().unwrap().clone();
        let ViewKind::Sheet { viewports, .. } = &sheet.kind else { panic!("not a sheet") };
        assert!(!viewports.is_empty());
        let labels = TitleLabels { project: "Projeto".into(), ..Default::default() };
        let composed = compose_sheet(&project, &sheet, &labels, |id| {
            project.view(id).and_then(|v| generate(&project, &geom, v)).map(std::rc::Rc::new)
        })
        .unwrap();
        assert_eq!(composed.paper, DVec2::new(420.0, 297.0));
        assert_eq!(composed.viewports.len(), viewports.len());
        // Each viewport keeps its view's extent at the view scale.
        for placed in &composed.viewports {
            let view = project.view(placed.view).unwrap();
            let (lo, hi) = generate(&project, &geom, view).unwrap().bounds().unwrap();
            let k = 1000.0 / view.scale as f64;
            assert!(((hi - lo) * 0.5 * k - placed.half).abs().max_element() < 1e-9);
        }
        let texts: Vec<&str> = composed
            .drawing
            .prims
            .iter()
            .filter_map(|p| match p {
                Prim::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&"Projeto"));
        assert!(texts.contains(&project.info.name.as_str()));
        // Everything lies on the paper.
        let (lo, hi) = composed.drawing.bounds().unwrap();
        assert!(lo.min_element() >= 0.0 && hi.x <= 420.0 && hi.y <= 297.0, "{lo} {hi}");
        let svg = export::to_svg(&composed.drawing, composed.paper);
        assert!(svg.matches("<line ").count() > 100);
        let pdf = export::to_pdf(&composed.drawing, composed.paper);
        assert!(pdf.starts_with(b"%PDF") && pdf.ends_with(b"%%EOF\n"));
    }

    #[test]
    fn placed_component_appears_in_3d_plan_schedule_and_legend() {
        let mut project = Project::from_template();
        let level = project.levels[0].id;
        let table = project
            .types_of(Category::Component)
            .find(|t| matches!(&t.data, cc_core::TypeData::Component { def, .. } if def.items.len() == 5))
            .unwrap()
            .id;
        let id = cc_core::ElementId(project.alloc_id());
        let component = cc_core::Component {
            position: DVec2::new(2.0, 3.0),
            rotation_deg: 90.0,
            base_offset: 0.0,
            overrides: Default::default(),
            properties: Vec::new(),
            system: None,
        };
        project.elements.insert(
            id,
            cc_core::Element {
                id,
                level,
                type_id: table,
                mark: "M1".into(),
                comments: String::new(),
                kind: cc_core::ElementKind::Component(component),
                uid: Default::default(),
                phase_created: Default::default(),
                phase_demolished: None,
            },
        );
        assert_eq!(project.repair(), 0);
        let geom = ModelGeometry::build(&project);
        assert_eq!(geom.solids_of(id).count(), 5);
        // Rotated 90°: the 1.20 m width runs along Y.
        let (lo, hi) =
            geom.solids_of(id).filter_map(|s| s.bounds()).fold(
                (glam::DVec3::splat(f64::INFINITY), glam::DVec3::splat(f64::NEG_INFINITY)),
                |(lo, hi), (a, b)| (lo.min(a), hi.max(b)),
            );
        assert!((hi - lo - glam::DVec3::new(0.8, 1.2, 0.75)).abs().max_element() < 1e-9, "{lo} {hi}");
        // In plan only the top (marked for plans) is drawn: one closed outline of 4 lines.
        let d = plan::plan(&project, &geom, level, 1.2, false, 50);
        let lines = d.prims.iter().filter(|p| matches!(p, Prim::Line { element: Some(e), .. } if *e == id)).count();
        assert_eq!(lines, 4);
        assert_eq!(d.hit_test(DVec2::new(2.4, 3.0), 0.05), Some(id));
        // Schedule extents come from the pieces, in the component's own frame.
        let table = schedule(&project, &geom, Category::Component, &Default::default());
        let row = table.rows.iter().find(|r| r.element == id).unwrap();
        let cells: Vec<String> = row.cells.iter().map(|c| c.raw().to_string()).collect();
        assert!(cells.contains(&format::number(1.2, 2)), "{cells:?}");
        assert!(cells.contains(&format::number(0.8, 2)), "{cells:?}");
        // The legend draws every library component.
        let types: Vec<_> = project.types_of(Category::Component).map(|t| t.id).collect();
        let legend = legend(&project, &types, 50);
        assert!(legend.prims.iter().filter(|p| matches!(p, Prim::Line { .. })).count() > 20);
    }

    #[test]
    fn analytical_view_draws_members_and_nodes() {
        let project = cc_core::from_ron(include_str!("../../cc_ifc/tests/fixtures/structure.ccad")).unwrap();
        let geom = ModelGeometry::build(&project);
        let model = cc_geom::analytical::analytical_model(&project, &geom);
        let mut view = project.views_of(ViewCategory::ThreeD).next().unwrap().clone();
        let ViewKind::ThreeD { analytical, .. } = &mut view.kind else { panic!("3D view") };
        *analytical = true;
        let d = generate(&project, &geom, &view).unwrap();
        // One line per member (tagged with its element), one marker per node.
        let members = d.prims.iter().filter(|p| matches!(p, crate::Prim::Line { element: Some(_), .. })).count();
        assert_eq!(members, model.members.len());
        assert!(model.nodes.iter().any(|n| n.support));
        // The physical model is not drawn.
        assert!(!d.prims.iter().any(|p| matches!(p, crate::Prim::Fill { .. })));
    }

    #[test]
    fn structural_plans_show_only_the_structure_tagged() {
        let project = cc_core::from_ron(include_str!("../../cc_ifc/tests/fixtures/structure.ccad")).unwrap();
        let geom = ModelGeometry::build(&project);
        let mut view = project.views_of(ViewCategory::FloorPlan).next().unwrap().clone();
        let elements = |d: &Drawing2D| -> std::collections::BTreeSet<cc_core::ElementId> {
            d.prims
                .iter()
                .filter_map(|p| match p {
                    crate::Prim::Line { element, .. } | crate::Prim::Fill { element, .. } => *element,
                    _ => None,
                })
                .collect()
        };
        let ordinary = generate(&project, &geom, &view).unwrap();
        let ViewKind::FloorPlan { structural, .. } = &mut view.kind else { panic!("floor plan") };
        *structural = true;
        let d = generate(&project, &geom, &view).unwrap();
        // The (non-structural) walls go; beams and columns stay.
        let walls: Vec<_> = project.walls().map(|(e, _)| e.id).collect();
        assert!(walls.iter().all(|w| elements(&ordinary).contains(w)));
        assert!(walls.iter().all(|w| !elements(&d).contains(w)));
        let beams = project.elements_of(Category::Beam).filter(|e| e.level == view.level().unwrap()).count();
        let texts: Vec<&str> = d
            .prims
            .iter()
            .filter_map(|p| match p {
                crate::Prim::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        // Beams and columns are tagged with their (numbered) mark and section in centimeters.
        let tagged = |prefix: char, section: &str| {
            texts.iter().filter(|t| t.starts_with(prefix) && t.ends_with(&format!(" {section}"))).count()
        };
        assert!(tagged('V', "15×40") >= beams.min(1), "{texts:?}");
        assert!(tagged('P', "20×20") >= 1, "{texts:?}");
    }
}
