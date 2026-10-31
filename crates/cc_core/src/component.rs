//! Parametric components: user-defined objects (furniture, fixtures,
//! casework…) whose solids are sized by named parameters.
//!
//! A [`ComponentDef`] lists numeric parameters and solid pieces. Each piece
//! is a box, a cylinder, a plan outline extruded upwards, or a profile of
//! lines and arcs drawn on the plan, front or side plane and extruded across
//! it or revolved about an axis ([`crate::profile`]); its coordinates and
//! sizes are [`crate::expr`] expressions over the parameters, in the
//! component's local frame: origin at the insertion point, X to the right,
//! Y to the back, Z up, meters. A piece marked `void` is cut out of the
//! solid pieces it overlaps. Types of the same component share the definition and
//! differ in parameter values; parameters marked `instance` can be
//! overridden per element.
//!
//! Pieces brought in from other tools (IFC import) may be fixed polyhedral
//! meshes instead, and the definition then remembers the IFC class the
//! object had, so it is written back as the same class.

use crate::MepData;
use crate::expr::{self, ExprError};
pub use crate::profile::{ProfilePoint, SketchPlane};
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What a component represents (used for IFC mapping and grouping).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ComponentKind {
    Furniture,
    Casework,
    Plumbing,
    Lighting,
    /// Outlets, switches and distribution boards.
    Electrical,
    #[default]
    Generic,
    /// Doors: with a [`HostOpening`], their types are of the door category.
    Door,
    /// Windows: with a [`HostOpening`], their types are of the window category.
    Window,
}

impl ComponentKind {
    pub const ALL: [ComponentKind; 8] = [
        ComponentKind::Furniture,
        ComponentKind::Casework,
        ComponentKind::Plumbing,
        ComponentKind::Lighting,
        ComponentKind::Electrical,
        ComponentKind::Generic,
        ComponentKind::Door,
        ComponentKind::Window,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ComponentKind::Furniture => "comp-kind-furniture",
            ComponentKind::Casework => "comp-kind-casework",
            ComponentKind::Plumbing => "comp-kind-plumbing",
            ComponentKind::Lighting => "comp-kind-lighting",
            ComponentKind::Electrical => "comp-kind-electrical",
            ComponentKind::Generic => "comp-kind-generic",
            ComponentKind::Door => "comp-kind-door",
            ComponentKind::Window => "comp-kind-window",
        }
    }
}

/// Unit of a parameter value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ParamKind {
    /// Meters.
    #[default]
    Length,
    /// Degrees.
    Angle,
    Number,
}

impl ParamKind {
    pub const ALL: [ParamKind; 3] = [ParamKind::Length, ParamKind::Angle, ParamKind::Number];

    pub fn key(self) -> &'static str {
        match self {
            ParamKind::Length => "param-kind-length",
            ParamKind::Angle => "param-kind-angle",
            ParamKind::Number => "param-kind-number",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentParam {
    /// Name used in expressions (see [`expr::is_valid_name`]).
    pub name: String,
    /// Value of this type.
    pub value: f64,
    #[serde(default)]
    pub kind: ParamKind,
    /// Elements may override the value.
    #[serde(default)]
    pub instance: bool,
}

/// Geometry of a piece; every field is an expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComponentShape {
    /// Box from the corner (`x`, `y`, `z`) with the given sizes.
    Box { x: String, y: String, z: String, width: String, depth: String, height: String },
    /// Vertical cylinder with its base centered at (`x`, `y`, `z`).
    Cylinder { x: String, y: String, z: String, radius: String, height: String },
    /// Closed profile in plan extruded upwards from `z`.
    Extrusion { points: Vec<(String, String)>, z: String, height: String },
    /// Closed profile of lines and arcs on `plane`, placed at `offset` on the
    /// axis normal to the plane (Z, Y or X) and extruded `depth` along it.
    Profile { plane: SketchPlane, points: Vec<ProfilePoint>, offset: String, depth: String },
    /// Closed profile on `plane` (placed at `offset` on its normal axis) turned
    /// `angle` degrees about the line `u = axis` of the plane (vertical in
    /// the front and side planes).
    Revolution { plane: SketchPlane, points: Vec<ProfilePoint>, offset: String, axis: String, angle: String },
    /// Closed `profile` (u to the left of the path, v along the plane's
    /// normal) carried along the open `path` drawn on `plane`, which stands
    /// at `offset` on its normal axis; mitered at the path's corners.
    Sweep { plane: SketchPlane, path: Vec<ProfilePoint>, offset: String, profile: Vec<ProfilePoint> },
    /// The solid joining the closed `bottom` outline on `plane` at `offset`
    /// to the closed `top` outline `depth` farther along its normal.
    Blend { plane: SketchPlane, bottom: Vec<ProfilePoint>, top: Vec<ProfilePoint>, offset: String, depth: String },
    /// Fixed polyhedral surface (not parametric): vertices in the component
    /// frame and faces as counter-clockwise (seen from outside) index loops.
    Mesh { vertices: Vec<[f64; 3]>, faces: Vec<Vec<u32>> },
}

impl ComponentShape {
    /// Every expression of the shape, with a field label.
    pub fn expressions(&self) -> Vec<(&'static str, &String)> {
        match self {
            ComponentShape::Box { x, y, z, width, depth, height } => {
                vec![("x", x), ("y", y), ("z", z), ("width", width), ("depth", depth), ("height", height)]
            }
            ComponentShape::Cylinder { x, y, z, radius, height } => {
                vec![("x", x), ("y", y), ("z", z), ("radius", radius), ("height", height)]
            }
            ComponentShape::Extrusion { points, z, height } => {
                let mut out: Vec<(&'static str, &String)> =
                    points.iter().flat_map(|(x, y)| [("x", x), ("y", y)]).collect();
                out.push(("z", z));
                out.push(("height", height));
                out
            }
            ComponentShape::Profile { points, offset, depth, .. } => {
                let mut out = profile_expressions(points);
                out.push(("offset", offset));
                out.push(("depth", depth));
                out
            }
            ComponentShape::Revolution { points, offset, axis, angle, .. } => {
                let mut out = profile_expressions(points);
                out.push(("offset", offset));
                out.push(("axis", axis));
                out.push(("angle", angle));
                out
            }
            ComponentShape::Sweep { path, offset, profile, .. } => {
                let mut out = profile_expressions(path);
                out.extend(profile_expressions(profile));
                out.push(("offset", offset));
                out
            }
            ComponentShape::Blend { bottom, top, offset, depth, .. } => {
                let mut out = profile_expressions(bottom);
                out.extend(profile_expressions(top));
                out.push(("offset", offset));
                out.push(("depth", depth));
                out
            }
            ComponentShape::Mesh { .. } => Vec::new(),
        }
    }

    pub fn expressions_mut(&mut self) -> Vec<&mut String> {
        match self {
            ComponentShape::Box { x, y, z, width, depth, height } => vec![x, y, z, width, depth, height],
            ComponentShape::Cylinder { x, y, z, radius, height } => vec![x, y, z, radius, height],
            ComponentShape::Extrusion { points, z, height } => {
                let mut out: Vec<&mut String> = points.iter_mut().flat_map(|(x, y)| [x, y]).collect();
                out.push(z);
                out.push(height);
                out
            }
            ComponentShape::Profile { points, offset, depth, .. } => {
                let mut out = profile_expressions_mut(points);
                out.push(offset);
                out.push(depth);
                out
            }
            ComponentShape::Revolution { points, offset, axis, angle, .. } => {
                let mut out = profile_expressions_mut(points);
                out.extend([offset, axis, angle]);
                out
            }
            ComponentShape::Sweep { path, offset, profile, .. } => {
                let mut out = profile_expressions_mut(path);
                out.extend(profile_expressions_mut(profile));
                out.push(offset);
                out
            }
            ComponentShape::Blend { bottom, top, offset, depth, .. } => {
                let mut out = profile_expressions_mut(bottom);
                out.extend(profile_expressions_mut(top));
                out.extend([offset, depth]);
                out
            }
            ComponentShape::Mesh { .. } => Vec::new(),
        }
    }

    /// Number of outline points of a profile (0 for boxes, cylinders and meshes).
    pub fn point_count(&self) -> usize {
        match self {
            ComponentShape::Extrusion { points, .. } => points.len(),
            ComponentShape::Profile { points, .. } | ComponentShape::Revolution { points, .. } => points.len(),
            ComponentShape::Sweep { path, profile, .. } => path.len() + profile.len(),
            ComponentShape::Blend { bottom, top, .. } => bottom.len() + top.len(),
            _ => 0,
        }
    }
}

/// The expressions of a profile's points (an arc's radius only when set).
fn profile_expressions(points: &[ProfilePoint]) -> Vec<(&'static str, &String)> {
    points.iter().flat_map(ProfilePoint::expressions).collect()
}

fn profile_expressions_mut(points: &mut [ProfilePoint]) -> Vec<&mut String> {
    points.iter_mut().flat_map(ProfilePoint::expressions_mut).collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentItem {
    pub name: String,
    pub shape: ComponentShape,
    /// Project material; empty for the default color.
    #[serde(default)]
    pub material: String,
    /// The piece's outline is drawn in plans.
    #[serde(default = "yes")]
    pub in_plan: bool,
    /// The piece is cut out of the solid pieces it overlaps instead of adding to them.
    #[serde(default, skip_serializing_if = "is_false")]
    pub void: bool,
    /// A plan symbol only (a door's swing): drawn in plans, not a solid.
    #[serde(default, skip_serializing_if = "is_false")]
    pub plan_only: bool,
    /// The detail levels of the views that show the piece (all by default).
    #[serde(default, skip_serializing_if = "DetailLevels::is_all")]
    pub detail: DetailLevels,
}

/// The detail levels of the views a piece shows in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct DetailLevels {
    pub coarse: bool,
    pub medium: bool,
    pub fine: bool,
}

impl Default for DetailLevels {
    fn default() -> Self {
        Self::ALL
    }
}

impl DetailLevels {
    pub const ALL: Self = Self { coarse: true, medium: true, fine: true };

