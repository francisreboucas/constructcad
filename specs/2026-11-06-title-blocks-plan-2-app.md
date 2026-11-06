# Title Blocks — Deliveries 2, 3 and 4 (app and docs) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Users set a sheet's title block, paper, issue date and per-sheet
values in the properties palette, keep their own project fields in Project
Information, and draw title blocks themselves in a *Sheet* view of the
component environment: lines and arcs, texts with `{fields}`, pictures and
reference planes, every change an engine command. The interface and
automation guides describe it all.

**Architecture:**
- **Delivery 2 (sheet side).** `engine_edit::sheet` turns a change on a copy
  of a sheet into one `sheet_set`. A new module `sheet_props.rs` draws the
  sheet's rows (number, title block, paper, issue date) and its per-sheet
  field rows. `dialogs::project_info` gains a *Project fields* table saved
  with `project_info_set` (through the existing `engine_edit::info`). The
  status bar shows the first piece of the active sheet's title block that
  does not draw.
- **Delivery 3 (editor).** The component environment hosts a title block in
  a new module `title_block_env.rs`, decided as follows. The existing
  environment is built around `cc_views::component_view` (plan, front, side,
  3D of 3D pieces, in meters) and an `egui_dock` of `ComponentViewKind`s; a
  2D paper view does not fit `ComponentViewKind` without touching cc_views'
  component views, IFC-free 3D code and every `match` over view kinds. So
  the environment keeps its `Env` (type, pick, tool, undo start, Finish and
  Cancel, parameters panel) and gains `Env::sheet: Option<SheetEditor>`;
  when it is set, `component_env::{central, ribbon, options, panel}` hand
  over to `title_block_env`, which paints one *Sheet* canvas of its own and
  reuses `sketch_mode::Session` (new `Target::Sheet`) through
  `sketch_mode::interact`, the way `profile_sketch.rs` already does for
  sweep profiles. The canvas works in **paper meters** (millimeters /
  1000): the sketch's millimeter snapping, its pick tolerances and typed
  lengths then measure the printed paper; pieces are written in
  millimeters (×1000) by the component sketch's existing `Writer`.
  The four paper edges are **virtual, locked planes** (not stored in the
  definition): a point on one is written `0`, `SheetWidth` or
  `SheetHeight`. User reference planes are the component's `ref_planes`
  (axis `x` vertical, `y` horizontal, positions in millimeters, which may
  read `SheetWidth`/`SheetHeight`).
- **Delivery 4 (docs).** `docs/interface.md`, `docs/interface.pt-BR.md`,
  `docs/automation.md`, `docs/automacao.md`.

**Tech Stack:** Rust 2024 workspace; egui 0.36 / egui_dock 0.21 /
egui_kittest 0.36; resvg 0.45 (no default features) and png 0.18 in cc_app.

