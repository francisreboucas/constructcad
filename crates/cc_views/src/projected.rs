//! Orthographic projected views: elevations, sections and axonometrics,
//! with painter's-algorithm hidden surface removal.
//!
//! Faces are painted back to front. The order comes from comparing the depth
//! of every pair of faces that overlap on screen, at a point inside their
//! overlap; sorting by centroid depth alone fails for large faces such as roofs.

use crate::{Drawing2D, LineStyle, cut::POCHE, format};
use cc_core::{CameraState, DetailLevel, Direction, ElementId, Project, TypeData, VisualStyle};
use cc_geom::polygon::signed_area;
use cc_geom::{ModelGeometry, clip_polygon, face_plane_segments};
use glam::{DVec2, DVec3};

/// Orthographic view frame with optional clip planes.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub origin: DVec3,
    pub right: DVec3,
    pub up: DVec3,
    /// View direction (into the scene).
    pub forward: DVec3,
    /// Keep depth in `near..far` (relative to `origin`).
    pub near: Option<f64>,
    pub far: Option<f64>,
    /// Keep screen x in this range.
    pub crop_x: Option<(f64, f64)>,
}

impl Frame {
    pub fn looking(origin: DVec3, forward: DVec3) -> Self {
        let forward = forward.normalize();
        let right = forward.cross(DVec3::Z).try_normalize().unwrap_or(DVec3::X);
        let up = right.cross(forward);
        Self { origin, right, up, forward, near: None, far: None, crop_x: None }
    }

    pub fn project(&self, p: DVec3) -> DVec2 {
        let r = p - self.origin;
        DVec2::new(r.dot(self.right), r.dot(self.up))
    }

    pub fn depth(&self, p: DVec3) -> f64 {
        (p - self.origin).dot(self.forward)
    }

    /// Clip planes as (point, normal) keeping the positive side.
    fn planes(&self) -> Vec<(DVec3, DVec3)> {
        let mut planes = Vec::new();
        if let Some(n) = self.near {
            planes.push((self.origin + self.forward * n, self.forward));
        }
        if let Some(f) = self.far {
            planes.push((self.origin + self.forward * f, -self.forward));
        }
        if let Some((x0, x1)) = self.crop_x {
            planes.push((self.origin + self.right * x0, self.right));
            planes.push((self.origin + self.right * x1, -self.right));
        }
        planes
    }
}

fn clip_segment(a: DVec3, b: DVec3, point: DVec3, normal: DVec3) -> Option<(DVec3, DVec3)> {
    let (da, db) = ((a - point).dot(normal), (b - point).dot(normal));
    match (da >= 0.0, db >= 0.0) {
        (true, true) => Some((a, b)),
        (false, false) => None,
        (true, false) => Some((a, a.lerp(b, da / (da - db)))),
        (false, true) => Some((a.lerp(b, da / (da - db)), b)),
    }
}

fn shade(color: [u8; 3], normal: DVec3, frame: &Frame) -> [u8; 4] {
    let light = (frame.forward * 0.6 + frame.right * 0.3 - frame.up * 0.5).normalize();
    let k = 0.62 + 0.38 * (-normal.dot(light)).max(0.0);
    // Wash colors towards white so linework stays legible.
    let c = |v: u8| ((v as f64 * 0.55 + 255.0 * 0.45) * k).min(255.0) as u8;
    [c(color[0]), c(color[1]), c(color[2]), 255]
}

/// How a view draws the model: its detail level, its visual style and the
/// spacing of patterns (cut and surface hatches) in drawing units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub detail: DetailLevel,
    pub style: VisualStyle,
    pub spacing: f64,
}

impl Look {
    /// The look of `view`.
    pub fn of(view: &cc_core::View) -> Self {
        Self { detail: view.detail, style: view.style(), spacing: crate::cut::spacing(view.scale) }
    }

