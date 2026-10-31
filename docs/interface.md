# The interface: ribbon, commands and palette

*Versão em português: [interface.pt-BR.md](interface.pt-BR.md).*

ConstructCAD's window follows the layout desktop CAD and BIM users expect. The **quick access toolbar** sits at the top left. Below it is the **ribbon**, a row of tabs with groups of buttons. The **options bar** for the active tool comes next. The **project browser** and **properties** are docked on the left, the **views** fill the tabs in the middle, and the **status bar** is at the bottom. When many views are open, **Close Inactive Views** (in the quick access toolbar and in *View → Windows*) closes every tab but the active one, and right-clicking a tab offers *Close Other Views*.

## Commands and their tooltips

Every button in the ribbon and the quick access toolbar runs an *interface command*. So does every two-letter shortcut and every entry of the command palette. Each command defines, in one place:

- its **name** and **icon**;
- its **shortcut**: a Ctrl chord, or two letters typed while a view has the focus;
- a **description** of what it does;
- the **engine command** that makes the change. Scripts, the `ccad` command line and the MCP server call this same command (see the [automation guide](automation.md));
- whether it **can run now**, and why not.

Hover over a button to see its tooltip:

```
Move  [MV]
Moves the selected elements from a base point to a target.
Unavailable: Select the elements before using the tool
Engine command: elements_move
```

Buttons that cannot run are greyed out, and their tooltip gives the reason. Typical reasons are "nothing to undo", "select the elements first", "open a sheet to export it", "select one text note" and "copy elements first". A shortcut typed when its command cannot run shows the same reason in the status bar.

## Icons and badges

Icons are drawn in two tones: the strokes in the text color and the soft fills in the theme's accent tint. A small badge in the bottom-right corner marks what kind of action a command is:

| Badge | Meaning | Examples |
|---|---|---|
| green **+** | creates something new | new floor or ceiling plan, 3D view, legend, schedule, sheet, new component |
| blue arrow down | brings data in from a file | Import IFC, Import CAD, Load component |
| blue arrow up | writes data out to a file | Export PDF, Export SVG, Export Image |
| red **−** | removes something | Delete |

The selected tool is highlighted in the accent color. Unavailable commands are greyed out, badges included.

## Collapsing the ribbon

To give the views more room, collapse the ribbon to its tab titles. Any of these collapses it or expands it again:

- **Ctrl+F1**;
- a **double click** on any tab;
- the **arrow** at the right end of the tab row.

While the ribbon is collapsed, a click on a tab shows that tab's panel over the views. The panel closes when a command runs, when you press Esc or click elsewhere, or when you click the same tab again. Whether the ribbon starts collapsed is set in *Preferences → Interface*.

## Narrow windows

When the window is narrower than a tab's groups, the groups scroll sideways instead of being cut off. Arrows appear at both ends of the row: click them to scroll, or use the mouse wheel. Each arrow is greyed out once that end is reached.

## Command palette

**Ctrl+Shift+P** opens the command palette, as does the magnifier at the right end of the tab row. Type part of what you are looking for. The search ignores case and accents ("niveis" finds "Níveis"), and every word you type must match. The palette finds:

- **interface commands**, by name, description or shortcut (typing `WA` finds *Wall*);
- **plans to create** for each level ("Floor plan: Ground floor") and **schedules** for each category;
- **views to open** ("Open view: …");
- **engine commands** (`level_create`, `elements_list`, `type_set`…), shown in a monospaced font.

Use ↑ and ↓ to choose and Enter to run, or click an entry. Esc closes the palette. An unavailable command stays in the list, greyed out, with its reason in place of the description.

### Running engine commands

Choosing an engine command opens a form with its description and its arguments in JSON, already filled with the required fields:

```json
{
  "elevation": 9.0,
  "name": "Roof deck"
}
```

**Run** (or Ctrl+Enter) executes the command in the open project and shows its result or error message. An edit is one undo step and is recorded while a macro is recording, like any other change. **Back** (or Esc) returns to the search. The arguments are the same as in scripts and in the [command reference](commands.md). Lengths are in meters, angles in degrees, points are `[x, y]`, and levels, types, views and materials can be named by id or by name.

The engine's file commands (`project_open`, `project_save`, `ifc_import`, `ifc_export`, `sheet_export`, `view_export`…) are not offered here. Their interface commands in the *File* and *Insert* tabs and the palette handle files through the usual dialogs.

## Keyboard

