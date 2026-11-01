//! Imported CAD drawings in the app: the Import CAD and Import Image
//! commands (file dialog or files dropped on the window), and their
//! properties and layers. Every change runs an engine command
//! (`cad_import`, `image_import`, `element_set`, `cad_layers_set`).

use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use cc_core::cad::{CadColorMode, CadImport, CadUnit};
use cc_core::{Element, ElementId, ElementKind, ViewId};
use cc_engine::commands::cad;
use egui::{DragValue, Ui};
use std::path::PathBuf;

/// File extensions read as CAD drawings.
pub const EXTENSIONS: [&str; 2] = ["dwg", "dxf"];

/// The active view when it can hold an imported drawing.
pub fn cad_view(state: &AppState) -> Option<ViewId> {
    let id = state.active_view?;
    cc_views::hosts_cad(&state.project().view(id)?.kind).then_some(id)
}

/// Pick a DWG or DXF file and import it into the active view.
pub fn import_cad(state: &mut AppState) {
    if cad_view(state).is_none() {
        state.message = t!("why-cad-view");
        return;
    }
    let mut dialog = rfd::FileDialog::new().add_filter(t!("file-filter-cad"), &EXTENSIONS);
    if let Some(dir) = state.prefs.projects_dir() {
        dialog = dialog.set_directory(dir);
    }
    let Some(path) = dialog.pick_file() else { return };
    import_cad_path(state, path);
}