    /// Shaded, at the medium detail level, patterns as at 1:100.
    pub fn plain() -> Self {
        Self { detail: DetailLevel::Medium, style: VisualStyle::Shaded, spacing: crate::cut::spacing(100) }
    }
}

/// The fill of a face of `solid` in `style` (none for the wireframe).
fn face_color(solid: &cc_geom::Solid, normal: DVec3, frame: &Frame, style: VisualStyle) -> Option<[u8; 4]> {
    let mut c = match style {
        VisualStyle::Wireframe => return None,
        VisualStyle::HiddenLine => [255, 255, 255, 255],
        VisualStyle::Shaded => shade(solid.color, normal, frame),
        VisualStyle::ConsistentColors => [solid.color[0], solid.color[1], solid.color[2], 255],
        VisualStyle::Textures => shade(solid.appearance, normal, frame),
    };
    // Transparent materials let what is behind show through (never fully: alpha 0 means "no hit").
    c[3] = ((1.0 - solid.transparency.clamp(0.0, 1.0)) * 255.0).round().clamp(40.0, 255.0) as u8;
    Some(c)
}

/// The surface pattern of `solid` over a face outline `pts` (the textures
/// style), each line with the pattern's color.
fn surface_lines(d: &mut Drawing2D, solid: &cc_geom::Solid, pts: &[DVec2], spacing: f64) {
    let fill = solid.surface;
    for (pattern, color) in [(fill.background, fill.background_color), (fill.foreground, fill.foreground_color)] {
        if matches!(pattern, cc_core::Pattern::Solid | cc_core::Pattern::None) {
            continue;
        }
        let pen = crate::Pen { color: Some(color), ..Default::default() };
        for (a, b) in crate::hatch::hatch(pattern, pts, spacing) {
            d.prims.push(crate::Prim::Line {
                a,
                b,
                style: LineStyle::Thin,
                element: Some(solid.element),
                pen: Some(pen),
            });
        }
    }
}

struct ProjectedFace {
    /// Mean depth of the vertices, used to order unrelated faces and break cycles.
    depth: f64,
    /// Plane of the face in 3D.
    normal: DVec3,
    point: DVec3,
    pts: Vec<DVec2>,
    color: Option<[u8; 4]>,
    element: ElementId,
    /// Index of its solid in the geometry (its surface pattern).
    solid: usize,
    edges: Vec<(DVec2, DVec2)>,
}

/// How section cuts are filled: the project's materials at a hatch spacing.
#[derive(Clone, Copy)]
pub struct CutStyle<'a> {
    pub project: &'a Project,
    pub spacing: f64,
}

