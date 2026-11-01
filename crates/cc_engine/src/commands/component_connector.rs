//! Connectors of a component: the points where pipes and conduits join it
//! (a toilet's water inlet and sewer outlet, an outlet's electrical
//! connection), each with the kind of system it takes, a position and a
//! diameter as expressions, and the direction runs leave it. A reference
//! plane's name in a coordinate locks the connector to that plane.

use super::component::edit_def;
use super::component_piece::ExprArg;
use crate::error::{EngineError, Result};
use crate::refs::Ref;
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use cc_core::component::{ComponentConnector, ConnectorDirection};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Most connectors of a component.
const MAX_CONNECTORS: usize = 64;

/// Names of the connector directions in commands.
pub const DIRECTIONS: [(&str, ConnectorDirection); 6] = [
    ("down", ConnectorDirection::Down),
    ("up", ConnectorDirection::Up),
    ("front", ConnectorDirection::Front),
    ("back", ConnectorDirection::Back),
    ("left", ConnectorDirection::Left),
    ("right", ConnectorDirection::Right),
];

/// The command name of a direction.
pub fn direction_name(d: ConnectorDirection) -> &'static str {
    DIRECTIONS.iter().find(|(_, x)| *x == d).map_or("down", |(n, _)| n)
}

fn direction(name: &str) -> Result<ConnectorDirection> {
    let wanted = name.trim().to_ascii_lowercase();
    DIRECTIONS.iter().find(|(n, _)| *n == wanted).map(|(_, d)| *d).ok_or_else(|| {
        let names: Vec<&str> = DIRECTIONS.iter().map(|(n, _)| *n).collect();
        EngineError::invalid(format!("unknown direction '{name}' (one of {})", names.join(", ")))
    })
}

fn name(text: &str) -> Result<String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(EngineError::invalid("a connector needs a name"));
    }
    Ok(text.to_string())
}

/// An empty diameter clears it; anything else is an expression.
fn diameter(d: &ExprArg) -> Result<String> {
    match d {
        ExprArg::Text(t) if t.trim().is_empty() => Ok(String::new()),
        other => other.text(),
    }
}

/// Add a connector to a component: where a pipe or conduit of `system`
/// joins it. `x`, `y` and `z` place it in the component's frame (numbers or
/// expressions; a reference plane's name locks it to that plane), and
/// `direction` is the way runs leave it.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentConnectorAdd {
    /// A type of the component: id or name.
    #[serde(rename = "type")]
    pub type_ref: Ref,
    /// Name (default `Connector1`, `Connector2`…).
    #[serde(default)]
    pub name: Option<String>,
    /// Kind of system: `cold_water` (default), `hot_water`, `sanitary`,
    /// `vent`, `rainwater`, `power` or `lighting`.
    #[serde(default)]
    pub system: Option<String>,
    /// Position across the component (default 0).
    #[serde(default)]
    pub x: Option<ExprArg>,
    /// Position front to back (default 0).
    #[serde(default)]
    pub y: Option<ExprArg>,
    /// Height (default 0).
    #[serde(default)]
    pub z: Option<ExprArg>,
    /// Which way runs leave it: `down` (default), `up`, `front`, `back`, `left` or `right`.
    #[serde(default)]
    pub direction: Option<String>,
    /// Outside diameter of the run it takes, meters (default: unset).
    #[serde(default)]
    pub diameter: Option<ExprArg>,
}

