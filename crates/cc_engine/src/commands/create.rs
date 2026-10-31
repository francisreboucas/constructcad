//! Creating elements, with the same defaults as the drawing tools.

use super::{add_element, created, edit};
use crate::defaults::{default_roof_base, default_top, next_number, walls_loop};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, finite, point, points, positive};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{
    Beam, BeamLateral, BeamVertical, Category, Ceiling, Column, Component, ElementId, ElementKind, Floor, Grid, Hosted,
    LevelId, LocationLine, Project, Railing, Roof, RoofShape, Room, Wall, WallTop,
};
use cc_geom::polygon;
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A top constraint: attached to a level (with an offset) or a fixed height.
pub(crate) fn top(p: &Project, top_level: Option<&Ref>, top_offset: Option<f64>) -> Result<Option<WallTop>> {
    match top_level {
        Some(r) => Ok(Some(WallTop::Level {
            level: refs::level(p, r)?,
            offset: finite(top_offset.unwrap_or(0.0), "top_offset")?,
        })),
        None => Ok(None),
    }
}

pub(crate) fn create(
    s: &mut Session,
    tx: String,
    level: LevelId,
    category: Category,
    ty: Option<&Ref>,
    kind: ElementKind,
) -> Result<Value> {
    let type_id = refs::type_of(s.project(), category, ty)?;
    let id = edit(s, tx, |e| Ok(add_element(e, level, type_id, kind)))?;
    Ok(created(s, id))
}

/// Create a wall from `start` to `end` on a level: straight, or an arc
/// through `mid` (or with a `bulge` or a `radius`). Its top is `height` above its base,
/// or attached to `top_level` (plus `top_offset`).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct WallCreate {
    /// Start point of the wall line.
    pub start: Point,
    /// End point of the wall line.
    pub end: Point,
    /// A point on the arc between the ends: makes an arc wall through the three points.
    #[serde(default)]
    pub mid: Option<Point>,
    /// Arc walls: tan(sweep / 4), positive when the arc turns counter-clockwise from `start`
    /// to `end` (1 is a half circle; at most 10 either way). Ignored when `mid` is given.
    #[serde(default)]
    pub bulge: Option<f64>,
    /// Arc walls: the radius in meters, positive when the arc turns counter-clockwise from
    /// `start` to `end`, negative clockwise; at least half the distance between the ends.
    /// Ignored when `mid` or `bulge` is given.
    #[serde(default)]
    pub radius: Option<f64>,
    /// With `radius`: take the longer of the two arcs (default: the shorter).
    #[serde(default)]
    pub major_arc: Option<bool>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Wall type id or name (default: the default wall type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Unconnected height in meters (default 3).
    #[serde(default)]
    pub height: Option<f64>,
    /// Level the top is attached to, instead of a height.
    #[serde(default)]
    pub top_level: Option<Ref>,
    /// Offset of the top from `top_level`, meters.
    #[serde(default)]
    pub top_offset: Option<f64>,
    /// Base offset from the level, meters.
    #[serde(default)]
    pub base_offset: Option<f64>,
    /// Which line of the wall `start`-`end` is: `Centerline` (default), `CoreCenter`,
    /// `FinishExterior`, `FinishInterior`, `CoreExterior` or `CoreInterior`.
    #[serde(default)]
    pub location_line: Option<String>,
}

