//! Home screen: open or create a project, recent projects and samples.

use crate::actions;
use crate::icons::{self, Icon};
use crate::state::{AppState, PendingAction};
use crate::t;
use crate::theme::{self, PALETTE};
use crate::thumbnails::{self, Job};
use cc_core::Project;
use egui::{Align2, Color32, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, vec2};
use std::path::{Path, PathBuf};

/// Bundled sample projects: (id, file contents).
const SAMPLES: &[(&str, &str)] = &[("casa_simples", include_str!("../../../examples/casa_simples.ccad"))];

const CARD: egui::Vec2 = egui::Vec2::new(200.0, 133.0);
const SIDEBAR_WIDTH: f32 = 250.0;

pub fn sample_project(name: &str) -> Option<Project> {
    let (_, text) = SAMPLES.iter().find(|(id, _)| *id == name)?;
    cc_core::from_ron(text).ok()
}

pub fn show(ui: &mut Ui, state: &mut AppState) {
    let side_panel = if crate::i18n::is_rtl() {
        egui::Panel::right("home-sidebar")
    } else {
        egui::Panel::left("home-sidebar")
    };
    side_panel
        .frame(egui::Frame::new().fill(PALETTE.surface).inner_margin(egui::Margin::symmetric(24, 24)))
        .resizable(false)
        .exact_size(SIDEBAR_WIDTH)
        .show(ui, |ui| sidebar(ui, state));
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(PALETTE.canvas).inner_margin(egui::Margin::symmetric(40, 32)))
        .show(ui, |ui| content(ui, state));
}

fn sidebar(ui: &mut Ui, state: &mut AppState) {
    // Back to the open project; without one there is nothing to return to.
    if state.project_open {
        let back = round_button(ui, Icon::ArrowLeft, &t!("home-back")).on_hover_text(t!("home-back"));
        if back.clicked() {
            state.show_home = false;
        }
        ui.add_space(28.0);
    }
    ui.label(theme::caption(t!("home-projects").to_uppercase()));
    ui.add_space(4.0);
    if side_button(ui, Icon::Open, &t!("action-open")).clicked() {
        actions::guarded(state, PendingAction::Open);
    }
    if side_button(ui, Icon::New, &t!("action-new")).clicked() {
        actions::guarded(state, PendingAction::New);
    }
    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);
    nav_item(ui, Icon::Open, &t!("home-recent"), true);
    // App settings at the bottom of the sidebar.
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        let command = crate::commands::UiCommand::Preferences;
        let label = command.label(state);
        if side_button(ui, Icon::Settings, &label).on_hover_text(command.shortcut().unwrap_or_default()).clicked() {
            crate::preferences::open(state, crate::preferences::Section::General);
        }
    });
}

fn round_button(ui: &mut Ui, icon: Icon, label: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let color = if response.hovered() { PALETTE.accent_text } else { PALETTE.text_muted };
    ui.painter().circle_stroke(rect.center(), 16.0, Stroke::new(1.5, color));
    icons::paint(ui.painter(), rect.shrink(11.0), icon, color);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn side_button(ui: &mut Ui, icon: Icon, label: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let painter = ui.painter();
    let fill = if response.hovered() { PALETTE.hover } else { PALETTE.canvas };
    painter.rect_filled(rect, theme::RADIUS as f32, fill);
    let border = if response.hovered() { PALETTE.border_strong } else { PALETTE.border };
    painter.rect_stroke(rect, theme::RADIUS as f32, Stroke::new(1.0, border), StrokeKind::Inside);
    let icon_rect = Rect::from_center_size(egui::pos2(rect.left() + 20.0, rect.center().y), vec2(15.0, 15.0));
    icons::paint(painter, icon_rect, icon, PALETTE.text_muted);
    let font = egui::FontId::proportional(13.0);
    painter.text(egui::pos2(rect.left() + 36.0, rect.center().y), Align2::LEFT_CENTER, label, font, PALETTE.text);
    ui.add_space(6.0);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn nav_item(ui: &mut Ui, icon: Icon, label: &str, selected: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect.expand2(vec2(24.0, 0.0)), 0.0, PALETTE.panel);
    }
    let color = if selected { PALETTE.accent_text } else { PALETTE.text };
    let icon_rect = Rect::from_center_size(egui::pos2(rect.left() + 8.0, rect.center().y), vec2(15.0, 15.0));
    icons::paint(painter, icon_rect, icon, color);
    let font = egui::FontId::proportional(13.0);
    painter.text(egui::pos2(rect.left() + 26.0, rect.center().y), Align2::LEFT_CENTER, label, font, color);
}

