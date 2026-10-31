//! Ribbon, options bar and status bar.

use crate::actions;
use crate::commands::{self, UiCommand};
use crate::icons::{self, Badge, Icon, Tones};
use crate::state::{AppState, RibbonTab};
use crate::t;
use crate::theme::{self, PALETTE};
use crate::tools::{DimensionMode, Tool, walls_loop};
use cc_core::{Category, ViewCategory};
use cc_views::format;
use egui::{Align, Button, DragValue, Layout, Response, RichText, Sense, Stroke, StrokeKind, Ui, vec2};

/// Height of the large ribbon buttons.
const BUTTON_HEIGHT: f32 = 66.0;
const ICON_SIZE: f32 = 22.0;
/// Icon size of the small stacked ribbon buttons.
const STACK_ICON_SIZE: f32 = 20.0;

/// Icon of a model category, shared by the ribbon, options bar and browser.
pub fn category_icon(category: Category) -> Icon {
    match category {
        Category::Wall => Icon::Wall,
        Category::Door => Icon::Door,
        Category::Window => Icon::Window,
        Category::Floor => Icon::Floor,
        Category::Ceiling => Icon::Ceiling,
        Category::Roof => Icon::Roof,
        Category::Room => Icon::Room,
        Category::Grid => Icon::Grid,
        Category::Foundation => Icon::Footing,
        Category::Rebar => Icon::Rebar,
        Category::Pipe => Icon::Pipe,
        Category::Conduit => Icon::Conduit,
        Category::Dimension => Icon::Dimension,
        Category::Column => Icon::Column,
        Category::Beam => Icon::Beam,
        Category::Stair => Icon::Stair,
        Category::Ramp => Icon::Ramp,
        Category::Railing => Icon::Railing,
        Category::Component => Icon::Component,
        Category::Text => Icon::Text,
        Category::CadImport => Icon::CadImport,
        Category::Mass => Icon::Mass,
    }
}

fn tool_icon(tool: &Tool) -> Icon {
    match tool {
        Tool::Select => Icon::Select,
        Tool::Section => Icon::Section,
        Tool::Move => Icon::Move,
        Tool::Copy => Icon::Copy,
        Tool::Rotate => Icon::Rotate,
        Tool::Mirror { draw: false } => Icon::MirrorPick,
        Tool::Mirror { draw: true } => Icon::MirrorDraw,
        Tool::Array => Icon::Array,
        Tool::TrimCorner => Icon::TrimCorner,
        Tool::TrimExtend => Icon::TrimExtend,
        Tool::Split => Icon::Split,
        Tool::Offset => Icon::Offset,
        Tool::Align => Icon::Align,
        Tool::PushPull => Icon::PushPull,
        Tool::ToggleJoin => Icon::Join,
        Tool::Measure => Icon::Measure,
        Tool::JoinGeometry { unjoin: false } => Icon::JoinGeometry,
        Tool::JoinGeometry { unjoin: true } => Icon::UnjoinGeometry,
        Tool::Paste => Icon::Paste,
        Tool::Dimension { mode, .. } => dimension_icon(*mode),
        Tool::MatchType => Icon::MatchType,
        Tool::Demolish => Icon::Demolish,
        Tool::AddLeader { .. } => Icon::Leader,
        Tool::Opening { floor: false, .. } => category_icon(Category::Roof),
        Tool::Opening { floor: true, .. } => category_icon(Category::Floor),
        _ => tool.category().map_or(Icon::Select, category_icon),
    }
}

/// Labels wider than this (points) wrap onto two lines at a space.
const WRAP_WIDTH: f32 = 64.0;

/// Split `label` into one or two lines: long labels break at the space
/// that best balances the two lines.
fn label_lines(ui: &Ui, label: &str, font: &egui::FontId) -> Vec<String> {
    let width = |t: &str| ui.painter().layout_no_wrap(t.to_string(), font.clone(), PALETTE.text).size().x;
    if width(label) <= WRAP_WIDTH || !label.contains(' ') {
        return vec![label.to_string()];
    }
    let best = label
        .match_indices(' ')
        .map(|(i, _)| (i, width(&label[..i]).max(width(&label[i + 1..]))))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i);
    match best {
        Some(i) => vec![label[..i].to_string(), label[i + 1..].to_string()],
        None => vec![label.to_string()],
    }
}

/// A large ribbon button: icon above a label of up to two lines. `dropdown` adds a chevron.
fn big_button(
    ui: &mut Ui,
    (icon, badge): (Icon, Option<Badge>),
    label: &str,
    selected: bool,
    enabled: bool,
    dropdown: bool,
) -> Response {
    let text_color = if enabled { PALETTE.text } else { PALETTE.text_faint };
    let font = egui::FontId::proportional(12.0);
    let galleys: Vec<_> = label_lines(ui, label, &font)
        .into_iter()
        .map(|line| ui.painter().layout_no_wrap(line, font.clone(), text_color))
        .collect();
    let chevron = if dropdown { 11.0 } else { 0.0 };
    let text_w = galleys.iter().map(|g| g.size().x).fold(0.0, f32::max);
    let width = (text_w + chevron + 14.0).max(50.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(width, BUTTON_HEIGHT), sense);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let radius = theme::RADIUS as f32;
        let pressed = response.is_pointer_button_down_on();
        if selected {
            painter.rect_filled(rect, radius, PALETTE.accent_soft);
            let border = Stroke::new(1.0, PALETTE.accent.gamma_multiply(0.55));
            painter.rect_stroke(rect, radius, border, StrokeKind::Inside);
        } else if enabled && (response.hovered() || pressed) {
            painter.rect_filled(rect, radius, if pressed { PALETTE.pressed } else { PALETTE.hover });
        }
        let icon_color = match (enabled, selected) {
            (false, _) => PALETTE.text_faint,
            (true, true) => PALETTE.accent_text,
            (true, false) => PALETTE.text,
        };
        let icon_center = egui::pos2(rect.center().x, rect.top() + 6.0 + ICON_SIZE / 2.0);
        let tones = Tones::of(theme::palette(ui.ctx()), badge, enabled, selected);
        let icon_rect = egui::Rect::from_center_size(icon_center, vec2(ICON_SIZE, ICON_SIZE));
        icons::paint_toned(painter, icon_rect, icon, tones, badge);
        // Lines start right under the icon, each centered; the chevron follows the last one.
        let text_color = if selected { PALETTE.accent_text } else { text_color };
        let mut y = rect.top() + 6.0 + ICON_SIZE + 3.0;
        let n = galleys.len();
        for (k, galley) in galleys.into_iter().enumerate() {
            let size = galley.size();
            let extra = if k + 1 == n { chevron } else { 0.0 };
            let pos = egui::pos2(rect.center().x - (size.x + extra) / 2.0, y);
            if dropdown && k + 1 == n {
                let side = size.y - 3.0;
                let c = egui::Rect::from_min_size(egui::pos2(pos.x + size.x + 2.0, pos.y + 2.0), vec2(side, side));
                icons::paint(painter, c, Icon::ChevronDown, icon_color);
            }
            painter.galley_with_override_text_color(pos, galley, text_color);
            y += size.y - 1.0;
        }
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, label));
    if enabled { response.on_hover_cursor(egui::CursorIcon::PointingHand) } else { response }
}

