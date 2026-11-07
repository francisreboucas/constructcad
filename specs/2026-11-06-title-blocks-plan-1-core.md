# Title Blocks — Delivery 1 (core and engine) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sheets draw an editable title block component. It reads the sheet
size, and its texts fill `{fields}` from the sheet, the project and per-sheet
values. It can be created and edited through engine commands, so the CLI,
MCP and Lua can drive it. The app UI comes in deliveries 2 and 3.

**Architecture:**
- A title block is a `ComponentKind::TitleBlock` component whose 2D pieces
  live in `ComponentDef::sheet`. The 3D code never reads that list.
- `cc_core::title_block` resolves the pieces in paper millimeters with
  `SheetWidth`/`SheetHeight` defined.
- `cc_views::sheet_fields` expands `{keys}`.
- `cc_views::sheet::compose` draws the resolved block in place of the
  hard-coded one.
- The engine provides `sheet_set`, `sheet_fields`, `title_block_create` and
  `component_sheet_piece_*`.
- Old files migrate to a standard title block (format 22).

**Tech Stack:** Rust 2024 workspace; serde/RON; schemars; egui only later.

**Spec:** `specs/2026-11-06-title-blocks-design.md` (read it first; this
plan implements its "Model", "Fields", "Engine", "Robustness" and
"Testing" sections except the cc_app items).

## Global Constraints

- Non-test code never panics: no `unwrap`, `expect`, `panic!`,
  `unreachable!`, `todo!` (clippy denies them; tests may use them with
  `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` in
  integration test files).
- No `unsafe`.
- Files are hostile input: every new field gets a serde default. `repair`
  fixes or drops invalid data and counts each fix. Loops over file data
  are bounded.
