//! Curtain walls in the world: the panels and mullions of a wall's grid
//! (see [`cc_core::curtain`]). Panels are flat: in an arc wall each one
//! spans the chord between its grid lines, and mullions stand square to the
//! arc on its lines.

use crate::Solid;
use crate::wall::WallGeom;
use cc_core::component::ComponentPart;
use cc_core::curtain::{self, CurtainPanel, GridAxis, Layout};
use cc_core::{Category, Element, Project, TypeData};
use glam::DVec2;

/// A panel placed in the world.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelGeom {
    /// Index of its cell in [`Layout::panels`].
    pub cell: usize,
    /// Its ends on the wall's centerline (the chord it spans), at the bottom.
    pub start: DVec2,
    pub end: DVec2,
    pub z: (f64, f64),
    /// The pieces of a component panel, in the world.
    pub parts: Vec<ComponentPart>,
}

impl PanelGeom {
    /// Unit direction along the panel and its left normal.
    pub fn frame(&self) -> (DVec2, DVec2) {
        let d = (self.end - self.start).try_normalize().unwrap_or(DVec2::X);
        (d, d.perp())
    }

    pub fn width(&self) -> f64 {
        self.start.distance(self.end)
    }
}

/// A mullion piece placed in the world: a vertical prism over `footprint`.
#[derive(Debug, Clone, PartialEq)]
pub struct MullionGeom {
    /// Index of its piece in [`Layout::mullions`].
    pub piece: usize,
    /// Plan outline, counter-clockwise.
    pub footprint: [DVec2; 4],
    pub z: (f64, f64),
}

/// The panels and mullions of a curtain wall.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CurtainGeom {
    pub layout: Layout,
    pub panels: Vec<PanelGeom>,
    pub mullions: Vec<MullionGeom>,
    /// The thickness of the type (component panels are resolved with it).
    pub thickness: f64,
}

impl CurtainGeom {
    /// The cell `(column, row)` holding plan point `p` at height `z`.
    pub fn cell_at(&self, g: &WallGeom, p: DVec2, z: f64) -> Option<(usize, usize)> {
        let (u, _) = g.local(p);
        let column = self.layout.u.windows(2).position(|w| w[0] <= u && u <= w[1])?;
        let row = self.layout.z.windows(2).position(|w| w[0] <= z && z <= w[1])?;
        Some((column, row))
    }
}

/// Lay out the curtain grid of wall `element` (type `system`) over its
/// geometry `g`: between its ends after joins, from its base to its top.
pub fn build(project: &Project, element: &Element, g: &WallGeom, system: &curtain::CurtainSystem) -> CurtainGeom {
    let edits = match &element.kind {
        cc_core::ElementKind::Wall(w) => &w.curtain,
        _ => return CurtainGeom::default(),
    };
    let layout = curtain::layout(system, edits, g.inner_range(), g.base_z, g.top_z);
    let thickness = system.thickness();
    let at = |u: f64| g.world(u, 0.0);
    let panels = layout
        .panels
        .iter()
        .enumerate()
        .map(|(i, cell)| {
            let (start, end) = (at(cell.u.0), at(cell.u.1));
            let mut panel = PanelGeom { cell: i, start, end, z: cell.z, parts: Vec::new() };
            if let CurtainPanel::Component { component } = cell.panel {
                panel.parts = component_panel(project, element, component, &panel, thickness);
            }
            panel
        })
        .collect();
    let mullions = layout
        .mullions
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let (w, d) = (m.mullion.width * 0.5, m.mullion.depth * 0.5);
            match m.axis {
                GridAxis::Vertical => {
                    let u = m.at + m.offset;
                    let (c, t) = (at(u), g.tangent(u));
                    let n = t.perp();
                    MullionGeom {
                        piece: i,
                        footprint: [c - t * w - n * d, c + t * w - n * d, c + t * w + n * d, c - t * w + n * d],
                        z: m.span,
                    }
                }
                GridAxis::Horizontal => {
                    let (a, b) = (at(m.span.0), at(m.span.1));
                    let n = (b - a).try_normalize().unwrap_or(g.dir).perp();
                    let z = m.at + m.offset;
                    MullionGeom { piece: i, footprint: [a - n * d, b - n * d, b + n * d, a + n * d], z: (z - w, z + w) }
                }
            }
        })
        .collect();
    CurtainGeom { layout, panels, mullions, thickness }
}

