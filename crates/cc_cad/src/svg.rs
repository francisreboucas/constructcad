//! SVG drawings into the neutral CAD drawing: paths (curves flattened to
//! polylines) in their stroke color, single-outline filled shapes as fills,
//! and embedded pictures as images. Units are millimeters at the CSS 96
//! pixels per inch, y up, the page's bottom-left corner at the origin.
//! Texts are not read (they need fonts): SVGs from CAD programs usually
//! carry them as outlines already.

use crate::{CadError, CadRead, Limits};
use cc_core::cad::{CadDrawing, CadImage, CadLayer, CadPrim, CadUnit, MAX_IMAGES, MAX_POLY_POINTS};
use cc_core::raster::RasterImage;
use glam::DVec2;
use resvg::usvg::{self, tiny_skia_path::PathSegment};
use std::sync::Arc;

/// Millimeters per CSS pixel.
const MM_PER_PX: f64 = 25.4 / 96.0;
/// Deepest group nesting followed.
const MAX_DEPTH: usize = 64;
/// Largest SVG file read, bytes.
pub const MAX_SVG_BYTES: usize = 64 << 20;

/// Whether `data` looks like SVG (an `<svg` element near the start).
pub fn is_svg(data: &[u8]) -> bool {
    let head = &data[..data.len().min(4096)];
    head.windows(4).any(|w| w.eq_ignore_ascii_case(b"<svg"))
}

struct Reader<'a> {
    limits: &'a Limits,
    height: f64,
    drawing: CadDrawing,
    read: CadRead,
}

fn color(paint: &usvg::Paint) -> [u8; 3] {
    match paint {
        usvg::Paint::Color(c) => [c.red, c.green, c.blue],
        usvg::Paint::LinearGradient(g) => g.stops().first().map_or([128; 3], stop_color),
        usvg::Paint::RadialGradient(g) => g.stops().first().map_or([128; 3], stop_color),
        usvg::Paint::Pattern(_) => [128; 3],
    }
}

fn stop_color(s: &usvg::Stop) -> [u8; 3] {
    let c = s.color();
    [c.red, c.green, c.blue]
}

/// Segments for a curve whose control polygon is `len` millimeters long.
fn steps(len: f64) -> usize {
    ((len / 0.5).ceil() as usize).clamp(2, 64)
}

