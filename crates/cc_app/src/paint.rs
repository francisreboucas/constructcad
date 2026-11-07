//! Painting [`Drawing2D`]s with egui.

use cc_core::ElementId;
use cc_core::raster::RasterImage;
use cc_views::{Drawing2D, LineStyle, Prim};
use egui::{Color32, Painter, Pos2, Shape, Stroke};
use glam::DVec2;
use std::collections::BTreeSet;

/// Highlight on the drawing paper, white in both themes.
pub const HIGHLIGHT: Color32 = crate::theme::LIGHT.accent;
pub const PREVIEW: Color32 = Color32::from_rgb(230, 110, 20);

/// Maps drawing coordinates to the screen.
#[derive(Clone, Copy)]
pub struct Xf {
    /// Screen position of `origin`.
    pub screen: Pos2,
    pub origin: DVec2,
    /// Screen points per drawing unit.
    pub scale: f64,
}

impl Xf {
    pub fn to_screen(self, p: DVec2) -> Pos2 {
        let d = (p - self.origin) * self.scale;
        Pos2::new(self.screen.x + d.x as f32, self.screen.y - d.y as f32)
    }

    pub fn to_world(self, s: Pos2) -> DVec2 {
        self.origin + DVec2::new((s.x - self.screen.x) as f64, (self.screen.y - s.y) as f64) / self.scale
    }
}

/// Screen points per paper millimeter of pen width (the default pens are
/// drawn at about this ratio).
const PX_PER_PEN_MM: f32 = 4.0;

pub fn stroke_for(style: LineStyle, highlighted: bool) -> Stroke {
    let (width, color) = match style {
        LineStyle::Cut => (2.0, Color32::from_gray(10)),
        LineStyle::Projection => (1.0, Color32::from_gray(45)),
        LineStyle::Thin | LineStyle::Hatch => (0.6, Color32::from_gray(95)),
        LineStyle::Hidden => (0.8, Color32::from_gray(120)),
        LineStyle::Annotation => (0.9, Color32::from_rgb(30, 90, 170)),
        LineStyle::Center => (0.8, Color32::from_rgb(30, 90, 170)),
        LineStyle::Faint => (0.6, Color32::from_gray(175)),
    };
    if highlighted { Stroke::new(width + 0.6, HIGHLIGHT) } else { Stroke::new(width, color) }
}

fn is_convex(pts: &[Pos2]) -> bool {
    let n = pts.len();
    let mut sign = 0.0f32;
    for i in 0..n {
        let (a, b, c) = (pts[i], pts[(i + 1) % n], pts[(i + 2) % n]);
        let cross = (b - a).x * (c - b).y - (b - a).y * (c - b).x;
        if cross.abs() > 1e-3 {
            if sign != 0.0 && cross.signum() != sign {
                return false;
            }
            sign = cross.signum();
        }
    }
    true
}

/// Longest miter (in half feather widths) a feathered corner may take.
const MITER_LIMIT: f32 = 8.0;

/// Whether egui's feathering would push a corner of `pts` far out. It moves
/// each corner along its miter, `1 / sin(angle / 2)` long, with no limit, so a
/// sliver face (a terrain triangle seen edge on) or repeated points painted a
/// long band fading from the face's color (issue #160).
fn has_sharp_corner(pts: &[Pos2]) -> bool {
    let n = pts.len();
    (0..n).any(|i| {
        let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
        let (n0, n1) = ((b - a).normalized().rot90(), (c - b).normalized().rot90());
        let miter = (n0 + n1) / 2.0;
        n0 == egui::Vec2::ZERO || n1 == egui::Vec2::ZERO || miter.length_sq() * MITER_LIMIT * MITER_LIMIT < 1.0
    })
}

