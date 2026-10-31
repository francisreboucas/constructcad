//! Properties palette: instance parameters of the selection, or of the active view.
//!
//! A header block names what is shown (and its type, with "Edit Type"); below
//! it, parameters are listed in collapsible sections of a two-column table.

use crate::actions;
use crate::i18n::tr;
use crate::icons::{self, Icon};
use crate::state::AppState;
use crate::t;
use crate::table::{header_block, padded, readonly_row, row, section};
use crate::theme::{self, PALETTE};
use cc_core::{Category, Element, ElementId, ElementKind, RoofShape, SheetSize, ViewId, ViewKind};
use cc_views::format;
use egui::{DragValue, Response, RichText, Sense, Ui, vec2};
use std::hash::{Hash, Hasher};

const SCALES: [u32; 9] = [10, 20, 25, 50, 75, 100, 200, 500, 1000];

fn key(parts: impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut h);
    h.finish()
}

/// End coalescing when a drag or text edit finishes.
fn finish(state: &mut AppState, r: &Response) {
    if r.drag_stopped() || r.lost_focus() {
        state.session.doc.break_merge();
    }
}

fn modify(state: &mut AppState, id: ElementId, field: &str, f: impl FnOnce(&mut Element)) {
    crate::engine_edit::element(state, id, Some(key((id, field))), f);
}

fn length_row(
    ui: &mut Ui,
    state: &mut AppState,
    label: &str,
    id: ElementId,
    field: &str,
    value: f64,
    set: impl FnOnce(&mut Element, f64),
) {
    let mut v = value;
    let r = row(ui, label, |ui| ui.add(crate::properties::length_drag(&mut v).speed(0.01)));
    if r.changed() {
        modify(state, id, field, |el| set(el, v));
    }
    finish(state, &r);
}

fn level_row(ui: &mut Ui, state: &mut AppState, el: &Element) {
    let mut level = el.level;
    let levels: Vec<_> = state.project().levels.iter().map(|l| (l.id, l.name.clone())).collect();
    let selected = state.project().level_name(el.level).to_string();
    row(ui, &t!("prop-level"), |ui| {
        egui::ComboBox::from_id_salt(("level", el.id.0)).selected_text(selected).show_ui(ui, |ui| {
            for (id, name) in levels {
                ui.selectable_value(&mut level, id, name);
            }
        });
    });
    // The engine moves the doors and windows of a wall along with it.
    if level != el.level {
        crate::engine_edit::element(state, el.id, None, |x| x.level = level);
    }
}

/// The value of a dimension: typing a new one (Enter) moves what it measures.
fn dimension_value_row(ui: &mut Ui, state: &mut AppState, id: ElementId, d: &cc_core::Dimension) {
    let current = match d.kind {
        cc_core::DimensionKind::Angular { .. } => format::angle(d.value().to_degrees()),
        cc_core::DimensionKind::Radial | cc_core::DimensionKind::Diameter => {
            readonly_row(ui, &t!("prop-value"), format::length(d.value()));
            return;
        }
        _ => format::length(d.value()),
    };
    let key = ui.make_persistent_id(("dim-value", id.0));
    let mut text = ui.data(|m| m.get_temp::<String>(key)).unwrap_or(current);
    let r = row(ui, &t!("prop-value"), |ui| ui.add(egui::TextEdit::singleline(&mut text)));
    if r.has_focus() {
        ui.data_mut(|m| m.insert_temp(key, text.clone()));
    } else {
        ui.data_mut(|m| m.remove::<String>(key));
    }
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        let digits = text.trim_end_matches(|c: char| !c.is_ascii_digit()).to_string();
        if let Err(message) = crate::dim_edit::apply_text(state, &crate::dim_edit::Target::Dimension(id), &digits) {
            state.message = message;
        }
    }
}

fn text_row(ui: &mut Ui, label: &str, value: String) -> (Response, String) {
    let mut text = value;
    let r = row(ui, label, |ui| {
        crate::table::live_text(ui, ("prop-text", label), &mut text, |ui, t| ui.text_edit_singleline(t))
    });
    (r, text)
}

/// Panel entry point.
pub fn show(ui: &mut Ui, state: &mut AppState) {
    theme::panel_header(ui, &t!("props-title"), None);
    egui::ScrollArea::vertical().id_salt("props").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if let Some((sheet, index)) = state.sheet_selection
            && state.active_view == Some(sheet)
        {
            viewport_props(ui, state, sheet, index);
            return;
        }
        match state.selection.len() {
            0 => match state.active_view {
                Some(v) => view_props(ui, state, v),
                None => {
                    padded(ui, |ui| {
                        ui.label(RichText::new(t!("props-nothing")).color(PALETTE.text_faint));
                    });
                }
            },
            1 => {
                if let Some(&id) = state.selection.iter().next() {
                    element_props(ui, state, id);
                }
            }
            n => multi_props(ui, state, n),
        }
    });
}

fn multi_props(ui: &mut Ui, state: &mut AppState, n: usize) {
    padded(ui, |ui| {
        header_block(ui, Icon::Select, &t!("props-multi", "count" => n), "");
        let mut counts = std::collections::BTreeMap::new();
        for id in &state.selection {
            if let Some(e) = state.project().element(*id) {
                *counts.entry(e.category()).or_insert(0) += 1;
            }
        }
        for (c, k) in counts {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                icons::paint(ui.painter(), r, crate::ribbon::category_icon(c), PALETTE.text_muted);
                ui.label(format!("{}: {k}", actions::t_category(c)));
            });
        }
        if ui.button(t!("action-delete")).clicked() {
            state.delete_selection();
        }
    });
}

