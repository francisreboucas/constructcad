//! Schedules: one row per element of a category, one column per field of the
//! schedule definition, with filtering, sorting and totals.

use crate::format;
use cc_core::{Category, Element, ElementKind, Project, ScheduleDef, ScheduleField, TypeData};
use cc_geom::{ModelGeometry, polygon};
use std::cmp::Ordering;

/// A cell: plain text, or a translation key for the application to resolve.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Text(String),
    Key(&'static str),
}

impl Cell {
    /// The text, or the key itself for untranslated cells.
    pub fn raw(&self) -> &str {
        match self {
            Cell::Text(s) => s,
            Cell::Key(k) => k,
        }
    }
}

/// A computed schedule. Headers are translation keys.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub headers: Vec<&'static str>,
    pub rows: Vec<Row>,
    /// Totals row aligned with `headers` (the first cell holds the row count);
    /// empty when totals are hidden.
    pub totals: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub element: cc_core::ElementId,
    pub cells: Vec<Cell>,
}

/// A field value before formatting, used for sorting and totals.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Text(String),
    Key(&'static str),
    Count(usize),
    Length(f64),
    Area(f64),
    Volume(f64),
    Money(f64),
    /// Kilograms.
    Weight(f64),
    /// Volt-amperes.
    Power(f64),
    /// A plain quantity (fixture units).
    Number(f64),
    Missing,
}

impl Value {
    fn cell(&self) -> Cell {
        match self {
            Value::Text(s) => Cell::Text(s.clone()),
            Value::Key(k) => Cell::Key(k),
            Value::Count(n) => Cell::Text(n.to_string()),
            Value::Length(v) => Cell::Text(format::meters(*v)),
            Value::Area(v) => Cell::Text(format::area(*v)),
            Value::Money(v) => Cell::Text(format::money(*v)),
            Value::Volume(v) => Cell::Text(format::volume(*v)),
            Value::Weight(v) => Cell::Text(format!("{} kg", format::number(*v, 2))),
            Value::Power(v) => Cell::Text(format!("{} VA", format::number(*v, 0))),
            Value::Number(v) => Cell::Text(format::number(*v, 1)),
            Value::Missing => Cell::Text(String::new()),
        }
    }

    fn number(&self) -> Option<f64> {
        match self {
            Value::Count(n) => Some(*n as f64),
            Value::Length(v)
            | Value::Area(v)
            | Value::Volume(v)
            | Value::Money(v)
            | Value::Weight(v)
            | Value::Power(v)
            | Value::Number(v) => Some(*v),
            _ => None,
        }
    }
}

/// Natural order: numbers numerically (so "10" follows "9"), then text.
fn natural(a: &str, b: &str) -> Ordering {
    match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
        (Ok(x), Ok(y)) => x.total_cmp(&y),
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Ok(_)) => Ordering::Greater,
        _ => a.cmp(b),
    }
}

fn compare(a: &Value, b: &Value) -> Ordering {
    match (a.number(), b.number()) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        _ => natural(&text_of(a), &text_of(b)),
    }
}

fn text_of(v: &Value) -> String {
    v.cell().raw().to_string()
}

/// Whether a field's values add up in the totals row.
fn summable(field: ScheduleField) -> bool {
    matches!(
        field,
        ScheduleField::Length
            | ScheduleField::CutLength
            | ScheduleField::BarCount
            | ScheduleField::Weight
            | ScheduleField::Power
            | ScheduleField::DrainUnits
            | ScheduleField::Area
            | ScheduleField::Volume
            | ScheduleField::Cost
    )
}

/// Net side area of a wall: length x height minus its openings.
fn wall_area(project: &Project, e: &Element, w: &cc_core::Wall) -> f64 {
    let openings: f64 = project
        .hosted_by(e.id)
        .filter_map(|h| project.element_type(h.type_id)?.opening_size())
        .map(|(w, h)| w * h)
        .sum();
    (project.wall_length(e, w) * project.wall_height(e, w) - openings).max(0.0)
}

