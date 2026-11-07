//! Sheets and their title blocks: paper, title block, fields and issue
//! date of a sheet; title block components and their 2D pieces.

use super::component::edit_def;
use super::component_piece::{ExprArg, SketchPointArg, profile_points};
use super::edit;
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::{Session, guard};
use crate::t;
use cc_core::raster::{MAX_IMAGE_BYTES, RasterImage};
use cc_core::title_block::{self, SheetItem, SheetShape, TextAnchor};
use cc_core::{
    ComponentDef, ComponentKind, EditCtx, ElementType, ElementTypeId, Project, SheetSize, TypeData, ViewId, ViewKind,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

/// A paper size: `"A0"`…`"A4"`, or `{width, height}` in millimeters.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum SizeArg {
    /// `"A0"`, `"A1"`, `"A2"`, `"A3"` or `"A4"`.
    Iso(String),
    /// A custom paper, millimeters.
    Custom {
        /// Width, millimeters.
        width: f64,
        /// Height, millimeters.
        height: f64,
    },
}

impl SizeArg {
    fn size(&self) -> Result<SheetSize> {
        let size = match self {
            SizeArg::Iso(s) => match s.trim().to_ascii_uppercase().as_str() {
                "A0" => SheetSize::A0,
                "A1" => SheetSize::A1,
                "A2" => SheetSize::A2,
                "A3" => SheetSize::A3,
                "A4" => SheetSize::A4,
                other => {
                    return Err(EngineError::invalid(format!(
                        "unknown paper '{other}' (A0 to A4, or {{width, height}})"
                    )));
                }
            },
            SizeArg::Custom { width, height } => SheetSize::Custom { width: *width, height: *height },
        };
        if !size.is_valid() {
            return Err(EngineError::invalid(format!(
                "a custom sheet is {} to {} mm on each side",
                cc_core::MIN_PAPER_MM,
                cc_core::MAX_PAPER_MM
            )));
        }
        Ok(size)
    }
}

/// A title block type by id or name.
fn title_block_type(p: &Project, r: &Ref) -> Result<ElementTypeId> {
    let id = refs::element_type(p, r)?;
    if p.title_blocks().contains(&id) { Ok(id) } else { Err(EngineError::invalid("not a title block type")) }
}

/// Sheets on any of the title block `types` keep their per-sheet values
/// under the parameters' new names (`renames`, `[old, new]` in order).
pub(crate) fn rename_sheet_fields(e: &mut EditCtx, types: &[ElementTypeId], renames: &[(String, String)]) {
    if renames.is_empty() {
        return;
    }
    let sheets: Vec<ViewId> = e
        .project()
        .views
        .values()
        .filter(|v| matches!(&v.kind, ViewKind::Sheet { title_block: Some(t), .. } if types.contains(t)))
        .map(|v| v.id)
        .collect();
    for id in sheets {
        let Some(ViewKind::Sheet { fields, .. }) = e.view_mut(id).map(|v| &mut v.kind) else { continue };
        for (old, new) in renames {
            if let Some(v) = fields.remove(old) {
                fields.insert(new.clone(), v);
            }
        }
    }
}

/// The title block definition of a type, if it is one.
fn title_block_of(p: &Project, id: Option<ElementTypeId>) -> Option<&ComponentDef> {
    match p.element_type(id?).map(|t| &t.data) {
        Some(TypeData::Component { def, .. }) if def.kind == ComponentKind::TitleBlock => Some(def),
        _ => None,
    }
}

/// Change a sheet: its number, paper (`size`, `portrait`), title block,
/// per-sheet field values and issue date.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetSet {
    /// The sheet: id or name.
    pub sheet: Ref,
    /// Sheet number.
    #[serde(default)]
    pub number: Option<String>,
    /// Paper: `"A0"`…`"A4"`, or `{width, height}` in millimeters (50 to 5000).
    #[serde(default)]
    pub size: Option<SizeArg>,
    /// The long side runs up.
    #[serde(default)]
    pub portrait: Option<bool>,
    /// The title block type (id or name), or `null` for none.
    #[serde(default, deserialize_with = "refs::nullable", skip_serializing_if = "Option::is_none")]
    pub title_block: Option<Option<Ref>>,
    /// Values of the title block's per-sheet fields by parameter name: a text sets one, `null` clears it.
    #[serde(default)]
    pub fields: Option<serde_json::Map<String, Value>>,
    /// Issue date, `YYYY-MM-DD`; empty to clear.
    #[serde(default)]
    pub issue_date: Option<String>,
}

