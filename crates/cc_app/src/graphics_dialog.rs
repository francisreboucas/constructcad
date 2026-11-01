//! View graphics dialogs: Visibility/Graphics of a view or view template
//! (categories and filters), graphic overrides of selected elements, the
//! view filter editor and the view template manager. Every change goes
//! through the engine's commands; OK keeps the changes, Cancel undoes them.

use crate::dialog::{Dialog, session};
use crate::i18n::tr;
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use crate::theme::PALETTE;
use cc_core::{
    Category, CategoryGraphics, ElementId, FillGraphics, FilterId, FilterOp, FilterParam, FilterUse, GraphicOverride,
    LineGraphics, LinePattern, Pattern, ScheduleField, TemplateId, ViewGraphics, ViewId,
};
use cc_engine::Ref;
use cc_engine::commands::graphics as cmd;
use egui::{Color32, Context, RichText, Sense, Stroke, Ui, vec2};
use std::hash::{Hash, Hasher};

/// Whose graphics are edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    View(ViewId),
    Template(TemplateId),
}

impl Target {
    fn view_ref(self) -> Option<Ref> {
        match self {
            Target::View(v) => Some(Ref::Id(v.0)),
            Target::Template(_) => None,
        }
    }

    fn template_ref(self) -> Option<Ref> {
        match self {
            Target::Template(t) => Some(Ref::Id(t.0)),
            Target::View(_) => None,
        }
    }
}

/// Edits made while a dialog is open, cancelled as a whole. A dialog opened
/// over another one undoes back to its own start (the outer one owns the
/// shared [`session`]).
#[derive(Debug, Clone, Copy)]
pub struct Edits {
    start: usize,
    owns: bool,
}

impl Edits {
    pub(crate) fn begin(state: &mut AppState) -> Self {
        let owns = state.dialogs.edit_start.is_none();
        if owns {
            session::begin(state);
        }
        state.session.doc.break_merge();
        Self { start: state.session.doc.undo_depth(), owns }
    }

