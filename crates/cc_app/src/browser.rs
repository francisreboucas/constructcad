//! Project Browser: the tree of views, schedules and sheets.

use crate::actions;
use crate::i18n::tr;
use crate::icons::{self, Icon};
use crate::state::AppState;
use crate::t;
use crate::theme::{self, PALETTE};
use crate::tools::Tool;
use cc_core::{Category, ViewCategory, ViewId, ViewKind};
use egui::{Rect, Response, Sense, Ui, vec2};

/// A view dragged from the browser (dropped on a sheet, it is placed there).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewDrag(pub ViewId);

const ROW_HEIGHT: f32 = 21.0;
const INDENT: f32 = 20.0;

pub fn category_icon(category: ViewCategory) -> Icon {
    match category {
        ViewCategory::FloorPlan => Icon::FloorPlan,
        ViewCategory::CeilingPlan => Icon::CeilingPlan,
        ViewCategory::ThreeD => Icon::View3D,
        ViewCategory::Elevation => Icon::Elevation,
        ViewCategory::Section => Icon::Section,
        ViewCategory::Legend => Icon::Legend,
        ViewCategory::Schedule => Icon::Schedule,
        ViewCategory::Diagram => Icon::Systems,
        ViewCategory::Sheet => Icon::Sheet,
    }
}

pub fn show(ui: &mut Ui, state: &mut AppState) {
    theme::panel_header(ui, &t!("browser-title"), Some(&state.project().info.name));
    egui::ScrollArea::vertical().id_salt("browser").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(4.0);
        for category in ViewCategory::ALL {
            let id = ui.make_persistent_id(("browser-open", category));
            let mut open = ui.data_mut(|d| *d.get_persisted_mut_or(id, !matches!(category, ViewCategory::Legend)));
            let count = state.project().views_of(category).count();
            let label = format!("{}  ", tr(category.key()));
            let row = tree_row(ui, 0, Some(open), false, category_icon(category), &label, Some(count), false);
            if add_button(ui, state, category, &row) {
                open = true;
            } else if row.clicked() {
                open = !open;
            }
            ui.data_mut(|d| d.insert_persisted(id, open));
            if open {
                category_items(ui, state, category);
            }
        }
        ui.add_space(6.0);
    });
}

/// Row layout: left padding, then the chevron column, then one indent per depth.
const PAD: f32 = 10.0;
const CHEVRON: f32 = 15.0;

/// Left edge of the icon of a row at `depth`.
fn icon_x(left: f32, depth: usize) -> f32 {
    left + PAD + CHEVRON + depth as f32 * INDENT
}

/// One row of the tree. `expander` is `Some(open)` for collapsible rows;
/// `last` tells child rows whether they end their branch (for the guide lines).
#[allow(clippy::too_many_arguments)]
fn tree_row(
    ui: &mut Ui,
    depth: usize,
    expander: Option<bool>,
    last: bool,
    icon: Icon,
    label: &str,
    count: Option<usize>,
    selected: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label));
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = ui.painter();
    let bg = rect.shrink2(vec2(4.0, 1.0));
    if selected {
        painter.rect_filled(bg, theme::RADIUS as f32, PALETTE.accent_soft);
    } else if response.hovered() {
        painter.rect_filled(bg, theme::RADIUS as f32, PALETTE.hover);
    }
    let cy = rect.center().y;
    let mut x = icon_x(rect.left(), depth);
    if let Some(open) = expander {
        let chevron = Rect::from_center_size(egui::pos2(x - CHEVRON + 5.0, cy), vec2(10.0, 10.0));
        icons::paint(painter, chevron, if open { Icon::ChevronDown } else { Icon::ChevronRight }, PALETTE.text_faint);
    }
    if depth > 0 {
        // Guide from the parent's icon down to this row, with a tick into it.
        let gx = icon_x(rect.left(), depth - 1) + 7.0;
        let guide = egui::Stroke::new(1.0, PALETTE.border_strong);
        let bottom = if last { cy } else { rect.bottom() };
        painter.vline(gx, rect.top()..=bottom, guide);
        painter.hline(gx..=x - 3.0, cy, guide);
    }
    let color = if selected { PALETTE.accent_text } else { PALETTE.text };
    let icon_color = if selected { PALETTE.accent_text } else { PALETTE.text_muted };
    icons::paint(painter, Rect::from_center_size(egui::pos2(x + 7.0, cy), vec2(14.0, 14.0)), icon, icon_color);
    x += 21.0;
    let family = if expander.is_some() { theme::semibold() } else { egui::FontFamily::Proportional };
    let galley = painter.layout_no_wrap(label.to_string(), egui::FontId::new(12.5, family), color);
    let width = galley.size().x;
    painter.galley(egui::pos2(x, cy - galley.size().y / 2.0), galley, color);
    if let Some(n) = count {
        let font = egui::FontId::proportional(11.0);
        painter.text(egui::pos2(x + width, cy), egui::Align2::LEFT_CENTER, n.to_string(), font, PALETTE.text_faint);
    }
    response
}

