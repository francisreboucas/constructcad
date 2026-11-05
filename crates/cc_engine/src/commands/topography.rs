//! Terrain: topography surfaces from measured points inside a boundary
//! (see `cc_core::topography`).

use super::sketch::CurveArg;
use super::{add_element, created, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, point, positive};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::material::{Material, MaterialClass};
use cc_core::topography::{MAX_OUTLINE, MAX_POINTS, Topography, clean_points};
use cc_core::{Category, ElementId, ElementKind, ElementType, ElementTypeId, Project, TypeData};
use glam::{DVec2, DVec3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A measured point `[x, y, z]`: plan position and elevation above the level, meters.
pub type Point3 = [f64; 3];

fn point3(p: Point3) -> Result<DVec3> {
    let v = DVec3::from_array(p);
    if v.is_finite() { Ok(v) } else { Err(EngineError::invalid("points must be finite numbers")) }
}

fn points3(list: &[Point3]) -> Result<Vec<DVec3>> {
    list.iter().map(|p| point3(*p)).collect()
}

/// The project's earth material: the first of the earth class, or none.
fn earth_material(p: &Project) -> Option<String> {
    p.materials.iter().find(|m| m.identity.class == MaterialClass::Earth).map(|m| m.name.clone())
}

/// A terrain type for a project that has none, in `material`.
pub fn new_topography_type(id: ElementTypeId, material: String) -> ElementType {
    ElementType {
        id,
        name: t!("tpl-type-topography"),
        data: TypeData::Topography { material, contour_interval: 1.0, major_every: 5 },
        info: Default::default(),
        uid: Default::default(),
    }
}

/// The earth material for a new terrain type, added to the project when it has none.
fn ensure_earth(e: &mut cc_core::EditCtx) -> String {
    if let Some(name) = earth_material(e.project()) {
        return name;
    }
    let mut name = t!("tpl-mat-earth");
    while e.project().material(&name).is_some() {
        name.push('*');
    }
    let mut m = Material::new(name.as_str(), [150, 122, 88]);
    cc_core::material::template_properties(&mut m, MaterialClass::Earth);
    e.materials_mut().push(m);
    name
}

/// Create a terrain surface (topography): measured `points` `[x, y, z]`
/// inside a closed `boundary` (or one drawn with `curves`; with neither,
/// the convex hull of the points). The points are triangulated with the
/// boundary, whose vertices take the elevations the points give them; the
/// earth closes down to a flat base `base_depth` below the lowest point.
/// Points outside the boundary are kept but take no part in the surface.
/// A project without a terrain type gets one (and an earth material).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct TopographyCreate {
    /// Measured points `[[x, y, z], …]`: plan position and elevation above the level, meters.
    #[serde(default)]
    pub points: Vec<Point3>,
    /// Boundary `[[x, y], …]` in plan, closed (at least three points); or `curves`.
    #[serde(default)]
    pub boundary: Vec<Point>,
    /// The boundary drawn with curves (lines, arcs, circles, rectangles…).
    #[serde(default)]
    pub curves: Option<Vec<CurveArg>>,
    /// Level id or name the elevations are measured from (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Depth of the flat base below the lowest point of the surface, meters (default 2).
    #[serde(default)]
    pub base_depth: Option<f64>,
    /// Type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
}

impl Command for TopographyCreate {
    const NAME: &'static str = "topography_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        if self.points.len() > MAX_POINTS {
            return Err(EngineError::invalid(format!("at most {MAX_POINTS} points")));
        }
        let points = clean_points(&points3(&self.points)?);
        let (boundary, drawn) = match &self.curves {
            Some(_) if !self.boundary.is_empty() => {
                return Err(EngineError::invalid("give boundary or curves, not both"));
            }
            Some(curves) => {
                let sketch = super::sketch::sketch_of(curves)?;
                (super::sketch::outline(&sketch)?, Some(sketch))
            }
            None if self.boundary.is_empty() => {
                let plan: Vec<DVec2> = points.iter().map(|p| p.truncate()).collect();
                (cc_core::component::convex_hull(&plan), None)
            }
            None => (self.boundary.iter().map(|p| point(*p)).collect::<Result<Vec<DVec2>>>()?, None),
        };
        if boundary.len() < 3 || boundary.len() > MAX_OUTLINE {
            return Err(EngineError::invalid(format!(
                "the boundary needs 3 to {MAX_OUTLINE} points (give boundary, curves, or at least three points around)"
            )));
        }
        let base_depth = positive(self.base_depth.unwrap_or(2.0), "base_depth")?;
        let topo = Topography { points, boundary, base_depth, sketch: drawn };
        if !topo.is_valid() {
            return Err(EngineError::invalid("the boundary encloses no area"));
        }
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let ty = match (&self.type_ref, p.default_type(Category::Topography)) {
            (None, None) => None,
            (r, _) => Some(refs::type_of(p, Category::Topography, r.as_ref())?),
        };
        let id = edit(s, t!("tx-create-topography"), |e| {
            let type_id = match ty {
                Some(t) => t,
                None => {
                    let material = ensure_earth(e);
                    let id = ElementTypeId(e.alloc_id());
                    e.add_type(new_topography_type(id, material))
                }
            };
            Ok(add_element(e, level, type_id, ElementKind::Topography(topo)))
        })?;
        Ok(created(s, id))
    }
}