impl Command for WallCreate {
    const NAME: &'static str = "wall_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let (start, end) = (point(self.start)?, point(self.end)?);
        if start.distance(end) < 1e-3 {
            return Err(EngineError::invalid("the wall has no length"));
        }
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let location_line = match self.location_line {
            Some(name) => serde_json::from_value::<LocationLine>(Value::String(name.clone()))
                .map_err(|_| EngineError::invalid(format!("unknown location line '{name}'")))?,
            None => LocationLine::default(),
        };
        let bulge = match (self.mid, self.bulge) {
            (Some(m), _) => cc_core::arc::Arc::bulge_through(start, point(m)?, end)
                .ok_or_else(|| EngineError::invalid("the three points of the arc are on a line"))?,
            (None, Some(b)) => {
                let b = finite(b, "bulge")?;
                if b.abs() > cc_core::arc::MAX_BULGE {
                    return Err(EngineError::invalid("the bulge must be between -10 and 10"));
                }
                b
            }
            (None, None) => match self.radius {
                Some(r) => {
                    let r = finite(r, "radius")?;
                    let major = self.major_arc.unwrap_or(false);
                    cc_core::arc::Arc::bulge_radius(start, end, r.abs(), r > 0.0, major).ok_or_else(|| {
                        EngineError::invalid("the radius must be at least half the distance between the ends")
                    })?
                }
                None => 0.0,
            },
        };
        let wall = Wall {
            start,
            end,
            bulge,
            base_offset: finite(self.base_offset.unwrap_or(0.0), "base_offset")?,
            height: positive(self.height.unwrap_or(3.0), "height")?,
            top: top(p, self.top_level.as_ref(), self.top_offset)?.unwrap_or(WallTop::Unconnected),
            location_line,
            ..Default::default()
        };
        let type_id = refs::type_of(p, Category::Wall, self.type_ref.as_ref())?;
        check_arc(p, type_id, &wall)?;
        create(s, t!("tx-create-wall"), level, Category::Wall, self.type_ref.as_ref(), ElementKind::Wall(wall))
    }
}

/// An arc wall must leave room inside its curve for its thickness.
pub(crate) fn check_arc(p: &Project, type_id: cc_core::ElementTypeId, wall: &Wall) -> Result<()> {
    let Some(arc) = wall.arc() else { return Ok(()) };
    let thickness = p.element_type(type_id).map_or(0.0, |t| t.thickness());
    // The tightest face may be the location line moved by up to the whole thickness.
    if arc.radius <= thickness + 1e-3 {
        return Err(EngineError::rejected("the arc is too tight for the thickness of the wall"));
    }
    Ok(())
}

/// Parameters shared by doors and windows.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct HostedArgs {
    /// Id of the host wall; omit it to use the wall nearest `point`.
    #[serde(default)]
    pub wall: Option<u64>,
    /// Distance along the wall from its start to the opening's center, meters.
    #[serde(default)]
    pub offset: Option<f64>,
    /// A point near the wall: the opening is centered at its projection, on its side.
    #[serde(default)]
    pub point: Option<Point>,
    /// Sill height (default: the type's default sill for windows, 0 for doors).
    #[serde(default)]
    pub sill: Option<f64>,
    /// Door or window type id or name (default: the default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Face the other side of the wall.
    #[serde(default)]
    pub flip_facing: Option<bool>,
    /// Hinge on the other jamb.
    #[serde(default)]
    pub flip_hand: Option<bool>,
}

fn place_hosted(s: &mut Session, a: HostedArgs, category: Category) -> Result<Value> {
    let p = s.project();
    let type_id = refs::type_of(p, category, a.type_ref.as_ref())?;
    let host = match (a.wall, a.point) {
        (Some(w), _) => refs::element(p, w)?,
        (None, Some(q)) => nearest_wall(p, point(q)?)?,
        (None, None) => return Err(EngineError::invalid("give the wall or a point near it")),
    };
    let host_el = p.element(host).ok_or_else(|| EngineError::not_found("wall"))?;
    let wall = host_el.as_wall().ok_or_else(|| EngineError::invalid(format!("element {} is not a wall", host.0)))?;
    let ty = p.element_type(type_id).ok_or_else(|| EngineError::not_found("type"))?;
    let (width, _) = ty.opening_size().ok_or_else(|| EngineError::invalid("not a door or window type"))?;
    let default_sill = ty.default_sill();
    // Openings are placed along the body centerline (an arc for arc walls).
    let (start, end) = p.wall_axis(host_el, wall);
    let axis = Wall { start, end, ..wall.clone() };
    let length = axis.length();
    let (u, side) = match (a.offset, a.point) {
        (Some(o), _) => (finite(o, "offset")?, 1.0),
        (None, Some(q)) => axis.project(point(q)?),
        (None, None) => (length / 2.0, 1.0),
    };
    let margin = 0.01;
    if length < width + 2.0 * margin {
        return Err(EngineError::rejected("the wall is too short for this opening"));
    }
    let hosted = Hosted {
        host,
        offset: u.clamp(width * 0.5 + margin, length - width * 0.5 - margin),
        sill_height: finite(a.sill.unwrap_or(default_sill), "sill")?.max(0.0),
        flip_facing: a.flip_facing.unwrap_or(side < 0.0),
        flip_hand: a.flip_hand.unwrap_or(false),
    };
    let level = host_el.level;
    let (kind, tx) = if category == Category::Door {
        (ElementKind::Door(hosted), t!("tx-create-door"))
    } else {
        (ElementKind::Window(hosted), t!("tx-create-window"))
    };
    let id = edit(s, tx, |e| Ok(super::add_element(e, level, type_id, kind)))?;
    Ok(created(s, id))
}

