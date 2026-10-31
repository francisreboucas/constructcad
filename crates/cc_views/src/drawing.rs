use cc_core::ElementId;
use glam::DVec2;

/// Graphic weight of a line, resolved to pen widths by the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineStyle {
    /// Element cut by the view plane.
    Cut,
    /// Visible element beyond the cut plane.
    Projection,
    /// Fine detail (layers, glass, door swings).
    Thin,
    /// Dashed: element above the cut plane or hidden.
    Hidden,
    /// Levels, section marks, text leaders.
    Annotation,
    /// Dash-dot: grid lines and center lines.
    Center,
    /// Background linework (the architecture under building services).
    Faint,
    /// Lines of a cut pattern (drawn like `Thin`; view graphics replace them
    /// when they override the cut pattern).
    Hatch,
}

/// Graphic overrides of one line (view graphics); `None` keeps the style's pen.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pen {
    pub color: Option<[u8; 3]>,
    /// Width on paper, millimeters.
    pub weight: Option<f64>,
    pub pattern: Option<cc_core::LinePattern>,
    /// Mixed with white.
    pub halftone: bool,
}

/// A line's pen resolved: width in paper millimeters, color and dash lengths
/// in paper millimeters (empty: continuous).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedPen {
    pub width: f64,
    pub color: [u8; 3],
    pub dashes: &'static [f64],
}

impl LineStyle {
    /// The default pen of the style: width (paper mm) and color.
    pub fn pen(self) -> (f64, [u8; 3]) {
        match self {
            LineStyle::Cut => (0.5, [10, 10, 10]),
            LineStyle::Projection => (0.25, [45, 45, 45]),
            LineStyle::Thin | LineStyle::Hatch => (0.13, [95, 95, 95]),
            LineStyle::Hidden => (0.18, [120, 120, 120]),
            LineStyle::Annotation => (0.18, [30, 90, 170]),
            LineStyle::Center => (0.18, [30, 90, 170]),
            LineStyle::Faint => (0.13, [175, 175, 175]),
        }
    }

    /// Dash pattern of the style, paper millimeters.
    pub fn dashes(self) -> &'static [f64] {
        match self {
            LineStyle::Hidden => cc_core::LinePattern::ShortDash.dashes_mm(),
            LineStyle::Center => cc_core::LinePattern::DashDot.dashes_mm(),
            _ => &[],
        }
    }

    /// The pen of a line of this style with its overrides.
    pub fn resolve(self, pen: Option<&Pen>) -> ResolvedPen {
        let (width, color) = self.pen();
        let mut out = ResolvedPen { width, color, dashes: self.dashes() };
        if let Some(p) = pen {
            if let Some(c) = p.color {
                out.color = c;
            }
            if let Some(w) = p.weight.filter(|w| w.is_finite() && *w > 0.0) {
                out.width = w.min(10.0);
            }
            if let Some(pattern) = p.pattern {
                out.dashes = pattern.dashes_mm();
            }
            if p.halftone {
                out.color = halftone(out.color);
            }
        }
        out
    }
}

/// A color mixed two thirds with white (faint drawing).
pub fn halftone(c: [u8; 3]) -> [u8; 3] {
    c.map(|v| (f64::from(v) + (255.0 - f64::from(v)) * 0.66).round() as u8)
}

/// Which point of a text box sits at its position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Anchor {
    #[default]
    LeftBottom,
    CenterBottom,
    Center,
}