    pub(crate) fn finish(self, state: &mut AppState, ok: bool) {
        state.session.doc.break_merge();
        match (ok, self.owns) {
            (true, true) => session::commit(state),
            (true, false) => {}
            (false, true) => session::cancel(state),
            (false, false) => state.session.undo_to(self.start),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Model,
    Annotation,
    Filters,
}

/// The open Visibility/Graphics dialog.
pub struct GraphicsDialog {
    pub target: Target,
    tab: Tab,
    edits: Edits,
}

/// The open element override dialog.
pub struct OverrideDialog {
    view: ViewId,
    ids: Vec<ElementId>,
    value: GraphicOverride,
    edits: Edits,
}

/// The open filter editor.
pub struct FilterEditor {
    selected: Option<FilterId>,
    edits: Edits,
}

/// The open view template manager.
pub struct TemplateManager {
    selected: Option<TemplateId>,
    edits: Edits,
}

pub(crate) fn key(v: impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

pub(crate) fn run<C: cc_engine::Command + serde::Serialize>(
    state: &mut AppState,
    merge: Option<u64>,
    command: C,
) -> bool {
    let result = match merge {
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

pub(crate) fn overrides_json(o: &GraphicOverride) -> serde_json::Value {
    serde_json::to_value(o).unwrap_or_default()
}

// ---------------------------------------------------------------- opening

/// Open Visibility/Graphics for a view or template.
pub fn open(state: &mut AppState, target: Target) {
    let edits = Edits::begin(state);
    state.dialogs.graphics = Some(GraphicsDialog { target, tab: Tab::Model, edits });
}

/// Open the override dialog for the selected elements in the active view.
pub fn open_overrides(state: &mut AppState) {
    let Some(view) = state.active_view else { return };
    let ids: Vec<ElementId> = state.selection.iter().copied().collect();
    let Some(first) = ids.first() else { return };
    let value = state.project().view(view).and_then(|v| v.graphics.elements.get(first).copied()).unwrap_or_default();
    let edits = Edits::begin(state);
    state.dialogs.element_override = Some(OverrideDialog { view, ids, value, edits });
}

pub fn open_filters(state: &mut AppState) {
    let selected = state.project().filters.keys().next().copied();
    let edits = Edits::begin(state);
    state.dialogs.filters = Some(FilterEditor { selected, edits });
}

pub fn open_templates(state: &mut AppState) {
    let selected = state
        .active_view
        .and_then(|v| state.project().view(v))
        .and_then(|v| v.template)
        .or_else(|| state.project().templates.keys().next().copied());
    let edits = Edits::begin(state);
    state.dialogs.templates = Some(TemplateManager { selected, edits });
}

pub fn show(ctx: &Context, state: &mut AppState) {
    templates_dialog(ctx, state);
    graphics_dialog(ctx, state);
    override_dialog(ctx, state);
    filters_dialog(ctx, state);
}

// ---------------------------------------------------------------- cells

const CELL: egui::Vec2 = egui::Vec2::new(80.0, 18.0);

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// A clickable cell that opens an editor popup; `paint` draws its content.
fn cell(ui: &mut Ui, label: &str, enabled: bool, paint: impl FnOnce(&egui::Painter, egui::Rect)) -> egui::Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, r) = ui.allocate_exact_size(CELL, sense);
    let fill = if r.hovered() && enabled { PALETTE.hover } else { PALETTE.panel };
    ui.painter().rect_filled(rect, 3.0, fill);
    ui.painter().rect_stroke(rect, 3.0, Stroke::new(1.0, PALETTE.border), egui::StrokeKind::Inside);
    paint(ui.painter(), rect.shrink2(vec2(6.0, 3.0)));
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    r.on_hover_text(label)
}

fn empty_mark(p: &egui::Painter, r: egui::Rect) {
    p.text(r.center(), egui::Align2::CENTER_CENTER, "—", egui::FontId::proportional(11.0), PALETTE.text_faint);
}

fn popup<R>(r: &egui::Response, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    egui::Popup::menu(r)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(220.0);
            add(ui)
        })
        .map(|i| i.inner)
}

/// An optional color: a checkbox and, when on, an inline picker.
fn color_option(ui: &mut Ui, label: &str, color: &mut Option<[u8; 3]>) -> bool {
    let mut on = color.is_some();
    let mut changed = ui.checkbox(&mut on, label).changed();
    if changed {
        *color = on.then_some(color.unwrap_or([0, 0, 0]));
    }
    if let Some(c) = color {
        let mut c32 = rgb(*c);
        if egui::color_picker::color_picker_color32(ui, &mut c32, egui::color_picker::Alpha::Opaque) {
            *c = [c32.r(), c32.g(), c32.b()];
            changed = true;
        }
    }
    changed
}

fn line_cell(ui: &mut Ui, label: &str, lines: &mut LineGraphics, enabled: bool) -> bool {
    let shown = *lines;
    let r = cell(ui, label, enabled, |p, rect| {
        if shown.is_empty() {
            return empty_mark(p, rect);
        }
        let color = shown.color.map_or(Color32::from_gray(45), rgb);
        let width = shown.weight.map_or(1.2, |w| (w as f32 * 4.0).clamp(0.6, 5.0));
        let stroke = Stroke::new(width, color);
        let pts = [rect.left_center(), rect.right_center()];
        match shown.pattern.map(LinePattern::dashes_mm).filter(|d| d.len() >= 2) {
            Some(d) => {
                let (on, off): (Vec<f32>, Vec<f32>) = d
                    .chunks(2)
                    .filter(|c| c.len() == 2)
                    .map(|c| ((c[0] * 3.0).max(1.0) as f32, (c[1] * 3.0) as f32))
                    .unzip();
                p.extend(egui::Shape::dashed_line_with_offset(&pts, stroke, &on, &off, 0.0));
            }
            None => {
                p.line_segment(pts, stroke);
            }
        }
    });
    let mut changed = false;
    popup(&r, |ui| {
        changed |= color_option(ui, &t!("vg-color"), &mut lines.color);
        ui.separator();
        let mut on = lines.weight.is_some();
        if ui.checkbox(&mut on, t!("vg-weight")).changed() {
            lines.weight = on.then_some(lines.weight.unwrap_or(0.35));
            changed = true;
        }
        if let Some(w) = &mut lines.weight {
            changed |= ui.add(egui::DragValue::new(w).range(0.05..=5.0).speed(0.01).suffix(" mm")).changed();
        }
        ui.separator();
        ui.label(RichText::new(t!("vg-pattern")).color(PALETTE.text_muted));
        let mut pattern = lines.pattern;
        ui.selectable_value(&mut pattern, None, t!("vg-by-default"));
        for lp in LinePattern::ALL {
            ui.selectable_value(&mut pattern, Some(lp), tr(lp.key()));
        }
        if pattern != lines.pattern {
            lines.pattern = pattern;
            changed = true;
        }
        ui.separator();
        if ui.button(t!("vg-reset")).clicked() {
            *lines = LineGraphics::default();
            changed = true;
        }
    });
    changed
}

fn fill_cell(ui: &mut Ui, label: &str, fill: &mut FillGraphics, enabled: bool) -> bool {
    let shown = *fill;
    let r = cell(ui, label, enabled, |p, rect| {
        if shown.is_empty() {
            return empty_mark(p, rect);
        }
        let color = shown.color.map_or(Color32::from_gray(150), rgb);
        let swatch = egui::Rect::from_min_size(rect.min, vec2(rect.height() * 1.6, rect.height()));
        match shown.pattern {
            Some(Pattern::None) => {
                p.rect_stroke(swatch, 1.0, Stroke::new(1.0, PALETTE.border), egui::StrokeKind::Inside);
            }
            Some(Pattern::Solid) | None => {
                p.rect_filled(swatch, 1.0, color);
            }
            Some(_) => {
                p.rect_stroke(swatch, 1.0, Stroke::new(1.0, color), egui::StrokeKind::Inside);
                for i in 0..4 {
                    let x = swatch.left() + i as f32 * swatch.width() / 3.0;
                    p.line_segment(
                        [egui::pos2(x - swatch.height() / 2.0, swatch.bottom()), egui::pos2(x, swatch.top())],
                        Stroke::new(1.0, color),
                    );
                }
            }
        }
        let text = shown.pattern.map(|pt| tr(pt.key())).unwrap_or_default();
        p.text(
            egui::pos2(swatch.right() + 4.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            text,
            egui::FontId::proportional(10.5),
            PALETTE.text_muted,
        );
    });
    let mut changed = false;
    popup(&r, |ui| {
        changed |= color_option(ui, &t!("vg-color"), &mut fill.color);
        ui.separator();
        ui.label(RichText::new(t!("vg-pattern")).color(PALETTE.text_muted));
        let mut pattern = fill.pattern;
        egui::Grid::new("fill-patterns").num_columns(2).show(ui, |ui| {
            ui.selectable_value(&mut pattern, None, t!("vg-by-default"));
            for (i, pt) in Pattern::ALL.iter().enumerate() {
                if i % 2 == 0 {
                    ui.end_row();
                }
                ui.selectable_value(&mut pattern, Some(*pt), tr(pt.key()));
            }
        });
        if pattern != fill.pattern {
            fill.pattern = pattern;
            changed = true;
        }
        ui.separator();
        if ui.button(t!("vg-reset")).clicked() {
            *fill = FillGraphics::default();
            changed = true;
        }
    });
    changed
}

fn transparency_cell(ui: &mut Ui, label: &str, value: &mut Option<f64>, enabled: bool) -> bool {
    let shown = *value;
    let r = cell(ui, label, enabled, |p, rect| match shown {
        None => empty_mark(p, rect),
        Some(t) => {
            p.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("{:.0} %", t * 100.0),
                egui::FontId::proportional(11.0),
                PALETTE.text,
            );
        }
    });
    let mut changed = false;
    popup(&r, |ui| {
        let mut on = value.is_some();
        if ui.checkbox(&mut on, t!("vg-transparency")).changed() {
            *value = on.then_some(value.unwrap_or(0.5));
            changed = true;
        }
        if let Some(t) = value {
            let mut pct = *t * 100.0;
            if ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix(" %")).changed() {
                *t = (pct / 100.0).clamp(0.0, 1.0);
                changed = true;
            }
        }
    });
    changed
}

