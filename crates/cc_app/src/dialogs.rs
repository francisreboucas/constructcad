//! Application dialogs, all built on [`crate::dialog::Dialog`].

use crate::actions;
use crate::dialog::{Dialog, Kind, session};
use crate::state::AppState;
use crate::t;
use crate::theme::{self, PALETTE};
use egui::{Context, RichText, Ui};
use std::hash::{Hash, Hasher};

fn key(parts: impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut h);
    h.finish()
}

fn done(state: &mut AppState, r: &egui::Response) {
    if r.drag_stopped() || r.lost_focus() {
        state.session.doc.break_merge();
    }
}

pub fn show(ctx: &Context, state: &mut AppState) {
    project_info(ctx, state);
    crate::units_dialog::show(ctx, state);
    levels(ctx, state);
    crate::material_browser::show(ctx, state);
    crate::type_dialog::show(ctx, state);
    crate::component_editor::show(ctx, state);
    crate::schedule_ui::dialog(ctx, state);
    crate::systems_dialog::show(ctx, state);
    crate::systems_dialog::sizing(ctx, state);
    crate::graphics_dialog::show(ctx, state);
    crate::phases_dialog::show(ctx, state);
    crate::library::show(ctx, state);
    crate::plugins_ui::drain_messages(state);
    crate::plugins_ui::show(ctx, state);
    // After the library window, which opens it over itself.
    crate::preferences::show(ctx, state);
    confirm(ctx, state);
    error(ctx, state);
    about(ctx, state);
    update_report(ctx, state);
    update_install(ctx, state);
}

/// Footer of a data dialog: OK keeps the edits, Cancel (or Esc / close) undoes them.
/// Returns true when the dialog closes.
pub(crate) fn ok_cancel(state: &mut AppState, response: &crate::dialog::DialogResponse, ok: usize) -> bool {
    match response.clicked {
        Some(i) if i == ok => {
            session::commit(state);
            true
        }
        Some(_) => {
            session::cancel(state);
            true
        }
        None if response.dismissed => {
            session::cancel(state);
            true
        }
        None => false,
    }
}

/// Label of a form row.
fn form_label(ui: &mut Ui, label: &str) {
    ui.label(RichText::new(label).color(PALETTE.text_muted));
}

fn project_info(ctx: &Context, state: &mut AppState) {
    if !state.dialogs.project_info {
        return;
    }
    session::begin(state);
    let mut ok = 0;
    let response = Dialog::new("project-info", t!("dlg-project-info")).width(440.0).show(
        ctx,
        |ui| {
            let info = state.project().info.clone();
            egui::Grid::new("info").num_columns(2).spacing([14.0, 8.0]).min_col_width(110.0).show(ui, |ui| {
                let fields = [
                    (t!("info-name"), "name", info.name),
                    (t!("info-number"), "number", info.number),
                    (t!("info-client"), "client", info.client),
                    (t!("info-address"), "address", info.address),
                    (t!("info-author"), "author", info.author),
                ];
                for (label, field, value) in fields {
                    form_label(ui, &label);
                    let mut text = value;
                    let r = crate::table::live_text(ui, ("project-info", field), &mut text, |ui, t| {
                        ui.add(egui::TextEdit::singleline(t).desired_width(300.0))
                    });
                    if r.changed() {
                        crate::engine_edit::info(state, Some(key(("info", field))), |info| match field {
                            "name" => info.name = text,
                            "number" => info.number = text,
                            "client" => info.client = text,
                            "address" => info.address = text,
                            _ => info.author = text,
                        });
                    }
                    done(state, &r);
                    ui.end_row();
                }
            });
            project_fields(ui, state);
        },
        |f| {
            ok = f.primary(t!("action-ok"));
            f.secondary(t!("action-cancel"));
        },
    );
    if ok_cancel(state, &response, ok) {
        state.dialogs.project_info = false;
    }
}

/// Add a project field with an unused name (`Field 1`, `Field 2`…) and no value.
pub(crate) fn add_project_field(state: &mut AppState) -> bool {
    let base = t!("info-new-field");
    let info = &state.project().info;
    let Some(name) = (1..=10_000).map(|i| format!("{base} {i}")).find(|n| info.field(n).is_none()) else {
        return false;
    };
    crate::engine_edit::info(state, None, |i| i.fields.push(cc_core::ProjectField { name, value: String::new() }))
}