pub fn fill_polygon(painter: &Painter, pts: Vec<Pos2>, color: Color32) {
    if pts.len() < 3 {
        return;
    }
    if is_convex(&pts) && !has_sharp_corner(&pts) {
        painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        return;
    }
    let flat: Vec<DVec2> = pts.iter().map(|p| DVec2::new(p.x as f64, p.y as f64)).collect();
    let mut mesh = egui::Mesh::default();
    for p in &pts {
        mesh.colored_vertex(*p, color);
    }
    for [a, b, c] in cc_geom::polygon::triangulate(&flat) {
        mesh.add_triangle(a as u32, b as u32, c as u32);
    }
    painter.add(Shape::mesh(mesh));
}

fn tint(c: [u8; 4], highlighted: bool) -> Color32 {
    if highlighted {
        let mix = |v: u8, h: u8| ((v as u16 + h as u16 * 2) / 3) as u8;
        // Invisible fills (room areas, tag pick areas) become a light tint when selected.
        let alpha = if c[3] < 8 { 60 } else { c[3] };
        Color32::from_rgba_unmultiplied(mix(c[0], 120), mix(c[1], 175), mix(c[2], 255), alpha)
    } else {
        Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
    }
}

pub struct PaintOptions<'a> {
    pub highlight: &'a BTreeSet<ElementId>,
    /// Screen pixels per paper millimeter, for text.
    pub px_per_mm: f64,
    /// Draw everything in a single color (ghost previews).
    pub mono: Option<Color32>,
    /// Only elements in this set (ghost previews).
    pub only: Option<&'a BTreeSet<ElementId>>,
}

/// Vertices the drawings of one frame may add. egui uploads all of a
/// frame's meshes in one vertex buffer, which the GPU caps (256 MB, about
/// 13 million vertices); past it the renderer stops the program, as a large
/// imported drawing seen whole did (issue #156). What does not fit is left
/// out of that frame.
const FRAME_VERTICES: usize = 6_000_000;
/// Of those, what imported drawings (drawn first, under the model) may use,
/// so the model is still drawn over them.
const BACKGROUND_VERTICES: usize = 4_000_000;
/// Vertices of one plain line segment (checked against egui's tessellator in a test).
const LINE_VERTICES: usize = 8;
/// Lines shorter than this on screen (points) draw one per pixel: many
/// tiny segments in one pixel look like one.
const DOT_LENGTH: f32 = 1.0;

/// What a frame's drawings have added so far, shared by the canvases painted in it.
#[derive(Clone, Copy, Default)]
struct FrameUse {
    pass: u64,
    vertices: usize,
}

/// The vertices a drawing still may add, and the pixels its tiny lines took.
struct Budget {
    left: usize,
    background: usize,
    used: usize,
    dots: std::collections::HashSet<(i32, i32)>,
}

impl Budget {
    fn start(ctx: &egui::Context) -> Self {
        let id = egui::Id::new("paint-frame-vertices");
        let pass = ctx.cumulative_pass_nr();
        let used = ctx.data(|d| d.get_temp::<FrameUse>(id)).filter(|u| u.pass == pass).map_or(0, |u| u.vertices);
        let left = FRAME_VERTICES.saturating_sub(used);
        Self { left, background: BACKGROUND_VERTICES.min(left), used: 0, dots: Default::default() }
    }

    /// Take `n` vertices, or refuse (and take none) when they do not fit.
    fn take(&mut self, n: usize, background: bool) -> bool {
        if n > self.left || (background && n > self.background) {
            return false;
        }
        self.left -= n;
        if background {
            self.background -= n;
        }
        self.used += n;
        true
    }