/// The wall whose axis passes nearest `q` (within half its thickness plus 0.5 m).
fn nearest_wall(p: &Project, q: DVec2) -> Result<cc_core::ElementId> {
    p.walls()
        .map(|(e, w)| {
            let (a, b) = p.wall_axis(e, w);
            let distance = match w.arc() {
                Some(_) => {
                    let axis = Wall { start: a, end: b, ..w.clone() };
                    let (u, v) = axis.project(q);
                    if (0.0..=axis.length()).contains(&u) { v.abs() } else { f64::INFINITY }
                }
                None => polygon::closest_on_segment(a, b, q).0.distance(q),
            };
            (e.id, distance, p.thickness_of(e))
        })
        .filter(|(_, d, t)| *d <= t / 2.0 + 0.5)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _, _)| id)
        .ok_or_else(|| EngineError::not_found("no wall near the point"))
}

/// Place a door in a wall (centered on the wall unless `offset` or `point` is given).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct DoorCreate {
    #[serde(flatten)]
    pub args: HostedArgs,
}

impl Command for DoorCreate {
    const NAME: &'static str = "door_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        place_hosted(s, self.args, Category::Door)
    }
}

/// Place a window in a wall (centered on the wall unless `offset` or `point` is given).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct WindowCreate {
    #[serde(flatten)]
    pub args: HostedArgs,
}

impl Command for WindowCreate {
    const NAME: &'static str = "window_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        place_hosted(s, self.args, Category::Window)
    }
}

/// Parameters shared by floors, ceilings and roofs.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct SketchArgs {
    /// Closed outline (at least 3 points); omit with `from_walls` or `curves`.
    #[serde(default)]
    pub boundary: Option<Vec<Point>>,
    /// The outline drawn with curves (lines, arcs, circles, ellipses, rectangles,
    /// polygons) instead of `boundary`; loops inside the outline are openings
    /// (floors and roofs). The element keeps the curves (see `sketch_get`).
    #[serde(default)]
    pub curves: Option<Vec<super::sketch::CurveArg>>,
    /// Follow the outer boundary of the walls of the level, leaving interior walls out
    /// (exterior faces; interior faces for ceilings).
    #[serde(default)]
    pub from_walls: bool,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Floors: top above the level (default 0). Ceilings: bottom above the level (default 2.6).
    /// Roofs: base above the level (default: top of the tallest wall).
    #[serde(default)]
    pub offset: Option<f64>,
}

/// The level, outline and openings of a sketched element, and the curves it
/// was drawn with (`curves`).
pub(crate) fn sketched(
    s: &Session,
    a: &SketchArgs,
    category: Category,
) -> Result<(LevelId, cc_core::sketch::Profile, Option<cc_core::sketch::Sketch>)> {
    let Some(curves) = &a.curves else {
        let (level, outline) = sketch(s, a, category)?;
        return Ok((level, cc_core::sketch::Profile { outline, openings: Vec::new() }, None));
    };
    if a.boundary.is_some() || a.from_walls {
        return Err(EngineError::invalid("give curves, a boundary or from_walls, only one"));
    }
    let level = refs::level_or_first(s.project(), a.level.as_ref())?;
    let drawn = super::sketch::sketch_of(curves)?;
    let profile = super::sketch::profile(&drawn)?;
    Ok((level, profile, Some(drawn)))
}