fn halftone_cell(ui: &mut Ui, label: &str, value: &mut Option<bool>, enabled: bool) -> bool {
    let mut on = *value == Some(true);
    let r = ui.add_enabled(enabled, egui::Checkbox::without_text(&mut on)).on_hover_text(label);
    if r.changed() {
        *value = on.then_some(true);
        return true;
    }
    false
}

/// The override cells of one row, in the column order of [`override_headers`].
pub(crate) fn override_cells(ui: &mut Ui, ov: &mut GraphicOverride, enabled: bool) -> bool {
    let mut changed = false;
    changed |= line_cell(ui, &t!("vg-projection-lines"), &mut ov.projection_lines, enabled);
    changed |= fill_cell(ui, &t!("vg-surface-fill"), &mut ov.surface_fill, enabled);
    changed |= line_cell(ui, &t!("vg-cut-lines"), &mut ov.cut_lines, enabled);
    changed |= fill_cell(ui, &t!("vg-cut-fill"), &mut ov.cut_fill, enabled);
    changed |= halftone_cell(ui, &t!("vg-halftone"), &mut ov.halftone, enabled);
    changed |= transparency_cell(ui, &t!("vg-transparency"), &mut ov.transparency, enabled);
    changed
}

pub(crate) fn header(ui: &mut Ui, text: String) {
    ui.label(RichText::new(text).size(11.5).color(PALETTE.text_muted));
}

pub(crate) fn override_headers(ui: &mut Ui) {
    for k in ["vg-projection-lines", "vg-surface-fill", "vg-cut-lines", "vg-cut-fill", "vg-halftone", "vg-transparency"]
    {
        header(ui, tr(k));
    }
}

// ---------------------------------------------------------------- Visibility/Graphics

fn target_graphics(state: &AppState, target: Target) -> Option<ViewGraphics> {
    match target {
        Target::View(v) => state.project().view(v).map(|v| v.graphics.clone()),
        Target::Template(t) => state.project().template(t).map(|t| t.graphics.clone()),
    }
}

/// The template that controls the view's categories or filters, if any.
fn controlling_template(state: &AppState, target: Target, filters: bool) -> Option<String> {
    let Target::View(v) = target else { return None };
    let t = state.project().template(state.project().view(v)?.template?)?;
    let controlled = if filters { t.include.filters } else { t.include.categories };
    controlled.then(|| t.name.clone())
}

fn graphics_title(state: &AppState, target: Target) -> String {
    let name = match target {
        Target::View(v) => state.project().view(v).map(|v| v.name.clone()),
        Target::Template(t) => state.project().template(t).map(|t| t.name.clone()),
    };
    format!("{}: {}", t!("dlg-visibility-graphics"), name.unwrap_or_default())
}

fn set_category(state: &mut AppState, target: Target, c: Category, g: CategoryGraphics, merge: Option<u64>) {
    let command = cmd::ViewGraphicsSet {
        view: target.view_ref(),
        template: target.template_ref(),
        category: cc_engine::category_name(c),
        visible: Some(g.visible),
        overrides: Some(overrides_json(&g.overrides)),
        reset: true,
    };
    run(state, merge, command);
}

fn set_filter_use(state: &mut AppState, target: Target, u: FilterUse) {
    let command = cmd::ViewFilterSet {
        view: target.view_ref(),
        template: target.template_ref(),
        filter: Ref::Id(u.filter.0),
        visible: Some(u.visible),
        enabled: Some(u.enabled),
        overrides: Some(overrides_json(&u.overrides)),
        reset: true,
    };
    run(state, None, command);
}

fn graphics_dialog(ctx: &Context, state: &mut AppState) {
    let Some((target, tab)) = state.dialogs.graphics.as_ref().map(|d| (d.target, d.tab)) else { return };
    let Some(graphics) = target_graphics(state, target) else {
        if let Some(d) = state.dialogs.graphics.take() {
            d.edits.finish(state, false);
        }
        return;
    };
    let mut new_tab = tab;
    let (mut ok, mut filters_button) = (0, usize::MAX);
    let title = graphics_title(state, target);
    let response = Dialog::new("visibility-graphics", title).width(900.0).show(
        ctx,
        |ui| {
            ui.horizontal(|ui| {
                for (t, label) in [
                    (Tab::Model, t!("vg-model-categories")),
                    (Tab::Annotation, t!("vg-annotation-categories")),
                    (Tab::Filters, t!("vg-filters")),
                ] {
                    ui.selectable_value(&mut new_tab, t, label);
                }
            });
            ui.separator();
            let filters = new_tab == Tab::Filters;
            if let Some(name) = controlling_template(state, target, filters) {
                ui.label(RichText::new(t!("msg-controlled-by-template", "template" => name)).color(PALETTE.warning));
            }
            let enabled = controlling_template(state, target, filters).is_none();
            egui::ScrollArea::vertical().max_height(440.0).show(ui, |ui| match new_tab {
                Tab::Model => categories_table(ui, state, target, &graphics, &Category::MODEL, enabled),
                Tab::Annotation => {
                    let annotation: Vec<Category> =
                        Category::ALL.into_iter().filter(|c| !Category::MODEL.contains(c)).collect();
                    categories_table(ui, state, target, &graphics, &annotation, enabled)
                }
                Tab::Filters => filters_table(ui, state, target, &graphics, enabled),
            });
            if let Target::View(v) = target
                && !graphics.hidden.is_empty()
            {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(t!("vg-hidden-elements", "count" => graphics.hidden.len()));
                    if ui.button(t!("vg-show-hidden")).clicked() {
                        let ids = graphics.hidden.iter().map(|e| e.0).collect();
                        run(state, None, cmd::ElementHideInView { view: Ref::Id(v.0), ids, hidden: false });
                    }
                });
            }
        },
        |f| {
            filters_button = f.extra(t!("vg-edit-filters"));
            ok = f.primary(t!("action-ok"));
            f.secondary(t!("action-cancel"));
        },
    );
    if let Some(d) = &mut state.dialogs.graphics {
        d.tab = new_tab;
    }
    if response.clicked == Some(filters_button) {
        open_filters(state);
        return;
    }
    let close = match response.clicked {
        Some(i) => Some(i == ok),
        None if response.dismissed => Some(false),
        None => None,
    };
    if let Some(keep) = close
        && let Some(d) = state.dialogs.graphics.take()
    {
        d.edits.finish(state, keep);
    }
}

