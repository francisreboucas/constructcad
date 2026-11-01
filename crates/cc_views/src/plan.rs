use crate::{Drawing2D, LineStyle};
use cc_core::{DetailLevel, ElementKind, LevelId, Project, TypeData, ViewKind};
use cc_geom::{Column, ModelGeometry, WallGeom, polygon};
use glam::DVec2;
use std::f64::consts::{PI, TAU};

pub(crate) use crate::cut::POCHE;
const CEILING_GRID: f64 = 0.625;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColumnCut {
    Cut,
    Below,
    None,
}

fn column_state(c: &Column, cut_z: f64) -> ColumnCut {
    if c.is_solid_at(cut_z) {
        ColumnCut::Cut
    } else if c.solid.first().is_some_and(|s| s.0 < cut_z) {
        ColumnCut::Below
    } else {
        ColumnCut::None
    }
}

fn style_of(state: ColumnCut) -> Option<LineStyle> {
    match state {
        ColumnCut::Cut => Some(LineStyle::Cut),
        ColumnCut::Below => Some(LineStyle::Projection),
        ColumnCut::None => None,
    }
}

/// Floor plan (`ceiling == false`) or reflected ceiling plan of `level`.
pub fn plan(
    project: &Project,
    geom: &ModelGeometry,
    level: LevelId,
    cut_height: f64,
    ceiling: bool,
    scale: u32,
) -> Drawing2D {
    plan_of(project, geom, level, cut_height, (ceiling, false), scale, DetailLevel::Medium)
}

/// Whether a structural plan shows the element: slabs, the load-bearing
/// walls and columns, beams and foundations.
pub fn is_structural(e: &cc_core::Element) -> bool {
    match &e.kind {
        ElementKind::Wall(w) => w.structural,
        ElementKind::Column(c) => c.structural,
        ElementKind::Floor(_) | ElementKind::Beam(_) | ElementKind::Foundation(_) | ElementKind::Grid(_) => true,
        _ => false,
    }
}

/// A plan as [`plan`] draws it, at `detail`; a `structural` plan shows only
/// the structure, with the beams above the cut drawn as seen from below.
pub fn plan_of(
    project: &Project,
    geom: &ModelGeometry,
    level: LevelId,
    cut_height: f64,
    (ceiling, structural): (bool, bool),
    scale: u32,
    detail: DetailLevel,
) -> Drawing2D {
    let shown = |e: &cc_core::Element| !structural || is_structural(e);
    let spacing = crate::cut::spacing(scale);
    // Zoom to fit leaves the elevation and section markers out.
    let mut d = Drawing2D { fit_elements: true, ..Default::default() };
    let level_z = project.level_elevation(level);
    let cut_z = level_z + cut_height;

    for e in project.elements.values().filter(|e| e.level == level && shown(e)) {
        let id = Some(e.id);
        match &e.kind {
            ElementKind::Floor(f) if !ceiling => {
                d.polyline(&f.boundary, true, LineStyle::Projection, id);
                for o in f.all_openings() {
                    d.polyline(o, true, LineStyle::Projection, id);
                }
            }
            ElementKind::Ceiling(c) if ceiling => {
                d.polyline(&c.boundary, true, LineStyle::Projection, id);
                ceiling_grid(&mut d, &c.boundary, e.id);
                for cut in &c.cuts {
                    d.polyline(cut, true, LineStyle::Projection, id);
                }
            }
            ElementKind::Roof(r) => {
                let outline = polygon::offset(&r.footprint, r.overhang);
                d.polyline(&outline, true, LineStyle::Projection, id);
                if r.overhang > 0.0 {
                    d.polyline(&r.footprint, true, LineStyle::Hidden, id);
                }
                // Courtyards: the eaves reach over them by the overhang.
                for hole in &r.holes {
                    d.polyline(&polygon::offset(hole, -r.overhang), true, LineStyle::Projection, id);
                    if r.overhang > 0.0 {
                        d.polyline(hole, true, LineStyle::Hidden, id);
                    }
                }
                for cut in &r.cuts {
                    d.polyline(cut, true, LineStyle::Projection, id);
                }
                for &(a, b) in geom.roof_ridges.get(&e.id).into_iter().flatten() {
                    d.line(a, b, LineStyle::Projection, id);
                }
            }
            _ => {}
        }
    }
    if !ceiling {
        draw_structure(&mut d, project, geom, level, cut_z, spacing, structural);
    }
    if !structural {
        draw_components(&mut d, project, geom, level, cut_z, ceiling, detail);
    }
    if !structural && !ceiling {
        draw_masses(&mut d, project, geom, level, cut_z);
    }
    draw_shafts(&mut d, project, level, cut_z);

    for (e, _) in project.walls().filter(|(e, _)| e.level == level && shown(e)) {
        if let Some(g) = geom.walls.get(&e.id) {
            draw_wall(&mut d, project, g, cut_z, spacing, detail);
            if !structural {
                draw_openings(&mut d, project, geom, g, ceiling, detail);
            }
        }
    }

    if !ceiling {
        for v in project.views.values() {
            if let ViewKind::Section { start, end, depth } = v.kind {
                section_mark(&mut d, start, end, depth, &v.name);
            }
        }
        if let Some((lo, hi)) = geom.bounds() {
            // Markers stay outside the grid lines too, which usually extend past the model.
            let grid_ends = project.elements.values().filter_map(|e| match &e.kind {
                ElementKind::Grid(g) => Some([g.start, g.end]),
                _ => None,
            });
            let (lo, hi) = grid_ends
                .flatten()
                .fold((DVec2::new(lo.x, lo.y), DVec2::new(hi.x, hi.y)), |(lo, hi), p| (lo.min(p), hi.max(p)));
            for v in project.views.values() {
                if let ViewKind::Elevation { direction } = v.kind {
                    elevation_mark(&mut d, lo, hi, direction.look(), &v.name);
                }
            }
        }
    }
    d
}