- Formatting: `cargo fmt --all` (max_width 120).
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`.
- Every engine command parameter has a doc comment.
  `cargo run -p cc_cli -- reference > docs/commands.md` is regenerated (a
  test checks it).
- No hard-coded UI text: new keys go in all 12 `assets/i18n/*.ftl` files
  (pt-BR is the reference). The i18n tests check keys and placeholders.
- Code, comments and commit messages are in English. Commits are signed off
  (`git commit -s`).
- Work in the worktree `D:\constructcad-titleblocks`, branch
  `feat/title-blocks`.
- Units: title block coordinates, sizes and pen weights are **paper
  millimeters**. Title block numeric parameters use `ParamKind::Number`.
- Limits (exact values): `MAX_SHEET_PIECES = 2000`,
  `MAX_STROKE_POINTS = 2000`, `MAX_SHEET_TEXT = 4000` characters, custom
  paper `MIN_PAPER_MM = 50.0` to `MAX_PAPER_MM = 5000.0`.
- Context names: `SheetWidth`, `SheetHeight` (constants
  `SHEET_WIDTH`/`SHEET_HEIGHT` in `cc_core::title_block`).
- `FORMAT_VERSION` 20 → 22 (main took 21).

## File Structure

| File | Responsibility |
|---|---|
| `crates/cc_core/src/title_block.rs` (new) | `SheetItem`, `SheetShape`, `TextAnchor`, limits, `resolve_sheet`, `standard()` layout, repair helpers |
| `crates/cc_core/src/component.rs` | `ComponentKind::TitleBlock`, `ComponentDef::sheet`, `category()`, `rename` over sheet pieces |
| `crates/cc_core/src/types.rs` | `Category::TitleBlock` (not in `ALL`/`MODEL`/`SCHEDULABLE`), its key |
| `crates/cc_core/src/view.rs` | `SheetSize::Custom`, `ViewKind::Sheet` new fields, `View::paper_mm()` |
| `crates/cc_core/src/project.rs` | `ProjectInfo::fields`, `ProjectField`, `sheets_in_order`, template title block |
| `crates/cc_core/src/repair.rs` | sheet, project field and title block repairs; migration hook |
| `crates/cc_core/src/persist.rs` | format 22, `from_ron_with_repairs_localized` |
| `crates/cc_views/src/sheet_fields.rs` (new) | `FieldCtx`, `expand`, `sheet_scale`, `paper_label` |
| `crates/cc_views/src/sheet.rs` | `SheetEnv`, compose draws the title block |
| `crates/cc_views/src/drawing.rs`, `export.rs`, `dxf.rs`, `crates/cc_app/src/paint.rs` | `Anchor::RightBottom` |
| `crates/cc_engine/src/i18n.rs` | `format_date`, `today()` |
| `crates/cc_engine/src/session.rs` | builds `SheetEnv`; `open` uses the localized load |
| `crates/cc_engine/src/commands/sheets.rs` (new) | `sheet_set`, `sheet_fields`, `title_block_create`, `component_sheet_piece_add/set/delete` |
| `crates/cc_engine/src/commands/project.rs` | `project_info_set`/`project_info` fields |
| `crates/cc_engine/src/commands/views.rs` | new sheets get a title block; paper via `paper_mm` |
| `crates/cc_engine/src/commands/types.rs` | `validate` accepts title blocks |
| `crates/cc_engine/tests/title_blocks.rs` (new) | engine integration tests |

---

### Task 1: Paper size, orientation and custom sizes

**Files:**
- Modify: `crates/cc_core/src/view.rs` (`SheetSize`, `ViewKind::Sheet`, `impl View`)
- Modify: `crates/cc_core/src/repair.rs`
- Modify: `crates/cc_views/src/sheet.rs:52-53`, `crates/cc_engine/src/commands/views.rs:200,436-437`
- Modify: `crates/cc_core/examples/casa_simples.rs:206`, `crates/cc_core/src/project.rs:383` (struct literals)
- Test: `crates/cc_core/src/view.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `SheetSize::Custom { width: f64, height: f64 }`;
  `ViewKind::Sheet { number, size, portrait: bool, viewports, title_block: Option<ElementTypeId>, fields: BTreeMap<String, String>, issue_date: String }`
  (all three trailing fields are added now with serde defaults, so the
  struct changes once); `View::paper_mm(&self) -> Option<DVec2>`;
  `SheetSize::label(self) -> String` (now `String`, since custom sizes are
  formatted); `cc_core::view::{MIN_PAPER_MM, MAX_PAPER_MM}`.

- [ ] **Step 1: Write the failing test** (in `view.rs` tests module; create it if absent)

```rust
#[cfg(test)]
mod paper_tests {
    use super::*;

    fn sheet(size: SheetSize, portrait: bool) -> View {
        let kind = ViewKind::Sheet {
            number: "A1".into(),
            size,
            portrait,
            viewports: Vec::new(),
            title_block: None,
            fields: Default::default(),
            issue_date: String::new(),
        };
        View::new(ViewId(1), "S", 100, kind)
    }

    #[test]
    fn paper_follows_size_and_orientation() {
        assert_eq!(sheet(SheetSize::A3, false).paper_mm(), Some(DVec2::new(420.0, 297.0)));
        assert_eq!(sheet(SheetSize::A3, true).paper_mm(), Some(DVec2::new(297.0, 420.0)));
        let custom = SheetSize::Custom { width: 1000.0, height: 594.0 };
        assert_eq!(sheet(custom, false).paper_mm(), Some(DVec2::new(1000.0, 594.0)));
        // Portrait puts the long side up whatever the order typed.
        assert_eq!(sheet(custom, true).paper_mm(), Some(DVec2::new(594.0, 1000.0)));
        assert_eq!(custom.label(), "1000 × 594 mm");
        assert_eq!(SheetSize::A1.label(), "A1");
    }

    #[test]
    fn old_sheets_read_with_defaults() {
        let ron = r#"Sheet(number: "A101", size: A3, viewports: [])"#;
        let kind: ViewKind = ron::from_str(ron).unwrap();
        let ViewKind::Sheet { portrait, title_block, fields, issue_date, .. } = kind else { panic!() };
        assert!(!portrait && title_block.is_none() && fields.is_empty() && issue_date.is_empty());
    }
}
```

If `View::new` doesn't exist with that signature, use the constructor
`crates/cc_core/src/view.rs` already offers. Check with
`grep -n "pub fn new" crates/cc_core/src/view.rs`, or build the struct the
way `project.rs:383` does.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p cc_core paper_tests`
Expected: compile errors (no `Custom`, `portrait`, `paper_mm`).

- [ ] **Step 3: Implement**

In `view.rs`:

```rust
/// Smallest and largest side of a custom sheet, millimeters.
pub const MIN_PAPER_MM: f64 = 50.0;
pub const MAX_PAPER_MM: f64 = 5000.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SheetSize {
    A0,
    A1,
    A2,
    A3,
    A4,
    /// Any size, millimeters (width and height as typed; `portrait` puts the long side up).
    Custom { width: f64, height: f64 },
}
```

`Eq` must go from the derive (f64). Fix any `Eq`/`Hash` users:
`cargo build -p cc_core` lists them.

```rust
impl SheetSize {
    /// Landscape paper size in millimeters (a custom size as typed).
    pub fn size_mm(self) -> DVec2 {
        match self {
            SheetSize::A0 => DVec2::new(1189.0, 841.0),
            SheetSize::A1 => DVec2::new(841.0, 594.0),
            SheetSize::A2 => DVec2::new(594.0, 420.0),
            SheetSize::A3 => DVec2::new(420.0, 297.0),
            SheetSize::A4 => DVec2::new(297.0, 210.0),
            SheetSize::Custom { width, height } => DVec2::new(width, height),
        }
    }

    pub fn label(self) -> String {
        match self {
            SheetSize::A0 => "A0".into(),
            SheetSize::A1 => "A1".into(),
            SheetSize::A2 => "A2".into(),
            SheetSize::A3 => "A3".into(),
            SheetSize::A4 => "A4".into(),
            SheetSize::Custom { width, height } => format!("{} × {} mm", width.round(), height.round()),
        }
    }

    /// A custom size within the limits, finite.
    pub fn is_valid(self) -> bool {
        match self {
            SheetSize::Custom { width, height } => {
                [width, height].iter().all(|v| v.is_finite() && (MIN_PAPER_MM..=MAX_PAPER_MM).contains(v))
            }
            _ => true,
        }
    }
}
```

`ViewKind::Sheet` gains its new fields:

```rust
    Sheet {
        number: String,
        size: SheetSize,
        /// The long side runs up.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        portrait: bool,
        viewports: Vec<Viewport>,
        /// The title block type drawn on it; `None` draws the blank paper.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title_block: Option<crate::ElementTypeId>,
        /// Values of the title block's per-sheet fields, by parameter name.
        #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        fields: std::collections::BTreeMap<String, String>,
        /// Issue date, ISO `YYYY-MM-DD`; empty when not issued.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        issue_date: String,
    },
```

In `impl View`:

```rust
    /// A sheet's paper size in millimeters, after its orientation.
    pub fn paper_mm(&self) -> Option<DVec2> {
        let ViewKind::Sheet { size, portrait, .. } = &self.kind else { return None };
        let s = size.size_mm();
        let (long, short) = (s.x.max(s.y), s.x.min(s.y));
        Some(match (size, portrait) {
            (SheetSize::Custom { .. }, false) => s,
            (_, true) => DVec2::new(short, long),
            (_, false) => DVec2::new(long, short),
        })
    }
```

Update the struct literals: `views.rs:200` (new sheet, `title_block` set in
Task 8), `project.rs:383` and `casa_simples.rs:206`. Add
`portrait: false, title_block: None, fields: Default::default(), issue_date: String::new()`.
Replace `size.size_mm()` with the view's `paper_mm()`. In
`cc_views/src/sheet.rs` it becomes
`let paper = sheet.paper_mm().unwrap_or(DVec2::new(420.0, 297.0));`. In
`views.rs:436`, read `paper_mm()` before the edit. Fix every `.label()` caller
for the `String` return: `cargo build --workspace` lists them. They're all in
cc_app and need `&size.label()` or `.label().as_str()`.

In `repair.rs`, inside the loop over views (find it with
`grep -n "views" crates/cc_core/src/repair.rs`), add:

```rust
if let ViewKind::Sheet { size, .. } = &mut view.kind
    && !size.is_valid()
{
    *size = SheetSize::A3;
    fixes += 1;
}
```

Use the counter name the surrounding code uses.

- [ ] **Step 4: Add a repair test** (in `repair.rs` tests)

```rust
#[test]
fn bad_custom_paper_resets_to_a3() {
    let mut p = Project::from_template();
    let sheet = p.views.values_mut().find(|v| matches!(v.kind, ViewKind::Sheet { .. }));
    let sheet = match sheet { Some(s) => s, None => return };
    if let ViewKind::Sheet { size, .. } = &mut sheet.kind {
        *size = SheetSize::Custom { width: f64::NAN, height: 10.0 };
    }
    assert!(p.repair() >= 1);
    assert!(p.views.values().any(|v| matches!(v.kind, ViewKind::Sheet { size: SheetSize::A3, .. })));
}
```

If the template has no sheet, insert one in the test with the literal from
Step 1.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p cc_core paper_tests bad_custom_paper && cargo build --workspace`
Expected: PASS, and the workspace builds.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -s -m "Sheets take portrait and custom paper sizes"
```

---

### Task 2: Project fields

**Files:**
- Modify: `crates/cc_core/src/project.rs:12-21` (`ProjectInfo`), `crates/cc_core/src/repair.rs`
- Modify: `crates/cc_engine/src/commands/project.rs:108-150` (`ProjectInfoSet`), and `ProjectInfoGet`'s output
- Test: `crates/cc_core/src/repair.rs` tests; `crates/cc_engine/tests/title_blocks.rs` (create)

**Interfaces:**
- Produces: `pub struct ProjectField { pub name: String, pub value: String }`
  (serde, Clone, PartialEq, Debug, Default); `ProjectInfo::fields: Vec<ProjectField>`;
  `ProjectInfo::field(&self, name: &str) -> Option<&str>`. `project_info_set`
  takes `fields: Option<serde_json::Map<String, Value>>` (a string sets, `null`
  removes). `project_info` returns `"fields": {name: value}`.

- [ ] **Step 1: Write the failing tests**

`crates/cc_engine/tests/title_blocks.rs`:

```rust
//! Title blocks: sheet paper, project and sheet fields, title block
//! components and their pieces, from engine commands.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::{ErrorCode, Session};
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

#[test]
fn project_fields_are_set_renamed_and_removed() {
    let mut s = Session::default();
    run(&mut s, "project_info_set", json!({"fields": {"Responsável técnico": "Ana", "CAU": "A123"}}));
    let info = run(&mut s, "project_info", json!({}));
    assert_eq!(info["fields"]["Responsável técnico"], "Ana");
    run(&mut s, "project_info_set", json!({"fields": {"CAU": null, "Etapa": "Executivo"}}));
    let info = run(&mut s, "project_info", json!({}));
    assert!(info["fields"].get("CAU").is_none());
    assert_eq!(info["fields"]["Etapa"], "Executivo");
    assert_eq!(fails(&mut s, "project_info_set", json!({"fields": {"  ": "x"}})), ErrorCode::InvalidArgument);
}
```

Check the `ErrorCode` variant names with
`grep -n "pub enum ErrorCode" -A 15 crates/cc_engine/src/error.rs` and use the
one that `EngineError::invalid` produces. Also check that the query
command's name is `project_info` (it is listed in the registry as
`ProjectInfoGet`).

In `repair.rs` tests:

```rust
#[test]
fn project_fields_drop_empty_and_repeated_names() {
    let mut p = Project::from_template();
    p.info.fields = vec![
        ProjectField { name: "A".into(), value: "1".into() },
        ProjectField { name: " ".into(), value: "2".into() },
        ProjectField { name: "A".into(), value: "3".into() },
    ];
    assert_eq!(p.repair(), 2);
    assert_eq!(p.info.fields, vec![ProjectField { name: "A".into(), value: "1".into() }]);
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p cc_core project_fields; cargo test -p cc_engine --test title_blocks`
Expected: compile errors.

- [ ] **Step 3: Implement**

`project.rs`:

```rust
/// A project value named by the user (shown by title blocks as `{project:Name}`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProjectField {
    pub name: String,
    pub value: String,
}
```

Add `#[serde(default, skip_serializing_if = "Vec::is_empty")] pub fields: Vec<ProjectField>,`
to `ProjectInfo`, plus:

```rust
impl ProjectInfo {
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields.iter().find(|f| f.name == name).map(|f| f.value.as_str())
    }
}
```

Fix the `ProjectInfo { .. }` literal in `from_template_localized`
(`project.rs:116`) with `fields: Vec::new()`.

`repair.rs`, in `Project::repair`:

```rust
let before = self.info.fields.len();
let mut seen = std::collections::BTreeSet::new();
self.info.fields.retain(|f| !f.name.trim().is_empty() && seen.insert(f.name.clone()));
fixes += before - self.info.fields.len();
```

`commands/project.rs`, `ProjectInfoSet` gains:

```rust
    /// Project fields by name: a text sets the field (adding it), `null` removes it.
    #[serde(default)]
    pub fields: Option<serde_json::Map<String, Value>>,
```

In `run`, before `edit`, validate:

```rust
let mut changes: Vec<(String, Option<String>)> = Vec::new();
for (name, value) in self.fields.iter().flatten() {
    let name = name.trim();
    if name.is_empty() {
        return Err(EngineError::invalid("a project field needs a name"));
    }
    let value = match value {
        Value::Null => None,
        Value::String(v) => Some(v.clone()),
        other => return Err(EngineError::invalid(format!("field '{name}': expected text or null, got {other}"))),
    };
    changes.push((name.to_string(), value));
}
```

Then inside the closure, after the fixed fields:

```rust
for (name, value) in changes {
    match (info.fields.iter().position(|f| f.name == name), value) {
        (Some(i), Some(v)) => info.fields[i].value = v,
        (Some(i), None) => {
            info.fields.remove(i);
        }
        (None, Some(v)) => info.fields.push(cc_core::ProjectField { name, value: v }),
        (None, None) => {}
    }
}
```

Use `get_mut`, not indexing, if clippy's indexing lint is on. Add the doc
comment line to the struct's description. In `ProjectInfoGet`'s output, add
`"fields": info.fields.iter().map(|f| (f.name.clone(), json!(f.value))).collect::<serde_json::Map<_, _>>()`.
Export `ProjectField` from `cc_core/src/lib.rs`, next to `ProjectInfo`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cc_core project_fields && cargo test -p cc_engine --test title_blocks`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -s -m "Add user-defined project fields"
```

---

### Task 3: Title block model and 2D resolution

**Files:**
- Create: `crates/cc_core/src/title_block.rs` (register `pub mod title_block;` in `lib.rs`)
- Modify: `crates/cc_core/src/component.rs` (`ComponentKind`, `ComponentDef`, `category`, `rename`)
- Modify: `crates/cc_core/src/types.rs` (`Category::TitleBlock`, `key()`)
- Modify: `crates/cc_core/src/repair.rs` (call `title_block::repair_def`)
- Test: in `title_block.rs`

**Interfaces:**
- Produces (all in `cc_core::title_block`, re-exported at crate root where noted):
  - `pub const SHEET_WIDTH: &str = "SheetWidth"; pub const SHEET_HEIGHT: &str = "SheetHeight";`
  - `pub const MAX_SHEET_PIECES: usize = 2000; MAX_STROKE_POINTS: usize = 2000; MAX_SHEET_TEXT: usize = 4000;`
  - `pub struct SheetItem { pub name: String, pub shape: SheetShape }`
  - `pub enum SheetShape { Stroke { points: Vec<ProfilePoint>, closed: bool, weight: String, color: Option<[u8; 3]>, fill: Option<[u8; 3]> }, Text { x: String, y: String, text: String, size: String, anchor: TextAnchor, rotation: String }, Image { x: String, y: String, width: String, height: String, image: Arc<RasterImage> } }`
  - `pub enum TextAnchor { #[default] Left, Center, Right, Middle }`
  - `pub struct SheetStroke { pub item: usize, pub points: Vec<DVec2>, pub closed: bool, pub weight: f64, pub color: [u8; 3], pub fill: Option<[u8; 3]> }`
  - `pub struct SheetText { pub item: usize, pub position: DVec2, pub text: String, pub size: f64, pub anchor: TextAnchor, pub rotation_deg: f64 }`
  - `pub struct SheetImage { pub item: usize, pub corner: DVec2, pub size: DVec2, pub image: Arc<RasterImage> }`
  - `pub struct SheetPieces { pub strokes: Vec<SheetStroke>, pub texts: Vec<SheetText>, pub images: Vec<SheetImage>, pub errors: Vec<(usize, PartError)> }`
  - `impl ComponentDef { pub fn resolve_sheet(&self, paper: DVec2) -> SheetPieces }`
  - `pub fn repair_def(def: &mut ComponentDef) -> usize`
  - `ComponentKind::TitleBlock`, `ComponentDef::sheet: Vec<SheetItem>`, `Category::TitleBlock` (key `category-title-block`)

- [ ] **Step 1: Write the failing tests** (bottom of `title_block.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::{ComponentDef, ComponentKind, ComponentParam, ParamKind};
    use crate::profile::ProfilePoint;

    fn line(a: (&str, &str), b: (&str, &str)) -> SheetItem {
        SheetItem {
            name: "L".into(),
            shape: SheetShape::Stroke {
                points: vec![ProfilePoint::new(a.0, a.1), ProfilePoint::new(b.0, b.1)],
                closed: false,
                weight: String::new(),
                color: None,
                fill: None,
            },
        }
    }

    fn block(sheet: Vec<SheetItem>) -> ComponentDef {
        ComponentDef { kind: ComponentKind::TitleBlock, sheet, ..Default::default() }
    }

    #[test]
    fn pieces_follow_the_paper() {
        let def = block(vec![line(("SheetWidth - 10", "10"), ("SheetWidth - 10", "SheetHeight - 10"))]);
        for paper in [DVec2::new(420.0, 297.0), DVec2::new(594.0, 841.0)] {
            let r = def.resolve_sheet(paper);
            assert!(r.errors.is_empty(), "{:?}", r.errors);
            let s = &r.strokes[0];
            assert_eq!(s.points, vec![DVec2::new(paper.x - 10.0, 10.0), DVec2::new(paper.x - 10.0, paper.y - 10.0)]);
            assert_eq!((s.weight, s.color, s.closed), (0.25, [0, 0, 0], false));
        }
    }

    #[test]
    fn params_texts_arcs_and_errors() {
        let mut def = block(vec![
            SheetItem {
                name: "T".into(),
                shape: SheetShape::Text {
                    x: "Margin".into(),
                    y: "20".into(),
                    text: "Folha {sheet_number}".into(),
                    size: "3.5".into(),
                    anchor: TextAnchor::Right,
                    rotation: String::new(),
                },
            },
            // A half circle of radius 10 from (0, 0) to (20, 0).
            SheetItem {
                name: "A".into(),
                shape: SheetShape::Stroke {
                    points: vec![ProfilePoint::arc("0", "0", "10"), ProfilePoint::new("20", "0")],
                    closed: false,
                    weight: "0.5".into(),
                    color: Some([200, 0, 0]),
                    fill: None,
                },
            },
            line(("Nope", "0"), ("1", "1")),
        ]);
        def.params.push(ComponentParam::number("Margin", 15.0, ParamKind::Number));
        let r = def.resolve_sheet(DVec2::new(420.0, 297.0));
        assert_eq!(r.texts[0].position, DVec2::new(15.0, 20.0));
        assert_eq!((r.texts[0].text.as_str(), r.texts[0].size), ("Folha {sheet_number}", 3.5));
        let arc = &r.strokes[0];
        assert!(arc.points.len() > 4, "flattened: {}", arc.points.len());
        assert!(arc.points.iter().all(|p| (p.distance(DVec2::new(10.0, 0.0)) - 10.0).abs() < 1e-6));
        assert_eq!((arc.weight, arc.color), (0.5, [200, 0, 0]));
        assert_eq!(r.errors.len(), 1);
        assert_eq!(r.errors[0].0, 2, "the bad piece is reported by index");
    }

    #[test]
    fn repair_keeps_title_blocks_2d_and_bounded() {
        let mut def = block((0..MAX_SHEET_PIECES + 5).map(|_| line(("0", "0"), ("1", "1"))).collect());
        def.items.push(crate::component::ComponentItem {
            name: "Box".into(),
            shape: crate::component::ComponentShape::Box {
                x: "0".into(),
                y: "0".into(),
                z: "0".into(),
                width: "1".into(),
                depth: "1".into(),
                height: "1".into(),
            },
            material: String::new(),
            in_plan: true,
            void: false,
            plan_only: false,
            detail: Default::default(),
        });
        assert_eq!(repair_def(&mut def), 6);
        assert_eq!((def.sheet.len(), def.items.len()), (MAX_SHEET_PIECES, 0));
        // Other kinds lose sheet pieces.
        let mut other = ComponentDef { sheet: vec![line(("0", "0"), ("1", "1"))], ..Default::default() };
        assert_eq!(repair_def(&mut other), 1);
        assert!(other.sheet.is_empty());
        assert_eq!(block(Vec::new()).category(), crate::Category::TitleBlock);
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p cc_core title_block`
Expected: compile errors.

- [ ] **Step 3: Implement the module**

`crates/cc_core/src/title_block.rs`:

```rust
//! Title blocks: components of kind [`ComponentKind::TitleBlock`] whose 2D
//! pieces ([`ComponentDef::sheet`]) draw a whole sheet (border, title
//! block, logo) in paper millimeters, origin at the paper's bottom-left
//! corner. Expressions read the parameters and [`SHEET_WIDTH`] and
//! [`SHEET_HEIGHT`]; texts keep `{fields}` for the sheet to fill in.

use crate::component::{ComponentDef, ComponentKind, PartError};
use crate::expr;
use crate::profile::{ProfileError, ProfilePoint};
use crate::raster::RasterImage;
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const SHEET_WIDTH: &str = "SheetWidth";
pub const SHEET_HEIGHT: &str = "SheetHeight";
pub const MAX_SHEET_PIECES: usize = 2000;
pub const MAX_STROKE_POINTS: usize = 2000;
pub const MAX_SHEET_TEXT: usize = 4000;
/// Pen width of a stroke without one, millimeters.
pub const DEFAULT_WEIGHT: f64 = 0.25;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetItem {
    pub name: String,
    pub shape: SheetShape,
}

/// Where a text's position sits: on the baseline at its start, middle or
/// end, or at its center.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TextAnchor {
    #[default]
    Left,
    Center,
    Right,
    Middle,
}

/// A 2D piece; every `String` is an expression in millimeters (degrees for rotations).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SheetShape {
    /// Lines and arcs: `points` run straight, or along an arc (`radius`) to the next.
    Stroke {
        points: Vec<ProfilePoint>,
        #[serde(default)]
        closed: bool,
        /// Pen width; empty for [`DEFAULT_WEIGHT`].
        #[serde(default, skip_serializing_if = "String::is_empty")]
        weight: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<[u8; 3]>,
        /// Fill of a closed stroke.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<[u8; 3]>,
    },
    /// Fixed words and `{fields}`; line breaks start new lines.
    Text {
        x: String,
        y: String,
        text: String,
        /// Text height.
        size: String,
        #[serde(default)]
        anchor: TextAnchor,
        /// Degrees counter-clockwise; empty for 0.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        rotation: String,
    },
    /// A picture filling the rectangle from (`x`, `y`) (its bottom-left corner).
    Image { x: String, y: String, width: String, height: String, image: Arc<RasterImage> },
}

impl SheetShape {
    pub fn expressions_mut(&mut self) -> Vec<&mut String> {
        match self {
            SheetShape::Stroke { points, weight, .. } => {
                let mut out: Vec<&mut String> = points.iter_mut().flat_map(ProfilePoint::expressions_mut).collect();
                out.push(weight);
                out
            }
            SheetShape::Text { x, y, size, rotation, .. } => vec![x, y, size, rotation],
            SheetShape::Image { x, y, width, height, .. } => vec![x, y, width, height],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetStroke {
    pub item: usize,
    pub points: Vec<DVec2>,
    pub closed: bool,
    pub weight: f64,
    pub color: [u8; 3],
    pub fill: Option<[u8; 3]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetText {
    pub item: usize,
    pub position: DVec2,
    /// As written: `{fields}` are filled in by the sheet.
    pub text: String,
    pub size: f64,
    pub anchor: TextAnchor,
    pub rotation_deg: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetImage {
    pub item: usize,
    /// Bottom-left corner.
    pub corner: DVec2,
    pub size: DVec2,
    pub image: Arc<RasterImage>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SheetPieces {
    pub strokes: Vec<SheetStroke>,
    pub texts: Vec<SheetText>,
    pub images: Vec<SheetImage>,
    pub errors: Vec<(usize, PartError)>,
}

fn profile_error(e: ProfileError) -> PartError {
    match e {
        ProfileError::Expr { field, error } => PartError::Expr { field, error },
        ProfileError::Degenerate => PartError::Degenerate,
    }
}

impl ComponentDef {
    /// The 2D pieces on a `paper` sheet (millimeters); pieces that fail are
    /// reported with their index and left out.
    pub fn resolve_sheet(&self, paper: DVec2) -> SheetPieces {
        let scope = self.scope(&Default::default(), &[(SHEET_WIDTH, paper.x), (SHEET_HEIGHT, paper.y)]);
        let lookup = |name: &str| scope.get(name);
        let mut out = SheetPieces::default();
        for (i, item) in self.sheet.iter().enumerate().take(MAX_SHEET_PIECES) {
            if let Err(e) = resolve_piece(i, &item.shape, &lookup, &mut out) {
                out.errors.push((i, e));
            }
        }
        out
    }
}

fn resolve_piece(
    i: usize,
    shape: &SheetShape,
    lookup: &dyn Fn(&str) -> Option<f64>,
    out: &mut SheetPieces,
) -> Result<(), PartError> {
    let ev = |field: &'static str, text: &str| expr::eval(text, lookup).map_err(|error| PartError::Expr { field, error });
    let ev_or = |field: &'static str, text: &str, default: f64| {
        if text.trim().is_empty() { Ok(default) } else { ev(field, text) }
    };
    match shape {
        SheetShape::Stroke { points, closed, weight, color, fill } => {
            if points.len() < 2 || points.len() > MAX_STROKE_POINTS {
                return Err(PartError::Degenerate);
            }
            let corners = crate::profile::vertices(points, lookup).map_err(profile_error)?;
            let n = corners.len();
            let segments = if *closed { n } else { n - 1 };
            let mut pts = Vec::with_capacity(n);
            for k in 0..segments {
                let (a, segment) = corners[k];
                let b = corners[(k + 1) % n].0;
                pts.push(a);
                crate::profile::segment_points(a, b, segment, &mut pts).map_err(profile_error)?;
            }
            if !*closed {
                pts.push(corners[n - 1].0);
            }
            let weight = ev_or("weight", weight, DEFAULT_WEIGHT)?;
            if !(weight.is_finite() && weight > 0.0) || pts.iter().any(|p| !p.is_finite()) {
                return Err(PartError::Degenerate);
            }
            out.strokes.push(SheetStroke {
                item: i,
                points: pts,
                closed: *closed,
                weight: weight.min(10.0),
                color: color.unwrap_or([0, 0, 0]),
                fill: if *closed { *fill } else { None },
            });
        }
        SheetShape::Text { x, y, text, size, anchor, rotation } => {
            let position = DVec2::new(ev("x", x)?, ev("y", y)?);
            let size = ev("size", size)?;
            let rotation_deg = ev_or("rotation", rotation, 0.0)?;
            if !(position.is_finite() && size.is_finite() && size > 0.0 && rotation_deg.is_finite()) {
                return Err(PartError::Degenerate);
            }
            out.texts.push(SheetText {
                item: i,
                position,
                text: text.chars().take(MAX_SHEET_TEXT).collect(),
                size,
                anchor: *anchor,
                rotation_deg,
            });
        }
        SheetShape::Image { x, y, width, height, image } => {
            let corner = DVec2::new(ev("x", x)?, ev("y", y)?);
            let size = DVec2::new(ev("width", width)?, ev("height", height)?);
            if !(corner.is_finite() && size.is_finite() && size.min_element() > 0.0) {
                return Err(PartError::Degenerate);
            }
            out.images.push(SheetImage { item: i, corner, size, image: image.clone() });
        }
    }
    Ok(())
}

/// Keep a definition's pieces to its kind and within the limits: title
/// blocks keep only sheet pieces, other kinds none. Returns the fixes.
pub fn repair_def(def: &mut ComponentDef) -> usize {
    let mut fixes = 0;
    if def.kind == ComponentKind::TitleBlock {
        fixes += def.items.len();
        def.items.clear();
        if def.sheet.len() > MAX_SHEET_PIECES {
            fixes += def.sheet.len() - MAX_SHEET_PIECES;
            def.sheet.truncate(MAX_SHEET_PIECES);
        }
        let before = def.sheet.len();
        def.sheet.retain(|item| match &item.shape {
            SheetShape::Stroke { points, .. } => (2..=MAX_STROKE_POINTS).contains(&points.len()),
            SheetShape::Text { text, .. } => text.chars().count() <= MAX_SHEET_TEXT,
            SheetShape::Image { image, .. } => image.is_valid(),
        });
        fixes += before - def.sheet.len();
    } else {
        fixes += def.sheet.len();
        def.sheet.clear();
    }
    fixes
}
```

Notes for the implementer:
- `crate::profile::segment_points` is `pub(crate)` already; `vertices` is `pub`.
- `self.scope(..)` is defined in `component_ref.rs:186` and returns a
  `Scope` with `get(&str) -> Option<f64>`; parameters override extra names.
- `RasterImage::is_valid` exists in `raster.rs:59`.
- `ProfilePoint::new`/`arc` exist in `profile.rs:101,105`. Check that a
  positive `radius` bulges as the test expects. If the half circle comes
  out on the other side the test still passes (distance to the center),
  but make sure the count passes too.

In `component.rs`:
- Add `TitleBlock` to `ComponentKind`, and add it to `ComponentKind::ALL`
  (the array grows to 9). Give it the `key()` arm `"comp-kind-title-block"`
  (find the kind keys with `grep -n "comp-kind" crates/cc_core/src/component.rs`
  and follow their naming). Then add that key with translations to all 12
  `.ftl` files (pt-BR "Carimbo", en "Title block", es "Rótulo", fr
  "Cartouche", de "Schriftfeld", it "Cartiglio", pl "Tabliczka rysunkowa",
  uk-UA "Штамп", ja "表題欄", zh-CN "图签", zh-TW "圖框標題欄", lo "ປ້າຍຊື່ແຜ່ນ").
- Add to `ComponentDef`:

```rust
    /// The 2D pieces of a title block (see [`crate::title_block`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sheet: Vec<crate::title_block::SheetItem>,
```

- In `category()`, put `(ComponentKind::TitleBlock, _) => crate::Category::TitleBlock,` as the first arm.
- In `rename`, after the items loop:

```rust
        for item in &mut self.sheet {
            for e in item.shape.expressions_mut() {
                *e = replace_name(e, old, new);
            }
        }
```

In `types.rs`: add `/// Title blocks: sheet layouts (components drawn on sheets).`
and `TitleBlock,` at the end of `Category`, and the
`Category::TitleBlock => "category-title-block",` key arm. Don't add it to
`ALL`, `MODEL` or `SCHEDULABLE`. Add `category-title-block` to all 12
`.ftl` files (pt-BR "Carimbos", en "Title Blocks", es "Rótulos", fr
"Cartouches", de "Schriftfelder", it "Cartigli", pl "Tabliczki rysunkowe",
uk-UA "Штампи", ja "表題欄", zh-CN "图签", zh-TW "圖框標題欄", lo
"ປ້າຍຊື່ແຜ່ນ"). Then run `cargo build --workspace`, fix every
non-exhaustive `match` on `Category` or `ComponentKind` it reports, and give
`TitleBlock` the arm of the closest annotation-like category (`Text` for
Category, `Generic` for ComponentKind) unless the code says otherwise.

In `repair.rs`, where component types are repaired (`grep -n "TypeData::Component" crates/cc_core/src/repair.rs`),
add `fixes += crate::title_block::repair_def(def);`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cc_core title_block && cargo build --workspace`
Expected: PASS; the workspace builds.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -s -m "Add title block components with 2D sheet pieces"
```

---

### Task 4: Sheets refer to title blocks; sheet order; repairs

**Files:**
- Modify: `crates/cc_core/src/project.rs` (`sheets_in_order`, `title_blocks`), `crates/cc_core/src/repair.rs`
- Test: `crates/cc_core/src/project.rs` tests, `repair.rs` tests

**Interfaces:**
- Consumes: Task 1's `ViewKind::Sheet { title_block, fields, issue_date, .. }`; Task 3's `Category::TitleBlock`.
- Produces: `Project::sheets_in_order(&self) -> Vec<ViewId>`;
  `Project::title_blocks(&self) -> Vec<ElementTypeId>` (types whose
  `category()` is `TitleBlock`, by id); `pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering`
  in `project.rs`.

- [ ] **Step 1: Write the failing tests**

In `project.rs` tests:

```rust
#[test]
fn sheets_sort_by_number_naturally() {
    assert_eq!(natural_cmp("A2", "A10"), std::cmp::Ordering::Less);
    assert_eq!(natural_cmp("A10", "A10"), std::cmp::Ordering::Equal);
    assert_eq!(natural_cmp("B1", "A99"), std::cmp::Ordering::Greater);
    let mut p = Project::from_template();
    p.views.retain(|_, v| !matches!(v.kind, ViewKind::Sheet { .. }));
    for (id, n) in [(9001, "A10"), (9002, "A2"), (9003, "A1")] {
        let kind = ViewKind::Sheet {
            number: n.into(),
            size: SheetSize::A3,
            portrait: false,
            viewports: Vec::new(),
            title_block: None,
            fields: Default::default(),
            issue_date: String::new(),
        };
        p.views.insert(ViewId(id), View::new(ViewId(id), n, 100, kind));
    }
    assert_eq!(p.sheets_in_order(), vec![ViewId(9003), ViewId(9002), ViewId(9001)]);
}
```

Use the same `View` constructor as in Task 1. If `p.views` isn't a
`BTreeMap<ViewId, View>`, adapt the insert to the real container (check
with `grep -n "pub views" crates/cc_core/src/project.rs`).

In `repair.rs` tests:

```rust
#[test]
fn sheets_drop_title_blocks_that_are_gone_or_not_title_blocks() {
    let mut p = Project::from_template();
    let wall_type = p.types.values().find(|t| t.category() == Category::Wall).map(|t| t.id).unwrap();
    let mut n = 0;
    for v in p.views.values_mut() {
        if let ViewKind::Sheet { title_block, .. } = &mut v.kind {
            *title_block = Some(wall_type);
            n += 1;
        }
    }
    let fixes = p.repair();
    assert!(fixes >= n);
    for v in p.views.values() {
        if let ViewKind::Sheet { title_block, .. } = &v.kind {
            assert!(title_block.is_none_or(|t| p.title_blocks().contains(&t)));
        }
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p cc_core sheets_`
Expected: compile errors.

- [ ] **Step 3: Implement**

`project.rs`:

```rust
/// Compare texts with their digit runs as numbers (A2 before A10).
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        s.push(c);
                        it.next();
                    }
                    s
                };
                let (m, n) = (take(&mut x), take(&mut y));
                let (m, n) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                let o = m.len().cmp(&n.len()).then_with(|| m.cmp(n));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                let o = c.cmp(&d);
                if o != Ordering::Equal {
                    return o;
                }
                x.next();
                y.next();
            }
        }
    }
}
```

In `impl Project`:

```rust
    /// The sheets by number (natural order), then id.
    pub fn sheets_in_order(&self) -> Vec<ViewId> {
        let mut sheets: Vec<(&str, ViewId)> = self
            .views
            .values()
            .filter_map(|v| match &v.kind {
                ViewKind::Sheet { number, .. } => Some((number.as_str(), v.id)),
                _ => None,
            })
            .collect();
        sheets.sort_by(|a, b| natural_cmp(a.0, b.0).then(a.1.cmp(&b.1)));
        sheets.into_iter().map(|(_, id)| id).collect()
    }

    /// The title block types, by id.
    pub fn title_blocks(&self) -> Vec<ElementTypeId> {
        self.types.values().filter(|t| t.category() == Category::TitleBlock).map(|t| t.id).collect()
    }
```

`ViewId` must derive `Ord`; it does if it's used as a `BTreeMap` key.

`repair.rs`, in the views loop (after Task 1's paper fix). Compute
`let title_blocks = self.title_blocks();` before the loop, so the borrow
checker doesn't complain about `self.types` inside it:

```rust
if let ViewKind::Sheet { title_block, issue_date, fields, .. } = &mut view.kind {
    if title_block.is_some_and(|t| !title_blocks.contains(&t)) {
        *title_block = title_blocks.first().copied();
        fixes += 1;
    }
    if !issue_date.is_empty() && !crate::title_block::is_iso_date(issue_date) {
        issue_date.clear();
        fixes += 1;
    }
    let before = fields.len();
    fields.retain(|k, v| !k.trim().is_empty() && v.chars().count() <= crate::title_block::MAX_SHEET_TEXT);
    fixes += before - fields.len();
}
```

Add to `title_block.rs`:

```rust
/// Whether `s` is a real `YYYY-MM-DD` date.
pub fn is_iso_date(s: &str) -> bool {
    parse_iso_date(s).is_some()
}

/// Year, month and day of a `YYYY-MM-DD` date.
pub fn parse_iso_date(s: &str) -> Option<(i32, u32, u32)> {
    let mut it = s.trim().splitn(3, '-');
    let y: i32 = it.next()?.parse().ok()?;
    let m: u32 = it.next()?.parse().ok()?;
    let d: u32 = it.next()?.parse().ok()?;
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    ((1..=9999).contains(&y) && (1..=days).contains(&d)).then_some((y, m, d))
}
```

and a test in `title_block.rs`:

```rust
#[test]
fn iso_dates() {
    assert_eq!(parse_iso_date("2026-11-06"), Some((2026, 11, 6)));
    assert!(is_iso_date("2024-02-29") && !is_iso_date("2026-02-29") && !is_iso_date("06/11/2026"));
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cc_core sheets_ iso_dates`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -s -m "Sheets refer to title block types; natural sheet order"
```

---

### Task 5: Field expansion

**Files:**
- Create: `crates/cc_views/src/sheet_fields.rs` (add `pub mod sheet_fields;` in `cc_views/src/lib.rs`)
- Test: in the new file

**Interfaces:**
- Consumes: `Project::sheets_in_order`, `ProjectInfo::field`, `View::paper_mm`, `SheetSize::label`, `parse_iso_date`.
- Produces:

```rust
pub struct FieldCtx<'a> {
    pub project: &'a Project,
    pub sheet: &'a View,
    /// The title block definition (for `param:` defaults), if any.
    pub def: Option<&'a ComponentDef>,
    pub today: (i32, u32, u32),
    pub format_date: fn(i32, u32, u32) -> String,
    /// "As indicated", translated.
    pub as_indicated: &'a str,
}
pub fn expand(text: &str, ctx: &FieldCtx) -> String;
pub fn value(key: &str, ctx: &FieldCtx) -> Option<String>;  // None: unknown key
pub const KEYS: [&str; 12];  // the fixed keys, in menu order
```

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::{ComponentDef, ComponentKind, ComponentParam, ParamKind, Project, SheetSize, View, ViewId, ViewKind};

    fn iso(y: i32, m: u32, d: u32) -> String {
        format!("{y:04}-{m:02}-{d:02}")
    }

    fn sheet(id: u64, number: &str, fields: &[(&str, &str)], issue: &str) -> View {
        let kind = ViewKind::Sheet {
            number: number.into(),
            size: SheetSize::A1,
            portrait: false,
            viewports: Vec::new(),
            title_block: None,
            fields: fields.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            issue_date: issue.into(),
        };
        View::new(ViewId(id), &format!("Planta {number}"), 100, kind)
    }

    fn project() -> Project {
        let mut p = Project::from_template();
        p.views.retain(|_, v| !matches!(v.kind, ViewKind::Sheet { .. }));
        for v in [sheet(9001, "A10", &[], ""), sheet(9002, "A2", &[("Drawn", "Bia")], "2026-03-04"), sheet(9003, "A1", &[], "")] {
            p.views.insert(v.id, v);
        }
        p.info.name = "Casa".into();
        p.info.fields.push(cc_core::ProjectField { name: "CAU".into(), value: "A123".into() });
        p
    }

    fn def() -> ComponentDef {
        let mut def = ComponentDef { kind: ComponentKind::TitleBlock, ..Default::default() };
        let mut drawn = ComponentParam::number("Drawn", 0.0, ParamKind::Text);
        drawn.text = "Equipe".into();
        drawn.instance = true;
        def.params.push(drawn);
        def
    }

    #[test]
    fn fields_fill_in() {
        let p = project();
        let d = def();
        let ctx = |id: u64| FieldCtx {
            project: &p,
            sheet: p.view(ViewId(id)).unwrap(),
            def: Some(&d),
            today: (2026, 11, 6),
            format_date: |y, m, d| iso(y, m, d),
            as_indicated: "Indicada",
        };
        let c = ctx(9002);
        assert_eq!(expand("Folha {sheet_number} ({sheet_index}/{sheet_count})", &c), "Folha A2 (2/3)");
        assert_eq!(expand("{project_name} · {project:CAU} · {paper}", &c), "Casa · A123 · A1");
        assert_eq!(expand("{date} {issue_date}", &c), "2026-11-06 2026-03-04");
        assert_eq!(expand("{param:Drawn}", &c), "Bia");
        assert_eq!(expand("{param:Drawn}", &ctx(9001)), "Equipe", "the type's default");
        assert_eq!(expand("[{issue_date}]", &ctx(9001)), "[]", "empty values vanish");
        assert_eq!(expand("{xyz} {sheet_number", &c), "{xyz} {sheet_number", "unknown stays");
        assert_eq!(expand("{{literal}}", &c), "{literal}");
        assert_eq!(expand("{sheet_scale}", &c), "", "no viewports");
    }

    #[test]
    fn scale_is_single_or_indicated() {
        let mut p = project();
        let plans: Vec<ViewId> =
            p.views.values().filter(|v| v.level().is_some()).map(|v| v.id).take(2).collect();
        for (i, id) in plans.iter().enumerate() {
            if let Some(v) = p.views.get_mut(id) {
                v.scale = if i == 0 { 50 } else { 100 };
            }
        }
        let place = |p: &mut Project, ids: &[ViewId]| {
            if let Some(ViewKind::Sheet { viewports, .. }) = p.views.get_mut(&ViewId(9001)).map(|v| &mut v.kind) {
                *viewports = ids.iter().map(|&view| cc_core::Viewport { view, center: glam::DVec2::ZERO }).collect();
            }
        };
        place(&mut p, &plans[..1]);
        let ctx = FieldCtx {
            project: &p,
            sheet: p.view(ViewId(9001)).unwrap(),
            def: None,
            today: (2026, 1, 1),
            format_date: |y, m, d| iso(y, m, d),
            as_indicated: "Indicada",
        };
        assert_eq!(expand("{sheet_scale}", &ctx), "1:50");
        place(&mut p, &plans);
        let ctx = FieldCtx { project: &p, sheet: p.view(ViewId(9001)).unwrap(), ..ctx };
        assert_eq!(expand("{sheet_scale}", &ctx), "Indicada");
    }
}
```

Adapt `Viewport`'s literal to its real fields: `grep -n "pub struct Viewport" -A 6 crates/cc_core/src/view.rs`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p cc_views sheet_fields`
Expected: compile errors.

- [ ] **Step 3: Implement**

```rust
//! `{fields}` in title block texts: sheet, project and date values, and
//! the title block's per-sheet parameters. Unknown keys stay as written.

use cc_core::{ComponentDef, Project, View, ViewKind};

/// The fixed keys, in the order the editor's menu lists them.
pub const KEYS: [&str; 12] = [
    "sheet_number",
    "sheet_name",
    "sheet_count",
    "sheet_index",
    "sheet_scale",
    "paper",
    "project_name",
    "project_number",
    "client",
    "address",
    "author",
    "date",
];
// `issue_date` is listed after `date` by the menu too.

pub struct FieldCtx<'a> {
    pub project: &'a Project,
    pub sheet: &'a View,
    pub def: Option<&'a ComponentDef>,
    pub today: (i32, u32, u32),
    pub format_date: fn(i32, u32, u32) -> String,
    pub as_indicated: &'a str,
}

/// Most bytes a field key may have (a longer `{…}` is not a field).
const MAX_KEY: usize = 200;

pub fn expand(text: &str, ctx: &FieldCtx) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            out.push_str(&tail[..1]);
            rest = &tail[2..];
            continue;
        }
        if tail.starts_with('}') {
            out.push('}');
            rest = &tail[1..];
            continue;
        }
        match tail[1..].find('}').filter(|&j| j <= MAX_KEY) {
            Some(j) => {
                let key = &tail[1..1 + j];
                match value(key, ctx) {
                    Some(v) => out.push_str(&v),
                    None => out.push_str(&tail[..j + 2]),
                }
                rest = &tail[j + 2..];
            }
            None => {
                out.push_str(tail);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn value(key: &str, ctx: &FieldCtx) -> Option<String> {
    let ViewKind::Sheet { number, size, viewports, fields, issue_date, .. } = &ctx.sheet.kind else {
        return None;
    };
    let p = ctx.project;
    let info = &p.info;
    let date = |(y, m, d): (i32, u32, u32)| (ctx.format_date)(y, m, d);
    let v = match key.trim() {
        "sheet_number" => number.clone(),
        "sheet_name" => ctx.sheet.name.clone(),
        "sheet_count" => p.sheets_in_order().len().to_string(),
        "sheet_index" => {
            p.sheets_in_order().iter().position(|id| *id == ctx.sheet.id).map_or(String::new(), |i| (i + 1).to_string())
        }
        "sheet_scale" => sheet_scale(p, viewports, ctx.as_indicated),
        "paper" => size.label(),
        "project_name" => info.name.clone(),
        "project_number" => info.number.clone(),
        "client" => info.client.clone(),
        "address" => info.address.clone(),
        "author" => info.author.clone(),
        "date" => date(ctx.today),
        "issue_date" => cc_core::title_block::parse_iso_date(issue_date).map_or(String::new(), date),
        k => {
            if let Some(name) = k.strip_prefix("project:") {
                info.field(name.trim())?.to_string()
            } else if let Some(name) = k.strip_prefix("param:") {
                let name = name.trim();
                let param = ctx.def.and_then(|d| d.param(name)).filter(|q| q.kind.is_text());
                match (fields.get(name), param) {
                    (Some(v), Some(q)) if q.instance => v.clone(),
                    (_, Some(q)) => q.text.clone(),
                    (Some(v), None) => v.clone(),
                    (None, None) => return None,
                }
            } else {
                return None;
            }
        }
    };
    Some(v)
}

fn sheet_scale(p: &Project, viewports: &[cc_core::Viewport], as_indicated: &str) -> String {
    let mut scales = viewports
        .iter()
        .filter_map(|vp| p.view(vp.view))
        .filter(|v| !matches!(v.kind, ViewKind::ThreeD { .. }))
        .map(|v| v.scale);
    match scales.next() {
        None => String::new(),
        Some(first) if scales.all(|s| s == first) => format!("1:{first}"),
        Some(_) => as_indicated.to_string(),
    }
}
```

Notes: `key.trim()` lets `{ sheet_number }` work. The `(Some(v), None)`
arm keeps a sheet value for a parameter the block no longer has; the
spec says such fields are kept. Check that `is_text()` exists on
`ParamKind`; `component.rs:631` uses it.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cc_views sheet_fields`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -s -m "Expand title block fields from the sheet and the project"
```

---

### Task 6: Compose sheets with their title block

**Files:**
- Modify: `crates/cc_views/src/sheet.rs` (replace `TitleLabels`/`title_block` with `SheetEnv` and drawing the block)
- Modify: `crates/cc_views/src/lib.rs:37,316-317` (exports, test)
- Modify: `crates/cc_views/src/drawing.rs` (`Anchor::RightBottom`), `crates/cc_views/src/export.rs:13-18`, `crates/cc_views/src/dxf.rs:328-330`, `crates/cc_app/src/paint.rs:405-407`
- Modify: `crates/cc_engine/src/i18n.rs` (`format_date`, `today`), `crates/cc_engine/src/session.rs:263-275`
- Test: `crates/cc_views/src/sheet.rs` tests; `crates/cc_engine/tests/title_blocks.rs`

**Interfaces:**
- Consumes: Task 3 `resolve_sheet`, Task 5 `expand`/`FieldCtx`.
- Produces:

```rust
pub struct SheetEnv {
    pub today: (i32, u32, u32),
    pub format_date: fn(i32, u32, u32) -> String,
    pub as_indicated: String,
}
pub fn compose(project: &Project, sheet: &View, env: &SheetEnv, drawing_of: impl FnMut(ViewId) -> Option<Rc<Drawing2D>>) -> Option<Sheet>;
```

  `cc_engine::i18n::format_date(y: i32, m: u32, d: u32) -> String` (for the
  current language); `cc_engine::i18n::today() -> (i32, u32, u32)` (UTC from
  `SystemTime`); i18n key `sheet-as-indicated`. `TitleLabels`, `TITLE_W`,
  `TITLE_H` and the `title-*` keys stay until Task 7 uses them for the
  standard block's captions. `MARGIN` stays exported.

- [ ] **Step 1: Write the failing test** (`cc_views/src/sheet.rs` tests; replace the existing test at `lib.rs:316` that builds `TitleLabels` so it builds a `SheetEnv`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::title_block::{SheetItem, SheetShape, TextAnchor};
    use cc_core::{ComponentDef, ComponentKind, ElementType, ElementTypeId, TypeData};

    fn env() -> SheetEnv {
        SheetEnv { today: (2026, 11, 6), format_date: |y, m, d| format!("{d:02}/{m:02}/{y}"), as_indicated: "Ind.".into() }
    }

    #[test]
    fn the_title_block_is_drawn_with_its_fields() {
        let mut p = Project::from_template();
        let def = ComponentDef {
            kind: ComponentKind::TitleBlock,
            sheet: vec![
                SheetItem {
                    name: "Border".into(),
                    shape: SheetShape::Stroke {
                        points: ["10", "SheetWidth - 10"]
                            .iter()
                            .flat_map(|x| [cc_core::profile::ProfilePoint::new(*x, "10")])
                            .collect(),
                        closed: false,
                        weight: "0.7".into(),
                        color: None,
                        fill: None,
                    },
                },
                SheetItem {
                    name: "Number".into(),
                    shape: SheetShape::Text {
                        x: "SheetWidth - 12".into(),
                        y: "14".into(),
                        text: "{sheet_number}\n{date}".into(),
                        size: "5".into(),
                        anchor: TextAnchor::Right,
                        rotation: String::new(),
                    },
                },
            ],
            ..Default::default()
        };
        let tb = ElementTypeId(77_000);
        p.types.insert(
            tb,
            ElementType {
                id: tb,
                name: "TB".into(),
                data: TypeData::Component { component: "TB".into(), def },
                info: Default::default(),
                uid: Default::default(),
            },
        );
        let sheet = p.views.values_mut().find(|v| matches!(v.kind, ViewKind::Sheet { .. })).unwrap();
        if let ViewKind::Sheet { title_block, number, .. } = &mut sheet.kind {
            *title_block = Some(tb);
            *number = "A7".into();
        }
        let sheet = sheet.clone();
        let s = compose(&p, &sheet, &env(), |_| None).unwrap();
        let paper = s.paper;
        let texts: Vec<(&str, DVec2, Anchor)> = s
            .drawing
            .prims
            .iter()
            .filter_map(|q| match q {
                Prim::Text { text, pos, anchor, .. } => Some((text.as_str(), *pos, *anchor)),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&("A7", DVec2::new(paper.x - 12.0, 14.0), Anchor::RightBottom)), "{texts:?}");
        assert!(texts.iter().any(|t| t.0 == "06/11/2026" && t.1.y < 14.0), "second line below: {texts:?}");
        assert!(s.drawing.prims.iter().any(|q| matches!(q,
            Prim::Line { a, b, pen: Some(pen), .. }
                if *a == DVec2::new(10.0, 10.0) && *b == DVec2::new(paper.x - 10.0, 10.0) && pen.weight == Some(0.7))));
    }

    #[test]
    fn no_title_block_draws_the_blank_paper() {
        let p = Project::from_template();
        let mut sheet = p.views.values().find(|v| matches!(v.kind, ViewKind::Sheet { .. })).unwrap().clone();
        if let ViewKind::Sheet { title_block, .. } = &mut sheet.kind {
            *title_block = None;
        }
        let s = compose(&p, &sheet, &env(), |_| None).unwrap();
        assert!(s.drawing.prims.is_empty());
    }
}
```

If the template has no sheet, insert one with the literal from Task 1.
Check that `Prim`, `Anchor` and `DVec2` are in scope via `use super::*`;
add `use crate::{Prim, drawing::Anchor};` if not.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p cc_views the_title_block_is_drawn no_title_block`
Expected: compile errors (`SheetEnv`).

- [ ] **Step 3: Implement**

`drawing.rs`, the `Anchor` enum gains:

```rust
    /// The end of the baseline (right-aligned).
    RightBottom,
```

Its handlers:
- `export.rs` `anchor_offsets`: `Anchor::RightBottom => (0.28, 1.0),`
- `dxf.rs`: `Anchor::RightBottom => (2, 1),`
- `paint.rs`: `cc_views::Anchor::RightBottom => egui::vec2(-size.x, -size.y),`

`sheet.rs`: replace the `TitleLabels` parameter and the `title_block(...)`
call in `compose`. Keep `TitleLabels` (used in Task 7) but drop it from
`compose`. Replace the border `rect(...)` too: the border now comes from
the title block.

```rust
/// What a sheet's fields read besides the model.
#[derive(Debug, Clone)]
pub struct SheetEnv {
    pub today: (i32, u32, u32),
    pub format_date: fn(i32, u32, u32) -> String,
    pub as_indicated: String,
}

pub fn compose(
    project: &Project,
    sheet: &View,
    env: &SheetEnv,
    mut drawing_of: impl FnMut(ViewId) -> Option<Rc<Drawing2D>>,
) -> Option<Sheet> {
    let ViewKind::Sheet { viewports, title_block, .. } = &sheet.kind else { return None };
    let paper = sheet.paper_mm()?;
    let mut d = Drawing2D::default();
    let def = title_block.and_then(|t| match project.element_type(t).map(|t| &t.data) {
        Some(cc_core::TypeData::Component { def, .. }) => Some(def),
        _ => None,
    });
    if let Some(def) = def {
        let ctx = crate::sheet_fields::FieldCtx {
            project,
            sheet,
            def: Some(def),
            today: env.today,
            format_date: env.format_date,
            as_indicated: &env.as_indicated,
        };
        draw_title_block(&mut d, &def.resolve_sheet(paper), &ctx);
    }
    // … the viewports loop, unchanged …
}

fn draw_title_block(d: &mut Drawing2D, pieces: &cc_core::title_block::SheetPieces, ctx: &crate::sheet_fields::FieldCtx) {
    use cc_core::title_block::TextAnchor;
    for img in &pieces.images {
        d.prims.push(Prim::Image {
            origin: img.corner,
            u: DVec2::new(img.size.x, 0.0),
            v: DVec2::new(0.0, img.size.y),
            image: img.image.clone(),
            element: None,
            faded: false,
        });
    }
    for s in &pieces.strokes {
        if let Some([r, g, b]) = s.fill {
            d.fill(s.points.clone(), [r, g, b, 255], None);
        }
        let pen = crate::drawing::Pen { color: Some(s.color), weight: Some(s.weight), ..Default::default() };
        let n = s.points.len();
        let segments = if s.closed { n } else { n.saturating_sub(1) };
        for k in 0..segments {
            let (Some(&a), Some(&b)) = (s.points.get(k), s.points.get((k + 1) % n)) else { continue };
            d.prims.push(Prim::Line { a, b, style: LineStyle::Cut, element: None, pen: Some(pen) });
        }
    }
    for t in &pieces.texts {
        let text = crate::sheet_fields::expand(&t.text, ctx);
        let angle = t.rotation_deg.to_radians();
        let (anchor, mid) = match t.anchor {
            TextAnchor::Left => (Anchor::LeftBottom, false),
            TextAnchor::Center => (Anchor::CenterBottom, false),
            TextAnchor::Right => (Anchor::RightBottom, false),
            TextAnchor::Middle => (Anchor::Center, true),
        };
        let lines: Vec<&str> = text.lines().collect();
        let step = DVec2::from_angle(angle).perp() * (-1.5 * t.size);
        // Centered text is centered as a block; the others hang down from the first line.
        let first = if mid { -step * ((lines.len() as f64 - 1.0) / 2.0) } else { DVec2::ZERO };
        for (i, line) in lines.iter().enumerate().filter(|(_, l)| !l.is_empty()) {
            d.text_at(t.position + first + step * i as f64, *line, t.size, anchor, angle);
        }
    }
}
```

Check the method that looks up a type: `project.element_type(id)` is used
in the app; confirm with `grep -n "pub fn element_type" crates/cc_core/src/project.rs`.
Images go first so lines draw over them, and fills sit under their own
lines. Remove `title_block()` and the border `rect` closure. Run
`cargo build -p cc_views` and fix the `lib.rs` re-export
(`pub use sheet::{MARGIN, PlacedViewport, Sheet, SheetEnv, TITLE_H, TITLE_W, TitleLabels, compose as compose_sheet};`).
Update the old test at `lib.rs:316` to pass `&SheetEnv { today: (2026, 1, 1), format_date: |y, m, d| format!("{y}-{m}-{d}"), as_indicated: String::new() }`.

`cc_engine/src/i18n.rs`:

```rust
/// A date written the way the current language writes it.
pub fn format_date(y: i32, m: u32, d: u32) -> String {
    match language() {
        Language::En | Language::Ja | Language::ZhCn | Language::ZhTw => format!("{y:04}-{m:02}-{d:02}"),
        Language::De | Language::Pl | Language::Uk => format!("{d:02}.{m:02}.{y:04}"),
        Language::PtBr | Language::Es | Language::Fr | Language::It | Language::Lo => format!("{d:02}/{m:02}/{y:04}"),
    }
}

/// Today's date (UTC), from the system clock.
pub fn today() -> (i32, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    civil_from_days((secs / 86_400) as i64)
}

/// Year, month, day of a count of days since 1970-01-01 (proleptic Gregorian).
pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = (yoe + era * 400 + i64::from(m <= 2)) as i32;
    (y, m, d)
}
```

Use the real `Language` variant names (`Language::ALL` lists them; the
file shows `PtBr, En, Es, Fr, De, It, Lo, Pl, Uk, ZhTw, ZhCn, Ja`). Add a
test in `i18n.rs`:

```rust
#[test]
fn dates() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(20_398), (2025, 11, 6));
    assert_eq!(civil_from_days(-1), (1969, 12, 31));
}
```

Check 20 398: 2025-11-06 is day 20 398 since the epoch. If the assertion
disagrees, compute the date with
`python -c "import datetime;print(datetime.date(1970,1,1)+datetime.timedelta(20398))"`
and fix the expected value, not the algorithm.

`session.rs:263-275`: replace the `labels` block with

```rust
        let env = cc_views::SheetEnv {
            today: crate::i18n::today(),
            format_date: crate::i18n::format_date,
            as_indicated: t!("sheet-as-indicated"),
        };
```

and pass `&env`. Add `sheet-as-indicated` to all 12 `.ftl` files (pt-BR
"Indicada", en "As indicated", es "Indicada", fr "Indiquée", de
"Angegeben", it "Indicata", pl "Jak podano", uk-UA "Як зазначено", ja
"図示", zh-CN "如图示", zh-TW "如圖示", lo "ຕາມທີ່ລະບຸ").

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cc_views && cargo test -p cc_engine i18n && cargo build --workspace`
Expected: PASS. Existing sheet tests in other crates may assert the old
hard-coded block. They pass again after Task 7 puts the standard block on
the template's sheets. If any fail now for that reason, note them and go
on to Task 7 before committing.

- [ ] **Step 5: Commit** (together with Task 7 if existing sheet tests need the standard block)

```bash
git add -A && git commit -s -m "Draw sheets' title block components with their fields"
```

---

### Task 7: The standard title block, template and migration (format 22)

**Files:**
- Modify: `crates/cc_core/src/title_block.rs` (`standard`, `TitleCaptions`)
- Modify: `crates/cc_core/src/project.rs` (`from_template_localized` adds the type and points sheets at it; `template_default` keys)
- Modify: `crates/cc_core/src/persist.rs` (format 22, `from_ron_with_repairs_localized`, `load_with_repairs_localized`)
- Modify: `crates/cc_engine/src/session.rs` (`Session::open` uses the localized load); the app's open path (`grep -rn "load_with_repairs" crates/cc_app/src`)
- Modify: `crates/cc_views/src/sheet.rs` (remove `TitleLabels`, `TITLE_W`, `TITLE_H` once unused), `assets/i18n/*.ftl` (`tpl-tb-*` keys)
- Test: `crates/cc_core/src/persist.rs` tests, `title_block.rs` tests

**Interfaces:**
- Produces: `pub fn standard(n: &dyn Fn(&str) -> String) -> ComponentDef`
  (captions via template keys); `pub const STANDARD_KEY: &str = "tpl-tb-standard";`
  (type name key); `Project::add_standard_title_block(&mut self, n: &dyn Fn(&str) -> String) -> ElementTypeId`;
  `persist::from_ron_with_repairs_localized(text: &str, translate: &dyn Fn(&str) -> Option<String>) -> Result<(Project, usize), PersistError>`;
  `persist::load_with_repairs_localized(path, translate)`.

- [ ] **Step 1: Write the failing tests**

`title_block.rs` tests:

```rust
#[test]
fn the_standard_block_matches_the_old_layout() {
    let def = standard(&|k| crate::project::template_default(k).to_string());
    assert_eq!(def.kind, ComponentKind::TitleBlock);
    let r = def.resolve_sheet(DVec2::new(420.0, 297.0));
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    // The border, 10 mm in, and the 170 × 42 block in the bottom-right corner.
    let has_line = |a: DVec2, b: DVec2| {
        r.strokes.iter().any(|s| s.points.windows(2).any(|w| (w[0] == a && w[1] == b) || (w[0] == b && w[1] == a))
            || (s.closed && s.points.first() == Some(&a) && s.points.last() == Some(&b)))
    };
    assert!(has_line(DVec2::new(10.0, 10.0), DVec2::new(410.0, 10.0)));
    assert!(has_line(DVec2::new(240.0, 10.0), DVec2::new(240.0, 52.0)));
    let texts: Vec<&str> = r.texts.iter().map(|t| t.text.as_str()).collect();
    for f in ["{project_name}", "{sheet_name}", "{client}", "{sheet_number}"] {
        assert!(texts.contains(&f), "{f} in {texts:?}");
    }
    assert!(texts.contains(&"PROJETO"), "captions from the template keys");
}
```

`persist.rs` tests:

```rust
#[test]
fn format_20_sheets_get_the_standard_title_block() {
    let mut p = Project::from_template();
    // As a format-20 file: no title block types, sheets without one.
    p.types.retain(|_, t| t.category() != crate::Category::TitleBlock);
    for v in p.views.values_mut() {
        if let crate::ViewKind::Sheet { title_block, .. } = &mut v.kind {
            *title_block = None;
        }
    }
    let text = to_ron(&p).unwrap().replace(&format!("format_version: {FORMAT_VERSION}"), "format_version: 20");
    let english = |k: &str| (k == "tpl-tb-project").then(|| "PROJECT".to_string());
    let (q, repairs) = from_ron_with_repairs_localized(&text, &english).unwrap();
    assert!(repairs >= 1);
    let blocks = q.title_blocks();
    assert_eq!(blocks.len(), 1);
    assert!(q.views.values().all(|v| match &v.kind {
        crate::ViewKind::Sheet { title_block, .. } => *title_block == Some(blocks[0]),
        _ => true,
    }));
    let Some(crate::TypeData::Component { def, .. }) = q.element_type(blocks[0]).map(|t| &t.data) else { panic!() };
    assert!(def.sheet.iter().any(|i| matches!(&i.shape, crate::title_block::SheetShape::Text { text, .. } if text == "PROJECT")));
    // A current file is left alone.
    let (r, _) = from_ron_with_repairs(&to_ron(&q).unwrap()).unwrap();
    assert_eq!(r.title_blocks().len(), 1);
}
```

Check how `to_ron` writes the version line
(`grep -n "format_version" crates/cc_core/src/persist.rs`). If the
`replace` doesn't match, build the RON with `ron::ser` and edit the field
the way `rejects_future_version` (persist.rs:100) does.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p cc_core the_standard_block format_20_sheets`
Expected: compile errors.

- [ ] **Step 3: Implement**

`title_block.rs`. This rebuilds today's `cc_views::sheet::title_block`
layout exactly. The constants come from that function: block width 170,
height 42, margin 10, rows at 14 and 28, number cell 40 wide.

```rust
/// Name key of the standard title block (component and type).
pub const STANDARD_KEY: &str = "tpl-tb-standard";

/// The title block every sheet had before title blocks were components:
/// a border 10 mm in, and a 170 × 42 mm block in the bottom-right corner
/// with the project, sheet and client rows and the sheet number cell.
/// Captions are looked up by `n` (template keys `tpl-tb-*`).
pub fn standard(n: &dyn Fn(&str) -> String) -> ComponentDef {
    let p = |u: &str, v: &str| ProfilePoint::new(u, v);
    let stroke = |name: &str, points: Vec<ProfilePoint>, closed: bool, weight: &str| SheetItem {
        name: name.into(),
        shape: SheetShape::Stroke { points, closed, weight: weight.into(), color: None, fill: None },
    };
    let text = |name: &str, x: &str, y: &str, text: String, size: &str| SheetItem {
        name: name.into(),
        shape: SheetShape::Text {
            x: x.into(),
            y: y.into(),
            text,
            size: size.into(),
            anchor: TextAnchor::Left,
            rotation: String::new(),
        },
    };
    // x0 = SheetWidth - 180 (block's left), x1 = SheetWidth - 10, y0 = 10, y1 = 52.
    let sheet = vec![
        stroke(
            "Border",
            vec![p("10", "10"), p("SheetWidth - 10", "10"), p("SheetWidth - 10", "SheetHeight - 10"), p("10", "SheetHeight - 10")],
            true,
            "0.5",
        ),
        stroke(
            "Block",
            vec![p("SheetWidth - 180", "10"), p("SheetWidth - 10", "10"), p("SheetWidth - 10", "52"), p("SheetWidth - 180", "52")],
            true,
            "0.25",
        ),
        stroke("Row 1", vec![p("SheetWidth - 180", "24"), p("SheetWidth - 50", "24")], false, "0.25"),
        stroke("Row 2", vec![p("SheetWidth - 180", "38"), p("SheetWidth - 10", "38")], false, "0.25"),
        stroke("Number cell", vec![p("SheetWidth - 50", "10"), p("SheetWidth - 50", "38")], false, "0.25"),
        // Captions sit at the top of each cell (their top 1 mm below its line); values below them.
        text("Project caption", "SheetWidth - 178", "47", n("tpl-tb-project"), "2"),
        text("Project", "SheetWidth - 178", "40", "{project_name}".into(), "4"),
        text("Sheet caption", "SheetWidth - 178", "33", n("tpl-tb-sheet"), "2"),
        text("Sheet", "SheetWidth - 178", "28.5", "{sheet_name}".into(), "3.5"),
        text("Client caption", "SheetWidth - 178", "19", n("tpl-tb-client"), "2"),
        text("Client", "SheetWidth - 178", "15", "{client}".into(), "3"),
        text("Number caption", "SheetWidth - 48", "33", n("tpl-tb-number"), "2"),
        text("Number", "SheetWidth - 48", "25", "{sheet_number}".into(), "6"),
        text("Project number", "SheetWidth - 48", "18.8", format!("{} {{project_number}}", n("tpl-tb-project-number")), "2.2"),
        text("Author", "SheetWidth - 48", "13.8", format!("{} {{author}}", n("tpl-tb-author")), "2.2"),
    ];
    ComponentDef { kind: ComponentKind::TitleBlock, sheet, ..Default::default() }
}
```

How the text y values were derived: the old code placed a text's
*left-bottom* at `cell_top - size`. For example, the project caption is at
`y1 - 3 - 2 = 47` and the project value at `y1 - 3 - 3 - 4 = 42`. The
values above are approximations. Derive them exactly from
`cc_views/src/sheet.rs:83-111` before deleting that function: for each
`top_left(d, p(x, y), text, size)` call the baseline y is `y - size`,
and x values become `SheetWidth - 180 + (x - x0)`. Then fix the numbers
here so the old and new texts land at the same points. The test above
checks the lines. Add a second assertion that the
`{sheet_number}` text sits at `(paper.x - 48, 10 + 21 - 6)`.

`project.rs`:
- In `template_default` add: `"tpl-tb-standard" => "Carimbo padrão"`,
  `"tpl-tb-project" => "PROJETO"`, `"tpl-tb-sheet" => "FOLHA"`,
  `"tpl-tb-client" => "CLIENTE"`, `"tpl-tb-number" => "Nº"`,
  `"tpl-tb-project-number" => "Proj. nº"`, `"tpl-tb-author" => "Autor:"`.
  Copy the existing `title-*` captions in pt-BR.ftl so the pt-BR output is
  unchanged, and check `title-sheet`, `title-client` and `title-number`
  with `grep -n "^title-" assets/i18n/pt-BR.ftl`.
- In every `.ftl` add `tpl-tb-*` keys with the same text as that file's
  `title-*` keys. For `tpl-tb-standard`: en "Standard title block", es
  "Rótulo estándar", fr "Cartouche standard", de "Standard-Schriftfeld",
  it "Cartiglio standard", pl "Tabliczka standardowa", uk-UA
  "Стандартний штамп", ja "標準表題欄", zh-CN "标准图签", zh-TW "標準圖框",
  lo "ປ້າຍຊື່ມາດຕະຖານ".
- Add:

```rust
    /// Add the standard title block type (named by `n`) and return its id.
    pub fn add_standard_title_block(&mut self, n: &dyn Fn(&str) -> String) -> ElementTypeId {
        let id = ElementTypeId(self.alloc_id());
        let name = n(crate::title_block::STANDARD_KEY);
        let data = TypeData::Component { component: name.clone(), def: crate::title_block::standard(n) };
        self.types.insert(id, ElementType { id, name, data, info: Default::default(), uid: Default::default() });
        id
    }

    /// Point every sheet without a title block at `id`.
    pub fn give_sheets_title_block(&mut self, id: ElementTypeId) -> usize {
        let mut n = 0;
        for v in self.views.values_mut() {
            if let ViewKind::Sheet { title_block: t @ None, .. } = &mut v.kind {
                *t = Some(id);
                n += 1;
            }
        }
        n
    }
```

  Check how types get their `Uid` (`Document::new` assigns missing ones,
  per AGENTS.md); `Uid::default()` is the "missing" value.
- In `from_template_localized`, after the views are created:
  `let tb = p.add_standard_title_block(&n); p.give_sheets_title_block(tb);`.
  `n` there is `|key| -> String`; pass `&|k: &str| n(k)` if the types
  differ.

`persist.rs`:

```rust
pub const FORMAT_VERSION: u32 = 22;

pub fn from_ron_with_repairs(text: &str) -> Result<(Project, usize), PersistError> {
    from_ron_with_repairs_localized(text, &|_| None)
}

/// Like [`from_ron_with_repairs`], naming what migration adds with
/// `translate` (template keys; Portuguese defaults when it gives none).
pub fn from_ron_with_repairs_localized(
    text: &str,
    translate: &dyn Fn(&str) -> Option<String>,
) -> Result<(Project, usize), PersistError> {
    // … the body of the old from_ron_with_repairs, plus, with the other
    // version-gated migrations:
    //   if file.format_version < 22 && project.title_blocks().is_empty() {
    //       let n = |k: &str| translate(k).unwrap_or_else(|| crate::project::template_default(k).to_string());
    //       let id = project.add_standard_title_block(&n);
    //       repairs += 1 + project.give_sheets_title_block(id);
    //   }
}
```

Add `load_with_repairs_localized(path, translate)` mirroring
`load_with_repairs`. Extend the version comment block (persist.rs:50-58)
with "version 22 makes title blocks components (older files: the standard
title block on every sheet)."

Engine: in `Session::open`, call
`cc_core::persist::load_with_repairs_localized(path, &crate::i18n::try_tr)`
(re-export it from `cc_core` like `load_with_repairs`). Do the same in the
app's open path and in `project_open`, wherever `load_with_repairs` is
called (`grep -rn "load_with_repairs" crates`).

Delete `TitleLabels`, `TITLE_W`, `TITLE_H` and `title_block()` from
`cc_views/src/sheet.rs` and the `lib.rs` re-export, if anything still uses
them (`grep -rn "TITLE_W\|TITLE_H\|TitleLabels" crates`). Leave the
`title-*` .ftl keys only if code still reads them. Otherwise remove them
from all 12 files, since the i18n test only checks used-key presence and
equal key sets.

Regenerate the sample: `cargo run -p cc_core --example casa_simples`. Its
sheets get the standard block through `from_template_localized`. If the
example builds sheets itself (casa_simples.rs:206), set `title_block` to
the template's first title block there.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p cc_core && cargo test -p cc_views && cargo test -p cc_engine`
Expected: PASS, including the older sheet tests from Task 6 Step 4.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -s -m "Make the standard title block a component; migrate format 20 files"
```

---

### Task 8: Engine commands

**Files:**
- Create: `crates/cc_engine/src/commands/sheets.rs` (add `pub mod sheets;` to `commands/mod.rs`; register in `registry.rs` next to `views::SheetRemoveViewport` at line 308)
- Modify: `crates/cc_engine/src/commands/views.rs:195-205` (new sheets get the first title block)
- Modify: `crates/cc_engine/src/commands/types.rs:17-31` (`validate`: a title block needs sheet pieces, not items)
- Modify: `assets/i18n/*.ftl` (`tx-sheet-set`, `tx-new-title-block`)
- Modify: `docs/commands.md` (regenerated)
- Test: `crates/cc_engine/tests/title_blocks.rs`

**Interfaces:**
- Consumes: everything above.
- Produces commands:
  - `sheet_set { sheet: Ref, number?: String, size?: SizeArg, portrait?: bool, title_block?: Ref|null, fields?: {name: String|null}, issue_date?: String }`
    where `enum SizeArg { Iso(String) /* "A0".."A4" */, Custom { width: f64, height: f64 } }` (untagged).
    Returns `view_out` of the sheet.
  - `sheet_fields { sheet: Ref }` → `{ "fields": {key: value}, "params": [{name, label, value, default}], "errors": [..] }`.
  - `title_block_create { name: String, empty?: bool }` → `type_out`.
  - `component_sheet_piece_add { type: Ref, piece: SheetPieceArg, name?: String }` → `{index}`.
  - `component_sheet_piece_set { type: Ref, index: usize, piece?: Value (merge patch over the piece's serde form), name?: String }`.
  - `component_sheet_piece_delete { type: Ref, index: usize }`.
  - `SheetPieceArg` (tagged by `kind`): `stroke {points: [SketchPointArg], closed?, weight?: ExprArg, color?: [u8;3], fill?: [u8;3]}`,
    `text {x, y: ExprArg, text: String, size?: ExprArg (default 2.5), anchor?: "left"|"center"|"right"|"middle", rotation?: ExprArg}`,
    `image {x, y, width, height: ExprArg, image_path?: String, image?: String (base64)}`.

- [ ] **Step 1: Write the failing tests** (append to `crates/cc_engine/tests/title_blocks.rs`)

```rust
fn first_sheet(s: &mut Session) -> u64 {
    let v = run(s, "view_create", json!({"kind": "sheet", "name": "Plantas"}));
    v["id"].as_u64().unwrap()
}

#[test]
fn new_sheets_have_the_standard_title_block_and_fill_fields() {
    let mut s = Session::default();
    let sheet = first_sheet(&mut s);
    let f = run(&mut s, "sheet_fields", json!({"sheet": sheet}));
    assert_eq!(f["fields"]["sheet_name"], "Plantas");
    assert!(f["fields"]["sheet_count"].as_str().unwrap().parse::<u32>().unwrap() >= 1);
    run(&mut s, "sheet_set", json!({"sheet": sheet, "number": "A7", "size": {"width": 1000, "height": 594}, "portrait": true, "issue_date": "2026-11-06"}));
    let f = run(&mut s, "sheet_fields", json!({"sheet": sheet}));
    assert_eq!(f["fields"]["sheet_number"], "A7");
    assert_eq!(f["fields"]["paper"], "1000 × 594 mm");
    assert!(!f["fields"]["issue_date"].as_str().unwrap().is_empty());
    assert_eq!(fails(&mut s, "sheet_set", json!({"sheet": sheet, "issue_date": "31/02/2026"})), ErrorCode::InvalidArgument);
    assert_eq!(fails(&mut s, "sheet_set", json!({"sheet": sheet, "size": {"width": 10, "height": 10}})), ErrorCode::InvalidArgument);
    assert_eq!(fails(&mut s, "sheet_set", json!({"sheet": sheet, "title_block": "Alvenaria 15 cm"})), ErrorCode::InvalidArgument);
    run(&mut s, "sheet_set", json!({"sheet": sheet, "title_block": null}));
    let sheet_view = run(&mut s, "view_get", json!({"view": sheet}));
    assert!(sheet_view.to_string().contains("\"title_block\":null") || sheet_view["params"]["title_block"].is_null());
}

#[test]
fn a_title_block_is_built_from_commands_and_saved() {
    let mut s = Session::default();
    let tb = run(&mut s, "title_block_create", json!({"name": "Meu carimbo", "empty": true}));
    let tb = tb["id"].as_u64().unwrap();
    run(&mut s, "component_param_add", json!({"type": tb, "name": "Drawn", "kind": "text", "instance": true, "text": "Equipe"}));
    let i = run(&mut s, "component_sheet_piece_add", json!({"type": tb, "piece": {"kind": "stroke", "closed": true,
        "points": [{"u": 10, "v": 10}, {"u": "SheetWidth - 10", "v": 10}, {"u": "SheetWidth - 10", "v": "SheetHeight - 10"}, {"u": 10, "v": "SheetHeight - 10"}]}}));
    assert_eq!(i["index"], 0);
    run(&mut s, "component_sheet_piece_add", json!({"type": tb, "piece": {"kind": "text", "x": 20, "y": 20, "text": "{param:Drawn} {sheet_number}", "anchor": "left"}}));
    // 1 × 1 PNG.
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    run(&mut s, "component_sheet_piece_add", json!({"type": tb, "piece": {"kind": "image", "x": 30, "y": 30, "width": 20, "height": 20, "image": png}}));
    run(&mut s, "component_sheet_piece_set", json!({"type": tb, "index": 1, "piece": {"Text": {"size": "4"}}}));
    let sheet = first_sheet(&mut s);
    run(&mut s, "sheet_set", json!({"sheet": sheet, "title_block": tb, "number": "B2", "fields": {"Drawn": "Bia"}}));
    let f = run(&mut s, "sheet_fields", json!({"sheet": sheet}));
    assert_eq!(f["params"][0]["value"], "Bia");
    assert_eq!(f["params"][0]["default"], "Equipe");
    // The composed sheet shows the expanded text.
    let sheet_id = cc_core::ViewId(sheet);
    let composed = s.sheet(sheet_id).unwrap();
    assert!(composed.drawing.prims.iter().any(|p| matches!(p, cc_views::Prim::Text { text, .. } if text == "Bia B2")));
    assert!(composed.drawing.prims.iter().any(|p| matches!(p, cc_views::Prim::Image { .. })));
    // A .ccomp round trip keeps the picture.
    let dir = std::env::temp_dir().join(format!("ccad-tb-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tb.ccomp");
    run(&mut s, "component_save", json!({"type": tb, "path": path}));
    let mut t = Session::default();
    let loaded = run(&mut t, "component_load", json!({"path": path}));
    let got = run(&mut t, "component_get", json!({"type": loaded["id"]}));
    assert!(got.to_string().contains("Image"));
    run(&mut s, "component_sheet_piece_delete", json!({"type": tb, "index": 2}));
    assert_eq!(fails(&mut s, "component_sheet_piece_delete", json!({"type": tb, "index": 9})), ErrorCode::InvalidArgument);
    // Sheet pieces only go into title blocks.
    let door = run(&mut s, "type_get", json!({"type": "Porta 80x210"}));
    assert_eq!(fails(&mut s, "component_sheet_piece_add", json!({"type": door["id"], "piece": {"kind": "text", "x": 0, "y": 0, "text": "x"}})), ErrorCode::InvalidArgument);
}
```

Before running: check the real argument names of `view_create`,
`view_get`, `component_param_add`, `component_save`, `component_load`,
`component_get` and `type_get`. Run `cargo run -p cc_cli -- schema <name>`
for each, or read them in `docs/commands.md`, and adjust the JSON. Check
that the error code for `EngineError::invalid` is `InvalidArgument`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p cc_engine --test title_blocks`
Expected: FAIL (unknown commands).

- [ ] **Step 3: Implement `commands/sheets.rs`**

```rust
//! Sheets and their title blocks: paper, title block, fields and issue
//! date of a sheet; title block components and their 2D pieces.

use super::component::edit_def;
use super::component_piece::{ExprArg, SketchPointArg};
use super::edit;
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::title_block::{self, SheetItem, SheetShape, TextAnchor};
use cc_core::{ElementType, ElementTypeId, SheetSize, TypeData, ViewKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

/// A paper size: `"A0"`…`"A4"`, or `{width, height}` in millimeters.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum SizeArg {
    Iso(String),
    Custom { width: f64, height: f64 },
}

impl SizeArg {
    fn size(&self) -> Result<SheetSize> {
        let size = match self {
            SizeArg::Iso(s) => match s.trim().to_ascii_uppercase().as_str() {
                "A0" => SheetSize::A0,
                "A1" => SheetSize::A1,
                "A2" => SheetSize::A2,
                "A3" => SheetSize::A3,
                "A4" => SheetSize::A4,
                other => return Err(EngineError::invalid(format!("unknown paper '{other}' (A0 to A4, or {{width, height}})"))),
            },
            SizeArg::Custom { width, height } => SheetSize::Custom { width: *width, height: *height },
        };
        if !size.is_valid() {
            return Err(EngineError::invalid(format!(
                "a custom sheet is {} to {} mm on each side",
                cc_core::view::MIN_PAPER_MM,
                cc_core::view::MAX_PAPER_MM
            )));
        }
        Ok(size)
    }
}

/// A title block type by id or name.
fn title_block_type(p: &cc_core::Project, r: &Ref) -> Result<ElementTypeId> {
    let id = refs::element_type(p, r)?;
    if p.title_blocks().contains(&id) { Ok(id) } else { Err(EngineError::invalid("not a title block type")) }
}

/// Change a sheet: its number, paper (`size`, `portrait`), title block,
/// per-sheet field values and issue date.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetSet {
    /// The sheet: id or name.
    pub sheet: Ref,
    /// Sheet number.
    #[serde(default)]
    pub number: Option<String>,
    /// Paper: `"A0"`…`"A4"`, or `{width, height}` in millimeters (50 to 5000).
    #[serde(default)]
    pub size: Option<SizeArg>,
    /// The long side runs up.
    #[serde(default)]
    pub portrait: Option<bool>,
    /// The title block type (id or name), or `null` for none.
    #[serde(default, deserialize_with = "refs::nullable")]
    pub title_block: Option<Option<Ref>>,
    /// Values of the title block's per-sheet fields by parameter name: a text sets one, `null` clears it.
    #[serde(default)]
    pub fields: Option<serde_json::Map<String, Value>>,
    /// Issue date, `YYYY-MM-DD`; empty to clear.
    #[serde(default)]
    pub issue_date: Option<String>,
}

impl Command for SheetSet {
    const NAME: &'static str = "sheet_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = super::views::sheet(p, &self.sheet)?;
        let size = self.size.as_ref().map(SizeArg::size).transpose()?;
        let title_block = match &self.title_block {
            Some(Some(r)) => Some(Some(title_block_type(p, r)?)),
            Some(None) => Some(None),
            None => None,
        };
        let issue_date = match self.issue_date.as_deref().map(str::trim) {
            Some("") => Some(String::new()),
            Some(d) if title_block::is_iso_date(d) => Some(d.to_string()),
            Some(d) => return Err(EngineError::invalid(format!("'{d}' is not a YYYY-MM-DD date"))),
            None => None,
        };
        let mut changes: Vec<(String, Option<String>)> = Vec::new();
        for (name, value) in self.fields.iter().flatten() {
            let value = match value {
                Value::Null => None,
                Value::String(v) if v.chars().count() <= title_block::MAX_SHEET_TEXT => Some(v.clone()),
                other => return Err(EngineError::invalid(format!("field '{name}': expected a text or null, got {other}"))),
            };
            changes.push((name.trim().to_string(), value));
        }
        edit(s, t!("tx-sheet-set"), |e| {
            let Some(ViewKind::Sheet { number, size: sz, portrait, title_block: tb, fields, issue_date: date, .. }) =
                e.view_mut(id).map(|v| &mut v.kind)
            else {
                return Err(EngineError::invalid("not a sheet"));
            };
            if let Some(n) = self.number {
                *number = n;
            }
            if let Some(v) = size {
                *sz = v;
            }
            if let Some(v) = self.portrait {
                *portrait = v;
            }
            if let Some(v) = title_block {
                *tb = v;
            }
            if let Some(v) = issue_date {
                *date = v;
            }
            for (name, value) in changes {
                match value {
                    Some(v) => {
                        fields.insert(name, v);
                    }
                    None => {
                        fields.remove(&name);
                    }
                }
            }
            Ok(())
        })?;
        Ok(super::views::view_out(s, id))
    }
}
```

`super::views::sheet` and `view_out` must be `pub(crate)`. Make them so if
they're private (`views.rs:396-405` holds `fn sheet`; find `fn view_out`).

```rust
/// A sheet's title block fields: every field key with its value on this
/// sheet, and the title block's per-sheet parameters (value and default).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetFields {
    /// The sheet: id or name.
    pub sheet: Ref,
}

