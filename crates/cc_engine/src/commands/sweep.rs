//! Swept profiles: a profile (a closed outline held by the type) carried
//! along a path in plan, for baseboards, crown moldings, fascias, gutters and
//! copings. Creating them, creating their types and redrawing a type's profile.

use super::sketch::CurveArg;
use super::{add_element, created, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, finite, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::profile_sweep::{MAX_PATH_POINTS, Sweep, baseboard, check_profile};
use cc_core::{Category, ElementKind, ElementType, ElementTypeId, Project, TypeData};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The sweep type a project gets with its first sweep: a baseboard.
pub fn new_sweep_type(p: &Project, id: ElementTypeId) -> ElementType {
    ElementType {
        id,
        name: t!("tpl-type-baseboard"),
        data: TypeData::Sweep {
            profile: baseboard(),
            material: p.materials.first().map(|m| m.name.clone()).unwrap_or_default(),
        },
        info: Default::default(),
        uid: Default::default(),
    }
}

/// A profile from points or curves (one of them), checked.
fn profile_from(points: Option<&[Point]>, curves: Option<&[CurveArg]>) -> Result<Option<Vec<DVec2>>> {
    let profile = match (points, curves) {
        (Some(_), Some(_)) => return Err(EngineError::invalid("give profile or curves, not both")),
        (Some(pts), None) => pts.iter().map(|p| point(*p)).collect::<Result<Vec<_>>>()?,
        (None, Some(curves)) => {
            let sketch = super::sketch::sketch_of(curves)?;
            super::sketch::outline(&sketch)?
        }
        (None, None) => return Ok(None),
    };
    check_profile(&profile).map_err(|p| EngineError::invalid(p.message()))?;
    Ok(Some(profile))
}

/// Create a swept profile: the profile of its type carried along `path` in
/// plan, `base_offset` above the level, on the left of the path walking
/// along it (`flip` puts it on the right); `closed` runs the path back to
/// its first point (around a room). A project without a sweep type gets
/// one (a baseboard).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct SweepCreate {
    /// Points of the path in plan (at least 2; 3 when closed).
    pub path: Vec<Point>,
    /// Run the path back to its first point.
    #[serde(default)]
    pub closed: bool,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Height of the path above the level, meters (default 0).
    #[serde(default)]
    pub base_offset: Option<f64>,
    /// Put the profile on the right of the path.
    #[serde(default)]
    pub flip: bool,
}

impl Command for SweepCreate {
    const NAME: &'static str = "sweep_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        if self.path.len() > MAX_PATH_POINTS {
            return Err(EngineError::invalid(format!("the path has more than {MAX_PATH_POINTS} points")));
        }
        let mut path = self.path.iter().map(|p| point(*p)).collect::<Result<Vec<_>>>()?;
        path.dedup_by(|a, b| a.distance(*b) < 1e-6);
        if self.closed && path.len() > 1 && path.first().zip(path.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-6) {
            path.pop();
        }
        let needed = if self.closed { 3 } else { 2 };
        if path.len() < needed {
            return Err(EngineError::invalid(format!("the path needs at least {needed} distinct points")));
        }
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let base_offset = finite(self.base_offset.unwrap_or(0.0), "base_offset")?;
        let ty = match (&self.type_ref, p.default_type(Category::Sweep)) {
            (None, None) => None,
            (r, _) => Some(refs::type_of(p, Category::Sweep, r.as_ref())?),
        };
        let sweep = Sweep { path, closed: self.closed, base_offset, flip: self.flip };
        let id = edit(s, t!("tx-create-sweep"), |e| {
            let type_id = match ty {
                Some(t) => t,
                None => {
                    let id = ElementTypeId(e.alloc_id());
                    e.add_type(new_sweep_type(e.project(), id))
                }
            };
            Ok(add_element(e, level, type_id, ElementKind::Sweep(sweep)))
        })?;
        Ok(created(s, id))
    }
}

/// Create a sweep type: its profile drawn as points `[x, y]` (x across, to
/// the left of the path; y up from it, meters) or as `curves` (lines, arcs,
/// circles, rectangles… making one closed outline), shown in `material`.
/// Without a profile it is a baseboard 1.5 cm × 10 cm.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct SweepTypeCreate {
    /// Type name (default: the template's baseboard name).
    #[serde(default)]
    pub name: Option<String>,
    /// The profile's points, in order (a closed outline that does not cross itself).
    #[serde(default)]
    pub profile: Option<Vec<Point>>,
    /// The profile drawn with curves instead of points.
    #[serde(default)]
    pub curves: Option<Vec<CurveArg>>,
    /// Material name (default: the project's first material).
    #[serde(default)]
    pub material: Option<String>,
}

impl Command for SweepTypeCreate {
    const NAME: &'static str = "sweep_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let profile = profile_from(self.profile.as_deref(), self.curves.as_deref())?;
        let material = match self.material {
            Some(m) => Some(refs::material(s.project(), &Ref::Name(m))?),
            None => None,
        };
        let name = self.name.filter(|n| !n.trim().is_empty());
        let id = edit(s, t!("tx-create-sweep-type"), |e| {
            let id = ElementTypeId(e.alloc_id());
            let mut ty = new_sweep_type(e.project(), id);
            if let TypeData::Sweep { profile: p, material: m } = &mut ty.data {
                if let Some(profile) = profile {
                    *p = profile;
                }
                if let Some(material) = material {
                    *m = material;
                }
            }
            if let Some(name) = name {
                ty.name = name;
            }
            Ok(e.add_type(ty))
        })?;
        Ok(json!({ "id": id.0 }))
    }
}

/// Redraw the profile of a sweep type, as points `[x, y]` or `curves`; every
/// sweep of the type follows.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SweepProfileSet {
    /// Sweep type id or name.
    #[serde(rename = "type")]
    pub type_ref: Ref,
    /// The profile's points, in order.
    #[serde(default)]
    pub profile: Option<Vec<Point>>,
    /// The profile drawn with curves instead of points.
    #[serde(default)]
    pub curves: Option<Vec<CurveArg>>,
}

impl Command for SweepProfileSet {
    const NAME: &'static str = "sweep_profile_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let profile = profile_from(self.profile.as_deref(), self.curves.as_deref())?
            .ok_or_else(|| EngineError::invalid("give profile or curves"))?;
        let p = s.project();
        let id = refs::element_type(p, &self.type_ref)?;
        if !matches!(p.element_type(id).map(|t| &t.data), Some(TypeData::Sweep { .. })) {
            return Err(EngineError::invalid("not a sweep type"));
        }
        let points = profile.len();
        edit(s, t!("tx-modify-type"), |e| {
            if let Some(TypeData::Sweep { profile: p, .. }) = e.type_mut(id).map(|t| &mut t.data) {
                *p = profile;
            }
            Ok(())
        })?;
        Ok(json!({ "type": id.0, "points": points }))
    }
}
