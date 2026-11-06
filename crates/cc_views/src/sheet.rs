//! Sheet composition: the title block and the placed viewports, as one
//! drawing in paper millimeters (origin at the bottom-left corner). The
//! title block is a component type (`ComponentKind::TitleBlock`) whose 2D
//! pieces draw the border and the block; its texts' `{fields}` are filled
//! in from the sheet and the project. The application paints the result
//! and the exporters write it, so the screen and the PDF/SVG files match.

use crate::drawing::{Anchor, Pen};
use crate::{Drawing2D, LineStyle, Prim};
use cc_core::title_block::{SheetPieces, TextAnchor};
use cc_core::{Project, TypeData, View, ViewId, ViewKind};
use glam::DVec2;
use std::rc::Rc;

/// The margin of the standard title block's border, millimeters.
pub const MARGIN: f64 = 10.0;

/// What a sheet's fields read besides the model.
#[derive(Debug, Clone)]
pub struct SheetEnv {
    /// Today's date (year, month, day), for `{date}` without an issue date.
    pub today: (i32, u32, u32),
    /// Writes a date the way the interface language does.
    pub format_date: fn(i32, u32, u32) -> String,
    /// "As indicated", translated (`{sheet_scale}` with several scales).
    pub as_indicated: String,
}

/// A viewport resolved on the sheet (for selection and dragging).
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedViewport {
    pub index: usize,
    pub view: ViewId,
    /// Center and half extents of the drawing on the sheet, in millimeters.
    pub center: DVec2,
    pub half: DVec2,
}

/// A composed sheet.
#[derive(Debug, Clone, Default)]
pub struct Sheet {
    pub paper: DVec2,
    pub drawing: Drawing2D,
    pub viewports: Vec<PlacedViewport>,
}

/// Compose `sheet`; `drawing_of` returns the drawing of a placed view.
pub fn compose(
    project: &Project,
    sheet: &View,
    env: &SheetEnv,
    mut drawing_of: impl FnMut(ViewId) -> Option<Rc<Drawing2D>>,
) -> Option<Sheet> {
    let ViewKind::Sheet { viewports, title_block, .. } = &sheet.kind else { return None };
    let paper = sheet.paper_mm()?;
    let mut d = Drawing2D::default();
    let def = title_block.and_then(|t| match project.element_type(t).map(|t| &t.data) {
        Some(TypeData::Component { def, .. }) => Some(def),
        _ => None,
    });
    if let Some(def) = def {
        let ctx = crate::sheet_fields::FieldCtx {
            project,
            sheet,
            def: Some(def),
            today: env.today,
            format_date: env.format_date,
            as_indicated: &env.as_indicated,
        };
        draw_title_block(&mut d, &def.resolve_sheet(paper), &ctx);
    }

    let mut placed = Vec::new();
    for (index, vp) in viewports.iter().enumerate() {
        let Some(view) = project.view(vp.view) else { continue };
        let Some(drawing) = drawing_of(vp.view) else { continue };
        let (lo, hi) = drawing.bounds().unwrap_or((DVec2::ZERO, DVec2::ONE));
        let k = 1000.0 / view.scale.max(1) as f64;
        let anchor = (lo + hi) * 0.5;
        let map = |p: DVec2| vp.center + (p - anchor) * k;
        d.prims.extend(drawing.prims.iter().map(|prim| prim.mapped(map)));
        let half = (hi - lo) * 0.5 * k;
        // Viewport title: name, scale (not for 3D views) and an underline.
        let title = vp.center + DVec2::new(-half.x, -half.y - 7.0);
        d.text(title, &view.name, 3.0);
        if !matches!(view.kind, ViewKind::ThreeD { .. }) {
            d.text(title - DVec2::new(0.0, 4.0), format!("1:{}", view.scale), 2.2);
        }
        d.line(title + DVec2::new(0.0, -0.8), title + DVec2::new(half.x.min(60.0), -0.8), LineStyle::Cut, None);
        placed.push(PlacedViewport { index, view: vp.view, center: vp.center, half });
    }
    Some(Sheet { paper, drawing: d, viewports: placed })
}