fn element_props(ui: &mut Ui, state: &mut AppState, id: ElementId) {
    let Some(el) = state.project().element(id).cloned() else { return };
    let category = el.category();

    // Type block: what is selected, its type and "Edit Type".
    let types: Vec<_> = state.project().types_of(category).map(|t| (t.id, t.name.clone())).collect();
    let mut type_id = el.type_id;
    let type_name = state.project().type_name(el.type_id).to_string();
    padded(ui, |ui| {
        let subtitle = format!("{} · {}", actions::t_category(category), el.id);
        header_block(ui, crate::ribbon::category_icon(category), &type_name, &subtitle);
        ui.horizontal(|ui| {
            let button_w = 96.0;
            egui::ComboBox::from_id_salt(("type", id.0))
                .width(ui.available_width() - button_w - 30.0)
                .selected_text(type_name.clone())
                .show_ui(ui, |ui| {
                    for (tid, name) in &types {
                        ui.selectable_value(&mut type_id, *tid, name);
                    }
                });
            if ui.add_sized([button_w, 22.0], egui::Button::new(t!("props-edit-type"))).clicked() {
                state.dialogs.type_editor = Some(el.type_id);
            }
        });
    });
    if type_id != el.type_id {
        crate::engine_edit::element(state, id, None, |x| x.type_id = type_id);
    }

    match &el.kind {
        ElementKind::Mass(m) => mass_props(ui, state, &el, m),
        ElementKind::Component(c) => component_props(ui, state, &el, c),
        ElementKind::Wall(w) => {
            section(ui, "constraints", t!("section-constraints"), |ui| wall_constraints(ui, state, &el, w));
            section(ui, "structural", t!("section-structural"), |ui| {
                let mut on = w.structural;
                if row(ui, &t!("prop-structural"), |ui| ui.checkbox(&mut on, "")).changed() {
                    modify(state, id, "structural", |e| {
                        if let ElementKind::Wall(w) = &mut e.kind {
                            w.structural = on;
                        }
                    });
                }
            });
            section(ui, "geometry", t!("section-geometry"), |ui| {
                for (label, field, value) in [
                    (t!("prop-start-x"), "sx", w.start.x),
                    (t!("prop-start-y"), "sy", w.start.y),
                    (t!("prop-end-x"), "ex", w.end.x),
                    (t!("prop-end-y"), "ey", w.end.y),
                ] {
                    length_row(ui, state, &label, id, field, value, |e, v| {
                        if let ElementKind::Wall(w) = &mut e.kind {
                            match field {
                                "sx" => w.start.x = v,
                                "sy" => w.start.y = v,
                                "ex" => w.end.x = v,
                                _ => w.end.y = v,
                            }
                        }
                    });
                }
                // Arc walls: the angle the arc turns (0 for a straight wall).
                let mut sweep = (4.0 * w.bulge.atan()).to_degrees();
                let drag = DragValue::new(&mut sweep).speed(1.0).range(-337.0..=337.0).suffix("°");
                let r = row(ui, &t!("prop-arc-angle"), |ui| ui.add(drag));
                if r.changed() {
                    modify(state, id, "sweep", |el| {
                        if let ElementKind::Wall(w) = &mut el.kind {
                            w.bulge = if sweep.abs() < 0.01 { 0.0 } else { (sweep.to_radians() / 4.0).tan() };
                        }
                    });
                }
                finish(state, &r);
            });
            section(ui, "dimensions", t!("section-dimensions"), |ui| {
                readonly_row(ui, &t!("prop-length"), format::length(state.project().wall_length(&el, w)));
                if let Some(arc) = w.arc() {
                    readonly_row(ui, &t!("prop-radius"), format::length(arc.radius));
                }
                let height = state.project().wall_height(&el, w);
                readonly_row(ui, &t!("prop-height"), format::length(height));
                let thickness = state.project().thickness_of(&el);
                readonly_row(ui, &t!("prop-thickness"), format::length(thickness));
                readonly_row(ui, &t!("prop-volume"), format::volume(state.session.geom().volume(id)));
            });
        }
        ElementKind::Door(h) | ElementKind::Window(h) => {
            section(ui, "constraints", t!("section-constraints"), |ui| {
                readonly_row(ui, &t!("prop-level"), state.project().level_name(el.level).to_string());
                let host = state.project().element(h.host).map(|w| w.id.to_string()).unwrap_or_default();
                readonly_row(ui, &t!("prop-host"), host);
                length_row(ui, state, &t!("prop-offset"), id, "offset", h.offset, |e, v| {
                    if let Some(h) = e.as_hosted_mut() {
                        h.offset = v;
                    }
                });
                // Doors too may stand above the wall's base (a raised floor, a step).
                let sill_label = if category == Category::Window { t!("prop-sill") } else { t!("prop-base-offset") };
                length_row(ui, state, &sill_label, id, "sill", h.sill_height, |e, v| {
                    if let Some(h) = e.as_hosted_mut() {
                        h.sill_height = v.max(0.0);
                    }
                });
                // The head: typing it moves the opening, keeping its height.
                if let Some((_, height)) = state.project().element_type(el.type_id).and_then(|t| t.opening_size()) {
                    length_row(ui, state, &t!("prop-head-height"), id, "head", h.sill_height + height, |e, v| {
                        if let Some(h) = e.as_hosted_mut() {
                            h.sill_height = (v - height).max(0.0);
                        }
                    });
                }
            });
            section(ui, "orientation", t!("section-orientation"), |ui| {
                for (label, field, value) in
                    [(t!("prop-flip-facing"), "ff", h.flip_facing), (t!("prop-flip-hand"), "fh", h.flip_hand)]
                {
                    let mut v = value;
                    if row(ui, &label, |ui| ui.checkbox(&mut v, "")).changed() {
                        crate::engine_edit::element(state, id, None, |x| {
                            if let Some(h) = x.as_hosted_mut() {
                                if field == "ff" {
                                    h.flip_facing = v;
                                } else {
                                    h.flip_hand = v;
                                }
                            }
                        });
                    }
                }
            });
            if let Some((w, hh)) = state.project().element_type(el.type_id).and_then(|t| t.opening_size()) {
                section(ui, "dimensions", t!("section-dimensions"), |ui| {
                    readonly_row(ui, &t!("col-width"), format::length(w));
                    readonly_row(ui, &t!("col-height"), format::length(hh));
                });
            }
        }
        ElementKind::Room(r) => room_props(ui, state, &el, r),
        ElementKind::Column(c) => {
            section(ui, "constraints", t!("section-constraints"), |ui| {
                level_row(ui, state, &el);
                length_row(ui, state, &t!("prop-base-offset"), id, "base", c.base_offset, |e, v| {
                    if let ElementKind::Column(c) = &mut e.kind {
                        c.base_offset = v;
                    }
                });
                top_rows(ui, state, &el, c.top, c.height);
                let mut rot = c.rotation_deg;
                let r = row(ui, &t!("prop-rotation"), |ui| ui.add(DragValue::new(&mut rot).speed(1.0).suffix("°")));
                if r.changed() {
                    modify(state, id, "rotation", |e| {
                        if let ElementKind::Column(c) = &mut e.kind {
                            c.rotation_deg = rot.rem_euclid(360.0);
                        }
                    });
                }
                finish(state, &r);
            });
            section(ui, "structural", t!("section-structural"), |ui| {
                let mut on = c.structural;
                if row(ui, &t!("prop-structural"), |ui| ui.checkbox(&mut on, "")).changed() {
                    modify(state, id, "structural", |e| {
                        if let ElementKind::Column(c) = &mut e.kind {
                            c.structural = on;
                        }
                    });
                }
            });
            section(ui, "dimensions", t!("section-dimensions"), |ui| {
                let (base, top) = state.project().column_span(&el, c);
                readonly_row(ui, &t!("prop-height"), format::length(top - base));
                readonly_row(ui, &t!("prop-volume"), format::volume(state.session.geom().volume(id)));
            });
            reinforcement(ui, state, id);
        }
        ElementKind::Rebar(r) => rebar_props(ui, state, &el, r),
        ElementKind::Pipe(s) | ElementKind::Conduit(s) => segment_props(ui, state, &el, s),
        ElementKind::Beam(b) => {
            section(ui, "constraints", t!("section-constraints"), |ui| {
                level_row(ui, state, &el);
                length_row(ui, state, &t!("prop-elevation-offset"), id, "beam-top", b.top_offset, |e, v| {
                    if let ElementKind::Beam(b) = &mut e.kind {
                        b.top_offset = v;
                    }
                });
                let end = b.end_offset.unwrap_or(b.top_offset);
                length_row(ui, state, &t!("prop-end-elevation-offset"), id, "beam-end", end, |e, v| {
                    if let ElementKind::Beam(b) = &mut e.kind {
                        // Level again when the end returns to the start's elevation.
                        b.end_offset = ((v - b.top_offset).abs() > 1e-9).then_some(v);
                    }
                });
                beam_justification(ui, state, id, b);
                for (label, at_start, value) in
                    [(t!("prop-join-start"), true, b.join_start), (t!("prop-join-end"), false, b.join_end)]
                {
                    let mut on = value;
                    if row(ui, &label, |ui| ui.checkbox(&mut on, "")).changed() {
                        crate::engine_edit::element(state, id, None, |x| {
                            if let ElementKind::Beam(b) = &mut x.kind {
                                *(if at_start { &mut b.join_start } else { &mut b.join_end }) = on;
                            }
                        });
                    }
                }
            });
            section(ui, "dimensions", t!("section-dimensions"), |ui| {
                readonly_row(ui, &t!("prop-length"), format::length(b.length()));
                if b.end_offset.is_some() {
                    readonly_row(ui, &t!("prop-true-length"), format::length(b.true_length()));
                }
                if let Some(g) = state.session.geom().beams.get(&id) {
                    readonly_row(ui, &t!("prop-cut-length"), format::length(g.cut_length()));
                }
                readonly_row(ui, &t!("prop-volume"), format::volume(state.session.geom().volume(id)));
            });
            reinforcement(ui, state, id);
        }
        ElementKind::Foundation(f) => {
            section(ui, "constraints", t!("section-constraints"), |ui| {
                level_row(ui, state, &el);
                length_row(ui, state, &t!("prop-top-offset"), id, "foundation-top", f.top_offset, |e, v| {
                    if let ElementKind::Foundation(f) = &mut e.kind {
                        f.top_offset = v;
                    }
                });
            });
            section(ui, "dimensions", t!("section-dimensions"), |ui| {
                if let Some(g) = state.session.geom().foundations.get(&id).cloned() {
                    readonly_row(ui, &t!("col-area"), format::area(cc_geom::polygon::area(&g.footprint)));
                }
                readonly_row(ui, &t!("prop-volume"), format::volume(state.session.geom().volume(id)));
            });
        }
        ElementKind::Stair(st) => {
            section(ui, "constraints", t!("section-constraints"), |ui| {
                level_row(ui, state, &el);
                length_row(ui, state, &t!("prop-base-offset"), id, "base", st.base_offset, |e, v| {
                    if let ElementKind::Stair(s) = &mut e.kind {
                        s.base_offset = v;
                    }
                });
                top_rows(ui, state, &el, st.top, st.height);
            });
            section(ui, "stair-layout", t!("section-stair-layout"), |ui| {
                let mut layout = st.layout;
                let edited = crate::stair_ui::layout_controls(ui, id.0, &mut layout, true);
                if edited.changed {
                    modify(state, id, "layout", |e| {
                        if let ElementKind::Stair(s) = &mut e.kind {
                            s.layout = layout;
                        }
                    });
                }
                if edited.done {
                    state.session.doc.break_merge();
                }
            });
            if let Some(g) = cc_geom::stair_geom(state.project(), &el, st) {
                section(ui, "dimensions", t!("section-stair-check"), |ui| crate::stair_ui::check_rows(ui, &g));
            }
            let railings = state.project().railings_of(id).count();
            if railings == 0 && ui.button(t!("action-stair-railings")).clicked() {
                let command =
                    cc_engine::commands::stair::StairRailingsAdd { id: id.0, side: None, type_ref: None, inset: None };
                actions::run_command(state, command);
            }
        }
        ElementKind::Ramp(rp) => {
            section(ui, "constraints", t!("section-constraints"), |ui| {
                level_row(ui, state, &el);
                length_row(ui, state, &t!("prop-base-offset"), id, "base", rp.base_offset, |e, v| {
                    if let ElementKind::Ramp(r) = &mut e.kind {
                        r.base_offset = v;
                    }
                });
                top_rows(ui, state, &el, rp.top, rp.height);
            });
            section(ui, "ramp-layout", t!("section-ramp-layout"), |ui| {
                let mut layout = rp.layout;
                let edited = crate::stair_ui::ramp_layout_controls(ui, id.0, &mut layout, true);
                if edited.changed {
                    modify(state, id, "layout", |e| {
                        if let ElementKind::Ramp(r) = &mut e.kind {
                            r.layout = layout;
                        }
                    });
                }
                if edited.done {
                    state.session.doc.break_merge();
                }
            });
            if let Some(g) = cc_geom::ramp_geom(state.project(), &el, rp) {
                section(ui, "dimensions", t!("section-ramp-check"), |ui| crate::stair_ui::ramp_check_rows(ui, &g));
            }
            let railings = state.project().railings_of(id).count();
            if railings == 0 && ui.button(t!("action-stair-railings")).clicked() {
                let command =
                    cc_engine::commands::stair::StairRailingsAdd { id: id.0, side: None, type_ref: None, inset: None };
                actions::run_command(state, command);
            }
        }
        ElementKind::Railing(r) => {
            section(ui, "constraints", t!("section-constraints"), |ui| {
                level_row(ui, state, &el);
                if r.stair.is_none() {
                    length_row(ui, state, &t!("prop-base-offset"), id, "base", r.base_offset, |e, v| {
                        if let ElementKind::Railing(r) = &mut e.kind {
                            r.base_offset = v;
                        }
                    });
                } else if let Some(h) = r.stair {
                    // The host's category and mark, e.g. "Ramp R1".
                    let host = state.project().element(h.host).map(|e| (e.category(), e.mark.clone()));
                    let text =
                        host.map_or(String::new(), |(c, mark)| format!("{} {mark}", tr(c.key())).trim().to_string());
                    readonly_row(ui, &t!("prop-railing-host"), text);
                }
            });
            section(ui, "dimensions", t!("section-dimensions"), |ui| {
                let length = cc_geom::railing_length(state.project(), &el, r);
                readonly_row(ui, &t!("prop-length"), format::length(length));
            });
        }
        ElementKind::Grid(g) => {
            section(ui, "identity-grid", t!("section-identity"), |ui| {
                let (resp, text) = text_row(ui, &t!("col-name"), g.name.clone());
                if resp.changed() {
                    modify(state, id, "grid-name", |e| {
                        if let ElementKind::Grid(g) = &mut e.kind {
                            g.name = text;
                        }
                    });
                }
                finish(state, &resp);
            });
            section(ui, "dimensions", t!("section-dimensions"), |ui| {
                readonly_row(ui, &t!("prop-length"), format::length(g.start.distance(g.end)));
            });
        }
        ElementKind::Text(note) => text_props(ui, state, id, note),
        ElementKind::CadImport(c) => crate::cad_ui::props(ui, state, id, c),
        ElementKind::Dimension(d) => {
            section(ui, "dimensions", t!("section-dimensions"), |ui| {
                dimension_value_row(ui, state, id, d);
                let view = state.project().view(d.view).map(|v| v.name.clone()).unwrap_or_default();
                readonly_row(ui, &t!("prop-view"), view);
                length_row(ui, state, &t!("prop-dimension-offset"), id, "dim-offset", d.offset, |e, v| {
                    if let ElementKind::Dimension(d) = &mut e.kind {
                        d.offset = v;
                    }
                });
            });
        }
        ElementKind::Floor(f) => {
            section(ui, "constraints", t!("section-constraints"), |ui| {
                level_row(ui, state, &el);
                length_row(ui, state, &t!("prop-height-offset"), id, "hoff", f.height_offset, |e, v| {
                    if let ElementKind::Floor(f) = &mut e.kind {
                        f.height_offset = v;
                    }
                });
            });
            section(ui, "dimensions", t!("section-dimensions"), |ui| {
                readonly_row(ui, &t!("col-area"), format::area(f.area()));
                // Openings (shafts, stairwells): sketch one more.
                row(ui, &t!("prop-roof-openings"), |ui| {
                    ui.label(f.openings.len().to_string());
                    if ui.button(t!("action-roof-add-opening")).clicked() {
                        state.set_tool(crate::tools::Tool::Opening { host: id, floor: true });
                        state.message = t!("hint-floor-opening");
                    }
                });
            });
            if !f.openings.is_empty() {
                section(ui, "floor-openings", t!("prop-roof-openings"), |ui| {
                    opening_rows(ui, state, id, &f.openings, true)
                });
            }
        }
        ElementKind::Ceiling(c) => {
            section(ui, "constraints", t!("section-constraints"), |ui| {
                level_row(ui, state, &el);
                length_row(ui, state, &t!("prop-height-offset"), id, "hoff", c.height_offset, |e, v| {
                    if let ElementKind::Ceiling(c) = &mut e.kind {
                        c.height_offset = v;
                    }
                });
            });
            section(ui, "dimensions", t!("section-dimensions"), |ui| {
                let area = cc_geom::polygon::area(&c.boundary);
                readonly_row(ui, &t!("col-area"), format::area(area));
            });
        }
        ElementKind::Roof(r) => {
            section(ui, "constraints", t!("section-constraints"), |ui| {
                level_row(ui, state, &el);
                length_row(ui, state, &t!("prop-base-offset"), id, "base", r.base_offset, |e, v| {
                    if let ElementKind::Roof(r) = &mut e.kind {
                        r.base_offset = v;
                    }
                });
            });
            section(ui, "construction", t!("section-construction"), |ui| roof_rows(ui, state, id, r));
            if matches!(r.shape, RoofShape::Gable { .. } | RoofShape::Hip { .. }) {
                section(ui, "roof-edges", t!("section-roof-edges"), |ui| roof_edge_rows(ui, state, id, r));
            }
            if !r.holes.is_empty() {
                section(ui, "roof-openings", t!("prop-roof-openings"), |ui| {
                    opening_rows(ui, state, id, &r.holes, false)
                });
            }
            roofing_rows(ui, state, &el);
        }
    }

    // Rooms, grids and dimensions have their own identity rows; only model elements carry a mark.
    let mut identity = vec![(t!("col-comments"), "comments", el.comments.clone())];
    if category.is_model() {
        identity.insert(0, (t!("col-mark"), "mark", el.mark.clone()));
    }
    let identity_id = if category.is_model() { "identity" } else { "identity-extra" };
    let identity_title = if category.is_model() { t!("section-identity") } else { t!("section-other") };
    section(ui, identity_id, identity_title, |ui| {
        for (label, field, value) in identity {
            let (r, text) = text_row(ui, &label, value);
            if r.changed() {
                modify(state, id, field, |e| {
                    if field == "mark" {
                        e.mark = text;
                    } else {
                        e.comments = text;
                    }
                });
            }
            finish(state, &r);
        }
        let mut pinned = state.project().is_pinned(id);
        if row(ui, &t!("prop-pinned"), |ui| ui.checkbox(&mut pinned, "")).changed() {
            crate::actions::run_command(state, cc_engine::commands::modify::ElementsPin { ids: vec![id.0], pinned });
        }
    });
    crate::phases_dialog::element_rows(ui, state, &el);
    section(ui, "ifc", t!("section-ifc"), |ui| {
        readonly_row(ui, &t!("prop-ifc-guid"), cc_ifc::guid::encode(el.uid.0));
    });
    let view = state.active_view;
    let moved = view.and_then(|v| state.project().view(v)).is_some_and(|v| v.tag_offsets.contains_key(&id));
    padded(ui, |ui| {
        ui.horizontal(|ui| {
            if ui.button(t!("action-delete")).clicked() {
                state.delete_selection();
            }
            // The tag was moved in this view: put it back.
            if moved
                && let Some(view) = view
                && ui.button(t!("action-reset-tag")).clicked()
            {
                let reset = cc_engine::commands::annotate::TagMove {
                    view: cc_engine::Ref::Id(view.0),
                    id: id.0,
                    delta: None,
                    offset: None,
                    reset: true,
                };
                actions::run_command(state, reset);
            }
        });
    });
}

