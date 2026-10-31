//! Document-level commands shared by the ribbon, browser and shortcuts.

use crate::state::{AppState, PendingAction};
use crate::t;
use crate::tools::Tool;
use cc_core::{Category, Document, ElementId, LevelId, Project, ViewCategory, ViewId, ViewKind};
use cc_engine::Ref;
use cc_engine::commands::{io, levels, project, views};

/// Run an engine command; failures go to the status bar.
pub fn run_command<C: cc_engine::Command + serde::Serialize>(
    state: &mut AppState,
    command: C,
) -> Option<serde_json::Value> {
    run(state, command)
}

fn run<C: cc_engine::Command + serde::Serialize>(state: &mut AppState, command: C) -> Option<serde_json::Value> {
    match state.session.run(command) {
        Ok(out) => Some(out),
        Err(e) => {
            state.message = e.message;
            None
        }
    }
}

fn view_id(out: &serde_json::Value) -> Option<ViewId> {
    out["id"].as_u64().map(ViewId)
}

fn new_view(state: &mut AppState, kind: &str, level: Option<LevelId>, category: Option<Category>) -> Option<ViewId> {
    let command = views::ViewCreate {
        kind: kind.to_string(),
        name: None,
        level: level.map(|l| Ref::Id(l.0)),
        category: category.map(cc_engine::category_name),
        start: None,
        end: None,
        depth: None,
        direction: None,
        scale: None,
        panel: None,
    };
    let id = run(state, command).as_ref().and_then(view_id)?;
    state.open_view(id);
    Some(id)
}

pub fn new_plan(state: &mut AppState, level: LevelId, ceiling: bool) -> Option<ViewId> {
    new_view(state, if ceiling { "ceiling_plan" } else { "floor_plan" }, Some(level), None)
}

pub fn new_3d(state: &mut AppState) -> Option<ViewId> {
    new_view(state, "three_d", None, None)
}

pub fn new_legend(state: &mut AppState) -> Option<ViewId> {
    new_view(state, "legend", None, None)
}

pub fn new_schedule(state: &mut AppState, category: Category) -> Option<ViewId> {
    new_view(state, "schedule", None, Some(category))
}

pub fn new_sheet(state: &mut AppState) -> Option<ViewId> {
    new_view(state, "sheet", None, None)
}

pub fn t_category(category: Category) -> String {
    crate::i18n::tr(category.key())
}

pub fn duplicate_view(state: &mut AppState, id: ViewId) {
    if let Some(new) = run(state, views::ViewDuplicate { view: Ref::Id(id.0) }).as_ref().and_then(view_id) {
        state.open_view(new);
    }
}

pub fn delete_view(state: &mut AppState, id: ViewId) {
    run(state, views::ViewDelete { view: Ref::Id(id.0) });
    if state.active_view == Some(id) {
        state.active_view = None;
    }
}

pub fn rename_view(state: &mut AppState, id: ViewId, name: String) {
    if name.trim().is_empty() {
        return;
    }
    run(
        state,
        views::ViewSet {
            view: Ref::Id(id.0),
            name: Some(name),
            scale: None,
            params: None,
            phase: None,
            phase_filter: None,
        },
    );
}

/// Sheet that already shows `view`, if any.
pub fn sheet_of(project: &Project, view: ViewId) -> Option<ViewId> {
    views::sheet_of(project, view)
}

pub fn place_on_sheet(state: &mut AppState, sheet: ViewId, view: ViewId) {
    run(state, views::SheetPlaceView { sheet: Ref::Id(sheet.0), view: Ref::Id(view.0), center: None });
}

pub fn remove_viewport(state: &mut AppState, sheet: ViewId, index: usize) {
    let view = match state.project().view(sheet).map(|v| &v.kind) {
        Some(ViewKind::Sheet { viewports, .. }) => viewports.get(index).map(|vp| vp.view),
        _ => None,
    };
    if let Some(view) = view {
        run(state, views::SheetRemoveViewport { sheet: Ref::Id(sheet.0), view: Ref::Id(view.0) });
    }
    state.sheet_selection = None;
}

/// Add a level above the highest one, with its floor and ceiling plans.
pub fn add_level(state: &mut AppState) {
    run(state, levels::LevelCreate { name: None, elevation: None, plans: None });
}

/// Delete a level and its plan views; refused while elements use it.
pub fn delete_level(state: &mut AppState, level: LevelId) {
    run(state, levels::LevelDelete { level: Ref::Id(level.0) });
}

/// Open the project's first 3D view, creating one when there is none.
pub fn open_default_3d(state: &mut AppState) {
    let existing = state.project().views_of(ViewCategory::ThreeD).next().map(|v| v.id);
    if let Some(id) = existing.or_else(|| new_3d(state)) {
        state.open_view(id);
    }
}

pub fn activate_category_tool(state: &mut AppState, category: Category) {
    // A project without a ramp type gets one.
    if category == Category::Ramp && state.project().default_type(Category::Ramp).is_none() {
        run_command(state, cc_engine::commands::ramp::RampTypeCreate::default());
    }
    if category == Category::Mass && state.project().default_type(Category::Mass).is_none() {
        run_command(state, cc_engine::commands::mass::MassTypeCreate::default());
    }
    match Tool::for_category(state.project(), category) {
        Some(tool) => {
            // Doors and windows also go on walls clicked in a 3D view.
            let in_3d = matches!(tool, Tool::Hosted { .. })
                && state
                    .active_view
                    .and_then(|v| state.project().view(v))
                    .is_some_and(|v| matches!(v.kind, cc_core::ViewKind::ThreeD { .. }));
            state.set_tool(tool);
            let in_plan = state.active_level().is_some();
            // Pipes and conduits show only in the plans of their discipline:
            // drawn in another plan they would vanish once created (#66).
            if in_plan && let Some((discipline, message)) = services_plan_needed(state, category) {
                open_mep_plan(state, discipline);
                state.message = message;
                return;
            }
            state.message = match () {
                _ if in_plan => String::new(),
                _ if in_3d => t!("hint-hosted"),
                _ => t!("msg-open-plan"),
            };
        }
        None => state.message = t!("msg-no-type"),
    }
}

