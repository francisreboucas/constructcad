//! Conceptual masses: creating them (an extrusion, or a loft to another
//! outline) and choosing and reading their mass floors.

use super::{add_element, created, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, finite, point, positive};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::mass::{MAX_OUTLINE, Mass};
use cc_core::{Category, ElementId, ElementKind, ElementType, ElementTypeId, LevelId, Project, TypeData};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Create a conceptual mass: `outline` extruded `height` up from its level
/// (plus `base_offset`), or lofted to another outline (`top`, or the base
/// outline set in by `top_inset`). Mass floors are taken at every level the
/// mass spans unless `floors` lists them. A project without a mass type
/// gets one.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct MassCreate {
    /// Base outline `[[x, y], …]` in plan, closed (at least three points); or `curves`.
    #[serde(default)]
    pub outline: Vec<Point>,
    /// The base outline drawn with curves (lines, arcs, circles, ellipses,
    /// rectangles, polygons) in one loop, instead of `outline`; the mass keeps them.
    #[serde(default)]
    pub curves: Option<Vec<super::sketch::CurveArg>>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Height above the base, meters (default: up to the level above, else 3).
    #[serde(default)]
    pub height: Option<f64>,
    /// Offset of the base from the level elevation, meters (default 0).
    #[serde(default)]
    pub base_offset: Option<f64>,
    /// Top outline of a loft `[[x, y], …]` (default: the base outline, an extrusion).
    #[serde(default)]
    pub top: Option<Vec<Point>>,
    /// Loft to the base outline set in by this distance, meters (instead of `top`).
    #[serde(default)]
    pub top_inset: Option<f64>,
    /// Levels (ids or names) where mass floors are taken (default: every level the mass spans).
    #[serde(default)]
    pub floors: Option<Vec<Ref>>,
}

/// An outline from command points: at least three finite points, at most [`MAX_OUTLINE`].
fn outline(points: &[Point], field: &str) -> Result<Vec<DVec2>> {
    if points.len() < 3 || points.len() > MAX_OUTLINE {
        return Err(EngineError::invalid(format!("{field} needs 3 to {MAX_OUTLINE} points")));
    }
    let pts = points.iter().map(|p| point(*p)).collect::<Result<Vec<_>>>()?;
    if cc_core::mass::area(&pts) < 1e-6 {
        return Err(EngineError::invalid(format!("{field} encloses no area")));
    }
    Ok(pts)
}

/// The levels a mass from `base` to `top` (elevations) spans: at or above
/// the base, below the top.
pub fn spanned_levels(p: &Project, base: f64, top: f64) -> Vec<LevelId> {
    p.levels.iter().filter(|l| l.elevation >= base - 1e-9 && l.elevation < top - 1e-9).map(|l| l.id).collect()
}

/// Levels named in a command, or every level the mass spans.
fn floor_levels(p: &Project, refs_: Option<&[Ref]>, base: f64, top: f64) -> Result<Vec<LevelId>> {
    match refs_ {
        Some(list) => {
            let mut ids = list.iter().map(|r| refs::level(p, r)).collect::<Result<Vec<_>>>()?;
            ids.sort_by(|a, b| p.level_elevation(*a).total_cmp(&p.level_elevation(*b)));
            ids.dedup();
            Ok(ids)
        }
        None => Ok(spanned_levels(p, base, top)),
    }
}

/// A mass type for a project that has none, in its first material.
pub fn new_mass_type(p: &Project, id: ElementTypeId) -> ElementType {
    ElementType {
        id,
        name: t!("tpl-type-mass"),
        data: TypeData::Mass { material: p.materials.first().map(|m| m.name.clone()).unwrap_or_default() },
        info: Default::default(),
        uid: Default::default(),
    }
}