/// Rows of a stacked family: families of 2 or 4 use two rows (one column,
/// or 2 x 2), everything else three.
fn stack_rows(n: usize) -> usize {
    if n == 2 || n == 4 { 2 } else { 3 }
}

/// One family of related commands as small icon buttons in columns of
/// two or three (see [`stack_rows`]); each family starts its own column,
/// so unrelated commands never share one. Two-row columns are centered on
/// the large buttons' height. Returns the clicked command.
fn stacked(ui: &mut Ui, state: &AppState, family: &[UiCommand]) -> Option<UiCommand> {
    let mut clicked = None;
    let rows = stack_rows(family.len());
    let size = vec2(28.0, (BUTTON_HEIGHT - 4.0) / 3.0);
    let column_h = rows as f32 * size.y + (rows as f32 - 1.0) * 2.0;
    for chunk in family.chunks(rows) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(((BUTTON_HEIGHT - column_h) / 2.0).max(0.0));
            for command in chunk {
                let why = command.unavailable(state);
                let (enabled, selected) = (why.is_none(), command.selected(state));
                let sense = if enabled { Sense::click() } else { Sense::hover() };
                let (rect, r) = ui.allocate_exact_size(size, sense);
                let painter = ui.painter();
                let radius = theme::RADIUS as f32;
                if selected {
                    painter.rect_filled(rect, radius, PALETTE.accent_soft);
                    painter.rect_stroke(
                        rect,
                        radius,
                        Stroke::new(1.0, PALETTE.accent.gamma_multiply(0.55)),
                        StrokeKind::Inside,
                    );
                } else if enabled && r.hovered() {
                    let fill = if r.is_pointer_button_down_on() { PALETTE.pressed } else { PALETTE.hover };
                    painter.rect_filled(rect, radius, fill);
                }
                let color = match (enabled, selected) {
                    (false, _) => PALETTE.text_faint,
                    (true, true) => PALETTE.accent_text,
                    (true, false) => PALETTE.text,
                };
                let icon_rect = egui::Rect::from_center_size(rect.center(), vec2(STACK_ICON_SIZE, STACK_ICON_SIZE));
                let tones =
                    Tones { ink: color, ..Tones::of(theme::palette(ui.ctx()), command.badge(), enabled, selected) };
                icons::paint_toned(painter, icon_rect, command.icon(), tones, command.badge());
                let label = command.label(state);
                r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, &label));
                let r = if enabled { r.on_hover_cursor(egui::CursorIcon::PointingHand) } else { r };
                if command_tooltip(r, state, *command, why).clicked() && enabled {
                    clicked = Some(*command);
                }
            }
        });
    }
    clicked
}

/// Small square icon button (quick access toolbar).
fn icon_button(ui: &mut Ui, (icon, badge): (Icon, Option<Badge>), enabled: bool) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(26.0, 26.0), sense);
    if enabled && response.hovered() {
        ui.painter().rect_filled(rect, theme::RADIUS as f32, PALETTE.hover);
    }
    let color = if enabled { PALETTE.text_muted } else { PALETTE.text_faint.gamma_multiply(0.6) };
    let tones = Tones { ink: color, ..Tones::of(theme::palette(ui.ctx()), badge, enabled, false) };
    icons::paint_toned(ui.painter(), rect.shrink(5.0), icon, tones, badge);
    response
}

/// A group of the component environment's ribbon.
pub fn env_group(ui: &mut Ui, state: &mut AppState, caption: String, commands: &[UiCommand]) {
    group(ui, caption, |ui| command_buttons(ui, state, commands));
}

/// A captioned group of ribbon buttons followed by a vertical divider.
fn group(ui: &mut Ui, caption: String, add: impl FnOnce(&mut Ui)) {
    let caption_w =
        ui.painter().layout_no_wrap(caption.clone(), egui::FontId::proportional(11.0), PALETTE.text).size().x;
    let inner = ui.vertical(|ui| {
        // At least as wide as the caption, so it never wraps.
        ui.set_min_width(caption_w + 8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            add(ui);
        });
        let width = ui.min_rect().width();
        ui.allocate_ui_with_layout(vec2(width, 14.0), Layout::top_down(Align::Center), |ui| {
            ui.label(theme::caption(caption));
        });
    });
    let height = inner.response.rect.height();
    let (r, _) = ui.allocate_exact_size(vec2(13.0, height), Sense::hover());
    ui.painter().vline(r.center().x, r.y_range().shrink(4.0), Stroke::new(1.0, PALETTE.border));
}

/// A ribbon tab title in the top strip.
/// Font of a ribbon tab title (semibold when active).
fn tab_font(active: bool) -> egui::FontId {
    egui::FontId::new(13.0, if active { theme::semibold() } else { egui::FontFamily::Proportional })
}

/// A tab title in a cell of `width`, its text centered.
fn tab_title(ui: &mut Ui, label: &str, active: bool, width: f32) -> Response {
    let color = if active { PALETTE.accent_text } else { PALETTE.text_muted };
    let galley = ui.painter().layout_no_wrap(label.to_string(), tab_font(active), color);
    let (rect, response) = ui.allocate_exact_size(vec2(width, galley.size().y + 12.0), Sense::click());
    if response.hovered() && !active {
        ui.painter().rect_filled(rect.shrink2(vec2(2.0, 3.0)), theme::RADIUS as f32, PALETTE.hover);
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, active, label));
    let text_w = galley.size().x;
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    if active {
        let y = rect.bottom() - 1.0;
        let half = text_w / 2.0 + 4.0;
        let line = [egui::pos2(rect.center().x - half, y), egui::pos2(rect.center().x + half, y)];
        ui.painter().line_segment(line, Stroke::new(2.0, PALETTE.accent));
    }
    response
}

/// A large ribbon button for `command`: its state, label and tooltip come from the command.
fn command_button(ui: &mut Ui, state: &mut AppState, command: UiCommand) {
    if command_big(ui, state, command, false).clicked() {
        commands::run(state, &ui.ctx().clone(), command);
    }
}

/// A large ribbon button for `command` that opens a menu (the caller attaches it).
fn command_menu(ui: &mut Ui, state: &AppState, command: UiCommand) -> Response {
    command_big(ui, state, command, true)
}

fn command_big(ui: &mut Ui, state: &AppState, command: UiCommand, dropdown: bool) -> Response {
    let why = command.unavailable(state);
    let label = command.label(state);
    let r = big_button(ui, (command.icon(), command.badge()), &label, command.selected(state), why.is_none(), dropdown);
    command_tooltip(r, state, command, why)
}

