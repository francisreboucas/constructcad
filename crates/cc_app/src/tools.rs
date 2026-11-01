//! Modeling tools: selection, walls, hosted openings, sketches, sections,
//! rooms, grids and dimensions.

use crate::paint::{HIGHLIGHT, PREVIEW, PaintOptions, Xf, paint_drawing};
use crate::state::AppState;
use crate::t;
use cc_core::{
    Category, ColumnSection, DimensionKind, ElementId, ElementKind, ElementTypeId, LevelId, Project, View, ViewId,
    ViewKind,
};
use cc_engine::Ref;
use cc_engine::commands::{create, modify as modify_cmd, views};
use cc_engine::default_top;
pub use cc_engine::walls_loop;
use cc_geom::{ModelGeometry, polygon};
use cc_views::{Drawing2D, format};
use egui::{Color32, Key, Painter, Pos2, Response, Stroke, Ui};
use glam::DVec2;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub enum Tool {
    Select,
    Wall {
        type_id: ElementTypeId,
    },
    Hosted {
        category: Category,
        type_id: ElementTypeId,
    },
    Sketch {
        category: Category,
        type_id: ElementTypeId,
    },
    Section,
    Room {
        type_id: ElementTypeId,
    },
    Grid {
        type_id: ElementTypeId,
    },
    Dimension {
        type_id: ElementTypeId,
        mode: DimensionMode,
    },
    Column {
        type_id: ElementTypeId,
    },
    Beam {
        type_id: ElementTypeId,
    },
    /// Click to place a pad footing; Space turns it by 90°.
    Footing {
        type_id: ElementTypeId,
    },
    /// Click the two ends of a truss (options in `AppState::truss`); `type_id` is its chord type.
    Truss {
        type_id: ElementTypeId,
    },
    Stair {
        type_id: ElementTypeId,
    },
    Ramp {
        type_id: ElementTypeId,
    },
    Railing {
        type_id: ElementTypeId,
    },
    /// Click the points of a swept profile's path (options in `AppState::sweep`).
    Sweep {
        type_id: ElementTypeId,
    },
    Component {
        type_id: ElementTypeId,
    },
    /// Click the ends of pipe runs, one after the other (options in `AppState::mep`).
    Pipe {
        type_id: ElementTypeId,
    },
    /// Click the ends of conduit runs, one after the other.
    Conduit {
        type_id: ElementTypeId,
    },
    Move,
    Copy,
    Rotate,
    /// Mirror across a picked line (`draw: false`) or a drawn axis.
    Mirror {
        draw: bool,
    },
    Array,
    TrimCorner,
    TrimExtend,
    Split,
    Offset,
    Align,
    /// Push or pull a face of a mass in a 3D view.
    PushPull,
    /// Click near a wall end to allow or disallow its join.
    ToggleJoin,
    /// Measure between two points; the result stays until the next pick.
    Measure,
    /// Pick an element, then others to join (or unjoin) their geometry with it.
    JoinGeometry {
        unjoin: bool,
    },
    /// Place the clipboard with a click.
    Paste,
    /// Pick an element, then click others to give them its type.
    MatchType,
    /// Place text notes (options in `AppState::text_options`).
    Text {
        type_id: ElementTypeId,
    },
    /// Add a leader to the text note `id`.
    AddLeader {
        id: ElementId,
    },
    /// Sketch an opening in `host`: a courtyard in a roof, or a shaft or
    /// stairwell in a floor (`floor`).
    Opening {
        host: ElementId,
        floor: bool,
    },
    /// Click elements to demolish them in the view's phase.
    Demolish,
}

impl Tool {
    /// Translation key of the tool name.
    pub fn key(&self) -> &'static str {
        match self {
            Tool::Select => "tool-select",
            Tool::Wall { .. } => "tool-wall",
            Tool::Hosted { category: Category::Door, .. } => "tool-door",
            Tool::Hosted { .. } => "tool-window",
            Tool::Sketch { category: Category::Ceiling, .. } => "tool-ceiling",
            Tool::Sketch { category: Category::Roof, .. } => "tool-roof",
            Tool::Sketch { category: Category::Foundation, .. } => "tool-foundation-slab",
            Tool::Sketch { category: Category::Mass, .. } => "tool-mass",
            Tool::Sketch { .. } => "tool-floor",
            Tool::Section => "tool-section",
            Tool::Room { .. } => "tool-room",
            Tool::Grid { .. } => "tool-grid",
            Tool::Dimension { mode, .. } => mode.key(),
            Tool::Column { .. } => "tool-column",
            Tool::Beam { .. } => "tool-beam",
            Tool::Footing { .. } => "tool-footing",
            Tool::Truss { .. } => "tool-truss",
            Tool::Stair { .. } => "tool-stair",
            Tool::Ramp { .. } => "tool-ramp",
            Tool::Railing { .. } => "tool-railing",
            Tool::Sweep { .. } => "tool-sweep",
            Tool::Component { .. } => "tool-component",
            Tool::Pipe { .. } => "tool-pipe",
            Tool::Conduit { .. } => "tool-conduit",
            Tool::Move => "tool-move",
            Tool::Copy => "tool-copy",
            Tool::Rotate => "tool-rotate",
            Tool::Mirror { draw: false } => "tool-mirror-pick",
            Tool::Mirror { draw: true } => "tool-mirror-draw",
            Tool::Array => "tool-array",
            Tool::TrimCorner => "tool-trim-corner",
            Tool::TrimExtend => "tool-trim-extend",
            Tool::Split => "tool-split",
            Tool::Offset => "tool-offset",
            Tool::Align => "tool-align",
            Tool::PushPull => "tool-push-pull",
            Tool::ToggleJoin => "tool-toggle-join",
            Tool::Measure => "tool-measure",
            Tool::JoinGeometry { unjoin: false } => "tool-join-geometry",
            Tool::JoinGeometry { unjoin: true } => "tool-unjoin-geometry",
            Tool::Paste => "action-paste",
            Tool::MatchType => "tool-match-type",
            Tool::Demolish => "tool-demolish",
            Tool::Text { .. } => "tool-text",
            Tool::AddLeader { .. } => "tool-add-leader",
            Tool::Opening { floor: false, .. } => "tool-roof-opening",
            Tool::Opening { floor: true, .. } => "tool-floor-opening",
        }
    }

    pub fn type_id(&self) -> Option<ElementTypeId> {
        match self {
            Tool::Wall { type_id }
            | Tool::Hosted { type_id, .. }
            | Tool::Sketch { type_id, .. }
            | Tool::Room { type_id }
            | Tool::Grid { type_id }
            | Tool::Dimension { type_id, .. }
            | Tool::Column { type_id }
            | Tool::Beam { type_id }
            | Tool::Footing { type_id }
            | Tool::Truss { type_id }
            | Tool::Stair { type_id }
            | Tool::Ramp { type_id }
            | Tool::Railing { type_id }
            | Tool::Sweep { type_id }
            | Tool::Component { type_id }
            | Tool::Pipe { type_id }
            | Tool::Conduit { type_id }
            | Tool::Text { type_id } => Some(*type_id),
            _ => None,
        }
    }

    pub fn category(&self) -> Option<Category> {
        match self {
            Tool::Wall { .. } => Some(Category::Wall),
            Tool::Hosted { category, .. } | Tool::Sketch { category, .. } => Some(*category),
            Tool::Room { .. } => Some(Category::Room),
            Tool::Grid { .. } => Some(Category::Grid),
            Tool::Dimension { .. } => Some(Category::Dimension),
            Tool::Column { .. } => Some(Category::Column),
            Tool::Beam { .. } => Some(Category::Beam),
            Tool::Footing { .. } => Some(Category::Foundation),
            Tool::Truss { .. } => Some(Category::Beam),
            Tool::Stair { .. } => Some(Category::Stair),
            Tool::Ramp { .. } => Some(Category::Ramp),
            Tool::Railing { .. } => Some(Category::Railing),
            Tool::Sweep { .. } => Some(Category::Sweep),
            Tool::Component { .. } => Some(Category::Component),
            Tool::Pipe { .. } => Some(Category::Pipe),
            Tool::Conduit { .. } => Some(Category::Conduit),
            Tool::Text { .. } => Some(Category::Text),
            _ => None,
        }
    }

    pub fn with_type(&self, id: ElementTypeId) -> Tool {
        match self.clone() {
            Tool::Wall { .. } => Tool::Wall { type_id: id },
            Tool::Hosted { category, .. } => Tool::Hosted { category, type_id: id },
            Tool::Sketch { category, .. } => Tool::Sketch { category, type_id: id },
            Tool::Room { .. } => Tool::Room { type_id: id },
            Tool::Grid { .. } => Tool::Grid { type_id: id },
            Tool::Dimension { mode, .. } => Tool::Dimension { type_id: id, mode },
            Tool::Column { .. } => Tool::Column { type_id: id },
            Tool::Beam { .. } => Tool::Beam { type_id: id },
            Tool::Footing { .. } => Tool::Footing { type_id: id },
            Tool::Truss { .. } => Tool::Truss { type_id: id },
            Tool::Stair { .. } => Tool::Stair { type_id: id },
            Tool::Ramp { .. } => Tool::Ramp { type_id: id },
            Tool::Railing { .. } => Tool::Railing { type_id: id },
            Tool::Sweep { .. } => Tool::Sweep { type_id: id },
            Tool::Component { .. } => Tool::Component { type_id: id },
            Tool::Pipe { .. } => Tool::Pipe { type_id: id },
            Tool::Conduit { .. } => Tool::Conduit { type_id: id },
            Tool::Text { .. } => Tool::Text { type_id: id },
            t => t,
        }
    }

    /// Tool for creating elements of `category` with its default type
    /// (foundations: a pad footing with the first pad type).
    pub fn for_category(project: &Project, category: Category) -> Option<Tool> {
        if category == Category::Foundation {
            return foundation_type(project, cc_core::FoundationKind::Pad).map(|type_id| Tool::Footing { type_id });
        }
        if category == Category::Rebar {
            // Bars are added to beams and columns, not drawn.
            return None;
        }
        let type_id = project.default_type(category)?;
        Some(match category {
            Category::Wall => Tool::Wall { type_id },
            Category::Door | Category::Window => Tool::Hosted { category, type_id },
            Category::Floor | Category::Ceiling | Category::Roof | Category::Mass => Tool::Sketch { category, type_id },
            Category::Room => Tool::Room { type_id },
            Category::Grid => Tool::Grid { type_id },
            Category::Dimension => Tool::Dimension { type_id, mode: DimensionMode::Aligned },
            Category::Column => Tool::Column { type_id },
            Category::Beam => Tool::Beam { type_id },
            Category::Foundation | Category::Rebar => Tool::Footing { type_id },
            Category::Stair => Tool::Stair { type_id },
            Category::Ramp => Tool::Ramp { type_id },
            Category::Railing => Tool::Railing { type_id },
            Category::Sweep => Tool::Sweep { type_id },
            Category::Component => Tool::Component { type_id },
            Category::Pipe => Tool::Pipe { type_id },
            Category::Conduit => Tool::Conduit { type_id },
            Category::Text => Tool::Text { type_id },
            // Imported, not drawn.
            Category::CadImport => return None,
        })
    }
}

/// What the dimension tool places.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DimensionMode {
    #[default]
    Aligned,
    /// Horizontal or vertical, chosen by where the line is placed.
    Linear,
    /// Running dimensions from an origin, horizontal or vertical.
    Ordinate,
    Angular,
    Radial,
    Diameter,
}

impl DimensionMode {
    pub const ALL: [DimensionMode; 6] = [
        DimensionMode::Aligned,
        DimensionMode::Linear,
        DimensionMode::Ordinate,
        DimensionMode::Angular,
        DimensionMode::Radial,
        DimensionMode::Diameter,
    ];

    pub fn key(self) -> &'static str {
        match self {
            DimensionMode::Aligned => "tool-dimension",
            DimensionMode::Linear => "tool-dim-linear",
            DimensionMode::Ordinate => "tool-dim-ordinate",
            DimensionMode::Angular => "tool-dim-angular",
            DimensionMode::Radial => "tool-dim-radial",
            DimensionMode::Diameter => "tool-dim-diameter",
        }
    }

    pub fn hint_key(self) -> &'static str {
        match self {
            DimensionMode::Aligned => "hint-dimension",
            DimensionMode::Linear => "hint-dim-linear",
            DimensionMode::Ordinate => "hint-dim-ordinate",
            DimensionMode::Angular => "hint-dim-angular",
            DimensionMode::Radial | DimensionMode::Diameter => "hint-dim-radial",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Drag {
    Box {
        start: Pos2,
    },
    /// An end handle of a wall, beam or grid; `fixed` is the other end.
    Grip {
        id: ElementId,
        at_start: bool,
        fixed: DVec2,
    },
    /// The curvature handle of an arc wall from `a` to `b`: the arc passes through the cursor.
    Bend {
        id: ElementId,
        a: DVec2,
        b: DVec2,
    },
    Move {
        start: DVec2,
        ids: Vec<ElementId>,
        copy: bool,
    },
    /// The tag of an element, as drawn when the drag started.
    Tag {
        tag: cc_views::TagHandle,
        start: DVec2,
    },
}

/// Transient state of the active tool.
#[derive(Debug, Clone, Default)]
pub struct Interaction {
    /// Lines under the measuring tool's first pick.
    pub targets_lines: Vec<(glam::DVec2, glam::DVec2)>,
    /// Wall chain anchor or section start.
    pub anchor: Option<DVec2>,
    /// Sketch points.
    pub points: Vec<DVec2>,
    pub flip_hand: bool,
    pub drag: Option<Drag>,
    /// Elements a transform tool acts on (the selection when it started).
    pub targets: Vec<ElementId>,
    /// Rotation center.
    pub center: Option<DVec2>,
    /// The next click sets the rotation center.
    pub placing_center: bool,
    /// Points acquired for alignment guides (see `snap`).
    pub tracked: Vec<DVec2>,
    /// Direction back along the previous segment of a chain (angle reference).
    pub prev_dir: Option<DVec2>,
    /// Rotation of the next component, degrees.
    pub rotation: f64,
    /// Height of the working plane of a tool in the 3D view (the first point picked).
    pub plane_z: Option<f64>,
    /// The dimension chain being continued (continuous dimensioning).
    pub dim_chain: Option<DimChain>,
    /// Push/pull: the mass face picked (the mass, the face, a point on it and its outward normal).
    pub push: Option<(ElementId, cc_core::mass::MassFace, glam::DVec3, glam::DVec3)>,
    /// The segments a chain tool placed, latest last (see [`chain_back`]).
    pub chain: Vec<ChainStep>,
    /// Segments taken back, latest last, for redo.
    pub chain_redo: Vec<ChainStep>,
}

/// Where a chain stands: its anchor, the direction angles are measured
/// from, and the elevation a pipe or conduit run goes on at.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ChainAt {
    anchor: Option<DVec2>,
    prev_dir: Option<DVec2>,
    mep: Option<f64>,
}

/// A segment a chain tool placed: the chain before and after it, its undo
/// steps (a pipe's rise and run are two), and the document revision it left
/// (or, taken back, the revision the undo left), so a step is only taken
/// back or redone when nothing else changed the model meanwhile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChainStep {
    before: ChainAt,
    after: ChainAt,
    steps: usize,
    revision: u64,
}

/// Most segments of a chain remembered.
const MAX_CHAIN: usize = 500;

fn chain_at(state: &AppState) -> ChainAt {
    ChainAt { anchor: state.interaction.anchor, prev_dir: state.interaction.prev_dir, mep: state.mep.chain }
}

fn set_chain_at(state: &mut AppState, at: ChainAt) {
    state.interaction.anchor = at.anchor;
    state.interaction.prev_dir = at.prev_dir;
    state.mep.chain = at.mep;
    state.mep.rise = None;
    state.typed.clear();
}

/// Remember the segment a chain tool just placed, from where the chain
/// stood (`before`) and the undo depth then.
fn chain_placed(state: &mut AppState, before: ChainAt, depth: usize) {
    let steps = state.session.doc.undo_depth().saturating_sub(depth);
    if steps == 0 {
        return;
    }
    let step = ChainStep { before, after: chain_at(state), steps, revision: state.session.doc.revision() };
    let chain = &mut state.interaction.chain;
    chain.push(step);
    if chain.len() > MAX_CHAIN {
        chain.remove(0);
    }
    state.interaction.chain_redo.clear();
}

/// The tools that place chains of segments.
fn chains(tool: &Tool) -> bool {
    matches!(
        tool,
        Tool::Wall { .. }
            | Tool::Beam { .. }
            | Tool::Pipe { .. }
            | Tool::Conduit { .. }
            | Tool::Railing { .. }
            | Tool::Sweep { .. }
    )
}

/// Backspace or Ctrl+Z while a chain tool is drawing: take back the last
/// segment (undoing it) and go on from where it started, without leaving
/// the tool; with no segment left, drop the start point. A railing's last
/// point and an arc wall's picked end go first. Returns whether it did
/// anything.
pub fn chain_back(state: &mut AppState) -> bool {
    if !chains(&state.tool) {
        return false;
    }
    // Points not placed yet: a railing's path, an arc wall's end.
    if state.interaction.points.pop().is_some() {
        return true;
    }
    if let Some(step) = state.interaction.chain.last().copied() {
        if state.session.doc.revision() == step.revision {
            for _ in 0..step.steps {
                crate::actions::run_command(state, cc_engine::commands::project::Undo {});
            }
            state.interaction.chain.pop();
            state.interaction.chain_redo.push(step);
            chain_synced(state);
            set_chain_at(state, step.before);
            state.selection.clear();
            return true;
        }
        // The model changed some other way: the chain's history no longer applies.
        state.interaction.chain.clear();
        state.interaction.chain_redo.clear();
    }
    if state.interaction.anchor.is_some() {
        set_chain_at(state, ChainAt::default());
        return true;
    }
    false
}

/// After stepping a chain back or forth (which changes the revision), the
/// next step either way is taken from the model as it now is.
fn chain_synced(state: &mut AppState) {
    let revision = state.session.doc.revision();
    let i = &mut state.interaction;
    for step in [i.chain.last_mut(), i.chain_redo.last_mut()].into_iter().flatten() {
        step.revision = revision;
    }
}