impl Command for ComponentConnectorAdd {
    const NAME: &'static str = "component_connector_add";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let coord = |e: &Option<ExprArg>| e.as_ref().map(ExprArg::text).transpose().map(|t| t.unwrap_or("0".into()));
        let (x, y, z) = (coord(&self.x)?, coord(&self.y)?, coord(&self.z)?);
        let system = self.system.as_deref().map(super::mep::parse_kind).transpose()?.unwrap_or_default();
        let direction = self.direction.as_deref().map(direction).transpose()?.unwrap_or_default();
        let diameter = self.diameter.as_ref().map(diameter).transpose()?.unwrap_or_default();
        let wanted = self.name.as_deref().map(name).transpose()?;
        let (index, name) = edit_def(s, &self.type_ref, &[], |def| {
            if def.connectors.len() >= MAX_CONNECTORS {
                return Err(EngineError::invalid(format!("a component has at most {MAX_CONNECTORS} connectors")));
            }
            let name = match wanted {
                Some(n) if def.connectors.iter().any(|c| c.name == n) => {
                    return Err(EngineError::invalid(format!("there is already a connector '{n}'")));
                }
                Some(n) => n,
                None => (1..)
                    .map(|i| format!("Connector{i}"))
                    .find(|n| !def.connectors.iter().any(|c| c.name == *n))
                    .unwrap_or_default(),
            };
            def.connectors.push(ComponentConnector { name: name.clone(), system, x, y, z, direction, diameter });
            Ok((def.connectors.len() - 1, name))
        })?;
        Ok(json!({ "index": index, "name": name }))
    }
}

/// Change a connector of a component: its name, kind of system, position
/// (`x`, `y`, `z`, numbers or expressions; a reference plane's name locks it
/// to that plane), direction or diameter (empty to clear it).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentConnectorSet {
    /// A type of the component: id or name.
    #[serde(rename = "type")]
    pub type_ref: Ref,
    /// The connector's index (from `component_get`).
    pub index: usize,
    /// New name.
    #[serde(default)]
    pub name: Option<String>,
    /// Kind of system: `cold_water`, `hot_water`, `sanitary`, `vent`, `rainwater`, `power` or `lighting`.
    #[serde(default)]
    pub system: Option<String>,
    /// Position across the component.
    #[serde(default)]
    pub x: Option<ExprArg>,
    /// Position front to back.
    #[serde(default)]
    pub y: Option<ExprArg>,
    /// Height.
    #[serde(default)]
    pub z: Option<ExprArg>,
    /// Which way runs leave it: `down`, `up`, `front`, `back`, `left` or `right`.
    #[serde(default)]
    pub direction: Option<String>,
    /// Outside diameter of the run it takes, meters (empty to clear it).
    #[serde(default)]
    pub diameter: Option<ExprArg>,
}

impl Command for ComponentConnectorSet {
    const NAME: &'static str = "component_connector_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let text = |e: &Option<ExprArg>| e.as_ref().map(ExprArg::text).transpose();
        let (x, y, z) = (text(&self.x)?, text(&self.y)?, text(&self.z)?);
        let system = self.system.as_deref().map(super::mep::parse_kind).transpose()?;
        let direction = self.direction.as_deref().map(direction).transpose()?;
        let diameter = self.diameter.as_ref().map(diameter).transpose()?;
        let new_name = self.name.as_deref().map(name).transpose()?;
        edit_def(s, &self.type_ref, &[], |def| {
            if let Some(n) = &new_name
                && def.connectors.iter().enumerate().any(|(i, c)| i != self.index && c.name == *n)
            {
                return Err(EngineError::invalid(format!("there is already a connector '{n}'")));
            }
            let c = def
                .connectors
                .get_mut(self.index)
                .ok_or_else(|| EngineError::not_found(format!("no connector {}", self.index)))?;
            if let Some(n) = new_name {
                c.name = n;
            }
            if let Some(k) = system {
                c.system = k;
            }
            for (field, value) in [(&mut c.x, x), (&mut c.y, y), (&mut c.z, z)] {
                if let Some(v) = value {
                    *field = v;
                }
            }
            if let Some(d) = direction {
                c.direction = d;
            }
            if let Some(d) = diameter {
                c.diameter = d;
            }
            Ok(())
        })?;
        Ok(json!({ "index": self.index }))
    }
}

/// Remove a connector of a component.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentConnectorDelete {
    /// A type of the component: id or name.
    #[serde(rename = "type")]
    pub type_ref: Ref,
    /// The connector's index.
    pub index: usize,
}

impl Command for ComponentConnectorDelete {
    const NAME: &'static str = "component_connector_delete";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        edit_def(s, &self.type_ref, &[], |def| {
            if self.index >= def.connectors.len() {
                return Err(EngineError::not_found(format!("no connector {}", self.index)));
            }
            def.connectors.remove(self.index);
            Ok(())
        })?;
        Ok(json!({ "deleted": self.index }))
    }
}