    pub fn is_all(&self) -> bool {
        *self == Self::ALL
    }

    /// Whether a view at `level` shows the piece.
    pub fn shows(&self, level: crate::DetailLevel) -> bool {
        match level {
            crate::DetailLevel::Coarse => self.coarse,
            crate::DetailLevel::Medium => self.medium,
            crate::DetailLevel::Fine => self.fine,
        }
    }

    pub fn set(&mut self, level: crate::DetailLevel, on: bool) {
        match level {
            crate::DetailLevel::Coarse => self.coarse = on,
            crate::DetailLevel::Medium => self.medium = on,
            crate::DetailLevel::Fine => self.fine = on,
        }
    }
}

fn yes() -> bool {
    true
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Which way a run leaves a connector, in the component's frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ConnectorDirection {
    #[default]
    Down,
    Up,
    /// Towards -Y.
    Front,
    /// Towards +Y (usually the wall).
    Back,
    /// Towards -X.
    Left,
    /// Towards +X.
    Right,
}

impl ConnectorDirection {
    pub const ALL: [ConnectorDirection; 6] = [
        ConnectorDirection::Down,
        ConnectorDirection::Up,
        ConnectorDirection::Front,
        ConnectorDirection::Back,
        ConnectorDirection::Left,
        ConnectorDirection::Right,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ConnectorDirection::Down => "conn-dir-down",
            ConnectorDirection::Up => "conn-dir-up",
            ConnectorDirection::Front => "conn-dir-front",
            ConnectorDirection::Back => "conn-dir-back",
            ConnectorDirection::Left => "conn-dir-left",
            ConnectorDirection::Right => "conn-dir-right",
        }
    }

    /// Unit vector in the component's frame.
    pub fn vector(self) -> DVec3 {
        match self {
            ConnectorDirection::Down => -DVec3::Z,
            ConnectorDirection::Up => DVec3::Z,
            ConnectorDirection::Front => -DVec3::Y,
            ConnectorDirection::Back => DVec3::Y,
            ConnectorDirection::Left => -DVec3::X,
            ConnectorDirection::Right => DVec3::X,
        }
    }

    /// Whether runs leave it vertically.
    pub fn is_vertical(self) -> bool {
        matches!(self, ConnectorDirection::Down | ConnectorDirection::Up)
    }
}

/// A point where a pipe or conduit connects to the component: a toilet has
/// a water inlet and a sewer outlet, an outlet its electrical connection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentConnector {
    pub name: String,
    /// The kind of system it connects to.
    pub system: crate::SystemKind,
    /// Position in the component's frame (expressions).
    pub x: String,
    pub y: String,
    pub z: String,
    #[serde(default)]
    pub direction: ConnectorDirection,
    /// Outside diameter of the run it takes, meters (an expression); empty for unset.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub diameter: String,
}

impl ComponentConnector {
    pub fn expressions_mut(&mut self) -> [&mut String; 4] {
        [&mut self.x, &mut self.y, &mut self.z, &mut self.diameter]
    }
}