| Key | Action |
|---|---|
| Ctrl+Shift+P | Command palette |
| Ctrl+F1 | Collapse / expand the ribbon |
| Ctrl+, | Preferences |
| Ctrl+N / Ctrl+O / Ctrl+S / Ctrl+Shift+S / Ctrl+W | New / Open / Save / Save as / Close project |
| Ctrl+Z / Ctrl+Y | Undo / Redo |
| Ctrl+X / Ctrl+C / Ctrl+V | Cut / Copy / Paste |
| Delete | Delete the selection |
| Esc | Cancel the operation, leave the tool, clear the selection |
| Space | Rotate or flip the selection or the element being placed |

Two-letter keys work while a view has the focus and no text field is being edited. The README lists them all. Each tooltip shows its own shortcut as well.

## Selecting in 2D views

A click picks the element under the pointer (Ctrl adds or removes it). A drag that starts on empty paper, or inside a floor, a hatch or another fill, draws a selection box: dragged to the right it is a window and takes the elements wholly inside it (their lines, fills and texts); dragged to the left it is a crossing, dashed and green, and also takes those it cuts or lies in: a wall crossed between its ends, a floor around the box, a text anchored in it. Room areas and imported drawings are not taken by a crossing (a window around them takes them). Ctrl keeps the current selection and adds to it. A drag that starts on an element's line, or on an element already selected, moves it instead (Ctrl+drag copies).

Tags of doors, windows, beams, columns and footings can be dragged to another spot with the Select tool (the pointer turns into a hand over them). The tag keeps its text and gets a leader back to its element; each plan keeps its own tag positions (a duplicated plan starts with those of the original). *Reset Tag*, in the element's properties, puts it back where it sits by default.

## Selecting in 3D views

With the Select tool, a click picks the element under the pointer (Ctrl adds or removes it). Shift+drag draws a selection box, as in plans: dragged to the right it is a window and takes the elements wholly inside it; dragged to the left it is a crossing and also takes those it touches. Elements behind others count too; hidden ones do not. Ctrl keeps the current selection and adds to it. With *Dragging in 3D views selects* on in *Preferences → Interface*, a plain left drag draws the box and Shift+drag orbits instead.

## Modifying in 3D views

