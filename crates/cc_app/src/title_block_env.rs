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
use crate::sketch_mode::{Draw, Session, Target};
use crate::state::AppState;
use crate::t;
use crate::theme::{self, PALETTE};
use cc_core::profile::ProfilePoint;
use cc_core::sketch::SketchCurve;
use cc_core::title_block::{SHEET_HEIGHT, SHEET_WIDTH, SheetPieces, SheetShape, SheetText, TextAnchor};
use cc_core::{ComponentDef, ElementTypeId, PlaneAxis, SheetSize, TypeData, ViewId, ViewKind};
use cc_engine::commands::component_piece::{ExprArg, SketchPointArg};
use cc_engine::commands::sheets::{
    ComponentSheetPieceAdd, ComponentSheetPieceDelete, ComponentSheetPieceSet, SheetPieceArg,
};
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
    paint_picked(&painter, &mm, &pieces, &picked(state));
    // An open sketch takes the clicks: its own points first, then the edges
    // and planes, then the millimeter.
    if let Some(mut s) = state.sketch.take() {
        let view = plane_view(&planes, 1.0 / MM);
        let snapped = raw.map(|r| {
            let own = s
                .snap_points()
                .into_iter()
                .filter(|q| q.distance(r) <= tol)
                .min_by(|a, b| a.distance(r).total_cmp(&b.distance(r)));
            match own {
                Some(q) => crate::tools::Snap::plain(q, true),
                None => {
                    let (at, locked) = crate::component_sketch::snap(&view, r, tol);
                    crate::tools::Snap::plain(at, locked)
                }
            }
        });
        let message = crate::sketch_mode::interact(ui, &resp, &painter, &mut s, &state.modify, raw, snapped, &xf);
        state.sketch = Some(s);
        if let Some(m) = message {
            state.message = m;
        }
        return;
    }
    let tool = state.component_env.as_ref().map(|e| e.tool.clone()).unwrap_or_default();
    let clicked = resp.clicked_by(egui::PointerButton::Primary);
    match tool {
        crate::component_env::EnvTool::RefPlane { first } => match (first, raw) {
            (Some((_, a)), Some(p)) => {
                painter.line_segment([xf.to_screen(a), xf.to_screen(p)], Stroke::new(1.5, PALETTE.accent));
                if clicked {
                    add_sheet_plane(state, a * MM, p * MM);
                    crate::component_env::set_tool(state, crate::component_env::EnvTool::RefPlane { first: None });
                }
            }
            (None, Some(p)) if clicked => crate::component_env::set_tool(
                state,
                crate::component_env::EnvTool::RefPlane { first: Some((cc_views::ComponentViewKind::Plan, p)) },
            ),
            _ => {}
        },
        crate::component_env::EnvTool::Select => {
            let hover = raw.map(|p| p * MM);
            let reach = tol * MM;
            let lone = match picked(state).as_slice() {
                [i] => Some(*i),
                _ => None,
            };
            let lone_grips = lone.map(|i| (i, grips(&pieces, i))).unwrap_or_default();
            for (_, at) in &lone_grips.1 {
                painter.rect_filled(Rect::from_center_size(mm.to_screen(*at), vec2(8.0, 8.0)), 0.0, PALETTE.accent);
            }
            // A press on a grip drags it, elsewhere starts a window.
            if resp.drag_started_by(egui::PointerButton::Primary)
                && let Some(origin) = ui.input(|i| i.pointer.press_origin())
            {
                let from = mm.to_world(origin);
                let grip = lone_grips
                    .1
                    .iter()
                    .filter(|(_, at)| at.distance(from) <= reach * 1.5)
                    .min_by(|a, b| a.1.distance(from).total_cmp(&b.1.distance(from)))
                    .map(|(which, _)| (lone_grips.0, *which));
                if let Some(e) = editor_mut(state) {
                    match grip {
                        Some(g) => e.grip = Some(g),
                        None => e.drag_from = Some(from),
                    }
                }
            }
            if let (Some((index, which)), Some(r)) = (editor(state).and_then(|e| e.grip), raw)
                && resp.dragged_by(egui::PointerButton::Primary)
            {
                let (at, _) = crate::component_sketch::snap(&plane_view(&planes, 1.0 / MM), r, tol);
                let shift = ui.input(|i| i.modifiers.shift);
                if resp.drag_delta() != egui::Vec2::ZERO {
                    move_grip(state, &pieces, &planes, index, which, at * MM, shift);
                }
            }
            if resp.drag_stopped_by(egui::PointerButton::Primary)
                && editor_mut(state).and_then(|e| e.grip.take()).is_some()
            {
                state.session.doc.break_merge();
            }
            if let (Some(a), Some(b)) = (editor(state).and_then(|e| e.drag_from), hover)
                && resp.dragged_by(egui::PointerButton::Primary)
            {
                let r = Rect::from_two_pos(mm.to_screen(a), mm.to_screen(b));
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, PALETTE.accent), egui::StrokeKind::Middle);
            }
            if resp.drag_stopped_by(egui::PointerButton::Primary) {
                let from = editor_mut(state).and_then(|e| e.drag_from.take());
                if let (Some(a), Some(b)) = (from, hover) {
                    set_pick(state, pieces_in(&pieces, a, b));
                }
            }
            if resp.double_clicked_by(egui::PointerButton::Primary)
                && let Some(i) = hover.and_then(|p| piece_at(&pieces, p, reach))
                && matches!(def.sheet.get(i).map(|s| &s.shape), Some(SheetShape::Stroke { .. }))
            {
                edit_stroke(state, i);
                return;
            }
            if clicked && let Some(p) = hover {
                let hit = piece_at(&pieces, p, reach)
                    .map(|i| crate::component_env::Pick::Sheet(vec![i]))
                    .or_else(|| plane_at(&planes, p, reach).map(|q| crate::component_env::Pick::Plane(q.name.clone())));
                if let Some(env) = state.component_env.as_mut() {
                    env.pick = hit;
                }
            }
        }
        // A click places a text, snapped to the edges, the planes and the millimeter.
        crate::component_env::EnvTool::SheetText => {
            if let Some(r) = raw {
                let (at, _) = crate::component_sketch::snap(&plane_view(&planes, 1.0 / MM), r, tol);
                painter.circle_stroke(xf.to_screen(at), 4.0, Stroke::new(1.5, PALETTE.accent));
                if clicked {
                    place_text(state, &planes, at * MM);
                }
            }
        }
        // A drag fills the picture's rectangle, its aspect kept unless Shift is held.
        crate::component_env::EnvTool::SheetImage { picture, start } => {
            let view = plane_view(&planes, 1.0 / MM);
            let snapped = raw.map(|r| crate::component_sketch::snap(&view, r, tol).0 * MM);
            let free = ui.input(|i| i.modifiers.shift);
            if resp.drag_started_by(egui::PointerButton::Primary)
                && let Some(origin) = ui.input(|i| i.pointer.press_origin())
            {
                let p = crate::component_sketch::snap(&view, xf.to_world(origin), tol).0 * MM;
                crate::component_env::set_tool(
                    state,
                    crate::component_env::EnvTool::SheetImage { picture: picture.clone(), start: Some(p) },
                );
                return;
            }
            if let Some(p) = snapped {
                painter.circle_stroke(mm.to_screen(p), 4.0, Stroke::new(1.5, PALETTE.accent));
            }
            let Some(a) = start else { return };
            let rect = snapped.map(|b| image_rect(a, b, picture.aspect, free));
            if let Some((corner, size)) = rect {
                let r = Rect::from_two_pos(mm.to_screen(corner), mm.to_screen(corner + size));
                painter.rect_stroke(r, 0.0, Stroke::new(1.5, PALETTE.accent), egui::StrokeKind::Middle);
            }
            if resp.drag_stopped_by(egui::PointerButton::Primary) {
                // A rectangle too small to keep leaves the tool on, ready for another drag.
                match rect {
                    Some((corner, size)) if add_image(state, &planes, corner, size, &picture) => {
                        crate::component_env::set_tool(state, crate::component_env::EnvTool::Select);
                    }
                    _ => crate::component_env::set_tool(
                        state,
                        crate::component_env::EnvTool::SheetImage { picture: picture.clone(), start: None },
                    ),
                }
            }
        }
        _ => {}
    }
}

/// A text's extents (millimeters), estimated: 0.6 × its height per
/// character and 1.5 × its height per line, from its anchor, turned.
pub fn text_box(t: &SheetText) -> (DVec2, DVec2) {
    let lines: Vec<&str> = t.text.lines().collect();
    let n = lines.len().max(1) as f64;
    let chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0).max(1) as f64;
    let (w, h) = (chars * 0.6 * t.size, t.size * (1.0 + 1.5 * (n - 1.0)));
    let below = h - t.size;
    let (x0, y0) = match t.anchor {
        TextAnchor::Left => (0.0, -below),
        TextAnchor::Center => (-w / 2.0, -below),
        TextAnchor::Right => (-w, -below),
        TextAnchor::Middle => (-w / 2.0, -h / 2.0),
    };
    let turn = DVec2::from_angle(t.rotation_deg.to_radians());
    [DVec2::new(x0, y0), DVec2::new(x0 + w, y0), DVec2::new(x0, y0 + h), DVec2::new(x0 + w, y0 + h)]
        .iter()
        .map(|c| t.position + turn.rotate(*c))
        .fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(lo, hi), c| (lo.min(c), hi.max(c)))
}

/// The segments of a resolved stroke.
fn segments(points: &[DVec2], closed: bool) -> impl Iterator<Item = (DVec2, DVec2)> + '_ {
    let n = points.len();
    let count = if closed && n > 2 { n } else { n.saturating_sub(1) };
    (0..count).filter_map(move |k| Some((*points.get(k)?, *points.get((k + 1) % n)?)))
}