/// A section as text: its size, with the profile's letter (I, U, L), a box
/// for tubes and Ø for round sections.
fn section_text(section: cc_core::ColumnSection) -> String {
    use cc_core::{ColumnSection as C, SectionShape as S};
    let l = format::length;
    match section {
        C::Rectangular { width, depth } | C::Profile { width, depth, shape: S::Solid } => {
            format!("{} × {}", l(width), l(depth))
        }
        C::Round { diameter } => format!("Ø {}", l(diameter)),
        C::Pipe { diameter, wall } => format!("Ø {} × {}", l(diameter), l(wall)),
        C::Profile { width, depth, shape } => {
            let shape = shape.fitted(width, depth);
            let (letter, thickness) = match shape {
                S::I { web, flange } => ("I", format!("{} / {}", l(web), l(flange))),
                S::Channel { web, flange } => ("U", format!("{} / {}", l(web), l(flange))),
                S::Angle { thickness } => ("L", l(thickness)),
                S::Tube { wall } => ("□", l(wall)),
                S::Solid => ("", String::new()),
            };
            format!("{letter} {} × {} × {thickness}", l(width), l(depth))
        }
    }
}

/// Width (X), depth (Y) and height of a component's pieces in its own frame.
fn component_extents(project: &Project, e: &Element) -> Option<(f64, f64, f64)> {
    let ElementKind::Component(c) = &e.kind else { return None };
    let Some(TypeData::Component { def, .. }) = project.element_type(e.type_id).map(|t| &t.data) else { return None };
    let (parts, _) = def.resolve(&c.overrides);
    let mut lo = glam::DVec3::splat(f64::INFINITY);
    let mut hi = glam::DVec3::splat(f64::NEG_INFINITY);
    for p in parts.iter().filter(|p| !p.void && !p.plan_only) {
        for q in &p.footprint {
            lo = lo.min(q.extend(p.z0));
            hi = hi.max(q.extend(p.z1));
        }
    }
    let size = hi - lo;
    size.is_finite().then_some((size.x, size.y, size.z))
}