fn category_items(ui: &mut Ui, state: &mut AppState, category: ViewCategory) {
    let mut views: Vec<(ViewId, String)> = state
        .project()
        .views_of(category)
        .map(|v| {
            let label = match &v.kind {
                ViewKind::Sheet { number, .. } => format!("{number} - {}", v.name),
                _ => v.name.clone(),
            };
            (v.id, label)
        })
        .collect();
    if matches!(category, ViewCategory::FloorPlan | ViewCategory::CeilingPlan) {
        // Order plans by level elevation like the levels themselves.
        let project = state.project();
        views.sort_by(|a, b| {
            let elev =
                |id: ViewId| project.view(id).and_then(|v| v.level()).map_or(0.0, |l| project.level_elevation(l));
            elev(a.0).total_cmp(&elev(b.0)).then(a.1.cmp(&b.1))
        });
    } else {
        views.sort_by(|a, b| a.1.cmp(&b.1));
    }

    let n = views.len();
    for (i, (id, label)) in views.into_iter().enumerate() {
        if let Some((rid, text)) = state.rename.as_mut().filter(|(rid, _)| *rid == id) {
            let rid = *rid;
            let finished = ui
                .horizontal(|ui| {
                    ui.add_space(PAD + CHEVRON + INDENT + 21.0);
                    let r = ui.add(egui::TextEdit::singleline(text).desired_width(ui.available_width() - 8.0));
                    // Focus once when renaming starts: Enter, Esc or a click
                    // elsewhere then ends it (asking every frame kept the
                    // field focused for good, and every other field locked).
                    let started = egui::Id::new(("rename-focused", rid.0));
                    if !ui.data(|d| d.get_temp::<bool>(started).unwrap_or(false)) {
                        ui.data_mut(|d| d.insert_temp(started, true));
                        r.request_focus();
                    }
                    let done = r.lost_focus();
                    if done {
                        ui.data_mut(|d| d.remove::<bool>(started));
                    }
                    done.then(|| !ui.input(|i| i.key_pressed(egui::Key::Escape)))
                })
                .inner;
            if let Some(accept) = finished
                && let Some((_, text)) = state.rename.take()
                && accept
            {
                actions::rename_view(state, rid, text);
            }
            continue;
        }
        let active = state.active_view == Some(id);
        let r = tree_row(ui, 1, None, i + 1 == n, category_icon(category), &label, None, active);
        // Views that go on sheets can be dragged onto one.
        let r = if state.project().view(id).is_some_and(|v| v.placeable()) {
            let r = r.interact(Sense::drag());
            r.dnd_set_drag_payload(ViewDrag(id));
            if r.dragged() {
                drag_label(ui, &label);
            }
            r
        } else {
            r
        };
        if r.clicked() {
            state.open_view(id);
        }
        r.context_menu(|ui| {
            if ui.button(t!("browser-open")).clicked() {
                state.open_view(id);
                ui.close();
            }
            if ui.button(t!("browser-rename")).clicked() {
                let name = state.project().view(id).map(|v| v.name.clone()).unwrap_or_default();
                state.rename = Some((id, name));
                ui.data_mut(|d| d.remove::<bool>(egui::Id::new(("rename-focused", id.0))));
                ui.close();
            }
            if ui.button(t!("browser-duplicate")).clicked() {
                actions::duplicate_view(state, id);
                ui.close();
            }
            if let Some(sheet) = state.active_view.filter(|s| {
                matches!(state.project().view(*s).map(|v| &v.kind), Some(ViewKind::Sheet { .. })) && *s != id
            }) {
                let placeable = state.project().view(id).is_some_and(|v| v.placeable());
                if placeable && ui.button(t!("browser-place-on-sheet")).clicked() {
                    actions::place_on_sheet(state, sheet, id);
                    ui.close();
                }
            }
            ui.separator();
            if ui.button(t!("browser-delete")).clicked() {
                actions::delete_view(state, id);
                ui.close();
            }
        });
    }
}