fn categories_table(
    ui: &mut Ui,
    state: &mut AppState,
    target: Target,
    graphics: &ViewGraphics,
    categories: &[Category],
    enabled: bool,
) {
    egui::Grid::new("vg-categories").num_columns(8).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
        header(ui, t!("vg-visible"));
        header(ui, t!("prop-category"));
        override_headers(ui);
        ui.end_row();
        for c in categories {
            let mut g = graphics.category(*c);
            let name = crate::actions::t_category(*c);
            let r = ui
                .push_id(("visible", *c), |ui| ui.add_enabled(enabled, egui::Checkbox::without_text(&mut g.visible)))
                .inner;
            let label = format!("{}: {name}", t!("vg-visible"));
            let on = g.visible;
            r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, on, &label));
            let mut changed = r.changed();
            ui.label(name);
            changed |= override_cells(ui, &mut g.overrides, enabled);
            if ui.add_enabled(enabled && !g.overrides.is_empty(), egui::Button::new(t!("vg-reset")).small()).clicked() {
                g.overrides = GraphicOverride::default();
                changed = true;
            }
            ui.end_row();
            if changed {
                set_category(state, target, *c, g, Some(key(("vg", target, *c))));
            }
        }
    });
}

fn filters_table(ui: &mut Ui, state: &mut AppState, target: Target, graphics: &ViewGraphics, enabled: bool) {
    if graphics.filters.is_empty() {
        ui.label(RichText::new(t!("vg-no-filters")).color(PALETTE.text_faint));
    }
    let count = graphics.filters.len();
    egui::Grid::new("vg-filters").num_columns(10).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
        if count > 0 {
            header(ui, t!("vg-enabled"));
            header(ui, t!("vg-filter"));
            header(ui, t!("vg-visible"));
            override_headers(ui);
            ui.label("");
            ui.end_row();
        }
        for (i, u) in graphics.filters.iter().enumerate() {
            let mut u = *u;
            let name = state.project().filter(u.filter).map(|f| f.name.clone()).unwrap_or_default();
            let mut changed = ui.add_enabled(enabled, egui::Checkbox::without_text(&mut u.enabled)).changed();
            ui.label(name);
            changed |= ui.add_enabled(enabled, egui::Checkbox::without_text(&mut u.visible)).changed();
            changed |= override_cells(ui, &mut u.overrides, enabled);
            ui.horizontal(|ui| {
                let filter = Ref::Id(u.filter.0);
                let moved = |index| cmd::ViewFilterMove {
                    view: target.view_ref(),
                    template: target.template_ref(),
                    filter: filter.clone(),
                    index,
                };
                if ui
                    .add_enabled(enabled && i > 0, egui::Button::new("↑").small())
                    .on_hover_text(t!("vg-move-up"))
                    .clicked()
                {
                    run(state, None, moved(i - 1));
                }
                if ui
                    .add_enabled(enabled && i + 1 < count, egui::Button::new("↓").small())
                    .on_hover_text(t!("vg-move-down"))
                    .clicked()
                {
                    run(state, None, moved(i + 1));
                }
                if ui.add_enabled(enabled, egui::Button::new("×").small()).on_hover_text(t!("vg-remove")).clicked() {
                    let command = cmd::ViewFilterRemove {
                        view: target.view_ref(),
                        template: target.template_ref(),
                        filter: filter.clone(),
                    };
                    run(state, None, command);
                }
            });
            ui.end_row();
            if changed {
                set_filter_use(state, target, u);
            }
        }
    });
    ui.add_space(6.0);
    let available: Vec<(FilterId, String)> = state
        .project()
        .filters
        .values()
        .filter(|f| !graphics.filters.iter().any(|u| u.filter == f.id))
        .map(|f| (f.id, f.name.clone()))
        .collect();
    ui.add_enabled_ui(enabled && !available.is_empty(), |ui| {
        egui::ComboBox::from_id_salt("vg-add-filter").selected_text(t!("vg-add-filter")).show_ui(ui, |ui| {
            for (id, name) in &available {
                if ui.selectable_label(false, name).clicked() {
                    let command = cmd::ViewFilterAdd {
                        view: target.view_ref(),
                        template: target.template_ref(),
                        filter: Ref::Id(id.0),
                        visible: None,
                        overrides: None,
                        index: None,
                    };
                    run(state, None, command);
                }
            }
        });
    });
    ui.label(RichText::new(t!("vg-filter-priority")).size(11.0).color(PALETTE.text_faint));
}

// ---------------------------------------------------------------- element overrides