/// Rename project field `old` (refused when the new name is empty or taken).
pub(crate) fn rename_project_field(state: &mut AppState, old: &str, new: &str) -> bool {
    let new = new.trim().to_string();
    if new.is_empty() || state.project().info.field(&new).is_some() {
        state.message = t!("msg-field-name-taken");
        return false;
    }
    crate::engine_edit::info(state, None, |i| {
        if let Some(f) = i.fields.iter_mut().find(|f| f.name == old) {
            f.name = new;
        }
    })
}

/// Remove project field `name`.
pub(crate) fn remove_project_field(state: &mut AppState, name: &str) -> bool {
    crate::engine_edit::info(state, None, |i| i.fields.retain(|f| f.name != name))
}

/// The project fields table: name (committed on Enter), value (as typed), ×.
fn project_fields(ui: &mut Ui, state: &mut AppState) {
    ui.add_space(12.0);
    ui.label(theme::title(t!("info-project-fields")));
    ui.add_space(4.0);
    let fields = state.project().info.fields.clone();
    egui::Grid::new("info-fields").num_columns(3).spacing([10.0, 6.0]).show(ui, |ui| {
        form_label(ui, &t!("prop-name"));
        form_label(ui, &t!("prop-value"));
        ui.end_row();
        for f in &fields {
            if let Some(new) = crate::table::commit_text(ui, ("info-field-name", &f.name), &f.name, 140.0, "") {
                rename_project_field(state, &f.name, &new);
            }
            let mut value = f.value.clone();
            let r = crate::table::live_text(ui, ("info-field-value", &f.name), &mut value, |ui, t| {
                ui.add(egui::TextEdit::singleline(t).desired_width(220.0))
            });
            if r.changed() {
                let name = f.name.clone();
                crate::engine_edit::info(state, Some(key(("info-field", &f.name))), |i| {
                    if let Some(x) = i.fields.iter_mut().find(|x| x.name == name) {
                        x.value = value;
                    }
                });
            }
            done(state, &r);
            if ui.small_button("×").on_hover_text(t!("action-delete")).clicked() {
                remove_project_field(state, &f.name);
            }
            ui.end_row();
        }
    });
    if ui.button(format!("+ {}", t!("info-add-field"))).clicked() {
        add_project_field(state);
    }
}

fn levels(ctx: &Context, state: &mut AppState) {
    if !state.dialogs.levels {
        return;
    }
    session::begin(state);
    let mut ok = 0;
    let response = Dialog::new("levels", t!("dlg-levels")).width(460.0).show(
        ctx,
        |ui| {
            let levels = state.project().levels.clone();
            egui::Grid::new("levels").num_columns(3).striped(true).spacing([12.0, 6.0]).show(ui, |ui| {
                form_label(ui, &t!("prop-name"));
                form_label(ui, &t!("level-elevation"));
                ui.label("");
                ui.end_row();
                for level in levels.iter().rev() {
                    let mut name = level.name.clone();
                    let r = crate::table::live_text(ui, ("level-name", level.id), &mut name, |ui, t| {
                        ui.add_sized([220.0, 22.0], egui::TextEdit::singleline(t))
                    });
                    if r.changed() {
                        crate::engine_edit::level(state, level.id, Some(key((level.id, "name"))), |l| l.name = name);
                    }
                    done(state, &r);
                    let mut elev = level.elevation;
                    let r = ui.add(crate::properties::length_drag(&mut elev).speed(0.01));
                    if r.changed() {
                        crate::engine_edit::level(state, level.id, Some(key((level.id, "elev"))), |l| {
                            l.elevation = elev;
                        });
                    }
                    done(state, &r);
                    if ui.add_enabled(levels.len() > 1, egui::Button::new(t!("action-delete"))).clicked() {
                        state.dialogs.delete_level = Some(level.id);
                    }
                    ui.end_row();
                }
            });
            ui.add_space(4.0);
            if ui.button(t!("level-add")).clicked() {
                actions::add_level(state);
            }
        },
        |f| {
            ok = f.primary(t!("action-ok"));
            f.secondary(t!("action-cancel"));
        },
    );
    if ok_cancel(state, &response, ok) {
        state.dialogs.levels = false;
    }
    delete_level(ctx, state);
}