impl Command for SheetFields {
    const NAME: &'static str = "sheet_fields";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = super::views::sheet(p, &self.sheet)?;
        let view = p.view(id).ok_or_else(|| EngineError::invalid("not a sheet"))?;
        let ViewKind::Sheet { title_block: tb, fields, .. } = &view.kind else {
            return Err(EngineError::invalid("not a sheet"));
        };
        let def = tb.and_then(|t| match p.element_type(t).map(|t| &t.data) {
            Some(TypeData::Component { def, .. }) => Some(def),
            _ => None,
        });
        let as_indicated = t!("sheet-as-indicated");
        let ctx = cc_views::sheet_fields::FieldCtx {
            project: p,
            sheet: view,
            def,
            today: crate::i18n::today(),
            format_date: crate::i18n::format_date,
            as_indicated: &as_indicated,
        };
        let mut out = serde_json::Map::new();
        for key in cc_views::sheet_fields::KEYS.iter().chain(["issue_date"].iter()) {
            out.insert((*key).to_string(), json!(cc_views::sheet_fields::value(key, &ctx).unwrap_or_default()));
        }
        for f in &p.info.fields {
            out.insert(format!("project:{}", f.name), json!(f.value));
        }
        let params: Vec<Value> = def
            .iter()
            .flat_map(|d| d.params.iter())
            .filter(|q| q.kind.is_text() && q.instance)
            .map(|q| json!({"name": q.name, "label": q.display(), "value": fields.get(&q.name).unwrap_or(&q.text), "default": q.text}))
            .collect();
        let errors: Vec<Value> = match (def, view.paper_mm()) {
            (Some(d), Some(paper)) => d
                .resolve_sheet(paper)
                .errors
                .iter()
                .map(|(i, e)| json!({"index": i, "error": e.to_string()}))
                .collect(),
            _ => Vec::new(),
        };
        Ok(json!({"fields": out, "params": params, "errors": errors}))
    }
}
```

```rust
/// Create a title block component: a copy of the standard one, or empty
/// with `empty`. Its types are chosen per sheet with `sheet_set`.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TitleBlockCreate {
    /// Name of the component and of its first type.
    pub name: String,
    /// Start with no pieces.
    #[serde(default)]
    pub empty: bool,
}

