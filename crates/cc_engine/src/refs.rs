//! Naming model objects in commands: levels, types, views, materials and
//! systems by numeric id or by name; elements by id.

use crate::error::{EngineError, Result};
use cc_core::{Category, ElementId, ElementTypeId, LevelId, Project, ViewId};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A level, type, view or material: its numeric id or its name.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum Ref {
    Id(u64),
    Name(String),
}

impl Default for Ref {
    /// Id 0, which names nothing: a placeholder for required fields.
    fn default() -> Self {
        Ref::Id(0)
    }
}

/// A point in plan, `[x, y]` in meters.
pub type Point = [f64; 2];

pub fn point(p: Point) -> Result<DVec2> {
    let v = DVec2::from_array(p);
    v.is_finite().then_some(v).ok_or_else(|| EngineError::invalid("points must be finite numbers"))
}

pub fn points(ps: &[Point]) -> Result<Vec<DVec2>> {
    ps.iter().map(|p| point(*p)).collect()
}

/// A finite number.
pub fn finite(v: f64, what: &str) -> Result<f64> {
    v.is_finite().then_some(v).ok_or_else(|| EngineError::invalid(format!("{what} must be a finite number")))
}

/// A positive finite number.
pub fn positive(v: f64, what: &str) -> Result<f64> {
    (v.is_finite() && v > 0.0).then_some(v).ok_or_else(|| EngineError::invalid(format!("{what} must be positive")))
}

fn matches(name: &str, wanted: &str) -> bool {
    name.trim().eq_ignore_ascii_case(wanted.trim())
}

/// One match for a name, or an error naming the ambiguity.
fn unique<T: Copy>(what: &str, wanted: &str, found: Vec<T>) -> Result<T> {
    match found.as_slice() {
        [one] => Ok(*one),
        [] => Err(EngineError::not_found(format!("no {what} named '{wanted}'"))),
        _ => Err(EngineError::invalid(format!("several {what}s are named '{wanted}'; use the id"))),
    }
}

pub fn level(p: &Project, r: &Ref) -> Result<LevelId> {
    match r {
        Ref::Id(id) => {
            p.level(LevelId(*id)).map(|l| l.id).ok_or_else(|| EngineError::not_found(format!("no level with id {id}")))
        }
        Ref::Name(n) => unique("level", n, p.levels.iter().filter(|l| matches(&l.name, n)).map(|l| l.id).collect()),
    }
}

/// The given level, else the lowest one.
pub fn level_or_first(p: &Project, r: Option<&Ref>) -> Result<LevelId> {
    match r {
        Some(r) => level(p, r),
        None => p.levels.first().map(|l| l.id).ok_or_else(|| EngineError::rejected("the project has no levels")),
    }
}

pub fn element_type(p: &Project, r: &Ref) -> Result<ElementTypeId> {
    match r {
        Ref::Id(id) => p
            .element_type(ElementTypeId(*id))
            .map(|t| t.id)
            .ok_or_else(|| EngineError::not_found(format!("no type with id {id}"))),
        Ref::Name(n) => unique("type", n, p.types.values().filter(|t| matches(&t.name, n)).map(|t| t.id).collect()),
    }
}

/// A type of `category`: the given one (checked), else the category's default.
pub fn type_of(p: &Project, category: Category, r: Option<&Ref>) -> Result<ElementTypeId> {
    let id = match r {
        Some(Ref::Name(n)) => {
            unique("type", n, p.types_of(category).filter(|t| matches(&t.name, n)).map(|t| t.id).collect())?
        }
        Some(r) => element_type(p, r)?,
        None => p
            .default_type(category)
            .ok_or_else(|| EngineError::rejected(format!("the project has no {category:?} type")))?,
    };
    match p.element_type(id) {
        Some(t) if t.category() == category => Ok(id),
        _ => Err(EngineError::invalid(format!("type {} is not a {category:?} type", id.0))),
    }
}

pub fn view(p: &Project, r: &Ref) -> Result<ViewId> {
    match r {
        Ref::Id(id) => {
            p.view(ViewId(*id)).map(|v| v.id).ok_or_else(|| EngineError::not_found(format!("no view with id {id}")))
        }
        Ref::Name(n) => {
            let named: Vec<ViewId> = p.views.values().filter(|v| matches(&v.name, n)).map(|v| v.id).collect();
            if !named.is_empty() {
                return unique("view", n, named);
            }
            // Sheets also by their number.
            let numbered = p
                .views
                .values()
                .filter(|v| matches!(&v.kind, cc_core::ViewKind::Sheet { number, .. } if matches(number, n)))
                .map(|v| v.id)
                .collect();
            unique("view", n, numbered)
        }
    }
}

/// A material's name (materials are keyed by name; an index also works).
pub fn material(p: &Project, r: &Ref) -> Result<String> {
    match r {
        Ref::Id(i) => usize::try_from(*i)
            .ok()
            .and_then(|i| p.materials.get(i))
            .map(|m| m.name.clone())
            .ok_or_else(|| EngineError::not_found(format!("no material at index {i}"))),
        Ref::Name(n) => p
            .materials
            .iter()
            .find(|m| m.name == *n)
            .or_else(|| p.materials.iter().find(|m| matches(&m.name, n)))
            .map(|m| m.name.clone())
            .ok_or_else(|| EngineError::not_found(format!("no material named '{n}'"))),
    }
}

