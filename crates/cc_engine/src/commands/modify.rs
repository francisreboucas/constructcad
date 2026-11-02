//! Modifying elements: delete, move, copy/array, rotate, mirror, pin, flip,
//! joins, trim/extend, split, offset, align, end points.

use super::{edit, ids_json};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, finite, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{ElementId, ElementKind, PlanTransform, Project};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Refuse to edit a pinned element.
fn not_pinned(p: &Project, id: ElementId) -> Result<()> {
    if p.is_pinned(id) { Err(EngineError::rejected(t!("msg-pinned-skipped", "count" => 1))) } else { Ok(()) }
}

/// Split `ids` into free and pinned elements (pinned ones are not changed).
fn unpinned(p: &Project, ids: Vec<ElementId>) -> (Vec<ElementId>, Vec<ElementId>) {
    ids.into_iter().partition(|id| !p.is_pinned(*id))
}

fn transform(s: &mut Session, ids: &[u64], t: PlanTransform, tx: String) -> Result<Value> {
    let ids = refs::elements(s.project(), ids)?;
    let (free, pinned) = unpinned(s.project(), ids);
    let changed = edit(s, tx, |e| Ok(e.transform_elements(&free, &t)))?;
    Ok(json!({ "changed": ids_json(&changed), "pinned_skipped": ids_json(&pinned) }))
}

fn copy(s: &mut Session, ids: &[u64], transforms: &[PlanTransform], tx: String) -> Result<Value> {
    let ids = refs::elements(s.project(), ids)?;
    let created = edit(s, tx, |e| Ok(e.copy_elements(&ids, transforms)))?;
    Ok(json!({ "created": ids_json(&created) }))
}

/// Delete elements (doors and windows of deleted walls go too; pinned elements stay).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementsDelete {
    /// Element ids.
    pub ids: Vec<u64>,
}

impl Command for ElementsDelete {
    const NAME: &'static str = "elements_delete";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let ids = refs::elements(s.project(), &self.ids)?;
        let (free, pinned) = unpinned(s.project(), ids);
        edit(s, t!("tx-delete"), |e| {
            for id in &free {
                e.remove_element(*id);
            }
            Ok(())
        })?;
        Ok(json!({ "deleted": ids_json(&free), "pinned_skipped": ids_json(&pinned) }))
    }
}

/// Move elements by `delta` (walls take their doors and windows; doors and
/// windows alone slide along their wall).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementsMove {
    /// Element ids.
    pub ids: Vec<u64>,
    /// Displacement `[dx, dy]` in meters.
    pub delta: Point,
}

impl Command for ElementsMove {
    const NAME: &'static str = "elements_move";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        transform(s, &self.ids, PlanTransform::translate(point(self.delta)?), t!("tx-move"))
    }
}

/// Copy elements by `delta`; with `count` > 1, a linear array of that many
/// copies spaced by `delta`.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementsCopy {
    /// Element ids.
    pub ids: Vec<u64>,
    /// Displacement `[dx, dy]` of the first copy, meters (the next ones repeat it).
    pub delta: Point,
    /// Number of copies (default 1, at most 1000).
    #[serde(default)]
    pub count: Option<u32>,
}

impl Command for ElementsCopy {
    const NAME: &'static str = "elements_copy";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let delta = point(self.delta)?;
        let count = self.count.unwrap_or(1).clamp(1, 1000);
        let transforms: Vec<PlanTransform> =
            (1..=count).map(|k| PlanTransform::translate(delta * f64::from(k))).collect();
        let tx = if count > 1 { t!("tx-array") } else { t!("tx-copy") };
        copy(s, &self.ids, &transforms, tx)
    }
}

/// Center of the elements' extents in plan.
fn center_of(s: &mut Session, ids: &[ElementId]) -> Option<DVec2> {
    let mut pts: Vec<DVec2> = Vec::new();
    let geom = s.geometry().clone();
    let p = s.project();
    for id in ids {
        for solid in geom.solids_of(*id) {
            if let Some((lo, hi)) = solid.bounds() {
                pts.extend([lo.truncate(), hi.truncate()]);
            }
        }
        let Some(e) = p.element(*id) else { continue };
        if let Some((a, b)) = cc_core::wall_ops::line_of(e) {
            pts.extend([a, b]);
        }
        if let ElementKind::Room(r) = &e.kind {
            pts.push(r.position);
        }
    }
    let (lo, hi) = pts
        .iter()
        .fold(None, |acc: Option<(DVec2, DVec2)>, q| Some(acc.map_or((*q, *q), |(lo, hi)| (lo.min(*q), hi.max(*q)))))?;
    Some((lo + hi) * 0.5)
}