/// The services plan to draw `category` in (with the message telling so),
/// when it is a pipe or conduit and the active plan does not show it.
fn services_plan_needed(state: &AppState, category: Category) -> Option<(cc_core::MepDiscipline, String)> {
    let (discipline, message) = match category {
        Category::Pipe => (cc_core::MepDiscipline::Plumbing, t!("msg-pipes-in-plumbing-plan")),
        Category::Conduit => (cc_core::MepDiscipline::Electrical, t!("msg-conduits-in-electrical-plan")),
        _ => return None,
    };
    let view = state.active_view.and_then(|v| state.project().view(v))?;
    let shown = matches!(view.kind, ViewKind::FloorPlan { mep: Some(d), .. } if d == discipline);
    (!shown).then_some((discipline, message))
}

/// A new project from the template, named in the active UI language.
pub fn new_project() -> Project {
    cc_engine::new_project()
}

/// Switch the UI language and refresh text baked into cached drawings.
pub fn set_language(state: &mut AppState, lang: crate::i18n::Language) {
    if crate::i18n::language() == lang {
        return;
    }
    crate::i18n::set_language(lang);
    state.invalidate_drawings();
    state.message = t!("msg-language-changed");
}

// --- Files ---

/// The project file dialog, starting in the default projects folder when one is set.
fn file_dialog(state: &AppState) -> rfd::FileDialog {
    let dialog = rfd::FileDialog::new().add_filter(t!("file-filter"), &["ccad"]);
    match state.prefs.projects_dir() {
        Some(dir) => dialog.set_directory(dir),
        None => dialog,
    }
}

/// Run `action` now, or ask for confirmation first if there are unsaved changes.
pub fn guarded(state: &mut AppState, action: PendingAction) {
    if state.session.doc.is_dirty() {
        state.dialogs.confirm = Some(action);
    } else {
        run_pending(state, action);
    }
}

pub fn run_pending(state: &mut AppState, action: PendingAction) {
    match action {
        PendingAction::New => {
            state.replace_document(Document::new(new_project()));
            state.message = t!("msg-new-project");
        }
        PendingAction::Open => open(state),
        PendingAction::OpenPath(path) => open_path(state, path),
        PendingAction::OpenSample(id) => {
            if let Some(project) = crate::home::sample_project(id) {
                let name = project.info.name.clone();
                state.replace_document(Document::new(project));
                state.message = t!("msg-opened", "path" => name);
            }
        }
        PendingAction::ImportIfc => import_ifc(state),
        PendingAction::Close => close_project(state),
        PendingAction::Quit => state.quit_confirmed = true,
    }
}

/// Close the project and return to the home screen.
fn close_project(state: &mut AppState) {
    state.replace_document(Document::new(new_project()));
    state.project_open = false;
    state.show_home = true;
    state.message = t!("msg-project-closed");
}

pub fn open(state: &mut AppState) {
    let Some(path) = file_dialog(state).pick_file() else { return };
    open_path(state, path);
}

pub fn open_path(state: &mut AppState, path: std::path::PathBuf) {
    let mut session = cc_engine::Session::default();
    match session.run(project::ProjectOpen { path: path.clone() }) {
        Ok(out) => {
            let repairs = out["repairs"].as_u64().unwrap_or(0);
            state.replace_document(session.doc);
            state.message = if repairs > 0 {
                t!("msg-opened-repaired", "path" => path.display(), "count" => repairs)
            } else {
                t!("msg-opened", "path" => path.display())
            };
            crate::home::push_recent(&mut state.recent, &path, state.prefs.recent_count);
            if crate::thumbnails::project_file(&path).is_some_and(|f| !f.exists()) {
                state.thumbnail_jobs.push(crate::thumbnails::Job::Project(path));
            }
        }
        Err(err) => state.dialogs.error = Some(t!("msg-open-failed", "error" => err.message)),
    }
}

/// Save the component `name` (all its types) to a `.ccomp` file.
pub fn save_component(state: &mut AppState, name: &str) {
    let dialog = rfd::FileDialog::new()
        .add_filter(t!("file-filter-component"), &["ccomp"])
        .set_file_name(format!("{name}.ccomp"));
    let Some(mut path) = dialog.save_file() else { return };
    if path.extension().is_none() {
        path.set_extension("ccomp");
    }
    match state.session.run(io::ComponentSave { component: name.to_string(), path: path.clone() }) {
        Ok(_) => state.message = t!("msg-exported", "path" => path.display()),
        Err(e) => state.dialogs.error = Some(t!("msg-component-save-failed", "error" => e.message)),
    }
}

/// Load a `.ccomp` file: its types (and missing materials) join the project
/// in one undo step, and the component tool picks the first type.
pub fn load_component(state: &mut AppState) {
    let dialog = rfd::FileDialog::new().add_filter(t!("file-filter-component"), &["ccomp"]);
    let Some(path) = dialog.pick_file() else { return };
    load_component_path(state, path);
}

