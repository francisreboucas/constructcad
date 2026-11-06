//! Curtain walls: walls of a type with a curtain grid (see
//! `cc_core::curtain`). They are created with `wall_create` like other walls;
//! these commands make curtain wall types and adjust one wall's grid and panels.

use super::{edit, patch_value};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref, finite};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::curtain::{CurtainEdits, CurtainPanel, CurtainSystem, GridAxis, GridSegment, LINE_TOL, Layout, PanelSwap};
use cc_core::{ElementId, ElementKind, ElementType, ElementTypeId, Project, TypeData, WallFunction};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A material of the project by its template name, if the project has it.
fn template_material(p: &Project, key: &str) -> String {
    let name = crate::i18n::tr(key);
    p.material(&name).map_or(String::new(), |m| m.name.clone())
}

/// Component types are the only types a panel may be.
fn is_component(p: &Project, id: ElementTypeId) -> bool {
    matches!(p.element_type(id).map(|t| &t.data), Some(TypeData::Component { .. }))
}

/// The values are sound: repairing a copy would change nothing.
fn check_system(p: &Project, system: &CurtainSystem) -> Result<()> {
    match system.clone().repair(&|id| is_component(p, id)) {
        0 => Ok(()),
        _ => Err(EngineError::invalid(
            "invalid curtain values (spacings of at least 5 cm, 1 to 501 divisions, positive sizes, panels of component types)",
        )),
    }
}

/// Create a curtain wall type: walls of it are a grid of panels and
/// mullions. `curtain` changes the defaults (vertical and horizontal lines
/// every 1.5 m, glass panels, 5 x 15 cm mullions inside and around) and is
/// shaped like `params.curtain` in `type_get`: `vertical` and `horizontal`
/// {layout: `None`, `FixedDistance`, `FixedNumber` or `MaxSpacing`,
/// spacing, count, justification: `Start`, `Center` or `End`}, `panel`
/// {kind: `Glass` or `Solid`, thickness, material} or {kind: `Empty`} or
/// {kind: `Component`, component: type id}, and `vertical_mullion`,
/// `horizontal_mullion`, `border_mullion` {width, depth, material} or null
/// for none. Place walls of it with `wall_create`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct CurtainTypeCreate {
    /// Type name (unique among wall types).
    pub name: String,
    /// Changes to the default grid, panel and mullions.
    #[serde(default)]
    pub curtain: Option<Value>,
}

impl Command for CurtainTypeCreate {
    const NAME: &'static str = "curtain_type_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let name = self.name.trim().to_string();
        if name.is_empty() || p.types_of(cc_core::Category::Wall).any(|t| t.name.trim() == name) {
            return Err(EngineError::invalid(format!("the name '{name}' is empty or already used")));
        }
        let mut system = CurtainSystem::with_materials(
            &template_material(p, "tpl-mat-glass"),
            &template_material(p, "tpl-mat-steel"),
        );
        if let Some(patch) = &self.curtain {
            system = patch_value(&system, patch)?;
        }
        check_system(p, &system)?;
        let id = edit(s, t!("tx-new-curtain-type"), |e| {
            let id = ElementTypeId(e.alloc_id());
            let data = TypeData::Wall { layers: Vec::new(), function: WallFunction::Exterior, curtain: Some(system) };
            Ok(e.add_type(ElementType { id, name, data, info: Default::default(), uid: Default::default() }))
        })?;
        Ok(super::query::type_json(s, id).unwrap_or_else(|| json!({ "id": id.0 })))
    }
}

/// A curtain wall's grid as laid out now, and its base elevation.
fn layout_of(s: &mut Session, element: u64) -> Result<(ElementId, Layout, f64)> {
    let id = refs::element(s.project(), element)?;
    let is_curtain = s
        .project()
        .element(id)
        .filter(|e| matches!(e.kind, ElementKind::Wall(_)))
        .and_then(|e| s.project().element_type(e.type_id))
        .is_some_and(|t| t.curtain().is_some());
    if !is_curtain {
        return Err(EngineError::invalid(format!("element {element} is not a curtain wall")));
    }
    let g = s.geometry().walls.get(&id).ok_or_else(|| EngineError::rejected("the wall has no geometry"))?;
    let c = g.curtain.as_ref().ok_or_else(|| EngineError::rejected("the wall has no curtain grid"))?;
    Ok((id, c.layout.clone(), g.base_z))
}

