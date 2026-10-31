# Automation: engine, command line and MCP

[Português](automacao.md)

Everything ConstructCAD does to a model can be done without the interface. The **engine** (`cc_engine`) holds the operations as a catalog of named **commands**; three clients use that one catalog:

```
                 ┌──────────────────────────────┐
  desktop app ──▶│                              │
  ccad (CLI)  ──▶│  cc_engine: Session +        │──▶ the model (.ccad)
  MCP agents  ──▶│  catalog of commands         │
                 └──────────────────────────────┘
```

A wall drawn with the mouse, a `ccad run wall_create …` and an AI agent calling the `wall_create` tool run the same code, with the same defaults, checks and undo. The full list of commands with their parameters is in the [command reference](commands.md).

## Concepts

**Session.** One open project: the document (with its undo history) plus geometry and drawings derived from it. The CLI opens one per invocation (or script), the MCP server one per connection, the app one per window.

**Commands.** Each command has a `snake_case` name, a JSON object of parameters described by a JSON Schema, and a JSON result. Kinds:

| Kind | Does | Examples |
|---|---|---|
| query | reads the model | `elements_list`, `element_get`, `schedule_table` |
| edit | changes the model: one undo step | `wall_create`, `elements_move`, `element_set` |
| file | works with files or replaces the project | `project_open`, `ifc_import`, `sheet_export` |

**Units and coordinates.** Lengths are meters, angles degrees, points `[x, y]` in plan. Dimensions use the coordinates of their view's drawing (plan X/Y in plans; horizontal position and height in elevations and sections). Areas and volumes in results are m² and m³.

**References.** Levels, types, views and materials are given by **id or name** (`"level": 3` or `"level": "Ground"`; names are matched without regard to case; an ambiguous name is an error asking for the id). Sheets can also be named by their number (`"A101"`). Elements are given by id; `elements_list` and the results of creating commands give them. Omitted levels default to the lowest level and omitted types to the category's default type.

**Reading and changing parameters.** `element_get`, `type_get` and `view_get` show an object's `params`; `element_set`, `type_set` and `view_set` take the fields to change in the same shape and merge them in:

```sh
ccad -p house.ccad run element_get id=42
ccad -p house.ccad --save run element_set id=42 'params={"height": 2.7}'
ccad -p house.ccad --save run element_set id=57 'params={"shape": {"Gable": {"slope": 0.5, "ridge_deg": 0}}}'
```

Objects merge field by field; arrays and plain values replace; `null` clears an optional value. `material_set`, `units_set` (shaped like `material_get`, `units_get`) work alike.

**View graphics.** What a view shows and how is part of the view: `view_graphics_get` lists every category's visibility and overrides, the applied filters and the element overrides; `view_graphics_set` shows or hides a category and overrides its projection and cut lines (color, weight in paper mm, pattern), surface and cut fills (color, pattern), halftone and transparency; `element_override_set` and `element_hide_in_view` do the same for single elements in one view. Overrides are objects like `{"cut_lines": {"color": [255, 0, 0], "weight": 0.5, "pattern": "Dash"}, "cut_fill": {"pattern": "Solid", "color": [200, 200, 200]}, "halftone": true, "transparency": 0.4}`; they merge like `params` (`null` resets an attribute, `reset: true` starts over). They apply to plans, ceiling plans, elevations, sections, 3D views, sheets and PDF/SVG exports.

```sh
ccad -p house.ccad --save run view_graphics_set view=Térreo category=door visible=false
ccad -p house.ccad --save run filter_create name=Long 'categories=["wall"]' 'rules=[{"param": "length", "op": "greater", "value": 5}]'
ccad -p house.ccad --save run view_filter_add view=Térreo filter=Long 'overrides={"cut_lines": {"color": [255, 0, 0]}}'
ccad -p house.ccad run filter_elements filter=Long
```

