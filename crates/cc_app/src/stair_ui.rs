//! Stair and ramp controls: the plan layout (options bar and properties), the
//! construction and step rules of stair types, the rules of ramp types, and their checks.

use crate::i18n::tr;
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row};
use crate::theme::PALETTE;
use cc_core::{
    StairBuild, StairConstruction, StairLayout, StairRules, StairShape, Stringers, TreadRule, Turn,
    stair::{MAX_RISERS, MAX_WINDERS},
};
use egui::{DragValue, Response, RichText, Ui};

/// What a set of controls did this frame.
#[derive(Default)]
pub struct Edited {
    pub changed: bool,
    /// A drag or text edit ended, or a discrete choice was made.
    pub done: bool,
}

impl Edited {
    fn add(&mut self, r: &Response) {
        self.changed |= r.changed();
        self.done |= r.drag_stopped() || r.lost_focus();
    }

    fn choice(&mut self, changed: bool) {
        self.changed |= changed;
        self.done |= changed;
    }
}

/// A labeled control: a table row, or label and control side by side.
fn field<R>(ui: &mut Ui, table: bool, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    if table {
        row(ui, label, add)
    } else {
        ui.label(label);
        add(ui)
    }
}

/// A combo box over `all`, named by `key`; true when the choice changed.
fn combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    value: &mut T,
    all: &[T],
    key: fn(T) -> &'static str,
) -> bool {
    let before = *value;
    egui::ComboBox::from_id_salt(salt).selected_text(tr(key(*value))).show_ui(ui, |ui| {
        for &option in all {
            ui.selectable_value(value, option, tr(key(option)));
        }
    });
    *value != before
}

fn length(ui: &mut Ui, value: &mut f64, max: f64) -> Response {
    ui.add(crate::properties::length_drag(value).speed(0.005).range(0.0..=max))
}

/// A riser count, 0 shown as automatic.
fn count(ui: &mut Ui, value: &mut u32) -> Response {
    ui.add(
        DragValue::new(value)
            .range(0..=MAX_RISERS)
            .custom_formatter(|n, _| if n < 0.5 { t!("stair-auto") } else { format!("{n:.0}") }),
    )
}

/// Shape, turn, riser count, tread and the sizes the shape uses.
pub fn layout_controls(ui: &mut Ui, salt: u64, layout: &mut StairLayout, table: bool) -> Edited {
    let mut e = Edited::default();
    let shape = field(ui, table, &t!("prop-stair-shape"), |ui| {
        combo(ui, ("stair-shape", salt), &mut layout.shape, &StairShape::ALL, StairShape::key)
    });
    e.choice(shape);
    if layout.shape.turns() {
        let turn = field(ui, table, &t!("prop-stair-turn"), |ui| {
            combo(ui, ("stair-turn", salt), &mut layout.turn, &Turn::ALL, Turn::key)
        });
        e.choice(turn);
    }
    let r = field(ui, table, &t!("col-risers"), |ui| count(ui, &mut layout.risers));
    e.add(&r);
    let r = field(ui, table, &t!("col-tread"), |ui| {
        ui.add(
            DragValue::new(&mut layout.tread)
                .speed(0.005)
                .range(0.0..=2.0)
                .custom_formatter(|n, _| if n < 1e-9 { t!("stair-auto") } else { cc_views::format::length(n) })
                .custom_parser(cc_views::format::parse_length),
        )
    });
    e.add(&r);
    if layout.shape.has_split() {
        let r = field(ui, table, &t!("prop-stair-split"), |ui| count(ui, &mut layout.split));
        e.add(&r);
    }
    if layout.shape == StairShape::ThreeFlight {
        let r = field(ui, table, &t!("prop-stair-split2"), |ui| count(ui, &mut layout.split2));
        e.add(&r);
    }
    if matches!(layout.shape, StairShape::StraightLanding | StairShape::UShaped) {
        let r = field(ui, table, &t!("prop-stair-landing"), |ui| length(ui, &mut layout.landing, 20.0));
        e.add(&r);
    }
    if layout.shape == StairShape::UShaped {
        let r = field(ui, table, &t!("prop-stair-gap"), |ui| length(ui, &mut layout.gap, 10.0));
        e.add(&r);
    }
    if layout.shape == StairShape::LShaped {
        let before = layout.winders;
        let r = field(ui, table, &t!("prop-stair-winders"), |ui| {
            ui.add(
                DragValue::new(&mut layout.winders)
                    .range(0..=MAX_WINDERS)
                    .custom_formatter(|n, _| if n < 0.5 { t!("stair-winders-none") } else { format!("{n:.0}") }),
            )
        });
        // One winder is no winder: step over it.
        if layout.winders == 1 {
            layout.winders = if before == 0 { 2 } else { 0 };
        }
        e.add(&r);
    }
    if layout.shape == StairShape::Spiral {
        let r = field(ui, table, &t!("prop-stair-inner-radius"), |ui| length(ui, &mut layout.inner_radius, 10.0));
        e.add(&r);
    }
    e
}

