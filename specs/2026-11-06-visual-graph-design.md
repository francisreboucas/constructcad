# Visual programming graphs — design

Date: 2026-11-06. Status: approved; phases 1 (core) and 2 (editor) implemented.

## Goal

A node-based programming environment inside ConstructCAD, in the spirit of
the visual programming tools BIM users already know: boxes (nodes) with
input and output ports, joined by wires, evaluated as a dataflow graph.

- **Every engine command is a node.** The catalog (`Session::commands` /
  `describe`, 229 commands today, each with a JSON schema) generates the
  nodes, so every element the engine can create, read or change — walls, floors, roofs,
  doors, terrains, pads, curtain grids, MEP runs, views, sheets — is
  available in a graph with no per-element work. New commands become nodes
  automatically.
- **Lua is the scripting language of the graph.** A *Lua* node holds code
  (the same sandbox as `ccad lua` and plugins); later, plugins can declare
  nodes and a graph can be exported as a readable Lua script.
- **Re-running updates, it does not duplicate.** Elements a graph created are
  bound to the node that made them; the next run changes them in place,
  creates the new ones and deletes the ones no longer produced.
- **Reachable without the GUI**, like everything else: `graph_run` is an
  engine command (`ccad`, MCP), and graphs are plain JSON files that an agent
  can write.

Not in scope: a general geometry kernel (NURBS surfaces, solids booleans as
values). Geometry values are the planar curves and points the engine already
takes (`CurveArg`) and points `[x, y]`.

## Concepts

- **Graph**: nodes, wires, groups and notes, saved as a `.ccgraph` file
  (JSON). It has a `Uid`.
- **Node**: a function from input values to output values. Kinds:
  - `Command(name)` — an engine command (plugin commands included);
  - `Builtin(name)` — inputs, math, logic, lists, geometry, parameters,
    outputs;
  - `Lua(code)` — a Lua function;
  - later: nodes declared by plugins, and graphs used as nodes.
- **Port**: named, typed (`PortType`), with an optional default. An unwired
  input with no default is *missing*; the node does not run.
- **Value**: plain JSON (`serde_json::Value`), in the forms commands take
  and return: numbers in engine units (meters, degrees), points `[x, y]`,
  curves as `CurveArg` objects, elements as their ids. Lists are JSON
  arrays; every port may receive a list (lacing).

## Values and types

Values are the JSON the commands already speak, so a command node passes
its inputs through unchanged and no conversion layer can drift from the
catalog. Ports carry a `PortType` (`cc_engine::graph::ports`) for wiring
checks, colors and lacing:

```rust
pub enum PortType { Any, Number, Integer, Bool, Text, Point, Element, Ref, Object, List(Box<PortType>) }
```

- A command's ports come from its JSON schema (`type_of_schema`): `number`
  → Number, a 2- or 3-number array → Point, an id-or-name `anyOf` → Ref,
  integers named like elements (`id`, `wall`, `host`, …) → Element, arrays →
  List, objects and `oneOf` objects (curves) → Object; anything else is
  `Any` and passes through whole.
- `rank` is how many list levels a port takes as one value (Point 1,
  List(Point) 2, Any none). `fits` decides whether a wire is allowed (ids
  are integers are numbers; an id or a text fits a Ref).
- Element ids stay valid across runs because bound elements keep their ids
  (below). `select_elements` stores persistent ids (`values.uids`) and
  resolves them at run time, so a saved graph survives reloads.

## Lists and lacing

A value deeper than its port's rank maps the node over it:

- **Shortest** (default), **Longest** (repeats the last item), **Cross**
  (every combination), per node (`lacing`).
- Nested lists map level by level: at each step the inputs at the deepest
  level are mapped and the others stay. Ports of type `Any` (list nodes, Lua
  nodes) take the whole value.
- A single item where a list port takes a list becomes a list of one (one
  curve into `curves`).
- Each call has a **path** (its indices through the mapping: `[2, 0]`),
  which is the key of element binding.
- Bounds: `MAX_LIST` items made by a node (100 000), `MAX_CALLS` node calls
  per run (100 000), `lace::MAX_LEVELS` list levels mapped (8). Exceeding one
  is a node error, never a panic or a hang.

