//! Shaft openings: vertical openings through the floors, ceilings and roofs
//! between their base and top (see `cc_core::shaft`).

use super::create::top;
use super::sketch::CurveArg;
use super::{add_element, created, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, finite, point, positive};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::shaft::{MAX_OUTLINE, Shaft};
use cc_core::{Category, ElementKind, ElementType, ElementTypeId, LevelId, Project, TypeData, WallTop};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The shaft type a project gets with its first shaft.
pub fn new_shaft_type(id: ElementTypeId) -> ElementType {
    ElementType {
        id,
        name: t!("tpl-type-shaft"),
        data: TypeData::Shaft {},
        info: Default::default(),
        uid: Default::default(),
    }
}

/// A shaft's top by default: the highest level above its own, else unconnected.
fn default_shaft_top(p: &Project, level: LevelId) -> WallTop {
    let base = p.level_elevation(level);
    p.levels
        .iter()
        .filter(|l| l.elevation > base + 1e-9)
        .max_by(|a, b| a.elevation.total_cmp(&b.elevation))
        .map_or(WallTop::Unconnected, |l| WallTop::Level { level: l.id, offset: 0.0 })
}

/// Create a shaft: a vertical opening with `outline` in plan (or drawn
/// with `curves`), from its level plus `base_offset` up to `top_level`
/// (plus `top_offset`) or `height`. Every floor, ceiling and roof it passes
/// through is cut where the outline lies inside it, clear of its own
/// openings; the cuts follow the shaft when it moves, changes or is
/// deleted. A project without a shaft type gets one.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ShaftCreate {
    /// Outline `[[x, y], …]` in plan, closed (at least three points); or `curves`.
    #[serde(default)]
    pub outline: Vec<Point>,
    /// The outline drawn with curves (lines, arcs, circles, rectangles…).
    #[serde(default)]
    pub curves: Option<Vec<CurveArg>>,
    /// Level id or name of the bottom (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Bottom above the level, meters (default 0).
    #[serde(default)]
    pub base_offset: Option<f64>,
    /// Level the shaft rises to (default: the highest level above its own).
    #[serde(default)]
    pub top_level: Option<Ref>,
    /// Top above `top_level`, meters (default 0).
    #[serde(default)]
    pub top_offset: Option<f64>,
    /// Height when there is no level above, meters (default 3).
    #[serde(default)]
    pub height: Option<f64>,
    /// Type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
}

impl Command for ShaftCreate {
    const NAME: &'static str = "shaft_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (outline, drawn) = match &self.curves {
            Some(_) if !self.outline.is_empty() => {
                return Err(EngineError::invalid("give outline or curves, not both"));
            }
            Some(curves) => {
                let sketch = super::sketch::sketch_of(curves)?;
                (super::sketch::outline(&sketch)?, Some(sketch))
            }
            None => (self.outline.iter().map(|p| point(*p)).collect::<Result<Vec<DVec2>>>()?, None),
        };
        if outline.len() < 3 || outline.len() > MAX_OUTLINE {
            return Err(EngineError::invalid(format!("the outline needs 3 to {MAX_OUTLINE} points")));
        }
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let base_offset = finite(self.base_offset.unwrap_or(0.0), "base_offset")?;
        let top = top(p, self.top_level.as_ref(), self.top_offset)?.unwrap_or_else(|| default_shaft_top(p, level));
        let height = positive(self.height.unwrap_or(3.0), "height")?;
        let shaft = Shaft { outline, base_offset, height, top, sketch: drawn };
        if !shaft.is_valid() {
            return Err(EngineError::invalid("the outline encloses no area"));
        }
        if let WallTop::Level { level: t, offset } = top
            && p.level_elevation(t) + offset <= p.level_elevation(level) + base_offset + 1e-3
        {
            return Err(EngineError::invalid("the top must be above the bottom"));
        }
        let ty = match (&self.type_ref, p.default_type(Category::Shaft)) {
            (None, None) => None,
            (r, _) => Some(refs::type_of(p, Category::Shaft, r.as_ref())?),
        };
        let id = edit(s, t!("tx-create-shaft"), |e| {
            let type_id = match ty {
                Some(t) => t,
                None => {
                    let id = ElementTypeId(e.alloc_id());
                    e.add_type(new_shaft_type(id))
                }
            };
            Ok(add_element(e, level, type_id, ElementKind::Shaft(shaft)))
        })?;
        Ok(created(s, id))
    }
}

/// Create a shaft type (shafts have no properties of their own; types name them).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ShaftTypeCreate {
    /// Type name (default: the template's shaft name).
    #[serde(default)]
    pub name: Option<String>,
}

impl Command for ShaftTypeCreate {
    const NAME: &'static str = "shaft_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let name = self.name.filter(|n| !n.trim().is_empty());
        let id = edit(s, t!("tx-create-shaft-type"), |e| {
            let id = ElementTypeId(e.alloc_id());
            let mut ty = new_shaft_type(id);
            if let Some(name) = name {
                ty.name = name;
            }
            Ok(e.add_type(ty))
        })?;
        Ok(serde_json::json!({ "id": id.0 }))
    }
}