/// The rich tooltip of a command: title and shortcut, what it does, why it
/// is unavailable, and the engine command that scripts call for the same change.
/// With detailed tooltips off (a preference), only the title, shortcut and
/// why it is unavailable.
pub fn command_tooltip(response: Response, state: &AppState, command: UiCommand, why: Option<String>) -> Response {
    let title = command.title(state);
    let detailed = state.prefs.detailed_tooltips;
    response.on_hover_ui(|ui| {
        ui.set_max_width(300.0);
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.horizontal(|ui| {
            ui.label(RichText::new(&title).family(theme::semibold()).color(PALETTE.text));
            if let Some(keys) = command.shortcut() {
                key_cap(ui, &keys);
            }
        });
        if detailed {
            ui.label(theme::muted(command.description()));
        }
        if let Some(why) = &why {
            ui.label(RichText::new(t!("tip-unavailable", "reason" => why)).color(PALETTE.warning));
        }
        if let Some(name) = command.engine().filter(|_| detailed) {
            ui.label(RichText::new(t!("tip-engine", "command" => name)).size(11.0).color(PALETTE.text_faint));
        }
    })
}

/// A keyboard shortcut drawn as a key cap.
pub(crate) fn key_cap(ui: &mut Ui, keys: &str) -> egui::Response {
    egui::Frame::new()
        .fill(PALETTE.surface)
        .stroke(Stroke::new(1.0, PALETTE.border))
        .corner_radius(3)
        .inner_margin(egui::Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(keys).size(11.0).color(PALETTE.text_muted));
        })
        .response
}

/// A small quick access button for `command`.
fn command_icon(ui: &mut Ui, state: &mut AppState, command: UiCommand) {
    let why = command.unavailable(state);
    let r = icon_button(ui, (command.icon(), command.badge()), why.is_none());
    let label = command.label(state);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, why.is_none(), &label));
    if command_tooltip(r, state, command, why).clicked() {
        commands::run(state, &ui.ctx().clone(), command);
    }
}

pub fn ribbon(ui: &mut Ui, state: &mut AppState) {
    // Measured first: rows wider than the window grow the Ui's rectangle.
    let full_width = ui.available_width();
    // Quick access toolbar at the top left, where desktop applications keep it.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        ui.add_space(2.0);
        let (mark, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
        if let Some(logo) = theme::logo(ui.ctx()) {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().image(logo.id(), mark, uv, egui::Color32::WHITE);
        }
        ui.add_space(8.0);
        let divider = |ui: &mut Ui| {
            let (div, _) = ui.allocate_exact_size(vec2(9.0, 18.0), Sense::hover());
            ui.painter().vline(div.center().x, div.y_range(), Stroke::new(1.0, PALETTE.border));
        };
        for command in [UiCommand::Home, UiCommand::New, UiCommand::Open, UiCommand::Save] {
            command_icon(ui, state, command);
        }
        divider(ui);
        command_icon(ui, state, UiCommand::Undo);
        command_icon(ui, state, UiCommand::Redo);
        // Frequent commands that already live in the ribbon.
        divider(ui);
        for command in [
            UiCommand::ExportPdf,
            UiCommand::Dimension(DimensionMode::Aligned),
            UiCommand::Default3D,
            UiCommand::Section,
            UiCommand::CloseInactiveViews,
        ] {
            command_icon(ui, state, command);
        }
    });
    // Ribbon tab titles, all as wide as the widest (measured bold, as when active).
    let tabs = [
        (RibbonTab::File, t!("ribbon-file")),
        (RibbonTab::Architecture, t!("ribbon-architecture")),
        (RibbonTab::Structure, t!("ribbon-structure")),
        (RibbonTab::Mep, t!("ribbon-mep")),
        (RibbonTab::Insert, t!("ribbon-insert")),
        (RibbonTab::Modify, t!("ribbon-modify")),
        (RibbonTab::Annotate, t!("ribbon-annotate")),
        (RibbonTab::View, t!("ribbon-view")),
        (RibbonTab::Manage, t!("ribbon-manage")),
    ];
    let width = tabs
        .iter()
        .map(|(_, label)| ui.painter().layout_no_wrap(label.clone(), tab_font(true), PALETTE.text).size().x)
        .fold(0.0, f32::max)
        + 28.0;
    // Narrow windows squeeze the titles, keeping room for the collapse button.
    let width = width.min(((full_width - 40.0) / tabs.len() as f32).max(width - 24.0));
    let collapsed = state.ribbon_collapsed;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (tab, label) in tabs {
            let active = state.ribbon_tab == tab && (!collapsed || state.ribbon_peek);
            let r = tab_title(ui, &label, active, width);
            // A double click collapses or expands; a click on a collapsed
            // ribbon shows that tab's panel over the views, or hides it again.
            if r.double_clicked() {
                state.ribbon_collapsed = !state.ribbon_collapsed;
                state.ribbon_peek = false;
            } else if r.clicked() {
                if collapsed {
                    state.ribbon_peek = !(state.ribbon_peek && state.ribbon_tab == tab);
                }
                state.ribbon_tab = tab;
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            command_icon(ui, state, UiCommand::CommandPalette);
            let command = UiCommand::ToggleRibbon;
            let icon = if collapsed { Icon::ChevronDown } else { Icon::ChevronUp };
            let r = icon_button(ui, (icon, None), true);
            let label = command.label(state);
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
            if command_tooltip(r, state, command, None).clicked() {
                commands::run(state, &ui.ctx().clone(), command);
            }
        });
    });
    let strip_bottom = ui.min_rect().bottom();
    ui.painter().hline(ui.max_rect().x_range(), strip_bottom + 3.0, Stroke::new(1.0, PALETTE.border));
    if !collapsed {
        ui.add_space(8.0);
        tab_row(ui, state, full_width);
    } else if state.ribbon_peek {
        peek(ui, state, strip_bottom + 4.0);
    }
    ui.add_space(2.0);
}

