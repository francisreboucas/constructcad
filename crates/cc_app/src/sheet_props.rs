//! A sheet's rows in the properties palette: its number, title block,
//! paper and issue date, and the title block's per-sheet fields. Each
//! change runs `sheet_set` (through [`crate::engine_edit::sheet`]).

use crate::state::AppState;
use crate::t;
use crate::table::row;
use cc_core::{ElementTypeId, MAX_PAPER_MM, MIN_PAPER_MM, SheetSize, View, ViewId, ViewKind};
use egui::{DragValue, Response, Ui};
use std::hash::{Hash, Hasher};

fn key(parts: impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut h);
    h.finish()
}

/// End coalescing when a drag or text edit finishes.
fn finish(state: &mut AppState, r: &Response) {
    if r.drag_stopped() || r.lost_focus() {
        state.session.doc.break_merge();
    }
}

/// Today as `YYYY-MM-DD`.
pub fn today_iso() -> String {
    let (y, m, d) = crate::i18n::today();
    format!("{y:04}-{m:02}-{d:02}")
}

/// Turn a sheet's ISO paper into a custom one of the size it shows now.
fn make_custom(v: &mut View) {
    let Some(paper) = v.paper_mm() else { return };
    if let ViewKind::Sheet { size, portrait, .. } = &mut v.kind {
        *size = SheetSize::Custom { width: paper.x, height: paper.y };
        *portrait = false;
    }
}

/// The sheet's own rows (inside its properties section).
pub fn rows(ui: &mut Ui, state: &mut AppState, id: ViewId) {
    let Some(view) = state.project().view(id).cloned() else { return };
    let ViewKind::Sheet { number, size, portrait, title_block, issue_date, .. } = &view.kind else { return };
    let mut refit = false;

    // Number.
    let mut text = number.clone();
    let r = row(ui, &t!("prop-sheet-number"), |ui| {
        crate::table::live_text(ui, ("sheet-number", id.0), &mut text, |ui, t| ui.text_edit_singleline(t))
    });
    if r.changed() {
        crate::engine_edit::sheet(state, id, Some(key((id, "number"))), |v| {
            if let ViewKind::Sheet { number, .. } = &mut v.kind {
                *number = text;
            }
        });
    }
    finish(state, &r);

    // Title block: the project's title block types, or none.
    let p = state.project();
    let blocks: Vec<(ElementTypeId, String)> =
        p.title_blocks().into_iter().filter_map(|t| p.element_type(t).map(|ty| (t, ty.name.clone()))).collect();
    let none = t!("prop-title-block-none");
    let shown =
        title_block.and_then(|t| blocks.iter().find(|(b, _)| *b == t)).map_or_else(|| none.clone(), |(_, n)| n.clone());
    let mut chosen = *title_block;
    row(ui, &t!("prop-title-block"), |ui| {
        egui::ComboBox::from_id_salt(("title-block", id.0)).selected_text(shown).show_ui(ui, |ui| {
            ui.selectable_value(&mut chosen, None, none.clone());
            for (t, n) in &blocks {
                ui.selectable_value(&mut chosen, Some(*t), n);
            }
        });
    });
    if chosen != *title_block {
        crate::engine_edit::sheet(state, id, None, |v| {
            if let ViewKind::Sheet { title_block, .. } = &mut v.kind {
                *title_block = chosen;
            }
        });
    }

    // Paper: A0–A4 or custom, and portrait.
    let custom = matches!(size, SheetSize::Custom { .. });
    let custom_label = t!("prop-paper-custom");
    let mut choice: Option<SheetSize> = (!custom).then_some(*size);
    let mut turned = *portrait;
    row(ui, &t!("prop-sheet-size"), |ui| {
        ui.horizontal(|ui| {
            let shown = if custom { custom_label.clone() } else { size.label() };
            egui::ComboBox::from_id_salt(("size", id.0)).selected_text(shown).width(90.0).show_ui(ui, |ui| {
                for s in SheetSize::ALL {
                    ui.selectable_value(&mut choice, Some(s), s.label());
                }
                ui.selectable_value(&mut choice, None, custom_label.clone());
            });
            ui.checkbox(&mut turned, t!("prop-portrait"));
        });
    });
    match choice {
        Some(s) if s != *size => {
            refit |= crate::engine_edit::sheet(state, id, None, |v| {
                if let ViewKind::Sheet { size, .. } = &mut v.kind {
                    *size = s;
                }
            });
        }
        None if !custom => refit |= crate::engine_edit::sheet(state, id, None, make_custom),
        _ => {}
    }
    if turned != *portrait {
        refit |= crate::engine_edit::sheet(state, id, None, |v| {
            if let ViewKind::Sheet { portrait, .. } = &mut v.kind {
                *portrait = turned;
            }
        });
    }
    if let SheetSize::Custom { width, height } = *size {
        let (mut w, mut h) = (width, height);
        let r = row(ui, &t!("prop-paper-dimensions"), |ui| {
            ui.horizontal(|ui| {
                let range = MIN_PAPER_MM..=MAX_PAPER_MM;
                let a = ui.add(DragValue::new(&mut w).range(range.clone()).speed(1.0).suffix(" mm"));
                ui.label("×");
                let b = ui.add(DragValue::new(&mut h).range(range).speed(1.0).suffix(" mm"));
                a | b
            })
            .inner
        });
        if r.changed() {
            refit |= crate::engine_edit::sheet(state, id, Some(key((id, "paper"))), |v| {
                if let ViewKind::Sheet { size, .. } = &mut v.kind {
                    *size = SheetSize::Custom { width: w, height: h };
                }
            });
        }
        finish(state, &r);
    }

    // Issue date: typed as YYYY-MM-DD, or today.
    let mut commit = None;
    row(ui, &t!("prop-issue-date"), |ui| {
        ui.horizontal(|ui| {
            commit = crate::table::commit_text(ui, ("issue-date", id.0), issue_date, 100.0, &t!("prop-date-hint"));
            if ui.button(t!("action-today")).clicked() {
                commit = Some(today_iso());
            }
        });
    });
    if let Some(date) = commit {
        if date.is_empty() || cc_core::title_block::is_iso_date(&date) {
            crate::engine_edit::sheet(state, id, None, |v| {
                if let ViewKind::Sheet { issue_date, .. } = &mut v.kind {
                    *issue_date = date;
                }
            });
        } else {
            state.message = t!("msg-invalid-date");
        }
    }
    if refit && let Some(st) = state.view2d.get_mut(&id) {
        st.fitted = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn today_is_written_as_an_iso_date() {
        let t = today_iso();
        assert!(cc_core::title_block::is_iso_date(&t), "{t}");
    }

    #[test]
    fn an_iso_paper_made_custom_keeps_its_shown_size() {
        let mut v = cc_core::View::new(
            ViewId(1),
            "S",
            100,
            ViewKind::Sheet {
                number: "A1".into(),
                size: SheetSize::A3,
                portrait: true,
                viewports: Vec::new(),
                title_block: None,
                fields: Default::default(),
                issue_date: String::new(),
            },
        );
        make_custom(&mut v);
        assert_eq!(v.paper_mm(), Some(glam::DVec2::new(297.0, 420.0)));
        let ViewKind::Sheet { size, portrait, .. } = &v.kind else { panic!() };
        assert_eq!((*size, *portrait), (SheetSize::Custom { width: 297.0, height: 420.0 }, false));
    }
}
