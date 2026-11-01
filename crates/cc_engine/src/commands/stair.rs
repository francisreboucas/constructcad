//! Stairs: creating them in any shape, checking their steps and adding the
//! railings that follow them.

use super::create::{create, justification, top};
use super::{add_element, edit};
use crate::defaults::default_top;
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, finite, point, positive};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{
    Category, ElementId, ElementKind, Railing, Stair, StairLayout, StairRailing, StairShape, StairSide, Turn, WallTop,
};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Names of the stair shapes in commands.
pub const SHAPES: [(&str, StairShape); 6] = [
    ("straight", StairShape::Straight),
    ("straight_landing", StairShape::StraightLanding),
    ("l_shaped", StairShape::LShaped),
    ("u_shaped", StairShape::UShaped),
    ("three_flight", StairShape::ThreeFlight),
    ("spiral", StairShape::Spiral),
];

const TURNS: [(&str, Turn); 2] = [("left", Turn::Left), ("right", Turn::Right)];

/// Create a stair starting at `start` (the middle of its first riser) and
/// rising along `direction` up to the level above (or `height`). Risers and
/// treads follow the type (the fewest risers within its maximum, and a fixed
/// tread or one from Blondel's formula `2 × riser + tread`) unless `risers`
/// or `tread` set them: the riser height is then the height over the risers.
/// `shape` lays it out straight, with a landing, L- or U-shaped, in three
/// flights round two landings, or around a center. The top floor is the last
/// step: a stair ends with a riser, not a tread.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct StairCreate {
    /// Start of the run (center of the first step's edge).
    pub start: Point,
    /// Direction of the first flight in plan, degrees counter-clockwise from +X.
    #[serde(default)]
    pub direction: Option<f64>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Height when there is no level above (default 3).
    #[serde(default)]
    pub height: Option<f64>,
    /// Level the stair rises to (default: the level above).
    #[serde(default)]
    pub top_level: Option<Ref>,
    /// Plan shape: `straight` (default), `straight_landing`, `l_shaped`,
    /// `u_shaped`, `three_flight` (up, across and back, with a landing at
    /// each corner) or `spiral`.
    #[serde(default)]
    pub shape: Option<String>,
    /// Side the stair turns to, walking up: `left` (default) or `right`.
    #[serde(default)]
    pub turn: Option<String>,
    /// Riser count (default: the fewest within the type's maximum riser).
    #[serde(default)]
    pub risers: Option<u32>,
    /// Tread depth, meters (default: the type's).
    #[serde(default)]
    pub tread: Option<f64>,
    /// Risers of the first flight (default: half of them; a third on a three-flight stair).
    #[serde(default)]
    pub split: Option<u32>,
    /// Risers of the middle flight of a three-flight stair (default: half of those left).
    #[serde(default)]
    pub split2: Option<u32>,
    /// Landing depth along the first flight, meters (default: the stair width).
    #[serde(default)]
    pub landing: Option<f64>,
    /// Clear gap between the flights of a U-shaped stair, meters (default 0.1).
    #[serde(default)]
    pub gap: Option<f64>,
    /// Winder treads in the corner of an L-shaped stair instead of a landing (2 to 6).
    #[serde(default)]
    pub winders: Option<u32>,
    /// Radius of the inner edge of a spiral stair, meters (default 0.15).
    #[serde(default)]
    pub inner_radius: Option<f64>,
}

impl StairCreate {
    /// The plan layout these parameters ask for.
    pub fn layout(&self) -> Result<StairLayout> {
        let mut layout = StairLayout::default();
        if let Some(shape) = justification(self.shape.as_deref(), &SHAPES, "shape")? {
            layout.shape = shape;
        }
        if let Some(turn) = justification(self.turn.as_deref(), &TURNS, "turn")? {
            layout.turn = turn;
        }
        let max = cc_core::stair::MAX_RISERS;
        layout.split = self.split.unwrap_or(0).min(max);
        layout.split2 = self.split2.unwrap_or(0).min(max);
        if let Some(n) = self.risers {
            if n == 0 || n > max {
                return Err(EngineError::invalid(format!("risers must be from 1 to {max}")));
            }
            layout.risers = n;
        }
        if let Some(v) = self.tread {
            let v = positive(v, "tread")?;
            if v > 2.0 {
                return Err(EngineError::invalid("tread must be at most 2 m"));
            }
            layout.tread = v;
        }
        if let Some(v) = self.landing {
            layout.landing = positive(v, "landing")?;
        }
        if let Some(v) = self.gap {
            layout.gap = finite(v, "gap")?.max(0.0);
        }
        if let Some(n) = self.winders {
            if n == 1 || n > cc_core::stair::MAX_WINDERS {
                return Err(EngineError::invalid("winders must be 0 or from 2 to 6"));
            }
            layout.winders = n;
        }
        if let Some(v) = self.inner_radius {
            layout.inner_radius = finite(v, "inner_radius")?.max(0.0);
        }
        Ok(layout)
    }
}

impl Command for StairCreate {
    const NAME: &'static str = "stair_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let top = top(p, self.top_level.as_ref(), None)?.unwrap_or_else(|| default_top(p, level));
        let height = match top {
            WallTop::Level { level: t, .. } => p.level_elevation(t) - p.level_elevation(level),
            WallTop::Unconnected => self.height.unwrap_or(3.0),
        };
        let stair = Stair {
            start: point(self.start)?,
            direction_deg: finite(self.direction.unwrap_or(0.0), "direction")?,
            base_offset: 0.0,
            height: positive(height, "height")?,
            top,
            layout: self.layout()?,
        };
        create(s, t!("tx-create-stair"), level, Category::Stair, self.type_ref.as_ref(), ElementKind::Stair(stair))
    }
}