/// The collapsed ribbon's panel, shown over the views until a command
/// runs, Escape is pressed or the pointer clicks elsewhere.
fn peek(ui: &mut Ui, state: &mut AppState, top: f32) {
    let rect = ui.max_rect();
    let palette = theme::palette(ui.ctx());
    let frame = egui::Frame::new()
        .fill(palette.panel)
        .stroke(Stroke::new(1.0, palette.border))
        .shadow(ui.style().visuals.popup_shadow)
        .inner_margin(egui::Margin { left: 10, right: 10, top: 8, bottom: 4 });
    let area = egui::Area::new(egui::Id::new("ribbon-peek"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(rect.left() - 10.0, top))
        .show(ui.ctx(), |ui| {
            frame.show(ui, |ui| {
                ui.set_width(rect.width());
                tab_row(ui, state, rect.width());
            })
        });
    let panel = area.response.rect;
    let (escape, outside) = ui.input(|i| {
        let pos = i.pointer.interact_pos();
        let outside = i.pointer.any_click() && pos.is_some_and(|p| p.y > top && !panel.contains(p));
        (i.key_pressed(egui::Key::Escape), outside)
    });
    if escape || outside {
        state.ribbon_peek = false;
    }
}

/// Width of the arrows that scroll a ribbon wider than the window.
const SCROLL_ARROW_WIDTH: f32 = 18.0;
/// How far one arrow click scrolls.
const SCROLL_STEP: f32 = 240.0;

/// What the ribbon row measured last frame: content width, offset and visible width.
#[derive(Clone, Copy, Default)]
struct RowExtent {
    content: f32,
    offset: f32,
    visible: f32,
    /// Offset an arrow asked for, applied on the next frame.
    pending: Option<f32>,
}

/// The active tab's groups. When they are wider than the window they
/// scroll sideways, with arrows at the ends (and the mouse wheel).
fn tab_row(ui: &mut Ui, state: &mut AppState, available: f32) {
    let id = egui::Id::new(("ribbon-row", state.ribbon_tab as u8));
    let mut extent: RowExtent = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
    let overflow = extent.content > available + 0.5;
    ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let max_offset = (extent.content - extent.visible).max(0.0);
        let mut target = extent.pending.take();
        if overflow && scroll_arrow(ui, false, extent.offset > 0.5).clicked() {
            target = Some((extent.offset - SCROLL_STEP).max(0.0));
        }
        let arrows = if overflow { 2.0 * SCROLL_ARROW_WIDTH } else { 0.0 };
        let mut area = egui::ScrollArea::horizontal()
            .id_salt(id)
            .max_width((available - arrows).max(0.0))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
        if let Some(offset) = target {
            area = area.horizontal_scroll_offset(offset);
        }
        let out = area.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                match state.ribbon_tab {
                    RibbonTab::File => file_tab(ui, state),
                    RibbonTab::Architecture => architecture_tab(ui, state),
                    RibbonTab::Structure => structure_tab(ui, state),
                    RibbonTab::Mep => mep_tab(ui, state),
                    RibbonTab::Insert => insert_tab(ui, state),
                    RibbonTab::Modify => modify_tab(ui, state),
                    RibbonTab::Annotate => annotate_tab(ui, state),
                    RibbonTab::View => view_tab(ui, state),
                    RibbonTab::Manage => manage_tab(ui, state),
                }
            });
        });
        if overflow && scroll_arrow(ui, true, extent.offset < max_offset - 0.5).clicked() {
            extent.pending = Some((extent.offset + SCROLL_STEP).min(max_offset));
            ui.ctx().request_repaint();
        }
        extent.content = out.content_size.x;
        extent.offset = out.state.offset.x;
        extent.visible = out.inner_rect.width();
    });
    if (extent.content > available + 0.5) != overflow {
        // The arrows appear or go: lay out again with them.
        ui.ctx().request_repaint();
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, extent));
}

/// An arrow at one end of an overflowing ribbon row.
fn scroll_arrow(ui: &mut Ui, right: bool, enabled: bool) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(SCROLL_ARROW_WIDTH, BUTTON_HEIGHT + 16.0), sense);
    let palette = theme::palette(ui.ctx());
    if enabled && response.hovered() {
        ui.painter().rect_filled(rect, theme::RADIUS as f32, palette.hover);
    }
    let color = if enabled { palette.text_muted } else { palette.text_faint.gamma_multiply(0.5) };
    let (c, d) = (rect.center(), if right { 1.0 } else { -1.0 });
    let tip = c + vec2(3.0 * d, 0.0);
    let stroke = Stroke::new(1.6, color);
    ui.painter().line_segment([tip, tip + vec2(-6.0 * d, -6.0)], stroke);
    ui.painter().line_segment([tip, tip + vec2(-6.0 * d, 6.0)], stroke);
    let label = if right { t!("ribbon-scroll-right") } else { t!("ribbon-scroll-left") };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &label));
    response.on_hover_text(label)
}

/// Large buttons for `list`, in order.
fn command_buttons(ui: &mut Ui, state: &mut AppState, list: &[UiCommand]) {
    for command in list {
        command_button(ui, state, *command);
    }
}

fn file_tab(ui: &mut Ui, state: &mut AppState) {
    use UiCommand::{CloseProject, ExportIfc, ExportImage, ExportPdf, ExportSvg, New, Open, Save, SaveAs};
    group(ui, t!("group-project"), |ui| command_buttons(ui, state, &[New, Open, Save, SaveAs, CloseProject]));
    group(ui, t!("group-export"), |ui| command_buttons(ui, state, &[ExportIfc, ExportPdf, ExportSvg, ExportImage]));
    group(ui, t!("group-settings"), |ui| command_button(ui, state, UiCommand::Preferences));
    group(ui, t!("group-help"), |ui| command_buttons(ui, state, &[UiCommand::About, UiCommand::CheckUpdates]));
}

/// Files brought into the project or written from it, and components to load.
fn insert_tab(ui: &mut Ui, state: &mut AppState) {
    use UiCommand::{ComponentLibrary, ExportIfc, ImportCad, ImportIfc, ImportImage, ImportSkp, LoadComponent};
    group(ui, t!("group-import"), |ui| command_buttons(ui, state, &[ImportIfc, ImportCad, ImportSkp, ImportImage]));
    group(ui, t!("group-export"), |ui| command_button(ui, state, ExportIfc));
    group(ui, t!("group-library"), |ui| command_buttons(ui, state, &[LoadComponent, ComponentLibrary]));
}

fn place(category: Category) -> UiCommand {
    UiCommand::Place(category)
}

fn architecture_tab(ui: &mut Ui, state: &mut AppState) {
    use Category::*;
    let buttons = |list: &[Category]| list.iter().map(|c| place(*c)).collect::<Vec<_>>();
    group(ui, t!("group-massing"), |ui| {
        command_buttons(ui, state, &[place(Mass), UiCommand::PushPull]);
        stacks(ui, state, &[&[UiCommand::MassWalls, UiCommand::MassSlabs, UiCommand::MassRoof]]);
    });
    group(ui, t!("group-build"), |ui| command_buttons(ui, state, &buttons(&[Wall, Door, Window])));
    group(ui, t!("group-horizontal"), |ui| command_buttons(ui, state, &buttons(&[Floor, Ceiling, Roof])));
    group(ui, t!("group-circulation"), |ui| command_buttons(ui, state, &buttons(&[Stair, Ramp, Railing])));
    group(ui, t!("group-room"), |ui| command_button(ui, state, place(Room)));
    group(ui, t!("group-model"), |ui| {
        command_buttons(
            ui,
            state,
            &[
                place(Component),
                UiCommand::NewComponent,
                UiCommand::EditComponent,
                UiCommand::LoadComponent,
                UiCommand::ComponentLibrary,
            ],
        );
    });
    group(ui, t!("group-datum"), |ui| command_buttons(ui, state, &[place(Grid), UiCommand::AlignGridEnds]));
    group(ui, t!("group-selection"), |ui| command_button(ui, state, UiCommand::Select));
}

