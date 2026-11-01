//! Stairs: the plan shapes of a stair element, how its type is built (solid,
//! cast in place, assembled) and the rules that size its steps (riser limits
//! and Blondel's formula `2 × riser + tread`).

use serde::{Deserialize, Serialize};

/// Plan shape of a stair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum StairShape {
    /// One straight flight.
    #[default]
    Straight,
    /// Two straight flights in line with a landing between them.
    StraightLanding,
    /// Two flights at a right angle, with a landing or winders at the corner.
    LShaped,
    /// Two parallel flights going opposite ways, joined by a landing.
    UShaped,
    /// Three flights turning twice at right angles, with a landing at each
    /// corner: the third comes back beside the first.
    ThreeFlight,
    /// Treads around a center (spiral or helical stair).
    Spiral,
}

impl StairShape {
    pub const ALL: [StairShape; 6] = [
        StairShape::Straight,
        StairShape::StraightLanding,
        StairShape::LShaped,
        StairShape::UShaped,
        StairShape::ThreeFlight,
        StairShape::Spiral,
    ];

    pub fn key(self) -> &'static str {
        match self {
            StairShape::Straight => "stair-shape-straight",
            StairShape::StraightLanding => "stair-shape-straight-landing",
            StairShape::LShaped => "stair-shape-l",
            StairShape::UShaped => "stair-shape-u",
            StairShape::ThreeFlight => "stair-shape-three-flight",
            StairShape::Spiral => "stair-shape-spiral",
        }
    }

    /// Whether the stair turns (so the side it turns to matters).
    pub fn turns(self) -> bool {
        matches!(self, StairShape::LShaped | StairShape::UShaped | StairShape::ThreeFlight | StairShape::Spiral)
    }

    /// Whether the stair has flights split by landings (or winders).
    pub fn has_split(self) -> bool {
        matches!(
            self,
            StairShape::StraightLanding | StairShape::LShaped | StairShape::UShaped | StairShape::ThreeFlight
        )
    }
}

/// Side a stair turns to, walking up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Turn {
    /// Counter-clockwise in plan.
    #[default]
    Left,
    /// Clockwise in plan.
    Right,
}

impl Turn {
    pub const ALL: [Turn; 2] = [Turn::Left, Turn::Right];

    pub fn key(self) -> &'static str {
        match self {
            Turn::Left => "stair-turn-left",
            Turn::Right => "stair-turn-right",
        }
    }

    /// +1 to the left, -1 to the right.
    pub fn sign(self) -> f64 {
        match self {
            Turn::Left => 1.0,
            Turn::Right => -1.0,
        }
    }

    pub fn flipped(self) -> Turn {
        match self {
            Turn::Left => Turn::Right,
            Turn::Right => Turn::Left,
        }
    }
}

/// Plan layout of a stair element (all fields have defaults, so files from
/// before shaped stairs read as straight ones).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StairLayout {
    pub shape: StairShape,
    pub turn: Turn,
    /// Risers of the first flight (0: half of them; a third on a three-flight stair).
    pub split: u32,
    /// Risers of the second flight of a three-flight stair (0: half of those
    /// left after the first).
    pub split2: u32,
    /// Riser count (0: the fewest within the type's maximum riser); the
    /// riser height is the stair's height over it.
    pub risers: u32,
    /// Tread depth (0: the type's, or Blondel's formula when its rules say so).
    pub tread: f64,
    /// Landing depth along the first flight (0: the stair width); straight
    /// and U-shaped stairs.
    pub landing: f64,
    /// Clear gap between the flights of a U-shaped stair.
    pub gap: f64,
    /// Winder treads in the corner of an L-shaped stair (0: a landing).
    pub winders: u32,
    /// Radius of the inner edge of a spiral stair (0: treads meet at the center).
    pub inner_radius: f64,
}

impl Default for StairLayout {
    fn default() -> Self {
        Self {
            shape: StairShape::Straight,
            turn: Turn::Left,
            split: 0,
            split2: 0,
            risers: 0,
            tread: 0.0,
            landing: 0.0,
            gap: 0.1,
            winders: 0,
            inner_radius: 0.15,
        }
    }
}

/// Most winder treads in one corner.
pub const MAX_WINDERS: u32 = 6;

