//! The title block editor: the component environment for a title block
//! (`ComponentKind::TitleBlock`). One *Sheet* view takes the place of the
//! plan, front, side and 3D views: the paper at a preview size (the sheet
//! the editor opened from, or A0–A4 portrait or landscape), its four edges
//! as locked reference planes, the component's own reference planes, and
//! the title block's pieces with their fields filled in (from that sheet,
//! else sample values) or shown as typed.
//!
//! The canvas works in paper meters (millimeters / [`MM`]), so the sketch's
//! snapping to the millimeter, its pick distances and typed lengths
//! measure the printed paper; pieces are written in millimeters, the unit
//! of title block expressions. A coordinate on an edge is written `0`,
//! `SheetWidth` or `SheetHeight`, one on a reference plane as its name, so
//! the piece follows the paper. Every change is a `component_*` command.

use crate::paint::{PaintOptions, Xf, paint_drawing};
use crate::state::AppState;
use crate::t;
use crate::theme::{self, PALETTE};
use cc_core::profile::ProfilePoint;
use cc_core::title_block::{SHEET_HEIGHT, SHEET_WIDTH};
use cc_core::{ComponentDef, ElementTypeId, PlaneAxis, SheetSize, TypeData, ViewId, ViewKind};
use cc_views::ComponentView;
use cc_views::component_view::ViewPlane;
use egui::{Color32, Rect, Sense, Stroke, Ui, vec2};
use glam::DVec2;

/// How close (screen pixels) a click must be to pick a plane.
const PICK_PX: f64 = 6.0;

/// Millimeters per canvas unit (the canvas is in paper meters).
pub const MM: f64 = 1000.0;

/// The paper the Sheet view shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Preview {
    /// A sheet's own paper.
    Sheet(ViewId),
    /// An ISO size, portrait or landscape.
    Paper { size: SheetSize, portrait: bool },
}

/// The Sheet view's state.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetEditor {
    /// The sheet the editor opened from: fields read it (else sample values).
    pub sheet: Option<ViewId>,
    pub preview: Preview,
    /// Show `{keys}` as typed instead of their values.
    pub raw_keys: bool,
    /// The stroke whose sketch is open, with its points as they were.
    pub editing: Option<(usize, Vec<ProfilePoint>)>,
    /// Center (paper meters) and screen points per meter.
    camera: Option<(DVec2, f64)>,
    /// Where a selection window or a picture's rectangle started (millimeters).
    drag_from: Option<DVec2>,
    /// The grip being dragged: piece index, and 0 (position) or 1 (size).
    grip: Option<(usize, u8)>,
}

impl SheetEditor {
    pub fn new(sheet: Option<ViewId>) -> Self {
        let preview = sheet.map_or(Preview::Paper { size: SheetSize::A3, portrait: false }, Preview::Sheet);
        Self { sheet, preview, raw_keys: false, editing: None, camera: None, drag_from: None, grip: None }
    }
}

/// Whether the component environment shows a title block.
pub fn is_open(state: &AppState) -> bool {
    state.component_env.as_ref().is_some_and(|e| e.sheet.is_some())
}

/// The title block type of sheet `id`.
pub fn sheet_title_block_of(state: &AppState, id: ViewId) -> Option<ElementTypeId> {
    match &state.project().view(id)?.kind {
        ViewKind::Sheet { title_block, .. } => *title_block,
        _ => None,
    }
}

/// The active sheet's title block type.
pub fn sheet_title_block(state: &AppState) -> Option<ElementTypeId> {
    sheet_title_block_of(state, crate::actions::active_sheet(state)?)
}

/// *Edit Title Block* on sheet `id`: make it the active view, then open its
/// title block on it. `open_view` only queues the tab, so the active view is
/// set here for the editor to read the sheet.
pub fn edit_sheet(state: &mut AppState, id: ViewId) {
    let Some(tb) = sheet_title_block_of(state, id) else { return };
    state.open_view(id);
    state.active_view = Some(id);
    crate::component_env::open(state, tb);
}

/// *New Title Block*: a copy of the standard layout under a name no type and
/// no component uses (`title_block_create` refuses both), opened.
pub fn new_title_block(state: &mut AppState) {
    let base = t!("tb-default-name");
    let p = state.project();
    let taken = |n: &str| {
        p.types.values().any(|t| {
            t.name.trim() == n || matches!(&t.data, TypeData::Component { component, .. } if component.trim() == n)
        })
    };
    let Some(name) = (1..=10_000).map(|i| format!("{base} {i}")).find(|n| !taken(n)) else { return };
    let command = cc_engine::commands::sheets::TitleBlockCreate { name, empty: false };
    let Some(id) = crate::actions::run_command(state, command).and_then(|o| o["id"].as_u64()) else { return };
    crate::component_env::open(state, ElementTypeId(id));
}