fn structure_tab(ui: &mut Ui, state: &mut AppState) {
    use UiCommand::*;
    group(ui, t!("group-structure"), |ui| {
        command_buttons(ui, state, &[place(Category::Column), place(Category::Beam), place(Category::Foundation)]);
    });
    // Layout from grids and foundations under what exists, as small stacked buttons.
    group(ui, t!("group-framing"), |ui| {
        stacks(ui, state, &[&[ColumnsAtGrids, BeamsOnGrids, BeamsBetweenColumns], &[BracesX, Truss]]);
    });
    group(ui, t!("group-foundation"), |ui| {
        stacks(ui, state, &[&[FootingsUnderColumns, StripFootingsUnderWalls, FoundationSlab]]);
    });
    group(ui, t!("group-reinforcement"), |ui| stacks(ui, state, &[&[TypicalRebar, RebarView, SteelSchedule]]));
    group(ui, t!("group-analytical"), |ui| {
        stacks(ui, state, &[&[SlabJoinSupports, AnalyticalView, AnalyticalExport]]);
    });
    group(ui, t!("group-documentation"), |ui| {
        stacks(ui, state, &[&[StructuralPlan, NumberStructure, StructuralSchedules]]);
    });
    group(ui, t!("group-datum"), |ui| command_buttons(ui, state, &[place(Category::Grid), AlignGridEnds]));
    group(ui, t!("group-selection"), |ui| command_button(ui, state, Select));
}

fn mep_tab(ui: &mut Ui, state: &mut AppState) {
    use UiCommand::*;
    group(ui, t!("group-plumbing"), |ui| command_buttons(ui, state, &[place(Category::Pipe), PlaceFixture]));
    group(ui, t!("group-electrical"), |ui| command_buttons(ui, state, &[place(Category::Conduit), PlaceDevice]));
    group(ui, t!("group-systems"), |ui| command_buttons(ui, state, &[MepConnect, Systems]));
    group(ui, t!("group-analysis"), |ui| command_button(ui, state, MepSizing));
    group(ui, t!("group-documentation"), |ui| {
        stacks(ui, state, &[&[PlumbingPlan, ElectricalPlan, MepSchedules], &[PanelDiagram]]);
    });
    group(ui, t!("group-selection"), |ui| command_button(ui, state, Select));
}

pub fn dimension_icon(mode: DimensionMode) -> Icon {
    match mode {
        DimensionMode::Aligned => Icon::Dimension,
        DimensionMode::Linear => Icon::DimLinear,
        DimensionMode::Ordinate => Icon::DimOrdinate,
        DimensionMode::Angular => Icon::DimAngular,
        DimensionMode::Radial => Icon::DimRadial,
        DimensionMode::Diameter => Icon::DimDiameter,
    }
}

fn annotate_tab(ui: &mut Ui, state: &mut AppState) {
    group(ui, t!("group-dimension"), |ui| {
        command_buttons(ui, state, &DimensionMode::ALL.map(UiCommand::Dimension));
    });
    group(ui, t!("group-text"), |ui| command_buttons(ui, state, &[UiCommand::Text, UiCommand::AddLeader]));
    group(ui, t!("group-tags"), |ui| command_button(ui, state, UiCommand::StructureTags));
    group(ui, t!("group-selection"), |ui| command_button(ui, state, UiCommand::Select));
}

/// The text note selected alone, if any.
pub fn selected_text(state: &AppState) -> Option<cc_core::ElementId> {
    let [id] = state.selection.iter().copied().collect::<Vec<_>>()[..] else { return None };
    matches!(state.project().element(id).map(|e| &e.kind), Some(cc_core::ElementKind::Text(_))).then_some(id)
}

/// Families of small buttons, each starting its own column.
fn stacks(ui: &mut Ui, state: &mut AppState, families: &[&[UiCommand]]) {
    for family in families {
        if let Some(command) = stacked(ui, state, family) {
            commands::run(state, &ui.ctx().clone(), command);
        }
    }
}

fn modify_tab(ui: &mut Ui, state: &mut AppState) {
    use UiCommand::*;
    group(ui, t!("group-selection"), |ui| command_buttons(ui, state, &[Select, SelectInstances]));
    group(ui, t!("group-clipboard"), |ui| clipboard_group(ui, state));
    group(ui, t!("group-modify"), |ui| {
        let transforms: &[UiCommand] = &[Move, CopyElements, Rotate, MirrorPick, MirrorDraw, Array];
        // Geometry edits pick their elements, no selection needed.
        let picks: &[UiCommand] = &[Align, Offset, TrimCorner, TrimExtend, Split, ToggleJoin];
        stacks(ui, state, &[transforms, picks, &[Pin, Unpin, Delete]]);
    });
    group(ui, t!("group-measure"), |ui| command_button(ui, state, Measure));
    group(ui, t!("group-geometry"), |ui| command_buttons(ui, state, &[JoinGeometry, UnjoinGeometry]));
    group(ui, t!("group-group"), |ui| command_buttons(ui, state, &[Group, Ungroup]));
    group(ui, t!("group-phasing"), |ui| command_button(ui, state, Demolish));
}

/// Paste (a menu of where to paste), then cut, copy and match type.
fn clipboard_group(ui: &mut Ui, state: &mut AppState) {
    let r = command_menu(ui, state, UiCommand::Paste);
    egui::Popup::menu(&r).show(|ui| {
        if ui.button(format!("{} (Ctrl+V)", t!("paste-click"))).clicked() {
            crate::clipboard::paste(state);
        }
        if ui.button(t!("paste-same-place")).clicked() {
            crate::clipboard::paste_aligned(state, false);
        }
        let plan = state.active_level().is_some();
        if ui.add_enabled(plan, Button::new(t!("paste-current-level"))).clicked() {
            crate::clipboard::paste_aligned(state, true);
        }
    });
    stacks(ui, state, &[&[UiCommand::Cut, UiCommand::ClipboardCopy, UiCommand::MatchType]]);
}

