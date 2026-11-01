//! Building services: pipes and conduits, plumbing systems and electrical
//! circuits, and what they add up to (lengths, loads, fixture units).

use super::{add_element, created, edit, patch_value};
use crate::defaults::{conduit_offset, drain_slope, pipe_offset};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, finite, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{
    Category, Element, ElementId, ElementKind, MepData, MepSegment, MepSystem, Project, SystemId, SystemKind,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

fn kind_name(kind: SystemKind) -> String {
    kind.name().to_string()
}

pub(crate) fn parse_kind(name: &str) -> Result<SystemKind> {
    let wanted = name.trim().to_ascii_lowercase();
    // Snake-case names, or the model's own variant names (`ColdWater`).
    let found = SystemKind::ALL
        .into_iter()
        .find(|k| kind_name(*k) == wanted || format!("{k:?}").to_ascii_lowercase() == wanted);
    found.ok_or_else(|| {
        let names: Vec<String> = SystemKind::ALL.into_iter().map(kind_name).collect();
        EngineError::invalid(format!("unknown system kind '{name}' (one of {})", names.join(", ")))
    })
}

/// Whether `system` may hold something electrical (`Some(true)`), of the
/// plumbing (`Some(false)`) or either (`None`).
pub(crate) fn check_fits(p: &Project, electrical: Option<bool>, system: SystemId) -> Result<()> {
    let Some(sys) = p.system(system) else {
        return Err(EngineError::not_found(format!("no system with id {}", system.0)));
    };
    match electrical {
        Some(electrical) if electrical != sys.kind.is_electrical() => {
            Err(EngineError::invalid(t!("msg-system-mismatch", "system" => sys.name.clone())))
        }
        _ => Ok(()),
    }
}

/// Whether an element of this MEP data is electrical (devices), of the plumbing (fixtures) or neither.
pub(crate) fn is_electrical(mep: Option<MepData>) -> Option<bool> {
    match mep {
        Some(MepData::Device { .. }) => Some(true),
        Some(MepData::Fixture { .. }) => Some(false),
        None => None,
    }
}

/// Whether `system` may hold element `e`: pipes and plumbing fixtures belong
/// to piping systems, conduits and electrical devices to circuits.
fn check_member(p: &Project, e: &Element, system: SystemId) -> Result<()> {
    let electrical = match &e.kind {
        ElementKind::Pipe(_) => Some(false),
        ElementKind::Conduit(_) => Some(true),
        ElementKind::Component(_) => is_electrical(p.mep_data(e)),
        _ => return Err(EngineError::invalid(format!("element {} cannot belong to a system", e.id.0))),
    };
    check_fits(p, electrical, system)
}

fn system_out(p: &Project, id: SystemId) -> Value {
    let Some(sys) = p.system(id) else { return json!({ "id": id.0 }) };
    json!({
        "id": id.0,
        "name": sys.name,
        "code": sys.code,
        "kind": kind_name(sys.kind),
        "color": sys.color(),
        "panel": sys.panel.map(|e| e.0),
        "voltage": sys.voltage,
    })
}

/// The given plumbing system, or the first system of `kind`.
fn optional_system(p: &Project, r: Option<&Ref>) -> Result<Option<SystemId>> {
    r.map(|r| refs::system(p, r)).transpose()
}

/// Where a segment ends: `end`, or straight up or down from `start` (a riser).
fn segment(start: Point, end: Option<Point>, offset: f64, end_offset: Option<f64>) -> Result<MepSegment> {
    let start = point(start)?;
    let end = end.map(point).transpose()?.unwrap_or(start);
    let mut s = MepSegment::new(start, end, finite(offset, "offset")?);
    if let Some(z) = end_offset {
        s.end_offset = finite(z, "end_offset")?;
    }
    if s.plan_length() < 1e-3 && s.rise().abs() < 1e-3 {
        return Err(EngineError::invalid(t!("msg-segment-empty")));
    }
    Ok(s)
}

/// The elevation above `level` where a new run starting at `start` joins a
/// run of its network there (pipes of `system`, any conduit; risers of other
/// floors included): the run's elevation at that point, or on a riser the
/// height nearest `wanted` within it.
fn joined_offset(
    p: &Project,
    level: cc_core::LevelId,
    start: Point,
    conduit: bool,
    system: Option<SystemId>,
    wanted: f64,
) -> Option<f64> {
    let start = point(start).ok()?;
    p.segments_at(level)
        .filter(|(e, seg)| match e.kind {
            ElementKind::Pipe(_) => !conduit && seg.system() == system,
            _ => conduit,
        })
        .filter_map(|(_, seg)| {
            let (q, z) = nearest_on(&seg, start, wanted);
            (q.distance(start) <= cc_geom::mep_net::TOL).then_some(z)
        })
        .min_by(|a, b| (a - wanted).abs().total_cmp(&(b - wanted).abs()))
}

/// Create a pipe from `start` to `end` (or a vertical riser at `start` up or
/// down to `end_offset`), in a plumbing system. Its center line starts
/// `offset` above the level (default: where it starts on a pipe of its
/// system, such as the end of a run or a riser; else 0.60 m for supply and
/// vent pipes, -0.30 m for drains). Drains fall along the flow by `slope` (percent;
/// default 2 % up to 75 mm, 1 % above); other pipes are level unless
/// `end_offset` or `slope` says otherwise.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct PipeCreate {
    /// Start of the center line in plan.
    pub start: Point,
    /// End in plan (default: the start, for a riser).
    #[serde(default)]
    pub end: Option<Point>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Pipe type id or name (default: the first pipe type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Plumbing system id, name or code (e.g. "AF").
    #[serde(default)]
    pub system: Option<Ref>,
    /// Elevation of the start above the level, meters.
    #[serde(default)]
    pub offset: Option<f64>,
    /// Elevation of the end above the level, meters (overrides `slope`).
    #[serde(default)]
    pub end_offset: Option<f64>,
    /// Fall from the start to the end, percent of the plan length.
    #[serde(default)]
    pub slope: Option<f64>,
}

impl Command for PipeCreate {
    const NAME: &'static str = "pipe_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let type_id = refs::type_of(p, Category::Pipe, self.type_ref.as_ref())?;
        let system = optional_system(p, self.system.as_ref())?;
        if let Some(id) = system {
            check_fits(p, Some(false), id)?;
        }
        let kind = system.and_then(|id| p.system(id)).map(|sys| sys.kind);
        let offset = self.offset.unwrap_or_else(|| {
            let wanted = pipe_offset(kind);
            joined_offset(p, level, self.start, false, system, wanted).unwrap_or(wanted)
        });
        let mut seg = segment(self.start, self.end, offset, self.end_offset)?;
        if self.end_offset.is_none() && !seg.is_vertical() {
            let diameter = p.element_type(type_id).and_then(|t| t.diameter()).unwrap_or(0.1);
            let slope = match self.slope {
                Some(percent) => finite(percent, "slope")? / 100.0,
                None if kind.is_some_and(SystemKind::is_drainage) => drain_slope(diameter),
                None => 0.0,
            };
            seg.end_offset = seg.start_offset - slope * seg.plan_length();
        }
        seg.systems = system.into_iter().collect();
        let id = edit(s, t!("tx-create-pipe"), |e| Ok(add_element(e, level, type_id, ElementKind::Pipe(seg))))?;
        Ok(created(s, id))
    }
}