/// Top constraint rows shared by columns, stairs and ramps (walls have their own).
fn top_rows(ui: &mut Ui, state: &mut AppState, el: &Element, top: cc_core::WallTop, height: f64) {
    use cc_core::WallTop;
    let id = el.id;
    let set_top = |state: &mut AppState, field: &str, top: WallTop, height: Option<f64>| {
        modify(state, id, field, |e| match &mut e.kind {
            ElementKind::Column(c) => {
                c.top = top;
                if let Some(h) = height {
                    c.height = h;
                }
            }
            ElementKind::Stair(s) => {
                s.top = top;
                if let Some(h) = height {
                    s.height = h;
                }
            }
            ElementKind::Ramp(r) => {
                r.top = top;
                if let Some(h) = height {
                    r.height = h;
                }
            }
            _ => {}
        });
    };
    let levels: Vec<_> = state.project().levels.iter().map(|l| (l.id, l.name.clone())).collect();
    let name = |level| levels.iter().find(|l| l.0 == level).map_or("?".to_string(), |l| l.1.clone());
    let current = match top {
        WallTop::Unconnected => t!("top-unconnected"),
        WallTop::Level { level, .. } => t!("top-up-to", "level" => name(level)),
    };
    let mut choice = match top {
        WallTop::Level { level, .. } => Some(level),
        WallTop::Unconnected => None,
    };
    let before = choice;
    row(ui, &t!("prop-top-constraint"), |ui| {
        egui::ComboBox::from_id_salt(("top", id.0)).selected_text(current).show_ui(ui, |ui| {
            ui.selectable_value(&mut choice, None, t!("top-unconnected"));
            for (lid, lname) in &levels {
                ui.selectable_value(&mut choice, Some(*lid), t!("top-up-to", "level" => lname));
            }
        });
    });
    if choice != before {
        let new_top = choice.map_or(WallTop::Unconnected, |level| WallTop::Level { level, offset: 0.0 });
        set_top(state, "top", new_top, None);
        state.session.doc.break_merge();
    }
    match top {
        WallTop::Level { level, offset } => {
            length_row(ui, state, &t!("prop-top-offset"), id, "top-offset", offset, move |e, v| match &mut e.kind {
                ElementKind::Column(c) => c.top = WallTop::Level { level, offset: v },
                ElementKind::Stair(s) => s.top = WallTop::Level { level, offset: v },
                ElementKind::Ramp(r) => r.top = WallTop::Level { level, offset: v },
                _ => {}
            });
        }
        WallTop::Unconnected => {
            length_row(ui, state, &t!("prop-unconnected-height"), id, "height", height, |e, v| match &mut e.kind {
                ElementKind::Column(c) => c.height = v.max(0.1),
                ElementKind::Stair(s) => s.height = v.max(0.1),
                ElementKind::Ramp(r) => r.height = v.max(0.01),
                _ => {}
            });
        }
    }
}

/// Number, name, height and computed area of a room.
/// Drag value for a length stored in meters, shown and typed in the
/// project's length unit (a typed unit symbol overrides it).
pub fn length_drag(value: &mut f64) -> DragValue<'_> {
    let (units, comma) = format::current();
    let unit = units.length.unit;
    let parse_units = units.clone();
    let drag = DragValue::new(value)
        .speed(0.005)
        .custom_formatter(move |v, _| format::length_number_with(&units, comma, v))
        .custom_parser(move |s| format::parse_length_with(&parse_units, s));
    // Feet and inches carry their own marks.
    if unit.is_fractional() { drag } else { drag.suffix(format!(" {}", unit.symbol())) }
}