/// Drawing primitive, in view coordinates (meters on the view plane).
#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    Line {
        a: DVec2,
        b: DVec2,
        style: LineStyle,
        element: Option<ElementId>,
        /// Overrides from the view's graphics.
        pen: Option<Pen>,
    },
    /// Simple polygon fill (any winding, may be concave).
    Fill {
        pts: Vec<DVec2>,
        color: [u8; 4],
        element: Option<ElementId>,
        /// A cut region (plans, sections), not a surface seen beyond the cut.
        cut: bool,
    },
    /// Text placed by `anchor` at `pos`, rotated by `angle` radians
    /// (counter-clockwise); `size` is paper height in millimeters.
    Text {
        pos: DVec2,
        text: String,
        size: f64,
        anchor: Anchor,
        angle: f64,
        /// The element the text annotates (hidden with it).
        element: Option<ElementId>,
    },
    /// A picture filling the parallelogram from `origin` (its bottom-left
    /// corner) along `u` (its width) and `v` (its height).
    Image {
        origin: DVec2,
        u: DVec2,
        v: DVec2,
        image: std::sync::Arc<cc_core::raster::RasterImage>,
        element: Option<ElementId>,
        /// Mixed two thirds with white.
        faded: bool,
    },
}

impl Prim {
    /// The primitive with its points moved by the affine map `f` (an image's
    /// sides follow from where its corners land).
    pub fn mapped(&self, f: impl Fn(DVec2) -> DVec2) -> Prim {
        match self {
            Prim::Line { a, b, style, element, pen } => {
                Prim::Line { a: f(*a), b: f(*b), style: *style, element: *element, pen: *pen }
            }
            Prim::Fill { pts, color, element, cut } => {
                Prim::Fill { pts: pts.iter().map(|p| f(*p)).collect(), color: *color, element: *element, cut: *cut }
            }
            Prim::Text { pos, text, size, anchor, angle, element } => Prim::Text {
                pos: f(*pos),
                text: text.clone(),
                size: *size,
                anchor: *anchor,
                angle: *angle,
                element: *element,
            },
            Prim::Image { origin, u, v, image, element, faded } => {
                let o = f(*origin);
                Prim::Image {
                    origin: o,
                    u: f(*origin + *u) - o,
                    v: f(*origin + *v) - o,
                    image: image.clone(),
                    element: *element,
                    faded: *faded,
                }
            }
        }
    }

    /// The four corners of an image (bottom-left first, counter-clockwise when not mirrored).
    pub fn image_corners(origin: DVec2, u: DVec2, v: DVec2) -> [DVec2; 4] {
        [origin, origin + u, origin + u + v, origin + v]
    }

    /// The element a primitive belongs to, if any.
    pub fn element(&self) -> Option<ElementId> {
        match self {
            Prim::Line { element, .. }
            | Prim::Fill { element, .. }
            | Prim::Text { element, .. }
            | Prim::Image { element, .. } => *element,
        }
    }
}

/// Ordered list of primitives, painted first to last.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Drawing2D {
    pub prims: Vec<Prim>,
    /// Fit the view to the geometry of elements only (plans: the markers of
    /// elevations and sections stand outside the model).
    pub fit_elements: bool,
    /// Elements drawn as background (imported CAD drawings): picked only
    /// where nothing else is.
    pub background: std::collections::BTreeSet<ElementId>,
    /// Tags that can be dragged: where each is and how far it was moved.
    pub tags: Vec<TagHandle>,
}

/// A tag of an element in a view, as drawn: picked within `half` of
/// `center`; `offset` is how far the view moved it from its default spot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TagHandle {
    pub element: ElementId,
    pub center: DVec2,
    pub half: DVec2,
    pub offset: DVec2,
}

impl Drawing2D {
    /// The tag at `p` (the last drawn on top), if any.
    pub fn tag_at(&self, p: DVec2) -> Option<TagHandle> {
        self.tags.iter().rev().find(|t| (p - t.center).abs().cmple(t.half).all()).copied()
    }
}

impl Drawing2D {
    pub fn line(&mut self, a: DVec2, b: DVec2, style: LineStyle, element: Option<ElementId>) {
        if a.distance_squared(b) > 1e-14 {
            self.prims.push(Prim::Line { a, b, style, element, pen: None });
        }
    }

    pub fn polyline(&mut self, pts: &[DVec2], closed: bool, style: LineStyle, element: Option<ElementId>) {
        for w in pts.windows(2) {
            self.line(w[0], w[1], style, element);
        }
        if closed && pts.len() > 2 {
            self.line(pts[pts.len() - 1], pts[0], style, element);
        }
    }

