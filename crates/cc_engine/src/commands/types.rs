//! Types and components: change, duplicate, delete, create components.

use super::{edit, patch_value, patch_variant};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::roofing::FramingSystem;
use cc_core::{ComponentDef, ElementType, ElementTypeId, TypeData, expr};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Parameter names valid and unique, sound reference planes, lines and
/// dimensions, at least one piece, every piece resolves.
pub(crate) fn validate(def: &ComponentDef) -> Result<()> {
    for (i, p) in def.params.iter().enumerate() {
        if !expr::is_valid_name(&p.name) {
            return Err(EngineError::invalid(format!("invalid parameter name '{}'", p.name)));
        }
        if def.params[..i].iter().any(|q| q.name == p.name) {
            return Err(EngineError::invalid(format!("parameter '{}' is repeated", p.name)));
        }
    }
    if def.items.is_empty() {
        return Err(EngineError::invalid("the component has no pieces"));
    }
    if let Some(problem) = def.ref_problems().into_iter().next() {
        return Err(EngineError::invalid(problem));
    }
    // Doors and windows are checked in a wall 15 cm thick, and need their opening.
    if matches!(def.kind, cc_core::ComponentKind::Door | cc_core::ComponentKind::Window) && def.opening_size().is_none()
    {
        return Err(EngineError::invalid("a door or window needs an opening with a positive width and height"));
    }
    let resolved = def.resolve_connectors(&Default::default());
    if let Some(c) = def.connectors.iter().enumerate().find(|(i, _)| !resolved.iter().any(|k| k.index == *i)) {
        return Err(EngineError::invalid(format!("connector '{}': invalid position or diameter", c.1.name)));
    }
    let (_, errors) = def.resolve_in(&Default::default(), &[(cc_core::component::WALL_THICKNESS, 0.15)]);
    if let Some((i, e)) = errors.first() {
        let piece = def.items.get(*i).map_or(String::new(), |it| it.name.clone());
        return Err(EngineError::invalid(format!("piece '{piece}': {e}")));
    }
    Ok(())
}

fn type_out(s: &Session, id: ElementTypeId) -> Value {
    super::query::type_json(s, id).unwrap_or_else(|| json!({ "id": id.0 }))
}

/// Change a type: its name, its parameters (`params`, merged into what
/// `type_get` shows: layers, sizes, materials, a component's `def`…) and its
/// identity data (`info`: type mark, description, manufacturer, cost…).
/// Changing a component's definition updates every type of that component,
/// each keeping its own parameter values.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TypeSet {
    #[serde(rename = "type")]
    pub type_ref: Ref,
    /// New name (unique in its category).
    #[serde(default)]
    pub name: Option<String>,
    /// Fields of the type's parameters to change, shaped like `params` in `type_get`.
    #[serde(default)]
    pub params: Option<Value>,
    /// Fields of the identity data to change, shaped like `info` in `type_get`.
    #[serde(default)]
    pub info: Option<Value>,
    /// Component parameters renamed by a new `def`, as `[old, new]` pairs in
    /// order, so the component's other types keep their values.
    #[serde(default)]
    pub renames: Vec<[String; 2]>,
}