/// A connector resolved in the component's frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedConnector {
    pub index: usize,
    pub name: String,
    pub system: crate::SystemKind,
    pub point: DVec3,
    pub direction: ConnectorDirection,
    pub diameter: Option<f64>,
}

/// Name of the host wall's thickness in the expressions of door and window
/// components (see [`crate::openings`]).
pub const WALL_THICKNESS: &str = "WallThickness";

/// The opening a door or window component cuts in its host wall: `width`
/// wide, centered on the component, `height` tall from the sill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostOpening {
    /// Expression of the parameters.
    pub width: String,
    /// Expression of the parameters.
    pub height: String,
    /// Sill height of new elements (windows), an expression; empty for 0.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sill: String,
    /// Plans draw the pieces marked `in_plan` instead of the category's
    /// symbol (a door's swing, a window's sash lines).
    #[serde(default, skip_serializing_if = "is_false")]
    pub own_symbol: bool,
}

impl HostOpening {
    pub fn expressions_mut(&mut self) -> [&mut String; 3] {
        [&mut self.width, &mut self.height, &mut self.sill]
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ComponentDef {
    #[serde(default)]
    pub kind: ComponentKind,
    #[serde(default)]
    pub params: Vec<ComponentParam>,
    #[serde(default)]
    pub items: Vec<ComponentItem>,
    /// IFC class of objects imported from IFC, written back on export.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ifc: Option<IfcClass>,
    /// Plumbing fixture or electrical device data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mep: Option<crate::MepData>,
    /// The opening a door or window cuts in its wall.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opening: Option<Box<HostOpening>>,
    /// Where pipes and conduits connect to it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connectors: Vec<ComponentConnector>,
    /// Named planes whose positions expressions read (see [`crate::component_ref`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ref_planes: Vec<crate::component_ref::RefPlane>,
    /// Named lines on the sketch planes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ref_lines: Vec<crate::component_ref::RefLine>,
    /// Dimensions between reference planes, labeled by the parameters that drive them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dimensions: Vec<crate::component_ref::RefDimension>,
    /// Planes kept halfway between two others (EQ).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub equals: Vec<crate::component_ref::RefEqual>,
}

/// An IFC entity class with its predefined and object types.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct IfcClass {
    /// Entity name as written in IFC files, e.g. `IFCPIPESEGMENT`.
    pub entity: String,
    /// `PredefinedType` enumeration value (without dots); empty if unset.
    #[serde(default)]
    pub predefined: String,
    /// `ObjectType` (the user-defined type when `predefined` is `USERDEFINED`).
    #[serde(default)]
    pub object_type: String,
    /// Property and quantity sets of the IFC type object.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<crate::properties::PropertySet>,
}

/// A resolved piece: a prism of `footprint` between `z0` and `z1`, in the
/// component's local frame. Meshes, profiles on the front and side planes
/// and revolutions carry their faces, with the convex hull of the vertices
/// as footprint and their height range.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentPart {
    pub item: usize,
    pub footprint: Vec<DVec2>,
    pub z0: f64,
    pub z1: f64,
    pub material: String,
    pub in_plan: bool,
    /// Cut out of the other pieces (see `cc_geom::resolve_component`).
    pub void: bool,
    /// Drawn in plans only (see [`ComponentItem::plan_only`]).
    pub plan_only: bool,
    /// The detail levels of the views that show it.
    pub detail: DetailLevels,
    /// Faces of a piece that is not a vertical prism (empty for prisms).
    pub faces: Vec<Vec<DVec3>>,
}

/// Why a piece could not be resolved.
#[derive(Debug, Clone, PartialEq)]
pub enum PartError {
    Expr {
        field: &'static str,
        error: ExprError,
    },
    /// A size is zero or negative, or a profile has fewer than 3 points.
    Degenerate,
}

impl std::fmt::Display for PartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PartError::Expr { field, error } => write!(f, "{field}: {error}"),
            PartError::Degenerate => write!(f, "zero or negative size"),
        }
    }
}

/// Segments of a cylinder's polygon.
const CYLINDER_SEGMENTS: usize = 24;

impl ComponentDef {
    pub fn param(&self, name: &str) -> Option<&ComponentParam> {
        self.params.iter().find(|p| p.name == name)
    }

    /// Parameter values with instance `overrides` applied (only to instance
    /// parameters; non-finite overrides are ignored).
    pub fn values(&self, overrides: &BTreeMap<String, f64>) -> BTreeMap<String, f64> {
        self.params
            .iter()
            .map(|p| {
                let v = overrides.get(&p.name).copied().filter(|v| p.instance && v.is_finite()).unwrap_or(p.value);
                (p.name.clone(), v)
            })
            .collect()
    }

    /// Width and height of the opening a door or window cuts (`None` for
    /// other components, or when they do not evaluate to positive sizes).
    pub fn opening_size(&self) -> Option<(f64, f64)> {
        let o = self.opening.as_ref()?;
        let scope = self.scope(&BTreeMap::new(), &[]);
        let lookup = |name: &str| scope.get(name);
        let (w, h) = (expr::eval(&o.width, &lookup).ok()?, expr::eval(&o.height, &lookup).ok()?);
        (w > 0.0 && h > 0.0).then_some((w, h))
    }

    /// The connectors whose expressions evaluate, in the component's frame.
    pub fn resolve_connectors(&self, overrides: &BTreeMap<String, f64>) -> Vec<ResolvedConnector> {
        let scope = self.scope(overrides, &[]);
        let lookup = |name: &str| scope.get(name);
        self.connectors
            .iter()
            .enumerate()
            .filter_map(|(index, c)| {
                let v = |text: &str| expr::eval(text, &lookup).ok();
                let point = DVec3::new(v(&c.x)?, v(&c.y)?, v(&c.z)?);
                let diameter = if c.diameter.trim().is_empty() { None } else { v(&c.diameter).filter(|d| *d > 0.0) };
                Some(ResolvedConnector {
                    index,
                    name: c.name.clone(),
                    system: c.system,
                    point,
                    direction: c.direction,
                    diameter,
                })
            })
            .collect()
    }

    /// Sill height of new doors and windows of this definition (0 when unset or invalid).
    pub fn default_sill(&self) -> f64 {
        let Some(o) = self.opening.as_ref().filter(|o| !o.sill.trim().is_empty()) else { return 0.0 };
        let scope = self.scope(&BTreeMap::new(), &[]);
        expr::eval(&o.sill, &|name: &str| scope.get(name)).map_or(0.0, |v| v.max(0.0))
    }