fn editor(state: &AppState) -> Option<SheetEditor> {
    state.component_env.as_ref().and_then(|e| e.sheet.clone())
}

fn editor_mut(state: &mut AppState) -> Option<&mut SheetEditor> {
    state.component_env.as_mut().and_then(|e| e.sheet.as_mut())
}

/// An ISO paper in millimeters, portrait (long side up) or landscape.
pub fn iso_paper(size: SheetSize, portrait: bool) -> DVec2 {
    let s = size.size_mm();
    let (long, short) = (s.x.max(s.y), s.x.min(s.y));
    if portrait { DVec2::new(short, long) } else { DVec2::new(long, short) }
}

/// The paper shown, millimeters.
pub fn paper(state: &AppState, preview: Preview) -> DVec2 {
    match preview {
        Preview::Sheet(id) => {
            state.project().view(id).and_then(|v| v.paper_mm()).unwrap_or_else(|| iso_paper(SheetSize::A3, false))
        }
        Preview::Paper { size, portrait } => iso_paper(size, portrait),
    }
}

/// The `paper` field of the preview: the size's label (`A1`, …).
fn paper_label(state: &AppState, preview: Preview) -> String {
    match preview {
        Preview::Sheet(id) => match state.project().view(id).map(|v| &v.kind) {
            Some(ViewKind::Sheet { size, .. }) => size.label(),
            _ => SheetSize::A3.label(),
        },
        Preview::Paper { size, .. } => size.label(),
    }
}

/// The names expressions read for the paper, millimeters.
pub fn scope_extra(paper: DVec2) -> [(&'static str, f64); 2] {
    [(SHEET_WIDTH, paper.x), (SHEET_HEIGHT, paper.y)]
}

/// A line pieces snap to and are written against.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetPlane {
    /// What a coordinate on it is written as (`0`, `SheetWidth`, a plane's name).
    pub name: String,
    /// What the canvas labels it.
    pub label: String,
    /// A vertical line (it fixes x); else horizontal (it fixes y).
    pub vertical: bool,
    /// Where it stands, millimeters.
    pub at: f64,
    /// A paper edge (locked).
    pub edge: bool,
}

/// The four paper edges, then the component's planes square to x or y
/// that resolve on this paper.
pub fn planes(def: &ComponentDef, paper: DVec2) -> Vec<SheetPlane> {
    let edge = |name: &str, label: String, vertical: bool, at: f64| SheetPlane {
        name: name.to_string(),
        label,
        vertical,
        at,
        edge: true,
    };
    let mut out = vec![
        edge("0", t!("tb-edge-left"), true, 0.0),
        edge(SHEET_WIDTH, t!("tb-edge-right"), true, paper.x),
        edge("0", t!("tb-edge-bottom"), false, 0.0),
        edge(SHEET_HEIGHT, t!("tb-edge-top"), false, paper.y),
    ];
    let scope = def.scope(&Default::default(), &scope_extra(paper));
    for p in &def.ref_planes {
        let vertical = match p.axis {
            PlaneAxis::X => true,
            PlaneAxis::Y => false,
            PlaneAxis::Z => continue,
        };
        let Some(at) = scope.get(&p.name).filter(|v| v.is_finite()) else { continue };
        out.push(SheetPlane { name: p.name.clone(), label: p.name.clone(), vertical, at, edge: false });
    }
    out
}

/// The planes as the lines of a [`ComponentView`], `k` canvas units per
/// millimeter: the sketch snaps to them ([`crate::component_sketch::snap`])
/// and writes coordinates on them as their names.
// The sheet sketch (the drawing tools of the next tasks) snaps to it.
#[cfg_attr(not(test), allow(dead_code))]
pub fn plane_view(planes: &[SheetPlane], k: f64) -> ComponentView {
    // Far past any paper, millimeters.
    const FAR: f64 = 1.0e5;
    let planes = planes
        .iter()
        .map(|p| {
            let (a, b) = if p.vertical {
                (DVec2::new(p.at, -FAR), DVec2::new(p.at, FAR))
            } else {
                (DVec2::new(-FAR, p.at), DVec2::new(FAR, p.at))
            };
            ViewPlane { name: p.name.clone(), a: a * k, b: b * k, origin: p.edge }
        })
        .collect();
    ComponentView { planes, ..Default::default() }
}