/// Create a conduit from `start` to `end` (or a vertical drop at `start` to
/// `end_offset`) carrying `circuits`. It runs `offset` above the level
/// (default: where it starts on another conduit, such as the end of a run or
/// a vertical drop; else in the slab above, at the level height).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ConduitCreate {
    /// Start of the center line in plan.
    pub start: Point,
    /// End in plan (default: the start, for a vertical drop).
    #[serde(default)]
    pub end: Option<Point>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Conduit type id or name (default: the first conduit type).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Circuits it carries: ids, names or codes.
    #[serde(default)]
    pub circuits: Vec<Ref>,
    /// Elevation of the start above the level, meters.
    #[serde(default)]
    pub offset: Option<f64>,
    /// Elevation of the end above the level, meters (default: as the start).
    #[serde(default)]
    pub end_offset: Option<f64>,
}

impl Command for ConduitCreate {
    const NAME: &'static str = "conduit_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let type_id = refs::type_of(p, Category::Conduit, self.type_ref.as_ref())?;
        let mut circuits = Vec::new();
        for r in &self.circuits {
            let id = refs::system(p, r)?;
            check_fits(p, Some(true), id)?;
            if !circuits.contains(&id) {
                circuits.push(id);
            }
        }
        let offset = self.offset.unwrap_or_else(|| {
            let wanted = conduit_offset(p, level);
            joined_offset(p, level, self.start, true, None, wanted).unwrap_or(wanted)
        });
        let mut seg = segment(self.start, self.end, offset, self.end_offset)?;
        seg.systems = circuits;
        let id = edit(s, t!("tx-create-conduit"), |e| Ok(add_element(e, level, type_id, ElementKind::Conduit(seg))))?;
        Ok(created(s, id))
    }
}