    pub fn fill(&mut self, pts: Vec<DVec2>, color: [u8; 4], element: Option<ElementId>) {
        if pts.len() >= 3 {
            self.prims.push(Prim::Fill { pts, color, element, cut: false });
        }
    }

    /// A fill of a cut region.
    pub fn cut_fill(&mut self, pts: Vec<DVec2>, color: [u8; 4], element: Option<ElementId>) {
        if pts.len() >= 3 {
            self.prims.push(Prim::Fill { pts, color, element, cut: true });
        }
    }

    pub fn text(&mut self, pos: DVec2, text: impl Into<String>, size: f64) {
        self.text_at(pos, text, size, Anchor::LeftBottom, 0.0);
    }

    pub fn text_at(&mut self, pos: DVec2, text: impl Into<String>, size: f64, anchor: Anchor, angle: f64) {
        self.prims.push(Prim::Text { pos, text: text.into(), size, anchor, angle, element: None });
    }

    /// Text that annotates `element` (a tag, a dimension value…).
    pub fn tag_text(
        &mut self,
        pos: DVec2,
        text: impl Into<String>,
        size: f64,
        anchor: Anchor,
        angle: f64,
        element: Option<ElementId>,
    ) {
        self.prims.push(Prim::Text { pos, text: text.into(), size, anchor, angle, element });
    }