/// Load the component file at `path` (see [`load_component`]).
pub fn load_component_path(state: &mut AppState, path: std::path::PathBuf) {
    match state.session.run(io::ComponentLoad { path }) {
        Ok(out) => {
            if let Some(first) = out["types"].get(0).and_then(serde_json::Value::as_u64) {
                // Doors and windows are placed in walls.
                let type_id = cc_core::ElementTypeId(first);
                state.set_tool(match state.project().element_type(type_id).map(|t| t.category()) {
                    Some(category @ (cc_core::Category::Door | cc_core::Category::Window)) => {
                        crate::tools::Tool::Hosted { category, type_id }
                    }
                    _ => crate::tools::Tool::Component { type_id },
                });
            }
            let name = out["component"].as_str().unwrap_or_default().to_string();
            state.message = t!("msg-component-loaded", "name" => name);
        }
        Err(e) => state.dialogs.error = Some(t!("msg-component-load-failed", "error" => e.message)),
    }
}

/// Replace the document with a project imported from an IFC file. The
/// result has no file path: saving asks where to write the `.ccad`.
fn import_ifc(state: &mut AppState) {
    let mut dialog = rfd::FileDialog::new().add_filter(t!("file-filter-ifc"), &["ifc"]);
    if let Some(dir) = state.prefs.projects_dir() {
        dialog = dialog.set_directory(dir);
    }
    let Some(path) = dialog.pick_file() else { return };
    let mut session = cc_engine::Session::default();
    match session.run(io::IfcImport { path: path.clone() }) {
        Ok(report) => {
            state.replace_document(session.doc);
            let count =
                |v: &serde_json::Value| v.as_object().map_or(0, |o| o.values().filter_map(|n| n.as_u64()).sum());
            let (imported, skipped, generic) =
                (count(&report["imported"]), count(&report["skipped"]), count(&report["generic"]));
            state.message = if skipped == 0 {
                t!("msg-imported", "path" => path.display(), "count" => imported)
            } else {
                t!("msg-imported-skipped", "path" => path.display(), "count" => imported, "skipped" => skipped)
            };
            if generic > 0 {
                state.message = format!("{} {}", state.message, t!("msg-imported-generic", "count" => generic));
            }
        }
        Err(e) => state.dialogs.error = Some(t!("msg-import-failed", "error" => e.message)),
    }
}

pub fn export_ifc(state: &mut AppState) {
    let name = format!("{}.ifc", state.project().info.name);
    let dialog = rfd::FileDialog::new().add_filter(t!("file-filter-ifc"), &["ifc"]).set_file_name(name);
    let Some(mut path) = dialog.save_file() else { return };
    if path.extension().is_none() {
        path.set_extension("ifc");
    }
    match state.session.run(io::IfcExport { path: path.clone() }) {
        Ok(_) => state.message = t!("msg-exported", "path" => path.display()),
        Err(e) => state.dialogs.error = Some(t!("msg-export-failed", "error" => e.message)),
    }
}

/// Vector formats a sheet can be exported to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetFormat {
    Pdf,
    Svg,
}

/// The active view when it is a sheet.
pub fn active_sheet(state: &AppState) -> Option<ViewId> {
    let id = state.active_view?;
    matches!(state.project().view(id)?.kind, ViewKind::Sheet { .. }).then_some(id)
}

/// Export the active sheet as PDF or SVG.
pub fn export_sheet(state: &mut AppState, format: SheetFormat) {
    let Some(id) = active_sheet(state) else { return };
    let Some(view) = state.project().view(id) else { return };
    let (ext, filter) = match format {
        SheetFormat::Pdf => ("pdf", t!("file-filter-pdf")),
        SheetFormat::Svg => ("svg", t!("file-filter-svg")),
    };
    let name = match &view.kind {
        ViewKind::Sheet { number, .. } => format!("{number} - {}.{ext}", view.name),
        _ => format!("{}.{ext}", view.name),
    };
    let dialog = rfd::FileDialog::new().add_filter(filter, &[ext]).set_file_name(name);
    let Some(mut path) = dialog.save_file() else { return };
    if path.extension().is_none() {
        path.set_extension(ext);
    }
    let format = match format {
        SheetFormat::Pdf => "pdf",
        SheetFormat::Svg => "svg",
    };
    let command = io::SheetExport { sheet: Ref::Id(id.0), path: path.clone(), format: Some(format.to_string()) };
    match state.session.run(command) {
        Ok(_) => state.message = t!("msg-exported", "path" => path.display()),
        Err(e) => state.dialogs.error = Some(t!("msg-export-failed", "error" => e.message)),
    }
}

/// Resolution of exported images, dots per inch.
pub const IMAGE_DPI: f64 = 150.0;

/// The active view when it has a drawing to export as an image (any view
/// but a schedule).
pub fn image_view(state: &AppState) -> Option<ViewId> {
    let id = state.active_view?;
    let kind = &state.project().view(id)?.kind;
    (!matches!(kind, ViewKind::Schedule { .. })).then_some(id)
}

/// Export the active view as a PNG image, as printed (no temporary hiding).
pub fn export_image(state: &mut AppState) {
    let Some(id) = image_view(state) else { return };
    let Some(view) = state.project().view(id) else { return };
    let name = match &view.kind {
        ViewKind::Sheet { number, .. } => format!("{number} - {}.png", view.name),
        _ => format!("{}.png", view.name),
    };
    let dialog = rfd::FileDialog::new().add_filter(t!("file-filter-png"), &["png"]).set_file_name(name);
    let Some(mut path) = dialog.save_file() else { return };
    if path.extension().is_none() {
        path.set_extension("png");
    }
    let command =
        io::ViewExport { view: Ref::Id(id.0), path: path.clone(), format: Some("png".into()), dpi: Some(IMAGE_DPI) };
    match state.session.run(command) {
        Ok(_) => state.message = t!("msg-exported", "path" => path.display()),
        Err(e) => state.dialogs.error = Some(t!("msg-export-failed", "error" => e.message)),
    }
}