/// List the plumbing systems and electrical circuits with what is in them:
/// element count, pipe or conduit length, fixture units and supply weights
/// of plumbing fixtures, and the devices and load of circuits.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct SystemsList {}

impl Command for SystemsList {
    const NAME: &'static str = "systems_list";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let out: Vec<Value> = p
            .systems
            .keys()
            .map(|id| {
                let mut v = system_out(p, *id);
                let totals = totals(p, *id);
                if let Some(o) = v.as_object_mut() {
                    o.extend(totals.as_object().cloned().unwrap_or_default());
                }
                v
            })
            .collect();
        Ok(json!(out))
    }
}

/// What a system adds up to.
fn totals(p: &Project, id: SystemId) -> Value {
    let (mut count, mut length, mut power, mut devices, mut units, mut weight) = (0, 0.0, 0.0, 0, 0.0, 0.0);
    for e in p.system_members(id) {
        count += 1;
        if let Some(seg) = e.as_segment() {
            length += seg.length();
        }
        match p.mep_data(e) {
            Some(MepData::Device { power: va, .. }) => {
                devices += 1;
                power += va;
            }
            Some(MepData::Fixture { drain_units, supply_weight, .. }) => {
                units += drain_units;
                weight += supply_weight;
            }
            None => {}
        }
    }
    let mut v = json!({"elements": count, "length": length});
    let electrical = p.system(id).is_some_and(|s| s.kind.is_electrical());
    if let Some(o) = v.as_object_mut() {
        if electrical {
            let volts = p.system(id).map_or(127.0, |s| s.voltage);
            o.insert("devices".into(), json!(devices));
            o.insert("power_va".into(), json!(power));
            o.insert("current_a".into(), json!(if volts > 0.0 { power / volts } else { 0.0 }));
        } else {
            o.insert("drain_units".into(), json!(units));
            o.insert("supply_weight".into(), json!(weight));
        }
    }
    v
}

/// Create a plumbing system or an electrical circuit. `kind` is
/// `cold_water`, `hot_water`, `sanitary`, `vent`, `rainwater`, `power` or
/// `lighting`; circuits may name the board (`panel`, a component element;
/// default: the project's only board, if it has one) feeding them and their
/// voltage (default 127 V). A circuit's code defaults to the next free
/// number on its board.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct SystemCreate {
    /// What it carries.
    pub kind: String,
    /// Name (default: the code, or the kind's name).
    #[serde(default)]
    pub name: Option<String>,
    /// Short code shown in tags (e.g. "AF", a circuit number).
    #[serde(default)]
    pub code: Option<String>,
    /// Display color `[r, g, b]` (default: by kind).
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// Circuits: the distribution board feeding it (element id; default: the only board).
    #[serde(default)]
    pub panel: Option<u64>,
    /// Circuits: nominal voltage, volts.
    #[serde(default)]
    pub voltage: Option<f64>,
}

/// The distribution boards of the project.
fn boards(p: &Project) -> Vec<ElementId> {
    p.elements
        .values()
        .filter(|e| p.mep_data(e).and_then(|m| m.device()) == Some(cc_core::DeviceKind::Panel))
        .map(|e| e.id)
        .collect()
}

impl Command for SystemCreate {
    const NAME: &'static str = "system_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let kind = parse_kind(&self.kind)?;
        let panel = match self.panel.map(|id| refs::element(p, id)).transpose()? {
            Some(panel) => {
                check_panel(p, panel)?;
                Some(panel)
            }
            None if kind.is_electrical() => match boards(p)[..] {
                [only] => Some(only),
                _ => None,
            },
            None => None,
        };
        let code = match self.code {
            Some(c) => c.trim().to_string(),
            None if kind.is_electrical() => crate::defaults::next_number(
                p.systems.values().filter(|c| c.kind.is_electrical() && c.panel == panel).map(|c| c.code.as_str()),
            ),
            None => String::new(),
        };
        let name = match self.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
            Some(n) => n,
            None if kind.is_electrical() => t!("default-circuit-name", "n" => code.clone()),
            None => crate::i18n::tr(kind.key()),
        };
        let voltage = refs::positive(self.voltage.unwrap_or(127.0), "voltage")?;
        let system = MepSystem { name, code, kind, color: self.color, panel, voltage };
        let id = edit(s, t!("tx-create-system"), |e| {
            let id = SystemId(e.alloc_id());
            e.systems_mut().insert(id, system);
            Ok(id)
        })?;
        Ok(system_out(s.project(), id))
    }
}

