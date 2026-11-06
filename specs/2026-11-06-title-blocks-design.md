# Title blocks as components — design

Date: 2026-11-06. Status: approved in conversation; spec under review.

## Goal

Today every sheet gets the same fixed title block, drawn by code in
`cc_views::sheet::title_block` (170 × 42 mm in the bottom-right corner,
with project name, sheet name, client, sheet number, project number and
author) inside an automatic 10 mm border. Users can't change it.

The goal is for users to draw their own **sheet layouts**: border, title
block, logo and the bands they need. They draw them with the sketch tools.
Texts fill in automatically from the sheet and the project (sheet number,
sheet count, date, client…) and from values each sheet sets. Layouts are
**components**: they live in the component library and in `.ccomp` files.
They're reused across projects and carried from one computer to another.

## Decisions taken

- A title block is a component (`ComponentKind::TitleBlock`), not a separate
  model and not script-only. Script pieces may come later inside it.
- It draws the **whole sheet**: border, margins and the title block. The
  paper's width and height are parameters it reads, so one component can
  serve every size.
- Sheets take ISO A0–A4, **portrait or landscape**, or a **custom** width ×
  height.
- Fields cover sheet data, project data (including user-defined project
  fields), dates and per-sheet values.
- Out of scope: a revision table and free drawing directly on sheets.

## Model (cc_core)

### Component kind and category

- `ComponentKind::TitleBlock`. Its definitions make types of a new
  `Category::TitleBlock` (i18n `category-title-block`, pt-BR "Carimbos",
  en "Title Blocks"). Like every component, types that share `component`
  share the definition and differ in parameter values.
- A title block has no elements. Sheets refer to its type. It has no phase,
  no IFC class, no MEP data, no opening and no connectors. `repair` clears
  any of these on a title block definition.

### 2D pieces (`ComponentShape`)

There are three new variants. Their coordinates are in **paper millimeters**,
with the origin at the paper's bottom-left corner, x to the right and y up.
Every number is a `cc_core::expr` expression, like the other pieces.

```rust
/// Lines and arcs on the sheet: `points` run straight, along an arc
/// (`radius`) or an ellipse to the next one, as in sketched profiles.
Stroke {
    points: Vec<ProfilePoint>,
    closed: bool,
    /// Pen width in millimeters (expression); "" = the thin default.
    weight: String,
    /// Fill of a closed stroke, `None` for none.
    fill: Option<[u8; 3]>,
    /// Color of the line; `None` = black.
    color: Option<[u8; 3]>,
}
/// Text: fixed words and `{fields}` (see Fields).
Text {
    x: String,
    y: String,
    text: String,
    /// Cap height in millimeters (expression).
    size: String,
    anchor: TextAnchor,      // 9 positions, LeftBottom by default
    rotation: String,        // degrees, expression
    bold: bool,
    /// Wrapping width in millimeters; "" for a single line.
    width: String,
}
/// A picture: the file's bytes, kept in the component (and its .ccomp).
Image {
    x: String,
    y: String,
    width: String,
    height: String,
    image: RasterImage,
}
```

- Title blocks take only these three shapes. Other kinds refuse them:
  `component_piece_add` errors, and `repair` drops mismatched pieces and
  reports them.
- Material, `in_plan`, `void`, `plan_only` and `detail` don't apply to
  2D pieces. They're ignored, and they're omitted in serialization when
  they hold their defaults.
- Limits: at most `MAX_SHEET_PIECES` (2 000) pieces, a text of at most
  4 000 characters, and images within the `raster` limits. A crafted file
  over a limit loses the extra pieces through `repair`.

### Context parameters

- While a title block resolves, it can read `SheetWidth` and `SheetHeight`,
  in millimeters, as expression names. The component environment shows
  them as **Sheet width** and **Sheet height**, read-only. This works the
  same way `WallThickness` is defined for openings.
- In the editor, reference planes at the four edges are bound to them.
  `0` is left and bottom, `SheetWidth` is right and `SheetHeight` is top.

### Resolution