/// The level and outline of an element with no openings.
pub(crate) fn sketch(s: &Session, a: &SketchArgs, category: Category) -> Result<(LevelId, Vec<DVec2>)> {
    let p = s.project();
    let level = refs::level_or_first(p, a.level.as_ref())?;
    let pts = match (&a.boundary, a.from_walls) {
        _ if a.curves.is_some() => {
            let drawn = super::sketch::sketch_of(a.curves.as_deref().unwrap_or_default())?;
            super::sketch::outline(&drawn)?
        }
        (Some(b), _) => points(b)?,
        (None, true) => {
            let outward = if category == Category::Ceiling { -1.0 } else { 1.0 };
            walls_loop(p, level, outward).ok_or_else(|| EngineError::rejected("the walls of the level form no loop"))?
        }
        (None, false) => return Err(EngineError::invalid("give a boundary or from_walls")),
    };
    if pts.len() < 3 || polygon::area(&pts) < 1e-4 {
        return Err(EngineError::invalid("the outline needs at least 3 points and an area"));
    }
    Ok((level, pts))
}

/// Create a floor slab from an outline (or the loop of walls).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct FloorCreate {
    #[serde(flatten)]
    pub sketch: SketchArgs,
    /// Openings through the slab (shafts, stairwells): closed outlines inside
    /// the boundary, apart from each other.
    #[serde(default)]
    pub openings: Vec<Vec<Point>>,
}

impl Command for FloorCreate {
    const NAME: &'static str = "floor_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (level, profile, drawn) = sketched(s, &self.sketch, Category::Floor)?;
        if drawn.is_some() && !self.openings.is_empty() {
            return Err(EngineError::invalid("with curves, draw the openings as loops inside the outline"));
        }
        let mut floor = Floor::new(profile.outline, finite(self.sketch.offset.unwrap_or(0.0), "offset")?);
        floor.openings = match drawn {
            Some(_) => profile.openings,
            None => self.openings.iter().map(|o| points(o)).collect::<Result<_>>()?,
        };
        floor.sketch = drawn;
        check_openings(&floor.boundary, &floor.openings)?;
        create(
            s,
            t!("tx-create-floor"),
            level,
            Category::Floor,
            self.sketch.type_ref.as_ref(),
            ElementKind::Floor(floor),
        )
    }
}

/// Create a ceiling from an outline (or the inside of the loop of walls).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct CeilingCreate {
    #[serde(flatten)]
    pub sketch: SketchArgs,
}

impl Command for CeilingCreate {
    const NAME: &'static str = "ceiling_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (level, profile, drawn) = sketched(s, &self.sketch, Category::Ceiling)?;
        if let Some(at) = profile.openings.first().and_then(|o| o.first()) {
            return Err(super::sketch::sketch_error(cc_core::sketch::SketchError::Openings(*at)));
        }
        let height_offset = finite(self.sketch.offset.unwrap_or(2.6), "offset")?;
        let ceiling = Ceiling { boundary: profile.outline, height_offset, sketch: drawn };
        let kind = ElementKind::Ceiling(ceiling);
        create(s, t!("tx-create-ceiling"), level, Category::Ceiling, self.sketch.type_ref.as_ref(), kind)
    }
}

/// Create a roof over an outline (or the loop of walls): flat by default; change its
/// shape (shed, gable, hip) with `element_set`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct RoofCreate {
    #[serde(flatten)]
    pub sketch: SketchArgs,
    /// Overhang beyond the outline, meters (default 0.5).
    #[serde(default)]
    pub overhang: Option<f64>,
    /// Openings through the roof (courtyards): closed outlines inside the boundary,
    /// apart from each other. Pitched roofs slope down to their eaves too.
    #[serde(default)]
    pub holes: Vec<Vec<Point>>,
}

/// A roof's openings must be outlines with an area, inside its footprint
/// (clear of its edges) and apart from each other.
pub(crate) fn check_roof_holes(roof: &Roof) -> Result<()> {
    check_openings(&roof.footprint, &roof.holes)
}

