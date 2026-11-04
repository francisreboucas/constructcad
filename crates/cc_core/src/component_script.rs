//! Scripted component pieces: the model side.
//!
//! A [`crate::ComponentShape::Script`] piece holds Lua source that declares
//! parameters and hotspots and returns ordinary pieces from its `geometry`
//! function. Running Lua is not this crate's business: resolving goes
//! through a [`ScriptEval`] (the `cc_script` crate implements it), which
//! gets the parameter values and returns the pieces, or an error and the
//! last pieces that worked for the same owner.
//!
//! The parameters a script declares are copied into the definition's
//! `params` (marked `from_script`) and its hotspots into `hotspots`, so
//! schedules, the properties palette, engine commands and files see them
//! without running Lua.

use crate::component::ComponentItem;
use crate::ids::Uid;
use glam::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Longest script source kept (bytes); longer ones are emptied on load.
pub const MAX_SCRIPT_LEN: usize = 256 * 1024;
/// Most choices of a choice parameter.
pub const MAX_CHOICES: usize = 256;
/// Longest text value, label or choice key kept (bytes).
pub const MAX_TEXT_LEN: usize = 4096;
/// Most hotspots of a definition.
pub const MAX_HOTSPOTS: usize = 64;

/// One option of a choice parameter: a stable key and the label shown.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ParamChoice {
    pub key: String,
    #[serde(default)]
    pub label: String,
}

/// The marker drawn for a hotspot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum HotspotShape {
    #[default]
    Circle,
    Square,
    Triangle,
    Diamond,
    TriangleDown,
}

impl HotspotShape {
    pub const ALL: [HotspotShape; 5] = [
        HotspotShape::Circle,
        HotspotShape::Square,
        HotspotShape::Triangle,
        HotspotShape::Diamond,
        HotspotShape::TriangleDown,
    ];

    /// The name scripts use.
    pub fn name(self) -> &'static str {
        match self {
            HotspotShape::Circle => "circle",
            HotspotShape::Square => "square",
            HotspotShape::Triangle => "triangle",
            HotspotShape::Diamond => "diamond",
            HotspotShape::TriangleDown => "triangle_down",
        }
    }

    pub fn from_name(name: &str) -> Option<HotspotShape> {
        HotspotShape::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// A point of a component that its elements move with a grip; scripts read
/// where it is. Positions are in the component's frame (meters).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Hotspot {
    pub name: String,
    #[serde(default)]
    pub label: String,
    /// Where new elements have it.
    pub position: [f64; 3],
    #[serde(default)]
    pub shape: HotspotShape,
    /// RGBA.
    #[serde(default = "cyan")]
    pub color: [u8; 4],
}

fn cyan() -> [u8; 4] {
    [0, 255, 255, 255]
}

/// Parse `#RRGGBB` or `#RRGGBBAA` (alpha last).
pub fn parse_color(text: &str) -> Option<[u8; 4]> {
    let hex = text.strip_prefix('#')?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok());
    let alpha = if hex.len() == 8 { byte(6)? } else { 255 };
    Some([byte(0)?, byte(2)?, byte(4)?, alpha])
}

/// `#RRGGBBAA`.
pub fn color_text(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])
}

/// Whose script result this is, for the last pieces that worked: the
/// element's type and the element (none for a type's own preview).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct ScriptOwner {
    pub type_uid: Uid,
    pub element: Option<Uid>,
    /// The piece of the definition.
    pub piece: usize,
}

/// What a script receives.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScriptInput {
    /// Numeric parameters (booleans as 0 or 1), reference planes and extra
    /// names such as a door's wall thickness.
    pub numbers: BTreeMap<String, f64>,
    /// Text, choice (the key) and material (the name) parameters.
    pub texts: BTreeMap<String, String>,
    /// Hotspot positions.
    pub hotspots: BTreeMap<String, DVec3>,
}

/// A script's pieces, or why there are none this time.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScriptResult {
    /// The pieces, or the last that worked for the same owner when the
    /// script failed (none when it never worked).
    pub items: Option<Arc<Vec<ComponentItem>>>,
    pub error: Option<String>,
}

/// Runs a piece's script.
pub trait ScriptEval {
    fn eval(&self, source: &str, input: &ScriptInput, owner: ScriptOwner) -> ScriptResult;
}

/// No runtime: every script fails.
pub struct NoScripts;

impl ScriptEval for NoScripts {
    fn eval(&self, _: &str, _: &ScriptInput, _: ScriptOwner) -> ScriptResult {
        ScriptResult { items: None, error: Some("scripts cannot run here".into()) }
    }
}

