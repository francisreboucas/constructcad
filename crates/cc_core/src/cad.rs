//! Imported CAD drawings (DWG and DXF underlays).
//!
//! A [`CadDrawing`] is the neutral form `cc_cad` reads both formats into:
//! layers and flat primitives (blocks already inserted) in drawing units.
//! Arcs stay arcs (centers and ends can be snapped to) and are tessellated
//! only when drawn. A [`CadImport`] element places one drawing in one 2D
//! view: units to meters, an extra scale, a rotation and an insertion point.
//! The drawing is shared (`Arc`), so undo steps that change the placement or
//! the layers do not copy it.

use crate::ViewId;
use crate::transform::PlanTransform;
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::f64::consts::TAU;

/// Most primitives one drawing may hold (after blocks are inserted).
pub const MAX_PRIMS: usize = 1_000_000;
/// Most points of one polyline or fill.
pub const MAX_POLY_POINTS: usize = 100_000;
/// Longest text kept, in characters.
pub const MAX_TEXT_CHARS: usize = 2_000;
/// Largest coordinate magnitude accepted, drawing units.
pub const MAX_COORD: f64 = 1e12;

fn yes() -> bool {
    true
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// Units a drawing is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum CadUnit {
    #[default]
    Millimeters,
    Centimeters,
    Decimeters,
    Meters,
    Kilometers,
    Inches,
    Feet,
    Yards,
    Miles,
}

impl CadUnit {
    pub const ALL: [CadUnit; 9] = [
        CadUnit::Millimeters,
        CadUnit::Centimeters,
        CadUnit::Decimeters,
        CadUnit::Meters,
        CadUnit::Kilometers,
        CadUnit::Inches,
        CadUnit::Feet,
        CadUnit::Yards,
        CadUnit::Miles,
    ];

    /// Meters in one unit.
    pub fn meters(self) -> f64 {
        match self {
            CadUnit::Millimeters => 0.001,
            CadUnit::Centimeters => 0.01,
            CadUnit::Decimeters => 0.1,
            CadUnit::Meters => 1.0,
            CadUnit::Kilometers => 1000.0,
            CadUnit::Inches => 0.0254,
            CadUnit::Feet => 0.3048,
            CadUnit::Yards => 0.9144,
            CadUnit::Miles => 1609.344,
        }
    }

    /// The unit of a DXF `$INSUNITS` code (DWG `INSUNITS` uses the same
    /// codes); `None` for unitless (0) and units not listed here.
    pub fn from_insunits(code: i64) -> Option<CadUnit> {
        Some(match code {
            1 => CadUnit::Inches,
            2 => CadUnit::Feet,
            3 => CadUnit::Miles,
            4 => CadUnit::Millimeters,
            5 => CadUnit::Centimeters,
            6 => CadUnit::Meters,
            7 => CadUnit::Kilometers,
            10 => CadUnit::Yards,
            14 => CadUnit::Decimeters,
            _ => return None,
        })
    }

    /// Short name used by commands (`mm`, `cm`, `dm`, `m`, `km`, `in`, `ft`, `yd`, `mi`).
    pub fn code(self) -> &'static str {
        match self {
            CadUnit::Millimeters => "mm",
            CadUnit::Centimeters => "cm",
            CadUnit::Decimeters => "dm",
            CadUnit::Meters => "m",
            CadUnit::Kilometers => "km",
            CadUnit::Inches => "in",
            CadUnit::Feet => "ft",
            CadUnit::Yards => "yd",
            CadUnit::Miles => "mi",
        }
    }

    pub fn from_code(code: &str) -> Option<CadUnit> {
        let code = code.trim().to_ascii_lowercase();
        CadUnit::ALL.into_iter().find(|u| u.code() == code || format!("{u:?}").eq_ignore_ascii_case(&code))
    }

    /// Translation key of the unit name.
    pub fn key(self) -> &'static str {
        match self {
            CadUnit::Millimeters => "cad-unit-millimeters",
            CadUnit::Centimeters => "cad-unit-centimeters",
            CadUnit::Decimeters => "cad-unit-decimeters",
            CadUnit::Meters => "cad-unit-meters",
            CadUnit::Kilometers => "cad-unit-kilometers",
            CadUnit::Inches => "cad-unit-inches",
            CadUnit::Feet => "cad-unit-feet",
            CadUnit::Yards => "cad-unit-yards",
            CadUnit::Miles => "cad-unit-miles",
        }
    }
}