**Move**, **Copy**, **Array**, **Rotate**, **Mirror** (pick or draw the axis), **Align** and **Join Geometry**/**Unjoin Geometry** also work in 3D views; left-drag still orbits (or Shift+drag, see above), middle or right drag pans.

- Points are picked on a horizontal working plane at the level of the elements being changed; once the first point is picked, the next ones stay at its height. A square marks a snap to a model point near the pointer: a corner of an element, the center of a column or component at its base or top, a wall end, a grid crossing. A cross marks a point on the plane. Shift keeps the main axis; a typed distance and Enter work as in plans. The preview shows the elements' outline where they will go.
- Every change is a plan transform: elements keep their level. Moving a level 2 column from one of its corners to the matching corner of the column below puts it over that column.
- **Align** (AL, also in plans): click the face or line to align to, then the face or line of the element to move. While you point, the face or line a click would take is drawn thick; once the reference is chosen it shows as a red dashed line across the view, until the second click or Esc. The element moves in plan, so the two may be on different levels (a column above onto the face of the column below). Every element aligns by the face or side nearest to the click (a wall by either face, a column by a side); grids, and elements picked from the command line without a point, by their location line. When the two are not parallel, the element first turns about the point clicked until they are.
- **Trim/Extend to Corner**, **Trim/Extend to Reference**, **Split Element**, **Offset** and **Allow/Disallow Join** pick ends and sides on plan linework and stay unavailable in 3D views, with that reason in their tooltip.

## View graphics

*View → Graphics* holds **Visibility/Graphics** (VG), **Filters** and **View Templates** (also under *Manage*).

- **Visibility/Graphics** edits the active view (or, from the template manager, a template). The *Model Categories* and *Annotation Categories* tabs list each category with a visibility box and cells for projection lines, surface fill, cut lines, cut fill, halftone and transparency; a cell shows the override (— when none) and opens an editor with color, weight (paper mm) and pattern. The *Filters* tab lists the filters applied to the view, in priority order (↑ ↓), with enabled and visible boxes and the same cells; *Add Filter…* applies another one and *Edit Filters…* opens the filter editor. When the view's template controls categories or filters, the tab says so and is read-only. OK keeps the changes, Cancel undoes them.
- **Filters** edits the project's filters: name, categories (none: all) and rules — a parameter, an operator and a value — combined with AND or OR, with the number of elements matching.
- **View Templates** lists the templates; *Create from View* makes one from the active view. Each property has an *Include* box; *Apply Properties* copies the template into the active view once, *Assign to View* makes the view follow it. The view's properties show its template (choose *None* to free it) and a *Visibility/Graphics* button; properties the template controls are read-only.
- With elements selected, **Override Graphics** changes their graphics in the active view only and **Hide in View** (EH) hides them there (shown again from Visibility/Graphics).
- **Hide/Isolate** (temporary): HH hides the selection, HI isolates it, HC and IC hide or isolate its categories, HR resets. While active, the view has a colored border and a chip with *Reset* and *Make Permanent*. Temporary hiding is not saved, not undone and not printed.

## Conceptual masses

*Architecture → Massing → Mass* (`MS`) sketches an outline in a plan, like a floor, and extrudes it up to the level above. Masses are for massing studies before walls and floors exist: see-through bodies in 3D views, elevations and sections, cut in the plans they cross.

- **Properties**: the base offset and the height, and whether the mass is an extrusion or a loft (a top outline different from the base, made with `mass_create` and `top` or `top_inset`).
- **Mass floors**: one box per level the mass spans, with the area of the floor at that level. Plans draw the floor of their level with its area; the properties and the *Masses* schedule give the gross floor area (the sum of the floors), the number of floors and the volume.
- **Push/Pull** (*Massing → Push/Pull*, `PP`, in a 3D view): click a face of a mass, then move the pointer along the arrow (or type the distance) and click or press Enter. The top changes the height, the bottom moves down or up, and a side of an extrusion moves out or in, the sides beside it keeping their directions (a loft's sides stay; push its top or bottom). A mass with a floor at every level it spans gets the floors of the levels it reaches. Esc lets go of the face.
- **From the mass to the building**: with masses selected, *Massing → Walls by Face* puts a wall on each side of an extrusion (exterior face on the mass's face, as tall as the mass, joined at the corners), *Floors from Mass Floors* a floor on each mass floor (the mass's outline at that level, its top at the level) and *Roof by Face* a flat roof on the top. The walls, floors and roof are ordinary elements: change their types, heights and shapes as usual. A loft's slanted sides take no walls.
- Moving, rotating, mirroring, copying and pasting work as for other elements. Deleting a level removes its mass floors. IFC export leaves masses out.

## Imported CAD drawings

- **Insert → Import CAD…** places a DWG or DXF drawing in the active floor plan, ceiling plan, elevation, section or legend (the button is greyed out in other views, and says why). You can also drop `.dwg` and `.dxf` files on the window. The file dialog starts in the *Projects folder*. The drawing keeps its origin at the view's origin, unless that leaves it more than 1 km from the model (a drawing in survey coordinates), then it is centered on what the view shows. The status bar says how many items and layers were read, how many entities were not drawn (3D solids, images, infinite lines…) and which units were assumed when the file has none.
- The drawing is a background to trace over: thin grey lines under the model, with its texts. It shows only in the view it was imported into (not in other plans or in 3D views), prints on sheets and exports, and is left out of IFC export. *Visibility/Graphics* hides or overrides it with the *Imported CAD* category.
- It starts **pinned**, so it is not moved by accident; *Unpin* it to move or rotate it with the usual tools. Clicking its lines selects it only where no model element is under the pointer.
- Its properties set the **units** (read from the file, else assumed), an extra **scale**, the **rotation**, the **position** of its origin, the **colors** (*Grey*, *Layer colors* with very light colors darkened for white paper, or *Black*) and **Halftone**. The *Layers* section shows or hides each layer (*All layers* toggles them all); layers that are off or frozen in the file start hidden.
- The drawing tools snap to its endpoints and vertices, midpoints, arc and circle centers (a circle marker), and to points on its lines.

## Construction phases

- **Manage → Phases** has three tabs. *Phases* lists the project's phases from earliest to latest with their name, description and how many elements are built and demolished in each; *Add* appends one, *Insert Before* puts one before the selected phase, ↑ ↓ reorder (a move that would put a demolition before its construction is refused), and deleting a phase moves its elements and views into the phase chosen under *Merge elements and views into*. *Phase Filters* sets, for each filter, how existing, demolished, new and temporary elements show: *Not Displayed*, *By Category* or *Overridden*. *Phase Graphics* sets the overrides used for each status (the same cells as Visibility/Graphics). OK keeps the changes, Cancel undoes them.
- The view's properties (plans, elevations, sections, 3D views, schedules) have a *Phasing* section with its *Phase* and *Phase Filter*; the filter is read-only when the view's template controls it. Elements show *Phase Created* and *Phase Demolished*. New elements are built in the active view's phase.
- **Modify → Demolish** (DE) demolishes the clicked elements in the active plan's, elevation's or section's phase; their doors and windows go with them.

## Component modeling environment

*Edit Component* (*Architecture → Model*, or *Edit Component…* in a component type's properties) opens a workspace of its own for the selected component, or the one the *Component* tool places. *Quick Edit…* keeps the older one-window editor.

- **Views:** the project's views give way to the component's *Plan*, *Front*, *Side* and *3D* views. The wheel zooms, a middle or right drag pans, and a middle double-click fits the view.
- **Ribbon:** the *Create* tab holds *Select*, *Extrusion*, *Revolution*, *Sweep*, *Blend*, *Reference Plane*, *Dimension*, *Equality (EQ)*, *Save Component…*, *Finish* and *Cancel Editing*.
- **Left panel:** the type whose values the views show, the parameters (name, value, whether elements may override it, ×, and *+ Parameter*) and the properties of the selected plane or dimension.
- **Reference planes:** three origin planes are always there: *CenterX* (center left/right), *CenterY* (center front/back) and *Base*. *Reference Plane* takes two clicks in the plan, front or side view; a mostly vertical or mostly horizontal line sets which way the plane faces. A plane's position is an expression (`Left + Width`), and its name can be used in any expression of the component, so a piece that reads `x = Left` follows the plane.
- **Dimensions:** *Dimension* takes two parallel planes, then measures them where you clicked. In its properties, a *Label* (a length parameter, or a new one typed under *New parameter*) makes the parameter drive the second plane. Changing the value changes the parameter, or moves the plane when the dimension has no label.
- **Sketched pieces:** *Extrusion* and *Revolution* draw an outline in the plan, front or side view. The options bar picks *Lines* (click the first point again to close; a non-zero *Arc radius* makes the next segment an arc), *Rectangle* (two corners) or *Circle* (center, then a point), and *Solid* or *Void*. A point that lands on a reference plane is locked to it (a square marker; free points show a circle), so the piece follows the plane. An extrusion is made as soon as the outline closes; a revolution then asks for its axis, a plane seen as a vertical line or any point. The new piece is selected: its properties set the name, *Void*, *Shown in plans*, the material and where it starts and ends (or its axis and angle), each an expression that the ⌖ menu locks to a plane.
- **Equalities (EQ):** *Equality (EQ)* takes three parallel planes in one view: an outer one, the one kept halfway, then the other outer one. The middle plane's position becomes `(a + b) / 2`, so it stays centered as the others move; the view shows two dimensions marked EQ. Moving the middle plane by hand would break it and is refused; delete the equality first.
- **Sketch points:** a sketched piece's properties list its points (and a sweep's path and section, a blend's top). Each coordinate is an expression; type a new one, or lock it to a plane seen edge-on in the sketch's view with ⌖.
- **Sweeps and blends:** *Sweep* draws a path with lines and arcs (click the last point again to end it) and carries a section along it, set in the options bar as a rectangle (width × height) or a circle (diameter), centered on the path; corners are mitered. *Blend* takes a bottom outline and then a top outline, drawn in the same view, and joins them; its start and end are set in its properties like an extrusion's.
- **Selection and keys:** click a plane's line, a dimension's text or a piece's edge to select it; Delete removes it, and Esc steps back through the tool's picks, the tool and the selection.
- **Finish and Cancel:** every change applies to every type of the component and undoes like any other edit. *Finish* keeps them, *Cancel Editing* undoes everything done since the environment opened.

## Preferences

*File → Preferences* (Ctrl+,, also at the bottom of the home screen) holds the settings of the app itself; they are not part of any project and are kept between sessions. Pick a page on the left. Changes show at once; **OK** keeps them and **Cancel** (or Esc) brings back the values the window opened with. **Restore defaults** (bottom left) resets the settings of the page shown; the language and the folders stay as they are.

- **General**: the interface language, the theme (light, dark or the system's) and *Check for updates automatically* (see the update notice below).
- **Interface**: the *Interface scale* (75–200%, everything grows: panels, icons, text; Ctrl+Plus and Ctrl+Minus change it too), the *Text size* (small, normal, large, extra large; only the text, at any scale), *Start with the ribbon collapsed*, *Detailed tooltips* (when off, command tooltips show only the name, the shortcut and why a command is unavailable, without the description and the engine command) and *Dragging in 3D views selects* (a left drag draws a selection box and Shift+drag orbits; off, the other way round).
- **File locations**: the *Macros folder* (the macro file dialogs start there and *Manage → Run Macro* lists its macros), the *Projects folder* (where Open, Import IFC, Import CAD and the first Save of a new project start; a saved project is saved again next to itself), and the component library folders (*Add Folder…*, *Remove*). The component library window's *Manage Folders…* opens this page. A folder that no longer exists is marked *Not found* and skipped.
- **Saving**: the *Autosave* interval (off, or every 1, 2, 5, 10 or 15 minutes; see below) and how many recent projects the home screen lists.
- **Keyboard shortcuts**: every command with its keys. Click a command's box and type two letters to give it new keys (they replace the old ones at once, in tooltips, the palette and views); keys another command already has are refused and the row says which. Leave the box empty to take the keys away, and *Default* brings back the original ones. Search finds a command by name or keys. Ctrl shortcuts, Delete and Esc are fixed.

## Update notice

ConstructCAD tells you when a newer version is published; it never downloads or installs anything by itself, only when you choose **Update Now**.

- **When**: at startup, at most once a day, in the background (the window never waits for it). *File → Check for Updates* (also in the *About* dialog) checks at once and reports "up to date", the new version, or that it could not check.
- **What is contacted**: one HTTPS request to the GitHub API, `https://api.github.com/repos/francisreboucas/constructcad/releases/latest`, which returns the latest published release. The request sends the usual connection data (your IP address) and a `ConstructCAD/<version>` user agent; no project data, identifier or usage information. GitHub's privacy statement applies to it.
- **What you see**: when the release is newer than the running version (pre-releases are ignored), a notice appears in the status bar and on the home screen: *Download* opens the release page in your browser, *Skip This Version* hides the notice until a later version comes out, and × hides it until the next check. Any failure (offline, timeout, proxy, rate limit) is silent.
- **Update Now**: where ConstructCAD can replace itself safely, the notice and the check's dialog offer *Update Now*. It downloads the package that fits how this copy was installed and checks its SHA-256 against the release's `SHA256SUMS.txt` (both from the release's own download links on GitHub) before anything changes; a download that does not match is deleted and the installed copy is left as it was.

  | Installed as | Update Now does |
  |---|---|
  | AppImage | Downloads the new AppImage next to the old one, checks it, renames it over the old file and offers *Restart Now* |
  | Windows installer (`.msi`, under Program Files) | Downloads and checks the installer; it runs when ConstructCAD closes (*Install Now* closes it, asking about unsaved changes first; *On Exit* waits), and ConstructCAD opens again when it is done |
  | Windows `.zip` | Downloads and checks the archive, puts the new `constructcad.exe` and `ccad.exe` in place (the old ones are kept as `.old` and removed at the next start) and offers *Restart Now* |
  | macOS app | Downloads and checks the disk image and opens it: drag ConstructCAD to Applications |
  | `.deb` | Nothing: the dialog shows the `apt` command, with Copy, and *Download* gets the package |
  | `.tar.gz`, builds from source | Nothing: *Download* opens the release page |

  Update Now needs write access to the installed files: an AppImage in a folder you cannot write to fails with a message, and *Download* is the way to update it.
- **Turning it off**: clear *Check for updates automatically* in *Preferences → General*. The environment variable `CONSTRUCTCAD_NO_UPDATE_CHECK=1` turns off every check, automatic or on request (useful for CI, kiosks and offline installations). ConstructCAD remembers the last check time, the skipped version and the setting with its other preferences.

## Autosave and recovery

A minute after a project gets unsaved changes (a new project that was never saved too; *Preferences → Saving* sets another interval or turns autosave off), ConstructCAD writes a copy of it, in the background, to the `recovery` folder of its data folder (`%APPDATA%\ConstructCAD\data` on Windows, `~/Library/Application Support/ConstructCAD` on macOS, `~/.local/share/constructcad` on Linux). It writes again only after the model changes, and never over your project file. Saving, closing the project or quitting removes the copy.

If ConstructCAD crashes or is killed, the next start shows *Recover Unsaved Work* with the copies left behind: *Restore* opens the chosen project with its unsaved changes (saving it writes to its original file, or asks where for a project that was never saved), *Discard* deletes the copy, and *Cancel* keeps the copies for the next start. A crash in the interface also writes a last copy before the app closes, and every crash is appended to `crash.log` in the data folder; attach it when you report a crash.

## Graphics start-up

ConstructCAD draws with the system's native graphics API (DirectX 12 or Vulkan on Windows, Metal on macOS, Vulkan on Linux) and falls back to OpenGL by itself when that fails, which some older or small integrated GPUs need:

- If the native API reports an error, the same start tries OpenGL. If it crashes before the window appears, ConstructCAD starts again once with OpenGL; if the whole process dies (a driver crash), the next start tries OpenGL first.
- Once OpenGL was needed, it stays first: the choice is kept in the file `gpu-backend` of the data folder. Delete that file (after updating the graphics driver, for example) to try the native API again.
- Every failed start is written to `crash.log` in the data folder. When no API works, a message says so and names that file.
- The environment variable `WGPU_BACKEND` picks the API yourself and turns the fallback off: `gl` for OpenGL, `dx12`, `vulkan` or `metal`, or several separated by commas (on Windows, for example, `set WGPU_BACKEND=gl` in a command prompt before starting `constructcad.exe`, or as a user environment variable). `WGPU_POWER_PREF=low` prefers the integrated GPU on computers with two.

## Accessibility

The ribbon buttons, tab titles, scroll arrows, palette entries, browser rows, home screen cards, dialog close buttons, property sections and the view cube's home button are drawn by ConstructCAD itself. They report their role (button, selectable item, collapsible header), their name and their state (enabled, selected, expanded) to the operating system's accessibility interface, so screen readers can announce them.

## For developers

### Adding a command to the interface

1. Make the change an engine command first (see *For developers* in the [automation guide](automation.md)).
2. Add a variant to `UiCommand` in `crates/cc_app/src/commands.rs`. Then fill in its `icon`, `label`, `description` (an i18n key `cmd-desc-…` in every `.ftl` file), `shortcut` (if any), `engine` (the engine command's name), `badge`, `unavailable` (a localized reason for each state in which it cannot run), `selected` (for tools) and `run`. Add it to `UiCommand::all`.
3. Place it in a ribbon group with `command_button` (large) or in a family of small buttons with `stacks` (`crates/cc_app/src/ribbon.rs`).

The tooltip, enabling, the palette entry and a two-letter shortcut all follow from the command. Tests check that each engine name exists in the catalog, that shortcuts are unique, and that every command appears in the ribbon with an accessible name.

### Icons

Icons are SVG files in `assets/icons/ui` on a 24-unit grid. They use white strokes (`stroke-width` 1.75, round caps and joins) and soft fills written as `fill="#fff" fill-opacity=".2"`. `icons.rs` splits each icon into two layers by those attributes: the strokes and solid marks form the ink, and the soft fills form the tint. Each layer is rasterized with resvg at the exact pixel size and painted in the theme's colors (`icons::Tones`). Add the new icon to `Icon`, `Icon::ALL` and `Icon::source`. `ICON_SHEET=sheet.png cargo test -p cc_app icon_sheet` writes every icon at several sizes for review.

### Theme

Colors are tokens of `theme::Palette`: `PALETTE` (light) and `DARK`, with a test of their contrast. Paint with `theme::palette(ctx)` so a dark theme can be switched on later.

### Interface tests

`crates/cc_app/src/ui_tests.rs` runs the ribbon and the palette headlessly with [egui_kittest](https://crates.io/crates/egui_kittest). The tests click and type through the accessibility tree, so custom widgets must call `Response::widget_info`. Screenshot references live in `crates/cc_app/tests/snapshots`. They cover each ribbon tab, the narrow and collapsed ribbon, a rich tooltip, the palette, each page of the Preferences window, and an icon gallery in both themes. GPUs render slightly differently, so the screenshot comparisons run on request:

```sh
CC_UI_SNAPSHOTS=1 cargo test -p cc_app snapshots     # compare with the references
UPDATE_SNAPSHOTS=1 cargo test -p cc_app snapshots    # rewrite them after an intended change
```

Review the new images before committing them. On a mismatch, kittest writes `*.new.png` and `*.diff.png` next to the references; Git ignores those files.