impl Command for TypeSet {
    const NAME: &'static str = "type_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::element_type(p, &self.type_ref)?;
        let ty = p.element_type(id).ok_or_else(|| EngineError::not_found("type"))?.clone();
        if let Some(name) = &self.name
            && (name.trim().is_empty() || p.types_of(ty.category()).any(|t| t.id != id && t.name.trim() == name.trim()))
        {
            return Err(EngineError::invalid(format!("the name '{name}' is empty or already used")));
        }
        let mut data = self.params.as_ref().map(|patch| patch_variant(&ty.data, patch)).transpose()?;
        // A new roof covering brings the framing it usually lies on, if none was chosen.
        if let (
            Some(TypeData::Roof { covering: Some(kind), framing, .. }),
            TypeData::Roof { covering: old, framing: was, .. },
        ) = (&mut data, &ty.data)
            && Some(*kind) != *old
            && framing.system == FramingSystem::None
            && was.system == FramingSystem::None
        {
            framing.system = kind.spec().framing;
        }
        if data.as_ref().is_some_and(|d| std::mem::discriminant(d) != std::mem::discriminant(&ty.data)) {
            return Err(EngineError::invalid("params cannot change the type's category"));
        }
        let info = self.info.as_ref().map(|patch| patch_value(&ty.info, patch)).transpose()?;
        // A new component definition goes to every type of the component.
        let mut siblings = Vec::new();
        if let (
            Some(TypeData::Component { def: new_def, component: new_name }),
            TypeData::Component { component, def: old_def },
        ) = (&data, &ty.data)
            && (new_def != old_def || new_name != component)
        {
            if new_name.trim().is_empty() {
                return Err(EngineError::invalid("the component needs a name"));
            }
            if new_def != old_def {
                validate(new_def)?;
            }
            siblings = p
                .types
                .values()
                .filter(|t| t.id != id && matches!(&t.data, TypeData::Component { component: c, .. } if c == component))
                .map(|t| t.id)
                .collect();
        }
        edit(s, t!("tx-modify-type"), |e| {
            let new_def = match &data {
                Some(TypeData::Component { def, .. }) => Some(def.clone()),
                _ => None,
            };
            let new_component = match &data {
                Some(TypeData::Component { component, .. }) => Some(component.clone()),
                _ => None,
            };
            for sid in &siblings {
                let Some(TypeData::Component { component, def }) = e.type_mut(*sid).map(|t| &mut t.data) else {
                    continue;
                };
                let Some(mut d) = new_def.clone() else { continue };
                for param in &mut d.params {
                    // The name this parameter had before the renames.
                    let old = self
                        .renames
                        .iter()
                        .rev()
                        .fold(param.name.clone(), |n, [from, to]| if n == *to { from.clone() } else { n });
                    if let Some(v) = def.param(&old).map(|q| q.value) {
                        param.value = v;
                    }
                }
                *def = d;
                if let Some(c) = &new_component {
                    *component = c.clone();
                }
            }
            let t = e.type_mut(id).ok_or_else(|| EngineError::not_found("type"))?;
            if let Some(n) = self.name {
                t.name = n.trim().to_string();
            }
            if let Some(d) = data {
                t.data = d;
            }
            if let Some(i) = info {
                t.info = i;
            }
            Ok(())
        })?;
        Ok(type_out(s, id))
    }
}

/// Copy a type under a new name (default "<name> (copy)"); the given elements of the type switch to the copy.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TypeDuplicate {
    #[serde(rename = "type")]
    pub type_ref: Ref,
    /// Name of the copy (default: "<name> (copy)").
    #[serde(default)]
    pub name: Option<String>,
    /// Elements of the type to switch to the copy.
    #[serde(default)]
    pub elements: Vec<u64>,
}

impl Command for TypeDuplicate {
    const NAME: &'static str = "type_duplicate";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let tid = refs::element_type(p, &self.type_ref)?;
        let ty = p.element_type(tid).ok_or_else(|| EngineError::not_found("type"))?.clone();
        let elements = self.elements.iter().map(|e| refs::element(p, *e)).collect::<Result<Vec<_>>>()?;
        let name = self.name.unwrap_or_else(|| t!("copy-of", "name" => ty.name));
        let new_id = edit(s, t!("tx-duplicate-type"), |e| {
            let id = ElementTypeId(e.alloc_id());
            e.add_type(ElementType { id, name, data: ty.data.clone(), info: ty.info.clone(), uid: Default::default() });
            for el in elements {
                if let Some(x) = e.element_mut(el).filter(|x| x.type_id == tid) {
                    x.type_id = id;
                }
            }
            Ok(id)
        })?;
        Ok(type_out(s, new_id))
    }
}

/// Delete a type that no element uses (the last type of a category is kept).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TypeDelete {
    #[serde(rename = "type")]
    pub type_ref: Ref,
}