/// Rotate elements by `angle` degrees (counter-clockwise) about `center`
/// (default: the center of their extents); with `copy`, rotated copies.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementsRotate {
    /// Element ids.
    pub ids: Vec<u64>,
    /// Angle in degrees, counter-clockwise.
    pub angle: f64,
    /// Center of rotation (default: the center of the elements' extents).
    #[serde(default)]
    pub center: Option<Point>,
    /// Rotate copies, keeping the originals.
    #[serde(default)]
    pub copy: bool,
}

impl Command for ElementsRotate {
    const NAME: &'static str = "elements_rotate";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let ids = refs::elements(s.project(), &self.ids)?;
        let center = match self.center {
            Some(c) => point(c)?,
            None => center_of(s, &ids).ok_or_else(|| EngineError::invalid("give a center"))?,
        };
        let t = PlanTransform::rotate_about(center, finite(self.angle, "angle")?.to_radians());
        if self.copy { copy(s, &self.ids, &[t], t!("tx-rotate")) } else { transform(s, &self.ids, t, t!("tx-rotate")) }
    }
}

/// Mirror elements across an axis: two points, or the line of a wall, grid,
/// beam or railing (`axis_element`). Copies by default (`copy: false` moves them).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementsMirror {
    /// Element ids.
    pub ids: Vec<u64>,
    /// Two points of the axis.
    #[serde(default)]
    pub axis: Option<[Point; 2]>,
    /// Id of an element whose line is the axis.
    #[serde(default)]
    pub axis_element: Option<u64>,
    /// Keep the originals and mirror copies (default true).
    #[serde(default)]
    pub copy: Option<bool>,
}

impl Command for ElementsMirror {
    const NAME: &'static str = "elements_mirror";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let (a, b) = match (self.axis, self.axis_element) {
            (Some([a, b]), _) => (point(a)?, point(b)?),
            (None, Some(id)) => {
                let e = p.element(refs::element(p, id)?).ok_or_else(|| EngineError::not_found("element"))?;
                match &e.kind {
                    ElementKind::Wall(w) => p.wall_axis(e, w),
                    _ => {
                        cc_core::wall_ops::line_of(e).ok_or_else(|| EngineError::invalid("the element has no line"))?
                    }
                }
            }
            (None, None) => return Err(EngineError::invalid("give an axis or an axis_element")),
        };
        if a.distance(b) < 1e-9 {
            return Err(EngineError::invalid("the axis has no length"));
        }
        let t = PlanTransform::mirror_across(a, b - a);
        if self.copy.unwrap_or(true) {
            copy(s, &self.ids, &[t], t!("tx-mirror"))
        } else {
            transform(s, &self.ids, t, t!("tx-mirror"))
        }
    }
}

/// Pin elements (they cannot be moved or deleted) or unpin them.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementsPin {
    /// Element ids.
    pub ids: Vec<u64>,
    /// `true` to pin, `false` to unpin.
    pub pinned: bool,
}

impl Command for ElementsPin {
    const NAME: &'static str = "elements_pin";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let ids = refs::elements(s.project(), &self.ids)?;
        let tx = if self.pinned { t!("tx-pin") } else { t!("tx-unpin") };
        let changed = edit(s, tx, |e| Ok(e.set_pinned(&ids, self.pinned)))?;
        Ok(json!({ "changed": changed }))
    }
}

/// Flip a door or window to the other side of its wall (`facing`) and/or to the other jamb (`hand`).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementFlip {
    /// Id of the door or window.
    pub id: u64,
    /// Flip to the other side of the wall.
    #[serde(default)]
    pub facing: bool,
    /// Flip the hinge to the other jamb.
    #[serde(default)]
    pub hand: bool,
}