/// How an imported drawing is colored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum CadColorMode {
    /// One grey for everything: a background to trace over.
    #[default]
    Grey,
    /// The colors of the layers and entities (white and very light colors darkened for white paper).
    Layers,
    /// Black, like the model's linework.
    Black,
}

impl CadColorMode {
    pub const ALL: [CadColorMode; 3] = [CadColorMode::Grey, CadColorMode::Layers, CadColorMode::Black];

    pub fn key(self) -> &'static str {
        match self {
            CadColorMode::Grey => "cad-color-grey",
            CadColorMode::Layers => "cad-color-layers",
            CadColorMode::Black => "cad-color-black",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            CadColorMode::Grey => "grey",
            CadColorMode::Layers => "layers",
            CadColorMode::Black => "black",
        }
    }

    pub fn from_code(code: &str) -> Option<CadColorMode> {
        let code = code.trim().to_ascii_lowercase();
        let code = if code == "gray" { "grey".to_string() } else { code };
        CadColorMode::ALL.into_iter().find(|m| m.code() == code)
    }
}

/// A layer of the drawing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CadLayer {
    pub name: String,
    /// The layer color (from its color index or true color).
    pub color: [u8; 3],
    /// On and thawed in the file.
    #[serde(default = "yes")]
    pub visible: bool,
}

/// Horizontal alignment of a text at its position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum CadHAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Vertical alignment of a text at its position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum CadVAlign {
    #[default]
    Baseline,
    Bottom,
    Middle,
    Top,
}

/// A primitive of a drawing, in drawing units. `layer` indexes
/// [`CadDrawing::layers`]; `color` is the entity's own color (`None`: the
/// layer's).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CadPrim {
    /// Polyline through `pts`; `bulges[i]` bends the segment from point `i`
    /// (`tan(sweep / 4)`, positive counter-clockwise; empty: all straight).
    Polyline {
        layer: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<[u8; 3]>,
        #[serde(default, skip_serializing_if = "is_false")]
        closed: bool,
        pts: Vec<DVec2>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        bulges: Vec<f64>,
    },
    /// Circular arc from `start` (radians) turning by `sweep` (positive
    /// counter-clockwise; ±τ for a circle).
    Arc {
        layer: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<[u8; 3]>,
        center: DVec2,
        radius: f64,
        start: f64,
        sweep: f64,
    },
    /// One line of text `height` tall, rotated by `rotation` radians.
    Text {
        layer: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<[u8; 3]>,
        pos: DVec2,
        height: f64,
        #[serde(default)]
        rotation: f64,
        text: String,
        #[serde(default)]
        h: CadHAlign,
        #[serde(default)]
        v: CadVAlign,
    },
    /// Solid fill of a simple outline.
    Fill {
        layer: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<[u8; 3]>,
        pts: Vec<DVec2>,
    },
}

impl CadPrim {
    pub fn layer(&self) -> u32 {
        match self {
            CadPrim::Polyline { layer, .. }
            | CadPrim::Arc { layer, .. }
            | CadPrim::Text { layer, .. }
            | CadPrim::Fill { layer, .. } => *layer,
        }
    }

    pub fn color(&self) -> Option<[u8; 3]> {
        match self {
            CadPrim::Polyline { color, .. }
            | CadPrim::Arc { color, .. }
            | CadPrim::Text { color, .. }
            | CadPrim::Fill { color, .. } => *color,
        }
    }