impl Command for TypeDelete {
    const NAME: &'static str = "type_delete";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::element_type(p, &self.type_ref)?;
        let category = p.element_type(id).map(|t| t.category()).ok_or_else(|| EngineError::not_found("type"))?;
        if p.elements.values().any(|e| e.type_id == id) {
            return Err(EngineError::rejected("elements use this type"));
        }
        if p.types_of(category).count() <= 1 {
            return Err(EngineError::rejected("the last type of a category cannot be deleted"));
        }
        let legends_use =
            p.views.values().any(|v| matches!(&v.kind, cc_core::ViewKind::Legend { types } if types.contains(&id)));
        edit(s, t!("tx-delete-type"), |e| {
            if legends_use {
                let views: Vec<_> = e.project().views.keys().copied().collect();
                for v in views {
                    if let Some(cc_core::ViewKind::Legend { types }) = e.view_mut(v).map(|v| &mut v.kind) {
                        types.retain(|t| *t != id);
                    }
                }
            }
            e.remove_type(id);
            Ok(())
        })?;
        Ok(json!({ "deleted": id.0 }))
    }
}

/// Create a component type: `def` holds its parameters and pieces, shaped
/// like a component type's `params.def` in `type_get`. A piece's `shape` is
/// a `Box` {x, y, z, width, depth, height}, a `Cylinder` {x, y, z, radius,
/// height}, an `Extrusion` {points: [[x, y], …], z, height} (a plan outline
/// extruded up), a `Profile` {plane: `Plan`, `Front` or `Side`, points:
/// [{u, v, radius}, …], offset, depth} (lines and arcs drawn on a plane:
/// u, v are x, y in plan, x, z from the front and y, z from the side; the
/// plane stands at `offset` on the remaining axis and extrudes `depth` along
/// it; a point's optional `radius` makes the segment to the next point an
/// arc, positive bulging outwards) or a `Revolution` {plane, points, offset, axis, angle}
/// (the profile turned about the line u = axis of its plane). Every field
/// is an expression over the parameters (meters, degrees). A piece with
/// `void: true` is cut out of the solid pieces it overlaps; one with
/// `plan_only: true` is a plan symbol (a door's swing), drawn in plans and
/// not a solid. A `kind` of
/// `Door` or `Window` with an `opening` {width, height, sill, own_symbol}
/// (expressions) makes a door or window type, placed with `door_create` or
/// `window_create`: it cuts that opening in its wall; its frame has x along
/// the wall from the opening's center, y across it (towards the side a door
/// opens to), z up from the sill, and `WallThickness` is the wall's thickness.
/// `connectors` [{name, system: `ColdWater`, `HotWater`, `Sanitary`, `Vent`,
/// `Rainwater`, `Power` or `Lighting`, x, y, z, direction: `Down`, `Up`,
/// `Front` (-y), `Back` (+y), `Left` (-x) or `Right` (+x), diameter}] are
/// where `mep_connect` joins pipes and conduits (a toilet's inlet and outlet).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentCreate {
    /// Name of the component and of its first type.
    pub name: String,
    /// The definition: `params` (name, value, kind, instance) and `items` (pieces), shaped like a component type's `params.def` in `type_get`.
    pub def: Value,
}

impl Command for ComponentCreate {
    const NAME: &'static str = "component_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let name = self.name.trim().to_string();
        if name.is_empty() {
            return Err(EngineError::invalid("the component needs a name"));
        }
        let def: ComponentDef =
            serde_json::from_value(self.def).map_err(|e| EngineError::invalid(format!("invalid def: {e}")))?;
        validate(&def)?;
        let id = edit(s, t!("tx-new-component"), |e| {
            let id = ElementTypeId(e.alloc_id());
            let data = TypeData::Component { component: name.clone(), def };
            e.add_type(ElementType { id, name: name.clone(), data, info: Default::default(), uid: Default::default() });
            Ok(id)
        })?;
        Ok(type_out(s, id))
    }
}