impl Command for MassCreate {
    const NAME: &'static str = "mass_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let base_offset = finite(self.base_offset.unwrap_or(0.0), "base_offset")?;
        let height = match self.height {
            Some(h) => positive(h, "height")?,
            None => p.level_height(level, 3.0).max(0.1),
        };
        let (base, drawn) = match &self.curves {
            Some(_) if !self.outline.is_empty() => {
                return Err(EngineError::invalid("give outline or curves, not both"));
            }
            Some(curves) => {
                let drawn = super::sketch::sketch_of(curves)?;
                let base = super::sketch::outline(&drawn)?;
                if base.len() > MAX_OUTLINE {
                    return Err(EngineError::invalid(format!("the outline makes more than {MAX_OUTLINE} points")));
                }
                (base, Some(drawn))
            }
            None => (outline(&self.outline, "outline")?, None),
        };
        let top = match (&self.top, self.top_inset) {
            (Some(_), Some(_)) => return Err(EngineError::invalid("give top or top_inset, not both")),
            (Some(top), None) => Some(outline(top, "top")?),
            (None, Some(inset)) => {
                let inset = positive(inset, "top_inset")?;
                let top = cc_geom::polygon::offset(&base, -inset);
                if top.len() < 3
                    || cc_core::mass::area(&top) < 1e-6
                    || cc_core::mass::area(&top) >= cc_core::mass::area(&base)
                {
                    return Err(EngineError::invalid("top_inset leaves no top outline"));
                }
                Some(top)
            }
            (None, None) => None,
        };
        let base_z = p.level_elevation(level) + base_offset;
        let floors = floor_levels(p, self.floors.as_deref(), base_z, base_z + height)?;
        let ty = match (&self.type_ref, p.default_type(Category::Mass)) {
            (None, None) => None,
            (r, _) => Some(refs::type_of(p, Category::Mass, r.as_ref())?),
        };
        let mass = Mass { outline: base, base_offset, height, top, floors, sketch: drawn };
        let id = edit(s, t!("tx-create-mass"), |e| {
            let type_id = match ty {
                Some(t) => t,
                None => {
                    let id = ElementTypeId(e.alloc_id());
                    e.add_type(new_mass_type(e.project(), id))
                }
            };
            Ok(add_element(e, level, type_id, ElementKind::Mass(mass)))
        })?;
        Ok(created(s, id))
    }
}

/// Choose the levels where a mass takes mass floors: `levels` (ids or
/// names), or every level it spans when `levels` is left out; an empty list
/// takes none.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MassFloorsSet {
    /// Mass element id.
    pub id: u64,
    /// Levels (ids or names); left out: every level the mass spans.
    #[serde(default)]
    pub levels: Option<Vec<Ref>>,
}

fn mass_of(p: &Project, id: u64) -> Result<(&cc_core::Element, &Mass)> {
    let e = p.element(ElementId(id)).ok_or_else(|| EngineError::not_found(format!("no element {id}")))?;
    match &e.kind {
        ElementKind::Mass(m) => Ok((e, m)),
        _ => Err(EngineError::invalid(format!("element {id} is not a mass"))),
    }
}

impl Command for MassFloorsSet {
    const NAME: &'static str = "mass_floors_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let (e, m) = mass_of(p, self.id)?;
        let base = p.level_elevation(e.level) + m.base_offset;
        let floors = floor_levels(p, self.levels.as_deref(), base, base + m.height)?;
        let id = e.id;
        edit(s, t!("tx-modify"), |e| {
            if let Some(ElementKind::Mass(m)) = e.element_mut(id).map(|x| &mut x.kind) {
                m.floors = floors;
            }
            Ok(())
        })?;
        Ok(created(s, id))
    }
}

/// The mass floors of a mass: for each, its level, elevation, area and
/// perimeter (and outline), with the gross floor area and the volume.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MassFloors {
    /// Mass element id.
    pub id: u64,
}

/// A mass's floors as JSON.
pub fn floors_json(p: &Project, g: &cc_geom::mass::MassGeom) -> Vec<Value> {
    g.floors
        .iter()
        .map(|f| {
            json!({
                "level": {"id": f.level.0, "name": p.level_name(f.level)},
                "elevation": p.level_elevation(f.level),
                "area": f.area,
                "perimeter": f.perimeter,
                "outline": f.outline.iter().map(|q| [q.x, q.y]).collect::<Vec<_>>(),
            })
        })
        .collect()
}

impl Command for MassFloors {
    const NAME: &'static str = "mass_floors";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = mass_of(s.project(), self.id)?.0.id;
        s.geometry();
        let g = s.geom().masses.get(&id).cloned().ok_or_else(|| EngineError::invalid("the mass is degenerate"))?;
        let p = s.project();
        Ok(json!({
            "floors": floors_json(p, &g),
            "floor_area": g.floor_area(),
            "volume": g.volume,
        }))
    }
}