impl Command for ElementFlip {
    const NAME: &'static str = "element_flip";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::element(s.project(), self.id)?;
        not_pinned(s.project(), id)?;
        edit(s, t!("tx-modify"), |e| {
            let h = e
                .element_mut(id)
                .and_then(|x| x.as_hosted_mut())
                .ok_or_else(|| EngineError::invalid("not a door or window"))?;
            h.flip_facing ^= self.facing;
            h.flip_hand ^= self.hand;
            Ok(())
        })?;
        Ok(super::created(s, id))
    }
}

/// Allow or disallow joining at one end of a wall or beam. A wall end joins
/// other walls; a beam end stops at the face of a structural column it lies
/// in, or meets other beams at the same depth (mitered corners, butt joints).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WallJoin {
    /// Wall or beam id.
    pub id: u64,
    /// `start` or `end`.
    pub end: String,
    /// `true` to allow joining at that end, `false` to keep it free.
    pub joined: bool,
}

fn at_start(end: &str) -> Result<bool> {
    match end {
        "start" => Ok(true),
        "end" => Ok(false),
        other => Err(EngineError::invalid(format!("end must be 'start' or 'end', not '{other}'"))),
    }
}

impl Command for WallJoin {
    const NAME: &'static str = "wall_join";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::element(s.project(), self.id)?;
        let start = at_start(&self.end)?;
        let name = match s.project().element(id).map(|x| &x.kind) {
            Some(ElementKind::Beam(_)) => t!("tx-beam-join"),
            _ => t!("tx-wall-join"),
        };
        edit(s, name, |e| match e.element_mut(id).map(|x| &mut x.kind) {
            Some(ElementKind::Wall(w)) => {
                *(if start { &mut w.join_start } else { &mut w.join_end }) = self.joined;
                Ok(())
            }
            Some(ElementKind::Beam(b)) => {
                *(if start { &mut b.join_start } else { &mut b.join_end }) = self.joined;
                Ok(())
            }
            _ => Err(EngineError::invalid("not a wall or beam")),
        })?;
        Ok(super::created(s, id))
    }
}

/// Join the geometry of two elements that touch or overlap (walls, floors,
/// ceilings, roofs, columns, beams, stairs, railings, components). Where they
/// overlap, one cuts the other: structural columns and beams cut walls,
/// floors, ceilings and roofs; floors, ceilings and roofs cut walls; walls
/// cut architectural columns; between equals the first cuts the second. The
/// shared volume is counted once, and plans and sections draw two elements
/// cut in the same material as one piece.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GeometryJoin {
    /// The first element.
    pub first: u64,
    /// The second element.
    pub second: u64,
}

/// The two elements of a join, checked: distinct, with a body.
fn join_pair(p: &Project, first: u64, second: u64) -> Result<(ElementId, ElementId)> {
    let (a, b) = (refs::element(p, first)?, refs::element(p, second)?);
    if a == b {
        return Err(EngineError::invalid(t!("msg-join-same")));
    }
    for id in [a, b] {
        if !p.element(id).is_some_and(|e| cc_core::can_join_geometry(e.category())) {
            return Err(EngineError::invalid(t!("msg-join-category")));
        }
    }
    Ok((a, b))
}

/// Whether two elements' bodies touch or overlap.
fn bodies_meet(geom: &cc_geom::ModelGeometry, a: ElementId, b: ElementId) -> bool {
    let bounds =
        |id| geom.solids_of(id).filter_map(cc_geom::Solid::bounds).reduce(|(lo, hi), (l, h)| (lo.min(l), hi.max(h)));
    let (Some((alo, ahi)), Some((blo, bhi))) = (bounds(a), bounds(b)) else { return false };
    let eps = glam::DVec3::splat(1e-6);
    alo.cmple(bhi + eps).all() && blo.cmple(ahi + eps).all()
}

impl Command for GeometryJoin {
    const NAME: &'static str = "geometry_join";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (a, b) = join_pair(s.project(), self.first, self.second)?;
        if !bodies_meet(s.geometry(), a, b) {
            return Err(EngineError::invalid(t!("msg-join-apart")));
        }
        let p = s.project();
        let priority = |id| p.element(id).map_or(0, cc_core::join_priority);
        let (cutting, cut) = if priority(b) > priority(a) { (b, a) } else { (a, b) };
        edit(s, t!("tx-join-geometry"), |e| {
            e.set_joined(cutting, cut, true);
            Ok(())
        })?;
        Ok(json!({"cutting": cutting.0, "cut": cut.0}))
    }
}