/// Distance from the model extents to elevation markers, in meters.
const ELEVATION_MARK_OFFSET: f64 = 1.2;
const ELEVATION_MARK_RADIUS: f64 = 0.25;

/// Elevation marker outside the model: a circle with a filled arrow pointing
/// along `look` (towards the model), labeled with the view name.
fn elevation_mark(d: &mut Drawing2D, lo: DVec2, hi: DVec2, look: DVec2, name: &str) {
    let center = (lo + hi) * 0.5;
    let half = (hi - lo) * 0.5;
    let reach = (half.x * look.x.abs() + half.y * look.y.abs()) + ELEVATION_MARK_OFFSET;
    let c = center - look * reach;
    let r = ELEVATION_MARK_RADIUS;
    let circle: Vec<DVec2> = (0..24)
        .map(|i| {
            let a = i as f64 / 24.0 * std::f64::consts::TAU;
            c + DVec2::new(a.cos(), a.sin()) * r
        })
        .collect();
    d.polyline(&circle, true, LineStyle::Annotation, None);
    let side = look.perp();
    let arrow = vec![c + look * (r * 1.6), c + side * (r * 0.8), c - side * (r * 0.8)];
    d.fill(arrow, [30, 90, 170, 255], None);
    // Label below and to the right of the circle, clear of the arrow.
    d.text(c + DVec2::new(r + 0.08, -r - 0.05), name, 2.5);
}

/// Columns, beams, stairs, ramps and railings of `level` as seen from the cut plane.
/// Outlines of component pieces marked for plans: in floor plans the pieces
/// starting below the cut (pieces above it dashed), in ceiling plans only
/// the pieces above the cut (light fixtures and the like). Swept profiles
/// are drawn the same way.
fn draw_components(
    d: &mut Drawing2D,
    project: &Project,
    geom: &ModelGeometry,
    level: LevelId,
    cut_z: f64,
    ceiling: bool,
    detail: DetailLevel,
) {
    for e in project.elements.values().filter(|e| e.level == level) {
        // Swept profiles (baseboards, crown moldings) cut and show like mesh pieces.
        if let ElementKind::Sweep(sw) = &e.kind {
            if let Some(faces) = cc_geom::profile_sweep::sweep_faces(project, e, sw) {
                draw_mesh(d, &faces, cut_z, ceiling, e.id);
            }
            continue;
        }
        if !matches!(e.kind, ElementKind::Component(_)) {
            continue;
        }
        // Floor plans leave electrical devices out: electrical plans draw their symbols.
        if !ceiling && matches!(project.mep_data(e), Some(cc_core::MepData::Device { .. })) {
            continue;
        }
        for part in geom.components.get(&e.id).into_iter().flatten().filter(|p| p.in_plan && p.detail.shows(detail)) {
            if !part.faces.is_empty() {
                draw_mesh(d, &part.faces, cut_z, ceiling, e.id);
                continue;
            }
            let above = part.z0 >= cut_z;
            let style = match (ceiling, above) {
                (false, false) | (true, true) => LineStyle::Projection,
                (false, true) => LineStyle::Hidden,
                (true, false) => continue,
            };
            d.polyline(&part.footprint, true, style, Some(e.id));
        }
    }
}