/// Export the active view as DXF, for other CAD programs: at full size in
/// millimeters (a sheet in paper millimeters), as printed.
pub fn export_dxf(state: &mut AppState) {
    let Some(id) = image_view(state) else { return };
    let Some(view) = state.project().view(id) else { return };
    let name = match &view.kind {
        ViewKind::Sheet { number, .. } => format!("{number} - {}.dxf", view.name),
        _ => format!("{}.dxf", view.name),
    };
    let dialog = rfd::FileDialog::new().add_filter(t!("file-filter-dxf"), &["dxf"]).set_file_name(name);
    let Some(mut path) = dialog.save_file() else { return };
    if path.extension().is_none() {
        path.set_extension("dxf");
    }
    let command = io::ViewExport { view: Ref::Id(id.0), path: path.clone(), format: Some("dxf".into()), dpi: None };
    match state.session.run(command) {
        Ok(_) => state.message = t!("msg-exported", "path" => path.display()),
        Err(e) => state.dialogs.error = Some(t!("msg-export-failed", "error" => e.message)),
    }
}

// --- Macros ---

/// The macro file dialog, starting in the macros folder when one is set.
fn macro_dialog(state: &AppState) -> rfd::FileDialog {
    let dialog = rfd::FileDialog::new().add_filter(t!("file-filter-macro"), &["ccmacro", "jsonl", "json"]);
    match state.prefs.macro_dir() {
        Some(dir) => dialog.set_directory(dir),
        None => dialog,
    }
}

/// Start recording a macro, or stop and save it as a script of commands
/// (the format `ccad script` runs).
pub fn toggle_macro_recording(state: &mut AppState) {
    if !state.session.is_recording() {
        state.session.start_recording();
        state.message = t!("msg-macro-recording");
        return;
    }
    let steps = state.session.stop_recording();
    if steps.is_empty() {
        state.message = t!("msg-macro-empty");
        return;
    }
    let Some(mut path) = macro_dialog(state).set_file_name("macro.ccmacro").save_file() else { return };
    if path.extension().is_none() {
        path.set_extension("ccmacro");
    }
    match std::fs::write(&path, cc_engine::script::to_text(&steps)) {
        Ok(()) => state.message = t!("msg-macro-saved", "path" => path.display(), "count" => steps.len()),
        Err(e) => state.dialogs.error = Some(t!("msg-export-failed", "error" => t!("err-io", "detail" => e))),
    }
}

/// Run a macro (a script of commands) on the open project; each step is one undo step.
pub fn run_macro(state: &mut AppState) {
    let Some(path) = macro_dialog(state).pick_file() else { return };
    run_macro_file(state, &path);
}

/// Run the macro at `path` (see [`run_macro`]).
pub fn run_macro_file(state: &mut AppState, path: &std::path::Path) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            state.dialogs.error = Some(t!("msg-macro-failed", "step" => 0, "error" => t!("err-io", "detail" => e)));
            return;
        }
    };
    let steps = match cc_engine::script::parse(&text) {
        Ok(steps) => steps,
        Err(e) => {
            state.dialogs.error = Some(t!("msg-macro-failed", "step" => 0, "error" => e.message));
            return;
        }
    };
    match cc_engine::script::run(&mut state.session, &steps) {
        Ok(done) => state.message = t!("msg-macro-done", "count" => done.len()),
        Err((_, failed)) => {
            let step = format!("{} ({})", failed.index + 1, failed.command);
            state.dialogs.error = Some(t!("msg-macro-failed", "step" => step, "error" => failed.error.message));
        }
    }
}

pub fn save(state: &mut AppState, save_as: bool) -> bool {
    let path = match (&state.session.doc.path, save_as) {
        (Some(p), false) => p.clone(),
        _ => {
            let name = format!("{}.ccad", state.project().info.name);
            // A saved project is saved again next to itself; a new one in the projects folder.
            let mut dialog = file_dialog(state).set_file_name(name);
            if let Some(dir) = state.session.doc.path.as_deref().and_then(std::path::Path::parent) {
                dialog = dialog.set_directory(dir);
            }
            let Some(mut p) = dialog.save_file() else { return false };
            if p.extension().is_none() {
                p.set_extension("ccad");
            }
            p
        }
    };
    match state.session.run(project::ProjectSave { path: Some(path.clone()) }) {
        Ok(_) => {
            state.message = t!("msg-saved", "path" => path.display());
            crate::home::push_recent(&mut state.recent, &path, state.prefs.recent_count);
            state.thumbnail_jobs.push(crate::thumbnails::Job::Project(path.clone()));
            true
        }
        Err(e) => {
            state.dialogs.error = Some(t!("msg-save-failed", "error" => e.message));
            false
        }
    }
}

/// Select every instance of the selected elements' types shown in the
/// active view (the engine's `elements_list` by type and view).
pub fn select_instances(state: &mut AppState) {
    let Some(view) = state.active_view else { return };
    let types: std::collections::BTreeSet<cc_core::ElementTypeId> =
        state.selection.iter().filter_map(|id| state.project().element(*id)).map(|e| e.type_id).collect();
    let mut found = std::collections::BTreeSet::new();
    for type_id in types {
        let command = cc_engine::commands::query::ElementsList {
            category: None,
            level: None,
            type_ref: Some(Ref::Id(type_id.0)),
            view: Some(Ref::Id(view.0)),
        };
        let Some(out) = run(state, command) else { return };
        let ids = out.as_array().into_iter().flatten().filter_map(|e| e["id"].as_u64());
        found.extend(ids.map(cc_core::ElementId));
    }
    if !found.is_empty() {
        state.message = t!("msg-instances-selected", "count" => found.len());
        state.selection = found;
    }
}

