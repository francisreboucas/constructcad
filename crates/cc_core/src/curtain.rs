//! Curtain walls: walls made of a grid of panels and mullions.
//!
//! A wall type with a [`CurtainSystem`] (`TypeData::Wall::curtain`) is a
//! curtain wall type: its walls are not layered bodies but a grid laid over
//! the wall's face, vertical lines spaced along the wall and horizontal lines
//! up from its base, with a panel in each cell and mullions on the lines and
//! around the border. Each wall may adjust its grid ([`CurtainEdits`]): its
//! own lines instead of the type's, segments of lines removed (the cells on
//! both sides become one panel) and panels swapped.
//!
//! [`layout`] works out the grid of one wall in wall-local coordinates
//! (`u` along the wall's body centerline from its start, `z` absolute
//! elevations); `cc_geom::curtain` places it in the world.

use crate::ElementTypeId;
use serde::{Deserialize, Serialize};

/// Most grid lines on one axis of a wall (files are hostile input).
pub const MAX_LINES: usize = 500;
/// Smallest spacing a layout uses, in meters.
pub const MIN_SPACING: f64 = 0.05;
/// Positions within this distance are the same grid line.
pub const LINE_TOL: f64 = 1e-3;

/// How a grid spaces its lines over the length (or height) of a wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum GridLayout {
    /// No lines: one cell across.
    None,
    /// A line every `spacing`, from where `justification` says.
    #[default]
    FixedDistance,
    /// `count` equal divisions.
    FixedNumber,
    /// As few equal divisions as keep them at most `spacing` wide.
    MaxSpacing,
}

impl GridLayout {
    pub const ALL: [GridLayout; 4] =
        [GridLayout::None, GridLayout::FixedDistance, GridLayout::FixedNumber, GridLayout::MaxSpacing];

    pub fn key(self) -> &'static str {
        match self {
            GridLayout::None => "grid-layout-none",
            GridLayout::FixedDistance => "grid-layout-fixed-distance",
            GridLayout::FixedNumber => "grid-layout-fixed-number",
            GridLayout::MaxSpacing => "grid-layout-max-spacing",
        }
    }
}

/// Where a fixed-distance grid starts: the remainder goes to the other end
/// (or half to each end, centered).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum GridJustification {
    #[default]
    Start,
    Center,
    End,
}

impl GridJustification {
    pub const ALL: [GridJustification; 3] =
        [GridJustification::Start, GridJustification::Center, GridJustification::End];

    pub fn key(self) -> &'static str {
        match self {
            GridJustification::Start => "grid-justify-start",
            GridJustification::Center => "grid-justify-center",
            GridJustification::End => "grid-justify-end",
        }
    }
}

/// The lines of one direction of a curtain grid.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CurtainGrid {
    pub layout: GridLayout,
    /// Distance between lines (fixed distance) or the widest cell (maximum spacing).
    pub spacing: f64,
    /// Number of divisions (fixed number).
    pub count: u32,
    pub justification: GridJustification,
}

impl Default for CurtainGrid {
    fn default() -> Self {
        Self { layout: GridLayout::FixedDistance, spacing: 1.5, count: 2, justification: GridJustification::Start }
    }
}

impl CurtainGrid {
    /// The interior lines between `lo` and `hi`, ascending (at most [`MAX_LINES`]).
    pub fn lines(&self, lo: f64, hi: f64) -> Vec<f64> {
        let len = hi - lo;
        if !(len.is_finite() && len > 2.0 * LINE_TOL) {
            return Vec::new();
        }
        let spacing = if self.spacing.is_finite() { self.spacing.max(MIN_SPACING) } else { 1.5 };
        let equal = |n: usize| -> Vec<f64> {
            let n = n.clamp(1, MAX_LINES + 1);
            (1..n).map(|i| lo + len * i as f64 / n as f64).collect()
        };
        match self.layout {
            GridLayout::None => Vec::new(),
            GridLayout::FixedNumber => equal(self.count as usize),
            GridLayout::MaxSpacing => {
                let n = (len / spacing - 1e-9).ceil();
                equal(if n.is_finite() && n >= 1.0 { n.min(MAX_LINES as f64 + 1.0) as usize } else { 1 })
            }
            GridLayout::FixedDistance => {
                let n = ((len + LINE_TOL) / spacing).floor().clamp(0.0, MAX_LINES as f64 + 1.0) as usize;
                let rest = (len - n as f64 * spacing).max(0.0);
                let first = match self.justification {
                    GridJustification::Start => lo + spacing,
                    GridJustification::Center => lo + rest * 0.5 + if rest * 0.5 > LINE_TOL { 0.0 } else { spacing },
                    GridJustification::End => lo + rest + if rest > LINE_TOL { 0.0 } else { spacing },
                };
                (0..=n)
                    .map(|i| first + spacing * i as f64)
                    .filter(|&x| x > lo + LINE_TOL && x < hi - LINE_TOL)
                    .take(MAX_LINES)
                    .collect()
            }
        }
    }

