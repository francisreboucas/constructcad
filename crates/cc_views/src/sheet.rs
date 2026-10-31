//! Sheet composition: paper border, title block and the placed viewports,
//! as one drawing in paper millimeters (origin at the bottom-left corner).
//! The application paints it and the exporters write it, so the screen and
//! the PDF/SVG files match.

use crate::drawing::Anchor;
use crate::{Drawing2D, LineStyle};
use cc_core::{Project, View, ViewId, ViewKind};
use glam::DVec2;
use std::rc::Rc;

pub const TITLE_W: f64 = 170.0;
pub const TITLE_H: f64 = 42.0;
pub const MARGIN: f64 = 10.0;

/// Translated captions of the title block.
#[derive(Debug, Clone, Default)]
pub struct TitleLabels {
    pub project: String,
    pub sheet: String,
    pub client: String,
    pub number: String,
    pub project_number: String,
    pub author: String,
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
    labels: &TitleLabels,
    mut drawing_of: impl FnMut(ViewId) -> Option<Rc<Drawing2D>>,
) -> Option<Sheet> {
    let ViewKind::Sheet { number, size, viewports } = &sheet.kind else { return None };
    let paper = size.size_mm();
    let mut d = Drawing2D::default();
    let rect = |d: &mut Drawing2D, lo: DVec2, hi: DVec2, style| {
        d.polyline(&[lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)], true, style, None);
    };
    rect(&mut d, DVec2::splat(MARGIN), paper - DVec2::splat(MARGIN), LineStyle::Cut);
    title_block(&mut d, project, paper, number, &sheet.name, labels);

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

fn title_block(d: &mut Drawing2D, project: &Project, paper: DVec2, number: &str, name: &str, l: &TitleLabels) {
    let x1 = paper.x - MARGIN;
    let x0 = x1 - TITLE_W;
    let y0 = MARGIN;
    let y1 = y0 + TITLE_H;
    let p = DVec2::new;
    d.polyline(&[p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)], true, LineStyle::Projection, None);
    // The sheet number cell (right, lower two rows) is not split.
    d.line(p(x0, y0 + 14.0), p(x1 - 40.0, y0 + 14.0), LineStyle::Projection, None);
    d.line(p(x0, y0 + 28.0), p(x1, y0 + 28.0), LineStyle::Projection, None);
    d.line(p(x1 - 40.0, y0), p(x1 - 40.0, y0 + 28.0), LineStyle::Projection, None);
    let info = &project.info;
    // Captions sit at the top of each cell; values below them.
    let top_left = |d: &mut Drawing2D, at: DVec2, text: &str, size: f64| {
        d.text_at(at - DVec2::new(0.0, size), text, size, Anchor::LeftBottom, 0.0);
    };
    let rows = [
        (y1 - 3.0, &l.project, info.name.as_str(), 4.0),
        (y0 + 25.0, &l.sheet, name, 3.5),
        (y0 + 11.0, &l.client, info.client.as_str(), 3.0),
    ];
    for (y, caption, value, size) in rows {
        top_left(d, p(x0 + 2.0, y), caption, 2.0);
        top_left(d, p(x0 + 2.0, y - 3.0), value, size);
    }
    top_left(d, p(x1 - 38.0, y0 + 25.0), &l.number, 2.0);
    top_left(d, p(x1 - 38.0, y0 + 21.0), number, 6.0);
    top_left(d, p(x1 - 38.0, y0 + 11.0), &format!("{} {}", l.project_number, info.number), 2.2);
    top_left(d, p(x1 - 38.0, y0 + 6.0), &format!("{} {}", l.author, info.author), 2.2);
}