    /// The category of the types of this definition: doors and windows
    /// (with an opening) have their own.
    pub fn category(&self) -> crate::Category {
        match (self.kind, &self.opening) {
            (ComponentKind::Door, Some(_)) => crate::Category::Door,
            (ComponentKind::Window, Some(_)) => crate::Category::Window,
            _ => crate::Category::Component,
        }
    }

    /// Resolve every piece; pieces that fail are reported with their index.
    pub fn resolve(&self, overrides: &BTreeMap<String, f64>) -> (Vec<ComponentPart>, Vec<(usize, PartError)>) {
        self.resolve_in(overrides, &[])
    }

    /// Resolve every piece with `extra` names defined too (a door's
    /// [`WALL_THICKNESS`]); parameters of the same name win. Pieces also
    /// read the reference planes.
    pub fn resolve_in(
        &self,
        overrides: &BTreeMap<String, f64>,
        extra: &[(&str, f64)],
    ) -> (Vec<ComponentPart>, Vec<(usize, PartError)>) {
        let scope = self.scope(overrides, extra);
        let lookup = |name: &str| scope.get(name);
        let mut parts = Vec::new();
        let mut errors = Vec::new();
        for (i, item) in self.items.iter().enumerate() {
            let resolved = match &item.shape {
                ComponentShape::Mesh { vertices, faces } => resolve_mesh(vertices, faces),
                ComponentShape::Profile { .. }
                | ComponentShape::Revolution { .. }
                | ComponentShape::Sweep { .. }
                | ComponentShape::Blend { .. } => resolve_profile(&item.shape, &lookup),
                shape => resolve_shape(shape, &lookup).map(|(footprint, z0, z1)| (footprint, z0, z1, Vec::new())),
            };
            match resolved {
                Ok((footprint, z0, z1, faces)) => parts.push(ComponentPart {
                    item: i,
                    footprint,
                    z0,
                    z1,
                    material: item.material.clone(),
                    in_plan: item.in_plan || item.plan_only,
                    void: item.void && !item.plan_only,
                    plan_only: item.plan_only,
                    detail: item.detail,
                    faces,
                }),
                Err(e) => errors.push((i, e)),
            }
        }
        (parts, errors)
    }

    /// Material of the first piece (for coloring and type legends).
    pub fn main_material(&self) -> &str {
        self.items.first().map_or("", |i| i.material.as_str())
    }

    /// Rename a parameter and every reference to it in the pieces.
    pub fn rename_param(&mut self, old: &str, new: &str) {
        self.rename(old, new);
    }

    /// Rename a parameter, reference plane or line and every expression,
    /// dimension and label that reads it.
    pub fn rename(&mut self, old: &str, new: &str) {
        for p in &mut self.params {
            if p.name == old {
                p.name = new.to_string();
            }
        }
        for p in &mut self.ref_planes {
            if p.name == old {
                p.name = new.to_string();
            }
            p.at = replace_name(&p.at, old, new);
        }
        for l in &mut self.ref_lines {
            if l.name == old {
                l.name = new.to_string();
            }
            for e in [&mut l.offset, &mut l.start.0, &mut l.start.1, &mut l.end.0, &mut l.end.1] {
                *e = replace_name(e, old, new);
            }
        }
        for d in &mut self.dimensions {
            for name in [&mut d.from, &mut d.to] {
                if name == old {
                    *name = new.to_string();
                }
            }
            if d.label.as_deref() == Some(old) {
                d.label = Some(new.to_string());
            }
        }
        for q in &mut self.equals {
            for name in [&mut q.a, &mut q.middle, &mut q.b] {
                if name == old {
                    *name = new.to_string();
                }
            }
        }
        for item in &mut self.items {
            for e in item.shape.expressions_mut() {
                *e = replace_name(e, old, new);
            }
        }
        if let Some(o) = &mut self.opening {
            for e in o.expressions_mut() {
                *e = replace_name(e, old, new);
            }
        }
        for c in &mut self.connectors {
            for e in c.expressions_mut() {
                *e = replace_name(e, old, new);
            }
        }
    }
}

/// Replace whole-identifier occurrences of `old` in `text` (numbers such as
/// `2e3` are left alone).
pub fn replace_name(text: &str, old: &str, new: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        if c.is_ascii_digit() || c == '.' {
            while i < chars.len() && (expr::is_ident_char(chars[i]) || chars[i] == '.') {
                i += 1;
            }
            out.extend(&chars[start..i]);
        } else if expr::is_ident_char(c) {
            while i < chars.len() && expr::is_ident_char(chars[i]) {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();
            out.push_str(if ident == old { new } else { &ident });
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Faces of a mesh piece, its footprint hull and height range. Faces with
/// fewer than three valid vertices are left out.
type ResolvedMesh = (Vec<DVec2>, f64, f64, Vec<Vec<DVec3>>);

/// A profile piece: a plan profile extruded upwards is a prism (its outline
/// the footprint); other profiles and revolutions become faces.
fn resolve_profile(shape: &ComponentShape, lookup: &dyn Fn(&str) -> Option<f64>) -> Result<ResolvedMesh, PartError> {
    use crate::profile::{self, ProfileError};
    let ev =
        |field: &'static str, text: &str| expr::eval(text, lookup).map_err(|error| PartError::Expr { field, error });
    let from = |e: ProfileError| match e {
        ProfileError::Expr { field, error } => PartError::Expr { field, error },
        ProfileError::Degenerate => PartError::Degenerate,
    };
    let faces = match shape {
        ComponentShape::Profile { plane, points, offset, depth } => {
            let outline = profile::outline(points, lookup).map_err(from)?;
            let offset = ev("offset", offset)?;
            let depth = ev("depth", depth)?;
            if depth <= 1e-6 {
                return Err(PartError::Degenerate);
            }
            if *plane == SketchPlane::Plan {
                return Ok((outline, offset, offset + depth, Vec::new()));
            }
            profile::extrude(*plane, &outline, offset, depth)
        }
        ComponentShape::Revolution { plane, points, offset, axis, angle } => {
            let outline = profile::outline(points, lookup).map_err(from)?;
            let (offset, axis, angle) = (ev("offset", offset)?, ev("axis", axis)?, ev("angle", angle)?);
            profile::revolve(*plane, &outline, offset, axis, angle).map_err(from)?
        }
        ComponentShape::Sweep { plane, path, offset, profile } => {
            let path = crate::sweep::path(path, lookup).map_err(from)?;
            let section = profile::outline(profile, lookup).map_err(from)?;
            crate::sweep::sweep(*plane, &path, ev("offset", offset)?, &section).map_err(from)?
        }
        ComponentShape::Blend { plane, bottom, top, offset, depth } => {
            let (low, high) =
                (profile::outline(bottom, lookup).map_err(from)?, profile::outline(top, lookup).map_err(from)?);
            crate::sweep::blend(*plane, &low, &high, ev("offset", offset)?, ev("depth", depth)?).map_err(from)?
        }
        _ => return Err(PartError::Degenerate),
    };
    let all: Vec<DVec3> = faces.iter().flatten().copied().collect();
    let (z0, z1) = all.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.z), hi.max(p.z)));
    if faces.is_empty() || !all.iter().all(|p| p.is_finite()) {
        return Err(PartError::Degenerate);
    }
    let footprint = convex_hull(&all.iter().map(|p| p.truncate()).collect::<Vec<_>>());
    Ok((footprint, z0, z1, faces))
}