impl Command for SheetSet {
    const NAME: &'static str = "sheet_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = super::views::sheet(p, &self.sheet)?;
        let size = self.size.as_ref().map(SizeArg::size).transpose()?;
        let title_block = match &self.title_block {
            Some(Some(r)) => Some(Some(title_block_type(p, r)?)),
            Some(None) => Some(None),
            None => None,
        };
        let issue_date = match self.issue_date.as_deref().map(str::trim) {
            Some("") => Some(String::new()),
            Some(d) if title_block::is_iso_date(d) => Some(d.to_string()),
            Some(d) => return Err(EngineError::invalid(format!("'{d}' is not a YYYY-MM-DD date"))),
            None => None,
        };
        let mut changes: Vec<(String, Option<String>)> = Vec::new();
        for (name, value) in self.fields.iter().flatten() {
            let name = name.trim();
            if name.is_empty() {
                return Err(EngineError::invalid("a field needs a name"));
            }
            let value = match value {
                Value::Null => None,
                Value::String(v) if v.chars().count() <= title_block::MAX_SHEET_TEXT => Some(v.clone()),
                Value::String(_) => return Err(EngineError::invalid(format!("field '{name}': the text is too long"))),
                other => {
                    return Err(EngineError::invalid(format!("field '{name}': expected a text or null, got {other}")));
                }
            };
            changes.push((name.to_string(), value));
        }
        edit(s, t!("tx-sheet-set"), |e| {
            let Some(ViewKind::Sheet { number, size: sz, portrait, title_block: tb, fields, issue_date: date, .. }) =
                e.view_mut(id).map(|v| &mut v.kind)
            else {
                return Err(EngineError::invalid("not a sheet"));
            };
            if let Some(n) = self.number {
                *number = n;
            }
            if let Some(v) = size {
                *sz = v;
            }
            if let Some(v) = self.portrait {
                *portrait = v;
            }
            if let Some(v) = title_block {
                *tb = v;
            }
            if let Some(v) = issue_date {
                *date = v;
            }
            for (name, value) in changes {
                match value {
                    Some(v) => {
                        fields.insert(name, v);
                    }
                    None => {
                        fields.remove(&name);
                    }
                }
            }
            Ok(())
        })?;
        Ok(super::views::view_out(s, id))
    }
}

/// A sheet's title block fields: every field key (`fields`, as `{key}`
/// in title block texts reads it) with its value on this sheet, the title
/// block's per-sheet parameters (`params`: name, label, the sheet's value
/// and the type's default), and the title block's pieces that do not
/// resolve on this paper (`errors`).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetFields {
    /// The sheet: id or name.
    pub sheet: Ref,
}

impl Command for SheetFields {
    const NAME: &'static str = "sheet_fields";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = super::views::sheet(p, &self.sheet)?;
        let view = p.view(id).ok_or_else(|| EngineError::invalid("not a sheet"))?;
        let ViewKind::Sheet { title_block: tb, fields, .. } = &view.kind else {
            return Err(EngineError::invalid("not a sheet"));
        };
        let def = title_block_of(p, *tb);
        let as_indicated = t!("sheet-as-indicated");
        let ctx = cc_views::sheet_fields::FieldCtx {
            project: p,
            sheet: view,
            def,
            today: crate::i18n::today(),
            format_date: crate::i18n::format_date,
            as_indicated: &as_indicated,
            sheets: Default::default(),
        };
        // `issue_date` is not in the menu's keys; it follows `date`.
        let mut keys: Vec<&str> = cc_views::sheet_fields::KEYS.to_vec();
        let after_date = keys.iter().position(|k| *k == "date").map_or(keys.len(), |i| i + 1);
        keys.insert(after_date, "issue_date");
        let mut out = serde_json::Map::new();
        for key in keys {
            out.insert(key.to_string(), json!(cc_views::sheet_fields::value(key, &ctx).unwrap_or_default()));
        }
        for f in &p.info.fields {
            out.insert(format!("project:{}", f.name), json!(f.value));
        }
        let params: Vec<Value> = def
            .iter()
            .flat_map(|d| d.params.iter())
            .filter(|q| q.kind.is_text() && q.instance)
            .map(|q| {
                json!({
                    "name": q.name,
                    "label": q.display(),
                    "value": fields.get(&q.name).unwrap_or(&q.text),
                    "default": q.text,
                })
            })
            .collect();
        let errors: Vec<Value> = match (def, view.paper_mm()) {
            (Some(d), Some(paper)) => d
                .resolve_sheet(paper)
                .errors
                .iter()
                .map(|(i, e)| {
                    let piece = d.sheet.get(*i).map_or("", |it| it.name.as_str());
                    json!({"index": i, "piece": piece, "error": e.to_string()})
                })
                .collect(),
            _ => Vec::new(),
        };
        Ok(json!({"fields": out, "params": params, "errors": errors}))
    }
}

