//! Building pads: flat areas graded into a terrain, with cut and fill slopes
//! out to the existing ground (see `cc_core::pad`).

use super::sketch::CurveArg;
use super::{add_element, created, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::pad::{DEFAULT_CUT_SLOPE, DEFAULT_FILL_SLOPE, MAX_OUTLINE, MAX_SLOPE, MIN_SLOPE, Pad, slope_ok};
use cc_core::{Category, ElementId, ElementKind, ElementType, ElementTypeId, Project, TypeData};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A pad type for a project that has none: slopes of 1:1 in cut and 1:1.5 in fill.
pub fn new_pad_type(id: ElementTypeId) -> ElementType {
    ElementType {
        id,
        name: t!("tpl-type-pad"),
        data: TypeData::Pad { cut_slope: DEFAULT_CUT_SLOPE, fill_slope: DEFAULT_FILL_SLOPE },
        info: Default::default(),
        uid: Default::default(),
    }
}

/// A slope ratio within the range pads keep.
fn ratio(r: f64, name: &str) -> Result<f64> {
    if slope_ok(r) {
        Ok(r)
    } else {
        Err(EngineError::invalid(format!("{name} must be between {MIN_SLOPE} and {MAX_SLOPE} (run per unit of rise)")))
    }
}

/// Whether polygons `a` and `b` overlap (share some area or touch).
fn overlap(a: &[DVec2], b: &[DVec2]) -> bool {
    let inside = |poly: &[DVec2], pts: &[DVec2]| pts.iter().any(|p| cc_geom::polygon::contains(poly, *p));
    let edges = |p: &[DVec2]| {
        let n = p.len();
        (0..n).map(move |i| (p[i], p[(i + 1) % n])).collect::<Vec<_>>()
    };
    let cross = |(p, q): (DVec2, DVec2), (r, s): (DVec2, DVec2)| {
        let side = |a: DVec2, b: DVec2, c: DVec2| (b - a).perp_dot(c - a);
        let (d1, d2, d3, d4) = (side(r, s, p), side(r, s, q), side(p, q, r), side(p, q, s));
        d1 * d2 <= 0.0 && d3 * d4 <= 0.0
    };
    inside(a, b) || inside(b, a) || edges(a).iter().any(|ea| edges(b).iter().any(|eb| cross(*ea, *eb)))
}

/// The terrain a pad with `outline` grades by default: the first (in id
/// order) on `level` (any level when `None`) whose boundary holds the
/// outline's center, else the first it overlaps.
pub fn default_host(p: &Project, level: Option<cc_core::LevelId>, outline: &[DVec2]) -> Option<ElementId> {
    let center = outline.iter().copied().sum::<DVec2>() / outline.len().max(1) as f64;
    let terrains: Vec<(ElementId, &[DVec2])> = p
        .elements
        .values()
        .filter(|e| level.is_none_or(|l| e.level == l))
        .filter_map(|e| match &e.kind {
            ElementKind::Topography(t) => Some((e.id, t.boundary.as_slice())),
            _ => None,
        })
        .collect();
    terrains
        .iter()
        .find(|(_, b)| cc_geom::polygon::contains(b, center))
        .or_else(|| terrains.iter().find(|(_, b)| overlap(b, outline)))
        .map(|(id, _)| *id)
}

/// Create a building pad: a flat area at `elevation` (meters above the
/// terrain's level) inside a closed `outline` (or one drawn with `curves`),
/// graded into a terrain (`host`) with cut and fill slopes out to the
/// existing ground. Slopes are the horizontal run per unit of rise
/// (`cut_slope` 1 is 1:1); without them the pad uses its type's. Pads of
/// one terrain apply in creation order. A project without a pad type gets one.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct PadCreate {
    /// Terrain element id (default: the terrain on `level` whose boundary holds the outline's center).
    #[serde(default)]
    pub host: Option<u64>,
    /// Outline `[[x, y], …]` in plan, closed (at least three points); or `curves`.
    #[serde(default)]
    pub outline: Vec<Point>,
    /// The outline drawn with curves (lines, arcs, circles, rectangles…).
    #[serde(default)]
    pub curves: Option<Vec<CurveArg>>,
    /// Level id or name where the default terrain is looked for (default: any level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Elevation of the pad above the terrain's level, meters.
    pub elevation: f64,
    /// Run per unit of rise of the cut slope (default: the type's).
    #[serde(default)]
    pub cut_slope: Option<f64>,
    /// Run per unit of rise of the fill slope (default: the type's).
    #[serde(default)]
    pub fill_slope: Option<f64>,
    /// Type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
}