```rust
impl ComponentDef {
    /// The 2D pieces of a title block on a `size` (mm) sheet: lines, texts
    /// (unresolved fields), images, and per-piece errors.
    pub fn resolve_sheet(&self, values: &ParamValues, size: DVec2) -> SheetPieces;
}
pub struct SheetPieces {
    pub strokes: Vec<SheetStroke>,  // polyline points (arcs flattened), closed, weight, fill, color
    pub texts: Vec<SheetText>,      // position, raw text, size, anchor, rotation, bold, width
    pub images: Vec<SheetImage>,    // corner, size, Arc<RasterImage>
    pub errors: Vec<PieceError>,
}
```

- Errors are values. A piece whose expression fails is left out and
  reported, and the other pieces are still drawn.
- Arcs are flattened with the same step `Sketch::profile` uses.

### Sheets

`ViewKind::Sheet` gains new fields, all with serde defaults:

```rust
Sheet {
    number: String,
    size: SheetSize,
    /// Portrait: the size's long side runs up.
    #[serde(default)] portrait: bool,
    viewports: Vec<Viewport>,
    /// The title block type drawn on it; `None` draws the blank paper.
    #[serde(default)] title_block: Option<ElementTypeId>,
    /// This sheet's values of the title block's per-sheet fields, by
    /// parameter name (kept when the title block changes).
    #[serde(default)] fields: BTreeMap<String, String>,
    /// Issue date, ISO `YYYY-MM-DD`; empty when not issued.
    #[serde(default)] issue_date: String,
}
```

- `SheetSize` gains `Custom { width: f64, height: f64 }` (mm). The unit
  variants keep their serialized form.
- `View::paper_mm()` returns the paper size after orientation. A custom
  size is clamped to 50 mm–5 000 mm, and repair resets NaN or infinite
  values to A3.
- `Project::sheets_in_order()` returns the sheets sorted by number in
  natural order (A2 before A10). The browser uses the same order.

### Per-sheet fields

These are the title block's **text parameters with `instance: true`**,
the existing flag for values that elements may override. The type's
`text` is the default, and a sheet's `fields[name]` overrides it.

### Project fields

`ProjectInfo` gains `fields: Vec<ProjectField { name: String, value: String }>`
with a serde default. Names are unique and not empty, and repair drops
duplicates.

### Standard title block and migration

- `cc_core::title_block::standard(captions: &TitleCaptions) -> ComponentDef`
  rebuilds today's layout as pieces: the 10 mm border, the 170 × 42 mm
  block anchored to `SheetWidth - 10`, its cell lines, the captions, and
  the field texts `{project_name}`, `{sheet_name}`, `{client}`,
  `{sheet_number}`, `{project_number}` and `{author}`.
- The project template carries it, with captions in the template language.
  Sheets created by `view_create` take the project's first title block type.
- `FORMAT_VERSION` goes 20 → 21. On load, a file before 21 gets the
  standard type (unless the file already has a title block type), and
  every sheet points at it. The repair report says so.
- cc_core has no i18n, so it writes English captions. After a load whose
  report includes that migration, the engine's open paths (`project_open`
  and the app's open) call `cc_engine::title_block::localize_migrated` to
  rewrite those captions into the UI language. That happens inside the
  load, not as an undo step.
- Repair resets a sheet's `title_block` that points at a missing type, or
  at a type that isn't a title block, to the project's first title block
  type (or `None`), and counts each reset.

## Fields (cc_views::sheet)

Texts hold `{key}` fields. Keys are stable English names, so a file reads
the same in every language. The editor's **Insert field** menu shows
translated names.

| Group | Keys |
|---|---|
| Sheet | `sheet_number`, `sheet_name`, `sheet_count`, `sheet_index`, `sheet_scale`, `paper` |
| Project | `project_name`, `project_number`, `client`, `address`, `author`, `project:<Name>` |
| Dates | `date`, `issue_date` |
| Per sheet | `param:<Name>` |

- `sheet_count` counts the project's sheets. `sheet_index` is the 1-based
  position in `sheets_in_order`.
- `sheet_scale` is `1:N` when every placed non-3D view has scale N,
  otherwise the translated "As indicated". It's empty with no viewports.