**Spec:** `specs/2026-11-06-title-blocks-design.md` (sections "App
(cc_app)", "Robustness", "Testing" → cc_app, "Delivery"). Delivery 1 is
`specs/2026-11-06-title-blocks-plan-1-core.md`; this plan starts after it
merges.

## Global Constraints

- Non-test code never panics: no `unwrap`, `expect`, `panic!`,
  `unreachable!`, `todo!`, `unimplemented!` (clippy denies them; tests may).
  Use `get()` for indices that come from the model or a selection.
- No `unsafe`.
- The app changes the model **only** through engine commands:
  `crate::actions::run_command`, `state.session.run` / `run_merged`, or the
  `crate::engine_edit::*` helpers. No model logic in cc_app.
- Ribbon, palette and shortcut entries are `commands::UiCommand`s: add each
  new one to `UiCommand::all()`, `icon()`, `label()`, `description()`,
  `engine()`, `unavailable()`, `selected()` (when it is a tool) and `run()`.
  `engine_commands_exist` checks the engine names.
- Modals use `dialog::Dialog`; property grids use `table.rs`
  (`section`, `row`, `readonly_row`, `live_text`).
- Colors from `theme::PALETTE`; the paper stays white in both themes.
- No hard-coded interface text: every new key goes into **all 12**
  `assets/i18n/*.ftl` files (pt-BR is the reference). Append new keys at the
  end of each file under a comment line `# Title blocks (app)`. The
  `all_keys_translated` test scans `t!("…")` literals in `crates/cc_app/src`
  and the engine; the placeholder test requires the same `{ $name }` in
  every language. Do not put literal `{` `}` in values.
- Formatting `cargo fmt --all` (max_width 120); lint
  `cargo clippy --workspace --all-targets -- -D warnings`; tests
  `cargo test --workspace`. Each task ends green on all three.
- Code, comments and commit messages in English; commits signed off
  (`git commit -s`).
- Worktree `D:\constructcad-titleblocks`, branch `feat/title-blocks`
  (other agents may commit there: pull/rebase before each task, and never
  reset or force-push). One PR per delivery.
- Units: title block coordinates, sizes and pen weights are **paper
  millimeters**. The title block canvas is in **paper meters**; convert
  with `MM = 1000.0`.

## Engine interfaces this plan uses (from delivery 1; verify names before each task)

`cc_engine::commands::sheets::{SheetSet { sheet: Ref, number, size: Option<SizeArg>, portrait, title_block: Option<Option<Ref>>, fields: Option<Map<String, Value>>, issue_date }, SizeArg::{Iso(String), Custom { width, height }}, SheetFields, TitleBlockCreate { name, empty }, SheetPieceArg::{Stroke { points, closed, weight, color, fill }, Text { x, y, text, size, anchor, rotation }, Image { x, y, width, height, image_path, image }}, ComponentSheetPieceAdd { type_ref, piece, name }, ComponentSheetPieceSet { type_ref, index, piece: Option<Value>, name }, ComponentSheetPieceDelete { type_ref, index }}`;
`cc_engine::commands::component::{ComponentParamAdd { type_ref, name, value, kind, instance, text }, ComponentParamSet, ComponentParamDelete, ComponentRefPlaneAdd, ComponentRefPlaneSet, ComponentRefPlaneDelete}`;
`cc_engine::commands::component_script::ComponentParamsSet { type_ref: Option<Ref>, elements, name, value }`;
`cc_engine::commands::project::ProjectInfoSet { …, fields }`;
`cc_core::title_block::{SheetShape, SheetText, SheetPieces, TextAnchor, SHEET_WIDTH, SHEET_HEIGHT, is_iso_date, MAX_SHEET_TEXT}`;
`cc_core::view::{MIN_PAPER_MM, MAX_PAPER_MM}`; `View::paper_mm()`; `Project::title_blocks()`;
`cc_views::sheet_fields::{KEYS, value, expand, FieldCtx}`; `cc_engine::i18n::{today, format_date}` (re-exported as `crate::i18n::*`).

## File Structure

| File | Responsibility | Delivery |
|---|---|---|
| `crates/cc_app/src/engine_edit.rs` | `sheet()` → `sheet_set` | 2 |
| `crates/cc_app/src/table.rs` | `commit_text()` (one-line field committed on Enter / focus loss) | 2 |
| `crates/cc_app/src/sheet_props.rs` (new) | sheet rows, per-sheet field rows, `title_block_error()` | 2 |
| `crates/cc_app/src/properties.rs` | sheet section calls `sheet_props` | 2 |
| `crates/cc_app/src/ribbon.rs` | status bar error; Modify/View buttons (3) | 2, 3 |
| `crates/cc_app/src/dialogs.rs` | Project fields table | 2 |
| `crates/cc_core/src/component_ref.rs` | `SheetWidth`/`SheetHeight` reserved and readable by planes | 3 |
| `crates/cc_views/src/sheet_fields.rs`, `sheet.rs`, `lib.rs` | `expand_with`, public `title_block_drawing` | 3 |
| `crates/cc_app/src/title_block_env.rs` (new) | the Sheet canvas, preview, planes, tools, palette | 3 |
| `crates/cc_app/src/component_env.rs` | `Env::sheet`, `Pick::Sheet`, `EnvTool::{SheetText, SheetImage}`, hand-over | 3 |
| `crates/cc_app/src/component_sketch.rs` | `written()` takes scope extras, `pub(crate)` | 3 |
| `crates/cc_app/src/sketch_mode.rs` | `Target::Sheet` | 3 |
| `crates/cc_app/src/commands.rs`, `icons.rs`, `assets/icons/ui/title-block.svg` | new UiCommands, icon | 3 |
| `crates/cc_app/src/browser.rs`, `sheet.rs`, `actions.rs`, `component_editor.rs` | context menus, library opening, kind filter | 3 |
| `crates/cc_app/src/ui_tests.rs`, `crates/cc_app/tests/snapshots/*.png` | UI tests, snapshots | 2, 3 |
| `assets/i18n/*.ftl` | new keys (12 files) | 2, 3 |
| `docs/interface.md`, `docs/interface.pt-BR.md`, `docs/automation.md`, `docs/automacao.md` | user docs | 4 |

---

# Delivery 2 — App, sheet side

### Task 1: `engine_edit::sheet`

**Files:**
- Modify: `crates/cc_app/src/engine_edit.rs` (imports at line 7-9; new fn after `view`, which ends at line 124)
- Test: `crates/cc_app/src/engine_edit.rs` (new `#[cfg(test)] mod tests` at the end)

**Interfaces:**
- Produces: `pub fn sheet(state: &mut AppState, id: ViewId, key: Option<u64>, f: impl FnOnce(&mut View)) -> bool`
  — runs one `sheet_set` with only what `f` changed (number, size,
  portrait, title block, fields as a merge patch, issue date); `false` when
  nothing changed or the engine refused (message in the status bar).

- [ ] **Step 1: Write the failing test** (end of `engine_edit.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::{Document, SheetSize, ViewKind};

    fn sheet_state() -> (AppState, ViewId) {
        let state = AppState::new(Document::new(crate::actions::new_project()));
        let id = state.project().views.values().find(|v| matches!(v.kind, ViewKind::Sheet { .. })).map(|v| v.id).unwrap();
        (state, id)
    }

    fn kind(state: &AppState, id: ViewId) -> ViewKind {
        state.project().view(id).unwrap().kind.clone()
    }

    #[test]
    fn sheet_changes_run_one_sheet_set() {
        let (mut state, id) = sheet_state();
        let depth = state.session.doc.undo_depth();
        let changed = sheet(&mut state, id, None, |v| {
            if let ViewKind::Sheet { size, portrait, issue_date, fields, number, .. } = &mut v.kind {
                *size = SheetSize::Custom { width: 1000.0, height: 594.0 };
                *portrait = true;
                *issue_date = "2026-11-06".into();
                *number = "B7".into();
                fields.insert("Drawn".into(), "Ana".into());
            }
        });
        assert!(changed, "{}", state.message);
        assert_eq!(state.session.doc.undo_depth(), depth + 1, "one undo step");
        let ViewKind::Sheet { size, portrait, issue_date, fields, number, title_block, .. } = kind(&state, id) else {
            panic!()
        };
        assert_eq!(size, SheetSize::Custom { width: 1000.0, height: 594.0 });
        assert!(portrait);
        assert_eq!((issue_date.as_str(), number.as_str()), ("2026-11-06", "B7"));
        assert_eq!(fields.get("Drawn").map(String::as_str), Some("Ana"));
        assert!(title_block.is_some(), "the template's sheet has the standard title block");

        // Clearing a field and the title block; ISO sizes go back as their names.
        assert!(sheet(&mut state, id, None, |v| {
            if let ViewKind::Sheet { fields, title_block, size, .. } = &mut v.kind {
                fields.clear();
                *title_block = None;
                *size = SheetSize::A1;
            }
        }));
        let ViewKind::Sheet { fields, title_block, size, .. } = kind(&state, id) else { panic!() };
        assert!(fields.is_empty() && title_block.is_none());
        assert_eq!(size, SheetSize::A1);

        // Nothing changed: no command. A refused value: false, and a message.
        assert!(!sheet(&mut state, id, None, |_| {}));
        state.message.clear();
        assert!(!sheet(&mut state, id, None, |v| {
            if let ViewKind::Sheet { issue_date, .. } = &mut v.kind {
                *issue_date = "31/02/2026".into();
            }
        }));
        assert!(!state.message.is_empty());
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p cc_app engine_edit::tests`
Expected: FAIL to compile (`sheet` not found).

- [ ] **Step 3: Implement** (after `pub fn view`)

Add `sheets` to the `use cc_engine::commands::{…}` list and
`SheetSize, ViewKind` to the `cc_core` import.

```rust
/// Change sheet `id` as `f` changes a copy of it (`sheet_set`): its
/// number, paper, title block, per-sheet field values and issue date, in
/// one undo step per `key`.
pub fn sheet(state: &mut AppState, id: ViewId, key: Option<u64>, f: impl FnOnce(&mut View)) -> bool {
    let Some(old) = state.project().view(id).cloned() else { return false };
    let mut new = old.clone();
    f(&mut new);
    let (
        ViewKind::Sheet { number: n0, size: s0, portrait: p0, title_block: t0, fields: f0, issue_date: d0, .. },
        ViewKind::Sheet { number: n1, size: s1, portrait: p1, title_block: t1, fields: f1, issue_date: d1, .. },
    ) = (&old.kind, &new.kind)
    else {
        return false;
    };
    let mut fields = serde_json::Map::new();
    for (name, value) in f1 {
        if f0.get(name) != Some(value) {
            fields.insert(name.clone(), serde_json::Value::String(value.clone()));
        }
    }
    for name in f0.keys().filter(|n| !f1.contains_key(*n)) {
        fields.insert(name.clone(), serde_json::Value::Null);
    }
    let size = (s1 != s0).then(|| match *s1 {
        SheetSize::Custom { width, height } => sheets::SizeArg::Custom { width, height },
        iso => sheets::SizeArg::Iso(iso.label()),
    });
    let command = sheets::SheetSet {
        sheet: Ref::Id(id.0),
        number: (n1 != n0).then(|| n1.clone()),
        size,
        portrait: (p1 != p0).then_some(*p1),
        title_block: (t1 != t0).then(|| t1.map(|t| Ref::Id(t.0))),
        fields: (!fields.is_empty()).then_some(fields),
        issue_date: (d1 != d0).then(|| d1.clone()),
    };
    let nothing = command.number.is_none()
        && command.size.is_none()
        && command.portrait.is_none()
        && command.title_block.is_none()
        && command.fields.is_none()
        && command.issue_date.is_none();
    !nothing && run(state, key, command)
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cc_app engine_edit::tests` → PASS. Then
`cargo clippy -p cc_app --all-targets -- -D warnings`.

- [ ] **Step 5: Commit**

```bash
git add crates/cc_app/src/engine_edit.rs
git commit -s -m "Change sheets through sheet_set from the app"
```

---

### Task 2: Sheet rows — number, title block, paper, issue date

**Files:**
- Create: `crates/cc_app/src/sheet_props.rs` (add `mod sheet_props;` in `crates/cc_app/src/main.rs` between `mod sheet;` and `mod sketch_mode;`)
- Modify: `crates/cc_app/src/table.rs` (new `commit_text` after `live_text`, line 143-162)
- Modify: `crates/cc_app/src/properties.rs:2602-2629` (the `ViewKind::Sheet { number, size, .. }` arm)
- Modify: `assets/i18n/*.ftl` (12 files)
- Test: `crates/cc_app/src/sheet_props.rs` tests; `crates/cc_app/src/ui_tests.rs`

**Interfaces:**
- Consumes: `engine_edit::sheet` (Task 1).
- Produces: `sheet_props::rows(ui, state, id)`; `table::commit_text(ui, salt, current, width, hint) -> Option<String>`;
  `sheet_props::today_iso() -> String`.

- [ ] **Step 1: Write the failing tests**

In `sheet_props.rs` (tests module at the end of the new file):

```rust
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
```

In `ui_tests.rs` (after `a_view_picked_on_a_sheet_shows_its_properties`):

```rust
/// The sheet's rows run `sheet_set`: Portrait turns the paper, Today sets
/// the issue date, each one undo step.
#[test]
fn sheet_rows_change_the_sheet() {
    let mut state = sample_state();
    let sheet = state
        .project()
        .views
        .values()
        .find(|v| matches!(v.kind, cc_core::ViewKind::Sheet { .. }))
        .map(|v| v.id)
        .expect("the sample has a sheet");
    state.active_view = Some(sheet);
    let mut harness = themed_harness(state, egui::vec2(360.0, 1400.0), egui::Theme::Light, |ui, state| {
        crate::properties::show(ui, state);
    });
    harness.run();
    assert!(harness.query_by_label(&t!("prop-title-block")).is_some());
    let depth = harness.state().session.doc.undo_depth();
    harness.get_by_label(&t!("prop-portrait")).click();
    harness.run();
    harness.get_by_label(&t!("action-today")).click();
    harness.run();
    let state = harness.state();
    assert_eq!(state.session.doc.undo_depth(), depth + 2);
    let Some(cc_core::ViewKind::Sheet { portrait, issue_date, .. }) = state.project().view(sheet).map(|v| &v.kind)
    else {
        panic!("not a sheet")
    };
    assert!(*portrait);
    assert_eq!(*issue_date, crate::sheet_props::today_iso());
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p cc_app sheet_props sheet_rows_change_the_sheet`
Expected: FAIL to compile (module missing).

- [ ] **Step 3: Implement**

`table.rs`, after `live_text`:

```rust
/// A one-line field that hands back its text, trimmed, when Enter is
/// pressed or it loses the focus, and only if it changed. While focused it
/// keeps what is typed; otherwise it shows `current`. `hint` shows when empty.
pub fn commit_text(
    ui: &mut Ui,
    salt: impl std::hash::Hash,
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
```

`sheet_props.rs`:

```rust
//! A sheet's rows in the properties palette: its number, title block,
//! paper and issue date, and the title block's per-sheet fields. Each
//! change runs `sheet_set` (through [`crate::engine_edit::sheet`]).

use crate::state::AppState;
use crate::t;
use crate::table::{row, section};
use cc_core::view::{MAX_PAPER_MM, MIN_PAPER_MM};
use cc_core::{ElementTypeId, SheetSize, TypeData, View, ViewId, ViewKind};
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
    let blocks: Vec<(ElementTypeId, String)> = p
        .title_blocks()
        .into_iter()
        .filter_map(|t| p.element_type(t).map(|ty| (t, ty.name.clone())))
        .collect();
    let none = t!("prop-title-block-none");
    let shown = title_block
        .and_then(|t| blocks.iter().find(|(b, _)| *b == t))
        .map_or_else(|| none.clone(), |(_, n)| n.clone());
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
```

`TypeData` and `section` are used by Task 3; leave them out of the
imports in this task if clippy reports them unused.

`properties.rs:2602-2629`: replace the whole arm body with

```rust
        ViewKind::Sheet { .. } => {
            section(ui, "sheet", t!("section-graphics"), |ui| crate::sheet_props::rows(ui, state, id));
        }
```

and remove `SheetSize` from the `cc_core` import at line 13 if it becomes
unused.

i18n — add to all 12 files:

| key | pt-BR | en | es | fr | de | it | pl | uk-UA | ja | zh-CN | zh-TW | lo |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `prop-title-block` | Carimbo | Title Block | Rótulo | Cartouche | Schriftfeld | Cartiglio | Tabliczka | Штамп | 表題欄 | 图签 | 圖框標題欄 | ປ້າຍຊື່ |
| `prop-title-block-none` | Nenhum | None | Ninguno | Aucun | Keines | Nessuno | Brak | Немає | なし | 无 | 無 | ບໍ່ມີ |
| `prop-paper-custom` | Personalizado | Custom | Personalizado | Personnalisé | Benutzerdefiniert | Personalizzato | Niestandardowy | Власний | カスタム | 自定义 | 自訂 | ກຳນົດເອງ |
| `prop-portrait` | Retrato | Portrait | Vertical | Portrait | Hochformat | Verticale | Pionowo | Книжкова | 縦向き | 纵向 | 直向 | ແນວຕັ້ງ |
| `prop-paper-dimensions` | Largura × altura | Width × height | Ancho × alto | Largeur × hauteur | Breite × Höhe | Larghezza × altezza | Szerokość × wysokość | Ширина × висота | 幅 × 高さ | 宽 × 高 | 寬 × 高 | ກວ້າງ × ສູງ |
| `prop-issue-date` | Data de emissão | Issue Date | Fecha de emisión | Date d'émission | Ausgabedatum | Data di emissione | Data wydania | Дата випуску | 発行日 | 发布日期 | 發行日期 | ວັນທີອອກ |
| `prop-date-hint` | AAAA-MM-DD | YYYY-MM-DD | AAAA-MM-DD | AAAA-MM-JJ | JJJJ-MM-TT | AAAA-MM-GG | RRRR-MM-DD | РРРР-ММ-ДД | YYYY-MM-DD | YYYY-MM-DD | YYYY-MM-DD | YYYY-MM-DD |
| `action-today` | Hoje | Today | Hoy | Aujourd'hui | Heute | Oggi | Dziś | Сьогодні | 今日 | 今天 | 今天 | ມື້ນີ້ |
| `msg-invalid-date` | Digite a data como AAAA-MM-DD, ou deixe vazio | Type the date as YYYY-MM-DD, or leave it empty | Escriba la fecha como AAAA-MM-DD, o déjela vacía | Saisissez la date au format AAAA-MM-JJ, ou laissez vide | Datum als JJJJ-MM-TT eingeben oder leer lassen | Scrivi la data come AAAA-MM-GG, o lasciala vuota | Wpisz datę jako RRRR-MM-DD lub zostaw puste | Введіть дату як РРРР-ММ-ДД або залиште порожнім | 日付は YYYY-MM-DD で入力するか、空にしてください | 请按 YYYY-MM-DD 输入日期，或留空 | 請以 YYYY-MM-DD 輸入日期，或留空 | ພິມວັນທີເປັນ YYYY-MM-DD ຫຼື ປ່ອຍວ່າງ |

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cc_app sheet_props sheet_rows_change_the_sheet i18n` → PASS.
Then `cargo clippy --workspace --all-targets -- -D warnings`.

- [ ] **Step 5: Commit**

```bash
git add crates/cc_app/src assets/i18n
git commit -s -m "Set a sheet's title block, paper and issue date in its properties"
```

---

### Task 3: Per-sheet field rows and the title block error in the status bar

**Files:**
- Modify: `crates/cc_app/src/sheet_props.rs` (`fields_section`, `title_block_error`)
- Modify: `crates/cc_app/src/properties.rs` (the second `match &view.kind` in `view_sections`, line 2640-2645)
- Modify: `crates/cc_app/src/ribbon.rs:1278` (`status_bar`, after the message label)
- Modify: `assets/i18n/*.ftl`
- Test: `crates/cc_app/src/sheet_props.rs`

**Interfaces:**
- Produces: `sheet_props::fields_section(ui, state, id)`; `sheet_props::title_block_error(state: &AppState) -> Option<String>`.

- [ ] **Step 1: Write the failing tests** (in `sheet_props.rs` tests)

```rust
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
```

Check the exact name of `sheets_in_order` (`cc_core::Project::sheets_in_order`) and that `ComponentSheetPieceAdd` accepts an expression naming an unknown variable (it checks syntax only); if it refuses it, use `"Nowhere + 1"`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p cc_app sheet_props`
Expected: FAIL to compile (`per_sheet_params`, `title_block_error`).

- [ ] **Step 3: Implement** (in `sheet_props.rs`; add `section` and `TypeData` to the imports)

```rust
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
```

`properties.rs`, in `view_sections`' second match, change the sheet arm to:

```rust
        ViewKind::Sheet { viewports, .. } => {
            crate::sheet_props::fields_section(ui, state, id);
            section(ui, "sheet-views", t!("prop-sheet-views"), |ui| sheet_views(ui, state, id, viewports));
        }
```

`ribbon.rs` `status_bar`, right after `ui.label(theme::muted(&state.message));`
(cached per active view and model revision, so it does not resolve every frame):

```rust
        let rev = (state.active_view, state.session.doc.revision());
        let cache = egui::Id::new("title-block-error");
        let error = match ui.data(|d| d.get_temp::<((Option<cc_core::ViewId>, u64), Option<String>)>(cache)) {
            Some((r, e)) if r == rev => e,
            _ => {
                let e = crate::sheet_props::title_block_error(state);
                ui.data_mut(|d| d.insert_temp(cache, (rev, e.clone())));
                e
            }
        };
        if let Some(e) = error {
            ui.separator();
            ui.label(RichText::new(e).color(PALETTE.warning));
        }
```

i18n:

| key | pt-BR | en | es | fr | de | it | pl | uk-UA | ja | zh-CN | zh-TW | lo |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `section-sheet-fields` | Campos da folha | Sheet Fields | Campos de la hoja | Champs de la feuille | Blattfelder | Campi della tavola | Pola arkusza | Поля аркуша | シートの項目 | 图纸字段 | 圖紙欄位 | ຊ່ອງຂໍ້ມູນແຜ່ນ |
| `msg-title-block-error` | Carimbo, peça { $piece }: { $error } | Title block, piece { $piece }: { $error } | Rótulo, pieza { $piece }: { $error } | Cartouche, élément { $piece } : { $error } | Schriftfeld, Teil { $piece }: { $error } | Cartiglio, pezzo { $piece }: { $error } | Tabliczka, element { $piece }: { $error } | Штамп, елемент { $piece }: { $error } | 表題欄、要素 { $piece }: { $error } | 图签，元素 { $piece }：{ $error } | 圖框標題欄，元素 { $piece }：{ $error } | ປ້າຍຊື່, ຊິ້ນ { $piece }: { $error } |

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cc_app sheet_props i18n` → PASS; clippy.

- [ ] **Step 5: Commit**

```bash
git add crates/cc_app/src assets/i18n
git commit -s -m "Fill per-sheet title block fields in the sheet's properties"
```

---

### Task 4: Project fields in Project Information

**Files:**
- Modify: `crates/cc_app/src/dialogs.rs:72-120` (`project_info`; new helpers below it)
- Modify: `assets/i18n/*.ftl`
- Test: `crates/cc_app/src/dialogs.rs` (new tests module) and `crates/cc_app/src/ui_tests.rs`

**Interfaces:**
- Consumes: `engine_edit::info` (already turns `ProjectInfo::fields` changes into `project_info_set` `fields`), `table::commit_text` (Task 2).
- Produces: `dialogs::add_project_field(state) -> bool`, `dialogs::rename_project_field(state, old, new) -> bool`, `dialogs::remove_project_field(state, name) -> bool`.

- [ ] **Step 1: Write the failing tests**

End of `dialogs.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_fields_are_added_renamed_and_removed() {
        let mut state = AppState::new(cc_core::Document::new(crate::actions::new_project()));
        assert!(add_project_field(&mut state));
        assert!(add_project_field(&mut state));
        let names: Vec<String> = state.project().info.fields.iter().map(|f| f.name.clone()).collect();
        let base = t!("info-new-field");
        assert_eq!(names, [format!("{base} 1"), format!("{base} 2")]);
        assert!(rename_project_field(&mut state, &names[0], "Engineer"));
        assert_eq!(state.project().info.field("Engineer"), Some(""));
        // Empty and taken names are refused.
        assert!(!rename_project_field(&mut state, "Engineer", " "));
        assert!(!rename_project_field(&mut state, "Engineer", &names[1]));
        assert_eq!(state.message, t!("msg-field-name-taken"));
        assert!(remove_project_field(&mut state, "Engineer"));
        assert_eq!(state.project().info.fields.len(), 1);
    }
}
```

`ui_tests.rs` (after `preferences_cancel_restores_and_ok_keeps`):

```rust
/// Project Information adds a project field; Cancel undoes it, OK keeps it.
#[test]
fn project_fields_are_kept_by_ok_and_undone_by_cancel() {
    for (button, kept) in [(t!("action-cancel"), false), (t!("action-ok"), true)] {
        let mut state = sample_state();
        state.dialogs.project_info = true;
        let mut harness = app_harness(state);
        harness.get_by_label(&format!("+ {}", t!("info-add-field"))).click();
        harness.run();
        assert_eq!(harness.state().project().info.fields.len(), 1);
        harness.get_by_label(&button).click();
        harness.run();
        assert!(!harness.state().dialogs.project_info);
        assert_eq!(harness.state().project().info.fields.len(), usize::from(kept), "{button}");
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p cc_app project_field`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

Helpers (after `project_info`):

```rust
/// Add a project field with an unused name (`Field 1`, `Field 2`…) and no value.
pub(crate) fn add_project_field(state: &mut AppState) -> bool {
    let base = t!("info-new-field");
    let info = &state.project().info;
    let Some(name) = (1..=10_000).map(|i| format!("{base} {i}")).find(|n| info.field(n).is_none()) else {
        return false;
    };
    crate::engine_edit::info(state, None, |i| i.fields.push(cc_core::ProjectField { name, value: String::new() }))
}

/// Rename project field `old` (refused when the new name is empty or taken).
pub(crate) fn rename_project_field(state: &mut AppState, old: &str, new: &str) -> bool {
    let new = new.trim().to_string();
    if new.is_empty() || state.project().info.field(&new).is_some() {
        state.message = t!("msg-field-name-taken");
        return false;
    }
    crate::engine_edit::info(state, None, |i| {
        if let Some(f) = i.fields.iter_mut().find(|f| f.name == old) {
            f.name = new;
        }
    })
}

/// Remove project field `name`.
pub(crate) fn remove_project_field(state: &mut AppState, name: &str) -> bool {
    crate::engine_edit::info(state, None, |i| i.fields.retain(|f| f.name != name))
}

/// The project fields table: name (committed on Enter), value (as typed), ×.
fn project_fields(ui: &mut Ui, state: &mut AppState) {
    ui.add_space(12.0);
    ui.label(theme::title(t!("info-project-fields")));
    ui.add_space(4.0);
    let fields = state.project().info.fields.clone();
    egui::Grid::new("info-fields").num_columns(3).spacing([10.0, 6.0]).show(ui, |ui| {
        form_label(ui, &t!("prop-name"));
        form_label(ui, &t!("prop-value"));
        ui.end_row();
        for f in &fields {
            if let Some(new) = crate::table::commit_text(ui, ("info-field-name", &f.name), &f.name, 140.0, "") {
                rename_project_field(state, &f.name, &new);
            }
            let mut value = f.value.clone();
            let r = crate::table::live_text(ui, ("info-field-value", &f.name), &mut value, |ui, t| {
                ui.add(egui::TextEdit::singleline(t).desired_width(220.0))
            });
            if r.changed() {
                let name = f.name.clone();
                crate::engine_edit::info(state, Some(key(("info-field", &f.name))), |i| {
                    if let Some(x) = i.fields.iter_mut().find(|x| x.name == name) {
                        x.value = value;
                    }
                });
            }
            done(state, &r);
            if ui.small_button("×").on_hover_text(t!("action-delete")).clicked() {
                remove_project_field(state, &f.name);
            }
            ui.end_row();
        }
    });
    if ui.button(format!("+ {}", t!("info-add-field"))).clicked() {
        add_project_field(state);
    }
}
```

In `project_info`, call `project_fields(ui, state);` right after the
`egui::Grid::new("info")…show(…);` statement inside the body closure. A
rename removes the old name and adds the new one in one `project_info_set`,
so the renamed field moves to the end of the list.

i18n:

| key | pt-BR | en | es | fr | de | it | pl | uk-UA | ja | zh-CN | zh-TW | lo |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `info-project-fields` | Campos do projeto | Project fields | Campos del proyecto | Champs du projet | Projektfelder | Campi del progetto | Pola projektu | Поля проєкту | プロジェクトの項目 | 项目字段 | 專案欄位 | ຊ່ອງຂໍ້ມູນໂຄງການ |
| `info-add-field` | Adicionar campo | Add field | Añadir campo | Ajouter un champ | Feld hinzufügen | Aggiungi campo | Dodaj pole | Додати поле | 項目を追加 | 添加字段 | 新增欄位 | ເພີ່ມຊ່ອງ |
| `info-new-field` | Campo | Field | Campo | Champ | Feld | Campo | Pole | Поле | 項目 | 字段 | 欄位 | ຊ່ອງ |
| `msg-field-name-taken` | Já existe um campo com esse nome, ou o nome está vazio | A field already has that name, or the name is empty | Ya hay un campo con ese nombre, o el nombre está vacío | Un champ porte déjà ce nom, ou le nom est vide | Ein Feld hat diesen Namen schon, oder der Name ist leer | Esiste già un campo con questo nome, o il nome è vuoto | Pole o tej nazwie już istnieje lub nazwa jest pusta | Поле з такою назвою вже є, або назва порожня | その名前の項目がすでにあるか、名前が空です | 已有同名字段，或名称为空 | 已有同名欄位，或名稱為空 | ມີຊ່ອງຊື່ນີ້ແລ້ວ ຫຼື ຊື່ວ່າງ |

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cc_app project_field i18n` → PASS; clippy.

- [ ] **Step 5: Commit**

```bash
git add crates/cc_app/src assets/i18n
git commit -s -m "Edit project fields in Project Information"
```

---

### Task 5: Delivery 2 snapshot, verification and PR

**Files:**
- Modify: `crates/cc_app/src/ui_tests.rs` (new snapshot test); Create: `crates/cc_app/tests/snapshots/sheet_properties.png`

- [ ] **Step 1: Add the snapshot test** (only on request, like the others)

```rust
#[test]
fn sheet_properties_snapshot() {
    if !snapshots_requested() {
        return;
    }
    let mut state = sample_state();
    let sheet = state.project().sheets_in_order()[0];
    state.active_view = Some(sheet);
    let mut harness = themed_harness(state, egui::vec2(360.0, 720.0), egui::Theme::Light, |ui, state| {
        crate::properties::show(ui, state);
    });
    harness.run();
    harness.snapshot("sheet_properties");
}
```

- [ ] **Step 2: Write the image and look at it**

Run: `UPDATE_SNAPSHOTS=1 cargo test -p cc_app sheet_properties_snapshot`, then open
`crates/cc_app/tests/snapshots/sheet_properties.png`: number, title block
combo, paper combo with Portrait, issue date with Today, Views on Sheet.

- [ ] **Step 3: Verify the workspace**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- [ ] **Step 4: Commit and open the PR**

```bash
git add crates/cc_app assets
git commit -s -m "Snapshot the sheet properties"
git push origin feat/title-blocks
gh pr create --base main --title "Title blocks: sheet properties and project fields (2 of 4)" --body-file <summary file>
```

The body links the spec and this plan, lists the rows, the project fields
table, the status bar error and the tests.

---

# Delivery 3 — App, title block editor

### Task 6: Prerequisites in cc_core and cc_views

**Files:**
- Modify: `crates/cc_core/src/component_ref.rs:178-180` (`is_reserved`) and `:300` (`ref_problems` scope)
- Modify: `crates/cc_views/src/sheet_fields.rs:39-74` (`expand` → `expand_with`)
- Modify: `crates/cc_views/src/sheet.rs` (`draw_title_block` → public `title_block_drawing`), `crates/cc_views/src/lib.rs:38`
- Test: `crates/cc_core/src/component_ref.rs` tests, `crates/cc_views/src/sheet_fields.rs` tests, `crates/cc_engine/tests/title_blocks.rs`

**Interfaces:**
- Produces: `cc_views::sheet_fields::expand_with(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String`;
  `cc_views::title_block_drawing(pieces: &SheetPieces, expand: &dyn Fn(&str) -> String) -> Drawing2D`;
  `SheetWidth`/`SheetHeight` reserved names; reference planes may read them.

Why: the editor's user reference planes stand at expressions such as
`SheetWidth - 10`; `validate` (via `ref_problems`) resolves planes without
the paper and would refuse them, and nothing stops a parameter named
`SheetWidth` from hiding the paper's.

- [ ] **Step 1: Write the failing tests**

`component_ref.rs` tests:

```rust
    #[test]
    fn planes_may_read_the_paper_and_its_names_are_reserved() {
        use crate::title_block::{SHEET_HEIGHT, SHEET_WIDTH};
        let def = ComponentDef {
            kind: crate::component::ComponentKind::TitleBlock,
            ref_planes: vec![RefPlane { name: "Margin".into(), axis: PlaneAxis::X, at: "SheetWidth - 10".into() }],
            ..Default::default()
        };
        assert!(def.ref_problems().is_empty(), "{:?}", def.ref_problems());
        assert!(is_reserved(SHEET_WIDTH) && is_reserved(SHEET_HEIGHT));
    }
```

`sheet_fields.rs` tests:

```rust
    #[test]
    fn any_lookup_fills_the_fields() {
        let lookup = |k: &str| (k == "a").then(|| "1".to_string());
        assert_eq!(expand_with("{a}-{b}-{{a}}", &lookup), "1-{b}-{a}");
    }
```

`crates/cc_engine/tests/title_blocks.rs`:

```rust
#[test]
fn title_block_planes_follow_the_paper() {
    let mut s = Session::default();
    let tb = run(&mut s, "title_block_create", json!({"name": "TB", "empty": true}))["id"].as_u64().unwrap();
    run(&mut s, "component_ref_plane_add", json!({"type": tb, "axis": "x", "at": "SheetWidth - 10", "name": "Margin"}));
    assert_eq!(
        fails(&mut s, "component_param_add", json!({"type": tb, "name": "SheetWidth", "kind": "number"})),
        ErrorCode::InvalidArgument
    );
}
```

Use the helpers `run`/`fails` already in that test file; check that the
engine's `check_name` uses `is_reserved` (`grep -n "fn check_name" -A12
crates/cc_engine/src/commands/component.rs`). If it does not, make it.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p cc_core planes_may_read_the_paper; cargo test -p cc_views any_lookup; cargo test -p cc_engine --test title_blocks title_block_planes`
Expected: FAIL.

- [ ] **Step 3: Implement**

`component_ref.rs`:

```rust
/// Whether `name` is one of the reserved names (the origin planes, a door's
/// wall thickness, a title block's paper size).
pub fn is_reserved(name: &str) -> bool {
    ORIGIN_PLANES.iter().any(|(n, _)| *n == name)
        || name == crate::component::WALL_THICKNESS
        || name == crate::title_block::SHEET_WIDTH
        || name == crate::title_block::SHEET_HEIGHT
}
```

and in `ref_problems` (line 300) probe with an A3 sheet too:

```rust
        let extra = [
            (crate::component::WALL_THICKNESS, 0.2),
            (crate::title_block::SHEET_WIDTH, 420.0),
            (crate::title_block::SHEET_HEIGHT, 297.0),
        ];
        let scope = self.scope(&BTreeMap::new(), &extra);
```

`sheet_fields.rs`: rename the body of `expand` into

```rust
/// Replaces each `{key}` of `text` with its value; `{{` and `}}` are literal
/// braces and unknown keys stay as written.
pub fn expand(text: &str, ctx: &FieldCtx) -> String {
    expand_with(text, &|key| value(key, ctx))
}

/// [`expand`] with any lookup (the title block editor's sample values).
pub fn expand_with(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    // … the former body of `expand`, with `match value(key, ctx)` → `match lookup(key)` …
}
```

`sheet.rs`: change `fn draw_title_block(d, pieces, ctx: &FieldCtx)` to take
`expand: &dyn Fn(&str) -> String` and use `let text = expand(&t.text);`;
in `compose`: `draw_title_block(&mut d, &def.resolve_sheet(paper), &|t| crate::sheet_fields::expand(t, &ctx));`.
Add:

```rust
/// The drawing of a title block's resolved pieces (paper millimeters),
/// each text passed through `expand` (the editor fills fields or shows them raw).
pub fn title_block_drawing(pieces: &SheetPieces, expand: &dyn Fn(&str) -> String) -> Drawing2D {
    let mut d = Drawing2D::default();
    draw_title_block(&mut d, pieces, expand);
    d
}
```

`lib.rs:38`: `pub use sheet::{MARGIN, PlacedViewport, Sheet, SheetEnv, compose as compose_sheet, title_block_drawing};`

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cc_core -p cc_views -p cc_engine` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/cc_core crates/cc_views crates/cc_engine/tests
git commit -s -m "Let title block planes read the paper; expose title block drawing"
```

---

### Task 7: The title block workspace (Sheet canvas, preview, parameters)

**Files:**
- Create: `crates/cc_app/src/title_block_env.rs` (add `mod title_block_env;` in `main.rs` after `mod thumbnails;`)
- Modify: `crates/cc_app/src/component_env.rs`: `Env` (line 66-77) gains `pub sheet`, `Env::new` (line 81) sets it, `open` (line 100) creates it; `central` (line 340), `ribbon` (line 301), `options` (line 325), `panel` (line 692) hand over; make `component`, `type_ref` (line 152), `draft_field` (line 675) and `types_row` (line 765) `pub(crate)`
- Modify: `assets/i18n/*.ftl`
- Test: `crates/cc_app/src/title_block_env.rs`

**Interfaces:**
- Produces:
  - `pub enum Preview { Sheet(ViewId), Paper { size: SheetSize, portrait: bool } }`
  - `pub struct SheetEditor { pub sheet: Option<ViewId>, pub preview: Preview, pub raw_keys: bool, pub editing: Option<(usize, Vec<ProfilePoint>)>, camera: Option<(DVec2, f64)>, drag_from: Option<DVec2>, grip: Option<(usize, u8)> }` + `SheetEditor::new(sheet)`
  - `pub struct SheetPlane { pub name, pub label, pub vertical, pub at, pub edge }`
  - `pub const MM: f64 = 1000.0`; `pub fn iso_paper(size, portrait) -> DVec2`; `pub fn paper(state, preview) -> DVec2`;
    `pub fn scope_extra(paper) -> [(&'static str, f64); 2]`; `pub fn planes(def, paper) -> Vec<SheetPlane>`;
    `pub fn plane_view(planes, k) -> ComponentView`; `pub fn field_value(state, ed, def, key) -> Option<String>`;
    `pub fn is_open(state) -> bool`; `pub fn central/ribbon/options/panel_content`.

- [ ] **Step 1: Write the failing tests** (end of `title_block_env.rs`)

```rust
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
        let at = |name: &str, vertical: bool| ps.iter().find(|p| p.name == name && p.vertical == vertical).map(|p| p.at);
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
    fn fields_read_the_sheet_or_samples() {
        let (mut state, sheet, _) = editor_state();
        let (_, def) = crate::component_env::component(&state).unwrap();
        let ed = state.component_env.as_ref().and_then(|e| e.sheet.clone()).unwrap();
        let number = match &state.project().view(sheet).unwrap().kind {
            ViewKind::Sheet { number, .. } => number.clone(),
            _ => unreachable!(),
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
}
```

`unreachable!` is allowed in tests (clippy.toml allows panics in tests).

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p cc_app title_block_env`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

`component_env.rs`:

```rust
pub struct Env {
    // … existing fields …
    /// A title block shows on one Sheet view instead ([`crate::title_block_env`]).
    pub sheet: Option<crate::title_block_env::SheetEditor>,
}
```

`Env::new` sets `sheet: None`. In `open`, after building the env:

```rust
    let mut env = Env::new(type_id, state.session.doc.undo_depth());
    let title_block = matches!(
        state.project().element_type(type_id).map(|t| &t.data),
        Some(TypeData::Component { def, .. }) if def.kind == cc_core::ComponentKind::TitleBlock
    );
    if title_block {
        env.sheet = Some(crate::title_block_env::SheetEditor::new(crate::actions::active_sheet(state)));
    }
    state.component_env = Some(env);
```

Hand-over, first statement of each function:

```rust
// central():
    if crate::title_block_env::is_open(state) {
        crate::title_block_env::central(ui, state);
        return;
    }
// ribbon():
    if crate::title_block_env::is_open(state) {
        crate::title_block_env::ribbon(ui, state);
        return;
    }
// options():
    if crate::title_block_env::is_open(state) {
        crate::title_block_env::options(ui, state);
        return;
    }
```

and in `panel`, replace `panel_content(ui, state, &name, &def);` by

```rust
            } else if crate::title_block_env::is_open(state) {
                crate::title_block_env::panel_content(ui, state, &name, &def);
            } else {
                panel_content(ui, state, &name, &def);
            }
```

`title_block_env.rs`:

```rust
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
use cc_core::{ComponentDef, ComponentKind, ElementTypeId, PlaneAxis, SheetSize, TypeData, ViewId, ViewKind};
use cc_views::ComponentView;
use cc_views::component_view::ViewPlane;
use egui::{Color32, Rect, Sense, Stroke, Ui, vec2};
use glam::DVec2;

/// Millimeters per canvas unit (the canvas is in paper meters).
pub const MM: f64 = 1000.0;
/// Pick distance, screen points.
pub const PICK_PX: f64 = 6.0;

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
        Preview::Sheet(id) => state
            .project()
            .view(id)
            .and_then(|v| v.paper_mm())
            .unwrap_or_else(|| iso_paper(SheetSize::A3, false)),
        Preview::Paper { size, portrait } => iso_paper(size, portrait),
    }
}

/// The `paper` field of the preview: `A1`, or `W × H mm`.
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
    if let Some(e) = editor_mut(state) {
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
    paint_drawing(&painter, &drawing, &mm, &PaintOptions { highlight: &none, px_per_mm: mm.scale, mono: None, only: None });

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
    // Tools and the sketch come in later tasks (see Tasks 9 to 13).
    let _ = (&xf, &resp, &planes);
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
    let label_at = if p.vertical { egui::pos2(at.x + 4.0, rect.top() + 4.0) } else { egui::pos2(rect.left() + 4.0, at.y - 4.0) };
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
fn hint(_state: &AppState) -> String {
    t!("tb-hint-view")
}

/// The left panel: name, type, parameters (and the paper's size), and the
/// picked piece or plane.
pub fn panel_content(ui: &mut Ui, state: &mut AppState, name: &str, def: &ComponentDef) {
    ui.add_space(6.0);
    ui.label(theme::title(name.to_string()));
    crate::component_env::types_row(ui, state);
    ui.add_space(6.0);
    crate::table::section(ui, "tb-params", t!("comp-params"), |ui| params(ui, state, def));
}

/// The parameters: numbers (millimeters) and texts; *New per-sheet field*
/// adds a text sheets set; the paper's width and height, read-only.
fn params(ui: &mut Ui, state: &mut AppState, def: &ComponentDef) {
    use cc_engine::commands::component as cmd;
    let t = crate::component_env::type_ref(state);
    for p in &def.params {
        let mut value = p.value;
        let mut instance = p.instance;
        let mut rename = None;
        let mut text = None;
        let mut delete = false;
        let r = ui.horizontal(|ui| {
            rename = crate::component_env::draft_field(ui, state, &format!("param:{}", p.name), &p.name, 100.0);
            let r = if p.kind.is_text() {
                text = crate::component_env::draft_field(ui, state, &format!("param-text:{}", p.name), &p.text, 120.0);
                None
            } else {
                Some(ui.add(egui::DragValue::new(&mut value).speed(0.5)))
            };
            ui.checkbox(&mut instance, "").on_hover_text(t!("comp-param-instance"));
            delete = ui.small_button("×").on_hover_text(t!("action-delete")).clicked();
            r
        });
        if r.inner.as_ref().is_some_and(egui::Response::changed) || instance != p.instance {
            let command = cmd::ComponentParamSet {
                type_ref: t.clone(),
                name: p.name.clone(),
                value: r.inner.as_ref().is_some_and(egui::Response::changed).then_some(value),
                kind: None,
                instance: (instance != p.instance).then_some(instance),
                rename: None,
            };
            crate::actions::run_command(state, command);
        }
        if let Some(text) = text {
            let command = cc_engine::commands::component_script::ComponentParamsSet {
                type_ref: Some(t.clone()),
                elements: Vec::new(),
                name: p.name.clone(),
                value: serde_json::Value::String(text),
            };
            crate::actions::run_command(state, command);
        }
        if let Some(new) = rename {
            let command = cmd::ComponentParamSet {
                type_ref: t.clone(),
                name: p.name.clone(),
                value: None,
                kind: None,
                instance: None,
                rename: Some(new),
            };
            crate::actions::run_command(state, command);
        }
        if delete {
            crate::actions::run_command(state, cmd::ComponentParamDelete { type_ref: t.clone(), name: p.name.clone() });
        }
    }
    let free = |base: &str| (1..=10_000).map(|i| format!("{base}{i}")).find(|n| !def.name_taken(n)).unwrap_or_default();
    ui.horizontal(|ui| {
        if ui.button(format!("+ {}", t!("comp-env-add-param"))).clicked() {
            let command = cmd::ComponentParamAdd {
                type_ref: t.clone(),
                name: free("Param"),
                value: Some(0.0),
                kind: Some("number".into()),
                instance: None,
                text: None,
            };
            crate::actions::run_command(state, command);
        }
        if ui.button(format!("+ {}", t!("tb-new-field"))).clicked() {
            let command = cmd::ComponentParamAdd {
                type_ref: t.clone(),
                name: free("Field"),
                value: None,
                kind: Some("text".into()),
                instance: Some(true),
                text: Some(String::new()),
            };
            crate::actions::run_command(state, command);
        }
    });
    let Some(ed) = editor(state) else { return };
    let paper = paper(state, ed.preview);
    crate::table::readonly_row(ui, &t!("tb-sheet-width"), format!("{} mm", paper.x));
    crate::table::readonly_row(ui, &t!("tb-sheet-height"), format!("{} mm", paper.y));
}
```

Notes for the implementer:
- `ComponentDef::name_taken` exists (used by `component_env::params`).
- The `let _ = (&xf, &resp, &planes);` line keeps the values Tasks 9-13
  use; remove it in Task 9.
- `egui::Shape::dashed_line` returns `Vec<Shape>`; check its signature in
  egui 0.36 (`dashed_line(path, stroke, dash_length, gap_length)`).

i18n:

| key | pt-BR | en | es | fr | de | it | pl | uk-UA | ja | zh-CN | zh-TW | lo |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `tb-preview` | Visualizar em | Preview on | Vista previa en | Aperçu sur | Vorschau auf | Anteprima su | Podgląd na | Перегляд на | プレビュー | 预览 | 預覽 | ສະແດງຕົວຢ່າງ |
| `tb-preview-sheet` | Folha ativa ({ $paper }) | Active sheet ({ $paper }) | Hoja activa ({ $paper }) | Feuille active ({ $paper }) | Aktives Blatt ({ $paper }) | Tavola attiva ({ $paper }) | Aktywny arkusz ({ $paper }) | Активний аркуш ({ $paper }) | アクティブなシート ({ $paper }) | 当前图纸（{ $paper }） | 目前圖紙（{ $paper }） | ແຜ່ນທີ່ໃຊ້ຢູ່ ({ $paper }) |
| `tb-preview-landscape` | { $paper } paisagem | { $paper } landscape | { $paper } horizontal | { $paper } paysage | { $paper } quer | { $paper } orizzontale | { $paper } poziomo | { $paper } альбомна | { $paper } 横 | { $paper } 横向 | { $paper } 橫向 | { $paper } ແນວນອນ |
| `tb-preview-portrait` | { $paper } retrato | { $paper } portrait | { $paper } vertical | { $paper } portrait | { $paper } hoch | { $paper } verticale | { $paper } pionowo | { $paper } книжкова | { $paper } 縦 | { $paper } 纵向 | { $paper } 直向 | { $paper } ແນວຕັ້ງ |
| `tb-show-keys` | Mostrar chaves dos campos | Show field keys | Mostrar claves de campos | Afficher les clés des champs | Feldschlüssel zeigen | Mostra chiavi dei campi | Pokaż klucze pól | Показати ключі полів | 項目キーを表示 | 显示字段键 | 顯示欄位鍵 | ສະແດງລະຫັດຊ່ອງ |
| `tb-sample-sheet` | Folha de exemplo | Sample sheet | Hoja de ejemplo | Feuille d'exemple | Beispielblatt | Tavola di esempio | Arkusz przykładowy | Зразок аркуша | サンプルシート | 示例图纸 | 範例圖紙 | ແຜ່ນຕົວຢ່າງ |
| `tb-edge-left` | Borda esquerda | Left edge | Borde izquierdo | Bord gauche | Linker Rand | Bordo sinistro | Lewa krawędź | Лівий край | 左端 | 左边缘 | 左邊緣 | ຂອບຊ້າຍ |
| `tb-edge-right` | Borda direita | Right edge | Borde derecho | Bord droit | Rechter Rand | Bordo destro | Prawa krawędź | Правий край | 右端 | 右边缘 | 右邊緣 | ຂອບຂວາ |
| `tb-edge-bottom` | Borda inferior | Bottom edge | Borde inferior | Bord inférieur | Unterer Rand | Bordo inferiore | Dolna krawędź | Нижній край | 下端 | 下边缘 | 下邊緣 | ຂອບລຸ່ມ |
| `tb-edge-top` | Borda superior | Top edge | Borde superior | Bord supérieur | Oberer Rand | Bordo superiore | Górna krawędź | Верхній край | 上端 | 上边缘 | 上邊緣 | ຂອບເທິງ |
| `tb-sheet-width` | Largura da folha | Sheet width | Ancho de la hoja | Largeur de la feuille | Blattbreite | Larghezza tavola | Szerokość arkusza | Ширина аркуша | シートの幅 | 图纸宽度 | 圖紙寬度 | ຄວາມກວ້າງແຜ່ນ |
| `tb-sheet-height` | Altura da folha | Sheet height | Alto de la hoja | Hauteur de la feuille | Blatthöhe | Altezza tavola | Wysokość arkusza | Висота аркуша | シートの高さ | 图纸高度 | 圖紙高度 | ຄວາມສູງແຜ່ນ |
| `tb-new-field` | Novo campo por folha | New per-sheet field | Nuevo campo por hoja | Nouveau champ par feuille | Neues Feld je Blatt | Nuovo campo per tavola | Nowe pole arkusza | Нове поле аркуша | シートごとの新しい項目 | 新建按图纸字段 | 新增依圖紙欄位 | ຊ່ອງໃໝ່ຕໍ່ແຜ່ນ |
| `tb-hint-view` | A roda aproxima · arraste com o botão do meio para mover · clique duplo com o botão do meio enquadra a folha | The wheel zooms · drag with the middle button to pan · double-click the middle button to fit the sheet | La rueda acerca · arrastre con el botón central para desplazar · doble clic con el botón central encuadra la hoja | La molette zoome · glissez avec le bouton du milieu pour déplacer · double-clic du milieu pour cadrer la feuille | Das Mausrad zoomt · mit der mittleren Taste ziehen verschiebt · Doppelklick mit der mittleren Taste zeigt das ganze Blatt | La rotella ingrandisce · trascina con il tasto centrale per spostare · doppio clic con il tasto centrale inquadra la tavola | Kółko powiększa · przeciągnij środkowym przyciskiem, aby przesunąć · dwuklik środkowym dopasowuje arkusz | Коліщатко масштабує · тягніть середньою кнопкою для зсуву · подвійне клацання середньою вміщує аркуш | ホイールでズーム · 中ボタンのドラッグで移動 · 中ボタンのダブルクリックでシート全体を表示 | 滚轮缩放 · 按住中键拖动平移 · 双击中键适配图纸 | 滾輪縮放 · 按住中鍵拖曳平移 · 按兩下中鍵符合圖紙 | ລໍ້ເມົາຊູມ · ລາກດ້ວຍປຸ່ມກາງເພື່ອເລື່ອນ · ຄລິກສອງເທື່ອປຸ່ມກາງເພື່ອເບິ່ງທັງແຜ່ນ |

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cc_app title_block_env component_env i18n` → PASS; clippy.

- [ ] **Step 5: Commit**

```bash
git add crates/cc_app/src assets/i18n
git commit -s -m "Show title blocks on a Sheet view in the component environment"
```

---

### Task 8: Opening the editor — Edit Title Block, New Title Block, menus, library

**Files:**
- Create: `assets/icons/ui/title-block.svg`
- Modify: `crates/cc_app/src/icons.rs` (`Icon::TitleBlock` in the enum, `Icon::ALL` 149 → 150, `Icon::source` next to `Icon::Sheet` at line 351)
- Modify: `crates/cc_app/src/commands.rs` (enum after `EditProfile` line 97; `all()` after `EditProfile` line 282; `icon()`; `badge()` line 516; `label()`; `description()`; `engine()` line ~1032; `unavailable()` next to `EditProfile` line 1135; `run()` next to `EditProfile` line 1552)
- Modify: `crates/cc_app/src/ribbon.rs` (`modify_tab` line 739; `view_tab` line 797)
- Modify: `crates/cc_app/src/browser.rs:193-223` (view row context menu), `crates/cc_app/src/sheet.rs` (canvas context menu)
- Modify: `crates/cc_app/src/actions.rs:341-352` (`load_component_path`), `crates/cc_app/src/component_editor.rs:293` (kind combo)
- Modify: `crates/cc_app/src/title_block_env.rs` (`sheet_title_block_of`, `sheet_title_block`, `edit_sheet`, `new_title_block`)
- Modify: `assets/i18n/*.ftl`
- Test: `crates/cc_app/src/title_block_env.rs`

**Interfaces:**
- Produces: `UiCommand::{EditTitleBlock, NewTitleBlock}`; `title_block_env::{sheet_title_block_of(state, ViewId) -> Option<ElementTypeId>, sheet_title_block(state) -> Option<ElementTypeId>, edit_sheet(state, ViewId), new_title_block(state)}`.

- [ ] **Step 1: Write the failing tests** (title_block_env tests)

```rust
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
    fn quick_edit_does_not_offer_title_blocks() {
        assert!(!crate::component_editor::kinds().contains(&ComponentKind::TitleBlock));
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p cc_app title_block_env`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

`assets/icons/ui/title-block.svg`:

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><rect x="2.5" y="4.5" width="19" height="15" rx="1.5"/><rect x="11" y="12.5" width="8" height="4.5" rx=".5" fill="#fff" fill-opacity=".2"/><path d="M11 14.75h8M15 14.75v2.25"/></svg>
```

`icons.rs`: `TitleBlock` in `Icon`, in `Icon::ALL` (size 150), and
`Icon::TitleBlock => include_str!("../../../assets/icons/ui/title-block.svg"),`.

`title_block_env.rs`:

```rust
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

/// *Edit Title Block* on sheet `id`: make it the active view, then open its title block.
pub fn edit_sheet(state: &mut AppState, id: ViewId) {
    let Some(tb) = sheet_title_block_of(state, id) else { return };
    state.open_view(id);
    crate::component_env::open(state, tb);
}

/// *New Title Block*: a copy of the standard layout under an unused name, opened.
pub fn new_title_block(state: &mut AppState) {
    let base = t!("tb-default-name");
    let p = state.project();
    let Some(name) = (1..=10_000).map(|i| format!("{base} {i}")).find(|n| !p.types.values().any(|t| t.name == *n))
    else {
        return;
    };
    let command = cc_engine::commands::sheets::TitleBlockCreate { name, empty: false };
    let Some(id) = crate::actions::run_command(state, command).and_then(|o| o["id"].as_u64()) else { return };
    crate::component_env::open(state, ElementTypeId(id));
}
```

`commands.rs` arms:

```rust
    /// Open the active sheet's title block in the editor.
    EditTitleBlock,
    /// Create a title block from the standard layout and open it.
    NewTitleBlock,
// all(): after EditProfile
            EditTitleBlock,
            NewTitleBlock,
// icon():
            EditTitleBlock | NewTitleBlock => Icon::TitleBlock,
// badge(): add NewTitleBlock to the Badge::New list
// label():
            EditTitleBlock => t!("action-edit-title-block"),
            NewTitleBlock => t!("action-new-title-block"),
// description():
            EditTitleBlock => t!("cmd-desc-edit-title-block"),
            NewTitleBlock => t!("cmd-desc-new-title-block"),
// engine():
            EditTitleBlock => "component_sheet_piece_set",
            NewTitleBlock => "title_block_create",
// unavailable(), next to EditProfile:
            EditTitleBlock if crate::title_block_env::sheet_title_block(state).is_none() => {
                Some(t!("why-sheet-title-block"))
            }
// run():
        EditTitleBlock => {
            if let Some(id) = actions::active_sheet(state) {
                crate::title_block_env::edit_sheet(state, id);
            }
        }
        NewTitleBlock => crate::title_block_env::new_title_block(state),
```

`ribbon.rs`: in `modify_tab`, after the `group-sketch` group:
`group(ui, t!("prop-title-block"), |ui| command_button(ui, state, EditTitleBlock));`
In `view_tab`'s `group-documentation`, after `command_button(ui, state, UiCommand::Sheet);`:
`command_button(ui, state, UiCommand::NewTitleBlock);`

`browser.rs`, in the row's `context_menu` after *Duplicate*:

```rust
            if crate::title_block_env::sheet_title_block_of(state, id).is_some()
                && ui.button(t!("action-edit-title-block")).clicked()
            {
                crate::title_block_env::edit_sheet(state, id);
                ui.close();
            }
```

`sheet.rs` `show`, right after `st.navigate(…)` / `let zooming = …` and before `if zooming { return; }`:

```rust
    resp.context_menu(|ui| {
        let enabled = crate::title_block_env::sheet_title_block_of(state, sheet_id).is_some();
        if ui.add_enabled(enabled, egui::Button::new(crate::t!("action-edit-title-block"))).clicked() {
            crate::title_block_env::edit_sheet(state, sheet_id);
            ui.close();
        }
    });
```

(Place it after the last use of `st`, whose borrow of `state.view2d` must end
first; move `let xf = st.xf(rect);` above it if the borrow checker asks.)

`actions.rs` `load_component_path`, in the `Ok(out)` arm, before
`state.set_tool(…)`:

```rust
                // Title blocks are not placed: they open in the editor.
                if state.project().element_type(type_id).map(|t| t.category()) == Some(cc_core::Category::TitleBlock) {
                    crate::component_env::open(state, type_id);
                    let name = out["component"].as_str().unwrap_or_default().to_string();
                    state.message = t!("msg-component-loaded", "name" => name);
                    return;
                }
```

`component_editor.rs`: add

```rust
/// The kinds *Quick Edit* offers: title blocks are drawn in their own editor.
pub(crate) fn kinds() -> Vec<ComponentKind> {
    ComponentKind::ALL.into_iter().filter(|k| *k != ComponentKind::TitleBlock).collect()
}
```

and iterate `for k in kinds()` at line 293.

i18n:

| key | pt-BR | en | es | fr | de | it | pl | uk-UA | ja | zh-CN | zh-TW | lo |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `action-edit-title-block` | Editar carimbo | Edit Title Block | Editar rótulo | Modifier le cartouche | Schriftfeld bearbeiten | Modifica cartiglio | Edytuj tabliczkę | Редагувати штамп | 表題欄を編集 | 编辑图签 | 編輯圖框標題欄 | ແກ້ໄຂປ້າຍຊື່ |
| `action-new-title-block` | Novo carimbo | New Title Block | Nuevo rótulo | Nouveau cartouche | Neues Schriftfeld | Nuovo cartiglio | Nowa tabliczka | Новий штамп | 新しい表題欄 | 新建图签 | 新增圖框標題欄 | ປ້າຍຊື່ໃໝ່ |
| `cmd-desc-edit-title-block` | Abre o carimbo da folha ativa no editor de carimbos. | Opens the active sheet's title block in the title block editor. | Abre el rótulo de la hoja activa en el editor de rótulos. | Ouvre le cartouche de la feuille active dans l'éditeur de cartouches. | Öffnet das Schriftfeld des aktiven Blatts im Schriftfeld-Editor. | Apre il cartiglio della tavola attiva nell'editor dei cartigli. | Otwiera tabliczkę aktywnego arkusza w edytorze tabliczek. | Відкриває штамп активного аркуша в редакторі штампів. | アクティブなシートの表題欄を表題欄エディターで開きます。 | 在图签编辑器中打开当前图纸的图签。 | 在圖框標題欄編輯器中開啟目前圖紙的圖框標題欄。 | ເປີດປ້າຍຊື່ຂອງແຜ່ນທີ່ໃຊ້ຢູ່ໃນຕົວແກ້ໄຂ. |
| `cmd-desc-new-title-block` | Cria um carimbo a partir do modelo padrão e o abre no editor. | Creates a title block from the standard layout and opens it in the editor. | Crea un rótulo a partir del diseño estándar y lo abre en el editor. | Crée un cartouche à partir de la mise en page standard et l'ouvre dans l'éditeur. | Erstellt ein Schriftfeld aus dem Standardlayout und öffnet es im Editor. | Crea un cartiglio dal modello standard e lo apre nell'editor. | Tworzy tabliczkę ze standardowego układu i otwiera ją w edytorze. | Створює штамп зі стандартного макета й відкриває його в редакторі. | 標準レイアウトから表題欄を作成し、エディターで開きます。 | 按标准布局新建图签并在编辑器中打开。 | 依標準版面建立圖框標題欄並在編輯器中開啟。 | ສ້າງປ້າຍຊື່ຈາກແບບມາດຕະຖານ ແລະ ເປີດໃນຕົວແກ້ໄຂ. |
| `why-sheet-title-block` | Abra uma folha que tenha carimbo | Open a sheet that has a title block | Abra una hoja que tenga rótulo | Ouvrez une feuille qui a un cartouche | Öffnen Sie ein Blatt mit Schriftfeld | Apri una tavola con un cartiglio | Otwórz arkusz z tabliczką | Відкрийте аркуш зі штампом | 表題欄のあるシートを開いてください | 请打开带图签的图纸 | 請開啟有圖框標題欄的圖紙 | ເປີດແຜ່ນທີ່ມີປ້າຍຊື່ |
| `tb-default-name` | Carimbo | Title block | Rótulo | Cartouche | Schriftfeld | Cartiglio | Tabliczka | Штамп | 表題欄 | 图签 | 圖框標題欄 | ປ້າຍຊື່ |

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cc_app` (icons, commands, title_block_env, i18n) → PASS; clippy.

- [ ] **Step 5: Commit**

```bash
git add crates/cc_app assets
git commit -s -m "Open title blocks from sheets, the library and New Title Block"
```

---

### Task 9: Reference planes on the sheet

**Files:**
- Modify: `crates/cc_app/src/title_block_env.rs` (ref plane tool, plane picking and properties, ribbon group)
- Modify: `assets/i18n/*.ftl`
- Test: `crates/cc_app/src/title_block_env.rs`

**Interfaces:**
- Consumes: `EnvTool::RefPlane { first: Option<(ComponentViewKind, DVec2)> }` and `Pick::Plane(String)` (existing), `CompRefPlane` UiCommand (its `run` already sets the tool).
- Produces: `pub fn sheet_plane_from_line(a_mm, b_mm) -> Option<(&'static str, f64)>`; `fn add_sheet_plane(state, a_mm, b_mm)`; `fn plane_props(ui, state, def, name)`.

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn two_clicks_make_a_plane_in_millimeters() {
        assert_eq!(sheet_plane_from_line(DVec2::new(30.04, 0.0), DVec2::new(30.06, 200.0)), Some(("x", 30.1)));
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p cc_app two_clicks_make_a_plane_in_millimeters` → FAIL.

- [ ] **Step 3: Implement**

```rust
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
```

In `central`, replace the placeholder `let _ = …;` with the tool dispatch
(this task adds the `RefPlane` arm and plane picking under `Select`; later
tasks add arms):

```rust
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
                // A component plane within reach of the click (edges are locked).
                let at = p * MM;
                let reach = tol * MM;
                let hit = planes
                    .iter()
                    .filter(|q| !q.edge)
                    .find(|q| (if q.vertical { at.x } else { at.y } - q.at).abs() <= reach)
                    .map(|q| crate::component_env::Pick::Plane(q.name.clone()));
                if let Some(env) = state.component_env.as_mut() {
                    env.pick = hit;
                }
            }
        }
        _ => {}
    }
```

`hint`:

```rust
fn hint(state: &AppState) -> String {
    match state.component_env.as_ref().map(|e| &e.tool) {
        Some(crate::component_env::EnvTool::RefPlane { .. }) => t!("tb-hint-plane"),
        _ => t!("tb-hint-view"),
    }
}
```

`ribbon`: add `crate::ribbon::env_group(ui, state, t!("comp-env-group-datum"), &[CompRefPlane]);` before the component group.

`panel_content`, after the parameters section:

```rust
    if let Some(crate::component_env::Pick::Plane(plane)) = state.component_env.as_ref().and_then(|e| e.pick.clone()) {
        crate::table::section(ui, "tb-plane", t!("comp-env-plane"), |ui| plane_props(ui, state, def, &plane));
    }
```

```rust
/// A reference plane's name, position (an expression in millimeters) and value.
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
        let command = cmd::ComponentRefPlaneSet { type_ref: t.clone(), name: name.to_string(), at, rename: rename.clone() };
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
```

(`delete_pick` already deletes a `Pick::Plane` with `ComponentRefPlaneDelete`.)

i18n:

| key | pt-BR | en | es | fr | de | it | pl | uk-UA | ja | zh-CN | zh-TW | lo |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `tb-hint-plane` | Clique duas vezes ao longo da linha do plano: mais vertical ou mais horizontal define a direção · Esc volta | Click twice along the plane's line: mostly vertical or horizontal sets its direction · Esc steps back | Haga clic dos veces a lo largo de la línea del plano: más vertical u horizontal define su dirección · Esc retrocede | Cliquez deux fois le long de la ligne du plan : plutôt verticale ou horizontale, elle fixe sa direction · Échap revient | Klicken Sie zweimal entlang der Ebenenlinie: eher senkrecht oder waagerecht legt die Richtung fest · Esc geht zurück | Fai clic due volte lungo la linea del piano: più verticale o orizzontale ne fissa la direzione · Esc torna indietro | Kliknij dwa razy wzdłuż linii płaszczyzny: bardziej pionowa lub pozioma ustala kierunek · Esc cofa | Клацніть двічі вздовж лінії площини: переважно вертикальна чи горизонтальна задає напрямок · Esc повертає | 面の線に沿って 2 回クリック: 縦寄りか横寄りかで向きが決まります · Esc で戻る | 沿平面线单击两次：偏竖直或偏水平决定方向 · Esc 返回 | 沿平面線按兩下：偏垂直或偏水平決定方向 · Esc 返回 | ຄລິກສອງເທື່ອຕາມເສັ້ນລະນາບ: ຕັ້ງ ຫຼື ນອນ ກຳນົດທິດທາງ · Esc ຖອຍຄືນ |

- [ ] **Step 4: Run the tests** — `cargo test -p cc_app title_block_env i18n`; clippy.

- [ ] **Step 5: Commit**

```bash
git add crates/cc_app/src assets/i18n
git commit -s -m "Draw reference planes on the title block's sheet"
```

---

### Task 10: Lines — sketching strokes

**Files:**
- Modify: `crates/cc_app/src/sketch_mode.rs` (`Target` line 40-49; `cancel` line 503; `finish` line 517-526 and the `Target::Piece => None` arm at line 569; `pick_walls` line 690; `ribbon` line 1278-1325)
- Modify: `crates/cc_app/src/component_sketch.rs` (`written` line 257: `pub(crate)`, new `extra` parameter; its two callers in `create` pass `&[]`)
- Modify: `crates/cc_app/src/component_env.rs` (`Pick::Sheet(Vec<usize>)`; `delete_pick` arm; `panel_content` match arm `Some(Pick::Sheet(_)) => {}`)
- Modify: `crates/cc_app/src/commands.rs` (`TbLines`; `SketchPickWalls` availability line 1117)
- Modify: `crates/cc_app/src/title_block_env.rs`
- Modify: `assets/i18n/*.ftl`
- Test: `crates/cc_app/src/title_block_env.rs`

**Interfaces:**
- Produces: `Target::Sheet`; `Pick::Sheet(Vec<usize>)`; `UiCommand::TbLines`;
  `title_block_env::{start_lines, edit_stroke, finish_stroke, chains, delete_pieces, is_sheet_sketch}`;
  `component_sketch::written(def, view, curves, closed, old, extra: &[(&str, f64)]) -> Result<Vec<SketchPointArg>, String>` (`pub(crate)`).

- [ ] **Step 1: Write the failing tests**

```rust
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
        assert_eq!(
            state.component_env.as_ref().unwrap().pick,
            Some(crate::component_env::Pick::Sheet(vec![before]))
        );
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
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p cc_app chains_split sketched_lines_become` → FAIL.

- [ ] **Step 3: Implement**

`component_sketch.rs` `written`:

```rust
/// The profile points that draw `curves` (closed, or an open path), or why
/// not; `extra` are names the old points' expressions may read (a title
/// block's paper).
pub(crate) fn written(
    def: &cc_core::ComponentDef,
    view: &ComponentView,
    curves: &[SketchCurve],
    closed: bool,
    old: &[ProfilePoint],
    extra: &[(&str, f64)],
) -> Result<Vec<SketchPointArg>, String> {
    let vertices = profile::from_curves(curves, closed).map_err(|e| crate::sketch_mode::error_text(&e))?;
    let scope = def.scope(&Default::default(), extra);
    // … rest unchanged …
}
```

and pass `&[]` in `create`'s two calls.

`sketch_mode.rs`:

```rust
    /// Lines of a title block on its Sheet view (see [`crate::title_block_env`]).
    Sheet,
```

`cancel`:

```rust
pub fn cancel(state: &mut AppState) {
    let keep_tool = state.sketch.as_ref().is_some_and(|s| s.piece.is_some() || s.target == Target::Sheet);
    state.sketch = None;
    if let Some(ed) = state.component_env.as_mut().and_then(|e| e.sheet.as_mut()) {
        ed.editing = None;
    }
    if !keep_tool {
        state.set_tool(Tool::Select);
    }
}
```

`finish`, in the `allows_openings` match, next to `Target::Piece`:

```rust
        Target::Sheet => {
            crate::title_block_env::finish_stroke(state);
            return;
        }
```

and in the later match: `Target::Piece | Target::Sheet => None,`.
`pick_walls`: `Some(Target::Piece | Target::Profile { .. } | Target::Sheet) | None => return,`.
`ribbon`: add the title arm `(Some(Target::Sheet), _) => t!("sketch-title-sheet"),` before `_ =>`, and
`let sheet = matches!(state.sketch.as_ref().map(|s| s.target), Some(Target::Sheet));`
with `if piece.is_some() || profile || sheet { &[Delete] } else { … }`.

`component_env.rs`:

```rust
    /// 2D pieces of a title block, by their indices in `def.sheet`.
    Sheet(Vec<usize>),
```

`delete_pick`: `Pick::Sheet(list) => { crate::title_block_env::delete_pieces(state, &list); return; }`
(as the first arm, since `delete_pieces` clears the pick itself).
`panel_content` match: `Some(Pick::Sheet(_)) => {}`.

`commands.rs`:

```rust
    /// Sketch lines and arcs of a title block.
    TbLines,
// all(): after NewTitleBlock: TbLines,
// icon(): TbLines => Icon::DrawLine,
// label(): TbLines => t!("tb-tool-lines"),
// description(): TbLines => t!("cmd-desc-tb-lines"),
// engine(): TbLines => "component_sheet_piece_add",
// unavailable():
            TbLines if !crate::title_block_env::is_open(state) => Some(t!("why-title-block-editor")),
// selected(): TbLines => crate::title_block_env::is_sheet_sketch(state),
// run(): TbLines => crate::title_block_env::start_lines(state),
// SketchPickWalls availability (line 1117): add `|| crate::title_block_env::is_sheet_sketch(state)`.
```

`title_block_env.rs` — `ribbon` gets
`crate::ribbon::env_group(ui, state, t!("group-draw"), &[TbLines]);` after Selection. New functions:

```rust
use crate::sketch_mode::{Draw, Session, Target};
use cc_core::sketch::SketchCurve;
use cc_engine::commands::component_piece::{ExprArg, SketchPointArg};
use cc_engine::commands::sheets::{ComponentSheetPieceAdd, ComponentSheetPieceDelete, ComponentSheetPieceSet, SheetPieceArg};

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
    let mut strokes = Vec::new();
    for (n, (chain, closed)) in chains(&session.curves).into_iter().enumerate() {
        let mm: Vec<SketchCurve> = chain.iter().map(|c| scaled(c, MM)).collect();
        let old: &[ProfilePoint] = if n == 0 { old.as_deref().unwrap_or(&[]) } else { &[] };
        match crate::component_sketch::written(&def, &view, &mm, closed, old, &extra) {
            Ok(points) => strokes.push((points, closed)),
            Err(e) => {
                state.message = e;
                return;
            }
        }
    }
    let t = crate::component_env::type_ref(state);
    let mut picked = Vec::new();
    for (n, (points, closed)) in strokes.into_iter().enumerate() {
        let done = match edit.filter(|_| n == 0) {
            Some(index) => {
                let points: Vec<ProfilePoint> = points.iter().map(point_of).collect();
                let patch = serde_json::json!({ "Stroke": { "points": points, "closed": closed } });
                let command = ComponentSheetPieceSet { type_ref: t.clone(), index, piece: Some(patch), name: None };
                crate::actions::run_command(state, command).map(|_| index)
            }
            None => {
                let piece = SheetPieceArg::Stroke { points, closed, weight: None, color: None, fill: None };
                crate::actions::run_command(state, ComponentSheetPieceAdd { type_ref: t.clone(), piece, name: None })
                    .and_then(|o| o["index"].as_u64())
                    .and_then(|i| usize::try_from(i).ok())
            }
        };
        let Some(index) = done else { return };
        picked.push(index);
    }
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

/// Delete pieces `list` (highest index first, so the others keep theirs).
pub fn delete_pieces(state: &mut AppState, list: &[usize]) {
    let t = crate::component_env::type_ref(state);
    let mut list = list.to_vec();
    list.sort_unstable();
    list.dedup();
    for index in list.into_iter().rev() {
        let command = ComponentSheetPieceDelete { type_ref: t.clone(), index };
        if crate::actions::run_command(state, command).is_none() {
            break;
        }
    }
    if let Some(env) = state.component_env.as_mut() {
        env.pick = None;
    }
}
```

The sketch on the canvas — in `central`, right before `let tool = …`:

```rust
    if state.sketch.is_some() {
        let view = plane_view(&planes, 1.0 / MM);
        let Some(mut s) = state.sketch.take() else { return };
        // The sketch's own points first, then the edges and planes, then the millimeter.
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
```

Typed lengths: `sketch_mode::interact` handles them when issue #179's
change (branch `fix/179-sketch-typed-length`, `Session::typed`) is merged;
they are in project length units, i.e. paper meters here, so nothing more
is needed in this module.

i18n:

| key | pt-BR | en | es | fr | de | it | pl | uk-UA | ja | zh-CN | zh-TW | lo |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `tb-tool-lines` | Linhas | Lines | Líneas | Lignes | Linien | Linee | Linie | Лінії | 線 | 线条 | 線條 | ເສັ້ນ |
| `cmd-desc-tb-lines` | Esboça linhas e arcos que viram traços do carimbo; pontos sobre as bordas e os planos de referência os acompanham. | Sketches lines and arcs that become strokes of the title block; points on the edges and reference planes follow them. | Dibuja líneas y arcos que se convierten en trazos del rótulo; los puntos sobre los bordes y planos de referencia los siguen. | Esquisse des lignes et des arcs qui deviennent des traits du cartouche ; les points sur les bords et les plans de référence les suivent. | Skizziert Linien und Bögen, die zu Strichen des Schriftfelds werden; Punkte auf Rändern und Referenzebenen folgen ihnen. | Disegna linee e archi che diventano tratti del cartiglio; i punti sui bordi e sui piani di riferimento li seguono. | Szkicuje linie i łuki, które stają się kreskami tabliczki; punkty na krawędziach i płaszczyznach odniesienia podążają za nimi. | Ескізує лінії та дуги, що стають штрихами штампа; точки на краях і опорних площинах слідують за ними. | 線と円弧をスケッチして表題欄の線にします。縁や参照面上の点はそれに追従します。 | 绘制直线和圆弧作为图签的线条；位于边缘和参照平面上的点随之移动。 | 繪製直線與圓弧作為圖框標題欄的線條；位於邊緣與參考平面上的點會跟著移動。 | ແຕ້ມເສັ້ນ ແລະ ເສັ້ນໂຄ້ງເປັນເສັ້ນຂອງປ້າຍຊື່; ຈຸດເທິງຂອບ ແລະ ລະນາບອ້າງອີງຈະຕາມໄປ. |
| `why-title-block-editor` | Abra o editor de carimbos | Open the title block editor | Abra el editor de rótulos | Ouvrez l'éditeur de cartouches | Öffnen Sie den Schriftfeld-Editor | Apri l'editor dei cartigli | Otwórz edytor tabliczek | Відкрийте редактор штампів | 表題欄エディターを開いてください | 请打开图签编辑器 | 請開啟圖框標題欄編輯器 | ເປີດຕົວແກ້ໄຂປ້າຍຊື່ |
| `sketch-title-sheet` | Esboço: linhas do carimbo | Sketch: Title Block Lines | Boceto: líneas del rótulo | Esquisse : lignes du cartouche | Skizze: Schriftfeldlinien | Schizzo: linee del cartiglio | Szkic: linie tabliczki | Ескіз: лінії штампа | スケッチ: 表題欄の線 | 草图：图签线条 | 草圖：圖框標題欄線條 | ຮ່າງ: ເສັ້ນປ້າຍຊື່ |

- [ ] **Step 4: Run the tests** — `cargo test -p cc_app` (component_env, component_sketch, sketch_mode, title_block_env, commands, i18n); clippy.

- [ ] **Step 5: Commit**

```bash
git add crates/cc_app/src assets/i18n
git commit -s -m "Sketch title block strokes on the Sheet view"
```

---

### Task 11: Selecting 2D pieces — pick, window, Delete, stroke properties

**Files:**
- Modify: `crates/cc_app/src/title_block_env.rs`
- Modify: `assets/i18n/*.ftl`
- Test: `crates/cc_app/src/title_block_env.rs`

**Interfaces:**
- Produces: `pub fn text_box(t: &SheetText) -> (DVec2, DVec2)`; `pub fn piece_at(pieces, p_mm, tol_mm) -> Option<usize>`;
  `pub fn pieces_in(pieces, a_mm, b_mm) -> Vec<usize>`; `fn patch(state, index, key: Option<u64>, value)`;
  `fn piece_props(ui, state, def, index)` (stroke part here; text and image parts in Tasks 12-13).

- [ ] **Step 1: Write the failing test**

```rust
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
```

- [ ] **Step 2: Run to verify it fails** — `cargo test -p cc_app pieces_are_picked` → FAIL.

- [ ] **Step 3: Implement**

```rust
use cc_core::title_block::{SheetPieces, SheetShape, SheetText, TextAnchor};

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
    let inside = |(lo, hi): (DVec2, DVec2)| p.cmpge(lo - DVec2::splat(tol)).all() && p.cmple(hi + DVec2::splat(tol)).all();
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
    let mut out: Vec<usize> = pieces
        .strokes
        .iter()
        .filter(|s| s.points.iter().all(|p| inside(*p)))
        .map(|s| s.item)
        .chain(pieces.texts.iter().filter(|t| { let (l, h) = text_box(t); inside(l) && inside(h) }).map(|t| t.item))
        .chain(pieces.images.iter().filter(|i| inside(i.corner) && inside(i.corner + i.size)).map(|i| i.item))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Change piece `index` with a merge patch over its shape (one undo step per `key`).
fn patch(state: &mut AppState, index: usize, key: Option<u64>, value: serde_json::Value) -> bool {
    let command = ComponentSheetPieceSet {
        type_ref: crate::component_env::type_ref(state),
        index,
        piece: Some(value),
        name: None,
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

fn picked(state: &AppState) -> Vec<usize> {
    match state.component_env.as_ref().and_then(|e| e.pick.clone()) {
        Some(crate::component_env::Pick::Sheet(list)) => list,
        _ => Vec::new(),
    }
}
```

In `central`: compute `let picked = picked(state);` and outline each
picked piece in `PALETTE.accent` (strokes: its segments at 2 px; texts and
images: their box). In the `EnvTool::Select` arm, before the plane pick:

```rust
        crate::component_env::EnvTool::Select => {
            let hover = resp.hover_pos().map(|p| mm.to_world(p));
            let reach = tol * MM;
            // A window from where the drag started (not on a grip, Task 13 adds those).
            if resp.drag_started_by(egui::PointerButton::Primary)
                && let Some(origin) = ui.input(|i| i.pointer.press_origin())
                && let Some(e) = editor_mut(state)
            {
                e.drag_from = Some(mm.to_world(origin));
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
                    let list = pieces_in(&pieces, a, b);
                    if let Some(env) = state.component_env.as_mut() {
                        env.pick = (!list.is_empty()).then_some(crate::component_env::Pick::Sheet(list));
                    }
                }
            }
            if resp.double_clicked()
                && let Some(i) = hover.and_then(|p| piece_at(&pieces, p, reach))
                && matches!(def.sheet.get(i).map(|s| &s.shape), Some(SheetShape::Stroke { .. }))
            {
                edit_stroke(state, i);
                return;
            }
            if clicked && let Some(p) = hover {
                let hit = piece_at(&pieces, p, reach).map(|i| crate::component_env::Pick::Sheet(vec![i])).or_else(|| {
                    planes
                        .iter()
                        .filter(|q| !q.edge)
                        .find(|q| (if q.vertical { p.x } else { p.y } - q.at).abs() <= reach)
                        .map(|q| crate::component_env::Pick::Plane(q.name.clone()))
                });
                if let Some(env) = state.component_env.as_mut() {
                    env.pick = hit;
                }
            }
        }
```

(This replaces Task 9's `Select` arm; the plane pick is the fallback.)
`hint`: `Some(EnvTool::Select) => t!("tb-hint-select")`.

`panel_content`, after the parameters section:

```rust
    if let [index] = picked(state)[..] {
        crate::table::section(ui, "tb-piece", t!("comp-env-piece"), |ui| piece_props(ui, state, def, index));
    }
```

```rust
/// The picked piece's values: name; a stroke's weight, color, fill and
/// *Edit Sketch* (texts and pictures in Tasks 12-13).
fn piece_props(ui: &mut Ui, state: &mut AppState, def: &ComponentDef, index: usize) {
    let Some(item) = def.sheet.get(index).cloned() else { return };
    let t = crate::component_env::type_ref(state);
    let mut rename = None;
    crate::table::row(ui, &t!("prop-name"), |ui| {
        rename = crate::component_env::draft_field(ui, state, &format!("tb-piece-name:{index}"), &item.name, 150.0)
    });
    if let Some(name) = rename {
        crate::actions::run_command(
            state,
            ComponentSheetPieceSet { type_ref: t.clone(), index, piece: None, name: Some(name) },
        );
    }
    if let SheetShape::Stroke { weight, color, fill, closed, .. } = &item.shape {
        crate::table::readonly_row(ui, &t!("tb-piece-stroke"), String::new());
        if ui.button(t!("comp-edit-sketch")).clicked() {
            edit_stroke(state, index);
            return;
        }
        let mut new_weight = None;
        crate::table::row(ui, &t!("vg-weight"), |ui| {
            new_weight = crate::component_env::draft_field(ui, state, &format!("tb-weight:{index}"), weight, 80.0)
        });
        if let Some(w) = new_weight {
            patch(state, index, None, serde_json::json!({ "Stroke": { "weight": w } }));
        }
        // Black is the default (no color stored).
        let mut rgb = color.unwrap_or([0, 0, 0]);
        let r = crate::table::row(ui, &t!("vg-color"), |ui| ui.color_edit_button_srgb(&mut rgb));
        if r.changed() {
            let value = if rgb == [0, 0, 0] { serde_json::Value::Null } else { serde_json::json!(rgb) };
            patch(state, index, Some(key(("tb-color", index))), serde_json::json!({ "Stroke": { "color": value } }));
        }
        if r.drag_stopped() || r.lost_focus() {
            state.session.doc.break_merge();
        }
        crate::table::readonly_row(ui, &t!("tb-closed"), if *closed { "✓".into() } else { "—".into() });
        if *closed {
            let mut on = fill.is_some();
            let mut rgb = fill.unwrap_or([220, 220, 220]);
            let r = crate::table::row(ui, &t!("tb-fill"), |ui| {
                ui.horizontal(|ui| ui.checkbox(&mut on, "").changed() | ui.color_edit_button_srgb(&mut rgb).changed()).inner
            });
            if r {
                let value = if on { serde_json::json!(rgb) } else { serde_json::Value::Null };
                patch(state, index, None, serde_json::json!({ "Stroke": { "fill": value } }));
            }
        }
    }
}

fn key(parts: impl std::hash::Hash) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut h);
    h.finish()
}
```

A merge patch with `null` removes the key; the model's `color`/`fill` are
`Option`s with `skip_serializing_if`, so the removed key deserializes as
`None`.

i18n:

| key | pt-BR | en | es | fr | de | it | pl | uk-UA | ja | zh-CN | zh-TW | lo |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `tb-hint-select` | Clique numa peça para selecioná-la; arraste uma janela para várias · Delete remove · clique duplo num traço edita o esboço | Click a piece to select it, drag a window for several · Delete removes · double-click a stroke to edit its sketch | Haga clic en una pieza para seleccionarla; arrastre una ventana para varias · Supr elimina · doble clic en un trazo edita su boceto | Cliquez sur un élément pour le sélectionner, tracez une fenêtre pour plusieurs · Suppr supprime · double-clic sur un trait pour modifier son esquisse | Klicken Sie ein Teil an, um es zu wählen; ziehen Sie ein Fenster für mehrere · Entf löscht · Doppelklick auf einen Strich bearbeitet seine Skizze | Fai clic su un pezzo per selezionarlo, trascina una finestra per più pezzi · Canc elimina · doppio clic su un tratto per modificarne lo schizzo | Kliknij element, aby go zaznaczyć; przeciągnij okno, aby zaznaczyć kilka · Delete usuwa · dwuklik na kresce edytuje jej szkic | Клацніть елемент, щоб вибрати; протягніть рамку для кількох · Delete видаляє · подвійне клацання на штриху редагує ескіз | 要素をクリックで選択、窓をドラッグで複数選択 · Delete で削除 · 線をダブルクリックでスケッチを編集 | 单击元素选择，拖出窗口选择多个 · Delete 删除 · 双击线条编辑其草图 | 按一下元素選取，拖曳視窗選取多個 · Delete 刪除 · 按兩下線條編輯其草圖 | ຄລິກຊິ້ນເພື່ອເລືອກ, ລາກກອບເພື່ອເລືອກຫຼາຍ · Delete ລຶບ · ຄລິກສອງເທື່ອເທິງເສັ້ນເພື່ອແກ້ຮ່າງ |
| `tb-piece-stroke` | Traço | Stroke | Trazo | Trait | Strich | Tratto | Kreska | Штрих | 線 | 线条 | 線條 | ເສັ້ນ |
| `tb-fill` | Preenchimento | Fill | Relleno | Remplissage | Füllung | Riempimento | Wypełnienie | Заливка | 塗りつぶし | 填充 | 填滿 | ສີພື້ນ |
| `tb-closed` | Fechado | Closed | Cerrado | Fermé | Geschlossen | Chiuso | Zamknięty | Замкнений | 閉じた | 闭合 | 封閉 | ປິດ |

- [ ] **Step 4: Run the tests** — `cargo test -p cc_app title_block_env i18n`; clippy.

- [ ] **Step 5: Commit**

```bash
git add crates/cc_app/src assets/i18n
git commit -s -m "Pick, window-select and delete title block pieces"
```

---

### Task 12: Text tool, text properties and Insert field

**Files:**
- Modify: `crates/cc_app/src/component_env.rs` (`EnvTool::SheetText`; arms in `options`' hint match and `canvas`' tool match: `EnvTool::SheetText => …`/`{}`)
- Modify: `crates/cc_app/src/commands.rs` (`TbText`)
- Modify: `crates/cc_app/src/title_block_env.rs`
- Modify: `assets/i18n/*.ftl`
- Test: `crates/cc_app/src/title_block_env.rs`

**Interfaces:**
- Produces: `EnvTool::SheetText`; `UiCommand::TbText`; `pub fn coordinate(planes, value_mm, vertical) -> String`;
  `pub fn place_text(state, planes, at_mm)`; `pub fn field_menu(project, def) -> Vec<(String, Vec<(String, String)>)>`;
  `pub fn insert_field(text, key, at: Option<usize>) -> (String, usize)`.

- [ ] **Step 1: Write the failing tests**

```rust
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
    }
```

- [ ] **Step 2: Run to verify they fail** — `cargo test -p cc_app fields_are_inserted texts_are_placed` → FAIL.

- [ ] **Step 3: Implement**

`component_env.rs`: `EnvTool::SheetText` (doc: "A click on a title block's Sheet view places a text."). In `options`' hint match add `EnvTool::SheetText => t!("tb-hint-text"),`; in `canvas`' tool match add `EnvTool::SheetText => {}`.

`commands.rs`: `TbText` (icon `Icon::Text`, label `t!("tb-tool-text")`, description `t!("cmd-desc-tb-text")`, engine `"component_sheet_piece_add"`, unavailable like `TbLines`, selected `state.component_env.as_ref().is_some_and(|e| e.tool == crate::component_env::EnvTool::SheetText)`, run `crate::component_env::set_tool(state, crate::component_env::EnvTool::SheetText)`). Ribbon draw group becomes `&[TbLines, TbText]`.

`title_block_env.rs`:

```rust
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
    let command = ComponentSheetPieceAdd { type_ref: crate::component_env::type_ref(state), piece, name: None };
    if let Some(i) = crate::actions::run_command(state, command)
        .and_then(|o| o["index"].as_u64())
        .and_then(|i| usize::try_from(i).ok())
        && let Some(env) = state.component_env.as_mut()
    {
        env.pick = Some(crate::component_env::Pick::Sheet(vec![i]));
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

/// The *Insert field* menu: groups of (name shown, key).
pub fn field_menu(p: &cc_core::Project, def: &ComponentDef) -> Vec<(String, Vec<(String, String)>)> {
    let fixed = |keys: &[&str]| keys.iter().map(|k| (field_label(k), (*k).to_string())).collect::<Vec<_>>();
    let mut project = fixed(&["project_name", "project_number", "client", "address", "author"]);
    project.extend(p.info.fields.iter().map(|f| (f.name.clone(), format!("project:{}", f.name))));
    let per_sheet = def
        .params
        .iter()
        .filter(|q| q.kind.is_text() && q.instance)
        .map(|q| (q.display().to_string(), format!("param:{}", q.name)))
        .collect();
    vec![
        (t!("tb-group-sheet"), fixed(&["sheet_number", "sheet_name", "sheet_count", "sheet_index", "sheet_scale", "paper"])),
        (t!("tb-group-project"), project),
        (t!("tb-group-dates"), fixed(&["date", "issue_date"])),
        (t!("tb-group-per-sheet"), per_sheet),
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
```

`central`, tool match: 

```rust
        crate::component_env::EnvTool::SheetText => {
            if clicked && let Some(r) = raw {
                let (at, _) = crate::component_sketch::snap(&plane_view(&planes, 1.0 / MM), r, tol);
                place_text(state, &planes, at * MM);
            }
        }
```

`hint`: `Some(EnvTool::SheetText) => t!("tb-hint-text")`.

`piece_props`, after the stroke block — the text block:

```rust
    if let SheetShape::Text { x, y, text, size, anchor, rotation } = &item.shape {
        let text_id = egui::Id::new(("tb-text", index));
        let mut words = text.clone();
        let r = crate::table::row(ui, &t!("prop-text"), |ui| {
            crate::table::live_text(ui, ("tb-text", index), &mut words, |ui, t| {
                ui.add(egui::TextEdit::multiline(t).id(text_id).desired_rows(2).desired_width(170.0))
            })
        });
        if r.changed() {
            patch(state, index, Some(key(("tb-text", index))), serde_json::json!({ "Text": { "text": words } }));
        }
        if r.lost_focus() {
            state.session.doc.break_merge();
        }
        // Insert field: at the text's cursor, or at its end.
        let mut chosen = None;
        let menu = field_menu(state.project(), def);
        ui.menu_button(t!("tb-insert-field"), |ui| {
            for (group, items) in &menu {
                ui.menu_button(group, |ui| {
                    for (label, k) in items {
                        if ui.button(label).clicked() {
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
                .map(|r| r.primary.index);
            let (new, _) = insert_field(text, &k, cursor);
            patch(state, index, None, serde_json::json!({ "Text": { "text": new } }));
        }
        let fields: [(&str, String, &str); 4] = [
            ("x", t!("comp-field-x"), x),
            ("y", t!("comp-field-y"), y),
            ("size", t!("tb-text-size"), size),
            ("rotation", t!("prop-rotation"), rotation),
        ];
        for (field, label, current) in fields {
            let mut new = None;
            crate::table::row(ui, &label, |ui| {
                new = crate::component_env::draft_field(ui, state, &format!("tb-{field}:{index}"), current, 120.0)
            });
            if let Some(v) = new {
                patch(state, index, None, serde_json::json!({ "Text": { field: v } }));
            }
        }
        let mut a = *anchor;
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
        if a != *anchor {
            patch(state, index, None, serde_json::json!({ "Text": { "anchor": a } }));
        }
    }
```

`TextAnchor` serializes as `"Left"`… (`serde_json::json!` of the enum
uses its serde form). Check `str::split_at_checked` is stable on the
workspace toolchain (Rust ≥ 1.80); it is required by edition 2024 anyway.

i18n (`tb-field-*` keys label the fixed fields in the menu):

| key | pt-BR | en | es | fr | de | it | pl | uk-UA | ja | zh-CN | zh-TW | lo |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `tb-tool-text` | Texto | Text | Texto | Texte | Text | Testo | Tekst | Текст | 文字 | 文字 | 文字 | ຂໍ້ຄວາມ |
| `cmd-desc-tb-text` | Clique na folha para colocar um texto; seus campos são preenchidos pela folha e pelo projeto. | Click on the sheet to place a text; its fields fill in from the sheet and the project. | Haga clic en la hoja para colocar un texto; sus campos se rellenan con la hoja y el proyecto. | Cliquez sur la feuille pour placer un texte ; ses champs se remplissent depuis la feuille et le projet. | Klicken Sie auf das Blatt, um einen Text zu setzen; seine Felder füllen sich aus Blatt und Projekt. | Fai clic sulla tavola per inserire un testo; i suoi campi si compilano dalla tavola e dal progetto. | Kliknij arkusz, aby wstawić tekst; jego pola wypełniają się danymi arkusza i projektu. | Клацніть на аркуші, щоб розмістити текст; його поля заповнюються з аркуша й проєкту. | シートをクリックしてテキストを配置します。項目はシートとプロジェクトから入ります。 | 单击图纸放置文字；其中的字段由图纸和项目填写。 | 按一下圖紙放置文字；其中的欄位由圖紙與專案填入。 | ຄລິກເທິງແຜ່ນເພື່ອວາງຂໍ້ຄວາມ; ຊ່ອງຂໍ້ມູນຈະເຕີມຈາກແຜ່ນ ແລະ ໂຄງການ. |
| `tb-hint-text` | Clique onde o texto fica · Esc encerra | Click where the text goes · Esc ends | Haga clic donde va el texto · Esc termina | Cliquez où va le texte · Échap termine | Klicken Sie, wo der Text hin soll · Esc beendet | Fai clic dove va il testo · Esc termina | Kliknij, gdzie ma być tekst · Esc kończy | Клацніть, де буде текст · Esc завершує | テキストを置く位置をクリック · Esc で終了 | 单击文字所在位置 · Esc 结束 | 按一下文字的位置 · Esc 結束 | ຄລິກບ່ອນວາງຂໍ້ຄວາມ · Esc ສິ້ນສຸດ |
| `tb-text-size` | Altura do texto | Text height | Altura del texto | Hauteur du texte | Texthöhe | Altezza testo | Wysokość tekstu | Висота тексту | 文字の高さ | 文字高度 | 文字高度 | ຄວາມສູງຂໍ້ຄວາມ |
| `tb-anchor` | Alinhamento | Alignment | Alineación | Alignement | Ausrichtung | Allineamento | Wyrównanie | Вирівнювання | 配置 | 对齐 | 對齊 | ການຈັດວາງ |
| `tb-anchor-left` | Esquerda | Left | Izquierda | Gauche | Links | Sinistra | Do lewej | Ліворуч | 左 | 左 | 左 | ຊ້າຍ |
| `tb-anchor-center` | Centro | Center | Centro | Centre | Mitte | Centro | Do środka | По центру | 中央 | 居中 | 置中 | ກາງ |
| `tb-anchor-right` | Direita | Right | Derecha | Droite | Rechts | Destra | Do prawej | Праворуч | 右 | 右 | 右 | ຂວາ |
| `tb-anchor-middle` | Centro do bloco | Middle | Centro del bloque | Milieu | Blockmitte | Centro del blocco | Środek bloku | Центр блоку | ブロック中央 | 块中心 | 區塊中心 | ກາງບລັອກ |
| `tb-insert-field` | Inserir campo | Insert field | Insertar campo | Insérer un champ | Feld einfügen | Inserisci campo | Wstaw pole | Вставити поле | 項目を挿入 | 插入字段 | 插入欄位 | ໃສ່ຊ່ອງ |
| `tb-group-sheet` | Folha | Sheet | Hoja | Feuille | Blatt | Tavola | Arkusz | Аркуш | シート | 图纸 | 圖紙 | ແຜ່ນ |
| `tb-group-project` | Projeto | Project | Proyecto | Projet | Projekt | Progetto | Projekt | Проєкт | プロジェクト | 项目 | 專案 | ໂຄງການ |
| `tb-group-dates` | Datas | Dates | Fechas | Dates | Daten | Date | Daty | Дати | 日付 | 日期 | 日期 | ວັນທີ |
| `tb-group-per-sheet` | Por folha | Per sheet | Por hoja | Par feuille | Je Blatt | Per tavola | Na arkusz | Для аркуша | シートごと | 按图纸 | 依圖紙 | ຕໍ່ແຜ່ນ |
| `tb-field-sheet-number` | Número da folha | Sheet number | Número de hoja | Numéro de feuille | Blattnummer | Numero tavola | Numer arkusza | Номер аркуша | シート番号 | 图纸编号 | 圖紙編號 | ເລກແຜ່ນ |
| `tb-field-sheet-name` | Nome da folha | Sheet name | Nombre de hoja | Nom de la feuille | Blattname | Nome tavola | Nazwa arkusza | Назва аркуша | シート名 | 图纸名称 | 圖紙名稱 | ຊື່ແຜ່ນ |
| `tb-field-sheet-count` | Total de folhas | Sheet count | Total de hojas | Nombre de feuilles | Anzahl Blätter | Numero di tavole | Liczba arkuszy | Кількість аркушів | シート数 | 图纸总数 | 圖紙總數 | ຈຳນວນແຜ່ນ |
| `tb-field-sheet-index` | Posição da folha | Sheet position | Posición de la hoja | Position de la feuille | Blattposition | Posizione della tavola | Pozycja arkusza | Позиція аркуша | シートの順番 | 图纸序号 | 圖紙序號 | ລຳດັບແຜ່ນ |
| `tb-field-sheet-scale` | Escala | Scale | Escala | Échelle | Maßstab | Scala | Skala | Масштаб | 縮尺 | 比例 | 比例 | ມາດຕາສ່ວນ |
| `tb-field-paper` | Formato do papel | Paper size | Tamaño del papel | Format du papier | Papierformat | Formato carta | Format papieru | Формат паперу | 用紙サイズ | 图幅 | 圖幅 | ຂະໜາດເຈ້ຍ |
| `tb-field-project-name` | Nome do projeto | Project name | Nombre del proyecto | Nom du projet | Projektname | Nome del progetto | Nazwa projektu | Назва проєкту | プロジェクト名 | 项目名称 | 專案名稱 | ຊື່ໂຄງການ |
| `tb-field-project-number` | Número do projeto | Project number | Número del proyecto | Numéro du projet | Projektnummer | Numero del progetto | Numer projektu | Номер проєкту | プロジェクト番号 | 项目编号 | 專案編號 | ເລກໂຄງການ |
| `tb-field-client` | Cliente | Client | Cliente | Client | Bauherr | Cliente | Klient | Замовник | クライアント | 客户 | 客戶 | ລູກຄ້າ |
| `tb-field-address` | Endereço | Address | Dirección | Adresse | Adresse | Indirizzo | Adres | Адреса | 住所 | 地址 | 地址 | ທີ່ຢູ່ |
| `tb-field-author` | Autor | Author | Autor | Auteur | Verfasser | Autore | Autor | Автор | 作成者 | 作者 | 作者 | ຜູ້ຂຽນ |
| `tb-field-date` | Data de hoje | Today's date | Fecha de hoy | Date du jour | Heutiges Datum | Data di oggi | Dzisiejsza data | Сьогоднішня дата | 今日の日付 | 今天日期 | 今天日期 | ວັນທີມື້ນີ້ |
| `tb-field-issue-date` | Data de emissão | Issue date | Fecha de emisión | Date d'émission | Ausgabedatum | Data di emissione | Data wydania | Дата випуску | 発行日 | 发布日期 | 發行日期 | ວັນທີອອກ |

- [ ] **Step 4: Run the tests** — `cargo test -p cc_app title_block_env component_env commands i18n`; clippy.

- [ ] **Step 5: Commit**

```bash
git add crates/cc_app/src assets/i18n
git commit -s -m "Place title block texts and insert fields"
```

---

### Task 13: Image tool and grips

**Files:**
- Modify: `crates/cc_app/src/component_env.rs` (`EnvTool::SheetImage { picture: std::sync::Arc<crate::title_block_env::Picture>, start: Option<DVec2> }`; `options`' hint match; `canvas`' tool match; `escape`: a started picture drops its start first)
- Modify: `crates/cc_app/src/commands.rs` (`TbImage`)
- Modify: `crates/cc_app/src/title_block_env.rs`
- Modify: `assets/i18n/*.ftl`
- Test: `crates/cc_app/src/title_block_env.rs`

**Interfaces:**
- Produces: `pub struct Picture { pub bytes: Vec<u8>, pub aspect: f64 }`; `pub fn picture(bytes: Vec<u8>) -> Option<Picture>`;
  `pub fn image_rect(a, b, aspect, free) -> (DVec2, DVec2)`; `pub fn pick_image(state)`; `fn add_image(state, planes, corner_mm, size_mm, picture)`;
  grips on a lone text (position) and picture (corner, size).

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn pictures_keep_their_aspect_unless_freed() {
        let (corner, size) = image_rect(DVec2::new(10.0, 10.0), DVec2::new(50.0, 15.0), 2.0, false);
        assert_eq!((corner, size), (DVec2::new(10.0, 10.0), DVec2::new(40.0, 20.0)));
        let (corner, size) = image_rect(DVec2::new(10.0, 10.0), DVec2::new(0.0, 0.0), 1.0, true);
        assert_eq!((corner, size), (DVec2::ZERO, DVec2::new(10.0, 10.0)));
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
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="red"/></svg>"#;
        let s = picture(svg.to_vec()).expect("an SVG drawn into a PNG");
        assert!((s.aspect - 2.0).abs() < 1e-3);
        assert!(picture(b"not a picture".to_vec()).is_none());
        let (mut state, _, _) = editor_state();
        let (_, def) = crate::component_env::component(&state).unwrap();
        let before = def.sheet.len();
        let ps = planes(&def, DVec2::new(420.0, 297.0));
        add_image(&mut state, &ps, DVec2::new(15.0, 15.0), DVec2::new(40.0, 20.0), &s);
        let (_, def) = crate::component_env::component(&state).unwrap();
        assert!(matches!(def.sheet.get(before).map(|i| &i.shape), Some(SheetShape::Image { .. })), "{}", state.message);
    }
```

- [ ] **Step 2: Run to verify they fail** — `cargo test -p cc_app pictures_` → FAIL.

- [ ] **Step 3: Implement**

```rust
/// A picture for the Image tool: PNG, JPEG, GIF, BMP or WebP bytes (an SVG
/// drawn into a PNG), and its width / height.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub bytes: Vec<u8>,
    pub aspect: f64,
}

/// Longest side an SVG is drawn at, pixels.
const SVG_PX: f32 = 2048.0;

/// The picture in a file's bytes; an SVG is drawn into a PNG.
pub fn picture(bytes: Vec<u8>) -> Option<Picture> {
    if let Some(img) = cc_views::raster::probe(bytes.clone()) {
        let aspect = f64::from(img.width) / f64::from(img.height.max(1));
        return Some(Picture { bytes: img.data, aspect });
    }
    let tree = resvg::usvg::Tree::from_data(&bytes, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    let k = SVG_PX / size.width().max(size.height()).max(1.0);
    let w = (size.width() * k).round().max(1.0) as u32;
    let h = (size.height() * k).round().max(1.0) as u32;
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
    Some(Picture { bytes: png, aspect: f64::from(w) / f64::from(h) })
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

/// *Image*: pick a PNG, JPG or SVG file, then drag its rectangle.
pub fn pick_image(state: &mut AppState) {
    let Some(path) = rfd::FileDialog::new().add_filter(t!("file-filter-image"), &["png", "jpg", "jpeg", "svg"]).pick_file()
    else {
        return;
    };
    match std::fs::read(&path).ok().and_then(picture) {
        Some(p) => crate::component_env::set_tool(
            state,
            crate::component_env::EnvTool::SheetImage { picture: std::sync::Arc::new(p), start: None },
        ),
        None => state.message = t!("msg-image-unreadable"),
    }
}

/// Add a picture filling `corner`–`corner + size` (millimeters) and pick it.
fn add_image(state: &mut AppState, planes: &[SheetPlane], corner: DVec2, size: DVec2, p: &Picture) {
    if size.min_element() < 0.5 {
        return;
    }
    let piece = SheetPieceArg::Image {
        x: ExprArg::Text(coordinate(planes, corner.x, true)),
        y: ExprArg::Text(coordinate(planes, corner.y, false)),
        width: ExprArg::Number((size.x * 100.0).round() / 100.0),
        height: ExprArg::Number((size.y * 100.0).round() / 100.0),
        image_path: None,
        image: Some(cc_core::raster::base64_encode(&p.bytes)),
    };
    let command = ComponentSheetPieceAdd { type_ref: crate::component_env::type_ref(state), piece, name: None };
    if let Some(i) = crate::actions::run_command(state, command)
        .and_then(|o| o["index"].as_u64())
        .and_then(|i| usize::try_from(i).ok())
        && let Some(env) = state.component_env.as_mut()
    {
        env.pick = Some(crate::component_env::Pick::Sheet(vec![i]));
    }
}

/// The grips of a lone picked text (its point) or picture (corner, opposite corner), millimeters.
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
fn move_grip(state: &mut AppState, pieces: &SheetPieces, planes: &[SheetPlane], index: usize, which: u8, at: DVec2, free: bool) {
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
```

`central`:
- Paint the grips of `picked` when it holds one piece: small filled squares
  (8 × 8 px) in `PALETTE.accent` at `mm.to_screen(at)`.
- In the `Select` arm, `drag_started_by(Primary)`: a press within
  `reach * 1.5` of a grip sets `editor_mut(state).grip = Some((index, which))`
  instead of `drag_from`. While dragged with a grip: snap the pointer
  (`crate::component_sketch::snap(&plane_view(&planes, 1.0 / MM), raw, tol)`),
  then `move_grip(state, &pieces, &planes, index, which, at * MM, shift)`
  with `let shift = ui.input(|i| i.modifiers.shift);`. On `drag_stopped`:
  `if editor_mut(state).and_then(|e| e.grip.take()).is_some() { state.session.doc.break_merge(); }`
  before the window branch.
- New arm:

```rust
        crate::component_env::EnvTool::SheetImage { picture, start } => {
            let snapped = raw.map(|r| crate::component_sketch::snap(&plane_view(&planes, 1.0 / MM), r, tol).0 * MM);
            let free = ui.input(|i| i.modifiers.shift);
            if resp.drag_started_by(egui::PointerButton::Primary) && let Some(p) = snapped {
                crate::component_env::set_tool(
                    state,
                    crate::component_env::EnvTool::SheetImage { picture: picture.clone(), start: Some(p) },
                );
            }
            if let (Some(a), Some(b)) = (start, snapped) {
                let (corner, size) = image_rect(a, b, picture.aspect, free);
                let r = Rect::from_two_pos(mm.to_screen(corner), mm.to_screen(corner + size));
                painter.rect_stroke(r, 0.0, Stroke::new(1.5, PALETTE.accent), egui::StrokeKind::Middle);
                if resp.drag_stopped_by(egui::PointerButton::Primary) {
                    add_image(state, &planes, corner, size, &picture);
                    crate::component_env::set_tool(state, crate::component_env::EnvTool::Select);
                }
            }
        }
```

`hint`: `Some(EnvTool::SheetImage { .. }) => t!("tb-hint-image")`.
`component_env::escape`: add the arm
`EnvTool::SheetImage { picture, start: Some(_) } => EnvTool::SheetImage { picture: picture.clone(), start: None },`
before `EnvTool::Select`. `options`' hint match: `EnvTool::SheetImage { .. } => t!("tb-hint-image"),`.
`canvas`' tool match: `EnvTool::SheetImage { .. } => {}`.

`commands.rs`: `TbImage` (icon `Icon::Image`, label `t!("tb-tool-image")`,
description `t!("cmd-desc-tb-image")`, engine `"component_sheet_piece_add"`,
unavailable like `TbLines`, selected when the env tool is
`EnvTool::SheetImage { .. }`, run `crate::title_block_env::pick_image(state)`).
Ribbon draw group: `&[TbLines, TbText, TbImage]`.

`piece_props`, the picture block:

```rust
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
```

i18n:

| key | pt-BR | en | es | fr | de | it | pl | uk-UA | ja | zh-CN | zh-TW | lo |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `tb-tool-image` | Imagem | Image | Imagen | Image | Bild | Immagine | Obraz | Зображення | 画像 | 图像 | 圖像 | ຮູບ |
| `cmd-desc-tb-image` | Escolha uma imagem PNG, JPG ou SVG e arraste o seu retângulo. | Pick a PNG, JPG or SVG picture, then drag its rectangle. | Elija una imagen PNG, JPG o SVG y arrastre su rectángulo. | Choisissez une image PNG, JPG ou SVG, puis tracez son rectangle. | Wählen Sie ein PNG-, JPG- oder SVG-Bild und ziehen Sie sein Rechteck auf. | Scegli un'immagine PNG, JPG o SVG, poi trascina il suo rettangolo. | Wybierz obraz PNG, JPG lub SVG, a następnie przeciągnij jego prostokąt. | Виберіть зображення PNG, JPG або SVG і протягніть його прямокутник. | PNG、JPG、SVG の画像を選び、その矩形をドラッグします。 | 选择 PNG、JPG 或 SVG 图片，然后拖出其矩形。 | 選擇 PNG、JPG 或 SVG 圖片，然後拖曳出其矩形。 | ເລືອກຮູບ PNG, JPG ຫຼື SVG ແລ້ວລາກສີ່ລ່ຽມຂອງມັນ. |
| `tb-hint-image` | Arraste o retângulo da imagem · Shift libera a proporção · Esc encerra | Drag the picture's rectangle · Shift frees the aspect · Esc ends | Arrastre el rectángulo de la imagen · Mayús libera la proporción · Esc termina | Tracez le rectangle de l'image · Maj libère les proportions · Échap termine | Ziehen Sie das Rechteck des Bildes auf · Umschalt löst das Seitenverhältnis · Esc beendet | Trascina il rettangolo dell'immagine · Maiusc libera le proporzioni · Esc termina | Przeciągnij prostokąt obrazu · Shift zwalnia proporcje · Esc kończy | Протягніть прямокутник зображення · Shift знімає пропорції · Esc завершує | 画像の矩形をドラッグ · Shift で縦横比を解除 · Esc で終了 | 拖出图片的矩形 · Shift 不保持比例 · Esc 结束 | 拖曳圖片的矩形 · Shift 不維持比例 · Esc 結束 | ລາກສີ່ລ່ຽມຂອງຮູບ · Shift ປົດອັດຕາສ່ວນ · Esc ສິ້ນສຸດ |
| `msg-image-unreadable` | Não foi possível ler a imagem (PNG, JPG ou SVG). | The picture could not be read (PNG, JPG or SVG). | No se pudo leer la imagen (PNG, JPG o SVG). | Impossible de lire l'image (PNG, JPG ou SVG). | Das Bild konnte nicht gelesen werden (PNG, JPG oder SVG). | Impossibile leggere l'immagine (PNG, JPG o SVG). | Nie udało się odczytać obrazu (PNG, JPG lub SVG). | Не вдалося прочитати зображення (PNG, JPG або SVG). | 画像を読み込めませんでした (PNG、JPG、SVG)。 | 无法读取图片（PNG、JPG 或 SVG）。 | 無法讀取圖片（PNG、JPG 或 SVG）。 | ອ່ານຮູບບໍ່ໄດ້ (PNG, JPG ຫຼື SVG). |

- [ ] **Step 4: Run the tests** — `cargo test -p cc_app`; clippy. Check
  `resvg::tiny_skia::PremultipliedColorU8::demultiply` and
  `ColorU8::{red, green, blue, alpha}` exist in tiny-skia 0.11 (resvg 0.45);
  `usvg::Tree::from_data(&[u8], &Options)` is the 0.45 signature.

- [ ] **Step 5: Commit**

```bash
git add crates/cc_app/src assets/i18n
git commit -s -m "Place title block pictures and move texts and pictures by their grips"
```

---

### Task 14: End-to-end tests, snapshots, verification and PR

**Files:**
- Modify: `crates/cc_app/src/title_block_env.rs` (end-to-end test), `crates/cc_app/src/ui_tests.rs` (kittest + snapshot)
- Create/Update: `crates/cc_app/tests/snapshots/title_block_editor.png`, `ribbon_modify.png`, `ribbon_view.png`

- [ ] **Step 1: The spec's end-to-end test** (title_block_env tests)

```rust
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
            _ => unreachable!(),
        };
        let composed = state.sheet(sheet).unwrap();
        let want = format!("Nº {number}");
        assert!(
            composed.drawing.prims.iter().any(|p| matches!(p, cc_views::Prim::Text { text, .. } if *text == want)),
            "{want} not on the sheet"
        );
    }
```

- [ ] **Step 2: A kittest test of the Text tool** (ui_tests.rs)

```rust
/// In the title block editor the ribbon's Text tool and a click on the
/// paper add a text piece and pick it.
#[test]
fn the_text_tool_places_a_text_on_the_sheet() {
    let mut state = sample_state();
    let sheet = state.project().sheets_in_order()[0];
    state.active_view = Some(sheet);
    crate::title_block_env::edit_sheet(&mut state, sheet);
    let before = crate::component_env::component(&state).unwrap().1.sheet.len();
    let mut harness = themed_harness(state, egui::vec2(1100.0, 800.0), egui::Theme::Light, |ui, state| {
        crate::component_env::ribbon(ui, state);
        crate::component_env::central(ui, state);
    });
    harness.run();
    harness.get_by_label(&t!("tb-tool-text")).click();
    harness.run();
    let at = egui::pos2(550.0, 500.0);
    harness.input_mut().events.push(egui::Event::PointerMoved(at));
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();
    }
    let state = harness.state();
    assert_eq!(crate::component_env::component(state).unwrap().1.sheet.len(), before + 1);
    assert_eq!(state.component_env.as_ref().unwrap().pick, Some(crate::component_env::Pick::Sheet(vec![before])));
}
```

- [ ] **Step 3: Snapshots**

Add (on request only):

```rust
#[test]
fn title_block_editor_snapshot() {
    if !snapshots_requested() {
        return;
    }
    let mut state = sample_state();
    let sheet = state.project().sheets_in_order()[0];
    state.active_view = Some(sheet);
    crate::title_block_env::edit_sheet(&mut state, sheet);
    let mut harness = themed_harness(state, egui::vec2(1100.0, 700.0), egui::Theme::Light, |ui, state| {
        crate::component_env::ribbon(ui, state);
        crate::component_env::options(ui, state);
        crate::component_env::central(ui, state);
    });
    harness.run();
    harness.snapshot("title_block_editor");
}
```

Run `UPDATE_SNAPSHOTS=1 cargo test -p cc_app snapshot`, review
`title_block_editor.png` (ribbon with Select, Lines/Text/Image, Reference
Plane, Save, Finish/Cancel; preview combo; the paper with the standard
title block), `ribbon_modify.png` (Edit Title Block group) and
`ribbon_view.png` (New Title Block), then commit them.

- [ ] **Step 4: Verify the workspace**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Then run the app once: `cargo run -p cc_app -- examples/casa_simples.ccad`,
open the sheet, *Modify → Edit Title Block*, draw a line on the right edge,
switch the preview to A1 portrait (the line follows), Finish.

- [ ] **Step 5: Commit and open the PR**

```bash
git add crates/cc_app
git commit -s -m "Test the title block editor end to end; update snapshots"
git push origin feat/title-blocks
gh pr create --base main --title "Title blocks: the title block editor (3 of 4)" --body-file <summary file>
```

---

# Delivery 4 — Docs

### Task 15: Interface guide — sheets and title blocks

**Files:**
- Modify: `docs/interface.md` (new section `## Sheets and title blocks` before `## Component modeling environment`, line 222)
- Modify: `docs/interface.pt-BR.md` (`## Folhas e carimbos` before `## Ambiente de modelagem de componentes`, line 222)

- [ ] **Step 1: Write the English section**

```markdown
## Sheets and title blocks

A sheet's properties set its **number**, its **title block** (a title block type of the project, or *None* for blank paper), its **paper** (A0 to A4, or *Custom* with a width × height in millimeters between 50 and 5000) with *Portrait* to put the long side up, and its **issue date** (typed as `YYYY-MM-DD`; *Today* fills it in). *Sheet Fields* lists the title block's per-sheet fields: type a value for this sheet, or leave the row empty to use the title block's own text, shown greyed. When a piece of the active sheet's title block cannot be drawn (an expression that does not resolve), the status bar names it.

**Manage → Project Information** has a *Project fields* table: your own values, such as the engineer or the permit number, each with a name and a value. *+ Add field* adds one, a name is changed by typing over it (Enter), × removes it. Title blocks show them as `{project:Name}`.

A title block draws the whole sheet: border, margins, the block itself, logos. It is a component, so it is saved to and loaded from `.ccomp` files and the component library. **Modify → Edit Title Block** (or *Edit Title Block* in a sheet's right-click menu, in the view or the project browser) opens the active sheet's title block in the editor; **View → New Title Block** makes one from the standard layout and opens it; loading a title block from the library opens it too.

- **The Sheet view:** the editor shows the paper at a preview size: the sheet it opened from, or A0 to A4 in portrait or landscape (*Preview on*, in the options bar). Its four edges are locked reference planes. The wheel zooms, a middle or right drag pans, a middle double-click fits the sheet.
- **Lines:** opens a sketch with the drawing and modify tools of *Sketching outlines*. *Finish* makes a stroke of each chain of curves, closed when its ends meet. A point on an edge is written `SheetWidth`, `SheetHeight` or `0`, and a point on a reference plane as the plane's name, so the stroke follows the paper: a line drawn on the right edge of an A3 stays on the right edge of an A1. Other points are kept to the hundredth of a millimeter. Lengths typed while drawing are lengths on the printed paper. A stroke's properties set its weight (millimeters), color and, when closed, its fill; *Edit Sketch* (or a double click) opens it again.
- **Text:** a click places a text. Its properties set the words, the height, the alignment (left, center or right on the baseline, or the middle of the block), the rotation and the position. *Insert field* adds a field at the cursor: sheet number, name, count and position, scale and paper; project name, number, client, address, author and project fields; today's date and the issue date; and the per-sheet fields. Fields show filled in from the sheet the editor opened from (or sample values); *Show field keys* shows them as typed, such as `{sheet_number}`. `{{` writes a brace.
- **Image:** pick a PNG, JPG or SVG file, then drag its rectangle; the picture keeps its proportions unless Shift is held. Its corner grips move and size it. The picture is kept in the title block (and in its `.ccomp` file).
- **Reference Plane:** two clicks along a line make a plane square to the paper. Its position is an expression in millimeters that may read the paper (`SheetWidth - 10`).
- **Parameters:** numbers (in millimeters) and texts. *New per-sheet field* adds a text that each sheet may set in its *Sheet Fields*; the parameter's text is the default. *Sheet width* and *Sheet height* show the preview's paper.
- **Selecting:** click a piece, or drag a window around several; Delete removes them. A picked text or picture shows grips.
- *Finish* keeps the changes and *Cancel Editing* undoes everything done in the editor. Every sheet that uses the title block follows.
```

- [ ] **Step 2: Write the Portuguese section**

```markdown
## Folhas e carimbos

As propriedades de uma folha definem o **número**, o **carimbo** (um tipo de carimbo do projeto, ou *Nenhum* para papel em branco), o **papel** (A0 a A4, ou *Personalizado*, com largura × altura em milímetros entre 50 e 5000), com *Retrato* para deixar o lado maior na vertical, e a **data de emissão** (digitada como `AAAA-MM-DD`; *Hoje* preenche a data de hoje). *Campos da folha* lista os campos por folha do carimbo: digite um valor para esta folha, ou deixe a linha vazia para usar o texto do próprio carimbo, mostrado em cinza. Quando uma peça do carimbo da folha ativa não pode ser desenhada (uma expressão que não se resolve), a barra de status a indica.

**Gerenciar → Informações do projeto** tem uma tabela *Campos do projeto*: valores seus, como o engenheiro ou o número do alvará, cada um com nome e valor. *+ Adicionar campo* acrescenta um, o nome muda ao digitar por cima (Enter), × remove. Os carimbos os mostram como `{project:Nome}`.

O carimbo desenha a folha inteira: borda, margens, o próprio carimbo, logotipos. Ele é um componente, então é salvo e carregado em arquivos `.ccomp` e na biblioteca de componentes. **Modificar → Editar carimbo** (ou *Editar carimbo* no menu do botão direito de uma folha, na vista ou no navegador do projeto) abre o carimbo da folha ativa no editor; **Vista → Novo carimbo** cria um a partir do modelo padrão e o abre; carregar um carimbo da biblioteca também o abre.

- **A vista Folha:** o editor mostra o papel num tamanho de visualização: o da folha de onde foi aberto, ou A0 a A4 em retrato ou paisagem (*Visualizar em*, na barra de opções). As quatro bordas são planos de referência travados. A roda aproxima, arrastar com o botão do meio ou o direito move, clique duplo com o botão do meio enquadra a folha.
- **Linhas:** abre um esboço com as ferramentas de desenho e modificação de *Esboço de contornos*. *Concluir* cria um traço para cada cadeia de curvas, fechado quando as pontas se encontram. Um ponto sobre uma borda é gravado como `SheetWidth`, `SheetHeight` ou `0`, e um ponto sobre um plano de referência como o nome do plano, então o traço acompanha o papel: uma linha desenhada na borda direita de uma A3 continua na borda direita de uma A1. Os demais pontos ficam com precisão de centésimo de milímetro. Comprimentos digitados durante o desenho são medidas no papel impresso. As propriedades de um traço definem a espessura (milímetros), a cor e, quando fechado, o preenchimento; *Editar esboço* (ou clique duplo) o reabre.
- **Texto:** um clique coloca um texto. As propriedades definem as palavras, a altura, o alinhamento (esquerda, centro ou direita na linha de base, ou o centro do bloco), a rotação e a posição. *Inserir campo* acrescenta um campo no cursor: número, nome, total e posição da folha, escala e papel; nome, número, cliente, endereço e autor do projeto e os campos do projeto; a data de hoje e a de emissão; e os campos por folha. Os campos aparecem preenchidos pela folha de onde o editor foi aberto (ou por valores de exemplo); *Mostrar chaves dos campos* os mostra como digitados, como `{sheet_number}`. `{{` escreve uma chave.
- **Imagem:** escolha um arquivo PNG, JPG ou SVG e arraste o retângulo; a imagem mantém as proporções, a não ser com Shift pressionado. As alças dos cantos a movem e redimensionam. A imagem fica guardada no carimbo (e no seu arquivo `.ccomp`).
- **Plano de referência:** dois cliques ao longo de uma linha criam um plano em esquadro com o papel. A posição é uma expressão em milímetros que pode ler o papel (`SheetWidth - 10`).
- **Parâmetros:** números (em milímetros) e textos. *Novo campo por folha* acrescenta um texto que cada folha pode definir em *Campos da folha*; o texto do parâmetro é o padrão. *Largura da folha* e *Altura da folha* mostram o papel da visualização.
- **Seleção:** clique numa peça, ou arraste uma janela em volta de várias; Delete as remove. Um texto ou imagem selecionado mostra alças.
- *Concluir* mantém as alterações e *Cancelar edição* desfaz tudo o que foi feito no editor. Todas as folhas que usam o carimbo acompanham.
```

Check the exact names of the menus used here against the app (`Manage`
tab name, `Project Information` label `dlg-project-info`, the sketch
section's title in each language) and adjust.

- [ ] **Step 3: Commit**

```bash
git add docs/interface.md docs/interface.pt-BR.md
git commit -s -m "Document sheets and the title block editor"
```

---

### Task 16: Automation guide — title block commands, and the PR

**Files:**
- Modify: `docs/automation.md` (new `### Sheets and title blocks` after `### Curtain walls`, before the paragraph "Scripts have no variables…" at line 190)
- Modify: `docs/automacao.md` (`### Folhas e carimbos` at the same place)

- [ ] **Step 1: Write the English section**

````markdown
### Sheets and title blocks

`sheet_set` changes a sheet: `number`, `size` (`"A0"`…`"A4"`, or `{"width": …, "height": …}` in millimeters), `portrait`, `title_block` (a title block type, or `null` for none), `fields` (the sheet's values of the title block's per-sheet fields; `null` clears one) and `issue_date` (`YYYY-MM-DD`, or `""`). `sheet_fields` shows what a sheet's title block will read: every field with its value, the per-sheet fields with this sheet's value and the default, and the pieces that do not resolve. `project_info_set` takes `fields` (project fields by name, read as `{project:Name}`).

A title block is a component whose 2D pieces draw the whole sheet, in millimeters from the paper's bottom-left corner. `title_block_create` copies the standard one (or starts `empty`); `component_sheet_piece_add`, `_set` (a merge patch over the piece as `component_get` shows it) and `_delete` edit its strokes, texts and pictures. Expressions read the parameters and `SheetWidth`/`SheetHeight`, so one title block serves every paper; texts hold `{fields}` (`sheet_number`, `sheet_name`, `sheet_count`, `sheet_index`, `sheet_scale`, `paper`, `project_name`, `project_number`, `client`, `address`, `author`, `date`, `issue_date`, `project:<Name>`, `param:<Name>`). A per-sheet field is a text parameter with `instance`.

```text
{"command": "title_block_create", "args": {"name": "Office"}}
{"command": "component_param_add", "args": {"type": "Office", "name": "Drawn", "kind": "text", "instance": true, "text": "Team"}}
{"command": "component_sheet_piece_add", "args": {"type": "Office", "piece": {"kind": "text", "x": "SheetWidth - 60", "y": 8, "text": "{param:Drawn} · {sheet_index}/{sheet_count}", "size": 2.5}}}
{"command": "component_sheet_piece_add", "args": {"type": "Office", "piece": {"kind": "image", "x": 15, "y": 15, "width": 30, "height": 12, "image_path": "logo.png"}}}
{"command": "view_create", "args": {"kind": "sheet", "name": "Plans"}}
{"command": "sheet_set", "args": {"sheet": "Plans", "title_block": "Office", "size": "A1", "issue_date": "2026-11-06", "fields": {"Drawn": "Ana"}}}
{"command": "sheet_fields", "args": {"sheet": "Plans"}}
```
````

- [ ] **Step 2: Write the Portuguese section**

````markdown
### Folhas e carimbos

`sheet_set` altera uma folha: `number`, `size` (`"A0"`…`"A4"`, ou `{"width": …, "height": …}` em milímetros), `portrait`, `title_block` (um tipo de carimbo, ou `null` para nenhum), `fields` (os valores da folha para os campos por folha do carimbo; `null` limpa um) e `issue_date` (`AAAA-MM-DD`, ou `""`). `sheet_fields` mostra o que o carimbo da folha vai ler: cada campo com seu valor, os campos por folha com o valor desta folha e o padrão, e as peças que não se resolvem. `project_info_set` aceita `fields` (campos do projeto por nome, lidos como `{project:Nome}`).

Um carimbo é um componente cujas peças 2D desenham a folha inteira, em milímetros a partir do canto inferior esquerdo do papel. `title_block_create` copia o padrão (ou começa vazio com `empty`); `component_sheet_piece_add`, `_set` (um merge patch sobre a peça como `component_get` a mostra) e `_delete` editam seus traços, textos e imagens. As expressões leem os parâmetros e `SheetWidth`/`SheetHeight`, então um carimbo serve para qualquer papel; os textos têm `{campos}` (`sheet_number`, `sheet_name`, `sheet_count`, `sheet_index`, `sheet_scale`, `paper`, `project_name`, `project_number`, `client`, `address`, `author`, `date`, `issue_date`, `project:<Nome>`, `param:<Nome>`). Um campo por folha é um parâmetro de texto com `instance`.

```text
{"command": "title_block_create", "args": {"name": "Escritório"}}
{"command": "component_param_add", "args": {"type": "Escritório", "name": "Desenho", "kind": "text", "instance": true, "text": "Equipe"}}
{"command": "component_sheet_piece_add", "args": {"type": "Escritório", "piece": {"kind": "text", "x": "SheetWidth - 60", "y": 8, "text": "{param:Desenho} · {sheet_index}/{sheet_count}", "size": 2.5}}}
{"command": "component_sheet_piece_add", "args": {"type": "Escritório", "piece": {"kind": "image", "x": 15, "y": 15, "width": 30, "height": 12, "image_path": "logo.png"}}}
{"command": "view_create", "args": {"kind": "sheet", "name": "Plantas"}}
{"command": "sheet_set", "args": {"sheet": "Plantas", "title_block": "Escritório", "size": "A1", "issue_date": "2026-11-06", "fields": {"Desenho": "Ana"}}}
{"command": "sheet_fields", "args": {"sheet": "Plantas"}}
```
````

- [ ] **Step 3: Check the example runs**

Write the English lines to a scratch file with a 1×1 `logo.png` next to it
and run `cargo run -p cc_cli -- script <file>` from that folder; every step
must succeed. Fix argument names that the engine reports.

- [ ] **Step 4: Verify the reference and the workspace**

```bash
cargo run -p cc_cli -- reference > docs/commands.md
git diff --exit-code docs/commands.md
cargo test --workspace
```

`docs/commands.md` must not change (delivery 1 regenerated it).

- [ ] **Step 5: Commit and open the PR**

```bash
git add docs/automation.md docs/automacao.md
git commit -s -m "Document the title block commands"
git push origin feat/title-blocks
gh pr create --base main --title "Title blocks: docs (4 of 4)" --body-file <summary file>
```

---

## Self-review against the spec

| Spec item (App, Delivery) | Task |
|---|---|
| Sheet properties: title block combo with None | 2 |
| Paper combo A0–A4/Custom, Portrait, width × height | 2 |
| Issue date validated, *Today* | 2 |
| One row per per-sheet field, hint = type default, clearing returns to default | 3 |
| Each change runs `sheet_set` through `engine_edit` | 1-3 |
| Project Information *Project fields* (add, rename, remove) via `project_info_set` | 4 |
| Status bar shows the first title block error of the active sheet | 3 |
| *Edit Title Block* (Modify tab, sheet context menu), library, *New Title Block* | 8 |
| Single *Sheet* view, preview sizes from the options bar, four locked edge planes | 7 |
| Sketch tools write `Stroke` pieces; points on edges/planes become expressions; closed loops closed | 10 |
| Weight, color, fill in the palette | 11 |
| Text tool, palette (text, size, anchor, rotation), Insert field (grouped, translated), fields resolved or sample, raw toggle | 7, 12 |
| Image tool (PNG/JPG/SVG, drag, aspect unless Shift, grips resize) | 13 |
| Parameters panel: *New per-sheet field*, Sheet width/height read-only | 7 |
| Pick, window, grips, Delete on 2D pieces | 11, 13 |
| Finish/Cancel as today; every change a `component_*` command | 7-13 |
| UI test: Edit Title Block, rectangle, `{sheet_number}`, Finish, sheet shows it | 14 |
| Sheet properties rows run `sheet_set` (test) | 1, 2 |
| Snapshot updated | 5, 14 |
| i18n in 12 languages | 2-4, 7-13 |
| Docs: interface (EN, pt-BR), automation (EN, pt-BR) | 15, 16 |