*Filters* (`filter_create`, `filter_set`, `filter_delete`, `filters_list`, `filter_elements`) select elements by category and rules on parameters — any schedule field (`type`, `mark`, `comments`, `level`, `material`, `system`, `structural`, `width`, `height`, `thickness`, `length`, `area`, `volume`, `diameter`…) or `elevation` (of the element's level) — with `equals`, `not_equals`, `contains`, `not_contains`, `begins_with`, `not_begins_with`, `greater`, `greater_or_equal`, `less`, `less_or_equal`, `has_value` and `has_no_value`, all of them (default) or any (`any: true`). A rule on a field that none of the filter's categories has (`width` on walls) is refused, since it could never pass. Quantities are compared in SI units (meters), text without case. A view applies filters in order (`view_filter_add`, `view_filter_set`, `view_filter_move`, `view_filter_remove`): the first matching filter wins for each attribute, an element's own overrides beat the filters, and filters beat the category; an element is hidden when it is hidden in the view, its category is off, or any matching filter hides it.

*View templates* (`template_create` from a view, `template_set`, `template_delete`, `templates_list`) hold a scale, category graphics, filters, the plan cut height, the building services display (services over a faint architecture) and the structural display, each with an `include` flag. `view_template_apply` copies them into views once; `view_template_assign` makes views follow a template: the included properties are copied now and on every change of the template (in the same undo step), and editing them in the view is refused. Deleting a template frees its views, which keep their properties.

*Temporary hide/isolate* (`view_temporary` with `action` `hide`, `isolate`, `hide_category`, `isolate_category` or `reset`) is display state of the session, not a model change: it is not saved, not undone and not printed (sheets and exports show the view whole). `view_temporary_apply` makes it permanent (hidden in the view, categories turned off) as one undo step.

**Construction phases.** A renovation is modeled in phases (`phases_list`; the template has *Existing* and *New Construction*): every model element is built in a phase (`phase_created`) and may be demolished in the same or a later one (`phase_demolished`), as `element_get` shows. `phase_create` (at the end or `before` another), `phase_set` (rename, or move with `index`; a move that would demolish elements before building them is refused) and `phase_delete` (a phase in use needs `merge_into`) manage them. New elements are built in the session's creation phase (`phase_creation_set`; the active view's in the app, else the last phase), copies too. `elements_phase_set` sets `created` and `demolished` (`null`: standing) and `elements_demolish` demolishes in a `phase` or the phase of a `view`. Doors, windows and rebar follow their hosts: they are never older than their host and are demolished with it unless demolished earlier. Dimensions, text and grids have no phase.

```sh
ccad -p house.ccad --save run elements_phase_set 'ids=[12, 13]' created=Existing
ccad -p house.ccad --save run elements_demolish 'ids=[13]' view=Térreo
ccad -p house.ccad --save run view_set view=Térreo 'phase_filter=Show Previous + Demo'
ccad -p house.ccad run phases_list
```

Each plan, elevation, section, 3D view and schedule shows the model in its `phase` with a `phase_filter` (`view_set`; view templates control the filter, not the phase). Relative to the view's phase an element is *existing* (built earlier, standing), *demolished* (in this phase), *new* (built in this phase) or *temporary* (built and demolished in it); elements built later or demolished earlier are not shown. Phase filters (`phase_filters_list`, `phase_filter_create`, `phase_filter_set`, `phase_filter_delete`; the template has *Show All*, *Show Complete*, *Show New*, *Show Previous + Demo*, *Show Previous + New*, *Show Previous Phase* and *Show Demo + New*) map each status to `not_displayed`, `by_category` or `overridden`, which uses the project's graphics of the status (`phase_graphics_get`, `phase_graphics_set`: existing faint, demolished dashed without cut pattern, temporary faint and dashed), below element overrides and view filters and above the category. A view whose phase leaves elements out gets geometry of its own: doors of later phases do not open existing walls and walls join only with walls of the phase. A room is bounded by the walls standing at the end of the phase it is created in, so walls demolished in that phase or earlier do not bound it. Schedules list what their phase and filter show and offer the fields `PhaseCreated` and `PhaseDemolished` (also usable in filter rules as `phase_created`, `phase_demolished`).

**Imported CAD drawings.** `cad_import` reads a DWG (R12 to 2018 and later, through the acadrust library) or DXF file (ASCII or binary, read by ConstructCAD itself) into a floor plan, ceiling plan, elevation, section or legend as one undo step, and reports what it read: `format`, `version`, `units` (and `units_assumed`), `layers`, `entities`, `blocks`, `primitives`, `skipped` (entities not drawn, by type), `truncated` and the `extents` in the view. Blocks are inserted (nested, scaled, rotated, arrayed), arcs and circles stay arcs, ellipses, splines and hatch boundaries become polylines, texts and MTEXT keep their position, height and rotation; 3D solids, images, infinite lines and similar entities are skipped. `position` with `align` places the drawing's origin (`origin`, default), the center of its extents (`center`) or the origin unless it is more than 1 km from the view's content (`auto`); `units` overrides the file's (`mm`, `cm`, `dm`, `m`, `km`, `in`, `ft`, `yd`, `mi`), and unitless files are read as meters when small, else millimeters. The element starts pinned and only its view draws it, under the model (category `cadimport`; 3D views and IFC export leave it out). `element_get` shows its placement in `params` (`units`, `scale`, `position`, `rotation_deg`, `mirrored`, `hidden_layers`, `color_mode` `Grey`/`Layers`/`Black`, `halftone`) but not the drawing; `element_set` changes them; the move, rotate, mirror, pin and delete commands work on it. `cad_layers` lists its layers and `cad_layers_set` shows or hides them.