impl Command for PadCreate {
    const NAME: &'static str = "pad_create";
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
        if !self.elevation.is_finite() {
            return Err(EngineError::invalid("elevation must be a finite number"));
        }
        let cut_slope = self.cut_slope.map(|r| ratio(r, "cut_slope")).transpose()?;
        let fill_slope = self.fill_slope.map(|r| ratio(r, "fill_slope")).transpose()?;
        let p = s.project();
        let level = self.level.as_ref().map(|l| refs::level(p, l)).transpose()?;
        let host = match self.host {
            Some(h) => ElementId(h),
            None => default_host(p, level, &outline).ok_or_else(|| {
                EngineError::invalid("no terrain under the outline (give host, or draw the pad over a terrain)")
            })?,
        };
        let Some((host_level, boundary)) = p.element(host).and_then(|e| match &e.kind {
            ElementKind::Topography(t) => Some((e.level, t.boundary.clone())),
            _ => None,
        }) else {
            return Err(EngineError::invalid(format!("element {} is not a terrain", host.0)));
        };
        if !overlap(&boundary, &outline) {
            return Err(EngineError::invalid("the outline lies outside the terrain's boundary"));
        }
        let pad = Pad { host, outline, elevation: self.elevation, cut_slope, fill_slope, sketch: drawn };
        if !pad.is_valid() {
            return Err(EngineError::invalid("the outline encloses no area"));
        }
        let ty = match (&self.type_ref, p.default_type(Category::Pad)) {
            (None, None) => None,
            (r, _) => Some(refs::type_of(p, Category::Pad, r.as_ref())?),
        };
        let id = edit(s, t!("tx-create-pad"), |e| {
            let type_id = match ty {
                Some(t) => t,
                None => {
                    let id = ElementTypeId(e.alloc_id());
                    e.add_type(new_pad_type(id))
                }
            };
            Ok(add_element(e, host_level, type_id, ElementKind::Pad(pad)))
        })?;
        Ok(created(s, id))
    }
}

/// Create a building pad type: the run per unit of rise of its cut and fill slopes.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct PadTypeCreate {
    /// Type name (default: the template's pad name).
    #[serde(default)]
    pub name: Option<String>,
    /// Run per unit of rise of the cut slope (default 1).
    #[serde(default)]
    pub cut_slope: Option<f64>,
    /// Run per unit of rise of the fill slope (default 1.5).
    #[serde(default)]
    pub fill_slope: Option<f64>,
}

impl Command for PadTypeCreate {
    const NAME: &'static str = "pad_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let cut = ratio(self.cut_slope.unwrap_or(DEFAULT_CUT_SLOPE), "cut_slope")?;
        let fill = ratio(self.fill_slope.unwrap_or(DEFAULT_FILL_SLOPE), "fill_slope")?;
        let name = self.name.filter(|n| !n.trim().is_empty());
        let id = edit(s, t!("tx-create-pad-type"), |e| {
            let id = ElementTypeId(e.alloc_id());
            let mut ty = new_pad_type(id);
            ty.data = TypeData::Pad { cut_slope: cut, fill_slope: fill };
            if let Some(name) = name {
                ty.name = name;
            }
            Ok(e.add_type(ty))
        })?;
        Ok(json!({ "id": id.0 }))
    }
}

/// The figures `element_get` shows for a pad, as `pad`.
pub(crate) fn derived(g: &cc_geom::pad::PadGeom, derived: &mut serde_json::Map<String, Value>) {
    let warnings: Vec<&str> = g.warnings.iter().map(|w| w.code()).collect();
    derived.insert(
        "pad".into(),
        json!({
            "z": g.z,
            "area": g.area,
            "cut": g.cut,
            "fill": g.fill,
            "net": g.fill - g.cut,
            "warnings": warnings,
        }),
    );
}