/// A distribution board: a component whose type is a panel device.
fn check_panel(p: &Project, id: ElementId) -> Result<()> {
    match p.element(id).and_then(|e| p.mep_data(e)).and_then(|m| m.device()) {
        Some(cc_core::DeviceKind::Panel) => Ok(()),
        _ => Err(EngineError::invalid(t!("msg-not-a-panel", "id" => id.0))),
    }
}

/// Change a system or circuit: `params` is a merge patch over the values
/// `systems_list` shows (`name`, `code`, `kind`, `color`, `panel`, `voltage`).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct SystemSet {
    /// System id, name or code.
    pub system: Ref,
    /// Values to change.
    pub params: Value,
}

impl Command for SystemSet {
    const NAME: &'static str = "system_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::system(p, &self.system)?;
        let Some(current) = p.system(id).cloned() else {
            return Err(EngineError::not_found(format!("no system with id {}", id.0)));
        };
        let mut patch = self.params.clone();
        // Kinds are given by name, as in `system_create`.
        if let Some(kind) = patch.get("kind").and_then(Value::as_str) {
            let kind = parse_kind(kind)?;
            if kind.is_electrical() != current.kind.is_electrical() && p.system_members(id).next().is_some() {
                return Err(EngineError::rejected(t!("msg-system-kind-in-use")));
            }
            let kind = serde_json::to_value(kind).map_err(|e| EngineError::invalid(e.to_string()))?;
            if let Some(o) = patch.as_object_mut() {
                o.insert("kind".into(), kind);
            }
        }
        if let Some(panel) = patch.get("panel").and_then(Value::as_u64) {
            check_panel(p, refs::element(p, panel)?)?;
        }
        let updated: MepSystem = patch_value(&current, &patch)?;
        if updated.name.trim().is_empty() {
            return Err(EngineError::invalid("the name is empty"));
        }
        if !(updated.voltage.is_finite() && updated.voltage > 0.0) {
            return Err(EngineError::invalid("voltage must be positive"));
        }
        edit(s, t!("tx-modify-system"), |e| {
            e.systems_mut().insert(id, updated);
            Ok(())
        })?;
        Ok(system_out(s.project(), id))
    }
}

/// Delete a system or circuit; its pipes, conduits and devices stay, outside any system.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct SystemDelete {
    /// System id, name or code.
    pub system: Ref,
}

impl Command for SystemDelete {
    const NAME: &'static str = "system_delete";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::system(p, &self.system)?;
        let members: Vec<ElementId> = p.system_members(id).map(|e| e.id).collect();
        edit(s, t!("tx-delete-system"), |e| {
            for m in &members {
                if let Some(el) = e.element_mut(*m) {
                    detach(el, id);
                }
            }
            e.systems_mut().remove(&id);
            Ok(())
        })?;
        Ok(json!({"deleted": id.0, "detached": members.len()}))
    }
}

fn detach(e: &mut Element, id: SystemId) {
    match &mut e.kind {
        ElementKind::Pipe(seg) | ElementKind::Conduit(seg) => seg.systems.retain(|s| *s != id),
        ElementKind::Component(c) if c.system == Some(id) => c.system = None,
        _ => {}
    }
}

/// Put elements in a system or circuit, or take them out (`remove`). A pipe
/// belongs to one system (it moves); a conduit carries several circuits (one
/// is added); a fixture or device connects to one.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct SystemAssign {
    /// Pipe, conduit and component ids.
    pub elements: Vec<u64>,
    /// System id, name or code.
    pub system: Ref,
    /// Take them out instead.
    #[serde(default)]
    pub remove: bool,
}

impl Command for SystemAssign {
    const NAME: &'static str = "system_assign";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::system(p, &self.system)?;
        let ids = refs::elements(p, &self.elements)?;
        for el in ids.iter().filter_map(|i| p.element(*i)) {
            check_member(p, el, id)?;
        }
        let remove = self.remove;
        edit(s, t!("tx-assign-system"), |e| {
            for i in &ids {
                let Some(el) = e.element_mut(*i) else { continue };
                if remove {
                    detach(el, id);
                    continue;
                }
                match &mut el.kind {
                    ElementKind::Pipe(seg) => seg.systems = vec![id],
                    ElementKind::Conduit(seg) if !seg.systems.contains(&id) => seg.systems.push(id),
                    ElementKind::Component(c) => c.system = Some(id),
                    _ => {}
                }
            }
            Ok(())
        })?;
        Ok(json!({"system": system_out(s.project(), id), "elements": self.elements}))
    }
}