/// The name of a view being dragged, next to the pointer.
fn drag_label(ui: &Ui, label: &str) {
    let Some(pos) = ui.ctx().pointer_interact_pos() else { return };
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("browser-view-drag"));
    let painter = ui.ctx().layer_painter(layer);
    let galley = painter.layout_no_wrap(label.to_string(), egui::FontId::proportional(12.5), PALETTE.text);
    let rect = Rect::from_min_size(pos + vec2(14.0, 10.0), galley.size() + vec2(12.0, 6.0));
    painter.rect_filled(rect, theme::RADIUS as f32, PALETTE.panel);
    painter.rect_stroke(
        rect,
        theme::RADIUS as f32,
        egui::Stroke::new(1.0, PALETTE.border_strong),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + vec2(6.0, 3.0), galley, PALETTE.text);
}

/// "+" button at the right of a category row. Returns true when it was used.
fn add_button(ui: &mut Ui, state: &mut AppState, category: ViewCategory, row: &Response) -> bool {
    if category == ViewCategory::Elevation {
        return false;
    }
    let rect = Rect::from_center_size(egui::pos2(row.rect.right() - 18.0, row.rect.center().y), vec2(20.0, 20.0));
    let r = ui.interact(rect, row.id.with("add"), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, t!("browser-new")));
    let r = r.on_hover_text(t!("browser-new"));
    if r.hovered() {
        ui.painter().rect_filled(rect, theme::RADIUS as f32, PALETTE.pressed);
    }
    if row.hovered() || r.hovered() || r.has_focus() {
        icons::paint(ui.painter(), rect.shrink(4.0), Icon::Plus, PALETTE.text_muted);
    }
    let levels: Vec<_> = state.project().levels.iter().map(|l| (l.id, l.name.clone())).collect();
    let mut used = false;
    match category {
        ViewCategory::FloorPlan | ViewCategory::CeilingPlan => {
            egui::Popup::menu(&r).show(|ui| {
                for (id, name) in levels {
                    if ui.button(name).clicked() {
                        actions::new_plan(state, id, category == ViewCategory::CeilingPlan);
                        used = true;
                    }
                }
            });
        }
        ViewCategory::Schedule => {
            egui::Popup::menu(&r).show(|ui| {
                for c in Category::SCHEDULABLE {
                    if ui.button(actions::t_category(c)).clicked() {
                        actions::new_schedule(state, c);
                        used = true;
                    }
                }
            });
        }
        _ if r.clicked() => {
            match category {
                ViewCategory::ThreeD => {
                    actions::new_3d(state);
                }
                ViewCategory::Section => {
                    state.set_tool(Tool::Section);
                    state.message = t!("msg-section-hint");
                }
                ViewCategory::Legend => {
                    actions::new_legend(state);
                }
                ViewCategory::Sheet => {
                    actions::new_sheet(state);
                }
                _ => {}
            }
            used = true;
        }
        _ => {}
    }
    used || r.clicked()
}