/// The piece at `p` (millimeters): a text whose box holds it, else the
/// nearest stroke within `tol`, else a picture under it.
pub fn piece_at(pieces: &SheetPieces, p: DVec2, tol: f64) -> Option<usize> {
    let inside =
        |(lo, hi): (DVec2, DVec2)| p.cmpge(lo - DVec2::splat(tol)).all() && p.cmple(hi + DVec2::splat(tol)).all();
    if let Some(t) = pieces.texts.iter().rev().find(|t| inside(text_box(t))) {
        return Some(t.item);
    }
    let stroke = pieces
        .strokes
        .iter()
        .flat_map(|s| segments(&s.points, s.closed).map(move |(a, b)| (s.item, a, b)))
        .map(|(i, a, b)| (cc_geom::polygon::closest_on_segment(a, b, p).0.distance(p), i))
        .filter(|(d, _)| *d <= tol)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, i)| i);
    stroke.or_else(|| pieces.images.iter().rev().find(|im| inside((im.corner, im.corner + im.size))).map(|im| im.item))
}

/// The pieces wholly inside the window from `a` to `b` (millimeters), in order.
pub fn pieces_in(pieces: &SheetPieces, a: DVec2, b: DVec2) -> Vec<usize> {
    let (lo, hi) = (a.min(b), a.max(b));
    let inside = |p: DVec2| p.cmpge(lo).all() && p.cmple(hi).all();
    let texts = pieces.texts.iter().filter(|t| {
        let (l, h) = text_box(t);
        inside(l) && inside(h)
    });
    let mut out: Vec<usize> = pieces
        .strokes
        .iter()
        .filter(|s| s.points.iter().all(|p| inside(*p)))
        .map(|s| s.item)
        .chain(texts.map(|t| t.item))
        .chain(pieces.images.iter().filter(|i| inside(i.corner) && inside(i.corner + i.size)).map(|i| i.item))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Outline the picked pieces in the accent color: strokes along their
/// segments, texts and pictures by their boxes.
fn paint_picked(painter: &egui::Painter, mm: &Xf, pieces: &SheetPieces, picked: &[usize]) {
    if picked.is_empty() {
        return;
    }
    let line = Stroke::new(2.0, PALETTE.accent);
    for s in pieces.strokes.iter().filter(|s| picked.contains(&s.item)) {
        for (a, b) in segments(&s.points, s.closed) {
            painter.line_segment([mm.to_screen(a), mm.to_screen(b)], line);
        }
    }
    let texts = pieces.texts.iter().filter(|t| picked.contains(&t.item)).map(text_box);
    let images = pieces.images.iter().filter(|i| picked.contains(&i.item)).map(|i| (i.corner, i.corner + i.size));
    for (lo, hi) in texts.chain(images) {
        let r = Rect::from_two_pos(mm.to_screen(lo), mm.to_screen(hi));
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, PALETTE.accent), egui::StrokeKind::Middle);
    }
}

/// Change piece `index` with a merge patch over its shape (one undo step per `key`).
fn patch(state: &mut AppState, index: usize, key: Option<u64>, value: serde_json::Value) -> bool {
    let command = ComponentSheetPieceSet {
        type_ref: crate::component_env::type_ref(state),
        index,
        piece: Some(value),
        name: None,
        image_path: None,
        image: None,
        add: Vec::new(),
    };
    let result = match key {
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

/// The picked pieces (none when a plane or nothing is picked).
fn picked(state: &AppState) -> Vec<usize> {
    match state.component_env.as_ref().and_then(|e| e.pick.clone()) {
        Some(crate::component_env::Pick::Sheet(list)) => list,
        _ => Vec::new(),
    }
}

/// Pick pieces `list` (an empty list picks nothing).
fn set_pick(state: &mut AppState, list: Vec<usize>) {
    if let Some(env) = state.component_env.as_mut() {
        env.pick = (!list.is_empty()).then_some(crate::component_env::Pick::Sheet(list));
    }
}

/// A merge key for edits of one value.
fn key(parts: impl std::hash::Hash) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut h);
    h.finish()
}

/// The picked piece's values: name; a stroke's weight, color, fill and
/// *Edit Sketch*.
fn piece_props(ui: &mut Ui, state: &mut AppState, def: &ComponentDef, index: usize) {
    let Some(item) = def.sheet.get(index).cloned() else { return };
    let t = crate::component_env::type_ref(state);
    let mut rename = None;
    crate::table::row(ui, &t!("prop-name"), |ui| {
        rename = crate::component_env::draft_field(ui, state, &format!("tb-piece-name:{index}"), &item.name, 150.0)
    });
    if let Some(name) = rename {
        let command =
            ComponentSheetPieceSet {
                type_ref: t,
                index,
                piece: None,
                name: Some(name),
                image_path: None,
                image: None,
                add: Vec::new(),
            };
        crate::actions::run_command(state, command);
    }
    if let SheetShape::Stroke { weight, color, fill, closed, .. } = &item.shape {
        crate::table::readonly_row(ui, &t!("tb-piece-stroke"), String::new());
        if ui.button(t!("comp-edit-sketch")).clicked() {
            edit_stroke(state, index);
            return;
        }
        number_field(ui, state, index, weight, &NumberField::WEIGHT);
        // Black is the default (no color stored).
        let mut rgb = color.unwrap_or([0, 0, 0]);
        let r = crate::table::row(ui, &t!("vg-color"), |ui| ui.color_edit_button_srgb(&mut rgb));
        let value =
            r.changed().then(|| if rgb == [0, 0, 0] { serde_json::Value::Null } else { serde_json::json!(rgb) });
        color_edit(state, index, "color", value, released(ui, &r));
        crate::table::readonly_row(ui, &t!("tb-closed"), if *closed { "✓".into() } else { "—".into() });
        if *closed {
            let mut on = fill.is_some();
            let mut rgb = fill.unwrap_or([220, 220, 220]);
            let (toggled, r) = crate::table::row(ui, &t!("tb-fill"), |ui| {
                ui.horizontal(|ui| (ui.checkbox(&mut on, "").changed(), ui.color_edit_button_srgb(&mut rgb))).inner
            });
            if toggled {
                let value = if on { serde_json::json!(rgb) } else { serde_json::Value::Null };
                patch(state, index, None, serde_json::json!({ "Stroke": { "fill": value } }));
            } else {
                // A color picked turns the fill on.
                color_edit(state, index, "fill", r.changed().then(|| serde_json::json!(rgb)), released(ui, &r));
            }
        }
    }
    if let SheetShape::Text { x, y, text, size, anchor, rotation } = &item.shape {
        text_props(ui, state, def, index, [x, y, size, rotation], text, *anchor);
    }
    // A picture's place and size (its bytes are never patched).
    if let SheetShape::Image { x, y, width, height, .. } = &item.shape {
        let fields: [(&str, String, &str); 4] = [
            ("x", t!("comp-field-x"), x),
            ("y", t!("comp-field-y"), y),
            ("width", t!("comp-field-width"), width),
            ("height", t!("prop-height"), height),
        ];
        for (field, label, current) in fields {
            let mut new = None;
            crate::table::row(ui, &label, |ui| {
                new = crate::component_env::draft_field(ui, state, &format!("tb-img-{field}:{index}"), current, 120.0)
            });
            if let Some(v) = new {
                patch(state, index, None, serde_json::json!({ "Image": { field: v } }));
            }
        }
    }
}

/// A picture for the Image tool: PNG, JPEG, GIF, BMP or WebP bytes (an SVG
/// drawn into a PNG), and its width / height.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub bytes: Vec<u8>,
    pub aspect: f64,
}

/// Longest side an SVG is drawn at, pixels.
const SVG_PX: f32 = 2048.0;

/// The picture in a file's bytes; an SVG is drawn into a PNG. `None` for
/// anything else, a file over the picture size limit, or a reader that
/// fails (a panic in a decoder is caught: files are hostile input).
pub fn picture(bytes: Vec<u8>) -> Option<Picture> {
    if bytes.len() > cc_core::raster::MAX_IMAGE_BYTES {
        return None;
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_picture(bytes))).ok().flatten()
}

fn read_picture(bytes: Vec<u8>) -> Option<Picture> {
    if cc_core::raster::RasterFormat::sniff(&bytes).is_some() {
        let img = cc_views::raster::probe(bytes)?;
        let aspect = f64::from(img.width) / f64::from(img.height.max(1));
        return Some(Picture { bytes: img.data, aspect });
    }
    let tree = resvg::usvg::Tree::from_data(&bytes, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    let aspect = f64::from(size.width()) / f64::from(size.height());
    let k = SVG_PX / size.width().max(size.height()).max(1.0);
    if !k.is_finite() || !aspect.is_finite() || aspect <= 0.0 {
        return None;
    }
    let side = |v: f32| (v * k).round().clamp(1.0, SVG_PX) as u32;
    let (w, h) = (side(size.width()), side(size.height()));
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(k, k), &mut pixmap.as_mut());
    let rgba: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().ok()?.write_image_data(&rgba).ok()?;
    }
    Some(Picture { bytes: png, aspect })
}

/// The picture in file `path`; a file over the picture size limit is
/// refused before it is read.
pub fn picture_file(path: &std::path::Path) -> Result<Picture, String> {
    use std::io::Read;
    let limit = cc_core::raster::MAX_IMAGE_BYTES as u64;
    let unreadable = || t!("msg-image-unreadable");
    let file = std::fs::File::open(path).map_err(|_| unreadable())?;
    if file.metadata().map_err(|_| unreadable())?.len() > limit {
        return Err(t!("err-image-too-large"));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).map_err(|_| unreadable())?;
    if bytes.len() as u64 > limit {
        return Err(t!("err-image-too-large"));
    }
    picture(bytes).ok_or_else(unreadable)
}

