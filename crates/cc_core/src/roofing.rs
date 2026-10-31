//! Roof coverings and framing. A roof type may name its tiles (a kind from
//! a built-in catalog, with reference values: the least slope they take,
//! their weight, how many make a square meter, the batten gauge and the
//! widest span between purlins) and its framing: purlins with rafters and
//! battens (timber), or purlins only (steel, for sheets). The framing is
//! derived from the roof's sloped faces (see `cc_geom::roof_framing`).
//!
//! The catalog's values are typical figures from public product data, for
//! early design; manufacturers' figures prevail.

use serde::{Deserialize, Serialize};

/// Kinds of roof covering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TileKind {
    /// Ceramic tiles laid as caps and channels (colonial).
    CeramicColonial,
    /// Ceramic interlocking tiles (French).
    CeramicFrench,
    /// Ceramic interlocking tiles (Roman).
    CeramicRoman,
    /// Ceramic interlocking tiles (Portuguese).
    CeramicPortuguese,
    /// Concrete tiles.
    Concrete,
    /// Corrugated fiber cement sheets (6 mm).
    FiberCement,
    /// Trapezoidal metal sheets.
    MetalTrapezoidal,
    /// Insulated sandwich panels.
    Sandwich,
}

/// What a kind of covering takes and weighs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileSpec {
    /// Least slope, rise over run.
    pub min_slope: f64,
    /// Weight of the covering, kg per square meter of roof (wet tiles).
    pub weight: f64,
    /// Tiles per square meter of roof; `None` for sheets and panels.
    pub per_m2: Option<f64>,
    /// Distance between battens along the slope (the tile's gauge); `None`
    /// when the covering lies on purlins.
    pub batten_gauge: Option<f64>,
    /// Widest distance between purlins along the slope.
    pub purlin_span: f64,
    /// The framing the covering usually lies on.
    pub framing: FramingSystem,
}

impl TileSpec {
    /// Whether a roof at `slope` (rise over run) is steep enough.
    pub fn accepts(&self, slope: f64) -> bool {
        slope + 1e-9 >= self.min_slope
    }

    /// Tiles for `area` square meters of roof, rounded up; `None` for sheets.
    pub fn tiles(&self, area: f64) -> Option<f64> {
        self.per_m2.map(|n| (n * area).ceil())
    }

    /// Weight of the covering on `area` square meters of roof, kg.
    pub fn weight_on(&self, area: f64) -> f64 {
        self.weight * area
    }
}

impl TileKind {
    pub const ALL: [TileKind; 8] = [
        TileKind::CeramicColonial,
        TileKind::CeramicFrench,
        TileKind::CeramicRoman,
        TileKind::CeramicPortuguese,
        TileKind::Concrete,
        TileKind::FiberCement,
        TileKind::MetalTrapezoidal,
        TileKind::Sandwich,
    ];

    /// Translation key of the name.
    pub fn key(self) -> &'static str {
        match self {
            TileKind::CeramicColonial => "tile-ceramic-colonial",
            TileKind::CeramicFrench => "tile-ceramic-french",
            TileKind::CeramicRoman => "tile-ceramic-roman",
            TileKind::CeramicPortuguese => "tile-ceramic-portuguese",
            TileKind::Concrete => "tile-concrete",
            TileKind::FiberCement => "tile-fiber-cement",
            TileKind::MetalTrapezoidal => "tile-metal",
            TileKind::Sandwich => "tile-sandwich",
        }
    }

    /// The name written in files (IFC property sets).
    pub fn code(self) -> &'static str {
        match self {
            TileKind::CeramicColonial => "CeramicColonial",
            TileKind::CeramicFrench => "CeramicFrench",
            TileKind::CeramicRoman => "CeramicRoman",
            TileKind::CeramicPortuguese => "CeramicPortuguese",
            TileKind::Concrete => "Concrete",
            TileKind::FiberCement => "FiberCement",
            TileKind::MetalTrapezoidal => "MetalTrapezoidal",
            TileKind::Sandwich => "Sandwich",
        }
    }

    /// The kind written as `code`.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.code() == code.trim())
    }

    /// The catalog's figures for this kind.
    pub fn spec(self) -> TileSpec {
        use FramingSystem::{Steel, Timber};
        let tiles = |min_slope, weight, per_m2, gauge| TileSpec {
            min_slope,
            weight,
            per_m2: Some(per_m2),
            batten_gauge: Some(gauge),
            purlin_span: 1.5,
            framing: Timber,
        };
        let sheets = |min_slope, weight, purlin_span| TileSpec {
            min_slope,
            weight,
            per_m2: None,
            batten_gauge: None,
            purlin_span,
            framing: Steel,
        };
        match self {
            TileKind::CeramicColonial => tiles(0.25, 50.0, 24.0, 0.36),
            TileKind::CeramicFrench => tiles(0.32, 45.0, 16.0, 0.33),
            TileKind::CeramicRoman => tiles(0.30, 42.0, 16.0, 0.34),
            TileKind::CeramicPortuguese => tiles(0.30, 45.0, 17.0, 0.34),
            TileKind::Concrete => tiles(0.30, 48.0, 10.4, 0.32),
            TileKind::FiberCement => sheets(0.15, 18.0, 1.69),
            TileKind::MetalTrapezoidal => sheets(0.05, 6.0, 1.8),
            TileKind::Sandwich => sheets(0.05, 12.0, 2.5),
        }
    }
}

/// How a roof is framed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum FramingSystem {
    /// No framing drawn (a concrete slab, or framing left out).
    #[default]
    None,
    /// Timber purlins carrying rafters, the rafters carrying battens.
    Timber,
    /// Timber trusses carrying the purlins, rafters and battens of `Timber`.
    TimberTrusses,
    /// Steel purlins carrying sheets or panels directly.
    Steel,
}