fn round(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// Show a curtain wall's grid: its interior lines (`vertical`: distances
/// along the wall from its start; `horizontal`: heights above its base),
/// the panels by column and row (from the start and from the base; merged
/// cells give their lowest) with their extent, kind and whether they are
/// swapped, and its panel count, glazed area and mullion length.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct CurtainGet {
    /// The curtain wall's id.
    pub element: u64,
}

impl Command for CurtainGet {
    const NAME: &'static str = "curtain_get";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (id, l, base) = layout_of(s, self.element)?;
        let p = s.project();
        let panels: Vec<Value> = l
            .panels
            .iter()
            .map(|c| {
                let mut v = json!({
                    "column": c.column,
                    "row": c.row,
                    "u": [round(c.u.0), round(c.u.1)],
                    "z": [round(c.z.0 - base), round(c.z.1 - base)],
                    "kind": c.panel.key().trim_start_matches("panel-"),
                    "swapped": c.swapped,
                });
                match &c.panel {
                    CurtainPanel::Glass { thickness, material } | CurtainPanel::Solid { thickness, material } => {
                        v["thickness"] = json!(thickness);
                        v["material"] = json!(material);
                    }
                    CurtainPanel::Component { component } => {
                        v["component"] = json!(component.0);
                        v["component_name"] = json!(p.element_type(*component).map_or("", |t| t.name.as_str()));
                    }
                    CurtainPanel::Empty => {}
                }
                v
            })
            .collect();
        Ok(json!({
            "element": id.0,
            "start": round(l.u.first().copied().unwrap_or(0.0)),
            "end": round(l.u.last().copied().unwrap_or(0.0)),
            "height": round(l.z.last().copied().unwrap_or(base) - base),
            "vertical": l.interior(GridAxis::Vertical).iter().map(|u| round(*u)).collect::<Vec<_>>(),
            "horizontal": l.interior(GridAxis::Horizontal).iter().map(|z| round(z - base)).collect::<Vec<_>>(),
            "columns": l.columns(),
            "rows": l.rows(),
            "panels": panels,
            "mullions": l.mullions.len(),
            "panel_count": l.panel_count(),
            "glazed_area": round(l.glazed_area()),
            "mullion_length": round(l.mullion_length()),
        }))
    }
}

/// A direction of a curtain grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub enum Axis {
    /// Vertical lines, spaced along the wall.
    Vertical,
    /// Horizontal lines, spaced up from the base.
    Horizontal,
}

impl From<Axis> for GridAxis {
    fn from(a: Axis) -> Self {
        match a {
            Axis::Vertical => GridAxis::Vertical,
            Axis::Horizontal => GridAxis::Horizontal,
        }
    }
}

/// Adjust one curtain wall's grid on one axis. Lines are numbered from 0 in
/// the order `curtain_get` lists them (`vertical` or `horizontal`); positions
/// are distances along the wall from its start for vertical lines and heights
/// above the base for horizontal ones. In order: `reset` goes back to the
/// type's lines; `lines` gives all of them; `move` moves lines `[index, to]`;
/// `remove` takes lines out; `add` puts lines in. Any of these gives the wall
/// its own lines (later type changes no longer move them). Segments are a
/// line's pieces between the lines across it (`[line, segment]`, segment
/// numbered from the base or from the start, as the grid was before this
/// command): `remove_segments` merges the cells on both sides into one panel
/// with no mullion between, `restore_segments` puts them back.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CurtainGridSet {
    /// The curtain wall's id.
    pub element: u64,
    /// Which lines: `Vertical` or `Horizontal`.
    pub axis: Axis,
    /// Go back to the type's lines (and drop removed segments of this axis).
    #[serde(default)]
    pub reset: bool,
    /// All the interior lines, replacing the current ones.
    #[serde(default)]
    pub lines: Option<Vec<f64>>,
    /// Lines to move, as `[index, new position]`.
    #[serde(default, rename = "move")]
    pub move_lines: Vec<(usize, f64)>,
    /// Indices of lines to remove.
    #[serde(default)]
    pub remove: Vec<usize>,
    /// Positions of new lines.
    #[serde(default)]
    pub add: Vec<f64>,
    /// Segments to remove, as `[line, segment]`.
    #[serde(default)]
    pub remove_segments: Vec<[usize; 2]>,
    /// Removed segments to put back, as `[line, segment]`.
    #[serde(default)]
    pub restore_segments: Vec<[usize; 2]>,
}