impl Command for TitleBlockCreate {
    const NAME: &'static str = "title_block_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let name = self.name.trim().to_string();
        if name.is_empty() {
            return Err(EngineError::invalid("the title block needs a name"));
        }
        if s.project().types.values().any(|t| t.name == name) {
            return Err(EngineError::invalid(format!("a type named '{name}' exists")));
        }
        let mut def = title_block::standard(&|k| {
            crate::i18n::try_tr(k).unwrap_or_else(|| cc_core::project::template_default(k).to_string())
        });
        if self.empty {
            def.sheet.clear();
        }
        let id = edit(s, t!("tx-new-title-block"), |e| {
            let id = ElementTypeId(e.alloc_id());
            let data = TypeData::Component { component: name.clone(), def };
            e.add_type(ElementType { id, name: name.clone(), data, info: Default::default(), uid: Default::default() });
            Ok(id)
        })?;
        Ok(super::types::type_out(s, id))
    }
}
```

Make `types::type_out` `pub(crate)`. Check the name-uniqueness rule other
create commands use (`grep -n "exists" crates/cc_engine/src/commands/types.rs`)
and follow it. If they allow duplicate names, drop the check.

Piece commands:

```rust
/// A 2D piece of a title block. Coordinates are millimeters from the
/// paper's bottom-left corner: numbers or expressions over the
/// parameters, `SheetWidth` and `SheetHeight`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SheetPieceArg {
    /// Lines and arcs through `points` ({u, v, radius}: u across, v up; a radius makes the segment to the next point an arc).
    Stroke {
        points: Vec<SketchPointArg>,
        /// Joins the last point to the first.
        #[serde(default)]
        closed: bool,
        /// Pen width, millimeters (default 0.25).
        #[serde(default)]
        weight: Option<ExprArg>,
        /// Line color [r, g, b] (default black).
        #[serde(default)]
        color: Option<[u8; 3]>,
        /// Fill color of a closed stroke [r, g, b].
        #[serde(default)]
        fill: Option<[u8; 3]>,
    },
    /// Text with `{fields}`; line breaks start new lines.
    Text {
        x: ExprArg,
        y: ExprArg,
        text: String,
        /// Height, millimeters (default 2.5).
        #[serde(default)]
        size: Option<ExprArg>,
        /// `left` (default), `center`, `right` (on the baseline) or `middle` (centered).
        #[serde(default)]
        anchor: Option<String>,
        /// Degrees counter-clockwise.
        #[serde(default)]
        rotation: Option<ExprArg>,
    },
    /// A PNG, JPEG or SVG picture filling a rectangle from its bottom-left corner (x, y).
    Image {
        x: ExprArg,
        y: ExprArg,
        width: ExprArg,
        height: ExprArg,
        /// The picture's file.
        #[serde(default)]
        image_path: Option<String>,
        /// The picture's bytes, base64.
        #[serde(default)]
        image: Option<String>,
    },
}
```

`SheetPieceArg::shape(&self) -> Result<SheetShape>` converts the argument.
- **Strokes:** points go through `component_piece::profile_points`. Make it
  `pub(crate)`. It requires 2 to 1024 points.
- **Text:** reject texts over `MAX_SHEET_TEXT`. The anchor maps
  `left|center|right|middle` to `TextAnchor`, and anything else errors.
- **Images:** read the bytes from `image_path` (`std::fs::read`) or from
  `image`, decoded with the base64 decoder `cc_core::raster` already has
  (`grep -n "pub fn" crates/cc_core/src/raster.rs`; it decodes the
  project-file base64). Then build the `RasterImage` the way the
  `image_import` command does: find the helper with
  `grep -rn "RasterImage" crates/cc_engine/src/commands/cad.rs` and reuse it
  (`cc_views::raster` reads the size). Refuse an image that fails
  `is_valid`.

```rust
fn title_block_def(def: &cc_core::ComponentDef) -> Result<()> {
    if def.kind == cc_core::ComponentKind::TitleBlock {
        Ok(())
    } else {
        Err(EngineError::invalid("sheet pieces go into title blocks only"))
    }
}

