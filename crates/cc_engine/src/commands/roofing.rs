//! Roof coverings and framing: checking a roof against its tiles.

use crate::error::{EngineError, Result};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::roofing::{FramingSystem, TileKind};
use cc_core::{ElementId, ElementKind, TypeData};
use cc_geom::roof_framing::{MemberKind, RoofFramingGeom};
use cc_views::format;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Check a roof against its covering: the least slope its tiles take and
/// the roof's slopes, the area covered, the number of tiles and their
/// weight, and its framing (purlins, rafters, battens and truss members:
/// lengths, the number of trusses and the volume). The covering and framing are set on the roof's type
/// (`type_set` with `covering` and `framing`).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoofCheck {
    /// Roof element id.
    pub id: u64,
}

/// A roof's covering and framing figures as JSON, with the rules it breaks.
pub fn roof_json(covering: Option<TileKind>, system: FramingSystem, g: &RoofFramingGeom) -> Value {
    let spec = covering.map(TileKind::spec);
    let mut issues = Vec::new();
    if let Some(spec) = spec {
        match g.slopes {
            Some((lo, _)) if !spec.accepts(lo) => issues.push(t!(
                "msg-roof-too-flat",
                "slope" => format::slope(lo),
                "min" => format::slope(spec.min_slope)
            )),
            None => issues.push(t!("msg-roof-flat-tiles")),
            _ => {}
        }
    }
    json!({
        "covering": covering,
        "min_slope": spec.map(|s| s.min_slope),
        "slopes": g.slopes.map(|(lo, hi)| [lo, hi]),
        "sloped_area": g.sloped_area,
        "tiles": spec.and_then(|s| s.tiles(g.sloped_area)),
        "weight": spec.map(|s| s.weight_on(g.sloped_area)),
        "framing": {
            "system": system,
            "purlins": g.length(MemberKind::Purlin),
            "rafters": g.length(MemberKind::Rafter),
            "battens": g.length(MemberKind::Batten),
            "trusses": g.trusses,
            "truss_members": g.length(MemberKind::Truss),
            "members": g.members.len(),
            "volume": g.volume(),
        },
        "ok": issues.is_empty(),
        "issues": issues,
    })
}

impl Command for RoofCheck {
    const NAME: &'static str = "roof_check";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let e =
            p.element(ElementId(self.id)).ok_or_else(|| EngineError::not_found(format!("no element {}", self.id)))?;
        if !matches!(e.kind, ElementKind::Roof(_)) {
            return Err(EngineError::invalid(format!("element {} is not a roof", self.id)));
        }
        let (covering, system) = match p.element_type(e.type_id).map(|t| &t.data) {
            Some(TypeData::Roof { covering, framing, .. }) => (*covering, framing.system),
            _ => (None, FramingSystem::None),
        };
        let id = e.id;
        s.geometry();
        let g = s.geom().roof_framing.get(&id).cloned().unwrap_or_default();
        Ok(roof_json(covering, system, &g))
    }
}
