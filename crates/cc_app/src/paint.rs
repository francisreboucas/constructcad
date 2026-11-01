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

pub fn fill_polygon(painter: &Painter, pts: Vec<Pos2>, color: Color32) {
    if pts.len() < 3 {
        return;
    }
    if is_convex(&pts) {
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

pub fn paint_drawing(painter: &Painter, d: &Drawing2D, xf: &Xf, opts: &PaintOptions) {
    let clip = painter.clip_rect().expand(50.0);
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
                if let Some(pattern) = dashes {
                    if pattern.len() >= 2 {
                        let (on, off): (Vec<f32>, Vec<f32>) =
                            pattern.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0], c[1])).unzip();
                        painter.extend(Shape::dashed_line_with_offset(&[pa, pb], stroke, &on, &off, 0.0));
                    } else {
                        painter.line_segment([pa, pb], stroke);
                    }
                } else if *style == LineStyle::Hidden {
                    painter.extend(Shape::dashed_line(&[pa, pb], stroke, 6.0, 4.0));
                } else if *style == LineStyle::Center {
                    // Dash-dot.
                    painter.extend(Shape::dashed_line_with_offset(&[pa, pb], stroke, &[14.0, 2.0], &[3.0, 3.0], 0.0));
                } else {
                    painter.line_segment([pa, pb], stroke);
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
                let hl = element.is_some_and(|e| opts.highlight.contains(&e));
                paint_image(painter, image, *faded, corners, hl);
            }
            Prim::Text { pos, text, size, anchor, angle, .. } => {
                if opts.only.is_some() {
                    continue;
                }
                let px = (size * opts.px_per_mm) as f32;
                if px >= 3.5 {
                    paint_text(painter, xf.to_screen(*pos), text, px, *anchor, *angle as f32);
                }
            }
        }
    }
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
        cc_views::Anchor::Center => egui::vec2(-size.x / 2.0, -size.y / 2.0),
    };
    // Screen Y points down, so drawing rotations are negated.
    let rot = egui::emath::Rot2::from_angle(-angle);
    let top_left = pos + rot * local;
    painter.add(egui::epaint::TextShape::new(top_left, galley, Color32::from_gray(20)).with_angle(-angle));
}