/// Group the selection (the engine's `group_create`).
pub fn group_selection(state: &mut AppState) {
    let ids = state.selection.iter().map(|id| id.0).collect();
    if let Some(out) = run(state, cc_engine::commands::modify::GroupCreate { ids, name: None }) {
        state.message = t!("msg-grouped", "name" => out["name"].as_str().unwrap_or_default());
    }
}

/// Ungroup every group the selection touches (the engine's `group_ungroup`).
pub fn ungroup_selection(state: &mut AppState) {
    let groups: std::collections::BTreeSet<cc_core::GroupId> =
        state.selection.iter().filter_map(|id| state.project().group_of(*id)).collect();
    for group in groups {
        run(state, cc_engine::commands::modify::GroupUngroup { group: group.0 });
    }
    state.message = t!("msg-ungrouped");
}

/// Ids of the selected grids, or of every grid when none is selected.
pub fn grids_for(state: &AppState) -> Vec<u64> {
    let project = state.project();
    let is_grid = |id: &cc_core::ElementId| project.element(*id).is_some_and(|e| e.category() == Category::Grid);
    let selected: Vec<u64> = state.selection.iter().filter(|id| is_grid(id)).map(|id| id.0).collect();
    if !selected.is_empty() {
        return selected;
    }
    project.elements_of(Category::Grid).map(|e| e.id.0).collect()
}

/// The single selected grid, the reference of [`align_grid_ends`].
pub fn reference_grid(state: &AppState) -> Option<u64> {
    let project = state.project();
    let mut grids =
        state.selection.iter().filter(|id| project.element(**id).is_some_and(|e| e.category() == Category::Grid));
    match (grids.next(), grids.next()) {
        (Some(id), None) => Some(id.0),
        _ => None,
    }
}

/// Give every grid parallel to the selected one its extents, so their
/// bubbles line up (the engine's `grids_align_ends`).
pub fn align_grid_ends(state: &mut AppState) {
    let Some(reference) = reference_grid(state) else {
        state.message = t!("why-select-one-grid");
        return;
    };
    let command = cc_engine::commands::modify::GridsAlignEnds { reference, grids: Vec::new(), ends: None };
    let Some(out) = run(state, command) else { return };
    let count = out["changed"].as_array().map_or(0, Vec::len);
    state.message = t!("msg-grid-ends-aligned", "count" => count);
}

/// The selected columns as a chain: from the lowest-left one, each followed by the nearest left.
pub fn selected_columns(state: &AppState) -> Vec<u64> {
    let project = state.project();
    let mut left: Vec<(u64, glam::DVec2)> = state
        .selection
        .iter()
        .filter_map(|id| match &project.element(*id)?.kind {
            cc_core::ElementKind::Column(c) => Some((id.0, c.position)),
            _ => None,
        })
        .collect();
    left.sort_by(|a, b| a.1.x.total_cmp(&b.1.x).then(a.1.y.total_cmp(&b.1.y)));
    let mut chain = Vec::with_capacity(left.len());
    let mut at = left.first().map(|c| c.1);
    while let Some(here) = at {
        let Some(i) = (0..left.len()).min_by(|&i, &j| left[i].1.distance(here).total_cmp(&left[j].1.distance(here)))
        else {
            break;
        };
        let (id, p) = left.remove(i);
        chain.push(id);
        at = (!left.is_empty()).then_some(p);
    }
    chain
}

/// The level new structure goes on: the active plan's, else the lowest.
fn structure_level(state: &AppState) -> Option<Ref> {
    let level = state.active_level().or_else(|| state.project().levels.first().map(|l| l.id))?;
    Some(Ref::Id(level.0))
}

/// Select what a placement created and say how much, or why nothing.
/// What a placement command placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placed {
    Columns,
    Beams,
    Footings,
}

fn placed(state: &mut AppState, out: Option<serde_json::Value>, what: Placed) {
    let Some(out) = out else { return };
    let created: std::collections::BTreeSet<cc_core::ElementId> =
        out["created"].as_array().into_iter().flatten().filter_map(|v| v.as_u64()).map(cc_core::ElementId).collect();
    let skipped = out["skipped"].as_u64().unwrap_or(0);
    state.message = if created.is_empty() {
        t!("msg-nothing-placed", "skipped" => skipped)
    } else {
        let count = created.len();
        match what {
            Placed::Beams => t!("msg-beams-placed", "count" => count, "skipped" => skipped),
            Placed::Columns => t!("msg-columns-placed", "count" => count, "skipped" => skipped),
            Placed::Footings => t!("msg-footings-placed", "count" => count, "skipped" => skipped),
        }
    };
    if !created.is_empty() {
        state.selection = created;
    }
}

/// Columns at the intersections of the selected grids (the engine's `columns_at_grids`).
pub fn columns_at_grids(state: &mut AppState) {
    let command = cc_engine::commands::structure::ColumnsAtGrids {
        grids: grids_for(state),
        level: structure_level(state),
        ..Default::default()
    };
    let out = run(state, command);
    placed(state, out, Placed::Columns);
}

/// Beams between the columns along the selected grids (the engine's `beams_on_grids`).
pub fn beams_on_grids(state: &mut AppState) {
    let command = cc_engine::commands::structure::BeamsOnGrids {
        grids: grids_for(state),
        level: structure_level(state),
        ..Default::default()
    };
    let out = run(state, command);
    placed(state, out, Placed::Beams);
}

/// Ids of the selected elements of `category`.
fn selected_of(state: &AppState, category: Category) -> Vec<u64> {
    let project = state.project();
    state
        .selection
        .iter()
        .filter(|id| project.element(**id).is_some_and(|e| e.category() == category))
        .map(|id| id.0)
        .collect()
}