/// The load schedule of the circuits (of one board, or all): per circuit its
/// devices, installed load (VA) and current at its voltage, with the total
/// per board.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct CircuitLoads {
    /// Distribution board element id (default: every circuit).
    #[serde(default)]
    pub panel: Option<u64>,
}

impl Command for CircuitLoads {
    const NAME: &'static str = "circuit_loads";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let panel = self.panel.map(|id| refs::element(p, id)).transpose()?;
        let mut circuits: Vec<(&SystemId, &MepSystem)> =
            p.systems.iter().filter(|(_, c)| c.kind.is_electrical() && (panel.is_none() || c.panel == panel)).collect();
        circuits.sort_by(|a, b| natural(&a.1.code, &b.1.code));
        let mut boards: std::collections::BTreeMap<Option<u64>, f64> = Default::default();
        let rows: Vec<Value> = circuits
            .into_iter()
            .map(|(id, c)| {
                let devices: Vec<&Element> =
                    p.system_members(*id).filter(|e| p.mep_data(e).and_then(|m| m.device()).is_some()).collect();
                let power: f64 = devices.iter().filter_map(|e| p.mep_data(e)).map(|m| m.power()).sum();
                *boards.entry(c.panel.map(|e| e.0)).or_default() += power;
                json!({
                    "id": id.0,
                    "code": c.code,
                    "name": c.name,
                    "kind": kind_name(c.kind),
                    "panel": c.panel.map(|e| e.0),
                    "voltage": c.voltage,
                    "devices": devices.len(),
                    "power_va": power,
                    "current_a": if c.voltage > 0.0 { power / c.voltage } else { 0.0 },
                })
            })
            .collect();
        let boards: Vec<Value> =
            boards.into_iter().map(|(panel, power)| json!({"panel": panel, "power_va": power})).collect();
        Ok(json!({"circuits": rows, "panels": boards}))
    }
}

/// The fittings where pipes and conduits meet, counted by category, kind,
/// angle (elbows), diameters and system: the fittings takeoff.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct MepFittings {
    /// Level id or name (default: every level).
    #[serde(default)]
    pub level: Option<Ref>,
}

impl Command for MepFittings {
    const NAME: &'static str = "mep_fittings";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let level = self.level.as_ref().map(|r| refs::level(s.project(), r)).transpose()?;
        let fittings = s.geometry().mep.fittings.clone();
        let p = s.project();
        let mut counts: std::collections::BTreeMap<String, (Value, usize)> = Default::default();
        for f in fittings.iter().filter(|f| level.is_none_or(|l| f.level == l)) {
            let mm: Vec<u64> = f.diameters().iter().map(|d| (d * 1000.0).round() as u64).collect();
            let system = f.system.and_then(|id| p.system(id)).map(|s| s.label().to_string());
            let kind = f.kind.key().trim_start_matches("fitting-").to_string();
            let item = json!({
                "category": refs::category_name(f.category),
                "kind": kind,
                "angle": f.kind.nominal_angle(),
                "diameters_mm": mm,
                "system": system,
            });
            let entry = counts.entry(item.to_string()).or_insert((item, 0));
            entry.1 += 1;
        }
        let total: usize = counts.values().map(|c| c.1).sum();
        let items: Vec<Value> = counts
            .into_values()
            .map(|(mut item, n)| {
                if let Some(o) = item.as_object_mut() {
                    o.insert("count".into(), json!(n));
                }
                item
            })
            .collect();
        Ok(json!({"fittings": items, "total": total}))
    }
}

/// Connect plumbing fixtures and electrical devices to the nearest pipe or
/// conduit of their level (or riser of another floor passing through it). A fixture gets a branch to the nearest sanitary
/// drain (falling towards it, with a drop from the floor), and to the
/// nearest cold (and, when it takes it, hot) water pipe; a device gets a
/// conduit to the nearest conduit, with a drop from its mounting height,
/// carrying its circuit. A component with connectors gets a branch from
/// each connector to the nearest run of a system of the connector's kind,
/// from the connector's position and of its diameter (leaving vertically or
/// horizontally as the connector points). Branches end on the runs, which
/// forms tees. `system` connects only to that system or circuit.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct MepConnect {
    /// Fixture and device ids.
    pub elements: Vec<u64>,
    /// Only to this system or circuit (id, name or code).
    #[serde(default)]
    pub system: Option<Ref>,
}