    /// Whether a line this short on screen should be drawn: the first in its pixel.
    fn dot(&mut self, a: Pos2, b: Pos2) -> bool {
        let m = egui::pos2((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        self.dots.insert((m.x.floor() as i32, m.y.floor() as i32))
    }

    fn finish(self, ctx: &egui::Context) {
        let id = egui::Id::new("paint-frame-vertices");
        let pass = ctx.cumulative_pass_nr();
        ctx.data_mut(|d| {
            let u = d.get_temp_mut_or_default::<FrameUse>(id);
            if u.pass != pass {
                *u = FrameUse { pass, vertices: 0 };
            }
            u.vertices = u.vertices.saturating_add(self.used);
        });
    }
}

/// The part of segment `a`–`b` inside `rect`, and how far along the segment it starts.
fn clip_segment(a: Pos2, b: Pos2, rect: egui::Rect) -> Option<(Pos2, Pos2, f32)> {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [(-d.x, a.x - rect.min.x), (d.x, rect.max.x - a.x), (-d.y, a.y - rect.min.y), (d.y, rect.max.y - a.y)]
    {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
        if t0 > t1 {
            return None;
        }
    }
    Some((a + d * t0, a + d * t1, d.length() * t0))
}

/// A dashed line inside `clip`, its dashes continuing the pattern from `a`,
/// with its vertex count (none when the budget has no room for it).
fn dashed(a: Pos2, b: Pos2, stroke: Stroke, on: &[f32], off: &[f32], clip: egui::Rect) -> Option<(Vec<Shape>, usize)> {
    let (pa, pb, start) = clip_segment(a, b, clip)?;
    let period: f32 = on.iter().chain(off).sum();
    if !(period > 0.0 && period.is_finite()) {
        return Some((vec![Shape::line_segment([pa, pb], stroke)], LINE_VERTICES));
    }
    let count = ((pa.distance(pb) / period).ceil() as usize + 1).saturating_mul(on.len()).saturating_mul(LINE_VERTICES);
    Some((Shape::dashed_line_with_offset(&[pa, pb], stroke, on, off, start % period), count))
}

pub fn paint_drawing(painter: &Painter, d: &Drawing2D, xf: &Xf, opts: &PaintOptions) {
    let clip = painter.clip_rect().expand(50.0);
    let mut budget = Budget::start(painter.ctx());
    for prim in &d.prims {
        match prim {
            Prim::Fill { pts, color, element, .. } => {
                if opts.only.is_some_and(|only| !element.is_some_and(|e| only.contains(&e))) {
                    continue;
                }
                let hl = element.is_some_and(|e| opts.highlight.contains(&e));
                let screen: Vec<Pos2> = pts.iter().map(|p| xf.to_screen(*p)).collect();
                if !screen.iter().any(|p| clip.contains(*p)) && !egui::Rect::from_points(&screen).intersects(clip) {
                    continue;
                }
                // Feathered convex polygons take two vertices a corner.
                let background = element.is_some_and(|e| d.background.contains(&e));
                if !budget.take(screen.len().saturating_mul(2), background) {
                    continue;
                }
                let c = match opts.mono {
                    Some(m) => m.gamma_multiply(0.25),
                    None => tint(*color, hl),
                };
                fill_polygon(painter, screen, c);
            }
            Prim::Line { a, b, style, element, pen } => {
                if opts.only.is_some_and(|only| !element.is_some_and(|e| only.contains(&e))) {
                    continue;
                }
                let hl = element.is_some_and(|e| opts.highlight.contains(&e));
                let (pa, pb) = (xf.to_screen(*a), xf.to_screen(*b));
                if !egui::Rect::from_two_pos(pa, pb).intersects(clip) {
                    continue;
                }
                if pa.distance(pb) < DOT_LENGTH && !budget.dot(pa, pb) {
                    continue;
                }
                let background = element.is_some_and(|e| d.background.contains(&e));
                let mut stroke = stroke_for(*style, hl);
                // View graphics overrides: paper millimeters at the screen's pen scale.
                let mut dashes: Option<Vec<f32>> = None;
                if let Some(p) = pen {
                    let r = style.resolve(Some(p));
                    if !hl {
                        stroke.color = Color32::from_rgb(r.color[0], r.color[1], r.color[2]);
                    }
                    if p.weight.is_some() {
                        stroke.width = (r.width as f32 * PX_PER_PEN_MM).clamp(0.4, 8.0) + if hl { 0.6 } else { 0.0 };
                    }
                    if p.pattern.is_some() {
                        dashes = Some(r.dashes.iter().map(|d| (*d as f32 * PX_PER_PEN_MM).max(1.0)).collect());
                    }
                }
                if let Some(m) = opts.mono {
                    stroke.color = m;
                }
                let (on, off): (Vec<f32>, Vec<f32>) = match dashes {
                    Some(pattern) => pattern.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0], c[1])).unzip(),
                    None if *style == LineStyle::Hidden => (vec![6.0], vec![4.0]),
                    // Dash-dot.
                    None if *style == LineStyle::Center => (vec![14.0, 2.0], vec![3.0, 3.0]),
                    None => (Vec::new(), Vec::new()),
                };
                if on.is_empty() {
                    if budget.take(LINE_VERTICES, background) {
                        painter.line_segment([pa, pb], stroke);
                    }
                } else if let Some((shapes, n)) = dashed(pa, pb, stroke, &on, &off, clip)
                    && budget.take(n, background)
                {
                    painter.extend(shapes);
                }
            }
            Prim::Image { origin, u, v, image, element, faded } => {
                if opts.only.is_some() || opts.mono.is_some() {
                    continue;
                }
                let corners = cc_views::Prim::image_corners(*origin, *u, *v).map(|p| xf.to_screen(p));
                if !egui::Rect::from_points(&corners).intersects(clip) {
                    continue;
                }
                let background = element.is_some_and(|e| d.background.contains(&e));
                if !budget.take(4, background) {
                    continue;
                }
                let hl = element.is_some_and(|e| opts.highlight.contains(&e));
                paint_image(painter, image, *faded, corners, hl);
            }
            Prim::Text { pos, text, size, anchor, angle, element, .. } => {
                if opts.only.is_some() {
                    continue;
                }
                let px = (size * opts.px_per_mm) as f32;
                let at = xf.to_screen(*pos);
                // Far outside the view a text cannot reach into it.
                if px < 3.5 || !clip.expand(px * text.chars().count() as f32).contains(at) {
                    continue;
                }
                // Four vertices a glyph.
                let background = element.is_some_and(|e| d.background.contains(&e));
                if budget.take(text.len().saturating_mul(4), background) {
                    paint_text(painter, at, text, px, *anchor, *angle as f32);
                }
            }
        }
    }
    budget.finish(painter.ctx());
}

