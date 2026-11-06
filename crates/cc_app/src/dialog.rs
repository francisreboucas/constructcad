//! Standard dialog frame shared by every modal window: a header with the
//! title and a close button, a padded body (with an icon for alerts) and a
//! footer with the buttons.
//!
//! Footer conventions: action buttons are right-aligned in the usual desktop
//! order (main action first, Cancel last), auxiliary buttons sit on the left,
//! the main action is highlighted and destructive actions are red. Esc and the
//! close button dismiss the dialog, like Cancel; clicks outside are ignored.

use crate::icons::{self, Icon};
use crate::theme::{self, PALETTE};
use egui::{Color32, Context, RichText, Stroke, Ui, vec2};

/// Minimum size of footer buttons.
pub const BUTTON_SIZE: egui::Vec2 = egui::Vec2::new(88.0, 28.0);
const PAD_X: i8 = 20;

/// Icon shown next to the message of an alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Warning,
    Error,
}

impl Kind {
    fn color(self) -> Color32 {
        match self {
            Kind::Warning => PALETTE.warning,
            Kind::Error => PALETTE.danger,
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Kind::Warning => "!",
            Kind::Error => "×",
        }
    }
}

/// Visual weight of a footer button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Primary,
    Secondary,
    Danger,
}

/// A highlighted button with the standard footer size.
pub fn styled_button(ui: &mut Ui, label: &str, style: Style) -> egui::Response {
    let button = match style {
        Style::Primary => {
            egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(PALETTE.accent).stroke(Stroke::NONE)
        }
        Style::Danger => {
            egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(PALETTE.danger).stroke(Stroke::NONE)
        }
        Style::Secondary => egui::Button::new(label),
    };
    ui.add(button.min_size(BUTTON_SIZE))
}

/// Footer buttons, declared in display order; [`Dialog::show`] reports which was clicked.
#[derive(Default)]
pub struct Footer {
    actions: Vec<(String, Style)>,
    extras: Vec<String>,
}

impl Footer {
    /// The main action (Enter triggers it when no text field has focus).
    pub fn primary(&mut self, label: impl Into<String>) -> usize {
        self.push(label, Style::Primary)
    }

    pub fn secondary(&mut self, label: impl Into<String>) -> usize {
        self.push(label, Style::Secondary)
    }

    pub fn danger(&mut self, label: impl Into<String>) -> usize {
        self.push(label, Style::Danger)
    }

    /// An auxiliary button on the left side. Its index follows the actions'.
    pub fn extra(&mut self, label: impl Into<String>) -> usize {
        self.extras.push(label.into());
        1000 + self.extras.len() - 1
    }

    fn push(&mut self, label: impl Into<String>, style: Style) -> usize {
        self.actions.push((label.into(), style));
        self.actions.len() - 1
    }
}

pub struct DialogResponse {
    /// Index returned by the [`Footer`] method of the clicked button.
    pub clicked: Option<usize>,
    /// Closed with Esc or the close button.
    pub dismissed: bool,
}

pub struct Dialog {
    id: egui::Id,
    title: String,
    width: f32,
    kind: Option<Kind>,
}

impl Dialog {
    pub fn new(id: impl std::hash::Hash + std::fmt::Debug, title: impl Into<String>) -> Self {
        Self { id: egui::Id::new(("dialog", id)), title: title.into(), width: 420.0, kind: None }
    }

