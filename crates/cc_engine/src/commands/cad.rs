//! Imported CAD drawings (DWG, DXF and SVG underlays, and pictures):
//! importing a file into a 2D view and showing or hiding its layers. The placement (units, scale,
//! position, rotation, colors) is edited with `element_set`; moving,
//! rotating, pinning and deleting use the generic commands.

use super::{created, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::{Session, guard};
use crate::t;
use cc_core::cad::{CadColorMode, CadDrawing, CadImport, CadUnit};
use cc_core::{Category, ElementId, ElementKind, ElementType, ElementTypeId, TypeData};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

/// The element kind without its drawing (what `params` show and patch):
/// the drawing is data read from the file, not a parameter.
pub(crate) fn light(kind: &ElementKind) -> std::borrow::Cow<'_, ElementKind> {
    match kind {
        ElementKind::CadImport(c) => {
            let mut c = c.clone();
            c.drawing = Arc::new(CadDrawing::default());
            std::borrow::Cow::Owned(ElementKind::CadImport(c))
        }
        other => std::borrow::Cow::Borrowed(other),
    }
}

/// `params` of an element kind (its variant's fields), without an imported drawing.
pub fn params(kind: &ElementKind) -> Value {
    let mut v = super::variant(serde_json::to_value(light(kind).as_ref()).unwrap_or_default()).1;
    if matches!(kind, ElementKind::CadImport(_))
        && let Some(o) = v.as_object_mut()
    {
        o.remove("drawing");
    }
    v
}

/// Apply a `params` patch to an imported drawing's element, keeping its drawing.
pub(crate) fn patch(c: &CadImport, patch: &Value) -> Result<ElementKind> {
    // `element_get` shows the view, so a patch built from it repeats it unchanged.
    let view_changed = patch.get("view").is_some_and(|v| v.as_u64() != Some(c.view.0));
    if patch.get("drawing").is_some() || view_changed {
        return Err(EngineError::invalid("the drawing and the view of an imported drawing cannot be changed"));
    }
    let kind = super::patch_variant(light(&ElementKind::CadImport(Box::new(c.clone()))).as_ref(), patch)?;
    match kind {
        ElementKind::CadImport(mut new) => {
            if !new.is_valid() {
                return Err(EngineError::invalid("scale, position and rotation must be finite (scale positive)"));
            }
            new.drawing = c.drawing.clone();
            Ok(ElementKind::CadImport(new))
        }
        _ => Err(EngineError::invalid("params cannot change the element's category")),
    }
}

/// Derived values of an imported drawing for `element_get`.
pub(crate) fn derived(c: &CadImport, out: &mut serde_json::Map<String, Value>) {
    let d = &c.drawing;
    out.insert("layers".into(), json!(d.layers.len()));
    out.insert("hidden_layers".into(), json!(c.hidden_layers.len()));
    out.insert("primitives".into(), json!(d.prims.len()));
    out.insert("file_units".into(), json!(d.units.map(CadUnit::code)));
    out.insert("meters_per_unit".into(), json!(c.factor()));
    if let Some(ext) = view_extents(c) {
        out.insert("extents".into(), ext);
    }
}

/// The drawing's extents placed in the view: `[[xmin, ymin], [xmax, ymax]]`.
fn view_extents(c: &CadImport) -> Option<Value> {
    let (lo, hi) = c.drawing.extents?;
    let corners = [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)].map(|p| c.to_view(p));
    let (a, b) = corners.iter().fold((corners[0], corners[0]), |(a, b), p| (a.min(*p), b.max(*p)));
    Some(json!([[a.x, a.y], [b.x, b.y]]))
}

/// Units for a drawing whose file declares none: meters when it is small
/// (a building drawn in meters spans tens of units), else millimeters.
fn guess_units(d: &CadDrawing) -> CadUnit {
    match d.extents {
        Some((lo, hi)) if (hi - lo).max_element() <= 1500.0 => CadUnit::Meters,
        _ => CadUnit::Millimeters,
    }
}