/// Confirmation before deleting a level (and the views on it), shown over the levels dialog.
fn delete_level(ctx: &Context, state: &mut AppState) {
    let Some(level) = state.dialogs.delete_level else { return };
    let project = state.project();
    let name = project.level_name(level).to_string();
    let in_use = project.elements.values().any(|e| e.level == level);
    let views = project.views.values().filter(|v| v.level() == Some(level)).count();
    let mut delete = None;
    let response = Dialog::new("delete-level", t!("dlg-delete-level")).kind(Kind::Warning).width(380.0).show(
        ctx,
        |ui| {
            if in_use {
                ui.label(t!("msg-level-in-use"));
            } else {
                ui.label(t!("dlg-delete-level-text", "level" => name, "views" => views));
            }
        },
        |f| {
            if in_use {
                f.primary(t!("action-ok"));
            } else {
                delete = Some(f.danger(t!("action-delete")));
                f.secondary(t!("action-cancel"));
            }
        },
    );
    if response.clicked.is_some() && response.clicked == delete {
        actions::delete_level(state, level);
    }
    if response.clicked.is_some() || response.dismissed {
        state.dialogs.delete_level = None;
    }
}

pub(crate) fn material_combo(
    ui: &mut Ui,
    state: &AppState,
    id: impl Hash + std::fmt::Debug,
    current: &mut String,
) -> bool {
    let before = current.clone();
    egui::ComboBox::from_id_salt(id).selected_text(current.as_str()).width(140.0).show_ui(ui, |ui| {
        for m in &state.project().materials {
            ui.selectable_value(current, m.name.clone(), &m.name);
        }
    });
    *current != before
}

fn confirm(ctx: &Context, state: &mut AppState) {
    let Some(action) = state.dialogs.confirm.clone() else { return };
    let (mut save, mut dont_save) = (0, 0);
    let response = Dialog::new("confirm", t!("dlg-unsaved")).kind(Kind::Warning).width(400.0).show(
        ctx,
        |ui| {
            ui.label(t!("dlg-unsaved-text"));
        },
        |f| {
            save = f.primary(t!("action-save"));
            dont_save = f.secondary(t!("action-dont-save"));
            f.secondary(t!("action-cancel"));
        },
    );
    match response.clicked {
        Some(i) if i == save => {
            state.dialogs.confirm = None;
            if actions::save(state, false) {
                actions::run_pending(state, action);
            }
        }
        Some(i) if i == dont_save => {
            state.dialogs.confirm = None;
            actions::run_pending(state, action);
        }
        Some(_) => state.dialogs.confirm = None,
        None if response.dismissed => state.dialogs.confirm = None,
        None => {}
    }
}

fn error(ctx: &Context, state: &mut AppState) {
    let Some(msg) = state.dialogs.error.clone() else { return };
    let response = Dialog::new("error", t!("dlg-error")).kind(Kind::Error).width(400.0).show(
        ctx,
        |ui| {
            ui.label(msg);
        },
        |f| {
            f.primary(t!("action-ok"));
        },
    );
    if response.clicked.is_some() || response.dismissed {
        state.dialogs.error = None;
    }
}

fn about(ctx: &Context, state: &mut AppState) {
    if !state.dialogs.about {
        return;
    }
    let repository = env!("CARGO_PKG_REPOSITORY");
    let mut open_repo = 0;
    let mut check = 0;
    let response = Dialog::new("about", t!("dlg-about")).width(440.0).show(
        ctx,
        |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(64.0, 64.0), egui::Sense::hover());
                if let Some(logo) = theme::logo(ui.ctx()) {
                    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                    ui.painter().image(logo.id(), rect, uv, egui::Color32::WHITE);
                }
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.add_space(8.0);
                    ui.label(RichText::new("ConstructCAD").family(theme::semibold()).size(22.0).color(PALETTE.text));
                    let version = format!("{} {}", t!("about-version"), env!("CARGO_PKG_VERSION"));
                    ui.label(RichText::new(version).color(PALETTE.text_muted));
                });
            });
            ui.add_space(8.0);
            ui.label(t!("about-text"));
            ui.label(RichText::new(t!("about-familiar")).color(PALETTE.text_muted));
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);
            egui::Grid::new("about-info").num_columns(2).striped(false).spacing([16.0, 6.0]).show(ui, |ui| {
                ui.label(theme::muted(t!("about-author")));
                ui.label(env!("CARGO_PKG_AUTHORS"));
                ui.end_row();
                ui.label(theme::muted(t!("about-license-label")));
                ui.hyperlink_to("GPL-3.0-or-later", "https://www.gnu.org/licenses/gpl-3.0.html");
                ui.end_row();
                ui.label(theme::muted(t!("about-repository")));
                ui.hyperlink_to(repository.trim_start_matches("https://"), repository);
                ui.end_row();
            });
            ui.add_space(8.0);
            ui.label(theme::title(t!("about-credits")));
            for line in [t!("about-built-with"), t!("about-font-credit"), t!("about-icon-credit")] {
                ui.label(RichText::new(line).size(12.0).color(PALETTE.text_muted));
            }
            ui.add_space(8.0);
            ui.label(RichText::new(t!("about-independence")).size(11.5).color(PALETTE.text_faint));
        },
        |f| {
            open_repo = f.extra(t!("about-open-repository"));
            check = f.extra(t!("action-check-updates"));
            f.primary(t!("action-close"));
        },
    );
    match response.clicked {
        Some(i) if i == open_repo => ctx.open_url(egui::OpenUrl::new_tab(repository)),
        Some(i) if i == check => {
            // The report replaces the About dialog.
            state.dialogs.about = false;
            state.updates.start(ctx, true);
        }
        Some(_) => state.dialogs.about = false,
        None if response.dismissed => state.dialogs.about = false,
        None => {}
    }
}