/// Render the solids a view shows through `frame`, at its detail level and
/// in its visual style; section cuts use `cut` (else, and at the coarse
/// level, the neutral poche; in the wireframe, only their outline).
pub fn render(geom: &ModelGeometry, frame: &Frame, cut: Option<CutStyle>, look: Look) -> Drawing2D {
    let (detail, style) = (look.detail, look.style);
    let planes = frame.planes();
    let mut faces = Vec::new();
    let shown = |s: &&cc_geom::Solid| s.detail.shows(detail);
    let cut = cut.filter(|_| detail.shows_layers());
    for (index, solid) in geom.solids.iter().enumerate().filter(|(_, s)| shown(s)) {
        let edges = solid.feature_edges();
        let seams = solid.smooth_edges();
        for ((face, mut face_edges), seams) in solid.faces.iter().zip(edges).zip(seams) {
            // The wireframe sees through: faces turned away show their edges too.
            if face.normal.dot(frame.forward) > -1e-6 && style.fills() {
                continue;
            }
            // Where a curved surface turns away, the edge to the hidden facet is its silhouette.
            face_edges.extend(seams.into_iter().filter(|s| s.2.dot(frame.forward) > -1e-6).map(|(a, b, _)| (a, b)));
            let mut verts = face.verts.clone();
            for &(p, n) in &planes {
                verts = clip_polygon(&verts, p, n);
            }
            if verts.len() < 3 {
                continue;
            }
            let edges = face_edges
                .into_iter()
                .filter_map(|(a, b)| planes.iter().try_fold((a, b), |(a, b), &(p, n)| clip_segment(a, b, p, n)))
                .map(|(a, b)| (frame.project(a), frame.project(b)))
                .collect();
            let depth = verts.iter().map(|v| frame.depth(*v)).sum::<f64>() / verts.len() as f64;
            faces.push(ProjectedFace {
                depth,
                normal: face.normal,
                point: verts[0],
                pts: verts.iter().map(|v| frame.project(*v)).collect(),
                color: face_color(solid, face.normal, frame, style),
                element: solid.element,
                solid: index,
                edges,
            });
        }
    }
    let mut d = Drawing2D::default();
    if style.fills() {
        let order = paint_order(frame, &faces);
        let mut slots: Vec<Option<ProjectedFace>> = faces.into_iter().map(Some).collect();
        for f in order.into_iter().filter_map(|i| slots[i].take()) {
            if let Some(color) = f.color {
                if style == VisualStyle::Textures
                    && let Some(solid) = geom.solids.get(f.solid)
                {
                    d.fill(f.pts.clone(), color, Some(f.element));
                    surface_lines(&mut d, solid, &f.pts, look.spacing);
                } else {
                    d.fill(f.pts, color, Some(f.element));
                }
            }
            for (a, b) in f.edges {
                d.line(a, b, LineStyle::Projection, Some(f.element));
            }
        }
    } else {
        // Every edge, in no particular order: nothing hides anything.
        for f in faces {
            for (a, b) in f.edges {
                d.line(a, b, LineStyle::Projection, Some(f.element));
            }
        }
    }

    // Section cut: poche where loops close, heavy outline always.
    if let Some(near) = frame.near {
        let point = frame.origin + frame.forward * near;
        for solid in geom.solids.iter().filter(shown) {
            let segs: Vec<(DVec2, DVec2)> = solid
                .faces
                .iter()
                .flat_map(|f| face_plane_segments(f, point, frame.forward))
                .filter_map(|(a, b)| planes.iter().skip(1).try_fold((a, b), |(a, b), &(p, n)| clip_segment(a, b, p, n)))
                .map(|(a, b)| (frame.project(a), frame.project(b)))
                .collect();
            for pts in chain_loops(&segs).into_iter().filter(|_| style.fills()) {
                match cut {
                    Some(c) => {
                        let ty = c.project.element(solid.element).and_then(|e| c.project.element_type(e.type_id));
                        let material = ty.map_or("", |t| match &t.data {
                            TypeData::Roof { framing, .. } if solid.secondary => framing.material.as_str(),
                            data => data.cut_material(),
                        });
                        crate::cut::cut_fill(&mut d, c.project, material, pts, c.spacing, Some(solid.element));
                    }
                    None => d.cut_fill(pts, POCHE, Some(solid.element)),
                }
            }
            for (a, b) in segs {
                d.line(a, b, LineStyle::Cut, Some(solid.element));
            }
        }
    }
    d
}

/// Depth (along the view direction) of the plane of `f` under screen point `s`.
fn depth_at(frame: &Frame, f: &ProjectedFace, s: DVec2) -> f64 {
    let base = frame.origin + frame.right * s.x + frame.up * s.y;
    f.normal.dot(f.point - base) / f.normal.dot(frame.forward)
}

/// Clip a convex polygon by a counter-clockwise convex polygon.
fn clip_convex(subject: &[DVec2], clipper: &[DVec2]) -> Vec<DVec2> {
    let mut out = subject.to_vec();
    for i in 0..clipper.len() {
        if out.is_empty() {
            break;
        }
        let (a, b) = (clipper[i], clipper[(i + 1) % clipper.len()]);
        let side = |p: DVec2| (b - a).perp_dot(p - a);
        let input = std::mem::take(&mut out);
        for j in 0..input.len() {
            let (p, q) = (input[j], input[(j + 1) % input.len()]);
            let (sp, sq) = (side(p), side(q));
            if sp >= 0.0 {
                out.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                out.push(p + (q - p) * (sp / (sp - sq)));
            }
        }
    }
    out
}