/// Unjoin the geometry of two joined elements: each keeps its whole body again.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GeometryUnjoin {
    /// The first element.
    pub first: u64,
    /// The second element.
    pub second: u64,
}

impl Command for GeometryUnjoin {
    const NAME: &'static str = "geometry_unjoin";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (a, b) = join_pair(s.project(), self.first, self.second)?;
        let Some((cutting, cut)) = s.project().join_of(a, b) else {
            return Err(EngineError::not_found(t!("msg-join-none")));
        };
        edit(s, t!("tx-unjoin-geometry"), |e| {
            e.set_joined(cutting, cut, false);
            Ok(())
        })?;
        Ok(json!({"unjoined": [a.0, b.0]}))
    }
}

/// Group elements: selecting one of them selects them all, so they move,
/// copy and rotate together. Elements already in a group leave it.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroupCreate {
    /// The elements (at least two).
    pub ids: Vec<u64>,
    /// Group name (default: "Group N").
    #[serde(default)]
    pub name: Option<String>,
}

impl Command for GroupCreate {
    const NAME: &'static str = "group_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let members: std::collections::BTreeSet<ElementId> = refs::elements(p, &self.ids)?.into_iter().collect();
        if members.len() < 2 {
            return Err(EngineError::invalid(t!("msg-group-two")));
        }
        let name = match self.name.map(|n| n.trim().to_string()) {
            Some(n) if !n.is_empty() => n,
            Some(_) => return Err(EngineError::invalid(t!("msg-group-name"))),
            None => (1..)
                .map(|n| t!("group-default-name", "n" => n))
                .find(|n| !p.groups.values().any(|g| &g.name == n))
                .unwrap_or_default(),
        };
        let mut id = cc_core::GroupId(0);
        edit(s, t!("tx-group"), |e| {
            id = e.add_group(name.clone(), members.clone());
            Ok(())
        })?;
        Ok(json!({"id": id.0, "name": name, "members": members.iter().map(|m| m.0).collect::<Vec<_>>()}))
    }
}

/// Ungroup: the group goes, its elements stay.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroupUngroup {
    /// Group id (see `groups_list`, or `group` in `element_get`).
    pub group: u64,
}

impl Command for GroupUngroup {
    const NAME: &'static str = "group_ungroup";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = cc_core::GroupId(self.group);
        let Some(group) = s.project().groups.get(&id).cloned() else {
            return Err(EngineError::not_found(t!("msg-group-missing")));
        };
        edit(s, t!("tx-ungroup"), |e| {
            e.remove_group(id);
            Ok(())
        })?;
        Ok(json!({"ungrouped": group.members.iter().map(|m| m.0).collect::<Vec<_>>()}))
    }
}

/// Trim or extend two linear elements (walls, beams, grids) to meet at a
/// corner; each pick point marks the part of its element to keep. Arc walls
/// meet lines and other arcs where their circles cross them.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrimCorner {
    /// First element id.
    pub a: u64,
    /// A point on the part of `a` to keep.
    pub pick_a: Point,
    /// Second element id.
    pub b: u64,
    /// A point on the part of `b` to keep.
    pub pick_b: Point,
}

impl Command for TrimCorner {
    const NAME: &'static str = "trim_corner";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (a, b) = (refs::element(s.project(), self.a)?, refs::element(s.project(), self.b)?);
        not_pinned(s.project(), a)?;
        not_pinned(s.project(), b)?;
        let (pa, pb) = (point(self.pick_a)?, point(self.pick_b)?);
        edit(s, t!("tx-trim"), |e| Ok(e.trim_corner(a, pa, b, pb)?))?;
        Ok(json!({ "changed": [a.0, b.0] }))
    }
}

/// Trim or extend a linear element to the line of a reference element (the
/// circle of an arc wall); the pick point marks the part to keep. Arc walls
/// trim and extend along their circle.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrimExtend {
    /// Element to trim or extend.
    pub id: u64,
    /// A point on the part to keep.
    pub pick: Point,
    /// Id of the element whose line is the boundary.
    pub reference: u64,
}