    /// Bounding box in drawing units (arcs by their full circle when that is simpler).
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let pts: &[DVec2] = match self {
            CadPrim::Polyline { pts, bulges, .. } => {
                let mut b = bounds_of(pts)?;
                // Bulged segments: their whole circle (a safe, simple box).
                let n = pts.len();
                for (i, bulge) in bulges.iter().enumerate().filter(|(_, v)| v.abs() > 1e-9) {
                    let (Some(a), Some(c)) = (pts.get(i), pts.get((i + 1) % n)) else { continue };
                    if let Some(arc) = crate::arc::Arc::from_bulge(*a, *c, *bulge) {
                        let r = DVec2::splat(arc.radius);
                        b = (b.0.min(arc.center - r), b.1.max(arc.center + r));
                    }
                }
                return Some(b);
            }
            CadPrim::Arc { center, radius, .. } => {
                return Some((*center - DVec2::splat(*radius), *center + DVec2::splat(*radius)));
            }
            CadPrim::Text { pos, height, text, rotation, h, .. } => {
                let w = text_width(text, *height);
                let dir = DVec2::from_angle(*rotation);
                let shift = match h {
                    CadHAlign::Left => 0.0,
                    CadHAlign::Center => -0.5 * w,
                    CadHAlign::Right => -w,
                };
                let a = *pos + dir * shift;
                let b = a + dir * w;
                let up = dir.perp() * *height;
                let corners = [a - up, b - up, a + up, b + up];
                return bounds_of(&corners);
            }
            CadPrim::Fill { pts, .. } => pts,
        };
        bounds_of(pts)
    }

    /// Whether every number is finite and within [`MAX_COORD`], with enough points.
    pub fn is_valid(&self) -> bool {
        let ok = |p: &DVec2| p.is_finite() && p.abs().max_element() <= MAX_COORD;
        match self {
            CadPrim::Polyline { pts, bulges, .. } => {
                pts.len() >= 2
                    && pts.len() <= MAX_POLY_POINTS
                    && pts.iter().all(ok)
                    && bulges.len() <= pts.len()
                    && bulges.iter().all(|b| b.is_finite())
            }
            CadPrim::Arc { center, radius, start, sweep, .. } => {
                ok(center)
                    && radius.is_finite()
                    && *radius > 0.0
                    && *radius <= MAX_COORD
                    && start.is_finite()
                    && sweep.is_finite()
                    && sweep.abs() > 1e-12
            }
            CadPrim::Text { pos, height, rotation, text, .. } => {
                ok(pos)
                    && height.is_finite()
                    && *height > 0.0
                    && *height <= MAX_COORD
                    && rotation.is_finite()
                    && !text.is_empty()
                    && text.chars().count() <= MAX_TEXT_CHARS
            }
            CadPrim::Fill { pts, .. } => pts.len() >= 3 && pts.len() <= MAX_POLY_POINTS && pts.iter().all(ok),
        }
    }
}

/// Approximate width of a text line (the proportion used for notes).
pub fn text_width(text: &str, height: f64) -> f64 {
    text.chars().count() as f64 * height * 0.6
}

fn bounds_of(pts: &[DVec2]) -> Option<(DVec2, DVec2)> {
    let first = *pts.first()?;
    Some(pts.iter().fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))))
}

/// Merge two optional boxes.
pub fn union(a: Option<(DVec2, DVec2)>, b: Option<(DVec2, DVec2)>) -> Option<(DVec2, DVec2)> {
    match (a, b) {
        (Some(a), Some(b)) => Some((a.0.min(b.0), a.1.max(b.1))),
        (a, None) => a,
        (None, b) => b,
    }
}

/// A raster image laid on the drawing: pixel (0, 0) is the top-left corner
/// of the parallelogram `origin + s·u + t·v` (`s`, `t` in 0..1), whose
/// bottom-left corner is `origin`, `u` running along the image's width and
/// `v` up its height (drawing units).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CadImage {
    pub layer: u32,
    pub origin: DVec2,
    pub u: DVec2,
    pub v: DVec2,
    pub image: std::sync::Arc<crate::raster::RasterImage>,
}

impl CadImage {
    /// Corners in order: bottom-left, bottom-right, top-right, top-left.
    pub fn corners(&self) -> [DVec2; 4] {
        [self.origin, self.origin + self.u, self.origin + self.u + self.v, self.origin + self.v]
    }

    /// Whether the placement is finite, not flat, and the image usable.
    pub fn is_valid(&self) -> bool {
        let ok = |p: DVec2| p.is_finite() && p.abs().max_element() <= MAX_COORD;
        ok(self.origin) && ok(self.u) && ok(self.v) && self.u.perp_dot(self.v).abs() > 1e-12 && self.image.is_valid()
    }

    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        bounds_of(&self.corners())
    }
}

/// Most images one drawing may hold.
pub const MAX_IMAGES: usize = 1_000;

/// A drawing read from a DWG, DXF, SVG or PDF file, or an imported picture.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CadDrawing {
    pub layers: Vec<CadLayer>,
    pub prims: Vec<CadPrim>,
    /// Pictures, drawn under the primitives.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<CadImage>,
    /// Extents of the primitives (min, max), drawing units.
    #[serde(default)]
    pub extents: Option<(DVec2, DVec2)>,
    /// Units the file declares (`None`: unitless).
    #[serde(default)]
    pub units: Option<CadUnit>,
}