```sh
ccad -p house.ccad --save run cad_import path=survey.dwg view=Térreo align=auto
ccad -p house.ccad --save run element_set id=120 'params={"units": "Centimeters", "halftone": true}'
ccad -p house.ccad --save run cad_layers_set id=120 'layers=["DIM", "TEXT"]' visible=false
```

**Validation and undo.** Every edit is one undo step (`undo`, `redo`). An edit that would leave the model invalid — a door pointing at a missing wall, a negative size, a reference to nothing — is refused and leaves no trace. Pinned elements are not moved, trimmed or deleted.

**Errors.** A failed command returns `{"error": {"code": …, "message": …}}`. Messages are in the session's language (`--lang`, `language_set`).

| Code | Meaning |
|---|---|
| `unknown_command` | no command has that name |
| `invalid_arguments` | the parameters do not match the schema or make no sense (zero length, unknown kind…) |
| `not_found` | a referenced element, type, level, view or material does not exist |
| `rejected` | the edit would leave the model invalid, or is not allowed (pinned, in use…) |
| `io` | reading or writing a file failed |
| `internal` | a bug; the project is kept as it was |

## Command line: `ccad`

`ccad` is a separate program without graphics dependencies (it runs on servers and in CI). Release packages include it; from the sources: `cargo build --release -p cc_cli` (the binary is `target/release/ccad`).

```text
ccad [options] commands             list the commands
ccad [options] schema <command>     the JSON Schema of a command's parameters
ccad reference                      the command reference in Markdown
ccad [options] run <command> [args] run one command
ccad [options] script <file|->      run a script of commands
ccad [options] mcp                  serve the commands over MCP (stdio)

-p, --project <file>   open a project (.ccad, or .ifc to import) first
--save                 save the project afterwards (to its file)
--save-as <file>       save the project afterwards to <file>
--lang <code>          language of default names and messages (also CCAD_LANG):
                       en, pt-BR, es, fr, de, it, lo, pl, uk-UA, zh-TW or zh-CN
```

Arguments of `run` can be a JSON object, `@file.json`, `-` (standard input) or `key=value` pairs, whose values are read as JSON when they parse and as text otherwise:

```sh
ccad -p house.ccad --save run wall_create start=[0,0] end=[5,0] height=2.8
ccad -p house.ccad --save run wall_create start=[5,0] end=[5,4] mid=[6,2]   # an arc wall through mid
ccad -p house.ccad run elements_list category=door
ccad -p house.ccad --save run view_set '{"view": "Section 1", "scale": 100}'
ccad -p model.ifc --save-as model.ccad run project_info              # import an IFC file
ccad -p house.ccad run sheet_export sheet=A101 path=a101.pdf
ccad -p house.ccad run view_export view="Section 1" path=section.png dpi=300   # any view as PNG, SVG or PDF
ccad -p house.ccad run view_export view="Level 1" path=level1.dxf              # or as DXF, at full size in mm
```

Results are printed as JSON on standard output. Errors go to standard error as JSON with exit code 1; usage errors exit with 2. Without `--save` or `--save-as` the project file is not changed.

Results combine with other tools; for example, with `jq`:

```sh
wall=$(ccad -p house.ccad --save run wall_create start=[0,0] end=[6,0] | jq .id)
ccad -p house.ccad --save run door_create wall=$wall offset=1.5
```

### Scripts

A script is a list of steps, either a JSON array of `{"command": …, "args": {…}}` objects or one such object per line (lines starting with `//` are comments). `ccad script` runs the steps in order in one session and stops at the first error; each step is one undo step.

[`examples/small_house.ccmacro`](../examples/small_house.ccmacro) builds a small house:

```text
// A small house built with engine commands only.
{"command": "wall_create", "args": {"start": [0, 0], "end": [8, 0], "height": 2.8}}
…
{"command": "door_create", "args": {"point": [2, -0.5]}}
{"command": "floor_create", "args": {"from_walls": true}}
{"command": "room_create", "args": {"point": [2, 3], "name": "Living room"}}
```

```sh
ccad --save-as house.ccad script examples/small_house.ccmacro
```

### Outlines drawn with curves

Floors, ceilings, roofs, foundation slabs and masses take their outline as `curves` instead of points: lines, arcs (by `bulge` or a point they pass `through`), circles, ellipses (whole or a part), rectangles and regular polygons (these two become lines). Loops inside the outline are openings of floors and roofs. The element keeps its curves: `sketch_get` gives them back (or lines along the outline of an element drawn with points) and `sketch_set` draws the outline again; moving, rotating and mirroring the element moves them too, and editing the outline's points directly drops them.