/// Construction of a stair type: solid, cast in place (waist) or assembled
/// (stringers and treads).
pub fn build_rows(ui: &mut Ui, salt: u64, build: &mut StairBuild) -> Edited {
    let mut e = Edited::default();
    let c = row(ui, &t!("prop-stair-construction"), |ui| {
        combo(ui, ("stair-build", salt), &mut build.construction, &StairConstruction::ALL, StairConstruction::key)
    });
    e.choice(c);
    match build.construction {
        StairConstruction::Solid => {}
        StairConstruction::CastInPlace => {
            let r = row(ui, &t!("prop-stair-waist"), |ui| length(ui, &mut build.waist, 1.0));
            e.add(&r);
        }
        StairConstruction::Assembled => {
            let s = row(ui, &t!("prop-stair-stringers"), |ui| {
                combo(ui, ("stair-stringers", salt), &mut build.stringers, &Stringers::ALL, Stringers::key)
            });
            e.choice(s);
            for (label, value) in [
                (t!("prop-stair-stringer-width"), &mut build.stringer_width),
                (t!("prop-stair-stringer-depth"), &mut build.stringer_depth),
                (t!("prop-stair-tread-thickness"), &mut build.tread_thickness),
            ] {
                let r = row(ui, &label, |ui| length(ui, value, 1.0));
                e.add(&r);
            }
            let r = row(ui, &t!("prop-stair-closed-risers"), |ui| ui.checkbox(&mut build.closed_risers, ""));
            e.choice(r.changed());
        }
    }
    e
}

/// Step rules of a stair type: how the tread is set and the comfort limits.
pub fn rules_rows(ui: &mut Ui, salt: u64, rules: &mut StairRules, max_riser: f64, tread: f64) -> Edited {
    let mut e = Edited::default();
    let c = row(ui, &t!("prop-stair-tread-rule"), |ui| {
        combo(ui, ("stair-tread-rule", salt), &mut rules.tread_rule, &TreadRule::ALL, TreadRule::key)
    });
    e.choice(c);
    for (label, value) in [
        (t!("prop-stair-min-riser"), &mut rules.min_riser),
        (t!("prop-stair-min-tread"), &mut rules.min_tread),
        (t!("prop-stair-blondel-min"), &mut rules.blondel_min),
        (t!("prop-stair-blondel-max"), &mut rules.blondel_max),
    ] {
        let r = row(ui, &label, |ui| length(ui, value, 2.0));
        e.add(&r);
    }
    // Blondel's step at the highest riser, as a hint of what the type gives.
    let g = rules.tread_for(max_riser, tread);
    readonly_row(ui, &t!("prop-stair-blondel"), cc_views::format::length(2.0 * max_riser + g));
    e
}

/// Riser, tread, Blondel's step and the broken rules of a laid-out stair.
pub fn check_rows(ui: &mut Ui, g: &cc_geom::StairGeom) {
    use cc_views::format;
    readonly_row(ui, &t!("col-risers"), g.risers.to_string());
    readonly_row(ui, &t!("col-riser-height"), format::length(g.riser));
    readonly_row(ui, &t!("col-tread"), format::length(g.tread));
    readonly_row(ui, &t!("col-run"), format::length(g.run()));
    readonly_row(ui, &t!("prop-stair-blondel"), format::length(g.check.blondel));
    if g.check.ok() {
        ui.label(RichText::new(t!("stair-check-ok")).color(PALETTE.success));
    }
    for issue in &g.check.issues {
        ui.label(RichText::new(cc_engine::commands::stair::issue_text(issue)).color(PALETTE.warning));
    }
}

/// The engine command creating a stair with `layout`.
pub fn create_command(
    layout: StairLayout,
    start: [f64; 2],
    direction: f64,
    level: cc_core::LevelId,
    type_id: cc_core::ElementTypeId,
) -> cc_engine::commands::stair::StairCreate {
    use cc_engine::Ref;
    let shape = cc_engine::commands::stair::SHAPES.iter().find(|(_, s)| *s == layout.shape).map(|(n, _)| n.to_string());
    let turn = match layout.turn {
        Turn::Left => "left",
        Turn::Right => "right",
    };
    cc_engine::commands::stair::StairCreate {
        start,
        direction: Some(direction),
        level: Some(Ref::Id(level.0)),
        type_ref: Some(Ref::Id(type_id.0)),
        shape,
        turn: Some(turn.into()),
        risers: (layout.risers > 0).then_some(layout.risers),
        tread: (layout.tread > 1e-3).then_some(layout.tread),
        split: Some(layout.split),
        split2: Some(layout.split2),
        landing: (layout.landing > 1e-3).then_some(layout.landing),
        gap: Some(layout.gap),
        winders: Some(layout.winders),
        inner_radius: Some(layout.inner_radius),
        ..Default::default()
    }
}

/// Options bar of the stair tool.
pub fn tool_options(ui: &mut Ui, state: &mut AppState) {
    let mut layout = state.stair_layout;
    layout_controls(ui, 0, &mut layout, false);
    state.stair_layout = layout;
    ui.separator();
    ui.label(crate::theme::muted(t!("hint-stair")));
}

