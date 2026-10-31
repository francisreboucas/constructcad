use cc_core::ElementId;
use cc_geom::ModelGeometry;
use std::collections::BTreeSet;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

/// GPU-ready triangles and edge lines.
#[derive(Debug, Clone, Default)]
pub struct SceneMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    /// Triangles of transparent solids, drawn last with blending.
    pub transparent_indices: Vec<u32>,
    pub edges: Vec<Vertex>,
}

/// The triangles and outline edges of a model's solids, without colors:
/// the costly part of a [`SceneMesh`], kept while the geometry stays, so a
/// new selection or view graphics only repaint it.
#[derive(Debug, Clone, Default)]
pub struct Tessellation {
    solids: Vec<SolidTess>,
}

#[derive(Debug, Clone)]
struct SolidTess {
    element: ElementId,
    color: [u8; 3],
    transparency: f64,
    /// The detail levels of the views that show it (a component's piece).
    detail: cc_core::component::DetailLevels,
    /// Position and normal of each vertex.
    vertices: Vec<([f32; 3], [f32; 3])>,
    /// Triangles, as indices into `vertices`.
    triangles: Vec<u32>,
    /// Outline segments (pairs of points) with their face's normal.
    edges: Vec<([f32; 3], [f32; 3])>,
}

impl Tessellation {
    pub fn new(geom: &ModelGeometry) -> Self {
        let solids = geom
            .solids
            .iter()
            .map(|solid| {
                let mut s = SolidTess {
                    element: solid.element,
                    color: solid.color,
                    transparency: solid.transparency,
                    detail: solid.detail,
                    vertices: Vec::new(),
                    triangles: Vec::new(),
                    edges: Vec::new(),
                };
                for face in &solid.faces {
                    let start = s.vertices.len() as u32;
                    let normal = face.normal.as_vec3().to_array();
                    s.vertices.extend(face.verts.iter().map(|p| (p.as_vec3().to_array(), normal)));
                    for [a, b, c] in face.triangulate() {
                        s.triangles.extend([start + a as u32, start + b as u32, start + c as u32]);
                    }
                }
                for (face, edges) in solid.faces.iter().zip(solid.feature_edges()) {
                    let normal = face.normal.as_vec3().to_array();
                    for (a, b) in edges {
                        s.edges.push((a.as_vec3().to_array(), normal));
                        s.edges.push((b.as_vec3().to_array(), normal));
                    }
                }
                s
            })
            .collect();
        Self { solids }
    }
}

impl SceneMesh {
    /// Append `solid`'s triangles in `color`, to the blended pass when transparent.
    fn add_faces(&mut self, solid: &SolidTess, color: [f32; 4]) {
        let start = self.vertices.len() as u32;
        self.vertices.extend(solid.vertices.iter().map(|&(pos, normal)| Vertex { pos, normal, color }));
        let target = if color[3] < 0.999 { &mut self.transparent_indices } else { &mut self.indices };
        target.extend(solid.triangles.iter().map(|i| start + i));
    }
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

const SELECTED: [u8; 3] = [70, 150, 255];

/// View graphics of an element's solids in 3D; `None` keeps the material's.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SolidStyle {
    pub color: Option<[u8; 3]>,
    /// 0 (opaque) to 1.
    pub transparency: Option<f64>,
    /// Faces and edges mixed with white.
    pub halftone: bool,
    /// Edge color.
    pub edges: Option<[u8; 3]>,
}

impl SceneMesh {
    /// The model's solids; `style` gives each element's view graphics
    /// (`None` hides it).
    pub fn build(
        tess: &Tessellation,
        selected: &BTreeSet<ElementId>,
        style: &mut dyn FnMut(ElementId) -> Option<SolidStyle>,
    ) -> Self {
        Self::build_at(tess, selected, style, cc_core::DetailLevel::Medium)
    }