/// Drag value for a component parameter, with the unit of its kind.
pub fn param_value(value: &mut f64, kind: cc_core::ParamKind) -> DragValue<'_> {
    match kind {
        cc_core::ParamKind::Length => crate::properties::length_drag(value).speed(0.01),
        cc_core::ParamKind::Angle => DragValue::new(value).speed(1.0).suffix("°"),
        cc_core::ParamKind::Number => DragValue::new(value).speed(0.1),
    }
}

/// A pipe or conduit: elevations of its ends, its system or circuits, length and slope.
fn segment_props(ui: &mut Ui, state: &mut AppState, el: &Element, s: &cc_core::MepSegment) {
    let id = el.id;
    let pipe = matches!(el.kind, ElementKind::Pipe(_));
    section(ui, "constraints", t!("section-constraints"), |ui| {
        level_row(ui, state, el);
        length_row(ui, state, &t!("prop-start-elevation"), id, "seg-start", s.start_offset, |e, v| {
            if let Some(s) = e.as_segment_mut() {
                s.start_offset = v;
            }
        });
        length_row(ui, state, &t!("prop-end-elevation"), id, "seg-end", s.end_offset, |e, v| {
            if let Some(s) = e.as_segment_mut() {
                s.end_offset = v;
            }
        });
    });
    section(ui, "mep-system", t!("section-mep"), |ui| {
        if pipe {
            let mut system = s.system();
            let changed = row(ui, &t!("col-system"), |ui| {
                crate::systems_dialog::system_combo(ui, state, ("pipe-system", id.0), Some(false), &mut system)
            });
            if changed {
                crate::engine_edit::element(state, id, None, |e| {
                    if let Some(s) = e.as_segment_mut() {
                        s.systems = system.into_iter().collect();
                    }
                });
            }
        } else {
            // A conduit carries any number of circuits.
            let circuits: Vec<(cc_core::SystemId, String)> = state
                .project()
                .systems
                .iter()
                .filter(|(_, c)| c.kind.is_electrical())
                .map(|(cid, c)| (*cid, format!("{} – {}", c.label(), c.name)))
                .collect();
            for (cid, label) in circuits {
                let mut on = s.systems.contains(&cid);
                if row(ui, &label, |ui| ui.checkbox(&mut on, "")).changed() {
                    crate::engine_edit::element(state, id, None, |e| {
                        if let Some(s) = e.as_segment_mut() {
                            s.systems.retain(|x| *x != cid);
                            if on {
                                s.systems.push(cid);
                            }
                        }
                    });
                }
            }
        }
    });
    section(ui, "dimensions", t!("section-dimensions"), |ui| {
        if let Some(d) = state.project().element_type(el.type_id).and_then(|t| t.diameter()) {
            readonly_row(ui, &t!("col-diameter"), format::length(d));
        }
        readonly_row(ui, &t!("prop-length"), format::length(s.length()));
        if pipe && !s.is_vertical() {
            readonly_row(ui, &t!("col-slope"), format::slope(s.slope()));
        }
    });
}

/// The system or circuit of a plumbing fixture or electrical device, and its data.
fn component_services(ui: &mut Ui, state: &mut AppState, el: &Element, c: &cc_core::Component) {
    let Some(mep) = state.project().mep_data(el) else { return };
    let id = el.id;
    let device = matches!(mep, cc_core::MepData::Device { .. });
    section(ui, "mep-system", t!("section-mep"), |ui| {
        let mut system = c.system;
        let label = if device { t!("prop-circuit") } else { t!("col-system") };
        let changed = row(ui, &label, |ui| {
            crate::systems_dialog::system_combo(ui, state, ("comp-system", id.0), Some(device), &mut system)
        });
        if changed {
            crate::engine_edit::element(state, id, None, |e| {
                if let ElementKind::Component(c) = &mut e.kind {
                    c.system = system;
                }
            });
        }
        match mep {
            cc_core::MepData::Device { device, power } => {
                readonly_row(ui, &t!("prop-device"), tr(device.key()));
                readonly_row(ui, &t!("col-power"), format!("{} VA", format::number(power, 0)));
            }
            cc_core::MepData::Fixture { drain_units, supply_weight, .. } => {
                readonly_row(ui, &t!("col-drain-units"), format::number(drain_units, 1));
                readonly_row(ui, &t!("prop-supply-weight"), format::number(supply_weight, 2));
            }
        }
    });
}

/// A conceptual mass: its level, base offset and height, whether it is a
/// loft, and its mass floors (a box per level it spans, with the area of
/// each and the totals).
fn mass_props(ui: &mut Ui, state: &mut AppState, el: &Element, m: &cc_core::mass::Mass) {
    let id = el.id;
    section(ui, "constraints", t!("section-constraints"), |ui| {
        level_row(ui, state, el);
        length_row(ui, state, &t!("prop-base-offset"), id, "base", m.base_offset, |e, v| {
            if let ElementKind::Mass(m) = &mut e.kind {
                m.base_offset = v;
            }
        });
        length_row(ui, state, &t!("prop-height"), id, "height", m.height, |e, v| {
            if let ElementKind::Mass(m) = &mut e.kind {
                m.height = v.max(0.1);
            }
        });
        let shape = if m.top.is_some() { t!("mass-loft") } else { t!("mass-extrusion") };
        readonly_row(ui, &t!("prop-mass-shape"), shape);
    });
    let geom = state.session.geom().masses.get(&id).cloned();
    section(ui, "mass-floors", t!("section-mass-floors"), |ui| {
        let project = state.project();
        let base = project.level_elevation(el.level) + m.base_offset;
        let spanned = cc_engine::commands::mass::spanned_levels(project, base, base + m.height);
        let rows: Vec<_> = spanned.iter().map(|l| (*l, project.level_name(*l).to_string())).collect();
        if rows.is_empty() {
            readonly_row(ui, &t!("section-mass-floors"), t!("mass-no-levels"));
        }
        for (level, name) in rows {
            let mut on = m.floors.contains(&level);
            let area = geom.as_ref().and_then(|g| g.floors.iter().find(|f| f.level == level)).map(|f| f.area);
            let changed = row(ui, &name, |ui| {
                let r = ui.checkbox(&mut on, area.map(format::area).unwrap_or_default());
                r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, &name));
                r.changed()
            });
            if changed {
                let mut levels: Vec<_> = m.floors.iter().copied().filter(|l| *l != level).collect();
                if on {
                    levels.push(level);
                }
                let levels = levels.into_iter().map(|l| cc_engine::Ref::Id(l.0)).collect();
                actions::run_command(
                    state,
                    cc_engine::commands::mass::MassFloorsSet { id: id.0, levels: Some(levels) },
                );
            }
        }
        if let Some(g) = &geom {
            readonly_row(ui, &t!("col-floor-area"), format::area(g.floor_area()));
            readonly_row(ui, &t!("prop-volume"), format::volume(g.volume));
        }
    });
}

fn component_props(ui: &mut Ui, state: &mut AppState, el: &Element, c: &cc_core::Component) {
    let id = el.id;
    section(ui, "constraints", t!("section-constraints"), |ui| {
        level_row(ui, state, el);
        length_row(ui, state, &t!("prop-base-offset"), id, "comp-base", c.base_offset, |e, v| {
            if let ElementKind::Component(c) = &mut e.kind {
                c.base_offset = v;
            }
        });
        let mut rot = c.rotation_deg;
        let r = row(ui, &t!("prop-rotation"), |ui| ui.add(DragValue::new(&mut rot).speed(1.0).suffix("°")));
        if r.changed() {
            modify(state, id, "comp-rotation", |e| {
                if let ElementKind::Component(c) = &mut e.kind {
                    c.rotation_deg = rot.rem_euclid(360.0);
                }
            });
        }
        finish(state, &r);
    });
    component_services(ui, state, el, c);
    let Some(cc_core::TypeData::Component { def, .. }) =
        state.project().element_type(el.type_id).map(|t| t.data.clone())
    else {
        return;
    };
    if let Some(ifc) = &def.ifc {
        section(ui, "comp-ifc", t!("section-ifc"), |ui| {
            readonly_row(ui, &t!("prop-ifc-class"), cc_ifc::display_name(&ifc.entity));
            if !ifc.predefined.is_empty() {
                readonly_row(ui, &t!("prop-ifc-predefined"), ifc.predefined.clone());
            }
            if !ifc.object_type.is_empty() {
                readonly_row(ui, &t!("prop-ifc-object-type"), ifc.object_type.clone());
            }
        });
    }
    // Property and quantity sets from the imported file: the object's, then its type's.
    let type_sets = def.ifc.as_ref().map(|i| i.properties.as_slice()).unwrap_or_default();
    let sets = c.properties.iter().map(|s| (s, false)).chain(type_sets.iter().map(|s| (s, true)));
    for (i, (set, of_type)) in sets.enumerate() {
        let title = if of_type { t!("section-type-set", "name" => set.name.as_str()) } else { set.name.clone() };
        section(ui, &format!("comp-set-{i}"), title, |ui| {
            for p in &set.properties {
                property_rows(ui, &p.name, &p.value);
            }
        });
    }
    let instance: Vec<_> = def.params.iter().filter(|p| p.instance).cloned().collect();
    if !instance.is_empty() {
        section(ui, "comp-params", t!("section-instance-params"), |ui| {
            for p in instance {
                let current = c.overrides.get(&p.name).copied();
                let mut v = current.unwrap_or(p.value);
                let (r, reset) = row(ui, &p.name, |ui| {
                    ui.horizontal(|ui| {
                        let r = ui.add(param_value(&mut v, p.kind));
                        let reset = ui
                            .add_enabled(current.is_some(), egui::Button::new("↺").small())
                            .on_hover_text(t!("hint-param-reset"));
                        (r, reset.clicked())
                    })
                    .inner
                });
                if r.changed() {
                    let name = p.name.clone();
                    modify(state, id, &format!("comp-param-{name}"), |e| {
                        if let ElementKind::Component(c) = &mut e.kind {
                            c.overrides.insert(name, v);
                        }
                    });
                }
                if reset {
                    let name = p.name.clone();
                    crate::engine_edit::element(state, id, None, |x| {
                        if let ElementKind::Component(c) = &mut x.kind {
                            c.overrides.remove(&name);
                        }
                    });
                }
                finish(state, &r);
            }
        });
    }
    section(ui, "dimensions", t!("section-dimensions"), |ui| {
        readonly_row(ui, &t!("prop-volume"), format::volume(state.session.geom().volume(id)));
    });
}