/// Check a stair's steps: riser count and height, tread on the walking line,
/// Blondel's step `2 × riser + tread` and the rules of its type they break
/// (riser out of range, short tread, step out of the Blondel range, short landing).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StairCheck {
    /// Stair element id.
    pub id: u64,
}

impl Command for StairCheck {
    const NAME: &'static str = "stair_check";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let (el, st) = stair(p, self.id)?;
        let g = cc_geom::stair_geom(p, el, st).ok_or_else(|| EngineError::invalid("the stair has no stair type"))?;
        let issues: Vec<Value> =
            g.check.issues.iter().map(|i| json!({"message": issue_text(i), "detail": i})).collect();
        Ok(json!({
            "risers": g.risers,
            "riser": g.riser,
            "tread": g.tread,
            "blondel": g.check.blondel,
            "run": g.run(),
            "ok": g.check.ok(),
            "issues": issues,
        }))
    }
}

/// A broken stair rule in words.
pub fn issue_text(issue: &cc_core::StairIssue) -> String {
    use cc_core::StairIssue as I;
    let cm = |v: f64| format!("{:.1}", v * 100.0);
    match *issue {
        I::RiserTooHigh { riser, max } => t!("stair-issue-riser-high", "riser" => cm(riser), "limit" => cm(max)),
        I::RiserTooLow { riser, min } => t!("stair-issue-riser-low", "riser" => cm(riser), "limit" => cm(min)),
        I::TreadTooShort { tread, min } => t!("stair-issue-tread-short", "tread" => cm(tread), "limit" => cm(min)),
        I::BlondelOutOfRange { step, min, max } => {
            t!("stair-issue-blondel", "step" => cm(step), "min" => cm(min), "max" => cm(max))
        }
        I::LandingTooShort { landing, min } => {
            t!("stair-issue-landing-short", "landing" => cm(landing), "limit" => cm(min))
        }
    }
}

fn stair(p: &cc_core::Project, id: u64) -> Result<(&cc_core::Element, &Stair)> {
    match p.element(ElementId(id)) {
        Some(e) => match &e.kind {
            ElementKind::Stair(st) => Ok((e, st)),
            _ => Err(EngineError::invalid(format!("element {id} is not a stair"))),
        },
        None => Err(EngineError::not_found(format!("no element {id}"))),
    }
}

/// Add railings along the sides of a stair or a ramp: they follow its
/// flights, landings and turns (at the pitch of the nosings on stairs), and
/// go with it when it changes, moves or is deleted.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StairRailingsAdd {
    /// Stair or ramp element id.
    pub id: u64,
    /// `left`, `right` or `both` (default), walking up.
    #[serde(default)]
    pub side: Option<String>,
    /// Railing type id or name (default: the category's default type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Distance in from the edge of the stair, meters (default 0.05).
    #[serde(default)]
    pub inset: Option<f64>,
}

impl Command for StairRailingsAdd {
    const NAME: &'static str = "stair_railings_add";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let Some(el) = p.element(ElementId(self.id)) else {
            return Err(EngineError::not_found(format!("no element {}", self.id)));
        };
        let sides = match self.side.as_deref() {
            None | Some("both") => vec![StairSide::Left, StairSide::Right],
            Some("left") => vec![StairSide::Left],
            Some("right") => vec![StairSide::Right],
            Some(other) => {
                return Err(EngineError::invalid(format!("side must be left, right or both, not '{other}'")));
            }
        };
        let inset = finite(self.inset.unwrap_or(0.05), "inset")?;
        if inset.abs() >= 1.0 {
            return Err(EngineError::invalid("inset must be under 1 m"));
        }
        let type_id = refs::type_of(p, Category::Railing, self.type_ref.as_ref())?;
        let no_type = || EngineError::invalid("the element has no type of its category");
        let path = |side: StairSide| -> Result<Vec<glam::DVec3>> {
            match &el.kind {
                ElementKind::Stair(st) => {
                    Ok(cc_geom::stair_geom(p, el, st).ok_or_else(no_type)?.railing_path(side, inset))
                }
                ElementKind::Ramp(r) => Ok(cc_geom::ramp_geom(p, el, r).ok_or_else(no_type)?.railing_path(side, inset)),
                _ => Err(EngineError::invalid(format!("element {} is not a stair or a ramp", el.id.0))),
            }
        };
        let (host, level) = (el.id, el.level);
        let mut railings: Vec<(StairSide, Vec<DVec2>)> = Vec::new();
        for side in sides {
            let plan: Vec<DVec2> = path(side)?.iter().map(|q| q.truncate()).collect();
            if plan.len() >= 2 {
                railings.push((side, plan));
            }
        }
        if railings.is_empty() {
            return Err(EngineError::invalid("the element has no side for a railing"));
        }
        let ids = edit(s, t!("tx-create-railing"), |e| {
            Ok(railings
                .into_iter()
                .map(|(side, path)| {
                    let stair = Some(StairRailing { host, side, inset });
                    add_element(e, level, type_id, ElementKind::Railing(Railing { path, base_offset: 0.0, stair }))
                })
                .collect::<Vec<_>>())
        })?;
        Ok(json!({ "ids": ids.iter().map(|id| id.0).collect::<Vec<_>>() }))
    }
}