/// Ctrl+Y after [`chain_back`]: place the segment again and go on from its end.
pub fn chain_forward(state: &mut AppState) -> bool {
    if !chains(&state.tool) {
        return false;
    }
    let Some(step) = state.interaction.chain_redo.last().copied() else { return false };
    if state.session.doc.revision() != step.revision {
        state.interaction.chain_redo.clear();
        return false;
    }
    for _ in 0..step.steps {
        crate::actions::run_command(state, cc_engine::commands::project::Redo {});
    }
    state.interaction.chain_redo.pop();
    state.interaction.chain.push(step);
    chain_synced(state);
    set_chain_at(state, step.after);
    true
}

impl Interaction {
    pub fn is_idle(&self) -> bool {
        self.anchor.is_none() && self.points.is_empty() && self.drag.is_none() && self.dim_chain.is_none()
    }
}

/// A chain of dimensions being placed: the next one measures from `last`
/// (the origin of ordinate dimensions, which stays) along `dir` and sits on
/// the line through `line`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DimChain {
    pub view: ViewId,
    pub last: DVec2,
    pub dir: DVec2,
    pub line: DVec2,
    /// Linear (horizontal or vertical) rather than aligned dimensions.
    pub linear: bool,
    /// Running dimensions from the origin `last`.
    pub ordinate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SnapKind {
    Endpoint,
    Midpoint,
    /// The center of an arc or circle (imported drawings).
    Center,
    Nearest,
    /// On a guide from an acquired point.
    Alignment,
    /// Where two guides cross.
    Intersection,
    Free,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Snap {
    pub(crate) point: DVec2,
    kind: SnapKind,
    /// Dashed guides to draw: from the point that casts them to `point`.
    guides: [Option<(DVec2, DVec2)>; 2],
}

impl Snap {
    fn at(point: DVec2, kind: SnapKind) -> Self {
        Self { point, kind, guides: [None, None] }
    }

    /// A snap to `point`, shown as an end point when `on_point`, else free.
    pub(crate) fn plain(point: DVec2, on_point: bool) -> Self {
        Self::at(point, if on_point { SnapKind::Endpoint } else { SnapKind::Free })
    }
}

/// Points acquired for alignment, most recent first.
const MAX_TRACKED: usize = 6;

/// Snap through [`snap`] with the interaction's acquired points, and
/// acquire the point when it is a model point.
#[allow(clippy::too_many_arguments)]
pub(crate) fn snap_at(
    state: &mut AppState,
    level: Option<LevelId>,
    raw: DVec2,
    anchor: Option<DVec2>,
    extra: &[DVec2],
    scale: f64,
    ortho: bool,
) -> Snap {
    let cad = cad_snap(state, raw, SNAP_PX / scale * 1.5);
    let (project, geom) = (&state.session.doc.project, state.session.geom());
    let s = snap_with(project, geom, level, raw, anchor, extra, &state.interaction.tracked, scale, ortho, &cad);
    if matches!(s.kind, SnapKind::Endpoint | SnapKind::Midpoint | SnapKind::Center) {
        let tracked = &mut state.interaction.tracked;
        tracked.retain(|p| p.distance(s.point) > 1e-9);
        tracked.insert(0, s.point);
        tracked.truncate(MAX_TRACKED);
    }
    s
}

const SNAP_PX: f64 = 10.0;

/// Snap features of the imported drawings shown in the active view near `raw`.
fn cad_snap(state: &mut AppState, raw: DVec2, r: f64) -> cc_core::cad::CadSnap {
    let mut out = cc_core::cad::CadSnap::default();
    let Some(view) = state.active_view else { return out };
    let ids: Vec<ElementId> = state
        .project()
        .elements
        .values()
        .filter(|e| matches!(&e.kind, ElementKind::CadImport(c) if c.view == view))
        .map(|e| e.id)
        .collect();
    if ids.is_empty() {
        return out;
    }
    // Only drawings the view shows (not hidden by its graphics or temporarily).
    let Some(drawing) = state.drawing(view) else { return out };
    for id in ids {
        if !drawing.prims.iter().any(|p| p.element() == Some(id)) {
            continue;
        }
        if let Some(ElementKind::CadImport(c)) = state.project().element(id).map(|e| &e.kind) {
            let f = c.snap_features(raw, r);
            out.ends.extend(f.ends);
            out.mids.extend(f.mids);
            out.centers.extend(f.centers);
            out.lines.extend(f.lines);
            out.arcs.extend(f.arcs);
        }
    }
    out
}

/// Increment for rounding free coordinates at the current zoom.
fn round_step(scale: f64) -> f64 {
    [0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 5.0].into_iter().find(|s| s * scale >= 6.0).unwrap_or(10.0)
}

/// Snap `raw` to model points, wall lines, alignments and angle/length
/// increments. `tracked` are points acquired by hovering them earlier:
/// with the anchor and `extra` they cast horizontal, vertical and wall-line
/// guides; a crossing of two guides, or of a guide with the current
/// direction from the anchor, is a snap point.
/// The selection rectangle from `start` to `cur`: a window (solid outline)
/// dragged to the right, a crossing (dashed, green) dragged to the left.
pub(crate) fn select_box(painter: &egui::Painter, start: egui::Pos2, cur: egui::Pos2) {
    let rect = egui::Rect::from_two_pos(start, cur);
    let crossing = cur.x < start.x;
    let color = if crossing { Color32::from_rgb(30, 160, 60) } else { HIGHLIGHT };
    painter.rect_filled(rect, 0.0, color.gamma_multiply(0.08));
    if crossing {
        let c = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom(), rect.left_top()];
        painter.extend(egui::Shape::dashed_line(&c, Stroke::new(1.0, color), 5.0, 3.0));
    } else {
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, color), egui::StrokeKind::Middle);
    }
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn snap(
    project: &Project,
    geom: &ModelGeometry,
    level: Option<LevelId>,
    raw: DVec2,
    anchor: Option<DVec2>,
    extra: &[DVec2],
    tracked: &[DVec2],
    scale: f64,
    ortho: bool,
) -> Snap {
    let cad = cc_core::cad::CadSnap::default();
    snap_with(project, geom, level, raw, anchor, extra, tracked, scale, ortho, &cad)
}

/// [`snap`] with the snap features of imported drawings too (their
/// endpoints, midpoints, arc centers, lines and arcs).
#[allow(clippy::too_many_arguments)]
fn snap_with(
    project: &Project,
    geom: &ModelGeometry,
    level: Option<LevelId>,
    raw: DVec2,
    anchor: Option<DVec2>,
    extra: &[DVec2],
    tracked: &[DVec2],
    scale: f64,
    ortho: bool,
    cad: &cc_core::cad::CadSnap,
) -> Snap {
    let r = SNAP_PX / scale;
    let mut ends: Vec<DVec2> = extra.to_vec();
    let mut mids = Vec::new();
    let mut lines = Vec::new();
    let mut arcs: Vec<cc_core::arc::Arc> = Vec::new();
    ends.extend_from_slice(&cad.ends);
    mids.extend_from_slice(&cad.mids);
    lines.extend_from_slice(&cad.lines);
    arcs.extend_from_slice(&cad.arcs);
    for e in project.elements.values().filter(|e| Some(e.level) == level) {
        if let Some(w) = e.as_wall() {
            ends.extend([w.start, w.end]);
            mids.push(w.point_at(w.length() * 0.5));
            match w.arc() {
                Some(a) => arcs.push(a),
                None => lines.push((w.start, w.end)),
            }
        } else if let Some(s) = e.as_segment() {
            // Pipe and conduit ends, and runs to branch from.
            ends.extend([s.start, s.end]);
            if !s.is_vertical() {
                mids.push((s.start + s.end) * 0.5);
                lines.push((s.start, s.end));
            }
        } else if let cc_core::ElementKind::Component(c) = &e.kind
            && project.mep_data(e).is_some()
        {
            // Fixtures and devices, where their branches start.
            ends.push(c.position);
        } else if let Some(s) = e.sketch() {
            ends.extend_from_slice(s);
            lines.extend(closed_edges(s));
        } else if let ElementKind::Railing(r) = &e.kind {
            ends.extend_from_slice(&r.path);
            lines.extend(r.path.windows(2).map(|w| (w[0], w[1])));
        } else if let ElementKind::Sweep(sw) = &e.kind {
            ends.extend_from_slice(&sw.path);
            lines.extend(sw.path.windows(2).map(|w| (w[0], w[1])));
            if sw.closed
                && let (Some(a), Some(b)) = (sw.path.last(), sw.path.first())
            {
                lines.push((*a, *b));
            }
        }
        // Outlines as the plan draws them: corners are endpoints, edges lines.
        let outlines: Vec<Vec<DVec2>> = match (&e.kind, project.element_type(e.type_id).map(|t| &t.data)) {
            (ElementKind::Wall(_), _) => geom.walls.get(&e.id).filter(|w| w.arc.is_none()).map(|w| vec![w.footprint()]),
            (
                ElementKind::Column(c),
                Some(cc_core::TypeData::Column {
                    section: section @ (ColumnSection::Rectangular { .. } | ColumnSection::Profile { .. }),
                    ..
                }),
            ) => Some(cc_geom::section::column_loops(*section, c.position, c.rotation_deg)),
            (ElementKind::Stair(st), _) => cc_geom::stair_geom(project, e, st).map(|g| g.parts),
            (ElementKind::Ramp(r), _) => {
                cc_geom::ramp_geom(project, e, r).map(|g| g.pieces.into_iter().map(|p| p.outline).collect())
            }
            _ => None,
        }
        .unwrap_or_default();
        for o in outlines {
            ends.extend_from_slice(&o);
            lines.extend(closed_edges(&o));
        }
    }
    // Grid lines show on every level; their crossings are endpoints.
    let grids = project.elements.values().filter_map(|e| match &e.kind {
        ElementKind::Grid(g) if g.start.distance(g.end) > 1e-9 => Some((g.start, g.end)),
        _ => None,
    });
    lines.extend(grids);
    ends.extend(grid_crossings(project));
    // Risers of other floors passing through the level.
    if let Some(level) = level {
        let risers = project.segments_at(level).filter(|(e, _)| e.level != level);
        ends.extend(risers.map(|(_, s)| s.start));
    }
    let nearest = |pts: &[DVec2]| {
        pts.iter().map(|p| (*p, p.distance(raw))).filter(|(_, d)| *d <= r).min_by(|a, b| a.1.total_cmp(&b.1))
    };
    if let Some((p, _)) = nearest(&ends) {
        return Snap::at(p, SnapKind::Endpoint);
    }
    if let Some((p, _)) = nearest(&cad.centers) {
        return Snap::at(p, SnapKind::Center);
    }
    if let Some((p, _)) = nearest(&mids) {
        return Snap::at(p, SnapKind::Midpoint);
    }

    // Guides (origin, unit direction) passing near the pointer.
    let mut origins: Vec<DVec2> = tracked.iter().chain(extra).copied().chain(anchor).collect();
    origins.dedup_by(|a, b| a.distance(*b) < 1e-9);
    let mut guides: Vec<(DVec2, DVec2)> = Vec::new();
    for o in &origins {
        guides.push((*o, DVec2::X));
        guides.push((*o, DVec2::Y));
        for (a, b) in &lines {
            if a.distance(*o) < 1e-9 || b.distance(*o) < 1e-9 {
                let d = (*b - *a).normalize_or_zero();
                if d.x.abs() > 1e-6 && d.y.abs() > 1e-6 {
                    guides.push((*o, d));
                }
            }
        }
    }
    let off = |g: &(DVec2, DVec2), q: DVec2| (q - g.0).perp_dot(g.1).abs();
    guides.retain(|g| off(g, raw) <= r && g.0.distance(raw) > r);
    // Two guides crossing near the pointer.
    let mut best: Option<(Snap, f64)> = None;
    for (i, g) in guides.iter().enumerate() {
        for h in guides.iter().skip(i + 1) {
            if g.1.perp_dot(h.1).abs() < 1e-6 {
                continue;
            }
            let Some(t) = polygon::line_intersect(g.0, g.1, h.0, h.1) else { continue };
            let q = g.0 + g.1 * t;
            let d = q.distance(raw);
            if d <= r * 1.5 && best.is_none_or(|b| d < b.1) {
                let guides = [Some((g.0, q)), Some((h.0, q))];
                best = Some((Snap { point: q, kind: SnapKind::Intersection, guides }, d));
            }
        }
    }
    if let Some((s, _)) = best {
        return s;
    }

    let step = round_step(scale);
    if let Some(a) = anchor {
        let v = raw - a;
        let mut dir = v.normalize_or_zero();
        if ortho {
            dir = if v.x.abs() >= v.y.abs() { DVec2::new(v.x.signum(), 0.0) } else { DVec2::new(0.0, v.y.signum()) };
        } else {
            let ang = v.y.atan2(v.x).to_degrees();
            let snapped = (ang / 15.0).round() * 15.0;
            if (ang - snapped).abs() < 3.0 {
                dir = DVec2::from_angle(snapped.to_radians());
            }
        }
        let len = (v.dot(dir) / step).round() * step;
        let p = a + dir * len;
        // Prefer landing exactly on a wall line when close (T-junctions).
        if let Some(q) = on_lines(&lines, raw, r) {
            return Snap::at(q, SnapKind::Nearest);
        }
        // Or on an arc wall's line.
        let on_arc = arcs.iter().find_map(|arc| {
            let (u, v) = arc.locate(raw);
            (v.abs() <= r && (0.0..=arc.length()).contains(&u)).then(|| arc.point(u))
        });
        if let Some(q) = on_arc {
            return Snap::at(q, SnapKind::Nearest);
        }
        // The current direction meeting a guide from another point.
        let aligned = guides
            .iter()
            .filter(|g| g.0.distance(a) > 1e-9 && dir.perp_dot(g.1).abs() > 1e-6)
            .filter_map(|g| polygon::line_intersect(a, dir, g.0, g.1).map(|t| (a + dir * t, *g)))
            .filter(|(q, _)| (*q - a).dot(dir) > 0.0 && q.distance(raw) <= r * 1.5)
            .min_by(|x, y| x.0.distance(raw).total_cmp(&y.0.distance(raw)));
        if let Some((q, g)) = aligned {
            return Snap { point: q, kind: SnapKind::Alignment, guides: [Some((g.0, q)), None] };
        }
        return Snap::at(p, SnapKind::Free);
    }
    if let Some(q) = on_lines(&lines, raw, r) {
        return Snap::at(q, SnapKind::Nearest);
    }
    if let Some(g) = guides.iter().min_by(|x, y| off(x, raw).total_cmp(&off(y, raw))) {
        let t = ((raw - g.0).dot(g.1) / step).round() * step;
        let q = g.0 + g.1 * t;
        return Snap { point: q, kind: SnapKind::Alignment, guides: [Some((g.0, q)), None] };
    }
    Snap::at((raw / step).round() * step, SnapKind::Free)
}

fn on_lines(lines: &[(DVec2, DVec2)], raw: DVec2, r: f64) -> Option<DVec2> {
    lines
        .iter()
        .map(|(a, b)| polygon::closest_on_segment(*a, *b, raw).0)
        .map(|q| (q, q.distance(raw)))
        .filter(|(_, d)| *d <= r)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(q, _)| q)
}

/// Edges of a closed outline.
fn closed_edges(pts: &[DVec2]) -> impl Iterator<Item = (DVec2, DVec2)> + '_ {
    let n = pts.len();
    (0..if n > 2 { n } else { n.saturating_sub(1) }).filter_map(move |i| Some((*pts.get(i)?, *pts.get((i + 1) % n)?)))
}
pub(crate) fn draw_snap(painter: &Painter, xf: &Xf, s: Snap) {
    let c = xf.to_screen(s.point);
    let stroke = Stroke::new(1.5, PREVIEW);
    for (from, to) in s.guides.iter().flatten() {
        // Extend the guide a little past the snapped point.
        let (a, b) = (xf.to_screen(*from), xf.to_screen(*to));
        let past = b + (b - a).normalized() * 40.0;
        painter.extend(egui::Shape::dashed_line(&[a, past], Stroke::new(1.0, TEMP_DIM.gamma_multiply(0.7)), 6.0, 4.0));
        painter.circle_stroke(a, 3.0, Stroke::new(1.0, TEMP_DIM));
    }
    match s.kind {
        SnapKind::Intersection => {
            painter.line_segment([c + egui::vec2(-6.0, -6.0), c + egui::vec2(6.0, 6.0)], Stroke::new(2.0, PREVIEW));
            painter.line_segment([c + egui::vec2(-6.0, 6.0), c + egui::vec2(6.0, -6.0)], Stroke::new(2.0, PREVIEW));
        }
        SnapKind::Alignment => {
            painter.circle_stroke(c, 4.0, stroke);
        }
        SnapKind::Endpoint => {
            painter.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(10.0, 10.0)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        SnapKind::Midpoint => {
            let pts = vec![c + egui::vec2(0.0, -6.0), c + egui::vec2(6.0, 5.0), c + egui::vec2(-6.0, 5.0)];
            painter.add(egui::Shape::closed_line(pts, stroke));
        }
        SnapKind::Center => {
            painter.circle_stroke(c, 6.0, stroke);
            painter.line_segment([c + egui::vec2(-3.0, 0.0), c + egui::vec2(3.0, 0.0)], stroke);
            painter.line_segment([c + egui::vec2(0.0, -3.0), c + egui::vec2(0.0, 3.0)], stroke);
        }
        SnapKind::Nearest => {
            painter.line_segment([c + egui::vec2(-5.0, -5.0), c + egui::vec2(5.0, 5.0)], stroke);
            painter.line_segment([c + egui::vec2(-5.0, 5.0), c + egui::vec2(5.0, -5.0)], stroke);
        }
        SnapKind::Free => {
            painter.circle_stroke(c, 3.0, Stroke::new(1.0, PREVIEW));
        }
    }
}

pub(crate) fn label(painter: &Painter, pos: Pos2, text: String) {
    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(13.0), Color32::BLACK);
    let rect = egui::Rect::from_min_size(pos + egui::vec2(12.0, -24.0), galley.size()).expand(3.0);
    painter.rect_filled(rect, 3.0, Color32::from_rgba_unmultiplied(255, 250, 225, 235));
    painter.galley(rect.min + egui::vec2(3.0, 3.0), galley, Color32::BLACK);
}

/// Color of temporary dimensions.
const TEMP_DIM: Color32 = Color32::from_rgb(40, 110, 200);