fn override_dialog(ctx: &Context, state: &mut AppState) {
    let Some((view, count, mut value)) =
        state.dialogs.element_override.as_ref().map(|d| (d.view, d.ids.len(), d.value))
    else {
        return;
    };
    let (mut ok, mut reset) = (0, 0);
    let mut changed = false;
    let response = Dialog::new("element-overrides", t!("dlg-element-overrides")).width(640.0).show(
        ctx,
        |ui| {
            ui.label(t!("vg-override-intro", "count" => count));
            ui.add_space(4.0);
            egui::Grid::new("vg-element").num_columns(6).spacing([8.0, 4.0]).show(ui, |ui| {
                override_headers(ui);
                ui.end_row();
                changed = override_cells(ui, &mut value, true);
                ui.end_row();
            });
        },
        |f| {
            reset = f.extra(t!("vg-reset"));
            ok = f.primary(t!("action-ok"));
            f.secondary(t!("action-cancel"));
        },
    );
    let ids: Vec<u64> =
        state.dialogs.element_override.as_ref().map(|d| d.ids.iter().map(|e| e.0).collect()).unwrap_or_default();
    if response.clicked == Some(reset) {
        value = GraphicOverride::default();
        changed = true;
    }
    if changed {
        let command = cmd::ElementOverrideSet {
            view: Ref::Id(view.0),
            ids,
            overrides: Some(overrides_json(&value)),
            reset: true,
        };
        run(state, Some(key(("element-override", view))), command);
        if let Some(d) = &mut state.dialogs.element_override {
            d.value = value;
        }
    }
    let close = match response.clicked {
        Some(i) if i == reset => None,
        Some(i) => Some(i == ok),
        None if response.dismissed => Some(false),
        None => None,
    };
    if let Some(keep) = close
        && let Some(d) = state.dialogs.element_override.take()
    {
        d.edits.finish(state, keep);
    }
}

// ---------------------------------------------------------------- filter editor

/// Parameters offered by the rule editor.
fn params() -> Vec<FilterParam> {
    use ScheduleField as F;
    let fields = [
        F::Type,
        F::Mark,
        F::Comments,
        F::Level,
        F::Material,
        F::System,
        F::Structural,
        F::Function,
        F::Width,
        F::Height,
        F::Thickness,
        F::Length,
        F::Area,
        F::Volume,
        F::Diameter,
        F::Sill,
        F::TypeMark,
        F::Description,
        F::Manufacturer,
        F::Model,
        F::FireRating,
        F::Name,
        F::Number,
        F::ComponentName,
    ];
    fields.into_iter().map(FilterParam::Field).chain([FilterParam::Elevation]).collect()
}

fn param_label(p: FilterParam) -> String {
    match p {
        FilterParam::Field(f) => tr(f.key()),
        FilterParam::Elevation => t!("vg-param-elevation"),
    }
}

/// Send a filter's fields to `filter_set`.
fn save_filter(state: &mut AppState, f: &cc_core::ViewFilter, merge: Option<u64>) {
    let rules = f
        .rules
        .iter()
        .map(|r| cmd::RuleArg {
            param: cmd::param_name(r.param),
            op: serde_json::to_value(r.op).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default(),
            value: Some(serde_json::Value::String(r.value.clone())),
        })
        .collect();
    let command = cmd::FilterSet {
        filter: Ref::Id(f.id.0),
        name: None,
        categories: Some(f.categories.iter().map(|c| cc_engine::category_name(*c)).collect()),
        rules: Some(rules),
        any: Some(f.any),
    };
    run(state, merge, command);
}

fn filters_dialog(ctx: &Context, state: &mut AppState) {
    let Some(selected) = state.dialogs.filters.as_ref().map(|d| d.selected) else { return };
    let selected = selected.filter(|id| state.project().filter(*id).is_some());
    let mut select = selected;
    let has_selection = selected.is_some();
    let (mut ok, mut new, mut delete) = (0, 0, usize::MAX);
    let response = Dialog::new("filters", t!("dlg-filters")).width(820.0).show(
        ctx,
        |ui| {
            crate::dialog::two_columns(ui, [200.0, 580.0], |ui, column| {
                if column == 0 {
                    egui::ScrollArea::vertical().id_salt("filter-list").max_height(420.0).show(ui, |ui| {
                        let list: Vec<(FilterId, String)> =
                            state.project().filters.values().map(|f| (f.id, f.name.clone())).collect();
                        if list.is_empty() {
                            ui.label(RichText::new(t!("vg-no-filters-yet")).color(PALETTE.text_faint));
                        }
                        for (id, name) in list {
                            if ui.selectable_label(select == Some(id), name).clicked() {
                                select = Some(id);
                            }
                        }
                    });
                    return;
                }
                match select.and_then(|id| state.project().filter(id).cloned()) {
                    Some(f) => filter_form(ui, state, f),
                    None => {
                        ui.label(RichText::new(t!("vg-select-filter")).color(PALETTE.text_faint));
                    }
                }
            });
        },
        |f| {
            new = f.extra(t!("vg-new-filter"));
            if has_selection {
                delete = f.extra(t!("vg-delete-filter"));
            }
            ok = f.primary(t!("action-ok"));
            f.secondary(t!("action-cancel"));
        },
    );
    if response.clicked == Some(new) {
        let names: Vec<String> = state.project().filters.values().map(|f| f.name.to_lowercase()).collect();
        let name = (1..1000)
            .map(|n| t!("vg-default-filter-name", "n" => n))
            .find(|n| !names.contains(&n.to_lowercase()))
            .unwrap_or_default();
        let command = cmd::FilterCreate { name, categories: Vec::new(), rules: Vec::new(), any: false };
        if let Ok(out) = state.session.run(command) {
            select = out["id"].as_u64().map(FilterId);
        }
    } else if response.clicked == Some(delete)
        && let Some(id) = select
    {
        run(state, None, cmd::FilterDelete { filter: Ref::Id(id.0) });
        select = state.project().filters.keys().next().copied();
    }
    if let Some(d) = &mut state.dialogs.filters {
        d.selected = select;
    }
    let close = match response.clicked {
        Some(i) if i == ok => Some(true),
        Some(i) if i >= 1000 => None,
        Some(_) => Some(false),
        None if response.dismissed => Some(false),
        None => None,
    };
    if let Some(keep) = close
        && let Some(d) = state.dialogs.filters.take()
    {
        d.edits.finish(state, keep);
    }
}