impl Command for CurtainGridSet {
    const NAME: &'static str = "curtain_grid_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (id, l, base) = layout_of(s, self.element)?;
        let axis = GridAxis::from(self.axis);
        // Positions are kept as `u` for vertical lines and heights above the base for horizontal ones.
        let (lo, hi, current, across): (f64, f64, Vec<f64>, Vec<f64>) = match axis {
            GridAxis::Vertical => (
                l.u.first().copied().unwrap_or(0.0),
                l.u.last().copied().unwrap_or(0.0),
                l.interior(axis).to_vec(),
                l.z.iter().map(|z| z - base).collect(),
            ),
            GridAxis::Horizontal => (
                0.0,
                l.z.last().copied().unwrap_or(base) - base,
                l.interior(axis).iter().map(|z| z - base).collect(),
                l.u.clone(),
            ),
        };
        let inside = |v: f64, what: &str| -> Result<f64> {
            let v = finite(v, what)?;
            if v > lo + LINE_TOL && v < hi - LINE_TOL {
                Ok(v)
            } else {
                Err(EngineError::invalid(format!("{what} {v} is not between {} and {}", round(lo), round(hi))))
            }
        };
        let line_at = |i: usize| -> Result<f64> {
            current.get(i).copied().ok_or_else(|| EngineError::invalid(format!("there is no line {i}")))
        };
        // A segment as stored: the line's position and the middle of the piece.
        let segment = |[i, k]: [usize; 2]| -> Result<GridSegment> {
            let at = line_at(i)?;
            let (a, b) = (across.get(k), across.get(k + 1));
            let (Some(a), Some(b)) = (a, b) else {
                return Err(EngineError::invalid(format!("line {i} has no segment {k}")));
            };
            Ok(GridSegment { axis, at, along: (a + b) * 0.5 })
        };
        let removing = self.remove_segments.iter().map(|s| segment(*s)).collect::<Result<Vec<_>>>()?;
        let restoring = self.restore_segments.iter().map(|s| segment(*s)).collect::<Result<Vec<_>>>()?;
        let moves = self
            .move_lines
            .iter()
            .map(|&(i, to)| Ok((line_at(i)?, inside(to, "move")?)))
            .collect::<Result<Vec<_>>>()?;
        let removed_lines = self.remove.iter().map(|&i| line_at(i)).collect::<Result<Vec<_>>>()?;
        let given = self
            .lines
            .as_ref()
            .map(|ls| ls.iter().map(|v| inside(*v, "line")).collect::<Result<Vec<_>>>())
            .transpose()?;
        let added = self.add.iter().map(|v| inside(*v, "add")).collect::<Result<Vec<_>>>()?;
        let line_ops = given.is_some() || !moves.is_empty() || !removed_lines.is_empty() || !added.is_empty();
        if self.reset && line_ops {
            return Err(EngineError::invalid("`reset` goes back to the type's lines; give it alone"));
        }
        if !self.reset && !line_ops && removing.is_empty() && restoring.is_empty() {
            return Err(EngineError::invalid("nothing to change"));
        }
        let same = |a: f64, b: f64| (a - b).abs() < LINE_TOL * 2.0;
        edit(s, t!("tx-curtain-grid"), |e| {
            let Some(ElementKind::Wall(w)) = e.element_mut(id).map(|x| &mut x.kind) else {
                return Err(EngineError::not_found("wall"));
            };
            let edits: &mut CurtainEdits = &mut w.curtain;
            let own = match axis {
                GridAxis::Vertical => &mut edits.vertical,
                GridAxis::Horizontal => &mut edits.horizontal,
            };
            if self.reset {
                *own = None;
                edits.removed.retain(|g| g.axis != axis);
            } else if line_ops {
                let mut lines = given.clone().unwrap_or_else(|| current.clone());
                for &(from, to) in &moves {
                    if let Some(x) = lines.iter_mut().find(|x| same(**x, from)) {
                        *x = to;
                    }
                }
                lines.retain(|x| !removed_lines.iter().any(|r| same(*x, *r)));
                lines.extend(added.iter().copied());
                lines.sort_by(f64::total_cmp);
                lines.dedup_by(|a, b| same(*a, *b));
                *own = Some(lines);
            }
            // Removed segments follow moved lines and go with removed ones.
            for seg in edits.removed.iter_mut().filter(|g| g.axis == axis) {
                if let Some(&(_, to)) = moves.iter().find(|(from, _)| same(seg.at, *from)) {
                    seg.at = to;
                }
            }
            edits.removed.retain(|g| g.axis != axis || !removed_lines.iter().any(|r| same(g.at, *r)));
            for r in &restoring {
                edits.removed.retain(|g| !(g.axis == axis && same(g.at, r.at) && same(g.along, r.along)));
            }
            for r in &removing {
                if !edits.removed.iter().any(|g| g.axis == axis && same(g.at, r.at) && same(g.along, r.along)) {
                    edits.removed.push(*r);
                }
            }
            Ok(())
        })?;
        CurtainGet { element: id.0 }.run(s)
    }
}