/// A new elevation for one point.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Clone, Copy)]
#[serde(deny_unknown_fields)]
pub struct PointElevation {
    /// Index of the point in the surface's `points`.
    pub index: usize,
    /// Elevation above the level, meters.
    pub z: f64,
}

/// Edit the measured points of a terrain surface: replace them all
/// (`points`), or change elevations (`set`), remove points (`remove`, by
/// index) and add new ones (`add`), in that order. Indices refer to the
/// points as they are before the edit. Points that repeat another in plan
/// are dropped (the first is kept); the answer says how many.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct TopographyPointsSet {
    /// Terrain element id.
    pub id: u64,
    /// All the points `[[x, y, z], …]`, replacing the current ones.
    #[serde(default)]
    pub points: Option<Vec<Point3>>,
    /// New elevations `[{index, z}, …]`.
    #[serde(default)]
    pub set: Vec<PointElevation>,
    /// Indices of points to remove.
    #[serde(default)]
    pub remove: Vec<usize>,
    /// Points `[[x, y, z], …]` to add.
    #[serde(default)]
    pub add: Vec<Point3>,
}

/// The points after the edits of `cmd` on `current`.
fn edited_points(current: &[DVec3], cmd: &TopographyPointsSet) -> Result<Vec<DVec3>> {
    let mut pts = match &cmd.points {
        Some(list) => points3(list)?,
        None => current.to_vec(),
    };
    for s in &cmd.set {
        if !s.z.is_finite() {
            return Err(EngineError::invalid("elevations must be finite numbers"));
        }
        let p = pts.get_mut(s.index).ok_or_else(|| EngineError::invalid(format!("no point {}", s.index)))?;
        p.z = s.z;
    }
    if let Some(bad) = cmd.remove.iter().find(|i| **i >= pts.len()) {
        return Err(EngineError::invalid(format!("no point {bad}")));
    }
    let removed: std::collections::BTreeSet<usize> = cmd.remove.iter().copied().collect();
    let mut pts: Vec<DVec3> =
        pts.into_iter().enumerate().filter(|(i, _)| !removed.contains(i)).map(|(_, p)| p).collect();
    pts.extend(points3(&cmd.add)?);
    if pts.len() > MAX_POINTS {
        return Err(EngineError::invalid(format!("at most {MAX_POINTS} points")));
    }
    Ok(pts)
}

impl Command for TopographyPointsSet {
    const NAME: &'static str = "topography_points_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = ElementId(self.id);
        let Some(ElementKind::Topography(t)) = s.project().element(id).map(|e| &e.kind) else {
            return Err(EngineError::not_found("terrain element"));
        };
        let wanted = edited_points(&t.points, &self)?;
        let points = clean_points(&wanted);
        let dropped = wanted.len() - points.len();
        let count = points.len();
        edit(s, t!("tx-modify"), |e| {
            if let Some(ElementKind::Topography(t)) = e.element_mut(id).map(|el| &mut el.kind) {
                t.points = points;
            }
            Ok(())
        })?;
        Ok(json!({ "id": id.0, "points": count, "dropped": dropped }))
    }
}

/// Create a terrain type: the earth `material` sections cut it in, and its
/// contour lines every `contour_interval` meters, every `major_every`-th a
/// major (heavier) one.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct TopographyTypeCreate {
    /// Type name (default: the template's terrain name).
    #[serde(default)]
    pub name: Option<String>,
    /// Material name (default: the project's earth material, added if missing).
    #[serde(default)]
    pub material: Option<String>,
    /// Spacing of the contour lines, meters (default 1).
    #[serde(default)]
    pub contour_interval: Option<f64>,
    /// Every this many contours, a major one (default 5).
    #[serde(default)]
    pub major_every: Option<u32>,
}

impl Command for TopographyTypeCreate {
    const NAME: &'static str = "topography_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let material = match self.material {
            Some(m) => Some(refs::material(s.project(), &Ref::Name(m))?),
            None => None,
        };
        let interval = positive(self.contour_interval.unwrap_or(1.0), "contour_interval")?;
        if interval < 0.01 {
            return Err(EngineError::invalid("contour_interval must be at least 0.01"));
        }
        let major = self.major_every.unwrap_or(5);
        if major == 0 {
            return Err(EngineError::invalid("major_every must be at least 1"));
        }
        let name = self.name.filter(|n| !n.trim().is_empty());
        let id = edit(s, t!("tx-create-topography-type"), |e| {
            let material = match material {
                Some(m) => m,
                None => ensure_earth(e),
            };
            let id = ElementTypeId(e.alloc_id());
            let mut ty = new_topography_type(id, material);
            if let TypeData::Topography { contour_interval, major_every, .. } = &mut ty.data {
                (*contour_interval, *major_every) = (interval, major);
            }
            if let Some(name) = name {
                ty.name = name;
            }
            Ok(e.add_type(ty))
        })?;
        Ok(json!({ "id": id.0 }))
    }
}

/// The figures `element_get` shows for a terrain surface.
pub(crate) fn derived(g: &cc_geom::topography::TopoGeom, derived: &mut serde_json::Map<String, Value>) {
    derived.insert("min_z".into(), json!(g.min_z));
    derived.insert("max_z".into(), json!(g.max_z));
    derived.insert("base_z".into(), json!(g.base_z));
    derived.insert("area".into(), json!(g.area));
    derived.insert("triangles".into(), json!(g.triangles.len()));
    let warnings: Vec<&str> = g.warnings.iter().map(|w| w.code()).collect();
    derived.insert("warnings".into(), json!(warnings));
}