impl Reader<'_> {
    /// A page point (pixels, y down, under `t`) in drawing millimeters, y up.
    fn point(&self, t: &usvg::Transform, x: f32, y: f32) -> DVec2 {
        let (x, y) = (f64::from(x), f64::from(y));
        let px = f64::from(t.sx) * x + f64::from(t.kx) * y + f64::from(t.tx);
        let py = f64::from(t.ky) * x + f64::from(t.sy) * y + f64::from(t.ty);
        DVec2::new(px * MM_PER_PX, (self.height - py) * MM_PER_PX)
    }

    fn full(&mut self) -> bool {
        if self.drawing.prims.len() >= self.limits.max_prims {
            self.read.truncated = true;
        }
        self.read.truncated
    }

    fn skip(&mut self, what: &str) {
        *self.read.skipped.entry(what.to_string()).or_default() += 1;
    }

    fn group(&mut self, g: &usvg::Group, depth: usize) {
        if depth > MAX_DEPTH {
            self.read.truncated = true;
            return;
        }
        for node in g.children() {
            if self.full() {
                return;
            }
            match node {
                usvg::Node::Group(g) => self.group(g, depth + 1),
                usvg::Node::Path(p) => self.path(p),
                usvg::Node::Image(i) => self.image(i, depth),
                usvg::Node::Text(_) => self.skip("TEXT"),
            }
        }
    }

    /// The subpaths of a path as drawing points, with whether each is closed.
    fn subpaths(&self, p: &usvg::Path) -> Vec<(Vec<DVec2>, bool)> {
        let t = p.abs_transform();
        let mut out: Vec<(Vec<DVec2>, bool)> = Vec::new();
        let mut current: Vec<DVec2> = Vec::new();
        let flush = |current: &mut Vec<DVec2>, out: &mut Vec<(Vec<DVec2>, bool)>, closed: bool| {
            if current.len() >= 2 {
                out.push((std::mem::take(current), closed));
            }
            current.clear();
        };
        for segment in p.data().segments() {
            match segment {
                PathSegment::MoveTo(a) => {
                    flush(&mut current, &mut out, false);
                    current.push(self.point(&t, a.x, a.y));
                }
                PathSegment::LineTo(a) => current.push(self.point(&t, a.x, a.y)),
                PathSegment::QuadTo(c, a) => {
                    let Some(&p0) = current.last() else { continue };
                    let (p1, p2) = (self.point(&t, c.x, c.y), self.point(&t, a.x, a.y));
                    let n = steps(p0.distance(p1) + p1.distance(p2));
                    for i in 1..=n {
                        let s = i as f64 / n as f64;
                        current.push(p0 * (1.0 - s).powi(2) + p1 * 2.0 * s * (1.0 - s) + p2 * s * s);
                    }
                }
                PathSegment::CubicTo(c1, c2, a) => {
                    let Some(&p0) = current.last() else { continue };
                    let (p1, p2, p3) =
                        (self.point(&t, c1.x, c1.y), self.point(&t, c2.x, c2.y), self.point(&t, a.x, a.y));
                    let n = steps(p0.distance(p1) + p1.distance(p2) + p2.distance(p3));
                    for i in 1..=n {
                        let s = i as f64 / n as f64;
                        let r = 1.0 - s;
                        current.push(p0 * r.powi(3) + p1 * 3.0 * s * r * r + p2 * 3.0 * s * s * r + p3 * s.powi(3));
                    }
                }
                PathSegment::Close => {
                    // Drop the repeated first point: the polyline closes itself.
                    if current.len() > 2
                        && current.first().zip(current.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-9)
                    {
                        current.pop();
                    }
                    let start = current.first().copied();
                    flush(&mut current, &mut out, true);
                    // Drawing may go on from the start of the closed subpath.
                    current.extend(start);
                }
            }
        }
        flush(&mut current, &mut out, false);
        for (pts, _) in &mut out {
            pts.dedup_by(|a, b| a.distance(*b) < 1e-9);
            pts.truncate(MAX_POLY_POINTS);
        }
        out.retain(|(pts, _)| pts.len() >= 2 && pts.iter().all(|p| p.is_finite()));
        out
    }

    fn path(&mut self, p: &usvg::Path) {
        if !p.is_visible() {
            return;
        }
        let subpaths = self.subpaths(p);
        if subpaths.is_empty() {
            return;
        }
        let fill = p.fill().map(|f| color(f.paint()));
        let stroke = p.stroke().map(|s| color(s.paint()));
        // A filled single outline is a fill; with holes (or several parts) only their outlines.
        if let (Some(c), [(pts, _)]) = (fill, subpaths.as_slice())
            && pts.len() >= 3
        {
            self.drawing.prims.push(CadPrim::Fill { layer: 0, color: Some(c), pts: pts.clone() });
            if stroke.is_none() {
                return;
            }
        }
        let Some(c) = stroke.or(fill) else { return };
        for (pts, closed) in subpaths {
            if self.full() {
                return;
            }
            self.drawing.prims.push(CadPrim::Polyline { layer: 0, color: Some(c), closed, pts, bulges: Vec::new() });
        }
    }

    fn image(&mut self, image: &usvg::Image, depth: usize) {
        if !image.is_visible() {
            return;
        }
        let data = match image.kind() {
            usvg::ImageKind::PNG(d) | usvg::ImageKind::JPEG(d) | usvg::ImageKind::GIF(d) | usvg::ImageKind::WEBP(d) => {
                d.clone()
            }
            usvg::ImageKind::SVG(tree) => {
                self.group(tree.root(), depth + 1);
                return;
            }
        };
        if self.drawing.images.len() >= MAX_IMAGES {
            self.read.truncated = true;
            return;
        }
        let Some(picture) = picture(data.as_slice()) else {
            self.skip("IMAGE");
            return;
        };
        // The image's box, from its top-left corner (0, 0) to its size, under its transform.
        let t = image.abs_transform();
        let size = image.size();
        let (w, h) = (size.width(), size.height());
        let origin = self.point(&t, 0.0, h);
        let u = self.point(&t, w, h) - origin;
        let v = self.point(&t, 0.0, 0.0) - origin;
        let placed = CadImage { layer: 0, origin, u, v, image: Arc::new(picture) };
        if placed.is_valid() {
            self.drawing.images.push(placed);
        } else {
            self.skip("IMAGE");
        }
    }
}

/// An embedded picture's bytes read as a raster image (its size from its header).
fn picture(data: &[u8]) -> Option<RasterImage> {
    let format = cc_core::raster::RasterFormat::sniff(data)?;
    let (width, height) = header_size(data, format)?;
    let image = RasterImage { format, width, height, data: data.to_vec() };
    image.is_valid().then_some(image)
}