/// The imported drawing type, creating one if the project has none.
fn cad_type(e: &mut cc_core::EditCtx) -> ElementTypeId {
    if let Some(t) = e.project().default_type(Category::CadImport) {
        return t;
    }
    let id = ElementTypeId(e.alloc_id());
    e.add_type(ElementType {
        id,
        name: t!("tpl-type-cad-import"),
        data: TypeData::CadImport {},
        info: Default::default(),
        uid: Default::default(),
    })
}

fn units_arg(code: &str) -> Result<CadUnit> {
    CadUnit::from_code(code)
        .ok_or_else(|| EngineError::invalid(format!("unknown units '{code}' (mm, cm, dm, m, km, in, ft, yd, mi)")))
}

fn color_arg(code: &str) -> Result<CadColorMode> {
    CadColorMode::from_code(code)
        .ok_or_else(|| EngineError::invalid(format!("unknown color mode '{code}' (grey, layers, black)")))
}

/// Import a DWG or DXF file into a 2D view (plan, ceiling plan, elevation,
/// section or legend) as a drawing to trace over, in one undo step. DXF
/// (ASCII or binary, any version) is read entirely by ConstructCAD; DWG
/// (R12 to 2018 and later) through the acadrust library. Blocks are
/// inserted, curves kept as arcs, texts kept; entities that are not drawn
/// (3D solids, images, infinite lines…) are counted in `skipped`. The
/// element starts pinned, with the layers that are off or frozen in the
/// file hidden, and in the view's level. It is drawn in the view only,
/// under the model, and is left out of 3D views and IFC export.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CadImportFile {
    /// Path of the .dwg or .dxf file.
    pub path: PathBuf,
    /// View id or name.
    pub view: Ref,
    /// Where the drawing goes, view coordinates in meters: its origin
    /// (`align` "origin", the default; [0, 0] if not given) or the center of
    /// its extents (`align` "center"; the center of what the view shows if
    /// not given).
    #[serde(default)]
    pub position: Option<Point>,
    /// `origin` (the drawing's origin at `position`), `center`, or `auto`:
    /// the origin, unless that leaves the drawing more than 1 km from what
    /// the view shows (a drawing in survey coordinates), then the center.
    #[serde(default)]
    pub align: Option<String>,
    /// Drawing units, overriding the file's: mm, cm, dm, m, km, in, ft, yd
    /// or mi. Unitless files are read as meters when small (up to 1500
    /// units across), else as millimeters.
    #[serde(default)]
    pub units: Option<String>,
    /// Rotation in degrees counter-clockwise.
    #[serde(default)]
    pub rotation: Option<f64>,
    /// Extra scale on top of the units (default 1).
    #[serde(default)]
    pub scale: Option<f64>,
    /// `grey` (default), `layers` (the file's colors) or `black`.
    #[serde(default)]
    pub color_mode: Option<String>,
    /// Draw it faded (default false).
    #[serde(default)]
    pub halftone: Option<bool>,
}

impl Command for CadImportFile {
    const NAME: &'static str = "cad_import";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (view_id, level) = import_view(s, &self.view)?;
        let (center, auto) = align_arg(self.align.as_deref(), (false, false))?;
        let units = self.units.as_deref().map(units_arg).transpose()?;
        let color_mode = self.color_mode.as_deref().map(color_arg).transpose()?.unwrap_or_default();
        let rotation_deg = refs::finite(self.rotation.unwrap_or(0.0), "rotation")?;
        let scale = refs::positive(self.scale.unwrap_or(1.0), "scale")?;
        let position = self.position.map(refs::point).transpose()?;