    /// Fix non-finite or too small values; returns the number of repairs.
    pub fn repair(&mut self) -> usize {
        let mut n = 0;
        if !(self.spacing.is_finite() && self.spacing >= MIN_SPACING) {
            self.spacing = 1.5;
            n += 1;
        }
        if self.count == 0 || self.count as usize > MAX_LINES + 1 {
            self.count = self.count.clamp(1, MAX_LINES as u32 + 1);
            n += 1;
        }
        n
    }
}

/// What fills a cell of the grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum CurtainPanel {
    /// A glazed panel `thickness` thick.
    Glass { thickness: f64, material: String },
    /// An opaque panel `thickness` thick (spandrel, infill).
    Solid { thickness: f64, material: String },
    /// No panel: the cell is open.
    Empty,
    /// A component placed in the cell (a door or window component in its
    /// opening, other components from the cell's lower left corner), its
    /// `Width` and `Height` parameters set to the cell's size.
    Component { component: ElementTypeId },
}

impl Default for CurtainPanel {
    fn default() -> Self {
        CurtainPanel::Glass { thickness: 0.024, material: String::new() }
    }
}

impl CurtainPanel {
    pub fn key(&self) -> &'static str {
        match self {
            CurtainPanel::Glass { .. } => "panel-glass",
            CurtainPanel::Solid { .. } => "panel-solid",
            CurtainPanel::Empty => "panel-empty",
            CurtainPanel::Component { .. } => "panel-component",
        }
    }

    pub fn material(&self) -> Option<&str> {
        match self {
            CurtainPanel::Glass { material, .. } | CurtainPanel::Solid { material, .. } => Some(material),
            _ => None,
        }
    }

    pub fn thickness(&self) -> f64 {
        match self {
            CurtainPanel::Glass { thickness, .. } | CurtainPanel::Solid { thickness, .. } => *thickness,
            _ => 0.0,
        }
    }

    pub fn is_glazed(&self) -> bool {
        matches!(self, CurtainPanel::Glass { .. })
    }

    fn rename_material(&mut self, old: &str, new: &str) {
        if let CurtainPanel::Glass { material, .. } | CurtainPanel::Solid { material, .. } = self
            && material == old
        {
            *material = new.to_string();
        }
    }

    /// Fix a bad thickness, and a component that is no longer a type
    /// (`exists` says which are); returns the number of repairs.
    pub fn repair(&mut self, exists: &dyn Fn(ElementTypeId) -> bool) -> usize {
        match self {
            CurtainPanel::Glass { thickness, .. } | CurtainPanel::Solid { thickness, .. } => {
                if thickness.is_finite() && *thickness > 0.0 && *thickness <= 2.0 {
                    0
                } else {
                    *thickness = 0.024;
                    1
                }
            }
            CurtainPanel::Empty => 0,
            CurtainPanel::Component { component } => {
                if exists(*component) {
                    0
                } else {
                    *self = CurtainPanel::default();
                    1
                }
            }
        }
    }
}

/// A rectangular mullion section: `width` seen on the face of the wall,
/// `depth` across it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mullion {
    pub width: f64,
    pub depth: f64,
    pub material: String,
}

impl Default for Mullion {
    fn default() -> Self {
        Self { width: 0.05, depth: 0.15, material: String::new() }
    }
}

impl Mullion {
    fn repair(&mut self) -> usize {
        let mut n = 0;
        for (v, fallback) in [(&mut self.width, 0.05), (&mut self.depth, 0.15)] {
            if !(v.is_finite() && *v > 0.0 && *v <= 5.0) {
                *v = fallback;
                n += 1;
            }
        }
        n
    }
}

/// The curtain grid of a wall type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CurtainSystem {
    /// Vertical lines, spaced along the wall.
    pub vertical: CurtainGrid,
    /// Horizontal lines, spaced up from the wall's base.
    pub horizontal: CurtainGrid,
    /// What fills the cells.
    pub panel: CurtainPanel,
    /// Mullions on the interior vertical lines.
    pub vertical_mullion: Option<Mullion>,
    /// Mullions on the interior horizontal lines.
    pub horizontal_mullion: Option<Mullion>,
    /// Mullions around the border.
    pub border_mullion: Option<Mullion>,
}

impl Default for CurtainSystem {
    fn default() -> Self {
        Self {
            vertical: CurtainGrid::default(),
            horizontal: CurtainGrid { spacing: 1.5, ..Default::default() },
            panel: CurtainPanel::default(),
            vertical_mullion: Some(Mullion::default()),
            horizontal_mullion: Some(Mullion::default()),
            border_mullion: Some(Mullion::default()),
        }
    }
}