- `paper` is `A1` for ISO sizes, or `W × H mm` for custom ones.
- `date` is the day the sheet is composed. `issue_date` is the sheet's
  date. Both are formatted for the UI language: `dd/mm/yyyy` in pt-BR,
  es, fr, it, de (with dots), pl and uk-UA, and `yyyy-mm-dd` in en, ja,
  zh-CN, zh-TW and lo. The formats are a table in `cc_engine::i18n`.
- `param:<Name>` returns the sheet's value, else the type's text.
- An empty value renders as nothing. An unknown key, or `{` with no
  match, stays as typed (`{xyz}`), so a mistake shows on screen and in
  the PDF. `{{` writes a literal brace.

`cc_views::sheet::compose` takes a `SheetEnv` instead of `TitleLabels`:

```rust
pub struct SheetEnv {
    pub today: (i32, u32, u32),
    pub format_date: fn(i32, u32, u32) -> String,
    pub as_indicated: String,
}
```

The engine builds it, so cc_views stays free of clocks and of i18n, and
tests pin the date.

### Drawing

- Strokes become `Drawing2D` polylines with a `Pen`: weight, color, solid.
  Closed strokes with a fill also get a fill.
- Texts become anchored and rotated text. A text with a width wraps the
  way multiline text notes wrap.
- Text primitives have no bold yet, so `Prim::Text` gains a `bold` flag
  with a serde default. The app paints it with Inter Semibold, which
  already ships. SVG writes `font-weight="600"`, PDF uses Helvetica-Bold,
  and PNG outlines the semibold face through `skrifa`.
- Images become `Prim::Image`.
- Everything sits under the viewports, as the current border does. The
  exporters (SVG, PDF, PNG, DXF) already write all these primitives. DXF
  puts the title block on layer `TitleBlock`. IFC export ignores title
  blocks.

## Engine (cc_engine)

- **`sheet_set`**: `sheet` (id or name), plus optional `number`, `size`
  (`"A0"`…`"A4"` or `{width, height}` in mm), `portrait`, `title_block`
  (type by id or name, or `null`), `fields` (a merge patch: `null`
  removes one) and `issue_date` (ISO date, or `""`). It errors when the
  type isn't a title block, a date doesn't parse, or a size is out of
  range.
- **`sheet_fields`** (query): `sheet` → every field key with its resolved
  value, plus the per-sheet parameters with the sheet's value and the
  type's default. Scripts and MCP clients can check a title block without
  rendering it.
- **`project_info_set`** also takes `fields` (a merge patch by name), and
  `project_info` returns them.
- **`component_create`** takes `kind: "title_block"`, whose definition
  starts as a copy of the standard one. `component_piece_add` and
  `component_piece_set` take the new shapes, and the JSON schema documents
  them. Images come in as a file path (`image_path`) or base64 (`image`).
  The command reads the file and checks it with the `raster` limits.
- **`view_create`** with kind sheet gives the new sheet the first title
  block type.
- Every parameter has a doc comment, and `docs/commands.md` is regenerated.

## App (cc_app)

### Sheet properties

The properties palette of a sheet shows:

- **Title block**: a combo of the title block types, plus *None*.
- **Paper**: the size combo (A0–A4, Custom), a Portrait checkbox, and
  width × height when the size is custom.
- **Issue date**: a text field validated as a date, with *Today* beside
  it.
- **Fields**: one row per per-sheet parameter. Its hint is the type's
  default, and clearing a row goes back to the default.

Each change runs `sheet_set` through `engine_edit::view`.

### Project information

The Project Information dialog gains a **Project fields** table (name and
value; add, rename, remove), saved with `project_info_set`.

### Title block editor