/// Pad footings under the selected columns, or every column of the active level.
pub fn footings_under_columns(state: &mut AppState) {
    let columns = selected_of(state, Category::Column);
    let level = if columns.is_empty() { structure_level(state) } else { None };
    let command = cc_engine::commands::foundation::FootingsUnderColumns { columns, level, type_ref: None };
    let out = run(state, command);
    placed(state, out, Placed::Footings);
}

/// Strip footings under the selected walls, or every wall of the active level.
pub fn strip_footings_under_walls(state: &mut AppState) {
    let walls = selected_of(state, Category::Wall);
    let level = if walls.is_empty() { structure_level(state) } else { None };
    let command = cc_engine::commands::foundation::StripFootingsUnderWalls { walls, level, type_ref: None };
    let out = run(state, command);
    placed(state, out, Placed::Footings);
}

/// Join the selected floors (or those of the active level) with the beams, columns and walls carrying them.
pub fn slab_join_supports(state: &mut AppState) {
    let floors = selected_of(state, Category::Floor);
    let level = if floors.is_empty() { state.active_level().map(|l| Ref::Id(l.0)) } else { None };
    let command = cc_engine::commands::structure::SlabJoinSupports { floors, level };
    if let Some(out) = run(state, command) {
        let count = out["joined"].as_array().map_or(0, Vec::len);
        state.message = t!("msg-supports-joined", "count" => count);
    }
}

/// Open the analytical model view, creating it the first time.
pub fn open_analytical_view(state: &mut AppState) {
    let existing = state
        .project()
        .views
        .values()
        .find(|v| matches!(v.kind, ViewKind::ThreeD { analytical: true, .. }))
        .map(|v| v.id);
    match existing {
        Some(id) => state.open_view(id),
        None => {
            new_view(state, "analytical", None, None);
        }
    }
}

/// Export the analytical model as JSON.
pub fn export_analytical(state: &mut AppState) {
    let name = format!("{}.analytical.json", state.project().info.name);
    let dialog = rfd::FileDialog::new().add_filter(t!("file-filter-json"), &["json"]).set_file_name(name);
    let Some(mut path) = dialog.save_file() else { return };
    if path.extension().is_none() {
        path.set_extension("json");
    }
    match state.session.run(cc_engine::commands::analytical::AnalyticalExport { path: path.clone() }) {
        Ok(_) => state.message = t!("msg-exported", "path" => path.display()),
        Err(e) => state.dialogs.error = Some(t!("msg-export-failed", "error" => e.message)),
    }
}

/// X bracing between the two selected columns.
pub fn braces_x(state: &mut AppState) {
    let columns = selected_columns(state);
    let command =
        cc_engine::commands::framing::BracesBetweenColumns { columns, pattern: Some("x".into()), type_ref: None };
    if let Some(out) = run(state, command) {
        let ids = out["created"].as_array().into_iter().flatten().filter_map(|v| v.as_u64());
        state.selection = ids.map(cc_core::ElementId).collect();
    }
}

/// Typical reinforcement for the selected beams and columns, or all of the active level.
pub fn typical_rebar(state: &mut AppState) {
    let project = state.project();
    let elements: Vec<u64> = state
        .selection
        .iter()
        .filter(|id| project.element(**id).is_some_and(|e| matches!(e.category(), Category::Beam | Category::Column)))
        .map(|id| id.0)
        .collect();
    let level = if elements.is_empty() { state.active_level().map(|l| Ref::Id(l.0)) } else { None };
    if let Some(out) = run(state, cc_engine::commands::rebar::RebarTypical { elements, level }) {
        let count = out["created"].as_array().map_or(0, Vec::len);
        let skipped = out["skipped"].as_u64().unwrap_or(0);
        state.message = t!("msg-rebar-placed", "count" => count, "skipped" => skipped);
    }
}

/// Open the first 3D view showing reinforcement, creating one the first time.
pub fn open_rebar_view(state: &mut AppState) {
    let existing =
        state.project().views.values().find(|v| matches!(v.kind, ViewKind::ThreeD { rebar: true, .. })).map(|v| v.id);
    match existing {
        Some(id) => state.open_view(id),
        None => {
            new_view(state, "rebar", None, None);
        }
    }
}

/// Open the structural plan of the active level (or the lowest), creating it the first time.
pub fn open_structural_plan(state: &mut AppState) {
    let Some(level) = state.active_level().or_else(|| state.project().levels.first().map(|l| l.id)) else { return };
    let existing = state
        .project()
        .views
        .values()
        .find(|v| matches!(v.kind, ViewKind::FloorPlan { level: l, structural: true, .. } if l == level))
        .map(|v| v.id);
    match existing {
        Some(id) => state.open_view(id),
        None => {
            new_view(state, "structural_plan", Some(level), None);
        }
    }
}

/// Connect the selected fixtures and devices to the nearest pipes and conduits (`mep_connect`).
pub fn mep_connect(state: &mut AppState) {
    let project = state.project();
    let elements: Vec<u64> = state
        .selection
        .iter()
        .filter(|id| project.element(**id).is_some_and(|e| project.mep_data(e).is_some()))
        .map(|id| id.0)
        .collect();
    if let Some(out) = run(state, cc_engine::commands::mep::MepConnect { elements, system: None }) {
        let count = out["created"].as_array().map_or(0, Vec::len);
        let skipped = out["skipped"].as_u64().unwrap_or(0);
        state.message = t!("msg-mep-connected", "count" => count, "skipped" => skipped);
    }
}

