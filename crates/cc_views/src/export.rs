//! Vector export of drawings in paper millimeters (composed sheets) to SVG
//! and PDF. Both writers are self-contained: the PDF uses the standard
//! Helvetica font, so no font is embedded.

use crate::{Anchor, Drawing2D, Prim};
use glam::DVec2;
use std::fmt::Write as _;

const TEXT_GRAY: u8 = 20;

/// Baseline offset (in text heights) from the anchor point, and the
/// horizontal fraction of the text width left of it.
fn anchor_offsets(anchor: Anchor) -> (f64, f64) {
    match anchor {
        Anchor::LeftBottom => (0.28, 0.0),
        Anchor::CenterBottom => (0.28, 0.5),
        Anchor::Center => (-0.35, 0.5),
    }
}

fn num(v: f64) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".to_string() } else { s.to_string() }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

/// SVG document of `drawing` on a sheet of `paper` millimeters.
pub fn to_svg(drawing: &Drawing2D, paper: DVec2) -> String {
    svg(drawing, paper, &[])
}

/// The SVG; with `fonts`, texts become filled outlines of their glyphs.
fn svg(drawing: &Drawing2D, paper: DVec2, fonts: &[skrifa::FontRef]) -> String {
    let (w, h) = (paper.x, paper.y);
    let y = |v: f64| num(h - v);
    let mut s = String::new();
    let _ = writeln!(s, r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    let _ = writeln!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{0}mm" height="{1}mm" viewBox="0 0 {0} {1}">"#,
        num(w),
        num(h)
    );
    let _ = writeln!(s, r#"<rect width="{}" height="{}" fill="white"/>"#, num(w), num(h));
    let _ = writeln!(s, r#"<g stroke-linecap="round" fill="none">"#);
    for prim in &drawing.prims {
        match prim {
            Prim::Line { a, b, style, pen, .. } => {
                let resolved = style.resolve(pen.as_ref());
                let (width, [r, g, bl]) = (resolved.width, resolved.color);
                let dash = resolved.dashes;
                let dash = if dash.is_empty() {
                    String::new()
                } else {
                    let d: Vec<String> = dash.iter().map(|v| num(*v)).collect();
                    format!(r#" stroke-dasharray="{}""#, d.join(" "))
                };
                let _ = writeln!(
                    s,
                    r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="rgb({r},{g},{bl})" stroke-width="{}"{dash}/>"#,
                    num(a.x),
                    y(a.y),
                    num(b.x),
                    y(b.y),
                    num(width)
                );
            }
            Prim::Fill { pts, color, .. } => {
                let [r, g, b, a] = *color;
                if a == 0 || pts.len() < 3 {
                    continue;
                }
                let points: Vec<String> = pts.iter().map(|p| format!("{},{}", num(p.x), y(p.y))).collect();
                let opacity =
                    if a == 255 { String::new() } else { format!(r#" fill-opacity="{}""#, num(a as f64 / 255.0)) };
                let _ = writeln!(
                    s,
                    r#"<polygon points="{}" fill="rgb({r},{g},{b})"{opacity} stroke="none"/>"#,
                    points.join(" ")
                );
            }
            Prim::Text { pos, text, size, anchor, angle, .. } => {
                if text.trim().is_empty() {
                    continue;
                }
                if !fonts.is_empty() {
                    let d = glyph_path(fonts, text, *pos, *size, *anchor, *angle, h);
                    if !d.is_empty() {
                        let _ = writeln!(s, r#"<path d="{d}" fill="rgb({g},{g},{g})" stroke="none"/>"#, g = TEXT_GRAY);
                    }
                    continue;
                }
                let (base, frac) = anchor_offsets(*anchor);
                let text_anchor = if frac == 0.0 { "start" } else { "middle" };
                // Offset the baseline in the text's own (rotated) frame.
                let up = DVec2::new(-angle.sin(), angle.cos());
                let p = *pos - up * (base * size);
                let rotate = if angle.abs() > 1e-9 {
                    format!(r#" transform="rotate({} {} {})""#, num(-angle.to_degrees()), num(p.x), y(p.y))
                } else {
                    String::new()
                };
                let _ = writeln!(
                    s,
                    r#"<text x="{}" y="{}" font-family="Helvetica, Arial, sans-serif" font-size="{}" text-anchor="{text_anchor}" fill="rgb({g},{g},{g})" stroke="none"{rotate}>{}</text>"#,
                    num(p.x),
                    y(p.y),
                    num(*size),
                    xml_escape(text),
                    g = TEXT_GRAY
                );
            }
            Prim::Image { origin, u, v, image, faded, .. } => {
                // The picture's unit square, top-left corner first, onto the flipped page.
                let top_left = *origin + *v;
                let opacity = if *faded { r#" opacity="0.34""# } else { "" };
                let _ = writeln!(
                    s,
                    r#"<image width="1" height="1" preserveAspectRatio="none" transform="matrix({} {} {} {} {} {})"{opacity} href="data:{};base64,{}"/>"#,
                    num(u.x),
                    num(-u.y),
                    num(-v.x),
                    num(v.y),
                    num(top_left.x),
                    y(top_left.y),
                    image.format.mime(),
                    cc_core::raster::base64_encode(&image.data)
                );
            }
        }
    }
    s.push_str("</g>\n</svg>\n");
    s
}

/// Advance widths of Helvetica for ASCII 32..=126, in 1/1000 em.
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556, 556,
    556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833,
    722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556,
    556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334,
    260, 334, 584,
];

/// Byte of `c` in WinAnsiEncoding (`?` when it has none).
fn win_ansi(c: char) -> u8 {
    let code = c as u32;
    match c {
        '€' => 0x80,
        '‚' => 0x82,
        '„' => 0x84,
        '…' => 0x85,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        _ if (0x20..0x7f).contains(&code) || (0xa0..=0xff).contains(&code) => code as u8,
        _ => b'?',
    }
}

/// Width of `bytes` (WinAnsi) set in Helvetica, in ems.
fn helvetica_width(bytes: &[u8]) -> f64 {
    bytes
        .iter()
        .map(|b| match b {
            0x20..=0x7e => HELVETICA[(b - 0x20) as usize] as f64,
            0x96 => 556.0,
            0x97 => 1000.0,
            0x85 => 1000.0,
            _ => 556.0,
        })
        .sum::<f64>()
        / 1000.0
}

fn pdf_string(bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![b'('];
    for &b in bytes {
        if matches!(b, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(b);
    }
    out.push(b')');
    out
}

fn rgb(c: [u8; 3]) -> String {
    format!("{} {} {}", num(c[0] as f64 / 255.0), num(c[1] as f64 / 255.0), num(c[2] as f64 / 255.0))
}

/// PDF document (one page) of `drawing` on a sheet of `paper` millimeters.
pub fn to_pdf(drawing: &Drawing2D, paper: DVec2) -> Vec<u8> {
    const PT_PER_MM: f64 = 72.0 / 25.4;
    let mut alphas: Vec<u8> = Vec::new();
    // Pictures as image objects: size, compressed RGB and, when not opaque, compressed alpha.
    let mut pictures: Vec<PdfPicture> = Vec::new();
    let mut content: Vec<u8> = Vec::new();
    // Work in millimeters with the origin at the bottom-left corner.
    let _ = writeln!(Utf8(&mut content), "{} 0 0 {} 0 0 cm 1 J 1 j", num(PT_PER_MM), num(PT_PER_MM));
    for prim in &drawing.prims {
        match prim {
            Prim::Line { a, b, style, pen, .. } => {
                let resolved = style.resolve(pen.as_ref());
                let (width, color) = (resolved.width, resolved.color);
                let dash: Vec<String> = resolved.dashes.iter().map(|v| num(*v)).collect();
                let _ = writeln!(
                    Utf8(&mut content),
                    "{} RG {} w [{}] 0 d {} {} m {} {} l S",
                    rgb(color),
                    num(width),
                    dash.join(" "),
                    num(a.x),
                    num(a.y),
                    num(b.x),
                    num(b.y)
                );
            }
            Prim::Fill { pts, color, .. } => {
                let [r, g, b, a] = *color;
                if a == 0 || pts.len() < 3 {
                    continue;
                }
                let gs = if a == 255 {
                    String::new()
                } else {
                    let i = alphas.iter().position(|v| *v == a).unwrap_or_else(|| {
                        alphas.push(a);
                        alphas.len() - 1
                    });
                    format!("/GS{i} gs ")
                };
                let mut path = String::new();
                for (i, p) in pts.iter().enumerate() {
                    let _ = write!(path, "{} {} {} ", num(p.x), num(p.y), if i == 0 { "m" } else { "l" });
                }
                let _ = writeln!(Utf8(&mut content), "q {gs}{} rg {path}h f Q", rgb([r, g, b]));
            }
            Prim::Text { pos, text, size, anchor, angle, .. } => {
                let bytes: Vec<u8> = text.chars().map(win_ansi).collect();
                if text.trim().is_empty() {
                    continue;
                }
                let (base, frac) = anchor_offsets(*anchor);
                let width = helvetica_width(&bytes) * size;
                let (sin, cos) = angle.sin_cos();
                let along = DVec2::new(cos, sin);
                let up = DVec2::new(-sin, cos);
                let p = *pos - along * (width * frac) - up * (base * size);
                let g = num(TEXT_GRAY as f64 / 255.0);
                let _ = write!(
                    Utf8(&mut content),
                    "BT {g} g /F1 {} Tf {} {} {} {} {} {} Tm ",
                    num(*size),
                    num(cos),
                    num(sin),
                    num(-sin),
                    num(cos),
                    num(p.x),
                    num(p.y)
                );
                content.extend(pdf_string(&bytes));
                content.extend(b" Tj ET\n");
            }
            Prim::Image { origin, u, v, image, faded, .. } => {
                let Some(mut rgba) = crate::raster::decode(image, PDF_IMAGE_SIDE) else { continue };
                if *faded {
                    crate::raster::fade(&mut rgba.pixels);
                }
                let rgb: Vec<u8> =
                    rgba.pixels.as_chunks::<4>().0.iter().flat_map(|[r, g, b, _]| [*r, *g, *b]).collect();
                let alpha: Vec<u8> = rgba.pixels.as_chunks::<4>().0.iter().map(|p| p[3]).collect();
                let alpha =
                    alpha.iter().any(|a| *a < 255).then(|| miniz_oxide::deflate::compress_to_vec_zlib(&alpha, 6));
                // The image's unit square (bottom-left at 0, 0) onto the parallelogram.
                let _ = writeln!(
                    Utf8(&mut content),
                    "q {} {} {} {} {} {} cm /Im{} Do Q",
                    num(u.x),
                    num(u.y),
                    num(v.x),
                    num(v.y),
                    num(origin.x),
                    num(origin.y),
                    pictures.len()
                );
                pictures.push((rgba.width, rgba.height, miniz_oxide::deflate::compress_to_vec_zlib(&rgb, 6), alpha));
            }
        }
    }

    let (w, h) = (paper.x * PT_PER_MM, paper.y * PT_PER_MM);
    let mut ext = String::new();
    for (i, a) in alphas.iter().enumerate() {
        let _ = write!(ext, "/GS{i} << /Type /ExtGState /ca {} >> ", num(*a as f64 / 255.0));
    }
    // Objects 1 to 5 are fixed; each picture takes two numbers from 6 (the image, then its alpha mask).
    let picture_object = |i: usize| 6 + 2 * i;
    let xobjects: String = (0..pictures.len()).map(|i| format!("/Im{i} {} 0 R ", picture_object(i))).collect();
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 5 0 R >> /ExtGState << {ext}>> /XObject << {xobjects}>> >> /Contents 4 0 R >>",
            num(w),
            num(h)
        )
        .into_bytes(),
    ];
    let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    stream.extend(&content);
    stream.extend(b"\nendstream");
    objects.push(stream);
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec());
    for (i, (pw, ph, rgb, alpha)) in pictures.iter().enumerate() {
        let image_stream = |extra: &str, color: &str, data: &[u8]| {
            let mut o = format!(
                "<< /Type /XObject /Subtype /Image /Width {pw} /Height {ph} /ColorSpace {color} /BitsPerComponent 8 /Filter /FlateDecode {extra}/Length {} >>\nstream\n",
                data.len()
            )
            .into_bytes();
            o.extend(data);
            o.extend(b"\nendstream");
            o
        };
        let mask = format!("/SMask {} 0 R ", picture_object(i) + 1);
        objects.push(image_stream(if alpha.is_some() { &mask } else { "" }, "/DeviceRGB", rgb));
        // Every picture keeps two numbers; an opaque one's second is an unused empty dictionary.
        objects.push(match alpha {
            Some(a) => image_stream("", "/DeviceGray", a),
            None => b"<< >>".to_vec(),
        });
    }
    objects.push(b"<< /Producer (ConstructCAD) >>".to_vec());

    let mut out: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).into_bytes());
        out.extend(body);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for o in offsets {
        let _ = writeln!(tail, "{o:010} 00000 n ");
    }
    let _ = write!(
        tail,
        "trailer\n<< /Size {} /Root 1 0 R /Info {} 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1,
        objects.len()
    );
    out.extend(tail.into_bytes());
    out
}

/// `fmt::Write` adapter for building ASCII content into a byte buffer.
struct Utf8<'a>(&'a mut Vec<u8>);

impl std::fmt::Write for Utf8<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

/// The largest raster export, in pixels (about 20000 x 5000, or an A0
/// sheet at 300 dpi).
pub const MAX_PIXELS: u64 = 160_000_000;

/// A picture of a PDF: width, height, compressed RGB and, when not opaque, compressed alpha.
type PdfPicture = (u32, u32, Vec<u8>, Option<Vec<u8>>);

/// Longest side of a picture in an exported PDF, pixels (larger ones are reduced).
const PDF_IMAGE_SIDE: u32 = 6000;

const INTER: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");

/// Writes glyph outlines as SVG path data, through `at` (glyph to page).
struct GlyphPen<'a> {
    d: &'a mut String,
    at: &'a dyn Fn(f32, f32) -> DVec2,
}

impl GlyphPen<'_> {
    fn point(&mut self, x: f32, y: f32) {
        let p = (self.at)(x, y);
        let _ = write!(self.d, "{} {} ", num(p.x), num(p.y));
    }
}