/// The rectangle a picture dragged from `a` to `b` fills (millimeters):
/// its bottom-left corner and size; the size keeps `aspect` (width /
/// height) unless `free`, following the longer dragged side.
pub fn image_rect(a: DVec2, b: DVec2, aspect: f64, free: bool) -> (DVec2, DVec2) {
    let d = b - a;
    let mut size = d.abs();
    if !free && aspect.is_finite() && aspect > 0.0 {
        if size.x / aspect >= size.y {
            size.y = size.x / aspect;
        } else {
            size.x = size.y * aspect;
        }
    }
    let x = if d.x < 0.0 { a.x - size.x } else { a.x };
    let y = if d.y < 0.0 { a.y - size.y } else { a.y };
    (DVec2::new(x, y), size)
}

/// File extensions the Image tool opens.
const PICTURE_EXTENSIONS: [&str; 7] = ["png", "jpg", "jpeg", "gif", "bmp", "webp", "svg"];

/// *Image*: pick a picture file, then drag its rectangle.
pub fn pick_image(state: &mut AppState) {
    let Some(path) = rfd::FileDialog::new().add_filter(t!("file-filter-image"), &PICTURE_EXTENSIONS).pick_file() else {
        return;
    };
    match picture_file(&path) {
        Ok(p) => crate::component_env::set_tool(
            state,
            crate::component_env::EnvTool::SheetImage { picture: std::sync::Arc::new(p), start: None },
        ),
        Err(m) => state.message = m,
    }
}

/// Add a picture filling `corner`–`corner + size` (millimeters) and pick
/// it; `false` (with a message) when the rectangle is too small to keep.
fn add_image(state: &mut AppState, planes: &[SheetPlane], corner: DVec2, size: DVec2, p: &Picture) -> bool {
    if size.min_element() < 0.5 {
        state.message = t!("tb-image-too-small");
        return false;
    }
    let piece = SheetPieceArg::Image {
        x: ExprArg::Text(coordinate(planes, corner.x, true)),
        y: ExprArg::Text(coordinate(planes, corner.y, false)),
        width: ExprArg::Number((size.x * 100.0).round() / 100.0),
        height: ExprArg::Number((size.y * 100.0).round() / 100.0),
        image_path: None,
        image: Some(cc_core::raster::base64_encode(&p.bytes)),
    };
    let command = ComponentSheetPieceAdd {
        type_ref: crate::component_env::type_ref(state),
        piece: Some(piece),
        pieces: Vec::new(),
        name: None,
    };
    let Some(out) = crate::actions::run_command(state, command) else { return false };
    if let Some(i) = out["index"].as_u64().and_then(|i| usize::try_from(i).ok()) {
        set_pick(state, vec![i]);
    }
    true
}

/// The grips of a lone picked text (its point) or picture (corner,
/// opposite corner), millimeters.
fn grips(pieces: &SheetPieces, index: usize) -> Vec<(u8, DVec2)> {
    if let Some(t) = pieces.texts.iter().find(|t| t.item == index) {
        return vec![(0, t.position)];
    }
    if let Some(i) = pieces.images.iter().find(|i| i.item == index) {
        return vec![(0, i.corner), (1, i.corner + i.size)];
    }
    Vec::new()
}

/// A grip dragged to `at` (millimeters): a text's or picture's point moves
/// there, a picture's far corner sizes it (its aspect kept unless `free`).
/// One undo step per grip until the drag ends; a picture's bytes are kept.
fn move_grip(
    state: &mut AppState,
    pieces: &SheetPieces,
    planes: &[SheetPlane],
    index: usize,
    which: u8,
    at: DVec2,
    free: bool,
) {
    let x = coordinate(planes, at.x, true);
    let y = coordinate(planes, at.y, false);
    let k = Some(key(("tb-grip", index, which)));
    if pieces.texts.iter().any(|t| t.item == index) {
        patch(state, index, k, serde_json::json!({ "Text": { "x": x, "y": y } }));
    } else if let Some(img) = pieces.images.iter().find(|i| i.item == index) {
        if which == 0 {
            patch(state, index, k, serde_json::json!({ "Image": { "x": x, "y": y } }));
        } else {
            let aspect = img.size.x / img.size.y.max(1e-9);
            let (_, size) = image_rect(img.corner, at, aspect, free);
            let size = size.max(DVec2::splat(0.5));
            let w = format!("{}", (size.x * 100.0).round() / 100.0);
            let h = format!("{}", (size.y * 100.0).round() / 100.0);
            patch(state, index, k, serde_json::json!({ "Image": { "width": w, "height": h } }));
        }
    }
}

/// A text's words (committed on Enter or when the field is left; Shift+Enter
/// starts a new line), *Insert field*, position, height, rotation and alignment.
fn text_props(
    ui: &mut Ui,
    state: &mut AppState,
    def: &ComponentDef,
    index: usize,
    [x, y, size, rotation]: [&String; 4],
    text: &str,
    anchor: TextAnchor,
) {
    crate::table::readonly_row(ui, &t!("tb-tool-text"), String::new());
    let text_id = egui::Id::new(("tb-text", index));
    let mut words = None;
    crate::table::row(ui, &t!("prop-text"), |ui| words = commit_words(ui, text_id, text));
    if let Some(words) = words {
        patch(state, index, None, serde_json::json!({ "Text": { "text": words } }));
    }
    // Insert field: at the text's cursor, or at its end.
    let mut chosen = None;
    let menu = field_menu(state.project(), def);
    ui.menu_button(t!("tb-insert-field"), |ui| {
        for (group, items) in &menu {
            ui.menu_button(group.as_str(), |ui| {
                for (label, k) in items {
                    if ui.button(label.as_str()).clicked() {
                        chosen = Some(k.clone());
                        ui.close();
                    }
                }
            });
        }
    });
    if let Some(k) = chosen {
        let cursor = egui::text_edit::TextEditState::load(ui.ctx(), text_id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| r.primary.index.0);
        insert_into(state, index, &k, cursor);
    }
    for (field, label, current) in [("x", t!("comp-field-x"), x), ("y", t!("comp-field-y"), y)] {
        let mut new = None;
        crate::table::row(ui, &label, |ui| {
            new = crate::component_env::draft_field(ui, state, &format!("tb-{field}:{index}"), current, 120.0)
        });
        if let Some(v) = new {
            patch(state, index, None, serde_json::json!({ "Text": { field: v } }));
        }
    }
    number_field(ui, state, index, size, &NumberField::TEXT_SIZE);
    number_field(ui, state, index, rotation, &NumberField::ROTATION);
    let mut a = anchor;
    let name = |a: TextAnchor| match a {
        TextAnchor::Left => t!("tb-anchor-left"),
        TextAnchor::Center => t!("tb-anchor-center"),
        TextAnchor::Right => t!("tb-anchor-right"),
        TextAnchor::Middle => t!("tb-anchor-middle"),
    };
    crate::table::row(ui, &t!("tb-anchor"), |ui| {
        egui::ComboBox::from_id_salt(("tb-anchor", index)).selected_text(name(a)).show_ui(ui, |ui| {
            for option in [TextAnchor::Left, TextAnchor::Center, TextAnchor::Right, TextAnchor::Middle] {
                ui.selectable_value(&mut a, option, name(option));
            }
        });
    });
    if a != anchor {
        patch(state, index, None, serde_json::json!({ "Text": { "anchor": a } }));
    }
}

/// A text's multiline field: what is typed stays a draft while it has the
/// focus, and is handed back (when it changed) on Enter, which also leaves
/// the field, or when the field loses the focus. Shift+Enter breaks the line.
fn commit_words(ui: &mut Ui, id: egui::Id, current: &str) -> Option<String> {
    let draft = id.with("draft");
    let mut text = ui.data(|d| d.get_temp::<String>(draft)).unwrap_or_else(|| current.to_string());
    let edit = egui::TextEdit::multiline(&mut text)
        .id(id)
        .desired_rows(2)
        .desired_width(170.0)
        .char_limit(cc_core::title_block::MAX_SHEET_TEXT)
        .return_key(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter));
    let r = ui.add(edit);
    let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.is_none());
    if enter {
        ui.memory_mut(|m| m.surrender_focus(id));
    } else if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(draft, text));
        return None;
    }
    ui.data_mut(|d| d.remove::<String>(draft));
    ((enter || r.lost_focus()) && text != current).then_some(text)
}

/// Insert field `key` into text `index` at character `at` (its end when
/// `None`): one undo step.
fn insert_into(state: &mut AppState, index: usize, key: &str, at: Option<usize>) -> bool {
    let Some((_, def)) = crate::component_env::component(state) else { return false };
    let Some(SheetShape::Text { text, .. }) = def.sheet.get(index).map(|s| &s.shape) else { return false };
    let (new, _) = insert_field(text, key, at);
    patch(state, index, None, serde_json::json!({ "Text": { "text": new } }))
}

/// The expression of a coordinate `value` (millimeters): the name of an
/// edge or plane standing there, else the number to a hundredth.
pub fn coordinate(planes: &[SheetPlane], value: f64, vertical: bool) -> String {
    if let Some(p) = planes.iter().find(|p| p.vertical == vertical && (p.at - value).abs() < 1e-6) {
        return p.name.clone();
    }
    let r = (value * 100.0).round() / 100.0;
    format!("{}", if r == 0.0 { 0.0 } else { r })
}