fn path(s: &Session, id: u64) -> Result<cc_core::wall_ops::Path> {
    let p = s.project();
    let e = p.element(refs::element(p, id)?).ok_or_else(|| EngineError::not_found("element"))?;
    cc_core::wall_ops::path_of(e).ok_or_else(|| EngineError::invalid(format!("element {id} has no line")))
}

impl Command for TrimExtend {
    const NAME: &'static str = "trim_extend";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::element(s.project(), self.id)?;
        not_pinned(s.project(), id)?;
        let reference = path(s, self.reference)?;
        let pick = point(self.pick)?;
        edit(s, t!("tx-trim"), |e| Ok(e.trim_to(id, pick, reference)?))?;
        Ok(super::created(s, id))
    }
}

/// Split a linear element (wall, beam, grid) at a point; returns the new element.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementSplit {
    /// Element id.
    pub id: u64,
    /// Where to split (projected on the element's line).
    pub point: Point,
}

impl Command for ElementSplit {
    const NAME: &'static str = "element_split";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::element(s.project(), self.id)?;
        not_pinned(s.project(), id)?;
        let at = point(self.point)?;
        let new = edit(s, t!("tx-split"), |e| Ok(e.split(id, at)?))?;
        Ok(json!({ "original": id.0, "created": new.0 }))
    }
}

/// Copy a linear element parallel at `distance` (positive: to the left of its direction).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementOffset {
    /// Element id.
    pub id: u64,
    /// Offset distance, meters: positive to the left of start→end, negative to the right.
    pub distance: f64,
}

impl Command for ElementOffset {
    const NAME: &'static str = "element_offset";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::element(s.project(), self.id)?;
        let d = finite(self.distance, "distance")?;
        let new = edit(s, t!("tx-offset"), |e| Ok(e.offset_copy(id, d)?))?;
        Ok(super::created(s, new))
    }
}

/// Align an element onto a line of a reference element, in plan: the edge
/// picked on the element (a wall's face, a column's side) moves onto the
/// line picked on the reference (a face or side, or the location line of a
/// grid or of an element picked without a point). When the two are not
/// parallel the element first turns about the picked point of its edge. An
/// arc wall moves onto the circle of a concentric arc wall (its radius
/// changes). Alignment is in plan, so the elements may be on different
/// levels (a column above onto the face of the column below).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementAlign {
    /// Element to move.
    pub id: u64,
    /// Element whose line it moves onto.
    pub reference: u64,
    /// Plan point `[x, y]` near the edge of `id` that moves onto the reference line (the edge of
    /// its plan outline nearest to it). Without it a linear element moves its location line, and
    /// other elements their insertion point, onto the line.
    #[serde(default)]
    pub pick: Option<Point>,
    /// Plan point `[x, y]` near the line of `reference` to align to (the edge of its plan outline
    /// nearest to it, such as a wall face). Without it the reference's location line is used,
    /// which a column or component does not have.
    #[serde(default)]
    pub reference_pick: Option<Point>,
}

/// The line of element `id` that Align uses: the edge of its plan outline
/// nearest to `pick` (a wall face, a column side), else its location line
/// (grids have only that). An arc wall gives its arc.
pub fn align_line(
    project: &cc_core::Project,
    geom: &cc_geom::ModelGeometry,
    id: ElementId,
    pick: Option<DVec2>,
) -> Option<cc_core::wall_ops::Path> {
    use cc_core::wall_ops::{Path, path_of};
    let path = project.element(id).and_then(path_of);
    if let Some(Path::Arc(_)) = path {
        return path;
    }
    pick.and_then(|at| geom.nearest_plan_edge(id, at)).map(|(a, b)| Path::Line(a, b)).or(path)
}

/// The turn (radians, within ±90°) that makes direction `from` parallel to `to`.
fn turn_to_parallel(from: DVec2, to: DVec2) -> f64 {
    let angle = from.perp_dot(to).atan2(from.dot(to));
    let half = std::f64::consts::FRAC_PI_2;
    if angle > half {
        angle - std::f64::consts::PI
    } else if angle <= -half {
        angle + std::f64::consts::PI
    } else {
        angle
    }
}