fn value(project: &Project, geom: &ModelGeometry, e: &Element, field: ScheduleField) -> Value {
    use ScheduleField as F;
    let ty = project.element_type(e.type_id);
    let info = ty.map(|t| &t.info);
    let text = |s: &str| if s.is_empty() { Value::Missing } else { Value::Text(s.to_string()) };
    let room_boundary = geom.rooms.get(&e.id);
    match field {
        F::Mark => text(&e.mark),
        F::Type => text(project.type_name(e.type_id)),
        F::Level => text(project.level_name(e.level)),
        F::Comments => text(&e.comments),
        F::PhaseCreated if e.category().is_phased() => {
            project.phases.get(project.created_index(e)).map_or(Value::Missing, |ph| text(&ph.name))
        }
        F::PhaseDemolished => {
            e.phase_demolished.and_then(|d| project.phase(d)).map_or(Value::Missing, |ph| text(&ph.name))
        }
        F::PhaseCreated => Value::Missing,
        F::TypeMark => info.map_or(Value::Missing, |i| text(&i.type_mark)),
        F::Description => info.map_or(Value::Missing, |i| text(&i.description)),
        F::Manufacturer => info.map_or(Value::Missing, |i| text(&i.manufacturer)),
        F::Model => info.map_or(Value::Missing, |i| text(&i.model)),
        F::FireRating => info.map_or(Value::Missing, |i| text(&i.fire_rating)),
        F::Cost => info.and_then(|i| i.cost).map_or(Value::Missing, Value::Money),
        F::Length => match &e.kind {
            ElementKind::Wall(w) => Value::Length(project.wall_length(e, w)),
            ElementKind::Beam(b) => Value::Length(b.true_length()),
            ElementKind::Rebar(_) => geom.rebar.get(&e.id).map_or(Value::Missing, |r| Value::Length(r.total_length())),
            ElementKind::Foundation(f) => match &f.shape {
                cc_core::FoundationShape::Strip { start, end } => Value::Length(start.distance(*end)),
                _ => Value::Missing,
            },
            ElementKind::Railing(r) => Value::Length(cc_geom::railing_length(project, e, r)),
            ElementKind::Pipe(s) | ElementKind::Conduit(s) => Value::Length(s.length()),
            ElementKind::Sweep(sw) => Value::Length(sw.length()),
            _ => Value::Missing,
        },
        F::Height => match (&e.kind, ty.map(|t| &t.data)) {
            (ElementKind::Wall(w), _) => Value::Length(project.wall_height(e, w)),
            (ElementKind::Column(c), _) => {
                let (base, top) = project.column_span(e, c);
                Value::Length(top - base)
            }
            (ElementKind::Room(r), _) => Value::Length(r.height),
            (ElementKind::Mass(m), _) => Value::Length(m.height),
            (ElementKind::Door(_) | ElementKind::Window(_), _) => {
                ty.and_then(|t| t.opening_size()).map_or(Value::Missing, |(_, h)| Value::Length(h))
            }
            (_, Some(TypeData::Railing { height, .. })) => Value::Length(*height),
            (ElementKind::Component(_), _) => {
                component_extents(project, e).map_or(Value::Missing, |s| Value::Length(s.2))
            }
            _ => Value::Missing,
        },
        F::Width => match ty.map(|t| &t.data) {
            _ if matches!(e.kind, ElementKind::Door(_) | ElementKind::Window(_)) => {
                ty.and_then(|t| t.opening_size()).map_or(Value::Missing, |(w, _)| Value::Length(w))
            }
            Some(TypeData::Stair { width, .. } | TypeData::Ramp { width, .. }) => Value::Length(*width),
            _ => component_extents(project, e).map_or(Value::Missing, |s| Value::Length(s.0)),
        },
        F::Depth => component_extents(project, e).map_or(Value::Missing, |s| Value::Length(s.1)),
        F::ComponentName => match ty.map(|t| &t.data) {
            Some(TypeData::Component { component, .. }) => text(component),
            _ => Value::Missing,
        },
        // Doors and windows have their category instead.
        F::ComponentKind => match ty.map(|t| &t.data) {
            Some(TypeData::Component { def, .. }) if e.category() == cc_core::Category::Component => {
                Value::Key(def.kind.key())
            }
            _ => Value::Missing,
        },
        F::Thickness => match ty {
            Some(t) if t.thickness() > 0.0 => Value::Length(t.thickness()),
            _ => Value::Missing,
        },
        F::Area => match &e.kind {
            ElementKind::Wall(w) => Value::Area(wall_area(project, e, w)),
            ElementKind::Room(_) => room_boundary.map_or(Value::Missing, |b| Value::Area(polygon::area(b))),
            ElementKind::Foundation(_) => {
                geom.foundations.get(&e.id).map_or(Value::Missing, |g| Value::Area(polygon::area(&g.footprint)))
            }
            ElementKind::Roof(r) => Value::Area(r.area()),
            ElementKind::Floor(f) => Value::Area(f.area()),
            _ => e.sketch().map_or(Value::Missing, |s| Value::Area(polygon::area(s))),
        },
        F::Perimeter => match &e.kind {
            ElementKind::Room(_) => room_boundary.map_or(Value::Missing, |b| Value::Length(polygon::perimeter(b))),
            _ => e.sketch().map_or(Value::Missing, |s| Value::Length(polygon::perimeter(s))),
        },
        F::Volume => match &e.kind {
            ElementKind::Room(r) => {
                room_boundary.map_or(Value::Missing, |b| Value::Volume(polygon::area(b) * r.height))
            }
            _ => Value::Volume(geom.volume(e.id)),
        },
        F::Sill => e.as_hosted().map_or(Value::Missing, |h| Value::Length(h.sill_height)),
        F::Host => e
            .as_hosted()
            .map(|h| h.host)
            .or(match &e.kind {
                ElementKind::Rebar(r) => Some(r.host),
                _ => None,
            })
            .and_then(|h| project.element(h))
            .map_or(Value::Missing, |w| text(&if w.mark.is_empty() { w.id.to_string() } else { w.mark.clone() })),
        F::Function => ty.and_then(|t| t.wall_function()).map_or(Value::Missing, |f| Value::Key(f.key())),
        F::Structural => match &e.kind {
            ElementKind::Wall(w) => Value::Key(if w.structural { "value-yes" } else { "value-no" }),
            ElementKind::Column(c) => Value::Key(if c.structural { "value-yes" } else { "value-no" }),
            _ => Value::Missing,
        },
        F::Section => match ty.map(|t| &t.data) {
            Some(TypeData::Column { section, .. }) => Value::Text(section_text(*section)),
            Some(TypeData::Beam { width, height, shape, .. }) => {
                let section = cc_core::ColumnSection::Profile { width: *width, depth: *height, shape: *shape };
                Value::Text(section_text(section))
            }
            Some(TypeData::Foundation { kind: cc_core::FoundationKind::Pad, width, length, .. }) => {
                Value::Text(format!("{} × {}", format::length(*width), format::length(*length)))
            }
            Some(TypeData::Foundation { kind: cc_core::FoundationKind::Strip, width, .. }) => {
                Value::Text(format::length(*width))
            }
            _ => Value::Missing,
        },
        F::CutLength => geom.beams.get(&e.id).map_or(Value::Missing, |b| Value::Length(b.cut_length())),
        F::Material => ty.map_or(Value::Missing, |t| text(t.main_material())),
        F::Diameter => match ty.map(|t| &t.data) {
            Some(
                TypeData::Rebar { diameter, .. } | TypeData::Pipe { diameter, .. } | TypeData::Conduit { diameter, .. },
            ) if format::imperial() => Value::Text(format!("Ø {}\"", format::inches(*diameter))),
            Some(TypeData::Rebar { diameter, .. }) => {
                Value::Text(format!("Ø {} mm", format::number(diameter * 1000.0, 1)))
            }
            Some(TypeData::Pipe { diameter, .. } | TypeData::Conduit { diameter, .. }) => {
                Value::Text(format!("Ø {} mm", format::number(diameter * 1000.0, 0)))
            }
            _ => Value::Missing,
        },
        F::System => {
            let labels: Vec<&str> =
                e.systems().iter().filter_map(|id| project.system(*id)).map(|s| s.label()).collect();
            if labels.is_empty() { Value::Missing } else { Value::Text(labels.join(", ")) }
        }
        F::Slope => match &e.kind {
            ElementKind::Pipe(s) if !s.is_vertical() => Value::Text(format::slope(s.slope())),
            ElementKind::Roof(_) => geom
                .roof_framing
                .get(&e.id)
                .and_then(|g| g.slopes)
                .map_or(Value::Missing, |(lo, _)| Value::Text(format::slope(lo))),
            ElementKind::Ramp(r) => {
                cc_geom::ramp_geom(project, e, r).map_or(Value::Missing, |g| Value::Text(format::slope(g.check.slope)))
            }
            _ => Value::Missing,
        },
        F::Power => match project.mep_data(e) {
            Some(m @ cc_core::MepData::Device { .. }) => Value::Power(m.power()),
            _ => Value::Missing,
        },
        F::DrainUnits => match project.mep_data(e) {
            Some(cc_core::MepData::Fixture { drain_units, .. }) => Value::Number(drain_units),
            _ => Value::Missing,
        },
        F::BarCount => geom.rebar.get(&e.id).map_or(Value::Missing, |r| Value::Count(r.count())),
        F::FloorArea => geom.masses.get(&e.id).map_or(Value::Missing, |m| Value::Area(m.floor_area())),
        F::FloorCount => geom.masses.get(&e.id).map_or(Value::Missing, |m| Value::Count(m.floors.len())),
        F::BarLength => geom.rebar.get(&e.id).map_or(Value::Missing, |r| Value::Length(r.bar_length())),
        F::Weight => match (geom.rebar.get(&e.id), roof_covering(project, geom, e)) {
            (Some(r), _) => Value::Weight(r.weight()),
            (None, Some((spec, g))) => Value::Weight(spec.weight_on(g.sloped_area)),
            _ => Value::Missing,
        },
        F::TileCount => roof_covering(project, geom, e)
            .and_then(|(spec, g)| spec.tiles(g.sloped_area).map(|n| Value::Count(n as usize)))
            .unwrap_or(Value::Missing),
        F::FramingLength => geom
            .roof_framing
            .get(&e.id)
            .filter(|g| !g.members.is_empty())
            .map_or(Value::Missing, |g| Value::Length(g.total_length())),
        F::FramingVolume => geom
            .roof_framing
            .get(&e.id)
            .filter(|g| !g.members.is_empty())
            .map_or(Value::Missing, |g| Value::Volume(g.volume())),
        F::Number => match &e.kind {
            ElementKind::Room(r) => text(&r.number),
            _ => Value::Missing,
        },
        F::Name => match &e.kind {
            ElementKind::Room(r) => text(&r.name),
            _ => Value::Missing,
        },
        F::Risers | F::RiserHeight | F::Run => match &e.kind {
            ElementKind::Stair(s) => match cc_geom::stair_geom(project, e, s) {
                Some(g) if field == F::Risers => Value::Count(g.risers),
                Some(g) if field == F::RiserHeight => Value::Length(g.riser),
                Some(g) => Value::Length(g.run()),
                None => Value::Missing,
            },
            ElementKind::Ramp(r) if field == F::Run => {
                cc_geom::ramp_geom(project, e, r).map_or(Value::Missing, |g| Value::Length(g.run()))
            }
            _ => Value::Missing,
        },
    }
}