/// Text on a light background, centered at `pos` and rotated by `angle`
/// (radians, screen orientation), kept upright.
fn dim_text(painter: &Painter, pos: Pos2, text: String, angle: f32, strong: bool) {
    let angle =
        if angle.abs() > std::f32::consts::FRAC_PI_2 { angle - angle.signum() * std::f32::consts::PI } else { angle };
    let font =
        if strong { egui::FontId::new(12.5, crate::theme::semibold()) } else { egui::FontId::proportional(12.0) };
    let galley = painter.layout_no_wrap(text, font, TEMP_DIM);
    let size = galley.size();
    let rot = egui::emath::Rot2::from_angle(angle);
    let top_left = pos - rot * (size / 2.0);
    let corners = [
        egui::vec2(-2.0, -1.0),
        egui::vec2(size.x + 2.0, -1.0),
        size + egui::vec2(2.0, 1.0),
        egui::vec2(-2.0, size.y + 1.0),
    ];
    let bg: Vec<Pos2> = corners.iter().map(|c| top_left + rot * *c).collect();
    painter.add(egui::Shape::convex_polygon(bg, Color32::from_rgba_unmultiplied(255, 255, 255, 230), Stroke::NONE));
    painter.add(egui::epaint::TextShape::new(top_left, galley, TEMP_DIM).with_angle(angle));
}

/// Temporary dimension measuring from `a` to `b` (both drawn as small
/// ticks on the measuring line). Returns the screen center, size and
/// rotation of its value, for clicking.
pub(crate) fn temp_measure(painter: &Painter, xf: &Xf, a: DVec2, b: DVec2) -> (Pos2, egui::Vec2, f32) {
    let (sa, sb) = (xf.to_screen(a), xf.to_screen(b));
    painter.line_segment([sa, sb], Stroke::new(1.0, TEMP_DIM));
    let d = (sb - sa).normalized();
    let n = egui::vec2(-d.y, d.x);
    for q in [sa, sb] {
        painter.line_segment([q - (d + n) * 4.0, q + (d + n) * 4.0], Stroke::new(1.5, TEMP_DIM));
    }
    let mut angle = d.y.atan2(d.x);
    if angle.abs() > std::f32::consts::FRAC_PI_2 {
        angle -= angle.signum() * std::f32::consts::PI;
    }
    let text = format::length(a.distance(b));
    let size = painter.layout_no_wrap(text.clone(), egui::FontId::proportional(12.0), TEMP_DIM).size();
    let up = egui::emath::Rot2::from_angle(angle) * egui::vec2(0.0, -(size.y / 2.0 + 3.0));
    let center = sa + (sb - sa) / 2.0 + up;
    dim_text(painter, center, text, angle, false);
    (center, size, angle)
}

/// Temporary dimension of a segment being drawn from `a` to `p`: a
/// dimension line beside it with its length (or the typed value) and an
/// arc with the angle to `reference` (the previous segment, or the X axis).
pub(crate) fn temp_dimension(
    painter: &Painter,
    xf: &Xf,
    a: DVec2,
    p: DVec2,
    reference: Option<DVec2>,
    typed: Option<&TypedInput>,
    side: f64,
) {
    let none = TypedInput::default();
    let typed = typed.unwrap_or(&none);
    let len = a.distance(p);
    let Some(dir) = (p - a).try_normalize() else { return };
    let thin = Stroke::new(1.0, TEMP_DIM);
    // Dimension line, offset to the left of the segment.
    let n = dir.perp();
    let offset = (side + 26.0 / xf.scale) * n;
    let gap = n * (4.0 / xf.scale);
    let over = n * (5.0 / xf.scale);
    let (da, dp) = (a + offset, p + offset);
    for q in [a, p] {
        painter.line_segment([xf.to_screen(q + n * side + gap), xf.to_screen(q + offset + over)], thin);
    }
    painter.line_segment([xf.to_screen(da), xf.to_screen(dp)], thin);
    // Architectural ticks.
    let tick = (dir + n).normalize_or_zero() * (5.0 / xf.scale);
    for q in [da, dp] {
        painter.line_segment([xf.to_screen(q - tick), xf.to_screen(q + tick)], Stroke::new(1.5, TEMP_DIM));
    }
    let unit = format::current().0.length.unit;
    let text = match (typed.length.is_empty(), unit.is_fractional()) {
        (true, _) => format::length(len),
        // Feet and inches carry their own marks.
        (false, true) => format!("{} \u{23ce}", typed.length),
        (false, false) => format!("{} {} \u{23ce}", typed.length, unit.symbol()),
    };
    let screen_dir = xf.to_screen(dp) - xf.to_screen(da);
    let mid = xf.to_screen((da + dp) * 0.5) + (xf.to_screen(da + n) - xf.to_screen(da)).normalized() * 9.0;
    dim_text(painter, mid, text, screen_dir.y.atan2(screen_dir.x), !typed.angle_active);

    // Angle to the reference direction.
    let reference = reference.and_then(|r| r.try_normalize()).unwrap_or(DVec2::X);
    let angle = reference.angle_to(dir);
    let degrees = angle.to_degrees();
    let radius_px = (len * xf.scale * 0.45).clamp(18.0, 70.0);
    if (len * xf.scale < 30.0 || degrees.abs() < 0.5) && !typed.angle_active {
        return;
    }
    let radius = radius_px / xf.scale;
    painter.extend(egui::Shape::dashed_line(
        &[xf.to_screen(a), xf.to_screen(a + reference * radius * 1.25)],
        thin,
        4.0,
        3.0,
    ));
    let steps = 24;
    let arc: Vec<Pos2> = (0..=steps)
        .map(|i| xf.to_screen(a + DVec2::from_angle(angle * i as f64 / steps as f64).rotate(reference) * radius))
        .collect();
    painter.add(egui::Shape::line(arc, thin));
    let label_at = xf.to_screen(a + DVec2::from_angle(angle / 2.0).rotate(reference) * (radius + 14.0 / xf.scale));
    let text =
        if typed.angle.is_empty() { format::angle(degrees.abs()) } else { format!("{}\u{b0} \u{23ce}", typed.angle) };
    dim_text(painter, label_at, text, 0.0, typed.angle_active);
}

/// Values typed while drawing a segment: digits go to the active field
/// (length, or angle after Tab), in the project's length unit and degrees.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TypedInput {
    pub length: String,
    pub angle: String,
    /// Typing goes to the angle.
    pub angle_active: bool,
}

impl TypedInput {
    pub fn is_empty(&self) -> bool {
        self.length.is_empty() && self.angle.is_empty()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn push(&mut self, ch: char) {
        if self.angle_active { &mut self.angle } else { &mut self.length }.push(ch);
    }

    pub fn pop(&mut self) {
        if self.angle_active { &mut self.angle } else { &mut self.length }.pop();
    }

    pub fn length_value(&self) -> Option<f64> {
        format::parse_length(&self.length).filter(|v| v.is_finite() && *v > 0.0)
    }

    pub fn angle_value(&self) -> Option<f64> {
        format::parse_number(&self.angle).filter(|v| v.is_finite())
    }

    /// The end of a segment from `a` toward the cursor `p`, with the typed
    /// length and the typed angle from `reference` (the X axis by default;
    /// the angle turns to the cursor's side of it).
    pub fn apply(&self, a: DVec2, p: DVec2, reference: Option<DVec2>) -> DVec2 {
        let reference = reference.and_then(|r| r.try_normalize()).unwrap_or(DVec2::X);
        let mut dir = (p - a).normalize_or(reference);
        if let Some(deg) = self.angle_value() {
            let side = if reference.perp_dot(dir) >= 0.0 { 1.0 } else { -1.0 };
            dir = DVec2::from_angle(side * deg.to_radians()).rotate(reference);
        }
        let len = self.length_value().unwrap_or_else(|| (p - a).dot(dir).max(0.0));
        a + dir * len
    }
}

pub fn handle(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    view: &View,
    xf: &Xf,
    drawing: &Drawing2D,
) {
    let is_plan = matches!(view.kind, ViewKind::FloorPlan { .. } | ViewKind::CeilingPlan { .. });
    let tool = state.tool.clone();
    match (&tool, view.level()) {
        (Tool::Select, _) => select(ui, resp, painter, state, is_plan, xf, drawing),
        // Matching types works in any 2D view.
        (Tool::MatchType, _) => crate::clipboard::match_type_tool(resp, painter, state, xf, drawing),
        // Demolishing works in plans, elevations and sections.
        (Tool::Demolish, _) => crate::phases_dialog::demolish_tool(resp, painter, state, view, xf, drawing),
        // Dimensions work in any 2D view.
        (Tool::Dimension { type_id, mode }, _) => {
            dimension_tool(ui, resp, painter, state, view, (*type_id, *mode), xf, drawing)
        }
        // Measuring too.
        (Tool::Measure, _) => measure_tool(ui, resp, painter, state, xf, drawing),
        // Joining geometry too (sections show it best).
        (Tool::JoinGeometry { unjoin }, _) => crate::modify::join_tool(resp, painter, state, xf, drawing, *unjoin),
        // Text notes too.
        (Tool::Text { type_id }, _) => {
            let px_per_mm = xf.scale * view.scale.max(1) as f64 / 1000.0;
            crate::text_tool::text_tool(ui, resp, painter, state, view, *type_id, xf, px_per_mm)
        }
        (Tool::AddLeader { id }, _) => {
            let px_per_mm = xf.scale * view.scale.max(1) as f64 / 1000.0;
            crate::text_tool::add_leader_tool(ui, resp, painter, state, view, *id, xf, px_per_mm)
        }
        (_, None) | (_, Some(_)) if !is_plan => {
            if resp.clicked() {
                state.message = t!("msg-plan-only");
            }
            select(ui, resp, painter, state, false, xf, drawing);
        }
        (Tool::Wall { type_id }, Some(level)) => wall_tool(ui, resp, painter, state, level, *type_id, xf),
        (Tool::Hosted { category, type_id }, Some(level)) => {
            hosted_tool(ui, resp, painter, state, level, *category, *type_id, xf)
        }
        (Tool::Sketch { .. }, Some(level)) => crate::sketch_mode::canvas(ui, resp, painter, state, level, xf),
        (Tool::Section, level) => section_tool(ui, resp, painter, state, level, xf),
        (Tool::Opening { host, .. }, Some(level)) => {
            if let Some(pts) = sketch_input(ui, resp, painter, state, level, xf) {
                add_opening(state, *host, pts);
            }
        }
        (Tool::Room { type_id }, Some(level)) => room_tool(resp, painter, state, level, *type_id, xf),
        (Tool::Grid { type_id }, Some(level)) => grid_tool(ui, resp, painter, state, level, *type_id, xf),
        (Tool::Column { type_id }, Some(level)) => column_tool(ui, resp, painter, state, level, *type_id, xf),
        (Tool::Beam { type_id }, Some(level)) => beam_tool(ui, resp, painter, state, level, *type_id, xf),
        (Tool::Footing { type_id }, Some(level)) => footing_tool(ui, resp, painter, state, level, *type_id, xf),
        (Tool::Truss { type_id }, Some(level)) => truss_tool(ui, resp, painter, state, level, *type_id, xf),
        (Tool::Stair { type_id }, Some(level)) => stair_tool(ui, resp, painter, state, level, *type_id, xf),
        (Tool::Ramp { type_id }, Some(level)) => ramp_tool(ui, resp, painter, state, level, *type_id, xf),
        (Tool::Railing { type_id }, _) if state.railing_host.on => {
            host_railing_tool(resp, painter, state, *type_id, xf, drawing);
        }
        (Tool::Railing { type_id }, Some(level)) => {
            path_tool(ui, resp, painter, state, level, *type_id, xf, false, finish_railing);
        }
        (Tool::Sweep { type_id }, Some(level)) => {
            let closed = state.sweep.closed;
            path_tool(ui, resp, painter, state, level, *type_id, xf, closed, finish_sweep);
        }
        (
            Tool::Move
            | Tool::Copy
            | Tool::Rotate
            | Tool::Mirror { .. }
            | Tool::Array
            | Tool::TrimCorner
            | Tool::TrimExtend
            | Tool::Split
            | Tool::Offset
            | Tool::Align
            | Tool::ToggleJoin,
            Some(level),
        ) => crate::modify::handle(ui, resp, painter, state, level, xf, drawing, &tool),
        (Tool::Paste, Some(level)) => crate::clipboard::paste_tool(ui, resp, painter, state, view.id, level, xf),
        (Tool::Component { type_id }, Some(level)) => component_tool(ui, resp, painter, state, level, *type_id, xf),
        (Tool::Pipe { type_id }, Some(level)) => segment_tool(ui, resp, painter, state, level, *type_id, true, xf),
        (Tool::Conduit { type_id }, Some(level)) => segment_tool(ui, resp, painter, state, level, *type_id, false, xf),
        _ => {}
    }
}

/// Run a creating command: the new element is selected; a refusal goes to the status bar.
fn run_create<C: cc_engine::Command + serde::Serialize>(state: &mut AppState, command: C) -> Option<ElementId> {
    let id = crate::actions::run_command(state, command)?["id"].as_u64().map(ElementId)?;
    state.selection = std::collections::BTreeSet::from([id]);
    Some(id)
}

/// Click to place a column (snaps to wall ends and grid crossings through the usual snaps).
fn column_tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    level: LevelId,
    type_id: ElementTypeId,
    xf: &Xf,
) {
    if resp.hovered() && ui.input(|i| i.key_pressed(Key::Space)) {
        state.interaction.rotation = (state.interaction.rotation + 90.0).rem_euclid(360.0);
    }
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let shift = ui.input(|i| i.modifiers.shift);
    let s = snap_at(state, Some(level), raw, None, &grid_crossings(state.project()), xf.scale, shift);
    draw_snap(painter, xf, s);
    if let Some(cc_core::TypeData::Column { section, .. }) = state.project().element_type(type_id).map(|t| &t.data) {
        let outline: Vec<Pos2> = cc_geom::column_outline(*section, s.point, state.interaction.rotation)
            .iter()
            .map(|p| xf.to_screen(*p))
            .collect();
        painter.add(egui::Shape::closed_line(outline, Stroke::new(1.5, PREVIEW)));
    }
    if resp.clicked() {
        let command = create::ColumnCreate {
            position: s.point.to_array(),
            rotation: Some(state.interaction.rotation),
            level: Some(Ref::Id(level.0)),
            type_ref: Some(Ref::Id(type_id.0)),
            ..Default::default()
        };
        run_create(state, command);
    }
}

/// Options of the pipe and conduit tools and of placing devices, kept between uses.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MepOptions {
    /// System of new pipes.
    pub system: Option<cc_core::SystemId>,
    /// Circuit of new conduits and devices.
    pub circuit: Option<cc_core::SystemId>,
    /// Elevation of new pipes and conduits above the level (`None`: the engine's default).
    pub offset: Option<f64>,
    /// Fall of new pipes, percent (`None`: the default for the system and diameter).
    pub slope: Option<f64>,
    /// Elevation where the next run of a chain starts (the end of the last one).
    pub chain: Option<f64>,
    /// Elevation a chain goes up or down to at its last point before the next
    /// run (a vertical run), set by changing the elevation while drawing.
    pub rise: Option<f64>,
}

/// The device kind of a component type, if it is an electrical device.
pub fn device_type(project: &Project, type_id: ElementTypeId) -> Option<cc_core::DeviceKind> {
    match project.element_type(type_id).map(|t| &t.data) {
        Some(cc_core::TypeData::Component { def, .. }) => def.mep.and_then(|m| m.device()),
        _ => None,
    }
}

/// The first component type that is an electrical device (`device`) or a plumbing fixture.
pub fn mep_component_type(project: &Project, device: bool) -> Option<ElementTypeId> {
    project
        .types_of(Category::Component)
        .find(|t| match &t.data {
            cc_core::TypeData::Component { def, .. } => match def.mep {
                Some(cc_core::MepData::Device { .. }) => device,
                Some(cc_core::MepData::Fixture { .. }) => !device,
                None => false,
            },
            _ => false,
        })
        .map(|t| t.id)
}

/// Clicks chain pipe or conduit runs: each ends where the next one starts,
/// at the elevation the last one reached (a sloped drain keeps falling). A
/// new elevation set while drawing makes a vertical run at the last point
/// first; clicking that point again makes only the vertical run.
#[allow(clippy::too_many_arguments)]
fn segment_tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    level: LevelId,
    type_id: ElementTypeId,
    pipe: bool,
    xf: &Xf,
) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let shift = ui.input(|i| i.modifiers.shift);
    let anchor = state.interaction.anchor;
    let s = snap_at(state, Some(level), raw, anchor, &[], xf.scale, shift);
    draw_snap(painter, xf, s);
    let project = state.project();
    let diameter = project.element_type(type_id).and_then(|t| t.diameter()).unwrap_or(0.025);
    let system = if pipe { state.mep.system } else { state.mep.circuit };
    let color = system.and_then(|id| project.system(id)).map_or(PREVIEW, |sys| {
        let [r, g, b] = sys.color();
        Color32::from_rgb(r, g, b)
    });
    if let Some(a) = anchor
        && state.mep.rise.is_some()
    {
        painter.circle_stroke(xf.to_screen(a), (diameter / 2.0 * xf.scale).max(4.0) as f32, Stroke::new(1.5, color));
    }
    if let Some(a) = anchor {
        let half = (diameter / 2.0).max(1.0 / xf.scale);
        let n = (s.point - a).normalize_or_zero().perp() * half;
        let pts = vec![xf.to_screen(a - n), xf.to_screen(s.point - n), xf.to_screen(s.point + n), xf.to_screen(a + n)];
        painter.add(egui::Shape::closed_line(pts, Stroke::new(1.5, color)));
        temp_dimension(painter, xf, a, s.point, None, None, half);
    }
    if !resp.clicked() {
        return;
    }
    let Some(a) = anchor else {
        state.interaction.anchor = Some(s.point);
        state.mep.chain = None;
        state.mep.rise = None;
        return;
    };
    let level_ref = Some(Ref::Id(level.0));
    let create = |state: &mut AppState, end: Option<DVec2>, offset: Option<f64>, end_offset: Option<f64>| {
        if pipe {
            let command = cc_engine::commands::mep::PipeCreate {
                start: a.to_array(),
                end: end.map(|p| p.to_array()),
                level: level_ref.clone(),
                type_ref: Some(Ref::Id(type_id.0)),
                system: state.mep.system.map(|id| Ref::Id(id.0)),
                offset,
                end_offset,
                slope: state.mep.slope,
            };
            crate::actions::run_command(state, command)
        } else {
            let command = cc_engine::commands::mep::ConduitCreate {
                start: a.to_array(),
                end: end.map(|p| p.to_array()),
                level: level_ref.clone(),
                type_ref: Some(Ref::Id(type_id.0)),
                circuits: state.mep.circuit.map(|id| Ref::Id(id.0)).into_iter().collect(),
                offset,
                end_offset,
            };
            crate::actions::run_command(state, command)
        }
    };
    let (before, depth) = (chain_at(state), state.session.doc.undo_depth());
    let select = |state: &mut AppState, out: &serde_json::Value| {
        if let Some(id) = out["id"].as_u64() {
            state.selection = BTreeSet::from([ElementId(id)]);
        }
    };
    if let (Some(to), Some(from)) = (state.mep.rise.take(), state.mep.chain)
        && (to - from).abs() > 1e-3
        && let Some(out) = create(state, None, Some(from), Some(to))
    {
        select(state, &out);
        state.mep.chain = Some(to);
    }
    if a.distance(s.point) <= 0.01 {
        return;
    }
    let offset = state.mep.chain.or(state.mep.offset);
    if let Some(out) = create(state, Some(s.point), offset, None) {
        select(state, &out);
        state.mep.chain = out["params"]["end_offset"].as_f64();
        state.interaction.anchor = Some(s.point);
    }
    chain_placed(state, before, depth);
}