/// Openings of a floor or roof must be outlines with an area, inside the
/// `boundary` (clear of its edges) and apart from each other.
pub(crate) fn check_openings(boundary: &[DVec2], openings: &[Vec<DVec2>]) -> Result<()> {
    let bad = |m: &str| Err(EngineError::invalid(m.to_string()));
    for (i, h) in openings.iter().enumerate() {
        if h.len() < 3 || polygon::area(h) < 1e-4 {
            return bad("an opening needs at least 3 points and an area");
        }
        let crosses = |a: &[DVec2], b: &[DVec2]| {
            (0..a.len())
                .any(|i| (0..b.len()).any(|j| segments_cross(a[i], a[(i + 1) % a.len()], b[j], b[(j + 1) % b.len()])))
        };
        if !h.iter().all(|p| polygon::contains(boundary, *p)) || crosses(h, boundary) {
            return bad("an opening must lie inside the outline");
        }
        for other in openings.iter().skip(i + 1) {
            if crosses(h, other)
                || other.iter().any(|p| polygon::contains(h, *p))
                || h.iter().any(|p| polygon::contains(other, *p))
            {
                return bad("openings must not overlap");
            }
        }
    }
    Ok(())
}

/// Whether segments `a`-`b` and `c`-`d` cross or touch.
fn segments_cross(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let side = |p: DVec2, q: DVec2, r: DVec2| (q - p).perp_dot(r - p);
    let (d1, d2, d3, d4) = (side(c, d, a), side(c, d, b), side(a, b, c), side(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return true;
    }
    let on = |p: DVec2, q: DVec2, r: DVec2| polygon::closest_on_segment(p, q, r).0.distance(r) < 1e-6;
    on(c, d, a) || on(c, d, b) || on(a, b, c) || on(a, b, d)
}

impl Command for RoofCreate {
    const NAME: &'static str = "roof_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (level, profile, drawn) = sketched(s, &self.sketch, Category::Roof)?;
        if drawn.is_some() && !self.holes.is_empty() {
            return Err(EngineError::invalid("with curves, draw the openings as loops inside the outline"));
        }
        let base_offset = match self.sketch.offset {
            Some(o) => finite(o, "offset")?,
            None => default_roof_base(s.project(), level),
        };
        let overhang = finite(self.overhang.unwrap_or(0.5), "overhang")?.max(0.0);
        let mut roof = Roof::new(profile.outline, base_offset, overhang, RoofShape::Flat);
        roof.holes = match drawn {
            Some(_) => profile.openings,
            None => self.holes.iter().map(|h| points(h)).collect::<Result<_>>()?,
        };
        roof.sketch = drawn;
        check_roof_holes(&roof)?;
        create(s, t!("tx-create-roof"), level, Category::Roof, self.sketch.type_ref.as_ref(), ElementKind::Roof(roof))
    }
}

/// Set the slope of one edge of a gable or hip roof's outline (edge `i` runs
/// from outline point `i` to the next): `0` makes it a gable end, a positive
/// slope an eave rising at that slope, `null` returns it to the shape's own.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoofEdgeSet {
    /// Roof id.
    pub id: u64,
    /// Edge index, from 0.
    pub edge: usize,
    /// Slope as rise over run (0.5 = 50 %), `0` for a gable end, or `null` for the roof's slope.
    #[serde(default)]
    pub slope: Option<f64>,
}

impl Command for RoofEdgeSet {
    const NAME: &'static str = "roof_edge_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::element(s.project(), self.id)?;
        let slope = self.slope.map(|v| finite(v, "slope")).transpose()?;
        if slope.is_some_and(|v| v < 0.0) {
            return Err(EngineError::invalid("the slope cannot be negative"));
        }
        edit(s, t!("tx-modify"), |e| {
            let Some(ElementKind::Roof(r)) = e.element_mut(id).map(|x| &mut x.kind) else {
                return Err(EngineError::invalid("not a roof"));
            };
            if !matches!(r.shape, RoofShape::Gable { .. } | RoofShape::Hip { .. }) {
                return Err(EngineError::invalid("only gable and hip roofs have edge slopes"));
            }
            if self.edge >= r.footprint.len() {
                return Err(EngineError::invalid(format!("the roof has {} edges", r.footprint.len())));
            }
            if r.edge_slopes.len() <= self.edge {
                r.edge_slopes.resize(self.edge + 1, None);
            }
            r.edge_slopes[self.edge] = slope;
            while r.edge_slopes.last().is_some_and(Option::is_none) {
                r.edge_slopes.pop();
            }
            Ok(())
        })?;
        Ok(created(s, id))
    }
}

