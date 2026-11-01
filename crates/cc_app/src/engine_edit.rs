//! Property edits through the engine. Panels and dialogs describe a change
//! as a function on a copy of the object; these helpers send the result to
//! the engine's `*_set` command, merged into one undo step per `key` while a
//! value is dragged. Refusals (invalid values) go to the status bar.

use crate::state::AppState;
use cc_core::{Element, ElementId, ElementType, ElementTypeId, Level, LevelId, Material, View, ViewId};
use cc_engine::Ref;
use cc_engine::commands::{edit, levels, materials, project, types, views};

fn run<C: cc_engine::Command + serde::Serialize>(state: &mut AppState, key: Option<u64>, command: C) -> bool {
    let result = match key {
        Some(k) => state.session.run_merged(k, command),
        None => state.session.run(command),
    };
    match result {
        Ok(_) => true,
        Err(e) => {
            state.message = e.message;
            false
        }
    }
}

fn json<T: serde::Serialize>(v: &T) -> serde_json::Value {
    serde_json::to_value(v).unwrap_or_default()
}

/// Change element `id` as `f` changes a copy of it (`element_set`).
pub fn element(state: &mut AppState, id: ElementId, key: Option<u64>, f: impl FnOnce(&mut Element)) -> bool {
    let Some(old) = state.project().element(id).cloned() else { return false };
    let mut new = old.clone();
    f(&mut new);
    if new == old {
        return false;
    }
    let command = edit::ElementSet {
        id: id.0,
        level: (new.level != old.level).then_some(Ref::Id(new.level.0)),
        type_ref: (new.type_id != old.type_id).then_some(Ref::Id(new.type_id.0)),
        mark: (new.mark != old.mark).then_some(new.mark),
        comments: (new.comments != old.comments).then_some(new.comments),
        params: (new.kind != old.kind).then(|| cc_engine::element_params(&new.kind)),
    };
    run(state, key, command)
}

/// Change type `id` as `f` changes a copy of it (`type_set`).
pub fn element_type(
    state: &mut AppState,
    id: ElementTypeId,
    key: Option<u64>,
    f: impl FnOnce(&mut ElementType),
) -> bool {
    let Some(old) = state.project().element_type(id).cloned() else { return false };
    let mut new = old.clone();
    f(&mut new);
    if new == old {
        return false;
    }
    let command = types::TypeSet {
        type_ref: Ref::Id(id.0),
        name: (new.name != old.name).then_some(new.name),
        params: (new.data != old.data).then(|| cc_engine::params_of(&new.data)),
        info: (new.info != old.info).then(|| json(&new.info)),
        renames: Vec::new(),
    };
    run(state, key, command)
}

/// Change view `id` as `f` changes a copy of it (`view_set`).
pub fn view(state: &mut AppState, id: ViewId, key: Option<u64>, f: impl FnOnce(&mut View)) -> bool {
    let Some(old) = state.project().view(id).cloned() else { return false };
    let mut new = old.clone();
    f(&mut new);
    if new == old {
        return false;
    }
    let command = views::ViewSet {
        view: Ref::Id(id.0),
        name: (new.name != old.name).then_some(new.name),
        scale: (new.scale != old.scale).then_some(new.scale),
        params: (new.kind != old.kind).then(|| cc_engine::params_of(&new.kind)),
        phase: (new.phase != old.phase).then_some(Ref::Id(new.phase.0)),
        detail: (new.detail != old.detail).then(|| new.detail.name().to_string()),
        phase_filter: (new.phase_filter != old.phase_filter).then(|| new.phase_filter.map(|f| Ref::Id(f.0))),
    };
    run(state, key, command)
}

/// Change material `name` as `f` changes a copy of it (`material_set`; renaming has its own command).
pub fn material(state: &mut AppState, name: &str, key: Option<u64>, f: impl FnOnce(&mut Material)) -> bool {
    let Some(old) = state.project().material(name).cloned() else { return false };
    let mut new = old.clone();
    f(&mut new);
    if new == old {
        return false;
    }
    let command = materials::MaterialSet { material: Ref::Name(name.to_string()), values: json(&new) };
    run(state, key, command)
}

/// Change the project units as `f` changes a copy of them (`units_set`).
pub fn units(state: &mut AppState, key: Option<u64>, f: impl FnOnce(&mut cc_core::Units)) -> bool {
    let old = state.project().units.clone();
    let mut new = old.clone();
    f(&mut new);
    new != old && run(state, key, project::UnitsSet { values: json(&new), preset: None })
}

/// Change the project information as `f` changes a copy of it (`project_info_set`).
pub fn info(state: &mut AppState, key: Option<u64>, f: impl FnOnce(&mut cc_core::ProjectInfo)) -> bool {
    let old = state.project().info.clone();
    let mut new = old.clone();
    f(&mut new);
    if new == old {
        return false;
    }
    let changed = |a: &String, b: &String| (a != b).then(|| b.clone());
    let command = project::ProjectInfoSet {
        name: changed(&old.name, &new.name),
        number: changed(&old.number, &new.number),
        client: changed(&old.client, &new.client),
        address: changed(&old.address, &new.address),
        author: changed(&old.author, &new.author),
    };
    run(state, key, command)
}

/// Change plumbing system or circuit `id` as `f` changes a copy (`system_set`).
pub fn system(
    state: &mut AppState,
    id: cc_core::SystemId,
    key: Option<u64>,
    f: impl FnOnce(&mut cc_core::MepSystem),
) -> bool {
    let Some(old) = state.project().system(id).cloned() else { return false };
    let mut new = old.clone();
    f(&mut new);
    if new == old {
        return false;
    }
    let mut params = json(&new);
    // Unset optional values are left out of the model's form; the patch clears them.
    if let Some(o) = params.as_object_mut() {
        for field in ["panel", "color"] {
            o.entry(field).or_insert(serde_json::Value::Null);
        }
    }
    let command = cc_engine::commands::mep::SystemSet { system: Ref::Id(id.0), params };
    run(state, key, command)
}

/// Rename a level or change its elevation as `f` changes a copy (`level_set`).
pub fn level(state: &mut AppState, id: LevelId, key: Option<u64>, f: impl FnOnce(&mut Level)) -> bool {
    let Some(old) = state.project().level(id).cloned() else { return false };
    let mut new = old.clone();
    f(&mut new);
    if new == old {
        return false;
    }
    let command = levels::LevelSet {
        level: Ref::Id(id.0),
        name: (new.name != old.name).then_some(new.name),
        elevation: (new.elevation != old.elevation).then_some(new.elevation),
    };
    run(state, key, command)
}