impl CurtainSystem {
    /// A system whose panels and mullions are of the given materials.
    pub fn with_materials(glass: &str, frame: &str) -> Self {
        let mut s =
            Self { panel: CurtainPanel::Glass { thickness: 0.024, material: glass.to_string() }, ..Self::default() };
        for m in [&mut s.vertical_mullion, &mut s.horizontal_mullion, &mut s.border_mullion].into_iter().flatten() {
            m.material = frame.to_string();
        }
        s
    }

    fn mullions(&self) -> impl Iterator<Item = &Mullion> {
        [&self.vertical_mullion, &self.horizontal_mullion, &self.border_mullion].into_iter().flatten()
    }

    /// Thickness of walls of this type: the deepest mullion or the panel (at least 1 cm).
    pub fn thickness(&self) -> f64 {
        self.mullions().map(|m| m.depth).fold(self.panel.thickness(), f64::max).max(0.01)
    }

    /// The material of the frame (the first mullion with one), else the panel's.
    pub fn main_material(&self) -> &str {
        self.mullions()
            .map(|m| m.material.as_str())
            .find(|m| !m.is_empty())
            .or_else(|| self.panel.material())
            .unwrap_or("")
    }

    /// Material names referenced.
    pub fn materials(&self) -> Vec<&str> {
        self.panel.material().into_iter().chain(self.mullions().map(|m| m.material.as_str())).collect()
    }

    pub fn rename_material(&mut self, old: &str, new: &str) {
        self.panel.rename_material(old, new);
        for m in
            [&mut self.vertical_mullion, &mut self.horizontal_mullion, &mut self.border_mullion].into_iter().flatten()
        {
            if m.material == old {
                m.material = new.to_string();
            }
        }
    }

    /// The component types used as panels.
    pub fn components(&self) -> Vec<ElementTypeId> {
        match self.panel {
            CurtainPanel::Component { component } => vec![component],
            _ => Vec::new(),
        }
    }

    pub fn repair(&mut self, exists: &dyn Fn(ElementTypeId) -> bool) -> usize {
        let mut n = self.vertical.repair() + self.horizontal.repair() + self.panel.repair(exists);
        for m in
            [&mut self.vertical_mullion, &mut self.horizontal_mullion, &mut self.border_mullion].into_iter().flatten()
        {
            n += m.repair();
        }
        n
    }
}

/// A direction of the grid: vertical lines (spaced along the wall) or
/// horizontal ones (spaced up it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GridAxis {
    Vertical,
    Horizontal,
}

/// A removed piece of an interior grid line: the line at `at` (`u` of a
/// vertical line, height above the base of a horizontal one), between the
/// two lines across it on both sides of `along` (a height for a vertical
/// line, a `u` for a horizontal one).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GridSegment {
    pub axis: GridAxis,
    pub at: f64,
    pub along: f64,
}

/// A panel other than the type's, in the cell (or merged cells) holding the
/// point `u` along the wall and `z` above its base.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelSwap {
    pub u: f64,
    pub z: f64,
    pub panel: CurtainPanel,
}

/// One wall's adjustments to its type's grid. Positions are wall-local:
/// `u` along the body centerline from its start, heights above its base.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CurtainEdits {
    /// The wall's own vertical lines instead of the type's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical: Option<Vec<f64>>,
    /// The wall's own horizontal lines instead of the type's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizontal: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<GridSegment>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub panels: Vec<PanelSwap>,
}

impl CurtainEdits {
    pub fn is_empty(&self) -> bool {
        self.vertical.is_none() && self.horizontal.is_none() && self.removed.is_empty() && self.panels.is_empty()
    }

    /// The same edits for the wall walked the other way (`length` long).
    pub fn reverse(&mut self, length: f64) {
        if let Some(lines) = &mut self.vertical {
            lines.iter_mut().for_each(|u| *u = length - *u);
            lines.sort_by(f64::total_cmp);
        }
        for s in &mut self.removed {
            match s.axis {
                GridAxis::Vertical => s.at = length - s.at,
                GridAxis::Horizontal => s.along = length - s.along,
            }
        }
        for p in &mut self.panels {
            p.u = length - p.u;
        }
    }

    pub fn materials(&self) -> impl Iterator<Item = &str> {
        self.panels.iter().filter_map(|p| p.panel.material())
    }

    pub fn rename_material(&mut self, old: &str, new: &str) {
        self.panels.iter_mut().for_each(|p| p.panel.rename_material(old, new));
    }