/// Add an opening (a shaft, a stairwell) to a floor: a closed outline inside
/// its boundary, apart from its other openings.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FloorOpeningAdd {
    /// Floor id.
    pub id: u64,
    /// Outline of the opening, at least 3 points.
    pub outline: Vec<Point>,
}

impl Command for FloorOpeningAdd {
    const NAME: &'static str = "floor_opening_add";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::element(s.project(), self.id)?;
        let outline = points(&self.outline)?;
        edit(s, t!("tx-modify"), |e| {
            let Some(ElementKind::Floor(f)) = e.element_mut(id).map(|x| &mut x.kind) else {
                return Err(EngineError::invalid("not a floor"));
            };
            f.openings.push(outline);
            check_openings(&f.boundary, &f.openings)
        })?;
        Ok(created(s, id))
    }
}

/// Remove one opening of a floor (by its index in the floor's `openings`).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FloorOpeningRemove {
    /// Floor id.
    pub id: u64,
    /// Opening index, from 0.
    pub index: usize,
}

impl Command for FloorOpeningRemove {
    const NAME: &'static str = "floor_opening_remove";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::element(s.project(), self.id)?;
        edit(s, t!("tx-modify"), |e| {
            let Some(ElementKind::Floor(f)) = e.element_mut(id).map(|x| &mut x.kind) else {
                return Err(EngineError::invalid("not a floor"));
            };
            if self.index >= f.openings.len() {
                return Err(EngineError::invalid(format!("the floor has {} openings", f.openings.len())));
            }
            f.openings.remove(self.index);
            Ok(())
        })?;
        Ok(created(s, id))
    }
}

/// Remove one opening of a roof (by its index in the roof's `holes`).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoofOpeningRemove {
    /// Roof id.
    pub id: u64,
    /// Opening index, from 0.
    pub index: usize,
}

impl Command for RoofOpeningRemove {
    const NAME: &'static str = "roof_opening_remove";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::element(s.project(), self.id)?;
        edit(s, t!("tx-modify"), |e| {
            let Some(ElementKind::Roof(r)) = e.element_mut(id).map(|x| &mut x.kind) else {
                return Err(EngineError::invalid("not a roof"));
            };
            if self.index >= r.holes.len() {
                return Err(EngineError::invalid(format!("the roof has {} openings", r.holes.len())));
            }
            r.holes.remove(self.index);
            Ok(())
        })?;
        Ok(created(s, id))
    }
}

/// Create a room at a point enclosed by room-bounding walls (its boundary follows the walls).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct RoomCreate {
    /// A point inside the room.
    pub point: Point,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Default: the room default name.
    #[serde(default)]
    pub name: Option<String>,
    /// Default: the next free number.
    #[serde(default)]
    pub number: Option<String>,
    /// Upper limit above the level (default: the level's height).
    #[serde(default)]
    pub height: Option<f64>,
}

impl Command for RoomCreate {
    const NAME: &'static str = "room_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let position = point(self.point)?;
        let level = refs::level_or_first(s.project(), self.level.as_ref())?;
        let project = s.project().clone();
        let phase = s.doc.creation_phase;
        if s.geometry().room_at(&project, level, position, phase).is_none() {
            return Err(EngineError::rejected("the point is not enclosed by room-bounding walls"));
        }
        let p = s.project();
        let number = self.number.unwrap_or_else(|| {
            next_number(p.elements.values().filter_map(|e| match &e.kind {
                ElementKind::Room(r) => Some(r.number.as_str()),
                _ => None,
            }))
        });
        let height = positive(self.height.unwrap_or_else(|| p.level_height(level, 3.0)), "height")?;
        let room = Room { position, number, name: self.name.unwrap_or_else(|| t!("room-default-name")), height };
        create(s, t!("tx-create-room"), level, Category::Room, None, ElementKind::Room(room))
    }
}