/// The result of a check for updates the user asked for (and the wait for it).
fn update_report(ctx: &Context, state: &mut AppState) {
    use crate::update::Outcome;
    let updates = &mut state.updates;
    if !updates.checking() && updates.report.is_none() {
        return;
    }
    let report = updates.report.clone();
    let current = env!("CARGO_PKG_VERSION");
    let (mut download, mut skip, mut now) = (usize::MAX, usize::MAX, usize::MAX);
    let installable = matches!(&report, Some(Outcome::Available(r)) if crate::update::plan(r).is_some());
    let mut dialog = Dialog::new("update-check", t!("action-check-updates")).width(420.0);
    if matches!(report, Some(Outcome::Failed | Outcome::Disabled)) {
        dialog = dialog.kind(Kind::Warning);
    }
    let response = dialog.show(
        ctx,
        |ui| match &report {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t!("update-checking"));
                });
            }
            Some(Outcome::UpToDate) => {
                ui.label(t!("update-up-to-date", "version" => current));
            }
            Some(Outcome::Available(release)) => {
                ui.label(t!("update-available", "version" => release.version.to_string()));
                ui.label(theme::muted(t!("update-current", "version" => current)));
            }
            Some(Outcome::Failed) => {
                ui.label(t!("update-failed"));
            }
            Some(Outcome::Disabled) => {
                ui.label(t!("update-disabled", "variable" => crate::update::NO_CHECK_ENV));
            }
        },
        |f| {
            if matches!(report, Some(Outcome::Available(_))) {
                if installable {
                    now = f.primary(t!("update-now"));
                    download = f.secondary(t!("update-download"));
                } else {
                    download = f.primary(t!("update-download"));
                }
                skip = f.extra(t!("update-skip"));
                f.secondary(t!("action-close"));
            } else {
                f.primary(t!("action-close"));
            }
        },
    );
    match response.clicked {
        Some(i) if i == now => {
            if let Some(Outcome::Available(release)) = &report {
                updates.update_now(ctx, release);
            }
            updates.cancel_report();
        }
        Some(i) if i == download => {
            if let Some(Outcome::Available(release)) = &report {
                ctx.open_url(egui::OpenUrl::new_tab(&release.url));
            }
            updates.dismiss();
            updates.cancel_report();
        }
        Some(i) if i == skip => {
            updates.skip();
            updates.cancel_report();
        }
        Some(_) => updates.cancel_report(),
        None if response.dismissed => updates.cancel_report(),
        None => {}
    }
}