    /// The component types used as panels.
    pub fn components(&self) -> impl Iterator<Item = ElementTypeId> + '_ {
        self.panels.iter().filter_map(|p| match p.panel {
            CurtainPanel::Component { component } => Some(component),
            _ => None,
        })
    }

    /// Drop non-finite values and keep the lists bounded; returns the number of repairs.
    pub fn repair(&mut self, exists: &dyn Fn(ElementTypeId) -> bool) -> usize {
        let mut n = 0;
        for lines in [&mut self.vertical, &mut self.horizontal].into_iter().flatten() {
            let before = lines.len();
            lines.retain(|v| v.is_finite());
            lines.truncate(MAX_LINES);
            n += before - lines.len();
        }
        let before = self.removed.len() + self.panels.len();
        self.removed.retain(|s| s.at.is_finite() && s.along.is_finite());
        self.removed.truncate(4 * MAX_LINES);
        self.panels.retain(|p| p.u.is_finite() && p.z.is_finite());
        self.panels.truncate(4 * MAX_LINES);
        n += before - self.removed.len() - self.panels.len();
        for p in &mut self.panels {
            n += p.panel.repair(exists);
        }
        n
    }
}

/// Longest text [`from_text`] reads (files are hostile input).
pub const MAX_TEXT: usize = 1 << 20;

/// A curtain system or a wall's edits as text (for exchange files, such as
/// an IFC property). Component panels keep their type id, which only means
/// something in the same project.
pub fn to_text<T: Serialize>(value: &T) -> String {
    ron::to_string(value).unwrap_or_default()
}

/// Read what [`to_text`] wrote; `None` when it is too long or not valid.
pub fn from_text<T: serde::de::DeserializeOwned>(text: &str) -> Option<T> {
    if text.len() > MAX_TEXT {
        return None;
    }
    ron::from_str(text).ok()
}

/// A cell, or cells merged where segments were removed, and its panel.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelCell {
    /// Its lowest column and row (cells are numbered from the start and from the base).
    pub column: usize,
    pub row: usize,
    /// The span between the grid lines around it (absolute elevations).
    pub cell_u: (f64, f64),
    pub cell_z: (f64, f64),
    /// The panel's own extent: the cell less the mullions around it.
    pub u: (f64, f64),
    pub z: (f64, f64),
    pub panel: CurtainPanel,
    /// Swapped on this wall rather than the type's.
    pub swapped: bool,
}

impl PanelCell {
    pub fn area(&self) -> f64 {
        (self.u.1 - self.u.0).max(0.0) * (self.z.1 - self.z.0).max(0.0)
    }
}

/// A straight piece of mullion: along a vertical line at `at` from `span.0`
/// to `span.1` (heights), or along a horizontal line at height `at` from
/// `span.0` to `span.1` (along the wall); `offset` is where its section's
/// center lies from the line, along the wall or up.
#[derive(Debug, Clone, PartialEq)]
pub struct MullionPiece {
    pub axis: GridAxis,
    pub border: bool,
    pub at: f64,
    pub offset: f64,
    pub span: (f64, f64),
    pub mullion: Mullion,
}

impl MullionPiece {
    pub fn length(&self) -> f64 {
        (self.span.1 - self.span.0).max(0.0)
    }
}

/// The grid of one curtain wall.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    /// Vertical lines along the wall, both ends included.
    pub u: Vec<f64>,
    /// Horizontal lines (absolute elevations), base and top included.
    pub z: Vec<f64>,
    pub panels: Vec<PanelCell>,
    pub mullions: Vec<MullionPiece>,
}

impl Layout {
    pub fn columns(&self) -> usize {
        self.u.len().saturating_sub(1)
    }

    pub fn rows(&self) -> usize {
        self.z.len().saturating_sub(1)
    }

    /// The panel covering cell `(column, row)`.
    pub fn panel_at(&self, column: usize, row: usize) -> Option<&PanelCell> {
        let (cu, cz) = self.cell_center(column, row)?;
        self.panels.iter().find(|p| p.cell_u.0 <= cu && cu <= p.cell_u.1 && p.cell_z.0 <= cz && cz <= p.cell_z.1)
    }

    /// The center `(u, z)` of cell `(column, row)`.
    pub fn cell_center(&self, column: usize, row: usize) -> Option<(f64, f64)> {
        let (u0, u1) = (self.u.get(column)?, self.u.get(column + 1)?);
        let (z0, z1) = (self.z.get(row)?, self.z.get(row + 1)?);
        Some(((u0 + u1) * 0.5, (z0 + z1) * 0.5))
    }

    /// Interior lines of an axis (without the borders).
    pub fn interior(&self, axis: GridAxis) -> &[f64] {
        let lines = match axis {
            GridAxis::Vertical => &self.u,
            GridAxis::Horizontal => &self.z,
        };
        if lines.len() < 2 { &[] } else { &lines[1..lines.len() - 1] }
    }

    pub fn glazed_area(&self) -> f64 {
        self.panels.iter().filter(|p| p.panel.is_glazed()).map(PanelCell::area).sum()
    }