/// A field's value for view filters: its text (keys reduced to their last
/// word, e.g. `yes`, `exterior`) and, for quantities, its number in SI
/// units (meters, square and cubic meters; yes/no as 1/0; diameters in
/// meters). `None` when the element has no value.
pub fn field_value(
    project: &Project,
    geom: &ModelGeometry,
    e: &Element,
    field: ScheduleField,
) -> Option<(String, Option<f64>)> {
    let v = value(project, geom, e, field);
    match v {
        Value::Missing => None,
        Value::Key(k) => {
            let word = k.rsplit('-').next().unwrap_or(k).to_string();
            let number = match word.as_str() {
                "yes" => Some(1.0),
                "no" => Some(0.0),
                _ => None,
            };
            Some((word, number))
        }
        Value::Text(t) => {
            let number = match (field, project.element_type(e.type_id).map(|t| &t.data)) {
                (
                    ScheduleField::Diameter,
                    Some(
                        TypeData::Rebar { diameter, .. }
                        | TypeData::Pipe { diameter, .. }
                        | TypeData::Conduit { diameter, .. },
                    ),
                ) => Some(*diameter),
                _ => t.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite()),
            };
            Some((t, number))
        }
        other => Some((text_of(&other), other.number())),
    }
}

/// Default row order: by mark (rooms by number), naturally, then by id.
fn default_order(a: &Element, b: &Element) -> Ordering {
    let key = |e: &Element| match &e.kind {
        ElementKind::Room(r) => r.number.clone(),
        _ => e.mark.clone(),
    };
    natural(&key(a), &key(b)).then(a.id.cmp(&b.id))
}

