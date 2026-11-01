//! DXF export of drawings, for exchanging 2D views with other CAD programs.
//! The writer is self-contained and writes ASCII DXF R12 (AC1009), the
//! version every CAD program reads: lines as `LINE` with their dash
//! patterns as line types, opaque fills as `SOLID` triangles, and texts as
//! `TEXT` with their alignment. Each primitive goes to the layer the
//! caller names (the element's category); colors are the nearest
//! AutoCAD Color Index of the pen's color, so a plot style by color can give
//! the pens their weights (R12 has no line weights).

use crate::{Anchor, Drawing2D, LineStyle, Prim};
use glam::DVec2;
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// How a drawing is written: coordinates are multiplied by `unit`; text
/// heights and dash lengths, in paper millimeters, by `paper`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DxfScale {
    pub unit: f64,
    pub paper: f64,
}

impl DxfScale {
    /// A view in meters drawn at 1:`scale`, written in millimeters at full size.
    pub fn model(scale: u32) -> Self {
        Self { unit: 1000.0, paper: scale.max(1) as f64 }
    }

    /// A drawing already in paper millimeters (a sheet).
    pub const PAPER: Self = Self { unit: 1.0, paper: 1.0 };
}

/// Fills more transparent than this are left out (overlays, not surfaces).
const MIN_FILL_ALPHA: u8 = 200;

/// The named colors of the AutoCAD Color Index used for pens: the seven
/// standard colors and the grays.
const ACI: [(u8, [u8; 3]); 13] = [
    (1, [255, 0, 0]),
    (2, [255, 255, 0]),
    (3, [0, 255, 0]),
    (4, [0, 255, 255]),
    (5, [0, 0, 255]),
    (6, [255, 0, 255]),
    (7, [0, 0, 0]),
    (8, [128, 128, 128]),
    (9, [192, 192, 192]),
    (250, [51, 51, 51]),
    (251, [91, 91, 91]),
    (252, [132, 132, 132]),
    (253, [173, 173, 173]),
];

/// The nearest index color of `c` (7, black on white paper, for dark colors).
pub fn aci(c: [u8; 3]) -> u8 {
    let d = |a: [u8; 3]| (0..3).map(|i| (i32::from(a[i]) - i32::from(c[i])).pow(2)).sum::<i32>();
    ACI.iter().min_by_key(|(_, rgb)| d(*rgb)).map_or(7, |(i, _)| *i)
}

/// A layer name DXF R12 accepts: letters, digits, `$`, `-` and `_`, at
/// most 31 characters, upper case.
pub fn layer_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '$' | '-' | '_') { c.to_ascii_uppercase() } else { '_' })
        .take(31)
        .collect();
    if s.is_empty() { "0".to_string() } else { s }
}

/// The corners of a picture's parallelogram, in order round it.
fn image_frame(origin: DVec2, u: DVec2, v: DVec2) -> [DVec2; 4] {
    [origin, origin + u, origin + u + v, origin + v]
}

/// The layer of a primitive with no element: annotation lines, texts, or the rest.
pub fn default_layer(prim: &Prim) -> &'static str {
    match prim {
        Prim::Line { style: LineStyle::Annotation | LineStyle::Center, .. } => "ANNOTATION",
        Prim::Text { .. } => "TEXT",
        _ => "DRAWING",
    }
}

fn num(v: f64) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".to_string() } else { s.to_string() }
}

/// The characters of code page 1252 from 0x80 to 0x9F (0 where it has none).
const CP1252_HIGH: [u16; 32] = [
    0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0, 0x017D, 0, 0,
    0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0, 0x017E, 0x0178,
];

/// The byte of `c` in code page 1252 (the file's `$DWGCODEPAGE`), if it has one.
fn cp1252(c: char) -> Option<u8> {
    let u = c as u32;
    match u {
        0..=0x7F | 0xA0..=0xFF => u8::try_from(u).ok(),
        _ => CP1252_HIGH.iter().position(|&h| u32::from(h) == u && h != 0).and_then(|i| u8::try_from(0x80 + i).ok()),
    }
}

/// The document's text in code page 1252 (every character the writer
/// leaves in it has a byte there; others were escaped).
fn encode(s: &str) -> Vec<u8> {
    s.chars().map(|c| cp1252(c).unwrap_or(b'?')).collect()
}