/// Add a 2D piece to a title block component.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentSheetPieceAdd {
    /// A type of the title block: id or name.
    #[serde(rename = "type")]
    pub type_ref: Ref,
    /// The piece.
    pub piece: SheetPieceArg,
    /// Name (default `Piece1`, `Piece2`…).
    #[serde(default)]
    pub name: Option<String>,
}

impl Command for ComponentSheetPieceAdd {
    const NAME: &'static str = "component_sheet_piece_add";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let shape = self.piece.shape()?;
        let index = edit_def(s, &self.type_ref, &[], |def| {
            title_block_def(def)?;
            if def.sheet.len() >= title_block::MAX_SHEET_PIECES {
                return Err(EngineError::invalid("the title block has the most pieces it can hold"));
            }
            let name = match self.name.as_deref().map(str::trim) {
                Some(n) if !n.is_empty() => n.to_string(),
                _ => (1..).map(|i| format!("Piece{i}")).find(|n| !def.sheet.iter().any(|it| it.name == *n)).unwrap_or_default(),
            };
            def.sheet.push(SheetItem { name, shape });
            Ok(def.sheet.len() - 1)
        })?;
        Ok(json!({ "index": index }))
    }
}

/// Change a 2D piece of a title block: `piece` is a merge patch over its
/// form in `component_get` (e.g. `{"Text": {"size": "4"}}`).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentSheetPieceSet {
    /// A type of the title block: id or name.
    #[serde(rename = "type")]
    pub type_ref: Ref,
    /// The piece's index (from `component_get`).
    pub index: usize,
    /// Merge patch over the piece's shape.
    #[serde(default)]
    pub piece: Option<Value>,
    /// New name.
    #[serde(default)]
    pub name: Option<String>,
}