fn view_tab(ui: &mut Ui, state: &mut AppState) {
    let levels: Vec<_> = state.project().levels.iter().map(|l| (l.id, l.name.clone())).collect();
    group(ui, t!("group-create-views"), |ui| {
        for command in [UiCommand::FloorPlan, UiCommand::CeilingPlan] {
            let r = command_menu(ui, state, command);
            egui::Popup::menu(&r).show(|ui| {
                for (id, name) in &levels {
                    if ui.button(name).clicked() {
                        actions::new_plan(state, *id, command == UiCommand::CeilingPlan);
                    }
                }
            });
        }
        command_buttons(ui, state, &[UiCommand::New3D, UiCommand::Section]);
        let r = command_menu(ui, state, UiCommand::Elevations);
        egui::Popup::menu(&r).show(|ui| {
            let elevations: Vec<_> =
                state.project().views_of(ViewCategory::Elevation).map(|v| (v.id, v.name.clone())).collect();
            for (id, name) in elevations {
                if ui.button(name).clicked() {
                    state.open_view(id);
                }
            }
        });
    });
    group(ui, t!("group-documentation"), |ui| {
        command_button(ui, state, UiCommand::Legend);
        let r = command_menu(ui, state, UiCommand::Schedule);
        egui::Popup::menu(&r).show(|ui| {
            for c in Category::SCHEDULABLE {
                if ui.button(actions::t_category(c)).clicked() {
                    actions::new_schedule(state, c);
                }
            }
        });
        command_button(ui, state, UiCommand::Sheet);
    });
    group(ui, t!("group-graphics"), |ui| {
        command_buttons(ui, state, &[UiCommand::VisibilityGraphics, UiCommand::ViewFilters, UiCommand::ViewTemplates]);
    });
    group(ui, t!("group-temporary"), |ui| {
        use UiCommand::*;
        stacks(
            ui,
            state,
            &[
                &[TempHide, TempIsolate, TempReset],
                &[TempHideCategory, TempIsolateCategory, TempPermanent],
                &[HideInView, OverrideGraphics],
            ],
        );
    });
    group(ui, t!("group-navigate"), |ui| command_button(ui, state, UiCommand::ZoomFit));
    group(ui, t!("group-windows"), |ui| command_button(ui, state, UiCommand::CloseInactiveViews));
}

fn manage_tab(ui: &mut Ui, state: &mut AppState) {
    use UiCommand::*;
    group(ui, t!("group-project"), |ui| command_buttons(ui, state, &[ProjectInfo, Units, Levels, Materials]));
    group(ui, t!("group-graphics"), |ui| command_buttons(ui, state, &[ViewTemplates, ViewFilters]));
    group(ui, t!("group-phasing"), |ui| command_button(ui, state, Phases));
    group(ui, t!("group-macros"), |ui| {
        command_button(ui, state, MacroRecord);
        macro_menu(ui, state);
    });
}

/// Run Macro: a menu of the macros in the macros folder (see Preferences)
/// and Browse… for any other file.
fn macro_menu(ui: &mut Ui, state: &mut AppState) {
    let r = command_menu(ui, state, UiCommand::MacroRun);
    egui::Popup::menu(&r).show(|ui| {
        match state.prefs.macro_folder.clone() {
            None => {
                ui.label(theme::muted(t!("macro-no-folder")).size(12.0));
            }
            Some(folder) => {
                let files = crate::preferences::macro_files(&folder);
                if files.is_empty() {
                    ui.label(theme::muted(t!("macro-none")).size(12.0));
                }
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    for path in files {
                        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                        if ui.button(name).on_hover_text(path.display().to_string()).clicked() {
                            state.ribbon_peek = false;
                            actions::run_macro_file(state, &path);
                        }
                    }
                });
            }
        }
        ui.separator();
        if ui.button(t!("pref-browse")).clicked() {
            state.ribbon_peek = false;
            actions::run_macro(state);
        }
    });
}

/// Context options for the active tool (options bar).
/// Options of the pipe and conduit tools: the system (or circuit), the
/// elevation of the run's start and, for pipes, the fall along it. Changing
/// the system brings back its defaults.
fn segment_options(ui: &mut Ui, state: &mut AppState, type_id: cc_core::ElementTypeId, pipe: bool) {
    let mut system = if pipe { state.mep.system } else { state.mep.circuit };
    ui.label(if pipe { t!("col-system") } else { t!("prop-circuit") });
    if crate::systems_dialog::system_combo(ui, state, ("segment-system", pipe), Some(!pipe), &mut system) {
        if pipe {
            state.mep.system = system;
        } else {
            state.mep.circuit = system;
        }
        state.mep.offset = None;
        state.mep.slope = None;
    }
    let project = state.project();
    let kind = system.and_then(|id| project.system(id)).map(|s| s.kind);
    let default_offset = match (pipe, state.active_level()) {
        (true, _) => cc_engine::pipe_offset(kind),
        (false, Some(level)) => cc_engine::conduit_offset(project, level),
        (false, None) => 2.8,
    };
    let diameter = project.element_type(type_id).and_then(|t| t.diameter()).unwrap_or(0.1);
    let default_slope =
        if kind.is_some_and(cc_core::SystemKind::is_drainage) { cc_engine::drain_slope(diameter) * 100.0 } else { 0.0 };
    ui.label(t!("prop-elevation-offset"));
    let mut offset = state.mep.offset.unwrap_or(default_offset);
    if ui.add(crate::properties::length_drag(&mut offset).speed(0.01)).changed() {
        state.mep.offset = Some(offset);
        // While drawing, the chain goes up or down to it at its last point.
        if state.interaction.anchor.is_some() && state.mep.chain.is_some() {
            state.mep.rise = Some(offset);
        } else {
            state.mep.chain = None;
        }
    }
    if pipe {
        ui.label(t!("col-slope"));
        let mut slope = state.mep.slope.unwrap_or(default_slope);
        if ui.add(DragValue::new(&mut slope).speed(0.1).range(-20.0..=20.0).suffix(" %")).changed() {
            state.mep.slope = Some(slope);
        }
    }
    ui.separator();
    ui.label(theme::muted(if pipe { t!("hint-pipe") } else { t!("hint-conduit") }));
}