/// A text note: its text, single-line or multiline (with a width), rotation and leaders.
fn text_props(ui: &mut Ui, state: &mut AppState, id: ElementId, note: &cc_core::TextNote) {
    let note = note.clone();
    let set = |state: &mut AppState, field: &str, f: &dyn Fn(&mut cc_core::TextNote)| {
        crate::engine_edit::element(state, id, Some(key((id, field))), |e| {
            if let ElementKind::Text(n) = &mut e.kind {
                f(n);
            }
        });
    };
    section(ui, "text", t!("section-text"), |ui| {
        let mut text = note.text.clone();
        let r = row(ui, &t!("prop-text"), |ui| ui.add(egui::TextEdit::multiline(&mut text).desired_rows(2)));
        if r.changed() && !text.trim().is_empty() {
            set(state, "text", &|n| n.text = text.clone());
        }
        finish(state, &r);
        let mut multiline = note.width.is_some();
        if row(ui, &t!("text-multiline"), |ui| ui.checkbox(&mut multiline, "")).changed() {
            let width = multiline.then_some(state.text_options.width);
            set(state, "multiline", &|n| n.width = width);
        }
        if let Some(width) = note.width {
            let mut w = width;
            let r = row(ui, &t!("prop-text-width"), |ui| {
                ui.add(DragValue::new(&mut w).speed(1.0).range(5.0..=1000.0).suffix(" mm"))
            });
            if r.changed() {
                set(state, "width", &|n| n.width = Some(w));
            }
            finish(state, &r);
        }
        let mut rot = note.rotation_deg;
        let r = row(ui, &t!("prop-rotation"), |ui| ui.add(DragValue::new(&mut rot).speed(1.0).suffix("°")));
        if r.changed() {
            set(state, "rotation", &|n| n.rotation_deg = rot.rem_euclid(360.0));
        }
        finish(state, &r);
        let view = state.project().view(note.view).map(|v| v.name.clone()).unwrap_or_default();
        readonly_row(ui, &t!("prop-view"), view);
    });
    section(ui, "leaders", t!("section-leaders"), |ui| {
        let mut remove = None;
        for (i, leader) in note.leaders.iter().enumerate() {
            let mut symbol = leader.symbol;
            let label = format!("{} {}", t!("label-leader"), i + 1);
            let (changed, removed) = row(ui, &label, |ui| {
                ui.horizontal(|ui| {
                    let changed = crate::text_tool::symbol_combo(ui, ("leader-symbol", id.0, i), &mut symbol);
                    (changed, ui.button(t!("action-remove")).clicked())
                })
                .inner
            });
            if changed {
                set(state, "leader-symbol", &|n| {
                    if let Some(l) = n.leaders.get_mut(i) {
                        l.symbol = symbol;
                    }
                });
            }
            if removed {
                remove = Some(i);
            }
        }
        if let Some(index) = remove {
            crate::actions::run_command(state, cc_engine::commands::annotate::TextRemoveLeader { id: id.0, index });
        }
        if row(ui, "", |ui| ui.button(t!("tool-add-leader")).clicked()) {
            state.set_tool(crate::tools::Tool::AddLeader { id });
            state.message = t!("hint-add-leader");
        }
    });
}

/// Read-only rows of an imported property (nested ones as `parent.child`).
fn property_rows(ui: &mut Ui, label: &str, value: &cc_core::properties::PropertyValue) {
    use cc_core::properties::PropertyValue as V;
    let text = match value {
        V::Single(v) => v.as_ref().map(ifc_value_text).unwrap_or_default(),
        V::Enumerated(values) | V::List(values) => values.iter().map(ifc_value_text).collect::<Vec<_>>().join(", "),
        V::Bounded { lower, upper, set_point } => {
            let t = |v: &Option<cc_core::properties::IfcValue>| v.as_ref().map(ifc_value_text).unwrap_or_default();
            let range = format!("{} – {}", t(lower), t(upper));
            if set_point.is_some() { format!("{range} ({})", t(set_point)) } else { range }
        }
        V::Table { defining, defined } => defining
            .iter()
            .zip(defined)
            .map(|(a, b)| format!("{} → {}", ifc_value_text(a), ifc_value_text(b)))
            .collect::<Vec<_>>()
            .join("; "),
        V::Complex { properties, .. } => {
            for p in properties {
                property_rows(ui, &format!("{label}.{}", p.name), &p.value);
            }
            return;
        }
        V::Quantity { kind, value } => match kind.as_str() {
            "IFCQUANTITYLENGTH" => format::meters(*value),
            "IFCQUANTITYAREA" => format::area(*value),
            "IFCQUANTITYVOLUME" => format::volume(*value),
            _ => format::number(*value, 3),
        },
    };
    readonly_row(ui, label, text);
}

/// An imported value as text, in the project's units for measures.
fn ifc_value_text(v: &cc_core::properties::IfcValue) -> String {
    use cc_core::properties::Atom;
    match &v.value {
        Atom::Real(x) => match v.measure.as_str() {
            "IFCLENGTHMEASURE" | "IFCPOSITIVELENGTHMEASURE" | "IFCNONNEGATIVELENGTHMEASURE" => format::meters(*x),
            "IFCAREAMEASURE" => format::area(*x),
            "IFCVOLUMEMEASURE" => format::volume(*x),
            "IFCPLANEANGLEMEASURE" | "IFCPOSITIVEPLANEANGLEMEASURE" => format::angle(x.to_degrees()),
            _ => format::number(*x, 3),
        },
        Atom::Integer(i) => i.to_string(),
        Atom::Bool(true) => t!("value-yes"),
        Atom::Bool(false) => t!("value-no"),
        Atom::Text(s) | Atom::Enum(s) => s.clone(),
    }
}

fn room_props(ui: &mut Ui, state: &mut AppState, el: &Element, r: &cc_core::Room) {
    let id = el.id;
    section(ui, "identity-room", t!("section-identity"), |ui| {
        for (label, field, value) in
            [(t!("col-number"), "number", r.number.clone()), (t!("col-name"), "name", r.name.clone())]
        {
            let (resp, text) = text_row(ui, &label, value);
            if resp.changed() {
                modify(state, id, field, |e| {
                    if let ElementKind::Room(r) = &mut e.kind {
                        if field == "number" {
                            r.number = text;
                        } else {
                            r.name = text;
                        }
                    }
                });
            }
            finish(state, &resp);
        }
    });
    section(ui, "constraints", t!("section-constraints"), |ui| {
        readonly_row(ui, &t!("prop-level"), state.project().level_name(el.level).to_string());
        length_row(ui, state, &t!("prop-room-height"), id, "room-height", r.height, |e, v| {
            if let ElementKind::Room(r) = &mut e.kind {
                r.height = v.max(0.1);
            }
        });
    });
    section(ui, "dimensions", t!("section-dimensions"), |ui| match state.session.geom().rooms.get(&id).cloned() {
        Some(boundary) => {
            let area = cc_geom::polygon::area(&boundary);
            readonly_row(ui, &t!("col-area"), format::area(area));
            readonly_row(ui, &t!("col-perimeter"), format::length(cc_geom::polygon::perimeter(&boundary)));
            readonly_row(ui, &t!("prop-volume"), format::volume(area * r.height));
        }
        None => readonly_row(ui, &t!("col-area"), t!("room-not-enclosed")),
    });
}

/// The bar count or the stirrup spacing of a reinforcement set, editable.
fn rebar_layout_row(ui: &mut Ui, state: &mut AppState, id: ElementId, label: &str, layout: cc_core::RebarLayout) {
    use cc_core::RebarLayout as L;
    match layout {
        L::Longitudinal { count, .. } => {
            let mut n = count;
            let r = row(ui, label, |ui| ui.add(DragValue::new(&mut n).range(1..=200).suffix(" ×")));
            if r.changed() {
                modify(state, id, "rebar-count", |e| {
                    if let ElementKind::Rebar(x) = &mut e.kind
                        && let L::Longitudinal { count, .. } = &mut x.layout
                    {
                        *count = n;
                    }
                });
            }
            finish(state, &r);
        }
        L::Stirrups { spacing } => {
            length_row(ui, state, label, id, "rebar-spacing", spacing, |e, v| {
                if let ElementKind::Rebar(x) = &mut e.kind
                    && let L::Stirrups { spacing } = &mut x.layout
                {
                    *spacing = v.max(0.02);
                }
            });
        }
    }
}

/// What a set is, in a word: its face, or stirrups.
fn rebar_role(layout: cc_core::RebarLayout) -> String {
    match layout {
        cc_core::RebarLayout::Longitudinal { face, .. } => tr(face.key()),
        cc_core::RebarLayout::Stirrups { .. } => t!("bars-stirrups"),
    }
}

