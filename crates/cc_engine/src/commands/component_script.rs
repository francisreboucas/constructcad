//! Scripted components: writing a script piece (its declarations become the
//! component's parameters and hotspots), reading and setting parameter
//! values of a type or of placed elements, and moving hotspots. See
//! `cc_script::object` for the scripts themselves.

use super::component::edit_def;
use super::edit;
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::component::{ComponentItem, ComponentShape, DetailLevels};
use cc_core::component_script::{MAX_SCRIPT_LEN, MAX_TEXT_LEN, color_text};
use cc_core::{ComponentDef, ComponentParam, ElementId, ElementKind, ElementTypeId, ParamKind, Project, TypeData};
use glam::{DVec2, DVec3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Write the Lua source of a script piece of a component: the piece named,
/// or a new script piece. What the script declares becomes the component's
/// parameters and hotspots (values of those still declared are kept); a
/// script that does not load is refused, one that fails while building its
/// pieces is kept and its error returned.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentScriptSet {
    /// A type of the component: id or name.
    #[serde(rename = "type")]
    pub type_ref: Ref,
    /// The Lua source: `param.length { "width", default = 1 }` declarations and a
    /// `geometry(p)` function returning pieces (`box`, `cylinder`, `extrusion`,
    /// `profile`, `revolution`, `sweep`, `blend`, `mesh`).
    pub source: String,
    /// The script piece to change; a new piece when left out.
    #[serde(default)]
    pub piece: Option<String>,
    /// Name of a new piece (default `Script1`, `Script2`…).
    #[serde(default)]
    pub name: Option<String>,
    /// Material of the pieces that set none (project material name).
    #[serde(default)]
    pub material: Option<String>,
}

impl Command for ComponentScriptSet {
    const NAME: &'static str = "component_script_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        if self.source.len() > MAX_SCRIPT_LEN {
            return Err(EngineError::invalid(format!("the script is longer than {MAX_SCRIPT_LEN} bytes")));
        }
        let material = match &self.material {
            Some(m) if !m.is_empty() => Some(refs::material(s.project(), &Ref::Name(m.clone()))?),
            Some(_) => Some(String::new()),
            None => None,
        };
        let decl = cc_script::declare(&self.source).map_err(|e| EngineError::invalid(format!("script: {e}")))?;
        let piece = edit_def(s, &self.type_ref, &[], |def| {
            let others_script = def.items.iter().filter(|i| matches!(i.shape, ComponentShape::Script { .. })).count();
            let index = match &self.piece {
                Some(name) => {
                    let Some(i) = def.items.iter().position(|i| &i.name == name) else {
                        return Err(EngineError::not_found(format!("no piece '{name}'")));
                    };
                    if !matches!(def.items[i].shape, ComponentShape::Script { .. }) {
                        return Err(EngineError::invalid(format!("'{name}' is not a script piece")));
                    }
                    i
                }
                None => {
                    if others_script > 0 {
                        return Err(EngineError::invalid(
                            "the component has a script piece already: name it with `piece`",
                        ));
                    }
                    let name = match &self.name {
                        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
                        _ => (1..)
                            .map(|i| format!("Script{i}"))
                            .find(|n| def.items.iter().all(|i| &i.name != n))
                            .unwrap_or_else(|| "Script".into()),
                    };
                    def.items.push(ComponentItem {
                        name,
                        shape: ComponentShape::Script { source: String::new() },
                        material: String::new(),
                        in_plan: true,
                        void: false,
                        plan_only: false,
                        detail: DetailLevels::default(),
                    });
                    def.items.len() - 1
                }
            };
            if self.piece.is_some() && others_script > 1 {
                return Err(EngineError::invalid("a component holds one script piece"));
            }
            let item = &mut def.items[index];
            item.shape = ComponentShape::Script { source: self.source.clone() };
            if let Some(m) = &material {
                item.material.clone_from(m);
            }
            let name = item.name.clone();
            cc_script::object::sync(&mut def.params, &mut def.hotspots, &decl).map_err(EngineError::invalid)?;
            Ok(name)
        })?;
        let (id, def) = def_of(s.project(), &self.type_ref)?;
        let errors = script_errors(s.project(), id, &def, None);
        Ok(json!({
            "piece": piece,
            "params": decl.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            "hotspots": decl.hotspots.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(),
            "errors": errors,
        }))
    }
}