    pub fn mullion_length(&self) -> f64 {
        self.mullions.iter().map(MullionPiece::length).sum()
    }

    /// Panels that are there (not empty cells).
    pub fn panel_count(&self) -> usize {
        self.panels.iter().filter(|p| p.panel != CurtainPanel::Empty).count()
    }
}

/// The interior lines of a wall's grid on one axis: its own, else the type's.
fn axis_lines(grid: &CurtainGrid, own: Option<&Vec<f64>>, lo: f64, hi: f64) -> Vec<f64> {
    let mut lines = match own {
        Some(own) => {
            own.iter().copied().filter(|x| x.is_finite() && *x > lo + LINE_TOL && *x < hi - LINE_TOL).collect()
        }
        None => grid.lines(lo, hi),
    };
    lines.sort_by(f64::total_cmp);
    lines.dedup_by(|a, b| (*a - *b).abs() < LINE_TOL);
    lines.truncate(MAX_LINES);
    let mut all = Vec::with_capacity(lines.len() + 2);
    all.push(lo);
    all.extend(lines);
    all.push(hi);
    all
}

/// Index of the interval of `lines` holding `x` (clamped to the ends).
fn interval(lines: &[f64], x: f64) -> usize {
    let n = lines.len().saturating_sub(1);
    lines.iter().skip(1).position(|&l| x < l).unwrap_or(n.saturating_sub(1)).min(n.saturating_sub(1))
}

/// Disjoint sets over the cells, to merge them where segments are removed.
struct Merge {
    parent: Vec<usize>,
}

impl Merge {
    fn find(&mut self, mut i: usize) -> usize {
        while let Some(&p) = self.parent.get(i) {
            if p == i {
                break;
            }
            let gp = self.parent.get(p).copied().unwrap_or(p);
            if let Some(slot) = self.parent.get_mut(i) {
                *slot = gp;
            }
            i = p;
        }
        i
    }

    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b
            && let Some(slot) = self.parent.get_mut(a.max(b))
        {
            *slot = a.min(b);
        }
    }
}