/// The reinforcement sets of a beam or column: each editable or removable,
/// and a button adding a typical reinforcement when there is none.
fn reinforcement(ui: &mut Ui, state: &mut AppState, host: ElementId) {
    let sets: Vec<(ElementId, cc_core::RebarLayout, String)> = state
        .project()
        .rebar_of(host)
        .map(|(e, r)| (e.id, r.layout, state.project().type_name(e.type_id).to_string()))
        .collect();
    section(ui, "reinforcement", t!("section-reinforcement"), |ui| {
        let mut remove = None;
        for (id, layout, name) in &sets {
            let label = format!("{} · {name}", rebar_role(*layout));
            rebar_layout_row(ui, state, *id, &label, *layout);
            if row(ui, "", |ui| ui.small_button(t!("action-remove-rebar")).clicked()) {
                remove = Some(*id);
            }
        }
        if let Some(id) = remove {
            crate::actions::run_command(state, cc_engine::commands::modify::ElementsDelete { ids: vec![id.0] });
        }
        if sets.is_empty() && row(ui, "", |ui| ui.button(t!("action-rebar-typical")).clicked()) {
            let command = cc_engine::commands::rebar::RebarTypical { elements: vec![host.0], level: None };
            crate::actions::run_command(state, command);
        }
        if !sets.is_empty() {
            let geom = state.session.geom();
            let weight: f64 = sets.iter().filter_map(|(id, ..)| geom.rebar.get(id)).map(|r| r.weight()).sum();
            readonly_row(ui, &t!("col-weight"), format!("{} kg", format::number(weight, 2)));
        }
    });
}

/// A reinforcement set: its layout, cover and quantities.
fn rebar_props(ui: &mut Ui, state: &mut AppState, el: &Element, r: &cc_core::Rebar) {
    let id = el.id;
    section(ui, "constraints", t!("section-reinforcement"), |ui| {
        let host = state
            .project()
            .element(r.host)
            .map_or(String::new(), |h| if h.mark.is_empty() { tr(h.category().key()) } else { h.mark.clone() });
        readonly_row(ui, &t!("col-host"), host);
        rebar_layout_row(ui, state, id, &rebar_role(r.layout), r.layout);
        length_row(ui, state, &t!("prop-cover"), id, "rebar-cover", r.cover, |e, v| {
            if let ElementKind::Rebar(x) = &mut e.kind {
                x.cover = v.max(0.0);
            }
        });
    });
    if let Some(set) = state.session.geom().rebar.get(&id).cloned() {
        section(ui, "dimensions", t!("section-dimensions"), |ui| {
            readonly_row(ui, &t!("col-bar-count"), set.count().to_string());
            readonly_row(ui, &t!("col-bar-length"), format::length(set.bar_length()));
            readonly_row(ui, &t!("prop-length"), format::length(set.total_length()));
            readonly_row(ui, &t!("col-weight"), format!("{} kg", format::number(set.weight(), 2)));
        });
    }
}

/// Lateral and vertical justification of a beam: which line of its section
/// the location line follows.
fn beam_justification(ui: &mut Ui, state: &mut AppState, id: cc_core::ElementId, b: &cc_core::Beam) {
    use cc_core::{BeamLateral, BeamVertical};
    let (mut lateral, mut vertical) = (b.lateral, b.vertical);
    row(ui, &t!("prop-lateral-justification"), |ui| {
        egui::ComboBox::from_id_salt(("beam-lateral", id.0)).selected_text(tr(lateral.key())).show_ui(ui, |ui| {
            for j in BeamLateral::ALL {
                ui.selectable_value(&mut lateral, j, tr(j.key()));
            }
        });
    });
    row(ui, &t!("prop-vertical-justification"), |ui| {
        egui::ComboBox::from_id_salt(("beam-vertical", id.0)).selected_text(tr(vertical.key())).show_ui(ui, |ui| {
            for j in BeamVertical::ALL {
                ui.selectable_value(&mut vertical, j, tr(j.key()));
            }
        });
    });
    if (lateral, vertical) != (b.lateral, b.vertical) {
        crate::engine_edit::element(state, id, None, |x| {
            if let ElementKind::Beam(b) = &mut x.kind {
                (b.lateral, b.vertical) = (lateral, vertical);
            }
        });
    }
}

/// A roof's covering and framing, from its type: the tiles and how many,
/// their weight, the roof's slope against the least the tiles take, and
/// the framing's lengths and volume.
fn roofing_rows(ui: &mut Ui, state: &mut AppState, el: &Element) {
    use cc_geom::roof_framing::MemberKind;
    let Some(cc_core::TypeData::Roof { covering, framing, .. }) =
        state.project().element_type(el.type_id).map(|t| &t.data)
    else {
        return;
    };
    if covering.is_none() && framing.system == cc_core::roofing::FramingSystem::None {
        return;
    }
    let covering = *covering;
    let Some(g) = state.session.geom().roof_framing.get(&el.id) else { return };
    section(ui, "roofing", t!("section-roofing"), |ui| {
        if let Some(kind) = covering {
            let spec = kind.spec();
            readonly_row(ui, &t!("prop-covering"), tr(kind.key()));
            let slope = g.slopes.map(|(lo, _)| lo);
            let ok = slope.is_some_and(|s| spec.accepts(s));
            let text = match slope {
                Some(s) => t!("value-slope-min", "slope" => format::slope(s), "min" => format::slope(spec.min_slope)),
                None => t!("msg-roof-flat-tiles"),
            };
            row(ui, &t!("prop-slope"), |ui| {
                let color = if ok { PALETTE.text } else { PALETTE.danger };
                ui.label(RichText::new(text).color(color)).on_hover_text(if ok {
                    String::new()
                } else {
                    t!("hint-roof-too-flat")
                })
            });
            if let Some(n) = spec.tiles(g.sloped_area) {
                readonly_row(ui, &t!("col-tile-count"), format!("{n:.0}"));
            }
            readonly_row(ui, &t!("col-weight"), format!("{:.0} kg", spec.weight_on(g.sloped_area)));
        }
        if !g.members.is_empty() {
            if g.trusses > 0 {
                readonly_row(ui, &t!("prop-truss-count"), g.trusses.to_string());
            }
            for (label, kind) in [
                ("prop-truss", MemberKind::Truss),
                ("prop-purlin", MemberKind::Purlin),
                ("prop-rafter", MemberKind::Rafter),
                ("prop-batten", MemberKind::Batten),
            ] {
                let length = g.length(kind);
                if length > 0.0 {
                    readonly_row(ui, &tr(label), format::length(length));
                }
            }
            readonly_row(ui, &t!("col-framing-volume"), format::volume(g.volume()));
        }
    });
}

/// Base and top constraints, location line and room bounding of a wall.
fn wall_constraints(ui: &mut Ui, state: &mut AppState, el: &Element, w: &cc_core::Wall) {
    use cc_core::{LocationLine, WallTop};
    let id = el.id;
    let set = |state: &mut AppState, field: &str, f: &dyn Fn(&mut cc_core::Wall)| {
        modify(state, id, field, |e| {
            if let ElementKind::Wall(w) = &mut e.kind {
                f(w);
            }
        });
    };

    let mut location = w.location_line;
    row(ui, &t!("prop-location-line"), |ui| {
        egui::ComboBox::from_id_salt(("loc", id.0)).selected_text(tr(location.key())).show_ui(ui, |ui| {
            for l in LocationLine::ALL {
                ui.selectable_value(&mut location, l, tr(l.key()));
            }
        });
    });
    if location != w.location_line {
        set(state, "loc", &|w| w.location_line = location);
        state.session.doc.break_merge();
    }
    for (label, at_start, value) in
        [(t!("prop-join-start"), true, w.join_start), (t!("prop-join-end"), false, w.join_end)]
    {
        let mut on = value;
        if row(ui, &label, |ui| ui.checkbox(&mut on, "")).changed() {
            crate::engine_edit::element(state, id, None, |x| {
                if let ElementKind::Wall(w) = &mut x.kind {
                    if at_start {
                        w.join_start = on;
                    } else {
                        w.join_end = on;
                    }
                }
            });
        }
    }

    level_row(ui, state, el);
    length_row(ui, state, &t!("prop-base-offset"), id, "base", w.base_offset, |e, v| {
        if let ElementKind::Wall(w) = &mut e.kind {
            w.base_offset = v;
        }
    });

    // Top: unconnected (explicit height) or attached to a level.
    let levels: Vec<_> = state.project().levels.iter().map(|l| (l.id, l.name.clone())).collect();
    let top_label = |top: WallTop| match top {
        WallTop::Unconnected => t!("top-unconnected"),
        WallTop::Level { level, .. } => {
            t!("top-up-to", "level" => levels.iter().find(|l| l.0 == level).map_or("?", |l| l.1.as_str()))
        }
    };
    let mut top_level = match w.top {
        WallTop::Level { level, .. } => Some(level),
        WallTop::Unconnected => None,
    };
    let before = top_level;
    row(ui, &t!("prop-top-constraint"), |ui| {
        egui::ComboBox::from_id_salt(("top", id.0)).selected_text(top_label(w.top)).show_ui(ui, |ui| {
            ui.selectable_value(&mut top_level, None, t!("top-unconnected"));
            for (lid, _) in &levels {
                let label = top_label(WallTop::Level { level: *lid, offset: 0.0 });
                ui.selectable_value(&mut top_level, Some(*lid), label);
            }
        });
    });
    if top_level != before {
        // Keep the wall's current top when switching to unconnected.
        let height = state.project().wall_height(el, w);
        set(state, "top", &|w| {
            w.top = match top_level {
                Some(level) => WallTop::Level { level, offset: 0.0 },
                None => WallTop::Unconnected,
            };
            if top_level.is_none() {
                w.height = height;
            }
        });
        state.session.doc.break_merge();
    }
    match w.top {
        WallTop::Level { level, offset } => {
            length_row(ui, state, &t!("prop-top-offset"), id, "top-offset", offset, move |e, v| {
                if let ElementKind::Wall(w) = &mut e.kind {
                    w.top = WallTop::Level { level, offset: v };
                }
            });
            let height = state.project().wall_height(el, w);
            readonly_row(ui, &t!("prop-unconnected-height"), format::length(height));
        }
        WallTop::Unconnected => {
            length_row(ui, state, &t!("prop-unconnected-height"), id, "height", w.height, |e, v| {
                if let ElementKind::Wall(w) = &mut e.kind {
                    w.height = v.max(0.1);
                }
            });
        }
    }

    let mut bounding = w.room_bounding;
    if row(ui, &t!("prop-room-bounding"), |ui| ui.checkbox(&mut bounding, "")).changed() {
        set(state, "bounding", &|w| w.room_bounding = bounding);
    }
}

