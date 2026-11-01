//! Changing element parameters (the Properties panel).

use super::{created, edit, ids_json, patch_variant};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Change an element: its level (doors and windows of a wall follow it),
/// type (of the same category), mark, comments, and any of its `params`
/// as `element_get` shows them (merged: only the fields given change, e.g.
/// `{"height": 2.8}` for a wall or `{"shape": {"Gable": {"slope": 0.5,
/// "ridge_deg": 0}}}` for a roof). The edit is refused if it would leave the
/// model invalid.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementSet {
    /// Element id.
    pub id: u64,
    /// New level id or name.
    #[serde(default)]
    pub level: Option<Ref>,
    /// New type id or name (same category).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// New mark.
    #[serde(default)]
    pub mark: Option<String>,
    /// New comments.
    #[serde(default)]
    pub comments: Option<String>,
    /// Fields of the element's parameters to change.
    #[serde(default)]
    pub params: Option<Value>,
}

impl Command for ElementSet {
    const NAME: &'static str = "element_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::element(p, self.id)?;
        let element = p.element(id).ok_or_else(|| EngineError::not_found("element"))?.clone();
        let level = self.level.as_ref().map(|l| refs::level(p, l)).transpose()?;
        let type_id = self.type_ref.as_ref().map(|t| refs::type_of(p, element.category(), Some(t))).transpose()?;
        let kind = self.params.as_ref().map(|patch| patched(p, &element, patch, type_id)).transpose()?;
        let hosted: Vec<_> = p.hosted_by(id).map(|h| h.id).collect();
        edit(s, t!("tx-modify"), |e| {
            if let Some(level) = level {
                for h in hosted.iter().copied().chain([id]) {
                    if let Some(x) = e.element_mut(h) {
                        x.level = level;
                    }
                }
            }
            let x = e.element_mut(id).ok_or_else(|| EngineError::not_found("element"))?;
            if let Some(t) = type_id {
                x.type_id = t;
            }
            if let Some(m) = self.mark {
                x.mark = m;
            }
            if let Some(c) = self.comments {
                x.comments = c;
            }
            if let Some(k) = kind {
                x.kind = k;
            }
            Ok(())
        })?;
        Ok(created(s, id))
    }
}

/// `element`'s kind with `patch` merged into its parameters, checked (with
/// its type, or `type_id` when it changes too).
fn patched(
    p: &cc_core::Project,
    element: &cc_core::Element,
    patch: &Value,
    type_id: Option<cc_core::ElementTypeId>,
) -> Result<cc_core::ElementKind> {
    let kind = match &element.kind {
        // The drawing read from the file is not a parameter.
        cc_core::ElementKind::CadImport(c) => super::cad::patch(c, patch)?,
        kind => patch_variant(kind, patch)?,
    };
    if std::mem::discriminant(&kind) != std::mem::discriminant(&element.kind) {
        return Err(EngineError::invalid("params cannot change the element's category"));
    }
    match &kind {
        cc_core::ElementKind::Wall(w) => {
            if w.bulge.abs() > cc_core::arc::MAX_BULGE || !w.bulge.is_finite() {
                return Err(EngineError::invalid("the bulge must be between -10 and 10"));
            }
            super::create::check_arc(p, type_id.unwrap_or(element.type_id), w)?;
        }
        cc_core::ElementKind::Roof(r) => super::create::check_roof_holes(r)?,
        cc_core::ElementKind::Floor(f) => super::create::check_openings(&f.boundary, &f.openings)?,
        _ => {}
    }
    Ok(kind)
}

/// Change several elements at once, in one undo step: a new level (doors
/// and windows of a wall follow it) and the same `params` merged into each
/// (as `element_set` takes them, e.g. `{"base_offset": -1.5}` for columns).
/// Every element must take the change, or none changes.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementsSet {
    /// Element ids.
    pub ids: Vec<u64>,
    /// New level id or name.
    #[serde(default)]
    pub level: Option<Ref>,
    /// Fields of the elements' parameters to change.
    #[serde(default)]
    pub params: Option<Value>,
}

impl Command for ElementsSet {
    const NAME: &'static str = "elements_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let ids = refs::elements(p, &self.ids)?;
        if ids.is_empty() {
            return Err(EngineError::invalid("no elements given"));
        }
        let level = self.level.as_ref().map(|l| refs::level(p, l)).transpose()?;
        let mut kinds = Vec::new();
        if let Some(patch) = &self.params {
            for id in &ids {
                let element = p.element(*id).ok_or_else(|| EngineError::not_found("element"))?;
                let kind = patched(p, element, patch, None)
                    .map_err(|e| EngineError { message: format!("element {}: {}", id.0, e.message), ..e })?;
                kinds.push((*id, kind));
            }
        }
        let hosted: Vec<cc_core::ElementId> = if level.is_some() {
            ids.iter().flat_map(|id| p.hosted_by(*id).map(|h| h.id)).collect()
        } else {
            Vec::new()
        };
        edit(s, t!("tx-modify"), |e| {
            if let Some(level) = level {
                for id in hosted.iter().chain(&ids) {
                    if let Some(x) = e.element_mut(*id) {
                        x.level = level;
                    }
                }
            }
            for (id, kind) in kinds {
                if let Some(x) = e.element_mut(id) {
                    x.kind = kind;
                }
            }
            Ok(())
        })?;
        Ok(json!({ "changed": ids_json(&ids) }))
    }
}

/// Give elements a type (all must be of the type's category), like Match Type.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementsSetType {
    /// Element ids.
    pub ids: Vec<u64>,
    /// Type id or name.
    #[serde(rename = "type")]
    pub type_ref: Ref,
}

impl Command for ElementsSetType {
    const NAME: &'static str = "elements_set_type";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let ids = refs::elements(p, &self.ids)?;
        let type_id = refs::element_type(p, &self.type_ref)?;
        let category = p.element_type(type_id).map(|t| t.category());
        if ids.iter().any(|id| p.element(*id).map(|e| e.category()) != category) {
            return Err(EngineError::invalid("every element must be of the type's category"));
        }
        edit(s, t!("tx-change-type"), |e| {
            for id in &ids {
                if let Some(x) = e.element_mut(*id) {
                    x.type_id = type_id;
                }
            }
            Ok(())
        })?;
        Ok(json!({ "changed": ids_json(&ids) }))
    }
}