impl CadDrawing {
    /// Recompute [`CadDrawing::extents`] from the primitives.
    pub fn compute_extents(&mut self) {
        let prims = self.prims.iter().fold(None, |acc, p| union(acc, p.bounds()));
        self.extents = self.images.iter().fold(prims, |acc, i| union(acc, i.bounds()));
    }

    /// The layer of a primitive.
    pub fn layer_of(&self, prim: &CadPrim) -> Option<&CadLayer> {
        self.layers.get(prim.layer() as usize)
    }

    /// Whether [`CadDrawing::repair`] would change anything.
    pub fn needs_repair(&self) -> bool {
        let layers = self.layers.len() as u32;
        let extents_ok = match self.extents {
            Some((lo, hi)) => lo.is_finite() && hi.is_finite() && lo.cmple(hi).all(),
            None => self.prims.is_empty() && self.images.is_empty(),
        };
        self.layers.is_empty()
            || !extents_ok
            || self.prims.len() > MAX_PRIMS
            || self.prims.iter().any(|p| p.layer() >= layers || !p.is_valid())
            || self.images.len() > MAX_IMAGES
            || self.images.iter().any(|i| i.layer >= layers || !i.is_valid())
    }

    /// Remove invalid primitives and dangling layer references; returns how many were fixed.
    pub fn repair(&mut self) -> usize {
        let before = self.prims.len();
        if self.prims.len() > MAX_PRIMS {
            self.prims.truncate(MAX_PRIMS);
        }
        if self.layers.is_empty() {
            self.layers.push(CadLayer { name: "0".into(), color: [0, 0, 0], visible: true });
        }
        let layers = self.layers.len() as u32;
        let mut fixed = 0;
        for p in &mut self.prims {
            let layer = match p {
                CadPrim::Polyline { layer, .. }
                | CadPrim::Arc { layer, .. }
                | CadPrim::Text { layer, .. }
                | CadPrim::Fill { layer, .. } => layer,
            };
            if *layer >= layers {
                *layer = 0;
                fixed += 1;
            }
        }
        self.prims.retain(CadPrim::is_valid);
        fixed += before - self.prims.len();
        let images = self.images.len();
        self.images.truncate(MAX_IMAGES);
        for image in &mut self.images {
            if image.layer >= layers {
                image.layer = 0;
                fixed += 1;
            }
        }
        self.images.retain(CadImage::is_valid);
        fixed += images - self.images.len();
        let extents_ok = self.extents.is_none_or(|(lo, hi)| lo.is_finite() && hi.is_finite() && lo.cmple(hi).all());
        let empty = self.prims.is_empty() && self.images.is_empty();
        if !extents_ok || (self.extents.is_none() && !empty) || fixed > 0 {
            if !extents_ok {
                fixed += 1;
            }
            self.compute_extents();
        }
        fixed
    }
}

/// A DWG or DXF drawing placed in one 2D view, in that view's drawing
/// coordinates (like text notes): drawing point `p` lands at
/// `position + R(rotation)·M·(p · units · scale)`, `M` mirroring across
/// the drawing's X axis when `mirrored`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CadImport {
    pub view: ViewId,
    /// Name of the file it was read from (for display).
    pub name: String,
    pub drawing: std::sync::Arc<CadDrawing>,
    /// Units of the drawing.
    #[serde(default)]
    pub units: CadUnit,
    /// Extra scale on top of the units.
    #[serde(default = "one")]
    pub scale: f64,
    /// Where the drawing's origin lands, view coordinates (meters).
    pub position: DVec2,
    /// Rotation in degrees counter-clockwise.
    #[serde(default)]
    pub rotation_deg: f64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub mirrored: bool,
    /// Layers hidden in this view (by name).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub hidden_layers: BTreeSet<String>,
    #[serde(default)]
    pub color_mode: CadColorMode,
    /// Drawn faded (mixed with white).
    #[serde(default)]
    pub halftone: bool,
}

fn one() -> f64 {
    1.0
}

impl CadImport {
    /// Meters per drawing unit, scale included.
    pub fn factor(&self) -> f64 {
        self.units.meters() * self.scale
    }

    /// The rigid part of the placement (rotation, mirror, position).
    pub fn placement(&self) -> PlanTransform {
        PlanTransform { angle: self.rotation_deg.to_radians(), mirror: self.mirrored, translation: self.position }
    }