- **Opening it**: from a sheet, *Edit Title Block* (Modify tab when a sheet
  is active, and the sheet's context menu). It also opens from the library,
  like other components. *New Title Block* creates one from the standard
  layout and opens it.
- **The workspace**: the component environment opens with a single
  **Sheet** view instead of plan, front, side and 3D. It shows the paper
  outline at a preview size: the active sheet's, or A0–A4 portrait or
  landscape from a combo in the options bar. The four edge reference
  planes are pre-made and locked.
- **Sketch tools**: the same ones as sketch mode (the `sketch_mode::Session`
  through `component_sketch`, Target::Piece): line, rectangle, arcs,
  circle, polygon, ellipse, offset, trim, split, move, copy, rotate,
  mirror, and typed lengths (#179). *Finish* writes the sketch as a
  `Stroke` piece. A point lying on an edge plane, or on a user reference
  plane, is written as an expression of it (`SheetWidth - 10`), so the
  stroke follows the paper. Other points become plain numbers. Closed
  loops become closed strokes. The properties palette edits weight, color
  and fill.
- **Text tool**: a click places a text. The palette edits the text, size,
  anchor, rotation, bold and width, and has an **Insert field** menu
  (grouped, translated, inserting `{key}`). The canvas shows fields
  resolved against the active sheet, or against sample values when the
  editor opened from the library. A toggle shows the raw `{keys}`.
- **Image tool**: pick a PNG, JPG or SVG, then drag its rectangle; the
  aspect is kept unless Shift is held. Grips resize it.
- **Parameters panel**: the existing panel. *New per-sheet field* adds a
  text parameter with `instance: true`. Sheet width and height show as
  read-only context parameters.
- **Selecting**: pick, window, grips and Delete work on 2D pieces as on
  other pieces.
- *Finish* and *Cancel* behave as for components today. Every change is a
  `component_*` command.

## Robustness

- Title block pieces and project fields go through `repair`. The checks
  are expression length, piece and text limits, valid rasters, finite
  numbers and unique field names.
- `resolve_sheet` is bounded by the piece limit and the arc step, and
  never panics.
- A sheet whose title block fails to resolve still draws the pieces that
  resolve. The app's status bar shows the first error when the sheet is
  active.

## Testing

- **cc_core**:
  - `resolve_sheet` evaluates expressions with `SheetWidth`/`SheetHeight`
    (a line at `SheetWidth - 10` on A3 and on A1, portrait and custom).
  - Strokes with arcs are flattened.
  - Pieces past the limits are dropped by repair.
  - Wrong-kind pieces are dropped.
  - Migration: a format-20 file gets the standard type; every sheet
    points at it; its pieces resolve to today's lines and text positions.
  - `ProjectInfo.fields` repair (duplicates, empty names).
- **cc_views**:
  - Each field key resolves: `sheet_count` and `sheet_index` with three
    sheets (natural order), `sheet_scale` mixed and uniform, `paper`
    custom.
  - Dates in pt-BR and en formats with a pinned `today`.
  - `param:` falls back to the type's default.
  - Empty values vanish; unknown keys stay; `{{` escapes.
  - An SVG and a PDF export of a sheet contain the resolved values.
- **cc_engine** (`tests/`):
  - `sheet_set` with every argument, and its errors.
  - `sheet_fields`.
  - `project_info_set` fields.
  - `component_create` title block, then add a stroke, a text and an
    image, `component_save` → `component_load` round trip with the image.
  - `view_create` gives new sheets a title block.
  - `localize_migrated` translates the captions.
- **cc_app**:
  - A UI test opens *Edit Title Block*, draws a rectangle, places a text
    with `{sheet_number}`, finishes, and checks the sheet shows the number.
  - The sheet properties rows run `sheet_set`.
  - A sheet snapshot is updated.
- **i18n**: new keys in all 12 languages.

## Delivery

Four PRs, each green on CI:

1. **Core and engine**: pieces, resolution, fields, sheets and paper,
   project fields, standard block and migration, commands, exports. Usable
   from the CLI and MCP.
2. **App, sheet side**: sheet properties, project fields and the title
   block combo.
3. **App, editor**: the Sheet view of the component environment, with the
   sketch, text and image tools.
4. **Docs**: `docs/interface*.md`, `docs/automation*.md`, and
   `docs/commands.md` regenerated (regeneration also happens in 1, as the
   test requires).

## Out of scope

- A revision table or revision list per sheet.
- Free lines, texts and images placed directly on a sheet.
- Script (Lua) pieces in title blocks.
- Per-user date formats.
- Title block types in IFC.
