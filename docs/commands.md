# Command reference

<!-- Generated from the engine's catalog: `cargo run -p cc_cli -- reference > docs/commands.md`. Do not edit by hand. -->

Every command runs the same way from the desktop app, `ccad run` / `ccad script` and MCP (see [the automation guide](automation.md)). Lengths are in meters, angles in degrees, points `[x, y]` in plan (in a view's drawing coordinates for dimensions). *id or name* parameters accept a numeric id or the name of a level, type, view or material. Kinds: **query** reads the model, **edit** changes it (one undo step, refused if it would leave the model invalid), **file** works with files or replaces the project.

## Contents

- **Project and files**: [`project_new`](#project_new), [`project_open`](#project_open), [`project_save`](#project_save), [`project_info`](#project_info), [`project_info_set`](#project_info_set), [`undo`](#undo), [`redo`](#redo), [`units_get`](#units_get), [`units_set`](#units_set), [`language_set`](#language_set), [`ifc_import`](#ifc_import), [`ifc_export`](#ifc_export), [`sheet_export`](#sheet_export), [`view_export`](#view_export), [`schedule_export_csv`](#schedule_export_csv), [`component_load`](#component_load), [`component_library_list`](#component_library_list), [`component_save`](#component_save), [`cad_import`](#cad_import), [`cad_layers`](#cad_layers), [`cad_layers_set`](#cad_layers_set)
- **Queries**: [`categories_list`](#categories_list), [`levels_list`](#levels_list), [`types_list`](#types_list), [`type_get`](#type_get), [`elements_list`](#elements_list), [`element_get`](#element_get), [`groups_list`](#groups_list), [`measure`](#measure), [`views_list`](#views_list), [`view_get`](#view_get), [`materials_list`](#materials_list), [`material_get`](#material_get), [`schedule_table`](#schedule_table)
- **Creating elements**: [`wall_create`](#wall_create), [`door_create`](#door_create), [`window_create`](#window_create), [`floor_create`](#floor_create), [`floor_opening_add`](#floor_opening_add), [`floor_opening_remove`](#floor_opening_remove), [`ceiling_create`](#ceiling_create), [`roof_create`](#roof_create), [`roof_edge_set`](#roof_edge_set), [`roof_opening_remove`](#roof_opening_remove), [`room_create`](#room_create), [`grid_create`](#grid_create), [`column_create`](#column_create), [`beam_create`](#beam_create), [`columns_at_grids`](#columns_at_grids), [`beams_on_grids`](#beams_on_grids), [`beams_between_columns`](#beams_between_columns), [`slab_join_supports`](#slab_join_supports), [`structure_number`](#structure_number), [`brace_create`](#brace_create), [`braces_between_columns`](#braces_between_columns), [`truss_create`](#truss_create), [`rebar_create`](#rebar_create), [`rebar_typical`](#rebar_typical), [`analytical_model`](#analytical_model), [`analytical_export`](#analytical_export), [`footing_create`](#footing_create), [`strip_footing_create`](#strip_footing_create), [`foundation_slab_create`](#foundation_slab_create), [`footings_under_columns`](#footings_under_columns), [`strip_footings_under_walls`](#strip_footings_under_walls), [`stair_create`](#stair_create), [`stair_check`](#stair_check), [`stair_railings_add`](#stair_railings_add), [`ramp_create`](#ramp_create), [`ramp_check`](#ramp_check), [`roof_check`](#roof_check), [`mass_create`](#mass_create), [`sketch_get`](#sketch_get), [`sketch_set`](#sketch_set), [`mass_floors_set`](#mass_floors_set), [`mass_floors`](#mass_floors), [`mass_type_create`](#mass_type_create), [`mass_face_move`](#mass_face_move), [`mass_walls_create`](#mass_walls_create), [`mass_slabs_create`](#mass_slabs_create), [`mass_roof_create`](#mass_roof_create), [`ramp_type_create`](#ramp_type_create), [`railing_create`](#railing_create), [`component_place`](#component_place)
- **Building services**: [`pipe_create`](#pipe_create), [`conduit_create`](#conduit_create), [`systems_list`](#systems_list), [`system_create`](#system_create), [`system_set`](#system_set), [`system_delete`](#system_delete), [`system_assign`](#system_assign), [`circuit_loads`](#circuit_loads), [`mep_fittings`](#mep_fittings), [`mep_connect`](#mep_connect), [`mep_sizing`](#mep_sizing), [`mep_size_apply`](#mep_size_apply)
- **Modifying elements**: [`elements_delete`](#elements_delete), [`elements_move`](#elements_move), [`elements_copy`](#elements_copy), [`elements_rotate`](#elements_rotate), [`elements_mirror`](#elements_mirror), [`elements_pin`](#elements_pin), [`element_flip`](#element_flip), [`elements_turn`](#elements_turn), [`elements_export`](#elements_export), [`elements_paste`](#elements_paste), [`wall_join`](#wall_join), [`geometry_join`](#geometry_join), [`geometry_unjoin`](#geometry_unjoin), [`group_create`](#group_create), [`group_ungroup`](#group_ungroup), [`trim_corner`](#trim_corner), [`trim_extend`](#trim_extend), [`element_split`](#element_split), [`element_offset`](#element_offset), [`element_align`](#element_align), [`element_move_end`](#element_move_end), [`grids_align_ends`](#grids_align_ends), [`element_set`](#element_set), [`elements_set_type`](#elements_set_type)
- **Dimensions and text**: [`dimension_create`](#dimension_create), [`tag_move`](#tag_move), [`dimension_chain_create`](#dimension_chain_create), [`dimension_ordinate_create`](#dimension_ordinate_create), [`dimension_set_value`](#dimension_set_value), [`text_create`](#text_create), [`text_add_leader`](#text_add_leader), [`text_remove_leader`](#text_remove_leader)
- **Types, components, materials**: [`type_set`](#type_set), [`type_duplicate`](#type_duplicate), [`type_delete`](#type_delete), [`component_create`](#component_create), [`component_get`](#component_get), [`component_param_add`](#component_param_add), [`component_param_set`](#component_param_set), [`component_param_delete`](#component_param_delete), [`component_ref_plane_add`](#component_ref_plane_add), [`component_ref_plane_set`](#component_ref_plane_set), [`component_ref_plane_delete`](#component_ref_plane_delete), [`component_ref_line_add`](#component_ref_line_add), [`component_ref_line_delete`](#component_ref_line_delete), [`component_dimension_add`](#component_dimension_add), [`component_dimension_set`](#component_dimension_set), [`component_dimension_delete`](#component_dimension_delete), [`component_equal_add`](#component_equal_add), [`component_equal_delete`](#component_equal_delete), [`component_piece_add`](#component_piece_add), [`component_piece_set`](#component_piece_set), [`component_piece_delete`](#component_piece_delete), [`material_create`](#material_create), [`material_duplicate`](#material_duplicate), [`material_rename`](#material_rename), [`material_set`](#material_set), [`material_delete`](#material_delete)
- **Levels and views**: [`level_create`](#level_create), [`level_set`](#level_set), [`level_delete`](#level_delete), [`view_create`](#view_create), [`view_set`](#view_set), [`view_duplicate`](#view_duplicate), [`view_delete`](#view_delete), [`sheet_place_view`](#sheet_place_view), [`sheet_remove_viewport`](#sheet_remove_viewport)
- **View graphics**: [`view_graphics_get`](#view_graphics_get), [`view_graphics_set`](#view_graphics_set), [`element_override_set`](#element_override_set), [`element_hide_in_view`](#element_hide_in_view), [`filters_list`](#filters_list), [`filter_create`](#filter_create), [`filter_set`](#filter_set), [`filter_delete`](#filter_delete), [`filter_elements`](#filter_elements), [`view_filter_add`](#view_filter_add), [`view_filter_set`](#view_filter_set), [`view_filter_remove`](#view_filter_remove), [`view_filter_move`](#view_filter_move), [`templates_list`](#templates_list), [`template_create`](#template_create), [`template_set`](#template_set), [`template_delete`](#template_delete), [`view_template_apply`](#view_template_apply), [`view_template_assign`](#view_template_assign), [`view_temporary`](#view_temporary), [`view_temporary_apply`](#view_temporary_apply)
- **Phases**: [`phases_list`](#phases_list), [`phase_create`](#phase_create), [`phase_set`](#phase_set), [`phase_delete`](#phase_delete), [`elements_phase_set`](#elements_phase_set), [`elements_demolish`](#elements_demolish), [`phase_creation_set`](#phase_creation_set), [`phase_filters_list`](#phase_filters_list), [`phase_filter_create`](#phase_filter_create), [`phase_filter_set`](#phase_filter_set), [`phase_filter_delete`](#phase_filter_delete), [`phase_graphics_get`](#phase_graphics_get), [`phase_graphics_set`](#phase_graphics_set)

## Project and files

### `project_new`

*file* — Start a new project from the template (unsaved changes are discarded).

No parameters.

### `project_open`

*file* — Open a ConstructCAD project (.ccad), replacing the session's project. Damaged files are repaired and the number of repairs reported.

| Parameter | Type | | Description |
|---|---|---|---|
| `path` | string | required | Path of the .ccad file. |

### `project_save`

*file* — Save the project to its file, or to `path` (which becomes its file).

| Parameter | Type | | Description |
|---|---|---|---|
| `path` | string | optional | Where to save; required when the project has no file yet. |

### `project_info`

*query* — Project information, file, unsaved state and counts.

No parameters.

### `project_info_set`

*edit* — Change the project information (title block fields).

| Parameter | Type | | Description |
|---|---|---|---|
| `address` | string | optional | Project address. |
| `author` | string | optional | Author of the drawings. |
| `client` | string | optional | Client name. |
| `name` | string | optional | Project name. |
| `number` | string | optional | Project number. |

### `undo`

*edit* — Undo the last edit.

No parameters.

### `redo`

*edit* — Redo the last undone edit.

No parameters.

### `units_get`

*query* — The project units (length, area, volume, angle, slope, currency, number format).

No parameters.

### `units_set`

*edit* — Change the project units: `values` is merged into the object `units_get` returns (only the fields given change), after `preset` when given. Length units are `Millimeter`, `Centimeter`, `Meter`, `Foot`, `Inch`, `FeetInches` (`12'-6 1/2"`) and `FractionalInch`; for the last two, `decimals` is the precision, 1/2^decimals of an inch.

| Parameter | Type | | Description |
|---|---|---|---|
| `preset` | string | optional | Start from `metric` (the defaults) or `imperial` (feet and inches to 1/16", square and cubic feet). |
| `values` | JSON | optional | Fields to change, shaped like the result of `units_get`. |

### `language_set`

*edit* — Language of default names, messages and sheet labels (`en`, `pt-BR`, `es`, `fr`, `de`, `it`, `lo`, `pl`, `uk-UA`, `zh-TW` or `zh-CN`).

| Parameter | Type | | Description |
|---|---|---|---|
| `language` | string | required | Language code: `en`, `pt-BR`, `es`, `fr`, `de`, `it`, `lo`, `pl`, `uk-UA`, `zh-TW` or `zh-CN`. |

### `ifc_import`

*file* — Import an IFC file (IFC2X3 or IFC4), replacing the session's project. The result has no file yet; save it with `project_save` and a path.

| Parameter | Type | | Description |
|---|---|---|---|
| `path` | string | required | Path of the .ifc file. |

### `ifc_export`

*file* — Export the project as IFC4.

| Parameter | Type | | Description |
|---|---|---|---|
| `path` | string | required | Path of the .ifc file to write. |

### `sheet_export`

*file* — Export a sheet as vector PDF or SVG (by `format`, else by the file extension).

| Parameter | Type | | Description |
|---|---|---|---|
| `path` | string | required | Path of the file to write (.pdf or .svg). |
| `sheet` | id or name | required | Sheet id or name. |
| `format` | string | optional | `pdf` or `svg`. |

### `view_export`

*file* — Export any view with a drawing (plan, elevation, section, 3D view as a hidden-line axonometric, legend, drafting view, diagram or sheet) as a PNG image, or as vector SVG or PDF (by `format`, else by the file extension). The view is drawn at its scale as printed, without temporary hiding, on a page that fits it; sheets keep their paper size.

| Parameter | Type | | Description |
|---|---|---|---|
| `path` | string | required | Path of the file to write (.png, .svg or .pdf). |
| `view` | id or name | required | View id or name. |
| `dpi` | number | optional | Resolution of a PNG, in dots per inch (default 150). |
| `format` | string | optional | `png`, `svg` or `pdf`. |

### `schedule_export_csv`

*file* — Export a schedule view as CSV (UTF-8; semicolons when the decimal comma is used).

| Parameter | Type | | Description |
|---|---|---|---|
| `path` | string | required | Path of the .csv file to write. |
| `view` | id or name | required | Schedule view id or name. |

### `component_load`

*edit* — Load a component file (.ccomp): its types and missing materials join the project.

| Parameter | Type | | Description |
|---|---|---|---|
| `path` | string | required | Path of the .ccomp file. |

### `component_library_list`

*file* — List the component library: the component files (.ccomp) in `folders` and their subfolders, each with its component, kind, category (doors and windows have their own), types, parameters and connectors; `search` keeps those whose component, type names or descriptions contain it. Files that do not load are listed with the reason.

| Parameter | Type | | Description |
|---|---|---|---|
| `folders` | [string, …] | required | Folders to look in. |
| `search` | string | optional | Text to look for (any case). |

### `component_save`

*file* — Save a component (all its types and the materials they use) to a .ccomp file.

| Parameter | Type | | Description |
|---|---|---|---|
| `component` | string | required | Component name (as in a component type's `params.component`). |
| `path` | string | required | Path of the .ccomp file to write. |

### `cad_import`

*edit* — Import a DWG or DXF file into a 2D view (plan, ceiling plan, elevation, section or legend) as a drawing to trace over, in one undo step. DXF (ASCII or binary, any version) is read entirely by ConstructCAD; DWG (R12 to 2018 and later) through the acadrust library. Blocks are inserted, curves kept as arcs, texts kept; entities that are not drawn (3D solids, images, infinite lines…) are counted in `skipped`. The element starts pinned, with the layers that are off or frozen in the file hidden, and in the view's level. It is drawn in the view only, under the model, and is left out of 3D views and IFC export.

| Parameter | Type | | Description |
|---|---|---|---|
| `path` | string | required | Path of the .dwg or .dxf file. |
| `view` | id or name | required | View id or name. |
| `align` | string | optional | `origin` (the drawing's origin at `position`), `center`, or `auto`: the origin, unless that leaves the drawing more than 1 km from what the view shows (a drawing in survey coordinates), then the center. |
| `color_mode` | string | optional | `grey` (default), `layers` (the file's colors) or `black`. |
| `halftone` | boolean | optional | Draw it faded (default false). |
| `position` | [x, y] | optional | Where the drawing goes, view coordinates in meters: its origin (`align` "origin", the default; [0, 0] if not given) or the center of its extents (`align` "center"; the center of what the view shows if not given). |
| `rotation` | number | optional | Rotation in degrees counter-clockwise. |
| `scale` | number | optional | Extra scale on top of the units (default 1). |
| `units` | string | optional | Drawing units, overriding the file's: mm, cm, dm, m, km, in, ft, yd or mi. Unitless files are read as meters when small (up to 1500 units across), else as millimeters. |

### `cad_layers`

*query* — The layers of an imported drawing: name, color, whether the view shows it, whether it was on in the file, and how many primitives it holds.

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Imported drawing element id. |

### `cad_layers_set`

*edit* — Show or hide layers of an imported drawing (by name; all of them when `layers` is empty).

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Imported drawing element id. |
| `visible` | boolean | required | Show (true) or hide (false) them. |
| `layers` | [string, …] | optional | Layer names (case-insensitive); empty for every layer. |

## Queries

### `categories_list`

*query* — Element categories with their display names and how many elements and types each has.

No parameters.

### `levels_list`

*query* — Levels from the lowest, with their elevations (meters).

No parameters.

### `types_list`

*query* — Types, optionally of one category.

| Parameter | Type | | Description |
|---|---|---|---|
| `category` | string | optional | Category name (`wall`, `door`, `component`…). |

### `type_get`

*query* — A type with all its parameters (`params`, editable with `type_set`) and identity data (`info`).

| Parameter | Type | | Description |
|---|---|---|---|
| `type` | id or name | required | Type id or name. |

### `elements_list`

*query* — Elements, optionally of one category, level and/or type, or only those shown in a view (all the instances of a type visible in it, for example), with a short summary each.

| Parameter | Type | | Description |
|---|---|---|---|
| `category` | string | optional | Category name (`wall`, `door`, `component`…); see `categories_list`. |
| `level` | id or name | optional | Level id or name. |
| `type` | id or name | optional | Type id or name: only its instances. |
| `view` | id or name | optional | View id or name: only the elements the view shows. |

### `element_get`

*query* — An element with all its parameters (`params`, editable with `element_set`) and derived values (length, height, thickness, volume, room area and perimeter; for beams the length after joins and what each end is framed into).

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Element id. |

### `groups_list`

*query* — The project's groups with their members.

No parameters.

### `measure`

*query* — Measure between two points of a view (meters on its plane: plan coordinates in plans): the distance, its horizontal and vertical parts and its angle. When both points lie on lines (`line_a`, `line_b`, each two points) that are parallel, the distance between the lines is given too. Nothing is created.

| Parameter | Type | | Description |
|---|---|---|---|
| `a` | [x, y] | required | First point `[x, y]`. |
| `b` | [x, y] | required | Second point `[x, y]`. |
| `line_a` | [[x, y], [x, y]] | optional | The line the first point was taken on, as two points. |
| `line_b` | [[x, y], [x, y]] | optional | The line the second point was taken on, as two points. |

### `views_list`

*query* — Views (plans, ceiling plans, elevations, sections, 3D, legends, schedules, sheets), optionally of one category.

| Parameter | Type | | Description |
|---|---|---|---|
| `category` | string | optional | `floor_plan`, `ceiling_plan`, `three_d`, `elevation`, `section`, `legend`, `schedule` or `sheet`. |

### `view_get`

*query* — A view with all its parameters (`params`, editable with `view_set`).

| Parameter | Type | | Description |
|---|---|---|---|
| `view` | id or name | required | View id or name. |

### `materials_list`

*query* — Materials with their colors and how many types use each.

No parameters.

### `material_get`

*query* — A material with all its fields (editable with `material_set`).

| Parameter | Type | | Description |
|---|---|---|---|
| `material` | id or name | required | Material name (or index in `materials_list`). |

### `schedule_table`

*query* — The table of a schedule view: headers, one row per element (with its id) and totals, formatted with the project units.

| Parameter | Type | | Description |
|---|---|---|---|
| `view` | id or name | required | Schedule view id or name. |

## Creating elements

### `wall_create`

*edit* — Create a wall from `start` to `end` on a level: straight, or an arc through `mid` (or with a `bulge` or a `radius`). Its top is `height` above its base, or attached to `top_level` (plus `top_offset`).

| Parameter | Type | | Description |
|---|---|---|---|
| `end` | [x, y] | required | End point of the wall line. |
| `start` | [x, y] | required | Start point of the wall line. |
| `base_offset` | number | optional | Base offset from the level, meters. |
| `bulge` | number | optional | Arc walls: tan(sweep / 4), positive when the arc turns counter-clockwise from `start` to `end` (1 is a half circle; at most 10 either way). Ignored when `mid` is given. |
| `height` | number | optional | Unconnected height in meters (default 3). |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `location_line` | string | optional | Which line of the wall `start`-`end` is: `Centerline` (default), `CoreCenter`, `FinishExterior`, `FinishInterior`, `CoreExterior` or `CoreInterior`. |
| `major_arc` | boolean | optional | With `radius`: take the longer of the two arcs (default: the shorter). |
| `mid` | [x, y] | optional | A point on the arc between the ends: makes an arc wall through the three points. |
| `radius` | number | optional | Arc walls: the radius in meters, positive when the arc turns counter-clockwise from `start` to `end`, negative clockwise; at least half the distance between the ends. Ignored when `mid` or `bulge` is given. |
| `top_level` | id or name | optional | Level the top is attached to, instead of a height. |
| `top_offset` | number | optional | Offset of the top from `top_level`, meters. |
| `type` | id or name | optional | Wall type id or name (default: the default wall type). |

### `door_create`

*edit* — Place a door in a wall (centered on the wall unless `offset` or `point` is given).

| Parameter | Type | | Description |
|---|---|---|---|
| `flip_facing` | boolean | optional | Face the other side of the wall. |
| `flip_hand` | boolean | optional | Hinge on the other jamb. |
| `offset` | number | optional | Distance along the wall from its start to the opening's center, meters. |
| `point` | [x, y] | optional | A point near the wall: the opening is centered at its projection, on its side. |
| `sill` | number | optional | Sill height (default: the type's default sill for windows, 0 for doors). |
| `type` | id or name | optional | Door or window type id or name (default: the default type). |
| `wall` | integer | optional | Id of the host wall; omit it to use the wall nearest `point`. |

### `window_create`

*edit* — Place a window in a wall (centered on the wall unless `offset` or `point` is given).

| Parameter | Type | | Description |
|---|---|---|---|
| `flip_facing` | boolean | optional | Face the other side of the wall. |
| `flip_hand` | boolean | optional | Hinge on the other jamb. |
| `offset` | number | optional | Distance along the wall from its start to the opening's center, meters. |
| `point` | [x, y] | optional | A point near the wall: the opening is centered at its projection, on its side. |
| `sill` | number | optional | Sill height (default: the type's default sill for windows, 0 for doors). |
| `type` | id or name | optional | Door or window type id or name (default: the default type). |
| `wall` | integer | optional | Id of the host wall; omit it to use the wall nearest `point`. |

### `floor_create`

*edit* — Create a floor slab from an outline (or the loop of walls).

| Parameter | Type | | Description |
|---|---|---|---|
| `boundary` | [[x, y], …] | optional | Closed outline (at least 3 points); omit with `from_walls` or `curves`. |
| `curves` | [JSON, …] | optional | The outline drawn with curves (lines, arcs, circles, ellipses, rectangles, polygons) instead of `boundary`; loops inside the outline are openings (floors and roofs). The element keeps the curves (see `sketch_get`). |
| `from_walls` | boolean | optional | Follow the outer boundary of the walls of the level, leaving interior walls out (exterior faces; interior faces for ceilings). |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `offset` | number | optional | Floors: top above the level (default 0). Ceilings: bottom above the level (default 2.6). Roofs: base above the level (default: top of the tallest wall). |
| `openings` | [[[x, y], …], …] | optional | Openings through the slab (shafts, stairwells): closed outlines inside the boundary, apart from each other. |
| `type` | id or name | optional | Type id or name (default: the category's default type). |

### `floor_opening_add`

*edit* — Add an opening (a shaft, a stairwell) to a floor: a closed outline inside its boundary, apart from its other openings.

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Floor id. |
| `outline` | [[x, y], …] | required | Outline of the opening, at least 3 points. |

### `floor_opening_remove`

*edit* — Remove one opening of a floor (by its index in the floor's `openings`).

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Floor id. |
| `index` | integer | required | Opening index, from 0. |

### `ceiling_create`

*edit* — Create a ceiling from an outline (or the inside of the loop of walls).

| Parameter | Type | | Description |
|---|---|---|---|
| `boundary` | [[x, y], …] | optional | Closed outline (at least 3 points); omit with `from_walls` or `curves`. |
| `curves` | [JSON, …] | optional | The outline drawn with curves (lines, arcs, circles, ellipses, rectangles, polygons) instead of `boundary`; loops inside the outline are openings (floors and roofs). The element keeps the curves (see `sketch_get`). |
| `from_walls` | boolean | optional | Follow the outer boundary of the walls of the level, leaving interior walls out (exterior faces; interior faces for ceilings). |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `offset` | number | optional | Floors: top above the level (default 0). Ceilings: bottom above the level (default 2.6). Roofs: base above the level (default: top of the tallest wall). |
| `type` | id or name | optional | Type id or name (default: the category's default type). |

### `roof_create`

*edit* — Create a roof over an outline (or the loop of walls): flat by default; change its shape (shed, gable, hip) with `element_set`.

| Parameter | Type | | Description |
|---|---|---|---|
| `boundary` | [[x, y], …] | optional | Closed outline (at least 3 points); omit with `from_walls` or `curves`. |
| `curves` | [JSON, …] | optional | The outline drawn with curves (lines, arcs, circles, ellipses, rectangles, polygons) instead of `boundary`; loops inside the outline are openings (floors and roofs). The element keeps the curves (see `sketch_get`). |
| `from_walls` | boolean | optional | Follow the outer boundary of the walls of the level, leaving interior walls out (exterior faces; interior faces for ceilings). |
| `holes` | [[[x, y], …], …] | optional | Openings through the roof (courtyards): closed outlines inside the boundary, apart from each other. Pitched roofs slope down to their eaves too. |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `offset` | number | optional | Floors: top above the level (default 0). Ceilings: bottom above the level (default 2.6). Roofs: base above the level (default: top of the tallest wall). |
| `overhang` | number | optional | Overhang beyond the outline, meters (default 0.5). |
| `type` | id or name | optional | Type id or name (default: the category's default type). |

### `roof_edge_set`

*edit* — Set the slope of one edge of a gable or hip roof's outline (edge `i` runs from outline point `i` to the next): `0` makes it a gable end, a positive slope an eave rising at that slope, `null` returns it to the shape's own.

| Parameter | Type | | Description |
|---|---|---|---|
| `edge` | integer | required | Edge index, from 0. |
| `id` | integer | required | Roof id. |
| `slope` | number | optional | Slope as rise over run (0.5 = 50 %), `0` for a gable end, or `null` for the roof's slope. |

### `roof_opening_remove`

*edit* — Remove one opening of a roof (by its index in the roof's `holes`).

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Roof id. |
| `index` | integer | required | Opening index, from 0. |

### `room_create`

*edit* — Create a room at a point enclosed by room-bounding walls (its boundary follows the walls).

| Parameter | Type | | Description |
|---|---|---|---|
| `point` | [x, y] | required | A point inside the room. |
| `height` | number | optional | Upper limit above the level (default: the level's height). |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `name` | string | optional | Default: the room default name. |
| `number` | string | optional | Default: the next free number. |

### `grid_create`

*edit* — Create a grid line. Without `name` it takes the next name of its series: the one after the most recently placed grid parallel to it (`1` → `2`, `A` → `B`, `Z` → `AA`, `A1` → `A2`, `1.1` → `1.2`); a grid across the existing ones starts a new series (letters across numbered grids, numbers otherwise). Names given this way are unique among the grids.

| Parameter | Type | | Description |
|---|---|---|---|
| `end` | [x, y] | required | End point. |
| `start` | [x, y] | required | Start point (where the bubble is). |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `name` | string | optional | Grid name (default: the next name of its series). |

### `column_create`

*edit* — Place a column; its top goes to the level above (or `height` when there is none).

| Parameter | Type | | Description |
|---|---|---|---|
| `position` | [x, y] | required | Center of the column in plan. |
| `height` | number | optional | Unconnected height (default: the level's height). |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `rotation` | number | optional | Rotation in plan, degrees. |
| `structural` | boolean | optional | Load-bearing (default true); an architectural column is cut by the walls it is joined with. |
| `top_level` | id or name | optional | Level the top is attached to (default: the level above). |
| `top_offset` | number | optional | Offset of the top from `top_level`, meters. |
| `type` | id or name | optional | Type id or name (default: the category's default type). |

### `beam_create`

*edit* — Create a beam from `start` to `end`; its location line is `top_offset` above the level (default: the level's height), at the top of the section unless `vertical` says otherwise.

| Parameter | Type | | Description |
|---|---|---|---|
| `end` | [x, y] | required | End of the beam axis. |
| `start` | [x, y] | required | Start of the beam axis. |
| `end_offset` | number | optional | Elevation of the location line at the end, when it differs from the start (a sloped beam). |
| `lateral` | string | optional | Line across the section the axis follows: `center` (default), `left` or `right` face. |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `top_offset` | number | optional | Elevation of the location line above the level, meters (default: the level height). |
| `type` | id or name | optional | Type id or name (default: the category's default type). |
| `vertical` | string | optional | Height of the section at the location elevation: `top` (default), `center` or `bottom`. |

### `columns_at_grids`

*edit* — Place a column at every intersection of grid lines on a level, skipping places that already have a column. Returns the new ids (`created`) and how many intersections were skipped.

| Parameter | Type | | Description |
|---|---|---|---|
| `grids` | [id, …] | optional | Grid ids (default: every grid). |
| `height` | number | optional | Unconnected height (default: the level's height). |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `rotation` | number | optional | Rotation in plan, degrees. |
| `structural` | boolean | optional | Load-bearing (default true). |
| `top_level` | id or name | optional | Level the tops are attached to (default: the level above). |
| `top_offset` | number | optional | Offset of the tops from `top_level`, meters. |
| `type` | id or name | optional | Column type id or name (default: the category's default type). |

### `beams_on_grids`

*edit* — Place beams along grid lines: on each grid, a beam between each pair of neighboring columns of the level whose centers lie on it. Spans that already have a beam are skipped.

| Parameter | Type | | Description |
|---|---|---|---|
| `grids` | [id, …] | optional | Grid ids (default: every grid). |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `top_offset` | number | optional | Top of the beams above the level, meters (default: the level height). |
| `type` | id or name | optional | Beam type id or name (default: the category's default type). |

### `beams_between_columns`

*edit* — Place beams from column to column in the order given (a chain), on the level of the first column. Spans that already have a beam are skipped.

| Parameter | Type | | Description |
|---|---|---|---|
| `columns` | [id, …] | required | Column ids, at least two, in the order to connect. |
| `top_offset` | number | optional | Top of the beams above the level, meters (default: the level height). |
| `type` | id or name | optional | Beam type id or name (default: the category's default type). |

### `slab_join_supports`

*edit* — Join each slab with the structure that carries it: beams, structural columns and structural walls whose bodies overlap it (default: every floor, or every floor of `level`). Beams and columns cut the slab, the slab cuts walls, and the shared volume counts once. Pairs already joined are left as they are.

| Parameter | Type | | Description |
|---|---|---|---|
| `floors` | [id, …] | optional | Floor ids (default: every floor, or those of `level`). |
| `level` | id or name | optional | Level id or name, when no floors are given. |

### `structure_number`

*edit* — Number the marks of beams, columns and pad footings (default: all three) in reading order: horizontal beams top to bottom then left to right, the other beams left to right then top to bottom, columns and footings top to bottom then left to right. Marks are a prefix per category (`V`, `P`, `S` in Portuguese; `B`, `C`, `F` in English) and a number.

| Parameter | Type | | Description |
|---|---|---|---|
| `categories` | [string, …] | optional | Categories to number: `beam`, `column` and/or `foundation` (default: all three). |
| `level` | id or name | optional | Level id or name (default: every level, numbered as one sequence per category). |

### `brace_create`

*edit* — Create a brace: a member between two points in space (`[x, y, z]`, `z` above the level), its axis through both. Its ends must not be one above the other.

| Parameter | Type | | Description |
|---|---|---|---|
| `end` | [number, …] | required | End point `[x, y, z]`. |
| `start` | [number, …] | required | Start point `[x, y, z]`. |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `type` | id or name | optional | Beam type id or name (default: the category's default type). |

### `braces_between_columns`

*edit* — Brace the bay between two columns: `x` (default) crosses two members from the base of each column to the top of the other, `diagonal` places one from the base of the first to the top of the second.

| Parameter | Type | | Description |
|---|---|---|---|
| `columns` | [id, …] | required | The two column ids. |
| `pattern` | string | optional | `x` or `diagonal`. |
| `type` | id or name | optional | Beam type id or name (default: the category's default type). |

### `truss_create`

*edit* — Lay out a plane truss from `start` to `end`: a bottom and a top chord `height` apart, `panels` panels and the web of `pattern` (`pratt`, the default, `howe` or `warren`). Pratt and Howe trusses get a post at every panel point, Warren trusses only at the ends; posts are columns. The members form a group, so the truss moves as one.

| Parameter | Type | | Description |
|---|---|---|---|
| `end` | [x, y] | required | End of the truss in plan. |
| `height` | number | required | Distance between the chord axes, meters. |
| `start` | [x, y] | required | Start of the truss in plan. |
| `bottom_offset` | number | optional | Elevation of the bottom chord's axis above the level (default: the level height minus `height`). |
| `chord_type` | id or name | optional | Beam type of the chords (default: the category's default type). |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `panels` | integer | optional | Number of panels (default: about one per 1.5 m). |
| `pattern` | string | optional | `pratt`, `howe` or `warren`. |
| `post_type` | id or name | optional | Column type of the posts (default: the first profile column type). |
| `web_type` | id or name | optional | Beam type of the diagonals (default: the chord type). |

### `rebar_create`

*edit* — Add a set of bars to a beam or column: `layout` is `top` or `bottom` (beams), `perimeter` (columns) with `count` bars, or `stirrups` every `spacing`. The bars are `cover` inside the faces (default 2.5 cm) and follow the host.

| Parameter | Type | | Description |
|---|---|---|---|
| `host` | integer | required | Beam or column id. |
| `layout` | string | required | `top`, `bottom`, `perimeter` or `stirrups`. |
| `count` | integer | optional | Number of longitudinal bars (default: 2 in beams, 4 in columns). |
| `cover` | number | optional | Concrete cover, meters (default 0.025). |
| `spacing` | number | optional | Stirrup spacing, meters (default 0.15). |
| `type` | id or name | optional | Bar type id or name (default: the type nearest 10 mm, or 5 mm for stirrups). |

### `rebar_typical`

*edit* — Give beams and columns without reinforcement a typical one: beams two (three when 20 cm wide or more) 10 mm bars at the bottom, two 8 mm at the top and 5 mm stirrups every 15 cm; columns four 10 mm bars (six in round ones) and 5 mm ties every 15 cm. Default: every beam and column of `level` (or of the project).

| Parameter | Type | | Description |
|---|---|---|---|
| `elements` | [id, …] | optional | Beam and column ids (default: all of `level`, or of the project). |
| `level` | id or name | optional | Level id or name, when no elements are given. |

### `analytical_model`

*query* — The analytical model of the structure: structural columns and beams as members along their center lines between shared nodes. Beams framed into columns end on the column axis, beams ending on beams end on their line, and members are split where others connect.

No parameters.

### `analytical_export`

*file* — Write the analytical model (as `analytical_model` returns it) to a JSON file.

| Parameter | Type | | Description |
|---|---|---|---|
| `path` | string | required | Path of the .json file to write. |

### `footing_create`

*edit* — Place a pad footing centered at `position`; its top is `top_offset` above the level (default 0).

| Parameter | Type | | Description |
|---|---|---|---|
| `position` | [x, y] | required | Center of the pad in plan. |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `rotation` | number | optional | Rotation in plan, degrees. |
| `top_offset` | number | optional | Top above the level, meters (default 0). |
| `type` | id or name | optional | Pad footing type id or name (default: the first pad type). |

### `strip_footing_create`

*edit* — Place a strip footing (a ground beam) centered on the line `start`-`end`; its top is `top_offset` above the level (default 0). Strips meet at corners and tees like beams.

| Parameter | Type | | Description |
|---|---|---|---|
| `end` | [x, y] | required | End of the strip's center line. |
| `start` | [x, y] | required | Start of the strip's center line. |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `top_offset` | number | optional | Top above the level, meters (default 0). |
| `type` | id or name | optional | Strip footing type id or name (default: the first strip type). |

### `foundation_slab_create`

*edit* — Create a foundation slab (a raft) from an outline or under the loop of walls of the level; `offset` is its top above the level (default 0).

| Parameter | Type | | Description |
|---|---|---|---|
| `boundary` | [[x, y], …] | optional | Closed outline (at least 3 points); omit with `from_walls` or `curves`. |
| `curves` | [JSON, …] | optional | The outline drawn with curves (lines, arcs, circles, ellipses, rectangles, polygons) instead of `boundary`; loops inside the outline are openings (floors and roofs). The element keeps the curves (see `sketch_get`). |
| `from_walls` | boolean | optional | Follow the outer boundary of the walls of the level, leaving interior walls out (exterior faces; interior faces for ceilings). |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `offset` | number | optional | Floors: top above the level (default 0). Ceilings: bottom above the level (default 2.6). Roofs: base above the level (default: top of the tallest wall). |
| `type` | id or name | optional | Type id or name (default: the category's default type). |

### `footings_under_columns`

*edit* — Place a pad footing under each column (default: every column of the level), centered and turned with it, its top at the column's base. Columns that already stand on a pad are skipped.

| Parameter | Type | | Description |
|---|---|---|---|
| `columns` | [id, …] | optional | Column ids (default: every column of `level`). |
| `level` | id or name | optional | Level id or name, when no columns are given (default: the lowest level). |
| `type` | id or name | optional | Pad footing type id or name (default: the first pad type). |

### `strip_footings_under_walls`

*edit* — Place a strip footing under each wall (default: every wall of the level), along its body's centerline, its top at the wall's base. Walls that already have a strip are skipped; strips meet at corners like beams.

| Parameter | Type | | Description |
|---|---|---|---|
| `level` | id or name | optional | Level id or name, when no walls are given (default: the lowest level). |
| `type` | id or name | optional | Strip footing type id or name (default: the first strip type). |
| `walls` | [id, …] | optional | Wall ids (default: every wall of `level`). |

### `stair_create`

*edit* — Create a stair starting at `start` (the middle of its first riser) and rising along `direction` up to the level above (or `height`). Risers and treads follow the type: the fewest risers within its maximum, and a fixed tread or one from Blondel's formula (`2 × riser + tread`). `shape` lays it out straight, with a landing, L- or U-shaped, or around a center.

| Parameter | Type | | Description |
|---|---|---|---|
| `start` | [x, y] | required | Start of the run (center of the first step's edge). |
| `direction` | number | optional | Direction of the first flight in plan, degrees counter-clockwise from +X. |
| `gap` | number | optional | Clear gap between the flights of a U-shaped stair, meters (default 0.1). |
| `height` | number | optional | Height when there is no level above (default 3). |
| `inner_radius` | number | optional | Radius of the inner edge of a spiral stair, meters (default 0.15). |
| `landing` | number | optional | Landing depth along the first flight, meters (default: the stair width). |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `shape` | string | optional | Plan shape: `straight` (default), `straight_landing`, `l_shaped`, `u_shaped` or `spiral`. |
| `split` | integer | optional | Risers of the first flight (default: half of them). |
| `top_level` | id or name | optional | Level the stair rises to (default: the level above). |
| `turn` | string | optional | Side the stair turns to, walking up: `left` (default) or `right`. |
| `type` | id or name | optional | Type id or name (default: the category's default type). |
| `winders` | integer | optional | Winder treads in the corner of an L-shaped stair instead of a landing (2 to 6). |

### `stair_check`

*query* — Check a stair's steps: riser count and height, tread on the walking line, Blondel's step `2 × riser + tread` and the rules of its type they break (riser out of range, short tread, step out of the Blondel range, short landing).

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Stair element id. |

### `stair_railings_add`

*edit* — Add railings along the sides of a stair or a ramp: they follow its flights, landings and turns (at the pitch of the nosings on stairs), and go with it when it changes, moves or is deleted.

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Stair or ramp element id. |
| `inset` | number | optional | Distance in from the edge of the stair, meters (default 0.05). |
| `side` | string | optional | `left`, `right` or `both` (default), walking up. |
| `type` | id or name | optional | Railing type id or name (default: the category's default type). |

### `ramp_create`

*edit* — Create a ramp starting at `start` (the middle of its foot) and rising along `direction` up to the level above (or `height`). Flights follow the type's rules: at most its steepest slope and its highest rise per flight, with landings between them; `shape` lays them out in line, L- or U-shaped (switchback), or as a helix. A project without a ramp type gets one.

| Parameter | Type | | Description |
|---|---|---|---|
| `start` | [x, y] | required | Foot of the ramp (middle of its lower edge). |
| `direction` | number | optional | Direction of the first flight in plan, degrees counter-clockwise from +X. |
| `gap` | number | optional | Clear gap between the flights of a U-shaped ramp, meters (default 0.1). |
| `height` | number | optional | Height when there is no level above (default 3). |
| `inner_radius` | number | optional | Radius of the inner edge of a spiral ramp, meters (default 1.5). |
| `landing` | number | optional | Landing length, meters (default: the type's shortest landing). |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `shape` | string | optional | Plan shape: `straight` (default), `l_shaped`, `u_shaped` or `spiral`. |
| `slope` | number | optional | Slope of the flights, percent (default: the steepest the type allows). |
| `top_level` | id or name | optional | Level the ramp rises to (default: the level above). |
| `turn` | string | optional | Side the ramp turns to, going up: `left` (default) or `right`. |
| `type` | id or name | optional | Type id or name (default: the category's default type). |

### `ramp_check`

*query* — Check a ramp: its slope, flights (count, rise and run of each) and the rules of its type it breaks (too steep, a flight rising too high, a short landing).

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Ramp element id. |

### `roof_check`

*query* — Check a roof against its covering: the least slope its tiles take and the roof's slopes, the area covered, the number of tiles and their weight, and its framing (purlins, rafters, battens and truss members: lengths, the number of trusses and the volume). The covering and framing are set on the roof's type (`type_set` with `covering` and `framing`).

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Roof element id. |

### `mass_create`

*edit* — Create a conceptual mass: `outline` extruded `height` up from its level (plus `base_offset`), or lofted to another outline (`top`, or the base outline set in by `top_inset`). Mass floors are taken at every level the mass spans unless `floors` lists them. A project without a mass type gets one.

| Parameter | Type | | Description |
|---|---|---|---|
| `base_offset` | number | optional | Offset of the base from the level elevation, meters (default 0). |
| `curves` | [JSON, …] | optional | The base outline drawn with curves (lines, arcs, circles, ellipses, rectangles, polygons) in one loop, instead of `outline`; the mass keeps them. |
| `floors` | [id or name, …] | optional | Levels (ids or names) where mass floors are taken (default: every level the mass spans). |
| `height` | number | optional | Height above the base, meters (default: up to the level above, else 3). |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `outline` | [[x, y], …] | optional | Base outline `[[x, y], …]` in plan, closed (at least three points); or `curves`. |
| `top` | [[x, y], …] | optional | Top outline of a loft `[[x, y], …]` (default: the base outline, an extrusion). |
| `top_inset` | number | optional | Loft to the base outline set in by this distance, meters (instead of `top`). |
| `type` | id or name | optional | Type id or name (default: the category's default type). |

### `sketch_get`

*query* — The curves an element's outline is drawn with: its sketch, or lines along its outline and openings when it has none (floors, ceilings, roofs and masses).

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Element id. |

### `sketch_set`

*edit* — Draw an element's outline again from curves (edit its boundary): floors and roofs take the loops inside the outline as their openings; ceilings and masses take one loop. The element keeps the curves, so `sketch_get` gives them back.

| Parameter | Type | | Description |
|---|---|---|---|
| `curves` | [JSON, …] | required | The curves, closed into loops. |
| `id` | integer | required | Element id (a floor, ceiling, roof or mass). |

### `mass_floors_set`

*edit* — Choose the levels where a mass takes mass floors: `levels` (ids or names), or every level it spans when `levels` is left out; an empty list takes none.

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Mass element id. |
| `levels` | [id or name, …] | optional | Levels (ids or names); left out: every level the mass spans. |

### `mass_floors`

*query* — The mass floors of a mass: for each, its level, elevation, area and perimeter (and outline), with the gross floor area and the volume.

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Mass element id. |

### `mass_type_create`

*edit* — Create a mass type, shown in `material` (default: the project's first material).

| Parameter | Type | | Description |
|---|---|---|---|
| `material` | string | optional | Material name. |
| `name` | string | optional | Type name (default: the template's mass name). |

### `mass_face_move`

*edit* — Push or pull a face of a mass `distance` meters outward (inward when negative): `top` changes its height, `bottom` its base offset and height, and `side` (with `side`, the index of the base outline's edge from that point to the next) moves that side, the sides beside it keeping their directions. A loft's sides are not moved.

| Parameter | Type | | Description |
|---|---|---|---|
| `distance` | number | required | Distance outward, meters (negative: inward). |
| `face` | string | required | `top`, `bottom` or `side`. |
| `id` | integer | required | Mass element id. |
| `side` | integer | optional | The side's edge of the base outline (with `face: "side"`). |

### `mass_walls_create`

*edit* — Walls by face: a wall over each side of a mass extruded straight up (`sides`: the base outline's edges, by default all), its exterior face on the mass's face, from the mass's base up its height. Walls meeting at the mass's corners join. A loft's slanted sides take no walls.

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Mass element id. |
| `sides` | [id, …] | optional | Edges of the base outline to put walls on (default: every edge). |
| `type` | id or name | optional | Wall type id or name (default: the category's default type). |

### `mass_slabs_create`

*edit* — Floors from mass floors: a floor on each mass floor of a mass (or those at `levels`), its outline the mass's at that level and its top at the level.

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Mass element id. |
| `levels` | [id or name, …] | optional | Levels (ids or names) of the mass floors to build (default: all of them). |
| `type` | id or name | optional | Floor type id or name (default: the category's default type). |

### `mass_roof_create`

*edit* — Roof by face: a flat roof on the top face of a mass, over its top outline, on the mass's level at the height of its top.

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Mass element id. |
| `overhang` | number | optional | How far the eaves reach past the top outline, meters (default 0). |
| `type` | id or name | optional | Roof type id or name (default: the category's default type). |

### `ramp_type_create`

*edit* — Create a ramp type: a slab `thickness` thick and `width` wide, within the accessibility rules given (by default 8.33 % slope, 0.80 m rise per flight and 1.20 m landings).

| Parameter | Type | | Description |
|---|---|---|---|
| `max_flight_rise` | number | optional | Highest rise of one flight, meters (default 0.80). |
| `max_slope` | number | optional | Steepest slope, percent (default 8.33). |
| `min_landing` | number | optional | Shortest landing, meters (default 1.20). |
| `name` | string | optional | Type name (default: the template's ramp name). |
| `thickness` | number | optional | Slab thickness, meters (default 0.15). |
| `width` | number | optional | Width, meters (default 1.2). |

### `railing_create`

*edit* — Create a railing along a path (at least 2 points).

| Parameter | Type | | Description |
|---|---|---|---|
| `path` | [[x, y], …] | required | Points of the railing path (at least 2). |
| `base_offset` | number | optional | Base above the level, meters. |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `type` | id or name | optional | Type id or name (default: the category's default type). |

### `component_place`

*edit* — Place a component (furniture, plumbing fixture, electrical device…) of a component type. Electrical devices default to their usual mounting height (outlets 0.30 m, switches 1.10 m, boards 1.30 m, wall lights 2.10 m, ceiling lights at the level height) and may be put on a circuit.

| Parameter | Type | | Description |
|---|---|---|---|
| `position` | [x, y] | required | Insertion point in plan. |
| `type` | id or name | required | Component type id or name. |
| `base_offset` | number | optional | Base above the level, meters (default 0; for devices, their mounting height). |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `rotation` | number | optional | Rotation in plan, degrees. |
| `system` | id or name | optional | Circuit or system id, name or code to connect it to. |

## Building services

### `pipe_create`

*edit* — Create a pipe from `start` to `end` (or a vertical riser at `start` up or down to `end_offset`), in a plumbing system. Its center line starts `offset` above the level (default: where it starts on a pipe of its system, such as the end of a run or a riser; else 0.60 m for supply and vent pipes, -0.30 m for drains). Drains fall along the flow by `slope` (percent; default 2 % up to 75 mm, 1 % above); other pipes are level unless `end_offset` or `slope` says otherwise.

| Parameter | Type | | Description |
|---|---|---|---|
| `start` | [x, y] | required | Start of the center line in plan. |
| `end` | [x, y] | optional | End in plan (default: the start, for a riser). |
| `end_offset` | number | optional | Elevation of the end above the level, meters (overrides `slope`). |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `offset` | number | optional | Elevation of the start above the level, meters. |
| `slope` | number | optional | Fall from the start to the end, percent of the plan length. |
| `system` | id or name | optional | Plumbing system id, name or code (e.g. "AF"). |
| `type` | id or name | optional | Pipe type id or name (default: the first pipe type). |

### `conduit_create`

*edit* — Create a conduit from `start` to `end` (or a vertical drop at `start` to `end_offset`) carrying `circuits`. It runs `offset` above the level (default: where it starts on another conduit, such as the end of a run or a vertical drop; else in the slab above, at the level height).

| Parameter | Type | | Description |
|---|---|---|---|
| `start` | [x, y] | required | Start of the center line in plan. |
| `circuits` | [id or name, …] | optional | Circuits it carries: ids, names or codes. |
| `end` | [x, y] | optional | End in plan (default: the start, for a vertical drop). |
| `end_offset` | number | optional | Elevation of the end above the level, meters (default: as the start). |
| `level` | id or name | optional | Level id or name (default: the lowest level). |
| `offset` | number | optional | Elevation of the start above the level, meters. |
| `type` | id or name | optional | Conduit type id or name (default: the first conduit type). |

### `systems_list`

*query* — List the plumbing systems and electrical circuits with what is in them: element count, pipe or conduit length, fixture units and supply weights of plumbing fixtures, and the devices and load of circuits.

No parameters.

### `system_create`

*edit* — Create a plumbing system or an electrical circuit. `kind` is `cold_water`, `hot_water`, `sanitary`, `vent`, `rainwater`, `power` or `lighting`; circuits may name the board (`panel`, a component element; default: the project's only board, if it has one) feeding them and their voltage (default 127 V). A circuit's code defaults to the next free number on its board.

| Parameter | Type | | Description |
|---|---|---|---|
| `kind` | string | required | What it carries. |
| `code` | string | optional | Short code shown in tags (e.g. "AF", a circuit number). |
| `color` | [id, …] | optional | Display color `[r, g, b]` (default: by kind). |
| `name` | string | optional | Name (default: the code, or the kind's name). |
| `panel` | integer | optional | Circuits: the distribution board feeding it (element id; default: the only board). |
| `voltage` | number | optional | Circuits: nominal voltage, volts. |

### `system_set`

*edit* — Change a system or circuit: `params` is a merge patch over the values `systems_list` shows (`name`, `code`, `kind`, `color`, `panel`, `voltage`).

| Parameter | Type | | Description |
|---|---|---|---|
| `params` | JSON | required | Values to change. |
| `system` | id or name | required | System id, name or code. |

### `system_delete`

*edit* — Delete a system or circuit; its pipes, conduits and devices stay, outside any system.

| Parameter | Type | | Description |
|---|---|---|---|
| `system` | id or name | required | System id, name or code. |

### `system_assign`

*edit* — Put elements in a system or circuit, or take them out (`remove`). A pipe belongs to one system (it moves); a conduit carries several circuits (one is added); a fixture or device connects to one.

| Parameter | Type | | Description |
|---|---|---|---|
| `elements` | [id, …] | required | Pipe, conduit and component ids. |
| `system` | id or name | required | System id, name or code. |
| `remove` | boolean | optional | Take them out instead. |

### `circuit_loads`

*query* — The load schedule of the circuits (of one board, or all): per circuit its devices, installed load (VA) and current at its voltage, with the total per board.

| Parameter | Type | | Description |
|---|---|---|---|
| `panel` | integer | optional | Distribution board element id (default: every circuit). |

### `mep_fittings`

*query* — The fittings where pipes and conduits meet, counted by category, kind, angle (elbows), diameters and system: the fittings takeoff.

| Parameter | Type | | Description |
|---|---|---|---|
| `level` | id or name | optional | Level id or name (default: every level). |

### `mep_connect`

*edit* — Connect plumbing fixtures and electrical devices to the nearest pipe or conduit of their level (or riser of another floor passing through it). A fixture gets a branch to the nearest sanitary drain (falling towards it, with a drop from the floor), and to the nearest cold (and, when it takes it, hot) water pipe; a device gets a conduit to the nearest conduit, with a drop from its mounting height, carrying its circuit. A component with connectors gets a branch from each connector to the nearest run of a system of the connector's kind, from the connector's position and of its diameter (leaving vertically or horizontally as the connector points). Branches end on the runs, which forms tees. `system` connects only to that system or circuit.

| Parameter | Type | | Description |
|---|---|---|---|
| `elements` | [id, …] | required | Fixture and device ids. |
| `system` | id or name | optional | Only to this system or circuit (id, name or code). |

### `mep_sizing`

*query* — Size the building services from their networks (simplified tables of common Brazilian practice): drains by the fixture units upstream, water pipes by the flow from the fixtures' weights (at most 3 m/s), circuits by their current (breaker, copper conductor) and the voltage drop to the farthest device (at most 4 %). Diameters in mm, lengths in m.

No parameters.

### `mep_size_apply`

*edit* — Give each sized pipe the smallest pipe type of the project at least as wide as `mep_sizing` requires. Pipes already wide enough keep their type unless `downsize` is set.

| Parameter | Type | | Description |
|---|---|---|---|
| `downsize` | boolean | optional | Also narrow pipes wider than required. |

## Modifying elements

### `elements_delete`

*edit* — Delete elements (doors and windows of deleted walls go too; pinned elements stay).

| Parameter | Type | | Description |
|---|---|---|---|
| `ids` | [id, …] | required | Element ids. |

### `elements_move`

*edit* — Move elements by `delta` (walls take their doors and windows; doors and windows alone slide along their wall).

| Parameter | Type | | Description |
|---|---|---|---|
| `delta` | [x, y] | required | Displacement `[dx, dy]` in meters. |
| `ids` | [id, …] | required | Element ids. |

### `elements_copy`

*edit* — Copy elements by `delta`; with `count` > 1, a linear array of that many copies spaced by `delta`.

| Parameter | Type | | Description |
|---|---|---|---|
| `delta` | [x, y] | required | Displacement `[dx, dy]` of the first copy, meters (the next ones repeat it). |
| `ids` | [id, …] | required | Element ids. |
| `count` | integer | optional | Number of copies (default 1, at most 1000). |

### `elements_rotate`

*edit* — Rotate elements by `angle` degrees (counter-clockwise) about `center` (default: the center of their extents); with `copy`, rotated copies.

| Parameter | Type | | Description |
|---|---|---|---|
| `angle` | number | required | Angle in degrees, counter-clockwise. |
| `ids` | [id, …] | required | Element ids. |
| `center` | [x, y] | optional | Center of rotation (default: the center of the elements' extents). |
| `copy` | boolean | optional | Rotate copies, keeping the originals. |

### `elements_mirror`

*edit* — Mirror elements across an axis: two points, or the line of a wall, grid, beam or railing (`axis_element`). Copies by default (`copy: false` moves them).

| Parameter | Type | | Description |
|---|---|---|---|
| `ids` | [id, …] | required | Element ids. |
| `axis` | [[x, y], [x, y]] | optional | Two points of the axis. |
| `axis_element` | integer | optional | Id of an element whose line is the axis. |
| `copy` | boolean | optional | Keep the originals and mirror copies (default true). |

### `elements_pin`

*edit* — Pin elements (they cannot be moved or deleted) or unpin them.

| Parameter | Type | | Description |
|---|---|---|---|
| `ids` | [id, …] | required | Element ids. |
| `pinned` | boolean | required | `true` to pin, `false` to unpin. |

### `element_flip`

*edit* — Flip a door or window to the other side of its wall (`facing`) and/or to the other jamb (`hand`).

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Id of the door or window. |
| `facing` | boolean | optional | Flip to the other side of the wall. |
| `hand` | boolean | optional | Flip the hinge to the other jamb. |

### `elements_turn`

*edit* — Turn elements a quarter: components and columns rotate 90° about their insertion point; doors and windows flip to the other side of their wall (the Space key on a selection). Others and pinned elements are left alone.

| Parameter | Type | | Description |
|---|---|---|---|
| `ids` | [id, …] | required | Element ids. |

### `elements_export`

*query* — The elements as a portable set for `elements_paste`: doors and windows of the given walls come along. The set keeps type, level and view ids, so it pastes into this project or one sharing them.

| Parameter | Type | | Description |
|---|---|---|---|
| `ids` | [id, …] | required | Element ids. |

### `elements_paste`

*edit* — Paste a set from `elements_export` moved by `delta`, on `level` when given (else on their own levels). Elements whose type, level, view or wall does not exist here are skipped. Returns the new ids.

| Parameter | Type | | Description |
|---|---|---|---|
| `elements` | JSON | required | The `elements` of an `elements_export` result. |
| `delta` | [x, y] | optional | Displacement `[dx, dy]`, meters (default: none). |
| `level` | id or name | optional | Level id or name to paste on (default: each element's own level). |

### `wall_join`

*edit* — Allow or disallow joining at one end of a wall or beam. A wall end joins other walls; a beam end stops at the face of a structural column it lies in, or meets other beams at the same depth (mitered corners, butt joints).

| Parameter | Type | | Description |
|---|---|---|---|
| `end` | string | required | `start` or `end`. |
| `id` | integer | required | Wall or beam id. |
| `joined` | boolean | required | `true` to allow joining at that end, `false` to keep it free. |

### `geometry_join`

*edit* — Join the geometry of two elements that touch or overlap (walls, floors, ceilings, roofs, columns, beams, stairs, railings, components). Where they overlap, one cuts the other: structural columns and beams cut walls, floors, ceilings and roofs; floors, ceilings and roofs cut walls; walls cut architectural columns; between equals the first cuts the second. The shared volume is counted once, and plans and sections draw two elements cut in the same material as one piece.

| Parameter | Type | | Description |
|---|---|---|---|
| `first` | integer | required | The first element. |
| `second` | integer | required | The second element. |

### `geometry_unjoin`

*edit* — Unjoin the geometry of two joined elements: each keeps its whole body again.

| Parameter | Type | | Description |
|---|---|---|---|
| `first` | integer | required | The first element. |
| `second` | integer | required | The second element. |

### `group_create`

*edit* — Group elements: selecting one of them selects them all, so they move, copy and rotate together. Elements already in a group leave it.

| Parameter | Type | | Description |
|---|---|---|---|
| `ids` | [id, …] | required | The elements (at least two). |
| `name` | string | optional | Group name (default: "Group N"). |

### `group_ungroup`

*edit* — Ungroup: the group goes, its elements stay.

| Parameter | Type | | Description |
|---|---|---|---|
| `group` | integer | required | Group id (see `groups_list`, or `group` in `element_get`). |

### `trim_corner`

*edit* — Trim or extend two linear elements (walls, beams, grids) to meet at a corner; each pick point marks the part of its element to keep. Arc walls meet lines and other arcs where their circles cross them.

| Parameter | Type | | Description |
|---|---|---|---|
| `a` | integer | required | First element id. |
| `b` | integer | required | Second element id. |
| `pick_a` | [x, y] | required | A point on the part of `a` to keep. |
| `pick_b` | [x, y] | required | A point on the part of `b` to keep. |

### `trim_extend`

*edit* — Trim or extend a linear element to the line of a reference element (the circle of an arc wall); the pick point marks the part to keep. Arc walls trim and extend along their circle.

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Element to trim or extend. |
| `pick` | [x, y] | required | A point on the part to keep. |
| `reference` | integer | required | Id of the element whose line is the boundary. |

### `element_split`

*edit* — Split a linear element (wall, beam, grid) at a point; returns the new element.

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Element id. |
| `point` | [x, y] | required | Where to split (projected on the element's line). |

### `element_offset`

*edit* — Copy a linear element parallel at `distance` (positive: to the left of its direction).

| Parameter | Type | | Description |
|---|---|---|---|
| `distance` | number | required | Offset distance, meters: positive to the left of start→end, negative to the right. |
| `id` | integer | required | Element id. |

### `element_align`

*edit* — Align an element onto a line of a reference element, in plan: the edge picked on the element (a wall's face, a column's side) moves onto the line picked on the reference (a face or side, or the location line of a grid or of an element picked without a point). When the two are not parallel the element first turns about the picked point of its edge. An arc wall moves onto the circle of a concentric arc wall (its radius changes). Alignment is in plan, so the elements may be on different levels (a column above onto the face of the column below).

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Element to move. |
| `reference` | integer | required | Element whose line it moves onto. |
| `pick` | [x, y] | optional | Plan point `[x, y]` near the edge of `id` that moves onto the reference line (the edge of its plan outline nearest to it). Without it a linear element moves its location line, and other elements their insertion point, onto the line. |
| `reference_pick` | [x, y] | optional | Plan point `[x, y]` near the line of `reference` to align to (the edge of its plan outline nearest to it, such as a wall face). Without it the reference's location line is used, which a column or component does not have. |

### `element_move_end`

*edit* — Move one end of a wall, beam or grid (joined wall ends follow).

| Parameter | Type | | Description |
|---|---|---|---|
| `end` | string | required | `start` or `end`. |
| `id` | integer | required | Element id. |
| `point` | [x, y] | required | New position of that end. |

### `grids_align_ends`

*edit* — Give grids the extents of a reference grid, so their bubbles line up: each grid parallel to the reference (within 1°) has its ends moved along its own line onto the lines through the reference's ends perpendicular to it (each end to the reference end on its side). Grids that are not parallel and pinned grids are left alone.

| Parameter | Type | | Description |
|---|---|---|---|
| `reference` | integer | required | Id of the reference grid. |
| `ends` | string | optional | Which ends move: `both` (default), `start` (the bubble end) or `end`. |
| `grids` | [id, …] | optional | Ids of the grids to align (default: every other grid parallel to the reference). |

### `element_set`

*edit* — Change an element: its level (doors and windows of a wall follow it), type (of the same category), mark, comments, and any of its `params` as `element_get` shows them (merged: only the fields given change, e.g. `{"height": 2.8}` for a wall or `{"shape": {"Gable": {"slope": 0.5, "ridge_deg": 0}}}` for a roof). The edit is refused if it would leave the model invalid.

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Element id. |
| `comments` | string | optional | New comments. |
| `level` | id or name | optional | New level id or name. |
| `mark` | string | optional | New mark. |
| `params` | JSON | optional | Fields of the element's parameters to change. |
| `type` | id or name | optional | New type id or name (same category). |

### `elements_set_type`

*edit* — Give elements a type (all must be of the type's category), like Match Type.

| Parameter | Type | | Description |
|---|---|---|---|
| `ids` | [id, …] | required | Element ids. |
| `type` | id or name | required | Type id or name. |

## Dimensions and text

### `dimension_create`

*edit* — Place a dimension in a 2D view (points in the view's drawing coordinates: plan X/Y in plans; horizontal position and height in elevations and sections). Its ends follow the elements (or level lines) their points lie on, so it keeps measuring them. Aligned dimensions measure from `a` to `b`, linear ones along a horizontal or vertical `direction`, ordinate ones from the origin `a` to `b` along that direction (a running dimension; see `dimension_ordinate_create`), angular ones the angle at `vertex` between the rays through `a` and `b`, radial and diameter ones the arc centered at `a` through `b`.

| Parameter | Type | | Description |
|---|---|---|---|
| `a` | [x, y] | required | First point (center for radial and diameter). |
| `b` | [x, y] | required | Second point (a point on the arc for radial and diameter). |
| `view` | id or name | required | View id or name. |
| `direction` | string | optional | Linear and ordinate dimensions: `horizontal` or `vertical` (default: the larger extent). |
| `kind` | string | optional | `aligned` (default), `linear`, `ordinate`, `angular`, `radial` or `diameter`. |
| `offset` | number | optional | Distance of the dimension line from `a` (left of a→b positive), or the arc radius for angular ones (default 1). |
| `type` | id or name | optional | Dimension type id or name (default: the default dimension type). |
| `vertex` | [x, y] | optional | Vertex of an angular dimension. |

### `tag_move`

*edit* — Move the tag of an element in a plan (the tag of a door, window, beam, column or footing): by `delta` from where it is, to `offset` from its default spot, or back there with `reset`. A tag moved away from its element gets a leader to it. Each plan keeps its own tag positions.

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Id of the element the tag names. |
| `view` | id or name | required | Plan id or name. |
| `delta` | [x, y] | optional | Move by `[dx, dy]` meters in plan. |
| `offset` | [x, y] | optional | Place the tag `[dx, dy]` meters from its default spot. |
| `reset` | boolean | optional | Put the tag back in its default spot. |

### `dimension_chain_create`

*edit* — Place a chain of dimensions on one dimension line (continuous dimensioning): one dimension from each point to the next, all measured along the same direction and drawn on the same line, in one undo step. Points are in the view's drawing coordinates, as for `dimension_create`, and each end follows the element (or level line) it lies on. In an aligned chain, segments along the direction are aligned dimensions and the others measure along that direction.

| Parameter | Type | | Description |
|---|---|---|---|
| `points` | [[x, y], …] | required | The measured points in order (at least 2, at most 500). |
| `view` | id or name | required | View id or name. |
| `angle` | number | optional | Direction to measure along, degrees counter-clockwise from the X axis (overrides `direction` and the direction of the first two points). |
| `direction` | string | optional | Linear chains: `horizontal` or `vertical` (default: the larger extent of the first two points). |
| `kind` | string | optional | `aligned` (default: along the first two points) or `linear` (horizontal or vertical). |
| `offset` | number | optional | Distance of the dimension line from the first point, positive to the left of the direction (default 1). |
| `type` | id or name | optional | Dimension type id or name (default: the default dimension type). |

### `dimension_ordinate_create`

*edit* — Place running (ordinate) dimensions on one dimension line: one dimension from the origin (the first point) to each other point, measured along the same direction, each showing its running total from the origin, in one undo step. Points are in the view's drawing coordinates, as for `dimension_create`, and each end follows the element (or level line) it lies on, so the totals follow the model; giving one a new value with `dimension_set_value` moves what its point measures, keeping the origin.

| Parameter | Type | | Description |
|---|---|---|---|
| `points` | [[x, y], …] | required | The origin, then the measured points (at least 2 in all, at most 500). |
| `view` | id or name | required | View id or name. |
| `angle` | number | optional | Direction to measure along, degrees counter-clockwise from the X axis (overrides `direction`). |
| `direction` | string | optional | `horizontal` or `vertical` (default: along the origin and the second point). |
| `offset` | number | optional | Distance of the dimension line from the origin, positive to the left of the direction (default 1). |
| `type` | id or name | optional | Dimension type id or name (default: the default dimension type). |

### `dimension_set_value`

*edit* — Give a dimension a new value (meters; degrees for angular dimensions): in plans what its moving end measures moves or stretches; between level lines in elevations and sections the upper level moves. Radial and diameter dimensions are read-only.

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Dimension element id. |
| `value` | number | required | New value: meters, or degrees for angular dimensions. |

### `text_create`

*edit* — Place a text note in a 2D view (points in the view's drawing coordinates, as for dimensions). Without `width` it is single-line text (only line breaks in `text` break it); with `width` (paper millimeters) it is multiline text wrapped to that width. Leaders point from the text to what it describes, each with one segment or two (with an `elbow`).

| Parameter | Type | | Description |
|---|---|---|---|
| `position` | [x, y] | required | Top left corner of the text. |
| `text` | string | required | The text; a line break starts a new line. |
| `view` | id or name | required | View id or name. |
| `leaders` | [object, …] | optional | Leaders from the text. |
| `rotation` | number | optional | Rotation in degrees counter-clockwise. |
| `type` | id or name | optional | Text type id or name (default: the default text type). |
| `width` | number | optional | Wrapping width in paper millimeters (multiline text). |

### `text_add_leader`

*edit* — Add a leader to a text note.

| Parameter | Type | | Description |
|---|---|---|---|
| `end` | [x, y] | required | The point the leader points at, where its symbol is drawn. |
| `id` | integer | required | Text element id. |
| `elbow` | [x, y] | optional | A bend point that splits the leader in two segments (text → elbow → end). |
| `symbol` | string | optional | End symbol: `Arrow`, `OpenArrow`, `Triangle`, `Dot`, `Circle`, `Square`, `FilledSquare`, `Tick` or `None` (default: the text type's symbol). |

### `text_remove_leader`

*edit* — Remove a leader from a text note.

| Parameter | Type | | Description |
|---|---|---|---|
| `id` | integer | required | Text element id. |
| `index` | integer | required | Index of the leader (from 0, as in `params.leaders`). |

## Types, components, materials

### `type_set`

*edit* — Change a type: its name, its parameters (`params`, merged into what `type_get` shows: layers, sizes, materials, a component's `def`…) and its identity data (`info`: type mark, description, manufacturer, cost…). Changing a component's definition updates every type of that component, each keeping its own parameter values.

| Parameter | Type | | Description |
|---|---|---|---|
| `type` | id or name | required | A level, type, view or material: its numeric id or its name. |
| `info` | JSON | optional | Fields of the identity data to change, shaped like `info` in `type_get`. |
| `name` | string | optional | New name (unique in its category). |
| `params` | JSON | optional | Fields of the type's parameters to change, shaped like `params` in `type_get`. |
| `renames` | [[old, new], …] | optional | Component parameters renamed by a new `def`, as `[old, new]` pairs in order, so the component's other types keep their values. |

### `type_duplicate`

*edit* — Copy a type under a new name (default "<name> (copy)"); the given elements of the type switch to the copy.

| Parameter | Type | | Description |
|---|---|---|---|
| `type` | id or name | required | A level, type, view or material: its numeric id or its name. |
| `elements` | [id, …] | optional | Elements of the type to switch to the copy. |
| `name` | string | optional | Name of the copy (default: "<name> (copy)"). |

### `type_delete`

*edit* — Delete a type that no element uses (the last type of a category is kept).

| Parameter | Type | | Description |
|---|---|---|---|
| `type` | id or name | required | A level, type, view or material: its numeric id or its name. |

### `component_create`

*edit* — Create a component type: `def` holds its parameters and pieces, shaped like a component type's `params.def` in `type_get`. A piece's `shape` is a `Box` {x, y, z, width, depth, height}, a `Cylinder` {x, y, z, radius, height}, an `Extrusion` {points: [[x, y], …], z, height} (a plan outline extruded up), a `Profile` {plane: `Plan`, `Front` or `Side`, points: [{u, v, radius}, …], offset, depth} (lines and arcs drawn on a plane: u, v are x, y in plan, x, z from the front and y, z from the side; the plane stands at `offset` on the remaining axis and extrudes `depth` along it; a point's optional `radius` makes the segment to the next point an arc, positive bulging outwards) or a `Revolution` {plane, points, offset, axis, angle} (the profile turned about the line u = axis of its plane). Every field is an expression over the parameters (meters, degrees). A piece with `void: true` is cut out of the solid pieces it overlaps; one with `plan_only: true` is a plan symbol (a door's swing), drawn in plans and not a solid. A `kind` of `Door` or `Window` with an `opening` {width, height, sill, own_symbol} (expressions) makes a door or window type, placed with `door_create` or `window_create`: it cuts that opening in its wall; its frame has x along the wall from the opening's center, y across it (towards the side a door opens to), z up from the sill, and `WallThickness` is the wall's thickness. `connectors` [{name, system: `ColdWater`, `HotWater`, `Sanitary`, `Vent`, `Rainwater`, `Power` or `Lighting`, x, y, z, direction: `Down`, `Up`, `Front` (-y), `Back` (+y), `Left` (-x) or `Right` (+x), diameter}] are where `mep_connect` joins pipes and conduits (a toilet's inlet and outlet).

| Parameter | Type | | Description |
|---|---|---|---|
| `def` | JSON | required | The definition: `params` (name, value, kind, instance) and `items` (pieces), shaped like a component type's `params.def` in `type_get`. |
| `name` | string | required | Name of the component and of its first type. |

### `component_get`

*query* — The component of a type, as the modeling environment shows it: the definition, the value of every parameter and reference plane, the value of each dimension, the problems found, and the component's types with their parameter values.

| Parameter | Type | | Description |
|---|---|---|---|
| `type` | id or name | required | A type of the component: id or name. |

### `component_param_add`

*edit* — Add a parameter to a component (to every type, with the same value).

| Parameter | Type | | Description |
|---|---|---|---|
| `name` | string | required | Name used in expressions (letters, digits and `_`, not starting with a digit). |
| `type` | id or name | required | A type of the component: id or name. |
| `instance` | boolean | optional | Elements may override the value. |
| `kind` | string | optional | `length` (default), `angle` or `number`. |
| `value` | number | optional | Value (meters for lengths, degrees for angles; default 0). |

### `component_param_set`

*edit* — Change a component parameter: its value (for the type named only), kind, whether elements may override it, or its name (every expression, dimension and type follows a rename).

| Parameter | Type | | Description |
|---|---|---|---|
| `name` | string | required | The parameter's current name. |
| `type` | id or name | required | A type of the component: id or name (the value changes for this type). |
| `instance` | boolean | optional | Elements may override the value. |
| `kind` | string | optional | `length`, `angle` or `number`. |
| `rename` | string | optional | New name. |
| `value` | number | optional | New value for the type named. |

### `component_param_delete`

*edit* — Remove a component parameter; refused while an expression reads it. Dimensions it labels lose the label (their planes keep their place).

| Parameter | Type | | Description |
|---|---|---|---|
| `name` | string | required | The parameter's name. |
| `type` | id or name | required | A type of the component: id or name. |

### `component_ref_plane_add`

*edit* — Add a reference plane to a component: square to `axis` (`x`: a left/right plane, `y`: front/back, `z`: a height), at the position `at`, an expression that may read parameters and other planes (`Left + Width`).

| Parameter | Type | | Description |
|---|---|---|---|
| `at` | string | required | Position along the axis (meters): a number or an expression. |
| `axis` | string | required | `x`, `y` or `z`. |
| `type` | id or name | required | A type of the component: id or name. |
| `name` | string | optional | Name (default `Plane1`, `Plane2`…). |

### `component_ref_plane_set`

*edit* — Move or rename a reference plane: `at` is its new position (an expression); a rename follows into every expression and dimension.

| Parameter | Type | | Description |
|---|---|---|---|
| `name` | string | required | The plane's current name. |
| `type` | id or name | required | A type of the component: id or name. |
| `at` | string | optional | New position: a number or an expression. |
| `rename` | string | optional | New name. |

### `component_ref_plane_delete`

*edit* — Remove a reference plane and the dimensions on it; refused while an expression reads it.

| Parameter | Type | | Description |
|---|---|---|---|
| `name` | string | required | The plane's name. |
| `type` | id or name | required | A type of the component: id or name. |

### `component_ref_line_add`

*edit* — Add a reference line on one of the sketch planes (`plan`, `front` or `side`, standing at `offset` along its normal axis): its ends are `[u, v]` pairs of expressions in that plane.

| Parameter | Type | | Description |
|---|---|---|---|
| `end` | [old, new] | required | Second end `[u, v]`. |
| `plane` | string | required | `plan`, `front` or `side`. |
| `start` | [old, new] | required | First end `[u, v]`: numbers or expressions. |
| `type` | id or name | required | A type of the component: id or name. |
| `name` | string | optional | Name (default `Line1`, `Line2`…). |
| `offset` | string | optional | Position of the sketch plane along its normal axis (default 0). |

### `component_ref_line_delete`

*edit* — Remove a reference line.

| Parameter | Type | | Description |
|---|---|---|---|
| `name` | string | required | The line's name. |
| `type` | id or name | required | A type of the component: id or name. |

### `component_dimension_add`

*edit* — Dimension two parallel reference planes in a view (`plan`, `front` or `side`, one that sees them edge-on). With `label`, a length parameter drives it: the second plane moves to the first plus the parameter (the parameter is created with the current distance when missing).

| Parameter | Type | | Description |
|---|---|---|---|
| `from` | string | required | The plane it measures from. |
| `to` | string | required | The plane it measures to (the one a label moves). |
| `type` | id or name | required | A type of the component: id or name. |
| `view` | string | required | `plan`, `front` or `side`. |
| `label` | string | optional | Parameter that drives the distance. |
| `place` | number | optional | Where the dimension line sits across the planes (meters, default 0). |

### `component_dimension_set`

*edit* — Change a dimension between reference planes: label it with a parameter (`label`, created when missing), take its label off (`label: ""`, the planes stay), move its line (`place`) or give it a new `value` (the label's parameter for this type, or the moving plane's position).

| Parameter | Type | | Description |
|---|---|---|---|
| `index` | integer | required | The dimension's index (from `component_get`). |
| `type` | id or name | required | A type of the component: id or name. |
| `label` | string | optional | Parameter that drives it; empty to take the label off. |
| `place` | number | optional | Where the dimension line sits across the planes. |
| `value` | number | optional | New distance (meters). |

### `component_dimension_delete`

*edit* — Remove a dimension between reference planes (the planes stay).

| Parameter | Type | | Description |
|---|---|---|---|
| `index` | integer | required | The dimension's index. |
| `type` | id or name | required | A type of the component: id or name. |

### `component_equal_add`

*edit* — Keep the plane `middle` halfway between the parallel planes `a` and `b` (an equality, EQ): its position becomes `(a + b) / 2`, so it follows whatever moves them. Shown in `view` (`plan`, `front` or `side`) as two equal dimensions.

| Parameter | Type | | Description |
|---|---|---|---|
| `a` | string | required | One outer plane. |
| `b` | string | required | The other outer plane. |
| `middle` | string | required | The plane kept halfway (not an origin plane). |
| `type` | id or name | required | A type of the component: id or name. |
| `view` | string | required | `plan`, `front` or `side`. |
| `place` | number | optional | Where the dimensions' line sits across the planes (meters, default 0). |

### `component_equal_delete`

*edit* — Remove an equality (EQ); its middle plane stays halfway as written.

| Parameter | Type | | Description |
|---|---|---|---|
| `index` | integer | required | The equality's index (from `component_get`). |
| `type` | id or name | required | A type of the component: id or name. |

### `component_piece_add`

*edit* — Add a piece drawn as a sketch on the `plan`, `front` or `side` plane: an `extrusion` of the closed outline from `start` to `end` along the plane's normal (Z for the plan, Y for the front, X for the side); a `revolution` about the line u = `axis` of the plane, `angle` degrees (default 360); a `sweep` of the closed outline (`points`: u to the left of the path, v along the normal) along the open `path` drawn on the plane at `start`, mitered at its corners; or a `blend` from the outline at `start` to the `top` outline at `end`. Coordinates are numbers or expressions; a reference plane's name locks a coordinate (or `start`, `end`, `axis`) to that plane, so the piece follows it. With `void`, the piece is cut out of the solid pieces it overlaps.

| Parameter | Type | | Description |
|---|---|---|---|
| `plane` | string | required | `plan`, `front` or `side`. |
| `points` | [object, …] | required | The closed outline, in order (a sweep's section, a blend's bottom). |
| `type` | id or name | required | A type of the component: id or name. |
| `angle` | number or string | optional | Revolutions: degrees turned (default 360). |
| `axis` | number or string | optional | Revolutions: the axis, u = `axis` in the sketch plane. |
| `end` | number or string | optional | Extrusions and blends: where they end along the normal (default 0.1 past `start`). |
| `kind` | string | optional | `extrusion` (default), `revolution`, `sweep` or `blend`. |
| `material` | string | optional | Project material name. |
| `name` | string | optional | Name (default `Piece1`, `Piece2`…). |
| `path` | [object, …] | optional | Sweeps: the open path the section follows. |
| `start` | number or string | optional | Where the sketch plane stands along its normal (default 0). |
| `top` | [object, …] | optional | Blends: the closed outline at `end`. |
| `void` | boolean | optional | Cut out of the solid pieces it overlaps. |

### `component_piece_set`

*edit* — Change a piece of a component: its name, material, whether it is a void or drawn in plans, and for sketched pieces their outline, `start`, `end` (extrusions), `axis` and `angle` (revolutions).

| Parameter | Type | | Description |
|---|---|---|---|
| `index` | integer | required | The piece's index (from `component_get`). |
| `type` | id or name | required | A type of the component: id or name. |
| `angle` | number or string | optional | Revolutions: degrees turned. |
| `axis` | number or string | optional | Revolutions: the axis, u = `axis` in the sketch plane. |
| `detail` | [string, …] | optional | The detail levels of the views that show the piece: any of `coarse`, `medium` and `fine` (all three by default). |
| `end` | number or string | optional | Extrusions: where they end along the normal. |
| `in_plan` | boolean | optional | Drawn in plans. |
| `material` | string | optional | Project material name (empty for the default color). |
| `name` | string | optional | New name. |
| `path` | [object, …] | optional | Sweeps: new path. |
| `points` | [object, …] | optional | New outline (a sweep's section, a blend's bottom). |
| `start` | number or string | optional | Where the sketch plane stands along its normal. |
| `top` | [object, …] | optional | Blends: new top outline. |
| `void` | boolean | optional | Cut out of the solid pieces it overlaps. |

### `component_piece_delete`

*edit* — Remove a piece of a component (a component keeps at least one).

| Parameter | Type | | Description |
|---|---|---|---|
| `index` | integer | required | The piece's index. |
| `type` | id or name | required | A type of the component: id or name. |

### `material_create`

*edit* — Create a material (default name "Material"; a number is added when taken).

| Parameter | Type | | Description |
|---|---|---|---|
| `color` | [id, …] | optional | Shading color `[r, g, b]`, 0–255 (default light gray). |
| `name` | string | optional | Name (default: "Material"; a number is added when taken). |

### `material_duplicate`

*edit* — Copy a material under a new name (default "<name> (copy)").

| Parameter | Type | | Description |
|---|---|---|---|
| `material` | id or name | required | A level, type, view or material: its numeric id or its name. |
| `name` | string | optional | Name of the copy (default: "<name> (copy)"). |

### `material_rename`

*edit* — Rename a material; every type using it follows.

| Parameter | Type | | Description |
|---|---|---|---|
| `material` | id or name | required | A level, type, view or material: its numeric id or its name. |
| `name` | string | required | New name. |

### `material_set`

*edit* — Change a material's fields (merged into what `material_get` shows: description, class, colors, patterns, transparency, finish, physical values…). Use `material_rename` to rename it.

| Parameter | Type | | Description |
|---|---|---|---|
| `material` | id or name | required | A level, type, view or material: its numeric id or its name. |
| `values` | JSON | required | Fields to change, shaped like the result of `material_get`. |

### `material_delete`

*edit* — Delete a material no type uses.

| Parameter | Type | | Description |
|---|---|---|---|
| `material` | id or name | required | A level, type, view or material: its numeric id or its name. |

## Levels and views

### `level_create`

*edit* — Add a level (default: 3 m above the highest, named "Level n") with its floor and ceiling plans.

| Parameter | Type | | Description |
|---|---|---|---|
| `elevation` | number | optional | Elevation in meters. |
| `name` | string | optional | Level name (default: "Level n"). |
| `plans` | boolean | optional | Also create its floor and ceiling plans (default true). |

### `level_set`

*edit* — Rename a level or change its elevation (what is attached to it moves along).

| Parameter | Type | | Description |
|---|---|---|---|
| `level` | id or name | required | A level, type, view or material: its numeric id or its name. |
| `elevation` | number | optional | New elevation, meters. |
| `name` | string | optional | New name. |

### `level_delete`

*edit* — Delete a level no element is on, with its plan views; walls rising to it keep their current height, unconnected. The last level is kept.

| Parameter | Type | | Description |
|---|---|---|---|
| `level` | id or name | required | A level, type, view or material: its numeric id or its name. |

### `view_create`

*edit* — Create a view. `kind` is `floor_plan` or `ceiling_plan` (with `level`), `section` (with `start`, `end` and optional `depth`), `elevation` (with `direction`: north, south, east or west), `structural_plan` (with `level`: a formwork plan of the structure, tagged), `plumbing_plan` and `electrical_plan` (with `level`: the pipes, or the conduits and devices, over the architecture, tagged), `three_d`, `analytical` (a 3D view of the analytical model), `rebar` (a 3D view of the reinforcement), `legend` (all types), `schedule` (with `category`), `panel_diagram` (with `panel`: the single-line diagram and load schedule of a distribution board; default the only board) or `sheet` (A3, numbered after the others).

| Parameter | Type | | Description |
|---|---|---|---|
| `kind` | string | required | `floor_plan`, `ceiling_plan`, `structural_plan`, `plumbing_plan`, `electrical_plan`, `section`, `elevation`, `three_d`, `analytical`, `rebar`, `legend`, `schedule`, `panel_diagram` or `sheet`. |
| `category` | string | optional | Schedules: the category scheduled (default `wall`). |
| `depth` | number | optional | Section depth beyond the cut line, meters (default 8). |
| `direction` | string | optional | Elevations: `north`, `south` (default), `east` or `west`. |
| `end` | [x, y] | optional | Sections: end of the cut line. |
| `level` | id or name | optional | Plans: level id or name (default: the lowest). |
| `name` | string | optional | View name (default: by kind). |
| `panel` | integer | optional | Distribution board element id, for a panel diagram. |
| `scale` | integer | optional | Scale denominator (e.g. 50 for 1:50). |
| `start` | [x, y] | optional | Sections: start of the cut line. |

### `view_set`

*edit* — Change a view: name, scale, phase and phase filter, and its `params` as `view_get` shows them (merged: cut height, section depth, legend types, schedule fields, sort, filter and totals, sheet number and size, viewport positions…).

| Parameter | Type | | Description |
|---|---|---|---|
| `view` | id or name | required | A level, type, view or material: its numeric id or its name. |
| `detail` | string | optional | Detail level: `coarse` (layered elements cut as one plain region; component pieces set for it), `medium` or `fine`. |
| `name` | string | optional | New name. |
| `params` | JSON | optional | Fields of the view's parameters to change, shaped like `params` in `view_get`. |
| `phase` | id or name | optional | Phase the view shows (id or name; plans, elevations, sections, 3D views, schedules). |
| `phase_filter` | id or name | optional | Phase filter (id or name), or `null` for none (every status by category). |
| `scale` | integer | optional | Scale denominator (e.g. 100 for 1:100). |

### `view_duplicate`

*edit* — Copy a view (a copied sheet starts without viewports).

| Parameter | Type | | Description |
|---|---|---|---|
| `view` | id or name | required | A level, type, view or material: its numeric id or its name. |

### `view_delete`

*edit* — Delete a view (and its viewports on sheets; dimensions drawn in it go too).

| Parameter | Type | | Description |
|---|---|---|---|
| `view` | id or name | required | A level, type, view or material: its numeric id or its name. |

### `sheet_place_view`

*edit* — Place a view on a sheet at `center` (paper millimeters; default left of center). A view can be on only one sheet; sheets cannot be placed.

| Parameter | Type | | Description |
|---|---|---|---|
| `sheet` | id or name | required | A level, type, view or material: its numeric id or its name. |
| `view` | id or name | required | A level, type, view or material: its numeric id or its name. |
| `center` | [x, y] | optional | Center of the viewport on the sheet, paper millimeters. |

### `sheet_remove_viewport`

*edit* — Remove a view from a sheet.

| Parameter | Type | | Description |
|---|---|---|---|
| `sheet` | id or name | required | A level, type, view or material: its numeric id or its name. |
| `view` | id or name | required | A level, type, view or material: its numeric id or its name. |

## View graphics

### `view_graphics_get`

*query* — The visibility/graphics of a view (or of a view template): every category's visibility and overrides, the applied filters in order, the element overrides and hidden elements, the view's template and what it controls, and what is temporarily hidden or isolated. Overrides are objects like `{"cut_lines": {"color": [255, 0, 0], "weight": 0.5, "pattern": "Dash"}, "projection_lines": {..}, "cut_fill": {"color": [..], "pattern": "Solid"}, "surface_fill": {..}, "halftone": true, "transparency": 0.5}` (absent attributes are not overridden).

| Parameter | Type | | Description |
|---|---|---|---|
| `template` | id or name | optional | Or a view template id or name. |
| `view` | id or name | optional | View id or name. |

### `view_graphics_set`

*edit* — Show or hide a category in a view (or view template) and change its graphic overrides: `overrides` is merged into the current ones (`null` resets one attribute; see `view_graphics_get`), `reset` clears them first. Refused when the view's template controls its categories.

| Parameter | Type | | Description |
|---|---|---|---|
| `category` | string | required | Category (`wall`, `door`, `room`, `dimension`…). |
| `overrides` | JSON | optional | Overrides to merge. |
| `reset` | boolean | optional | Clear the category's overrides before merging. |
| `template` | id or name | optional | Or a view template id or name. |
| `view` | id or name | optional | View id or name. |
| `visible` | boolean | optional | Show (`true`) or hide (`false`) the category. |

### `element_override_set`

*edit* — Override the graphics of elements in one view (they keep their category's and filters' graphics for what is not overridden): `overrides` is merged into each element's (`null` resets an attribute), `reset` clears them first, so `reset` alone removes the overrides.

| Parameter | Type | | Description |
|---|---|---|---|
| `ids` | [id, …] | required | Element ids. |
| `view` | id or name | required | View id or name. |
| `overrides` | JSON | optional | Overrides to merge (see `view_graphics_get`). |
| `reset` | boolean | optional | Clear the elements' overrides before merging. |

### `element_hide_in_view`

*edit* — Hide elements in one view (`hidden: false` shows them again). Hidden elements stay in the model and in other views.

| Parameter | Type | | Description |
|---|---|---|---|
| `ids` | [id, …] | required | Element ids. |
| `view` | id or name | required | View id or name. |
| `hidden` | boolean | optional | Hide (default) or unhide. |

### `filters_list`

*query* — The project's view filters with their categories and rules.

No parameters.

### `filter_create`

*edit* — Create a view filter: the elements of `categories` (all when empty) that pass its rules — all of them, or any with `any`; a filter without rules matches every element of its categories. Apply it to views with `view_filter_add`.

| Parameter | Type | | Description |
|---|---|---|---|
| `name` | string | required | Name (unique). |
| `any` | boolean | optional | Match when any rule passes (default: all must pass). |
| `categories` | [string, …] | optional | Categories (`wall`, `door`…); empty for all. |
| `rules` | [object, …] | optional | Rules on parameters. |

### `filter_set`

*edit* — Change a view filter: name, categories, rules (replaced as a whole) or how rules combine. Views that apply it follow.

| Parameter | Type | | Description |
|---|---|---|---|
| `filter` | id or name | required | Filter id or name. |
| `any` | boolean | optional | Match when any rule passes (`false`: all must pass). |
| `categories` | [string, …] | optional | New categories (empty for all). |
| `name` | string | optional | New name. |
| `rules` | [object, …] | optional | New rules (replace the old ones). |

### `filter_delete`

*edit* — Delete a view filter (views and templates stop applying it).

| Parameter | Type | | Description |
|---|---|---|---|
| `filter` | id or name | required | Filter id or name. |

### `filter_elements`

*query* — The elements a view filter matches (only those drawn in `view` when given).

| Parameter | Type | | Description |
|---|---|---|---|
| `filter` | id or name | required | Filter id or name. |
| `view` | id or name | optional | Only elements drawn in this view. |

### `view_filter_add`

*edit* — Apply a filter to a view (or view template), at `index` in the list (default last: the first matching filter wins for each attribute), hiding what it matches (`visible: false`) or overriding its graphics.

| Parameter | Type | | Description |
|---|---|---|---|
| `filter` | id or name | required | Filter id or name. |
| `index` | integer | optional | Position in the view's filter list (0 = first). |
| `overrides` | JSON | optional | Overrides of the matching elements (see `view_graphics_get`). |
| `template` | id or name | optional | Or a view template id or name. |
| `view` | id or name | optional | View id or name. |
| `visible` | boolean | optional | Show (default) or hide the matching elements. |

### `view_filter_set`

*edit* — Change how a view (or view template) applies a filter: visibility of the matching elements, whether it is enabled, and its overrides (merged; `reset` clears them first).

| Parameter | Type | | Description |
|---|---|---|---|
| `filter` | id or name | required | Filter id or name. |
| `enabled` | boolean | optional | Apply the filter (`false` keeps it listed without effect). |
| `overrides` | JSON | optional | Overrides to merge (see `view_graphics_get`). |
| `reset` | boolean | optional | Clear the overrides before merging. |
| `template` | id or name | optional | Or a view template id or name. |
| `view` | id or name | optional | View id or name. |
| `visible` | boolean | optional | Show (`true`) or hide (`false`) the matching elements. |

### `view_filter_remove`

*edit* — Stop applying a filter in a view (or view template).

| Parameter | Type | | Description |
|---|---|---|---|
| `filter` | id or name | required | Filter id or name. |
| `template` | id or name | optional | Or a view template id or name. |
| `view` | id or name | optional | View id or name. |

### `view_filter_move`

*edit* — Move a filter to `index` in a view's (or view template's) filter list (0 = first, the highest priority).

| Parameter | Type | | Description |
|---|---|---|---|
| `filter` | id or name | required | Filter id or name. |
| `index` | integer | required | New position (0 = first). |
| `template` | id or name | optional | Or a view template id or name. |
| `view` | id or name | optional | View id or name. |

### `templates_list`

*query* — The project's view templates: the properties each holds, which of them it controls (`include`) and the views that follow it.

No parameters.

### `template_create`

*edit* — Create a view template, from the properties of `view` when given (scale, category graphics, filters, plan cut height, building services display, structural display), else with defaults. Every property is controlled (`include`) until changed with `template_set`.

| Parameter | Type | | Description |
|---|---|---|---|
| `name` | string | required | Name (unique). |
| `view` | id or name | optional | View to take the properties from. |

### `template_set`

*edit* — Change a view template: name, properties and which of them it controls (`include`, merged: `{"scale": false}` lets views keep their own scale). Views that follow the template change with it, in the same undo step.

| Parameter | Type | | Description |
|---|---|---|---|
| `template` | id or name | required | Template id or name. |
| `cut_height` | number | optional | Plans: cut plane height, meters. |
| `detail` | string | optional | Detail level: `coarse`, `medium` or `fine`. |
| `discipline` | string | optional | Floor plans: `none`, `plumbing` or `electrical` (services over a faint architecture). |
| `include` | JSON | optional | Which properties the template controls: `scale`, `categories`, `filters`, `cut_height`, `discipline`, `structural`, `phase_filter`, `detail` (true/false). |
| `name` | string | optional | New name. |
| `phase_filter` | id or name | optional | Phase filter (id or name), or `null` for none. |
| `scale` | integer | optional | Scale denominator. |
| `structural` | boolean | optional | Floor plans: structural (formwork) display. |
| `structure_tags` | boolean | optional | Floor plans: tag beams and columns. |

### `template_delete`

*edit* — Delete a view template; views that follow it keep their current properties and become free.

| Parameter | Type | | Description |
|---|---|---|---|
| `template` | id or name | required | Template id or name. |

### `view_template_apply`

*edit* — Copy a template's properties into views once (they stay free to change). Views that follow another template are refused.

| Parameter | Type | | Description |
|---|---|---|---|
| `template` | id or name | required | Template id or name. |
| `views` | [id or name, …] | required | View ids or names. |

### `view_template_assign`

*edit* — Make views follow a template (`template: null` frees them): the properties it controls are copied now and whenever the template changes, and cannot be changed in the view.

| Parameter | Type | | Description |
|---|---|---|---|
| `views` | [id or name, …] | required | View ids or names. |
| `template` | id or name | optional | Template id or name, or null to unassign. |

### `view_temporary`

*query* — Temporarily hide or isolate elements or categories in a view, or reset. This is display state, not a model change: it is not saved, not undone and not printed (sheets and exports show the view as it is); the application marks the view while it is active. `action`: `hide` and `isolate` (the elements `ids`), `hide_category` and `isolate_category` (`categories`, and the categories of `ids`), or `reset`. Hiding adds to what is already hidden; isolating replaces it.

| Parameter | Type | | Description |
|---|---|---|---|
| `action` | string | required | `hide`, `isolate`, `hide_category`, `isolate_category` or `reset`. |
| `view` | id or name | required | View id or name. |
| `categories` | [string, …] | optional | Category names. |
| `ids` | [id, …] | optional | Element ids. |

### `view_temporary_apply`

*edit* — Make a view's temporary hiding permanent: hidden elements become hidden in the view and hidden categories are turned off; when isolating, every other element drawn in the view is hidden in it. The temporary state is then reset. One undo step.

| Parameter | Type | | Description |
|---|---|---|---|
| `view` | id or name | required | View id or name. |

## Phases

### `phases_list`

*query* — The project's construction phases in time order, with how many elements each builds and demolishes and how many views show it.

No parameters.

### `phase_create`

*edit* — Add a construction phase, at the end (the latest) or before another. Elements and views keep their phases.

| Parameter | Type | | Description |
|---|---|---|---|
| `name` | string | required | Name (unique). |
| `before` | id or name | optional | Insert before this phase (id or name; default: after the last). |
| `description` | string | optional | Description. |

### `phase_set`

*edit* — Rename a phase, change its description, or move it to another place in time (`index`, 0 = earliest). A move that would put an element's demolition before its construction is refused.

| Parameter | Type | | Description |
|---|---|---|---|
| `phase` | id or name | required | Phase id or name. |
| `description` | string | optional | New description. |
| `index` | integer | optional | New position in time order (0 = earliest). |
| `name` | string | optional | New name. |

### `phase_delete`

*edit* — Delete a phase. A phase that elements are built or demolished in, or that views show, needs `merge_into`: they move to that phase (an element then built and demolished in the same phase is temporary). The last remaining phase cannot be deleted.

| Parameter | Type | | Description |
|---|---|---|---|
| `phase` | id or name | required | Phase id or name. |
| `merge_into` | id or name | optional | Phase that takes over its elements and views. |

### `elements_phase_set`

*edit* — Set the phase elements are built in and/or demolished in (`null`: not demolished). A demolition cannot come before the construction. Doors, windows and rebar follow their hosts: they are never built before them and are demolished with them (unless demolished earlier).

| Parameter | Type | | Description |
|---|---|---|---|
| `ids` | [id, …] | required | Element ids. |
| `created` | id or name | optional | Phase built in (id or name). |
| `demolished` | id or name | optional | Phase demolished in (id or name), or `null` to keep it standing. |

### `elements_demolish`

*edit* — Demolish elements in a phase: `phase` when given, else the phase of `view`, else the phase new elements are built in (the last by default). Their doors, windows and rebar go with them (unless demolished earlier). Elements not built by then, or already demolished earlier, are refused.

| Parameter | Type | | Description |
|---|---|---|---|
| `ids` | [id, …] | required | Element ids. |
| `phase` | id or name | optional | Phase id or name. |
| `view` | id or name | optional | Or the view whose phase to use. |

### `phase_creation_set`

*query* — Set the phase new elements are built in by this session (the active view's phase in the application): display state, not saved, not undone. `phase` (id or name), or the phase of `view`; neither resets it to the last phase.

| Parameter | Type | | Description |
|---|---|---|---|
| `phase` | id or name | optional | Phase id or name. |
| `view` | id or name | optional | Or the view whose phase to use. |

### `phase_filters_list`

*query* — The phase filters: for each status (`existing`, `demolished`, `new`, `temporary`) whether views using the filter show it `by_category`, `overridden` (with the status graphics, see `phase_graphics_get`) or `not_displayed`.

No parameters.

### `phase_filter_create`

*edit* — Create a phase filter; statuses not given show `by_category`.

| Parameter | Type | | Description |
|---|---|---|---|
| `name` | string | required | Name (unique). |
| `demolished` | string | optional | Demolished elements. |
| `existing` | string | optional | Existing elements: `by_category`, `overridden` or `not_displayed`. |
| `new` | string | optional | New elements. |
| `temporary` | string | optional | Temporary elements. |

### `phase_filter_set`

*edit* — Change a phase filter: its name or how it shows each status. Views using it follow.

| Parameter | Type | | Description |
|---|---|---|---|
| `filter` | id or name | required | Phase filter id or name. |
| `demolished` | string | optional | Demolished elements. |
| `existing` | string | optional | Existing elements: `by_category`, `overridden` or `not_displayed`. |
| `name` | string | optional | New name. |
| `new` | string | optional | New elements. |
| `temporary` | string | optional | Temporary elements. |

### `phase_filter_delete`

*edit* — Delete a phase filter; views and view templates using it are left without one (showing every status by category).

| Parameter | Type | | Description |
|---|---|---|---|
| `filter` | id or name | required | Phase filter id or name. |

### `phase_graphics_get`

*query* — The graphics of each phase status (`existing`, `demolished`, `new`, `temporary`), used by phase filters that show a status `overridden`. Overrides are shaped as in `view_graphics_get`.

No parameters.

### `phase_graphics_set`

*edit* — Change the graphics of a phase status: `overrides` is merged into the current ones (`null` resets an attribute; see `view_graphics_get`), `reset` clears them first.

| Parameter | Type | | Description |
|---|---|---|---|
| `status` | string | required | `existing`, `demolished`, `new` or `temporary`. |
| `overrides` | JSON | optional | Overrides to merge. |
| `reset` | boolean | optional | Clear the status overrides before merging. |