/// What a curtain panel becomes.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelArg {
    /// `Glass`, `Solid`, `Empty` or `Component`.
    pub kind: PanelKind,
    /// Panel thickness, meters (glass and solid; default 24 mm).
    #[serde(default)]
    pub thickness: Option<f64>,
    /// Material name or index (glass and solid; default the type panel's material).
    #[serde(default)]
    pub material: Option<Ref>,
    /// The component type id or name (component panels): a door or window
    /// fills the panel's opening; `Width` and `Height` parameters take its size.
    #[serde(default)]
    pub component: Option<Ref>,
}

/// The kinds of curtain panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub enum PanelKind {
    Glass,
    Solid,
    Empty,
    Component,
}

impl PanelArg {
    fn resolve(&self, p: &Project, default: &CurtainPanel) -> Result<CurtainPanel> {
        let thickness = match self.thickness {
            Some(t) if t.is_finite() && t > 0.0 && t <= 2.0 => t,
            Some(_) => return Err(EngineError::invalid("thickness must be positive (at most 2 m)")),
            None => Some(default.thickness()).filter(|t| *t > 0.0).unwrap_or(0.024),
        };
        let material = match &self.material {
            Some(r) => refs::material(p, r)?,
            None => default.material().unwrap_or("").to_string(),
        };
        Ok(match self.kind {
            PanelKind::Glass => CurtainPanel::Glass { thickness, material },
            PanelKind::Solid => CurtainPanel::Solid { thickness, material },
            PanelKind::Empty => CurtainPanel::Empty,
            PanelKind::Component => {
                let r = self
                    .component
                    .as_ref()
                    .ok_or_else(|| EngineError::invalid("a component panel needs `component`"))?;
                let id = refs::element_type(p, r)?;
                if !is_component(p, id) {
                    return Err(EngineError::invalid("the panel's type is not a component"));
                }
                CurtainPanel::Component { component: id }
            }
        })
    }
}

/// Swap panels of one curtain wall: the panels of cells `[column, row]`
/// (numbered from the start and from the base, as `curtain_get` lists them;
/// a cell of merged cells swaps all of them) become `panel`, or the type's
/// again when `panel` is null.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CurtainPanelSet {
    /// The curtain wall's id.
    pub element: u64,
    /// The cells, as `[column, row]`.
    pub cells: Vec<[usize; 2]>,
    /// The new panel (null: the type's).
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

impl Command for CurtainPanelSet {
    const NAME: &'static str = "curtain_panel_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        if self.cells.is_empty() {
            return Err(EngineError::invalid("no cells given"));
        }
        let (id, l, base) = layout_of(s, self.element)?;
        let p = s.project();
        let default = p
            .element(id)
            .and_then(|e| p.element_type(e.type_id))
            .and_then(|t| t.curtain())
            .map(|c| c.panel.clone())
            .unwrap_or_default();
        let panel = self.panel.as_ref().map(|a| a.resolve(p, &default)).transpose()?;
        // Each cell's panel: the extent of its merged cells, and the point stored for it.
        let mut targets = Vec::new();
        for &[c, r] in &self.cells {
            let cell = l.panel_at(c, r).ok_or_else(|| EngineError::invalid(format!("there is no cell [{c}, {r}]")))?;
            let (u, z) =
                l.cell_center(c, r).ok_or_else(|| EngineError::invalid(format!("there is no cell [{c}, {r}]")))?;
            targets.push((cell.cell_u, (cell.cell_z.0 - base, cell.cell_z.1 - base), u, z - base));
        }
        edit(s, t!("tx-curtain-panel"), |e| {
            let Some(ElementKind::Wall(w)) = e.element_mut(id).map(|x| &mut x.kind) else {
                return Err(EngineError::not_found("wall"));
            };
            for (cu, cz, u, z) in &targets {
                w.curtain.panels.retain(|s| !(cu.0 <= s.u && s.u <= cu.1 && cz.0 <= s.z && s.z <= cz.1));
                if let Some(panel) = &panel {
                    w.curtain.panels.push(PanelSwap { u: *u, z: *z, panel: panel.clone() });
                }
            }
            Ok(())
        })?;
        CurtainGet { element: id.0 }.run(s)
    }
}