/// Create a title block component: a copy of the standard one, or empty
/// with `empty`. Sheets use it through `sheet_set` `title_block`.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TitleBlockCreate {
    /// Name of the component and of its first type.
    pub name: String,
    /// Start with no pieces.
    #[serde(default)]
    pub empty: bool,
}

impl Command for TitleBlockCreate {
    const NAME: &'static str = "title_block_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let name = self.name.trim().to_string();
        if name.is_empty() {
            return Err(EngineError::invalid("the title block needs a name"));
        }
        let mut def = title_block::standard(&|k| {
            crate::i18n::try_tr(k).unwrap_or_else(|| cc_core::template_default(k).to_string())
        });
        if self.empty {
            def.sheet.clear();
        }
        // Types of one component share its definition: a name already in use
        // would mix the title block with another component's types.
        let p = s.project();
        if p.types.values().any(|t| t.name.trim() == name) {
            return Err(EngineError::invalid(format!("a type is already named '{name}'")));
        }
        if p.types
            .values()
            .any(|t| matches!(&t.data, TypeData::Component { component, .. } if component.trim() == name))
        {
            return Err(EngineError::invalid(format!("a component is already named '{name}'")));
        }
        super::types::validate(&def)?;
        let id = edit(s, t!("tx-new-title-block"), |e| {
            let id = ElementTypeId(e.alloc_id());
            let data = TypeData::Component { component: name.clone(), def };
            e.add_type(ElementType { id, name: name.clone(), data, info: Default::default(), uid: Default::default() });
            Ok(id)
        })?;
        Ok(super::types::type_out(s, id))
    }
}

/// A 2D piece of a title block. Coordinates are millimeters from the
/// paper's bottom-left corner: numbers or expressions over the
/// parameters, `SheetWidth` and `SheetHeight`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SheetPieceArg {
    /// Lines and arcs through `points`.
    Stroke {
        /// The points ({u, v, radius}: u across, v up; a radius makes the segment to the next point an arc).
        points: Vec<SketchPointArg>,
        /// Joins the last point to the first.
        #[serde(default)]
        closed: bool,
        /// Pen width, millimeters (default 0.25).
        #[serde(default)]
        weight: Option<ExprArg>,
        /// Line color [r, g, b] (default black).
        #[serde(default)]
        color: Option<[u8; 3]>,
        /// Fill color of a closed stroke [r, g, b].
        #[serde(default)]
        fill: Option<[u8; 3]>,
    },
    /// Text with `{fields}`; line breaks start new lines.
    Text {
        /// Position across, millimeters.
        x: ExprArg,
        /// Position up, millimeters.
        y: ExprArg,
        /// The words, with `{fields}` (`sheet_fields` lists them; `{param:Name}` reads a parameter).
        text: String,
        /// Height, millimeters (default 2.5).
        #[serde(default)]
        size: Option<ExprArg>,
        /// `left` (default), `center`, `right` (on the baseline) or `middle` (centered).
        #[serde(default)]
        anchor: Option<String>,
        /// Degrees counter-clockwise.
        #[serde(default)]
        rotation: Option<ExprArg>,
    },
    /// A PNG, JPEG, GIF, BMP or WebP picture filling a rectangle from its bottom-left corner (x, y).
    Image {
        /// Corner across, millimeters.
        x: ExprArg,
        /// Corner up, millimeters.
        y: ExprArg,
        /// Width, millimeters.
        width: ExprArg,
        /// Height, millimeters.
        height: ExprArg,
        /// The picture's file.
        #[serde(default)]
        image_path: Option<PathBuf>,
        /// The picture's bytes, base64.
        #[serde(default)]
        image: Option<String>,
    },
}