/// Cut `text` to at most `max` bytes on a character boundary; whether it was cut.
pub fn truncate(text: &mut String, max: usize) -> bool {
    if text.len() <= max {
        return false;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_parse_with_alpha_last() {
        assert_eq!(parse_color("#00ffff"), Some([0, 255, 255, 255]));
        assert_eq!(parse_color("#ffccddee"), Some([0xff, 0xcc, 0xdd, 0xee]));
        assert_eq!(parse_color("ffccdd"), None);
        assert_eq!(parse_color("#ffccd"), None);
        assert_eq!(parse_color("#gg0000"), None);
        assert_eq!(color_text([0xff, 0xcc, 0xdd, 0xee]), "#ffccddee");
    }

    #[test]
    fn shapes_have_script_names() {
        for s in HotspotShape::ALL {
            assert_eq!(HotspotShape::from_name(s.name()), Some(s));
        }
        assert_eq!(HotspotShape::from_name("hexagon"), None);
    }

    use crate::component::{ComponentItem, ComponentShape, DetailLevels, PartError, ResolveCtx};
    use crate::{ComponentDef, ComponentParam, ParamKind};

    /// Returns one box as wide as `width`, or fails when `fail` is set.
    struct Boxes;

    impl ScriptEval for Boxes {
        fn eval(&self, source: &str, input: &ScriptInput, _: ScriptOwner) -> ScriptResult {
            if source == "fail" {
                return ScriptResult { items: None, error: Some("boom".into()) };
            }
            let w = input.numbers.get("width").copied().unwrap_or(1.0);
            let x = input.hotspots.get("end").map_or(0.0, |p| p.x);
            let material = input.texts.get("finish").cloned().unwrap_or_default();
            let shape = ComponentShape::Box {
                x: format!("{x}"),
                y: "0".into(),
                z: "0".into(),
                width: format!("{w}"),
                depth: "1".into(),
                height: "1".into(),
            };
            let item = ComponentItem {
                name: "box".into(),
                shape,
                material,
                in_plan: true,
                void: false,
                plan_only: false,
                detail: DetailLevels::default(),
            };
            ScriptResult { items: Some(Arc::new(vec![item])), error: None }
        }
    }

    fn scripted(source: &str) -> ComponentDef {
        ComponentDef {
            params: vec![
                ComponentParam { instance: true, ..ComponentParam::number("width", 2.0, ParamKind::Length) },
                ComponentParam {
                    name: "finish".into(),
                    kind: ParamKind::Choice,
                    instance: true,
                    text: "oak".into(),
                    choices: vec![
                        ParamChoice { key: "oak".into(), label: "Oak".into() },
                        ParamChoice { key: "pine".into(), label: "Pine".into() },
                    ],
                    ..Default::default()
                },
            ],
            items: vec![ComponentItem {
                name: "script".into(),
                shape: ComponentShape::Script { source: source.into() },
                material: "Default".into(),
                in_plan: true,
                void: false,
                plan_only: false,
                detail: DetailLevels::default(),
            }],
            hotspots: vec![Hotspot { name: "end".into(), position: [0.5, 0.0, 0.0], ..Default::default() }],
            ..Default::default()
        }
    }

    #[test]
    fn script_pieces_resolve_through_the_evaluator() {
        let def = scripted("ok");
        let overrides = BTreeMap::from([("width".to_string(), 3.0)]);
        let texts = BTreeMap::from([("finish".to_string(), "pine".to_string())]);
        let hotspots = BTreeMap::from([("end".to_string(), [1.0, 0.0, 0.0])]);
        let ctx = ResolveCtx {
            overrides: &overrides,
            texts: &texts,
            hotspots: &hotspots,
            scripts: &Boxes,
            ..Default::default()
        };
        let (parts, errors) = def.resolve_with(&ctx);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].item, 0);
        assert_eq!(parts[0].material, "pine");
        let xs: Vec<f64> = parts[0].footprint.iter().map(|p| p.x).collect();
        assert_eq!(xs, vec![1.0, 4.0, 4.0, 1.0]);
        // Defaults: the type's width, choice and hotspot.
        let (parts, _) = def.resolve_with(&ResolveCtx { scripts: &Boxes, ..Default::default() });
        assert_eq!(parts[0].material, "oak");
        assert_eq!(parts[0].footprint[1].x, 2.5);
    }

    #[test]
    fn choices_accept_only_their_keys() {
        let def = scripted("ok");
        let texts = BTreeMap::from([("finish".to_string(), "steel".to_string())]);
        assert_eq!(def.texts(&texts)["finish"], "oak");
        assert!(!def.values(&BTreeMap::new()).contains_key("finish"));
    }

    #[test]
    fn script_errors_and_missing_runtime_are_reported() {
        let (parts, errors) = scripted("fail").resolve_with(&ResolveCtx { scripts: &Boxes, ..Default::default() });
        assert!(parts.is_empty());
        assert!(matches!(&errors[..], [(0, PartError::Script(m))] if m == "boom"));
        let (_, errors) = scripted("ok").resolve(&BTreeMap::new());
        assert!(matches!(&errors[..], [(0, PartError::Script(_))]));
    }

    #[test]
    fn truncation_keeps_whole_characters() {
        let mut s = "aé".repeat(3);
        assert!(truncate(&mut s, 4));
        assert_eq!(s, "aéa");
        assert!(!truncate(&mut s, 100));
    }
}
