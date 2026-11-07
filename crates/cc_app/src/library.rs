//! Component library: browse the component files (`.ccomp`) of the folders
//! the user adds (such as a copy of the ConstructCraft Components library),
//! search them, see a component's types, parameters, connectors and plan,
//! load it into the project, and save the project's components into a
//! library folder. Listing, loading and saving are engine commands.

use crate::dialog::Dialog;
use crate::i18n::tr;
use crate::state::AppState;
use crate::t;
use crate::theme::{self, PALETTE};
use cc_core::ComponentKind;
use cc_engine::commands::io;
use egui::{Context, Pos2, RichText, Sense, Stroke, Ui, vec2};
use glam::DVec2;
use std::path::PathBuf;

/// A component file found in a library folder (from `component_library_list`).
#[derive(Debug, Clone, Default)]
pub struct Entry {
    pub path: PathBuf,
    pub component: String,
    pub kind: String,
    pub types: Vec<(String, String)>,
    pub parameters: Vec<String>,
    pub connectors: Vec<String>,
}

/// State of the open library window.
#[derive(Debug, Clone, Default)]
pub struct Library {
    pub entries: Vec<Entry>,
    /// Files that could not be read.
    pub errors: usize,
    pub search: String,
    /// Only this kind (by its `Debug` name), or all.
    pub kind: Option<String>,
    pub selected: Option<PathBuf>,
    /// Plan outlines of the selected component (for its thumbnail).
    pub outlines: Vec<Vec<DVec2>>,
    /// The project component to save into the library, and the folder.
    pub save_component: Option<String>,
    pub save_folder: Option<PathBuf>,
    /// The folders the entries were read from (read again when the preference changes).
    listed: Vec<PathBuf>,
}

/// Open the library window, reading the folders.
pub fn open(state: &mut AppState) {
    let mut lib = Library::default();
    refresh(state, &mut lib);
    state.dialogs.library = Some(lib);
}

/// Read the folders again through the engine.
fn refresh(state: &mut AppState, lib: &mut Library) {
    let folders = state.prefs.library_folders.clone();
    lib.listed = folders.clone();
    let out = state.session.run(io::ComponentLibraryList { folders, search: None });
    let Ok(out) = out else {
        lib.entries.clear();
        return;
    };
    let text = |v: &serde_json::Value| v.as_str().unwrap_or_default().to_string();
    lib.entries = out["components"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| Entry {
            path: PathBuf::from(text(&c["path"])),
            component: text(&c["component"]),
            kind: text(&c["kind"]),
            types: c["types"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| (text(&t["name"]), text(&t["description"])))
                .collect(),
            parameters: c["parameters"].as_array().into_iter().flatten().map(text).collect(),
            connectors: c["connectors"].as_array().into_iter().flatten().map(text).collect(),
        })
        .collect();
    lib.errors = out["errors"].as_array().map_or(0, Vec::len);
    if lib.selected.as_ref().is_some_and(|p| !lib.entries.iter().any(|e| &e.path == p)) {
        lib.selected = None;
        lib.outlines.clear();
    }
}

/// Plan outlines of the component in `path` (its first type), for the thumbnail.
fn outlines(path: &std::path::Path) -> Vec<Vec<DVec2>> {
    let Ok(file) = cc_core::component_file::load(path) else { return Vec::new() };
    let thickness = [(cc_core::component::WALL_THICKNESS, 0.15)];
    let parts = cc_geom::resolve_component_in(&file.def, &Default::default(), &thickness);
    let (in_plan, all): (Vec<_>, Vec<_>) = parts.into_iter().partition(|p| p.in_plan);
    // Components with no piece marked for plans show all their outlines.
    let shown = if in_plan.is_empty() { all } else { in_plan };
    shown.into_iter().filter(|p| !p.void).map(|p| p.footprint).collect()
}

fn kind_label(kind: &str) -> String {
    ComponentKind::ALL.into_iter().find(|k| format!("{k:?}") == kind).map_or_else(|| kind.to_string(), |k| tr(k.key()))
}