/// Place a text at `at` (millimeters) and pick it.
pub fn place_text(state: &mut AppState, planes: &[SheetPlane], at: DVec2) {
    let piece = SheetPieceArg::Text {
        x: ExprArg::Text(coordinate(planes, at.x, true)),
        y: ExprArg::Text(coordinate(planes, at.y, false)),
        text: t!("tb-tool-text"),
        size: None,
        anchor: None,
        rotation: None,
    };
    let command = ComponentSheetPieceAdd {
        type_ref: crate::component_env::type_ref(state),
        piece: Some(piece),
        pieces: Vec::new(),
        name: None,
    };
    if let Some(i) = crate::actions::run_command(state, command)
        .and_then(|o| o["index"].as_u64())
        .and_then(|i| usize::try_from(i).ok())
    {
        set_pick(state, vec![i]);
    }
}

/// A fixed field's name in the menu.
fn field_label(key: &str) -> String {
    match key {
        "sheet_number" => t!("tb-field-sheet-number"),
        "sheet_name" => t!("tb-field-sheet-name"),
        "sheet_count" => t!("tb-field-sheet-count"),
        "sheet_index" => t!("tb-field-sheet-index"),
        "sheet_scale" => t!("tb-field-sheet-scale"),
        "paper" => t!("tb-field-paper"),
        "project_name" => t!("tb-field-project-name"),
        "project_number" => t!("tb-field-project-number"),
        "client" => t!("tb-field-client"),
        "address" => t!("tb-field-address"),
        "author" => t!("tb-field-author"),
        "date" => t!("tb-field-date"),
        "issue_date" => t!("tb-field-issue-date"),
        other => other.to_string(),
    }
}

/// The *Insert field* menu: groups of (name shown, key). The fixed fields,
/// with `issue_date` right after `date`, then a `project:` key per project
/// field and a `param:` key per text parameter of the title block.
pub fn field_menu(p: &cc_core::Project, def: &ComponentDef) -> Vec<(String, Vec<(String, String)>)> {
    let fixed = |keys: &[&str]| keys.iter().map(|k| (field_label(k), (*k).to_string())).collect::<Vec<_>>();
    let mut project = fixed(&["project_name", "project_number", "client", "address", "author"]);
    project.extend(p.info.fields.iter().map(|f| (f.name.clone(), format!("project:{}", f.name))));
    let params = def
        .params
        .iter()
        .filter(|q| q.kind.is_text())
        .map(|q| (q.display().to_string(), format!("param:{}", q.name)))
        .collect();
    vec![
        (
            t!("tb-group-sheet"),
            fixed(&["sheet_number", "sheet_name", "sheet_count", "sheet_index", "sheet_scale", "paper"]),
        ),
        (t!("tb-group-project"), project),
        (t!("tb-group-dates"), fixed(&["date", "issue_date"])),
        (t!("tb-group-per-sheet"), params),
    ]
}

/// `text` with `{key}` inserted at character `at` (the end when `None` or
/// past it), and the character the cursor goes to after it.
pub fn insert_field(text: &str, key: &str, at: Option<usize>) -> (String, usize) {
    let n = text.chars().count();
    let at = at.unwrap_or(n).min(n);
    let field = format!("{{{key}}}");
    let byte = text.char_indices().nth(at).map_or(text.len(), |(b, _)| b);
    let (head, tail) = text.split_at_checked(byte).unwrap_or((text, ""));
    (format!("{head}{field}{tail}"), at + field.chars().count())
}

/// Whether an interaction with a color button ends this frame: the pointer
/// let go (in its popup or anywhere) or the button lost the focus.
fn released(ui: &Ui, r: &egui::Response) -> bool {
    r.lost_focus() || ui.input(|i| i.pointer.any_released())
}

/// One frame of a stroke's color picker: `value` (when it changed) is
/// patched into `field` under the field's merge key, and the merge ends
/// only once the pointer is let go, after the patch, so a press and drag in
/// the popup is one undo step and the next interaction starts another.
fn color_edit(state: &mut AppState, index: usize, field: &str, value: Option<serde_json::Value>, released: bool) {
    if let Some(value) = value {
        let mut shape = serde_json::Map::new();
        shape.insert(field.to_string(), value);
        patch(state, index, Some(key(("tb-stroke", field, index))), serde_json::json!({ "Stroke": shape }));
    }
    if released {
        state.session.doc.break_merge();
    }
}

/// A numeric value of a piece shown by [`number_field`].
struct NumberField {
    /// The shape's serde name (`Stroke`, `Text`) and the field.
    shape: &'static str,
    field: &'static str,
    /// What an empty expression reads as.
    default: f64,
    min: f64,
    max: f64,
    speed: f64,
    suffix: &'static str,
}

impl NumberField {
    /// A stroke's pen width, millimeters.
    const WEIGHT: NumberField = NumberField {
        shape: "Stroke",
        field: "weight",
        default: cc_core::title_block::DEFAULT_WEIGHT,
        min: 0.01,
        max: 10.0,
        speed: 0.01,
        suffix: " mm",
    };
    /// A text's height, millimeters.
    const TEXT_SIZE: NumberField =
        NumberField { shape: "Text", field: "size", default: 2.5, min: 0.1, max: 100.0, speed: 0.05, suffix: " mm" };
    /// A text's rotation, degrees counter-clockwise.
    const ROTATION: NumberField = NumberField {
        shape: "Text",
        field: "rotation",
        default: 0.0,
        min: -360.0,
        max: 360.0,
        speed: 1.0,
        suffix: "°",
    };

    fn label(&self) -> String {
        match self.field {
            "weight" => t!("vg-weight"),
            "size" => t!("tb-text-size"),
            _ => t!("prop-rotation"),
        }
    }
}