fn is_cad(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Import the drawing at `path` into the active view, select it and report
/// what was read.
pub fn import_cad_path(state: &mut AppState, path: PathBuf) {
    let Some(view) = cad_view(state) else {
        state.message = t!("why-cad-view");
        return;
    };
    let command = cad::CadImportFile {
        path: path.clone(),
        view: cc_engine::Ref::Id(view.0),
        position: None,
        align: Some("auto".into()),
        units: None,
        rotation: None,
        scale: None,
        color_mode: None,
        halftone: None,
    };
    match state.session.run(command) {
        Ok(out) => {
            if let Some(id) = out["id"].as_u64() {
                state.selection = std::collections::BTreeSet::from([ElementId(id)]);
            }
            let name = out["name"].as_str().unwrap_or_default().to_string();
            let count = out["primitives"].as_u64().unwrap_or(0);
            let layers = out["layers"].as_u64().unwrap_or(0);
            let mut message = t!("msg-cad-imported", "name" => name, "count" => count, "layers" => layers);
            let skipped = out["skipped_total"].as_u64().unwrap_or(0);
            if skipped > 0 {
                message = format!("{message} {}", t!("msg-cad-skipped", "count" => skipped));
            }
            if out["units_assumed"].as_bool() == Some(true) {
                let units = out["units"].as_str().and_then(CadUnit::from_code).unwrap_or_default();
                message = format!("{message} {}", t!("msg-cad-units-assumed", "units" => crate::i18n::tr(units.key())));
            }
            if out["truncated"].as_bool() == Some(true) {
                message = format!("{message} {}", t!("msg-cad-truncated"));
            }
            state.message = message;
        }
        Err(e) => state.dialogs.error = Some(t!("msg-cad-import-failed", "error" => e.message)),
    }
}

fn is_image(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| cad::IMAGE_EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Pick a picture or an SVG file and import it into the active view.
pub fn import_image(state: &mut AppState) {
    if cad_view(state).is_none() {
        state.message = t!("why-cad-view");
        return;
    }
    let mut dialog = rfd::FileDialog::new().add_filter(t!("file-filter-image"), &cad::IMAGE_EXTENSIONS);
    if let Some(dir) = state.prefs.projects_dir() {
        dialog = dialog.set_directory(dir);
    }
    let Some(path) = dialog.pick_file() else { return };
    import_image_path(state, path);
}

/// Import the picture or SVG at `path` into the active view, centered on
/// what it shows, and select it.
pub fn import_image_path(state: &mut AppState, path: PathBuf) {
    let Some(view) = cad_view(state) else {
        state.message = t!("why-cad-view");
        return;
    };
    let command = cad::ImageImport {
        path,
        view: cc_engine::Ref::Id(view.0),
        position: None,
        align: None,
        width: None,
        scale: None,
        rotation: None,
        halftone: None,
    };
    match state.session.run(command) {
        Ok(out) => {
            if let Some(id) = out["id"].as_u64() {
                state.selection = std::collections::BTreeSet::from([ElementId(id)]);
            }
            let name = out["name"].as_str().unwrap_or_default().to_string();
            state.message = t!("msg-image-imported", "name" => name);
        }
        Err(e) => state.dialogs.error = Some(t!("msg-cad-import-failed", "error" => e.message)),
    }
}

/// DWG and DXF drawings, pictures and SVG files dropped on the window go into the active view.
pub fn dropped_files(ctx: &egui::Context, state: &mut AppState) {
    if !state.project_open || state.show_home {
        return;
    }
    let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
    for path in dropped {
        if is_cad(&path) {
            import_cad_path(state, path);
        } else if is_image(&path) {
            import_image_path(state, path);
        }
    }
}

fn key(id: ElementId, field: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (id, "cad", field).hash(&mut h);
    h.finish()
}

/// Change the imported drawing `id` as `f` changes a copy (`element_set`).
fn set(state: &mut AppState, id: ElementId, field: &str, f: impl FnOnce(&mut CadImport)) {
    crate::engine_edit::element(state, id, Some(key(id, field)), |e: &mut Element| {
        if let ElementKind::CadImport(c) = &mut e.kind {
            f(c);
        }
    });
}

fn finish(state: &mut AppState, r: &egui::Response) {
    if r.drag_stopped() || r.lost_focus() {
        state.session.doc.break_merge();
    }
}

/// Properties of an imported drawing: file, placement, colors and layers.
pub fn props(ui: &mut Ui, state: &mut AppState, id: ElementId, c: &CadImport) {
    section(ui, "cad-import", t!("section-cad-import"), |ui| {
        readonly_row(ui, &t!("prop-cad-file"), c.name.clone());
        let view = state.project().view(c.view).map(|v| v.name.clone()).unwrap_or_default();
        readonly_row(ui, &t!("prop-view"), view);
        let mut units = c.units;
        row(ui, &t!("prop-cad-units"), |ui| {
            egui::ComboBox::from_id_salt(("cad-units", id.0)).selected_text(t_key(units.key())).show_ui(ui, |ui| {
                for u in CadUnit::ALL {
                    ui.selectable_value(&mut units, u, t_key(u.key()));
                }
            });
        });
        if units != c.units {
            set(state, id, "units", |x| x.units = units);
        }
        let mut scale = c.scale;
        let r = row(ui, &t!("prop-cad-scale"), |ui| {
            ui.add(DragValue::new(&mut scale).speed(0.01).range(0.000_001..=1_000_000.0).max_decimals(6))
        });
        if r.changed() && scale > 0.0 {
            set(state, id, "scale", |x| x.scale = scale);
        }
        finish(state, &r);
        let mut rot = c.rotation_deg;
        let r = row(ui, &t!("prop-rotation"), |ui| ui.add(DragValue::new(&mut rot).speed(1.0).suffix("°")));
        if r.changed() {
            set(state, id, "rotation", |x| x.rotation_deg = rot.rem_euclid(360.0));
        }
        finish(state, &r);
        for (label, axis) in [(t!("prop-cad-x"), 0usize), (t!("prop-cad-y"), 1)] {
            let mut v = c.position[axis];
            let r = row(ui, &label, |ui| ui.add(crate::properties::length_drag(&mut v).speed(0.01)));
            if r.changed() {
                set(state, id, if axis == 0 { "x" } else { "y" }, |x| x.position[axis] = v);
            }
            finish(state, &r);
        }
        let mut mode = c.color_mode;
        row(ui, &t!("prop-cad-colors"), |ui| {
            egui::ComboBox::from_id_salt(("cad-colors", id.0)).selected_text(t_key(mode.key())).show_ui(ui, |ui| {
                for m in CadColorMode::ALL {
                    ui.selectable_value(&mut mode, m, t_key(m.key()));
                }
            });
        });
        if mode != c.color_mode {
            set(state, id, "colors", |x| x.color_mode = mode);
        }
        let mut halftone = c.halftone;
        if row(ui, &t!("vg-halftone"), |ui| ui.checkbox(&mut halftone, "")).changed() {
            set(state, id, "halftone", |x| x.halftone = halftone);
        }
        readonly_row(ui, &t!("prop-cad-primitives"), c.drawing.prims.len().to_string());
    });
    section(ui, "cad-layers", t!("section-cad-layers"), |ui| {
        if c.drawing.layers.len() > 1 {
            let all_shown = c.hidden_layers.is_empty();
            let mut all = all_shown;
            if row(ui, &t!("prop-cad-all-layers"), |ui| ui.checkbox(&mut all, "")).changed() {
                layers_set(state, id, Vec::new(), all);
            }
        }
        for layer in c.drawing.layers.iter() {
            let mut visible = !c.hidden_layers.contains(&layer.name);
            let changed = row(ui, &layer.name, |ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                let [red, g, b] = layer.color;
                ui.painter().rect_filled(r, 2.0, egui::Color32::from_rgb(red, g, b));
                ui.checkbox(&mut visible, "").changed()
            });
            if changed {
                layers_set(state, id, vec![layer.name.clone()], visible);
            }
        }
    });
}

fn layers_set(state: &mut AppState, id: ElementId, layers: Vec<String>, visible: bool) {
    let command = cad::CadLayersSet { id: id.0, layers, visible };
    if let Err(e) = state.session.run(command) {
        state.message = e.message;
    }
}

fn t_key(key: &str) -> String {
    crate::i18n::tr(key)
}