fn content(ui: &mut Ui, state: &mut AppState) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if state.updates.available.is_some() {
            egui::Frame::new()
                .fill(PALETTE.accent_soft)
                .corner_radius(theme::RADIUS)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| crate::update::notice(ui, &mut state.updates));
            ui.add_space(16.0);
        }
        ui.label(RichText::new(t!("home-recent")).family(theme::semibold()).size(20.0).color(PALETTE.text));
        ui.add_space(20.0);

        section_bar(ui, &t!("home-projects"));
        let recent = state.recent.clone();
        if recent.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new(t!("home-empty")).color(PALETTE.text_faint));
        }
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(18.0, 18.0);
            for path in &recent {
                let exists = path.exists();
                let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let thumb = if exists { thumbnails::project_file(path) } else { None };
                let r = card(ui, &name, thumb.as_deref(), exists);
                let r = r.on_hover_text(if exists { path.display().to_string() } else { t!("home-missing") });
                if r.clicked() && exists {
                    actions::guarded(state, PendingAction::OpenPath(path.clone()));
                }
                r.context_menu(|ui| {
                    if ui.button(t!("home-remove")).clicked() {
                        remove = Some(path.clone());
                        ui.close();
                    }
                });
            }
        });
        if let Some(path) = remove {
            state.recent.retain(|p| *p != path);
        }

        ui.add_space(28.0);
        section_bar(ui, &t!("home-samples"));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(18.0, 18.0);
            for (id, _) in SAMPLES {
                let Some(project) = sample_project(id) else { continue };
                let file = thumbnails::sample_file(id);
                if file.as_ref().is_some_and(|f| !f.exists()) && state.requested_samples.insert(id) {
                    state.thumbnail_jobs.push(Job::Sample(id));
                }
                if card(ui, &project.info.name, file.as_deref(), true).clicked() {
                    actions::guarded(state, PendingAction::OpenSample(id));
                }
            }
        });
    });
}

fn section_bar(ui: &mut Ui, title: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, PALETTE.surface);
    let font = egui::FontId::new(13.0, theme::semibold());
    let pos = egui::pos2(rect.left() + 20.0, rect.center().y);
    ui.painter().text(pos, Align2::LEFT_CENTER, title.to_uppercase(), font, PALETTE.text);
    ui.add_space(18.0);
}

/// A project card: thumbnail (or placeholder) and name.
fn card(ui: &mut Ui, name: &str, thumbnail: Option<&Path>, enabled: bool) -> Response {
    let label_h = 26.0;
    let (rect, response) = ui.allocate_exact_size(vec2(CARD.x, CARD.y + label_h), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
    let image_rect = Rect::from_min_size(rect.min, CARD);
    let painter = ui.painter();
    painter.rect_filled(image_rect, 2.0, Color32::WHITE);
    match thumbnail.and_then(|f| load_thumbnail(ui.ctx(), f)) {
        Some(texture) => {
            let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            let tint = if enabled { Color32::WHITE } else { Color32::from_white_alpha(90) };
            painter.image(texture.id(), image_rect.shrink(1.0), uv, tint);
        }
        None => {
            let color = if enabled { PALETTE.border_strong } else { PALETTE.border };
            icons::paint(painter, Rect::from_center_size(image_rect.center(), vec2(44.0, 44.0)), Icon::View3D, color);
        }
    }
    let hovered = enabled && response.hovered();
    let border = if hovered { Stroke::new(1.5, PALETTE.accent) } else { Stroke::new(1.0, PALETTE.border_strong) };
    painter.rect_stroke(image_rect, 2.0, border, StrokeKind::Inside);
    let color = if enabled { PALETTE.text } else { PALETTE.text_faint };
    let font = egui::FontId::proportional(13.0);
    let galley = painter.layout(name.to_string(), font, color, CARD.x);
    let clip = Rect::from_min_size(egui::pos2(rect.left(), image_rect.bottom() + 6.0), vec2(CARD.x, label_h));
    ui.painter_at(clip).galley(clip.min, galley, color);
    if enabled { response.on_hover_cursor(egui::CursorIcon::PointingHand) } else { response }
}

/// Load a thumbnail PNG as a texture, cached per file and modification time.
fn load_thumbnail(ctx: &egui::Context, file: &Path) -> Option<egui::TextureHandle> {
    let modified = std::fs::metadata(file).ok()?.modified().ok()?;
    let id = egui::Id::new(("thumbnail", file, modified));
    if let Some(texture) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(texture);
    }
    let icon = eframe::icon_data::from_png_bytes(&std::fs::read(file).ok()?).ok()?;
    let image = egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba);
    let texture = ctx.load_texture(file.to_string_lossy(), image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    Some(texture)
}

/// Remember `path` at the top of the recent projects list, which keeps at most `max` (at least one).
pub fn push_recent(recent: &mut Vec<PathBuf>, path: &Path, max: usize) {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    recent.retain(|p| *p != path);
    recent.insert(0, path);
    recent.truncate(max.max(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_load() {
        for (id, _) in SAMPLES {
            assert!(sample_project(id).is_some(), "{id}");
        }
    }

    #[test]
    fn recent_list_is_deduplicated_and_capped() {
        let mut recent = Vec::new();
        for i in 0..15 {
            push_recent(&mut recent, Path::new(&format!("p{i}.ccad")), 12);
        }
        push_recent(&mut recent, Path::new("p10.ccad"), 12);
        assert_eq!(recent.len(), 12);
        assert!(recent[0].ends_with("p10.ccad"));
        assert_eq!(recent.iter().filter(|p| p.ends_with("p10.ccad")).count(), 1);
        // A smaller count (the preference) trims the list.
        push_recent(&mut recent, Path::new("p3.ccad"), 5);
        assert_eq!(recent.len(), 5);
        push_recent(&mut recent, Path::new("p4.ccad"), 0);
        assert_eq!(recent.len(), 1, "always keeps the latest");
    }
}