        let path = self.path.clone();
        let mut read = guard(|| cc_cad::read_file(&path, &cc_cad::Limits::default()))?
            .map_err(|e| EngineError::invalid(e.to_string()))?;
        let drawing = Arc::new(std::mem::take(&mut read.drawing));
        if drawing.prims.is_empty() {
            return Err(EngineError::invalid(t!("msg-cad-empty")));
        }
        let assumed = units.is_none() && drawing.units.is_none();
        let units = units.or(drawing.units).unwrap_or_else(|| guess_units(&drawing));
        let hidden_layers = drawing.layers.iter().filter(|l| !l.visible).map(|l| l.name.clone()).collect();
        let name = self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let import = CadImport {
            view: view_id,
            name: name.clone(),
            drawing: drawing.clone(),
            units,
            scale,
            position: DVec2::ZERO,
            rotation_deg,
            mirrored: false,
            hidden_layers,
            color_mode,
            halftone: self.halftone.unwrap_or(false),
        };
        let (id, import) =
            add_import(s, view_id, level, import, position, center, auto, t!("tx-import-cad", "name" => name.clone()))?;
        Ok(json!({
            "id": id.0,
            "name": name,
            "format": match read.format { cc_cad::Format::Dwg => "dwg", cc_cad::Format::Dxf => "dxf", cc_cad::Format::Svg => "svg" },
            "version": read.version,
            "binary": read.binary,
            "units": units.code(),
            "units_assumed": assumed,
            "layers": drawing.layers.len(),
            "entities": read.entities,
            "blocks": read.blocks,
            "primitives": drawing.prims.len(),
            "skipped": read.skipped,
            "skipped_total": read.skipped_total(),
            "truncated": read.truncated,
            "extents": view_extents(&import),
        }))
    }
}

/// Place `import` in its view and add it pinned, in one undo step: its
/// origin at `position` (default [0, 0]); with `center`, the center of its
/// extents at `position` (default: the middle of what the view shows); with
/// `auto`, the origin unless that leaves the drawing more than 1 km from
/// what the view shows, then the center there.
#[allow(clippy::too_many_arguments)]
fn add_import(
    s: &mut Session,
    view_id: cc_core::ViewId,
    level: cc_core::LevelId,
    mut import: CadImport,
    position: Option<DVec2>,
    center: bool,
    auto: bool,
    tx: String,
) -> Result<(ElementId, CadImport)> {
    let drawing = import.drawing.clone();
    let shown = s.drawing(view_id).and_then(|d| d.fit_bounds()).map_or(DVec2::ZERO, |(a, b)| (a + b) * 0.5);
    let mid = drawing.extents.map_or(DVec2::ZERO, |(a, b)| (a + b) * 0.5);
    import.position = position.unwrap_or(DVec2::ZERO);
    let far = || {
        // The nearest point of the placed extents to what the view shows.
        let (lo, hi) = drawing.extents.unwrap_or_default();
        let corners = [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)].map(|p| import.to_view(p));
        let (a, b) = corners.iter().fold((corners[0], corners[0]), |(a, b), p| (a.min(*p), b.max(*p)));
        shown.clamp(a, b).distance(shown) > 1000.0
    };
    if center || (auto && far()) {
        let target = if center { position.unwrap_or(shown) } else { shown };
        // Where the extents' center lands with the drawing at the origin, moved onto the target.
        import.position = DVec2::ZERO;
        import.position = target - import.to_view(mid);
    }
    let kind = ElementKind::CadImport(Box::new(import.clone()));
    let id = edit(s, tx, |e| {
        let type_id = cad_type(e);
        let id = super::add_element(e, level, type_id, kind);
        e.set_pinned(&[id], true);
        Ok(id)
    })?;
    Ok((id, import))
}

/// The view an import goes into (one that holds underlays) and its level.
fn import_view(s: &Session, view: &Ref) -> Result<(cc_core::ViewId, cc_core::LevelId)> {
    let p = s.project();
    let view_id = refs::view(p, view)?;
    let view = p.view(view_id).ok_or_else(|| EngineError::not_found("view"))?;
    if !cc_views::hosts_cad(&view.kind) {
        return Err(EngineError::invalid(t!("msg-cad-view")));
    }
    let level =
        view.level().or_else(|| p.levels.first().map(|l| l.id)).ok_or_else(|| EngineError::rejected("no levels"))?;
    Ok((view_id, level))
}

/// `origin`, `center` or `auto` as (center, auto).
fn align_arg(align: Option<&str>, default: (bool, bool)) -> Result<(bool, bool)> {
    match align.map(|a| a.trim().to_ascii_lowercase()).as_deref() {
        None => Ok(default),
        Some("origin") => Ok((false, false)),
        Some("center") => Ok((true, false)),
        Some("auto") => Ok((false, true)),
        Some(a) => Err(EngineError::invalid(format!("unknown align '{a}' (origin, center, auto)"))),
    }
}

