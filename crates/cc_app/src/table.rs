//! Property table primitives shared by the properties palette and dialogs:
//! collapsible sections of two-column rows, header blocks and padded content.

use crate::icons::{self, Icon};
use crate::theme::{self, PALETTE};
use egui::{Align, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, vec2};

/// Height of a table row and of a section header.
pub const ROW_H: f32 = 22.0;
/// Share of the table width taken by the label column.
pub const LABEL_SHARE: f32 = 0.46;
pub const FONT_SIZE: f32 = 12.5;

/// Text size of the tables, following the font size preference.
pub fn font_size() -> f32 {
    FONT_SIZE * theme::font_scale()
}

/// Height of a row: [`ROW_H`], taller for large text.
fn row_height() -> f32 {
    ROW_H.max((font_size() + 9.0).round())
}

// --- Table primitives ---

/// Collapsible section header; `add` draws the rows while it is open.
pub fn section(ui: &mut Ui, id: &str, title: String, add: impl FnOnce(&mut Ui)) {
    let state_id = ui.make_persistent_id(("prop-section", id));
    let mut open = ui.data_mut(|d| *d.get_persisted_mut_or(state_id, true));
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), row_height()), Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::CollapsingHeader, true, open, &title));
    let painter = ui.painter();
    let fill = if response.hovered() { PALETTE.hover } else { PALETTE.surface };
    painter.rect_filled(rect, 0.0, fill);
    painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, PALETTE.border));
    let chevron = Rect::from_center_size(egui::pos2(rect.left() + 11.0, rect.center().y), vec2(9.0, 9.0));
    icons::paint(painter, chevron, if open { Icon::ChevronDown } else { Icon::ChevronRight }, PALETTE.text_muted);
    let font = egui::FontId::new(font_size(), theme::semibold());
    painter.text(egui::pos2(rect.left() + 22.0, rect.center().y), egui::Align2::LEFT_CENTER, title, font, PALETTE.text);
    if response.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_persisted(state_id, open));
    }
    if open {
        add(ui);
    }
}

/// One table row: a label cell and a value cell filled by `add`.
pub fn row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let width = ui.available_width();
    let row_h = row_height();
    let (rect, _) = ui.allocate_exact_size(vec2(width, row_h), Sense::hover());
    let split = (rect.left() + width * LABEL_SHARE).round();
    let painter = ui.painter();
    let grid = Stroke::new(1.0, PALETTE.border);
    painter.hline(rect.x_range(), rect.bottom() - 0.5, grid);
    painter.vline(split + 0.5, rect.y_range(), grid);
    let label_rect = Rect::from_min_max(rect.min, egui::pos2(split, rect.bottom()));
    let font = egui::FontId::proportional(font_size());
    ui.painter_at(label_rect.shrink2(vec2(6.0, 0.0))).text(
        egui::pos2(label_rect.left() + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        PALETTE.text_muted,
    );
    let label_hover = ui.interact(label_rect, ui.id().with(("prop-label", label)), Sense::hover());
    label_hover.on_hover_text(label);

    let value_rect = Rect::from_min_max(egui::pos2(split + 2.0, rect.top() + 1.0), rect.max - vec2(2.0, 2.0));
    let builder = UiBuilder::new().max_rect(value_rect).layout(Layout::left_to_right(Align::Center));
    ui.scope_builder(builder, |ui| {
        let w = value_rect.width();
        let s = ui.spacing_mut();
        s.interact_size = vec2(w, row_h - 4.0);
        s.combo_width = w - 8.0;
        s.text_edit_width = w - 4.0;
        s.button_padding = vec2(6.0, 1.0);
        s.item_spacing = vec2(4.0, 0.0);
        // Flat cells: borders appear only on hover and focus.
        let v = ui.visuals_mut();
        v.widgets.inactive.bg_stroke = Stroke::NONE;
        v.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        v.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
        v.extreme_bg_color = egui::Color32::TRANSPARENT;
        ui.style_mut().override_font_id = Some(egui::FontId::proportional(font_size()));
        add(ui)
    })
    .inner
}

/// A row whose value cannot be edited (shown dimmed).
pub fn readonly_row(ui: &mut Ui, label: &str, value: String) {
    row(ui, label, |ui| {
        ui.add_space(4.0);
        ui.label(RichText::new(value).color(PALETTE.text_faint));
    });
}

// --- Header block ---

/// Header with a large icon tile, a title and a subtitle.
pub fn header_block(ui: &mut Ui, icon: Icon, title: &str, subtitle: &str) {
    ui.horizontal(|ui| {
        let (tile, _) = ui.allocate_exact_size(vec2(42.0, 42.0), Sense::hover());
        ui.painter().rect_filled(tile, theme::RADIUS as f32, PALETTE.canvas);
        ui.painter().rect_stroke(
            tile,
            theme::RADIUS as f32,
            Stroke::new(1.0, PALETTE.border),
            egui::StrokeKind::Inside,
        );
        icons::paint(ui.painter(), tile.shrink(10.0), icon, PALETTE.accent_text);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.add_space(3.0);
            ui.label(RichText::new(title).family(theme::semibold()).size(13.5).color(PALETTE.text));
            ui.label(RichText::new(subtitle).size(12.0).color(PALETTE.text_faint));
        });
    });
    ui.add_space(6.0);
}

/// Content with the panel's side padding (headers, buttons, lists).
pub fn padded<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            add(ui)
        })
        .inner
}

/// A single-line text field over a value read from the model every frame.
/// While it has the focus it shows what was typed, not the value read back,
/// so a command that trims or refuses the text mid-typing (a trailing
/// space, an emptied name) or a value only kept when the field is left
/// never undoes the typing. `text` holds what was typed this frame; `salt`
/// names the field (unique among those shown), `add` draws it.
pub fn live_text(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    text: &mut String,
    add: impl FnOnce(&mut Ui, &mut String) -> egui::Response,
) -> egui::Response {
    let key = egui::Id::new(("live-text", salt));
    if let Some((widget, typed)) = ui.data(|d| d.get_temp::<(egui::Id, String)>(key))
        && ui.memory(|m| m.has_focus(widget))
    {
        *text = typed;
    }
    let r = add(ui, text);
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(key, (r.id, text.clone())));
    } else {
        ui.data_mut(|d| d.remove::<(egui::Id, String)>(key));
    }
    r
}

/// A one-line field that hands back its text, trimmed, when Enter is
/// pressed or it loses the focus, and only if it changed. While focused it
/// keeps what is typed; otherwise it shows `current`. `hint` shows when empty.
pub fn commit_text(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    current: &str,
    width: f32,
    hint: &str,
) -> Option<String> {
    let id = egui::Id::new(("commit-text", salt));
    let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| current.to_string());
    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(width).hint_text(hint));
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text));
        return None;
    }
    ui.data_mut(|d| d.remove::<String>(id));
    let text = text.trim().to_string();
    (r.lost_focus() && text != current).then_some(text)
}
