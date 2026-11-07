//! Section marks in plans: picked with the Select tool, moved by dragging,
//! their ends and the pieces of a split line dragged by their handles,
//! flipped with Space. Every change goes through `view_set`.

use crate::paint::{HIGHLIGHT, PREVIEW, Xf};
use crate::state::AppState;
use crate::t;
use crate::tools::Drag;
use cc_core::{ViewId, ViewKind};
use cc_views::{Drawing2D, SectionHandle};
use egui::{Color32, Painter, Pos2, Response, Stroke, Ui};
use glam::DVec2;

/// Screen distance at which a handle is grabbed, points.
const GRIP_PX: f32 = 7.0;

/// The selected section's mark in `drawing`.
fn selected(state: &AppState, drawing: &Drawing2D) -> Option<SectionHandle> {
    let view = state.section_selection?;
    drawing.sections.iter().find(|s| s.view == view).cloned()
}

/// A section mark's line pieces and head circles in one color.
fn outline(painter: &Painter, xf: &Xf, pieces: &[(DVec2, DVec2)], ends: [DVec2; 2], radius: f64, color: Color32) {
    let stroke = Stroke::new(2.0, color);
    for &(a, b) in pieces {
        painter.line_segment([xf.to_screen(a), xf.to_screen(b)], stroke);
    }
    for c in ends {
        painter.circle_stroke(xf.to_screen(c), (radius * xf.scale) as f32, stroke);
    }
}

fn mark_outline(painter: &Painter, xf: &Xf, s: &SectionHandle, color: Color32) {
    outline(painter, xf, &s.pieces, [s.start, s.end], s.radius, color);
}

/// The mark as it would be with other ends.
fn preview(painter: &Painter, xf: &Xf, s: &SectionHandle, start: DVec2, end: DVec2, segments: Option<[f64; 2]>) {
    let pieces = cc_views::section_pieces(start, end, segments, s.head);
    outline(painter, xf, &pieces, [start, end], s.radius, PREVIEW);
}

/// Where the piece of a split line next to one head ends, `t` from that
/// head along the line: kept off the head and short of the other piece.
fn segment_length(s: &SectionHandle, at_start: bool, p: DVec2) -> f64 {
    let (head, other) = if at_start { (s.start, s.end) } else { (s.end, s.start) };
    let dir = (other - head).normalize_or_zero();
    let length = head.distance(other);
    let other_piece = s.segments.map_or(0.0, |[a, b]| if at_start { b } else { a });
    let max = (length - other_piece).max(s.head);
    (p - head).dot(dir).clamp(s.head, max.min(cc_core::MAX_SECTION_SEGMENT))
}