fn filter_form(ui: &mut Ui, state: &mut AppState, mut f: cc_core::ViewFilter) {
    let id = f.id;
    let mut name = f.name.clone();
    ui.horizontal(|ui| {
        ui.label(t!("prop-name"));
        let r = crate::table::live_text(ui, ("filter-name", id), &mut name, |ui, t| {
            ui.add(egui::TextEdit::singleline(t).desired_width(260.0))
        });
        if r.lost_focus() && name.trim() != f.name && !name.trim().is_empty() {
            let command = cmd::FilterSet {
                filter: Ref::Id(id.0),
                name: Some(name.clone()),
                categories: None,
                rules: None,
                any: None,
            };
            run(state, None, command);
        }
    });
    ui.add_space(4.0);
    let mut changed = false;
    section(ui, "filter-categories", t!("vg-filter-categories"), |ui| {
        ui.label(RichText::new(t!("vg-filter-categories-hint")).size(11.0).color(PALETTE.text_faint));
        egui::Grid::new("filter-cats").num_columns(4).show(ui, |ui| {
            for (i, c) in Category::ALL.iter().enumerate() {
                let mut on = f.categories.contains(c);
                if ui.checkbox(&mut on, crate::actions::t_category(*c)).changed() {
                    if on {
                        f.categories.insert(*c);
                    } else {
                        f.categories.remove(c);
                    }
                    changed = true;
                }
                if i % 4 == 3 {
                    ui.end_row();
                }
            }
        });
    });
    section(ui, "filter-rules", t!("vg-filter-rules"), |ui| {
        ui.horizontal(|ui| {
            changed |= ui.radio_value(&mut f.any, false, t!("vg-rules-all")).changed();
            changed |= ui.radio_value(&mut f.any, true, t!("vg-rules-any")).changed();
        });
        let mut remove = None;
        for (i, rule) in f.rules.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(("rule-param", id, i))
                    .width(150.0)
                    .selected_text(param_label(rule.param))
                    .show_ui(ui, |ui| {
                        // Only parameters the chosen categories have.
                        let applies =
                            |p: &FilterParam| f.categories.is_empty() || f.categories.iter().any(|c| p.applies_to(*c));
                        for p in params().into_iter().filter(applies) {
                            changed |= ui.selectable_value(&mut rule.param, p, param_label(p)).changed();
                        }
                    });
                egui::ComboBox::from_id_salt(("rule-op", id, i)).width(170.0).selected_text(tr(rule.op.key())).show_ui(
                    ui,
                    |ui| {
                        for op in FilterOp::ALL {
                            changed |= ui.selectable_value(&mut rule.op, op, tr(op.key())).changed();
                        }
                    },
                );
                if rule.op.needs_value() {
                    let r = crate::table::live_text(ui, ("filter-rule", id, i), &mut rule.value, |ui, t| {
                        ui.add(egui::TextEdit::singleline(t).desired_width(120.0))
                    });
                    changed |= r.changed();
                }
                if ui.small_button("×").on_hover_text(t!("vg-remove-rule")).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            f.rules.remove(i);
            changed = true;
        }
        if ui.button(t!("vg-add-rule")).clicked() {
            f.rules.push(cc_core::FilterRule {
                param: FilterParam::Field(ScheduleField::Type),
                op: FilterOp::Contains,
                value: String::new(),
            });
            changed = true;
        }
        ui.label(RichText::new(t!("vg-rules-hint")).size(11.0).color(PALETTE.text_faint));
    });
    if changed {
        save_filter(state, &f, Some(key(("filter", id))));
    }
    let count = state
        .session
        .run(cmd::FilterElements { filter: Ref::Id(id.0), view: None })
        .ok()
        .and_then(|v| v["count"].as_u64())
        .unwrap_or(0);
    ui.add_space(4.0);
    ui.label(RichText::new(t!("vg-filter-matches", "count" => count)).color(PALETTE.text_muted));
}

// ---------------------------------------------------------------- templates

fn templates_dialog(ctx: &Context, state: &mut AppState) {
    let Some(selected) = state.dialogs.templates.as_ref().map(|d| d.selected) else { return };
    let selected = selected.filter(|id| state.project().template(*id).is_some());
    let mut select = selected;
    let active = state
        .active_view
        .filter(|v| state.project().view(*v).is_some_and(|v| cc_core::ViewTemplate::applies_to(&v.kind)));
    let has_selection = selected.is_some();
    let (mut ok, mut new, mut delete) = (0, usize::MAX, usize::MAX);
    let mut edit_graphics = None;
    let response = Dialog::new("view-templates", t!("dlg-view-templates")).width(800.0).show(
        ctx,
        |ui| {
            crate::dialog::two_columns(ui, [200.0, 520.0], |ui, column| {
                if column == 0 {
                    let list: Vec<(TemplateId, String)> =
                        state.project().templates.values().map(|t| (t.id, t.name.clone())).collect();
                    if list.is_empty() {
                        ui.label(RichText::new(t!("vg-no-templates")).color(PALETTE.text_faint));
                    }
                    for (id, name) in list {
                        if ui.selectable_label(select == Some(id), name).clicked() {
                            select = Some(id);
                        }
                    }
                } else if let Some(t) = select.and_then(|id| state.project().template(id).cloned()) {
                    if template_form(ui, state, t, active) {
                        edit_graphics = select;
                    }
                } else {
                    ui.label(RichText::new(t!("vg-select-template")).color(PALETTE.text_faint));
                }
            });
        },
        |f| {
            if active.is_some() {
                new = f.extra(t!("vg-template-from-view"));
            }
            if has_selection {
                delete = f.extra(t!("vg-delete-template"));
            }
            ok = f.primary(t!("action-ok"));
            f.secondary(t!("action-cancel"));
        },
    );
    if let Some(t) = edit_graphics {
        open(state, Target::Template(t));
    }
    if response.clicked == Some(new)
        && let Some(view) = active
    {
        let base = state.project().view(view).map(|v| v.name.clone()).unwrap_or_default();
        let names: Vec<String> = state.project().templates.values().map(|t| t.name.to_lowercase()).collect();
        let name = std::iter::once(base.clone())
            .chain((2..1000).map(|n| format!("{base} {n}")))
            .find(|n| !names.contains(&n.to_lowercase()))
            .unwrap_or(base);
        if let Ok(out) = state.session.run(cmd::TemplateCreate { name, view: Some(Ref::Id(view.0)) }) {
            select = out["id"].as_u64().map(TemplateId);
        }
    } else if response.clicked == Some(delete)
        && let Some(id) = select
    {
        run(state, None, cmd::TemplateDelete { template: Ref::Id(id.0) });
        select = state.project().templates.keys().next().copied();
    }
    if let Some(d) = &mut state.dialogs.templates {
        d.selected = select;
    }
    let close = match response.clicked {
        Some(i) if i == ok => Some(true),
        Some(i) if i >= 1000 => None,
        Some(_) => Some(false),
        None if response.dismissed => Some(false),
        None => None,
    };
    if let Some(keep) = close
        && let Some(d) = state.dialogs.templates.take()
    {
        d.edits.finish(state, keep);
    }
}