/// Longest side of a picture's texture, pixels (larger pictures are reduced).
const IMAGE_TEXTURE_SIDE: u32 = 4096;

/// Textures of the pictures shown, by picture (held weakly, so a picture
/// no longer in any drawing drops its texture) and faded or not.
#[derive(Clone, Default)]
struct ImageTextures(Vec<(std::sync::Weak<RasterImage>, bool, Option<egui::TextureHandle>)>);

/// The texture of `image`, decoded the first time it is shown; `None` when
/// it cannot be decoded.
fn image_texture(ctx: &egui::Context, image: &std::sync::Arc<RasterImage>, faded: bool) -> Option<egui::TextureHandle> {
    let id = egui::Id::new("image-textures");
    let mut cache: ImageTextures = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    cache.0.retain(|(w, _, _)| w.strong_count() > 0);
    let found = cache.0.iter().find(|(w, f, _)| *f == faded && std::ptr::eq(w.as_ptr(), std::sync::Arc::as_ptr(image)));
    let texture = match found {
        Some((_, _, t)) => t.clone(),
        None => {
            let side = ctx.input(|i| i.max_texture_side).min(IMAGE_TEXTURE_SIDE as usize) as u32;
            let texture = cc_views::raster::decode(image, side).map(|mut rgba| {
                if faded {
                    cc_views::raster::fade(&mut rgba.pixels);
                }
                let size = [rgba.width as usize, rgba.height as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, &rgba.pixels);
                ctx.load_texture("imported-image", color, egui::TextureOptions::LINEAR)
            });
            cache.0.push((std::sync::Arc::downgrade(image), faded, texture.clone()));
            texture
        }
    };
    ctx.data_mut(|d| d.insert_temp(id, cache));
    texture
}