/// Open the diagram of the selected distribution board (or the first), creating it the first time.
pub fn open_panel_diagram(state: &mut AppState) {
    let project = state.project();
    let is_board = |id: &cc_core::ElementId| {
        project.element(*id).and_then(|e| project.mep_data(e)).and_then(|m| m.device())
            == Some(cc_core::DeviceKind::Panel)
    };
    let boards: Vec<cc_core::ElementId> = project.elements.keys().copied().filter(is_board).collect();
    let Some(board) = state.selection.iter().copied().find(is_board).or(boards.first().copied()) else { return };
    let existing =
        project.views.values().find(|v| matches!(v.kind, ViewKind::Diagram { panel } if panel == board)).map(|v| v.id);
    match existing {
        Some(id) => state.open_view(id),
        None => {
            let command =
                views::ViewCreate { kind: "panel_diagram".into(), panel: Some(board.0), ..Default::default() };
            if let Some(id) = run(state, command).as_ref().and_then(view_id) {
                state.open_view(id);
            }
        }
    }
}

/// Open the plumbing or electrical plan of the active level (or the lowest), creating it the first time.
pub fn open_mep_plan(state: &mut AppState, discipline: cc_core::MepDiscipline) {
    let Some(level) = state.active_level().or_else(|| state.project().levels.first().map(|l| l.id)) else { return };
    let existing = state
        .project()
        .views
        .values()
        .find(|v| matches!(v.kind, ViewKind::FloorPlan { level: l, mep: Some(d), .. } if l == level && d == discipline))
        .map(|v| v.id);
    match existing {
        Some(id) => state.open_view(id),
        None => {
            let kind = match discipline {
                cc_core::MepDiscipline::Plumbing => "plumbing_plan",
                cc_core::MepDiscipline::Electrical => "electrical_plan",
            };
            new_view(state, kind, Some(level), None);
        }
    }
}

/// Number the beams, columns and footings of the active level (or the whole project).
pub fn number_structure(state: &mut AppState) {
    let level = state.active_level().map(|l| Ref::Id(l.0));
    let command = cc_engine::commands::structure::StructureNumber { categories: Vec::new(), level };
    if let Some(out) = run(state, command) {
        state.message = t!("msg-numbered", "count" => out["numbered"].as_u64().unwrap_or(0));
    }
}

/// The active view when it is a floor plan, with whether it shows structure tags.
pub fn active_floor_plan(state: &AppState) -> Option<(ViewId, bool)> {
    let id = state.active_view?;
    match state.project().view(id)?.kind {
        ViewKind::FloorPlan { structure_tags, structural, .. } => Some((id, structure_tags || structural)),
        _ => None,
    }
}

/// Show or hide the beam and column tags of the active floor plan.
pub fn toggle_structure_tags(state: &mut AppState) {
    let Some((id, on)) = active_floor_plan(state) else { return };
    crate::engine_edit::view(state, id, None, |v| {
        if let ViewKind::FloorPlan { structure_tags, structural, .. } = &mut v.kind {
            *structure_tags = !on;
            if on {
                *structural = false;
            }
        }
    });
}

/// Beams through the selected columns (the engine's `beams_between_columns`).
pub fn beams_between_columns(state: &mut AppState) {
    let command =
        cc_engine::commands::structure::BeamsBetweenColumns { columns: selected_columns(state), ..Default::default() };
    let out = run(state, command);
    placed(state, out, Placed::Beams);
}

/// Hide the selection in the active view (permanently; Visibility/Graphics shows it again).
pub fn hide_in_view(state: &mut AppState) {
    let Some(view) = crate::commands::graphics_view(state) else { return };
    let ids = state.selection.iter().map(|e| e.0).collect();
    let command = cc_engine::commands::graphics::ElementHideInView { view: Ref::Id(view.0), ids, hidden: true };
    if run(state, command).is_some() {
        state.selection.clear();
    }
}

/// Temporarily hide or isolate the selection (or its categories) in the active view, or reset.
pub fn temporary(state: &mut AppState, action: &str) {
    let Some(view) = crate::commands::graphics_view(state) else { return };
    let ids = if action == "reset" { Vec::new() } else { state.selection.iter().map(|e| e.0).collect() };
    let command = cc_engine::commands::graphics::ViewTemporary {
        view: Ref::Id(view.0),
        action: action.to_string(),
        ids,
        categories: Vec::new(),
    };
    if run(state, command).is_some() && action.starts_with("hide") {
        state.selection.clear();
    }
}

/// Make the active view's temporary hide/isolate permanent.
pub fn temporary_permanent(state: &mut AppState) {
    let Some(view) = crate::commands::graphics_view(state) else { return };
    run(state, cc_engine::commands::graphics::ViewTemporaryApply { view: Ref::Id(view.0) });
}

/// The masses in the selection.
pub fn selected_masses(state: &AppState) -> Vec<ElementId> {
    let project = state.project();
    state
        .selection
        .iter()
        .copied()
        .filter(|id| project.element(*id).is_some_and(|e| e.category() == Category::Mass))
        .collect()
}

