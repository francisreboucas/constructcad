# Building pads and their slopes — design

Date: 2026-11-06. Status: approved in conversation, awaiting spec review.

Second terrain sub-project, after `2026-11-05-topography-design.md`.

## Goal

A building pad is a flat area at an elevation, graded into a terrain. From
its edges, slopes run out to the existing ground:

- a **cut** slope where the ground stands above the pad;
- a **fill** slope where the ground lies below it;
- each at its own ratio.

The terrain's surface becomes the graded one in plans, sections, 3D and IFC.
The existing surface is kept, and the pad reports its cut and fill volumes.

## Model (cc_core::pad)

`ElementKind::Pad(Pad)`, `Category::Pad`: i18n `category-pad`, pt-BR
"Platôs", en "Building Pads".

```rust
pub struct Pad {
    /// The terrain it grades.
    pub host: ElementId,
    /// Closed outline in plan (either winding).
    pub outline: Vec<DVec2>,
    /// Elevation of the pad above the terrain's level.
    pub elevation: f64,
    /// Horizontal run per unit of rise of the cut and fill slopes; `None`
    /// uses the type's.
    #[serde(default)]
    pub cut_slope: Option<f64>,
    #[serde(default)]
    pub fill_slope: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch: Option<Sketch>,
}
```

### Type and limits

- The type is `TypeData::Pad { cut_slope, fill_slope }`, with defaults 1.0
  (1:1) and 1.5 (1:1.5).
- `MIN_SLOPE` = 0.1 and `MAX_SLOPE` = 100. Repair clamps the type's ratios
  into that range and drops an element's bad overrides.
- The type is created on first use, like the terrain type.

### Element rules

- **Validity:** an outline of 3 to `MAX_OUTLINE` (1024) finite points with
  an area, and a finite elevation.
- **Host:** must be a topography. Repair removes a pad whose host is missing
  or is not a terrain.
- **Level:** the pad's level is its host's. `EditCtx::finish` keeps them
  equal (`sync_pads`), so a terrain moved to another level takes its pads
  along.
- **Deletion:** deleting a terrain deletes its pads, through the same path
  as hosted elements.
- **Moves:** moving or rotating a terrain moves its pads with it (hosted
  transforms). Moving only a pad moves its outline.
- `Element::drawn_sketch` and `sketched_profile` cover the outline.
- Bump `FORMAT_VERSION` to 20.

## Geometry (cc_geom::pad, applied in cc_geom::topography)

### The graded surface

The pads of a terrain apply one after another, in element id (creation)
order, over the result of the previous ones. For a pad at absolute
elevation `z_p`, with `d(x, y)` the distance from the pad's outline (0
inside):

- `C(x, y) = z_p + d / cut_slope`: the cut slope rising away from the edge.
- `L(x, y) = z_p - d / fill_slope`: the fill slope falling away from it.
- `F(x, y) = max(L, min(T, C))`, where `T` is the surface before this pad.

So inside the pad `F` is `z_p`. Outside it, `F` follows the cut slope where
the ground stands above it, the fill slope where the ground lies below it,
and the ground itself beyond the daylight line, where `T = C` or `T = L`.

### Mesh

- **Points:** the vertices are
  - the pad's outline vertices at `z_p`;
  - offset rings at `d` = 0.25, 0.5, 1, 2, 4… m (each ring a polygon
    offset of the outline, with arcs at convex corners sampled every 15°),
    up to the farthest daylight distance;
  - daylight points, found by bisection on `T - C` and `T - L` along each
    ring vertex's outward normal (at most 40 steps);
  - the surface's existing vertices.

  Every vertex gets `z = F`.
- **Constraints:** a spade CDT takes the pad outline, the daylight polyline
  (chained, see `topography::chain`) and the terrain boundary. The existing
  surface's triangle edges are added with `add_constraint_and_split`.
  Consequences:
  - `T` and `F` are both linear on every resulting triangle, so the volumes
    are exact between the two meshes;
  - the graded mesh still passes through every measured point.
- **Limits:**
  - at most `MAX_RINGS` = 16 rings and `MAX_PAD_VERTICES` = 50 000
    vertices per pad;
  - when a limit is reached, the slope samples get coarser and a
    `coarse` warning is reported;
  - the whole step runs inside `catch_unwind`. A failure leaves the
    surface as it was before the pad, with a `failed` warning.
- **Outside the terrain:** the pad and its slopes act only inside the
  terrain's boundary, since the triangles are clipped to it. A slope that
  reaches the boundary before daylight stops there, with an
  `off-terrain` warning.

### Results

- **`TopoGeom` changes:**
  - `existing`: the surface before any pad;
  - `triangles`: the graded surface;
  - `min_z` and `max_z`: over both surfaces;
  - `base_z`: `min_z - base_depth`;
  - `volume`: the graded volume.
- **Per pad**, `ModelGeometry::pads: BTreeMap<ElementId, PadGeom>`:
  - `cut` and `fill`, the volumes between `existing` (as it stood before
    this pad) and the surface after it;
  - `area`, the pad's area in plan;
  - `daylight`, the polylines where the slopes meet the ground;
  - `slopes`, the slope triangles, for the plan symbol;
  - `warnings`.
- **Picking and solids:** a pad has no solid of its own; plans and the
  property palette select it by its outline. In 3D and sections, a click
  on the terrain inside a pad's outline picks the pad.

## Views

### Plans (cc_views::topography)

- **Pad outline:** in `LineStyle::Projection`, with the pad's elevation as
  text at its centroid.