/// Create a mass type, shown in `material` (default: the project's first material).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct MassTypeCreate {
    /// Type name (default: the template's mass name).
    #[serde(default)]
    pub name: Option<String>,
    /// Material name.
    #[serde(default)]
    pub material: Option<String>,
}

impl Command for MassTypeCreate {
    const NAME: &'static str = "mass_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let material = match self.material {
            Some(m) => Some(refs::material(s.project(), &Ref::Name(m))?),
            None => None,
        };
        let name = self.name.filter(|n| !n.trim().is_empty());
        let id = edit(s, t!("tx-create-mass-type"), |e| {
            let id = ElementTypeId(e.alloc_id());
            let mut ty = new_mass_type(e.project(), id);
            if let (TypeData::Mass { material: m }, Some(name)) = (&mut ty.data, material) {
                *m = name;
            }
            if let Some(name) = name {
                ty.name = name;
            }
            Ok(e.add_type(ty))
        })?;
        Ok(json!({ "id": id.0 }))
    }
}

/// Push or pull a face of a mass `distance` meters outward (inward when
/// negative): `top` changes its height, `bottom` its base offset and
/// height, and `side` (with `side`, the index of the base outline's edge
/// from that point to the next) moves that side, the sides beside it
/// keeping their directions. A loft's sides are not moved.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MassFaceMove {
    /// Mass element id.
    pub id: u64,
    /// `top`, `bottom` or `side`.
    pub face: String,
    /// The side's edge of the base outline (with `face: "side"`).
    #[serde(default)]
    pub side: Option<usize>,
    /// Distance outward, meters (negative: inward).
    pub distance: f64,
}

impl Command for MassFaceMove {
    const NAME: &'static str = "mass_face_move";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        use cc_core::mass::MassFace;
        let (e, m) = mass_of(s.project(), self.id)?;
        let face = match (self.face.as_str(), self.side) {
            ("top", None) => MassFace::Top,
            ("bottom", None) => MassFace::Bottom,
            ("side", Some(i)) if i < m.outline.len() => MassFace::Side(i),
            ("side", _) => return Err(EngineError::invalid(format!("side must be 0 to {}", m.outline.len() - 1))),
            _ => return Err(EngineError::invalid("face must be top, bottom or side (with side)")),
        };
        let distance = finite(self.distance, "distance")?;
        if m.top.is_some() && matches!(face, MassFace::Side(_)) {
            return Err(EngineError::rejected(t!("msg-loft-sides")));
        }
        let mut moved = m.pushed(face, distance).ok_or_else(|| EngineError::rejected(t!("msg-mass-face-too-far")))?;
        // A mass with a floor at every level it spans keeps one at every level it spans now.
        let p = s.project();
        let span = |m: &Mass| {
            let base = p.level_elevation(e.level) + m.base_offset;
            spanned_levels(p, base, base + m.height)
        };
        let set = |l: &[LevelId]| l.iter().copied().collect::<std::collections::BTreeSet<_>>();
        if set(&m.floors) == set(&span(m)) {
            moved.floors = span(&moved);
        }
        let id = e.id;
        edit(s, t!("tx-push-pull"), |e| {
            if let Some(ElementKind::Mass(m)) = e.element_mut(id).map(|x| &mut x.kind) {
                *m = moved;
            }
            Ok(())
        })?;
        Ok(created(s, id))
    }
}

/// The ids of new elements as JSON.
fn ids(list: &[ElementId]) -> Value {
    json!(list.iter().map(|i| i.0).collect::<Vec<_>>())
}

/// Walls by face: a wall over each side of a mass extruded straight up
/// (`sides`: the base outline's edges, by default all), its exterior face
/// on the mass's face, from the mass's base up its height. Walls meeting at
/// the mass's corners join. A loft's slanted sides take no walls.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MassWallsCreate {
    /// Mass element id.
    pub id: u64,
    /// Wall type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Edges of the base outline to put walls on (default: every edge).
    #[serde(default)]
    pub sides: Option<Vec<usize>>,
}

