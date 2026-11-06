//! Title blocks: components of kind [`ComponentKind::TitleBlock`] whose 2D
//! pieces ([`ComponentDef::sheet`]) draw a whole sheet (border, title
//! block, logo) in paper millimeters, origin at the paper's bottom-left
//! corner. Expressions read the parameters and [`SHEET_WIDTH`] and
//! [`SHEET_HEIGHT`]; texts keep `{fields}` for the sheet to fill in.

use crate::component::{ComponentDef, ComponentKind, PartError};
use crate::expr;
use crate::profile::{ProfileError, ProfilePoint};
use crate::raster::RasterImage;
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Name of the paper width (millimeters) in expressions.
pub const SHEET_WIDTH: &str = "SheetWidth";
/// Name of the paper height (millimeters) in expressions.
pub const SHEET_HEIGHT: &str = "SheetHeight";
pub const MAX_SHEET_PIECES: usize = 2000;
pub const MAX_STROKE_POINTS: usize = 2000;
pub const MAX_SHEET_TEXT: usize = 4000;
/// Most points a title block's strokes may have in all, once arcs are flattened.
pub const MAX_SHEET_POINTS: usize = 500_000;
/// Pen width of a stroke without one, millimeters.
pub const DEFAULT_WEIGHT: f64 = 0.25;

/// Name key of the standard title block (component and type).
pub const STANDARD_KEY: &str = "tpl-tb-standard";

/// The title block every sheet had before title blocks were components:
/// a border 10 mm in, and a 170 × 42 mm block in the bottom-right corner
/// with the project, sheet and client rows and the sheet number cell.
/// Captions are looked up by `n` (template keys `tpl-tb-*`).
pub fn standard(n: &dyn Fn(&str) -> String) -> ComponentDef {
    // A caption is fixed words: its braces are not fields.
    let n = |key: &str| n(key).replace('{', "{{").replace('}', "}}");
    let p = |u: &str, v: &str| ProfilePoint::new(u, v);
    let stroke = |name: &str, points: Vec<ProfilePoint>, closed: bool, weight: &str| SheetItem {
        name: name.into(),
        shape: SheetShape::Stroke { points, closed, weight: weight.into(), color: None, fill: None },
    };
    let text = |name: &str, x: &str, y: &str, text: String, size: &str| SheetItem {
        name: name.into(),
        shape: SheetShape::Text {
            x: x.into(),
            y: y.into(),
            text,
            size: size.into(),
            anchor: TextAnchor::Left,
            rotation: String::new(),
        },
    };
    // The block runs from x = SheetWidth - 180 to SheetWidth - 10 and from
    // y = 10 to 52; rows split it at 24 and 38, the number cell at
    // SheetWidth - 50. Texts start 2 mm into their cell, with their
    // baseline one text height below where their top sits.
    let (left, right) = ("SheetWidth - 180", "SheetWidth - 10");
    let sheet = vec![
        stroke(
            "Border",
            vec![p("10", "10"), p(right, "10"), p(right, "SheetHeight - 10"), p("10", "SheetHeight - 10")],
            true,
            "0.5",
        ),
        stroke("Block", vec![p(left, "10"), p(right, "10"), p(right, "52"), p(left, "52")], true, "0.25"),
        stroke("Row 1", vec![p(left, "24"), p("SheetWidth - 50", "24")], false, "0.25"),
        stroke("Row 2", vec![p(left, "38"), p(right, "38")], false, "0.25"),
        stroke("Number cell", vec![p("SheetWidth - 50", "10"), p("SheetWidth - 50", "38")], false, "0.25"),
        text("Project caption", "SheetWidth - 178", "47", n("tpl-tb-project"), "2"),
        text("Project", "SheetWidth - 178", "42", "{project_name}".into(), "4"),
        text("Sheet caption", "SheetWidth - 178", "33", n("tpl-tb-sheet"), "2"),
        text("Sheet", "SheetWidth - 178", "28.5", "{sheet_name}".into(), "3.5"),
        text("Client caption", "SheetWidth - 178", "19", n("tpl-tb-client"), "2"),
        text("Client", "SheetWidth - 178", "15", "{client}".into(), "3"),
        text("Number caption", "SheetWidth - 48", "33", n("tpl-tb-number"), "2"),
        text("Number", "SheetWidth - 48", "25", "{sheet_number}".into(), "6"),
        text(
            "Project number",
            "SheetWidth - 48",
            "18.8",
            format!("{} {{project_number}}", n("tpl-tb-project-number")),
            "2.2",
        ),
        text("Author", "SheetWidth - 48", "13.8", format!("{} {{author}}", n("tpl-tb-author")), "2.2"),
    ];
    ComponentDef { kind: ComponentKind::TitleBlock, sheet, ..Default::default() }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetItem {
    pub name: String,
    pub shape: SheetShape,
}

/// Where a text's position sits: on the baseline at its start, middle or
/// end, or at its center.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TextAnchor {
    #[default]
    Left,
    Center,
    Right,
    Middle,
}