/// The grid of a curtain wall of `system`, adjusted by `edits`, over `u`
/// (along the body centerline, between the ends after joins) and from
/// `base` to `top` (absolute elevations).
pub fn layout(system: &CurtainSystem, edits: &CurtainEdits, u: (f64, f64), base: f64, top: f64) -> Layout {
    if !(u.0.is_finite() && u.1.is_finite() && base.is_finite() && top.is_finite()) || u.1 <= u.0 || top <= base {
        return Layout::default();
    }
    let us = axis_lines(&system.vertical, edits.vertical.as_ref(), u.0, u.1);
    // Horizontal lines are kept as heights above the base.
    let own_z: Option<Vec<f64>> = edits.horizontal.as_ref().map(|h| h.iter().map(|z| base + z).collect());
    let zs = axis_lines(&system.horizontal, own_z.as_ref(), base, top);
    let (nu, nz) = (us.len() - 1, zs.len() - 1);
    let cell = |c: usize, r: usize| r * nu + c;

    // Removed segments of interior lines, as (line index, segment index).
    let line_index = |lines: &[f64], at: f64| {
        lines
            .iter()
            .enumerate()
            .skip(1)
            .take(lines.len().saturating_sub(2))
            .find_map(|(i, &l)| ((l - at).abs() < LINE_TOL * 2.0).then_some(i))
    };
    let mut v_removed = vec![false; (nu + 1) * nz];
    let mut h_removed = vec![false; (nz + 1) * nu];
    for s in &edits.removed {
        match s.axis {
            GridAxis::Vertical => {
                if let Some(i) = line_index(&us, s.at) {
                    let r = interval(&zs, base + s.along);
                    if let Some(slot) = v_removed.get_mut(r * (nu + 1) + i) {
                        *slot = true;
                    }
                }
            }
            GridAxis::Horizontal => {
                if let Some(j) = line_index(&zs, base + s.at) {
                    let c = interval(&us, s.along);
                    if let Some(slot) = h_removed.get_mut(j * nu + c) {
                        *slot = true;
                    }
                }
            }
        }
    }
    let v_gone = |i: usize, r: usize| v_removed.get(r * (nu + 1) + i).copied().unwrap_or(false);
    let h_gone = |j: usize, c: usize| h_removed.get(j * nu + c).copied().unwrap_or(false);

    // Cells merged across removed segments.
    let mut merge = Merge { parent: (0..nu * nz).collect() };
    for r in 0..nz {
        for i in 1..nu {
            if v_gone(i, r) {
                merge.union(cell(i - 1, r), cell(i, r));
            }
        }
    }
    for j in 1..nz {
        for c in 0..nu {
            if h_gone(j, c) {
                merge.union(cell(c, j - 1), cell(c, j));
            }
        }
    }

    // Mullions.
    let border = system.border_mullion.clone();
    let bw = border.as_ref().map_or(0.0, |m| m.width);
    let vm = system.vertical_mullion.clone();
    let hm = system.horizontal_mullion.clone();
    let vw = vm.as_ref().map_or(0.0, |m| m.width);
    let hw = hm.as_ref().map_or(0.0, |m| m.width);
    // Whether a vertical mullion stands on line `i` in row `r`.
    let v_at = |i: usize, r: usize| {
        if i == 0 || i == nu { border.is_some() } else { vm.is_some() && !v_gone(i, r) }
    };
    let h_at = |j: usize, c: usize| {
        if j == 0 || j == nz { border.is_some() } else { hm.is_some() && !h_gone(j, c) }
    };
    // How far a mullion on vertical line `i` reaches from the line into the cell beside it.
    let v_reach = |i: usize| if i == 0 || i == nu { bw } else { vw * 0.5 };
    let h_reach = |j: usize| if j == 0 || j == nz { bw } else { hw * 0.5 };

    let mut mullions = Vec::new();
    if let Some(b) = &border {
        for (i, offset) in [(0, bw * 0.5), (nu, -bw * 0.5)] {
            mullions.push(MullionPiece {
                axis: GridAxis::Vertical,
                border: true,
                at: us[i],
                offset,
                span: (base, top),
                mullion: b.clone(),
            });
        }
    }
    if let Some(m) = &vm {
        for (i, &line) in us.iter().enumerate().take(nu).skip(1) {
            for r in 0..nz {
                if v_gone(i, r) {
                    continue;
                }
                let lo = if r == 0 { zs[0] + bw } else { zs[r] };
                let hi = if r + 1 == nz { zs[nz] - bw } else { zs[r + 1] };
                if hi - lo > 1e-6 {
                    let piece = MullionPiece {
                        axis: GridAxis::Vertical,
                        border: false,
                        at: line,
                        offset: 0.0,
                        span: (lo, hi),
                        mullion: m.clone(),
                    };
                    // Pieces of one line in consecutive rows run on as one.
                    match mullions.last_mut() {
                        Some(last @ MullionPiece { axis: GridAxis::Vertical, border: false, .. })
                            if (last.at - piece.at).abs() < 1e-9 && (last.span.1 - lo).abs() < 1e-9 =>
                        {
                            last.span.1 = hi;
                        }
                        _ => mullions.push(piece),
                    }
                }
            }
        }
    }
    for (j, &height) in zs.iter().enumerate() {
        let (m, offset) = match j {
            0 => (border.as_ref(), bw * 0.5),
            _ if j == nz => (border.as_ref(), -bw * 0.5),
            _ => (hm.as_ref(), 0.0),
        };
        let Some(m) = m else { continue };
        for c in 0..nu {
            if !h_at(j, c) {
                continue;
            }
            // Between the vertical mullions at its ends (in the rows on both sides of the line).
            let stands = |i: usize| (j > 0 && v_at(i, j - 1)) || (j < nz && v_at(i, j));
            let lo = us[c] + if stands(c) { v_reach(c) } else { 0.0 };
            let hi = us[c + 1] - if stands(c + 1) { v_reach(c + 1) } else { 0.0 };
            if hi - lo > 1e-6 {
                mullions.push(MullionPiece {
                    axis: GridAxis::Horizontal,
                    border: j == 0 || j == nz,
                    at: height,
                    offset,
                    span: (lo, hi),
                    mullion: m.clone(),
                });
            }
        }
    }

    // Panels: one per group of merged cells that makes a rectangle, else one per cell.
    let mut groups: std::collections::BTreeMap<usize, Vec<(usize, usize)>> = Default::default();
    for r in 0..nz {
        for c in 0..nu {
            groups.entry(merge.find(cell(c, r))).or_default().push((c, r));
        }
    }
    let swap_in = |cu: (f64, f64), cz: (f64, f64)| {
        edits
            .panels
            .iter()
            .rev()
            .find(|p| cu.0 <= p.u && p.u <= cu.1 && cz.0 <= base + p.z && base + p.z <= cz.1)
            .map(|p| p.panel.clone())
    };
    let mut panels = Vec::new();
    for cells in groups.values() {
        let (c0, c1) = cells.iter().fold((usize::MAX, 0), |(a, b), &(c, _)| (a.min(c), b.max(c)));
        let (r0, r1) = cells.iter().fold((usize::MAX, 0), |(a, b), &(_, r)| (a.min(r), b.max(r)));
        let rect = cells.len() == (c1 - c0 + 1) * (r1 - r0 + 1);
        let pieces: Vec<(usize, usize, usize, usize)> =
            if rect { vec![(c0, c1, r0, r1)] } else { cells.iter().map(|&(c, r)| (c, c, r, r)).collect() };
        // A swap anywhere in the group applies to all of it.
        let swap = cells.iter().find_map(|&(c, r)| swap_in((us[c], us[c + 1]), (zs[r], zs[r + 1])));
        for (a, b, lo, hi) in pieces {
            let cell_u = (us[a], us[b + 1]);
            let cell_z = (zs[lo], zs[hi + 1]);
            let left = if v_at(a, lo) { v_reach(a) } else { 0.0 };
            let right = if v_at(b + 1, lo) { v_reach(b + 1) } else { 0.0 };
            let below = if h_at(lo, a) { h_reach(lo) } else { 0.0 };
            let above = if h_at(hi + 1, a) { h_reach(hi + 1) } else { 0.0 };
            let pu = (cell_u.0 + left, cell_u.1 - right);
            let pz = (cell_z.0 + below, cell_z.1 - above);
            if pu.1 - pu.0 <= 1e-6 || pz.1 - pz.0 <= 1e-6 {
                continue;
            }
            panels.push(PanelCell {
                column: a,
                row: lo,
                cell_u,
                cell_z,
                u: pu,
                z: pz,
                panel: swap.clone().unwrap_or_else(|| system.panel.clone()),
                swapped: swap.is_some(),
            });
        }
    }
    panels.sort_by_key(|p| (p.row, p.column));
    Layout { u: us, z: zs, panels, mullions }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &[f64], b: &[f64]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
    }

    #[test]
    fn grids_space_their_lines() {
        let g = |layout, spacing, count, justification| CurtainGrid { layout, spacing, count, justification };
        use GridJustification as J;
        use GridLayout as L;
        assert!(close(&g(L::FixedDistance, 1.5, 0, J::Start).lines(0.0, 5.0), &[1.5, 3.0, 4.5]));
        assert!(close(&g(L::FixedDistance, 1.5, 0, J::End).lines(0.0, 5.0), &[0.5, 2.0, 3.5]));
        assert!(close(&g(L::FixedDistance, 1.5, 0, J::Center).lines(0.0, 5.0), &[0.25, 1.75, 3.25, 4.75]));
        // A whole number of spacings leaves no sliver at either end.
        assert!(close(&g(L::FixedDistance, 1.0, 0, J::Center).lines(0.0, 3.0), &[1.0, 2.0]));
        assert!(close(&g(L::FixedDistance, 1.0, 0, J::End).lines(0.0, 3.0), &[1.0, 2.0]));
        assert!(close(&g(L::FixedNumber, 0.0, 4, J::Start).lines(2.0, 6.0), &[3.0, 4.0, 5.0]));
        assert!(close(&g(L::MaxSpacing, 1.2, 0, J::Start).lines(0.0, 5.0), &[1.0, 2.0, 3.0, 4.0]));
        assert!(g(L::None, 1.0, 3, J::Start).lines(0.0, 5.0).is_empty());
        // Hostile values stay bounded.
        assert!(g(L::FixedDistance, 1e-12, 0, J::Start).lines(0.0, 1e9).len() <= MAX_LINES);
        assert!(g(L::FixedNumber, 0.0, u32::MAX, J::Start).lines(0.0, 10.0).len() <= MAX_LINES);
        assert!(g(L::MaxSpacing, f64::NAN, 0, J::Start).lines(0.0, f64::INFINITY).is_empty());
    }

    fn system() -> CurtainSystem {
        let mut s = CurtainSystem::with_materials("Glass", "Steel");
        s.vertical = CurtainGrid { layout: GridLayout::FixedNumber, count: 3, ..Default::default() };
        s.horizontal = CurtainGrid { layout: GridLayout::FixedNumber, count: 2, ..Default::default() };
        s
    }

    #[test]
    fn layout_fills_cells_between_mullions() {
        let l = layout(&system(), &CurtainEdits::default(), (0.0, 3.0), 10.0, 12.0);
        assert!(close(&l.u, &[0.0, 1.0, 2.0, 3.0]) && close(&l.z, &[10.0, 11.0, 12.0]));
        assert_eq!(l.panels.len(), 6);
        let w = 0.05;
        // A corner panel stops at the border mullions and halfway into the interior ones.
        let p = &l.panels[0];
        assert!((p.u.0 - w).abs() < 1e-9 && (p.u.1 - (1.0 - w / 2.0)).abs() < 1e-9, "{p:?}");
        assert!((p.z.0 - (10.0 + w)).abs() < 1e-9 && (p.z.1 - (11.0 - w / 2.0)).abs() < 1e-9, "{p:?}");
        // Mullions: 2 border verticals, 2 interior verticals running the full height,
        // 3 columns x (bottom, top) borders and 3 interior horizontals.
        let count = |axis, border| l.mullions.iter().filter(|m| m.axis == axis && m.border == border).count();
        assert_eq!(count(GridAxis::Vertical, true), 2);
        assert_eq!(count(GridAxis::Vertical, false), 2);
        assert_eq!(count(GridAxis::Horizontal, true), 6);
        assert_eq!(count(GridAxis::Horizontal, false), 3);
        let interior = l.mullions.iter().find(|m| m.axis == GridAxis::Vertical && !m.border).map(|m| m.span);
        assert_eq!(interior, Some((10.0 + w, 12.0 - w)));
        // Glazed area: every panel's own extent.
        let expected: f64 = l.panels.iter().map(|p| (p.u.1 - p.u.0) * (p.z.1 - p.z.0)).sum();
        assert!((l.glazed_area() - expected).abs() < 1e-12);
        assert_eq!(l.panel_count(), 6);
    }

    #[test]
    fn removed_segments_merge_cells_and_swaps_follow_them() {
        let mut edits = CurtainEdits::default();
        // The vertical line at u = 1 loses its lower segment: cells (0, 0) and (1, 0) merge.
        edits.removed.push(GridSegment { axis: GridAxis::Vertical, at: 1.0, along: 0.5 });
        edits.panels.push(PanelSwap { u: 1.5, z: 0.5, panel: CurtainPanel::Empty });
        let l = layout(&system(), &edits, (0.0, 3.0), 0.0, 2.0);
        assert_eq!(l.panels.len(), 5);
        let merged = l.panel_at(0, 0).expect("merged panel");
        assert_eq!((merged.cell_u, merged.cell_z), ((0.0, 2.0), (0.0, 1.0)));
        assert_eq!(merged.panel, CurtainPanel::Empty);
        assert!(merged.swapped);
        assert_eq!(l.panel_at(1, 0), Some(merged));
        assert_eq!(l.panel_count(), 4);
        // The removed piece leaves the mullion on that line in the upper row only.
        let on_line: Vec<_> = l.mullions.iter().filter(|m| m.axis == GridAxis::Vertical && m.at == 1.0).collect();
        assert_eq!(on_line.len(), 1);
        assert!((on_line[0].span.0 - 1.0).abs() < 1e-9);
        // An L of merged cells is not a rectangle: one panel per cell, no mullion between them.
        edits.removed.push(GridSegment { axis: GridAxis::Horizontal, at: 1.0, along: 0.5 });
        let l = layout(&system(), &edits, (0.0, 3.0), 0.0, 2.0);
        assert_eq!(l.panels.len(), 6);
        assert!(l.panels.iter().filter(|p| p.swapped).count() == 3);
    }

    #[test]
    fn own_lines_replace_the_types_and_reverse() {
        let edits = CurtainEdits {
            vertical: Some(vec![2.5, 0.5, 9.0, f64::NAN, 0.5]),
            horizontal: Some(vec![]),
            ..Default::default()
        };
        let l = layout(&system(), &edits, (0.0, 3.0), 0.0, 2.0);
        assert!(close(&l.u, &[0.0, 0.5, 2.5, 3.0]) && close(&l.z, &[0.0, 2.0]));
        let mut e = CurtainEdits {
            vertical: Some(vec![0.5, 1.0]),
            panels: vec![PanelSwap { u: 0.2, z: 1.0, panel: CurtainPanel::Empty }],
            ..Default::default()
        };
        e.reverse(3.0);
        assert_eq!(e.vertical, Some(vec![2.0, 2.5]));
        assert!((e.panels[0].u - 2.8).abs() < 1e-12);
    }

    #[test]
    fn systems_and_edits_survive_as_text() {
        let system = system();
        let edits = CurtainEdits {
            vertical: Some(vec![0.5]),
            removed: vec![GridSegment { axis: GridAxis::Horizontal, at: 1.0, along: 0.2 }],
            panels: vec![PanelSwap { u: 0.2, z: 0.4, panel: CurtainPanel::Empty }],
            ..Default::default()
        };
        assert_eq!(from_text::<CurtainSystem>(&to_text(&system)), Some(system));
        assert_eq!(from_text::<CurtainEdits>(&to_text(&edits)), Some(edits));
        assert_eq!(from_text::<CurtainSystem>("not ron"), None);
        assert_eq!(from_text::<CurtainSystem>(&" ".repeat(MAX_TEXT + 1)), None);
    }

    #[test]
    fn bad_input_gives_an_empty_layout() {
        let e = CurtainEdits::default();
        assert_eq!(layout(&system(), &e, (1.0, 1.0), 0.0, 3.0), Layout::default());
        assert_eq!(layout(&system(), &e, (0.0, f64::NAN), 0.0, 3.0), Layout::default());
        assert_eq!(layout(&system(), &e, (0.0, 3.0), 3.0, 1.0), Layout::default());
    }
}