/// A branch to create: a run on `level` of `type_id`.
struct Branch {
    level: cc_core::LevelId,
    type_id: cc_core::ElementTypeId,
    conduit: bool,
    run: MepSegment,
}

/// The nearest point in plan of run `s` to `p`, and the run's elevation there
/// (on a riser, the height nearest `wanted_z`).
fn nearest_on(s: &MepSegment, p: glam::DVec2, wanted_z: f64) -> (glam::DVec2, f64) {
    if s.is_vertical() {
        let (lo, hi) = (s.start_offset.min(s.end_offset), s.start_offset.max(s.end_offset));
        return (s.start, wanted_z.clamp(lo, hi));
    }
    let d = s.end - s.start;
    let t = ((p - s.start).dot(d) / d.length_squared()).clamp(0.0, 1.0);
    (s.start + d * t, s.start_offset + (s.end_offset - s.start_offset) * t)
}

/// The smallest pipe type at least `diameter` across (else the largest).
fn pipe_type_for(p: &Project, diameter: f64) -> Option<cc_core::ElementTypeId> {
    let mut types: Vec<(f64, cc_core::ElementTypeId)> =
        p.types_of(Category::Pipe).filter_map(|t| Some((t.diameter()?, t.id))).collect();
    types.sort_by(|a, b| a.0.total_cmp(&b.0));
    types.iter().find(|(d, _)| *d >= diameter - 1e-6).or(types.last()).map(|x| x.1)
}

/// The runs seen from `level` as (element, run with elevations above the
/// level), pipes or conduits: its own and the risers of other floors through it.
fn runs_on(p: &Project, level: cc_core::LevelId, conduit: bool) -> Vec<(&Element, MepSegment)> {
    p.segments_at(level).filter(|(r, _)| matches!(r.kind, ElementKind::Conduit(_)) == conduit).collect()
}

/// The run nearest `p` in plan among `runs`, with the point and elevation there.
fn nearest_run<'a>(
    runs: impl Iterator<Item = (&'a Element, &'a MepSegment)>,
    p: glam::DVec2,
    z: f64,
) -> Option<(&'a Element, &'a MepSegment, glam::DVec2, f64)> {
    runs.map(|(r, seg)| {
        let (q, zq) = nearest_on(seg, p, z);
        (r, seg, q, zq)
    })
    .min_by(|a, b| a.2.distance(p).total_cmp(&b.2.distance(p)))
}