/// Truss tool options, kept between uses.
#[derive(Debug, Clone, PartialEq)]
pub struct TrussOptions {
    /// Distance between the chord axes, meters.
    pub height: f64,
    /// Number of panels; 0 picks about one per 1.5 m.
    pub panels: u32,
    /// `pratt`, `howe` or `warren`.
    pub pattern: &'static str,
}

impl Default for TrussOptions {
    fn default() -> Self {
        Self { height: 1.0, panels: 0, pattern: "pratt" }
    }
}

/// The truss patterns, with their translation keys.
pub const TRUSS_PATTERNS: [(&str, &str); 3] =
    [("pratt", "truss-pratt"), ("howe", "truss-howe"), ("warren", "truss-warren")];

/// Two clicks: the ends of a truss, laid out with the options bar's height, panels and pattern.
fn truss_tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    level: LevelId,
    type_id: ElementTypeId,
    xf: &Xf,
) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let shift = ui.input(|i| i.modifiers.shift);
    let crossings = grid_crossings(state.project());
    let anchor = state.interaction.anchor;
    let s = snap_at(state, Some(level), raw, anchor, &crossings, xf.scale, shift);
    draw_snap(painter, xf, s);
    if let Some(a) = anchor {
        painter.line_segment([xf.to_screen(a), xf.to_screen(s.point)], Stroke::new(1.5, PREVIEW));
        temp_dimension(painter, xf, a, s.point, None, None, 0.1);
    }
    if resp.clicked() {
        match anchor {
            None => state.interaction.anchor = Some(s.point),
            Some(a) if a.distance(s.point) > 0.1 => {
                let options = state.truss.clone();
                let command = cc_engine::commands::framing::TrussCreate {
                    start: a.to_array(),
                    end: s.point.to_array(),
                    height: options.height,
                    panels: (options.panels > 0).then_some(options.panels),
                    pattern: Some(options.pattern.to_string()),
                    level: Some(Ref::Id(level.0)),
                    chord_type: Some(Ref::Id(type_id.0)),
                    ..Default::default()
                };
                if let Some(out) = crate::actions::run_command(state, command) {
                    let ids = out["created"].as_array().into_iter().flatten().filter_map(|v| v.as_u64());
                    state.selection = ids.map(ElementId).collect();
                }
                state.interaction.anchor = None;
            }
            _ => {}
        }
    }
}

/// The first foundation type of `kind`.
pub fn foundation_type(project: &Project, kind: cc_core::FoundationKind) -> Option<ElementTypeId> {
    project
        .types_of(Category::Foundation)
        .find(|t| matches!(t.data, cc_core::TypeData::Foundation { kind: k, .. } if k == kind))
        .map(|t| t.id)
}

/// Click to place a pad footing; Space turns it by 90°.
fn footing_tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    level: LevelId,
    type_id: ElementTypeId,
    xf: &Xf,
) {
    if resp.hovered() && ui.input(|i| i.key_pressed(Key::Space)) {
        state.interaction.rotation = (state.interaction.rotation + 90.0).rem_euclid(360.0);
    }
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let shift = ui.input(|i| i.modifiers.shift);
    let s = snap_at(state, Some(level), raw, None, &grid_crossings(state.project()), xf.scale, shift);
    draw_snap(painter, xf, s);
    if let Some(cc_core::TypeData::Foundation { width, length, .. }) =
        state.project().element_type(type_id).map(|t| &t.data)
    {
        let section = cc_core::ColumnSection::Rectangular { width: *width, depth: *length };
        let outline: Vec<Pos2> = cc_geom::column_outline(section, s.point, state.interaction.rotation)
            .iter()
            .map(|p| xf.to_screen(*p))
            .collect();
        painter.add(egui::Shape::closed_line(outline, Stroke::new(1.5, PREVIEW)));
    }
    if resp.clicked() {
        let command = cc_engine::commands::foundation::FootingCreate {
            position: s.point.to_array(),
            rotation: Some(state.interaction.rotation),
            level: Some(Ref::Id(level.0)),
            type_ref: Some(Ref::Id(type_id.0)),
            ..Default::default()
        };
        run_create(state, command);
    }
}

/// Click to place a component; Space turns it by 90°.
fn component_tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    level: LevelId,
    type_id: ElementTypeId,
    xf: &Xf,
) {
    if resp.hovered() && ui.input(|i| i.key_pressed(Key::Space)) {
        state.interaction.rotation = (state.interaction.rotation + 90.0).rem_euclid(360.0);
    }
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let shift = ui.input(|i| i.modifiers.shift);
    let s = snap_at(state, Some(level), raw, None, &[], xf.scale, shift);
    draw_snap(painter, xf, s);
    let rotation = state.interaction.rotation;
    if let Some(cc_core::TypeData::Component { def, .. }) = state.project().element_type(type_id).map(|t| &t.data) {
        let rot = DVec2::from_angle(rotation.to_radians());
        let (parts, _) = def.resolve(&Default::default());
        for part in parts.iter().filter(|p| p.in_plan && !p.void) {
            let outline: Vec<Pos2> = part.footprint.iter().map(|p| xf.to_screen(s.point + rot.rotate(*p))).collect();
            painter.add(egui::Shape::closed_line(outline, Stroke::new(1.5, PREVIEW)));
        }
    }
    if resp.clicked() {
        let command = create::ComponentPlace {
            position: s.point.to_array(),
            type_ref: Ref::Id(type_id.0),
            rotation: Some(rotation),
            level: Some(Ref::Id(level.0)),
            base_offset: None,
            system: device_type(state.project(), type_id).and(state.mep.circuit).map(|s| Ref::Id(s.0)),
        };
        run_create(state, command);
    }
}

/// Intersections of grid lines, offered as snap points.
pub(crate) fn grid_crossings(project: &Project) -> Vec<DVec2> {
    let grids: Vec<(DVec2, DVec2)> = project
        .elements
        .values()
        .filter_map(|e| match &e.kind {
            ElementKind::Grid(g) => Some((g.start, g.end)),
            _ => None,
        })
        .collect();
    let mut points = Vec::new();
    for (i, &(a, b)) in grids.iter().enumerate() {
        for &(c, d) in grids.iter().skip(i + 1) {
            if let Some(t) = polygon::line_intersect(a, b - a, c, d - c)
                && (0.0..=1.0).contains(&t)
            {
                points.push(a + (b - a) * t);
            }
        }
    }
    points
}

/// A grid end being drawn or dragged lines up with the ends of the other
/// grids (`cc_core::grid::align_end`, within the snap distance): `s` moves
/// onto the bubble line of a parallel grid's end, drawn as a guide from that
/// end, unless it already sits on a model point or a crossing of guides.
/// `skip` is the grid being dragged.
fn align_grid_end(
    project: &Project,
    skip: Option<ElementId>,
    fixed: Option<DVec2>,
    raw: DVec2,
    s: Snap,
    scale: f64,
) -> Snap {
    if matches!(s.kind, SnapKind::Endpoint | SnapKind::Midpoint | SnapKind::Intersection) {
        return s;
    }
    let others: Vec<(DVec2, DVec2)> = project
        .elements
        .values()
        .filter(|e| Some(e.id) != skip)
        .filter_map(|e| match &e.kind {
            ElementKind::Grid(g) => Some((g.start, g.end)),
            _ => None,
        })
        .collect();
    match cc_core::grid::align_end(&others, fixed, raw, SNAP_PX / scale) {
        Some(a) => Snap { point: a.point, kind: SnapKind::Alignment, guides: [Some((a.reference, a.point)), None] },
        None => s,
    }
}

/// Two clicks: start and end of a beam. Its top is at the level above by default.
fn beam_tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    level: LevelId,
    type_id: ElementTypeId,
    xf: &Xf,
) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let shift = ui.input(|i| i.modifiers.shift);
    let crossings = grid_crossings(state.project());
    let anchor = state.interaction.anchor;
    let s = snap_at(state, Some(level), raw, anchor, &crossings, xf.scale, shift);
    draw_snap(painter, xf, s);
    if let Some(a) = state.interaction.anchor {
        let width = match state.project().element_type(type_id).map(|t| &t.data) {
            Some(cc_core::TypeData::Beam { width, .. }) => *width,
            _ => 0.15,
        };
        let n = (s.point - a).normalize_or_zero().perp() * (width / 2.0);
        let pts = vec![xf.to_screen(a - n), xf.to_screen(s.point - n), xf.to_screen(s.point + n), xf.to_screen(a + n)];
        painter.add(egui::Shape::closed_line(pts, Stroke::new(1.5, PREVIEW)));
        temp_dimension(painter, xf, a, s.point, None, None, width / 2.0);
    }
    if resp.clicked() {
        match state.interaction.anchor {
            None => state.interaction.anchor = Some(s.point),
            Some(a) if a.distance(s.point) > 0.05 => {
                let command = create::BeamCreate {
                    start: a.to_array(),
                    end: s.point.to_array(),
                    level: Some(Ref::Id(level.0)),
                    type_ref: Some(Ref::Id(type_id.0)),
                    ..Default::default()
                };
                let (before, depth) = (chain_at(state), state.session.doc.undo_depth());
                run_create(state, command);
                state.interaction.anchor = Some(s.point);
                chain_placed(state, before, depth);
            }
            _ => {}
        }
    }
}

/// Two clicks: the foot of the stair, then a point giving the direction of
/// its first flight; the shape comes from the options bar.
fn stair_tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    level: LevelId,
    type_id: ElementTypeId,
    xf: &Xf,
) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let shift = ui.input(|i| i.modifiers.shift);
    let s = snap_at(state, Some(level), raw, state.interaction.anchor, &[], xf.scale, shift);
    draw_snap(painter, xf, s);
    let project = state.project();
    let Some(cc_core::TypeData::Stair { width, max_riser, tread, rules, .. }) =
        project.element_type(type_id).map(|t| t.data.clone())
    else {
        return;
    };
    let height = match default_top(project, level) {
        cc_core::WallTop::Level { level: top, .. } => project.level_elevation(top) - project.level_elevation(level),
        cc_core::WallTop::Unconnected => 3.0,
    };
    let layout = state.stair_layout;
    if let Some(a) = state.interaction.anchor {
        let u = (s.point - a).normalize_or(DVec2::X);
        let g = cc_geom::stair::layout_stair(a, u, width, 0.0, height, max_riser, tread, &rules, layout);
        for step in &g.steps {
            let pts: Vec<_> = step.outline.iter().map(|p| xf.to_screen(*p)).collect();
            painter.add(egui::Shape::closed_line(pts, Stroke::new(1.0, PREVIEW)));
        }
        let end = g.walkline.last().map_or(a, |p| p.truncate());
        let text = t!("label-stair", "risers" => g.risers, "run" => format::meters(g.run()));
        label(painter, xf.to_screen(end), text);
    }
    if resp.clicked() {
        match state.interaction.anchor {
            None => state.interaction.anchor = Some(s.point),
            Some(a) if a.distance(s.point) > 0.05 => {
                let d = s.point - a;
                let command =
                    crate::stair_ui::create_command(layout, a.to_array(), d.y.atan2(d.x).to_degrees(), level, type_id);
                run_create(state, command);
                state.interaction.anchor = None;
            }
            _ => {}
        }
    }
}

/// Two clicks: the foot of the ramp, then a point giving the direction of
/// its first flight; the shape comes from the options bar.
fn ramp_tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    level: LevelId,
    type_id: ElementTypeId,
    xf: &Xf,
) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let shift = ui.input(|i| i.modifiers.shift);
    let s = snap_at(state, Some(level), raw, state.interaction.anchor, &[], xf.scale, shift);
    draw_snap(painter, xf, s);
    let project = state.project();
    let Some(cc_core::TypeData::Ramp { width, rules, .. }) = project.element_type(type_id).map(|t| t.data.clone())
    else {
        return;
    };
    let height = match default_top(project, level) {
        cc_core::WallTop::Level { level: top, .. } => project.level_elevation(top) - project.level_elevation(level),
        cc_core::WallTop::Unconnected => 3.0,
    };
    let layout = state.ramp_layout;
    if let Some(a) = state.interaction.anchor {
        let u = (s.point - a).normalize_or(DVec2::X);
        let g = cc_geom::ramp::layout_ramp(a, u, width, 0.0, height, &rules, layout);
        for piece in &g.pieces {
            let pts: Vec<_> = piece.outline.iter().map(|p| xf.to_screen(*p)).collect();
            painter.add(egui::Shape::closed_line(pts, Stroke::new(1.0, PREVIEW)));
        }
        let end = g.walkline.last().map_or(a, |p| p.truncate());
        let text = t!("label-ramp", "flights" => g.check.flights, "run" => format::meters(g.run()));
        label(painter, xf.to_screen(end), text);
    }
    if resp.clicked() {
        match state.interaction.anchor {
            None => state.interaction.anchor = Some(s.point),
            Some(a) if a.distance(s.point) > 0.05 => {
                let d = s.point - a;
                let command =
                    crate::stair_ui::ramp_command(layout, a.to_array(), d.y.atan2(d.x).to_degrees(), level, type_id);
                run_create(state, command);
                state.interaction.anchor = None;
            }
            _ => {}
        }
    }
}

/// Click the points of a railing's or swept profile's path; Enter, a double
/// click or Finish ends it (`finish` makes the element). A `closed` path
/// previews its way back to the first point.
#[allow(clippy::too_many_arguments)]
fn path_tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    level: LevelId,
    type_id: ElementTypeId,
    xf: &Xf,
    closed: bool,
    finish: fn(&mut AppState, LevelId, ElementTypeId),
) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let shift = ui.input(|i| i.modifiers.shift);
    let last = state.interaction.points.last().copied();
    let s = snap_at(state, Some(level), raw, last, &state.interaction.points.clone(), xf.scale, shift);
    draw_snap(painter, xf, s);
    let pts: Vec<Pos2> =
        state.interaction.points.iter().map(|p| xf.to_screen(*p)).chain([xf.to_screen(s.point)]).collect();
    if pts.len() >= 3 && closed {
        painter.add(egui::Shape::closed_line(pts, Stroke::new(1.5, PREVIEW)));
    } else if pts.len() >= 2 {
        painter.add(egui::Shape::line(pts, Stroke::new(1.5, PREVIEW)));
    }
    let done = resp.double_clicked() || ui.input(|i| i.key_pressed(Key::Enter));
    if resp.clicked() && !resp.double_clicked() {
        state.interaction.points.push(s.point);
    }
    if done {
        finish(state, level, type_id);
    }
}

/// Options of the railing tool: put railings on a stair or ramp picked in
/// the view instead of drawing a path.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RailingHostOptions {
    pub on: bool,
    /// Both sides of the host, not only the side nearest the click.
    pub both: bool,
}

/// Whether `id` is a stair or a ramp (an element railings can follow).
pub fn railing_host(state: &AppState, id: ElementId) -> bool {
    state.project().element(id).is_some_and(|e| matches!(e.kind, ElementKind::Stair(_) | ElementKind::Ramp(_)))
}

/// The railing tool picking a host: the stair or ramp under the pointer is
/// highlighted, and a click puts a railing on its side nearest the click
/// (or on both sides).
fn host_railing_tool(
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    type_id: ElementTypeId,
    xf: &Xf,
    drawing: &cc_views::Drawing2D,
) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let Some(host) = drawing.hit_test(raw, 6.0 / xf.scale).filter(|id| railing_host(state, *id)) else { return };
    let lit: std::collections::BTreeSet<ElementId> = [host].into_iter().collect();
    crate::modify::paint_ghost(painter, xf, drawing, &lit, &cc_core::PlanTransform::IDENTITY);
    if resp.clicked() {
        let command = cc_engine::commands::stair::StairRailingsAdd {
            id: host.0,
            side: state.railing_host.both.then(|| "both".to_string()),
            type_ref: Some(Ref::Id(type_id.0)),
            inset: None,
            near: Some(raw.to_array()),
        };
        crate::actions::run_command(state, command);
    }
}

/// Options of the swept profile tool.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SweepOptions {
    /// Height of the path above the level.
    pub base_offset: f64,
    /// The path runs back to its first point.
    pub closed: bool,
    /// The profile on the right of the path.
    pub flip: bool,
}

/// Create the swept profile from the points clicked so far.
pub fn finish_sweep(state: &mut AppState, level: LevelId, type_id: ElementTypeId) {
    let mut path = std::mem::take(&mut state.interaction.points);
    path.dedup_by(|a, b| a.distance(*b) < 1e-3);
    let o = state.sweep;
    if path.len() >= if o.closed { 3 } else { 2 } {
        let command = cc_engine::commands::sweep::SweepCreate {
            path: path.iter().map(|p| p.to_array()).collect(),
            closed: o.closed,
            level: Some(Ref::Id(level.0)),
            type_ref: Some(Ref::Id(type_id.0)),
            base_offset: Some(o.base_offset),
            flip: o.flip,
        };
        run_create(state, command);
    }
}

/// Create the railing from the points clicked so far.
pub fn finish_railing(state: &mut AppState, level: LevelId, type_id: ElementTypeId) {
    let mut path = std::mem::take(&mut state.interaction.points);
    path.dedup_by(|a, b| a.distance(*b) < 1e-3);
    if path.len() >= 2 {
        let command = create::RailingCreate {
            path: path.iter().map(|p| p.to_array()).collect(),
            level: Some(Ref::Id(level.0)),
            type_ref: Some(Ref::Id(type_id.0)),
            ..Default::default()
        };
        run_create(state, command);
    }
}