/// The definition of component type `r`.
fn def_of(p: &Project, r: &Ref) -> Result<(ElementTypeId, ComponentDef)> {
    let id = refs::element_type(p, r)?;
    match p.element_type(id).map(|t| &t.data) {
        Some(TypeData::Component { def, .. }) => Ok((id, def.clone())),
        _ => Err(EngineError::invalid("not a component type")),
    }
}

/// The errors of a type's script pieces, run with the type's values or an
/// element's.
pub(crate) fn script_errors(
    p: &Project,
    ty: ElementTypeId,
    def: &ComponentDef,
    element: Option<ElementId>,
) -> Vec<String> {
    if !def.has_script() {
        return Vec::new();
    }
    let Some(t) = p.element_type(ty) else { return Vec::new() };
    let errors = match element.and_then(|id| p.element(id)) {
        Some(e) => {
            let c = match &e.kind {
                ElementKind::Component(c) => Some(c),
                _ => None,
            };
            cc_geom::instance_errors(t, def, e, c)
        }
        None => cc_geom::resolve_pieces(def, &BTreeMap::new(), &[(cc_core::component::WALL_THICKNESS, 0.15)]).1,
    };
    errors
        .into_iter()
        .filter(|(_, e)| matches!(e, cc_core::component::PartError::Script(_)))
        .map(|(i, e)| format!("{}: {e}", def.items.get(i).map_or(String::new(), |it| it.name.clone())))
        .collect()
}

/// A parameter's value as JSON: numbers, booleans and texts.
fn value_json(p: &ComponentParam, numbers: &BTreeMap<String, f64>, texts: &BTreeMap<String, String>) -> Value {
    match p.kind {
        ParamKind::Boolean => json!(numbers.get(&p.name).is_some_and(|v| *v != 0.0)),
        k if k.is_text() => json!(texts.get(&p.name)),
        _ => json!(numbers.get(&p.name)),
    }
}

fn param_json(p: &ComponentParam) -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    m.insert("name".into(), json!(p.name));
    m.insert("label".into(), json!(p.display()));
    m.insert("kind".into(), json!(p.kind.name()));
    m.insert("instance".into(), json!(p.instance));
    m.insert("from_script".into(), json!(p.from_script));
    if p.kind == ParamKind::Choice {
        let choices: Vec<Value> = p.choices.iter().map(|c| json!({ "key": c.key, "label": c.label })).collect();
        m.insert("choices".into(), json!(choices));
    }
    m
}

/// The components `ids` (all components of one component definition) with their types.
fn components_of(p: &Project, ids: &[u64]) -> Result<Vec<(ElementId, ElementTypeId, ComponentDef)>> {
    if ids.is_empty() {
        return Err(EngineError::invalid("no elements"));
    }
    let mut out = Vec::new();
    let mut component: Option<&str> = None;
    for id in refs::elements(p, ids)? {
        let Some(e) = p.element(id) else { return Err(EngineError::not_found(format!("no element {}", id.0))) };
        let ElementKind::Component(_) = &e.kind else {
            return Err(EngineError::invalid(format!("element {} is not a component", id.0)));
        };
        let Some(TypeData::Component { component: name, def }) = p.element_type(e.type_id).map(|t| &t.data) else {
            return Err(EngineError::invalid(format!("element {} has no component type", id.0)));
        };
        match component {
            Some(c) if c != name => {
                return Err(EngineError::invalid("the elements are of different components"));
            }
            _ => component = Some(name),
        }
        out.push((id, e.type_id, def.clone()));
    }
    Ok(out)
}

/// The parameters and hotspots of a component type, with the type's values;
/// or of placed components (all of one component), with the value they
/// share or `mixed` where they differ.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentParamsGet {
    /// A type of the component: id or name (leave out with `elements`).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Placed components of one component (leave out with `type`).
    #[serde(default)]
    pub elements: Vec<u64>,
}