    /// A drawing point in view coordinates.
    pub fn to_view(&self, p: DVec2) -> DVec2 {
        self.placement().apply(p * self.factor())
    }

    /// A view point in drawing units (inverse of [`CadImport::to_view`]).
    pub fn to_drawing(&self, q: DVec2) -> DVec2 {
        let f = self.factor();
        let local = DVec2::from_angle(-self.rotation_deg.to_radians()).rotate(q - self.position);
        let local = if self.mirrored { DVec2::new(local.x, -local.y) } else { local };
        if f.abs() > 0.0 { local / f } else { local }
    }

    /// Whether layer `index` is shown.
    pub fn layer_visible(&self, index: u32) -> bool {
        self.drawing.layers.get(index as usize).is_none_or(|l| !self.hidden_layers.contains(&l.name))
    }

    /// Move, rotate or mirror the placement.
    pub fn transform(&mut self, t: &PlanTransform) {
        let placed = self.placement().then(*t);
        self.position = placed.translation;
        self.rotation_deg = placed.angle.to_degrees().rem_euclid(360.0);
        self.mirrored = placed.mirror;
    }

    /// Whether the placement's numbers are usable.
    pub fn is_valid(&self) -> bool {
        self.position.is_finite()
            && self.rotation_deg.is_finite()
            && self.scale.is_finite()
            && self.scale > 1e-9
            && self.scale < 1e9
    }

    /// Snap features of the visible primitives near `q` (view coordinates)
    /// within `r`: endpoints and vertices, midpoints, arc centers, and
    /// segments, all in view coordinates.
    pub fn snap_features(&self, q: DVec2, r: f64) -> CadSnap {
        let mut out = CadSnap::default();
        let f = self.factor();
        if !(f.is_finite() && f > 0.0 && r.is_finite() && r > 0.0) {
            return out;
        }
        let local = self.to_drawing(q);
        let lr = r / f;
        let near = |b: Option<(DVec2, DVec2)>| {
            b.is_some_and(|(lo, hi)| {
                local.x >= lo.x - lr && local.y >= lo.y - lr && local.x <= hi.x + lr && local.y <= hi.y + lr
            })
        };
        let to = |p: DVec2| self.to_view(p);
        for prim in &self.drawing.prims {
            if !self.layer_visible(prim.layer()) || !near(prim.bounds()) {
                continue;
            }
            match prim {
                CadPrim::Polyline { pts, bulges, closed, .. } => {
                    let n = pts.len();
                    let segs = if *closed { n } else { n.saturating_sub(1) };
                    for i in 0..segs {
                        let (Some(a), Some(b)) = (pts.get(i), pts.get((i + 1) % n)) else { continue };
                        let bulge = bulges.get(i).copied().unwrap_or(0.0);
                        match crate::arc::Arc::from_bulge(*a, *b, bulge) {
                            Some(arc) => {
                                out.ends.extend([to(*a), to(*b)]);
                                out.mids.push(to(arc.point(arc.length() * 0.5)));
                                out.centers.push(to(arc.center));
                                out.arcs.push(self.arc_to_view(arc.center, arc.radius, arc.start_angle, arc.sweep));
                            }
                            None => {
                                out.ends.extend([to(*a), to(*b)]);
                                out.mids.push(to((*a + *b) * 0.5));
                                out.lines.push((to(*a), to(*b)));
                            }
                        }
                    }
                }
                CadPrim::Arc { center, radius, start, sweep, .. } => {
                    out.centers.push(to(*center));
                    if sweep.abs() < TAU - 1e-9 {
                        let p = |a: f64| *center + DVec2::from_angle(a) * *radius;
                        out.ends.extend([to(p(*start)), to(p(start + sweep))]);
                        out.mids.push(to(p(start + sweep * 0.5)));
                    } else {
                        // Quadrant points of a circle.
                        for k in 0..4 {
                            out.ends.push(to(*center + DVec2::from_angle(k as f64 * TAU / 4.0) * *radius));
                        }
                    }
                    out.arcs.push(self.arc_to_view(*center, *radius, *start, *sweep));
                }
                CadPrim::Text { pos, .. } => out.ends.push(to(*pos)),
                CadPrim::Fill { pts, .. } => {
                    out.ends.extend(pts.iter().map(|p| to(*p)));
                    let n = pts.len();
                    for i in 0..n {
                        if let (Some(a), Some(b)) = (pts.get(i), pts.get((i + 1) % n)) {
                            out.lines.push((to(*a), to(*b)));
                        }
                    }
                }
            }
        }
        out
    }