/// A mesh piece: its section at the cut plane, the edges seen below it and,
/// dashed, those above (in ceiling plans only the edges above).
/// Conceptual masses in a floor plan: those on its level or cut by its cut
/// plane, as their body cuts and shows (see [`draw_mesh`]), with the mass
/// floor of the level and its area.
fn draw_masses(d: &mut Drawing2D, project: &Project, geom: &ModelGeometry, level: LevelId, cut_z: f64) {
    for (id, g) in &geom.masses {
        let own = project.element(*id).is_some_and(|e| e.level == level);
        if !own && !(g.z_range.0 <= cut_z && cut_z <= g.z_range.1) {
            continue;
        }
        draw_mesh(d, &g.faces, cut_z, false, *id);
        if let Some(floor) = g.floors.iter().find(|f| f.level == level) {
            d.polyline(&floor.outline, true, LineStyle::Thin, Some(*id));
            let n = floor.outline.len().max(1) as f64;
            let center = floor.outline.iter().copied().sum::<DVec2>() / n;
            d.prims.push(crate::Prim::Text {
                pos: center,
                text: crate::format::area(floor.area),
                size: 2.5,
                anchor: crate::Anchor::Center,
                angle: 0.0,
                element: Some(*id),
            });
        }
    }
}

fn draw_mesh(d: &mut Drawing2D, faces: &[Vec<glam::DVec3>], cut_z: f64, ceiling: bool, id: cc_core::ElementId) {
    let plan = cc_geom::mesh_plan(faces, cut_z);
    let id = Some(id);
    if ceiling {
        plan.above.iter().for_each(|&(a, b)| d.line(a, b, LineStyle::Projection, id));
        return;
    }
    plan.cut.iter().for_each(|&(a, b)| d.line(a, b, LineStyle::Cut, id));
    plan.below.iter().for_each(|&(a, b)| d.line(a, b, LineStyle::Projection, id));
    plan.above.iter().for_each(|&(a, b)| d.line(a, b, LineStyle::Hidden, id));
}

fn draw_structure(
    d: &mut Drawing2D,
    project: &Project,
    geom: &ModelGeometry,
    level: LevelId,
    cut_z: f64,
    spacing: f64,
    structural: bool,
) {
    for e in project.elements.values().filter(|e| e.level == level && (!structural || is_structural(e))) {
        let id = Some(e.id);
        let data = project.element_type(e.type_id).map(|t| &t.data);
        match (&e.kind, data) {
            (ElementKind::Column(c), Some(TypeData::Column { section, .. })) => {
                let loops = cc_geom::section::column_loops(*section, c.position, c.rotation_deg);
                let (base, top) = project.column_span(e, c);
                let style = if base <= cut_z && cut_z <= top {
                    let material = project.element_type(e.type_id).map_or("", |t| t.data.cut_material());
                    for part in cc_geom::section::column_parts(*section, c.position, c.rotation_deg) {
                        crate::cut::cut_fill(d, project, material, part, spacing, id);
                    }
                    LineStyle::Cut
                } else if base > cut_z {
                    LineStyle::Hidden
                } else {
                    LineStyle::Projection
                };
                for outline in &loops {
                    d.polyline(outline, true, style, id);
                }
            }
            (ElementKind::Foundation(_), Some(TypeData::Foundation { .. })) => {
                // Foundations lie under the floor: dashed, unless the cut plane passes through them.
                let Some(g) = geom.foundations.get(&e.id) else { continue };
                let (bottom, top) = g.z_range;
                if bottom <= cut_z && cut_z <= top {
                    let material = project.element_type(e.type_id).map_or("", |t| t.data.cut_material());
                    crate::cut::cut_fill(d, project, material, g.footprint.clone(), spacing, id);
                    d.polyline(&g.footprint, true, LineStyle::Cut, id);
                } else {
                    d.polyline(&g.footprint, true, LineStyle::Hidden, id);
                }
            }
            (ElementKind::Beam(_), Some(TypeData::Beam { .. })) => {
                // The body as framed into columns and other beams.
                let Some(g) = geom.beams.get(&e.id) else { continue };
                let (bottom, top) = g.z_bounds();
                // Beams above the cut plane are dashed (in structural plans, seen
                // from below, they are drawn); cut beams are drawn heavy.
                let style = if bottom > cut_z && structural {
                    LineStyle::Projection
                } else if bottom > cut_z {
                    LineStyle::Hidden
                } else if top >= cut_z {
                    LineStyle::Cut
                } else {
                    LineStyle::Projection
                };
                d.polyline(&g.footprint(), true, style, id);
            }
            (ElementKind::Stair(st), Some(TypeData::Stair { .. })) => {
                if let Some(g) = cc_geom::stair_geom(project, e, st) {
                    draw_stair(d, &g, cut_z, e.id);
                }
            }
            (ElementKind::Ramp(r), Some(TypeData::Ramp { .. })) => {
                if let Some(g) = cc_geom::ramp_geom(project, e, r) {
                    draw_ramp(d, &g, cut_z, e.id);
                }
            }
            (ElementKind::Railing(r), Some(TypeData::Railing { .. })) => {
                let path: Vec<DVec2> = cc_geom::railing_path(project, e, r).iter().map(|p| p.truncate()).collect();
                d.polyline(&path, false, LineStyle::Projection, id);
            }
            _ => {}
        }
    }
}