/// Resolution assumed for a picture that declares none, dots per inch.
const DEFAULT_DPI: f64 = 96.0;

/// Import a picture (PNG, JPEG, GIF, BMP or WebP) or an SVG drawing into a
/// 2D view (plan, ceiling plan, elevation, section or legend), in one undo
/// step, as an imported drawing under the model: placed, scaled, rotated,
/// pinned, hidden and printed like an imported DWG or DXF. A picture is as
/// large as its resolution makes it on paper (96 dpi when the file declares
/// none), times `scale`, unless `width` sets its width in the view; the
/// file's bytes are kept in the project. An SVG keeps its paths (curves as
/// polylines), fills and embedded pictures in their own colors; its texts
/// are not read.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImageImport {
    /// Path of the picture or .svg file.
    pub path: PathBuf,
    /// View id or name.
    pub view: Ref,
    /// Where it goes, view coordinates in meters: the center of the picture
    /// (`align` "center", the default; the center of what the view shows if
    /// not given) or its bottom-left corner for pictures and the page's for
    /// SVG (`align` "origin"; [0, 0] if not given).
    #[serde(default)]
    pub position: Option<Point>,
    /// `center` (default) or `origin`.
    #[serde(default)]
    pub align: Option<String>,
    /// Width in the view, meters (overrides `scale`).
    #[serde(default)]
    pub width: Option<f64>,
    /// Scale on top of its size on paper (default 1; 100 for a plan drawn at 1:100).
    #[serde(default)]
    pub scale: Option<f64>,
    /// Rotation in degrees counter-clockwise.
    #[serde(default)]
    pub rotation: Option<f64>,
    /// Draw it faded (default false).
    #[serde(default)]
    pub halftone: Option<bool>,
}

/// File extensions read by `image_import`.
pub const IMAGE_EXTENSIONS: [&str; 7] = ["png", "jpg", "jpeg", "gif", "bmp", "webp", "svg"];

impl Command for ImageImport {
    const NAME: &'static str = "image_import";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (view_id, level) = import_view(s, &self.view)?;
        let (center, _) = align_arg(self.align.as_deref(), (true, false))?;
        let rotation_deg = refs::finite(self.rotation.unwrap_or(0.0), "rotation")?;
        let mut scale = refs::positive(self.scale.unwrap_or(1.0), "scale")?;
        let width = self.width.map(|w| refs::positive(w, "width")).transpose()?;
        let position = self.position.map(refs::point).transpose()?;
        let len = std::fs::metadata(&self.path)?.len();
        if len > (cc_core::raster::MAX_IMAGE_BYTES as u64).max(cc_cad::svg::MAX_SVG_BYTES as u64) {
            return Err(EngineError::invalid(t!("err-image-too-large")));
        }
        let data = std::fs::read(&self.path)?;
        let (drawing, kind, dpi) = if cc_core::raster::RasterFormat::sniff(&data).is_some() {
            let image = guard(|| cc_views::raster::probe(data))?
                .ok_or_else(|| EngineError::invalid(t!("err-image-unreadable")))?;
            let declared = image.dpi();
            let (dx, dy) = declared.unwrap_or((DEFAULT_DPI, DEFAULT_DPI));
            let size = DVec2::new(f64::from(image.width) / dx, f64::from(image.height) / dy) * 25.4;
            let drawing = CadDrawing {
                layers: vec![cc_core::cad::CadLayer { name: "0".into(), color: [0, 0, 0], visible: true }],
                images: vec![cc_core::cad::CadImage {
                    layer: 0,
                    origin: DVec2::ZERO,
                    u: DVec2::new(size.x, 0.0),
                    v: DVec2::new(0.0, size.y),
                    image: Arc::new(image),
                }],
                units: Some(CadUnit::Millimeters),
                extents: Some((DVec2::ZERO, size)),
                ..Default::default()
            };
            (drawing, "picture", declared.map(|d| d.0))
        } else if cc_cad::svg::is_svg(&data) {
            let read = guard(|| cc_cad::svg::read(&data, &cc_cad::Limits::default()))?
                .map_err(|e| EngineError::invalid(e.to_string()))?;
            (read.drawing, "svg", None)
        } else {
            return Err(EngineError::invalid(t!("err-image-unreadable")));
        };
        let Some((lo, hi)) = drawing.extents.filter(|_| !(drawing.prims.is_empty() && drawing.images.is_empty()))
        else {
            return Err(EngineError::invalid(t!("msg-cad-empty")));
        };
        if let Some(w) = width {
            let paper = (hi.x - lo.x) * CadUnit::Millimeters.meters();
            if paper > 1e-12 {
                scale = w / paper;
            }
        }
        let name = self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let import = CadImport {
            view: view_id,
            name: name.clone(),
            drawing: Arc::new(drawing),
            units: CadUnit::Millimeters,
            scale,
            position: DVec2::ZERO,
            rotation_deg,
            mirrored: false,
            hidden_layers: Default::default(),
            color_mode: CadColorMode::Layers,
            halftone: self.halftone.unwrap_or(false),
        };
        let tx = t!("tx-import-image", "name" => name.clone());
        let (id, import) = add_import(s, view_id, level, import, position, center, false, tx)?;
        let image = import.drawing.images.first().map(|i| json!([i.image.width, i.image.height]));
        Ok(json!({
            "id": id.0,
            "name": name,
            "kind": kind,
            "pixels": image,
            "dpi": dpi,
            "primitives": import.drawing.prims.len(),
            "images": import.drawing.images.len(),
            "scale": import.scale,
            "extents": view_extents(&import),
        }))
    }
}