## Command nodes

Generated from the catalog (`eval::command_def`):

- Inputs: the schema's properties, required ones first; an unwired input
  takes the node's stored value (`values`), else the command's default.
- Outputs: `element` (the result's `id`) for every `Edit` command whose name
  ends in `_create`, and `result` (the whole result). A `get` node reads any
  field (`params.height`, `ids.0`). Output schemas for commands can come
  later.
- `File` commands run only when the graph sets `allow_files`, and never when
  `graph_run` is itself called by another command (a plugin); `project_new`,
  `project_open`, `undo`, `redo`, `language_set`, `graph_run` and
  `graph_release` never run in a graph.
- Names, groups and descriptions come from the catalog.

## Element binding (re-run updates)

The heart of the feature. Bindings live in the project, keyed by element:

```rust
// cc_core::Project
#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
pub graph_bindings: BTreeMap<ElementId, GraphBinding>,

// cc_core::graph_binding
pub struct GraphBinding {
    pub graph: Uid,      // the graph's file uid
    pub node: u64,       // the node's id inside the graph
    pub path: Vec<u32>,  // the lacing path of the call that made it
    pub index: u32,      // which of the elements that call made
}
```

A map rather than a field of `Element`: copies of an element never carry
the binding, and removing an element drops it in `EditCtx::remove_element`
(like pins). It is tracked by undo with the other settings; repair drops
bindings of missing elements, invalid ones and a second element per call.

On each run, for every call of an `Edit` command (`eval::bound_call`):

1. Take the elements bound to `(graph, node, path)` by an earlier run.
2. Run the command with them still in place; the elements with ids from the
   project's `next_id` before the call on are what it made.
3. The k-th new element takes the place of the k-th old one when they are
   of the same category (`eval::bind`): the old element gets the new content
   and keeps its id, uid, mark, comments and phases, and the new one is
   removed. What refers to the old element (hosted doors, dimensions, other
   nodes' inputs) stays valid. Old elements left over are deleted; new ones
   without an old one are bound as new.
4. If the command fails while old elements exist (a room where one already
   is), the old ones are removed and the command tried again; if that fails
   too, the removal is undone and the node errors.
5. After the run, the graph's bound elements are deleted when their node is
   gone from the graph, or when their node ran successfully and no call made
   them this time (a shorter list). Nodes that failed, were skipped or were
   reused from the cache keep theirs. Hosted elements go with their hosts.

Rules:

- With nothing changed, a run changes nothing and adds no undo step (the
  squashed step's changes cancel out and are pruned).
- **Release**: `graph_release` unbinds elements (all of a graph, or chosen
  ones); the next run makes new ones.
- An element deleted by hand is made again by the next run. Hand edits of a
  bound element are replaced by the next run, except mark, comments and
  phases.
- Commands that change elements without creating any (`element_set`,
  `parameter_set`, moves) are plain edits and run on each run; a move by a
  delta therefore accumulates, so automatic runs should avoid them.
- `FORMAT_VERSION` 21.

## Evaluation

- `Graph::order`: a node after the nodes wired into it; among the nodes
  ready to run, the lowest id first, so independent nodes run in the order
  they were added (a `roof_create` with `from_walls` added after the walls
  runs after them). Cycles are refused when wiring and dropped on load.
- `Evaluator` keeps each node's outputs keyed by a hash of its definition
  and input values; they are reused only while the document revision equals
  the one after the last run, so any model change re-runs everything (and
  binding keeps that idempotent).
- **One run = one undo step**, named after the graph (`squash_since`).
- Errors are values: a failing node reports its message (with the lacing
  path of the failing call), the nodes after it are skipped with the reason,
  independent nodes still run, and the successful edits are kept. A panic in
  a command is caught by `Session::execute` and becomes that node's error.
- Run modes (`run_mode`): **Manual** (default) and **Automatic** (the editor
  re-runs after each change of the graph).
- The evaluator is in `cc_engine`, so the GUI, `ccad graph` and MCP run the
  same code.

## Lua nodes

- A `Lua` node holds a chunk; its inputs (`inputs`, names) are locals, and
  it returns its outputs (`outputs`, names) in order, or one table with
  their names:

  ```lua
  -- inputs: n        outputs: list, size
  local t = {}
  for i = 1, n do t[i] = i * 2 end
  return t, #t
  ```

- It is a **pure function**: it runs in the `cc_script` sandbox (50 million
  instructions per call, the sandbox's memory limit, no `load`/`require`)
  with `ccad.null` and `ccad.array`, and no access to the model. Nodes that
  read or change the model are command nodes, so binding and caching stay
  exact. What it prints is returned in the run's `messages`.
- Its ports are `Any`: it takes whole values (no lacing), like list nodes.
- Input and output names must be Lua identifiers.

## Plugin nodes (phase 4)

Plugin commands are already nodes (`<id>__<name>`). Plugins will also be
able to declare nodes:

```lua
ccad.node {
  name = "stair_grid",
  group = "Structure",
  inputs = { … }, outputs = { … },
  run = function(...) … end,
}
```

Listed under the plugin in the node search, run with the plugin's grants.
A graph using a missing plugin node loads with that node marked missing
(kept, not run).

## Built-in nodes (`builtin::catalog`)

- **Input**: `number`, `integer`, `slider` (`values.min`/`max`), `boolean`,
  `text` (also a level, type, view or material by name), `select_elements`
  (persistent ids in `values.uids`), `elements_of_category`, `levels`.
- **Math**: `add`, `subtract`, `multiply`, `divide`, `modulo`, `power`,
  `min`, `max`, `abs`, `round`, `floor`, `ceiling`, `sqrt`, `sin`/`cos`/`tan`
  (degrees), `pi`, `range`, `sequence`.
- **Logic**: `equal`, `less`, `greater`, `and`, `or`, `not`, `if`.
- **Lists**: `count`, `get_item`, `first`, `last`, `reverse`, `flatten`,
  `sort`, `chop`, `transpose`, `repeat`, `filter` (by a mask), `sum`,
  `create_list`, `join_lists`, `get` (a field by path).
- **Geometry** (JSON values, never elements): `point`, `point_xy`,
  `translate`, `rotate`, `distance`, `divide_line`, `polygon_points`,
  `rectangle_points`, `polyline_segments`, and the curves `line`, `arc`,
  `circle`, `rectangle` for `curves` inputs.
- **Elements**: `parameter_get` / `parameter_set` (a path: `mark`,
  `comments`, `level`, `type`, or a field of `params` such as `height`).
- **Output**: `watch` (passes its value on), `output` (a named result of
  `graph_run`).

Input nodes with a `name` are the graph's inputs.

## File format (.ccgraph)

```json
{
  "format": 1,
  "uid": "6f1c2a0e9b3d4c8a8e2f5a7b1c3d9e01",
  "name": "Parametric house",
  "run_mode": "manual",
  "allow_files": false,
  "nodes": [
    { "id": 1, "kind": { "builtin": "number" }, "name": "width", "pos": [0, 0], "values": { "value": 10 } },
    { "id": 7, "kind": { "command": "wall_create" }, "pos": [800, 0], "lacing": "shortest" },
    { "id": 9, "kind": { "lua": "return x * 2" }, "inputs": ["x"], "outputs": ["result"] }
  ],
  "wires": [ { "from": [1, "value"], "to": [9, "x"] } ],
  "notes": [], "groups": []
}
```

- Hostile input (`Graph::from_json`): file size, node, wire and text counts
  bounded (`MAX_FILE` 32 MB, `MAX_NODES` 5 000, `MAX_WIRES` 20 000,
  `MAX_TEXT` 256 KB per node), repeated node ids and non-finite positions
  dropped, wires that dangle, repeat an input or close a cycle dropped (the
  count is reported as `repairs`), a missing uid generated, unknown node
  kinds kept and reported as nodes that cannot run.

## Engine commands (`cc_engine::commands::graph`, group "Visual programming")

- `graph_run { path | graph, inputs, brief }` — run a graph (edit, one undo
  step); returns per-node outputs, errors and skips, the `output` nodes'
  values, counts (created, changed, deleted, errors), Lua output, the
  graph's uid and the repairs made reading it.
- `graph_check { path | graph }` — problems without running: nodes that
  cannot run, required inputs with no value, wires to missing ports or whose
  types do not fit.
- `graph_nodes { search?, group? }` — the node catalog (built-ins, the Lua
  node, a node per command) with ports and types, so agents can author
  graphs.
- `graph_elements { graph }` — the elements bound to a graph (uid), with
  node, path and index.
- `graph_release { graph, elements? }` — unbind elements.

`ccad graph house.ccgraph width=12` runs `graph_run` with `brief` (exit
code 1 when a node failed).

## Where the code lives

Everything is in `cc_engine::graph` (`mod.rs` file format and graph edits,
`ports.rs` types and schema mapping, `lace.rs` lacing, `builtin.rs`
built-in nodes, `eval.rs` evaluator and binding) plus the commands; it
needs only `cc_script` (already a dependency through `cc_geom`) and `mlua`
for Lua nodes, so no new crate and no `Extension`. `cc_core` gains
`graph_binding` and `Project::graph_bindings`, and `Project::next_id`
(read-only).

## App (cc_app)

- **Graph editor** in a resizable panel under the views (the dock's tabs
  hold views), so the model and the graph are seen together; opened from
  *Manage → Macros → Visual Programming*, or at startup with `--graph`.
- Canvas: pan, zoom, grid; nodes with title, ports and inline value editors;
  bezier wires colored by type; box selection, copy/paste, notes; undo of
  graph edits (separate from model undo).
- Node search: right click or double click opens a search over the node
  catalog (by name, group, description), placing the node at the cursor.
- Preview: each node shows its value (lists with their counts); the
  elements of the selected node are selected in the views.
- Status line with run mode, *Run*, and errors; failing nodes in the error
  color with the message on hover.
- All text through `t!` in every `.ftl`; ribbon entries are `UiCommand`s;
  dialogs use `dialog::Dialog`; icons in `assets/icons/ui`.
- Node editor widget: written in-house on egui `Painter` (pan/zoom
  transform, hit testing, wires), no node-editor dependency.

## Phases

1. **Core** (no UI) — done: `cc_engine::graph`, binding in `cc_core`, the
   five commands, `ccad graph`, `examples/graphs/house.ccgraph`, docs.
2. **Editor** — done: `cc_app::graph_ui`, the panel, canvas, search,
   inspector, values on nodes, manual and automatic runs, selection sync,
   UI tests.
3. **Preview**: geometry values drawn over plan and 3D views.
4. **Reuse**: custom nodes (graphs as nodes), plugin nodes (`ccad.node`),
   a Player panel with only the inputs, `graph_export_lua`, a graph library
   folder in Preferences.

## Tests

- `crates/cc_engine/tests/graph.rs`: lists over a command feeding another
  (walls and their doors), one undo step; re-runs in place (same ids, hand
  comments kept, no undo step when nothing changed, a shorter list deletes
  the last wall and its door, a changed value moves the walls, undo restores
  walls and bindings); deleted elements made again and released ones left
  alone; removing a node deletes its elements; a failing node keeps its
  elements while other branches run; Lua nodes (several outputs, a table of
  outputs, the instruction budget, print); nodes that cannot run (refused,
  file, unknown, bad input, skipped downstream); every command is a node
  with the expected port types; a list of outlines and a single curve into
  list ports; hostile graphs (missing source, a graph inside a graph, too
  many calls); the example house graph.
- Unit tests: port types from schemas, ranks and depths, lacing (shortest,
  longest, cross, nested, paths, budget, depth), file repair and cycles,
  built-in math and lists.
- UI tests (egui_kittest), with the editor: add a node from the search,
  wire two nodes, run, read the value.

## Decisions

1. Graphs live in `.ccgraph` files (shareable, like macros); bindings are
   keyed by the graph's uid, so embedding graphs in the project can come
   later without changing them.
2. Hand edits on bound elements are replaced by the next run, except mark,
   comments and phases.
3. The node editor is written in-house on egui.
4. Command outputs are `element` (for `*_create`) and `result`; a `get`
   node reads fields. Output schemas can come later.
5. Name: *Visual Programming* / pt-BR *Programação Visual*; extension
   `.ccgraph`.