pub fn show(ctx: &Context, state: &mut AppState) {
    let Some(mut lib) = state.dialogs.library.take() else { return };
    let mut load = 0;
    let response = Dialog::new("component-library", t!("dlg-library")).width(900.0).show(
        ctx,
        |ui| body(ui, state, &mut lib),
        |f| {
            load = f.primary(t!("lib-load"));
            f.secondary(t!("action-close"));
        },
    );
    if response.dismissed {
        return;
    }
    match response.clicked {
        Some(i) if i == load => {
            if let Some(path) = lib.selected.clone() {
                crate::actions::load_component_path(state, path);
                return;
            }
            state.dialogs.library = Some(lib);
        }
        Some(_) => {}
        None => state.dialogs.library = Some(lib),
    }
}

fn body(ui: &mut Ui, state: &mut AppState, lib: &mut Library) {
    folders(ui, state, lib);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut lib.search).hint_text(t!("lib-search")).desired_width(260.0));
        let label = lib.kind.as_deref().map_or_else(|| t!("lib-all-kinds"), kind_label);
        egui::ComboBox::from_id_salt("lib-kind").selected_text(label).width(160.0).show_ui(ui, |ui| {
            ui.selectable_value(&mut lib.kind, None, t!("lib-all-kinds"));
            for k in ComponentKind::ALL {
                ui.selectable_value(&mut lib.kind, Some(format!("{k:?}")), tr(k.key()));
            }
        });
        if lib.errors > 0 {
            ui.label(RichText::new(t!("lib-errors", "count" => lib.errors)).color(PALETTE.warning).size(12.0));
        }
    });
    ui.add_space(6.0);
    let query = lib.search.trim().to_lowercase();
    let shown: Vec<usize> = (0..lib.entries.len())
        .filter(|&i| {
            let e = &lib.entries[i];
            lib.kind.as_ref().is_none_or(|k| &e.kind == k)
                && (query.is_empty()
                    || e.component.to_lowercase().contains(&query)
                    || e.types
                        .iter()
                        .any(|(n, d)| n.to_lowercase().contains(&query) || d.to_lowercase().contains(&query)))
        })
        .collect();
    crate::dialog::two_columns(ui, [340.0, 540.0], |ui, column| {
        if column == 0 {
            list(ui, lib, &shown);
        } else {
            details(ui, lib);
        }
    });
    ui.add_space(8.0);
    save_section(ui, state, lib);
}

/// How many library folders are read, a button to manage them in the
/// Preferences window (File locations) and one to read them again.
fn folders(ui: &mut Ui, state: &mut AppState, lib: &mut Library) {
    // The folders changed in Preferences: read them again.
    let changed = lib.listed != state.prefs.library_folders;
    ui.horizontal(|ui| {
        ui.label(theme::title(t!("lib-folders")));
        let count = state.prefs.library_folders.len();
        let text = if count == 0 { t!("lib-empty") } else { t!("lib-folder-count", "count" => count) };
        ui.label(theme::muted(text).size(12.0));
    });
    let mut refresh_now = changed;
    ui.horizontal(|ui| {
        if ui.button(t!("lib-manage-folders")).clicked() {
            crate::preferences::open(state, crate::preferences::Section::FileLocations);
        }
        if ui.button(t!("lib-refresh")).clicked() {
            refresh_now = true;
        }
    });
    if refresh_now {
        refresh(state, lib);
    }
}

fn list(ui: &mut Ui, lib: &mut Library, shown: &[usize]) {
    egui::Frame::new().stroke(Stroke::new(1.0, PALETTE.border)).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("lib-list").max_height(360.0).auto_shrink([false, false]).show(ui, |ui| {
            if shown.is_empty() {
                ui.label(theme::muted(t!("lib-none")));
            }
            for &i in shown {
                let Some(e) = lib.entries.get(i) else { continue };
                let selected = lib.selected.as_ref() == Some(&e.path);
                let text = format!("{}  ·  {}", e.component, kind_label(&e.kind));
                let r = ui.selectable_label(selected, text).on_hover_text(e.path.display().to_string());
                if r.clicked() && !selected {
                    lib.selected = Some(e.path.clone());
                    lib.outlines = outlines(&e.path);
                }
            }
        });
    });
}