impl StairLayout {
    /// Replace invalid values by defaults; returns how many changed.
    pub fn repair(&mut self) -> usize {
        let d = StairLayout::default();
        let mut n = 0;
        for (v, default) in
            [(&mut self.landing, d.landing), (&mut self.gap, d.gap), (&mut self.inner_radius, d.inner_radius)]
        {
            if !v.is_finite() || *v < 0.0 {
                *v = default;
                n += 1;
            }
        }
        if self.winders > MAX_WINDERS {
            self.winders = MAX_WINDERS;
            n += 1;
        }
        for v in [&mut self.split, &mut self.split2, &mut self.risers] {
            if *v > MAX_RISERS {
                *v = 0;
                n += 1;
            }
        }
        if !self.tread.is_finite() || !(0.0..=2.0).contains(&self.tread) {
            self.tread = 0.0;
            n += 1;
        }
        n
    }

    /// Riser count, riser height and tread depth of a stair of `height` of a
    /// type with `max_riser`, `tread` and `rules`, with this layout's riser
    /// count and tread when it sets them.
    pub fn steps(&self, height: f64, max_riser: f64, tread: f64, rules: &StairRules) -> (usize, f64, f64) {
        let (risers, riser, g) = match self.risers {
            0 => stair_steps(height, max_riser, tread, rules),
            n => {
                let n = n.min(MAX_RISERS) as usize;
                let riser = if height.is_finite() { height.max(0.0) / n as f64 } else { 0.0 };
                (n, riser, rules.tread_for(riser, tread.max(0.0)))
            }
        };
        let g = if self.tread > 1e-3 && self.tread.is_finite() { self.tread } else { g };
        (risers, riser, g)
    }
}

/// Most risers in one stair.
pub const MAX_RISERS: u32 = 200;

/// How a stair is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum StairConstruction {
    /// Steps filled down to the base (a solid block, e.g. on fill).
    #[default]
    Solid,
    /// Reinforced concrete cast in place: steps on an inclined waist slab
    /// `waist` thick (landings are slabs of the same thickness).
    CastInPlace,
    /// Assembled (precast, steel or timber): treads `tread_thickness` thick
    /// on stringers, with or without riser boards.
    Assembled,
}

impl StairConstruction {
    pub const ALL: [StairConstruction; 3] =
        [StairConstruction::Solid, StairConstruction::CastInPlace, StairConstruction::Assembled];

    pub fn key(self) -> &'static str {
        match self {
            StairConstruction::Solid => "stair-build-solid",
            StairConstruction::CastInPlace => "stair-build-cast",
            StairConstruction::Assembled => "stair-build-assembled",
        }
    }
}

/// Where the stringers of an assembled stair go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Stringers {
    /// One under each side of the treads.
    #[default]
    Sides,
    /// One under the middle of the treads (a central column on spirals).
    Center,
}

impl Stringers {
    pub const ALL: [Stringers; 2] = [Stringers::Sides, Stringers::Center];

    pub fn key(self) -> &'static str {
        match self {
            Stringers::Sides => "stair-stringers-sides",
            Stringers::Center => "stair-stringers-center",
        }
    }
}

/// Construction of a stair type.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StairBuild {
    pub construction: StairConstruction,
    /// Thickness of the waist slab of a cast-in-place stair, square to its pitch.
    pub waist: f64,
    pub stringers: Stringers,
    /// Width and depth of the stringers of an assembled stair.
    pub stringer_width: f64,
    pub stringer_depth: f64,
    /// Thickness of the treads of an assembled stair.
    pub tread_thickness: f64,
    /// Riser boards closing the gap between treads (assembled stairs).
    pub closed_risers: bool,
}

impl Default for StairBuild {
    fn default() -> Self {
        Self {
            construction: StairConstruction::Solid,
            waist: 0.12,
            stringers: Stringers::Sides,
            stringer_width: 0.06,
            stringer_depth: 0.25,
            tread_thickness: 0.04,
            closed_risers: false,
        }
    }
}

impl StairBuild {
    /// Replace invalid sizes by defaults; returns how many changed.
    pub fn repair(&mut self) -> usize {
        let d = StairBuild::default();
        let mut n = 0;
        for (v, default) in [
            (&mut self.waist, d.waist),
            (&mut self.stringer_width, d.stringer_width),
            (&mut self.stringer_depth, d.stringer_depth),
            (&mut self.tread_thickness, d.tread_thickness),
        ] {
            if !v.is_finite() || *v < 0.005 || *v > 2.0 {
                *v = default;
                n += 1;
            }
        }
        n
    }
}