impl Command for ComponentSheetPieceSet {
    const NAME: &'static str = "component_sheet_piece_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        edit_def(s, &self.type_ref, &[], |def| {
            title_block_def(def)?;
            let item = def.sheet.get_mut(self.index).ok_or_else(|| EngineError::invalid("no such piece"))?;
            if let Some(patch) = &self.piece {
                let mut value = serde_json::to_value(&item.shape).map_err(|e| EngineError::invalid(e.to_string()))?;
                super::patch_value(&mut value, patch);
                let shape: SheetShape =
                    serde_json::from_value(value).map_err(|e| EngineError::invalid(format!("invalid piece: {e}")))?;
                let mut probe = def.clone();
                if let Some(it) = probe.sheet.get_mut(self.index) {
                    it.shape = shape.clone();
                }
                if title_block::repair_def(&mut probe) > 0 {
                    return Err(EngineError::invalid("the piece is out of bounds"));
                }
                item.shape = shape;
            }
            if let Some(n) = self.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                item.name = n.to_string();
            }
            Ok(())
        })?;
        Ok(json!({ "index": self.index }))
    }
}

/// Delete a 2D piece of a title block.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentSheetPieceDelete {
    /// A type of the title block: id or name.
    #[serde(rename = "type")]
    pub type_ref: Ref,
    /// The piece's index.
    pub index: usize,
}