impl Command for MepConnect {
    const NAME: &'static str = "mep_connect";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let only = self.system.as_ref().map(|r| refs::system(p, r)).transpose()?;
        let mut branches: Vec<Branch> = Vec::new();
        let mut skipped = 0;
        for id in refs::elements(p, &self.elements)? {
            let Some(e) = p.element(id) else { continue };
            let connectors = cc_geom::component_connectors(p, e);
            let (ElementKind::Component(c), mep) = (&e.kind, p.mep_data(e)) else {
                return Err(EngineError::invalid(t!("msg-not-fixture", "id" => id.0)));
            };
            let Some(mep) = mep.filter(|_| connectors.is_empty()) else {
                if connectors.is_empty() {
                    return Err(EngineError::invalid(t!("msg-not-fixture", "id" => id.0)));
                }
                let mut connected = false;
                let level_z = p.level_elevation(e.level);
                for k in &connectors {
                    let conduit = k.system.is_electrical();
                    let runs = runs_on(p, e.level, conduit);
                    let fits = |seg: &MepSegment| match only {
                        Some(o) => seg.systems.contains(&o),
                        None if conduit => c.system.is_none_or(|x| seg.systems.contains(&x)),
                        None => seg.system().and_then(|x| p.system(x)).is_some_and(|x| x.kind == k.system),
                    };
                    let (at, zc) = (k.point.truncate(), k.point.z - level_z);
                    let candidates = runs.iter().filter(|(_, seg)| fits(seg)).map(|(r, seg)| (*r, seg));
                    let Some((run_el, seg, q, zq)) = nearest_run(candidates, at, zc) else { continue };
                    let type_id = if conduit {
                        run_el.type_id
                    } else {
                        let need = k.diameter.unwrap_or(if k.system.is_drainage() { 0.04 } else { 0.02 });
                        pipe_type_for(p, need).unwrap_or(run_el.type_id)
                    };
                    let systems = match (conduit, c.system.or(only)) {
                        (true, Some(circuit)) => vec![circuit],
                        _ => seg.systems.clone(),
                    };
                    let dist = q.distance(at);
                    let piece = |a: glam::DVec2, b: glam::DVec2, z0: f64, z1: f64| {
                        let mut run = MepSegment::new(a, b, z0);
                        run.end_offset = z1;
                        run.systems = systems.clone();
                        Branch { level: e.level, type_id, conduit, run }
                    };
                    if k.system.is_drainage() {
                        // Down from the outlet, then falling towards the drain.
                        let diameter = p.element_type(type_id).and_then(|t| t.diameter()).unwrap_or(0.1);
                        let zp = zq + crate::defaults::drain_slope(diameter) * dist;
                        if zc - zp > 0.01 {
                            branches.push(piece(at, at, zc, zp));
                        }
                        if dist > 0.01 {
                            branches.push(piece(at, q, zp, zq));
                        }
                    } else if k.vertical {
                        // Straight up or down to the run's height, then across to it.
                        if (zc - zq).abs() > 0.01 {
                            branches.push(piece(at, at, zc, zq));
                        }
                        if dist > 0.01 {
                            branches.push(piece(at, q, zq, zq));
                        }
                    } else {
                        // Across at the connector's height, then up or down onto the run.
                        if dist > 0.01 {
                            branches.push(piece(at, q, zc, zc));
                        }
                        if (zc - zq).abs() > 0.01 {
                            branches.push(piece(q, q, zc, zq));
                        }
                    }
                    connected = true;
                }
                if !connected {
                    skipped += 1;
                }
                continue;
            };
            let base = c.base_offset;
            let mut connected = false;
            match mep {
                MepData::Fixture { drain_units, supply_weight, hot_water } => {
                    let mut kinds = vec![SystemKind::Sanitary];
                    if supply_weight > 0.0 {
                        kinds.push(SystemKind::ColdWater);
                    }
                    if hot_water {
                        kinds.push(SystemKind::HotWater);
                    }
                    let pipes = runs_on(p, e.level, false);
                    for kind in kinds {
                        let fits = |seg: &MepSegment| match only {
                            Some(o) => seg.system() == Some(o),
                            None => seg.system().and_then(|x| p.system(x)).is_some_and(|x| x.kind == kind),
                        };
                        let candidates = pipes.iter().filter(|(_, seg)| fits(seg)).map(|(r, seg)| (*r, seg));
                        let Some((run_el, seg, q, zq)) = nearest_run(candidates, c.position, base) else { continue };
                        let drain = kind.is_drainage();
                        // Fixture branches as in common drainage practice: 100 mm for toilets.
                        let need = match (drain, drain_units) {
                            (false, _) => 0.02,
                            (true, u) if u >= 6.0 => 0.1,
                            (true, u) if u >= 3.0 => 0.05,
                            (true, _) => 0.04,
                        };
                        let type_id = pipe_type_for(p, need).unwrap_or(run_el.type_id);
                        let diameter = p.element_type(type_id).and_then(|t| t.diameter()).unwrap_or(need);
                        let dist = q.distance(c.position);
                        let zp = if drain { zq + crate::defaults::drain_slope(diameter) * dist } else { zq };
                        let systems = seg.systems.clone();
                        if drain && base - zp > 0.01 {
                            let mut drop = MepSegment::new(c.position, c.position, base);
                            drop.end_offset = zp;
                            drop.systems = systems.clone();
                            branches.push(Branch { level: e.level, type_id, conduit: false, run: drop });
                        }
                        if dist > 0.01 {
                            let mut run = MepSegment::new(c.position, q, zp);
                            run.end_offset = zq;
                            run.systems = systems;
                            branches.push(Branch { level: e.level, type_id, conduit: false, run });
                        }
                        connected = true;
                        if only.is_some() {
                            break;
                        }
                    }
                }
                MepData::Device { .. } => {
                    let conduits = runs_on(p, e.level, true);
                    let candidates = conduits
                        .iter()
                        .filter(|(_, seg)| only.is_none_or(|o| seg.systems.contains(&o)))
                        .map(|(r, seg)| (*r, seg));
                    if let Some((run_el, seg, q, zq)) = nearest_run(candidates, c.position, base) {
                        let systems = match c.system.or(only) {
                            Some(circuit) => vec![circuit],
                            None => seg.systems.clone(),
                        };
                        if (base - zq).abs() > 0.01 {
                            let mut drop = MepSegment::new(c.position, c.position, base);
                            drop.end_offset = zq;
                            drop.systems = systems.clone();
                            branches.push(Branch { level: e.level, type_id: run_el.type_id, conduit: true, run: drop });
                        }
                        if q.distance(c.position) > 0.01 {
                            let mut run = MepSegment::new(c.position, q, zq);
                            run.systems = systems;
                            branches.push(Branch { level: e.level, type_id: run_el.type_id, conduit: true, run });
                        }
                        connected = true;
                    }
                }
            }
            if !connected {
                skipped += 1;
            }
        }
        if branches.is_empty() {
            return Ok(json!({"created": [], "skipped": skipped}));
        }
        let created = edit(s, t!("tx-mep-connect"), |e| {
            Ok(branches
                .into_iter()
                .map(|b| {
                    let kind = if b.conduit { ElementKind::Conduit(b.run) } else { ElementKind::Pipe(b.run) };
                    add_element(e, b.level, b.type_id, kind)
                })
                .collect::<Vec<_>>())
        })?;
        Ok(json!({"created": super::ids_json(&created), "skipped": skipped}))
    }
}