/// The pieces of component type `component` filling a panel: a door or
/// window centered in it (its frame's x from the panel's middle), other
/// components from its lower left corner; `Width` and `Height` parameters
/// set to the panel's size, `WallThickness` to the wall's.
fn component_panel(
    project: &Project,
    element: &Element,
    component: cc_core::ElementTypeId,
    panel: &PanelGeom,
    thickness: f64,
) -> Vec<ComponentPart> {
    let Some(ty) = project.element_type(component) else { return Vec::new() };
    let TypeData::Component { def, .. } = &ty.data else { return Vec::new() };
    let (width, height) = (panel.width(), panel.z.1 - panel.z.0);
    let mut instance = cc_core::Component::default();
    for p in &def.params {
        let size = match p.name.to_ascii_lowercase().as_str() {
            "width" => width,
            "height" => height,
            _ => continue,
        };
        instance.overrides.insert(p.name.clone(), size);
    }
    let extra = [(cc_core::component::WALL_THICKNESS, thickness)];
    let parts = crate::resolve_instance(ty, def, element, Some(&instance), &extra);
    let (d, n) = panel.frame();
    let origin = if def.opening.is_some() { (panel.start + panel.end) * 0.5 } else { panel.start };
    let world = |q: DVec2| origin + d * q.x + n * q.y;
    parts
        .into_iter()
        .map(|mut p| {
            p.footprint = p.footprint.iter().map(|q| world(*q)).collect();
            p.z0 += panel.z.0;
            p.z1 += panel.z.0;
            for face in &mut p.faces {
                for v in face.iter_mut() {
                    *v = world(v.truncate()).extend(v.z + panel.z.0);
                }
            }
            p
        })
        .filter(|p| p.footprint.iter().all(|q| q.is_finite()) && p.z0.is_finite() && p.z1.is_finite())
        .collect()
}

/// The solids of a curtain wall: one per material of its panels and mullions.
pub fn solids(project: &Project, element: &Element, c: &CurtainGeom) -> Vec<Solid> {
    let mut by_material: Vec<(String, Solid)> = Vec::new();
    let solid_for = |by_material: &mut Vec<(String, Solid)>, material: &str| -> usize {
        if let Some(i) = by_material.iter().position(|(m, _)| m == material) {
            return i;
        }
        by_material.push((material.to_string(), Solid::of_material(element.id, Category::Wall, project, material)));
        by_material.len() - 1
    };
    for p in &c.panels {
        let Some(cell) = c.layout.panels.get(p.cell) else { continue };
        match &cell.panel {
            CurtainPanel::Glass { thickness, material } | CurtainPanel::Solid { thickness, material } => {
                let i = solid_for(&mut by_material, material);
                let (d, n) = p.frame();
                let t = thickness * 0.5;
                if let Some((_, s)) = by_material.get_mut(i) {
                    s.add_box(p.start, d, n, 0.0, p.width(), -t, t, p.z.0, p.z.1);
                }
            }
            CurtainPanel::Empty => {}
            CurtainPanel::Component { .. } => {
                for part in p.parts.iter().filter(|q| !q.plan_only && !q.void) {
                    let i = solid_for(&mut by_material, &part.material);
                    let Some((_, s)) = by_material.get_mut(i) else { continue };
                    if part.faces.is_empty() {
                        s.add_prism(&part.footprint, |_| part.z0, |_| part.z1);
                    } else {
                        s.add_polygons(part.faces.clone());
                    }
                }
            }
        }
    }
    for m in &c.mullions {
        let Some(piece) = c.layout.mullions.get(m.piece) else { continue };
        let i = solid_for(&mut by_material, &piece.mullion.material);
        if let Some((_, s)) = by_material.get_mut(i) {
            s.add_prism(&m.footprint, |_| m.z.0, |_| m.z.1);
        }
    }
    by_material.into_iter().map(|(_, s)| s).filter(|s| !s.faces.is_empty()).collect()
}

/// Volume of the glass and solid panels and of the mullions.
pub fn volume(c: &CurtainGeom) -> f64 {
    let panels: f64 = c
        .panels
        .iter()
        .filter_map(|p| Some((p, c.layout.panels.get(p.cell)?)))
        .map(|(p, cell)| p.width() * (p.z.1 - p.z.0) * cell.panel.thickness())
        .sum();
    let mullions: f64 = c.mullions.iter().map(|m| crate::polygon::area(&m.footprint) * (m.z.1 - m.z.0)).sum();
    panels + mullions
}