/// Stair symbol: the treads and landings (dashed beyond the cut), a break
/// line across the first step above the cut plane and an arrow along the
/// walking line pointing up, to the cut (or the top).
fn draw_stair(d: &mut Drawing2D, g: &cc_geom::StairGeom, cut_z: f64, element: cc_core::ElementId) {
    let id = Some(element);
    let below = |z: f64| z <= cut_z + 1e-9;
    for st in &g.steps {
        let style = if below(st.z) { LineStyle::Projection } else { LineStyle::Hidden };
        d.polyline(&st.outline, true, style, id);
    }
    if let Some(st) = g.steps.iter().find(|st| !below(st.z)) {
        // Break line: a diagonal with a small zigzag across the step's front.
        let [a, b] = st.front;
        let n = st.outline.len().max(1) as f64;
        let centroid = st.outline.iter().copied().sum::<DVec2>() / n;
        let mut inward = (b - a).perp().normalize_or_zero();
        if inward.dot(centroid - a) < 0.0 {
            inward = -inward;
        }
        let (a, b) = (a - inward * 0.15, b + inward * 0.15);
        let m = (a + b) * 0.5;
        let z = inward * 0.08;
        d.polyline(&[a, m - z, m + z, b], false, LineStyle::Projection, id);
    }
    // Arrow along the walking line, to the cut plane or the top.
    let min_z = g.base + g.riser * 1.5;
    let mut arrow: Vec<DVec2> = Vec::new();
    for w in g.walkline.windows(2) {
        let (a, b) = (w[0], w[1]);
        if arrow.is_empty() {
            arrow.push(a.truncate());
        }
        let limit = cut_z.max(min_z);
        if b.z <= limit + 1e-9 || (b.z - a.z).abs() < 1e-12 {
            arrow.push(b.truncate());
            continue;
        }
        if a.z < limit {
            let t = (limit - a.z) / (b.z - a.z);
            arrow.push(a.truncate().lerp(b.truncate(), t));
        }
        break;
    }
    arrow.dedup_by(|a, b| a.distance(*b) < 1e-6);
    let (Some(&first), [.., before, end]) = (arrow.first(), arrow.as_slice()) else { return };
    let end = *end;
    d.polyline(&arrow, false, LineStyle::Annotation, id);
    let dir = (end - *before).normalize_or(g.u);
    let head = 0.12;
    d.line(end, end - dir * head + dir.perp() * head * 0.6, LineStyle::Annotation, id);
    d.line(end, end - dir * head - dir.perp() * head * 0.6, LineStyle::Annotation, id);
    let start_dir = arrow.get(1).map_or(g.u, |p| (*p - first).normalize_or(g.u));
    let foot = start_dir.perp() * 0.06;
    d.line(first - foot, first + foot, LineStyle::Annotation, id);
}

/// The part of a polygon with an elevation per vertex at or below `cut`
/// (`below`) or above it, with the elevations interpolated along its edges.
fn clip_at(outline: &[DVec2], z: &[f64], cut: f64, below: bool) -> Vec<DVec2> {
    let inside = |v: f64| if below { v <= cut + 1e-9 } else { v > cut + 1e-9 };
    let n = outline.len().min(z.len());
    let mut out = Vec::new();
    for i in 0..n {
        let j = (i + 1) % n;
        let (a, b, za, zb) = (outline[i], outline[j], z[i], z[j]);
        if inside(za) {
            out.push(a);
        }
        if inside(za) != inside(zb) && (zb - za).abs() > 1e-12 {
            out.push(a.lerp(b, (cut - za) / (zb - za)));
        }
    }
    out
}