/// Size the building services from their networks (simplified tables of
/// common Brazilian practice): drains by the fixture units upstream, water
/// pipes by the flow from the fixtures' weights (at most 3 m/s), circuits
/// by their current (breaker, copper conductor) and the voltage drop to the
/// farthest device (at most 4 %). Diameters in mm, lengths in m.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct MepSizing {}

impl Command for MepSizing {
    const NAME: &'static str = "mep_sizing";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (p, geom) = s.project_and_geometry();
        let code = |id: SystemId| p.system(id).map_or(String::new(), |x| x.label().to_string());
        let pipes: Vec<Value> = cc_geom::sizing::size_pipes(p, geom)
            .into_iter()
            .map(|x| {
                json!({
                    "id": x.run.0,
                    "system": code(x.system),
                    "kind": kind_name(x.kind),
                    "load": x.load,
                    "flow_l_s": x.flow,
                    "speed_m_s": x.speed,
                    "diameter_mm": (x.diameter * 1000.0).round(),
                    "required_mm": (x.required * 1000.0).round(),
                    "ok": x.ok(),
                })
            })
            .collect();
        let circuits: Vec<Value> = cc_geom::sizing::size_circuits(p, geom)
            .into_iter()
            .map(|x| {
                json!({
                    "id": x.system.0,
                    "code": code(x.system),
                    "power_va": x.power,
                    "voltage": x.voltage,
                    "current_a": x.current,
                    "breaker_a": x.breaker,
                    "conductor_mm2": x.conductor,
                    "length_m": x.length,
                    "voltage_drop_pct": x.drop,
                    "ok": x.ok(),
                })
            })
            .collect();
        Ok(json!({"pipes": pipes, "circuits": circuits}))
    }
}

/// Give each sized pipe the smallest pipe type of the project at least as
/// wide as `mep_sizing` requires. Pipes already wide enough keep their type
/// unless `downsize` is set.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct MepSizeApply {
    /// Also narrow pipes wider than required.
    #[serde(default)]
    pub downsize: bool,
}

impl Command for MepSizeApply {
    const NAME: &'static str = "mep_size_apply";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (p, geom) = s.project_and_geometry();
        let mut changes = Vec::new();
        for x in cc_geom::sizing::size_pipes(p, geom) {
            if x.ok() && !self.downsize {
                continue;
            }
            let Some(type_id) = pipe_type_for(p, x.required) else { continue };
            let wide_enough =
                p.element_type(type_id).and_then(|t| t.diameter()).is_some_and(|d| d >= x.required - 1e-6);
            if wide_enough && p.element(x.run).is_some_and(|e| e.type_id != type_id) {
                changes.push((x.run, type_id));
            }
        }
        if changes.is_empty() {
            return Ok(json!({"changed": []}));
        }
        let ids: Vec<ElementId> = changes.iter().map(|c| c.0).collect();
        edit(s, t!("tx-mep-size"), |e| {
            for (id, type_id) in changes {
                if let Some(el) = e.element_mut(id) {
                    el.type_id = type_id;
                }
            }
            Ok(())
        })?;
        Ok(json!({"changed": super::ids_json(&ids)}))
    }
}

/// Numbers numerically, then text.
fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
        (Ok(x), Ok(y)) => x.total_cmp(&y),
        (Ok(_), Err(_)) => std::cmp::Ordering::Less,
        (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
        _ => a.cmp(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip_by_name() {
        for k in SystemKind::ALL {
            assert_eq!(parse_kind(&kind_name(k)).ok(), Some(k));
        }
        assert_eq!(kind_name(SystemKind::ColdWater), "cold_water");
        assert!(parse_kind("steam").is_err());
    }
}
