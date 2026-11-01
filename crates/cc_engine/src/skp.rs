//! SketchUp (.skp) models as component types: the geometry of each placed
//! component or group becomes a type of fixed mesh pieces (one per
//! material), placed by an element at the instance's position and plan
//! rotation. Whatever else the instance's transform holds (tilt, scale,
//! mirror) is baked into the type, so placements that differ only by a
//! move or a plan rotation share one type.

use cc_core::{ComponentDef, ComponentItem, ComponentKind, ComponentShape};
use cc_skp::{Definition, EntityId, SkpModel};
use glam::{DMat3, DMat4, DVec2, DVec3};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Inches (SketchUp's unit) to meters.
const INCH: f64 = 0.0254;
/// Deepest nesting of instances followed (a crafted file can nest forever).
const MAX_DEPTH: usize = 32;
/// Faces written over the whole import.
pub const MAX_FACES: usize = 2_000_000;
/// Distinct types an import may make.
const MAX_TYPES: usize = 20_000;

/// A component type to add: its name and definition.
#[derive(Debug, Clone)]
pub struct SkpType {
    pub name: String,
    pub def: ComponentDef,
}

/// An element placing `types[ty]`, in meters and degrees, before the
/// import's own move and rotation.
#[derive(Debug, Clone, Copy)]
pub struct SkpPlacement {
    pub ty: usize,
    pub position: DVec2,
    pub z: f64,
    pub rotation_deg: f64,
}

/// What a file becomes: types, placements and the materials they use.
#[derive(Debug, Clone, Default)]
pub struct Converted {
    pub types: Vec<SkpType>,
    pub placements: Vec<SkpPlacement>,
    /// Materials (name, color) the pieces use.
    pub materials: Vec<(String, [u8; 3])>,
    /// Hidden instances and faces (or on hidden layers) left out.
    pub hidden: usize,
    pub faces: usize,
}

#[derive(Debug)]
pub enum SkpError {
    /// More faces than [`MAX_FACES`], or more types than the import makes.
    TooLarge,
}

/// Same type: the placed definition, what is left of its transform after
/// the plan move and rotation (rounded), and the material it inherits.
type TypeKey = (EntityId, [i64; 9], Option<EntityId>);

struct Converter<'a> {
    model: &'a SkpModel,
    hidden_layers: HashSet<&'a str>,
    out: Converted,
    keys: HashMap<TypeKey, usize>,
    materials: HashMap<EntityId, Option<usize>>,
}

/// An instance's 13-value matrix (row-major 3x3, translation, scale term) in inches.
fn matrix(m: &[f64]) -> DMat4 {
    let [a, b, c, d, e, f, g, h, i, tx, ty, tz] = match m.get(..12) {
        Some(&[a, b, c, d, e, f, g, h, i, tx, ty, tz]) => [a, b, c, d, e, f, g, h, i, tx, ty, tz],
        _ => return DMat4::IDENTITY,
    };
    let s = m.get(12).copied().filter(|s| s.is_finite() && *s != 0.0).unwrap_or(1.0);
    let out = DMat4::from_cols(
        DVec3::new(a, d, g).extend(0.0) * s,
        DVec3::new(b, e, h).extend(0.0) * s,
        DVec3::new(c, f, i).extend(0.0) * s,
        (DVec3::new(tx, ty, tz) * s).extend(1.0),
    );
    if out.is_finite() { out } else { DMat4::IDENTITY }
}

/// The vertex ids around a face loop (`orientation` 1 starts at the edge's first vertex).
fn loop_vertices(def: &Definition, lp: &[cc_skp::CoEdge]) -> Vec<EntityId> {
    let mut out: Vec<EntityId> = Vec::with_capacity(lp.len());
    for c in lp {
        let Some(edge) = def.edges.get(&c.edge_id) else { continue };
        let id = if c.orientation == 1 { edge.v1_id } else { edge.v2_id };
        if out.last() != Some(&id) {
            out.push(id);
        }
    }
    if out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
    out
}

/// Faces of one piece, with vertices shared by position.
#[derive(Default)]
struct MeshBuild {
    vertices: Vec<[f64; 3]>,
    index: HashMap<[i64; 3], u32>,
    faces: Vec<Vec<u32>>,
}

impl MeshBuild {
    fn add(&mut self, face: &[DVec3]) {
        let mut f: Vec<u32> = Vec::with_capacity(face.len());
        for p in face {
            // Snap to 1/100 mm so shared corners share a vertex.
            let key = [(p.x * 1e5).round() as i64, (p.y * 1e5).round() as i64, (p.z * 1e5).round() as i64];
            let i = *self.index.entry(key).or_insert_with(|| {
                self.vertices.push([p.x, p.y, p.z]);
                (self.vertices.len() - 1) as u32
            });
            if f.last() != Some(&i) && f.first() != Some(&i) {
                f.push(i);
            }
        }
        if f.len() >= 3 {
            self.faces.push(f);
        }
    }
}