/// How the tread depth of a stair type is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TreadRule {
    /// The type's tread depth.
    #[default]
    Fixed,
    /// From Blondel's formula: the middle of the step range minus twice the riser.
    Blondel,
}

impl TreadRule {
    pub const ALL: [TreadRule; 2] = [TreadRule::Fixed, TreadRule::Blondel];

    pub fn key(self) -> &'static str {
        match self {
            TreadRule::Fixed => "stair-tread-fixed",
            TreadRule::Blondel => "stair-tread-blondel",
        }
    }
}

/// Comfort and safety limits of a stair type (defaults after common
/// building codes: risers 16-18 cm, treads at least 28 cm and
/// `2 × riser + tread` between 63 and 65 cm).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StairRules {
    pub tread_rule: TreadRule,
    pub min_riser: f64,
    pub min_tread: f64,
    /// Range of Blondel's step `2 × riser + tread`.
    pub blondel_min: f64,
    pub blondel_max: f64,
}

impl Default for StairRules {
    fn default() -> Self {
        Self { tread_rule: TreadRule::Fixed, min_riser: 0.16, min_tread: 0.28, blondel_min: 0.63, blondel_max: 0.65 }
    }
}

impl StairRules {
    /// Replace invalid limits by defaults; returns how many changed.
    pub fn repair(&mut self) -> usize {
        let d = StairRules::default();
        let mut n = 0;
        for (v, default) in [
            (&mut self.min_riser, d.min_riser),
            (&mut self.min_tread, d.min_tread),
            (&mut self.blondel_min, d.blondel_min),
            (&mut self.blondel_max, d.blondel_max),
        ] {
            if !v.is_finite() || *v < 0.0 || *v > 2.0 {
                *v = default;
                n += 1;
            }
        }
        if self.blondel_max < self.blondel_min {
            std::mem::swap(&mut self.blondel_min, &mut self.blondel_max);
            n += 1;
        }
        n
    }

    /// Tread depth for a riser height: `tread` for a fixed rule, else the
    /// middle of the Blondel range minus twice the riser (at least 10 cm).
    pub fn tread_for(&self, riser: f64, tread: f64) -> f64 {
        match self.tread_rule {
            TreadRule::Fixed => tread,
            TreadRule::Blondel => ((self.blondel_min + self.blondel_max) / 2.0 - 2.0 * riser).max(0.1),
        }
    }
}

/// Riser count, riser height and tread depth of a stair of `height`: the
/// fewest risers at most `max_riser` high; the tread from the rules.
pub fn stair_steps(height: f64, max_riser: f64, tread: f64, rules: &StairRules) -> (usize, f64, f64) {
    let (risers, riser, _) = crate::stair_layout(height, max_riser, tread);
    (risers, riser, rules.tread_for(riser, tread.max(0.0)))
}

/// A broken rule of a stair.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "issue", rename_all = "snake_case")]
pub enum StairIssue {
    RiserTooHigh { riser: f64, max: f64 },
    RiserTooLow { riser: f64, min: f64 },
    TreadTooShort { tread: f64, min: f64 },
    BlondelOutOfRange { step: f64, min: f64, max: f64 },
    LandingTooShort { landing: f64, min: f64 },
}

impl StairIssue {
    pub fn key(&self) -> &'static str {
        match self {
            StairIssue::RiserTooHigh { .. } => "stair-issue-riser-high",
            StairIssue::RiserTooLow { .. } => "stair-issue-riser-low",
            StairIssue::TreadTooShort { .. } => "stair-issue-tread-short",
            StairIssue::BlondelOutOfRange { .. } => "stair-issue-blondel",
            StairIssue::LandingTooShort { .. } => "stair-issue-landing-short",
        }
    }
}

/// Riser, tread (on the walking line), Blondel's step and the rules they break.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StairCheck {
    pub risers: usize,
    pub riser: f64,
    pub tread: f64,
    /// `2 × riser + tread`.
    pub blondel: f64,
    pub issues: Vec<StairIssue>,
}