impl Command for ElementAlign {
    const NAME: &'static str = "element_align";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        use cc_core::wall_ops::{EditError, Path};
        let id = refs::element(s.project(), self.id)?;
        not_pinned(s.project(), id)?;
        let reference_id = refs::element(s.project(), self.reference)?;
        let pick = self.pick.map(point).transpose()?;
        let reference_pick = self.reference_pick.map(point).transpose()?;
        s.geometry();
        let (project, geom) = (&s.doc.project, s.geom());
        let reference = align_line(project, geom, reference_id, reference_pick).ok_or_else(|| {
            EngineError::invalid(format!(
                "element {} has no line: give reference_pick near the edge to align to",
                self.reference
            ))
        })?;
        let moved = align_line(project, geom, id, pick);
        // Arc walls onto concentric arcs.
        if let (Some(Path::Arc(_)), Path::Arc(_)) = (&moved, &reference) {
            edit(s, t!("tx-align"), |e| Ok(e.align_to(id, reference)?))?;
            return Ok(super::created(s, id));
        }
        let Path::Line(r0, r1) = reference else { return Err(EditError::Curved.into()) };
        let v = (r1 - r0).normalize_or_zero();
        if v == DVec2::ZERO {
            return Err(EditError::Degenerate.into());
        }
        // The point of the element that lands on the reference line, and the
        // direction of its edge (none for a lone insertion point).
        let (on, dir) = match moved {
            Some(Path::Line(a, b)) => {
                let u = (b - a).normalize_or_zero();
                if u == DVec2::ZERO {
                    return Err(EditError::Degenerate.into());
                }
                let at = pick.map_or(a, |p| a + u * (p - a).dot(u));
                (at, Some(u))
            }
            Some(Path::Arc(_)) => return Err(EditError::Curved.into()),
            None => (insertion_point(s, id)?, None),
        };
        let turn = dir.map_or(0.0, |u| turn_to_parallel(u, v));
        let n = v.perp();
        let shift = n * (r0 - on).dot(n);
        let mut t = PlanTransform::translate(shift);
        if turn.abs() > 1e-9 {
            t = PlanTransform::rotate_about(on, turn).then(t);
        }
        edit(s, t!("tx-align"), |e| {
            e.transform_elements(&[id], &t);
            Ok(())
        })?;
        Ok(super::created(s, id))
    }
}

/// Where an element without a location line stands in plan: a column's or
/// component's position, else the center of its outline.
fn insertion_point(s: &mut Session, id: ElementId) -> Result<DVec2> {
    match s.project().element(id).map(|e| &e.kind) {
        Some(ElementKind::Column(c)) => return Ok(c.position),
        Some(ElementKind::Component(c)) => return Ok(c.position),
        _ => {}
    }
    let solids = s.geometry().solids_of(id).filter_map(|solid| solid.bounds());
    let bounds = solids.reduce(|(a, b), (c, d)| (a.min(c), b.max(d)));
    let (lo, hi) =
        bounds.ok_or_else(|| EngineError::invalid(format!("element {} has no outline to align by", id.0)))?;
    Ok(DVec2::new(lo.x + hi.x, lo.y + hi.y) * 0.5)
}

/// Move one end of a wall, beam or grid (joined wall ends follow).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementMoveEnd {
    /// Element id.
    pub id: u64,
    /// `start` or `end`.
    pub end: String,
    /// New position of that end.
    pub point: Point,
}

impl Command for ElementMoveEnd {
    const NAME: &'static str = "element_move_end";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::element(s.project(), self.id)?;
        not_pinned(s.project(), id)?;
        let start = at_start(&self.end)?;
        let to = point(self.point)?;
        edit(s, t!("tx-modify"), |e| Ok(e.move_endpoint(id, start, to)?))?;
        Ok(super::created(s, id))
    }
}