/// Text as DXF R12 writes it: the characters of code page 1252 as they are
/// (written in it), others as `\U+XXXX` escapes; line breaks become spaces.
fn text_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\n' | '\r' | '\t' => out.push(' '),
            c if (c as u32) < 0x20 || c == '\u{7F}' => {}
            c if cp1252(c).is_some() => out.push(c),
            c if (c as u32) <= 0xFFFF => {
                let _ = write!(out, "\\U+{:04X}", c as u32);
            }
            // Outside the basic plane: not representable in R12 text.
            _ => out.push('?'),
        }
    }
    out
}

struct Writer {
    out: String,
}

impl Writer {
    fn pair(&mut self, code: u16, value: impl std::fmt::Display) {
        let _ = write!(self.out, "{code:>3}\n{value}\n");
    }

    fn point(&mut self, base: u16, p: DVec2) {
        self.pair(base, num(p.x));
        self.pair(base + 10, num(p.y));
        self.pair(base + 20, "0.0");
    }
}

/// The DXF R12 document of `drawing` (in code page 1252), scaled by
/// `scale`, each primitive on the layer `layer` names (sanitized with
/// [`layer_name`]).
pub fn to_dxf(drawing: &Drawing2D, scale: DxfScale, layer: &dyn Fn(&Prim) -> String) -> Vec<u8> {
    let unit = if scale.unit.is_finite() && scale.unit > 0.0 { scale.unit } else { 1.0 };
    let paper = if scale.paper.is_finite() && scale.paper > 0.0 { scale.paper } else { 1.0 };
    let at = |p: DVec2| p * unit;
    // The line types of the dash patterns used, and the layers.
    let mut line_types: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut layers: BTreeMap<String, ()> = BTreeMap::new();
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    let mut grow = |p: DVec2| {
        if p.is_finite() {
            lo = lo.min(p);
            hi = hi.max(p);
        }
    };
    let dash_name = |dashes: &[f64]| {
        let parts: Vec<String> = dashes.iter().map(|d| num(d * paper).replace('.', "_")).collect();
        layer_name(&format!("CC_{}", parts.join("-")))
    };
    for prim in &drawing.prims {
        layers.insert(layer_name(&layer(prim)), ());
        match prim {
            Prim::Line { a, b, style, pen, .. } => {
                grow(at(*a));
                grow(at(*b));
                let dashes = style.resolve(pen.as_ref()).dashes;
                if !dashes.is_empty() {
                    line_types.entry(dash_name(dashes)).or_insert_with(|| dashes.iter().map(|d| d * paper).collect());
                }
            }
            Prim::Fill { pts, .. } => pts.iter().for_each(|p| grow(at(*p))),
            Prim::Text { pos, .. } => grow(at(*pos)),
            Prim::Image { origin, u, v, .. } => image_frame(*origin, *u, *v).iter().for_each(|p| grow(at(*p))),
        }
    }
    if !(lo.is_finite() && hi.is_finite()) {
        (lo, hi) = (DVec2::ZERO, DVec2::ZERO);
    }
    let mut w = Writer { out: String::new() };
    // Header: version, code page, units (millimeters) and extents.
    w.pair(0, "SECTION");
    w.pair(2, "HEADER");
    w.pair(9, "$ACADVER");
    w.pair(1, "AC1009");
    w.pair(9, "$DWGCODEPAGE");
    w.pair(3, "ANSI_1252");
    w.pair(9, "$INSUNITS");
    w.pair(70, 4);
    w.pair(9, "$INSBASE");
    w.point(10, DVec2::ZERO);
    w.pair(9, "$EXTMIN");
    w.point(10, lo);
    w.pair(9, "$EXTMAX");
    w.point(10, hi);
    w.pair(9, "$LTSCALE");
    w.pair(40, "1.0");
    w.pair(0, "ENDSEC");
    // Tables: line types, layers, the text style.
    w.pair(0, "SECTION");
    w.pair(2, "TABLES");
    w.pair(0, "TABLE");
    w.pair(2, "LTYPE");
    w.pair(70, line_types.len() + 1);
    w.pair(0, "LTYPE");
    w.pair(2, "CONTINUOUS");
    w.pair(70, 0);
    w.pair(3, "Solid line");
    w.pair(72, 65);
    w.pair(73, 0);
    w.pair(40, "0.0");
    for (name, dashes) in &line_types {
        w.pair(0, "LTYPE");
        w.pair(2, name);
        w.pair(70, 0);
        w.pair(3, "");
        w.pair(72, 65);
        w.pair(73, dashes.len());
        w.pair(40, num(dashes.iter().sum()));
        // Dashes positive, gaps negative, alternating from a dash.
        for (i, d) in dashes.iter().enumerate() {
            w.pair(49, num(if i % 2 == 0 { *d } else { -*d }));
        }
    }
    w.pair(0, "ENDTAB");
    w.pair(0, "TABLE");
    w.pair(2, "LAYER");
    w.pair(70, layers.len() + 1);
    for name in std::iter::once("0").chain(layers.keys().map(String::as_str).filter(|n| *n != "0")) {
        w.pair(0, "LAYER");
        w.pair(2, name);
        w.pair(70, 0);
        w.pair(62, 7);
        w.pair(6, "CONTINUOUS");
    }
    w.pair(0, "ENDTAB");
    w.pair(0, "TABLE");
    w.pair(2, "STYLE");
    w.pair(70, 1);
    w.pair(0, "STYLE");
    w.pair(2, "STANDARD");
    w.pair(70, 0);
    w.pair(40, "0.0");
    w.pair(41, "1.0");
    w.pair(50, "0.0");
    w.pair(71, 0);
    w.pair(42, "2.5");
    w.pair(3, "txt");
    w.pair(4, "");
    w.pair(0, "ENDTAB");
    w.pair(0, "ENDSEC");
    w.pair(0, "SECTION");
    w.pair(2, "BLOCKS");
    w.pair(0, "ENDSEC");
    // Entities, in drawing order.
    w.pair(0, "SECTION");
    w.pair(2, "ENTITIES");
    for prim in &drawing.prims {
        let layer = layer_name(&layer(prim));
        match prim {
            Prim::Line { a, b, style, pen, .. } => {
                let (a, b) = (at(*a), at(*b));
                if !(a.is_finite() && b.is_finite()) {
                    continue;
                }
                let resolved = style.resolve(pen.as_ref());
                w.pair(0, "LINE");
                w.pair(8, &layer);
                if !resolved.dashes.is_empty() {
                    w.pair(6, dash_name(resolved.dashes));
                }
                w.pair(62, aci(resolved.color));
                w.point(10, a);
                w.point(11, b);
            }
            Prim::Fill { pts, color, .. } => {
                let [r, g, b, alpha] = *color;
                if alpha < MIN_FILL_ALPHA || pts.len() < 3 {
                    continue;
                }
                let pts: Vec<DVec2> = pts.iter().map(|p| at(*p)).collect();
                if !pts.iter().all(|p| p.is_finite()) {
                    continue;
                }
                for [i, j, k] in cc_geom::polygon::triangulate(&pts) {
                    let (Some(p), Some(q), Some(s)) = (pts.get(i), pts.get(j), pts.get(k)) else { continue };
                    w.pair(0, "SOLID");
                    w.pair(8, &layer);
                    w.pair(62, aci([r, g, b]));
                    // A triangle repeats its last corner as the fourth.
                    w.point(10, *p);
                    w.point(11, *q);
                    w.point(12, *s);
                    w.point(13, *s);
                }
            }
            // R12 has no images: a picture is written as its frame.
            Prim::Image { origin, u, v, .. } => {
                let corners = image_frame(*origin, *u, *v).map(at);
                if !corners.iter().all(|p| p.is_finite()) {
                    continue;
                }
                for i in 0..4 {
                    w.pair(0, "LINE");
                    w.pair(8, &layer);
                    w.pair(62, 8);
                    w.point(10, corners[i]);
                    w.point(11, corners[(i + 1) % 4]);
                }
            }
            Prim::Text { pos, text, size, anchor, angle, .. } => {
                let value = text_value(text);
                let p = at(*pos);
                if value.trim().is_empty() || !p.is_finite() || !(size.is_finite() && *size > 0.0) {
                    continue;
                }
                // Horizontal (left, center) and vertical (bottom, middle) alignment.
                let (h, v) = match anchor {
                    Anchor::LeftBottom => (0, 1),
                    Anchor::CenterBottom => (1, 1),
                    Anchor::Center => (1, 2),
                };
                w.pair(0, "TEXT");
                w.pair(8, &layer);
                w.pair(62, 7);
                w.point(10, p);
                w.pair(40, num(size * paper));
                w.pair(1, value);
                let angle = if angle.is_finite() { angle.to_degrees() } else { 0.0 };
                if angle.abs() > 1e-9 {
                    w.pair(50, num(angle));
                }
                w.pair(72, h);
                w.point(11, p);
                w.pair(73, v);
            }
        }
    }
    w.pair(0, "ENDSEC");
    w.pair(0, "EOF");
    encode(&w.out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The group code / value pairs of a DXF document.
    fn pairs(bytes: &[u8]) -> Vec<(u16, String)> {
        // Code page 1252 read back as Latin-1 (the same for the characters tested).
        let s: String = bytes.iter().map(|&b| char::from(b)).collect();
        let lines: Vec<&str> = s.lines().collect();
        lines.chunks(2).map(|c| (c[0].trim().parse().unwrap(), c[1].to_string())).collect()
    }

    #[test]
    fn drawings_become_lines_solids_and_texts_on_their_layers() {
        let mut d = Drawing2D::default();
        let line = |a: DVec2, b: DVec2, style| Prim::Line { a, b, style, element: None, pen: None };
        d.prims.push(line(DVec2::ZERO, DVec2::new(4.0, 0.0), LineStyle::Cut));
        d.prims.push(line(DVec2::ZERO, DVec2::new(0.0, 3.0), LineStyle::Hidden));
        d.prims.push(line(DVec2::ZERO, DVec2::new(2.0, 2.0), LineStyle::Center));
        let square = vec![DVec2::ZERO, DVec2::new(1.0, 0.0), DVec2::new(1.0, 1.0), DVec2::new(0.0, 1.0)];
        d.prims.push(Prim::Fill { pts: square.clone(), color: [40, 40, 40, 255], element: None, cut: true });
        d.prims.push(Prim::Fill { pts: square, color: [200, 0, 0, 40], element: None, cut: false });
        d.prims.push(Prim::Text {
            pos: DVec2::new(1.0, 1.0),
            text: "Área 12,5 m² ∅".into(),
            size: 2.5,
            anchor: Anchor::Center,
            angle: std::f64::consts::FRAC_PI_2,
            element: None,
        });
        let dxf = to_dxf(&d, DxfScale::model(50), &|p| match p {
            Prim::Fill { .. } => "Wall".into(),
            p => default_layer(p).into(),
        });
        let p = pairs(&dxf);
        let count = |name: &str| p.iter().filter(|(c, v)| *c == 0 && v == name).count();
        assert_eq!((count("LINE"), count("SOLID"), count("TEXT")), (3, 2, 1), "the faint fill is left out");
        assert_eq!(p.last(), Some(&(0, "EOF".to_string())));
        // Meters at 1:50 become millimeters at full size; text 2.5 mm on paper is 125 mm.
        assert!(p.windows(2).any(|w| w[0] == (11, "4000".into()) && w[1] == (21, "0".into())));
        assert!(p.contains(&(40, "125".into())) && p.contains(&(50, "90".into())));
        // Latin letters in the code page, others escaped.
        assert!(p.contains(&(1, "Área 12,5 m² \\U+2205".into())), "{:?}", p.iter().find(|(c, _)| *c == 1));
        assert_eq!(encode("€Ÿ"), vec![0x80, 0x9F]);
        // The hidden and center lines have line types, defined in the table.
        let types: Vec<&String> = p.iter().filter(|(c, v)| *c == 6 && v != "CONTINUOUS").map(|(_, v)| v).collect();
        assert_eq!(types.len(), 2);
        for t in types {
            assert!(p.iter().any(|(c, v)| *c == 2 && v == t), "{t} is defined");
        }
        for layer in ["ANNOTATION", "DRAWING", "TEXT", "WALL"] {
            assert!(p.contains(&(2, layer.into())), "{layer}");
        }
        assert_eq!(aci([10, 10, 10]), 7);
        // A picture becomes its frame.
        let mut d = Drawing2D::default();
        let image = std::sync::Arc::new(cc_core::raster::RasterImage {
            format: cc_core::raster::RasterFormat::Png,
            width: 1,
            height: 1,
            data: Vec::new(),
        });
        d.prims.push(Prim::Image {
            origin: DVec2::ZERO,
            u: DVec2::new(2.0, 0.0),
            v: DVec2::new(0.0, 1.0),
            image,
            element: None,
            faded: false,
        });
        let p = pairs(&to_dxf(&d, DxfScale::PAPER, &|p| default_layer(p).into()));
        assert_eq!(p.iter().filter(|(c, v)| *c == 0 && v == "LINE").count(), 4);
        assert_eq!(layer_name("Sala de estar/1"), "SALA_DE_ESTAR_1");
    }
}