pub fn schedule(project: &Project, geom: &ModelGeometry, category: Category, def: &ScheduleDef) -> Table {
    schedule_where(project, geom, category, def, &mut |_| true)
}

/// A schedule of the elements `keep` accepts (those a schedule view's
/// phase and phase filter show, see [`crate::graphics::PhaseView`]).
pub fn schedule_where(
    project: &Project,
    geom: &ModelGeometry,
    category: Category,
    def: &ScheduleDef,
    keep: &mut dyn FnMut(&Element) -> bool,
) -> Table {
    let fields = def.fields_for(category);
    let mut elements: Vec<&Element> = project.elements_of(category).filter(|e| keep(e)).collect();
    elements.sort_by(|a, b| default_order(a, b));

    let mut rows: Vec<(&Element, Vec<Value>)> =
        elements.into_iter().map(|e| (e, fields.iter().map(|&f| value(project, geom, e, f)).collect())).collect();

    if let Some((field, needle)) = &def.filter {
        let needle = needle.trim().to_lowercase();
        if !needle.is_empty() {
            rows.retain(|(e, _)| text_of(&value(project, geom, e, *field)).to_lowercase().contains(&needle));
        }
    }
    if let Some(field) = def.sort_by {
        // Stable: ties keep the default order.
        rows.sort_by(|(a, _), (b, _)| {
            let order = compare(&value(project, geom, a, field), &value(project, geom, b, field));
            if def.descending { order.reverse() } else { order }
        });
    }

    let totals = if def.hide_totals {
        Vec::new()
    } else {
        let mut totals: Vec<String> = fields
            .iter()
            .enumerate()
            .map(|(i, &f)| {
                if !summable(f) {
                    return String::new();
                }
                let sum: f64 = rows.iter().filter_map(|(_, values)| values.get(i)?.number()).sum();
                match f {
                    ScheduleField::Length | ScheduleField::CutLength => format::meters(sum),
                    ScheduleField::BarCount => format::number(sum, 0),
                    ScheduleField::Weight => format!("{} kg", format::number(sum, 2)),
                    ScheduleField::Volume => format::volume(sum),
                    ScheduleField::Area => format::area(sum),
                    ScheduleField::Cost => format::money(sum),
                    _ => format::number(sum, 2),
                }
            })
            .collect();
        if let Some(first) = totals.first_mut() {
            *first = rows.len().to_string();
        }
        totals
    };

    Table {
        headers: fields.iter().map(|f| f.key()).collect(),
        rows: rows
            .into_iter()
            .map(|(e, values)| Row { element: e.id, cells: values.iter().map(Value::cell).collect() })
            .collect(),
        totals,
    }
}