    /// An arc of the drawing in view coordinates.
    fn arc_to_view(&self, center: DVec2, radius: f64, start: f64, sweep: f64) -> crate::arc::Arc {
        let rot = self.rotation_deg.to_radians();
        let (start, sweep) = if self.mirrored { (-start, -sweep) } else { (start, sweep) };
        crate::arc::Arc {
            center: self.to_view(center),
            radius: radius * self.factor(),
            start_angle: start + rot,
            sweep,
        }
    }
}

/// Snap features of an imported drawing near a point, view coordinates.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CadSnap {
    pub ends: Vec<DVec2>,
    pub mids: Vec<DVec2>,
    pub centers: Vec<DVec2>,
    pub lines: Vec<(DVec2, DVec2)>,
    pub arcs: Vec<crate::arc::Arc>,
}

/// RGB of an AutoCAD Color Index (the standard 256-entry palette: 1-9 the
/// named colors, 10-249 hue/saturation steps, 250-255 greys); 0 and 256
/// (by block / by layer) and 7 come out black for white paper.
pub fn aci_color(index: i64) -> [u8; 3] {
    let i = index.unsigned_abs();
    match i {
        1 => [255, 0, 0],
        2 => [255, 255, 0],
        3 => [0, 255, 0],
        4 => [0, 255, 255],
        5 => [0, 0, 255],
        6 => [255, 0, 255],
        8 => [128, 128, 128],
        9 => [192, 192, 192],
        10..=249 => {
            // Hue in 24 steps of 15°, then 5 brightness levels × (full, half saturation).
            let hue = ((i - 10) / 10) as f64 * 15.0;
            let k = (i - 10) % 10;
            let value = [1.0, 1.0, 0.8, 0.8, 0.6, 0.6, 0.5, 0.5, 0.3, 0.3][k as usize];
            let sat = if k.is_multiple_of(2) { 1.0 } else { 0.5 };
            hsv(hue, sat, value)
        }
        250..=255 => {
            let g = [51, 91, 132, 173, 214, 255][(i - 250) as usize];
            [g, g, g]
        }
        _ => [0, 0, 0],
    }
}