/// Counter-clockwise triangles of a projected face.
fn triangles(pts: &[DVec2]) -> Vec<[DVec2; 3]> {
    cc_geom::polygon::triangulate(pts)
        .into_iter()
        .map(|[a, b, c]| {
            let t = [pts[a], pts[b], pts[c]];
            if signed_area(&t) < 0.0 { [t[0], t[2], t[1]] } else { t }
        })
        .collect()
}

/// A point strictly inside the overlap of two faces on screen: the centroid of
/// the largest overlap between their triangles.
fn overlap_point(a: &[[DVec2; 3]], b: &[[DVec2; 3]]) -> Option<DVec2> {
    const MIN_AREA: f64 = 1e-8;
    let mut best: Option<(f64, DVec2)> = None;
    for ta in a {
        for tb in b {
            let clipped = clip_convex(ta, tb);
            if clipped.len() < 3 {
                continue;
            }
            let area = signed_area(&clipped).abs();
            if area > MIN_AREA && best.is_none_or(|(a, _)| area > a) {
                let centroid = clipped.iter().copied().sum::<DVec2>() / clipped.len() as f64;
                best = Some((area, centroid));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Back-to-front painting order: a face is painted after every face it hides.
fn paint_order(frame: &Frame, faces: &[ProjectedFace]) -> Vec<usize> {
    const DEPTH_TOL: f64 = 1e-6;
    let n = faces.len();
    let boxes: Vec<(DVec2, DVec2)> =
        faces.iter().map(|f| f.pts.iter().fold((f.pts[0], f.pts[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)))).collect();
    let tris: Vec<Vec<[DVec2; 3]>> = faces.iter().map(|f| triangles(&f.pts)).collect();
    // `before[i]` lists the faces that must be painted after face `i`.
    let mut before: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut pending = vec![0usize; n];
    for i in 0..n {
        for j in i + 1..n {
            let (a, b) = (boxes[i], boxes[j]);
            if a.0.x >= b.1.x || b.0.x >= a.1.x || a.0.y >= b.1.y || b.0.y >= a.1.y {
                continue;
            }
            let Some(s) = overlap_point(&tris[i], &tris[j]) else { continue };
            let (di, dj) = (depth_at(frame, &faces[i], s), depth_at(frame, &faces[j], s));
            if (di - dj).abs() < DEPTH_TOL {
                continue;
            }
            let (back, front) = if di > dj { (i, j) } else { (j, i) };
            before[back].push(front);
            pending[front] += 1;
        }
    }
    // Kahn's algorithm, taking the farthest ready face first. On a cycle the
    // face waiting on the fewest others is forced (the farthest among them):
    // forcing the farthest instead lets a large face, such as a roof with
    // many members under it, be painted before most of what it hides.
    let mut done = vec![false; n];
    let mut order = Vec::with_capacity(n);
    for _ in 0..n {
        let ready =
            (0..n).filter(|&k| !done[k] && pending[k] == 0).max_by(|&x, &y| faces[x].depth.total_cmp(&faces[y].depth));
        let forced = || {
            (0..n)
                .filter(|&k| !done[k])
                .min_by(|&x, &y| pending[x].cmp(&pending[y]).then(faces[y].depth.total_cmp(&faces[x].depth)))
        };
        let Some(k) = ready.or_else(forced) else { break };
        done[k] = true;
        order.push(k);
        for &f in &before[k] {
            pending[f] = pending[f].saturating_sub(1);
        }
    }
    order
}

/// Join segments end-to-end into closed loops.
fn chain_loops(segs: &[(DVec2, DVec2)]) -> Vec<Vec<DVec2>> {
    const TOL: f64 = 1e-6;
    let mut used = vec![false; segs.len()];
    let mut loops = Vec::new();
    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut pts = vec![segs[start].0, segs[start].1];
        while let Some(&tail) = pts.last() {
            if pts.len() > 2 && tail.distance(pts[0]) < TOL {
                pts.pop();
                loops.push(pts);
                break;
            }
            let next = (0..segs.len())
                .find(|&i| !used[i] && (segs[i].0.distance(tail) < TOL || segs[i].1.distance(tail) < TOL));
            let Some(i) = next else { break };
            used[i] = true;
            let (a, b) = segs[i];
            pts.push(if a.distance(tail) < TOL { b } else { a });
        }
    }
    loops
}

/// Level lines across the drawing; before anything is modelled they span
/// the grids, or a few meters from the origin.
fn level_lines(d: &mut Drawing2D, project: &Project, frame: &Frame) {
    let grid_xs: Vec<f64> = project
        .elements
        .values()
        .filter_map(|e| match &e.kind {
            cc_core::ElementKind::Grid(g) => Some([g.start, g.end]),
            _ => None,
        })
        .flatten()
        .map(|p| frame.project(DVec3::new(p.x, p.y, 0.0)).x)
        .collect();
    let span = match d.bounds() {
        Some((lo, hi)) => (lo.x, hi.x),
        None if !grid_xs.is_empty() => grid_xs.iter().fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(*x), b.max(*x))),
        None => (0.0, 10.0),
    };
    let (x0, x1) = (span.0 - 1.5, span.1 + 1.5);
    for level in &project.levels {
        let y = frame.project(DVec3::new(0.0, 0.0, level.elevation)).y;
        d.line(DVec2::new(x0, y), DVec2::new(x1, y), LineStyle::Annotation, None);
        // Elevation target symbol: a small triangle on the line.
        d.line(DVec2::new(x1, y), DVec2::new(x1 + 0.15, y + 0.2), LineStyle::Annotation, None);
        d.line(DVec2::new(x1 + 0.15, y + 0.2), DVec2::new(x1 - 0.15, y + 0.2), LineStyle::Annotation, None);
        d.line(DVec2::new(x1 - 0.15, y + 0.2), DVec2::new(x1, y), LineStyle::Annotation, None);
        d.text(DVec2::new(x1 + 0.3, y + 0.12), &level.name, 2.5);
        d.text(DVec2::new(x1 + 0.3, y - 0.18), format::elevation(level.elevation), 2.0);
    }
}

pub fn elevation(project: &Project, geom: &ModelGeometry, direction: Direction, look: Look) -> Drawing2D {
    let toward = direction.look();
    let frame = Frame::looking(DVec3::ZERO, DVec3::new(toward.x, toward.y, 0.0));
    let mut d = render(geom, &frame, None, look);
    level_lines(&mut d, project, &frame);
    d
}

pub fn section(
    project: &Project,
    geom: &ModelGeometry,
    (start, end): (DVec2, DVec2),
    depth: f64,
    scale: u32,
    look: Look,
) -> Drawing2D {
    let dir = (end - start).normalize_or_zero();
    let toward = -dir.perp();
    let mut frame = Frame::looking(DVec3::new(start.x, start.y, 0.0), DVec3::new(toward.x, toward.y, 0.0));
    let xe = frame.project(DVec3::new(end.x, end.y, 0.0)).x;
    frame.near = Some(0.0);
    frame.far = Some(depth.max(0.01));
    frame.crop_x = Some((xe.min(0.0), xe.max(0.0)));
    let cut = CutStyle { project, spacing: crate::cut::spacing(scale) };
    let mut d = render(geom, &frame, Some(cut), look);
    level_lines(&mut d, project, &frame);
    d
}

pub fn axonometric(_project: &Project, geom: &ModelGeometry, camera: &CameraState, look: Look) -> Drawing2D {
    render(geom, &Frame::looking(DVec3::ZERO, camera.forward()), None, look)
}

/// The analytical model seen along the camera: member lines (columns and
/// beams) and nodes, supports marked with a triangle.
pub fn analytical(project: &Project, geom: &ModelGeometry, camera: &CameraState) -> Drawing2D {
    let frame = Frame::looking(DVec3::ZERO, camera.forward());
    let model = cc_geom::analytical::analytical_model(project, geom);
    let mut d = Drawing2D::default();
    for m in &model.members {
        if let Some((a, b)) = model.line(m) {
            let style = match m.kind {
                cc_geom::analytical::MemberKind::Column => crate::LineStyle::Cut,
                cc_geom::analytical::MemberKind::Beam => crate::LineStyle::Projection,
            };
            d.line(frame.project(a), frame.project(b), style, Some(m.element));
        }
    }
    for n in &model.nodes {
        let p = frame.project(n.position);
        let r = 0.08;
        let marker: Vec<DVec2> = if n.support {
            vec![p, p + DVec2::new(r, -1.6 * r), p + DVec2::new(-r, -1.6 * r)]
        } else {
            (0..8).map(|i| p + DVec2::from_angle(i as f64 * std::f64::consts::FRAC_PI_4) * (r * 0.5)).collect()
        };
        d.polyline(&marker, true, crate::LineStyle::Thin, None);
    }
    d
}

/// The reinforcing bars seen along the camera, tagged with their sets.
pub fn rebar(geom: &ModelGeometry, camera: &CameraState) -> Drawing2D {
    let frame = Frame::looking(DVec3::ZERO, camera.forward());
    let mut d = Drawing2D::default();
    for set in geom.rebar.values() {
        for bar in &set.bars {
            let pts: Vec<DVec2> = bar.points.iter().map(|p| frame.project(*p)).collect();
            d.polyline(&pts, bar.closed, crate::LineStyle::Projection, Some(set.element));
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Prim;
    use cc_core::{Category, Document, Element, ElementKind, Wall};

    fn one_wall() -> (Document, ElementId) {
        let mut doc = Document::new(Project::from_template());
        let id = doc.edit("t", |e| {
            let level = e.project().levels[0].id;
            let type_id = e.project().default_type(Category::Wall).unwrap();
            let id = ElementId(e.alloc_id());
            e.add_element(Element {
                id,
                level,
                type_id,
                mark: String::new(),
                comments: String::new(),
                uid: Default::default(),
                kind: ElementKind::Wall(Wall {
                    start: DVec2::ZERO,
                    end: DVec2::new(4.0, 0.0),
                    base_offset: 0.0,
                    height: 3.0,
                    ..Default::default()
                }),
                phase_created: Default::default(),
                phase_demolished: None,
            })
        });
        (doc, id)
    }

    /// A new project shows its levels in elevations before anything is modelled.
    #[test]
    fn empty_elevation_shows_the_levels() {
        let project = Project::from_template();
        let geom = ModelGeometry::build(&project);
        let d = elevation(&project, &geom, Direction::South, Look::plain());
        for level in &project.levels {
            assert!(
                d.prims.iter().any(|p| matches!(p, Prim::Text { text, .. } if *text == level.name)),
                "{}",
                level.name
            );
        }
    }
    /// Each visual style draws the same wall differently: the wireframe
    /// only lines (the far face's edges too), hidden line white faces,
    /// consistent colors the material's color unlit, textures the
    /// material's appearance with its surface pattern in the pattern's color.
    #[test]
    fn visual_styles_fill_faces_their_own_way() {
        use cc_core::VisualStyle as V;
        let (mut doc, id) = one_wall();
        let material = doc
            .project
            .element(id)
            .and_then(|e| doc.project.element_type(e.type_id))
            .map(|t| t.main_material().to_string())
            .unwrap();
        let m = doc.project.materials.iter_mut().find(|m| m.name == material).unwrap();
        m.appearance.color = [180, 60, 40];
        m.graphics.surface.foreground = cc_core::Pattern::Brick;
        m.graphics.surface.foreground_color = [90, 20, 10];
        let geom = ModelGeometry::build(&doc.project);
        let draw = |style| elevation(&doc.project, &geom, Direction::South, Look { style, ..Look::plain() });
        let fills = |d: &Drawing2D| -> Vec<[u8; 4]> {
            d.prims
                .iter()
                .filter_map(|p| match p {
                    Prim::Fill { color, cut: false, .. } => Some(*color),
                    _ => None,
                })
                .collect()
        };
        let lines = |d: &Drawing2D| d.prims.iter().filter(|p| matches!(p, Prim::Line { .. })).count();
        let (wire, hidden) = (draw(V::Wireframe), draw(V::HiddenLine));
        assert!(fills(&wire).is_empty());
        assert!(lines(&wire) > lines(&hidden), "the far face's edges show through");
        assert!(!fills(&hidden).is_empty() && fills(&hidden).iter().all(|c| c[..3] == [255, 255, 255]));
        let color = doc.project.material_color(&material);
        let consistent = fills(&draw(V::ConsistentColors));
        assert!(consistent.iter().all(|c| c[..3] == color), "{consistent:?} vs {color:?}");
        let shaded = fills(&draw(V::Shaded));
        assert!(shaded.iter().all(|c| c[..3] != color), "shading washes the color");
        let textures = draw(V::Textures);
        let bricks = textures
            .prims
            .iter()
            .filter(|p| matches!(p, Prim::Line { pen: Some(pen), .. } if pen.color == Some([90, 20, 10])))
            .count();
        assert!(bricks > 10, "{bricks} brick lines");
        assert!(fills(&textures).iter().all(|c| c[0] > c[1]), "the reddish appearance color");
        // The wireframe is still picked by its lines.
        assert_eq!(wire.hit_test(DVec2::new(0.0, 1.5), 0.05), Some(id));
    }

    #[test]
    fn south_elevation_shows_wall_face() {
        let (doc, id) = one_wall();
        let geom = ModelGeometry::build(&doc.project);
        let d = elevation(&doc.project, &geom, Direction::South, Look::plain());
        assert_eq!(d.hit_test(DVec2::new(2.0, 1.5), 0.0), Some(id));
        assert_eq!(d.hit_test(DVec2::new(2.0, 3.5), 0.0), None);
    }

    /// A roof slab overhanging a wall, seen from above: every wall face lies
    /// under the roof, so the roof's top face must be painted after all of them.
    /// Sorting faces by their centroid depth alone paints the near end of the
    /// wall over the roof.
    #[test]
    fn large_faces_hide_what_is_behind_them() {
        use cc_geom::Solid;
        let (roof_id, wall_id) = (ElementId(1), ElementId(2));
        let mut roof = Solid::new(roof_id, Category::Roof, [200, 80, 60]);
        roof.add_box(DVec2::ZERO, DVec2::X, DVec2::Y, 0.0, 10.0, -2.0, 12.0, 3.0, 3.2);
        let mut wall = Solid::new(wall_id, Category::Wall, [200, 200, 200]);
        wall.add_box(DVec2::ZERO, DVec2::X, DVec2::Y, 4.0, 4.2, 0.0, 10.0, 0.0, 3.0);
        let geom = ModelGeometry { solids: vec![roof, wall], ..Default::default() };
        let frame = Frame::looking(DVec3::ZERO, DVec3::new(1.0, 1.0, -1.0));
        let d = render(&geom, &frame, None, Look::plain());
        let fills: Vec<(usize, ElementId, f64)> = d
            .prims
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match p {
                Prim::Fill { pts, element: Some(e), .. } => Some((i, *e, cc_geom::polygon::area(pts))),
                _ => None,
            })
            .collect();
        let roof_top = fills.iter().filter(|f| f.1 == roof_id).max_by(|a, b| a.2.total_cmp(&b.2)).unwrap().0;
        let last_wall = fills.iter().filter(|f| f.1 == wall_id).map(|f| f.0).max().unwrap();
        assert!(roof_top > last_wall, "roof top painted at {roof_top}, a wall face at {last_wall}");
    }

    /// Three members lying over one another in a ring (a cycle) under a
    /// large tilted face whose mean depth is the farthest: breaking the cycle
    /// must not paint the large face before the members it hides.
    #[test]
    fn cycles_do_not_paint_a_large_face_early() {
        let frame = Frame::looking(DVec3::new(0.0, 0.0, 10.0), -DVec3::Z);
        let face = |verts: Vec<DVec3>| {
            let normal = (verts[1] - verts[0]).cross(verts[2] - verts[0]).normalize();
            ProjectedFace {
                depth: verts.iter().map(|v| frame.depth(*v)).sum::<f64>() / verts.len() as f64,
                normal,
                point: verts[0],
                pts: verts.iter().map(|v| frame.project(*v)).collect(),
                color: Some([0; 4]),
                element: ElementId(1),
                solid: 0,
                edges: Vec::new(),
            }
        };
        // The large face rises along x: 4.25 to 5.75 m high over the members.
        let z = |x: f64| 1.0 + 0.5 * x;
        let big = face(vec![
            DVec3::new(-10.0, -10.0, z(-10.0)),
            DVec3::new(10.0, -10.0, z(10.0)),
            DVec3::new(10.0, 10.0, z(10.0)),
            DVec3::new(-10.0, 10.0, z(-10.0)),
        ]);
        // Strips along the sides of a triangle around (8, 0), each rising from
        // 3 to 3.5 m in turn, so each lies over the next where they cross.
        let center = DVec2::new(8.0, 0.0);
        let corner = |k: usize| {
            let a = std::f64::consts::TAU * k as f64 / 3.0;
            center + DVec2::new(a.cos(), a.sin()) * 1.5
        };
        let mut faces = vec![big];
        for k in 0..3 {
            let (p, q) = (corner(k), corner(k + 1));
            let d = (q - p).normalize();
            let (a, b) = (p - d * 0.5, q + d * 0.5);
            let side = d.perp() * 0.15;
            let at = |p: DVec2, h: f64| DVec3::new(p.x, p.y, h);
            faces.push(face(vec![at(a - side, 3.0), at(b - side, 3.5), at(b + side, 3.5), at(a + side, 3.0)]));
        }
        let order = paint_order(&frame, &faces);
        assert_eq!(order.len(), 4);
        assert_eq!(order.last(), Some(&0), "the large face hides the members: painted last, {order:?}");
    }

    #[test]
    fn section_cuts_wall_with_poche() {
        let (doc, id) = one_wall();
        let geom = ModelGeometry::build(&doc.project);
        // Section line along X at x = 2, looking -Y... crossing the wall north to south.
        let d = section(&doc.project, &geom, (DVec2::new(2.0, 2.0), DVec2::new(2.0, -2.0)), 5.0, 50, Look::plain());
        let cut = d.prims.iter().filter(|p| matches!(p, Prim::Line { style: LineStyle::Cut, .. })).count();
        assert_eq!(cut, 4);
        // The cut is filled with the core material's pattern (the template's masonry hatch).
        let fills = d.prims.iter().filter(|p| matches!(p, Prim::Fill { element: Some(e), .. } if *e == id)).count();
        assert!(fills >= 1);
        let hatch = d
            .prims
            .iter()
            .filter(|p| matches!(p, Prim::Line { style: LineStyle::Hatch, element: Some(e), .. } if *e == id))
            .count();
        assert!(hatch > 0);
        assert!(d.elements_in(DVec2::splat(-100.0), DVec2::splat(100.0), false).contains(&id));
    }
}