impl SheetPieceArg {
    fn shape(&self) -> Result<SheetShape> {
        let opt = |e: &Option<ExprArg>| e.as_ref().map(ExprArg::text).transpose().map(Option::unwrap_or_default);
        Ok(match self {
            SheetPieceArg::Stroke { points, closed, weight, color, fill } => {
                if points.len() > title_block::MAX_STROKE_POINTS {
                    return Err(EngineError::invalid("too many points"));
                }
                SheetShape::Stroke {
                    points: profile_points(points)?,
                    closed: *closed,
                    weight: opt(weight)?,
                    color: *color,
                    fill: *fill,
                }
            }
            SheetPieceArg::Text { x, y, text, size, anchor, rotation } => {
                if text.chars().count() > title_block::MAX_SHEET_TEXT {
                    return Err(EngineError::invalid(format!(
                        "a text holds at most {} characters",
                        title_block::MAX_SHEET_TEXT
                    )));
                }
                let anchor = match anchor.as_deref().map(|a| a.trim().to_ascii_lowercase()).as_deref() {
                    None | Some("left") => TextAnchor::Left,
                    Some("center") => TextAnchor::Center,
                    Some("right") => TextAnchor::Right,
                    Some("middle") => TextAnchor::Middle,
                    Some(other) => {
                        return Err(EngineError::invalid(format!(
                            "unknown anchor '{other}' (left, center, right or middle)"
                        )));
                    }
                };
                let size = match size {
                    Some(v) => v.text()?,
                    None => "2.5".to_string(),
                };
                SheetShape::Text {
                    x: x.text()?,
                    y: y.text()?,
                    text: text.clone(),
                    size,
                    anchor,
                    rotation: opt(rotation)?,
                }
            }
            SheetPieceArg::Image { x, y, width, height, image_path, image } => SheetShape::Image {
                x: x.text()?,
                y: y.text()?,
                width: width.text()?,
                height: height.text()?,
                image: read_picture(image_path.as_deref(), image.as_deref())?,
            },
        })
    }
}

/// A picture from a file or base64 text (one of them), checked against the
/// raster limits.
fn read_picture(image_path: Option<&std::path::Path>, image: Option<&str>) -> Result<Arc<RasterImage>> {
    let bytes = match (image_path, image) {
        (Some(path), None) => {
            if std::fs::metadata(path)?.len() > MAX_IMAGE_BYTES as u64 {
                return Err(EngineError::invalid(t!("err-image-too-large")));
            }
            std::fs::read(path)?
        }
        (None, Some(text)) => {
            if text.len() > MAX_IMAGE_BYTES / 3 * 4 + 8 {
                return Err(EngineError::invalid(t!("err-image-too-large")));
            }
            cc_core::raster::base64_decode(text.trim())
                .ok_or_else(|| EngineError::invalid("the picture is not valid base64"))?
        }
        _ => return Err(EngineError::invalid("give the picture as `image_path` or `image`, one of them")),
    };
    let raster: RasterImage =
        guard(|| cc_views::raster::probe(bytes))?.ok_or_else(|| EngineError::invalid(t!("err-image-unreadable")))?;
    Ok(Arc::new(raster))
}

/// The same picture without its bytes (for patches and summaries).
fn without_bytes(image: &RasterImage) -> Arc<RasterImage> {
    Arc::new(RasterImage { format: image.format, width: image.width, height: image.height, data: Vec::new() })
}