    /// Width of the body content.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Show an alert icon next to the body.
    pub fn kind(mut self, kind: Kind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn show(self, ctx: &Context, body: impl FnOnce(&mut Ui), footer: impl FnOnce(&mut Footer)) -> DialogResponse {
        let mut buttons = Footer::default();
        footer(&mut buttons);
        let frame = egui::Frame::popup(&ctx.style_of(egui::Theme::Light))
            .inner_margin(egui::Margin::ZERO)
            .corner_radius(8)
            .fill(PALETTE.panel);
        let mut clicked = None;
        let mut close_clicked = false;
        let modal = egui::Modal::new(self.id).frame(frame).show(ctx, |ui| {
            let width = self.width + 2.0 * PAD_X as f32;
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 0.0;

            let is_rtl = crate::i18n::is_rtl();

            // Header.
            let header_margin = if is_rtl {
                egui::Margin { left: 12, right: PAD_X, top: 14, bottom: 6 }
            } else {
                egui::Margin { left: PAD_X, right: 12, top: 14, bottom: 6 }
            };
            egui::Frame::new().inner_margin(header_margin).show(
                ui,
                |ui| {
                    ui.horizontal(|ui| {
                        if is_rtl {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(&self.title).family(theme::semibold()).size(15.0).color(PALETTE.text));
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    let (rect, r) = ui.allocate_exact_size(vec2(26.0, 26.0), egui::Sense::click());
                                    if r.hovered() {
                                        ui.painter().rect_filled(rect, theme::RADIUS as f32, PALETTE.hover);
                                    }
                                    let color = if r.hovered() { PALETTE.text } else { PALETTE.text_muted };
                                    icons::paint(ui.painter(), rect.shrink(8.0), Icon::Close, color);
                                    r.widget_info(|| {
                                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, crate::t!("action-close"))
                                    });
                                    close_clicked = r.on_hover_text(crate::t!("action-close")).clicked();
                                });
                            });
                        } else {
                            ui.label(RichText::new(&self.title).family(theme::semibold()).size(15.0).color(PALETTE.text));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let (rect, r) = ui.allocate_exact_size(vec2(26.0, 26.0), egui::Sense::click());
                                if r.hovered() {
                                    ui.painter().rect_filled(rect, theme::RADIUS as f32, PALETTE.hover);
                                }
                                let color = if r.hovered() { PALETTE.text } else { PALETTE.text_muted };
                                icons::paint(ui.painter(), rect.shrink(8.0), Icon::Close, color);
                                r.widget_info(|| {
                                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, crate::t!("action-close"))
                                });
                                close_clicked = r.on_hover_text(crate::t!("action-close")).clicked();
                            });
                        }
                    });
                },
            );

            // Body.
            egui::Frame::new().inner_margin(egui::Margin { left: PAD_X, right: PAD_X, top: 8, bottom: 18 }).show(
                ui,
                |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    match self.kind {
                        Some(kind) => {
                            if is_rtl {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                    alert_icon(ui, kind);
                                    ui.add_space(6.0);
                                    ui.vertical(|ui| {
                                        ui.set_width(self.width - 48.0);
                                        body(ui);
                                    });
                                });
                            } else {
                                ui.horizontal_top(|ui| {
                                    alert_icon(ui, kind);
                                    ui.add_space(6.0);
                                    ui.vertical(|ui| {
                                        ui.set_width(self.width - 48.0);
                                        body(ui);
                                    });
                                });
                            }
                        }
                        None => {
                            ui.set_width(self.width);
                            body(ui);
                        }
                    }
                },
            );

            // Footer.
            let response = egui::Frame::new()
                .fill(PALETTE.surface)
                .inner_margin(egui::Margin { left: PAD_X, right: PAD_X, top: 12, bottom: 12 })
                .show(ui, |ui| {
                    ui.set_width(self.width);
                    ui.horizontal(|ui| {
                        if is_rtl {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                for (i, label) in buttons.extras.iter().enumerate() {
                                    if styled_button(ui, label, Style::Secondary).clicked() {
                                        clicked = Some(1000 + i);
                                    }
                                }
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    for (i, (label, style)) in buttons.actions.iter().enumerate().rev() {
                                        if styled_button(ui, label, *style).clicked() {
                                            clicked = Some(i);
                                        }
                                    }
                                });
                            });
                        } else {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            for (i, label) in buttons.extras.iter().enumerate() {
                                if styled_button(ui, label, Style::Secondary).clicked() {
                                    clicked = Some(1000 + i);
                                }
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                for (i, (label, style)) in buttons.actions.iter().enumerate().rev() {
                                    if styled_button(ui, label, *style).clicked() {
                                        clicked = Some(i);
                                    }
                                }
                            });
                        }
                    });
                });
            let r = response.response.rect;
            ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0, PALETTE.border));
        });

        // Keyboard shortcuts act on the topmost dialog only (one may open over another).
        let top = modal.is_top_modal;
        let no_text_focus = ctx.memory(|m| m.focused().is_none());
        if top && clicked.is_none() && no_text_focus && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            clicked = buttons.actions.iter().position(|(_, s)| *s == Style::Primary);
        }
        let escape = top && ctx.input(|i| i.key_pressed(egui::Key::Escape));
        DialogResponse { clicked, dismissed: close_clicked || escape }
    }
}