/// A piece's number (a pen width, a text's height or rotation): a number
/// is dragged, one undo step per drag; an expression is typed.
fn number_field(ui: &mut Ui, state: &mut AppState, index: usize, current: &str, spec: &NumberField) {
    let number = if current.trim().is_empty() {
        Some(spec.default)
    } else {
        current.trim().parse::<f64>().ok().filter(|v| v.is_finite())
    };
    let label = spec.label();
    let Some(mut v) = number else {
        let mut typed = None;
        crate::table::row(ui, &label, |ui| {
            let salt = format!("tb-{}:{index}", spec.field);
            typed = crate::component_env::draft_field(ui, state, &salt, current, 120.0)
        });
        if let Some(v) = typed {
            patch(state, index, None, serde_json::json!({ spec.shape: { spec.field: v } }));
        }
        return;
    };
    let r = crate::table::row(ui, &label, |ui| {
        // Only what the user enters is clamped: showing a value never rewrites it.
        let drag =
            egui::DragValue::new(&mut v).speed(spec.speed).range(spec.min..=spec.max).clamp_existing_to_range(false);
        ui.add(drag.max_decimals(3).suffix(spec.suffix))
    });
    if r.changed() {
        let value = serde_json::json!({ spec.shape: { spec.field: format!("{v}") } });
        patch(state, index, Some(key(("tb-number", spec.shape, spec.field, index))), value);
    }
    if r.drag_stopped() || r.lost_focus() {
        state.session.doc.break_merge();
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

/// Whether the open sketch draws a title block's lines.
pub fn is_sheet_sketch(state: &AppState) -> bool {
    state.sketch.as_ref().is_some_and(|s| s.target == Target::Sheet)
}

/// *Lines*: a new sketch on the Sheet view.
pub fn start_lines(state: &mut AppState) {
    if !is_open(state) {
        return;
    }
    state.sketch = Some(Session::new(Target::Sheet, Vec::new()));
    if let Some(env) = state.component_env.as_mut() {
        env.tool = crate::component_env::EnvTool::Select;
        if let Some(ed) = env.sheet.as_mut() {
            ed.editing = None;
        }
    }
}

/// `c` with every length times `k`.
fn scaled(c: &SketchCurve, k: f64) -> SketchCurve {
    match *c {
        SketchCurve::Line { a, b } => SketchCurve::Line { a: a * k, b: b * k },
        SketchCurve::Arc { a, b, bulge } => SketchCurve::Arc { a: a * k, b: b * k, bulge },
        SketchCurve::Circle { center, radius } => SketchCurve::Circle { center: center * k, radius: radius * k },
        SketchCurve::Ellipse { center, radii, rotation_deg, start_deg, sweep_deg } => {
            SketchCurve::Ellipse { center: center * k, radii: radii * k, rotation_deg, start_deg, sweep_deg }
        }
    }
}

/// The curves in chains joined end to end (a chain stops where more than
/// two curves meet), each with whether it closes.
pub fn chains(curves: &[SketchCurve]) -> Vec<(Vec<SketchCurve>, bool)> {
    let mut used = vec![false; curves.len()];
    let mut out = Vec::new();
    for i in 0..curves.len() {
        if used.get(i).copied().unwrap_or(true) {
            continue;
        }
        let (links, closed) = cc_core::sketch_edit::chain(curves, i);
        let mut chain = Vec::new();
        let mut whole = true;
        for (k, _) in links {
            match (used.get_mut(k), curves.get(k)) {
                (Some(u), Some(c)) if !*u => {
                    *u = true;
                    chain.push(*c);
                }
                _ => whole = false,
            }
        }
        if chain.is_empty() {
            if let (Some(u), Some(c)) = (used.get_mut(i), curves.get(i)) {
                *u = true;
                chain.push(*c);
            }
            whole = false;
        }
        out.push((chain, closed && whole));
    }
    out
}

fn expr_text(e: &ExprArg) -> String {
    match e {
        ExprArg::Text(t) => t.clone(),
        ExprArg::Number(v) => format!("{v}"),
    }
}

/// The model's form of a written point (for a merge patch over a stroke).
fn point_of(a: &SketchPointArg) -> ProfilePoint {
    ProfilePoint {
        u: expr_text(&a.u),
        v: expr_text(&a.v),
        radius: a.radius.as_ref().map(expr_text).unwrap_or_default(),
        ellipse: a.ellipse.as_ref().map(|e| cc_core::profile::ProfileEllipse {
            u: expr_text(&e.u),
            v: expr_text(&e.v),
            rx: expr_text(&e.rx),
            ry: expr_text(&e.ry),
            rotation: e.rotation.as_ref().map(expr_text).unwrap_or_default(),
            clockwise: e.clockwise,
        }),
    }
}

/// *Finish* of a lines sketch: each chain becomes a stroke, closed when its
/// ends meet; an edited stroke takes the first chain. A chain that does not
/// make a stroke keeps the sketch open with the reason.
pub fn finish_stroke(state: &mut AppState) {
    let Some(session) = state.sketch.clone() else { return };
    let Some((_, def)) = crate::component_env::component(state) else { return };
    let Some(ed) = editor(state) else { return };
    if session.curves.is_empty() {
        state.message = t!("sketch-empty");
        return;
    }
    let paper = paper(state, ed.preview);
    let view = plane_view(&planes(&def, paper), 1.0);
    let extra = scope_extra(paper);
    let (edit, old) = ed.editing.clone().unzip();
    // Chains are found on the curves in millimeters, the scale the strokes
    // are written and checked at, so a join `chains` accepts also holds there.
    let mm: Vec<SketchCurve> = session.curves.iter().map(|c| scaled(c, MM)).collect();
    let mut strokes = Vec::new();
    for (n, (chain, closed)) in chains(&mm).into_iter().enumerate() {
        let old: &[ProfilePoint] = if n == 0 { old.as_deref().unwrap_or(&[]) } else { &[] };
        match crate::component_sketch::written(&def, &view, &chain, closed, old, &extra) {
            Ok(points) => strokes.push((points, closed)),
            Err(e) => {
                state.message = e;
                return;
            }
        }
    }
    let t = crate::component_env::type_ref(state);
    let new = |(points, closed): (Vec<SketchPointArg>, bool)| SheetPieceArg::Stroke {
        points,
        closed,
        weight: None,
        color: None,
        fill: None,
    };
    let indices = |out: &serde_json::Value, key: &str| -> Vec<usize> {
        out[key].as_array().into_iter().flatten().filter_map(|i| i.as_u64()?.try_into().ok()).collect()
    };
    // One command, so finishing the sketch is one undo step.
    let picked = match edit {
        Some(index) => {
            let mut strokes = strokes.into_iter();
            let Some((points, closed)) = strokes.next() else { return };
            let points: Vec<ProfilePoint> = points.iter().map(point_of).collect();
            let command = ComponentSheetPieceSet {
                type_ref: t,
                index,
                piece: Some(serde_json::json!({ "Stroke": { "points": points, "closed": closed } })),
                name: None,
                image_path: None,
                image: None,
                add: strokes.map(new).collect(),
            };
            let Some(out) = crate::actions::run_command(state, command) else { return };
            std::iter::once(index).chain(indices(&out, "added")).collect()
        }
        None => {
            let command = ComponentSheetPieceAdd {
                type_ref: t,
                piece: None,
                pieces: strokes.into_iter().map(new).collect(),
                name: None,
            };
            let Some(out) = crate::actions::run_command(state, command) else { return };
            indices(&out, "indices")
        }
    };
    state.sketch = None;
    if let Some(env) = state.component_env.as_mut() {
        env.pick = Some(crate::component_env::Pick::Sheet(picked));
        if let Some(ed) = env.sheet.as_mut() {
            ed.editing = None;
        }
    }
}

/// *Edit Sketch* of stroke `index`: its lines and arcs on the Sheet view.
pub fn edit_stroke(state: &mut AppState, index: usize) {
    let Some((_, def)) = crate::component_env::component(state) else { return };
    let Some(ed) = editor(state) else { return };
    let Some(cc_core::title_block::SheetShape::Stroke { points, closed, .. }) = def.sheet.get(index).map(|i| &i.shape)
    else {
        return;
    };
    let scope = def.scope(&Default::default(), &scope_extra(paper(state, ed.preview)));
    let lookup = |name: &str| scope.get(name);
    let Ok(vertices) = cc_core::profile::vertices(points, &lookup) else {
        state.message = t!("sketch-piece-unreadable");
        return;
    };
    let curves = cc_core::profile::to_curves(&vertices, *closed).iter().map(|c| scaled(c, 1.0 / MM)).collect();
    let mut session = Session::new(Target::Sheet, curves);
    session.draw = Draw::Select;
    state.sketch = Some(session);
    if let Some(ed) = editor_mut(state) {
        ed.editing = Some((index, points.clone()));
    }
}

/// Delete pieces `list`, in one step.
pub fn delete_pieces(state: &mut AppState, list: &[usize]) {
    if !list.is_empty() {
        let t = crate::component_env::type_ref(state);
        crate::actions::run_command(state, ComponentSheetPieceDelete { type_ref: t, index: None, indices: list.to_vec() });
    }
    if let Some(env) = state.component_env.as_mut() {
        env.pick = None;
    }
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
        crate::ribbon::env_group(ui, state, t!("group-draw"), &[TbLines, TbText, TbImage]);
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
        Some(crate::component_env::EnvTool::Select) => t!("tb-hint-select"),
        Some(crate::component_env::EnvTool::SheetText) => t!("tb-hint-text"),
        Some(crate::component_env::EnvTool::SheetImage { .. }) => t!("tb-hint-image"),
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
    if let [index] = picked(state)[..] {
        crate::table::section(ui, "tb-piece", t!("comp-env-piece"), |ui| piece_props(ui, state, def, index));
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

    fn rect(a: (f64, f64), b: (f64, f64)) -> Vec<cc_core::sketch::SketchCurve> {
        cc_engine::commands::sketch::CurveArg::Rectangle { corner: [a.0, a.1], opposite: [b.0, b.1], rotation: 0.0 }
            .curves()
            .unwrap()
    }

    #[test]
    fn chains_split_a_sketch_into_strokes() {
        use cc_core::sketch::SketchCurve;
        let mut curves = rect((0.0, 0.0), (1.0, 1.0));
        curves.push(SketchCurve::Line { a: DVec2::new(2.0, 0.0), b: DVec2::new(3.0, 0.0) });
        curves.push(SketchCurve::Line { a: DVec2::new(3.0, 0.0), b: DVec2::new(3.0, 1.0) });
        let c = chains(&curves);
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].0.len(), c[0].1), (4, true));
        assert_eq!((c[1].0.len(), c[1].1), (2, false));
    }

    #[test]
    fn sketched_lines_become_strokes_that_follow_the_paper() {
        let (mut state, _, _) = editor_state();
        let before = crate::component_env::component(&state).unwrap().1.sheet.len();
        start_lines(&mut state);
        assert!(is_sheet_sketch(&state));
        // A frame 10 mm in from the sheet's left and bottom, out to its right edge (in paper meters).
        let ed = state.component_env.as_ref().and_then(|e| e.sheet.clone()).unwrap();
        let right = paper(&state, ed.preview).x / MM;
        state.sketch.as_mut().unwrap().curves = rect((0.01, 0.01), (right, 0.1));
        crate::sketch_mode::finish(&mut state);
        assert!(state.sketch.is_none(), "{}", state.message);
        let (_, def) = crate::component_env::component(&state).unwrap();
        assert_eq!(def.sheet.len(), before + 1);
        let cc_core::title_block::SheetShape::Stroke { points, closed, .. } = &def.sheet[before].shape else {
            panic!()
        };
        assert!(*closed);
        assert!(points.iter().any(|p| p.u == "SheetWidth"), "{points:?}");
        assert!(points.iter().any(|p| p.u == "10" && p.v == "10"), "{points:?}");
        assert_eq!(state.component_env.as_ref().unwrap().pick, Some(crate::component_env::Pick::Sheet(vec![before])));
        // On A1 the right side follows the paper.
        let a1 = def.resolve_sheet(DVec2::new(841.0, 594.0));
        let stroke = a1.strokes.iter().find(|s| s.item == before).unwrap();
        assert!(stroke.points.iter().any(|p| (p.x - 841.0).abs() < 1e-9));

        // Edit Sketch: moving the bottom-left corner keeps the edge points as written.
        edit_stroke(&mut state, before);
        let s = state.sketch.as_mut().unwrap();
        assert_eq!(s.curves.len(), 4);
        for c in &mut s.curves {
            if let cc_core::sketch::SketchCurve::Line { a, b } = c {
                for p in [a, b] {
                    if p.distance(DVec2::new(0.01, 0.01)) < 1e-9 {
                        *p = DVec2::new(0.02, 0.01);
                    }
                }
            }
        }
        crate::sketch_mode::finish(&mut state);
        let (_, def) = crate::component_env::component(&state).unwrap();
        assert_eq!(def.sheet.len(), before + 1, "edited, not added");
        let cc_core::title_block::SheetShape::Stroke { points, .. } = &def.sheet[before].shape else { panic!() };
        assert!(points.iter().any(|p| p.u == "20"), "{points:?}");
        assert!(points.iter().any(|p| p.u == "SheetWidth"), "{points:?}");
        // Delete removes the picked strokes.
        crate::component_env::delete_pick(&mut state);
        assert_eq!(crate::component_env::component(&state).unwrap().1.sheet.len(), before);
    }

    #[test]
    fn finishing_or_deleting_many_strokes_is_one_undo_step() {
        use cc_core::sketch::SketchCurve;
        let (mut state, _, _) = editor_state();
        let before = crate::component_env::component(&state).unwrap().1.sheet.len();
        let depth = state.session.doc.undo_depth();
        // Two chains: a frame and a separate line.
        start_lines(&mut state);
        let mut curves = rect((0.01, 0.01), (0.05, 0.03));
        curves.push(SketchCurve::Line { a: DVec2::new(0.06, 0.01), b: DVec2::new(0.09, 0.01) });
        state.sketch.as_mut().unwrap().curves = curves.clone();
        crate::sketch_mode::finish(&mut state);
        assert!(state.sketch.is_none(), "{}", state.message);
        assert_eq!(crate::component_env::component(&state).unwrap().1.sheet.len(), before + 2);
        assert_eq!(state.session.doc.undo_depth(), depth + 1, "two strokes, one step");
        let picked = vec![before, before + 1];
        assert_eq!(state.component_env.as_ref().unwrap().pick, Some(crate::component_env::Pick::Sheet(picked)));

        // Edit Sketch of the first, then a third chain: the patch and the new stroke are one step.
        edit_stroke(&mut state, before);
        state.sketch.as_mut().unwrap().curves.push(SketchCurve::Line {
            a: DVec2::new(0.06, 0.05),
            b: DVec2::new(0.09, 0.05),
        });
        crate::sketch_mode::finish(&mut state);
        assert!(state.sketch.is_none(), "{}", state.message);
        assert_eq!(crate::component_env::component(&state).unwrap().1.sheet.len(), before + 3);
        assert_eq!(state.session.doc.undo_depth(), depth + 2, "edited and added, one step");

        // A window of three pieces is deleted in one step.
        delete_pieces(&mut state, &[before, before + 2, before + 1]);
        assert_eq!(crate::component_env::component(&state).unwrap().1.sheet.len(), before);
        assert_eq!(state.session.doc.undo_depth(), depth + 3, "three deleted, one step");
        state.session.doc.undo();
        assert_eq!(crate::component_env::component(&state).unwrap().1.sheet.len(), before + 3);
    }

    #[test]
    fn a_join_the_sketch_accepts_holds_in_millimeters() {
        use cc_core::sketch::SketchCurve;
        let (mut state, _, _) = editor_state();
        let before = crate::component_env::component(&state).unwrap().1.sheet.len();
        start_lines(&mut state);
        // Two lines whose ends are 0.05 mm apart: within the sketch's join
        // tolerance in paper meters, beyond it in millimeters.
        let gap = cc_core::sketch::JOIN_TOL / 2.0;
        state.sketch.as_mut().unwrap().curves = vec![
            SketchCurve::Line { a: DVec2::new(0.01, 0.01), b: DVec2::new(0.05, 0.01) },
            SketchCurve::Line { a: DVec2::new(0.05 + gap, 0.01), b: DVec2::new(0.05, 0.04) },
        ];
        crate::sketch_mode::finish(&mut state);
        assert!(state.sketch.is_none(), "finished: {}", state.message);
        assert!(crate::component_env::component(&state).unwrap().1.sheet.len() > before);
    }

    #[test]
    fn pieces_are_picked_by_their_lines_boxes_and_windows() {
        use cc_core::title_block::{SheetPieces, SheetStroke, SheetText, TextAnchor};
        let pieces = SheetPieces {
            strokes: vec![SheetStroke {
                item: 0,
                points: vec![DVec2::new(10.0, 10.0), DVec2::new(100.0, 10.0)],
                closed: false,
                weight: 0.25,
                color: [0, 0, 0],
                fill: None,
            }],
            texts: vec![SheetText {
                item: 1,
                position: DVec2::new(50.0, 50.0),
                text: "ABCD".into(),
                size: 5.0,
                anchor: TextAnchor::Right,
                rotation_deg: 0.0,
            }],
            ..Default::default()
        };
        assert_eq!(piece_at(&pieces, DVec2::new(40.0, 10.4), 1.0), Some(0));
        // A right-aligned text runs left of its point: 4 × 0.6 × 5 = 12 mm.
        assert_eq!(piece_at(&pieces, DVec2::new(42.0, 52.0), 0.5), Some(1));
        assert_eq!(piece_at(&pieces, DVec2::new(58.0, 52.0), 0.5), None);
        let (lo, hi) = text_box(&pieces.texts[0]);
        assert!((lo.x - 38.0).abs() < 1e-9 && (hi.x - 50.0).abs() < 1e-9 && (hi.y - 55.0).abs() < 1e-9);
        assert_eq!(pieces_in(&pieces, DVec2::new(0.0, 0.0), DVec2::new(120.0, 20.0)), vec![0]);
        assert_eq!(pieces_in(&pieces, DVec2::new(120.0, 60.0), DVec2::new(0.0, 0.0)), vec![0, 1]);
    }

    /// The index of the first stroke of the open title block, and its weight as written.
    fn first_stroke(state: &AppState) -> (usize, String) {
        let (_, def) = crate::component_env::component(state).unwrap();
        def.sheet
            .iter()
            .enumerate()
            .find_map(|(i, s)| match &s.shape {
                cc_core::title_block::SheetShape::Stroke { weight, .. } => Some((i, weight.clone())),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn dragging_a_stroke_weight_is_one_undo_step() {
        let (mut state, _, _) = editor_state();
        let (index, before) = first_stroke(&state);
        let depth = state.session.doc.undo_depth();
        let k = key(("tb-weight", index));
        for w in [0.3, 0.4, 0.5] {
            assert!(patch(&mut state, index, Some(k), serde_json::json!({ "Stroke": { "weight": format!("{w}") } })));
        }
        state.session.doc.break_merge();
        assert_eq!(first_stroke(&state).1, "0.5");
        assert_eq!(state.session.doc.undo_depth(), depth + 1, "one undo step");
        state.session.doc.undo();
        assert_eq!(first_stroke(&state).1, before);
        // Without a key each edit is its own step.
        patch(&mut state, index, None, serde_json::json!({ "Stroke": { "weight": "0.3" } }));
        patch(&mut state, index, None, serde_json::json!({ "Stroke": { "weight": "0.4" } }));
        assert_eq!(state.session.doc.undo_depth(), depth + 2);
    }

    /// A harness showing the properties of stroke `index` of the open title block.
    fn props_harness(state: AppState, index: usize) -> egui_kittest::Harness<'static, AppState> {
        egui_kittest::Harness::builder().with_size(egui::vec2(400.0, 600.0)).build_ui_state(
            move |ui, state: &mut AppState| {
                let Some((_, def)) = crate::component_env::component(state) else { return };
                piece_props(ui, state, &def, index);
            },
            state,
        )
    }

    /// Press at `from`, drag through `to` in a few frames, and let go there.
    fn drag(harness: &mut egui_kittest::Harness<'static, AppState>, from: egui::Pos2, to: egui::Pos2) {
        harness.drag_at(from);
        harness.step();
        for k in 1..=4 {
            harness.hover_at(from + (to - from) * (k as f32 / 4.0));
            harness.step();
        }
        harness.drop_at(to);
        harness.run();
    }

    #[test]
    fn each_drag_in_the_palette_is_one_undo_step() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::Queryable;
        let (state, _, _) = editor_state();
        let (index, before) = first_stroke(&state);
        let mut harness = props_harness(state, index);
        harness.run();
        let depth = harness.state().session.doc.undo_depth();

        // The weight: two drags, two steps.
        let spin = harness.get_by_role(Role::SpinButton).rect().center();
        drag(&mut harness, spin, spin + egui::vec2(30.0, 0.0));
        let after_one = first_stroke(harness.state()).1;
        assert_ne!(after_one, before, "the drag changed the weight");
        assert_eq!(harness.state().session.doc.undo_depth(), depth + 1, "one drag, one step");
        drag(&mut harness, spin, spin + egui::vec2(20.0, 0.0));
        assert_eq!(harness.state().session.doc.undo_depth(), depth + 2, "the next drag is its own step");
        harness.state_mut().session.doc.undo();
        assert_eq!(first_stroke(harness.state()).1, after_one);

        // The color: open the picker, then press and drag inside it: one step.
        let depth = harness.state().session.doc.undo_depth();
        let button = harness.query_all_by_role(Role::ColorWell).next().expect("the color button").rect();
        harness.query_all_by_role(Role::ColorWell).next().expect("the color button").click();
        harness.run();
        assert_eq!(harness.state().session.doc.undo_depth(), depth, "opening the picker changes nothing");
        // The popup's red value (the leftmost number below the button).
        let red = harness
            .query_all_by_role(Role::SpinButton)
            .map(|n| n.rect())
            .filter(|r| r.top() >= button.bottom())
            .min_by(|a, b| a.left().total_cmp(&b.left()))
            .expect("the picker's red value");
        drag(&mut harness, red.center(), red.center() + egui::vec2(60.0, 0.0));
        assert_eq!(harness.state().session.doc.undo_depth(), depth + 1, "press and drag in the picker, one step");
        let color = match &crate::component_env::component(harness.state()).unwrap().1.sheet[index].shape {
            cc_core::title_block::SheetShape::Stroke { color, .. } => *color,
            _ => panic!(),
        };
        assert!(color.is_some(), "the drag picked a color");
        // The picker's color plane (below the values and the current color)
        // changes on the press: press and drag there is the next step, one only.
        let plane = red.left_bottom() + egui::vec2(120.0, 140.0);
        drag(&mut harness, plane, plane + egui::vec2(40.0, -40.0));
        assert_eq!(harness.state().session.doc.undo_depth(), depth + 2, "the next press and drag, one more step");
        let after = match &crate::component_env::component(harness.state()).unwrap().1.sheet[index].shape {
            cc_core::title_block::SheetShape::Stroke { color, .. } => *color,
            _ => panic!(),
        };
        assert_ne!(after, color, "the plane changed the color");
    }

    #[test]
    fn dragging_a_parameter_is_one_undo_step() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::Queryable;
        // Both rows: the title block's plain numbers and the other components' values.
        for title_block in [true, false] {
            let (mut state, _, _) = editor_state();
            let command = cc_engine::commands::component::ComponentParamAdd {
                type_ref: crate::component_env::type_ref(&state),
                name: "Gap".into(),
                value: Some(5.0),
                kind: Some("number".into()),
                instance: None,
                text: None,
            };
            state.session.run(command).unwrap();
            let gap = |s: &AppState| {
                let (_, def) = crate::component_env::component(s).unwrap();
                def.params.iter().find(|p| p.name == "Gap").map(|p| p.value).unwrap()
            };
            let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(400.0, 600.0)).build_ui_state(
                move |ui, state: &mut AppState| {
                    let Some((_, def)) = crate::component_env::component(state) else { return };
                    crate::component_env::params(ui, state, &def, title_block);
                },
                state,
            );
            harness.run();
            let depth = harness.state().session.doc.undo_depth();
            let spin = harness.query_all_by_role(Role::SpinButton).last().expect("the Gap value").rect().center();
            drag(&mut harness, spin, spin + egui::vec2(40.0, 0.0));
            assert_ne!(gap(harness.state()), 5.0, "the drag changed the value");
            assert_eq!(harness.state().session.doc.undo_depth(), depth + 1, "one drag, one step ({title_block})");
            drag(&mut harness, spin, spin + egui::vec2(20.0, 0.0));
            assert_eq!(harness.state().session.doc.undo_depth(), depth + 2, "the next drag is its own step");
            harness.state_mut().session.doc.undo();
            harness.state_mut().session.doc.undo();
            assert_eq!(gap(harness.state()), 5.0);
        }
    }

    #[test]
    fn showing_an_out_of_range_weight_keeps_it() {
        let (mut state, _, _) = editor_state();
        let (index, _) = first_stroke(&state);
        assert!(patch(&mut state, index, None, serde_json::json!({ "Stroke": { "weight": "20" } })));
        let depth = state.session.doc.undo_depth();
        let mut harness = props_harness(state, index);
        harness.run();
        assert_eq!(first_stroke(harness.state()).1, "20");
        assert_eq!(harness.state().session.doc.undo_depth(), depth);
    }

    #[test]
    fn a_color_interaction_is_one_undo_step() {
        // The frames of one press, drag and release in the picker, then a second one.
        let (mut state, _, _) = editor_state();
        let (index, _) = first_stroke(&state);
        let depth = state.session.doc.undo_depth();
        let frames = [(Some([10, 0, 0]), false), (Some([20, 0, 0]), false), (Some([30, 0, 0]), true)];
        for (rgb, released) in frames {
            color_edit(&mut state, index, "color", rgb.map(|c| serde_json::json!(c)), released);
        }
        assert_eq!(state.session.doc.undo_depth(), depth + 1);
        color_edit(&mut state, index, "color", Some(serde_json::json!([40, 0, 0])), true);
        assert_eq!(state.session.doc.undo_depth(), depth + 2);
    }

    #[test]
    fn a_null_color_patch_restores_black() {
        let (mut state, _, _) = editor_state();
        let (index, _) = first_stroke(&state);
        assert!(patch(&mut state, index, None, serde_json::json!({ "Stroke": { "color": [255, 0, 0] } })));
        let color = |state: &AppState| match &crate::component_env::component(state).unwrap().1.sheet[index].shape {
            cc_core::title_block::SheetShape::Stroke { color, .. } => *color,
            _ => panic!(),
        };
        assert_eq!(color(&state), Some([255, 0, 0]));
        assert!(patch(&mut state, index, None, serde_json::json!({ "Stroke": { "color": null } })));
        assert_eq!(color(&state), None);
    }

    #[test]
    fn picking_selects_and_double_click_target_opens_the_sketch() {
        let (mut state, _, _) = editor_state();
        let (index, _) = first_stroke(&state);
        set_pick(&mut state, vec![index]);
        assert_eq!(picked(&state), vec![index]);
        edit_stroke(&mut state, index);
        assert!(is_sheet_sketch(&state));
        set_pick(&mut state, Vec::new());
        assert!(picked(&state).is_empty());
        assert!(state.component_env.as_ref().unwrap().pick.is_none());
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

    #[test]
    fn fields_are_inserted_where_the_cursor_is() {
        assert_eq!(insert_field("Sheet  of", "sheet_index", Some(6)), ("Sheet {sheet_index} of".to_string(), 19));
        assert_eq!(insert_field("Nº ", "sheet_number", None).0, "Nº {sheet_number}");
        assert_eq!(insert_field("ab", "date", Some(99)).0, "ab{date}");
    }

    #[test]
    fn texts_are_placed_on_planes_and_offer_every_field() {
        let (mut state, _, tb) = editor_state();
        let (_, def) = crate::component_env::component(&state).unwrap();
        let ps = planes(&def, DVec2::new(420.0, 297.0));
        assert_eq!(coordinate(&ps, 420.0, true), "SheetWidth");
        assert_eq!(coordinate(&ps, 12.3456, true), "12.35");
        let before = def.sheet.len();
        place_text(&mut state, &ps, DVec2::new(420.0, 12.0));
        let (_, def) = crate::component_env::component(&state).unwrap();
        let Some(SheetShape::Text { x, y, .. }) = def.sheet.get(before).map(|s| &s.shape) else { panic!() };
        assert_eq!((x.as_str(), y.as_str()), ("SheetWidth", "12"));
        // The menu: sheet, project (with project fields), dates, per-sheet fields.
        let command = cc_engine::commands::component::ComponentParamAdd {
            type_ref: cc_engine::Ref::Id(tb.0),
            name: "Drawn".into(),
            value: None,
            kind: Some("text".into()),
            instance: Some(true),
            text: Some(String::new()),
        };
        crate::actions::run_command(&mut state, command).unwrap();
        crate::engine_edit::info(&mut state, None, |i| {
            i.fields.push(cc_core::ProjectField { name: "Engineer".into(), value: "Rui".into() })
        });
        let (_, def) = crate::component_env::component(&state).unwrap();
        let menu = field_menu(state.project(), &def);
        let keys: Vec<&str> = menu.iter().flat_map(|(_, items)| items.iter().map(|(_, k)| k.as_str())).collect();
        for k in cc_views::sheet_fields::KEYS.iter().chain(["issue_date", "project:Engineer", "param:Drawn"].iter()) {
            assert!(keys.contains(k), "{k} missing from {keys:?}");
        }
        let dates = &menu.iter().find(|(g, _)| *g == t!("tb-group-dates")).unwrap().1;
        assert_eq!(dates.iter().map(|(_, k)| k.as_str()).collect::<Vec<_>>(), ["date", "issue_date"]);
        assert!(dates.iter().all(|(label, k)| label != k), "labels are translated");
    }

    /// The index and shape of the last piece of the open title block.
    fn last_piece(state: &AppState) -> (usize, SheetShape) {
        let (_, def) = crate::component_env::component(state).unwrap();
        (def.sheet.len() - 1, def.sheet.last().unwrap().shape.clone())
    }

    #[test]
    fn the_text_tool_places_a_text_and_picks_it() {
        let (mut state, _, _) = editor_state();
        crate::commands::run(&mut state, &egui::Context::default(), crate::commands::UiCommand::TbText);
        assert!(crate::commands::UiCommand::TbText.selected(&state));
        let (_, def) = crate::component_env::component(&state).unwrap();
        let ps = planes(&def, DVec2::new(420.0, 297.0));
        let depth = state.session.doc.undo_depth();
        place_text(&mut state, &ps, DVec2::new(30.0, 40.5));
        assert_eq!(state.session.doc.undo_depth(), depth + 1);
        let (index, shape) = last_piece(&state);
        let SheetShape::Text { x, y, text, .. } = shape else { panic!() };
        assert_eq!((x.as_str(), y.as_str(), text.as_str()), ("30", "40.5", t!("tb-tool-text").as_str()));
        assert_eq!(picked(&state), vec![index]);
    }

    #[test]
    fn a_text_is_committed_on_enter_and_its_size_drags_in_one_step() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::Queryable;
        let (mut state, _, _) = editor_state();
        let (_, def) = crate::component_env::component(&state).unwrap();
        let ps = planes(&def, DVec2::new(420.0, 297.0));
        place_text(&mut state, &ps, DVec2::new(30.0, 40.0));
        let (index, _) = last_piece(&state);
        let mut harness = props_harness(state, index);
        harness.run();
        let depth = harness.state().session.doc.undo_depth();
        let words = |h: &egui_kittest::Harness<'static, AppState>| match last_piece(h.state()).1 {
            SheetShape::Text { text, .. } => text,
            _ => panic!(),
        };
        harness.get_by_role(Role::MultilineTextInput).click();
        harness.run();
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        harness.run();
        harness.get_by_role(Role::MultilineTextInput).type_text("Sheet");
        harness.run();
        assert_eq!(harness.state().session.doc.undo_depth(), depth, "typing changes nothing yet");
        harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Enter);
        harness.run();
        harness.get_by_role(Role::MultilineTextInput).type_text("A1");
        harness.run();
        harness.key_press(egui::Key::Enter);
        harness.run();
        assert_eq!(words(&harness), "Sheet\nA1");
        assert_eq!(harness.state().session.doc.undo_depth(), depth + 1, "one commit, one step");

        // The size: a drag is one step.
        let depth = harness.state().session.doc.undo_depth();
        let spin = harness.query_all_by_role(Role::SpinButton).next().expect("the size").rect().center();
        drag(&mut harness, spin, spin + egui::vec2(30.0, 0.0));
        let size = match last_piece(harness.state()).1 {
            SheetShape::Text { size, .. } => size,
            _ => panic!(),
        };
        assert_ne!(size, "2.5", "the drag changed the size");
        assert_eq!(harness.state().session.doc.undo_depth(), depth + 1, "one drag, one step");
    }

    #[test]
    fn insert_field_patches_the_text_in_one_step() {
        let (mut state, _, _) = editor_state();
        let (_, def) = crate::component_env::component(&state).unwrap();
        let ps = planes(&def, DVec2::new(420.0, 297.0));
        place_text(&mut state, &ps, DVec2::new(30.0, 40.0));
        let (index, _) = last_piece(&state);
        let depth = state.session.doc.undo_depth();
        assert!(insert_into(&mut state, index, "sheet_number", None));
        let SheetShape::Text { text, .. } = last_piece(&state).1 else { panic!() };
        assert_eq!(text, format!("{}{{sheet_number}}", t!("tb-tool-text")));
        assert_eq!(state.session.doc.undo_depth(), depth + 1);
    }

    #[test]
    fn pictures_keep_their_aspect_unless_freed() {
        let (corner, size) = image_rect(DVec2::new(10.0, 10.0), DVec2::new(50.0, 15.0), 2.0, false);
        assert_eq!((corner, size), (DVec2::new(10.0, 10.0), DVec2::new(40.0, 20.0)));
        let (corner, size) = image_rect(DVec2::new(10.0, 10.0), DVec2::new(0.0, 0.0), 1.0, true);
        assert_eq!((corner, size), (DVec2::ZERO, DVec2::new(10.0, 10.0)));
    }

    /// A 2 × 1 SVG drawn into a PNG.
    fn svg_picture() -> Picture {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="red"/></svg>"#;
        picture(svg.to_vec()).expect("an SVG drawn into a PNG")
    }

    #[test]
    fn pictures_and_svgs_are_read_and_placed() {
        // A 1 × 1 PNG.
        let png = cc_core::raster::base64_decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
        )
        .unwrap();
        let p = picture(png).expect("a PNG");
        assert!(p.aspect > 0.0);
        let s = svg_picture();
        assert!((s.aspect - 2.0).abs() < 1e-3);
        assert!(picture(b"not a picture".to_vec()).is_none());
        let (mut state, _, _) = editor_state();
        let (_, def) = crate::component_env::component(&state).unwrap();
        let before = def.sheet.len();
        let ps = planes(&def, DVec2::new(420.0, 297.0));
        assert!(add_image(&mut state, &ps, DVec2::new(15.0, 15.0), DVec2::new(40.0, 20.0), &s));
        let (_, def) = crate::component_env::component(&state).unwrap();
        assert!(matches!(def.sheet.get(before).map(|i| &i.shape), Some(SheetShape::Image { .. })), "{}", state.message);
        assert_eq!(picked(&state), vec![before]);
    }

    #[test]
    fn dragging_a_grip_moves_or_sizes_in_one_step_and_keeps_the_bytes() {
        let (mut state, _, _) = editor_state();
        let paper_mm = DVec2::new(420.0, 297.0);
        let (_, def) = crate::component_env::component(&state).unwrap();
        let ps = planes(&def, paper_mm);
        assert!(add_image(&mut state, &ps, DVec2::new(15.0, 15.0), DVec2::new(40.0, 20.0), &svg_picture()));
        let (index, _) = last_piece(&state);
        let image_of = |state: &AppState| {
            let (_, def) = crate::component_env::component(state).unwrap();
            match def.sheet.get(index).map(|i| i.shape.clone()) {
                Some(SheetShape::Image { x, y, width, height, image }) => (x, y, width, height, image),
                _ => panic!("a picture"),
            }
        };
        let bytes = image_of(&state).4;
        let depth = state.session.doc.undo_depth();
        // Move the corner over three frames: one step.
        for at in [DVec2::new(20.0, 16.0), DVec2::new(25.0, 18.0), DVec2::new(30.0, 20.0)] {
            let (_, def) = crate::component_env::component(&state).unwrap();
            move_grip(&mut state, &def.resolve_sheet(paper_mm), &ps, index, 0, at, false);
        }
        state.session.doc.break_merge();
        let (x, y, ..) = image_of(&state);
        assert_eq!((x.as_str(), y.as_str()), ("30", "20"), "{}", state.message);
        assert_eq!(state.session.doc.undo_depth(), depth + 1, "one drag, one step");
        // Size by the far corner, the aspect kept: 50 wide makes 25 high.
        let (_, def) = crate::component_env::component(&state).unwrap();
        let pieces = def.resolve_sheet(paper_mm);
        assert_eq!(grips(&pieces, index), vec![(0, DVec2::new(30.0, 20.0)), (1, DVec2::new(70.0, 40.0))]);
        move_grip(&mut state, &pieces, &ps, index, 1, DVec2::new(80.0, 30.0), false);
        state.session.doc.break_merge();
        let (_, _, w, h, image) = image_of(&state);
        assert_eq!((w.as_str(), h.as_str()), ("50", "25"));
        assert!(std::sync::Arc::ptr_eq(&image, &bytes), "the picture is kept, not resent");
        assert_eq!(state.session.doc.undo_depth(), depth + 2);
    }

    #[test]
    fn escape_drops_a_started_picture_then_the_tool() {
        use crate::component_env::EnvTool;
        let (mut state, _, _) = editor_state();
        let picture = std::sync::Arc::new(svg_picture());
        let tool = |start| EnvTool::SheetImage { picture: picture.clone(), start };
        crate::component_env::set_tool(&mut state, tool(Some(DVec2::new(1.0, 2.0))));
        assert!(crate::commands::UiCommand::TbImage.selected(&state));
        crate::component_env::escape(&mut state);
        assert_eq!(state.component_env.as_ref().unwrap().tool, tool(None));
        crate::component_env::escape(&mut state);
        assert_eq!(state.component_env.as_ref().unwrap().tool, EnvTool::Select);
    }

    #[test]
    fn broken_and_oversized_pictures_are_refused_without_panicking() {
        let truncated = br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="#;
        assert!(picture(truncated.to_vec()).is_none());
        assert!(picture(br#"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="0"/>"#.to_vec()).is_none());
        // A PNG signature on garbage.
        assert!(picture([0x89, b'P', b'N', b'G', 0, 1, 2, 3].to_vec()).is_none());
        let mut huge = vec![0u8; cc_core::raster::MAX_IMAGE_BYTES + 1];
        huge[..4].copy_from_slice(&[0x89, b'P', b'N', b'G']);
        assert!(picture(huge).is_none());
        let dir = std::env::temp_dir().join(format!("cc-tb-picture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.png");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(cc_core::raster::MAX_IMAGE_BYTES as u64 + 1).unwrap();
        drop(file);
        assert_eq!(picture_file(&path).err(), Some(t!("err-image-too-large")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_picture_too_small_to_keep_is_refused_with_a_message() {
        let (mut state, _, _) = editor_state();
        let (_, def) = crate::component_env::component(&state).unwrap();
        let before = def.sheet.len();
        let ps = planes(&def, DVec2::new(420.0, 297.0));
        assert!(!add_image(&mut state, &ps, DVec2::new(15.0, 15.0), DVec2::new(0.2, 0.1), &svg_picture()));
        assert_eq!(state.message, t!("tb-image-too-small"));
        assert_eq!(crate::component_env::component(&state).unwrap().1.sheet.len(), before);
    }

    /// Edit Title Block, draw a rectangle, place a `{sheet_number}` text,
    /// Finish: the sheet shows its number in the new text.
    #[test]
    fn a_title_block_drawn_in_the_editor_shows_on_its_sheet() {
        let mut state = AppState::new(Document::new(crate::home::sample_project("casa_simples").unwrap()));
        state.project_open = true;
        state.show_home = false;
        let sheet = state.project().sheets_in_order()[0];
        state.active_view = Some(sheet);
        let ctx = egui::Context::default();
        crate::commands::run(&mut state, &ctx, crate::commands::UiCommand::EditTitleBlock);
        assert!(is_open(&state));
        crate::commands::run(&mut state, &ctx, crate::commands::UiCommand::TbLines);
        state.sketch.as_mut().unwrap().curves = rect((0.02, 0.02), (0.1, 0.05));
        crate::sketch_mode::finish(&mut state);
        assert!(state.sketch.is_none(), "{}", state.message);
        let (_, def) = crate::component_env::component(&state).unwrap();
        let ps = planes(&def, paper(&state, Preview::Sheet(sheet)));
        place_text(&mut state, &ps, DVec2::new(30.0, 30.0));
        let Some(crate::component_env::Pick::Sheet(list)) = state.component_env.as_ref().unwrap().pick.clone() else {
            panic!()
        };
        assert!(patch(&mut state, list[0], None, serde_json::json!({ "Text": { "text": "Nº {sheet_number}" } })));
        crate::commands::run(&mut state, &ctx, crate::commands::UiCommand::CompFinish);
        assert!(!is_open(&state));
        let number = match &state.project().view(sheet).unwrap().kind {
            ViewKind::Sheet { number, .. } => number.clone(),
            _ => panic!("a sheet"),
        };
        let composed = state.sheet(sheet).unwrap();
        let want = format!("Nº {number}");
        assert!(
            composed.drawing.prims.iter().any(|p| matches!(p, cc_views::Prim::Text { text, .. } if *text == want)),
            "{want} not on the sheet"
        );
    }
}