pub fn options_bar(ui: &mut Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        egui::Frame::new()
            .fill(PALETTE.accent_soft)
            .corner_radius(theme::RADIUS)
            .inner_margin(egui::Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (r, _) = ui.allocate_exact_size(vec2(15.0, 15.0), Sense::hover());
                icons::paint(ui.painter(), r, tool_icon(&state.tool), PALETTE.accent_text);
                let name = crate::i18n::tr(state.tool.key());
                ui.label(RichText::new(name).family(theme::semibold()).color(PALETTE.accent_text));
            });
        ui.separator();
        if let (Some(category), Some(type_id)) = (state.tool.category(), state.tool.type_id()) {
            let types: Vec<_> = state.project().types_of(category).map(|t| (t.id, t.name.clone())).collect();
            let mut selected = type_id;
            egui::ComboBox::from_id_salt("tool_type")
                .selected_text(state.project().type_name(type_id).to_string())
                .width(200.0)
                .show_ui(ui, |ui| {
                    for (id, name) in types {
                        ui.selectable_value(&mut selected, id, name);
                    }
                });
            if selected != type_id {
                state.tool = state.tool.with_type(selected);
            }
            if ui.button(t!("props-edit-type")).clicked() {
                state.dialogs.type_editor = Some(type_id);
            }
            ui.separator();
        }
        match state.tool.clone() {
            Tool::Select => {
                ui.label(theme::muted(t!("hint-select")));
            }
            Tool::Wall { .. } => {
                // Straight walls, or arcs through a third point.
                let mut arc = state.wall_arc;
                ui.selectable_value(&mut arc, false, t!("wall-mode-line")).on_hover_text(t!("hint-wall"));
                ui.selectable_value(&mut arc, true, t!("wall-mode-arc")).on_hover_text(t!("hint-wall-arc"));
                if arc != state.wall_arc {
                    state.wall_arc = arc;
                    state.interaction.points.clear();
                }
                ui.separator();
                ui.label(t!("prop-top-constraint"));
                let levels: Vec<_> = state.project().levels.iter().map(|l| (l.id, l.name.clone())).collect();
                let current = match state.wall_top_level.and_then(|id| levels.iter().find(|l| l.0 == id)) {
                    Some((_, name)) => t!("top-up-to", "level" => name),
                    None => t!("top-unconnected"),
                };
                egui::ComboBox::from_id_salt("wall_top").selected_text(current).width(170.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.wall_top_level, None, t!("top-unconnected"));
                    for (id, name) in &levels {
                        ui.selectable_value(&mut state.wall_top_level, Some(*id), t!("top-up-to", "level" => name));
                    }
                });
                if state.wall_top_level.is_none() {
                    ui.add(crate::properties::length_drag(&mut state.wall_height).speed(0.01).range(0.1..=50.0));
                }
                ui.separator();
                ui.label(t!("prop-location-line"));
                let location = &mut state.wall_location_line;
                egui::ComboBox::from_id_salt("wall_location")
                    .selected_text(crate::i18n::tr(location.key()))
                    .width(170.0)
                    .show_ui(ui, |ui| {
                        for l in cc_core::LocationLine::ALL {
                            ui.selectable_value(location, l, crate::i18n::tr(l.key()));
                        }
                    });
                ui.separator();
                ui.label(theme::muted(if state.wall_arc { t!("hint-wall-arc") } else { t!("hint-wall") }));
            }
            Tool::Hosted { .. } => {
                ui.checkbox(&mut state.interaction.flip_hand, t!("prop-flip-hand"));
                ui.label(theme::muted(t!("hint-hosted")));
            }
            Tool::Sketch { category, type_id } => {
                let n = state.interaction.points.len();
                if ui.add_enabled(n >= 3, Button::new(t!("sketch-finish"))).clicked()
                    && let Some(level) = state.active_level()
                {
                    let pts = state.interaction.points.clone();
                    crate::tools::finish_sketch(state, level, category, type_id, pts);
                }
                if ui.button(t!("sketch-from-walls")).on_hover_text(t!("hint-from-walls")).clicked() {
                    match state.active_level() {
                        Some(level) => {
                            let outward = if category == Category::Ceiling { -1.0 } else { 1.0 };
                            match walls_loop(state.project(), level, outward) {
                                Some(pts) => crate::tools::finish_sketch(state, level, category, type_id, pts),
                                None => state.message = t!("msg-no-wall-loop"),
                            }
                        }
                        None => state.message = t!("msg-open-plan"),
                    }
                }
                if ui.add_enabled(n > 0, Button::new(t!("action-cancel"))).clicked() {
                    state.interaction.points.clear();
                }
                ui.label(theme::muted(t!("hint-sketch")));
            }
            Tool::Section => {
                ui.label(theme::muted(t!("msg-section-hint")));
            }
            Tool::Opening { host, floor } => {
                let n = state.interaction.points.len();
                if ui.add_enabled(n >= 3, Button::new(t!("sketch-finish"))).clicked() {
                    let pts = state.interaction.points.clone();
                    crate::tools::add_opening(state, host, pts);
                }
                if ui.button(t!("action-cancel")).clicked() {
                    state.set_tool(Tool::Select);
                }
                ui.label(theme::muted(if floor { t!("hint-floor-opening") } else { t!("hint-roof-opening") }));
            }
            Tool::Room { .. } => {
                ui.label(theme::muted(t!("hint-room")));
            }
            Tool::Grid { .. } => {
                ui.label(theme::muted(t!("hint-grid")));
            }
            Tool::Dimension { mode, .. } => {
                if matches!(mode, DimensionMode::Aligned | DimensionMode::Linear) {
                    let c = &mut state.dim_continuous;
                    ui.selectable_value(c, false, t!("dim-single")).on_hover_text(t!("dim-single-tip"));
                    ui.selectable_value(c, true, t!("dim-continuous")).on_hover_text(t!("dim-continuous-tip"));
                    ui.separator();
                }
                let hint = if state.interaction.dim_chain.is_some_and(|c| c.ordinate) {
                    t!("hint-dim-ordinate-next")
                } else if state.interaction.dim_chain.is_some() {
                    t!("hint-dim-chain")
                } else {
                    crate::i18n::tr(mode.hint_key())
                };
                let r = ui.label(theme::muted(hint));
                if matches!(mode, DimensionMode::Aligned | DimensionMode::Linear | DimensionMode::Ordinate) {
                    r.on_hover_text(t!("hint-dim-align"));
                }
            }
            Tool::Text { .. } => crate::text_tool::options(ui, state, false),
            Tool::AddLeader { .. } => crate::text_tool::options(ui, state, true),
            Tool::Column { .. } => {
                ui.label(theme::muted(t!("hint-column")));
            }
            Tool::Beam { .. } => {
                ui.label(theme::muted(t!("hint-beam")));
            }
            Tool::Footing { .. } => {
                ui.label(theme::muted(t!("hint-footing")));
            }
            Tool::Truss { .. } => {
                let o = &mut state.truss;
                ui.label(t!("prop-truss-height"));
                ui.add(crate::properties::length_drag(&mut o.height).speed(0.01).range(0.1..=20.0));
                ui.label(t!("prop-truss-panels"));
                ui.add(
                    DragValue::new(&mut o.panels)
                        .range(0..=100)
                        .custom_formatter(|n, _| if n < 0.5 { t!("truss-panels-auto") } else { format!("{n:.0}") }),
                );
                let current = crate::tools::TRUSS_PATTERNS
                    .iter()
                    .find(|(p, _)| *p == o.pattern)
                    .map_or(String::new(), |(_, key)| crate::i18n::tr(key));
                egui::ComboBox::from_id_salt("truss-pattern").selected_text(current).show_ui(ui, |ui| {
                    for (pattern, key) in crate::tools::TRUSS_PATTERNS {
                        ui.selectable_value(&mut o.pattern, pattern, crate::i18n::tr(key));
                    }
                });
                ui.separator();
                ui.label(theme::muted(t!("hint-truss")));
            }
            Tool::Stair { .. } => crate::stair_ui::tool_options(ui, state),
            Tool::Ramp { .. } => crate::stair_ui::ramp_tool_options(ui, state),
            Tool::Pipe { type_id } => segment_options(ui, state, type_id, true),
            Tool::Conduit { type_id } => segment_options(ui, state, type_id, false),
            Tool::Component { type_id } if crate::tools::device_type(state.project(), type_id).is_some() => {
                ui.label(t!("prop-rotation"));
                let rotation = &mut state.interaction.rotation;
                if ui.add(DragValue::new(rotation).speed(1.0).suffix("°")).changed() {
                    *rotation = rotation.rem_euclid(360.0);
                }
                ui.label(t!("prop-circuit"));
                let mut circuit = state.mep.circuit;
                if crate::systems_dialog::system_combo(
                    ui,
                    state,
                    ("device-circuit", type_id.0),
                    Some(true),
                    &mut circuit,
                ) {
                    state.mep.circuit = circuit;
                }
                ui.label(theme::muted(t!("hint-device")));
            }
            Tool::Component { .. } => {
                ui.label(t!("prop-rotation"));
                let rotation = &mut state.interaction.rotation;
                if ui.add(DragValue::new(rotation).speed(1.0).suffix("°")).changed() {
                    *rotation = rotation.rem_euclid(360.0);
                }
                ui.label(theme::muted(t!("hint-component")));
            }
            Tool::Move => {
                ui.label(theme::muted(t!("hint-base-point")));
            }
            Tool::Copy => {
                ui.checkbox(&mut state.modify.multiple, t!("opt-multiple"));
                ui.label(theme::muted(t!("hint-base-point")));
            }
            Tool::Array => {
                ui.label(t!("opt-count"));
                ui.add(DragValue::new(&mut state.modify.count).range(2..=200));
                ui.label(theme::muted(t!("hint-array")));
            }
            Tool::Rotate => {
                ui.checkbox(&mut state.modify.rotate_copy, t!("opt-copy"));
                ui.separator();
                ui.label(t!("prop-rotation"));
                ui.add(DragValue::new(&mut state.modify.angle).speed(1.0).suffix("°"));
                if ui.button(t!("action-apply")).clicked()
                    && let Some(center) = state.interaction.center
                {
                    let targets = state.interaction.targets.iter().copied().collect();
                    crate::modify::apply_rotation(state, &targets, center, state.modify.angle.to_radians());
                }
                if ui.button(t!("opt-set-center")).clicked() {
                    state.interaction.placing_center = true;
                }
                ui.label(theme::muted(t!("hint-rotate")));
            }
            Tool::Offset => {
                ui.label(t!("opt-distance"));
                ui.add(crate::properties::length_drag(&mut state.modify.offset).range(0.001..=100.0));
                ui.label(theme::muted(t!("hint-offset")));
            }
            Tool::TrimCorner | Tool::TrimExtend | Tool::Split | Tool::Align | Tool::ToggleJoin => {
                let key = match state.tool {
                    Tool::TrimCorner => "hint-trim-corner",
                    Tool::TrimExtend => "hint-trim-extend",
                    Tool::Split => "hint-split",
                    Tool::ToggleJoin => "hint-toggle-join",
                    _ => "hint-align",
                };
                ui.label(theme::muted(crate::i18n::tr(key)));
            }
            Tool::Measure => {
                ui.label(theme::muted(t!("hint-measure")));
            }
            Tool::PushPull => {
                let key = if state.interaction.push.is_some() { "hint-push-pull-drag" } else { "hint-push-pull" };
                ui.label(theme::muted(crate::i18n::tr(key)));
            }
            Tool::JoinGeometry { unjoin } => {
                let key = match (state.interaction.targets.is_empty(), unjoin) {
                    (true, _) => "hint-join-pick",
                    (false, false) => "hint-join-second",
                    (false, true) => "hint-unjoin-second",
                };
                ui.label(theme::muted(crate::i18n::tr(key)));
            }
            Tool::Paste => {
                ui.label(theme::muted(t!("hint-paste")));
            }
            Tool::MatchType => {
                ui.label(theme::muted(t!("hint-match-type")));
            }
            Tool::Demolish => {
                ui.label(theme::muted(t!("hint-demolish")));
            }
            Tool::Mirror { draw } => {
                ui.checkbox(&mut state.modify.mirror_copy, t!("opt-copy"));
                ui.label(theme::muted(if draw { t!("hint-mirror-draw") } else { t!("hint-mirror-pick") }));
            }
            Tool::Railing { type_id } => {
                let n = state.interaction.points.len();
                if ui.add_enabled(n >= 2, Button::new(t!("sketch-finish"))).clicked()
                    && let Some(level) = state.active_level()
                {
                    crate::tools::finish_railing(state, level, type_id);
                }
                if ui.add_enabled(n > 0, Button::new(t!("action-cancel"))).clicked() {
                    state.interaction.points.clear();
                }
                ui.label(theme::muted(t!("hint-railing")));
            }
        }
    });
}