const SCALES: [u32; 9] = [10, 20, 25, 50, 75, 100, 200, 250, 500];

/// The template's properties, each with its "include" box. Returns whether
/// Visibility/Graphics should open for it.
fn template_form(ui: &mut Ui, state: &mut AppState, t: cc_core::ViewTemplate, active: Option<ViewId>) -> bool {
    let id = t.id;
    let mut open_vg = false;
    let mut name = t.name.clone();
    ui.horizontal(|ui| {
        ui.label(t!("prop-name"));
        let r = crate::table::live_text(ui, ("template-name", id), &mut name, |ui, t| {
            ui.add(egui::TextEdit::singleline(t).desired_width(260.0))
        });
        if r.lost_focus() && name.trim() != t.name && !name.trim().is_empty() {
            run(
                state,
                None,
                cmd::TemplateSet { template: Ref::Id(id.0), name: Some(name.clone()), ..Default::default() },
            );
        }
    });
    ui.add_space(4.0);
    ui.label(RichText::new(t!("vg-include-hint")).size(11.0).color(PALETTE.text_faint));
    let mut include = t.include;
    let mut set = cmd::TemplateSet { template: Ref::Id(id.0), ..Default::default() };
    let mut changed = false;
    egui::Grid::new("template-props").num_columns(3).striped(true).spacing([10.0, 6.0]).show(ui, |ui| {
        header(ui, t!("vg-include"));
        header(ui, t!("vg-property"));
        header(ui, t!("vg-value"));
        ui.end_row();

        changed |= ui.checkbox(&mut include.scale, "").changed();
        ui.label(t!("prop-scale"));
        let mut scale = t.scale;
        egui::ComboBox::from_id_salt(("tpl-scale", id)).selected_text(format!("1:{scale}")).show_ui(ui, |ui| {
            for s in SCALES {
                ui.selectable_value(&mut scale, s, format!("1:{s}"));
            }
        });
        if scale != t.scale {
            set.scale = Some(scale);
        }
        ui.end_row();

        changed |= ui.checkbox(&mut include.detail, "").changed();
        ui.label(t!("prop-detail-level"));
        let mut detail = t.detail;
        egui::ComboBox::from_id_salt(("tpl-detail", id)).selected_text(tr(detail.key())).show_ui(ui, |ui| {
            for level in cc_core::DetailLevel::ALL {
                ui.selectable_value(&mut detail, level, tr(level.key()));
            }
        });
        if detail != t.detail {
            set.detail = Some(detail.name().to_string());
        }
        ui.end_row();

        changed |= ui.checkbox(&mut include.visual_style, "").changed();
        ui.label(t!("prop-visual-style"));
        let mut style = t.visual_style;
        let name = |s: Option<cc_core::VisualStyle>| s.map_or_else(|| t!("style-default"), |s| tr(s.key()));
        egui::ComboBox::from_id_salt(("tpl-style", id)).selected_text(name(style)).show_ui(ui, |ui| {
            ui.selectable_value(&mut style, None, name(None));
            for s in cc_core::VisualStyle::ALL {
                ui.selectable_value(&mut style, Some(s), name(Some(s)));
            }
        });
        if style != t.visual_style {
            set.visual_style = Some(style.map_or("default", |s| s.name()).to_string());
        }
        ui.end_row();

        changed |= ui.checkbox(&mut include.categories, "").changed();
        ui.label(t!("vg-categories-graphics"));
        if ui.button(t!("vg-edit")).clicked() {
            open_vg = true;
        }
        ui.end_row();

        changed |= ui.checkbox(&mut include.filters, "").changed();
        ui.label(t!("vg-filters"));
        ui.label(RichText::new(t!("vg-filter-count", "count" => t.graphics.filters.len())).color(PALETTE.text_muted));
        ui.end_row();

        changed |= ui.checkbox(&mut include.cut_height, "").changed();
        ui.label(t!("prop-cut-height"));
        let mut h = t.cut_height;
        let r = ui.add(crate::properties::length_drag(&mut h).speed(0.01).range(0.0..=20.0));
        if r.changed() {
            set.cut_height = Some(h);
        }
        ui.end_row();

        changed |= ui.checkbox(&mut include.discipline, "").changed();
        ui.label(t!("vg-discipline"));
        let label = |d: Option<cc_core::MepDiscipline>| d.map_or(t!("vg-discipline-none"), |d| tr(d.key()));
        let mut d = t.discipline;
        egui::ComboBox::from_id_salt(("tpl-discipline", id)).selected_text(label(d)).show_ui(ui, |ui| {
            for o in [None, Some(cc_core::MepDiscipline::Plumbing), Some(cc_core::MepDiscipline::Electrical)] {
                ui.selectable_value(&mut d, o, label(o));
            }
        });
        if d != t.discipline {
            set.discipline = Some(match d {
                None => "none".into(),
                Some(cc_core::MepDiscipline::Plumbing) => "plumbing".into(),
                Some(cc_core::MepDiscipline::Electrical) => "electrical".into(),
            });
        }
        ui.end_row();

        changed |= ui.checkbox(&mut include.structural, "").changed();
        ui.label(t!("section-structural"));
        ui.horizontal(|ui| {
            let (mut s, mut tags) = (t.structural, t.structure_tags);
            if ui.checkbox(&mut s, t!("prop-structural-plan")).changed() {
                set.structural = Some(s);
            }
            if ui.checkbox(&mut tags, t!("prop-structure-tags")).changed() {
                set.structure_tags = Some(tags);
            }
        });
        ui.end_row();

        changed |= ui.checkbox(&mut include.phase_filter, "").changed();
        ui.label(t!("prop-phase-filter"));
        let filters: Vec<(cc_core::PhaseFilterId, String)> =
            state.project().phase_filters.values().map(|f| (f.id, f.name.clone())).collect();
        let name = |id: Option<cc_core::PhaseFilterId>| {
            id.and_then(|i| filters.iter().find(|(f, _)| *f == i)).map_or(t!("ph-filter-none"), |(_, n)| n.clone())
        };
        let mut pf = t.phase_filter;
        egui::ComboBox::from_id_salt(("tpl-phase-filter", id)).selected_text(name(pf)).show_ui(ui, |ui| {
            ui.selectable_value(&mut pf, None, t!("ph-filter-none"));
            for (f, n) in &filters {
                ui.selectable_value(&mut pf, Some(*f), n);
            }
        });
        if pf != t.phase_filter {
            set.phase_filter = Some(pf.map(|f| Ref::Id(f.0)));
        }
        ui.end_row();
    });
    if changed {
        set.include = serde_json::to_value(include).ok();
    }
    let any = set.scale.is_some()
        || set.detail.is_some()
        || set.visual_style.is_some()
        || set.cut_height.is_some()
        || set.discipline.is_some()
        || set.structural.is_some()
        || set.structure_tags.is_some()
        || set.phase_filter.is_some()
        || set.include.is_some();
    if any {
        run(state, Some(key(("template", id))), set);
    }

    ui.add_space(6.0);
    let views: Vec<String> =
        state.project().views.values().filter(|v| v.template == Some(id)).map(|v| v.name.clone()).collect();
    readonly_list(ui, &t!("vg-template-views"), &views);
    ui.add_space(6.0);
    if let Some(view) = active {
        let view_name = state.project().view(view).map(|v| v.name.clone()).unwrap_or_default();
        ui.label(RichText::new(t!("vg-active-view", "view" => view_name)).color(PALETTE.text_muted));
        ui.horizontal(|ui| {
            if ui.button(t!("vg-apply-template")).on_hover_text(t!("vg-apply-template-tip")).clicked() {
                let command = cmd::ViewTemplateApply { views: vec![Ref::Id(view.0)], template: Ref::Id(id.0) };
                run(state, None, command);
            }
            if ui.button(t!("vg-assign-template")).on_hover_text(t!("vg-assign-template-tip")).clicked() {
                let command = cmd::ViewTemplateAssign { views: vec![Ref::Id(view.0)], template: Some(Ref::Id(id.0)) };
                run(state, None, command);
            }
        });
    }
    open_vg
}