impl FramingSystem {
    pub const ALL: [FramingSystem; 4] =
        [FramingSystem::None, FramingSystem::Timber, FramingSystem::TimberTrusses, FramingSystem::Steel];

    /// The name written in files (IFC property sets).
    pub fn code(self) -> &'static str {
        match self {
            FramingSystem::None => "None",
            FramingSystem::Timber => "Timber",
            FramingSystem::TimberTrusses => "TimberTrusses",
            FramingSystem::Steel => "Steel",
        }
    }

    /// The system written as `code`.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.code() == code.trim())
    }

    pub fn key(self) -> &'static str {
        match self {
            FramingSystem::None => "framing-none",
            FramingSystem::Timber => "framing-timber",
            FramingSystem::TimberTrusses => "framing-trusses",
            FramingSystem::Steel => "framing-steel",
        }
    }
}

/// A member's section: width and height, meters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MemberSize {
    pub width: f64,
    pub height: f64,
}

impl MemberSize {
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    fn is_valid(self) -> bool {
        self.width.is_finite() && self.height.is_finite() && self.width > 1e-3 && self.height > 1e-3
    }
}

/// The framing of a roof type: members under its sloped faces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoofFraming {
    pub system: FramingSystem,
    /// Material of the members.
    #[serde(default)]
    pub material: String,
    /// Purlins, along the eaves.
    pub purlin: MemberSize,
    /// Widest distance between purlins along the slope (the covering's
    /// span is used when smaller).
    pub purlin_spacing: f64,
    /// Rafters (timber), down the slope.
    pub rafter: MemberSize,
    pub rafter_spacing: f64,
    /// Battens (timber), along the eaves at the tile's gauge.
    pub batten: MemberSize,
    /// Trusses (`TimberTrusses`): the section of their members and the
    /// widest distance between them along the ridge.
    #[serde(default = "default_truss")]
    pub truss: MemberSize,
    #[serde(default = "default_truss_spacing")]
    pub truss_spacing: f64,
}

/// Trusses of 6 x 16 cm members.
fn default_truss() -> MemberSize {
    MemberSize::new(0.06, 0.16)
}

/// Trusses at most 3 m apart.
fn default_truss_spacing() -> f64 {
    3.0
}

impl Default for RoofFraming {
    fn default() -> Self {
        Self::timber(String::new())
    }
}

impl RoofFraming {
    /// Timber framing of common sizes: 6 x 16 cm purlins at most 1.5 m
    /// apart, 5 x 6 cm rafters every 50 cm, 1.5 x 5 cm battens; `None` until
    /// a system is chosen.
    pub fn timber(material: String) -> Self {
        Self {
            system: FramingSystem::None,
            material,
            purlin: MemberSize::new(0.06, 0.16),
            purlin_spacing: 1.5,
            rafter: MemberSize::new(0.05, 0.06),
            rafter_spacing: 0.5,
            batten: MemberSize::new(0.015, 0.05),
            truss: default_truss(),
            truss_spacing: default_truss_spacing(),
        }
    }

    /// Sizes and spacings that are positive and finite; the others are reset
    /// to the defaults. Returns how many were reset.
    pub fn repair(&mut self) -> usize {
        let d = Self::timber(String::new());
        let mut n = 0;
        for (size, default) in [
            (&mut self.purlin, d.purlin),
            (&mut self.rafter, d.rafter),
            (&mut self.batten, d.batten),
            (&mut self.truss, d.truss),
        ] {
            if !size.is_valid() {
                *size = default;
                n += 1;
            }
        }
        for (v, default) in [
            (&mut self.purlin_spacing, d.purlin_spacing),
            (&mut self.rafter_spacing, d.rafter_spacing),
            (&mut self.truss_spacing, d.truss_spacing),
        ] {
            if !v.is_finite() || *v < 0.1 {
                *v = default;
                n += 1;
            }
        }
        n
    }

    /// Distance between purlins along the slope for `covering`.
    pub fn purlin_step(&self, covering: Option<TileKind>) -> f64 {
        let span = covering.map_or(f64::INFINITY, |c| c.spec().purlin_span);
        self.purlin_spacing.min(span).max(0.1)
    }

    /// Distance between battens along the slope for `covering`.
    pub fn batten_step(&self, covering: Option<TileKind>) -> f64 {
        covering.and_then(|c| c.spec().batten_gauge).unwrap_or(0.35).max(0.05)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_and_spacings() {
        for kind in TileKind::ALL {
            let s = kind.spec();
            assert!(s.min_slope > 0.0 && s.weight > 0.0 && s.purlin_span > 0.5, "{kind:?}");
            assert_eq!(s.per_m2.is_some(), s.batten_gauge.is_some(), "tiles have both, sheets neither: {kind:?}");
        }
        let f = RoofFraming::timber("Madeira".into());
        assert_eq!(f.purlin_step(Some(TileKind::FiberCement)), 1.5, "the framing's spacing when narrower");
        let wide = RoofFraming { purlin_spacing: 3.0, ..f.clone() };
        assert_eq!(wide.purlin_step(Some(TileKind::FiberCement)), 1.69, "the sheet's span when narrower");
        assert_eq!(f.batten_step(Some(TileKind::CeramicFrench)), 0.33);
        let mut bad = RoofFraming { purlin: MemberSize::new(f64::NAN, 0.1), rafter_spacing: -1.0, ..f };
        assert_eq!(bad.repair(), 2);
        assert_eq!(bad.rafter_spacing, 0.5);
    }
}