/// Click inside walls to create a room; the enclosed region under the cursor is previewed.
fn room_tool(
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    level: LevelId,
    type_id: ElementTypeId,
    xf: &Xf,
) {
    let Some(p) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let boundary = state.session.geom().room_at(state.project(), level, p, state.session.doc.creation_phase);
    if let Some(b) = &boundary {
        let pts: Vec<Pos2> = b.iter().map(|q| xf.to_screen(*q)).collect();
        crate::paint::fill_polygon(painter, pts.clone(), PREVIEW.gamma_multiply(0.18));
        painter.add(egui::Shape::closed_line(pts, Stroke::new(1.5, PREVIEW)));
    }
    if resp.clicked() {
        if boundary.is_none() {
            state.message = t!("msg-room-not-enclosed");
            return;
        }
        let _ = type_id;
        let command = create::RoomCreate { point: p.to_array(), level: Some(Ref::Id(level.0)), ..Default::default() };
        run_create(state, command);
    }
}

/// Two clicks: start and end of a grid line.
fn grid_tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    level: LevelId,
    type_id: ElementTypeId,
    xf: &Xf,
) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let shift = ui.input(|i| i.modifiers.shift);
    let anchor = state.interaction.anchor;
    let s = snap_at(state, Some(level), raw, anchor, &[], xf.scale, shift);
    // The ends line up with the bubbles of the other grids.
    let s = align_grid_end(state.project(), None, anchor, raw, s, xf.scale);
    draw_snap(painter, xf, s);
    if let Some(a) = anchor {
        painter.add(egui::Shape::dashed_line(
            &[xf.to_screen(a), xf.to_screen(s.point)],
            Stroke::new(1.5, PREVIEW),
            10.0,
            4.0,
        ));
        temp_dimension(painter, xf, a, s.point, None, None, 0.0);
        // The name the grid will get (the engine's default for `grid_create`).
        if a.distance(s.point) > 0.1 {
            let name = cc_core::grid::next_name(create::grids(state.project()), a, s.point);
            label(painter, xf.to_screen(s.point), format!("{} {name}", t!("tool-grid")));
        }
    }
    if resp.clicked() {
        match state.interaction.anchor {
            None => state.interaction.anchor = Some(s.point),
            Some(a) if a.distance(s.point) > 0.1 => {
                let _ = type_id;
                let command = create::GridCreate {
                    start: a.to_array(),
                    end: s.point.to_array(),
                    level: Some(Ref::Id(level.0)),
                    ..Default::default()
                };
                run_create(state, command);
                state.interaction.anchor = None;
            }
            _ => {}
        }
    }
}

/// Measure between two references: two clicks snapped to the view's
/// linework; the result (the engine's `measure`) stays until the next click.
fn measure_tool(ui: &Ui, resp: &Response, painter: &Painter, state: &mut AppState, xf: &Xf, drawing: &Drawing2D) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let shift = ui.input(|i| i.modifiers.shift);
    let ends: Vec<DVec2> = drawing
        .prims
        .iter()
        .filter_map(|p| match p {
            cc_views::Prim::Line { a, b, .. } => Some([*a, *b]),
            _ => None,
        })
        .flatten()
        .collect();
    let points = state.interaction.points.clone();
    let anchor = match points.as_slice() {
        [a] => Some(*a),
        _ => None,
    };
    let s = snap_at(state, None, raw, anchor, &ends, xf.scale, shift);
    draw_snap(painter, xf, s);
    let tol = 6.0 / xf.scale;
    let stroke = Stroke::new(1.5, PREVIEW);
    match points.as_slice() {
        [a] => {
            painter.line_segment([xf.to_screen(*a), xf.to_screen(s.point)], stroke);
            label(painter, xf.to_screen(s.point), format::length(a.distance(s.point)));
            if resp.clicked() {
                let line_a = state.interaction.targets_lines.first().copied();
                let line_b = line_at(drawing, s.point, tol);
                let command = cc_engine::commands::query::Measure {
                    a: a.to_array(),
                    b: s.point.to_array(),
                    line_a: line_a.map(|(p, q)| [p.to_array(), q.to_array()]),
                    line_b: line_b.map(|(p, q)| [p.to_array(), q.to_array()]),
                };
                if let Some(out) = crate::actions::run_command(state, command) {
                    let num = |k: &str| out[k].as_f64().unwrap_or(0.0);
                    let mut text = t!(
                        "msg-measure",
                        "distance" => format::length(num("distance")),
                        "dx" => format::length(num("dx").abs()),
                        "dy" => format::length(num("dy").abs()),
                        "angle" => format!("{:.2}°", num("angle"))
                    );
                    if let Some(between) = out["between_lines"].as_f64() {
                        text.push_str(&t!("msg-measure-lines", "distance" => format::length(between)));
                    }
                    state.message = text;
                }
                state.interaction.points.push(s.point);
            }
        }
        [a, b, ..] => {
            // The last measurement stays on screen until the next pick.
            painter.line_segment([xf.to_screen(*a), xf.to_screen(*b)], stroke);
            label(painter, xf.to_screen((*a + *b) / 2.0), format::length(a.distance(*b)));
            if resp.clicked() {
                state.interaction.points = vec![s.point];
                state.interaction.targets_lines = line_at(drawing, s.point, tol).into_iter().collect();
            }
        }
        [] => {
            if resp.clicked() {
                state.interaction.points.push(s.point);
                state.interaction.targets_lines = line_at(drawing, s.point, tol).into_iter().collect();
            }
        }
    }
}

/// Three clicks: the two measured points, then the position of the dimension line.
#[allow(clippy::too_many_arguments)]
fn dimension_tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    view: &View,
    (type_id, mode): (ElementTypeId, DimensionMode),
    xf: &Xf,
    drawing: &Drawing2D,
) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let scale = view.scale.max(1);
    let tol = 6.0 / xf.scale;
    let points = state.interaction.points.clone();
    let place = |state: &mut AppState, dim: cc_core::Dimension| {
        // The engine places it; its ends follow the elements they were placed on.
        let (kind, direction, vertex) = match dim.kind {
            DimensionKind::Aligned => ("aligned", None, None),
            DimensionKind::Linear { dir } => {
                ("linear", Some(if dir.x.abs() >= dir.y.abs() { "horizontal" } else { "vertical" }), None)
            }
            DimensionKind::Ordinate { dir } => {
                ("ordinate", Some(if dir.x.abs() >= dir.y.abs() { "horizontal" } else { "vertical" }), None)
            }
            DimensionKind::Angular { vertex, .. } => ("angular", None, Some(vertex.to_array())),
            DimensionKind::Radial => ("radial", None, None),
            DimensionKind::Diameter => ("diameter", None, None),
        };
        let command = cc_engine::commands::annotate::DimensionCreate {
            view: cc_engine::Ref::Id(dim.view.0),
            a: dim.a.to_array(),
            b: dim.b.to_array(),
            kind: Some(kind.to_string()),
            offset: Some(dim.offset),
            vertex,
            direction: direction.map(str::to_string),
            type_ref: Some(cc_engine::Ref::Id(type_id.0)),
        };
        match state.session.run(command) {
            Ok(out) => {
                if let Some(id) = out["id"].as_u64() {
                    state.selection = std::collections::BTreeSet::from([ElementId(id)]);
                }
            }
            Err(e) => state.message = e.message,
        }
        state.interaction.points.clear();
    };
    let dim = |a: DVec2, b: DVec2, offset: f64, kind: DimensionKind| cc_core::Dimension {
        view: view.id,
        a,
        b,
        offset,
        kind,
        refs: Default::default(),
    };
    match mode {
        DimensionMode::Aligned | DimensionMode::Linear | DimensionMode::Ordinate => {
            let shift = ui.input(|i| i.modifiers.shift);
            // Snap to the visible linework of the view.
            let ends: Vec<DVec2> = drawing
                .prims
                .iter()
                .filter_map(|p| match p {
                    cc_views::Prim::Line { a, b, .. } => Some([*a, *b]),
                    _ => None,
                })
                .flatten()
                .collect();
            let linear = mode != DimensionMode::Aligned;
            // Ordinate dimensions always continue from their origin.
            let ordinate = mode == DimensionMode::Ordinate;
            let continuous = state.dim_continuous || ordinate;
            // A chain of another view or mode, or with Single chosen, ends.
            if state
                .interaction
                .dim_chain
                .is_some_and(|c| c.view != view.id || c.linear != linear || c.ordinate != ordinate)
                || !continuous
            {
                state.interaction.dim_chain = None;
            }
            if let Some(chain) = state.interaction.dim_chain {
                // Each click adds the next dimension of the chain, on its line.
                if ui.input(|i| i.key_pressed(Key::Enter)) || resp.secondary_clicked() {
                    state.interaction.dim_chain = None;
                    return;
                }
                let s = snap_at(state, None, raw, Some(chain.last), &ends, xf.scale, shift);
                draw_snap(painter, xf, s);
                let offset = (chain.line - chain.last).dot(chain.dir.perp());
                let next = if ordinate {
                    cc_core::dim_line::ordinate(view.id, &[chain.last, s.point], chain.dir, offset)
                } else {
                    cc_core::dim_line::chain(view.id, &[chain.last, s.point], chain.dir, offset, !linear)
                };
                match next.and_then(|dims| dims.into_iter().next()) {
                    Some(d) => {
                        paint_dimension(painter, xf, &d, scale);
                        if resp.clicked() && place_chain(state, type_id, &chain, s.point, offset) && !ordinate {
                            state.interaction.dim_chain = Some(DimChain { last: s.point, ..chain });
                        }
                    }
                    None if resp.clicked() => state.message = t!("msg-dimension-zero"),
                    None => {}
                }
                return;
            }
            let anchor = points.last().copied();
            let s = snap_at(state, None, raw, anchor, &ends, xf.scale, shift);
            match points.as_slice() {
                [] => {
                    draw_snap(painter, xf, s);
                    if resp.clicked() {
                        state.interaction.points.push(s.point);
                    }
                }
                [a] => {
                    draw_snap(painter, xf, s);
                    painter.line_segment([xf.to_screen(*a), xf.to_screen(s.point)], Stroke::new(1.5, PREVIEW));
                    label(painter, xf.to_screen(s.point), format::meters(a.distance(s.point)));
                    if resp.clicked() && a.distance(s.point) > 1e-3 {
                        state.interaction.points.push(s.point);
                    }
                }
                [a, b, ..] => {
                    let kind = match mode {
                        DimensionMode::Linear => DimensionKind::Linear { dir: linear_direction(*a, *b, raw) },
                        DimensionMode::Ordinate => DimensionKind::Ordinate { dir: linear_direction(*a, *b, raw) },
                        _ => DimensionKind::Aligned,
                    };
                    let d = dim(*a, *b, 0.0, kind);
                    let dir = d.direction().unwrap_or(DVec2::X);
                    let d = cc_core::Dimension { offset: (raw - *a).dot(dir.perp()), ..d };
                    // The line lines up with the line of a parallel dimension nearby.
                    let others: Vec<cc_core::Dimension> = state
                        .project()
                        .elements
                        .values()
                        .filter_map(|e| match &e.kind {
                            ElementKind::Dimension(o) if o.view == view.id => Some((**o).clone()),
                            _ => None,
                        })
                        .collect();
                    let d = match cc_core::dim_line::align(&d, &others, SNAP_PX / xf.scale) {
                        Some(line) => {
                            draw_line_guide(painter, xf, line.reference, line.point);
                            cc_core::Dimension { offset: line.offset, ..d }
                        }
                        None => d,
                    };
                    paint_dimension(painter, xf, &d, scale);
                    if resp.clicked() {
                        if d.value() > 1e-6 && continuous {
                            // The first dimension of a chain.
                            let line = *a + dir.perp() * d.offset;
                            let chain = DimChain { view: view.id, last: *a, dir, line, linear, ordinate };
                            if place_chain(state, type_id, &chain, *b, d.offset) {
                                let last = if ordinate { *a } else { *b };
                                state.interaction.dim_chain = Some(DimChain { last, ..chain });
                            }
                            state.interaction.points.clear();
                        } else if d.value() > 1e-6 {
                            place(state, d);
                        } else {
                            state.message = t!("msg-dimension-zero");
                        }
                    }
                }
            }
        }
        DimensionMode::Angular => {
            let hovered = line_at(drawing, raw, tol);
            if let Some((p, q)) = hovered
                && points.len() < 4
            {
                painter.line_segment([xf.to_screen(p), xf.to_screen(q)], Stroke::new(2.5, PREVIEW));
            }
            // points: [pick on line 1, its other end, pick on line 2, its other end].
            match points.as_slice() {
                [p1, q1, p2, q2] => {
                    let Some(vertex) = line_intersection(*p1, *q1, *p2, *q2) else {
                        state.interaction.points.clear();
                        state.message = t!("msg-lines-parallel");
                        return;
                    };
                    // The quadrant holding the cursor picks the rays.
                    let ray = |p: DVec2, q: DVec2, pick: DVec2| {
                        let dir = (q - p).normalize_or(DVec2::X);
                        let toward = if (raw - vertex).dot(dir) >= 0.0 { dir } else { -dir };
                        vertex + toward * (pick - vertex).length().max(1e-3)
                    };
                    let d = dim(
                        ray(*p1, *q1, *p1),
                        ray(*p2, *q2, *p2),
                        (raw - vertex).length(),
                        DimensionKind::Angular { vertex, rays: Default::default() },
                    );
                    paint_dimension(painter, xf, &d, scale);
                    if resp.clicked() && d.value() > 1e-6 {
                        place(state, d);
                    }
                }
                _ => {
                    if resp.clicked() {
                        match hovered {
                            Some((p, q)) => {
                                let pick = closest_on_segment(p, q, raw);
                                let other = if pick.distance(p) > pick.distance(q) { p } else { q };
                                state.interaction.points.extend([pick, other]);
                                if state.interaction.points.len() == 2 {
                                    state.message = t!("hint-dim-angular-second");
                                }
                            }
                            None => state.message = t!("hint-dim-angular"),
                        }
                    }
                }
            }
        }
        DimensionMode::Radial | DimensionMode::Diameter => {
            let picked = match points.as_slice() {
                [center, rim] => Some((*center, center.distance(*rim))),
                _ => arc_at(drawing, raw, tol),
            };
            let Some((center, r)) = picked else {
                if resp.clicked() {
                    state.message = t!("msg-pick-arc");
                }
                return;
            };
            let b = center + (raw - center).normalize_or(DVec2::X) * r;
            let kind = if mode == DimensionMode::Radial { DimensionKind::Radial } else { DimensionKind::Diameter };
            let d = dim(center, b, 0.0, kind);
            if points.is_empty() {
                painter.circle_stroke(xf.to_screen(center), (r * xf.scale) as f32, Stroke::new(2.5, PREVIEW));
            }
            paint_dimension(painter, xf, &d, scale);
            if resp.clicked() {
                if points.is_empty() {
                    // Keep the arc while the leader is placed.
                    state.interaction.points = vec![center, b];
                    state.message = t!("hint-dim-radial-place");
                } else {
                    place(state, d);
                }
            }
        }
    }
}

/// Horizontal or vertical measuring direction for a linear dimension from
/// `a` to `b`, by where the cursor places the line: above or below the
/// points' box measures horizontally, beside it vertically.
pub(crate) fn linear_direction(a: DVec2, b: DVec2, cursor: DVec2) -> DVec2 {
    let (lo, hi) = (a.min(b), a.max(b));
    let outside = |v: f64, lo: f64, hi: f64| (lo - v).max(v - hi).max(0.0);
    let (ox, oy) = (outside(cursor.x, lo.x, hi.x), outside(cursor.y, lo.y, hi.y));
    let horizontal = if ox == oy { (b - a).x.abs() >= (b - a).y.abs() } else { oy > ox };
    if horizontal { DVec2::X } else { DVec2::Y }
}

/// Place the next dimension of `chain`, from its last point (or origin) to
/// `to`, with its line at `offset` from that point (`dimension_chain_create`
/// or `dimension_ordinate_create`); selects it. False (with the error
/// shown) when it was refused.
fn place_chain(state: &mut AppState, type_id: ElementTypeId, chain: &DimChain, to: DVec2, offset: f64) -> bool {
    let dir = chain.dir;
    let axis = if dir.x.abs() >= dir.y.abs() { "horizontal" } else { "vertical" };
    let points = vec![chain.last.to_array(), to.to_array()];
    let view = Ref::Id(chain.view.0);
    let type_ref = Some(Ref::Id(type_id.0));
    let result = if chain.ordinate {
        state.session.run(cc_engine::commands::annotate::DimensionOrdinateCreate {
            view,
            points,
            offset: Some(offset),
            direction: Some(axis.to_string()),
            angle: None,
            type_ref,
        })
    } else {
        state.session.run(cc_engine::commands::annotate::DimensionChainCreate {
            view,
            points,
            kind: Some(if chain.linear { "linear" } else { "aligned" }.to_string()),
            offset: Some(offset),
            direction: chain.linear.then(|| axis.to_string()),
            angle: (!chain.linear).then(|| dir.y.atan2(dir.x).to_degrees()),
            type_ref,
        })
    };
    match result {
        Ok(out) => {
            let ids = out["ids"].as_array().map(|ids| ids.iter().filter_map(|id| id.as_u64()).map(ElementId));
            state.selection = ids.into_iter().flatten().collect();
            true
        }
        Err(e) => {
            state.message = e.message;
            false
        }
    }
}

/// The dashed guide from the end of another dimension's line to the line
/// being placed on it.
fn draw_line_guide(painter: &Painter, xf: &Xf, reference: DVec2, point: DVec2) {
    let (a, b) = (xf.to_screen(reference), xf.to_screen(point));
    let stroke = Stroke::new(1.0, TEMP_DIM.gamma_multiply(0.7));
    if a.distance(b) > 1.0 {
        painter.extend(egui::Shape::dashed_line(&[a, b], stroke, 6.0, 4.0));
    }
    painter.circle_stroke(a, 3.0, Stroke::new(1.0, TEMP_DIM));
}

/// Paint a dimension being placed, as it will be drawn.
fn paint_dimension(painter: &Painter, xf: &Xf, d: &cc_core::Dimension, scale: u32) {
    let Some(layout) = cc_views::dimension_layout(d, scale) else { return };
    let stroke = Stroke::new(1.25, PREVIEW);
    for (a, b, _) in &layout.lines {
        painter.line_segment([xf.to_screen(*a), xf.to_screen(*b)], stroke);
    }
    let up = DVec2::from_angle(layout.text_angle).perp();
    let at = xf.to_screen(layout.text_pos + up * (2.0 * scale as f64 / 1000.0));
    dim_text(painter, at, layout.text, -layout.text_angle as f32, true);
}

