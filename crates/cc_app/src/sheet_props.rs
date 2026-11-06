//! A sheet's rows in the properties palette: its number, title block,
//! paper and issue date, and the title block's per-sheet fields. Each
//! change runs `sheet_set` (through [`crate::engine_edit::sheet`]).

use crate::state::AppState;
use crate::t;
use crate::table::{row, section};
use cc_core::{ElementTypeId, MAX_PAPER_MM, MIN_PAPER_MM, SheetSize, TypeData, View, ViewId, ViewKind};
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

/// The title block's per-sheet fields: its text parameters that sheets may set.
fn per_sheet_params(state: &AppState, id: ViewId) -> Vec<cc_core::ComponentParam> {
    let p = state.project();
    let Some(ViewKind::Sheet { title_block: Some(tb), .. }) = p.view(id).map(|v| &v.kind) else { return Vec::new() };
    match p.element_type(*tb).map(|t| &t.data) {
        Some(TypeData::Component { def, .. }) => {
            def.params.iter().filter(|q| q.kind.is_text() && q.instance).cloned().collect()
        }
        _ => Vec::new(),
    }
}

/// One row per per-sheet field: empty shows (and uses) the type's text.
pub fn fields_section(ui: &mut Ui, state: &mut AppState, id: ViewId) {
    let params = per_sheet_params(state, id);
    if params.is_empty() {
        return;
    }
    let Some(ViewKind::Sheet { fields, .. }) = state.project().view(id).map(|v| v.kind.clone()) else { return };
    section(ui, "sheet-fields", t!("section-sheet-fields"), |ui| {
        for q in &params {
            let mut text = fields.get(&q.name).cloned().unwrap_or_default();
            let r = row(ui, q.display(), |ui| {
                crate::table::live_text(ui, ("sheet-field", id.0, &q.name), &mut text, |ui, t| {
                    ui.add(egui::TextEdit::singleline(t).hint_text(&q.text))
                })
            });
            if r.changed() {
                let name = q.name.clone();
                crate::engine_edit::sheet(state, id, Some(key((id, "field", &q.name))), |v| {
                    if let ViewKind::Sheet { fields, .. } = &mut v.kind {
                        if text.is_empty() {
                            fields.remove(&name);
                        } else {
                            fields.insert(name, text);
                        }
                    }
                });
            }
            finish(state, &r);
        }
    });
}

/// The first piece of the active sheet's title block that does not draw,
/// as a status bar message.
pub fn title_block_error(state: &AppState) -> Option<String> {
    let id = crate::actions::active_sheet(state)?;
    let p = state.project();
    let view = p.view(id)?;
    let ViewKind::Sheet { title_block: Some(tb), .. } = &view.kind else { return None };
    let TypeData::Component { def, .. } = &p.element_type(*tb)?.data else { return None };
    let (index, error) = def.resolve_sheet(view.paper_mm()?).errors.into_iter().next()?;
    let piece = def.sheet.get(index).map_or_else(|| (index + 1).to_string(), |s| s.name.clone());
    Some(t!("msg-title-block-error", "piece" => piece, "error" => error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_engine::Ref;
    use cc_engine::commands::component::ComponentParamAdd;
    use cc_engine::commands::component_piece::ExprArg;
    use cc_engine::commands::sheets::{ComponentSheetPieceAdd, SheetPieceArg, TitleBlockCreate};

    fn state_with_block() -> (AppState, ViewId, cc_core::ElementTypeId) {
        let mut state = AppState::new(cc_core::Document::new(crate::actions::new_project()));
        let sheet = state.project().sheets_in_order()[0];
        state.active_view = Some(sheet);
        let out = state.session.run(TitleBlockCreate { name: "TB".into(), empty: true }).unwrap();
        let tb = cc_core::ElementTypeId(out["id"].as_u64().unwrap());
        (state, sheet, tb)
    }

    #[test]
    fn per_sheet_fields_are_the_instance_text_parameters() {
        let (mut state, sheet, tb) = state_with_block();
        let add = |name: &str, instance| ComponentParamAdd {
            type_ref: Ref::Id(tb.0),
            name: name.into(),
            value: None,
            kind: Some("text".into()),
            instance: Some(instance),
            text: Some("Equipe".into()),
        };
        state.session.run(add("Drawn", true)).unwrap();
        state.session.run(add("Firm", false)).unwrap();
        crate::engine_edit::sheet(&mut state, sheet, None, |v| {
            if let ViewKind::Sheet { title_block, .. } = &mut v.kind {
                *title_block = Some(tb);
            }
        });
        let names: Vec<String> = per_sheet_params(&state, sheet).into_iter().map(|q| q.name).collect();
        assert_eq!(names, ["Drawn"]);
    }

    #[test]
    fn a_piece_that_does_not_draw_shows_in_the_status_bar() {
        let (mut state, sheet, tb) = state_with_block();
        crate::engine_edit::sheet(&mut state, sheet, None, |v| {
            if let ViewKind::Sheet { title_block, .. } = &mut v.kind {
                *title_block = Some(tb);
            }
        });
        assert_eq!(title_block_error(&state), None);
        let piece = SheetPieceArg::Text {
            x: ExprArg::Text("Nowhere".into()),
            y: ExprArg::Number(10.0),
            text: "x".into(),
            size: None,
            anchor: None,
            rotation: None,
        };
        state
            .session
            .run(ComponentSheetPieceAdd { type_ref: Ref::Id(tb.0), piece, name: Some("Lost".into()) })
            .unwrap();
        let message = title_block_error(&state).expect("an error");
        assert!(message.contains("Lost"), "{message}");
        // Only for the active sheet.
        state.active_view = None;
        assert_eq!(title_block_error(&state), None);
    }

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
