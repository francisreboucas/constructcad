# Topography (terrain surface) — design

Date: 2026-11-05. Status: approved in conversation, awaiting spec review.

First of several terrain sub-projects. Later, separately: a surface from an
imported DWG/DXF (points and contours with elevations), building pads with
cut/fill volumes, property lines and setbacks (issue #159), sub-regions,
contour labels.

## Goal

A native terrain element: measured points (x, y, z) inside a sketched boundary,
triangulated into a surface, closed down to a flat base into a solid. Plans
draw contours and the cut. Elevations, sections and 3D show it, and sections
hatch it in its material. It exports to IFC and is reachable through engine
commands, `ccad` and MCP.

## Model (cc_core)

New module `cc_core::topography`, `ElementKind::Topography(Topography)`,
`Category::Topography` (i18n `category-topography`, pt-BR "Terreno").

```rust
pub struct Topography {
    /// Measured points; z relative to the element's level elevation.
    pub points: Vec<DVec3>,
    /// Closed boundary in plan (either winding).
    pub boundary: Vec<DVec2>,
    /// Depth of the flat base below the lowest surface point (> 0).
    #[serde(default = "default_base_depth")] // 2.0
    pub base_depth: f64,
    /// The curves the boundary was drawn with (`cc_core::sketch`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch: Option<Sketch>,
}
```

- `MAX_POINTS = 20_000`. The boundary is bounded by `MAX_OUTLINE`, like masses.
- `is_valid`: a boundary of 3 or more finite points, finite points, and a finite
  `base_depth > 1e-6`.
- `Project::repair` drops non-finite points and points that duplicate another
  in plan (within `1e-6`, keeping the first), and resets a bad `base_depth`.
  An element whose boundary is invalid is removed, like other outline elements.
- The type is `TypeData::Topography { material, contour_interval, major_every }`.
  Its serde defaults are 1.0 m and 5. Repair clamps the interval to at least
  0.01 and `major_every` to at least 1. The type is created on the first use,
  with a project earth material (added if missing, by name).
- `Element::drawn_sketch`/`sketched_profile` cover the boundary, so
  `sync_sketches` drops a sketch that no longer makes it.
- `Element::transform`/`translate` move the points (in plan), the boundary and
  the sketch.
- Bump `FORMAT_VERSION`. The new fields have serde defaults, so older files
  load unchanged.

## Geometry (cc_geom::topography)

- Dependency: `spade` (MIT/Apache-2.0, pure Rust) for constrained Delaunay
  triangulation with exact predicates.
- Point elevations are measured; boundary vertices get theirs by
  interpolation:
  1. Triangulate the points inside the boundary (unconstrained Delaunay) and
     interpolate linearly to give each boundary vertex a z.
  2. Outside the points' convex hull, a boundary vertex takes the z of the
     nearest point.
- The surface triangulates the boundary vertices and the inner points together,
  with the boundary edges as constraints. Only triangles inside the boundary
  are kept, so non-convex boundaries work. Points outside the boundary are
  kept in the model but ignored.
- Degenerate cases:
  - Fewer than 3 usable points: a flat surface at the level elevation, or at
    the single point's z.
  - Collinear points: the surface still triangulates through the boundary
    vertices.
  - Each degenerate case produces a warning string.
  - No panics; `spade` insertion errors are skipped point by point.
- The result:
  ```rust
  pub struct TopoGeom {
      pub triangles: Vec<[DVec3; 3]>,
      pub min_z: f64,
      pub max_z: f64,
      pub base_z: f64,
      pub area: f64,
      pub volume: f64,
      pub warnings: Vec<String>,
  }
  ```
  `base_z` is `min_z - base_depth`. `area` is the area in plan. `volume` is
  analytic: the sum over triangles of the plan area times the mean height
  above `base_z`.
- The solid: the surface triangles as the top (`Face::smooth`), the flat base,
  and vertical side faces along the boundary. It is pushed to
  `ModelGeometry::solids` with the category and the type material's colors and
  appearance. It is stored in `ModelGeometry::topographies: BTreeMap<ElementId, TopoGeom>`.
- `contours(geom, interval, major_every, max) -> Vec<Contour { z, major, polyline }>`:
  - Each triangle is cut at the multiples of the interval and the segments are
    chained into polylines by shared endpoints.
  - The interval is multiplied by 2 until the count is at most `max`
    (`MAX_CONTOURS = 500`).
  - Chaining is bounded by the segment count.

## Views (cc_views)

- Floor plans draw the terrain of their level in every plan, whatever the
  height.
- Below the cut plane:
  - Minor contours as `LineStyle::Thin` and majors as `LineStyle::Projection`
    (there is no heavier projection style, so the weight difference comes from
    these two).
  - The boundary as projection.
- Where the cut plane crosses the surface:
  - The part above the cut is filled with the material's cut pattern
    (`cut::cut_fill`), using the triangles clipped by the plane z > cut. Fills
    have no strokes.
  - The cut-height contour is drawn as `LineStyle::Cut`, together with the
    boundary segments above the cut.
- Ceiling plans draw nothing. MEP plans fade it like the rest of the architecture.
- Elevations, sections and axonometric views go through the generic solid
  path. Section cuts are hatched in the type's material and coarse detail uses
  the poché.
- Graphics overrides, filters and phases work through the existing `Resolver`,
  because lines and fills carry the element.

## Engine (cc_engine::commands::topography)

- `topography_create`:
  - Parameters: `level`, optional `type`, `points: [[x, y, z]]`, either
    `boundary: [[x, y]]` or `curves: [CurveArg]` (the sketch is kept), and an
    optional `base_depth`.
  - A project without a terrain type gets one.
- `topography_points_set`:
  - Parameters: `element`, and either `points` (replace all) or the edits
    `add: [[x, y, z]]`, `remove: [index]` and `set: [{index, z}]`.
  - Indices are checked with `get`; an out-of-range index is an error and the
    edit is refused.
- `topography_type_create`: parameters `name`, `material`, `contour_interval`
  and `major_every`.
- `type_set` and `type_get` cover the new type fields. `element_get` and
  `element_set` (`element_params`) show and patch `base_depth`, points and
  boundary.
- `element_get` adds `topography: { min_z, max_z, base_z, area, volume, warnings }`.
- `sketch_get` and `sketch_set` work on the boundary.
- Schedules: `Category::Topography` fields `Mark`, `Type`, `Level`, `Area`,
  `Volume`.
- Register everything in `registry!`, with doc comments on every parameter.
- Regenerate `docs/commands.md`, and add a paragraph to `docs/automation.md`
  and `docs/automacao.md`.

## App (cc_app)

- A ribbon button *Terrain* (`UiCommand` `Place(Category::Topography)`,
  shortcut `TS`) in a *Site* group of the *Massing & Site* tab (which already
  holds the massing group), with its own icon `assets/icons/ui/topography.svg`
  (24 grid, the house style).
- The button opens sketch mode (`Tool::Sketch { category: Topography }`) for
  the boundary. Sketch mode gets one more tool for this category, *Point*,
  which places points at a clicked position. The options bar has an
  *Elevation* field (the last value is kept) for the z of the next point.
- Points show in the sketch preview as small crosses with their elevation,
  and can be picked and deleted. *Finish* runs `topography_create` with the
  `curves` and `points`.
- Properties:
  - `base_depth`.
  - A collapsible *Points* section (`table.rs`) with editable x, y and z per
    row and a remove button. It goes through `topography_points_set` or
    `element_set`, with dragged values merged into one undo step.
  - Read-only min/max z, area and volume.
  - *Edit Boundary* reopens the sketch, keeping the points.
- Type properties: material, contour interval and `major_every`.
- i18n keys in all 12 languages.

## IFC (cc_ifc)

- Export:
  - An `IfcGeographicElement` with `PredefinedType` `TERRAIN`, contained in
    its level's storey like the other elements (valid in IFC4, and the
    elevations stay relative to the level), typed by an
    `IfcGeographicElementType` with the earth material.
  - The body is an `IFCFACETEDBREP` (the writer's existing path for mesh
    solids), not a new triangulated face set.
  - The property set `ConstructCAD_Topography` holds the points, the boundary
    and `base_depth` (as text-encoded lists) and the type's contour settings.