/// Walls (`walls`), floors (`slabs`) or roofs (`roof`) from each selected
/// mass; what they make is selected.
pub fn from_masses(state: &mut AppState, what: &str) {
    use cc_engine::commands::mass::{MassRoofCreate, MassSlabsCreate, MassWallsCreate};
    let mut made = Vec::new();
    for id in selected_masses(state) {
        let out = match what {
            "walls" => run_command(state, MassWallsCreate { id: id.0, type_ref: None, sides: None }),
            "slabs" => run_command(state, MassSlabsCreate { id: id.0, type_ref: None, levels: None }),
            _ => run_command(state, MassRoofCreate { id: id.0, type_ref: None, overhang: None }),
        };
        let Some(out) = out else { continue };
        match out["created"].as_array() {
            Some(list) => made.extend(list.iter().filter_map(serde_json::Value::as_u64).map(ElementId)),
            None => made.extend(out["id"].as_u64().map(ElementId)),
        }
    }
    if !made.is_empty() {
        state.message = t!("msg-mass-made", "count" => made.len());
        state.selection = made.into_iter().collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recorded_macro_replays_on_another_project() {
        let mut state = AppState::new(Document::new(new_project()));
        let levels = state.project().levels.len();
        state.session.start_recording();
        add_level(&mut state);
        let wall = run_command(
            &mut state,
            cc_engine::commands::create::WallCreate { start: [0.0, 0.0], end: [5.0, 0.0], ..Default::default() },
        )
        .and_then(|o| o["id"].as_u64())
        .map(cc_core::ElementId)
        .unwrap();
        // Dragging the height in the properties: one recorded step.
        for h in [3.2, 3.4] {
            crate::engine_edit::element(&mut state, wall, Some(1), |e| {
                if let cc_core::ElementKind::Wall(w) = &mut e.kind {
                    w.height = h;
                }
            });
        }
        let steps = state.session.stop_recording();
        assert_eq!(steps.len(), 3);
        let path = std::env::temp_dir().join(format!("ccad-macro-{}.ccmacro", std::process::id()));
        std::fs::write(&path, cc_engine::script::to_text(&steps)).unwrap();
        let mut other = AppState::new(Document::new(new_project()));
        run_macro_file(&mut other, &path);
        assert!(other.dialogs.error.is_none(), "{:?}", other.dialogs.error);
        assert_eq!(other.project().levels.len(), levels + 1);
        let (_, w) = other.project().walls().next().unwrap();
        assert!((w.height - 3.4).abs() < 1e-12);
    }

    #[test]
    fn closing_returns_home_and_opening_leaves_it() {
        let mut state = AppState::new(Document::new(new_project()));
        run_pending(&mut state, PendingAction::Close);
        assert!(state.show_home && !state.project_open);

        // Opening a sample leaves the home screen with a project open.
        run_pending(&mut state, PendingAction::OpenSample("casa_simples"));
        assert!(!state.show_home && state.project_open);
        assert_eq!(state.project().info.name, crate::home::sample_project("casa_simples").unwrap().info.name);

        run_pending(&mut state, PendingAction::Close);
        run_pending(&mut state, PendingAction::New);
        assert!(!state.show_home && state.project_open);
    }

    #[test]
    fn deleting_a_level_unconnects_walls_rising_to_it() {
        let mut state = AppState::new(Document::new(crate::home::sample_project("casa_simples").unwrap()));
        let upper = state.project().levels[1].id;
        let (id, height) = state
            .project()
            .walls()
            .find(|(_, w)| matches!(w.top, cc_core::WallTop::Level { .. }))
            .map(|(e, w)| (e.id, state.project().wall_height(e, w)))
            .unwrap();
        delete_level(&mut state, upper);
        let e = state.project().element(id).unwrap();
        let w = e.as_wall().unwrap();
        assert_eq!(w.top, cc_core::WallTop::Unconnected);
        assert!((state.project().wall_height(e, w) - height).abs() < 1e-9);
        assert!(state.project().level(upper).is_none());
    }

    /// Issue #66: a pipe drawn from the ground floor plan of a new project
    /// vanished once created, since architectural plans leave pipes out.
    #[test]
    fn the_pipe_tool_draws_in_a_plan_that_shows_pipes() {
        use cc_core::MepDiscipline;
        let mut state = AppState::new(Document::new(new_project()));
        let level = state.project().levels[0].id;
        let plan = state
            .project()
            .views
            .values()
            .find(|v| matches!(v.kind, ViewKind::FloorPlan { level: l, mep: None, .. } if l == level))
            .map(|v| v.id)
            .unwrap();
        state.active_view = Some(plan);
        // The app's dock makes the last view asked for the active one.
        let follow = |state: &mut AppState| {
            if let Some(id) = std::mem::take(&mut state.open_requests).pop() {
                state.active_view = Some(id);
            }
        };

        activate_category_tool(&mut state, Category::Pipe);
        follow(&mut state);
        assert!(matches!(state.tool, Tool::Pipe { .. }));
        let view = state.active_view.and_then(|v| state.project().view(v)).unwrap();
        assert!(
            matches!(view.kind, ViewKind::FloorPlan { level: l, mep: Some(MepDiscipline::Plumbing), .. } if l == level)
        );
        let plumbing = view.id;

        // The pipe the tool creates shows in the plan it is drawn in.
        let pipe = run_command(
            &mut state,
            cc_engine::commands::mep::PipeCreate {
                start: [0.0, 0.0],
                end: Some([3.0, 0.0]),
                level: Some(Ref::Id(level.0)),
                ..Default::default()
            },
        )
        .and_then(|o| o["id"].as_u64())
        .map(cc_core::ElementId)
        .unwrap();
        let drawing = state.session.drawing(plumbing).unwrap();
        assert!(drawing.prims.iter().any(|p| p.element() == Some(pipe)));

        // Already in the plumbing plan, the tool stays there; conduits go to the electrical plan.
        let views = state.project().views.len();
        activate_category_tool(&mut state, Category::Pipe);
        assert!(state.open_requests.is_empty() && state.project().views.len() == views);
        activate_category_tool(&mut state, Category::Conduit);
        follow(&mut state);
        let view = state.active_view.and_then(|v| state.project().view(v)).unwrap();
        assert!(matches!(view.kind, ViewKind::FloorPlan { mep: Some(MepDiscipline::Electrical), .. }));
        assert!(matches!(state.tool, Tool::Conduit { .. }));
    }
}