impl<'a> Converter<'a> {
    fn new(model: &'a SkpModel) -> Self {
        let hidden_layers = model.layers.iter().filter(|l| l.hidden).map(|l| l.name.as_str()).collect();
        Self { model, hidden_layers, out: Converted::default(), keys: HashMap::new(), materials: HashMap::new() }
    }

    fn layer_hidden(&self, layer: &str) -> bool {
        !layer.is_empty() && self.hidden_layers.contains(layer)
    }

    /// The index in `out.materials` of a file material (`None`: the default color).
    fn material(&mut self, id: Option<EntityId>) -> Option<usize> {
        let id = id?;
        if let Some(m) = self.materials.get(&id) {
            return *m;
        }
        let found = self.model.material_by_id(id).map(|m| {
            let name = m.name.trim();
            let name = if name.is_empty() { format!("SketchUp {id}") } else { name.to_string() };
            let color = [m.color[0], m.color[1], m.color[2]];
            match self.out.materials.iter().position(|(n, _)| *n == name) {
                Some(i) => i,
                None => {
                    self.out.materials.push((name, color));
                    self.out.materials.len() - 1
                }
            }
        });
        self.materials.insert(id, found);
        found
    }

    /// Adds the faces of `def` and of what it places to `pieces`, through
    /// `to_type` (definition inches to the type frame in meters); unpainted
    /// faces take `inherited`.
    fn gather(
        &mut self,
        def: &Definition,
        to_type: DMat4,
        inherited: Option<EntityId>,
        depth: usize,
        pieces: &mut BTreeMap<Option<usize>, MeshBuild>,
    ) -> Result<(), SkpError> {
        if depth > MAX_DEPTH {
            return Ok(());
        }
        self.faces(def, to_type, inherited, pieces)?;
        for instance in &def.instances {
            if instance.hidden || self.layer_hidden(&instance.layer) {
                self.out.hidden += 1;
                continue;
            }
            let Some(child) = instance.ref_idx.and_then(|id| self.model.definitions.get(&id)) else { continue };
            if child.is_image {
                continue;
            }
            let inherited = instance.material_id.or(inherited);
            self.gather(child, to_type * matrix(&instance.matrix), inherited, depth + 1, pieces)?;
        }
        Ok(())
    }

    /// Adds the faces of `def` itself to `pieces` (see [`Self::gather`]).
    fn faces(
        &mut self,
        def: &Definition,
        to_type: DMat4,
        inherited: Option<EntityId>,
        pieces: &mut BTreeMap<Option<usize>, MeshBuild>,
    ) -> Result<(), SkpError> {
        let mirrored = to_type.determinant() < 0.0;
        for face in def.faces.values() {
            if face.hidden || self.layer_hidden(&face.layer) {
                self.out.hidden += 1;
                continue;
            }
            let loops: Vec<Vec<EntityId>> =
                face.loops.iter().map(|l| loop_vertices(def, l)).filter(|l| l.len() >= 3).collect();
            let Some(outer) = loops.first() else { continue };
            let point = |id: &EntityId| def.vertices.get(id).map(|v| DVec3::new(v.x, v.y, v.z));
            let polygons: Vec<Vec<DVec3>> = if loops.len() == 1 {
                vec![outer.iter().filter_map(point).collect()]
            } else {
                let outer_pts: Vec<[f64; 3]> = outer.iter().filter_map(point).map(|p| p.to_array()).collect();
                let normal = face.normal.or_else(|| cc_skp::compute_face_normal(&outer_pts)).unwrap_or([0.0, 0.0, 1.0]);
                let n = DVec3::from_array(normal);
                cc_skp::triangulate_face_3d(&def.vertices, &loops, normal)
                    .iter()
                    .filter_map(|t| {
                        let (a, b, c) = (point(&t[0])?, point(&t[1])?, point(&t[2])?);
                        Some(if (b - a).cross(c - a).dot(n) >= 0.0 { vec![a, b, c] } else { vec![a, c, b] })
                    })
                    .collect()
            };
            let material = self.material(face.material_id.or(inherited));
            let mesh = pieces.entry(material).or_default();
            for polygon in polygons {
                let mut placed: Vec<DVec3> = polygon.iter().map(|p| to_type.transform_point3(*p)).collect();
                if placed.len() < 3 || !placed.iter().all(|p| p.is_finite()) {
                    continue;
                }
                if mirrored {
                    placed.reverse();
                }
                self.out.faces += 1;
                if self.out.faces > MAX_FACES {
                    return Err(SkpError::TooLarge);
                }
                mesh.add(&placed);
            }
        }
        Ok(())
    }