- **Slope symbol** across each slope band, from the pad's edge (the top of
  a fill slope, the foot of a cut slope) to the daylight line:
  - thin ticks perpendicular to the edge, every `spacing` (2.5 mm on
    paper), alternately long (the full band) and short (a third of it);
  - the daylight line in `LineStyle::Thin`.
- **Contours:**
  - the graded surface's contours, as before;
  - the existing contours, in `LineStyle::Hidden`, only inside the area
    the pads changed (the pad outline and its slope bands), clipped to it.
- **Hatch:** the earth above the cut follows the graded surface.

### Sections and elevations

- The graded solid, as before.
- In sections, the existing ground line, where it differs from the graded
  surface, in `LineStyle::Hidden` (the existing triangles sliced by the
  section plane).

### 3D

The graded solid.

## Engine (cc_engine::commands::pad)

- **`pad_create`:**
  - `host`: the terrain id; by default, the only terrain on the level,
    whose boundary contains the outline's centroid;
  - `outline` or `curves`;
  - `elevation` (meters above the terrain's level, required);
  - optional `cut_slope` and `fill_slope`;
  - `type`.

  A project without a pad type gets one. The command refuses a host that
  is not a terrain, and an outline entirely outside the terrain's boundary.
- **`pad_type_create`:** `name`, `cut_slope` and `fill_slope`.
- **Generic commands:**
  - `element_get` and `element_set` show and patch `elevation`, the slopes
    and the outline;
  - `sketch_get` and `sketch_set` work on the outline;
  - `type_set` changes the type's ratios.
- **`element_get` figures:**
  - for a pad: `cut`, `fill`, `net` (fill − cut), `area` and `warnings`;
  - for a terrain: also `cut` and `fill`, the sums over its pads.
- **Schedules:** `Category::Pad` has the fields `Mark`, `Type`, `Level`,
  `Elevation`, `Area`, `CutVolume` and `FillVolume`. The new fields
  `ScheduleField::CutVolume` and `FillVolume` total in their columns.
- **Docs:**
  - `docs/commands.md` regenerated;
  - a paragraph in `docs/automation.md` and `docs/automacao.md`;
  - a section in `docs/interface.md` and `docs/interface.pt-BR.md`;
  - an `AGENTS.md` bullet;
  - a CHANGELOG entry.

## App (cc_app)

- **Ribbon:** *Massing & Site → Site → Pad* (`Place(Category::Pad)`,
  shortcut `PD`, a new icon `assets/icons/ui/pad.svg`).
  - It opens sketch mode for the outline.
  - The options bar shows *Elevation* (`ModifyOptions::pad_elevation`).
  - *Finish* runs `pad_create` with the curves and that elevation; the
    host is the default one.
  - The button is unavailable, with a reason (`why-no-terrain`), when the
    active plan's level has no terrain.
- **Properties:**
  - elevation;
  - cut and fill slopes: a *By type* checkbox per slope, with the ratio
    when unchecked;
  - read-only area, cut, fill and net;
  - warnings;
  - *Edit Boundary*.
- **Type dialog:** the cut and fill ratios.
- **i18n:** keys in all 12 languages.

## IFC (cc_ifc)

- **Export:**
  - The terrain's body is the graded solid.
  - Its pads travel in `PADS_PSET` = `ConstructCAD_Pads` on the terrain:
    one text property per pad (`Pad1`, `Pad2`…) holding the outline,
    elevation, slope overrides, type name and type ratios. The format is
    `elevation;cut;fill;type;type_cut;type_fill|x y;x y;…`, with an empty
    field for no override.
- **Import:**
  - Import recreates the pads on the rebuilt terrain, matching or creating
    pad types by name and ratios.
  - Pad uids are not kept, because IFC4 has no pad object.
- **Classes:** IFC4 has no earthworks class (`IfcEarthworksCut` and
  `IfcEarthworksFill` are IFC4.3), so pads are not separate products.
- **Fixture:** `crates/cc_ifc/tests/fixtures/topography.ccad` gains a pad.
  It stays in the IfcOpenShell job.

## Testing

### cc_geom

- **Pad on an inclined plane** (`z = x / 10`): the pad at mid height has
  cut on the high side and fill on the low side. The daylight distances
  equal `Δz · slope / (1 - slope / 10)` along x. The volumes match the
  analytic prisms within 1e-6.
- **Single-sided cases:**
  - a pad all below the ground (only cut);
  - a pad all above it (only fill).
- **Corners:** a convex corner's slope is conical; its daylight is
  checked at the bisector.
- **Edge cases:**
  - a slope running off the terrain gives the `off-terrain` warning;
  - two overlapping pads apply in order;
  - degenerate input (tiny outline, NaN, a pad outside the terrain) never
    panics.
- **Measured points:** the graded mesh passes through every measured
  point outside the changed area.

### Other crates

- **cc_views:**
  - slope ticks lie between the pad edge and the daylight;
  - existing contours appear only inside the changed area;
  - a section shows the hidden existing ground line.
- **cc_core:**
  - repair drops a pad with a missing or non-terrain host, and clamps the
    ratios;
  - deleting the terrain deletes its pads;
  - moving the terrain moves them;
  - a RON round trip.
- **cc_engine:**
  - `pad_create` with the default and an explicit host;
  - refused hosts;
  - changing the elevation updates the volumes;
  - schedule totals.
- **cc_ifc:** a round trip keeps pads, ratios and volumes.
- **cc_app:** sketch, then *Finish*, creates a pad on the level's terrain.
  The tool is unavailable without a terrain.

## Out of scope

- Sloped (non-level) pads.
- Pads with a body (base course layers).
- Earthworks classes of IFC4.3.
- Walls and floors cutting the terrain.
- Retaining walls at slopes.
- Mass haul diagrams.