/// A definition as queries show it: each title block picture as its
/// format, size in pixels and `bytes`, not the bytes themselves.
pub(crate) fn def_json(def: &ComponentDef) -> Value {
    if !def.sheet.iter().any(|i| matches!(i.shape, SheetShape::Image { .. })) {
        return serde_json::to_value(def).unwrap_or(Value::Null);
    }
    let mut light = def.clone();
    let mut sizes = Vec::new();
    for item in &mut light.sheet {
        if let SheetShape::Image { image, .. } = &mut item.shape {
            sizes.push(image.data.len());
            *image = without_bytes(image);
        }
    }
    let mut out = serde_json::to_value(&light).unwrap_or(Value::Null);
    let pictures = out
        .get_mut("sheet")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(|item| item.pointer_mut("/shape/Image/image").and_then(Value::as_object_mut));
    for (picture, bytes) in pictures.zip(sizes) {
        picture.remove("data");
        picture.insert("bytes".into(), json!(bytes));
    }
    out
}

fn title_block_def(def: &ComponentDef) -> Result<()> {
    if def.kind == ComponentKind::TitleBlock {
        Ok(())
    } else {
        Err(EngineError::invalid("sheet pieces go into title blocks only"))
    }
}

/// Push `shapes` onto a title block's pieces, the first named `name` (the
/// others `Piece1`, `Piece2`…), and return their indices; refused whole
/// when they would pass the most pieces it holds.
fn push_pieces(def: &mut ComponentDef, shapes: Vec<SheetShape>, name: Option<&str>) -> Result<Vec<usize>> {
    title_block_def(def)?;
    if def.sheet.len().saturating_add(shapes.len()) > title_block::MAX_SHEET_PIECES {
        return Err(EngineError::invalid("the title block has the most pieces it can hold"));
    }
    let mut indices = Vec::with_capacity(shapes.len());
    for (n, shape) in shapes.into_iter().enumerate() {
        let name = match name.map(str::trim) {
            Some(given) if n == 0 && !given.is_empty() => given.to_string(),
            _ => (1..=def.sheet.len() + 1)
                .map(|i| format!("Piece{i}"))
                .find(|candidate| !def.sheet.iter().any(|it| it.name == *candidate))
                .unwrap_or_default(),
        };
        def.sheet.push(SheetItem { name, shape });
        indices.push(def.sheet.len() - 1);
    }
    Ok(indices)
}

/// Add 2D pieces to a title block component (every type of it), in one step.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentSheetPieceAdd {
    /// A type of the title block: id or name.
    #[serde(rename = "type")]
    pub type_ref: Ref,
    /// The piece, by `kind`: `stroke` (`points` as [{u, v, radius}], `closed`,
    /// `weight`, `color`, `fill`), `text` (`x`, `y`, `text` with `{fields}`,
    /// `size`, `anchor`: left, center, right or middle, `rotation`) or
    /// `image` (`x`, `y`, `width`, `height`, and `image_path` or base64
    /// `image`). Millimeters from the paper's bottom-left corner, as
    /// numbers or expressions over the parameters, `SheetWidth` and `SheetHeight`.
    #[serde(default)]
    pub piece: Option<SheetPieceArg>,
    /// More pieces (each as `piece`), added after it in the same step: all
    /// of them or, when one is refused, none.
    #[serde(default)]
    pub pieces: Vec<SheetPieceArg>,
    /// Name of the first piece added (default `Piece1`, `Piece2`…).
    #[serde(default)]
    pub name: Option<String>,
}

impl Command for ComponentSheetPieceAdd {
    const NAME: &'static str = "component_sheet_piece_add";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let shapes = self.piece.iter().chain(&self.pieces).map(SheetPieceArg::shape).collect::<Result<Vec<_>>>()?;
        if shapes.is_empty() {
            return Err(EngineError::invalid("give a `piece` or `pieces` to add"));
        }
        let indices = edit_def(s, &self.type_ref, &[], |def| push_pieces(def, shapes, self.name.as_deref()))?;
        Ok(json!({ "index": indices.first(), "indices": indices }))
    }
}