/// Ramp symbol: the flights and landings (dashed above the cut plane, with a
/// break line where a flight passes it), an arrow along the walking line
/// pointing up and the slope written along the first flight.
fn draw_ramp(d: &mut Drawing2D, g: &cc_geom::RampGeom, cut_z: f64, element: cc_core::ElementId) {
    use crate::drawing::Anchor;
    let id = Some(element);
    // Edges of the parts below and above the cut; those two flight parts
    // share (the facets of a helix, the cut itself) are not drawn.
    let mut edges: Vec<(DVec2, DVec2, LineStyle, bool)> = Vec::new();
    for p in &g.pieces {
        let below = clip_at(&p.outline, &p.z, cut_z, true);
        let above = clip_at(&p.outline, &p.z, cut_z, false);
        for (part, style) in [(&below, LineStyle::Projection), (&above, LineStyle::Hidden)] {
            if part.len() >= 3 {
                let n = part.len();
                edges.extend((0..n).map(|i| (part[i], part[(i + 1) % n], style, !p.landing)));
            }
        }
        if below.len() >= 3 && above.len() >= 3 && !p.landing {
            // The cut runs between the two points the parts share.
            let shared: Vec<DVec2> =
                below.iter().copied().filter(|q| above.iter().any(|r| r.distance(*q) < 1e-9)).collect();
            if let [a, b] = shared.as_slice() {
                let m = (*a + *b) * 0.5;
                let z = (*b - *a).perp().normalize_or_zero() * 0.08;
                d.polyline(&[*a, m - z, m + z, *b], false, LineStyle::Projection, id);
            }
        }
    }
    // Edges keyed by their ends rounded to a micrometer, in either direction.
    let point = |p: DVec2| ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64);
    let key = |a: DVec2, b: DVec2| {
        let (a, b) = (point(a), point(b));
        if a <= b { (a, b) } else { (b, a) }
    };
    let mut flight_edges: std::collections::HashMap<_, usize> = std::collections::HashMap::new();
    for &(a, b, _, flight) in &edges {
        if flight {
            *flight_edges.entry(key(a, b)).or_default() += 1;
        }
    }
    for &(a, b, style, flight) in &edges {
        let shared = flight && flight_edges.get(&key(a, b)).is_some_and(|n| *n > 1);
        if !shared && a.distance(b) > 1e-9 {
            d.line(a, b, style, id);
        }
    }
    // Arrow along the walking line, to the cut plane (or the top).
    let limit = cut_z.max(g.base + 0.05);
    let mut arrow: Vec<DVec2> = Vec::new();
    for w in g.walkline.windows(2) {
        let (a, b) = (w[0], w[1]);
        if arrow.is_empty() {
            arrow.push(a.truncate());
        }
        if b.z <= limit + 1e-9 || (b.z - a.z).abs() < 1e-12 {
            arrow.push(b.truncate());
            continue;
        }
        if a.z < limit {
            arrow.push(a.truncate().lerp(b.truncate(), (limit - a.z) / (b.z - a.z)));
        }
        break;
    }
    arrow.dedup_by(|a, b| a.distance(*b) < 1e-6);
    if let [.., before, end] = arrow.as_slice() {
        let (before, end) = (*before, *end);
        d.polyline(&arrow, false, LineStyle::Annotation, id);
        let dir = (end - before).normalize_or(g.u);
        let head = 0.15;
        d.line(end, end - dir * head + dir.perp() * head * 0.6, LineStyle::Annotation, id);
        d.line(end, end - dir * head - dir.perp() * head * 0.6, LineStyle::Annotation, id);
    }
    // The slope beside the walking line of the first flight, kept upright.
    if let [a, b, ..] = g.walkline.as_slice() {
        let (a, b) = (a.truncate(), b.truncate());
        let dir = (b - a).normalize_or(g.u);
        let mut angle = dir.y.atan2(dir.x);
        if angle > PI / 2.0 + 1e-9 || angle <= -PI / 2.0 {
            angle += if angle > 0.0 { -PI } else { PI };
        }
        let at = a.lerp(b, 0.5) + dir.perp() * (g.width * 0.2);
        d.tag_text(at, crate::format::slope(g.check.slope), 2.5, Anchor::Center, angle, id);
    }
}

/// A wall in plan: cut, its layers with their materials' patterns (at the
/// coarse level one plain region), or seen below or above the cut.
fn draw_wall(d: &mut Drawing2D, project: &Project, g: &WallGeom, cut_z: f64, spacing: f64, detail: DetailLevel) {
    let id = Some(g.element);
    let h = g.half;
    let states: Vec<ColumnCut> = g.columns.iter().map(|c| column_state(c, cut_z)).collect();
    for (i, c) in g.columns.iter().enumerate() {
        let poly = g.column_polygon(c);
        let state = states[i];
        if state == ColumnCut::Cut {
            // Each layer with its material's cut pattern; coarse, one plain region.
            let layers: &[_] = if detail.shows_layers() { &g.layers } else { &[] };
            if layers.is_empty() {
                d.cut_fill(poly.to_vec(), POCHE, id);
            }
            for (v0, v1, material) in layers {
                let at = |v: f64| {
                    let s = (v + h) / (2.0 * h);
                    (c.right.0 + (c.left.0 - c.right.0) * s, c.right.1 + (c.left.1 - c.right.1) * s)
                };
                let ((a0, a1), (b0, b1)) = (at(*v0), at(*v1));
                let mut strip = g.path(a0, a1, *v0);
                strip.extend(g.path(b1, b0, *v1));
                crate::cut::cut_fill(d, project, material, strip, spacing, id);
            }
            // Interior layer boundaries; the outer faces are drawn below.
            for &(_, v, _) in layers.iter().take(layers.len().saturating_sub(1)) {
                let s = (v + h) / (2.0 * h);
                let u0 = c.right.0 + (c.left.0 - c.right.0) * s;
                let u1 = c.right.1 + (c.left.1 - c.right.1) * s;
                d.polyline(&g.path(u0, u1, v), false, LineStyle::Thin, id);
            }
        }
        if let Some(style) = style_of(state) {
            // Faces, opened where other walls meet them in a T.
            for (side, (u0, u1)) in [(-1.0, c.right), (1.0, c.left)] {
                for (a, b) in g.face_segments(side, u0, u1) {
                    d.polyline(&g.path(a, b, side * h), false, style, id);
                }
            }
        }
        if let Some(&next) = states.get(i + 1)
            && next != state
        {
            let style =
                if state == ColumnCut::Cut || next == ColumnCut::Cut { LineStyle::Cut } else { LineStyle::Projection };
            d.line(g.world(c.left.1, -h), g.world(c.left.1, h), style, id);
        }
    }
    let caps = [
        (g.start_joined, g.columns.first(), states.first(), g.start_ext),
        (g.end_joined, g.columns.last(), states.last(), g.end_ext),
    ];
    for (joined, col, state, (ul, ur)) in caps {
        if let (false, Some(_), Some(&state)) = (joined, col, state)
            && let Some(style) = style_of(state)
        {
            d.line(g.world(ur, -h), g.world(ul, h), style, id);
        }
    }
}