fn cad_of(s: &Session, id: u64) -> Result<(ElementId, CadImport)> {
    let id = refs::element(s.project(), id)?;
    match s.project().element(id).map(|e| &e.kind) {
        Some(ElementKind::CadImport(c)) => Ok((id, (**c).clone())),
        _ => Err(EngineError::invalid(format!("element {} is not an imported drawing", id.0))),
    }
}

/// The layers of an imported drawing: name, color, whether the view shows
/// it, whether it was on in the file, and how many primitives it holds.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CadLayers {
    /// Imported drawing element id.
    pub id: u64,
}

impl Command for CadLayers {
    const NAME: &'static str = "cad_layers";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (_, c) = cad_of(s, self.id)?;
        let mut counts = vec![0usize; c.drawing.layers.len()];
        for p in &c.drawing.prims {
            if let Some(n) = counts.get_mut(p.layer() as usize) {
                *n += 1;
            }
        }
        Ok(c.drawing
            .layers
            .iter()
            .zip(counts)
            .map(|(l, n)| {
                json!({
                    "name": l.name,
                    "color": l.color,
                    "visible": !c.hidden_layers.contains(&l.name),
                    "on_in_file": l.visible,
                    "primitives": n,
                })
            })
            .collect())
    }
}

/// Show or hide layers of an imported drawing (by name; all of them when
/// `layers` is empty).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CadLayersSet {
    /// Imported drawing element id.
    pub id: u64,
    /// Layer names (case-insensitive); empty for every layer.
    #[serde(default)]
    pub layers: Vec<String>,
    /// Show (true) or hide (false) them.
    pub visible: bool,
}

impl Command for CadLayersSet {
    const NAME: &'static str = "cad_layers_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (id, c) = cad_of(s, self.id)?;
        let names: Vec<String> = if self.layers.is_empty() {
            c.drawing.layers.iter().map(|l| l.name.clone()).collect()
        } else {
            self.layers
                .iter()
                .map(|wanted| {
                    c.drawing
                        .layers
                        .iter()
                        .find(|l| l.name.trim().eq_ignore_ascii_case(wanted.trim()))
                        .map(|l| l.name.clone())
                        .ok_or_else(|| EngineError::not_found(format!("no layer named '{wanted}'")))
                })
                .collect::<Result<_>>()?
        };
        edit(s, t!("tx-cad-layers"), |e| {
            let Some(ElementKind::CadImport(c)) = e.element_mut(id).map(|x| &mut x.kind) else {
                return Err(EngineError::not_found("element"));
            };
            for n in names {
                if self.visible {
                    c.hidden_layers.remove(&n);
                } else {
                    c.hidden_layers.insert(n);
                }
            }
            Ok(())
        })?;
        Ok(created(s, id))
    }
}