/// The linework segment (of an element) nearest to `p` within `tol`.
pub(crate) fn line_at(drawing: &Drawing2D, p: DVec2, tol: f64) -> Option<(DVec2, DVec2)> {
    drawing
        .prims
        .iter()
        .filter_map(|prim| match prim {
            cc_views::Prim::Line { a, b, element: Some(_), .. } => Some((*a, *b)),
            _ => None,
        })
        .map(|(a, b)| (closest_on_segment(a, b, p).distance(p), (a, b)))
        .filter(|(d, _)| *d <= tol)
        .min_by(|x, y| x.0.total_cmp(&y.0))
        .map(|(_, l)| l)
}

/// Whether `p` lies on the segment `a`-`b` within `tol`.
pub(crate) fn point_on_segment(a: DVec2, b: DVec2, p: DVec2, tol: f64) -> bool {
    closest_on_segment(a, b, p).distance(p) <= tol
}

fn closest_on_segment(a: DVec2, b: DVec2, p: DVec2) -> DVec2 {
    let ab = b - a;
    let t = if ab.length_squared() > 1e-18 { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
    a + ab * t
}

/// Intersection of the infinite lines `p1`-`q1` and `p2`-`q2` (`None` when parallel).
fn line_intersection(p1: DVec2, q1: DVec2, p2: DVec2, q2: DVec2) -> Option<DVec2> {
    let (d1, d2) = (q1 - p1, q2 - p2);
    let den = d1.perp_dot(d2);
    if den.abs() < 1e-12 * d1.length() * d2.length() {
        return None;
    }
    Some(p1 + d1 * ((p2 - p1).perp_dot(d2) / den))
}

/// An arc or circle drawn as a chain of short segments near `p`: its
/// center and radius. Needs four consecutive points of one element on a
/// common circle (corners of straight linework do not qualify).
pub(crate) fn arc_at(drawing: &Drawing2D, p: DVec2, tol: f64) -> Option<(DVec2, f64)> {
    let (hit, element) = drawing
        .prims
        .iter()
        .filter_map(|prim| match prim {
            cc_views::Prim::Line { a, b, element: Some(e), .. } => Some(((*a, *b), *e)),
            _ => None,
        })
        .map(|(l, e)| (closest_on_segment(l.0, l.1, p).distance(p), l, e))
        .filter(|(d, _, _)| *d <= tol)
        .min_by(|x, y| x.0.total_cmp(&y.0))
        .map(|(_, l, e)| (l, e))?;
    let segments: Vec<(DVec2, DVec2)> = drawing
        .prims
        .iter()
        .filter_map(|prim| match prim {
            cc_views::Prim::Line { a, b, element: Some(e), .. } if *e == element => Some((*a, *b)),
            _ => None,
        })
        .collect();
    let eps = 1e-6;
    // The far end of a segment touching `at`, other than `not`.
    let next = |at: DVec2, not: DVec2| {
        segments.iter().find_map(|(a, b)| {
            if a.distance(at) < eps && b.distance(not) > eps {
                Some(*b)
            } else if b.distance(at) < eps && a.distance(not) > eps {
                Some(*a)
            } else {
                None
            }
        })
    };
    let (a, b) = hit;
    let mut chain = vec![a, b];
    if let Some(c) = next(b, a) {
        chain.push(c);
        if let Some(d) = next(c, b) {
            chain.push(d);
        }
    }
    if chain.len() < 4
        && let Some(z) = next(a, b)
    {
        chain.insert(0, z);
        if chain.len() < 4
            && let Some(y) = next(z, a)
        {
            chain.insert(0, y);
        }
    }
    if chain.len() < 4 {
        return None;
    }
    let center = circumcenter(chain[0], chain[1], chain[2])?;
    let r = center.distance(chain[0]);
    // Short segments of a real arc, all on the circle.
    let on_circle = chain.iter().all(|q| (q.distance(center) - r).abs() <= 1e-3 * r.max(1e-3));
    let short = chain.windows(2).all(|w| w[0].distance(w[1]) < r);
    (on_circle && short && r.is_finite() && r > 1e-6).then_some((center, r))
}

fn circumcenter(a: DVec2, b: DVec2, c: DVec2) -> Option<DVec2> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let (a2, b2, c2) = (a.length_squared(), b.length_squared(), c.length_squared());
    Some(DVec2::new(
        (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d,
        (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d,
    ))
}

fn wall_tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    level: LevelId,
    type_id: ElementTypeId,
    xf: &Xf,
) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let shift = ui.input(|i| i.modifiers.shift);
    let anchor = state.interaction.anchor;
    let s = snap_at(state, Some(level), raw, anchor, &[], xf.scale, shift);
    let mut p = s.point;
    draw_snap(painter, xf, s);

    let half = state.project().element_type(type_id).map_or(0.075, |t| t.thickness() * 0.5);
    // Arc mode: after the start and the end, the third point is on the arc,
    // or a typed radius gives the arc on the cursor's side.
    let arc_end = state.interaction.points.first().copied().filter(|_| state.wall_arc);
    if let (Some(a), Some(b)) = (anchor, arc_end) {
        let radius = state.typed.length_value();
        let bulge = match radius {
            Some(r) => bulge_with_radius(a, b, p, r),
            None => cc_core::arc::Arc::bulge_through(a, p, b),
        };
        match bulge {
            Some(bulge) => arc_preview(painter, xf, a, b, bulge, half),
            None => paint_wall_preview(painter, xf, a, b, half),
        }
        let unit = format::current().0.length.unit.symbol();
        let arc = bulge.and_then(|bulge| cc_core::arc::Arc::from_bulge(a, b, bulge));
        let text = match (radius, arc) {
            (Some(_), Some(arc)) => {
                format!("R {} {unit} \u{23ce}  {}", state.typed.length, format::length(arc.length()))
            }
            (Some(_), None) => format!("R {} {unit}  {}", state.typed.length, t!("msg-arc-radius-short")),
            (None, Some(arc)) => format!("R {}  {}", format::length(arc.radius), format::length(arc.length())),
            (None, None) => String::new(),
        };
        if !text.is_empty() {
            label(painter, xf.to_screen(p), text);
        }
        let enter = ui.input(|i| i.key_pressed(Key::Enter)) && radius.is_some();
        if resp.clicked() || enter {
            match bulge {
                Some(bulge) => {
                    let (before, depth) = (chain_at(state), state.session.doc.undo_depth());
                    create_wall(state, level, type_id, a, b, bulge);
                    state.interaction.anchor = Some(b);
                    state.interaction.points.clear();
                    state.typed.clear();
                    // The next wall starts along the arc's tangent at its end.
                    if let Some(arc) = cc_core::arc::Arc::from_bulge(a, b, bulge) {
                        state.interaction.prev_dir = Some(-arc.tangent(arc.length()));
                    }
                    chain_placed(state, before, depth);
                }
                None if radius.is_some() => state.message = t!("msg-arc-radius-short"),
                None => state.message = t!("msg-arc-collinear"),
            }
        }
        return;
    }
    if let Some(a) = anchor {
        if !state.typed.is_empty() {
            p = state.typed.apply(a, p, state.interaction.prev_dir);
        }
        if state.wall_arc {
            painter.line_segment([xf.to_screen(a), xf.to_screen(p)], Stroke::new(1.0, PREVIEW));
        } else {
            paint_wall_preview(painter, xf, a, p, half);
        }
        temp_dimension(painter, xf, a, p, state.interaction.prev_dir, Some(&state.typed), half);
    }

    let enter = ui.input(|i| i.key_pressed(Key::Enter)) && !state.typed.is_empty();
    if resp.clicked() || enter {
        match anchor {
            None => state.interaction.anchor = Some(p),
            Some(a) if a.distance(p) > 0.01 && state.wall_arc => {
                // The end of the arc; the next click places a point on it.
                state.interaction.points = vec![p];
                state.message = t!("hint-wall-arc-point");
                state.typed.clear();
            }
            Some(a) if a.distance(p) > 0.01 => {
                let (before, depth) = (chain_at(state), state.session.doc.undo_depth());
                create_wall(state, level, type_id, a, p, 0.0);
                state.interaction.anchor = Some(p);
                // The next wall's angle is measured from this one (as an interior angle).
                state.interaction.prev_dir = Some(a - p);
                state.typed.clear();
                chain_placed(state, before, depth);
            }
            _ => {}
        }
    }
}

/// The arc of `radius` from `a` to `b` on the side of `p`: of the shorter and
/// the longer arc, the one whose middle is nearer `p`.
fn bulge_with_radius(a: DVec2, b: DVec2, p: DVec2, radius: f64) -> Option<f64> {
    use cc_core::arc::Arc;
    // A counter-clockwise arc bulges to the right of a → b.
    let ccw = (b - a).perp_dot(p - a) < 0.0;
    let gap = |bulge: &f64| {
        Arc::from_bulge(a, b, *bulge).map_or(f64::INFINITY, |arc| arc.point(arc.length() * 0.5).distance(p))
    };
    [false, true]
        .into_iter()
        .filter_map(|major| Arc::bulge_radius(a, b, radius, ccw, major))
        .min_by(|x, y| gap(x).total_cmp(&gap(y)))
}

/// Create a wall from `a` to `b` (an arc when `bulge` is not 0) with the tool options.
fn create_wall(state: &mut AppState, level: LevelId, type_id: ElementTypeId, a: DVec2, b: DVec2, bulge: f64) {
    let location_line =
        serde_json::to_value(state.wall_location_line).ok().and_then(|v| v.as_str().map(str::to_string));
    let command = create::WallCreate {
        start: a.to_array(),
        end: b.to_array(),
        bulge: (bulge != 0.0).then_some(bulge),
        level: Some(Ref::Id(level.0)),
        type_ref: Some(Ref::Id(type_id.0)),
        height: Some(state.wall_height),
        top_level: state.wall_top_level.map(|l| Ref::Id(l.0)),
        location_line,
        ..Default::default()
    };
    run_create(state, command);
}

/// Preview of a straight wall from `a` to `b`: its line and faces.
fn paint_wall_preview(painter: &Painter, xf: &Xf, a: DVec2, b: DVec2, half: f64) {
    let n = (b - a).normalize_or_zero().perp() * half;
    let stroke = Stroke::new(1.5, PREVIEW);
    painter.line_segment([xf.to_screen(a), xf.to_screen(b)], Stroke::new(1.0, PREVIEW));
    painter.line_segment([xf.to_screen(a + n), xf.to_screen(b + n)], stroke);
    painter.line_segment([xf.to_screen(a - n), xf.to_screen(b - n)], stroke);
}

/// Preview of an arc wall from `a` to `b` with `bulge`: its line and faces.
pub(crate) fn arc_preview(painter: &Painter, xf: &Xf, a: DVec2, b: DVec2, bulge: f64, half: f64) {
    let Some(arc) = cc_core::arc::Arc::from_bulge(a, b, bulge) else {
        paint_wall_preview(painter, xf, a, b, half);
        return;
    };
    for (v, width) in [(0.0, 1.0), (half, 1.5), (-half, 1.5)] {
        if arc.radius_at(v) <= 0.0 {
            continue;
        }
        let pts: Vec<Pos2> =
            arc.points(0.0, arc.length(), v, cc_core::arc::ARC_STEP).into_iter().map(|q| xf.to_screen(q)).collect();
        painter.add(egui::Shape::line(pts, Stroke::new(width, PREVIEW)));
    }
}

/// Nearest wall on `level` to `raw`: (wall, u along wall, signed v).
fn wall_under(
    project: &Project,
    geom: &ModelGeometry,
    level: LevelId,
    raw: DVec2,
    slack: f64,
) -> Option<(ElementId, f64, f64)> {
    geom.walls
        .values()
        .filter(|g| project.element(g.element).is_some_and(|e| e.level == level))
        .filter_map(|g| {
            let (u, v) = g.local(raw);
            (u >= 0.0 && u <= g.length && v.abs() <= g.half + slack).then_some((g.element, u, v, v.abs()))
        })
        .min_by(|a, b| a.3.total_cmp(&b.3))
        .map(|(id, u, v, _)| (id, u, v))
}

#[allow(clippy::too_many_arguments)]
fn hosted_tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    level: LevelId,
    category: Category,
    type_id: ElementTypeId,
    xf: &Xf,
) {
    if ui.input(|i| i.key_pressed(Key::Space)) {
        state.interaction.flip_hand = !state.interaction.flip_hand;
    }
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let project = &state.session.doc.project;
    let Some(ty) = project.element_type(type_id) else { return };
    let Some((width, _)) = ty.opening_size() else { return };
    let sill = ty.default_sill();
    let Some((host, u, v)) = wall_under(project, state.session.geom(), level, raw, 12.0 / xf.scale) else {
        painter.circle_stroke(xf.to_screen(raw), 4.0, Stroke::new(1.0, Color32::GRAY));
        return;
    };
    let g = &state.session.geom().walls[&host];
    let Some(offset) = opening_offset(g, width, u) else {
        label(painter, xf.to_screen(raw), t!("msg-wall-too-short"));
        return;
    };
    let flip_facing = v < 0.0;
    let (u0, u1) = (offset - width * 0.5, offset + width * 0.5);
    let rect = g.outline((u0, u1), (u0, u1));
    let pts: Vec<Pos2> = rect.iter().map(|p| xf.to_screen(*p)).collect();
    painter.add(egui::Shape::closed_line(pts, Stroke::new(2.0, PREVIEW)));
    if category == Category::Door {
        let side = if flip_facing { -1.0 } else { 1.0 };
        let hinge_u = if state.interaction.flip_hand { u1 } else { u0 };
        let hinge = g.world(hinge_u, side * g.half);
        let normal = g.normal_at(offset);
        painter.line_segment(
            [xf.to_screen(hinge), xf.to_screen(hinge + normal * side * width)],
            Stroke::new(1.5, PREVIEW),
        );
    }
    label(painter, xf.to_screen(raw), format!("{} {}", t!("label-offset"), format::meters(offset)));

    if resp.clicked() {
        let flip_hand = state.interaction.flip_hand;
        create_opening(state, category, type_id, host, offset, sill, flip_facing, flip_hand);
    }
}

/// Center of an opening `width` wide near `u` along wall `g`, kept inside
/// the wall (`None` when the wall is too short).
fn opening_offset(g: &cc_geom::WallGeom, width: f64, u: f64) -> Option<f64> {
    let margin = 0.01;
    (g.length >= width + 2.0 * margin).then(|| u.clamp(width * 0.5 + margin, g.length - width * 0.5 - margin))
}

#[allow(clippy::too_many_arguments)]
fn create_opening(
    state: &mut AppState,
    category: Category,
    type_id: ElementTypeId,
    host: ElementId,
    offset: f64,
    sill: f64,
    flip_facing: bool,
    flip_hand: bool,
) {
    let args = create::HostedArgs {
        wall: Some(host.0),
        offset: Some(offset),
        sill: Some(sill),
        type_ref: Some(Ref::Id(type_id.0)),
        flip_facing: Some(flip_facing),
        flip_hand: Some(flip_hand),
        point: None,
    };
    if category == Category::Door {
        run_create(state, create::DoorCreate { args });
    } else {
        run_create(state, create::WindowCreate { args });
    }
}

/// A door or window tool clicked on `point` of wall `host` in a 3D view: the
/// opening is centered there (at the type's sill), facing the side clicked.
pub(crate) fn place_opening_3d(state: &mut AppState, tool: &Tool, host: ElementId, point: DVec2) {
    let Tool::Hosted { category, type_id } = *tool else { return };
    let project = &state.session.doc.project;
    let (Some(ty), Some(g)) = (project.element_type(type_id), state.session.geom().walls.get(&host)) else {
        state.message = t!("hint-hosted");
        return;
    };
    let Some((width, _)) = ty.opening_size() else { return };
    let sill = ty.default_sill();
    let (u, v) = g.local(point);
    let Some(offset) = opening_offset(g, width, u) else {
        state.message = t!("msg-wall-too-short");
        return;
    };
    let flip_hand = state.interaction.flip_hand;
    create_opening(state, category, type_id, host, offset, sill, v < 0.0, flip_hand);
}

/// Add the sketched outline `pts` as an opening of `roof` (refused, with a
/// message, when it is not inside the roof or overlaps another opening).
/// Add the sketched opening `pts` to the roof or floor `host`.
pub fn add_opening(state: &mut AppState, host: ElementId, pts: Vec<DVec2>) {
    if pts.len() < 3 || polygon::area(&pts) < 1e-4 {
        state.message = t!("msg-sketch-invalid");
        return;
    }
    let floor = matches!(state.project().element(host).map(|e| &e.kind), Some(ElementKind::Floor(_)));
    let added = if floor {
        let outline = pts.iter().map(|p| [p.x, p.y]).collect();
        crate::actions::run_command(state, create::FloorOpeningAdd { id: host.0, outline }).is_some()
    } else {
        crate::engine_edit::element(state, host, None, |e| {
            if let ElementKind::Roof(r) = &mut e.kind {
                r.holes.push(pts);
            }
        })
    };
    state.interaction.points.clear();
    if added {
        state.set_tool(Tool::Select);
        state.selection = BTreeSet::from([host]);
    }
}

/// Points of an outline sketched with clicks: closing it (a click on the
/// first point, or Enter) returns them.
fn sketch_input(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    level: LevelId,
    xf: &Xf,
) -> Option<Vec<DVec2>> {
    if ui.input(|i| i.key_pressed(Key::Backspace)) && state.typed.is_empty() {
        state.interaction.points.pop();
    }
    let finish_key = ui.input(|i| i.key_pressed(Key::Enter));
    let pts = state.interaction.points.clone();
    let stroke = Stroke::new(1.8, Color32::from_rgb(200, 40, 160));
    for w in pts.windows(2) {
        painter.line_segment([xf.to_screen(w[0]), xf.to_screen(w[1])], stroke);
    }
    let mut point = None;
    if let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) {
        let shift = ui.input(|i| i.modifiers.shift);
        let s = snap_at(state, Some(level), raw, pts.last().copied(), &pts, xf.scale, shift);
        draw_snap(painter, xf, s);
        if let Some(last) = pts.last() {
            painter.line_segment([xf.to_screen(*last), xf.to_screen(s.point)], Stroke::new(1.0, PREVIEW));
            let previous = pts.len().checked_sub(2).and_then(|i| pts.get(i)).map(|q| *q - *last);
            temp_dimension(painter, xf, *last, s.point, previous, None, 0.0);
        }
        if pts.len() >= 2 {
            painter.extend(egui::Shape::dashed_line(
                &[xf.to_screen(s.point), xf.to_screen(pts[0])],
                Stroke::new(1.0, PREVIEW),
                5.0,
                4.0,
            ));
        }
        point = Some(s.point);
    }
    if finish_key && pts.len() >= 3 {
        return Some(pts);
    }
    if resp.clicked()
        && let Some(p) = point
    {
        if pts.len() >= 3 && p.distance(pts[0]) * xf.scale < SNAP_PX {
            return Some(pts);
        } else if pts.last().is_none_or(|l| l.distance(p) > 1e-3) {
            state.interaction.points.push(p);
        }
    }
    None
}