fn roof_rows(ui: &mut Ui, state: &mut AppState, id: ElementId, r: &cc_core::Roof) {
    length_row(ui, state, &t!("prop-overhang"), id, "overhang", r.overhang, |e, v| {
        if let ElementKind::Roof(r) = &mut e.kind {
            r.overhang = v.max(0.0);
        }
    });
    // Openings (courtyards): sketch one more, or remove them all.
    let holes = r.holes.len();
    row(ui, &t!("prop-roof-openings"), |ui| {
        ui.label(holes.to_string());
        if ui.button(t!("action-roof-add-opening")).clicked() {
            state.set_tool(crate::tools::Tool::Opening { host: id, floor: false });
            state.message = t!("hint-roof-opening");
        }
        if holes > 0 && ui.button(t!("action-roof-remove-openings")).clicked() {
            crate::engine_edit::element(state, id, None, |e| {
                if let ElementKind::Roof(r) = &mut e.kind {
                    r.holes.clear();
                }
            });
        }
    });
    // Shape: 0 flat, 1 shed, 2 gable, 3 hip.
    let index = |shape: RoofShape| match shape {
        RoofShape::Flat => 0,
        RoofShape::Shed { .. } => 1,
        RoofShape::Gable { .. } => 2,
        RoofShape::Hip { .. } => 3,
    };
    let current = index(r.shape);
    let mut shape = current;
    let labels = [t!("roof-flat"), t!("roof-shed"), t!("roof-gable"), t!("roof-hip")];
    row(ui, &t!("prop-roof-shape"), |ui| {
        egui::ComboBox::from_id_salt(("roof-shape", id.0)).selected_text(labels[current].clone()).show_ui(ui, |ui| {
            for (i, label) in labels.iter().enumerate() {
                ui.selectable_value(&mut shape, i, label);
            }
        });
    });
    if shape != current {
        // Keep the slope when switching between pitched shapes; gables run along the longest edge.
        let slope = match r.shape {
            RoofShape::Shed { slope, .. } | RoofShape::Gable { slope, .. } | RoofShape::Hip { slope } => slope,
            RoofShape::Flat => 0.4,
        };
        let longest = r
            .footprint
            .iter()
            .zip(r.footprint.iter().cycle().skip(1))
            .map(|(a, b)| *b - *a)
            .max_by(|a, b| a.length().total_cmp(&b.length()))
            .map_or(0.0, |d| d.y.atan2(d.x).to_degrees());
        crate::engine_edit::element(state, id, None, |x| {
            if let ElementKind::Roof(r) = &mut x.kind {
                r.shape = match shape {
                    1 => RoofShape::Shed { slope, direction_deg: 90.0 },
                    2 => RoofShape::Gable { slope, ridge_deg: longest },
                    3 => RoofShape::Hip { slope },
                    _ => RoofShape::Flat,
                };
            }
        });
    }
    if let RoofShape::Gable { slope, ridge_deg } | RoofShape::Shed { slope, direction_deg: ridge_deg } = r.shape {
        let is_gable = matches!(r.shape, RoofShape::Gable { .. });
        let mut pct = slope * 100.0;
        let rs = row(ui, &t!("prop-slope"), |ui| {
            ui.add(DragValue::new(&mut pct).speed(0.5).range(0.0..=200.0).suffix(" %"))
        });
        let mut dir = ridge_deg;
        let label = if is_gable { t!("prop-ridge-direction") } else { t!("prop-direction") };
        let rd = row(ui, &label, |ui| ui.add(DragValue::new(&mut dir).speed(1.0).suffix("°")));
        if rs.changed() || rd.changed() {
            modify(state, id, "shape", |e| {
                if let ElementKind::Roof(r) = &mut e.kind {
                    let (slope, angle) = (pct / 100.0, dir.rem_euclid(360.0));
                    r.shape = if is_gable {
                        RoofShape::Gable { slope, ridge_deg: angle }
                    } else {
                        RoofShape::Shed { slope, direction_deg: angle }
                    };
                }
            });
        }
        finish(state, &rs);
        finish(state, &rd);
    }
    if let RoofShape::Hip { slope } = r.shape {
        let mut pct = slope * 100.0;
        let rs = row(ui, &t!("prop-slope"), |ui| {
            ui.add(DragValue::new(&mut pct).speed(0.5).range(0.0..=200.0).suffix(" %"))
        });
        if rs.changed() {
            modify(state, id, "shape", |e| {
                if let ElementKind::Roof(r) = &mut e.kind {
                    r.shape = RoofShape::Hip { slope: pct / 100.0 };
                }
            });
        }
        finish(state, &rs);
    }
}

/// One row per opening of a roof or floor, each with its area and a button that removes it.
fn opening_rows(ui: &mut Ui, state: &mut AppState, id: ElementId, openings: &[Vec<glam::DVec2>], floor: bool) {
    use cc_engine::commands::create::{FloorOpeningRemove, RoofOpeningRemove};
    let mut remove = None;
    for (i, h) in openings.iter().enumerate() {
        row(ui, &t!("prop-roof-opening", "n" => i + 1), |ui| {
            ui.label(format::area(cc_geom::polygon::area(h)));
            if ui.button(t!("action-remove")).clicked() {
                remove = Some(i);
            }
        });
    }
    match remove {
        Some(index) if floor => {
            crate::actions::run_command(state, FloorOpeningRemove { id: id.0, index });
        }
        Some(index) => {
            crate::actions::run_command(state, RoofOpeningRemove { id: id.0, index });
        }
        None => {}
    }
}

/// The slope of each outline edge of a gable or hip roof: the roof's own,
/// a gable end, or a slope of its own.
fn roof_edge_rows(ui: &mut Ui, state: &mut AppState, id: ElementId, r: &cc_core::Roof) {
    let (slope, ridge) = match r.shape {
        RoofShape::Gable { slope, ridge_deg } => (slope, Some(glam::DVec2::from_angle(ridge_deg.to_radians()))),
        RoofShape::Hip { slope } => (slope, None),
        _ => return,
    };
    let effective = cc_geom::roofs::edge_slopes(&r.footprint, slope, ridge, &r.edge_slopes);
    let n = r.footprint.len();
    // 0: the roof's slope, 1: a gable end, 2: a slope of its own.
    let labels = [t!("roof-edge-default"), t!("roof-edge-gable"), t!("roof-edge-custom")];
    for i in 0..n {
        let (a, b) = (r.footprint[i], r.footprint[(i + 1) % n]);
        let own = r.edge_slope(i);
        let current = match own {
            None => 0,
            Some(v) if v <= 0.0 => 1,
            Some(_) => 2,
        };
        let mut choice = current;
        let mut pct = effective.get(i).copied().unwrap_or(slope) * 100.0;
        let label = t!("prop-roof-edge", "n" => i + 1, "length" => format::length(a.distance(b)));
        let mut drag = None;
        row(ui, &label, |ui| {
            egui::ComboBox::from_id_salt(("roof-edge", id.0, i)).selected_text(labels[current].clone()).show_ui(
                ui,
                |ui| {
                    for (k, l) in labels.iter().enumerate() {
                        ui.selectable_value(&mut choice, k, l);
                    }
                },
            );
            if current == 2 {
                drag = Some(ui.add(DragValue::new(&mut pct).speed(0.5).range(0.1..=200.0).suffix(" %")));
            }
        });
        if choice != current {
            let slope = match choice {
                0 => None,
                1 => Some(0.0),
                _ => Some(if slope > 0.0 { slope } else { 0.4 }),
            };
            crate::actions::run_command(state, cc_engine::commands::create::RoofEdgeSet { id: id.0, edge: i, slope });
        } else if let Some(d) = drag {
            if d.changed() {
                modify(state, id, &format!("edge-{i}"), |e| {
                    if let ElementKind::Roof(r) = &mut e.kind {
                        if r.edge_slopes.len() <= i {
                            r.edge_slopes.resize(i + 1, None);
                        }
                        if let Some(s) = r.edge_slopes.get_mut(i) {
                            *s = Some((pct / 100.0).max(0.001));
                        }
                    }
                });
            }
            finish(state, &d);
        }
    }
}

fn view_text_row(
    ui: &mut Ui,
    state: &mut AppState,
    view: ViewId,
    label: &str,
    field: &str,
    value: String,
    set: impl FnOnce(&mut cc_core::View, String),
) {
    let (r, text) = text_row(ui, label, value);
    if r.changed() {
        crate::engine_edit::view(state, view, Some(key((view, field))), |v| set(v, text));
    }
    finish(state, &r);
}

fn view_props(ui: &mut Ui, state: &mut AppState, id: ViewId) {
    let Some(view) = state.project().view(id).cloned() else { return };
    padded(ui, |ui| {
        let category_name = tr(view.category().key());
        header_block(ui, crate::browser::category_icon(view.category()), &view.name, &category_name);
    });
    view_sections(ui, state, id);
}