fn draw_openings(
    d: &mut Drawing2D,
    project: &Project,
    geom: &ModelGeometry,
    g: &WallGeom,
    ceiling: bool,
    detail: DetailLevel,
) {
    for o in &g.openings {
        let Some(e) = project.element(o.element) else { continue };
        let Some(host) = e.as_hosted() else { continue };
        let id = Some(e.id);
        // A door or window component may draw its own symbol: its pieces marked for plans.
        let own_symbol = match project.element_type(e.type_id).map(|t| &t.data) {
            Some(cc_core::TypeData::Component { def, .. }) => def.opening.as_ref().is_some_and(|o| o.own_symbol),
            _ => false,
        };
        if own_symbol {
            if !ceiling {
                let shown = |p: &&cc_core::component::ComponentPart| p.in_plan && !p.void && p.detail.shows(detail);
                for p in geom.opening_fills.get(&e.id).into_iter().flatten().filter(shown) {
                    d.polyline(&p.footprint, true, LineStyle::Projection, id);
                }
            }
            continue;
        }
        match e.kind {
            ElementKind::Door(_) if !ceiling => {
                let width = o.u1 - o.u0;
                let side = if host.flip_facing { -1.0 } else { 1.0 };
                let (hinge_u, toward) = if host.flip_hand { (o.u1, -1.0) } else { (o.u0, 1.0) };
                let hinge = g.world(hinge_u, side * g.half);
                // Square to the wall at the opening's center (an arc wall's chord).
                let (_, dir, normal) = g.frame_at((o.u0 + o.u1) * 0.5);
                let leaf_dir = normal * side;
                d.line(hinge, hinge + leaf_dir * width, LineStyle::Projection, id);
                let a0 = leaf_dir.to_angle();
                let mut a1 = (dir * toward).to_angle();
                if a1 - a0 > PI {
                    a1 -= TAU;
                } else if a0 - a1 > PI {
                    a1 += TAU;
                }
                d.arc(hinge, width, a0, a1, LineStyle::Thin, id);
            }
            ElementKind::Window(_) => {
                for v in [-0.01, 0.01] {
                    d.polyline(&g.path(o.u0, o.u1, v), false, LineStyle::Thin, id);
                }
            }
            _ => {}
        }
    }
}

fn ceiling_grid(d: &mut Drawing2D, boundary: &[DVec2], id: cc_core::ElementId) {
    let Some((lo, hi)) = boundary
        .iter()
        .fold(None, |acc: Option<(DVec2, DVec2)>, p| Some(acc.map_or((*p, *p), |(lo, hi)| (lo.min(*p), hi.max(*p)))))
    else {
        return;
    };
    for (axis, dir, start, end) in [(0, DVec2::Y, lo.x, hi.x), (1, DVec2::X, lo.y, hi.y)] {
        let mut t = start + CEILING_GRID;
        while t < end - 1e-6 {
            let origin = if axis == 0 { DVec2::new(t, lo.y - 1.0) } else { DVec2::new(lo.x - 1.0, t) };
            for (a, b) in polygon::clip_line(boundary, origin, dir) {
                d.line(origin + dir * a, origin + dir * b, LineStyle::Thin, Some(id));
            }
            t += CEILING_GRID;
        }
    }
}

fn section_mark(d: &mut Drawing2D, start: DVec2, end: DVec2, depth: f64, name: &str) {
    let dir = (end - start).normalize_or_zero();
    let look = -dir.perp();
    d.line(start, end, LineStyle::Annotation, None);
    for p in [start, end] {
        let tip = p + look * 0.6;
        d.line(p, tip, LineStyle::Annotation, None);
        d.line(tip, tip - look * 0.2 + dir * 0.12, LineStyle::Annotation, None);
        d.line(tip, tip - look * 0.2 - dir * 0.12, LineStyle::Annotation, None);
    }
    // Far clip extent, dashed.
    d.line(start + look * depth, end + look * depth, LineStyle::Hidden, None);
    d.text(start - dir * 0.4 + DVec2::new(0.0, 0.1), name, 2.5);
}