impl Command for ComponentSheetPieceDelete {
    const NAME: &'static str = "component_sheet_piece_delete";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        edit_def(s, &self.type_ref, &[], |def| {
            title_block_def(def)?;
            if self.index >= def.sheet.len() {
                return Err(EngineError::invalid("no such piece"));
            }
            def.sheet.remove(self.index);
            Ok(())
        })?;
        Ok(json!({}))
    }
}
```

The probe-clone in `ComponentSheetPieceSet` has a borrow problem:
`item` is a mutable borrow of `def` while `def.clone()` runs. Restructure
it: compute `shape` from `def.sheet.get(self.index)` immutably, build and
check the probe, then take `get_mut` to assign. Check `patch_value`'s
signature in `commands/mod.rs` (Task 2 already used it).

`types.rs` `validate`: replace the "no pieces" check with

```rust
    let title_block = def.kind == cc_core::ComponentKind::TitleBlock;
    if def.items.is_empty() && !title_block {
        return Err(EngineError::invalid("the component has no pieces"));
    }
    if title_block && !def.items.is_empty() {
        return Err(EngineError::invalid("a title block has only sheet pieces"));
    }
    if !title_block && !def.sheet.is_empty() {
        return Err(EngineError::invalid("sheet pieces go into title blocks only"));
    }
