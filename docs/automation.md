# Automation: engine, command line and MCP

[Português](automacao.md)

Everything ConstructCAD does to a model can be done without the interface. The **engine** (`cc_engine`) holds the operations as a catalog of named **commands**; three clients use that one catalog:

```
                 ┌──────────────────────────────┐
  desktop app ──▶│                              │
  ccad (CLI)  ──▶│  cc_engine: Session +        │──▶ the model (.ccad)
  MCP agents  ──▶│  catalog of commands         │
  Lua scripts ──▶│  (+ plugin commands)         │
                 └──────────────────────────────┘
```

A wall drawn with the mouse, a `ccad run wall_create …`, a Lua script calling `ccad.run("wall_create", …)` and an AI agent calling the `wall_create` tool run the same code, with the same defaults, checks and undo. The full list of commands with their parameters is in the [command reference](commands.md); [plugins](#plugins) add their own commands to the catalog.

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
| `script` | a Lua script or plugin failed: a syntax or run-time error, or a limit reached |

## Command line: `ccad`

`ccad` is a separate program without graphics dependencies (it runs on servers and in CI). Release packages include it; from the sources: `cargo build --release -p cc_cli` (the binary is `target/release/ccad`).

```text
ccad [options] commands             list the commands
ccad [options] schema <command>     the JSON Schema of a command's parameters
ccad reference                      the command reference in Markdown
ccad [options] run <command> [args] run one command
ccad [options] script <file|->      run a script of commands
ccad [options] lua <file|-> [args]  run a Lua script
ccad [options] plugins              list the plugins loaded with --plugins
ccad plugins install <zip> <folder> install a plugin archive into a plugins folder
ccad plugins remove <id> <folder>   remove an installed plugin
ccad [options] mcp                  serve the commands over MCP (stdio)

-p, --project <file>   open a project (.ccad, or .ifc to import) first
--save                 save the project afterwards (to its file)
--save-as <file>       save the project afterwards to <file>
--plugins <folder>     load the plugin in <folder>, or each plugin in its
                       subfolders (repeatable)
--lang <code>          language of default names and messages (also CCAD_LANG):
                       en, pt-BR, es, fr, de, it, lo, pl, uk-UA, zh-TW, zh-CN or ja
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

### Terrain

`topography_create` makes a terrain surface from measured points `[x, y, z]` (elevations above the level) inside a `boundary` or `curves`; with neither, the convex hull of the points bounds it. The boundary's vertices take the elevations the points give them, and the earth closes down to a flat base `base_depth` (default 2 m) below the lowest point, so sections cut it as a solid. `element_get` shows its lowest and highest elevations, base, area, volume and any warnings (`few-points`, `collinear`, `boundary-crossing`) in `derived.topography`. `topography_points_set` replaces the points or changes, removes and adds them by index; `sketch_set` redraws the boundary, and with `points` replaces the points in the same step. A terrain type sets the earth material and the contour lines' interval and majors (`topography_type_create`, `type_set`).

```text
{"command": "topography_create", "args": {"points": [[0, 0, 0], [20, 0, 2], [20, 10, 2.5], [0, 10, 0.5], [10, 5, 1.2]]}}
```

Scripts have no variables: a step cannot use an id created by an earlier one. Commands help by accepting positions where it matters (`door_create` and `window_create` find the wall nearest `point`; floors, ceilings and roofs follow the walls with `from_walls`). For logic between steps, write a [Lua script](#lua-scripts), call `ccad run` from a shell script or use an agent over MCP.

## Lua scripts

`ccad lua <file>` runs a [Lua 5.4](https://www.lua.org/manual/5.4/) script over the session. The script calls commands with `ccad.run`, keeps their results in variables and loops, so an id created by one step can be used by the next. Its arguments (the same forms as `run`: `key=value` pairs, a JSON object, `@file.json`) are `ccad.args`; what it prints goes to standard output and what it returns is printed as JSON.

[`examples/small_house.lua`](../examples/small_house.lua) builds the small house above, sized by its arguments:

```lua
local w = ccad.args.width or 8
local d = ccad.args.depth or 6
local corners = { { 0, 0 }, { w, 0 }, { w, d }, { 0, d } }
local outer = {}
for i = 1, 4 do
  outer[i] = ccad.run("wall_create", { start = corners[i], ["end"] = corners[i % 4 + 1], height = 2.8 }).id
end
ccad.run("door_create", { wall = outer[1], offset = w / 4 })
…
```

```sh
ccad --save-as house.ccad lua examples/small_house.lua width=10 depth=7
```

The `ccad` table:

| | |
|---|---|
| `ccad.run(name, args)` | runs a command and returns its result as a Lua table; raises an error if it fails |
| `ccad.try(name, args)` | the same, but returns `nil, err` on failure (`err.code`, `err.message`; see [Errors](#concepts)) |
| `ccad.commands()` | the commands: `name`, `kind`, `group`, `description` each |
| `ccad.describe(name)` | one command, with the JSON Schema of its parameters as `schema` |
| `ccad.args` | the script's arguments (also `...`) |
| `ccad.null` | JSON `null` (Lua's `nil` leaves a field out) |
| `ccad.array(t)` | marks `t` as a list, so that an empty table is sent as `[]` rather than `{}` |
| `ccad.json.encode(v)`, `ccad.json.decode(s)` | JSON text to and from Lua values |
| `ccad.version`, `ccad.api` | the ConstructCAD version and the plugin API version (1) |

Arguments and results are JSON: objects are tables with string keys, arrays are sequences (`{ 0, 0 }` is a point), `end` is a Lua keyword and is written `["end"]`. A whole number computed with `/` (which always gives a float in Lua) is sent as an integer, so `10 / 2` works where an id or count is expected.

**The sandbox.** Scripts run in a restricted Lua: the `table`, `string`, `math`, `utf8` and `coroutine` libraries and the base functions, without `io`, `os`, `debug`, `package`, `require`, `load`, `dofile` or `loadfile`. A script reaches the project only through commands, and files only through the commands that read or write them. A run has a budget of 500 million Lua instructions and 256 MB of memory; past them it stops with a `script` error (catching that error with `pcall` does not keep the script going). Each command a script runs is its own undo step.

## Plugins

A plugin is a folder with a `plugin.toml` manifest and Lua code that adds commands to the catalog. Its commands are listed and run like the built-in ones, by `ccad run`, scripts, Lua scripts and MCP agents, and are named `<plugin id>__<command>` (two underscores, which built-in names never contain).

```sh
ccad --plugins examples/plugins commands                # the catalog, with the plugins' commands
ccad --plugins examples/plugins plugins                 # the plugins, their permissions and commands
ccad --plugins examples/plugins run quick_rooms__rectangle_room width=4 depth=3 name=Office door=south
claude mcp add constructcad -- ccad mcp --plugins /path/to/plugins
```

`--plugins` takes a plugin's folder or a folder of plugin folders, and can be repeated. A plugin that fails to load is reported on standard error and left out; what plugins print also goes to standard error.

### Writing a plugin

[`examples/plugins/quick_rooms`](../examples/plugins/quick_rooms) is a complete example. `plugin.toml`:

```toml
id = "quick_rooms"          # lowercase letters, digits and single underscores, up to 32
name = "Quick rooms"
version = "0.1.0"
api = 1                     # the plugin API it is written for
description = "Rectangular rooms in one step, and wall totals by type."
authors = ["…"]
license = "GPL-3.0-or-later"
homepage = "https://…"
main = "main.lua"           # the default
permissions = []            # or ["files"], see below
```

`main.lua` runs once when the plugin loads and registers commands with `ccad.command`:

```lua
local rect = require("lib.rect")   -- lib/rect.lua in the plugin's folder

ccad.command {
  name = "rectangle_room",         -- the command is quick_rooms__rectangle_room
  title = "Rectangular room",
  description = "Four walls around a rectangle, its floor and its room.",
  kind = "edit",                   -- "query", "edit" (the default) or "file"
  params = {                       -- a JSON Schema of the arguments
    properties = { width = { type = "number" }, depth = { type = "number" } },
    required = { "width", "depth" },
  },
  run = function(args)
    local p = rect.corners(0, 0, args.width, args.depth)
    for i = 1, 4 do ccad.run("wall_create", { start = p[i], ["end"] = p[i % 4 + 1] }) end
    return { room = ccad.run("room_create", { point = { args.width / 2, args.depth / 2 } }).id }
  end,
}
```

- `run` gets the arguments as a table and returns the command's result (a table, a value or nothing). Missing `required` arguments are refused before it runs; `error("message", 0)` fails the command with that message.
- **One undo step:** however many commands a plugin command runs, it is undone at once, and when it fails (an error, a failed `ccad.run`, a limit reached) everything it did is undone.
- **What a command may run:** a `query` command only queries; an `edit` command queries and edits; commands that read or write files (import, export, save) need the `files` permission, and a command of kind `file` can only be declared with it. Opening or creating a project, `undo`, `redo` and `language_set` cannot run inside a plugin command, nor can another plugin's command.
- **The sandbox** is that of [Lua scripts](#lua-scripts), with the budget counted per command; each plugin has its own Lua state, which keeps its variables between commands. `require("a.b")` loads `a/b.lua` from the plugin's folder (once; files outside the folder are refused), and `ccad.plugin` holds its `id`, `name` and `version`. `ccad.run` works only while a command runs, not while the plugin loads.
- **Permissions** are granted by whoever loads the plugin: `ccad --plugins` grants what the manifest asks for, since you named the folder.

Plugins are shared through the community index, [constructcad-plugins](https://github.com/francisreboucas/constructcad-plugins): one entry per plugin, with where to download each version and its checksum, submitted by pull request. In the app, *Manage → Plugins* installs, updates, turns off and removes plugins ([Plugins](interface.md#plugins)); without it, `ccad plugins install <file.zip> <folder>` installs a downloaded archive (checked the same way: safe paths, sizes, and a plugin that loads) and `ccad plugins remove <id> <folder>` removes one.


## Scripted components

A component piece can be a **script**: Lua source that declares the component's parameters and hotspots and builds its pieces from their values. The script is pure: it sees the values and nothing else (no commands, files or model), so it runs once for each set of values; moving or rotating a placed component does not run it again. [`examples/components/cabinet.lua`](../examples/components/cabinet.lua) is a complete example.

```lua
param.length   { "depth", label = "Depth", default = 0.55 }          -- shared by every element of a type
param.choice   { "handle", { { "bar", "Bar" }, { "none", "None" } } }
param.length   { "width", label = "Width", default = 0.8, instance = true }  -- each element's own
param.boolean  { "worktop", default = true, instance = true }
param.material { "body", label = "Body", instance = true }
hotspot        { "plinth", 0, 0.05, 0, label = "Plinth", shape = "diamond", color = "#ff8800" }

function geometry(p)
  local pieces = { box { 0, 0, 0, p.width, p.depth, 0.85, material = p.body } }
  if p.worktop then pieces[#pieces + 1] = box { 0, 0, 0.85, p.width, p.depth + 0.02, 0.03 } end
  return pieces
end
```

| Declaration | Value in `p` |
|---|---|
| `param.length`, `param.angle`, `param.number { id, label=, default=0, instance= }` | a number (meters, degrees) |
| `param.boolean { id, default=false }` | `true` or `false` |
| `param.text { id, default="" }` | a string |
| `param.choice { id, { {key, label}, … }, default=first key }` | the chosen key |
| `param.material { id, default="" }` | a project material's name (empty: the piece's material) |
| `hotspot { id, x, y, z, label=, shape="circle", color="#00ffff" }` | `{ x, y, z }`, where the element has it |

Declarations go at the script's top level, never inside `geometry`. Ids are names (letters, digits, `_`) and must be distinct; changing a label does not rename a parameter. A parameter without `instance = true` belongs to the type, and changing it changes every element of the type; with it, each element keeps its own value. Hotspot shapes are `circle`, `square`, `triangle`, `diamond` and `triangle_down`; colors are `#RRGGBB` or `#RRGGBBAA`. `p` also holds the component's other parameters, its reference planes and, in doors and windows, `WallThickness`.

`geometry(p)` returns a list of pieces (lists of pieces are flattened), in the component's frame (meters from the insertion point, x to the right, y to the back, z up). Every builder takes its values in order or by name, plus `name`, `material`, `void`, `in_plan`, `plan_only` and `detail` (a list of `coarse`, `medium`, `fine`):

| Builder | Values |
|---|---|
| `box` | `x, y, z, width, depth, height` |
| `cylinder` | `x, y, z, radius, height` |
| `extrusion` | `points` (`{x, y}` in plan), `z, height` |
| `profile` | `points` (`{u, v}` or `{u, v, radius = r}` for an arc to the next point), `offset, depth`, `plane = "plan"`, `"front"` or `"side"` |
| `revolution` | `points, offset, axis, angle` (default 360), `plane` |
| `sweep` | `path, profile, offset`, `plane` |
| `blend` | `bottom, top, offset, depth`, `plane` |
| `mesh` | `vertices` (`{x, y, z}`), `faces` (lists of vertex numbers from 1, counter-clockwise seen from outside) |

Scripts run in the [sandbox](#lua-scripts) with smaller limits (20 million instructions, 64 MB, 10,000 pieces, a million points), and `math.random` repeats the same numbers. When a script fails, the element keeps the last pieces that worked and the error, with its line, shows in `element_get` (`derived.script_errors`), in the properties palette and in `component_params_get`; fixing the script or the value brings it back.

Commands:

```sh
ccad run component_create name=Cabinet 'def={"items": [{"name": "Script", "shape": {"Script": {"source": ""}}}]}'
ccad run component_script_set type=Cabinet piece=Script source=@examples/components/cabinet.lua
ccad run component_params_get type=Cabinet
ccad run component_params_set type=Cabinet name=handle value='"none"'
ccad run component_params_set 'elements=[100, 101]' name=width value=1.2
ccad run component_hotspot_move element=100 name=plinth 'point=[0, 0.08, 0]'
```

- `component_script_set` writes a script piece (or adds one). A script that does not load (a syntax error, a bad declaration) is refused; the declarations become the component's parameters and hotspots, those still declared keep their values when their kind still fits, and the result lists the errors of the first run. A component holds one script piece; its parameters are changed in the script, not with `component_param_set` or `component_param_delete`.
- `component_params_get` shows a type's parameters and hotspots, or those of placed components of one component, with `mixed` where they differ.
- `component_params_set` sets a type's value, or the instance value of several elements at once: numbers, `true`/`false`, texts, a choice's key or a material's name. It is one undo step, and nothing changes when any element refuses the value.
- `component_hotspot_move` moves a hotspot in the component's frame, or in project coordinates with `world=true`.

A material a parameter names counts as used: it cannot be deleted, and renaming it renames the values. IFC export writes the pieces the script made, as for any component.

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

Commands added at run time implement `cc_engine::Extension` (`crates/cc_engine/src/extension.rs`) and are set on the session with `Session::set_extension`; `cc_plugins::PluginHost` is the one for Lua plugins. `Session::commands`, `describe` and `kind_of` cover both, and `Session::execute` runs an extension command as one undo step (`Document::squash_since`).

Architecture details are in [AGENTS.md](../AGENTS.md).