    /// A component definition of `pieces` (none when nothing was drawn).
    fn definition(&self, name: &str, pieces: BTreeMap<Option<usize>, MeshBuild>) -> Option<ComponentDef> {
        let items: Vec<ComponentItem> = pieces
            .into_iter()
            .filter(|(_, m)| !m.faces.is_empty())
            .map(|(material, m)| {
                let material = material.and_then(|i| self.out.materials.get(i)).map(|(n, _)| n.clone());
                ComponentItem {
                    name: material.clone().unwrap_or_else(|| name.to_string()),
                    shape: ComponentShape::Mesh { vertices: m.vertices, faces: m.faces },
                    material: material.unwrap_or_default(),
                    in_plan: true,
                    void: false,
                    plan_only: false,
                    detail: Default::default(),
                }
            })
            .collect();
        (!items.is_empty()).then(|| ComponentDef { kind: ComponentKind::Generic, items, ..Default::default() })
    }

    fn add_type(&mut self, name: String, def: ComponentDef) -> Result<usize, SkpError> {
        if self.out.types.len() >= MAX_TYPES {
            return Err(SkpError::TooLarge);
        }
        self.out.types.push(SkpType { name, def });
        Ok(self.out.types.len() - 1)
    }

    /// Each instance of the root (a type per definition and leftover
    /// transform) and the root's own faces (one type at the origin).
    fn model(mut self, loose_name: &str) -> Result<Converted, SkpError> {
        let root = self.model.root();
        let mut loose = BTreeMap::new();
        self.faces(root, DMat4::from_scale(DVec3::splat(INCH)), None, &mut loose)?;
        if let Some(def) = self.definition(loose_name, loose) {
            let ty = self.add_type(loose_name.to_string(), def)?;
            self.out.placements.push(SkpPlacement { ty, position: DVec2::ZERO, z: 0.0, rotation_deg: 0.0 });
        }
        for instance in &root.instances {
            if instance.hidden || self.layer_hidden(&instance.layer) {
                self.out.hidden += 1;
                continue;
            }
            let Some(def) = instance.ref_idx.and_then(|id| self.model.definitions.get(&id)) else { continue };
            if def.is_image {
                continue;
            }
            let world = matrix(&instance.matrix);
            let linear = DMat3::from_mat4(world);
            // Plan rotation from the projected x axis (or the y axis for a vertical one).
            let x = linear.x_axis.truncate();
            let angle = if x.length() > 1e-9 {
                x.y.atan2(x.x)
            } else {
                let y = linear.y_axis.truncate();
                if y.length() > 1e-9 { y.y.atan2(y.x) - std::f64::consts::FRAC_PI_2 } else { 0.0 }
            };
            let residual = DMat3::from_rotation_z(-angle) * linear;
            let quantum = |v: f64| (v * 1e6).round() as i64;
            let r = residual.to_cols_array().map(quantum);
            let key = (def.id, r, instance.material_id);
            let ty = match self.keys.get(&key) {
                Some(&ty) => ty,
                None => {
                    let mut pieces = BTreeMap::new();
                    let to_type = DMat4::from_scale(DVec3::splat(INCH)) * DMat4::from_mat3(residual);
                    self.gather(def, to_type, instance.material_id, 1, &mut pieces)?;
                    let name = match (def.is_group, instance.name.trim()) {
                        (true, n) if !n.is_empty() => n.to_string(),
                        _ if def.name.trim().is_empty() => "Group".to_string(),
                        _ => def.name.trim().to_string(),
                    };
                    let Some(component) = self.definition(&name, pieces) else { continue };
                    let ty = self.add_type(name, component)?;
                    self.keys.insert(key, ty);
                    ty
                }
            };
            let t = world.w_axis.truncate() * INCH;
            self.out.placements.push(SkpPlacement {
                ty,
                position: t.truncate(),
                z: t.z,
                rotation_deg: angle.to_degrees(),
            });
        }
        Ok(self.out)
    }

    /// The whole model as one type, its origin at the file's origin.
    fn single(mut self, name: &str) -> Result<Converted, SkpError> {
        let mut pieces = BTreeMap::new();
        self.gather(self.model.root(), DMat4::from_scale(DVec3::splat(INCH)), None, 0, &mut pieces)?;
        if let Some(def) = self.definition(name, pieces) {
            self.add_type(name.to_string(), def)?;
        }
        Ok(self.out)
    }
}

/// Every placed component and group of `model` as an element of a
/// component type; the root's loose faces make one more type, `loose_name`.
pub fn convert(model: &SkpModel, loose_name: &str) -> Result<Converted, SkpError> {
    Converter::new(model).model(loose_name)
}

/// The whole of `model` as one component type named `name`.
pub fn convert_single(model: &SkpModel, name: &str) -> Result<Converted, SkpError> {
    Converter::new(model).single(name)
}