/// Pixel size from a PNG, GIF or JPEG header (others: `None`).
fn header_size(d: &[u8], format: cc_core::raster::RasterFormat) -> Option<(u32, u32)> {
    use cc_core::raster::RasterFormat as F;
    let be32 = |i: usize| d.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let be16 = |i: usize| d.get(i..i + 2).map(|b| u32::from(u16::from_be_bytes([b[0], b[1]])));
    let le16 = |i: usize| d.get(i..i + 2).map(|b| u32::from(u16::from_le_bytes([b[0], b[1]])));
    match format {
        F::Png => Some((be32(16)?, be32(20)?)),
        F::Gif => Some((le16(6)?, le16(8)?)),
        F::Jpeg => {
            // Walk the markers to the first start-of-frame.
            let mut at = 2;
            for _ in 0..1000 {
                if *d.get(at)? != 0xFF {
                    return None;
                }
                let marker = *d.get(at + 1)?;
                let len = be16(at + 2)? as usize;
                if matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                    return Some((be16(at + 7)?, be16(at + 5)?));
                }
                at += 2 + len;
            }
            None
        }
        F::Bmp | F::Webp => None,
    }
}

/// Read an SVG file's bytes.
pub fn read(data: &[u8], limits: &Limits) -> Result<CadRead, CadError> {
    if data.len() > MAX_SVG_BYTES {
        return Err(CadError::TooLarge(MAX_SVG_BYTES >> 20));
    }
    let tree = usvg::Tree::from_data(data, &usvg::Options::default()).map_err(|e| CadError::Format(e.to_string()))?;
    let mut reader = Reader {
        limits,
        height: f64::from(tree.size().height()),
        drawing: CadDrawing {
            layers: vec![CadLayer { name: "0".into(), color: [0, 0, 0], visible: true }],
            units: Some(CadUnit::Millimeters),
            ..Default::default()
        },
        read: CadRead { format: crate::Format::Svg, ..Default::default() },
    };
    reader.group(tree.root(), 0);
    let mut read = reader.read;
    read.drawing = reader.drawing;
    read.entities = read.drawing.prims.len() + read.drawing.images.len();
    read.drawing.repair();
    read.drawing.compute_extents();
    Ok(read)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svg(body: &str) -> CadRead {
        let text = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96" viewBox="0 0 96 96">{body}</svg>"#
        );
        read(text.as_bytes(), &Limits::default()).unwrap()
    }

    #[test]
    fn shapes_become_polylines_and_fills() {
        let r = svg(r#"<rect x="0" y="0" width="48" height="96" fill="rgb(255,0,0)"/>
            <line x1="0" y1="96" x2="96" y2="96" stroke="blue"/>
            <circle cx="48" cy="48" r="24" fill="none" stroke="black"/>"#);
        let d = &r.drawing;
        // The 96 px page is one inch: 25.4 mm, y up from its bottom edge.
        let fill = d.prims.iter().find_map(|p| match p {
            CadPrim::Fill { pts, color, .. } => Some((pts.clone(), *color)),
            _ => None,
        });
        let (pts, color) = fill.unwrap();
        assert_eq!(color, Some([255, 0, 0]));
        let (lo, hi) = pts.iter().fold((pts[0], pts[0]), |(a, b), p| (a.min(*p), b.max(*p)));
        assert!(lo.distance(DVec2::ZERO) < 1e-6 && hi.distance(DVec2::new(12.7, 25.4)) < 1e-6, "{lo} {hi}");
        let line = d.prims.iter().find_map(|p| match p {
            CadPrim::Polyline { pts, color: Some([0, 0, 255]), .. } => Some(pts.clone()),
            _ => None,
        });
        let line = line.unwrap();
        assert!(line[0].distance(DVec2::ZERO) < 1e-6 && line[1].distance(DVec2::new(25.4, 0.0)) < 1e-6);
        // The circle is a closed polyline about its center, a quarter inch round.
        let circle = d.prims.iter().find_map(|p| match p {
            CadPrim::Polyline { pts, closed: true, color: Some([0, 0, 0]), .. } => Some(pts.clone()),
            _ => None,
        });
        let circle = circle.unwrap();
        assert!(circle.len() > 8);
        assert!(circle.iter().all(|p| (p.distance(DVec2::splat(12.7)) - 6.35).abs() < 0.05));
        assert_eq!(d.units, Some(CadUnit::Millimeters));
    }

    #[test]
    fn embedded_pictures_become_images() {
        // A 1 x 1 PNG.
        let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";
        let r = svg(&format!(r#"<image x="0" y="48" width="48" height="48" href="data:image/png;base64,{png}"/>"#));
        let image = r.drawing.images.first().unwrap();
        assert_eq!((image.image.width, image.image.height), (1, 1));
        assert!(image.origin.distance(DVec2::ZERO) < 1e-6);
        assert!(image.u.distance(DVec2::new(12.7, 0.0)) < 1e-6 && image.v.distance(DVec2::new(0.0, 12.7)) < 1e-6);
    }

    #[test]
    fn bad_svgs_are_errors() {
        assert!(read(b"<svg", &Limits::default()).is_err());
        assert!(is_svg(b"<?xml version=\"1.0\"?>\n<svg xmlns=\"\">"));
        assert!(!is_svg(b"0\nSECTION"));
    }
}