fn resolve_mesh(vertices: &[[f64; 3]], faces: &[Vec<u32>]) -> Result<ResolvedMesh, PartError> {
    let point = |i: &u32| {
        let v = vertices.get(*i as usize)?;
        let p = DVec3::from_array(*v);
        p.is_finite().then_some(p)
    };
    let faces: Vec<Vec<DVec3>> =
        faces.iter().map(|f| f.iter().filter_map(point).collect::<Vec<_>>()).filter(|f| f.len() >= 3).collect();
    let all: Vec<DVec3> = faces.iter().flatten().copied().collect();
    let (z0, z1) = all.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.z), hi.max(p.z)));
    let footprint = convex_hull(&all.iter().map(|p| p.truncate()).collect::<Vec<_>>());
    if faces.is_empty() || !(z0.is_finite() && z1.is_finite()) {
        return Err(PartError::Degenerate);
    }
    Ok((footprint, z0, z1, faces))
}

/// Convex hull of plan points (counter-clockwise; fewer than 3 points when degenerate).
pub fn convex_hull(points: &[DVec2]) -> Vec<DVec2> {
    let mut pts: Vec<DVec2> = points.iter().copied().filter(|p| p.is_finite()).collect();
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup_by(|a, b| a.distance(*b) < 1e-9);
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o);
    let mut hull: Vec<DVec2> = Vec::new();
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &DVec2>> =
            if pass == 0 { Box::new(pts.iter()) } else { Box::new(pts.iter().rev()) };
        for &p in iter {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 1e-12 {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

fn resolve_shape(
    shape: &ComponentShape,
    lookup: &dyn Fn(&str) -> Option<f64>,
) -> Result<(Vec<DVec2>, f64, f64), PartError> {
    let ev =
        |field: &'static str, text: &str| expr::eval(text, lookup).map_err(|error| PartError::Expr { field, error });
    let positive = |v: f64| if v > 1e-6 { Ok(v) } else { Err(PartError::Degenerate) };
    match shape {
        ComponentShape::Box { x, y, z, width, depth, height } => {
            let (x, y, z) = (ev("x", x)?, ev("y", y)?, ev("z", z)?);
            let w = positive(ev("width", width)?)?;
            let d = positive(ev("depth", depth)?)?;
            let h = positive(ev("height", height)?)?;
            let footprint =
                vec![DVec2::new(x, y), DVec2::new(x + w, y), DVec2::new(x + w, y + d), DVec2::new(x, y + d)];
            Ok((footprint, z, z + h))
        }
        ComponentShape::Cylinder { x, y, z, radius, height } => {
            let c = DVec2::new(ev("x", x)?, ev("y", y)?);
            let z = ev("z", z)?;
            let r = positive(ev("radius", radius)?)?;
            let h = positive(ev("height", height)?)?;
            let footprint = (0..CYLINDER_SEGMENTS)
                .map(|i| c + DVec2::from_angle(i as f64 / CYLINDER_SEGMENTS as f64 * std::f64::consts::TAU) * r)
                .collect();
            Ok((footprint, z, z + h))
        }
        ComponentShape::Extrusion { points, z, height } => {
            let mut footprint = Vec::with_capacity(points.len());
            for (x, y) in points {
                footprint.push(DVec2::new(ev("x", x)?, ev("y", y)?));
            }
            let z = ev("z", z)?;
            let h = positive(ev("height", height)?)?;
            footprint.dedup_by(|a, b| a.distance(*b) < 1e-9);
            let area: f64 =
                (0..footprint.len()).map(|i| footprint[i].perp_dot(footprint[(i + 1) % footprint.len()])).sum::<f64>()
                    / 2.0;
            if footprint.len() < 3 || area.abs() < 1e-9 {
                return Err(PartError::Degenerate);
            }
            Ok((footprint, z, z + h))
        }
        ComponentShape::Mesh { .. }
        | ComponentShape::Profile { .. }
        | ComponentShape::Revolution { .. }
        | ComponentShape::Sweep { .. }
        | ComponentShape::Blend { .. } => Err(PartError::Degenerate),
    }
}

/// The default component library: (component name, type name, definition).
/// Parameter names come from `n` (keys `tpl-param-*`), so expressions use
/// the translated names; materials are the template's.
pub fn library(n: &dyn Fn(&str) -> String) -> Vec<(String, String, ComponentDef)> {
    let (w, d, h) = (n("tpl-param-width"), n("tpl-param-depth"), n("tpl-param-height"));
    let wood = n("tpl-mat-wood");
    let fabric = n("tpl-mat-fabric");
    let porcelain = n("tpl-mat-porcelain");
    let length = |name: &str, value: f64| ComponentParam {
        name: name.to_string(),
        value,
        kind: ParamKind::Length,
        instance: false,
    };
    let bx = |name: &str,
              x: String,
              y: String,
              z: String,
              sw: String,
              sd: String,
              sh: String,
              material: &str,
              plan: bool| {
        ComponentItem {
            name: name.to_string(),
            shape: ComponentShape::Box { x, y, z, width: sw, depth: sd, height: sh },
            material: material.to_string(),
            in_plan: plan,
            void: false,
            plan_only: false,
            detail: Default::default(),
        }
    };
    let s = |v: &str| v.to_string();
    // Four legs of `t` inset at the corners of a W x D footprint centered on the origin.
    let legs = |t: &str, top: String, material: &str| -> Vec<ComponentItem> {
        [("-", "-"), ("+", "-"), ("+", "+"), ("-", "+")]
            .iter()
            .enumerate()
            .map(|(i, (sx, sy))| {
                let x = if *sx == "-" { format!("-{w} / 2") } else { format!("{w} / 2 - {t}") };
                let y = if *sy == "-" { format!("-{d} / 2") } else { format!("{d} / 2 - {t}") };
                bx(&format!("{} {}", n("tpl-part-leg"), i + 1), x, y, s("0"), s(t), s(t), top.clone(), material, false)
            })
            .collect()
    };

    let mut out = Vec::new();

    // Table: top and four legs.
    let mut items = vec![bx(
        &n("tpl-part-top"),
        format!("-{w} / 2"),
        format!("-{d} / 2"),
        format!("{h} - 0.03"),
        s(&w),
        s(&d),
        s("0.03"),
        &wood,
        true,
    )];
    items.extend(legs("0.05", format!("{h} - 0.03"), &wood));
    let table = ComponentDef {
        kind: ComponentKind::Furniture,
        opening: None,
        connectors: Vec::new(),
        ifc: None,
        mep: None,
        params: vec![length(&w, 1.2), length(&d, 0.8), length(&h, 0.75)],
        items,
        ..Default::default()
    };
    out.push((n("tpl-comp-table"), n("tpl-type-table"), table));

    // Chair: seat, legs and backrest.
    let seat = n("tpl-param-seat-height");
    let mut items = vec![bx(
        &n("tpl-part-seat"),
        format!("-{w} / 2"),
        format!("-{d} / 2"),
        format!("{seat} - 0.04"),
        s(&w),
        s(&d),
        s("0.04"),
        &wood,
        true,
    )];
    items.extend(legs("0.04", format!("{seat} - 0.04"), &wood));
    items.push(bx(
        &n("tpl-part-back"),
        format!("-{w} / 2"),
        format!("{d} / 2 - 0.03"),
        s(&seat),
        s(&w),
        s("0.03"),
        format!("{h} - {seat}"),
        &wood,
        true,
    ));
    let chair = ComponentDef {
        kind: ComponentKind::Furniture,
        opening: None,
        connectors: Vec::new(),
        ifc: None,
        mep: None,
        params: vec![length(&w, 0.45), length(&d, 0.5), length(&h, 0.9), length(&seat, 0.45)],
        items,
        ..Default::default()
    };
    out.push((n("tpl-comp-chair"), n("tpl-type-chair"), chair));

    // Bed: mattress base and headboard at the back.
    let bed = ComponentDef {
        kind: ComponentKind::Furniture,
        opening: None,
        connectors: Vec::new(),
        ifc: None,
        mep: None,
        params: vec![length(&w, 1.4), length(&d, 1.9), length(&h, 0.5)],
        items: vec![
            bx(
                &n("tpl-part-mattress"),
                format!("-{w} / 2"),
                format!("-{d} / 2"),
                s("0"),
                s(&w),
                format!("{d} - 0.05"),
                s(&h),
                &fabric,
                true,
            ),
            bx(
                &n("tpl-part-headboard"),
                format!("-{w} / 2"),
                format!("{d} / 2 - 0.05"),
                s("0"),
                s(&w),
                s("0.05"),
                format!("{h} + 0.5"),
                &wood,
                true,
            ),
        ],
        ..Default::default()
    };
    out.push((n("tpl-comp-bed"), n("tpl-type-bed"), bed));

    // Sofa: seat, back and two arms.
    let arm = "0.15";
    let sofa = ComponentDef {
        kind: ComponentKind::Furniture,
        opening: None,
        connectors: Vec::new(),
        ifc: None,
        mep: None,
        params: vec![length(&w, 2.0), length(&d, 0.9), length(&h, 0.8), length(&seat, 0.42)],
        items: vec![
            bx(
                &n("tpl-part-seat"),
                format!("-{w} / 2 + {arm}"),
                format!("-{d} / 2"),
                s("0"),
                format!("{w} - 2 * {arm}"),
                format!("{d} - 0.2"),
                s(&seat),
                &fabric,
                true,
            ),
            bx(
                &n("tpl-part-back"),
                format!("-{w} / 2"),
                format!("{d} / 2 - 0.2"),
                s("0"),
                s(&w),
                s("0.2"),
                s(&h),
                &fabric,
                true,
            ),
            bx(
                &format!("{} 1", n("tpl-part-arm")),
                format!("-{w} / 2"),
                format!("-{d} / 2"),
                s("0"),
                s(arm),
                format!("{d} - 0.2"),
                format!("{seat} + 0.2"),
                &fabric,
                true,
            ),
            bx(
                &format!("{} 2", n("tpl-part-arm")),
                format!("{w} / 2 - {arm}"),
                format!("-{d} / 2"),
                s("0"),
                s(arm),
                format!("{d} - 0.2"),
                format!("{seat} + 0.2"),
                &fabric,
                true,
            ),
        ],
        ..Default::default()
    };
    out.push((n("tpl-comp-sofa"), n("tpl-type-sofa"), sofa));

    // Toilet: bowl in front of the tank, against the wall at the back.
    let connector =
        |name: String, system: crate::SystemKind, at: [String; 3], direction, diameter: &str| ComponentConnector {
            name,
            system,
            x: at[0].clone(),
            y: at[1].clone(),
            z: at[2].clone(),
            direction,
            diameter: diameter.to_string(),
        };
    use crate::SystemKind as K;
    use ConnectorDirection as D;
    let toilet = ComponentDef {
        kind: ComponentKind::Plumbing,
        opening: None,
        // The outlet 30 cm from the wall under the bowl; the inlet at the back, beside the tank.
        connectors: vec![
            connector(n("tpl-conn-sewer"), K::Sanitary, [s("0"), format!("{d} / 2 - 0.3"), s("0")], D::Down, "0.1"),
            connector(
                n("tpl-conn-cold-water"),
                K::ColdWater,
                [s("-0.15"), format!("{d} / 2"), s("0.2")],
                D::Back,
                "0.02",
            ),
        ],
        ifc: None,
        // Toilet with a close-coupled tank.
        mep: Some(MepData::Fixture { drain_units: 6.0, supply_weight: 0.3, hot_water: false }),
        params: vec![length(&w, 0.38), length(&d, 0.65), length(&h, 0.78)],
        items: vec![
            ComponentItem {
                name: n("tpl-part-bowl"),
                shape: ComponentShape::Cylinder {
                    x: s("0"),
                    y: format!("-{d} / 2 + {w} / 2"),
                    z: s("0"),
                    radius: format!("{w} / 2"),
                    height: s("0.4"),
                },
                material: porcelain.clone(),
                in_plan: true,
                void: false,
                plan_only: false,
                detail: Default::default(),
            },
            bx(
                &n("tpl-part-tank"),
                format!("-{w} / 2"),
                format!("{d} / 2 - 0.18"),
                s("0"),
                s(&w),
                s("0.18"),
                s(&h),
                &porcelain,
                true,
            ),
        ],
        ..Default::default()
    };
    out.push((n("tpl-comp-toilet"), n("tpl-type-toilet"), toilet));

    // Washbasin: basin slab on a pedestal.
    let basin = ComponentDef {
        kind: ComponentKind::Plumbing,
        opening: None,
        // Waste behind the pedestal, cold and hot water at the wall above it.
        connectors: vec![
            connector(n("tpl-conn-sewer"), K::Sanitary, [s("0"), format!("{d} / 2 - 0.05"), s("0.5")], D::Down, "0.04"),
            connector(
                n("tpl-conn-cold-water"),
                K::ColdWater,
                [s("-0.1"), format!("{d} / 2"), s("0.6")],
                D::Back,
                "0.02",
            ),
            connector(n("tpl-conn-hot-water"), K::HotWater, [s("0.1"), format!("{d} / 2"), s("0.6")], D::Back, "0.02"),
        ],
        ifc: None,
        mep: Some(MepData::Fixture { drain_units: 1.0, supply_weight: 0.3, hot_water: true }),
        params: vec![length(&w, 0.55), length(&d, 0.45), length(&h, 0.85)],
        items: vec![
            bx(
                &n("tpl-part-basin"),
                format!("-{w} / 2"),
                format!("-{d} / 2"),
                format!("{h} - 0.15"),
                s(&w),
                s(&d),
                s("0.15"),
                &porcelain,
                true,
            ),
            ComponentItem {
                name: n("tpl-part-pedestal"),
                shape: ComponentShape::Cylinder {
                    x: s("0"),
                    y: format!("{d} / 2 - 0.12"),
                    z: s("0"),
                    radius: s("0.09"),
                    height: format!("{h} - 0.15"),
                },
                material: porcelain.clone(),
                in_plan: false,
                void: false,
                plan_only: false,
                detail: Default::default(),
            },
        ],
        ..Default::default()
    };
    out.push((n("tpl-comp-washbasin"), n("tpl-type-washbasin"), basin));

    // Wardrobe: one box.
    let wardrobe = ComponentDef {
        kind: ComponentKind::Casework,
        opening: None,
        connectors: Vec::new(),
        ifc: None,
        mep: None,
        params: vec![length(&w, 1.6), length(&d, 0.6), length(&h, 2.2)],
        items: vec![bx(
            &n("tpl-part-body"),
            format!("-{w} / 2"),
            format!("-{d} / 2"),
            s("0"),
            s(&w),
            s(&d),
            s(&h),
            &wood,
            true,
        )],
        ..Default::default()
    };
    out.push((n("tpl-comp-wardrobe"), n("tpl-type-wardrobe"), wardrobe));
    out.extend(mep_library(n));
    out
}

/// The building-services part of the default library: plumbing fixtures
/// and electrical devices. Wall-mounted devices have their back on the
/// insertion point and stand out towards -Y; their elevation is the
/// instance's base offset (the mounting height).
pub fn mep_library(n: &dyn Fn(&str) -> String) -> Vec<(String, String, ComponentDef)> {
    use crate::DeviceKind as D;
    let (w, d, h) = (n("tpl-param-width"), n("tpl-param-depth"), n("tpl-param-height"));
    let porcelain = n("tpl-mat-porcelain");
    let steel = n("tpl-mat-steel");
    let plastic = n("tpl-mat-pvc");
    let length = |name: &str, value: f64| ComponentParam {
        name: name.to_string(),
        value,
        kind: ParamKind::Length,
        instance: false,
    };
    let sizes = |width: f64, depth: f64, height: f64| vec![length(&w, width), length(&d, depth), length(&h, height)];
    let piece = |name: &str, shape: ComponentShape, material: &str| ComponentItem {
        name: name.to_string(),
        shape,
        material: material.to_string(),
        in_plan: true,
        void: false,
        plan_only: false,
        detail: Default::default(),
    };
    // A box W x D x H with its back on the X axis, standing out towards -Y.
    let wall_box = |z: &str| ComponentShape::Box {
        x: format!("-{w} / 2"),
        y: format!("-{d}"),
        z: z.to_string(),
        width: w.clone(),
        depth: d.clone(),
        height: h.clone(),
    };
    let def =
        |kind: ComponentKind, params: Vec<ComponentParam>, items: Vec<ComponentItem>, mep: MepData| ComponentDef {
            kind,
            params,
            items,
            ifc: None,
            mep: Some(mep),
            opening: None,
            connectors: Vec::new(),
            ..Default::default()
        };
    let device = |device: D, power: f64| MepData::Device { device, power };
    let body = n("tpl-part-body");
    let mut out = Vec::new();

    // Shower head on its arm, high on the wall.
    let shower = def(
        ComponentKind::Plumbing,
        sizes(0.15, 0.4, 2.1),
        vec![piece(
            &n("tpl-part-head"),
            ComponentShape::Cylinder {
                x: "0".into(),
                y: format!("-{d}"),
                z: h.clone(),
                radius: format!("{w} / 2"),
                height: "0.03".into(),
            },
            &steel,
        )],
        MepData::Fixture { drain_units: 2.0, supply_weight: 0.4, hot_water: true },
    );
    out.push((n("tpl-comp-shower"), n("tpl-type-shower"), shower));

    // Kitchen sink: a countertop basin against the wall.
    let sink = def(
        ComponentKind::Plumbing,
        sizes(1.2, 0.55, 0.9),
        vec![piece(
            &n("tpl-part-basin"),
            ComponentShape::Box {
                x: format!("-{w} / 2"),
                y: format!("-{d}"),
                z: format!("{h} - 0.2"),
                width: w.clone(),
                depth: d.clone(),
                height: "0.2".into(),
            },
            &steel,
        )],
        MepData::Fixture { drain_units: 3.0, supply_weight: 0.7, hot_water: true },
    );
    out.push((n("tpl-comp-kitchen-sink"), n("tpl-type-kitchen-sink"), sink));

    // Floor drain: a grate flush with the floor.
    let drain = def(
        ComponentKind::Plumbing,
        sizes(0.15, 0.15, 0.01),
        vec![piece(
            &n("tpl-part-grate"),
            ComponentShape::Box {
                x: format!("-{w} / 2"),
                y: format!("-{d} / 2"),
                z: format!("-{h}"),
                width: w.clone(),
                depth: d.clone(),
                height: h.clone(),
            },
            &steel,
        )],
        MepData::Fixture { drain_units: 0.0, supply_weight: 0.0, hot_water: false },
    );
    out.push((n("tpl-comp-floor-drain"), n("tpl-type-floor-drain"), drain));

    let electrical = [
        ("tpl-comp-outlet", "tpl-type-outlet", ComponentKind::Electrical, (0.1, 0.03, 0.1), device(D::Outlet, 100.0)),
        (
            "tpl-comp-dedicated-outlet",
            "tpl-type-dedicated-outlet",
            ComponentKind::Electrical,
            (0.1, 0.03, 0.1),
            device(D::DedicatedOutlet, 1500.0),
        ),
        ("tpl-comp-switch", "tpl-type-switch", ComponentKind::Electrical, (0.06, 0.03, 0.1), device(D::Switch, 0.0)),
        (
            "tpl-comp-wall-light",
            "tpl-type-wall-light",
            ComponentKind::Lighting,
            (0.2, 0.1, 0.15),
            device(D::WallLight, 60.0),
        ),
        ("tpl-comp-panel", "tpl-type-panel", ComponentKind::Electrical, (0.4, 0.1, 0.5), device(D::Panel, 0.0)),
    ];
    for (component, type_name, kind, (width, depth, height), mep) in electrical {
        let material = if mep.device() == Some(D::Panel) { &steel } else { &plastic };
        let item = piece(&body, wall_box("0"), material);
        out.push((n(component), n(type_name), def(kind, sizes(width, depth, height), vec![item], mep)));
    }

    // Ceiling light: a disc hanging under the insertion point (on the ceiling).
    let light = def(
        ComponentKind::Lighting,
        sizes(0.3, 0.3, 0.06),
        vec![piece(
            &body,
            ComponentShape::Cylinder {
                x: "0".into(),
                y: "0".into(),
                z: format!("-{h}"),
                radius: format!("{w} / 2"),
                height: h.clone(),
            },
            &porcelain,
        )],
        device(D::CeilingLight, 100.0),
    );
    out.push((n("tpl-comp-ceiling-light"), n("tpl-type-ceiling-light"), light));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(n: &str) -> String {
        match n {
            "tpl-param-width" => "Largura".into(),
            "tpl-param-depth" => "Profundidade".into(),
            "tpl-param-height" => "Altura".into(),
            "tpl-param-seat-height" => "AlturaAssento".into(),
            other => other.into(),
        }
    }

    #[test]
    fn library_resolves_without_errors() {
        let lib = library(&pt);
        assert_eq!(lib.len(), 16);
        for (name, _, def) in &lib {
            let (parts, errors) = def.resolve(&BTreeMap::new());
            assert!(errors.is_empty(), "{name}: {errors:?}");
            assert_eq!(parts.len(), def.items.len());
            for p in &parts {
                assert!(p.z1 > p.z0);
            }
        }
        // The table top sits at the table height.
        let (_, _, table) = &lib[0];
        let (parts, _) = table.resolve(&BTreeMap::new());
        assert!((parts[0].z1 - 0.75).abs() < 1e-12);
        let (lo, hi) = parts
            .iter()
            .flat_map(|p| p.footprint.iter())
            .fold((DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)), |(lo, hi), p| {
                (lo.min(*p), hi.max(*p))
            });
        assert!((hi - lo - DVec2::new(1.2, 0.8)).abs().max_element() < 1e-12);
    }

    #[test]
    fn instance_overrides_apply_only_to_instance_params() {
        let mut def = library(&pt).remove(0).2;
        let mut overrides = BTreeMap::from([("Largura".to_string(), 2.0), ("Altura".to_string(), 1.0)]);
        def.params[0].instance = true;
        let values = def.values(&overrides);
        assert_eq!(values["Largura"], 2.0);
        assert_eq!(values["Altura"], 0.75);
        overrides.insert("Largura".into(), f64::NAN);
        assert_eq!(def.values(&overrides)["Largura"], 1.2);
    }

    #[test]
    fn errors_name_the_piece() {
        let mut def = library(&pt).remove(0).2;
        if let ComponentShape::Box { width, .. } = &mut def.items[1].shape {
            *width = "Largur * 2".into();
        }
        if let ComponentShape::Box { height, .. } = &mut def.items[2].shape {
            *height = "0 - 1".into();
        }
        let (parts, errors) = def.resolve(&BTreeMap::new());
        assert_eq!(parts.len(), def.items.len() - 2);
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].0, 1);
        assert!(
            matches!(&errors[0].1, PartError::Expr { field: "width", error: ExprError::UnknownName(n) } if n == "Largur")
        );
        assert_eq!(errors[1], (2, PartError::Degenerate));
    }

    #[test]
    fn mesh_pieces_resolve_to_faces_and_hull() {
        let item = ComponentItem {
            name: "m".into(),
            shape: ComponentShape::Mesh {
                vertices: vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0, 2.0],
                    [f64::NAN, 0.0, 0.0],
                ],
                // The last face has a bad vertex and an index out of range: it drops to nothing.
                faces: vec![vec![0, 2, 1], vec![0, 1, 3], vec![1, 2, 3], vec![2, 0, 3], vec![4, 9, 4]],
            },
            material: String::new(),
            in_plan: true,
            void: false,
            plan_only: false,
            detail: Default::default(),
        };
        let def = ComponentDef { items: vec![item.clone()], ..Default::default() };
        let (parts, errors) = def.resolve(&BTreeMap::new());
        assert!(errors.is_empty());
        assert_eq!(parts[0].faces.len(), 4);
        assert_eq!(parts[0].footprint.len(), 3);
        assert_eq!((parts[0].z0, parts[0].z1), (0.0, 2.0));
        let empty = ComponentShape::Mesh { vertices: vec![], faces: vec![vec![0, 1, 2]] };
        let def = ComponentDef { items: vec![ComponentItem { shape: empty, ..item }], ..Default::default() };
        assert_eq!(def.resolve(&BTreeMap::new()).1.len(), 1);
    }

    #[test]
    fn rename_updates_expressions() {
        let mut def = library(&pt).remove(0).2;
        def.rename_param("Largura", "W");
        assert_eq!(def.params[0].name, "W");
        let all: Vec<String> = def.items.iter().flat_map(|i| i.shape.expressions()).map(|(_, e)| e.clone()).collect();
        assert!(all.iter().any(|e| e.contains("W")));
        assert!(!all.iter().any(|e| e.contains("Largura")));
        assert_eq!(replace_name("Largura2 + Largura * 2e3 - a", "Largura", "W"), "Largura2 + W * 2e3 - a");
        assert_eq!(replace_name("x1 + 1x", "x", "y"), "x1 + 1x");
        assert!(def.resolve(&BTreeMap::new()).1.is_empty());
    }
}