/// The sections of a view's properties (also shown for a view placed on a
/// sheet, so it can be changed there).
fn view_sections(ui: &mut Ui, state: &mut AppState, id: ViewId) {
    let Some(view) = state.project().view(id).cloned() else { return };
    let graphical = !matches!(view.kind, ViewKind::Schedule { .. } | ViewKind::Sheet { .. });
    if graphical {
        section(ui, "graphics", t!("section-graphics"), |ui| {
            crate::graphics_dialog::view_rows(ui, state, id);
            let mut scale = view.scale;
            if crate::graphics_dialog::controlled(state, id, "scale") {
                readonly_row(ui, &t!("prop-scale"), format!("1:{scale}"));
            } else {
                row(ui, &t!("prop-scale"), |ui| {
                    egui::ComboBox::from_id_salt(("scale", id.0)).selected_text(format!("1:{scale}")).show_ui(
                        ui,
                        |ui| {
                            for s in SCALES {
                                ui.selectable_value(&mut scale, s, format!("1:{s}"));
                            }
                        },
                    );
                });
            }
            if scale != view.scale {
                crate::engine_edit::view(state, id, None, |v| v.scale = scale);
            }
            if let ViewKind::ThreeD { analytical, .. } = view.kind {
                let mut on = analytical;
                if row(ui, &t!("prop-analytical"), |ui| ui.checkbox(&mut on, "")).changed() {
                    crate::engine_edit::view(state, id, None, |v| {
                        if let ViewKind::ThreeD { analytical, .. } = &mut v.kind {
                            *analytical = on;
                        }
                    });
                }
                let clicked = row(ui, "", |ui| ui.button(t!("action-zoom-fit")).clicked());
                if clicked && let Some(v3) = state.view3d.get_mut(&id) {
                    v3.request_fit();
                }
            }
        });
    }

    match &view.kind {
        ViewKind::FloorPlan { level, cut_height, .. } | ViewKind::CeilingPlan { level, cut_height } => {
            if let ViewKind::FloorPlan { structural, structure_tags, .. } = view.kind {
                let locked = crate::graphics_dialog::controlled(state, id, "structural");
                section(ui, "structure-display", t!("section-structural"), |ui| {
                    let (mut s, mut tags) = (structural, structure_tags);
                    let a = row(ui, &t!("prop-structural-plan"), |ui| {
                        ui.add_enabled(!locked, egui::Checkbox::without_text(&mut s))
                    })
                    .changed();
                    let b = row(ui, &t!("prop-structure-tags"), |ui| {
                        ui.add_enabled(!locked, egui::Checkbox::without_text(&mut tags))
                    })
                    .changed();
                    if a || b {
                        crate::engine_edit::view(state, id, None, |x| {
                            if let ViewKind::FloorPlan { structural, structure_tags, .. } = &mut x.kind {
                                (*structural, *structure_tags) = (s, tags);
                            }
                        });
                    }
                });
            }
            section(ui, "extents", t!("section-extents"), |ui| {
                readonly_row(ui, &t!("prop-level"), state.project().level_name(*level).to_string());
                let mut h = *cut_height;
                let locked = crate::graphics_dialog::controlled(state, id, "cut_height");
                let r = row(ui, &t!("prop-cut-height"), |ui| {
                    ui.add_enabled(!locked, crate::properties::length_drag(&mut h).speed(0.01).range(0.0..=20.0))
                });
                if r.changed() {
                    crate::engine_edit::view(state, id, Some(key((id, "cut"))), |x| {
                        if let Some(ViewKind::FloorPlan { cut_height, .. } | ViewKind::CeilingPlan { cut_height, .. }) =
                            Some(&mut x.kind)
                        {
                            *cut_height = h;
                        }
                    });
                }
                finish(state, &r);
            });
        }
        ViewKind::Section { depth, .. } => {
            section(ui, "extents", t!("section-extents"), |ui| {
                let mut d = *depth;
                let r = row(ui, &t!("prop-depth"), |ui| {
                    ui.add(crate::properties::length_drag(&mut d).speed(0.05).range(0.1..=500.0))
                });
                if r.changed() {
                    crate::engine_edit::view(state, id, Some(key((id, "depth"))), |x| {
                        if let ViewKind::Section { depth, .. } = &mut x.kind {
                            *depth = d;
                        }
                    });
                }
                finish(state, &r);
                if row(ui, "", |ui| ui.button(t!("prop-flip-section")).clicked()) {
                    crate::engine_edit::view(state, id, None, |x| {
                        if let ViewKind::Section { start, end, .. } = &mut x.kind {
                            std::mem::swap(start, end);
                        }
                    });
                }
            });
        }
        ViewKind::Schedule { category, .. } => {
            section(ui, "graphics", t!("section-graphics"), |ui| {
                readonly_row(ui, &t!("prop-category"), actions::t_category(*category));
            });
        }
        ViewKind::Sheet { number, size, .. } => {
            section(ui, "sheet", t!("section-graphics"), |ui| {
                view_text_row(ui, state, id, &t!("prop-sheet-number"), "number", number.clone(), |v, s| {
                    if let ViewKind::Sheet { number, .. } = &mut v.kind {
                        *number = s;
                    }
                });
                let mut sz = *size;
                row(ui, &t!("prop-sheet-size"), |ui| {
                    egui::ComboBox::from_id_salt(("size", id.0)).selected_text(sz.label()).show_ui(ui, |ui| {
                        for s in SheetSize::ALL {
                            ui.selectable_value(&mut sz, s, s.label());
                        }
                    });
                });
                if sz != *size {
                    crate::engine_edit::view(state, id, None, |x| {
                        if let ViewKind::Sheet { size, .. } = &mut x.kind {
                            *size = sz;
                        }
                    });
                    if let Some(st) = state.view2d.get_mut(&id) {
                        st.fitted = false;
                    }
                }
            });
        }
        ViewKind::ThreeD { .. } | ViewKind::Legend { .. } | ViewKind::Elevation { .. } | ViewKind::Diagram { .. } => {}
    }
    crate::phases_dialog::view_section(ui, state, id);

    section(ui, "identity", t!("section-identity"), |ui| {
        view_text_row(ui, state, id, &t!("prop-name"), "name", view.name.clone(), |v, s| v.name = s);
    });

    match &view.kind {
        ViewKind::Legend { types } => {
            section(ui, "legend-types", t!("prop-legend-types"), |ui| legend_types(ui, state, id, types));
        }
        ViewKind::Sheet { viewports, .. } => {
            section(ui, "sheet-views", t!("prop-sheet-views"), |ui| sheet_views(ui, state, id, viewports));
        }
        _ => {}
    }
}

fn legend_types(ui: &mut Ui, state: &mut AppState, id: ViewId, current: &[cc_core::ElementTypeId]) {
    let all: Vec<_> = state.project().types.values().map(|t| (t.id, t.name.clone())).collect();
    for (tid, name) in all {
        let mut on = current.contains(&tid);
        if row(ui, &name, |ui| ui.checkbox(&mut on, "")).changed() {
            crate::engine_edit::view(state, id, None, |x| {
                if let ViewKind::Legend { types } = &mut x.kind {
                    if on {
                        types.push(tid);
                    } else {
                        types.retain(|t| *t != tid);
                    }
                }
            });
        }
    }
}

fn sheet_views(ui: &mut Ui, state: &mut AppState, sheet: ViewId, viewports: &[cc_core::Viewport]) {
    padded(ui, |ui| {
        for (i, vp) in viewports.iter().enumerate() {
            let name = state.project().view(vp.view).map(|v| v.name.clone()).unwrap_or_default();
            if ui.selectable_label(state.sheet_selection == Some((sheet, i)), name).clicked() {
                state.sheet_selection = Some((sheet, i));
            }
        }
        let candidates: Vec<_> = state
            .project()
            .views
            .values()
            .filter(|v| v.placeable() && actions::sheet_of(state.project(), v.id).is_none())
            .map(|v| (v.id, state.view_title(v.id)))
            .collect();
        ui.menu_button(t!("prop-add-view"), |ui| {
            if candidates.is_empty() {
                ui.label(RichText::new(t!("prop-no-views")).weak());
            }
            for (vid, title) in candidates {
                if ui.button(title).clicked() {
                    actions::place_on_sheet(state, sheet, vid);
                    ui.close();
                }
            }
        });
    });
}

fn viewport_props(ui: &mut Ui, state: &mut AppState, sheet: ViewId, index: usize) {
    let Some(ViewKind::Sheet { viewports, .. }) = state.project().view(sheet).map(|v| v.kind.clone()) else { return };
    let Some(vp) = viewports.get(index).cloned() else {
        state.sheet_selection = None;
        return;
    };
    padded(ui, |ui| header_block(ui, Icon::Sheet, &state.view_title(vp.view), &t!("props-viewport")));
    section(ui, "position", t!("section-position"), |ui| {
        for (label, axis) in [(t!("prop-center-x"), 0), (t!("prop-center-y"), 1)] {
            let mut v = vp.center[axis];
            let r = row(ui, &label, |ui| ui.add(DragValue::new(&mut v).speed(1.0).fixed_decimals(1).suffix(" mm")));
            if r.changed() {
                crate::engine_edit::view(state, sheet, Some(key((sheet, index, axis))), |x| {
                    if let ViewKind::Sheet { viewports, .. } = &mut x.kind
                        && let Some(vp) = viewports.get_mut(index)
                    {
                        vp.center[axis] = v;
                    }
                });
            }
            finish(state, &r);
        }
    });
    padded(ui, |ui| {
        ui.horizontal(|ui| {
            if ui.button(t!("browser-open")).clicked() {
                state.open_view(vp.view);
            }
            if ui.button(t!("prop-remove-from-sheet")).clicked() {
                actions::remove_viewport(state, sheet, index);
            }
        });
    });
    // The placed view's own properties: its scale, graphics, crop and phase
    // change here, and the sheet follows.
    view_sections(ui, state, vp.view);
}