/// A plumbing system or electrical circuit by id, name, code or, when it is
/// the only one of its kind, kind (`cold_water`, `sanitary`…).
pub fn system(p: &Project, r: &Ref) -> Result<cc_core::SystemId> {
    match r {
        Ref::Id(id) => {
            let id = cc_core::SystemId(*id);
            p.system(id).map(|_| id).ok_or_else(|| EngineError::not_found(format!("no system with id {}", id.0)))
        }
        Ref::Name(n) => {
            let named: Vec<_> = p.systems.iter().filter(|(_, s)| matches(&s.name, n)).map(|(id, _)| *id).collect();
            if !named.is_empty() {
                return unique("system", n, named);
            }
            let coded: Vec<_> = p.systems.iter().filter(|(_, s)| matches(&s.code, n)).map(|(id, _)| *id).collect();
            if !coded.is_empty() {
                return unique("system", n, coded);
            }
            unique(
                "system",
                n,
                p.systems.iter().filter(|(_, s)| matches(s.kind.name(), n)).map(|(id, _)| *id).collect(),
            )
        }
    }
}

pub fn element(p: &Project, id: u64) -> Result<ElementId> {
    p.element(ElementId(id)).map(|e| e.id).ok_or_else(|| EngineError::not_found(format!("no element with id {id}")))
}

pub fn elements(p: &Project, ids: &[u64]) -> Result<Vec<ElementId>> {
    if ids.is_empty() {
        return Err(EngineError::invalid("no elements given"));
    }
    ids.iter().map(|id| element(p, *id)).collect()
}

/// A category by its English name (`wall`, `door`…), case-insensitive.
pub fn category(name: &str) -> Result<Category> {
    Category::ALL
        .into_iter()
        .find(|c| format!("{c:?}").eq_ignore_ascii_case(name.trim()))
        .ok_or_else(|| EngineError::invalid(format!("unknown category '{name}'")))
}

pub fn category_name(c: Category) -> String {
    format!("{c:?}").to_lowercase()
}

/// A view filter by id or name.
pub fn filter(p: &Project, r: &Ref) -> Result<cc_core::FilterId> {
    match r {
        Ref::Id(id) => {
            let id = cc_core::FilterId(*id);
            p.filter(id).map(|_| id).ok_or_else(|| EngineError::not_found(format!("no filter with id {}", id.0)))
        }
        Ref::Name(n) => unique("filter", n, p.filters.values().filter(|f| matches(&f.name, n)).map(|f| f.id).collect()),
    }
}

/// A view template by id or name.
pub fn template(p: &Project, r: &Ref) -> Result<cc_core::TemplateId> {
    match r {
        Ref::Id(id) => {
            let id = cc_core::TemplateId(*id);
            p.template(id)
                .map(|_| id)
                .ok_or_else(|| EngineError::not_found(format!("no view template with id {}", id.0)))
        }
        Ref::Name(n) => {
            unique("view template", n, p.templates.values().filter(|t| matches(&t.name, n)).map(|t| t.id).collect())
        }
    }
}

/// A construction phase by id or name.
pub fn phase(p: &Project, r: &Ref) -> Result<cc_core::PhaseId> {
    match r {
        Ref::Id(id) => {
            let id = cc_core::PhaseId(*id);
            p.phase(id).map(|_| id).ok_or_else(|| EngineError::not_found(format!("no phase with id {}", id.0)))
        }
        Ref::Name(n) => unique("phase", n, p.phases.iter().filter(|x| matches(&x.name, n)).map(|x| x.id).collect()),
    }
}

/// A phase filter by id or name.
pub fn phase_filter(p: &Project, r: &Ref) -> Result<cc_core::PhaseFilterId> {
    match r {
        Ref::Id(id) => {
            let id = cc_core::PhaseFilterId(*id);
            p.phase_filter(id)
                .map(|_| id)
                .ok_or_else(|| EngineError::not_found(format!("no phase filter with id {}", id.0)))
        }
        Ref::Name(n) => {
            unique("phase filter", n, p.phase_filters.values().filter(|f| matches(&f.name, n)).map(|f| f.id).collect())
        }
    }
}

/// Deserialize an optional field that may also be `null`: absent gives
/// `None`, `null` gives `Some(None)` (use with `#[serde(default)]`).
pub fn nullable<'de, D, T>(d: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// A detail level by name: `coarse`, `medium` or `fine`.
pub fn detail_level(name: &str) -> Result<cc_core::DetailLevel> {
    match name.trim().to_ascii_lowercase().as_str() {
        "coarse" => Ok(cc_core::DetailLevel::Coarse),
        "medium" => Ok(cc_core::DetailLevel::Medium),
        "fine" => Ok(cc_core::DetailLevel::Fine),
        other => Err(EngineError::invalid(format!("unknown detail level '{other}': use coarse, medium or fine"))),
    }
}