/// A 2D piece; every `String` is an expression in millimeters (degrees for rotations).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SheetShape {
    /// Lines and arcs: `points` run straight, or along an arc (`radius`) to the next.
    Stroke {
        points: Vec<ProfilePoint>,
        #[serde(default)]
        closed: bool,
        /// Pen width; empty for [`DEFAULT_WEIGHT`].
        #[serde(default, skip_serializing_if = "String::is_empty")]
        weight: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<[u8; 3]>,
        /// Fill of a closed stroke.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<[u8; 3]>,
    },
    /// Fixed words and `{fields}`; line breaks start new lines.
    Text {
        x: String,
        y: String,
        text: String,
        /// Text height.
        size: String,
        #[serde(default)]
        anchor: TextAnchor,
        /// Degrees counter-clockwise; empty for 0.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        rotation: String,
    },
    /// A picture filling the rectangle from (`x`, `y`) (its bottom-left corner).
    Image { x: String, y: String, width: String, height: String, image: Arc<RasterImage> },
}

impl SheetShape {
    /// Every expression of the piece, for renaming parameters.
    pub fn expressions_mut(&mut self) -> Vec<&mut String> {
        match self {
            SheetShape::Stroke { points, weight, .. } => {
                let mut out: Vec<&mut String> = points.iter_mut().flat_map(ProfilePoint::expressions_mut).collect();
                out.push(weight);
                out
            }
            SheetShape::Text { x, y, size, rotation, .. } => vec![x, y, size, rotation],
            SheetShape::Image { x, y, width, height, .. } => vec![x, y, width, height],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetStroke {
    pub item: usize,
    pub points: Vec<DVec2>,
    pub closed: bool,
    pub weight: f64,
    pub color: [u8; 3],
    pub fill: Option<[u8; 3]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetText {
    pub item: usize,
    pub position: DVec2,
    /// As written: `{fields}` are filled in by the sheet.
    pub text: String,
    pub size: f64,
    pub anchor: TextAnchor,
    pub rotation_deg: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheetImage {
    pub item: usize,
    /// Bottom-left corner.
    pub corner: DVec2,
    pub size: DVec2,
    pub image: Arc<RasterImage>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SheetPieces {
    pub strokes: Vec<SheetStroke>,
    pub texts: Vec<SheetText>,
    pub images: Vec<SheetImage>,
    pub errors: Vec<(usize, PartError)>,
}

fn profile_error(e: ProfileError) -> PartError {
    match e {
        ProfileError::Expr { field, error } => PartError::Expr { field, error },
        ProfileError::Degenerate => PartError::Degenerate,
    }
}

impl ComponentDef {
    /// The 2D pieces on a `paper` sheet (millimeters); pieces that fail are
    /// reported with their index and left out.
    pub fn resolve_sheet(&self, paper: DVec2) -> SheetPieces {
        let scope = self.scope(&Default::default(), &[(SHEET_WIDTH, paper.x), (SHEET_HEIGHT, paper.y)]);
        let lookup = |name: &str| scope.get(name);
        let mut out = SheetPieces::default();
        let mut points = 0;
        for (i, item) in self.sheet.iter().enumerate().take(MAX_SHEET_PIECES) {
            let strokes = out.strokes.len();
            if let Err(e) = resolve_piece(i, &item.shape, &lookup, &mut out) {
                out.errors.push((i, e));
            } else if out.strokes.len() > strokes {
                // Strokes past the budget of flattened points are left out.
                let n = out.strokes.last().map_or(0, |s| s.points.len());
                if points + n > MAX_SHEET_POINTS {
                    out.strokes.truncate(strokes);
                    out.errors.push((i, PartError::Degenerate));
                } else {
                    points += n;
                }
            }
        }
        out
    }
}

fn resolve_piece(
    i: usize,
    shape: &SheetShape,
    lookup: &dyn Fn(&str) -> Option<f64>,
    out: &mut SheetPieces,
) -> Result<(), PartError> {
    let ev =
        |field: &'static str, text: &str| expr::eval(text, lookup).map_err(|error| PartError::Expr { field, error });
    let ev_or = |field: &'static str, text: &str, default: f64| {
        if text.trim().is_empty() { Ok(default) } else { ev(field, text) }
    };
    match shape {
        SheetShape::Stroke { points, closed, weight, color, fill } => {
            if points.len() < 2 || points.len() > MAX_STROKE_POINTS {
                return Err(PartError::Degenerate);
            }
            let corners = crate::profile::vertices(points, lookup).map_err(profile_error)?;
            let n = corners.len();
            let segments = if *closed { n } else { n - 1 };
            let mut pts = Vec::with_capacity(n);
            for k in 0..segments {
                let Some(&(a, segment)) = corners.get(k) else { return Err(PartError::Degenerate) };
                let Some(&(b, _)) = corners.get((k + 1) % n) else { return Err(PartError::Degenerate) };
                pts.push(a);
                crate::profile::segment_points(a, b, segment, &mut pts).map_err(profile_error)?;
            }
            if !*closed {
                let Some(&(last, _)) = corners.last() else { return Err(PartError::Degenerate) };
                pts.push(last);
            }
            let weight = ev_or("weight", weight, DEFAULT_WEIGHT)?;
            if !(weight.is_finite() && weight > 0.0) || pts.iter().any(|p| !p.is_finite()) {
                return Err(PartError::Degenerate);
            }
            out.strokes.push(SheetStroke {
                item: i,
                points: pts,
                closed: *closed,
                weight: weight.min(10.0),
                color: color.unwrap_or([0, 0, 0]),
                fill: if *closed { *fill } else { None },
            });
        }
        SheetShape::Text { x, y, text, size, anchor, rotation } => {
            let position = DVec2::new(ev("x", x)?, ev("y", y)?);
            let size = ev("size", size)?;
            let rotation_deg = ev_or("rotation", rotation, 0.0)?;
            if !(position.is_finite() && size.is_finite() && size > 0.0 && rotation_deg.is_finite()) {
                return Err(PartError::Degenerate);
            }
            out.texts.push(SheetText {
                item: i,
                position,
                text: text.chars().take(MAX_SHEET_TEXT).collect(),
                size,
                anchor: *anchor,
                rotation_deg,
            });
        }
        SheetShape::Image { x, y, width, height, image } => {
            let corner = DVec2::new(ev("x", x)?, ev("y", y)?);
            let size = DVec2::new(ev("width", width)?, ev("height", height)?);
            if !(corner.is_finite() && size.is_finite() && size.min_element() > 0.0) {
                return Err(PartError::Degenerate);
            }
            out.images.push(SheetImage { item: i, corner, size, image: image.clone() });
        }
    }
    Ok(())
}

/// Keep a definition's pieces to its kind and within the limits: title
/// blocks keep only sheet pieces, other kinds none. Returns the fixes.
pub fn repair_def(def: &mut ComponentDef) -> usize {
    let mut fixes = 0;
    if def.kind == ComponentKind::TitleBlock {
        fixes += def.items.len();
        def.items.clear();
        if def.sheet.len() > MAX_SHEET_PIECES {
            fixes += def.sheet.len() - MAX_SHEET_PIECES;
            def.sheet.truncate(MAX_SHEET_PIECES);
        }
        let before = def.sheet.len();
        def.sheet.retain(|item| match &item.shape {
            SheetShape::Stroke { points, .. } => (2..=MAX_STROKE_POINTS).contains(&points.len()),
            SheetShape::Text { text, .. } => text.chars().count() <= MAX_SHEET_TEXT,
            SheetShape::Image { image, .. } => image.is_valid(),
        });
        fixes += before - def.sheet.len();
    } else {
        fixes += def.sheet.len();
        def.sheet.clear();
    }
    fixes
}

/// Most bytes a field key may have (a longer `{…}` is not a field).
pub const MAX_FIELD_KEY: usize = 200;

/// A run of a title block text, as [`field_parts`] splits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldPart<'a> {
    /// Words as written (`raw`) and as shown (`shown`: `{{` and `}}` as one brace).
    Text { raw: &'a str, shown: &'a str },
    /// `{key}`: the key between the braces and the whole field as written.
    Field { key: &'a str, raw: &'a str },
}

/// Splits `text` into words and `{key}` fields, in order: `{{` and `}}`
/// are literal braces, and a `{` closed by no `}` within [`MAX_FIELD_KEY`]
/// bytes (before another `{`) is a brace like any other character.
pub fn field_parts<'a>(text: &'a str, mut f: impl FnMut(FieldPart<'a>)) {
    let mut rest = text;
    // `find` lands on ASCII braces, so every slice below is on a char boundary.
    while let Some(i) = rest.find(['{', '}']) {
        let (before, tail) = rest.split_at(i);
        if !before.is_empty() {
            f(FieldPart::Text { raw: before, shown: before });
        }
        if tail.starts_with("{{") || tail.starts_with("}}") {
            f(FieldPart::Text { raw: &tail[..2], shown: &tail[..1] });
            rest = &tail[2..];
            continue;
        }
        let body = &tail[1..];
        match body.find(['{', '}']) {
            Some(j) if tail.starts_with('{') && j <= MAX_FIELD_KEY && body[j..].starts_with('}') => {
                f(FieldPart::Field { key: &body[..j], raw: &tail[..j + 2] });
                rest = &body[j + 1..];
            }
            _ => {
                f(FieldPart::Text { raw: &tail[..1], shown: &tail[..1] });
                rest = body;
            }
        }
    }
    if !rest.is_empty() {
        f(FieldPart::Text { raw: rest, shown: rest });
    }
}

/// `text` with every `{param:old}` field reading `{param:new}`.
pub fn rename_field_param(text: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(text.len());
    field_parts(text, |part| match part {
        FieldPart::Field { key, .. } if key.trim().strip_prefix("param:").map(str::trim) == Some(old) => {
            out.push_str("{param:");
            out.push_str(new);
            out.push('}');
        }
        FieldPart::Text { raw, .. } | FieldPart::Field { raw, .. } => out.push_str(raw),
    });
    out
}

/// Whether `s` is a real `YYYY-MM-DD` date.
pub fn is_iso_date(s: &str) -> bool {
    parse_iso_date(s).is_some()
}

/// Year, month and day of a `YYYY-MM-DD` date: exactly four, two and two
/// ASCII digits.
pub fn parse_iso_date(s: &str) -> Option<(i32, u32, u32)> {
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| {
        b.get(r)?.iter().try_fold(0u32, |n, c| c.is_ascii_digit().then(|| n * 10 + u32::from(c - b'0')))
    };
    if b.len() != 10 || b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') {
        return None;
    }
    let y = i32::try_from(digits(0..4)?).ok()?;
    let (m, d) = (digits(5..7)?, digits(8..10)?);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    ((1..=9999).contains(&y) && (1..=days).contains(&d)).then_some((y, m, d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_standard_block_matches_the_old_layout() {
        let def = standard(&|k| crate::project::template_default(k).to_string());
        assert_eq!(def.kind, ComponentKind::TitleBlock);
        let paper = DVec2::new(420.0, 297.0);
        let r = def.resolve_sheet(paper);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        // The border, 10 mm in, and the 170 × 42 block in the bottom-right corner.
        let has_line = |a: DVec2, b: DVec2| {
            r.strokes.iter().any(|s| {
                s.points.windows(2).any(|w| (w[0] == a && w[1] == b) || (w[0] == b && w[1] == a))
                    || (s.closed && s.points.first() == Some(&a) && s.points.last() == Some(&b))
            })
        };
        assert!(has_line(DVec2::new(10.0, 10.0), DVec2::new(410.0, 10.0)));
        assert!(has_line(DVec2::new(240.0, 10.0), DVec2::new(240.0, 52.0)));
        let texts: Vec<&str> = r.texts.iter().map(|t| t.text.as_str()).collect();
        for f in ["{project_name}", "{sheet_name}", "{client}", "{sheet_number}"] {
            assert!(texts.contains(&f), "{f} in {texts:?}");
        }
        assert!(texts.contains(&"PROJETO"), "captions from the template keys");
        // The sheet number's baseline start, where the old block drew it.
        let number = r.texts.iter().find(|t| t.text == "{sheet_number}").unwrap();
        assert_eq!(number.position, DVec2::new(paper.x - 48.0, 10.0 + 21.0 - 6.0));
        assert_eq!((number.size, number.anchor), (6.0, TextAnchor::Left));
    }

    #[test]
    fn renames_follow_into_text_fields() {
        let text = |t: &str| SheetItem {
            name: "T".into(),
            shape: SheetShape::Text {
                x: "Old".into(),
                y: "0".into(),
                text: t.into(),
                size: "2".into(),
                anchor: TextAnchor::Left,
                rotation: String::new(),
            },
        };
        let mut def = block(vec![text("{param:Old} { param: Old } {{param:Old}} {param:Older} {Old} {param:Old")]);
        def.params.push(ComponentParam::number("Old", 1.0, ParamKind::Text));
        def.rename("Old", "New");
        let SheetShape::Text { x, text, .. } = &def.sheet[0].shape else { panic!() };
        assert_eq!(x, "New");
        assert_eq!(text, "{param:New} {param:New} {{param:Old}} {param:Older} {Old} {param:Old");
        assert_eq!(rename_field_param("é{param:A}ü", "A", "B"), "é{param:B}ü");
    }

    #[test]
    fn the_flattened_points_are_bounded() {
        let long = || {
            let points = (0..MAX_STROKE_POINTS).map(|k| ProfilePoint::new(&k.to_string(), "0")).collect();
            let shape = SheetShape::Stroke { points, closed: false, weight: String::new(), color: None, fill: None };
            SheetItem { name: "L".into(), shape }
        };
        let def = block((0..300).map(|_| long()).collect());
        let r = def.resolve_sheet(DVec2::new(420.0, 297.0));
        let kept = MAX_SHEET_POINTS / MAX_STROKE_POINTS;
        assert_eq!(r.strokes.len(), kept);
        assert_eq!(r.strokes.iter().map(|s| s.points.len()).sum::<usize>(), MAX_SHEET_POINTS);
        assert_eq!(r.errors.len(), 300 - kept);
        assert_eq!(r.errors[0], (kept, PartError::Degenerate));
    }

    #[test]
    fn iso_dates() {
        assert_eq!(parse_iso_date("2026-11-06"), Some((2026, 11, 6)));
        assert!(is_iso_date("2024-02-29") && !is_iso_date("2026-02-29") && !is_iso_date("06/11/2026"));
        for bad in [
            " 2026-11-06",
            "2026-11-06 ",
            "+2026-11-06",
            "2026-+11-06",
            "2026-11-+6",
            "2026-1-6",
            "0000-01-01",
            "２０２６-11-06",
        ] {
            assert!(!is_iso_date(bad), "{bad}");
        }
    }
    use crate::component::{ComponentDef, ComponentKind, ComponentParam, ParamKind};
    use crate::profile::ProfilePoint;

    fn line(a: (&str, &str), b: (&str, &str)) -> SheetItem {
        SheetItem {
            name: "L".into(),
            shape: SheetShape::Stroke {
                points: vec![ProfilePoint::new(a.0, a.1), ProfilePoint::new(b.0, b.1)],
                closed: false,
                weight: String::new(),
                color: None,
                fill: None,
            },
        }
    }

    fn block(sheet: Vec<SheetItem>) -> ComponentDef {
        ComponentDef { kind: ComponentKind::TitleBlock, sheet, ..Default::default() }
    }

    #[test]
    fn pieces_follow_the_paper() {
        let def = block(vec![line(("SheetWidth - 10", "10"), ("SheetWidth - 10", "SheetHeight - 10"))]);
        for paper in [DVec2::new(420.0, 297.0), DVec2::new(594.0, 841.0)] {
            let r = def.resolve_sheet(paper);
            assert!(r.errors.is_empty(), "{:?}", r.errors);
            let s = &r.strokes[0];
            assert_eq!(s.points, vec![DVec2::new(paper.x - 10.0, 10.0), DVec2::new(paper.x - 10.0, paper.y - 10.0)]);
            assert_eq!((s.weight, s.color, s.closed), (0.25, [0, 0, 0], false));
        }
    }

    #[test]
    fn params_texts_arcs_and_errors() {
        let mut def = block(vec![
            SheetItem {
                name: "T".into(),
                shape: SheetShape::Text {
                    x: "Margin".into(),
                    y: "20".into(),
                    text: "Folha {sheet_number}".into(),
                    size: "3.5".into(),
                    anchor: TextAnchor::Right,
                    rotation: String::new(),
                },
            },
            // A half circle of radius 10 from (0, 0) to (20, 0).
            SheetItem {
                name: "A".into(),
                shape: SheetShape::Stroke {
                    points: vec![ProfilePoint::arc("0", "0", "10"), ProfilePoint::new("20", "0")],
                    closed: false,
                    weight: "0.5".into(),
                    color: Some([200, 0, 0]),
                    fill: None,
                },
            },
            line(("Nope", "0"), ("1", "1")),
        ]);
        def.params.push(ComponentParam::number("Margin", 15.0, ParamKind::Number));
        let r = def.resolve_sheet(DVec2::new(420.0, 297.0));
        assert_eq!(r.texts[0].position, DVec2::new(15.0, 20.0));
        assert_eq!((r.texts[0].text.as_str(), r.texts[0].size), ("Folha {sheet_number}", 3.5));
        let arc = &r.strokes[0];
        assert!(arc.points.len() > 4, "flattened: {}", arc.points.len());
        assert!(arc.points.iter().all(|p| (p.distance(DVec2::new(10.0, 0.0)) - 10.0).abs() < 1e-6));
        assert_eq!((arc.weight, arc.color), (0.5, [200, 0, 0]));
        assert_eq!(r.errors.len(), 1);
        assert_eq!(r.errors[0].0, 2, "the bad piece is reported by index");
    }

    #[test]
    fn repair_keeps_title_blocks_2d_and_bounded() {
        let mut def = block((0..MAX_SHEET_PIECES + 5).map(|_| line(("0", "0"), ("1", "1"))).collect());
        def.items.push(crate::component::ComponentItem {
            name: "Box".into(),
            shape: crate::component::ComponentShape::Box {
                x: "0".into(),
                y: "0".into(),
                z: "0".into(),
                width: "1".into(),
                depth: "1".into(),
                height: "1".into(),
            },
            material: String::new(),
            in_plan: true,
            void: false,
            plan_only: false,
            detail: Default::default(),
        });
        assert_eq!(repair_def(&mut def), 6);
        assert_eq!((def.sheet.len(), def.items.len()), (MAX_SHEET_PIECES, 0));
        // Other kinds lose sheet pieces.
        let mut other = ComponentDef { sheet: vec![line(("0", "0"), ("1", "1"))], ..Default::default() };
        assert_eq!(repair_def(&mut other), 1);
        assert!(other.sheet.is_empty());
        assert_eq!(block(Vec::new()).category(), crate::Category::TitleBlock);
    }
}