impl Command for ComponentParamsGet {
    const NAME: &'static str = "component_params_get";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let empty = cc_core::Component::default();
        // (definition, numbers, texts, hotspots) per element or for the type.
        type Values = (ComponentDef, BTreeMap<String, f64>, BTreeMap<String, String>, BTreeMap<String, DVec3>);
        let (values, errors): (Vec<Values>, Vec<String>) = match (&self.type_ref, self.elements.is_empty()) {
            (Some(r), true) => {
                let (id, def) = def_of(p, r)?;
                let errors = script_errors(p, id, &def, None);
                let v = (
                    def.clone(),
                    def.values(&empty.overrides),
                    def.texts(&empty.text_overrides),
                    def.hotspot_positions(&empty.hotspots),
                );
                (vec![v], errors)
            }
            (None, false) => {
                let list = components_of(p, &self.elements)?;
                let mut errors = Vec::new();
                let mut values = Vec::new();
                for (id, ty, def) in list {
                    let c = match p.element(id).map(|e| &e.kind) {
                        Some(ElementKind::Component(c)) => c,
                        _ => &empty,
                    };
                    errors.extend(
                        script_errors(p, ty, &def, Some(id)).into_iter().map(|e| format!("element {}: {e}", id.0)),
                    );
                    values.push((
                        def.clone(),
                        def.values(&c.overrides),
                        def.texts(&c.text_overrides),
                        def.hotspot_positions(&c.hotspots),
                    ));
                }
                (values, errors)
            }
            _ => return Err(EngineError::invalid("give either `type` or `elements`")),
        };
        let Some((first, ..)) = values.first() else { return Err(EngineError::invalid("no elements")) };
        let params: Vec<Value> = first
            .params
            .iter()
            .map(|param| {
                let all: Vec<Value> = values
                    .iter()
                    .map(|(d, n, t, _)| d.param(&param.name).map_or(Value::Null, |q| value_json(q, n, t)))
                    .collect();
                let mixed = all.windows(2).any(|w| w[0] != w[1]);
                let mut m = param_json(param);
                m.insert("value".into(), if mixed { Value::Null } else { all.first().cloned().unwrap_or(Value::Null) });
                m.insert("mixed".into(), json!(mixed));
                Value::Object(m)
            })
            .collect();
        let hotspots: Vec<Value> = first
            .hotspots
            .iter()
            .map(|h| {
                let all: Vec<Option<DVec3>> = values.iter().map(|(_, _, _, hs)| hs.get(&h.name).copied()).collect();
                let mixed = all.windows(2).any(|w| w[0] != w[1]);
                let position = all.first().copied().flatten().filter(|_| !mixed).map(|p| [p.x, p.y, p.z]);
                json!({
                    "name": h.name,
                    "label": if h.label.is_empty() { h.name.clone() } else { h.label.clone() },
                    "shape": h.shape.name(),
                    "color": color_text(h.color),
                    "position": position,
                    "mixed": mixed,
                })
            })
            .collect();
        Ok(json!({ "params": params, "hotspots": hotspots, "errors": errors }))
    }
}

/// A new value for `param`, checked: a finite number (lengths in meters,
/// angles in degrees), true or false, a text, a choice's key or a project
/// material's name.
enum NewValue {
    Number(f64),
    Text(String),
}

fn new_value(p: &Project, param: &ComponentParam, value: &Value) -> Result<NewValue> {
    let wrong = |what: &str| EngineError::invalid(format!("'{}' takes {what}", param.name));
    match param.kind {
        ParamKind::Boolean => match value {
            Value::Bool(b) => Ok(NewValue::Number(f64::from(u8::from(*b)))),
            Value::Number(n) if n.as_f64() == Some(0.0) || n.as_f64() == Some(1.0) => {
                Ok(NewValue::Number(n.as_f64().unwrap_or(0.0)))
            }
            _ => Err(wrong("true or false")),
        },
        ParamKind::Length | ParamKind::Angle | ParamKind::Number => {
            let v = value.as_f64().ok_or_else(|| wrong("a number"))?;
            Ok(NewValue::Number(refs::finite(v, &param.name)?))
        }
        ParamKind::Text => {
            let t = value.as_str().ok_or_else(|| wrong("a text"))?;
            if t.len() > MAX_TEXT_LEN {
                return Err(EngineError::invalid(format!("the text is longer than {MAX_TEXT_LEN} bytes")));
            }
            Ok(NewValue::Text(t.to_string()))
        }
        ParamKind::Choice => {
            let t = value.as_str().ok_or_else(|| wrong("the key of one of its choices"))?;
            if !param.accepts_text(t) {
                let keys: Vec<&str> = param.choices.iter().map(|c| c.key.as_str()).collect();
                return Err(EngineError::invalid(format!(
                    "'{t}' is not a choice of '{}' ({})",
                    param.name,
                    keys.join(", ")
                )));
            }
            Ok(NewValue::Text(t.to_string()))
        }
        ParamKind::Material => {
            let t = value.as_str().ok_or_else(|| wrong("a material name"))?;
            if !t.is_empty() && p.material(t).is_none() {
                return Err(EngineError::not_found(format!("no material '{t}'")));
            }
            Ok(NewValue::Text(t.to_string()))
        }
    }
}