impl StairCheck {
    /// Check a riser and tread against `rules`, `max_riser` and, when the
    /// stair has one, a landing at least `width` deep.
    pub fn new(
        risers: usize,
        riser: f64,
        tread: f64,
        max_riser: f64,
        rules: &StairRules,
        landing: Option<(f64, f64)>,
    ) -> Self {
        let blondel = 2.0 * riser + tread;
        let mut issues = Vec::new();
        if riser > max_riser + 1e-6 {
            issues.push(StairIssue::RiserTooHigh { riser, max: max_riser });
        }
        if riser < rules.min_riser - 1e-6 {
            issues.push(StairIssue::RiserTooLow { riser, min: rules.min_riser });
        }
        if tread < rules.min_tread - 1e-6 {
            issues.push(StairIssue::TreadTooShort { tread, min: rules.min_tread });
        }
        if blondel < rules.blondel_min - 1e-6 || blondel > rules.blondel_max + 1e-6 {
            issues.push(StairIssue::BlondelOutOfRange {
                step: blondel,
                min: rules.blondel_min,
                max: rules.blondel_max,
            });
        }
        if let Some((landing, min)) = landing
            && landing < min - 1e-6
        {
            issues.push(StairIssue::LandingTooShort { landing, min });
        }
        Self { risers, riser, tread, blondel, issues }
    }

    pub fn ok(&self) -> bool {
        self.issues.is_empty()
    }
}

/// Side of a stair, walking up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum StairSide {
    #[default]
    Left,
    Right,
}

impl StairSide {
    pub const ALL: [StairSide; 2] = [StairSide::Left, StairSide::Right];

    pub fn flipped(self) -> StairSide {
        match self {
            StairSide::Left => StairSide::Right,
            StairSide::Right => StairSide::Left,
        }
    }
}

/// A railing that follows a side of a stair (`host`), `inset` in from its edge.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StairRailing {
    pub host: crate::ElementId,
    pub side: StairSide,
    #[serde(default = "default_inset")]
    pub inset: f64,
}

fn default_inset() -> f64 {
    0.05
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blondel_tread_follows_the_riser() {
        let rules = StairRules { tread_rule: TreadRule::Blondel, ..Default::default() };
        // 2.88 m in 16 risers of 18 cm: 64 - 36 = 28 cm.
        let (risers, riser, tread) = stair_steps(2.88, 0.18, 0.30, &rules);
        assert_eq!(risers, 16);
        assert!((riser - 0.18).abs() < 1e-12);
        assert!((tread - 0.28).abs() < 1e-12);
        let check = StairCheck::new(risers, riser, tread, 0.18, &rules, None);
        assert!(check.ok(), "{:?}", check.issues);
        assert!((check.blondel - 0.64).abs() < 1e-12);
    }

    #[test]
    fn check_reports_broken_rules() {
        let rules = StairRules::default();
        // 15 cm risers with 25 cm treads: 55 cm step, low riser, short tread.
        let check = StairCheck::new(10, 0.15, 0.25, 0.18, &rules, Some((0.8, 1.0)));
        let keys: Vec<_> = check.issues.iter().map(|i| i.key()).collect();
        assert_eq!(
            keys,
            ["stair-issue-riser-low", "stair-issue-tread-short", "stair-issue-blondel", "stair-issue-landing-short"]
        );
    }

    #[test]
    fn a_layout_can_set_the_risers_and_tread() {
        let rules = StairRules::default();
        let auto = StairLayout::default();
        assert_eq!(auto.steps(2.88, 0.18, 0.28, &rules), stair_steps(2.88, 0.18, 0.28, &rules));
        let set = StairLayout { risers: 18, tread: 0.3, ..Default::default() };
        let (n, riser, tread) = set.steps(2.88, 0.18, 0.28, &rules);
        assert_eq!(n, 18);
        assert!((riser - 0.16).abs() < 1e-12 && (tread - 0.3).abs() < 1e-12);
        // Bad heights give flat steps, not NaN.
        assert_eq!(set.steps(f64::NAN, 0.18, 0.28, &rules).1, 0.0);
        let mut bad = StairLayout { risers: 900, split2: 900, tread: f64::NAN, ..Default::default() };
        assert_eq!(bad.repair(), 3);
        assert_eq!((bad.risers, bad.split2, bad.tread), (0, 0, 0.0));
    }

    #[test]
    fn repair_resets_bad_values() {
        let mut l = StairLayout { gap: f64::NAN, inner_radius: -1.0, winders: 99, ..Default::default() };
        assert_eq!(l.repair(), 3);
        let mut b = StairBuild { waist: f64::INFINITY, ..Default::default() };
        assert_eq!(b.repair(), 1);
        let mut r = StairRules { blondel_min: 0.7, blondel_max: 0.6, ..Default::default() };
        assert_eq!(r.repair(), 1);
        assert!(r.blondel_min < r.blondel_max);
    }
}