/// Two top-down columns side by side (a list and its details), with a divider
/// as tall as the taller column; `column` draws column 0 (left) and 1 (right).
/// `ui.separator()` in a row takes the available height instead, and in a
/// modal that grows with the modal every frame until the dialog runs off the
/// window.
pub fn two_columns(ui: &mut Ui, widths: [f32; 2], mut column: impl FnMut(&mut Ui, usize)) {
    let layout = egui::Layout::top_down(egui::Align::Min);
    let height = ui.ctx().content_rect().height();
    ui.horizontal_top(|ui| {
        let mut side = |ui: &mut Ui, i: usize| {
            // The last column takes what is left, so the columns never widen the dialog.
            let width = if i == 1 { widths[i].min(ui.available_width()) } else { widths[i] };
            ui.allocate_ui_with_layout(vec2(width, height), layout, |ui| {
                ui.set_width(width);
                column(ui, i);
            })
            .response
            .rect
        };
        let l = side(ui, 0);
        let (divider, _) = ui.allocate_exact_size(vec2(9.0, 0.0), egui::Sense::hover());
        let r = side(ui, 1);
        let y = l.top().min(r.top())..=l.bottom().max(r.bottom());
        ui.painter().vline(divider.center().x, y, Stroke::new(1.0, PALETTE.border));
    });
}

/// Round colored badge with the alert glyph.
fn alert_icon(ui: &mut Ui, kind: Kind) {
    let (rect, _) = ui.allocate_exact_size(vec2(32.0, 32.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 15.0, kind.color());
    let font = egui::FontId::new(17.0, theme::semibold());
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, kind.glyph(), font, Color32::WHITE);
}

/// Edits made while a data dialog is open can be cancelled as a whole: the
/// dialog remembers the undo depth when it opens and undoes back to it.
pub mod session {
    use crate::state::AppState;

    /// Mark the start of the dialog's edits (no-op if already marked).
    pub fn begin(state: &mut AppState) {
        if state.dialogs.edit_start.is_none() {
            state.session.doc.break_merge();
            state.dialogs.edit_start = Some(state.session.doc.undo_depth());
        }
    }

    /// Keep the edits.
    pub fn commit(state: &mut AppState) {
        state.session.doc.break_merge();
        state.dialogs.edit_start = None;
    }

    /// Undo every edit made since [`begin`].
    pub fn cancel(state: &mut AppState) {
        if let Some(start) = state.dialogs.edit_start.take() {
            state.session.undo_to(start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::session;
    use crate::state::AppState;
    use cc_core::{Document, Project};

    #[test]
    fn cancel_undoes_the_dialog_edits_and_ok_keeps_them() {
        let mut state = AppState::new(Document::new(Project::from_template()));
        state.session.doc.edit("before", |e| e.info_mut().name = "A".into());

        session::begin(&mut state);
        state.session.doc.edit("in dialog", |e| e.info_mut().name = "B".into());
        state.session.doc.edit("in dialog", |e| e.info_mut().number = "42".into());
        session::cancel(&mut state);
        assert_eq!(state.project().info.name, "A");
        assert_ne!(state.project().info.number, "42");

        session::begin(&mut state);
        state.session.doc.edit("in dialog", |e| e.info_mut().name = "C".into());
        session::commit(&mut state);
        assert_eq!(state.project().info.name, "C");
        assert!(state.dialogs.edit_start.is_none());
    }
}