fn readonly_list(ui: &mut Ui, label: &str, items: &[String]) {
    let text = if items.is_empty() { "—".to_string() } else { items.join(", ") };
    ui.label(RichText::new(format!("{label}: {text}")).color(PALETTE.text_muted));
}

// ---------------------------------------------------------------- properties panel

/// The view's template and Visibility/Graphics in its properties.
pub fn view_rows(ui: &mut Ui, state: &mut AppState, view: ViewId) {
    let Some(v) = state.project().view(view).cloned() else { return };
    if !cc_core::ViewTemplate::applies_to(&v.kind) {
        return;
    }
    let templates: Vec<(TemplateId, String)> =
        state.project().templates.values().map(|t| (t.id, t.name.clone())).collect();
    let current = v.template;
    let name = |id: Option<TemplateId>| {
        id.and_then(|i| templates.iter().find(|(t, _)| *t == i)).map_or(t!("vg-template-none"), |(_, n)| n.clone())
    };
    let mut chosen = current;
    row(ui, &t!("prop-view-template"), |ui| {
        egui::ComboBox::from_id_salt(("view-template", view.0)).selected_text(name(current)).show_ui(ui, |ui| {
            ui.selectable_value(&mut chosen, None, t!("vg-template-none"));
            for (id, n) in &templates {
                ui.selectable_value(&mut chosen, Some(*id), n);
            }
        });
    });
    if chosen != current {
        let command = cmd::ViewTemplateAssign { views: vec![Ref::Id(view.0)], template: chosen.map(|t| Ref::Id(t.0)) };
        run(state, None, command);
    }
    if row(ui, &t!("dlg-visibility-graphics"), |ui| ui.button(t!("vg-edit")).clicked()) {
        open(state, Target::View(view));
    }
    let hidden = v.graphics.hidden.len();
    if hidden > 0 {
        readonly_row(ui, &t!("vg-hidden-in-view"), hidden.to_string());
    }
}

/// Whether the view's template controls `what` (`scale`, `detail`,
/// `visual_style`, `cut_height`, `discipline`, `structural`), for read-only rows.
pub fn controlled(state: &AppState, view: ViewId, what: &str) -> bool {
    let Some(t) = state.project().view(view).and_then(|v| v.template).and_then(|t| state.project().template(t)) else {
        return false;
    };
    match what {
        "scale" => t.include.scale,
        "detail" => t.include.detail,
        "visual_style" => t.include.visual_style,
        "cut_height" => t.include.cut_height,
        "discipline" => t.include.discipline,
        "structural" => t.include.structural,
        _ => false,
    }
}