/// The value of field `key` in the preview: from the sheet the editor
/// opened from, else a sample; `paper` is always the preview's.
pub fn field_value(state: &AppState, ed: &SheetEditor, def: &ComponentDef, key: &str) -> Option<String> {
    if key.trim() == "paper" {
        return Some(paper_label(state, ed.preview));
    }
    let p = state.project();
    if let Some(view) = ed.sheet.and_then(|id| p.view(id)) {
        let as_indicated = t!("sheet-as-indicated");
        let ctx = cc_views::sheet_fields::FieldCtx {
            project: p,
            sheet: view,
            def: Some(def),
            today: crate::i18n::today(),
            format_date: crate::i18n::format_date,
            as_indicated: &as_indicated,
            sheets: Default::default(),
        };
        return cc_views::sheet_fields::value(key, &ctx);
    }
    let (y, m, d) = crate::i18n::today();
    let info = &p.info;
    Some(match key.trim() {
        "sheet_number" => "A101".to_string(),
        "sheet_name" => t!("tb-sample-sheet"),
        "sheet_count" | "sheet_index" => "1".to_string(),
        "sheet_scale" => "1:50".to_string(),
        "project_name" => info.name.clone(),
        "project_number" => info.number.clone(),
        "client" => info.client.clone(),
        "address" => info.address.clone(),
        "author" => info.author.clone(),
        "date" | "issue_date" => crate::i18n::format_date(y, m, d),
        k => match k.strip_prefix("project:") {
            Some(name) => info.field(name.trim())?.to_string(),
            None => def.param(k.strip_prefix("param:")?.trim()).filter(|q| q.kind.is_text())?.text.clone(),
        },
    })
}

/// The camera that shows the whole paper.
fn fit(paper: DVec2, rect: Rect) -> (DVec2, f64) {
    let size = paper / MM * 1.1;
    let scale = (f64::from(rect.width()) / size.x).min(f64::from(rect.height()) / size.y).max(50.0);
    (paper / MM / 2.0, scale)
}

/// The Sheet view: the paper, the planes, the title block, and the tools.
pub fn central(ui: &mut Ui, state: &mut AppState) {
    let Some((_, def)) = crate::component_env::component(state) else { return };
    let Some(ed) = editor(state) else { return };
    let paper_mm = paper(state, ed.preview);
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, PALETTE.sheet_bg);

    // Pan (middle or right drag), zoom (wheel), fit (middle double click).
    let (mut center, mut scale) = ed.camera.unwrap_or_else(|| fit(paper_mm, rect));
    if resp.dragged_by(egui::PointerButton::Middle) || resp.dragged_by(egui::PointerButton::Secondary) {
        let d = resp.drag_delta();
        center -= DVec2::new(f64::from(d.x), -f64::from(d.y)) / scale;
    }
    if let Some(hover) = resp.hover_pos() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let xf = Xf { screen: rect.center(), origin: center, scale };
            let before = xf.to_world(hover);
            scale = (scale * (1.0 + f64::from(scroll) * 0.002)).clamp(50.0, 1.0e6);
            center += before - Xf { scale, ..xf }.to_world(hover);
        }
    }
    if resp.double_clicked_by(egui::PointerButton::Middle) {
        (center, scale) = fit(paper_mm, rect);
    }
    // A canvas with no area yet (the first layout pass) fits again next frame.
    if rect.width() > 0.0
        && rect.height() > 0.0
        && let Some(e) = editor_mut(state)
    {
        e.camera = Some((center, scale));
    }
    let xf = Xf { screen: rect.center(), origin: center, scale };
    let mm = Xf { screen: rect.center(), origin: center * MM, scale: scale / MM };

    // The paper.
    let paper_rect = Rect::from_two_pos(mm.to_screen(DVec2::ZERO), mm.to_screen(paper_mm));
    painter.rect_filled(paper_rect.translate(vec2(4.0, 4.0)), 0.0, Color32::from_black_alpha(60));
    painter.rect_filled(paper_rect, 0.0, Color32::WHITE);

    // The pieces, fields filled in (or as typed).
    let pieces = def.resolve_sheet(paper_mm);
    let drawing = cc_views::title_block_drawing(&pieces, &|text| {
        if ed.raw_keys {
            text.to_string()
        } else {
            cc_views::sheet_fields::expand_with(text, &|k| field_value(state, &ed, &def, k))
        }
    });
    let none = Default::default();
    paint_drawing(
        &painter,
        &drawing,
        &mm,
        &PaintOptions { highlight: &none, px_per_mm: mm.scale, mono: None, only: None },
    );

    // The planes: edges grey and dashed, the component's in the accent tint.
    let planes = planes(&def, paper_mm);
    for p in &planes {
        paint_plane(&painter, &mm, rect, p, false);
    }
    if let Some((index, error)) = pieces.errors.first() {
        let piece = def.sheet.get(*index).map_or_else(|| (index + 1).to_string(), |s| s.name.clone());
        painter.text(
            rect.left_bottom() + vec2(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            t!("msg-title-block-error", "piece" => piece, "error" => error),
            egui::FontId::proportional(12.0),
            PALETTE.danger,
        );
    }
    let tol = PICK_PX / scale; // paper meters
    let raw = resp.hover_pos().map(|p| xf.to_world(p));
    let picked_plane = match state.component_env.as_ref().and_then(|e| e.pick.clone()) {
        Some(crate::component_env::Pick::Plane(n)) => Some(n),
        _ => None,
    };
    for p in planes.iter().filter(|p| !p.edge && Some(&p.name) == picked_plane.as_ref()) {
        paint_plane(&painter, &mm, rect, p, true);
    }
    let tool = state.component_env.as_ref().map(|e| e.tool.clone()).unwrap_or_default();
    let clicked = resp.clicked_by(egui::PointerButton::Primary);
    match tool {
        crate::component_env::EnvTool::RefPlane { first } => {
            let Some(p) = raw else { return };
            match first {
                Some((_, a)) => {
                    painter.line_segment([xf.to_screen(a), xf.to_screen(p)], Stroke::new(1.5, PALETTE.accent));
                    if clicked {
                        add_sheet_plane(state, a * MM, p * MM);
                        crate::component_env::set_tool(state, crate::component_env::EnvTool::RefPlane { first: None });
                    }
                }
                None if clicked => crate::component_env::set_tool(
                    state,
                    crate::component_env::EnvTool::RefPlane { first: Some((cc_views::ComponentViewKind::Plan, p)) },
                ),
                None => {}
            }
        }
        crate::component_env::EnvTool::Select => {
            if clicked && let Some(p) = raw {
                let hit =
                    plane_at(&planes, p * MM, tol * MM).map(|q| crate::component_env::Pick::Plane(q.name.clone()));
                if let Some(env) = state.component_env.as_mut() {
                    env.pick = hit;
                }
            }
        }
        _ => {}
    }
}