/// Section marks under the Select tool in a plan. Returns `true` when they
/// took the pointer this frame (the element selection is left alone).
pub fn select(ui: &Ui, resp: &Response, painter: &Painter, state: &mut AppState, xf: &Xf, drawing: &Drawing2D) -> bool {
    if drawing.sections.is_empty() && !matches!(state.interaction.drag, Some(Drag::Section { .. })) {
        if resp.clicked() || resp.drag_started() {
            state.section_selection = None;
        }
        return false;
    }
    let raw = resp.hover_pos().map(|p| xf.to_world(p));
    let tol = 5.0 / xf.scale;
    let shift = ui.input(|i| i.modifiers.shift);
    let current = selected(state, drawing);
    let idle = state.interaction.drag.is_none();

    // The selected mark and its handles: squares at the ends, circles where
    // the pieces of a split line end.
    let mut grips: Vec<(Pos2, Drag)> = Vec::new();
    if let Some(s) = &current {
        mark_outline(painter, xf, s, HIGHLIGHT);
        if idle {
            for (at_start, at, fixed) in [(true, s.start, s.end), (false, s.end, s.start)] {
                let p = xf.to_screen(at);
                let r = egui::Rect::from_center_size(p, egui::vec2(9.0, 9.0));
                painter.rect_filled(r, 1.5, Color32::WHITE);
                painter.rect_stroke(r, 1.5, Stroke::new(1.5, HIGHLIGHT), egui::StrokeKind::Middle);
                grips.push((p, Drag::Section { view: s.view, part: Part::End { at_start, fixed }, start: at }));
            }
            for (at_start, at) in s.segment_ends().into_iter().flat_map(|[a, b]| [(true, a), (false, b)]) {
                let p = xf.to_screen(at);
                painter.circle(p, 4.5, Color32::WHITE, Stroke::new(1.5, HIGHLIGHT));
                grips.push((p, Drag::Section { view: s.view, part: Part::Segment { at_start }, start: at }));
            }
        }
    }
    let grip_at = |p: Pos2| grips.iter().find(|(at, _)| at.distance(p) <= GRIP_PX).map(|(_, d)| d.clone());

    // Pre-selection highlight.
    if idle
        && let Some(h) = raw.and_then(|p| drawing.section_at(p, tol))
        && Some(h.view) != state.section_selection
    {
        mark_outline(painter, xf, h, HIGHLIGHT.gamma_multiply(0.7));
    }
    if idle && resp.hover_pos().and_then(grip_at).is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    if resp.drag_started_by(egui::PointerButton::Primary) {
        let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or_default();
        let w = xf.to_world(origin);
        if let Some(drag) = grip_at(origin) {
            state.interaction.drag = Some(drag);
            return true;
        }
        if let Some(h) = drawing.section_at(w, tol) {
            state.section_selection = Some(h.view);
            state.selection.clear();
            state.interaction.drag = Some(Drag::Section { view: h.view, part: Part::Body, start: w });
            return true;
        }
        state.section_selection = None;
        return false;
    }

    if let Some(Drag::Section { view, part, start }) = state.interaction.drag.clone() {
        let Some(s) = drawing.sections.iter().find(|s| s.view == view).cloned() else {
            state.interaction.drag = None;
            return true;
        };
        let level = state.active_view.and_then(|v| state.project().view(v)).and_then(|v| v.level());
        // Where the drag would leave the section.
        let mut moved: Option<(DVec2, DVec2, Option<[f64; 2]>)> = None;
        if let Some(raw) = raw.or(state.cursor) {
            match part {
                Part::Body => {
                    let mut d = raw - start;
                    if shift {
                        d = if d.x.abs() >= d.y.abs() { DVec2::new(d.x, 0.0) } else { DVec2::new(0.0, d.y) };
                    }
                    let step = crate::tools::round_step(xf.scale);
                    let d = (d / step).round() * step;
                    preview(painter, xf, &s, s.start + d, s.end + d, s.segments);
                    crate::tools::label(
                        painter,
                        xf.to_screen(raw),
                        format!("{} {}", t!("label-move"), cc_views::format::length(d.length())),
                    );
                    moved = (d.length() > 1e-6).then_some((s.start + d, s.end + d, s.segments));
                }
                Part::End { at_start, fixed } => {
                    let snap = crate::tools::snap_at(state, level, raw, Some(fixed), &[], xf.scale, shift);
                    crate::tools::draw_snap(painter, xf, snap);
                    let (a, b) = if at_start { (snap.point, fixed) } else { (fixed, snap.point) };
                    preview(painter, xf, &s, a, b, s.segments);
                    crate::tools::temp_dimension(painter, xf, fixed, snap.point, None, None, 0.0);
                    moved = (a.distance(b) > 0.1).then_some((a, b, s.segments));
                }
                Part::Segment { at_start } => {
                    let t = segment_length(&s, at_start, raw);
                    let [s0, s1] = s.segments.unwrap_or_default();
                    let segments = if at_start { [t, s1] } else { [s0, t] };
                    preview(painter, xf, &s, s.start, s.end, Some(segments));
                    let head = if at_start { s.start } else { s.end };
                    crate::tools::label(
                        painter,
                        xf.to_screen(raw),
                        cc_views::format::length(head.distance(raw).min(t)),
                    );
                    moved = Some((s.start, s.end, Some(segments)));
                }
            }
        }
        if resp.drag_stopped() {
            state.interaction.drag = None;
            if let Some((a, b, segments)) = moved {
                set(state, view, a, b, segments);
            }
        }
        return true;
    }

    if resp.clicked() {
        if let Some(h) = raw.and_then(|p| drawing.section_at(p, tol)) {
            state.section_selection = Some(h.view);
            state.selection.clear();
            return true;
        }
        state.section_selection = None;
    }
    false
}

/// What a drag of a section mark holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Part {
    /// The whole mark, moved.
    Body,
    /// One end; `fixed` is the other.
    End { at_start: bool, fixed: DVec2 },
    /// The inner end of a split line's piece next to the start or the end.
    Segment { at_start: bool },
}

fn set(state: &mut AppState, view: ViewId, a: DVec2, b: DVec2, new_segments: Option<[f64; 2]>) {
    crate::engine_edit::view(state, view, None, |x| {
        if let ViewKind::Section { start, end, segments, .. } = &mut x.kind {
            (*start, *end, *segments) = (a, b, new_segments);
        }
    });
}

/// Turn the selected section to look the other way (Space).
pub fn flip(state: &mut AppState) -> bool {
    let Some(view) = state.section_selection else { return false };
    let done = crate::engine_edit::view(state, view, None, |x| {
        x.kind.flip_section();
    });
    if done {
        state.message = t!("msg-section-flipped");
    }
    done
}

/// Split the section's line (a piece next to each head, a quarter of it
/// long), or draw it whole again.
pub fn set_split(state: &mut AppState, view: ViewId, split: bool) {
    crate::engine_edit::view(state, view, None, |x| {
        if let ViewKind::Section { start, end, segments, .. } = &mut x.kind {
            let quarter = (start.distance(*end) * 0.25).min(cc_core::MAX_SECTION_SEGMENT);
            *segments = split.then_some([quarter, quarter]);
        }
    });
}