```

An empty title block (`title_block_create` with `empty: true`) is valid.

`views.rs:195-205`, the new sheet:

```rust
let kind = ViewKind::Sheet {
    number: format!("A{n}"),
    size: SheetSize::A3,
    portrait: false,
    viewports: Vec::new(),
    title_block: p.title_blocks().first().copied(),
    fields: Default::default(),
    issue_date: String::new(),
};
```

Registry (`registry.rs`, after `views::SheetRemoveViewport`):

```rust
        sheets::SheetSet,
        sheets::SheetFields,
        sheets::TitleBlockCreate,
        sheets::ComponentSheetPieceAdd,
        sheets::ComponentSheetPieceSet,
        sheets::ComponentSheetPieceDelete,
```

Use the module path the registry expects (it names `views::…`, so add
`sheets` to the `use` there if needed).

i18n: add `tx-sheet-set` (pt-BR "Alterar folha", en "Change sheet", es
"Cambiar hoja", fr "Modifier la feuille", de "Blatt ändern", it "Modifica
tavola", pl "Zmień arkusz", uk-UA "Змінити аркуш", ja "シートを変更", zh-CN
"修改图纸", zh-TW "修改圖紙", lo "ປ່ຽນແຜ່ນ") and `tx-new-title-block`
(pt-BR "Novo carimbo", en "New title block", es "Nuevo rótulo", fr "Nouveau
cartouche", de "Neues Schriftfeld", it "Nuovo cartiglio", pl "Nowa
tabliczka", uk-UA "Новий штамп", ja "新しい表題欄", zh-CN "新建图签", zh-TW
"新增圖框標題欄", lo "ປ້າຍຊື່ໃໝ່") to all 12 files.

- [ ] **Step 4: Run the tests and regenerate the reference**

Run:

```bash
cargo test -p cc_engine --test title_blocks
cargo run -p cc_cli -- reference > docs/commands.md
cargo test -p cc_engine
```

Expected: PASS (the reference test compares `docs/commands.md`).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -s -m "Add sheet_set, sheet_fields and title block piece commands"
```

---

### Task 9: Whole-workspace verification and PR

**Files:** none new.

- [ ] **Step 1: Format, lint and test everything**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all pass. Fix any failure in the task that caused it, and
re-run.

- [ ] **Step 2: Check the IFC fixtures still export** (CI runs this on Linux; locally check that export doesn't fail)

```bash
cargo run -p cc_ifc --example export_ifc -- examples/casa_simples.ccad target/out.ifc
```

Expected: writes the file; title blocks are not exported.

- [ ] **Step 3: Push and open the PR**

```bash
git push -u origin feat/title-blocks
gh pr create --base main --title "Title blocks as components (core and engine)" --body-file <file with summary, spec link, test list>
```

The PR body links `specs/2026-11-06-title-blocks-design.md`, says it is
delivery 1 of 4 (app sheet properties, editor and docs follow), and lists
the new commands and format 22.

---

## Later deliveries (separate plans, written when delivery 1 merges)

2. **App, sheet side**: sheet properties rows (title block, paper, issue
   date, fields), project fields in Project Information. Commands:
   `sheet_set`, `project_info_set`.
3. **App, editor**: the component environment's Sheet view, edge planes,
   sketch tools writing `Stroke` pieces, Text and Image tools, Insert field
   menu, preview sizes.
4. **Docs**: `docs/interface*.md`, `docs/automation*.md`.