/// A slope as a percentage drag (stored as rise over run); `zero` names 0.
fn percent(ui: &mut Ui, value: &mut f64, zero: Option<String>) -> Response {
    let mut pct = *value * 100.0;
    let r = ui.add(DragValue::new(&mut pct).speed(0.05).range(0.0..=100.0).custom_formatter(move |n, _| match &zero {
        Some(z) if n < 1e-6 => z.clone(),
        _ => format!("{n:.2} %"),
    }));
    if r.changed() {
        *value = pct / 100.0;
    }
    r
}

/// Shape, turn, slope and the sizes a ramp's shape uses.
pub fn ramp_layout_controls(ui: &mut Ui, salt: u64, layout: &mut cc_core::RampLayout, table: bool) -> Edited {
    use cc_core::RampShape;
    let mut e = Edited::default();
    let shape = field(ui, table, &t!("prop-stair-shape"), |ui| {
        combo(ui, ("ramp-shape", salt), &mut layout.shape, &RampShape::ALL, RampShape::key)
    });
    e.choice(shape);
    if layout.shape.turns() {
        let turn = field(ui, table, &t!("prop-stair-turn"), |ui| {
            combo(ui, ("ramp-turn", salt), &mut layout.turn, &Turn::ALL, Turn::key)
        });
        e.choice(turn);
    }
    let r = field(ui, table, &t!("prop-ramp-slope"), |ui| percent(ui, &mut layout.slope, Some(t!("ramp-slope-max"))));
    e.add(&r);
    if matches!(layout.shape, RampShape::Straight | RampShape::UShaped) {
        let r = field(ui, table, &t!("prop-stair-landing"), |ui| length(ui, &mut layout.landing, 100.0));
        e.add(&r);
    }
    if layout.shape == RampShape::UShaped {
        let r = field(ui, table, &t!("prop-stair-gap"), |ui| length(ui, &mut layout.gap, 100.0));
        e.add(&r);
    }
    if layout.shape == RampShape::Spiral {
        let r = field(ui, table, &t!("prop-stair-inner-radius"), |ui| length(ui, &mut layout.inner_radius, 100.0));
        e.add(&r);
    }
    e
}

/// Accessibility rules of a ramp type.
pub fn ramp_rules_rows(ui: &mut Ui, rules: &mut cc_core::RampRules) -> Edited {
    let mut e = Edited::default();
    let r = row(ui, &t!("prop-ramp-max-slope"), |ui| percent(ui, &mut rules.max_slope, None));
    e.add(&r);
    let r = row(ui, &t!("prop-ramp-max-flight-rise"), |ui| length(ui, &mut rules.max_flight_rise, 100.0));
    e.add(&r);
    let r = row(ui, &t!("prop-ramp-min-landing"), |ui| length(ui, &mut rules.min_landing, 100.0));
    e.add(&r);
    e
}

/// Slope, flights and the broken rules of a laid-out ramp.
pub fn ramp_check_rows(ui: &mut Ui, g: &cc_geom::RampGeom) {
    use cc_views::format;
    readonly_row(ui, &t!("col-slope"), format::slope(g.check.slope));
    readonly_row(ui, &t!("prop-ramp-flights"), g.check.flights.to_string());
    readonly_row(ui, &t!("prop-ramp-flight-rise"), format::length(g.check.flight_rise));
    readonly_row(ui, &t!("col-run"), format::length(g.run()));
    if g.check.ok() {
        ui.label(RichText::new(t!("stair-check-ok")).color(PALETTE.success));
    }
    for issue in &g.check.issues {
        ui.label(RichText::new(cc_engine::commands::ramp::issue_text(issue)).color(PALETTE.warning));
    }
}

/// The engine command creating a ramp with `layout`.
pub fn ramp_command(
    layout: cc_core::RampLayout,
    start: [f64; 2],
    direction: f64,
    level: cc_core::LevelId,
    type_id: cc_core::ElementTypeId,
) -> cc_engine::commands::ramp::RampCreate {
    use cc_engine::Ref;
    let shape = cc_engine::commands::ramp::SHAPES.iter().find(|(_, s)| *s == layout.shape).map(|(n, _)| n.to_string());
    let turn = match layout.turn {
        Turn::Left => "left",
        Turn::Right => "right",
    };
    cc_engine::commands::ramp::RampCreate {
        start,
        direction: Some(direction),
        level: Some(Ref::Id(level.0)),
        type_ref: Some(Ref::Id(type_id.0)),
        shape,
        turn: Some(turn.into()),
        slope: (layout.slope > 1e-6).then_some(layout.slope * 100.0),
        landing: (layout.landing > 1e-3).then_some(layout.landing),
        gap: Some(layout.gap),
        inner_radius: Some(layout.inner_radius),
        ..Default::default()
    }
}

/// Options bar of the ramp tool.
pub fn ramp_tool_options(ui: &mut Ui, state: &mut AppState) {
    let mut layout = state.ramp_layout;
    ramp_layout_controls(ui, 0, &mut layout, false);
    state.ramp_layout = layout;
    ui.separator();
    ui.label(crate::theme::muted(t!("hint-ramp")));
}