pub fn status_bar(ui: &mut Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        if let Some(c) = state.cursor {
            let coords = format!("X {:>9}  Y {:>9}", format::meters(c.x), format::meters(c.y));
            ui.label(RichText::new(coords).monospace().color(PALETTE.text_muted));
            ui.separator();
        }
        if !state.typed.length.is_empty() {
            ui.label(RichText::new(t!("status-length", "value" => state.typed.length)).strong());
            ui.separator();
        }
        if !state.typed.angle.is_empty() || state.typed.angle_active {
            ui.label(RichText::new(t!("status-angle", "value" => state.typed.angle)).strong());
            ui.separator();
        }
        let n = state.selection.len();
        if n > 0 {
            ui.label(t!("status-selected", "count" => n));
            ui.separator();
        }
        ui.label(theme::muted(&state.message));
        if state.updates.available.is_some() {
            ui.separator();
            crate::update::notice(ui, &mut state.updates);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let file = state
                .session
                .doc
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| t!("status-unsaved-file"));
            let dirty = if state.session.doc.is_dirty() { " *" } else { "" };
            ui.label(RichText::new(format!("{file}{dirty}")).color(PALETTE.text_faint));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_labels_wrap_on_two_balanced_lines() {
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx);
        let mut lines = Vec::new();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let font = egui::FontId::proportional(12.0);
            lines = [
                "Níveis",
                "Unidades do projeto",
                "Informações do projeto",
                "Relatório de perda de pressão da tubulação",
                "Supercalifragilístico",
            ]
            .map(|l| label_lines(ui, l, &font))
            .to_vec();
        });
        out.textures_delta.clear();
        assert_eq!(lines[0], vec!["Níveis"]);
        // Long labels break at a space into two lines that keep all the words.
        for (i, label) in [
            (1, "Unidades do projeto"),
            (2, "Informações do projeto"),
            (3, "Relatório de perda de pressão da tubulação"),
        ] {
            assert_eq!(lines[i].len(), 2, "{label}");
            assert_eq!(lines[i].join(" "), label);
            assert!(lines[i].iter().all(|l| l.len() < label.len()));
        }
        // A single long word cannot wrap.
        assert_eq!(lines[4], vec!["Supercalifragilístico"]);
    }

    #[test]
    fn families_of_two_or_four_use_two_rows() {
        assert_eq!([1, 2, 3, 4, 5, 6, 7].map(stack_rows), [3, 2, 3, 2, 3, 3, 3]);
    }
}