/// The grids of `p` with their ids.
pub fn grids(p: &Project) -> impl Iterator<Item = (ElementId, &Grid)> {
    p.elements.values().filter_map(|e| match &e.kind {
        ElementKind::Grid(g) => Some((e.id, g)),
        _ => None,
    })
}

/// Create a grid line. Without `name` it takes the next name of its series:
/// the one after the most recently placed grid parallel to it (`1` → `2`,
/// `A` → `B`, `Z` → `AA`, `A1` → `A2`, `1.1` → `1.2`); a grid across the
/// existing ones starts a new series (letters across numbered grids,
/// numbers otherwise). Names given this way are unique among the grids.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct GridCreate {
    /// Start point (where the bubble is).
    pub start: Point,
    /// End point.
    pub end: Point,
    /// Grid name (default: the next name of its series).
    #[serde(default)]
    pub name: Option<String>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
}

impl Command for GridCreate {
    const NAME: &'static str = "grid_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (start, end) = (point(self.start)?, point(self.end)?);
        if start.distance(end) < 1e-3 {
            return Err(EngineError::invalid("the grid has no length"));
        }
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let name = self.name.unwrap_or_else(|| cc_core::grid::next_name(grids(p), start, end));
        create(s, t!("tx-create-grid"), level, Category::Grid, None, ElementKind::Grid(Grid { start, end, name }))
    }
}

/// Place a column; its top goes to the level above (or `height` when there is none).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ColumnCreate {
    /// Center of the column in plan.
    pub position: Point,
    /// Rotation in plan, degrees.
    #[serde(default)]
    pub rotation: Option<f64>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Unconnected height (default: the level's height).
    #[serde(default)]
    pub height: Option<f64>,
    /// Level the top is attached to (default: the level above).
    #[serde(default)]
    pub top_level: Option<Ref>,
    /// Offset of the top from `top_level`, meters.
    #[serde(default)]
    pub top_offset: Option<f64>,
    /// Load-bearing (default true); an architectural column is cut by the walls it is joined with.
    #[serde(default)]
    pub structural: Option<bool>,
}

impl Command for ColumnCreate {
    const NAME: &'static str = "column_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let column = Column {
            position: point(self.position)?,
            rotation_deg: finite(self.rotation.unwrap_or(0.0), "rotation")?,
            base_offset: 0.0,
            height: positive(self.height.unwrap_or_else(|| p.level_height(level, 3.0)), "height")?,
            top: top(p, self.top_level.as_ref(), self.top_offset)?.unwrap_or_else(|| default_top(p, level)),
            structural: self.structural.unwrap_or(true),
        };
        create(s, t!("tx-create-column"), level, Category::Column, self.type_ref.as_ref(), ElementKind::Column(column))
    }
}

/// Create a beam from `start` to `end`; its location line is `top_offset` above the level
/// (default: the level's height), at the top of the section unless `vertical` says otherwise.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct BeamCreate {
    /// Start of the beam axis.
    pub start: Point,
    /// End of the beam axis.
    pub end: Point,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Elevation of the location line above the level, meters (default: the level height).
    #[serde(default)]
    pub top_offset: Option<f64>,
    /// Elevation of the location line at the end, when it differs from the start (a sloped beam).
    #[serde(default)]
    pub end_offset: Option<f64>,
    /// Line across the section the axis follows: `center` (default), `left` or `right` face.
    #[serde(default)]
    pub lateral: Option<String>,
    /// Height of the section at the location elevation: `top` (default), `center` or `bottom`.
    #[serde(default)]
    pub vertical: Option<String>,
}

/// A beam justification by name.
pub(crate) fn justification<T: Copy>(value: Option<&str>, all: &[(&str, T)], what: &str) -> Result<Option<T>> {
    let Some(v) = value else { return Ok(None) };
    all.iter().find(|(name, _)| *name == v).map(|(_, j)| Some(*j)).ok_or_else(|| {
        let names: Vec<&str> = all.iter().map(|(n, _)| *n).collect();
        EngineError::invalid(format!("{what} must be one of {}, not '{v}'", names.join(", ")))
    })
}