/// The table as CSV: `;`-separated when the decimal separator is a comma, so
/// spreadsheets in those locales open it directly. `header` and `cell`
/// translate keys.
pub fn to_csv(table: &Table, header: impl Fn(&str) -> String, cell: impl Fn(&Cell) -> String) -> String {
    let sep = if format::number(0.5, 1).contains(',') { ';' } else { ',' };
    let quote = |s: &str| {
        if s.contains(sep) || s.contains('"') || s.contains('\n') {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    let mut out = String::new();
    let mut line = |cells: Vec<String>| {
        out.push_str(&cells.iter().map(|c| quote(c)).collect::<Vec<_>>().join(&sep.to_string()));
        out.push_str("\r\n");
    };
    line(table.headers.iter().map(|h| header(h)).collect());
    for row in &table.rows {
        line(row.cells.iter().map(&cell).collect());
    }
    if !table.totals.is_empty() {
        line(table.totals.clone());
    }
    out
}

/// A roof's covering (from its type) and its framing figures.
fn roof_covering<'a>(
    project: &Project,
    geom: &'a ModelGeometry,
    e: &Element,
) -> Option<(cc_core::roofing::TileSpec, &'a cc_geom::roof_framing::RoofFramingGeom)> {
    let Some(TypeData::Roof { covering: Some(kind), .. }) = project.element_type(e.type_id).map(|t| &t.data) else {
        return None;
    };
    Some((kind.spec(), geom.roof_framing.get(&e.id)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (Project, ModelGeometry) {
        let project = cc_core::from_ron(include_str!("../../../examples/casa_simples.ccad")).unwrap();
        let geom = ModelGeometry::build(&project);
        (project, geom)
    }

    /// Filters refuse rules on fields that apply to none of their categories,
    /// so every value an element has must be on a field that applies to it.
    #[test]
    fn fields_with_values_apply_to_the_category() {
        let mut fields: std::collections::BTreeSet<ScheduleField> =
            Category::ALL.into_iter().flat_map(ScheduleField::available).collect();
        fields.extend([ScheduleField::Type, ScheduleField::Level, ScheduleField::Material, ScheduleField::Volume]);
        let files = [
            include_str!("../../../examples/casa_simples.ccad"),
            include_str!("../../../examples/showcase.ccad"),
            include_str!("../../cc_ifc/tests/fixtures/features.ccad"),
            include_str!("../../cc_ifc/tests/fixtures/structure.ccad"),
            include_str!("../../cc_ifc/tests/fixtures/mep.ccad"),
            include_str!("../../cc_ifc/tests/fixtures/arcs.ccad"),
            include_str!("../../cc_ifc/tests/fixtures/roofs.ccad"),
            include_str!("../../cc_ifc/tests/fixtures/phases.ccad"),
        ];
        for file in files {
            let project = cc_core::from_ron(file).unwrap();
            let geom = ModelGeometry::build(&project);
            for e in project.elements.values() {
                for &field in &fields {
                    if value(&project, &geom, e, field) != Value::Missing {
                        assert!(field.applies_to(e.category()), "{field:?} has a value on {:?}", e.category());
                    }
                }
            }
        }
    }

    #[test]
    fn sorting_filtering_and_totals() {
        let (project, geom) = sample();
        let def = ScheduleDef {
            fields: vec![ScheduleField::Mark, ScheduleField::Width, ScheduleField::Area],
            sort_by: Some(ScheduleField::Mark),
            descending: true,
            ..Default::default()
        };
        let t = schedule(&project, &geom, Category::Window, &def);
        assert_eq!(t.headers, vec!["col-mark", "col-width", "col-area"]);
        assert_eq!(t.rows.len(), 4);
        // Marks 1..4, descending; windows have no area (empty cell).
        let marks: Vec<&str> = t.rows.iter().map(|r| r.cells[0].raw()).collect();
        assert_eq!(marks, ["4", "3", "2", "1"]);
        assert_eq!(t.rows[0].cells[1], Cell::Text(format::meters(1.2)));
        assert_eq!(t.totals[0], "4");

        let filtered = ScheduleDef { filter: Some((ScheduleField::Mark, "3".into())), ..def };
        let t = schedule(&project, &geom, Category::Window, &filtered);
        assert_eq!(t.rows.len(), 1);
        assert_eq!(t.totals[0], "1");
    }

    #[test]
    fn structural_quantities() {
        let (project, geom) = sample();
        // The porch beam spans 5 m between two 0.20 m columns: 4.80 m of body.
        let t = schedule(&project, &geom, Category::Beam, &ScheduleDef::default());
        let col = |t: &Table, key: &str| t.headers.iter().position(|h| *h == key).unwrap();
        assert_eq!(t.rows.len(), 1);
        let row = &t.rows[0].cells;
        assert_eq!(
            row[col(&t, "col-section")],
            Cell::Text(format!("{} × {}", format::length(0.15), format::length(0.4)))
        );
        assert_eq!(row[col(&t, "col-length")], Cell::Text(format::meters(5.0)));
        assert_eq!(row[col(&t, "col-cut-length")], Cell::Text(format::meters(4.8)));
        assert_eq!(row[col(&t, "col-volume")], Cell::Text(format::volume(0.15 * 0.4 * 4.8)));
        assert_ne!(row[col(&t, "col-material")], Cell::Text(String::new()));
        assert_eq!(t.totals[col(&t, "col-cut-length")], format::meters(4.8));
        let t = schedule(&project, &geom, Category::Column, &ScheduleDef::default());
        assert_eq!(t.rows.len(), 2);
        let section = format!("{} × {}", format::length(0.2), format::length(0.2));
        assert!(t.rows.iter().all(|r| r.cells[col(&t, "col-section")] == Cell::Text(section.clone())));
    }

    #[test]
    fn csv_quotes_and_separates() {
        let table = Table {
            headers: vec!["a", "b"],
            rows: vec![Row { element: cc_core::ElementId(1), cells: vec![Cell::Text("x;y".into()), Cell::Key("k")] }],
            totals: vec!["1".into(), String::new()],
        };
        let csv = to_csv(&table, |h| h.to_uppercase(), |c| c.raw().to_string());
        assert_eq!(csv, "A;B\r\n\"x;y\";k\r\n1;\r\n");
    }
}