/// Draws the resolved pieces of a title block: pictures first, then the
/// strokes (each fill under its own lines), then the texts with their
/// fields filled in.
fn draw_title_block(d: &mut Drawing2D, pieces: &SheetPieces, ctx: &crate::sheet_fields::FieldCtx) {
    for img in &pieces.images {
        d.prims.push(Prim::Image {
            origin: img.corner,
            u: DVec2::new(img.size.x, 0.0),
            v: DVec2::new(0.0, img.size.y),
            image: img.image.clone(),
            element: None,
            faded: false,
        });
    }
    for s in &pieces.strokes {
        if let Some([r, g, b]) = s.fill {
            d.fill(s.points.clone(), [r, g, b, 255], None);
        }
        let pen = Pen { color: Some(s.color), weight: Some(s.weight), ..Default::default() };
        let n = s.points.len();
        let segments = if s.closed && n > 2 { n } else { n.saturating_sub(1) };
        for k in 0..segments {
            let (Some(&a), Some(&b)) = (s.points.get(k), s.points.get((k + 1) % n)) else { continue };
            d.prims.push(Prim::Line { a, b, style: LineStyle::Cut, element: None, pen: Some(pen) });
        }
    }
    for t in &pieces.texts {
        let text = crate::sheet_fields::expand(&t.text, ctx);
        let angle = t.rotation_deg.to_radians();
        let (anchor, mid) = match t.anchor {
            TextAnchor::Left => (Anchor::LeftBottom, false),
            TextAnchor::Center => (Anchor::CenterBottom, false),
            TextAnchor::Right => (Anchor::RightBottom, false),
            TextAnchor::Middle => (Anchor::Center, true),
        };
        let lines: Vec<&str> = text.lines().collect();
        // One line down, 1.5 text heights, across the text's direction.
        let step = DVec2::from_angle(angle).perp() * (-1.5 * t.size);
        // Centered text is centered as a block; the others hang down from the first line.
        let first = if mid { -step * ((lines.len() as f64 - 1.0) / 2.0) } else { DVec2::ZERO };
        for (i, line) in lines.iter().enumerate().filter(|(_, l)| !l.is_empty()) {
            d.text_at(t.position + first + step * i as f64, *line, t.size, anchor, angle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::title_block::{SheetItem, SheetShape, TextAnchor};
    use cc_core::{ComponentDef, ComponentKind, ElementType, ElementTypeId, TypeData};

    fn env() -> SheetEnv {
        SheetEnv {
            today: (2026, 11, 6),
            format_date: |y, m, d| format!("{d:02}/{m:02}/{y}"),
            as_indicated: "Ind.".into(),
        }
    }

    #[test]
    fn the_title_block_is_drawn_with_its_fields() {
        let mut p = Project::from_template();
        let def = ComponentDef {
            kind: ComponentKind::TitleBlock,
            sheet: vec![
                SheetItem {
                    name: "Border".into(),
                    shape: SheetShape::Stroke {
                        points: ["10", "SheetWidth - 10"]
                            .iter()
                            .flat_map(|x| [cc_core::profile::ProfilePoint::new(*x, "10")])
                            .collect(),
                        closed: false,
                        weight: "0.7".into(),
                        color: None,
                        fill: None,
                    },
                },
                SheetItem {
                    name: "Number".into(),
                    shape: SheetShape::Text {
                        x: "SheetWidth - 12".into(),
                        y: "14".into(),
                        text: "{sheet_number}\n{date}".into(),
                        size: "5".into(),
                        anchor: TextAnchor::Right,
                        rotation: String::new(),
                    },
                },
            ],
            ..Default::default()
        };
        let tb = ElementTypeId(77_000);
        p.types.insert(
            tb,
            ElementType {
                id: tb,
                name: "TB".into(),
                data: TypeData::Component { component: "TB".into(), def },
                info: Default::default(),
                uid: Default::default(),
            },
        );
        let sheet = p.views.values_mut().find(|v| matches!(v.kind, ViewKind::Sheet { .. })).unwrap();
        if let ViewKind::Sheet { title_block, number, .. } = &mut sheet.kind {
            *title_block = Some(tb);
            *number = "A7".into();
        }
        let sheet = sheet.clone();
        let s = compose(&p, &sheet, &env(), |_| None).unwrap();
        let paper = s.paper;
        let texts: Vec<(&str, DVec2, Anchor)> = s
            .drawing
            .prims
            .iter()
            .filter_map(|q| match q {
                Prim::Text { text, pos, anchor, .. } => Some((text.as_str(), *pos, *anchor)),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&("A7", DVec2::new(paper.x - 12.0, 14.0), Anchor::RightBottom)), "{texts:?}");
        assert!(texts.iter().any(|t| t.0 == "06/11/2026" && t.1.y < 14.0), "second line below: {texts:?}");
        assert!(s.drawing.prims.iter().any(|q| matches!(q,
            Prim::Line { a, b, pen: Some(pen), .. }
                if *a == DVec2::new(10.0, 10.0) && *b == DVec2::new(paper.x - 10.0, 10.0) && pen.weight == Some(0.7))));
    }

    #[test]
    fn no_title_block_draws_the_blank_paper() {
        let p = Project::from_template();
        let mut sheet = p.views.values().find(|v| matches!(v.kind, ViewKind::Sheet { .. })).unwrap().clone();
        if let ViewKind::Sheet { title_block, .. } = &mut sheet.kind {
            *title_block = None;
        }
        let s = compose(&p, &sheet, &env(), |_| None).unwrap();
        assert!(s.drawing.prims.is_empty());
    }
}