impl skrifa::outline::OutlinePen for GlyphPen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.d.push('M');
        self.point(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.d.push('L');
        self.point(x, y);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.d.push('Q');
        self.point(cx0, cy0);
        self.point(x, y);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.d.push('C');
        self.point(cx0, cy0);
        self.point(cx1, cy1);
        self.point(x, y);
    }
    fn close(&mut self) {
        self.d.push('Z');
    }
}

/// SVG path data of `text` set in `fonts` (each character from the first
/// font that has it; no kerning), placed like the SVG writer's text: `size`
/// millimeters high, anchored and rotated, on a page `page_h` high (SVG's
/// y runs down).
fn glyph_path(
    fonts: &[skrifa::FontRef],
    text: &str,
    pos: DVec2,
    size: f64,
    anchor: Anchor,
    angle: f64,
    page_h: f64,
) -> String {
    use skrifa::MetadataProvider;
    use skrifa::instance::{LocationRef, Size};
    use skrifa::outline::DrawSettings;
    let em = Size::new(size as f32);
    let location = LocationRef::default();
    let charmaps: Vec<_> = fonts.iter().map(|f| f.charmap()).collect();
    let metrics: Vec<_> = fonts.iter().map(|f| f.glyph_metrics(em, location)).collect();
    let outlines: Vec<_> = fonts.iter().map(|f| f.outline_glyphs()).collect();
    let glyphs: Vec<(usize, skrifa::GlyphId)> = text.chars().map(|c| pick_glyph(&charmaps, c)).collect();
    let advance =
        |(f, g): (usize, skrifa::GlyphId)| metrics.get(f).and_then(|m| m.advance_width(g)).unwrap_or(0.0) as f64;
    let width: f64 = glyphs.iter().map(|g| advance(*g)).sum();
    let (base, frac) = anchor_offsets(anchor);
    let along = DVec2::new(angle.cos(), angle.sin());
    let up = along.perp();
    let origin = pos - up * (base * size) - along * (frac * width);
    let mut d = String::new();
    let mut x = 0.0;
    for (f, g) in glyphs {
        if let Some(glyph) = outlines.get(f).and_then(|o| o.get(g)) {
            let at = |gx: f32, gy: f32| {
                let p = origin + along * (x + gx as f64) + up * gy as f64;
                DVec2::new(p.x, page_h - p.y)
            };
            let mut pen = GlyphPen { d: &mut d, at: &at };
            // A glyph that fails to draw is left out.
            let _ = glyph.draw(DrawSettings::unhinted(em, location), &mut pen);
        }
        x += advance((f, g));
    }
    d
}