fn hsv(h: f64, s: f64, v: f64) -> [u8; 3] {
    let c = v * s;
    let hp = (h / 60.0).rem_euclid(6.0);
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r, g, b].map(|t| ((t + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import(position: DVec2, rotation_deg: f64, units: CadUnit, scale: f64) -> CadImport {
        CadImport {
            view: ViewId(1),
            name: "a.dxf".into(),
            drawing: Default::default(),
            units,
            scale,
            position,
            rotation_deg,
            mirrored: false,
            hidden_layers: Default::default(),
            color_mode: Default::default(),
            halftone: false,
        }
    }

    #[test]
    fn placement_round_trips() {
        let mut c = import(DVec2::new(5.0, -2.0), 30.0, CadUnit::Millimeters, 2.0);
        let p = DVec2::new(1000.0, 500.0);
        let q = c.to_view(p);
        assert!((c.to_drawing(q) - p).length() < 1e-9);
        // 1000 mm × 2 along a direction turned 30°.
        let o = c.to_view(DVec2::ZERO);
        assert!((o - DVec2::new(5.0, -2.0)).length() < 1e-12);
        let x = c.to_view(DVec2::new(1000.0, 0.0)) - o;
        assert!((x - DVec2::from_angle(30f64.to_radians()) * 2.0).length() < 1e-12);
        c.mirrored = true;
        let q = c.to_view(p);
        assert!((c.to_drawing(q) - p).length() < 1e-9);
    }

    #[test]
    fn transforms_compose() {
        let mut c = import(DVec2::new(1.0, 0.0), 0.0, CadUnit::Meters, 1.0);
        let p = DVec2::new(2.0, 1.0);
        let t = PlanTransform::rotate_about(DVec2::ZERO, std::f64::consts::FRAC_PI_2);
        let expected = t.apply(c.to_view(p));
        c.transform(&t);
        assert!((c.to_view(p) - expected).length() < 1e-12);
        let m = PlanTransform::mirror_across(DVec2::ZERO, DVec2::Y);
        let expected = m.apply(c.to_view(p));
        c.transform(&m);
        assert!((c.to_view(p) - expected).length() < 1e-12);
    }

    #[test]
    fn aci_palette_basics() {
        assert_eq!(aci_color(1), [255, 0, 0]);
        assert_eq!(aci_color(7), [0, 0, 0]);
        assert_eq!(aci_color(10), [255, 0, 0]);
        assert_eq!(aci_color(250), [51, 51, 51]);
        assert_eq!(aci_color(-5), [0, 0, 255], "a negative index is a layer turned off");
    }

    #[test]
    fn repair_drops_bad_primitives() {
        let mut d = CadDrawing {
            layers: vec![],
            prims: vec![
                CadPrim::Polyline {
                    layer: 3,
                    color: None,
                    closed: false,
                    pts: vec![DVec2::ZERO, DVec2::X],
                    bulges: vec![],
                },
                CadPrim::Arc { layer: 0, color: None, center: DVec2::ZERO, radius: f64::NAN, start: 0.0, sweep: 1.0 },
            ],
            extents: None,
            units: None,
            images: vec![],
        };
        assert_eq!(d.repair(), 2);
        assert_eq!(d.prims.len(), 1);
        assert_eq!(d.prims[0].layer(), 0);
        assert_eq!(d.layers.len(), 1);
        assert_eq!(d.extents, Some((DVec2::ZERO, DVec2::X)));
    }

    #[test]
    fn repair_drops_bad_images() {
        use crate::raster::{RasterFormat, RasterImage};
        const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let png = RasterImage { format: RasterFormat::Png, width: 4, height: 2, data: PNG_SIGNATURE.to_vec() };
        let image = |layer, u: DVec2, image: RasterImage| CadImage {
            layer,
            origin: DVec2::ZERO,
            u,
            v: DVec2::Y,
            image: std::sync::Arc::new(image),
        };
        let mut d = CadDrawing {
            layers: vec![CadLayer { name: "0".into(), color: [0, 0, 0], visible: true }],
            images: vec![
                image(0, DVec2::new(2.0, 0.0), png.clone()),
                image(5, DVec2::X, png.clone()),
                image(0, DVec2::new(f64::NAN, 0.0), png.clone()),
                image(0, DVec2::Y, png.clone()),
                image(0, DVec2::X, RasterImage { format: RasterFormat::Jpeg, ..png }),
            ],
            ..Default::default()
        };
        assert!(d.needs_repair());
        // A dangling layer reset, a NaN, a flat and a mislabeled image dropped.
        assert_eq!(d.repair(), 4);
        assert_eq!(d.images.len(), 2);
        assert_eq!(d.extents, Some((DVec2::ZERO, DVec2::new(2.0, 1.0))));
        assert!(!d.needs_repair());
    }

    #[test]
    fn snaps_to_ends_centers_and_lines() {
        let mut c = import(DVec2::new(10.0, 0.0), 90.0, CadUnit::Millimeters, 1.0);
        c.drawing = std::sync::Arc::new(CadDrawing {
            layers: vec![CadLayer { name: "0".into(), color: [0, 0, 0], visible: true }],
            prims: vec![
                CadPrim::Polyline {
                    layer: 0,
                    color: None,
                    closed: false,
                    pts: vec![DVec2::ZERO, DVec2::new(1000.0, 0.0)],
                    bulges: vec![],
                },
                CadPrim::Arc {
                    layer: 0,
                    color: None,
                    center: DVec2::new(0.0, 2000.0),
                    radius: 500.0,
                    start: 0.0,
                    sweep: TAU,
                },
            ],
            extents: None,
            units: Some(CadUnit::Millimeters),
            images: vec![],
        });
        let s = c.snap_features(DVec2::new(10.0, 1.0), 0.1);
        // The line's end at (1000, 0) mm lands 1 m north of the position.
        assert!(s.ends.iter().any(|p| (*p - DVec2::new(10.0, 1.0)).length() < 1e-12), "{s:?}");
        assert_eq!(s.lines.len(), 1);
        assert!(s.centers.is_empty(), "the circle is far away");
        let s = c.snap_features(DVec2::new(8.0, 0.0), 0.6);
        assert!(s.centers.iter().any(|p| (*p - DVec2::new(8.0, 0.0)).length() < 1e-12), "{s:?}");
        c.hidden_layers.insert("0".into());
        assert!(c.snap_features(DVec2::new(8.0, 0.0), 0.6).centers.is_empty());
    }
}
