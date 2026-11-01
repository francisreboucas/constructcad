//! The commands, by area. Each is a parameter struct implementing
//! [`crate::Command`]; [`crate::catalog`] lists them all.

pub mod analytical;
pub mod annotate;
pub mod cad;
pub mod component;
pub mod component_piece;
pub mod create;
pub mod edit;
pub mod foundation;
pub mod framing;
pub mod graphics;
pub mod io;
pub mod levels;
pub mod mass;
pub mod materials;
pub mod mep;
pub mod modify;
pub mod phases;
pub mod project;
pub mod query;
pub mod ramp;
pub mod rebar;
pub mod roofing;
pub mod sketch;
pub mod stair;
pub mod structure;
pub mod sweep;
pub mod types;
pub mod views;

use crate::error::{EngineError, Result};
use crate::session::Session;
use cc_core::{Category, EditCtx, Element, ElementId, ElementKind, ElementTypeId, LevelId, Project};
use serde_json::{Value, json};

/// Run an edit as one undo step, kept only if it succeeds and leaves a
/// project that needs no repair (no dangling references, no invalid numbers).
pub(crate) fn edit<R>(s: &mut Session, tx: String, f: impl FnOnce(&mut EditCtx) -> Result<R>) -> Result<R> {
    let key = s.take_merge_key();
    s.doc.try_edit_merged(tx, key, f, check)
}

/// The project is consistent: repairing a copy would change nothing.
pub(crate) fn check(p: &Project) -> Result<()> {
    match p.clone().repair() {
        0 => Ok(()),
        n => Err(EngineError::rejected(format!("the edit would leave the model invalid ({n} problems)"))),
    }
}

/// Add an element (doors and windows get the next free mark).
pub(crate) fn add_element(e: &mut EditCtx, level: LevelId, type_id: ElementTypeId, kind: ElementKind) -> ElementId {
    let id = ElementId(e.alloc_id());
    let mut element = Element {
        id,
        level,
        type_id,
        mark: String::new(),
        comments: String::new(),
        kind,
        uid: Default::default(),
        phase_created: Default::default(),
        phase_demolished: None,
    };
    if matches!(element.category(), Category::Door | Category::Window) {
        element.mark = e.project().next_mark(element.category());
    }
    e.add_element(element)
}

/// The fields of an enum value's single variant: `{"Wall": {...}}` gives
/// `("Wall", {...})`; unit variants give an empty object.
pub(crate) fn variant(v: Value) -> (String, Value) {
    match v {
        Value::Object(o) if o.len() == 1 => match o.into_iter().next() {
            Some((k, inner)) => (k, inner),
            None => (String::new(), json!({})),
        },
        Value::String(name) => (name, json!({})),
        other => (String::new(), other),
    }
}

/// Whether `v` is an enum value in serde's form: one key, a variant name.
fn variant_name(v: &serde_json::Map<String, Value>) -> Option<&str> {
    let mut keys = v.keys();
    match (keys.next(), keys.next()) {
        (Some(k), None) if k.starts_with(|c: char| c.is_ascii_uppercase()) => Some(k),
        _ => None,
    }
}

/// Merge patch: objects merge key by key, anything else (arrays, numbers,
/// `null` for an optional value) replaces. Unlike RFC 7396, `null` sets a
/// field to null rather than removing it (model fields are never absent).
/// An enum value given as another variant (`{"Round": ..}` over
/// `{"Rectangular": ..}`) replaces the old one.
pub(crate) fn merge(target: &mut Value, patch: &Value) {
    if let (Value::Object(t), Value::Object(p)) = (&*target, patch)
        && let (Some(a), Some(b)) = (variant_name(t), variant_name(p))
        && a != b
    {
        *target = patch.clone();
        return;
    }
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                merge(t.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (t, p) => *t = p.clone(),
    }
}

/// Apply a merge patch to the fields of an enum value (keeping its variant).
pub(crate) fn patch_variant<T: serde::Serialize + serde::de::DeserializeOwned>(value: &T, patch: &Value) -> Result<T> {
    let (name, mut fields) = variant(serde_json::to_value(value).map_err(|e| EngineError::invalid(e.to_string()))?);
    merge(&mut fields, patch);
    let rebuilt = if fields.as_object().is_some_and(|o| o.is_empty()) && !patch.is_object() {
        Value::String(name)
    } else {
        json!({ name: fields })
    };
    serde_json::from_value(rebuilt).map_err(|e| EngineError::invalid(format!("invalid params: {e}")))
}

/// Apply a merge patch to a whole serializable value.
pub(crate) fn patch_value<T: serde::Serialize + serde::de::DeserializeOwned>(value: &T, patch: &Value) -> Result<T> {
    let mut v = serde_json::to_value(value).map_err(|e| EngineError::invalid(e.to_string()))?;
    merge(&mut v, patch);
    serde_json::from_value(v).map_err(|e| EngineError::invalid(format!("invalid values: {e}")))
}

/// What a created or changed element looks like now.
pub(crate) fn created(s: &mut Session, id: ElementId) -> Value {
    query::element_json(s, id).unwrap_or_else(|| json!({ "id": id.0 }))
}

pub(crate) fn ids_json(ids: &[ElementId]) -> Value {
    json!(ids.iter().map(|i| i.0).collect::<Vec<_>>())
}