fn section_tool(ui: &Ui, resp: &Response, painter: &Painter, state: &mut AppState, level: Option<LevelId>, xf: &Xf) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let shift = ui.input(|i| i.modifiers.shift);
    let s = snap_at(state, level, raw, state.interaction.anchor, &[], xf.scale, shift);
    draw_snap(painter, xf, s);
    if let Some(a) = state.interaction.anchor {
        let stroke = Stroke::new(1.5, PREVIEW);
        painter.line_segment([xf.to_screen(a), xf.to_screen(s.point)], stroke);
        let look = -(s.point - a).normalize_or_zero().perp();
        let mid = (a + s.point) * 0.5;
        painter.arrow(xf.to_screen(mid), (xf.to_screen(mid + look) - xf.to_screen(mid)).normalized() * 30.0, stroke);
    }
    if resp.clicked() {
        match state.interaction.anchor {
            None => state.interaction.anchor = Some(s.point),
            Some(a) if a.distance(s.point) > 0.1 => {
                let end = s.point;
                let command = views::ViewCreate {
                    kind: "section".into(),
                    start: Some(a.to_array()),
                    end: Some(end.to_array()),
                    ..Default::default()
                };
                let Some(id) = crate::actions::run_command(state, command).and_then(|o| o["id"].as_u64()) else {
                    return;
                };
                let id = ViewId(id);
                state.set_tool(Tool::Select);
                state.open_view(id);
            }
            _ => {}
        }
    }
}

/// Resize cursor along a screen direction (end handles stretch along the element).
fn resize_cursor(dir: egui::Vec2) -> egui::CursorIcon {
    // Fold the angle into [0°, 180°): screen y points down.
    let angle = dir.y.atan2(dir.x).to_degrees().rem_euclid(180.0);
    match angle {
        a if !(22.5..157.5).contains(&a) => egui::CursorIcon::ResizeHorizontal,
        a if a < 67.5 => egui::CursorIcon::ResizeNwSe,
        a if a < 112.5 => egui::CursorIcon::ResizeVertical,
        _ => egui::CursorIcon::ResizeNeSw,
    }
}

fn select(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    movable: bool,
    xf: &Xf,
    drawing: &Drawing2D,
) {
    let raw = resp.hover_pos().map(|p| xf.to_world(p));
    let tol = 5.0 / xf.scale;
    let (ctrl, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));

    // Pre-selection highlight.
    if state.interaction.drag.is_none()
        && let Some(hit) = raw.and_then(|p| drawing.hit_test(p, tol))
        && !state.selection.contains(&hit)
    {
        let only = BTreeSet::from([hit]);
        let opts = PaintOptions {
            highlight: &only,
            px_per_mm: 0.0,
            mono: Some(HIGHLIGHT.gamma_multiply(0.7)),
            only: Some(&only),
        };
        paint_drawing(painter, drawing, xf, &opts);
    }

    // End handles of selected walls, beams and grids (not pinned ones).
    let grips: Vec<(ElementId, bool, DVec2, DVec2)> = if movable && state.interaction.drag.is_none() {
        state
            .selection
            .iter()
            .take(50)
            .filter(|id| !state.project().is_pinned(**id))
            .filter_map(|id| state.project().element(*id).and_then(cc_core::wall_ops::line_of).map(|l| (*id, l)))
            .flat_map(|(id, (a, b))| [(id, true, a, b), (id, false, b, a)])
            .collect()
    } else {
        Vec::new()
    };
    // Curvature handles of selected arc walls, at the middle of the arc.
    let bends: Vec<(ElementId, DVec2, DVec2, cc_core::arc::Arc)> = if movable && state.interaction.drag.is_none() {
        state
            .selection
            .iter()
            .take(50)
            .filter(|id| !state.project().is_pinned(**id))
            .filter_map(|id| {
                let w = state.project().element(*id)?.as_wall()?;
                Some((*id, w.start, w.end, w.arc()?))
            })
            .collect()
    } else {
        Vec::new()
    };
    let grip_at = |p: Pos2| grips.iter().find(|g| xf.to_screen(g.2).distance(p) <= 7.0).copied();
    let bend_at = |p: Pos2| {
        bends.iter().find(|(.., arc)| xf.to_screen(arc.point(arc.length() * 0.5)).distance(p) <= 7.0).copied()
    };
    for (_, _, at, _) in &grips {
        let r = egui::Rect::from_center_size(xf.to_screen(*at), egui::vec2(9.0, 9.0));
        painter.rect_filled(r, 1.5, Color32::WHITE);
        painter.rect_stroke(r, 1.5, Stroke::new(1.5, HIGHLIGHT), egui::StrokeKind::Middle);
    }
    for (.., arc) in &bends {
        let c = xf.to_screen(arc.point(arc.length() * 0.5));
        painter.circle(c, 4.5, Color32::WHITE, Stroke::new(1.5, HIGHLIGHT));
    }
    if let Some((_, _, at, other)) = resp.hover_pos().and_then(grip_at) {
        ui.ctx().set_cursor_icon(resize_cursor(xf.to_screen(other) - xf.to_screen(at)));
    } else if let Some((.., arc)) = resp.hover_pos().and_then(bend_at) {
        // The handle moves the arc's middle toward or away from its center.
        let mid = arc.point(arc.length() * 0.5);
        ui.ctx().set_cursor_icon(resize_cursor(xf.to_screen(mid) - xf.to_screen(arc.center)));
    } else if movable && raw.is_some_and(|p| drawing.tag_at(p).is_some()) {
        // Tags are dragged to a new spot.
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    if resp.drag_started_by(egui::PointerButton::Primary) {
        let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or_default();
        let w = xf.to_world(origin);
        // A tag is dragged on its own, away from its element.
        if movable && let Some(tag) = drawing.tag_at(w) {
            state.interaction.drag = Some(Drag::Tag { tag, start: w });
            return;
        }
        if let Some((id, at_start, _, fixed)) = grip_at(origin) {
            state.interaction.drag = Some(Drag::Grip { id, at_start, fixed });
            return;
        }
        if let Some((id, a, b, _)) = bend_at(origin) {
            state.interaction.drag = Some(Drag::Bend { id, a, b });
            return;
        }
        // A drag moves what it starts on: an element's line, or an element
        // already selected. Elsewhere (empty paper, inside a floor or a
        // hatch) it draws a selection box.
        let grabbed = drawing
            .hit_test_line(w, tol)
            .or_else(|| drawing.hit_test(w, tol).filter(|id| state.selection.contains(id)));
        match grabbed {
            Some(hit) if movable => {
                if !state.selection.contains(&hit) {
                    state.selection = BTreeSet::from([hit]);
                }
                let ids = state.selection.iter().copied().collect();
                state.interaction.drag = Some(Drag::Move { start: w, ids, copy: ctrl });
            }
            _ => state.interaction.drag = Some(Drag::Box { start: origin }),
        }
    }

    let delta_of = |start: DVec2, raw: DVec2| {
        let step = round_step(xf.scale);
        let mut d = raw - start;
        if shift {
            d = if d.x.abs() >= d.y.abs() { DVec2::new(d.x, 0.0) } else { DVec2::new(0.0, d.y) };
        }
        (d / step).round() * step
    };

    // Dragging an end handle: the end snaps like a drawing point.
    if let (Some(Drag::Grip { id, at_start, fixed }), Some(raw)) = (state.interaction.drag.clone(), raw) {
        ui.ctx().set_cursor_icon(resize_cursor(xf.to_screen(raw) - xf.to_screen(fixed)));
        let level = state.project().element(id).map(|e| e.level);
        let s = snap_at(state, level, raw, Some(fixed), &[], xf.scale, shift);
        // A grid's end lines up with the ends of the other grids.
        let is_grid = state.project().element(id).is_some_and(|e| e.category() == Category::Grid);
        let s = if is_grid { align_grid_end(state.project(), Some(id), Some(fixed), raw, s, xf.scale) } else { s };
        draw_snap(painter, xf, s);
        // An arc wall keeps its bulge: the arc stretches with the end.
        let bulge = state.project().element(id).and_then(|e| e.as_wall()).map_or(0.0, |w| w.bulge);
        if bulge != 0.0 {
            let (a, b) = if at_start { (s.point, fixed) } else { (fixed, s.point) };
            arc_preview(painter, xf, a, b, bulge, 0.0);
        } else {
            painter.line_segment([xf.to_screen(fixed), xf.to_screen(s.point)], Stroke::new(2.0, PREVIEW));
        }
        temp_dimension(painter, xf, fixed, s.point, None, None, 0.0);
        if resp.drag_stopped() {
            state.interaction.drag = None;
            let end = if at_start { "start" } else { "end" };
            let command = modify_cmd::ElementMoveEnd { id: id.0, end: end.into(), point: s.point.to_array() };
            crate::actions::run_command(state, command);
        }
        return;
    }

    // Dragging an arc wall's curvature handle: the arc passes through the cursor
    // and its ends stay.
    if let (Some(Drag::Bend { id, a, b }), Some(raw)) = (state.interaction.drag.clone(), raw) {
        let bulge = cc_core::arc::Arc::bulge_through(a, raw, b);
        if let Some((bulge, arc)) = bulge.and_then(|bulge| Some((bulge, cc_core::arc::Arc::from_bulge(a, b, bulge)?))) {
            let mid = arc.point(arc.length() * 0.5);
            ui.ctx().set_cursor_icon(resize_cursor(xf.to_screen(mid) - xf.to_screen(arc.center)));
            arc_preview(painter, xf, a, b, bulge, 0.0);
            label(
                painter,
                xf.to_screen(raw),
                format!("R {}  {}", format::length(arc.radius), format::length(arc.length())),
            );
        }
        if resp.drag_stopped() {
            state.interaction.drag = None;
            match bulge {
                Some(bulge) => {
                    crate::engine_edit::element(state, id, None, |e| {
                        if let ElementKind::Wall(w) = &mut e.kind {
                            w.bulge = bulge;
                        }
                    });
                }
                None => state.message = t!("msg-arc-collinear"),
            }
        }
        return;
    }

    match (&state.interaction.drag, raw) {
        (Some(Drag::Move { start, ids, copy }), Some(raw)) => {
            let delta = delta_of(*start, raw);
            let only: BTreeSet<_> = ids.iter().copied().collect();
            let ghost = Xf { origin: xf.origin - delta, ..*xf };
            let opts = PaintOptions { highlight: &only, px_per_mm: 0.0, mono: Some(PREVIEW), only: Some(&only) };
            paint_drawing(painter, drawing, &ghost, &opts);
            let verb = if *copy { t!("label-copy") } else { t!("label-move") };
            label(painter, xf.to_screen(raw), format!("{verb} {}", format::length(delta.length())));
        }
        (Some(Drag::Tag { tag, start }), Some(raw)) => {
            // The tag's box where it would go.
            let center = tag.center + (raw - *start);
            let rect = egui::Rect::from_two_pos(xf.to_screen(center - tag.half), xf.to_screen(center + tag.half));
            painter.rect_stroke(rect.expand(2.0), 2.0, Stroke::new(1.5, PREVIEW), egui::StrokeKind::Middle);
        }
        (Some(Drag::Box { start }), _) => {
            if let Some(cur) = resp.interact_pointer_pos().or(resp.hover_pos()) {
                select_box(painter, *start, cur);
            }
        }
        _ => {}
    }

    if resp.drag_stopped() {
        match state.interaction.drag.take() {
            Some(Drag::Move { start, ids, copy }) => {
                if let Some(raw) = raw.or(state.cursor) {
                    let delta = delta_of(start, raw);
                    if delta.length() > 1e-6 {
                        if copy {
                            duplicate(state, &ids, delta);
                        } else {
                            move_elements(state, &ids, delta);
                        }
                    }
                }
            }
            // Handled above while the pointer is over the view.
            Some(Drag::Grip { .. } | Drag::Bend { .. }) => {}
            Some(Drag::Tag { tag, start }) => {
                if let (Some(raw), Some(view)) = (raw.or(state.cursor), state.active_view)
                    && raw.distance(start) > 1e-6
                {
                    let command = cc_engine::commands::annotate::TagMove {
                        view: Ref::Id(view.0),
                        id: tag.element.0,
                        delta: Some((raw - start).to_array()),
                        offset: None,
                        reset: false,
                    };
                    crate::actions::run_command(state, command);
                }
            }
            Some(Drag::Box { start }) => {
                let end = resp.interact_pointer_pos().or(resp.hover_pos()).unwrap_or(start);
                let (a, b) = (xf.to_world(start), xf.to_world(end));
                let ids = drawing.elements_in(a.min(b), a.max(b), end.x >= start.x);
                if !ctrl {
                    state.selection.clear();
                }
                state.selection.extend(ids);
            }
            None => {}
        }
    }

    if resp.clicked() {
        match raw.and_then(|p| drawing.hit_test(p, tol)) {
            Some(hit) if ctrl => {
                if !state.selection.remove(&hit) {
                    state.selection.insert(hit);
                }
            }
            Some(hit) => state.selection = BTreeSet::from([hit]),
            None if !ctrl => state.selection.clear(),
            None => {}
        }
    }
}

fn raw_ids(ids: &[ElementId]) -> Vec<u64> {
    ids.iter().map(|i| i.0).collect()
}

fn ids_of(v: &serde_json::Value) -> Vec<ElementId> {
    v.as_array().map(|a| a.iter().filter_map(serde_json::Value::as_u64).map(ElementId).collect()).unwrap_or_default()
}

/// After a move, rotate or mirror: tell how many pinned elements stayed.
fn note_pinned(state: &mut AppState, out: &serde_json::Value) {
    let pinned = ids_of(&out["pinned_skipped"]).len();
    if pinned > 0 {
        state.message = t!("msg-pinned-skipped", "count" => pinned);
    }
}

/// Translate elements; hosted elements slide along their host unless it moves too.
pub fn move_elements(state: &mut AppState, ids: &[ElementId], delta: DVec2) {
    let command = modify_cmd::ElementsMove { ids: raw_ids(ids), delta: delta.to_array() };
    if let Some(out) = crate::actions::run_command(state, command) {
        note_pinned(state, &out);
    }
}

/// Copy elements by `delta`, `count` times (an array); the copies are selected and returned.
pub fn copy_elements(state: &mut AppState, ids: &[ElementId], delta: DVec2, count: u32) -> Vec<ElementId> {
    let command = modify_cmd::ElementsCopy { ids: raw_ids(ids), delta: delta.to_array(), count: Some(count) };
    let created = crate::actions::run_command(state, command).map(|o| ids_of(&o["created"])).unwrap_or_default();
    state.selection = created.iter().copied().collect();
    created
}

/// Copy elements by `delta`; hosted elements follow copied hosts.
pub fn duplicate(state: &mut AppState, ids: &[ElementId], delta: DVec2) {
    copy_elements(state, ids, delta, 1);
}

/// Rotate (or copy rotated) by `angle` radians about `center`; returns the
/// elements rotated or created (copies are selected).
pub fn rotate_elements(
    state: &mut AppState,
    ids: &[ElementId],
    center: DVec2,
    angle: f64,
    copy: bool,
) -> Vec<ElementId> {
    let command = modify_cmd::ElementsRotate {
        ids: raw_ids(ids),
        angle: angle.to_degrees(),
        center: Some(center.to_array()),
        copy,
    };
    let Some(out) = crate::actions::run_command(state, command) else { return Vec::new() };
    if copy {
        let created = ids_of(&out["created"]);
        state.selection = created.iter().copied().collect();
        created
    } else {
        note_pinned(state, &out);
        ids_of(&out["changed"])
    }
}

/// Mirror (or copy mirrored) across the line through `a` and `b`; returns the
/// elements mirrored or created (copies are selected).
pub fn mirror_elements(state: &mut AppState, ids: &[ElementId], a: DVec2, b: DVec2, copy: bool) -> Vec<ElementId> {
    let command = modify_cmd::ElementsMirror {
        ids: raw_ids(ids),
        axis: Some([a.to_array(), b.to_array()]),
        axis_element: None,
        copy: Some(copy),
    };
    let Some(out) = crate::actions::run_command(state, command) else { return Vec::new() };
    if copy {
        let created = ids_of(&out["created"]);
        state.selection = created.iter().copied().collect();
        created
    } else {
        note_pinned(state, &out);
        ids_of(&out["changed"])
    }
}