    /// [`SceneMesh::build`] for a view at `detail`: component pieces show at
    /// the levels they are set for.
    pub fn build_at(
        tess: &Tessellation,
        selected: &BTreeSet<ElementId>,
        style: &mut dyn FnMut(ElementId) -> Option<SolidStyle>,
        detail: cc_core::DetailLevel,
    ) -> Self {
        let mut mesh = Self::default();
        let mut styles: std::collections::HashMap<ElementId, Option<SolidStyle>> = Default::default();
        for solid in tess.solids.iter().filter(|s| s.detail.shows(detail)) {
            let Some(st) = *styles.entry(solid.element).or_insert_with(|| style(solid.element)) else { continue };
            let sel = selected.contains(&solid.element);
            let mut base = st.color.unwrap_or(solid.color);
            if st.halftone {
                base = base.map(|v| (f32::from(v) + (255.0 - f32::from(v)) * 0.66).round() as u8);
            }
            let base = if sel { SELECTED } else { base };
            let transparency = st.transparency.unwrap_or(solid.transparency).clamp(0.0, 1.0) as f32;
            // Selected solids stay opaque so they read clearly.
            let alpha = if sel { 1.0 } else { (1.0 - transparency).clamp(0.08, 1.0) };
            let color = [srgb_to_linear(base[0]), srgb_to_linear(base[1]), srgb_to_linear(base[2]), alpha];
            let edge_color = match (sel, st.edges) {
                (true, _) => [0.02, 0.15, 0.6, 1.0],
                (false, Some(c)) => [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), 1.0],
                (false, None) if st.halftone => [0.45, 0.45, 0.46, 1.0],
                (false, None) => [0.02, 0.02, 0.025, 1.0],
            };
            mesh.add_faces(solid, color);
            mesh.edges.extend(solid.edges.iter().map(|&(pos, normal)| Vertex { pos, normal, color: edge_color }));
        }
        mesh
    }

    /// Every solid as its material shows it.
    pub fn build_plain(tess: &Tessellation, selected: &BTreeSet<ElementId>) -> Self {
        Self::build(tess, selected, &mut |_| Some(SolidStyle::default()))
    }

    /// The reinforcing bars (those of the selected hosts or sets highlighted)
    /// over the model's solids drawn faint.
    pub fn build_rebar(geom: &ModelGeometry, tess: &Tessellation, selected: &BTreeSet<ElementId>) -> Self {
        let mut mesh = Self::faint(tess);
        for set in geom.rebar.values() {
            let sel = selected.contains(&set.element) || selected.contains(&set.host);
            let color = if sel { [0.02, 0.15, 0.6, 1.0] } else { [0.55, 0.06, 0.04, 1.0] };
            for bar in &set.bars {
                let n = bar.points.len();
                let segments = if bar.closed { n } else { n.saturating_sub(1) };
                for i in 0..segments {
                    let (a, b) = (bar.points[i], bar.points[(i + 1) % n]);
                    let normal = [0.0, 0.0, 1.0];
                    mesh.edges.push(Vertex { pos: a.as_vec3().to_array(), normal, color });
                    mesh.edges.push(Vertex { pos: b.as_vec3().to_array(), normal, color });
                }
            }
        }
        mesh
    }

    /// The model's solids drawn faint, with no edges.
    fn faint(tess: &Tessellation) -> Self {
        let mut mesh = Self::default();
        for solid in &tess.solids {
            let base = solid.color;
            mesh.add_faces(solid, [srgb_to_linear(base[0]), srgb_to_linear(base[1]), srgb_to_linear(base[2]), 0.12]);
        }
        mesh
    }

    /// The analytical model: member lines (columns blue, beams red, the
    /// selection highlighted) and node crosses (supports green) over the
    /// model's solids drawn faint.
    pub fn build_analytical(
        tess: &Tessellation,
        model: &cc_geom::analytical::AnalyticalModel,
        selected: &BTreeSet<ElementId>,
    ) -> Self {
        let mut mesh = Self::faint(tess);
        let mut line = |a: glam::DVec3, b: glam::DVec3, color: [f32; 4]| {
            let normal = [0.0, 0.0, 1.0];
            mesh.edges.push(Vertex { pos: a.as_vec3().to_array(), normal, color });
            mesh.edges.push(Vertex { pos: b.as_vec3().to_array(), normal, color });
        };
        for m in &model.members {
            let Some((a, b)) = model.line(m) else { continue };
            let color = if selected.contains(&m.element) {
                [0.02, 0.15, 0.6, 1.0]
            } else if m.kind == cc_geom::analytical::MemberKind::Column {
                [0.05, 0.25, 0.75, 1.0]
            } else {
                [0.75, 0.08, 0.05, 1.0]
            };
            line(a, b, color);
        }
        for n in &model.nodes {
            let color = if n.support { [0.05, 0.5, 0.1, 1.0] } else { [0.02, 0.02, 0.025, 1.0] };
            let r = 0.08;
            for d in [glam::DVec3::X, glam::DVec3::Y, glam::DVec3::Z] {
                line(n.position - d * r, n.position + d * r, color);
            }
        }
        mesh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_materials_go_to_the_blended_pass() {
        let project = cc_core::from_ron(include_str!("../../../examples/casa_simples.ccad")).unwrap();
        let tess = Tessellation::new(&ModelGeometry::build(&project));
        let mesh = SceneMesh::build_plain(&tess, &BTreeSet::new());
        // Window glass (60 % transparent in the template) is blended; walls are not.
        assert!(!mesh.transparent_indices.is_empty());
        assert!(!mesh.indices.is_empty());
        let glass: Vec<&Vertex> =
            mesh.transparent_indices.iter().filter_map(|i| mesh.vertices.get(*i as usize)).collect();
        assert!(glass.iter().all(|v| (v.color[3] - 0.4).abs() < 1e-6));
        // A selected window is drawn opaque.
        let window = project.elements.values().find(|e| matches!(e.kind, cc_core::ElementKind::Window(_))).unwrap().id;
        let sel = SceneMesh::build_plain(&tess, &BTreeSet::from([window]));
        assert!(sel.transparent_indices.len() < mesh.transparent_indices.len());
        // View graphics: a hidden window leaves the mesh, an opaque override takes it out of the blended pass.
        let hidden = SceneMesh::build(&tess, &BTreeSet::new(), &mut |e| (e != window).then(SolidStyle::default));
        assert!(hidden.vertices.len() < mesh.vertices.len());
        let opaque = SolidStyle { transparency: Some(0.0), color: Some([255, 0, 0]), ..Default::default() };
        let solid = SceneMesh::build(&tess, &BTreeSet::new(), &mut |e| {
            Some(if e == window { opaque } else { SolidStyle::default() })
        });
        assert!(solid.transparent_indices.len() < mesh.transparent_indices.len());
        assert!(solid.vertices.iter().any(|v| v.color == [1.0, 0.0, 0.0, 1.0]));
    }
}