/// The component plane (never a paper edge: those are locked) within `reach`
/// millimeters of `at`.
fn plane_at(planes: &[SheetPlane], at: DVec2, reach: f64) -> Option<&SheetPlane> {
    planes.iter().filter(|q| !q.edge).find(|q| ((if q.vertical { at.x } else { at.y }) - q.at).abs() <= reach)
}

/// The plane two clicks (millimeters) make: `x` (a vertical line) or `y`,
/// and its position to a tenth of a millimeter.
pub fn sheet_plane_from_line(a: DVec2, b: DVec2) -> Option<(&'static str, f64)> {
    let d = b - a;
    if d.length() < 1e-6 {
        return None;
    }
    let vertical = d.y.abs() >= d.x.abs();
    let at = if vertical { (a.x + b.x) / 2.0 } else { (a.y + b.y) / 2.0 };
    Some((if vertical { "x" } else { "y" }, (at * 10.0).round() / 10.0))
}

fn add_sheet_plane(state: &mut AppState, a: DVec2, b: DVec2) {
    let Some((axis, at)) = sheet_plane_from_line(a, b) else { return };
    let command = cc_engine::commands::component::ComponentRefPlaneAdd {
        type_ref: crate::component_env::type_ref(state),
        axis: axis.into(),
        at: format!("{at}"),
        name: None,
    };
    if let Some(out) = crate::actions::run_command(state, command)
        && let (Some(env), Some(name)) = (state.component_env.as_mut(), out["name"].as_str())
    {
        env.pick = Some(crate::component_env::Pick::Plane(name.to_string()));
    }
}