/// Give grids the extents of a reference grid, so their bubbles line up:
/// each grid parallel to the reference (within 1°) has its ends moved along
/// its own line onto the lines through the reference's ends perpendicular
/// to it (each end to the reference end on its side). Grids that are not
/// parallel and pinned grids are left alone.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GridsAlignEnds {
    /// Id of the reference grid.
    pub reference: u64,
    /// Ids of the grids to align (default: every other grid parallel to the reference).
    #[serde(default)]
    pub grids: Vec<u64>,
    /// Which ends move: `both` (default), `start` (the bubble end) or `end`.
    #[serde(default)]
    pub ends: Option<String>,
}

impl Command for GridsAlignEnds {
    const NAME: &'static str = "grids_align_ends";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let grid_of = |id: ElementId| match p.element(id).map(|e| &e.kind) {
            Some(ElementKind::Grid(g)) => Ok(g),
            _ => Err(EngineError::invalid(format!("element {} is not a grid", id.0))),
        };
        let reference_id = refs::element(p, self.reference)?;
        let reference = grid_of(reference_id)?.clone();
        let (start, end) = match self.ends.as_deref().unwrap_or("both") {
            "both" => (true, true),
            "start" => (true, false),
            "end" => (false, true),
            other => return Err(EngineError::invalid(format!("ends must be 'both', 'start' or 'end', not '{other}'"))),
        };
        let ids: Vec<ElementId> = if self.grids.is_empty() {
            super::create::grids(p).map(|(id, _)| id).collect()
        } else {
            let ids = refs::elements(p, &self.grids)?;
            for id in &ids {
                grid_of(*id)?;
            }
            ids
        };
        let ids: Vec<ElementId> = ids.into_iter().filter(|id| *id != reference_id).collect();
        let (free, pinned) = unpinned(p, ids);
        let mut lines: Vec<(ElementId, (DVec2, DVec2))> = Vec::new();
        let mut not_parallel: Vec<ElementId> = Vec::new();
        for id in free {
            let Ok(g) = grid_of(id) else { continue };
            match cc_core::grid::aligned_ends(g, &reference, start, end) {
                Some((a, b)) if g.start.distance(a) > 1e-9 || g.end.distance(b) > 1e-9 => lines.push((id, (a, b))),
                Some(_) => {}
                None => not_parallel.push(id),
            }
        }
        // Explicit ids that are not parallel are reported; by default they are just other grids.
        if self.grids.is_empty() {
            not_parallel.clear();
        }
        let changed: Vec<ElementId> = lines.iter().map(|(id, _)| *id).collect();
        if !lines.is_empty() {
            edit(s, t!("tx-align-grid-ends"), |e| {
                for (id, (a, b)) in &lines {
                    if let Some(ElementKind::Grid(g)) = e.element_mut(*id).map(|el| &mut el.kind) {
                        (g.start, g.end) = (*a, *b);
                    }
                }
                Ok(())
            })?;
        }
        Ok(json!({
            "changed": ids_json(&changed),
            "pinned_skipped": ids_json(&pinned),
            "not_parallel": ids_json(&not_parallel),
        }))
    }
}

/// Turn elements a quarter: components and columns rotate 90° about their
/// insertion point; doors and windows flip to the other side of their wall
/// (the Space key on a selection). Others and pinned elements are left alone.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementsTurn {
    /// Element ids.
    pub ids: Vec<u64>,
}

impl Command for ElementsTurn {
    const NAME: &'static str = "elements_turn";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let ids = refs::elements(s.project(), &self.ids)?;
        let (free, pinned) = unpinned(s.project(), ids);
        let turnable: Vec<ElementId> = free
            .into_iter()
            .filter(|id| {
                matches!(
                    s.project().element(*id).map(|e| &e.kind),
                    Some(
                        ElementKind::Component(_)
                            | ElementKind::Column(_)
                            | ElementKind::Door(_)
                            | ElementKind::Window(_)
                    )
                )
            })
            .collect();
        edit(s, t!("tx-rotate"), |e| {
            for id in &turnable {
                let Some(el) = e.element_mut(*id) else { continue };
                match &mut el.kind {
                    ElementKind::Component(c) => c.rotation_deg = (c.rotation_deg + 90.0).rem_euclid(360.0),
                    ElementKind::Column(c) => c.rotation_deg = (c.rotation_deg + 90.0).rem_euclid(360.0),
                    ElementKind::Door(h) | ElementKind::Window(h) => h.flip_facing = !h.flip_facing,
                    _ => {}
                }
            }
            Ok(())
        })?;
        Ok(json!({ "changed": ids_json(&turnable), "pinned_skipped": ids_json(&pinned) }))
    }
}