    /// Circular arc approximated with line segments.
    pub fn arc(&mut self, center: DVec2, radius: f64, a0: f64, a1: f64, style: LineStyle, element: Option<ElementId>) {
        let steps = 16;
        let pts: Vec<DVec2> = (0..=steps)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f64 / steps as f64;
                center + DVec2::new(a.cos(), a.sin()) * radius
            })
            .collect();
        self.polyline(&pts, false, style, element);
    }

    /// Bounding box of geometry (text anchors included).
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let mut it = self.prims.iter().flat_map(|p| match p {
            Prim::Line { a, b, .. } => vec![*a, *b],
            Prim::Fill { pts, .. } => pts.clone(),
            Prim::Text { pos, .. } => vec![*pos],
            Prim::Image { origin, u, v, .. } => Prim::image_corners(*origin, *u, *v).to_vec(),
        });
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
    }

    /// The extents to fit the view to: [`bounds`](Self::bounds), or only the
    /// elements' geometry with `fit_elements` (all of it when there is none).
    pub fn fit_bounds(&self) -> Option<(DVec2, DVec2)> {
        if !self.fit_elements {
            return self.bounds();
        }
        let mut it = self.prims.iter().filter(|p| p.element().is_some()).flat_map(|p| match p {
            Prim::Line { a, b, .. } => vec![*a, *b],
            Prim::Fill { pts, .. } => pts.clone(),
            Prim::Text { pos, .. } => vec![*pos],
            Prim::Image { origin, u, v, .. } => Prim::image_corners(*origin, *u, *v).to_vec(),
        });
        match it.next() {
            Some(first) => Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p)))),
            None => self.bounds(),
        }
    }

    /// Topmost element under `p` within `tol` (view units). Background
    /// elements (imported drawings) are picked only where nothing else is.
    pub fn hit_test(&self, p: DVec2, tol: f64) -> Option<ElementId> {
        self.hit_test_tier(p, tol, false).or_else(|| self.hit_test_tier(p, tol, true))
    }

    /// The element whose line passes within `tol` of `p` (the nearest),
    /// leaving out fills and background elements: what a drag starting at
    /// `p` grabs.
    pub fn hit_test_line(&self, p: DVec2, tol: f64) -> Option<ElementId> {
        let mut best: Option<(f64, ElementId)> = None;
        for prim in &self.prims {
            if let Prim::Line { a, b, element: Some(id), .. } = prim
                && !self.background.contains(id)
            {
                let d = cc_geom::polygon::closest_on_segment(*a, *b, p).0.distance(p);
                if d <= tol && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, *id));
                }
            }
        }
        best.map(|(_, id)| id)
    }

    fn hit_test_tier(&self, p: DVec2, tol: f64, background: bool) -> Option<ElementId> {
        let tier = |id: &ElementId| self.background.contains(id) == background;
        // Lines first: they are thin and drawn over fills.
        let mut best: Option<(f64, ElementId)> = None;
        for prim in &self.prims {
            if let Prim::Line { a, b, element: Some(id), .. } = prim
                && tier(id)
            {
                let (q, _) = cc_geom::polygon::closest_on_segment(*a, *b, p);
                let d = q.distance(p);
                if d <= tol && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, *id));
                }
            }
        }
        if let Some((_, id)) = best {
            return Some(id);
        }
        self.prims.iter().rev().find_map(|prim| match prim {
            // Fully transparent fills only show selection (room areas); they are not picked.
            Prim::Fill { pts, element: Some(id), color, .. }
                if color[3] > 0 && tier(id) && cc_geom::polygon::contains(pts, p) =>
            {
                Some(*id)
            }
            Prim::Image { origin, u, v, element: Some(id), .. }
                if tier(id) && cc_geom::polygon::contains(&Prim::image_corners(*origin, *u, *v), p) =>
            {
                Some(*id)
            }
            _ => None,
        })
    }

    /// Elements taken by the rectangle `lo`..`hi`: with `fully` (a window)
    /// those whose lines, fills and texts all lie inside it, otherwise (a
    /// crossing) those that any of them reaches into: a line crossing it, a
    /// fill overlapping it, a text anchored in it. A crossing leaves out
    /// background elements (imported drawings), which lie under everything.
    pub fn elements_in(&self, lo: DVec2, hi: DVec2, fully: bool) -> Vec<ElementId> {
        use cc_geom::polygon::{contains, segment_meets_rect};
        use std::collections::BTreeMap;
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        let inside = |p: &DVec2| p.x >= lo.x && p.y >= lo.y && p.x <= hi.x && p.y <= hi.y;
        let center = (lo + hi) * 0.5;
        // Per element: (reaches in, lies wholly inside).
        let mut state: BTreeMap<ElementId, (bool, bool)> = BTreeMap::new();
        for prim in &self.prims {
            let (id, reaches, within) = match prim {
                Prim::Line { a, b, element: Some(id), .. } => {
                    (*id, segment_meets_rect(*a, *b, lo, hi), inside(a) && inside(b))
                }
                Prim::Fill { pts, color, element: Some(id), .. } => {
                    let edges = pts.iter().zip(pts.iter().cycle().skip(1));
                    let crosses = edges.take(pts.len()).any(|(a, b)| segment_meets_rect(*a, *b, lo, hi));
                    // A fill around the whole rectangle, unless it only shows
                    // when selected (room areas): dragging inside a room
                    // would otherwise always take it.
                    let around = color[3] > 0 && pts.len() >= 3 && contains(pts, center);
                    (*id, crosses || around || pts.iter().any(inside), !pts.is_empty() && pts.iter().all(inside))
                }
                Prim::Text { pos, element: Some(id), .. } => (*id, inside(pos), inside(pos)),
                Prim::Image { origin, u, v, element: Some(id), .. } => {
                    let c = Prim::image_corners(*origin, *u, *v);
                    let crosses = (0..4).any(|i| segment_meets_rect(c[i], c[(i + 1) % 4], lo, hi));
                    (*id, crosses || contains(&c, center) || c.iter().any(inside), c.iter().all(inside))
                }
                _ => continue,
            };
            let s = state.entry(id).or_insert((false, true));
            s.0 |= reaches;
            s.1 &= within;
        }
        state
            .into_iter()
            .filter(|(id, (any, all))| if fully { *all } else { *any && !self.background.contains(id) })
            .map(|(id, _)| id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64) -> DVec2 {
        DVec2::new(x, y)
    }

    /// A long wall line, a floor fill, a room area (transparent fill), a
    /// text note and an imported drawing's line.
    fn sample() -> Drawing2D {
        let mut d = Drawing2D::default();
        d.line(v(0.0, 0.0), v(10.0, 0.0), LineStyle::Cut, Some(ElementId(1)));
        let square = vec![v(20.0, 0.0), v(30.0, 0.0), v(30.0, 10.0), v(20.0, 10.0)];
        d.fill(square.clone(), [200, 200, 200, 255], Some(ElementId(2)));
        d.fill(square.iter().map(|p| *p + v(20.0, 0.0)).collect(), [0, 0, 0, 0], Some(ElementId(3)));
        d.prims.push(Prim::Text {
            pos: v(5.0, 5.0),
            text: "Nota".into(),
            size: 2.5,
            anchor: Anchor::LeftBottom,
            angle: 0.0,
            element: Some(ElementId(4)),
        });
        d.line(v(-50.0, -50.0), v(100.0, 50.0), LineStyle::Thin, Some(ElementId(5)));
        d.background.insert(ElementId(5));
        d
    }

    fn ids(list: Vec<ElementId>) -> Vec<u64> {
        list.into_iter().map(|e| e.0).collect()
    }

    /// Issue #93: a crossing that cuts a line between its ends, or lies
    /// inside a floor, takes the element; it used to look at ends only.
    #[test]
    fn a_crossing_takes_what_it_cuts_or_lies_in() {
        let d = sample();
        // Across the middle of the wall line, no end inside.
        assert_eq!(ids(d.elements_in(v(4.0, -1.0), v(6.0, 1.0), false)), [1]);
        // Inside the floor, touching none of its edges.
        assert_eq!(ids(d.elements_in(v(24.0, 4.0), v(26.0, 6.0), false)), [2]);
        // Inside a room area: rooms show only when selected, so not taken.
        assert!(d.elements_in(v(44.0, 4.0), v(46.0, 6.0), false).is_empty());
        // Around a text note's anchor.
        assert_eq!(ids(d.elements_in(v(4.0, 4.0), v(6.0, 6.0), false)), [4]);
        // The imported drawing's line passes through all of these, but a crossing leaves it out.
        assert!(!ids(d.elements_in(v(-60.0, -60.0), v(110.0, 60.0), false)).contains(&5));
    }

    #[test]
    fn a_window_takes_only_what_lies_wholly_inside() {
        let d = sample();
        assert!(d.elements_in(v(4.0, -1.0), v(6.0, 1.0), true).is_empty());
        assert_eq!(ids(d.elements_in(v(-1.0, -1.0), v(11.0, 6.0), true)), [1, 4]);
        // A window around everything takes the imported drawing too.
        assert_eq!(ids(d.elements_in(v(-60.0, -60.0), v(110.0, 60.0), true)), [1, 2, 3, 4, 5]);
    }

    /// A drag grabs an element by its lines only, never by a fill or an imported drawing.
    #[test]
    fn drags_grab_lines() {
        let d = sample();
        assert_eq!(d.hit_test_line(v(5.0, 0.05), 0.1), Some(ElementId(1)));
        assert_eq!(d.hit_test_line(v(25.0, 5.0), 0.1), None, "inside the floor");
        assert_eq!(d.hit_test(v(25.0, 5.0), 0.1), Some(ElementId(2)), "a click still picks it");
        assert_eq!(d.hit_test_line(v(25.0, 12.5), 0.1), None, "on the imported line");
    }

    /// Tags are picked within their box, the last drawn first.
    #[test]
    fn tags_are_picked_in_their_box() {
        let mut d = Drawing2D::default();
        let tag = |id: u64, x: f64| TagHandle {
            element: ElementId(id),
            center: v(x, 0.0),
            half: v(1.0, 0.5),
            offset: DVec2::ZERO,
        };
        d.tags = vec![tag(1, 0.0), tag(2, 1.5)];
        assert_eq!(d.tag_at(v(-0.9, 0.4)).map(|t| t.element.0), Some(1));
        assert_eq!(d.tag_at(v(0.8, 0.0)).map(|t| t.element.0), Some(2), "both boxes: the one on top");
        assert_eq!(d.tag_at(v(0.0, 0.6)), None);
    }
}