/// A reference plane's name, position (an expression in millimeters) and value.
/// Paper edges are not component planes, so they show nothing here.
fn plane_props(ui: &mut Ui, state: &mut AppState, def: &ComponentDef, name: &str) {
    use cc_engine::commands::component as cmd;
    let Some(plane) = def.ref_plane(name) else { return };
    let Some(ed) = editor(state) else { return };
    let paper = paper(state, ed.preview);
    let value = def.scope(&Default::default(), &scope_extra(paper)).get(name);
    let t = crate::component_env::type_ref(state);
    let mut rename = None;
    crate::table::row(ui, &t!("prop-name"), |ui| {
        rename = crate::component_env::draft_field(ui, state, &format!("tb-plane-name:{name}"), name, 120.0)
    });
    let mut at = None;
    crate::table::row(ui, &t!("comp-env-at"), |ui| {
        at = crate::component_env::draft_field(ui, state, &format!("tb-plane-at:{name}"), &plane.at, 160.0)
    });
    crate::table::readonly_row(ui, &t!("prop-value"), value.map_or_else(String::new, |v| format!("{v} mm")));
    if at.is_some() || rename.is_some() {
        let command =
            cmd::ComponentRefPlaneSet { type_ref: t.clone(), name: name.to_string(), at, rename: rename.clone() };
        if crate::actions::run_command(state, command).is_some()
            && let (Some(new), Some(env)) = (rename, state.component_env.as_mut())
        {
            env.pick = Some(crate::component_env::Pick::Plane(new));
        }
    }
    if ui.button(t!("action-delete")).clicked() {
        crate::component_env::delete_pick(state);
    }
}

/// A plane's line across the view and its label; `hot` draws it picked.
fn paint_plane(painter: &egui::Painter, mm: &Xf, rect: Rect, p: &SheetPlane, hot: bool) {
    let at = mm.to_screen(if p.vertical { DVec2::new(p.at, 0.0) } else { DVec2::new(0.0, p.at) });
    let (a, b) = if p.vertical {
        (egui::pos2(at.x, rect.top()), egui::pos2(at.x, rect.bottom()))
    } else {
        (egui::pos2(rect.left(), at.y), egui::pos2(rect.right(), at.y))
    };
    let color = match (hot, p.edge) {
        (true, _) => PALETTE.accent,
        (false, true) => Color32::from_gray(150),
        (false, false) => PALETTE.accent.gamma_multiply(0.6),
    };
    let stroke = Stroke::new(if hot { 2.0 } else { 1.0 }, color);
    painter.extend(egui::Shape::dashed_line(&[a, b], stroke, 6.0, 4.0));
    let label_at =
        if p.vertical { egui::pos2(at.x + 4.0, rect.top() + 4.0) } else { egui::pos2(rect.left() + 4.0, at.y - 4.0) };
    let align = if p.vertical { egui::Align2::LEFT_TOP } else { egui::Align2::LEFT_BOTTOM };
    painter.text(label_at, align, &p.label, egui::FontId::proportional(11.0), color);
}

/// The *Create* tab of the title block editor.
pub fn ribbon(ui: &mut Ui, state: &mut AppState) {
    use crate::commands::UiCommand::*;
    let title = crate::component_env::component(state).map(|(name, _)| name).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(theme::title(t!("comp-env-title", "name" => title)));
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        crate::ribbon::env_group(ui, state, t!("group-selection"), &[Select]);
        crate::ribbon::env_group(ui, state, t!("comp-env-group-datum"), &[CompRefPlane]);
        crate::ribbon::env_group(ui, state, t!("comp-env-group-component"), &[CompSaveFile]);
        crate::ribbon::env_group(ui, state, t!("comp-env-group-editor"), &[CompFinish, CompCancel]);
    });
}

/// The options bar: the preview paper, raw keys, and the tool's hint.
pub fn options(ui: &mut Ui, state: &mut AppState) {
    let Some(ed) = editor(state) else { return };
    let label = |p: Preview| -> String {
        match p {
            Preview::Sheet(_) => t!("tb-preview-sheet", "paper" => paper_label(state, p)),
            Preview::Paper { size, portrait: false } => t!("tb-preview-landscape", "paper" => size.label()),
            Preview::Paper { size, portrait: true } => t!("tb-preview-portrait", "paper" => size.label()),
        }
    };
    let mut preview = ed.preview;
    let mut raw = ed.raw_keys;
    ui.horizontal(|ui| {
        ui.label(t!("tb-preview"));
        egui::ComboBox::from_id_salt("tb-preview").selected_text(label(preview)).width(170.0).show_ui(ui, |ui| {
            if let Some(id) = ed.sheet {
                ui.selectable_value(&mut preview, Preview::Sheet(id), label(Preview::Sheet(id)));
            }
            for size in SheetSize::ALL {
                for portrait in [false, true] {
                    let p = Preview::Paper { size, portrait };
                    ui.selectable_value(&mut preview, p, label(p));
                }
            }
        });
        ui.checkbox(&mut raw, t!("tb-show-keys"));
        ui.separator();
        ui.label(theme::muted(hint(state)));
    });
    if let Some(e) = editor_mut(state) {
        if e.preview != preview {
            e.preview = preview;
            e.camera = None;
        }
        e.raw_keys = raw;
    }
}