impl Command for BeamCreate {
    const NAME: &'static str = "beam_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (start, end) = (point(self.start)?, point(self.end)?);
        if start.distance(end) < 1e-3 {
            return Err(EngineError::invalid("the beam has no length"));
        }
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let top_offset = finite(self.top_offset.unwrap_or_else(|| p.level_height(level, 3.0)), "top_offset")?;
        let mut beam = Beam::new(start, end, top_offset);
        beam.end_offset = self.end_offset.map(|z| finite(z, "end_offset")).transpose()?;
        let lateral = [("center", BeamLateral::Center), ("left", BeamLateral::Left), ("right", BeamLateral::Right)];
        let vertical = [("top", BeamVertical::Top), ("center", BeamVertical::Center), ("bottom", BeamVertical::Bottom)];
        if let Some(j) = justification(self.lateral.as_deref(), &lateral, "lateral")? {
            beam.lateral = j;
        }
        if let Some(j) = justification(self.vertical.as_deref(), &vertical, "vertical")? {
            beam.vertical = j;
        }
        create(s, t!("tx-create-beam"), level, Category::Beam, self.type_ref.as_ref(), ElementKind::Beam(beam))
    }
}

/// Create a railing along a path (at least 2 points).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct RailingCreate {
    /// Points of the railing path (at least 2).
    pub path: Vec<Point>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Base above the level, meters.
    #[serde(default)]
    pub base_offset: Option<f64>,
}

impl Command for RailingCreate {
    const NAME: &'static str = "railing_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let mut path = points(&self.path)?;
        path.dedup_by(|a, b| a.distance(*b) < 1e-6);
        if path.len() < 2 {
            return Err(EngineError::invalid("the path needs at least 2 distinct points"));
        }
        let level = refs::level_or_first(s.project(), self.level.as_ref())?;
        let base_offset = finite(self.base_offset.unwrap_or(0.0), "base_offset")?;
        let railing = Railing { path, base_offset, stair: None };
        let kind = ElementKind::Railing(railing);
        create(s, t!("tx-create-railing"), level, Category::Railing, self.type_ref.as_ref(), kind)
    }
}

/// Place a component (furniture, plumbing fixture, electrical device…) of a
/// component type. Electrical devices default to their usual mounting height
/// (outlets 0.30 m, switches 1.10 m, boards 1.30 m, wall lights 2.10 m,
/// ceiling lights at the level height) and may be put on a circuit.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentPlace {
    /// Insertion point in plan.
    pub position: Point,
    /// Component type id or name.
    #[serde(rename = "type")]
    pub type_ref: Ref,
    /// Rotation in plan, degrees.
    #[serde(default)]
    pub rotation: Option<f64>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Base above the level, meters (default 0; for devices, their mounting height).
    #[serde(default)]
    pub base_offset: Option<f64>,
    /// Circuit or system id, name or code to connect it to.
    #[serde(default)]
    pub system: Option<Ref>,
}

impl Command for ComponentPlace {
    const NAME: &'static str = "component_place";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let type_id = refs::type_of(p, Category::Component, Some(&self.type_ref))?;
        let device = match p.element_type(type_id).map(|t| &t.data) {
            Some(cc_core::TypeData::Component { def, .. }) => def.mep.and_then(|m| m.device()),
            _ => None,
        };
        let height = device.map_or(0.0, |d| crate::defaults::device_height(p, level, d));
        let system = self.system.as_ref().map(|r| refs::system(p, r)).transpose()?;
        if let Some(id) = system {
            let mep = match p.element_type(type_id).map(|t| &t.data) {
                Some(cc_core::TypeData::Component { def, .. }) => def.mep,
                _ => None,
            };
            super::mep::check_fits(p, super::mep::is_electrical(mep), id)?;
        }
        let component = Component {
            position: point(self.position)?,
            rotation_deg: finite(self.rotation.unwrap_or(0.0), "rotation")?,
            base_offset: finite(self.base_offset.unwrap_or(height), "base_offset")?,
            overrides: Default::default(),
            properties: Vec::new(),
            system,
        };
        let kind = ElementKind::Component(component);
        create(s, t!("tx-create-component"), level, Category::Component, Some(&self.type_ref), kind)
    }
}