/// Shafts in a plan: those on its level and those its cut plane passes
/// through, as their outline crossed by its two diagonals (the usual
/// symbol of an opening through the floors).
fn draw_shafts(d: &mut Drawing2D, project: &Project, level: LevelId, cut_z: f64) {
    for e in project.elements.values() {
        let ElementKind::Shaft(s) = &e.kind else { continue };
        let (base, top) = project.shaft_span(e, s);
        if e.level != level && !(base <= cut_z && cut_z <= top) {
            continue;
        }
        let id = Some(e.id);
        d.polyline(&s.outline, true, LineStyle::Projection, id);
        // Corner to opposite corner: the points farthest apart, then the
        // two farthest from that line on either side.
        let n = s.outline.len();
        let pairs = (0..n).flat_map(|i| (i + 1..n).map(move |j| (i, j)));
        let Some((i, j)) = pairs.max_by(|a, b| {
            let da = s.outline[a.0].distance(s.outline[a.1]);
            let db = s.outline[b.0].distance(s.outline[b.1]);
            da.total_cmp(&db)
        }) else {
            continue;
        };
        let (a, b) = (s.outline[i], s.outline[j]);
        d.line(a, b, LineStyle::Thin, id);
        let side = |p: DVec2| (b - a).perp_dot(p - a);
        let left = s.outline.iter().copied().max_by(|p, q| side(*p).total_cmp(&side(*q)));
        let right = s.outline.iter().copied().min_by(|p, q| side(*p).total_cmp(&side(*q)));
        if let (Some(l), Some(r)) = (left, right)
            && side(l) > 1e-9
            && side(r) < -1e-9
        {
            d.line(l, r, LineStyle::Thin, id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Prim;
    use cc_core::{Category, Document, Element, ElementId, Hosted, Wall};

    #[test]
    fn plan_cuts_wall_and_gaps_at_door() {
        let mut doc = Document::new(Project::from_template());
        let level = doc.project.levels[0].id;
        let (wall_id, door_id) = doc.edit("t", |e| {
            let wall_t = e.project().default_type(Category::Wall).unwrap();
            let door_t = e.project().default_type(Category::Door).unwrap();
            let w = ElementId(e.alloc_id());
            e.add_element(Element {
                id: w,
                level,
                type_id: wall_t,
                mark: String::new(),
                comments: String::new(),
                uid: Default::default(),
                kind: ElementKind::Wall(Wall {
                    start: DVec2::ZERO,
                    end: DVec2::new(5.0, 0.0),
                    base_offset: 0.0,
                    height: 3.0,
                    ..Default::default()
                }),
                phase_created: Default::default(),
                phase_demolished: None,
            });
            let dr = ElementId(e.alloc_id());
            e.add_element(Element {
                id: dr,
                level,
                type_id: door_t,
                mark: "1".into(),
                comments: String::new(),
                uid: Default::default(),
                kind: ElementKind::Door(Hosted {
                    host: w,
                    offset: 2.5,
                    sill_height: 0.0,
                    flip_facing: false,
                    flip_hand: false,
                }),
                phase_created: Default::default(),
                phase_demolished: None,
            });
            (w, dr)
        });
        let geom = ModelGeometry::build(&doc.project);
        let d = plan(&doc.project, &geom, level, 1.2, false, 50);
        let fills =
            d.prims.iter().filter(|p| matches!(p, Prim::Fill { element: Some(id), .. } if *id == wall_id)).count();
        // One fill per layer on each side of the door.
        let layers = geom.walls[&wall_id].layers.len();
        assert_eq!(fills, 2 * layers, "wall cut split by the door");
        // The template's masonry core is hatched (the hatch belongs to the wall).
        let hatched =
            |p: &Prim| matches!(p, Prim::Line { style: LineStyle::Hatch, element: Some(e), .. } if *e == wall_id);
        assert!(d.prims.iter().any(hatched));
        assert_eq!(d.hit_test(DVec2::new(1.0, 0.0), 0.01), Some(wall_id));
        // On the open leaf, drawn from the hinge at u = 2.1 on the left face.
        assert_eq!(d.hit_test(DVec2::new(2.12, 0.5), 0.05), Some(door_id));
    }

    /// A half-circle wall: plans draw its faces as arcs (pickable as arcs),
    /// elevations draw no seams between the facets of its curved faces.
    #[test]
    fn arc_walls_draw_arcs_without_seams() {
        let mut doc = Document::new(Project::from_template());
        let level = doc.project.levels[0].id;
        let wall_id = doc.edit("t", |e| {
            let wall_t = e.project().default_type(Category::Wall).unwrap();
            let w = ElementId(e.alloc_id());
            let wall =
                Wall { start: DVec2::new(4.0, 0.0), end: DVec2::new(-4.0, 0.0), bulge: 1.0, ..Default::default() };
            e.add_element(Element {
                id: w,
                level,
                type_id: wall_t,
                mark: String::new(),
                comments: String::new(),
                uid: Default::default(),
                kind: ElementKind::Wall(wall),
                phase_created: Default::default(),
                phase_demolished: None,
            });
            w
        });
        let geom = ModelGeometry::build(&doc.project);
        let half = geom.walls[&wall_id].half;
        let d = plan(&doc.project, &geom, level, 1.2, false, 50);
        let cut: Vec<(DVec2, DVec2)> = d
            .prims
            .iter()
            .filter_map(|p| match p {
                Prim::Line { a, b, style: LineStyle::Cut, element: Some(e), .. } if *e == wall_id => Some((*a, *b)),
                _ => None,
            })
            .collect();
        // Many short chords, each on the inner or outer circle (the end caps join the two).
        let on_circle = |p: DVec2| (p.length() - (4.0 - half)).abs() < 1e-9 || (p.length() - (4.0 + half)).abs() < 1e-9;
        assert!(cut.len() > 40, "{}", cut.len());
        assert!(cut.iter().all(|(a, b)| on_circle(*a) && on_circle(*b) && a.distance(*b) < 0.25));
        assert_eq!(d.hit_test(DVec2::new(0.0, 4.0 + half), 0.01), Some(wall_id));
        // Seen from the north, the outer face shows only its outline.
        let view = doc
            .project
            .views
            .values()
            .find(|v| matches!(v.kind, cc_core::ViewKind::Elevation { direction: cc_core::Direction::North }))
            .unwrap();
        let d = crate::generate(&doc.project, &geom, view).unwrap();
        let vertical = d
            .prims
            .iter()
            .filter(|p| match p {
                Prim::Line { a, b, element: Some(e), .. } if *e == wall_id => (a.x - b.x).abs() < 1e-9,
                _ => false,
            })
            .count();
        assert!(vertical <= 4, "{vertical} vertical lines");
    }

    /// A mesh piece (as imported from IFC) is cut at the plan's cut plane.
    #[test]
    fn plan_cuts_mesh_pieces() {
        use cc_core::{Component, ComponentDef, ComponentItem, ComponentShape, ElementType, ElementTypeId, TypeData};
        let mut doc = Document::new(Project::from_template());
        let level = doc.project.levels[0].id;
        // A 1 m cube standing on the level, rotated 90° and moved to (5, 0).
        let v = |i: u32| [f64::from(i & 1), f64::from((i >> 1) & 1), f64::from((i >> 2) & 1)];
        let faces = vec![
            vec![0, 2, 3, 1],
            vec![4, 5, 7, 6],
            vec![0, 1, 5, 4],
            vec![1, 3, 7, 5],
            vec![3, 2, 6, 7],
            vec![2, 0, 4, 6],
        ];
        let shape = ComponentShape::Mesh { vertices: (0..8).map(v).collect(), faces };
        let item = ComponentItem {
            name: "m".into(),
            shape,
            material: String::new(),
            in_plan: true,
            void: false,
            plan_only: false,
            detail: Default::default(),
        };
        let id = doc.edit("t", |e| {
            let type_id = ElementTypeId(e.alloc_id());
            let def = ComponentDef { items: vec![item], ..Default::default() };
            let data = TypeData::Component { component: "Mesh".into(), def };
            e.add_type(ElementType {
                id: type_id,
                name: "Mesh".into(),
                data,
                info: Default::default(),
                uid: Default::default(),
            });
            let id = ElementId(e.alloc_id());
            let kind = ElementKind::Component(Component {
                position: DVec2::new(5.0, 0.0),
                rotation_deg: 90.0,
                base_offset: 0.0,
                overrides: Default::default(),
                properties: Vec::new(),
                system: None,
            });
            e.add_element(Element {
                id,
                level,
                type_id,
                mark: String::new(),
                comments: String::new(),
                uid: Default::default(),
                kind,
                phase_created: Default::default(),
                phase_demolished: None,
            });
            id
        });
        let geom = ModelGeometry::build(&doc.project);
        assert!((geom.volume(id) - 1.0).abs() < 1e-9);
        let d = plan(&doc.project, &geom, level, 0.5, false, 50);
        let cut: Vec<_> = d
            .prims
            .iter()
            .filter_map(|p| match p {
                Prim::Line { a, b, style: LineStyle::Cut, element: Some(e), .. } if *e == id => Some((*a, *b)),
                _ => None,
            })
            .collect();
        assert_eq!(cut.len(), 4);
        // Rotated about the insertion point: the square spans x 4..5, y 0..1.
        assert!(cut.iter().all(|(a, b)| {
            [a, b].iter().all(|p| (4.0 - 1e-9..=5.0 + 1e-9).contains(&p.x) && (-1e-9..=1.0 + 1e-9).contains(&p.y))
        }));
    }
}