/// Space on a selection: components and columns turn 90° about their
/// insertion point, doors and windows flip to the other side of the wall.
/// Returns whether anything changed (one undo step).
pub fn rotate_selection(state: &mut AppState) -> bool {
    let ids: Vec<ElementId> = state.selection.iter().copied().collect();
    if ids.is_empty() {
        return false;
    }
    let out = state.session.run(modify_cmd::ElementsTurn { ids: raw_ids(&ids) });
    out.is_ok_and(|o| !ids_of(&o["changed"]).is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 100 px per meter: the snap radius is 0.1 m.
    const SCALE: f64 = 100.0;

    #[test]
    fn typed_radius_takes_the_cursor_side() {
        use cc_core::arc::Arc;
        let (a, b) = (DVec2::ZERO, DVec2::new(4.0, 0.0));
        let mid = |p: DVec2| {
            let arc = Arc::from_bulge(a, b, bulge_with_radius(a, b, p, 4.0).unwrap()).unwrap();
            assert!((arc.radius - 4.0).abs() < 1e-9);
            (arc.point(arc.length() * 0.5), arc.sweep.abs().to_degrees())
        };
        // Near the chord: the shorter arc on the cursor's side (above, then below).
        let (m, sweep) = mid(DVec2::new(2.0, 0.5));
        assert!(m.y > 0.0 && (sweep - 60.0).abs() < 1e-9, "{m} {sweep}");
        let (m, sweep) = mid(DVec2::new(2.0, -0.5));
        assert!(m.y < 0.0 && (sweep - 60.0).abs() < 1e-9, "{m} {sweep}");
        // Far out on that side: the longer arc.
        let (m, sweep) = mid(DVec2::new(2.0, 7.0));
        assert!(m.y > 7.0 && (sweep - 300.0).abs() < 1e-9, "{m} {sweep}");
        // Too short a radius.
        assert_eq!(bulge_with_radius(a, b, DVec2::new(2.0, 1.0), 1.0), None);
    }

    #[test]
    fn typed_length_and_angle_place_the_segment_end() {
        let a = DVec2::new(1.0, 1.0);
        let mut typed = TypedInput::default();
        // Nothing typed: the cursor point.
        assert_eq!(typed.apply(a, DVec2::new(4.0, 1.0), None), DVec2::new(4.0, 1.0));
        "2,5".chars().for_each(|c| typed.push(c));
        assert!(typed.apply(a, DVec2::new(4.0, 1.0), None).distance(DVec2::new(3.5, 1.0)) < 1e-9);
        // Tab: digits go to the angle, turned to the cursor's side of the X axis.
        typed.angle_active = true;
        "90".chars().for_each(|c| typed.push(c));
        let below = typed.apply(a, DVec2::new(4.0, -0.5), None);
        assert!(
            below.distance(DVec2::new(1.0, -1.5)) < 1e-9,
            "{below} {:?} {:?}",
            typed.angle_value(),
            typed.length_value()
        );
        assert!(typed.apply(a, DVec2::new(4.0, 1.5), None).distance(DVec2::new(1.0, 3.5)) < 1e-9);
        typed.pop();
        typed.pop();
        assert!(typed.angle.is_empty() && typed.length == "2,5");
    }

    #[test]
    fn linear_dimensions_follow_the_cursor_side() {
        let (a, b) = (DVec2::new(0.0, 0.0), DVec2::new(4.0, 3.0));
        // Above the points' box: horizontal; beside it: vertical.
        assert_eq!(linear_direction(a, b, DVec2::new(2.0, 5.0)), DVec2::X);
        assert_eq!(linear_direction(a, b, DVec2::new(6.0, 1.0)), DVec2::Y);
        assert_eq!(
            line_intersection(a, DVec2::X, DVec2::new(2.0, 5.0), DVec2::new(2.0, 1.0)),
            Some(DVec2::new(2.0, 0.0))
        );
        assert_eq!(line_intersection(a, DVec2::X, DVec2::Y, DVec2::new(1.0, 1.0)), None);
    }

    #[test]
    fn ordinate_points_measure_from_the_origin_on_one_line() {
        let mut state = AppState::new(cc_core::Document::new(crate::home::sample_project("casa_simples").unwrap()));
        let plan = *state.project().views.iter().find(|(_, v)| v.is_plan()).unwrap().0;
        let type_id = state.project().default_type(Category::Dimension).unwrap();
        let origin = DVec2::new(100.0, 100.0);
        let line = origin - DVec2::Y;
        let chain = DimChain { view: plan, last: origin, dir: DVec2::X, line, linear: true, ordinate: true };
        for (x, total) in [(100.6, 0.6), (101.2, 1.2), (102.4, 2.4)] {
            assert!(place_chain(&mut state, type_id, &chain, DVec2::new(x, 100.2), -1.0));
            let [id] = state.selection.iter().copied().collect::<Vec<_>>()[..] else { panic!("one selected") };
            let Some(ElementKind::Dimension(d)) = state.project().element(id).map(|e| &e.kind) else { panic!() };
            assert!(matches!(d.kind, DimensionKind::Ordinate { .. }), "{d:?}");
            assert_eq!(d.a, origin);
            assert!((d.value() - total).abs() < 1e-9, "{d:?}");
            assert!((d.a.y + d.offset - line.y).abs() < 1e-9);
        }
    }

    #[test]
    fn a_continued_chain_stays_on_its_line_and_selects_the_new_dimension() {
        let mut state = AppState::new(cc_core::Document::new(crate::home::sample_project("casa_simples").unwrap()));
        let plan = *state.project().views.iter().find(|(_, v)| v.is_plan()).unwrap().0;
        let type_id = state.project().default_type(Category::Dimension).unwrap();
        let dims = |state: &AppState| -> Vec<cc_core::Dimension> {
            let mut ids: Vec<_> = state.selection.iter().copied().collect();
            ids.sort();
            ids.iter()
                .filter_map(|id| match &state.project().element(*id)?.kind {
                    ElementKind::Dimension(d) => Some((**d).clone()),
                    _ => None,
                })
                .collect()
        };
        // An aligned chain along a slanted line, its line 1.5 m to the left.
        let dir = DVec2::new(3.0, 4.0).normalize();
        let first = DVec2::new(100.0, 100.0);
        let line = first + dir.perp() * 1.5;
        let mut chain = DimChain { view: plan, last: first, dir, line, linear: false, ordinate: false };
        for (to, aligned) in [(first + dir * 5.0, true), (first + DVec2::new(7.0, 6.0), false)] {
            let offset = (chain.line - chain.last).dot(dir.perp());
            assert!(place_chain(&mut state, type_id, &chain, to, offset));
            let placed = dims(&state);
            let [d] = placed.as_slice() else { panic!("one new dimension selected: {placed:?}") };
            assert_eq!(matches!(d.kind, DimensionKind::Aligned), aligned, "{d:?}");
            let (pa, pb) = cc_core::dim_line::extent(d).unwrap();
            for p in [pa, pb] {
                assert!((p - line).perp_dot(dir).abs() < 1e-9, "{p} on the chain line");
            }
            chain.last = to;
        }
        // A segment across the direction measures nothing and is refused.
        let across = chain.last + dir.perp() * 2.0;
        assert!(!place_chain(&mut state, type_id, &chain, across, 0.0));
        assert!(!state.message.is_empty());
    }

    #[test]
    fn arcs_are_found_on_curved_linework_only() {
        let mut d = Drawing2D::default();
        let (c, r) = (DVec2::new(2.0, 1.0), 0.5);
        let pts: Vec<DVec2> =
            (0..24).map(|i| c + DVec2::from_angle(i as f64 / 24.0 * std::f64::consts::TAU) * r).collect();
        d.polyline(&pts, true, cc_views::LineStyle::Cut, Some(ElementId(7)));
        // A rectangle's corner is not an arc.
        let rect = [DVec2::new(5.0, 0.0), DVec2::new(7.0, 0.0), DVec2::new(7.0, 2.0), DVec2::new(5.0, 2.0)];
        d.polyline(&rect, true, cc_views::LineStyle::Cut, Some(ElementId(8)));
        let (center, radius) = arc_at(&d, c + DVec2::new(r, 0.01), 0.05).unwrap();
        assert!(center.distance(c) < 1e-9 && (radius - r).abs() < 1e-9);
        assert!(arc_at(&d, DVec2::new(7.0, 1.0), 0.05).is_none());
        assert_eq!(line_at(&d, DVec2::new(7.01, 1.0), 0.05), Some((DVec2::new(7.0, 0.0), DVec2::new(7.0, 2.0))));
    }

    fn empty() -> (Project, LevelId) {
        let p = crate::actions::new_project();
        let level = p.levels[0].id;
        (p, level)
    }

    /// The tools snap to an imported drawing's ends and circle centers in its view only.
    #[test]
    fn snaps_to_imported_drawings() {
        let dir = std::env::temp_dir().join(format!("ccad-app-snap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("snap.dxf");
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n3\n21\n0\n\
                   0\nCIRCLE\n8\n0\n10\n6\n20\n2\n40\n1\n0\nENDSEC\n0\nEOF\n";
        std::fs::write(&path, dxf).unwrap();
        let mut state = AppState::new(cc_core::Document::new(crate::actions::new_project()));
        let level = state.project().levels[0].id;
        let plan = state.project().views.values().find(|v| v.level() == Some(level)).map(|v| v.id).unwrap();
        state.active_view = Some(plan);
        crate::cad_ui::import_cad_path(&mut state, path);
        assert_eq!(state.selection.len(), 1, "{}", state.message);
        state.sync();
        let s = snap_at(&mut state, Some(level), DVec2::new(3.04, 0.03), None, &[], SCALE, false);
        assert_eq!(s.kind, SnapKind::Endpoint);
        assert!(s.point.distance(DVec2::new(3.0, 0.0)) < 1e-12);
        let s = snap_at(&mut state, Some(level), DVec2::new(6.03, 2.02), None, &[], SCALE, false);
        assert_eq!(s.kind, SnapKind::Center);
        assert!(s.point.distance(DVec2::new(6.0, 2.0)) < 1e-12);
        // Not in another view.
        let other = state.project().views.values().find(|v| v.level().is_some() && v.id != plan).map(|v| v.id);
        state.active_view = other;
        let s = snap_at(&mut state, Some(level), DVec2::new(6.03, 2.02), None, &[], SCALE, false);
        assert_ne!(s.kind, SnapKind::Center);
    }

    #[test]
    fn two_guides_cross_at_an_alignment() {
        let (p, level) = empty();
        // Acquired points (0, 0) and (5, 3): their vertical and horizontal guides cross at (5, 0) and (0, 3).
        let tracked = [DVec2::new(0.0, 0.0), DVec2::new(5.0, 3.0)];
        let s =
            snap(&p, &ModelGeometry::default(), Some(level), DVec2::new(5.04, 0.03), None, &[], &tracked, SCALE, false);
        assert_eq!(s.kind, SnapKind::Intersection);
        assert!(s.point.distance(DVec2::new(5.0, 0.0)) < 1e-12, "{}", s.point);
        assert!(s.guides.iter().all(Option::is_some));
    }

    #[test]
    fn the_drawing_direction_meets_a_guide() {
        let (p, level) = empty();
        // Drawing from (0, 0) at 30°: the line meets the vertical guide of (4, 9) at x = 4.
        let anchor = DVec2::ZERO;
        let dir = DVec2::from_angle(30f64.to_radians());
        let target = anchor + dir * (4.0 / dir.x);
        let raw = target + dir.perp() * 0.02 + DVec2::new(0.03, 0.0);
        let s = snap(
            &p,
            &ModelGeometry::default(),
            Some(level),
            raw,
            Some(anchor),
            &[],
            &[DVec2::new(4.0, 9.0)],
            SCALE,
            false,
        );
        assert_eq!(s.kind, SnapKind::Alignment);
        assert!(s.point.distance(target) < 1e-9, "{} vs {target}", s.point);
    }

    /// Issue #125: Backspace or Ctrl+Z takes back the last wall of a chain
    /// and the tool goes on from where it started; Ctrl+Y puts it back.
    #[test]
    fn chains_step_back_one_segment_without_leaving_the_tool() {
        use crate::commands::{UiCommand, run};
        let mut state = AppState::new(cc_core::Document::new(crate::home::sample_project("casa_simples").unwrap()));
        let level = state.project().levels[0].id;
        let type_id = Tool::for_category(state.project(), cc_core::Category::Wall).and_then(|t| match t {
            Tool::Wall { type_id } => Some(type_id),
            _ => None,
        });
        let type_id = type_id.unwrap();
        state.set_tool(Tool::Wall { type_id });
        let walls = |state: &AppState| {
            state.project().elements.values().filter(|e| e.category() == cc_core::Category::Wall).count()
        };
        let before = walls(&state);
        // Three walls, placed the way the tool places them.
        let pts = [DVec2::new(30.0, 30.0), DVec2::new(34.0, 30.0), DVec2::new(34.0, 33.0), DVec2::new(30.0, 33.0)];
        state.interaction.anchor = Some(pts[0]);
        for w in pts.windows(2) {
            let (at, depth) = (chain_at(&state), state.session.doc.undo_depth());
            create_wall(&mut state, level, type_id, w[0], w[1], 0.0);
            state.interaction.anchor = Some(w[1]);
            state.interaction.prev_dir = Some(w[0] - w[1]);
            chain_placed(&mut state, at, depth);
        }
        assert_eq!(walls(&state), before + 3);
        let ctx = egui::Context::default();
        // Ctrl+Z: the last wall goes, the chain goes on from its start, the tool stays.
        run(&mut state, &ctx, UiCommand::Undo);
        assert_eq!(walls(&state), before + 2);
        assert_eq!(state.interaction.anchor, Some(pts[2]));
        assert_eq!(state.interaction.prev_dir, Some(pts[1] - pts[2]));
        assert!(matches!(state.tool, Tool::Wall { .. }));
        // Ctrl+Y puts it back and the chain goes on from its end.
        run(&mut state, &ctx, UiCommand::Redo);
        assert_eq!((walls(&state), state.interaction.anchor), (before + 3, Some(pts[3])));
        // Backspace walks back the whole chain, then drops the start point.
        for (left, anchor) in [(2, pts[2]), (1, pts[1]), (0, pts[0])] {
            assert!(chain_back(&mut state));
            assert_eq!((walls(&state), state.interaction.anchor), (before + left, Some(anchor)));
        }
        assert!(chain_back(&mut state));
        assert_eq!(state.interaction.anchor, None);
        assert!(!chain_back(&mut state), "nothing left to take back");
        // A change made some other way is not undone by stepping back.
        state.interaction.anchor = Some(pts[0]);
        let (at, depth) = (chain_at(&state), state.session.doc.undo_depth());
        create_wall(&mut state, level, type_id, pts[0], pts[1], 0.0);
        state.interaction.anchor = Some(pts[1]);
        chain_placed(&mut state, at, depth);
        create_wall(&mut state, level, type_id, pts[2], pts[3], 0.0);
        assert!(chain_back(&mut state), "the anchor is dropped");
        assert_eq!(walls(&state), before + 2, "both walls stay");
    }

    #[test]
    fn space_turns_components_and_flips_doors() {
        let mut state = AppState::new(cc_core::Document::new(crate::home::sample_project("casa_simples").unwrap()));
        let pick = |state: &AppState, f: fn(&ElementKind) -> bool| {
            state.project().elements.values().find(|e| f(&e.kind)).map(|e| e.id).unwrap()
        };
        let comp = pick(&state, |k| matches!(k, ElementKind::Component(_)));
        let door = pick(&state, |k| matches!(k, ElementKind::Door(_)));
        let wall = pick(&state, |k| matches!(k, ElementKind::Wall(_)));
        let before = state.project().element(comp).cloned().unwrap();
        state.selection = [comp, door, wall].into_iter().collect();
        let depth = state.session.doc.undo_depth();
        assert!(rotate_selection(&mut state));
        assert_eq!(state.session.doc.undo_depth(), depth + 1);
        let (ElementKind::Component(a), Some(ElementKind::Component(b))) =
            (&before.kind, state.project().element(comp).map(|e| &e.kind))
        else {
            panic!("component")
        };
        assert_eq!(b.rotation_deg, (a.rotation_deg + 90.0).rem_euclid(360.0));
        assert_eq!(a.position, b.position);
        assert!(matches!(state.project().element(door).map(|e| &e.kind), Some(ElementKind::Door(h)) if h.flip_facing));
        // Nothing turnable selected: no undo step.
        state.selection = [wall].into_iter().collect();
        assert!(!rotate_selection(&mut state));
    }

    #[test]
    fn end_handles_use_a_resize_cursor_along_the_element() {
        use egui::{CursorIcon as C, vec2};
        assert_eq!(resize_cursor(vec2(10.0, 0.0)), C::ResizeHorizontal);
        assert_eq!(resize_cursor(vec2(-10.0, 1.0)), C::ResizeHorizontal);
        assert_eq!(resize_cursor(vec2(0.0, -10.0)), C::ResizeVertical);
        // Screen y points down: towards the lower right is NW-SE.
        assert_eq!(resize_cursor(vec2(10.0, 10.0)), C::ResizeNwSe);
        assert_eq!(resize_cursor(vec2(10.0, -10.0)), C::ResizeNeSw);
    }

    #[test]
    fn far_from_guides_the_pointer_is_free() {
        let (p, level) = empty();
        let s = snap(
            &p,
            &ModelGeometry::default(),
            Some(level),
            DVec2::new(2.33, 1.71),
            None,
            &[],
            &[DVec2::new(5.0, 5.0)],
            SCALE,
            false,
        );
        assert_eq!(s.kind, SnapKind::Free);
        assert!(s.guides.iter().all(Option::is_none));
        // Endpoints still win over alignments.
        let s = snap(
            &p,
            &ModelGeometry::default(),
            Some(level),
            DVec2::new(5.01, 5.0),
            None,
            &[DVec2::new(5.0, 5.0)],
            &[DVec2::ZERO],
            SCALE,
            false,
        );
        assert_eq!(s.kind, SnapKind::Endpoint);
    }

    /// Wall corners and faces are snap points, not only the wall's axis:
    /// sketching a ceiling inside a room lands on the inner face.
    #[test]
    fn wall_faces_and_corners_snap() {
        let p = crate::home::sample_project("casa_simples").unwrap();
        let geom = ModelGeometry::build(&p);
        let (id, w) = geom.walls.iter().find(|(_, w)| w.arc.is_none() && w.half > 0.05).unwrap();
        let level = p.element(*id).unwrap().level;
        let corner = w.footprint()[0];
        let s = snap(&p, &geom, Some(level), corner + DVec2::splat(0.004), None, &[], &[], SCALE, false);
        assert_eq!(s.kind, SnapKind::Endpoint);
        assert!(s.point.distance(corner) < 1e-9, "{} vs {corner}", s.point);
        // Along the right face, just outside it.
        let (u0, u1) = w.inner_range();
        let face = w.world(u0 + (u1 - u0) * 0.3, -w.half);
        let s = snap(&p, &geom, Some(level), face - w.normal * 0.01, None, &[], &[], SCALE, false);
        assert_eq!(s.kind, SnapKind::Nearest);
        assert!(s.point.distance(face) < 1e-6, "{} vs {face}", s.point);
    }

    /// A window tool clicked on a wall in a 3D view places the window there.
    #[test]
    fn windows_are_placed_on_walls_picked_in_3d() {
        let mut state = AppState::new(cc_core::Document::new(crate::home::sample_project("casa_simples").unwrap()));
        state.session.geometry();
        let window = state.project().default_type(Category::Window).unwrap();
        let (wall, g) = state.session.geom().walls.iter().max_by(|a, b| a.1.length.total_cmp(&b.1.length)).unwrap();
        let (wall, point) = (*wall, g.world(g.length * 0.5, -g.half));
        let before = state.project().elements.len();
        let tool = Tool::Hosted { category: Category::Window, type_id: window };
        place_opening_3d(&mut state, &tool, wall, point);
        assert_eq!(state.project().elements.len(), before + 1);
        let new = state.selection.iter().next().copied().unwrap();
        let e = state.project().element(new).unwrap();
        assert_eq!(e.as_hosted().map(|h| h.host), Some(wall));
        assert!(matches!(&e.kind, ElementKind::Window(h) if h.flip_facing));
    }
}