/// The font (index in `charmaps`) and glyph of `c`: the first font that
/// has it, else the first font's missing-glyph box.
fn pick_glyph(charmaps: &[skrifa::charmap::Charmap], c: char) -> (usize, skrifa::GlyphId) {
    charmaps.iter().enumerate().find_map(|(i, m)| m.map(c).map(|g| (i, g))).unwrap_or((0, skrifa::GlyphId::NOTDEF))
}

/// A view's drawing (meters, drawn at 1:`scale`; text sizes in paper
/// millimeters) laid out on paper: the drawing in millimeters with `margin`
/// around it, and the paper size.
pub fn view_on_paper(drawing: &Drawing2D, scale: u32, margin: f64) -> (Drawing2D, DVec2) {
    let k = 1000.0 / scale.max(1) as f64;
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    let mut grow = |p: DVec2, r: DVec2| {
        if p.is_finite() && r.is_finite() {
            lo = lo.min(p - r);
            hi = hi.max(p + r);
        }
    };
    for prim in &drawing.prims {
        match prim {
            Prim::Line { a, b, .. } => {
                grow(*a * k, DVec2::ZERO);
                grow(*b * k, DVec2::ZERO);
            }
            Prim::Fill { pts, .. } => pts.iter().for_each(|p| grow(*p * k, DVec2::ZERO)),
            // Text runs off its anchor: a generous box (any rotation).
            Prim::Text { pos, text, size, .. } => {
                // Ideographs and other wide characters take a full em.
                let ems: f64 = text.chars().map(|c| if c >= '\u{1100}' { 1.0 } else { 0.6 }).sum();
                let width = ems * size;
                grow(*pos * k, DVec2::splat(width.max(*size)));
            }
            Prim::Image { origin, u, v, .. } => {
                for p in Prim::image_corners(*origin, *u, *v) {
                    grow(p * k, DVec2::ZERO);
                }
            }
        }
    }
    if !(lo.is_finite() && hi.is_finite()) {
        (lo, hi) = (DVec2::ZERO, DVec2::splat(10.0));
    }
    let origin = lo - DVec2::splat(margin);
    let map = |p: DVec2| p * k - origin;
    let prims = drawing.prims.iter().map(|prim| prim.mapped(map)).collect();
    (
        Drawing2D {
            prims,
            fit_elements: drawing.fit_elements,
            background: drawing.background.clone(),
            tags: Vec::new(),
        },
        hi - lo + DVec2::splat(2.0 * margin),
    )
}