fn details(ui: &mut Ui, lib: &Library) {
    let Some(e) = lib.selected.as_ref().and_then(|p| lib.entries.iter().find(|e| &e.path == p)) else {
        ui.label(theme::muted(t!("lib-pick")));
        return;
    };
    ui.label(theme::title(&e.component));
    ui.label(theme::muted(format!("{} · {}", kind_label(&e.kind), e.path.display())).size(11.5));
    ui.add_space(6.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 150.0), Sense::hover());
    thumbnail(ui, rect, &lib.outlines);
    ui.add_space(6.0);
    egui::ScrollArea::vertical().id_salt("lib-details").max_height(170.0).show(ui, |ui| {
        ui.label(RichText::new(t!("lib-types")).strong());
        for (name, description) in &e.types {
            if description.is_empty() {
                ui.label(name);
            } else {
                ui.label(format!("{name} — {description}"));
            }
        }
        if !e.parameters.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new(t!("comp-params")).strong());
            ui.label(e.parameters.join(", "));
        }
        if !e.connectors.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new(t!("comp-connectors")).strong());
            ui.label(e.connectors.join(", "));
        }
    });
}

/// The component's plan outlines, fitted in `rect`.
fn thumbnail(ui: &Ui, rect: egui::Rect, outlines: &[Vec<DVec2>]) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, PALETTE.paper);
    let pts = outlines.iter().flatten();
    let Some((lo, hi)) = pts
        .fold(None, |acc: Option<(DVec2, DVec2)>, p| Some(acc.map_or((*p, *p), |(lo, hi)| (lo.min(*p), hi.max(*p)))))
    else {
        return;
    };
    let inner = rect.shrink(14.0);
    let extent = (hi - lo).max(DVec2::splat(1e-3));
    let k = (inner.width() as f64 / extent.x).min(inner.height() as f64 / extent.y);
    let center = (lo + hi) * 0.5;
    let to = |p: DVec2| {
        let q = (p - center) * k;
        Pos2::new(inner.center().x + q.x as f32, inner.center().y - q.y as f32)
    };
    let stroke = Stroke::new(1.0, egui::Color32::from_gray(50));
    for outline in outlines {
        let mut points: Vec<Pos2> = outline.iter().map(|p| to(*p)).collect();
        if let Some(first) = points.first().copied() {
            points.push(first);
        }
        painter.add(egui::Shape::line(points, stroke));
    }
}

/// Save one of the project's components into a library folder.
fn save_section(ui: &mut Ui, state: &mut AppState, lib: &mut Library) {
    let mut components: Vec<String> = state
        .project()
        .component_types()
        .filter_map(|t| match &t.data {
            cc_core::TypeData::Component { component, .. } => Some(component.clone()),
            _ => None,
        })
        .collect();
    components.sort();
    components.dedup();
    if components.is_empty() || state.prefs.library_folders.is_empty() {
        return;
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(theme::muted(t!("lib-save")));
        let selected = lib.save_component.clone().unwrap_or_default();
        egui::ComboBox::from_id_salt("lib-save-component").selected_text(selected).width(170.0).show_ui(ui, |ui| {
            for c in &components {
                ui.selectable_value(&mut lib.save_component, Some(c.clone()), c);
            }
        });
        ui.label(theme::muted(t!("lib-save-into")));
        let folder = lib.save_folder.clone().or_else(|| state.prefs.library_folders.first().cloned());
        let folder_name = folder.as_ref().map(|f| f.display().to_string()).unwrap_or_default();
        egui::ComboBox::from_id_salt("lib-save-folder").selected_text(folder_name).width(180.0).show_ui(ui, |ui| {
            for f in &state.prefs.library_folders {
                ui.selectable_value(&mut lib.save_folder, Some(f.clone()), f.display().to_string());
            }
        });
        let ready = lib.save_component.is_some() && folder.is_some();
        if ui.add_enabled(ready, egui::Button::new(t!("lib-save-button"))).clicked()
            && let (Some(component), Some(folder)) = (lib.save_component.clone(), folder)
        {
            let file: String =
                component.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == ' ' { c } else { '_' }).collect();
            let path = folder.join(format!("{}.ccomp", file.trim()));
            match state.session.run(io::ComponentSave { component, path: path.clone() }) {
                Ok(_) => {
                    state.message = t!("msg-exported", "path" => path.display());
                    refresh(state, lib);
                }
                Err(e) => state.dialogs.error = Some(t!("msg-component-save-failed", "error" => e.message)),
            }
        }
    });
}