/// Change a 2D piece of a title block: `piece` is a merge patch over its
/// form in `component_get` `def.sheet` (e.g. `{"Text": {"size": "4"}}`);
/// an image piece's picture is replaced with `image_path` or `image`, not
/// by the patch.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentSheetPieceSet {
    /// A type of the title block: id or name.
    #[serde(rename = "type")]
    pub type_ref: Ref,
    /// The piece's index (in `component_get` `def.sheet`).
    pub index: usize,
    /// Merge patch over the piece's shape (without a picture).
    #[serde(default)]
    pub piece: Option<Value>,
    /// New name.
    #[serde(default)]
    pub name: Option<String>,
    /// A new picture for an image piece: its file (PNG, JPEG, GIF, BMP or WebP).
    #[serde(default)]
    pub image_path: Option<PathBuf>,
    /// A new picture for an image piece: its bytes, base64.
    #[serde(default)]
    pub image: Option<String>,
    /// Pieces added in the same step (each as `component_sheet_piece_add`
    /// `piece`; e.g. the other chains of a redrawn sketch); the result lists
    /// their indices in `added`.
    #[serde(default)]
    pub add: Vec<SheetPieceArg>,
}

impl Command for ComponentSheetPieceSet {
    const NAME: &'static str = "component_sheet_piece_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        if self.piece.as_ref().and_then(|p| p.get("Image")).and_then(|p| p.get("image")).is_some() {
            return Err(EngineError::invalid("give a new picture as `image_path` or `image`, not in the patch"));
        }
        let picture = match (&self.image_path, &self.image) {
            (None, None) => None,
            (path, text) => Some(read_picture(path.as_deref(), text.as_deref())?),
        };
        let added = self.add.iter().map(SheetPieceArg::shape).collect::<Result<Vec<_>>>()?;
        let added = edit_def(s, &self.type_ref, &[], |def| {
            title_block_def(def)?;
            let item = def.sheet.get_mut(self.index).ok_or_else(|| EngineError::invalid("no such piece"))?;
            if let Some(patch) = &self.piece {
                // The patch runs over the piece without its picture's bytes,
                // and the piece keeps the same picture.
                let kept = match &mut item.shape {
                    SheetShape::Image { image, .. } => {
                        let light = without_bytes(image);
                        Some(std::mem::replace(image, light))
                    }
                    _ => None,
                };
                // `edit_def` refuses a piece the patch leaves out of bounds.
                let mut shape = super::patch_value(&item.shape, patch)?;
                match (&mut shape, kept) {
                    (SheetShape::Image { image, .. }, Some(kept)) => *image = kept,
                    (SheetShape::Image { .. }, None) => {
                        return Err(EngineError::invalid("give the picture as `image_path` or `image`"));
                    }
                    _ => {}
                }
                item.shape = shape;
            }
            if let Some(new) = picture {
                let SheetShape::Image { image, .. } = &mut item.shape else {
                    return Err(EngineError::invalid("only an image piece takes a picture"));
                };
                *image = new;
            }
            if let Some(n) = self.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                item.name = n.to_string();
            }
            if added.is_empty() { Ok(Vec::new()) } else { push_pieces(def, added, None) }
        })?;
        Ok(json!({ "index": self.index, "added": added }))
    }
}

/// Delete 2D pieces of a title block, in one step.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentSheetPieceDelete {
    /// A type of the title block: id or name.
    #[serde(rename = "type")]
    pub type_ref: Ref,
    /// The piece's index (in `component_get` `def.sheet`).
    #[serde(default)]
    pub index: Option<usize>,
    /// More pieces' indices, deleted with `index` in the same step: all of
    /// them or, when one does not exist, none.
    #[serde(default)]
    pub indices: Vec<usize>,
}

impl Command for ComponentSheetPieceDelete {
    const NAME: &'static str = "component_sheet_piece_delete";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let mut list: Vec<usize> = self.index.into_iter().chain(self.indices).collect();
        if list.is_empty() {
            return Err(EngineError::invalid("give an `index` or `indices` to delete"));
        }
        list.sort_unstable();
        list.dedup();
        edit_def(s, &self.type_ref, &[], |def| {
            title_block_def(def)?;
            if list.last().is_some_and(|&last| last >= def.sheet.len()) {
                return Err(EngineError::invalid("no such piece"));
            }
            // Highest first, so the others keep their indices.
            for &index in list.iter().rev() {
                def.sheet.remove(index);
            }
            Ok(())
        })?;
        Ok(json!({ "deleted": list.len() }))
    }
}