/// What the active tool waits for.
fn hint(state: &AppState) -> String {
    match state.component_env.as_ref().map(|e| &e.tool) {
        Some(crate::component_env::EnvTool::RefPlane { .. }) => t!("tb-hint-plane"),
        _ => t!("tb-hint-view"),
    }
}

/// The left panel: name, type, parameters (and the paper's size), and the
/// picked piece or plane.
pub fn panel_content(ui: &mut Ui, state: &mut AppState, name: &str, def: &ComponentDef) {
    ui.add_space(6.0);
    ui.label(theme::title(name.to_string()));
    crate::component_env::types_row(ui, state);
    ui.add_space(6.0);
    crate::table::section(ui, "tb-params", t!("comp-params"), |ui| params(ui, state, def));
    if let Some(crate::component_env::Pick::Plane(plane)) = state.component_env.as_ref().and_then(|e| e.pick.clone()) {
        crate::table::section(ui, "tb-plane", t!("comp-env-plane"), |ui| plane_props(ui, state, def, &plane));
    }
    crate::component_env::ref_problems(ui, def);
}

/// The component environment's parameters (numbers in millimeters, texts
/// typed in), then *New per-sheet field*, a text sheets set, and the
/// paper's width and height, read-only.
fn params(ui: &mut Ui, state: &mut AppState, def: &ComponentDef) {
    crate::component_env::params(ui, state, def, true);
    if ui.button(format!("+ {}", t!("tb-new-field"))).clicked() {
        let name = (1..=10_000).map(|i| format!("Field{i}")).find(|n| !def.name_taken(n)).unwrap_or_default();
        let command = cc_engine::commands::component::ComponentParamAdd {
            type_ref: crate::component_env::type_ref(state),
            name,
            value: None,
            kind: Some("text".into()),
            instance: Some(true),
            text: Some(String::new()),
        };
        crate::actions::run_command(state, command);
    }
    let Some(ed) = editor(state) else { return };
    let paper = paper(state, ed.preview);
    crate::table::readonly_row(ui, &t!("tb-sheet-width"), format!("{} mm", paper.x));
    crate::table::readonly_row(ui, &t!("tb-sheet-height"), format!("{} mm", paper.y));
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::Document;

    /// The sample house with its first sheet active and its title block open.
    pub(crate) fn editor_state() -> (AppState, ViewId, ElementTypeId) {
        let mut state = AppState::new(Document::new(crate::home::sample_project("casa_simples").unwrap()));
        let sheet = state.project().sheets_in_order()[0];
        state.active_view = Some(sheet);
        let Some(ViewKind::Sheet { title_block: Some(tb), .. }) = state.project().view(sheet).map(|v| v.kind.clone())
        else {
            panic!("the sample's sheet has a title block")
        };
        crate::component_env::open(&mut state, tb);
        (state, sheet, tb)
    }

    #[test]
    fn a_title_block_opens_on_its_sheet() {
        let (state, sheet, _) = editor_state();
        let ed = state.component_env.as_ref().and_then(|e| e.sheet.clone()).expect("the Sheet view");
        assert_eq!((ed.sheet, ed.preview), (Some(sheet), Preview::Sheet(sheet)));
        assert!(is_open(&state));
        let expected = state.project().view(sheet).and_then(|v| v.paper_mm()).unwrap();
        assert_eq!(paper(&state, ed.preview), expected);
        assert_eq!(iso_paper(SheetSize::A4, true), DVec2::new(210.0, 297.0));
        assert_eq!(iso_paper(SheetSize::A4, false), DVec2::new(297.0, 210.0));
    }

    #[test]
    fn other_components_keep_their_four_views() {
        let mut state = AppState::new(Document::new(crate::actions::new_project()));
        let table = state
            .project()
            .types_of(cc_core::Category::Component)
            .find(|t| matches!(&t.data, TypeData::Component { component, .. } if component == "Mesa"))
            .map(|t| t.id)
            .unwrap();
        crate::component_env::open(&mut state, table);
        assert!(state.component_env.as_ref().is_some_and(|e| e.sheet.is_none()));
        assert!(!is_open(&state));
    }

    #[test]
    fn the_edges_and_the_planes_are_where_the_paper_says() {
        let (mut state, _, tb) = editor_state();
        let command = cc_engine::commands::component::ComponentRefPlaneAdd {
            type_ref: cc_engine::Ref::Id(tb.0),
            axis: "x".into(),
            at: "SheetWidth - 10".into(),
            name: Some("Margin".into()),
        };
        crate::actions::run_command(&mut state, command).unwrap();
        let (_, def) = crate::component_env::component(&state).unwrap();
        let ps = planes(&def, DVec2::new(420.0, 297.0));
        let at =
            |name: &str, vertical: bool| ps.iter().find(|p| p.name == name && p.vertical == vertical).map(|p| p.at);
        assert_eq!(at("SheetWidth", true), Some(420.0));
        assert_eq!(at("SheetHeight", false), Some(297.0));
        assert_eq!(at("0", true), Some(0.0));
        assert_eq!(at("Margin", true), Some(410.0));
        assert!(ps.iter().filter(|p| p.edge).count() == 4);
        // In meters for the sketch: the margin's line stands at 0.41.
        let view = plane_view(&ps, 1.0 / MM);
        let (snapped, locked) = crate::component_sketch::snap(&view, DVec2::new(0.4102, 0.1), 0.001);
        assert!(locked && (snapped.x - 0.41).abs() < 1e-12);
    }

    #[test]
    fn two_clicks_make_a_plane_in_millimeters() {
        assert_eq!(sheet_plane_from_line(DVec2::new(30.04, 0.0), DVec2::new(30.08, 200.0)), Some(("x", 30.1)));
        assert_eq!(sheet_plane_from_line(DVec2::new(0.0, 52.0), DVec2::new(100.0, 52.0)), Some(("y", 52.0)));
        assert_eq!(sheet_plane_from_line(DVec2::ONE, DVec2::ONE), None);
        let (mut state, _, _) = editor_state();
        add_sheet_plane(&mut state, DVec2::new(30.0, 0.0), DVec2::new(30.0, 100.0));
        let pick = state.component_env.as_ref().and_then(|e| e.pick.clone());
        let Some(crate::component_env::Pick::Plane(name)) = pick else { panic!("{pick:?}") };
        let (_, def) = crate::component_env::component(&state).unwrap();
        let p = planes(&def, DVec2::new(420.0, 297.0));
        assert!(p.iter().any(|q| q.name == name && q.vertical && (q.at - 30.0).abs() < 1e-9));
    }

    #[test]
    fn the_paper_edges_cannot_be_picked_edited_or_deleted() {
        use cc_engine::commands::component as cmd;
        let (mut state, _, _) = editor_state();
        add_sheet_plane(&mut state, DVec2::new(30.0, 0.0), DVec2::new(30.0, 100.0));
        let (_, def) = crate::component_env::component(&state).unwrap();
        let ps = planes(&def, DVec2::new(420.0, 297.0));
        // Clicks on an edge pick nothing; the user's plane is picked.
        assert!(plane_at(&ps, DVec2::new(0.1, 50.0), 1.0).is_none());
        assert!(plane_at(&ps, DVec2::new(419.9, 50.0), 1.0).is_none());
        assert!(plane_at(&ps, DVec2::new(50.0, 296.9), 1.0).is_none());
        assert!(plane_at(&ps, DVec2::new(30.2, 50.0), 1.0).is_some_and(|p| !p.edge));
        // Neither the engine commands nor the delete button reach an edge.
        let t = crate::component_env::type_ref(&state);
        for name in ["0", "SheetWidth", "SheetHeight"] {
            let delete = cmd::ComponentRefPlaneDelete { type_ref: t.clone(), name: name.into() };
            assert!(crate::actions::run_command(&mut state, delete).is_none());
            let set = cmd::ComponentRefPlaneSet {
                type_ref: t.clone(),
                name: name.into(),
                at: Some("5".into()),
                rename: Some("Moved".into()),
            };
            assert!(crate::actions::run_command(&mut state, set).is_none());
            if let Some(env) = state.component_env.as_mut() {
                env.pick = Some(crate::component_env::Pick::Plane(name.into()));
            }
            crate::component_env::delete_pick(&mut state);
        }
        let (_, def) = crate::component_env::component(&state).unwrap();
        assert!(def.ref_plane("Moved").is_none());
        assert_eq!(planes(&def, DVec2::new(420.0, 297.0)).iter().filter(|p| p.edge).count(), 4);
    }

    #[test]
    fn fields_read_the_sheet_or_samples() {
        let (mut state, sheet, _) = editor_state();
        let (_, def) = crate::component_env::component(&state).unwrap();
        let ed = state.component_env.as_ref().and_then(|e| e.sheet.clone()).unwrap();
        let number = match &state.project().view(sheet).unwrap().kind {
            ViewKind::Sheet { number, .. } => number.clone(),
            _ => panic!("a sheet"),
        };
        assert_eq!(field_value(&state, &ed, &def, "sheet_number"), Some(number));
        let sample = SheetEditor::new(None);
        assert_eq!(field_value(&state, &sample, &def, "sheet_number").as_deref(), Some("A101"));
        assert_eq!(field_value(&state, &sample, &def, "paper").as_deref(), Some("A3"));
        assert_eq!(field_value(&state, &sample, &def, "nothing"), None);
        state.component_env.as_mut().unwrap().sheet = Some(sample);
        // Finish leaves the editor.
        crate::component_env::finish(&mut state);
        assert!(!is_open(&state));
    }

    #[test]
    fn edit_and_new_title_block_open_the_editor() {
        use crate::commands::UiCommand;
        let mut state = AppState::new(Document::new(crate::home::sample_project("casa_simples").unwrap()));
        state.project_open = true;
        state.show_home = false;
        assert_eq!(UiCommand::EditTitleBlock.unavailable(&state), Some(t!("why-sheet-title-block")));
        let sheet = state.project().sheets_in_order()[0];
        state.active_view = Some(sheet);
        assert_eq!(UiCommand::EditTitleBlock.unavailable(&state), None);
        crate::commands::run(&mut state, &egui::Context::default(), UiCommand::EditTitleBlock);
        assert!(is_open(&state));
        crate::component_env::cancel(&mut state);

        let before = state.project().title_blocks().len();
        crate::commands::run(&mut state, &egui::Context::default(), UiCommand::NewTitleBlock);
        assert_eq!(state.project().title_blocks().len(), before + 1);
        assert!(is_open(&state));
        let (name, def) = crate::component_env::component(&state).unwrap();
        assert!(name.starts_with(&t!("tb-default-name")));
        assert!(!def.sheet.is_empty(), "a copy of the standard layout");
    }

    #[test]
    fn a_title_block_from_the_library_opens_in_the_editor() {
        let mut state = AppState::new(Document::new(crate::home::sample_project("casa_simples").unwrap()));
        let command = cc_engine::commands::sheets::TitleBlockCreate { name: "Library TB".into(), empty: false };
        crate::actions::run_command(&mut state, command).unwrap();
        let path = std::env::temp_dir().join(format!("cc_tb_library_{}.ccomp", std::process::id()));
        let save = cc_engine::commands::io::ComponentSave { component: "Library TB".into(), path: path.clone() };
        state.session.run(save).unwrap();

        let mut other = AppState::new(Document::new(crate::actions::new_project()));
        crate::actions::load_component_path(&mut other, path.clone());
        let _ = std::fs::remove_file(&path);
        assert!(is_open(&other), "opened in the editor, not placed");
        assert!(matches!(other.tool, crate::tools::Tool::Select));
        assert_eq!(crate::component_env::component(&other).map(|(n, _)| n).as_deref(), Some("Library TB"));
    }

    #[test]
    fn quick_edit_does_not_offer_title_blocks() {
        assert!(!crate::component_editor::kinds().contains(&cc_core::ComponentKind::TitleBlock));
    }

    #[test]
    fn edit_title_block_from_the_browser_opens_on_that_sheet() {
        // The browser's sheet menu runs `edit_sheet` on a sheet that is not the active view.
        let mut state = AppState::new(Document::new(crate::home::sample_project("casa_simples").unwrap()));
        let sheet = state.project().sheets_in_order()[0];
        let other = state.project().views.keys().copied().find(|&v| v != sheet).unwrap();
        state.active_view = Some(other);
        edit_sheet(&mut state, sheet);
        let ed = state.component_env.as_ref().and_then(|e| e.sheet.clone()).expect("the Sheet view");
        assert_eq!((ed.sheet, ed.preview), (Some(sheet), Preview::Sheet(sheet)));
        assert_eq!(state.active_view, Some(sheet));
        crate::component_env::cancel(&mut state);
        // And with no active view.
        state.active_view = None;
        edit_sheet(&mut state, sheet);
        assert_eq!(state.component_env.as_ref().and_then(|e| e.sheet.as_ref()).and_then(|e| e.sheet), Some(sheet));
    }

    #[test]
    fn new_title_block_skips_names_taken_by_types_or_components() {
        let mut state = AppState::new(Document::new(crate::home::sample_project("casa_simples").unwrap()));
        let base = t!("tb-default-name");
        // A component named like the first candidate, under a type named otherwise.
        let command = cc_engine::commands::sheets::TitleBlockCreate { name: format!("{base} 1"), empty: true };
        let id = crate::actions::run_command(&mut state, command).and_then(|o| o["id"].as_u64()).unwrap();
        assert!(crate::engine_edit::element_type(&mut state, ElementTypeId(id), None, |t| t.name = "Other".into()));
        new_title_block(&mut state);
        let (name, _) = crate::component_env::component(&state).expect("a new title block opens");
        assert_eq!(name, format!("{base} 2"));
    }
}