impl Command for MassWallsCreate {
    const NAME: &'static str = "mass_walls_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let (e, m) = mass_of(p, self.id)?;
        if m.top.is_some() {
            return Err(EngineError::rejected(t!("msg-loft-walls")));
        }
        let n = m.outline.len();
        let sides = self.sides.unwrap_or_else(|| (0..n).collect());
        if let Some(bad) = sides.iter().find(|i| **i >= n) {
            return Err(EngineError::invalid(format!("side {bad} is not an edge (0 to {})", n - 1)));
        }
        let type_id = refs::type_of(p, Category::Wall, self.type_ref.as_ref())?;
        // Walls keep their exterior on the left of their line: the outline runs
        // clockwise for that, with the line on the exterior face.
        let ccw = cc_geom::polygon::signed_area(&m.outline) > 0.0;
        let walls: Vec<cc_core::Wall> = sides
            .iter()
            .filter_map(|&i| {
                let (a, b) = (*m.outline.get(i)?, *m.outline.get((i + 1) % n)?);
                let (start, end) = if ccw { (b, a) } else { (a, b) };
                (start.distance(end) > 1e-3).then(|| cc_core::Wall {
                    start,
                    end,
                    base_offset: m.base_offset,
                    height: m.height,
                    location_line: cc_core::LocationLine::FinishExterior,
                    ..Default::default()
                })
            })
            .collect();
        let level = e.level;
        let created = edit(s, t!("tx-mass-walls"), |e| {
            Ok(walls.into_iter().map(|w| add_element(e, level, type_id, ElementKind::Wall(w))).collect::<Vec<_>>())
        })?;
        Ok(json!({ "created": ids(&created) }))
    }
}

/// Floors from mass floors: a floor on each mass floor of a mass (or those
/// at `levels`), its outline the mass's at that level and its top at the level.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MassSlabsCreate {
    /// Mass element id.
    pub id: u64,
    /// Floor type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Levels (ids or names) of the mass floors to build (default: all of them).
    #[serde(default)]
    pub levels: Option<Vec<Ref>>,
}

impl Command for MassSlabsCreate {
    const NAME: &'static str = "mass_slabs_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = mass_of(s.project(), self.id)?.0.id;
        let p = s.project();
        let type_id = refs::type_of(p, Category::Floor, self.type_ref.as_ref())?;
        let only = self
            .levels
            .as_ref()
            .map(|l| l.iter().map(|r| refs::level(p, r)).collect::<Result<Vec<_>>>())
            .transpose()?;
        s.geometry();
        let g = s.geom().masses.get(&id).cloned().ok_or_else(|| EngineError::invalid("the mass is degenerate"))?;
        let floors: Vec<_> = g
            .floors
            .iter()
            .filter(|f| only.as_ref().is_none_or(|l| l.contains(&f.level)))
            .map(|f| (f.level, cc_core::Floor::new(f.outline.clone(), 0.0)))
            .collect();
        if floors.is_empty() {
            return Err(EngineError::rejected(t!("msg-no-mass-floors")));
        }
        let created = edit(s, t!("tx-mass-slabs"), |e| {
            Ok(floors.into_iter().map(|(l, f)| add_element(e, l, type_id, ElementKind::Floor(f))).collect::<Vec<_>>())
        })?;
        Ok(json!({ "created": ids(&created) }))
    }
}

/// Roof by face: a flat roof on the top face of a mass, over its top
/// outline, on the mass's level at the height of its top.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MassRoofCreate {
    /// Mass element id.
    pub id: u64,
    /// Roof type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// How far the eaves reach past the top outline, meters (default 0).
    #[serde(default)]
    pub overhang: Option<f64>,
}

impl Command for MassRoofCreate {
    const NAME: &'static str = "mass_roof_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let (e, m) = mass_of(p, self.id)?;
        let type_id = refs::type_of(p, Category::Roof, self.type_ref.as_ref())?;
        let overhang = finite(self.overhang.unwrap_or(0.0), "overhang")?.clamp(0.0, 10.0);
        let roof = cc_core::Roof::new(m.section(1.0), m.base_offset + m.height, overhang, cc_core::RoofShape::Flat);
        let level = e.level;
        let id = edit(s, t!("tx-mass-roof"), |e| Ok(add_element(e, level, type_id, ElementKind::Roof(roof))))?;
        Ok(created(s, id))
    }
}
