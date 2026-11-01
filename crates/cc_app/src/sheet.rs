//! Sheet composer: paper, title block and placed viewports.

use crate::paint::{HIGHLIGHT, PaintOptions, paint_drawing};
use crate::state::AppState;

use cc_core::{ViewId, ViewKind};
use egui::{Color32, Rect, Sense, Stroke, Ui};
use glam::DVec2;
use std::collections::BTreeSet;

pub fn show(ui: &mut Ui, state: &mut AppState, sheet_id: ViewId) {
    let Some(sheet) = state.sheet(sheet_id) else { return };
    let paper = sheet.paper;
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let rect = resp.rect;
    painter.rect_filled(rect, 0.0, crate::theme::PALETTE.sheet_bg);

    let st = state.view2d.entry(sheet_id).or_default();
    if st.needs_fit(rect) {
        st.fit((DVec2::ZERO, paper), rect);
    }
    st.navigate(ui, &resp, rect);
    let zooming = st.region(ui, &resp, &painter, rect);
    let xf = st.xf(rect);
    let to = |p: DVec2| xf.to_screen(p);

    // Paper with shadow; border, title block and viewports come composed.
    let paper_rect = Rect::from_two_pos(to(DVec2::ZERO), to(paper));
    painter.rect_filled(paper_rect.translate(egui::vec2(4.0, 4.0)), 0.0, Color32::from_black_alpha(60));
    painter.rect_filled(paper_rect, 0.0, Color32::WHITE);
    let opts = PaintOptions { highlight: &BTreeSet::new(), px_per_mm: xf.scale, mono: None, only: None };
    paint_drawing(&painter.with_clip_rect(paper_rect.intersect(rect)), &sheet.drawing, &xf, &opts);

    let placed = &sheet.viewports;
    for p in placed {
        if state.sheet_selection == Some((sheet_id, p.index)) {
            let frame = Rect::from_two_pos(
                to(p.center - p.half - DVec2::splat(4.0)),
                to(p.center + p.half + DVec2::new(25.0, 6.0)),
            );
            painter.rect_stroke(frame, 0.0, Stroke::new(1.5, HIGHLIGHT), egui::StrokeKind::Middle);
        }
    }

    let nav = state.view2d.get(&sheet_id).map(crate::canvas::View2DState::nav).unwrap_or_default();
    if let Some(cmd) = crate::canvas::view_toolbar(ui, state.zoom_default, rect, sheet_id, None, nav) {
        crate::canvas::zoom(state, sheet_id, cmd, 1.0);
    }
    if zooming {
        return;
    }
    // A view dragged from the project browser: outline the paper, place it where dropped.
    if resp.dnd_hover_payload::<crate::browser::ViewDrag>().is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Copy);
        painter.rect_stroke(paper_rect, 0.0, Stroke::new(2.0, HIGHLIGHT), egui::StrokeKind::Outside);
    }
    if let Some(drag) = resp.dnd_release_payload::<crate::browser::ViewDrag>()
        && let Some(pos) = ui.ctx().pointer_interact_pos()
    {
        let center = xf.to_world(pos).clamp(DVec2::ZERO, paper);
        crate::actions::place_on_sheet_at(state, sheet_id, drag.0, center);
        return;
    }
    // Select and drag viewports.
    let hit = |pos: egui::Pos2| {
        let w = xf.to_world(pos);
        placed.iter().rev().find(|p| (w - p.center).abs().cmple(p.half + DVec2::splat(4.0)).all()).map(|p| p.index)
    };
    if resp.clicked() {
        state.sheet_selection = resp.interact_pointer_pos().and_then(hit).map(|i| (sheet_id, i));
        state.selection.clear();
    }
    // A double click on a placed view opens it.
    if resp.double_clicked()
        && let Some(i) = resp.interact_pointer_pos().and_then(hit)
        && let Some(view) = placed.iter().find(|p| p.index == i).map(|p| p.view)
    {
        state.open_view(view);
    }
    if resp.drag_started_by(egui::PointerButton::Primary) {
        let origin = ui.input(|i| i.pointer.press_origin());
        state.sheet_selection = origin.and_then(hit).map(|i| (sheet_id, i));
    }
    if let Some((sid, index)) = state.sheet_selection {
        if sid == sheet_id && resp.dragged_by(egui::PointerButton::Primary) {
            let d = resp.drag_delta();
            let delta = DVec2::new(d.x as f64, -d.y as f64) / xf.scale;
            let key = sheet_id.0 ^ ((index as u64) << 48);
            crate::engine_edit::view(state, sheet_id, Some(key), |x| {
                if let ViewKind::Sheet { viewports, .. } = &mut x.kind
                    && let Some(vp) = viewports.get_mut(index)
                {
                    vp.center += delta;
                }
            });
        }
        if resp.drag_stopped() {
            state.session.doc.break_merge();
        }
    }
    if let Some(p) = resp.hover_pos() {
        state.cursor = Some(xf.to_world(p));
    }
}