- Import:
  - An `IfcGeographicElement` carrying that property set becomes a native
    `Topography` again.
  - Other terrain stays a generic component, as today.
- CI:
  - Add a fixture `crates/cc_ifc/tests/fixtures/topography.ccad` to the
    IfcOpenShell validation job (`.github/workflows/ci.yml`).
  - Cover export → import in `tests/round_trip.rs`.

## Testing

- **cc_geom:**
  - The surface passes exactly through the points.
  - A non-convex (L-shaped) boundary leaves no triangle outside it.
  - The boundary z matches the interpolation.
  - The analytic volume of a known inclined plane over a rectangle matches.
- **Degenerate inputs**, with no panics and the expected warnings: 0, 1 or 2
  points, collinear points, duplicate points, NaN/inf, points outside the
  boundary, a self-touching boundary.
- **Contours:**
  - An inclined plane gives straight parallel contours, with the expected
    count and majors.
  - The interval doubling caps the count.
- **cc_views plan:** the hatched fill appears above the cut, and only
  projection contours appear when the cut is above the surface.
- **cc_core:**
  - A RON round trip.
  - Repair of a crafted file with bad points or depth.
  - A transform moves the points.
- **cc_engine:** `topography_create` with a boundary or curves, the
  `topography_points_set` edits and index errors, and the `element_get`
  figures.
- **cc_ifc:** export → import gives back a native element with the same points.
- **cc_app** (ui_tests): sketch a boundary, place a point, finish, and check
  the element.

## Out of scope

- Walls, floors and pads cutting the terrain.
- Terrain from imported CAD drawings.
- Contour labels.
- Sub-regions.
- Property lines.
- A "ground line" treatment in elevations.