/// PNG image of `drawing` on a sheet of `paper` millimeters at `dpi` dots
/// per inch, on white; text is drawn as outlines of Inter (embedded), with
/// the system's fallback fonts (`fonts::fallbacks`: CJK, Lao) for the
/// characters Inter lacks.
pub fn to_png(drawing: &Drawing2D, paper: DVec2, dpi: f64) -> Result<Vec<u8>, String> {
    use resvg::{tiny_skia, usvg};
    if !(dpi.is_finite() && dpi > 0.0) {
        return Err("the resolution must be positive".into());
    }
    let px = (paper / 25.4 * dpi).round();
    if !(px.is_finite() && px.x >= 1.0 && px.y >= 1.0) || px.x * px.y > MAX_PIXELS as f64 {
        return Err(format!("the image would be {} x {} pixels: lower the resolution", px.x, px.y));
    }
    let (w, h) = (px.x as u32, px.y as u32);
    let mut fonts = vec![skrifa::FontRef::new(INTER).map_err(|e| e.to_string())?];
    fonts.extend(crate::fonts::fallbacks().filter_map(|f| f.font_ref()));
    let source = svg(drawing, paper, &fonts);
    let tree = usvg::Tree::from_str(&source, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("the image is too large")?;
    pixmap.fill(tiny_skia::Color::WHITE);
    let size = tree.size();
    let transform = tiny_skia::Transform::from_scale(w as f32 / size.width(), h as f32 / size.height());
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let per_meter = (dpi / 0.0254).round() as u32;
    encoder.set_pixel_dims(Some(png::PixelDimensions { xppu: per_meter, yppu: per_meter, unit: png::Unit::Meter }));
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    // Opaque pixels: premultiplied and straight alpha are the same.
    writer.write_image_data(pixmap.data()).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LineStyle;

    fn sample() -> Drawing2D {
        let mut d = Drawing2D::default();
        d.line(DVec2::new(10.0, 10.0), DVec2::new(100.0, 10.0), LineStyle::Cut, None);
        d.line(DVec2::new(10.0, 20.0), DVec2::new(100.0, 20.0), LineStyle::Hidden, None);
        d.fill(vec![DVec2::ZERO, DVec2::new(5.0, 0.0), DVec2::new(5.0, 5.0)], [200, 0, 0, 128], None);
        d.text_at(DVec2::new(50.0, 50.0), "Planta <Térreo> & (cobertura) — 1:50", 3.0, Anchor::Center, 0.3);
        d
    }

    #[test]
    fn svg_has_every_primitive() {
        let svg = to_svg(&sample(), DVec2::new(420.0, 297.0));
        assert!(svg.contains(r#"width="420mm" height="297mm" viewBox="0 0 420 297""#));
        assert_eq!(svg.matches("<line ").count(), 2);
        assert!(svg.contains("stroke-dasharray"));
        assert!(svg.contains("fill-opacity"));
        assert!(svg.contains("Planta &lt;Térreo&gt; &amp; (cobertura) — 1:50"));
        assert!(svg.contains("rotate("));
        // Paper y is flipped: the line at y = 10 sits at 287 from the top.
        assert!(svg.contains(r#"y1="287""#));
    }

    #[test]
    fn pdf_structure_is_consistent() {
        let pdf = to_pdf(&sample(), DVec2::new(420.0, 297.0));
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let find = |needle: &[u8]| pdf.windows(needle.len()).position(|w| w == needle).unwrap();
        let rfind = |needle: &[u8]| pdf.windows(needle.len()).rposition(|w| w == needle).unwrap();
        let ascii = |a: usize, b: usize| String::from_utf8(pdf[a..b].to_vec()).unwrap();
        // startxref points at the xref table.
        let sx = rfind(
            b"startxref
",
        ) + 10;
        let start: usize = ascii(sx, pdf.len()).lines().next().unwrap().parse().unwrap();
        assert!(pdf[start..].starts_with(b"xref"));
        // Every object offset in the table points at its "N 0 obj".
        let table = ascii(start, pdf.len());
        for (i, line) in table.lines().skip(3).take(6).enumerate() {
            let off: usize = line[..10].parse().unwrap();
            assert!(pdf[off..].starts_with(format!("{} 0 obj", i + 1).as_bytes()), "object {}", i + 1);
        }
        // Stream length matches its content.
        let s = find(
            b"stream
",
        ) + 7;
        let e = find(
            b"
endstream",
        );
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains(&format!("/Length {}", e - s)));
        assert!(text.contains("/GS0 gs"));
        assert!(text.contains("1190.551 841.89"));
        // Parentheses are escaped; the em dash is WinAnsi 0x97.
        assert!(pdf.windows(2).any(|w| w == b"\\("));
        assert!(pdf.contains(&0x97));
    }

    #[test]
    fn glyphs_come_from_the_first_font_that_has_them() {
        use skrifa::MetadataProvider;
        let inter = skrifa::FontRef::new(INTER).unwrap();
        let maps = [inter.charmap()];
        assert_eq!(pick_glyph(&maps, 'A').0, 0);
        // A character no font has: the first font's box.
        assert_eq!(pick_glyph(&maps, '\u{4E2D}'), (0, skrifa::GlyphId::NOTDEF));
        // With the system's CJK font (when there is one), ideographs come from it.
        if let Some(cjk) = crate::fonts::cjk().first().and_then(|f| f.font_ref()) {
            let maps = [inter.charmap(), cjk.charmap()];
            assert_eq!(pick_glyph(&maps, 'A').0, 0);
            let (font, glyph) = pick_glyph(&maps, '\u{4E2D}');
            assert_eq!(font, 1);
            assert_ne!(glyph, skrifa::GlyphId::NOTDEF);
        }
    }

    #[test]
    fn png_draws_text_as_glyph_outlines() {
        let mut d = Drawing2D::default();
        d.text(DVec2::new(4.0, 4.0), "Sala 1", 5.0);
        let font = skrifa::FontRef::new(INTER).unwrap();
        let svg = svg(&d, DVec2::new(40.0, 12.0), &[font]);
        assert!(svg.contains("<path d=\"M") && !svg.contains("<text"), "{svg}");
        // 40 x 12 mm at 254 dpi: 400 x 120 pixels.
        let png = to_png(&d, DVec2::new(40.0, 12.0), 254.0).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        let be = |i: usize| u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]);
        assert_eq!((be(16), be(20)), (400, 120));
        assert!(to_png(&d, DVec2::new(40.0, 12.0), 1e6).is_err(), "too many pixels");
        // A view at 1:100 lands on paper in millimeters, with the margin.
        let mut v = Drawing2D::default();
        v.line(DVec2::ZERO, DVec2::new(10.0, 5.0), crate::LineStyle::Cut, None);
        let (_, paper) = view_on_paper(&v, 100, 5.0);
        assert!((paper - DVec2::new(110.0, 60.0)).length() < 1e-9, "{paper}");
    }
}