/// Update Now: the download's progress, then what is left to do (restart,
/// run the installer, drag the app) or why it failed.
fn update_install(ctx: &Context, state: &mut AppState) {
    use crate::update::Install;
    use crate::update_install::Next;
    use std::sync::atomic::Ordering;
    let Some(install) = &state.updates.install else { return };
    let mut ids = [usize::MAX; 3];
    let mut dialog = Dialog::new("update-install", t!("dlg-update-install")).width(440.0);
    if matches!(install, Install::Failed(..)) {
        dialog = dialog.kind(Kind::Warning);
    }
    let response = dialog.show(
        ctx,
        |ui| match install {
            Install::Running(version, _, running) => {
                ui.label(t!("update-downloading", "version" => version.to_string()));
                let (done, total) =
                    (running.progress.done.load(Ordering::Relaxed), running.progress.total.load(Ordering::Relaxed));
                let mb = |b: u64| format!("{:.1}", b as f64 / 1_048_576.0);
                let bar = if total > 0 {
                    egui::ProgressBar::new(done as f32 / total as f32).text(format!("{} / {} MB", mb(done), mb(total)))
                } else {
                    egui::ProgressBar::new(0.0).animate(true).text(format!("{} MB", mb(done)))
                };
                ui.add(bar);
                ui.label(theme::muted(t!("update-verify-note")));
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            }
            Install::Ready(version, Next::Restart(_)) => {
                ui.label(t!("update-installed-restart", "version" => version.to_string()));
            }
            Install::Ready(version, Next::Installer(_)) => {
                ui.label(t!("update-installer-ready", "version" => version.to_string()));
            }
            Install::Ready(_, Next::Opened) => {
                ui.label(t!("update-dmg-opened"));
            }
            Install::Command(command, _) => {
                ui.label(t!("update-deb-command"));
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(RichText::new(command).monospace()).wrap());
                    if ui.small_button(t!("update-copy")).clicked() {
                        ui.ctx().copy_text(command.clone());
                    }
                });
            }
            Install::Failed(why, _) => {
                ui.label(t!("update-install-failed"));
                ui.label(theme::muted(why.clone()));
            }
        },
        |f| match install {
            Install::Running(..) => {
                ids[0] = f.secondary(t!("action-cancel"));
            }
            Install::Ready(_, Next::Restart(_)) => {
                ids[0] = f.primary(t!("update-restart"));
                ids[1] = f.secondary(t!("update-later"));
            }
            Install::Ready(_, Next::Installer(_)) => {
                ids[0] = f.primary(t!("update-install-close"));
                ids[1] = f.secondary(t!("update-install-on-exit"));
            }
            Install::Ready(_, Next::Opened) => {
                ids[1] = f.primary(t!("action-close"));
            }
            Install::Command(..) | Install::Failed(..) => {
                ids[2] = f.primary(t!("update-download"));
                ids[1] = f.secondary(t!("action-close"));
            }
        },
    );
    let Some(clicked) = response.clicked.or(response.dismissed.then_some(usize::MAX - 1)) else { return };
    let updates = &mut state.updates;
    match updates.install.take() {
        Some(Install::Running(version, url, running)) => {
            if clicked == ids[0] || clicked == usize::MAX - 1 {
                running.progress.cancel.store(true, Ordering::Relaxed);
                // The thread ends on its own; its result is no longer read.
            } else {
                updates.install = Some(Install::Running(version, url, running));
            }
        }
        Some(Install::Ready(_, next @ (Next::Restart(_) | Next::Installer(_)))) => {
            let now = clicked == ids[0];
            // The installer runs when the app closes, now or later; a replaced
            // copy restarts only when asked (its files are already in place).
            if now || matches!(next, Next::Installer(_)) {
                updates.on_exit = Some(next);
            }
            if now {
                actions::guarded(state, crate::state::PendingAction::Quit);
            }
        }
        Some(Install::Command(_, url) | Install::Failed(_, url)) if clicked == ids[2] => {
            ctx.open_url(egui::OpenUrl::new_tab(&url));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_fields_are_added_renamed_and_removed() {
        let mut state = AppState::new(cc_core::Document::new(crate::actions::new_project()));
        assert!(add_project_field(&mut state));
        assert!(add_project_field(&mut state));
        let names: Vec<String> = state.project().info.fields.iter().map(|f| f.name.clone()).collect();
        let base = t!("info-new-field");
        assert_eq!(names, [format!("{base} 1"), format!("{base} 2")]);
        assert!(rename_project_field(&mut state, &names[0], "Engineer"));
        assert_eq!(state.project().info.field("Engineer"), Some(""));
        // Empty and taken names are refused.
        assert!(!rename_project_field(&mut state, "Engineer", " "));
        assert!(!rename_project_field(&mut state, "Engineer", &names[1]));
        assert_eq!(state.message, t!("msg-field-name-taken"));
        assert!(remove_project_field(&mut state, "Engineer"));
        assert_eq!(state.project().info.fields.len(), 1);
    }
}