/// The elements as a portable set for `elements_paste`: doors and windows of
/// the given walls come along. The set keeps type, level and view ids, so it
/// pastes into this project or one sharing them.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementsExport {
    /// Element ids.
    pub ids: Vec<u64>,
}

impl Command for ElementsExport {
    const NAME: &'static str = "elements_export";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let ids = refs::elements(s.project(), &self.ids)?;
        let set = s.project().copy_set(&ids);
        Ok(json!({ "elements": serde_json::to_value(set).map_err(|e| EngineError::invalid(e.to_string()))? }))
    }
}

/// Paste a set from `elements_export` moved by `delta`, on `level` when
/// given (else on their own levels). Elements whose type, level, view or
/// wall does not exist here are skipped. Returns the new ids.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElementsPaste {
    /// The `elements` of an `elements_export` result.
    pub elements: Value,
    /// Displacement `[dx, dy]`, meters (default: none).
    #[serde(default)]
    pub delta: Option<Point>,
    /// Level id or name to paste on (default: each element's own level).
    #[serde(default)]
    pub level: Option<crate::refs::Ref>,
}

impl Command for ElementsPaste {
    const NAME: &'static str = "elements_paste";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let set: Vec<cc_core::Element> = serde_json::from_value(self.elements)
            .map_err(|e| EngineError::invalid(format!("invalid elements: {e}")))?;
        let t = PlanTransform::translate(self.delta.map(point).transpose()?.unwrap_or(DVec2::ZERO));
        let level = self.level.as_ref().map(|l| refs::level(s.project(), l)).transpose()?;
        let created = edit(s, t!("tx-paste"), |e| Ok(e.paste_elements(&set, &[t], level)))?;
        Ok(json!({ "created": ids_json(&created) }))
    }
}

/// Attach the tops of walls to a roof, ceiling or floor (`top`), or take
/// them off it (`detach`). Under the element a wall rises or drops to its
/// underside, following its slope (a gable wall ends in a triangle);
/// elsewhere it keeps its own top. The walls follow the element when it
/// changes.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WallAttach {
    /// Wall ids.
    pub ids: Vec<u64>,
    /// The roof, ceiling or floor to attach the tops to.
    #[serde(default)]
    pub top: Option<u64>,
    /// Take the tops off what they are attached to.
    #[serde(default)]
    pub detach: bool,
}

impl Command for WallAttach {
    const NAME: &'static str = "wall_attach";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let target = match (self.top, self.detach) {
            (Some(_), true) => return Err(EngineError::invalid("give top or detach, not both")),
            (None, false) => {
                return Err(EngineError::invalid("give the roof, ceiling or floor to attach to, or detach"));
            }
            (Some(t), false) => {
                let t = refs::element(p, t)?;
                match p.element(t).map(|e| &e.kind) {
                    Some(ElementKind::Roof(_) | ElementKind::Ceiling(_) | ElementKind::Floor(_)) => Some(t),
                    _ => return Err(EngineError::invalid(format!("element {} is not a roof, ceiling or floor", t.0))),
                }
            }
            (None, true) => None,
        };
        if self.ids.is_empty() {
            return Err(EngineError::invalid("no walls given"));
        }
        let mut walls = Vec::with_capacity(self.ids.len());
        for id in &self.ids {
            let id = refs::element(p, *id)?;
            if !matches!(p.element(id).map(|e| &e.kind), Some(ElementKind::Wall(_))) {
                return Err(EngineError::invalid(format!("element {} is not a wall", id.0)));
            }
            walls.push(id);
        }
        edit(s, t!("tx-wall-attach"), |e| {
            for id in &walls {
                if let Some(ElementKind::Wall(w)) = e.element_mut(*id).map(|x| &mut x.kind) {
                    w.attach_top = target;
                }
            }
            Ok(())
        })?;
        Ok(json!({ "ids": walls.iter().map(|id| id.0).collect::<Vec<_>>(), "top": target.map(|t| t.0) }))
    }
}