```text
{"command": "mass_create", "args": {"curves": [{"kind": "circle", "center": [0, 0], "radius": 5}], "height": 9}}
{"command": "floor_create", "args": {"curves": [{"kind": "rectangle", "corner": [0, 0], "opposite": [10, 8]}, {"kind": "circle", "center": [5, 4], "radius": 1}]}}
```

Scripts have no variables: a step cannot use an id created by an earlier one. Commands help by accepting positions where it matters (`door_create` and `window_create` find the wall nearest `point`; floors, ceilings and roofs follow the walls with `from_walls`). For logic between steps, call `ccad run` from a shell script or use an agent over MCP.

## Macros in the app

*Manage → Macros → Record Macro* records the changes you make until you press *Stop Recording*; the macro is saved as a script (`.ccmacro`, the format above). *Run Macro* plays a macro or any script on the open project; `ccad script` runs the same files. With a macros folder set in *Preferences → File locations*, the macro file dialogs start there and *Run Macro* is a menu of its `.ccmacro` files (plus *Browse…* for any other file).

- Only edits are recorded: opening, saving and exporting are left out, so replaying never overwrites files.
- A value dragged in the properties is recorded once, with its final value; changes made in a dialog that you cancel are not recorded.
- Recorded steps refer to elements by id. Played on the project it was recorded on (or on a copy of it as it was when recording started) a macro reproduces the same changes; on another project, steps that name elements by id may fail or reach other elements.

## Command palette in the app

**Ctrl+Shift+P** (or the magnifier at the end of the ribbon tabs) opens the command palette. It finds interface commands, views to open or create and the engine's commands by name, description or shortcut, without regard to accents or case. Engine commands open a form with their JSON arguments, prefilled with the required fields; **Run** (Ctrl+Enter) executes them in the open project, as one undo step, and shows the result. File commands (`project_open`, `ifc_export`…) are left to the *File* tab.

Every ribbon button's tooltip names the engine command it runs, so what you do by hand can be written as a script.

## MCP server

`ccad mcp` is a [Model Context Protocol](https://modelcontextprotocol.io/) server on standard input/output. Every command is a tool with the same name, description and input schema; queries are marked read-only. The server keeps one session, so an agent opens or creates a project, works on it over many calls and saves it.

**Claude Code:**

```sh
claude mcp add constructcad -- ccad mcp
# or with a project open from the start
claude mcp add constructcad -- ccad mcp -p /path/to/house.ccad
```

**Claude Desktop and other clients** (`claude_desktop_config.json` or the client's MCP settings; use the full path of `ccad`):

```json
{
  "mcpServers": {
    "constructcad": { "command": "/path/to/ccad", "args": ["mcp", "--lang", "en"] }
  }
}
```

Requests an agent can handle with these tools, for example: *"open house.ccad, list the doors without a mark and number them"*, *"import model.ifc and tell me the volume of the concrete walls"*, *"add a second floor 3 m above the ground floor with the same exterior walls"*, *"export sheet A101 as PDF"*.

Notes:

- Paths are on the machine where `ccad` runs.
- Errors come back as tool results with `isError` and the error object, so the model can correct its call.
- The server speaks protocol revisions 2025-06-18, 2025-03-26 and 2024-11-05 and is implemented in `crates/cc_cli/src/mcp.rs` (JSON-RPC 2.0, one message per line).

## For developers: adding a command

Every user-facing feature starts as an engine command; the app calls it rather than editing the document itself.

1. Write the parameter struct in `crates/cc_engine/src/commands/<area>.rs`: `#[derive(Debug, Deserialize, Serialize, JsonSchema)]`, `#[serde(deny_unknown_fields)]`, a doc comment (the command's description) and a doc comment on every field (its parameter description; a test fails without them).
2. Implement `Command` with `NAME` (`snake_case`), `KIND` and `run`. Resolve references with `refs`, change the model inside `commands::edit` (one undo step, refused if the result needs repair), and return JSON (`created(s, id)` for elements). Use `t!` messages for errors people will read.
3. Register it in its group in `registry.rs`.
4. Test it in `crates/cc_engine/tests/commands.rs` through `Session::execute`.
5. Regenerate the reference: `cargo run -p cc_cli -- reference > docs/commands.md` (a test fails while it is stale).
6. In the app, call it with `Session::run(Command { … })`, `actions::run_command` or the `engine_edit` helpers (use `Session::run_merged` for dragged values). That also makes it recordable in macros.

Architecture details are in [AGENTS.md](../AGENTS.md).