/// A picture on the parallelogram `corners` (bottom-left first, on screen),
/// tinted and outlined when highlighted; a grey box when it cannot be decoded.
fn paint_image(painter: &Painter, image: &std::sync::Arc<RasterImage>, faded: bool, corners: [Pos2; 4], hl: bool) {
    match image_texture(painter.ctx(), image, faded) {
        Some(texture) => {
            let mut mesh = egui::Mesh::with_texture(texture.id());
            let tint = if hl { Color32::from_rgb(190, 215, 255) } else { Color32::WHITE };
            // Texture rows run from the top: the bottom-left corner is uv (0, 1).
            let uvs = [egui::pos2(0.0, 1.0), egui::pos2(1.0, 1.0), egui::pos2(1.0, 0.0), egui::pos2(0.0, 0.0)];
            for (pos, uv) in corners.iter().zip(uvs) {
                mesh.vertices.push(egui::epaint::Vertex { pos: *pos, uv, color: tint });
            }
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            painter.add(Shape::mesh(mesh));
        }
        None => fill_polygon(painter, corners.to_vec(), Color32::from_gray(225)),
    }
    if hl {
        painter.add(Shape::closed_line(corners.to_vec(), Stroke::new(1.5, HIGHLIGHT)));
    }
}

/// Text anchored at `pos` and rotated by `angle` (radians, counter-clockwise on the drawing).
fn paint_text(painter: &Painter, pos: Pos2, text: &str, px: f32, anchor: cc_views::Anchor, angle: f32) {
    let galley = painter.layout_no_wrap(text.to_string(), egui::FontId::proportional(px), Color32::from_gray(20));
    let size = galley.size();
    let local = match anchor {
        cc_views::Anchor::LeftBottom => egui::vec2(0.0, -size.y),
        cc_views::Anchor::CenterBottom => egui::vec2(-size.x / 2.0, -size.y),
        cc_views::Anchor::RightBottom => egui::vec2(-size.x, -size.y),
        cc_views::Anchor::Center => egui::vec2(-size.x / 2.0, -size.y / 2.0),
    };
    // Screen Y points down, so drawing rotations are negated.
    let rot = egui::emath::Rot2::from_angle(-angle);
    let top_left = pos + rot * local;
    painter.add(egui::epaint::TextShape::new(top_left, galley, Color32::from_gray(20)).with_angle(-angle));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertices(primitives: &[egui::ClippedPrimitive]) -> usize {
        primitives
            .iter()
            .map(|p| match &p.primitive {
                egui::epaint::Primitive::Mesh(m) => m.vertices.len(),
                _ => 0,
            })
            .sum()
    }

    /// Paint `d` in one frame and tessellate it the way the window does.
    fn paint(d: &Drawing2D, scale: f64) -> usize {
        let ctx = egui::Context::default();
        let none = BTreeSet::new();
        let opts = PaintOptions { highlight: &none, px_per_mm: 4.0, mono: None, only: None };
        let xf = Xf { screen: Pos2::new(0.0, 800.0), origin: DVec2::ZERO, scale };
        ctx.begin_pass(egui::RawInput::default());
        paint_drawing(&ctx.layer_painter(egui::LayerId::background()), d, &xf, &opts);
        let mut output = ctx.end_pass();
        output.textures_delta.clear();
        vertices(&ctx.tessellate(output.shapes, output.pixels_per_point))
    }

    #[test]
    fn the_line_estimate_covers_what_egui_makes() {
        for ppp in [1.0, 1.5, 2.0] {
            let mut tess = egui::epaint::Tessellator::new(ppp, Default::default(), [1024, 1024], Vec::new());
            for width in [0.4, 0.6, 1.0, 2.0, 2.6, 8.6] {
                for (a, b) in [((0.0, 0.0), (100.0, 37.0)), ((0.5, 3.0), (0.5, 90.0)), ((0.0, 0.0), (0.3, 0.2))] {
                    let mut mesh = egui::Mesh::default();
                    let shape = Shape::line_segment(
                        [Pos2::new(a.0, a.1), Pos2::new(b.0, b.1)],
                        Stroke::new(width, Color32::BLACK),
                    );
                    tess.tessellate_shape(shape, &mut mesh);
                    assert!(mesh.vertices.len() <= LINE_VERTICES, "{ppp} {width}: {}", mesh.vertices.len());
                }
            }
        }
    }

    #[test]
    fn dashed_lines_are_cut_to_the_view() {
        let clip = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(100.0, 100.0));
        let stroke = Stroke::new(1.0, Color32::BLACK);
        // A line a million points long through a view 100 wide.
        let (shapes, n) = dashed(Pos2::new(-5e5, 50.0), Pos2::new(5e5, 50.0), stroke, &[6.0], &[4.0], clip).unwrap();
        assert!(shapes.len() <= 11 && n <= 12 * LINE_VERTICES, "{} {n}", shapes.len());
        assert!(dashed(Pos2::new(-50.0, 500.0), Pos2::new(50.0, 500.0), stroke, &[6.0], &[4.0], clip).is_none());
        let (a, b, start) = clip_segment(Pos2::new(-10.0, 0.0), Pos2::new(110.0, 0.0), clip).unwrap();
        assert_eq!((a, b, start), (Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0), 10.0));
    }

    /// Issue #160: egui feathered a sliver face along its miters, out of all
    /// proportion, and a shaded section of a graded terrain showed long bands.
    #[test]
    fn sliver_fills_stay_on_their_points() {
        let ctx = egui::Context::default();
        let slivers = [
            vec![Pos2::new(100.0, 100.0), Pos2::new(600.0, 110.0), Pos2::new(100.0, 100.4)],
            vec![Pos2::new(10.0, 10.0), Pos2::new(10.0, 10.0), Pos2::new(50.0, 10.0), Pos2::new(50.0, 40.0)],
        ];
        for pts in slivers {
            ctx.begin_pass(egui::RawInput::default());
            fill_polygon(&ctx.layer_painter(egui::LayerId::background()), pts.clone(), Color32::GRAY);
            let mut output = ctx.end_pass();
            output.textures_delta.clear();
            let bounds = egui::Rect::from_points(&pts).expand(4.0);
            for p in ctx.tessellate(output.shapes, output.pixels_per_point) {
                if let egui::epaint::Primitive::Mesh(m) = p.primitive {
                    assert!(m.vertices.iter().all(|v| bounds.contains(v.pos)), "{:?}", m.vertices);
                }
            }
        }
    }

    /// Issue #156: a large imported drawing seen whole made more vertices
    /// than the GPU buffer holds, and the renderer stopped the program.
    #[test]
    fn large_drawings_stay_within_the_frame_budget() {
        let cad = ElementId(1);
        let wall = ElementId(2);
        let mut d = Drawing2D::default();
        d.background.insert(cad);
        // 600 000 long lines of an imported drawing across the view…
        for i in 0..600_000 {
            let y = (i % 800) as f64;
            d.line(DVec2::new(0.0, y), DVec2::new(1000.0, y + 1.0), LineStyle::Thin, Some(cad));
        }
        // …two million tiny ones in a few pixels…
        for i in 0..2_000_000 {
            let x = (i % 7) as f64 * 0.01;
            d.line(DVec2::new(x, 1.0), DVec2::new(x + 0.001, 1.0), LineStyle::Thin, Some(cad));
        }
        // …and the model over them.
        for i in 0..10 {
            d.line(DVec2::new(0.0, i as f64 * 10.0), DVec2::new(500.0, i as f64 * 10.0), LineStyle::Cut, Some(wall));
        }
        let n = paint(&d, 1.0);
        assert!(n <= FRAME_VERTICES, "{n}");
        assert!(n >= BACKGROUND_VERTICES / 2, "most of the drawing is still drawn: {n}");
        // The model's lines are drawn after the drawing's ran out.
        d.prims.truncate(d.prims.len() - 10);
        assert!(n > paint(&d, 1.0));
    }
}