/// Set a parameter of a component type (every element of the type follows)
/// or the instance value of placed components (all of one component; the
/// parameter must be an instance one). Values: numbers (meters, degrees),
/// true/false, texts, choice keys or material names. One undo step; when
/// any target refuses the value nothing changes.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentParamsSet {
    /// A type of the component: id or name (leave out with `elements`).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Placed components of one component (leave out with `type`).
    #[serde(default)]
    pub elements: Vec<u64>,
    /// The parameter's name.
    pub name: String,
    /// The new value.
    pub value: Value,
}

impl Command for ComponentParamsSet {
    const NAME: &'static str = "component_params_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        match (&self.type_ref, self.elements.is_empty()) {
            (Some(r), true) => {
                let (id, def) = def_of(p, r)?;
                let Some(param) = def.param(&self.name) else {
                    return Err(EngineError::not_found(format!("no parameter '{}'", self.name)));
                };
                let v = new_value(p, param, &self.value)?;
                edit(s, t!("tx-modify-type"), |e| {
                    if let Some(TypeData::Component { def, .. }) = e.type_mut(id).map(|t| &mut t.data)
                        && let Some(q) = def.params.iter_mut().find(|q| q.name == self.name)
                    {
                        match &v {
                            NewValue::Number(n) => q.value = *n,
                            NewValue::Text(t) => q.text.clone_from(t),
                        }
                    }
                    Ok(())
                })?;
                Ok(json!({ "type": id.0, "name": self.name }))
            }
            (None, false) => {
                let list = components_of(p, &self.elements)?;
                let mut changes = Vec::new();
                for (id, _, def) in &list {
                    let Some(param) = def.param(&self.name) else {
                        return Err(EngineError::not_found(format!("no parameter '{}'", self.name)));
                    };
                    if !param.instance {
                        return Err(EngineError::invalid(format!(
                            "'{}' is a type parameter: set it with `type`",
                            self.name
                        )));
                    }
                    changes.push((*id, new_value(p, param, &self.value)?));
                }
                edit(s, t!("tx-modify"), |e| {
                    for (id, v) in &changes {
                        if let Some(ElementKind::Component(c)) = e.element_mut(*id).map(|e| &mut e.kind) {
                            match v {
                                NewValue::Number(n) => {
                                    c.overrides.insert(self.name.clone(), *n);
                                }
                                NewValue::Text(t) => {
                                    c.text_overrides.insert(self.name.clone(), t.clone());
                                }
                            }
                        }
                    }
                    Ok(())
                })?;
                Ok(json!({ "elements": self.elements, "name": self.name }))
            }
            _ => Err(EngineError::invalid("give either `type` or `elements`")),
        }
    }
}

/// Move a hotspot of a placed component (what its script reads as
/// `p.<name>`). The point is in the component's frame (meters from the
/// insertion point, x to its right, y to its back, z up) unless `world`.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentHotspotMove {
    /// The placed component.
    pub element: u64,
    /// The hotspot's name.
    pub name: String,
    /// The new position `[x, y, z]`.
    pub point: [f64; 3],
    /// The point is in project coordinates (z absolute) instead of the component's frame.
    #[serde(default)]
    pub world: bool,
}

impl Command for ComponentHotspotMove {
    const NAME: &'static str = "component_hotspot_move";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::element(p, self.element)?;
        let point = DVec3::from_array(self.point);
        if !point.is_finite() {
            return Err(EngineError::invalid("the point must be finite"));
        }
        let Some(e) = p.element(id) else { return Err(EngineError::not_found("no such element")) };
        let ElementKind::Component(c) = &e.kind else { return Err(EngineError::invalid("not a component")) };
        let Some(TypeData::Component { def, .. }) = p.element_type(e.type_id).map(|t| &t.data) else {
            return Err(EngineError::invalid("not a component"));
        };
        if def.hotspot(&self.name).is_none() {
            return Err(EngineError::not_found(format!("no hotspot '{}'", self.name)));
        }
        let local = if self.world {
            let z = p.level_elevation(e.level) + c.base_offset;
            let plan = DVec2::from_angle(-c.rotation_deg.to_radians()).rotate(point.truncate() - c.position);
            plan.extend(point.z - z)
        } else {
            point
        };
        edit(s, t!("tx-modify"), |e| {
            if let Some(ElementKind::Component(c)) = e.element_mut(id).map(|e| &mut e.kind) {
                c.hotspots.insert(self.name.clone(), local.to_array());
            }
            Ok(())
        })?;
        Ok(json!({ "element": id.0, "name": self.name, "point": local.to_array() }))
    }
}
