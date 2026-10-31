//! Type Properties dialog: switch, duplicate and rename types, edit the
//! compound structure (layers) and the type parameters in sections.

use crate::actions;
use crate::i18n::tr;
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use crate::theme::{self, PALETTE};
use cc_core::{ElementType, ElementTypeId, Layer, LayerFunction, TypeData, TypeInfo, WallFunction};
use cc_views::format;
use egui::{Context, DragValue, RichText, Ui};
use std::hash::{Hash, Hasher};

fn key(parts: impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut h);
    h.finish()
}

fn finish(state: &mut AppState, r: &egui::Response) {
    if r.drag_stopped() || r.lost_focus() {
        state.session.doc.break_merge();
    }
}

/// Apply `f` to type `tid`, merging consecutive edits of the same field.
fn update(state: &mut AppState, tid: ElementTypeId, field: &str, f: impl FnOnce(&mut ElementType)) {
    crate::engine_edit::element_type(state, tid, Some(key((tid, field))), f);
}

pub fn show(ctx: &Context, state: &mut AppState) {
    let Some(tid) = state.dialogs.type_editor else { return };
    let Some(ty) = state.project().element_type(tid).cloned() else {
        state.dialogs.type_editor = None;
        return;
    };
    crate::dialog::session::begin(state);
    let mut ok = 0;
    let max_height = (ctx.content_rect().height() - 320.0).clamp(240.0, 560.0);
    let response = crate::dialog::Dialog::new("type-editor", t!("dlg-type")).width(580.0).show(
        ctx,
        |ui| {
            header(ui, state, &ty);
            ui.add_space(6.0);
            ui.label(theme::muted(t!("type-parameters")).size(12.0));
            egui::Frame::new().stroke(egui::Stroke::new(1.0, PALETTE.border)).show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(max_height).auto_shrink([false, true]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    parameters(ui, state, &ty);
                });
            });
        },
        |f| {
            ok = f.primary(t!("action-ok"));
            f.secondary(t!("action-cancel"));
        },
    );
    // While renaming, Esc only cancels the rename.
    let renaming = state.dialogs.type_rename.is_some();
    let response = crate::dialog::DialogResponse { dismissed: response.dismissed && !renaming, ..response };
    // OK keeps a name still being typed.
    if response.clicked == Some(ok)
        && let Some(name) = state.dialogs.type_rename.take()
    {
        rename(state, &ty, &name);
    }
    if crate::dialogs::ok_cancel(state, &response, ok) {
        state.dialogs.type_editor = None;
        state.dialogs.type_rename = None;
    }
}

/// Family (category), type selector and the Duplicate / Rename buttons.
fn header(ui: &mut Ui, state: &mut AppState, ty: &ElementType) {
    let tid = ty.id;
    let category = ty.category();
    egui::Grid::new("type-header").num_columns(3).striped(false).spacing([10.0, 8.0]).show(ui, |ui| {
        ui.label(theme::muted(t!("type-category")));
        ui.label(RichText::new(actions::t_category(category)).color(PALETTE.text));
        ui.label("");
        ui.end_row();

        ui.label(theme::muted(t!("type-type")));
        let button_w = 110.0;
        let field_w = (ui.available_width() - button_w - 12.0).max(200.0);
        match state.dialogs.type_rename.take() {
            Some(mut name) => {
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(field_w));
                // Focus once: the user may then click another field, which
                // confirms the name.
                let starting = std::mem::take(&mut state.dialogs.type_rename_focus);
                if starting {
                    r.request_focus();
                }
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if !escape {
                    if enter || (!starting && !r.has_focus()) {
                        rename(state, ty, &name);
                    } else {
                        state.dialogs.type_rename = Some(name);
                    }
                }
            }
            None => {
                let mut selected = tid;
                let types: Vec<_> = state.project().types_of(category).map(|t| (t.id, t.name.clone())).collect();
                egui::ComboBox::from_id_salt("type-switch").width(field_w).selected_text(ty.name.clone()).show_ui(
                    ui,
                    |ui| {
                        for (id, name) in types {
                            ui.selectable_value(&mut selected, id, name);
                        }
                    },
                );
                if selected != tid {
                    state.dialogs.type_editor = Some(selected);
                }
            }
        }
        ui.vertical(|ui| {
            if ui.add_sized([button_w, 24.0], egui::Button::new(t!("type-duplicate"))).clicked() {
                duplicate(state, ty);
            }
            if ui.add_sized([button_w, 24.0], egui::Button::new(t!("type-rename"))).clicked() {
                state.dialogs.type_rename = Some(ty.name.clone());
                state.dialogs.type_rename_focus = true;
            }
        });
        ui.end_row();
    });
}

/// Copy the type under a new name; selected instances of it switch to the copy.
fn duplicate(state: &mut AppState, ty: &ElementType) {
    let selection: Vec<u64> = state.selection.iter().map(|e| e.0).collect();
    let name = t!("copy-of", "name" => ty.name);
    let command = cc_engine::commands::types::TypeDuplicate {
        type_ref: cc_engine::Ref::Id(ty.id.0),
        name: Some(name.clone()),
        elements: selection,
    };
    let Some(new_id) = crate::actions::run_command(state, command).and_then(|o| o["id"].as_u64()) else { return };
    state.dialogs.type_editor = Some(ElementTypeId(new_id));
    state.dialogs.type_rename = Some(name);
    state.dialogs.type_rename_focus = true;
}

/// Give type `ty` the name typed in the rename field.
fn rename(state: &mut AppState, ty: &ElementType, name: &str) {
    let name = name.trim().to_string();
    if !name.is_empty() && name != ty.name {
        crate::engine_edit::element_type(state, ty.id, None, |t| t.name = name);
    }
}

fn length_row(
    ui: &mut Ui,
    state: &mut AppState,
    label: &str,
    tid: ElementTypeId,
    field: &str,
    value: f64,
    set: impl FnOnce(&mut TypeData, f64),
) {
    let mut v = value;
    let r = row(ui, label, |ui| ui.add(crate::properties::length_drag(&mut v).speed(0.005).range(0.001..=20.0)));
    if r.changed() {
        update(state, tid, field, |t| set(&mut t.data, v));
    }
    finish(state, &r);
}

/// Component name and kind, the editor button and this type's parameter values.
fn component_parameters(ui: &mut Ui, state: &mut AppState, ty: &ElementType) {
    let TypeData::Component { component, def } = &ty.data else { return };
    let tid = ty.id;
    section(ui, "type-component", t!("section-component"), |ui| {
        readonly_row(ui, &t!("col-component"), component.clone());
        readonly_row(ui, &t!("col-component-kind"), tr(def.kind.key()));
        let (edit, quick, save) = row(ui, &t!("comp-definition"), |ui| {
            ui.horizontal(|ui| {
                (
                    ui.button(t!("action-edit-component")).clicked(),
                    ui.button(t!("action-quick-edit-component")).clicked(),
                    ui.button(t!("action-save-component")).clicked(),
                )
            })
            .inner
        });
        if edit {
            crate::component_env::open(state, tid);
        }
        if quick {
            state.dialogs.component_editor = crate::component_editor::Editor::open(state, tid);
        }
        if save {
            actions::save_component(state, component);
        }
    });
    section(ui, "type-comp-params", t!("comp-params"), |ui| {
        for (i, p) in def.params.iter().enumerate() {
            let mut v = p.value;
            let label = if p.instance { format!("{} ({})", p.name, t!("comp-param-instance")) } else { p.name.clone() };
            let r = row(ui, &label, |ui| ui.add(crate::properties::param_value(&mut v, p.kind)));
            if r.changed() {
                update(state, tid, &format!("comp-param-{i}"), |t| {
                    if let TypeData::Component { def, .. } = &mut t.data
                        && let Some(p) = def.params.get_mut(i)
                    {
                        p.value = v;
                    }
                });
            }
            finish(state, &r);
        }
    });
}

fn parameters(ui: &mut Ui, state: &mut AppState, ty: &ElementType) {
    let tid = ty.id;
    if matches!(ty.data, TypeData::Component { .. }) {
        component_parameters(ui, state, ty);
        identity(ui, state, tid, &ty.info);
        return;
    }
    let layered = !ty.layers().is_empty();

    section(ui, "type-construction", t!("section-construction"), |ui| {
        if layered {
            let editing = state.dialogs.type_layers_open;
            let label = if editing { t!("action-hide") } else { t!("action-edit") };
            if row(ui, &t!("prop-structure"), |ui| ui.button(label).clicked()) {
                state.dialogs.type_layers_open = !editing;
            }
            if state.dialogs.type_layers_open {
                layers_editor(ui, state, ty);
            }
        }
        if let Some(function) = ty.wall_function() {
            let mut f = function;
            row(ui, &t!("prop-function"), |ui| {
                egui::ComboBox::from_id_salt("wall-function").selected_text(tr(f.key())).show_ui(ui, |ui| {
                    for option in WallFunction::ALL {
                        ui.selectable_value(&mut f, option, tr(option.key()));
                    }
                });
            });
            if f != function {
                update(state, tid, "function", |t| {
                    if let TypeData::Wall { function, .. } = &mut t.data {
                        *function = f;
                    }
                });
            }
        }
        match ty.data {
            TypeData::Ceiling { thickness, .. } => {
                length_row(ui, state, &t!("prop-thickness"), tid, "thk", thickness, |d, v| {
                    if let TypeData::Ceiling { thickness, .. } = d {
                        *thickness = v;
                    }
                });
            }
            _ if layered => {
                readonly_row(ui, &t!("prop-thickness"), format::length(ty.thickness()));
            }
            _ => {}
        }
    });

    structure_parameters(ui, state, ty);

    if let TypeData::Door { width, height, .. } | TypeData::Window { width, height, .. } = ty.data {
        section(ui, "type-dimensions", t!("section-dimensions"), |ui| {
            length_row(ui, state, &t!("col-width"), tid, "w", width, |d, v| {
                if let TypeData::Door { width, .. } | TypeData::Window { width, .. } = d {
                    *width = v.max(0.1);
                }
            });
            length_row(ui, state, &t!("col-height"), tid, "h", height, |d, v| {
                if let TypeData::Door { height, .. } | TypeData::Window { height, .. } = d {
                    *height = v.max(0.1);
                }
            });
            if let TypeData::Window { default_sill, .. } = ty.data {
                length_row(ui, state, &t!("prop-sill"), tid, "sill", default_sill, |d, v| {
                    if let TypeData::Window { default_sill, .. } = d {
                        *default_sill = v.max(0.0);
                    }
                });
            }
        });
    }

    if let TypeData::Door { material, .. }
    | TypeData::Window { material, .. }
    | TypeData::Ceiling { material, .. }
    | TypeData::Column { material, .. }
    | TypeData::Beam { material, .. }
    | TypeData::Foundation { material, .. }
    | TypeData::Stair { material, .. }
    | TypeData::Ramp { material, .. }
    | TypeData::Pipe { material, .. }
    | TypeData::Conduit { material, .. }
    | TypeData::Railing { material, .. } = &ty.data
    {
        section(ui, "type-materials", t!("section-materials"), |ui| {
            let mut m = material.clone();
            let changed =
                row(ui, &t!("prop-material"), |ui| crate::dialogs::material_combo(ui, state, "type-material", &mut m));
            if changed {
                update(state, tid, "mat", |t| match &mut t.data {
                    TypeData::Door { material, .. }
                    | TypeData::Window { material, .. }
                    | TypeData::Ceiling { material, .. }
                    | TypeData::Column { material, .. }
                    | TypeData::Beam { material, .. }
                    | TypeData::Foundation { material, .. }
                    | TypeData::Stair { material, .. }
                    | TypeData::Ramp { material, .. }
                    | TypeData::Pipe { material, .. }
                    | TypeData::Conduit { material, .. }
                    | TypeData::Railing { material, .. } => *material = m,
                    _ => {}
                });
            }
        });
    }

    if let TypeData::Roof { covering, framing, .. } = &ty.data {
        section(ui, "type-roofing", t!("section-roofing"), |ui| roofing(ui, state, tid, *covering, framing));
    }

    section(ui, "type-identity", t!("section-identity"), |ui| identity(ui, state, tid, &ty.info));

    section(ui, "type-ifc", t!("section-ifc"), |ui| {
        readonly_row(ui, &t!("prop-ifc-guid"), cc_ifc::guid::encode(ty.uid.0));
    });
}

/// A roof type's covering (from the catalog, with its least slope and
/// weight) and framing: the system, material, and the members' sizes and
/// spacings (rafters and battens only for timber).
fn roofing(
    ui: &mut Ui,
    state: &mut AppState,
    tid: ElementTypeId,
    covering: Option<cc_core::roofing::TileKind>,
    framing: &cc_core::roofing::RoofFraming,
) {
    use cc_core::roofing::{FramingSystem, MemberSize, RoofFraming, TileKind};
    let set = |state: &mut AppState, field: &str, f: &dyn Fn(&mut Option<TileKind>, &mut RoofFraming)| {
        update(state, tid, field, |t| {
            if let TypeData::Roof { covering, framing, .. } = &mut t.data {
                f(covering, framing);
            }
        });
    };
    let mut c = covering;
    let name = |c: Option<TileKind>| c.map_or_else(|| t!("tile-none"), |k| tr(k.key()));
    row(ui, &t!("prop-covering"), |ui| {
        egui::ComboBox::from_id_salt(("covering", tid.0)).selected_text(name(c)).show_ui(ui, |ui| {
            ui.selectable_value(&mut c, None, name(None));
            for k in TileKind::ALL {
                ui.selectable_value(&mut c, Some(k), name(Some(k)));
            }
        });
    });
    if c != covering {
        // `type_set` brings the framing the covering usually lies on, if none was chosen.
        set(state, "covering", &|cov, _| *cov = c);
    }
    if let Some(spec) = covering.map(TileKind::spec) {
        readonly_row(ui, &t!("prop-min-slope"), format::slope(spec.min_slope));
        readonly_row(ui, &t!("prop-covering-weight"), format!("{:.0} kg/m²", spec.weight));
        if let Some(n) = spec.per_m2 {
            readonly_row(ui, &t!("prop-tiles-per-m2"), format!("{n:.1}"));
        }
    }
    let mut system = framing.system;
    row(ui, &t!("prop-framing"), |ui| {
        egui::ComboBox::from_id_salt(("framing", tid.0)).selected_text(tr(system.key())).show_ui(ui, |ui| {
            for s in FramingSystem::ALL {
                ui.selectable_value(&mut system, s, tr(s.key()));
            }
        });
    });
    if system != framing.system {
        set(state, "framing", &|_, fr| fr.system = system);
    }
    if framing.system == FramingSystem::None {
        return;
    }
    let mut m = framing.material.clone();
    if row(ui, &t!("prop-material"), |ui| crate::dialogs::material_combo(ui, state, ("framing-mat", tid.0), &mut m)) {
        set(state, "framing-mat", &|_, fr| fr.material = m.clone());
    }
    type Pick = fn(&mut RoofFraming) -> &mut MemberSize;
    let timber = matches!(framing.system, FramingSystem::Timber | FramingSystem::TimberTrusses);
    let trusses = framing.system == FramingSystem::TimberTrusses;
    let mut members: Vec<(&str, Pick)> = Vec::new();
    if trusses {
        members.push(("prop-truss", |f| &mut f.truss));
    }
    members.push(("prop-purlin", |f| &mut f.purlin));
    if timber {
        members.push(("prop-rafter", |f| &mut f.rafter));
        members.push(("prop-batten", |f| &mut f.batten));
    }
    for (label, pick) in members {
        let size = *pick(&mut framing.clone());
        for (axis, value) in [("w", size.width), ("h", size.height)] {
            let caption = format!("{} ({})", tr(label), if axis == "w" { t!("col-width") } else { t!("col-height") });
            length_row(ui, state, &caption, tid, &format!("{label}-{axis}"), value, |d, v| {
                if let TypeData::Roof { framing, .. } = d {
                    let s = pick(framing);
                    if axis == "w" {
                        s.width = v.max(0.005);
                    } else {
                        s.height = v.max(0.005);
                    }
                }
            });
        }
    }
    if trusses {
        length_row(ui, state, &t!("prop-truss-spacing"), tid, "truss-sp", framing.truss_spacing, |d, v| {
            if let TypeData::Roof { framing, .. } = d {
                framing.truss_spacing = v.max(0.3);
            }
        });
    }
    length_row(ui, state, &t!("prop-purlin-spacing"), tid, "purlin-sp", framing.purlin_spacing, |d, v| {
        if let TypeData::Roof { framing, .. } = d {
            framing.purlin_spacing = v.max(0.1);
        }
    });
    if timber {
        length_row(ui, state, &t!("prop-rafter-spacing"), tid, "rafter-sp", framing.rafter_spacing, |d, v| {
            if let TypeData::Roof { framing, .. } = d {
                framing.rafter_spacing = v.max(0.1);
            }
        });
    }
}

/// Text fields of the type information.
fn identity(ui: &mut Ui, state: &mut AppState, tid: ElementTypeId, info: &TypeInfo) {
    type Field = (&'static str, fn(&mut TypeInfo) -> &mut String);
    let fields: [Field; 9] = [
        ("info-type-mark", |i| &mut i.type_mark),
        ("info-description", |i| &mut i.description),
        ("info-manufacturer", |i| &mut i.manufacturer),
        ("info-model", |i| &mut i.model),
        ("info-url", |i| &mut i.url),
        ("info-keynote", |i| &mut i.keynote),
        ("info-classification", |i| &mut i.classification),
        ("info-fire-rating", |i| &mut i.fire_rating),
        ("info-type-comments", |i| &mut i.comments),
    ];
    for (label, field) in fields {
        let mut text = field(&mut info.clone()).clone();
        let r = row(ui, &tr(label), |ui| {
            crate::table::live_text(ui, ("type-info", tid, label), &mut text, |ui, t| ui.text_edit_singleline(t))
        });
        if r.changed() {
            update(state, tid, label, |t| *field(&mut t.info) = text);
        }
        finish(state, &r);
    }
    let mut cost = info.cost.unwrap_or(0.0);
    let r = row(ui, &t!("info-cost"), |ui| {
        ui.add(DragValue::new(&mut cost).speed(1.0).range(0.0..=f64::MAX).fixed_decimals(2))
    });
    if r.changed() {
        update(state, tid, "cost", |t| t.info.cost = Some(cost));
    }
    finish(state, &r);
}

/// Layers table: function, material and thickness, with reorder and removal.
fn layers_editor(ui: &mut Ui, state: &mut AppState, ty: &ElementType) {
    let tid = ty.id;
    let layers = ty.layers().to_vec();
    let set_layers = |state: &mut AppState, field: &str, layers: Vec<Layer>| {
        update(state, tid, field, |t| match &mut t.data {
            TypeData::Wall { layers: l, .. } | TypeData::Floor { layers: l } | TypeData::Roof { layers: l, .. } => {
                *l = layers
            }
            _ => {}
        });
    };
    egui::Frame::new().fill(PALETTE.panel).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
        ui.label(theme::muted(t!("type-layers")).size(12.0));
        egui::Grid::new(("layers", tid.0)).num_columns(5).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
            for h in ["", "col-function", "prop-material", "prop-thickness", ""] {
                ui.label(theme::muted(if h.is_empty() { String::new() } else { tr(h) }).size(11.5));
            }
            ui.end_row();
            for (i, layer) in layers.iter().enumerate() {
                ui.label(theme::muted((i + 1).to_string()));
                let mut f = layer.function;
                egui::ComboBox::from_id_salt(("lfn", tid.0, i)).width(120.0).selected_text(tr(f.key())).show_ui(
                    ui,
                    |ui| {
                        for option in LayerFunction::ALL {
                            ui.selectable_value(&mut f, option, tr(option.key()));
                        }
                    },
                );
                if f != layer.function {
                    let mut l = layers.clone();
                    l[i].function = f;
                    set_layers(state, "lfn", l);
                    state.session.doc.break_merge();
                }
                let mut m = layer.material.clone();
                if crate::dialogs::material_combo(ui, state, ("lmat", tid.0, i), &mut m) {
                    let mut l = layers.clone();
                    l[i].material = m;
                    set_layers(state, "lmat", l);
                    state.session.doc.break_merge();
                }
                let mut thk = layer.thickness;
                let r = ui.add(crate::properties::length_drag(&mut thk).speed(0.002).range(0.001..=2.0));
                if r.changed() {
                    let mut l = layers.clone();
                    l[i].thickness = thk;
                    set_layers(state, &format!("lthk{i}"), l);
                }
                finish(state, &r);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let n = layers.len();
                    let moves = [("↑", i > 0, i.wrapping_sub(1), "layer-up"), ("↓", i + 1 < n, i + 1, "layer-down")];
                    for (glyph, enabled, target, tip) in moves {
                        if ui.add_enabled(enabled, egui::Button::new(glyph).small()).on_hover_text(tr(tip)).clicked() {
                            let mut l = layers.clone();
                            l.swap(i, target);
                            set_layers(state, "lmove", l);
                            state.session.doc.break_merge();
                        }
                    }
                    let remove =
                        ui.add_enabled(n > 1, egui::Button::new("×").small()).on_hover_text(t!("layer-remove"));
                    if remove.clicked() {
                        let mut l = layers.clone();
                        l.remove(i);
                        set_layers(state, "lremove", l);
                        state.session.doc.break_merge();
                    }
                });
                ui.end_row();
            }
        });
        ui.horizontal(|ui| {
            if ui.button(t!("type-add-layer")).clicked() {
                let mut l = layers.clone();
                let material = state.project().materials.first().map(|m| m.name.clone()).unwrap_or_default();
                l.push(Layer { material, thickness: 0.02, function: LayerFunction::Finish });
                set_layers(state, "ladd", l);
                state.session.doc.break_merge();
            }
            let total = format!("{}: {}", t!("type-total-thickness"), format::length(ty.thickness()));
            ui.label(theme::muted(total));
        });
    });
}

/// Section and layout parameters of columns, beams, stairs, ramps and railings.
fn structure_parameters(ui: &mut Ui, state: &mut AppState, ty: &ElementType) {
    let tid = ty.id;
    match ty.data {
        TypeData::Column { section, .. } => {
            section_rows(ui, state, tid, section, false);
        }
        TypeData::Beam { width, height, shape, .. } => {
            section_rows(ui, state, tid, cc_core::ColumnSection::Profile { width, depth: height, shape }, true);
        }
        TypeData::Foundation { kind, width, length, thickness, .. } => {
            crate::table::section(ui, "type-dimensions", t!("section-dimensions"), |ui| {
                let mut k = kind;
                row(ui, &t!("prop-foundation-kind"), |ui| {
                    egui::ComboBox::from_id_salt(("foundation-kind", tid.0)).selected_text(tr(k.key())).show_ui(
                        ui,
                        |ui| {
                            for option in cc_core::FoundationKind::ALL {
                                ui.selectable_value(&mut k, option, tr(option.key()));
                            }
                        },
                    );
                });
                if k != kind {
                    update(state, tid, "fk", |t| {
                        if let TypeData::Foundation { kind, .. } = &mut t.data {
                            *kind = k;
                        }
                    });
                    state.session.doc.break_merge();
                }
                if kind != cc_core::FoundationKind::Slab {
                    length_row(ui, state, &t!("col-width"), tid, "fw", width, |d, v| {
                        if let TypeData::Foundation { width, .. } = d {
                            *width = v.max(0.05);
                        }
                    });
                }
                if kind == cc_core::FoundationKind::Pad {
                    length_row(ui, state, &t!("prop-length"), tid, "fl", length, |d, v| {
                        if let TypeData::Foundation { length, .. } = d {
                            *length = v.max(0.05);
                        }
                    });
                }
                length_row(ui, state, &t!("col-thickness"), tid, "ft", thickness, |d, v| {
                    if let TypeData::Foundation { thickness, .. } = d {
                        *thickness = v.max(0.05);
                    }
                });
            });
        }
        TypeData::Stair { width, max_riser, tread, build, rules, .. } => {
            crate::table::section(ui, "type-dimensions", t!("section-dimensions"), |ui| {
                length_row(ui, state, &t!("col-width"), tid, "sw", width, |d, v| {
                    if let TypeData::Stair { width, .. } = d {
                        *width = v.max(0.3);
                    }
                });
                length_row(ui, state, &t!("prop-max-riser"), tid, "sr", max_riser, |d, v| {
                    if let TypeData::Stair { max_riser, .. } = d {
                        *max_riser = v.clamp(0.05, 0.5);
                    }
                });
                length_row(ui, state, &t!("col-tread"), tid, "st", tread, |d, v| {
                    if let TypeData::Stair { tread, .. } = d {
                        *tread = v.clamp(0.1, 1.0);
                    }
                });
            });
            crate::table::section(ui, "type-stair-build", t!("section-stair-build"), |ui| {
                let mut b = build;
                let edited = crate::stair_ui::build_rows(ui, tid.0, &mut b);
                if edited.changed {
                    update(state, tid, "sb", |t| {
                        if let TypeData::Stair { build, .. } = &mut t.data {
                            *build = b;
                        }
                    });
                }
                if edited.done {
                    state.session.doc.break_merge();
                }
            });
            crate::table::section(ui, "type-stair-rules", t!("section-stair-rules"), |ui| {
                let mut r = rules;
                let edited = crate::stair_ui::rules_rows(ui, tid.0, &mut r, max_riser, tread);
                if edited.changed {
                    update(state, tid, "sq", |t| {
                        if let TypeData::Stair { rules, .. } = &mut t.data {
                            *rules = r;
                        }
                    });
                }
                if edited.done {
                    state.session.doc.break_merge();
                }
            });
        }
        TypeData::Pipe { diameter, .. } | TypeData::Conduit { diameter, .. } => {
            crate::table::section(ui, "type-dimensions", t!("section-dimensions"), |ui| {
                length_row(ui, state, &t!("col-diameter"), tid, "pd", diameter, |d, v| {
                    if let TypeData::Pipe { diameter, .. } | TypeData::Conduit { diameter, .. } = d {
                        *diameter = v.clamp(0.005, 2.0);
                    }
                });
            });
        }
        TypeData::Ramp { width, thickness, rules, .. } => {
            crate::table::section(ui, "type-dimensions", t!("section-dimensions"), |ui| {
                length_row(ui, state, &t!("col-width"), tid, "rw", width, |d, v| {
                    if let TypeData::Ramp { width, .. } = d {
                        *width = v.clamp(0.3, 50.0);
                    }
                });
                length_row(ui, state, &t!("col-thickness"), tid, "rt", thickness, |d, v| {
                    if let TypeData::Ramp { thickness, .. } = d {
                        *thickness = v.clamp(0.01, 5.0);
                    }
                });
            });
            crate::table::section(ui, "type-ramp-rules", t!("section-ramp-rules"), |ui| {
                let mut r = rules;
                let edited = crate::stair_ui::ramp_rules_rows(ui, &mut r);
                if edited.changed {
                    update(state, tid, "rq", |t| {
                        if let TypeData::Ramp { rules, .. } = &mut t.data {
                            *rules = r;
                        }
                    });
                }
                if edited.done {
                    state.session.doc.break_merge();
                }
            });
        }
        TypeData::Railing { height, post_spacing, .. } => {
            crate::table::section(ui, "type-dimensions", t!("section-dimensions"), |ui| {
                length_row(ui, state, &t!("col-height"), tid, "rh", height, |d, v| {
                    if let TypeData::Railing { height, .. } = d {
                        *height = v.clamp(0.3, 3.0);
                    }
                });
                length_row(ui, state, &t!("prop-post-spacing"), tid, "rp", post_spacing, |d, v| {
                    if let TypeData::Railing { post_spacing, .. } = d {
                        *post_spacing = v.clamp(0.1, 5.0);
                    }
                });
            });
        }
        _ => {}
    }
}

/// The kinds of section a type can choose; beams take all but the round ones.
#[derive(Debug, Clone, Copy, PartialEq)]
enum SectionKind {
    Rectangular,
    Round,
    I,
    Channel,
    Angle,
    Tube,
    Pipe,
}

impl SectionKind {
    const ALL: [SectionKind; 7] = [
        SectionKind::Rectangular,
        SectionKind::Round,
        SectionKind::I,
        SectionKind::Channel,
        SectionKind::Angle,
        SectionKind::Tube,
        SectionKind::Pipe,
    ];

    fn key(self) -> &'static str {
        match self {
            SectionKind::Rectangular => "section-solid",
            SectionKind::Round => "section-round",
            SectionKind::I => "section-i",
            SectionKind::Channel => "section-channel",
            SectionKind::Angle => "section-angle",
            SectionKind::Tube => "section-tube",
            SectionKind::Pipe => "section-pipe",
        }
    }

    fn of(section: cc_core::ColumnSection) -> Self {
        use cc_core::{ColumnSection as C, SectionShape as S};
        match section {
            C::Rectangular { .. } | C::Profile { shape: S::Solid, .. } => SectionKind::Rectangular,
            C::Round { .. } => SectionKind::Round,
            C::Pipe { .. } => SectionKind::Pipe,
            C::Profile { shape: S::I { .. }, .. } => SectionKind::I,
            C::Profile { shape: S::Channel { .. }, .. } => SectionKind::Channel,
            C::Profile { shape: S::Angle { .. }, .. } => SectionKind::Angle,
            C::Profile { shape: S::Tube { .. }, .. } => SectionKind::Tube,
        }
    }

    /// `section` turned into this kind, keeping its size; thicknesses start
    /// at typical proportions.
    fn convert(self, section: cc_core::ColumnSection) -> cc_core::ColumnSection {
        use cc_core::{ColumnSection as C, SectionShape as S};
        let (w, d) = section.size();
        let profile = |shape| C::Profile { width: w, depth: d, shape };
        match self {
            SectionKind::Rectangular => C::Rectangular { width: w, depth: d },
            SectionKind::Round => C::Round { diameter: w.max(d) },
            SectionKind::Pipe => C::Pipe { diameter: w.max(d), wall: w.max(d) * 0.05 },
            SectionKind::I => profile(S::I { web: w * 0.05, flange: d * 0.075 }),
            SectionKind::Channel => profile(S::Channel { web: w * 0.1, flange: d * 0.075 }),
            SectionKind::Angle => profile(S::Angle { thickness: w.min(d) * 0.1 }),
            SectionKind::Tube => profile(S::Tube { wall: w.min(d) * 0.05 }),
        }
    }
}

/// The section of a column type, or a beam type's as a profile.
fn type_section(d: &TypeData) -> Option<cc_core::ColumnSection> {
    match *d {
        TypeData::Column { section, .. } => Some(section),
        TypeData::Beam { width, height, shape, .. } => {
            Some(cc_core::ColumnSection::Profile { width, depth: height, shape })
        }
        _ => None,
    }
}

fn set_type_section(d: &mut TypeData, s: cc_core::ColumnSection) {
    use cc_core::ColumnSection as C;
    match d {
        TypeData::Column { section, .. } => *section = s,
        TypeData::Beam { width, height, shape, .. } => match s {
            C::Profile { width: w, depth: h, shape: p } => (*width, *height, *shape) = (w, h, p),
            C::Rectangular { width: w, depth: h } => (*width, *height, *shape) = (w, h, cc_core::SectionShape::Solid),
            C::Round { .. } | C::Pipe { .. } => {}
        },
        _ => {}
    }
}

/// Kind and dimensions of a column's section, or a beam's (`beam`: the
/// depth is its height and round sections are not offered).
fn section_rows(ui: &mut Ui, state: &mut AppState, tid: ElementTypeId, section: cc_core::ColumnSection, beam: bool) {
    use cc_core::{ColumnSection as C, SectionShape as S};
    crate::table::section(ui, "type-dimensions", t!("section-dimensions"), |ui| {
        let current = SectionKind::of(section);
        let mut kind = current;
        row(ui, &t!("prop-section-kind"), |ui| {
            egui::ComboBox::from_id_salt(("section-kind", tid.0)).selected_text(tr(kind.key())).show_ui(ui, |ui| {
                for option in SectionKind::ALL {
                    if beam && matches!(option, SectionKind::Round | SectionKind::Pipe) {
                        continue;
                    }
                    ui.selectable_value(&mut kind, option, tr(option.key()));
                }
            });
        });
        if kind != current {
            update(state, tid, "skind", |t| set_type_section(&mut t.data, kind.convert(section)));
            state.session.doc.break_merge();
        }
        // Each dimension: label, field key, value and how to set it.
        type Setter = fn(&mut C, f64);
        let depth_label = if beam { t!("col-height") } else { t!("prop-depth-section") };
        let mut rows: Vec<(String, &str, f64, Setter)> = Vec::new();
        match section {
            C::Rectangular { width, depth } | C::Profile { width, depth, .. } => {
                rows.push((t!("col-width"), "sw", width, |s, v| match s {
                    C::Rectangular { width, .. } | C::Profile { width, .. } => *width = v.max(0.01),
                    _ => {}
                }));
                rows.push((depth_label, "sd", depth, |s, v| match s {
                    C::Rectangular { depth, .. } | C::Profile { depth, .. } => *depth = v.max(0.01),
                    _ => {}
                }));
            }
            C::Round { diameter } | C::Pipe { diameter, .. } => {
                rows.push((t!("prop-diameter"), "sdia", diameter, |s, v| match s {
                    C::Round { diameter } | C::Pipe { diameter, .. } => *diameter = v.max(0.01),
                    _ => {}
                }));
            }
        }
        match section {
            C::Profile { shape: S::I { web, flange } | S::Channel { web, flange }, .. } => {
                rows.push((t!("prop-web"), "sweb", web, |s, v| {
                    if let C::Profile { shape: S::I { web, .. } | S::Channel { web, .. }, .. } = s {
                        *web = v.max(0.001);
                    }
                }));
                rows.push((t!("prop-flange"), "sfl", flange, |s, v| {
                    if let C::Profile { shape: S::I { flange, .. } | S::Channel { flange, .. }, .. } = s {
                        *flange = v.max(0.001);
                    }
                }));
            }
            C::Profile { shape: S::Angle { thickness }, .. } => {
                rows.push((t!("col-thickness"), "sth", thickness, |s, v| {
                    if let C::Profile { shape: S::Angle { thickness }, .. } = s {
                        *thickness = v.max(0.001);
                    }
                }));
            }
            C::Profile { shape: S::Tube { wall }, .. } | C::Pipe { wall, .. } => {
                rows.push((t!("prop-wall-thickness"), "swall", wall, |s, v| match s {
                    C::Profile { shape: S::Tube { wall }, .. } | C::Pipe { wall, .. } => *wall = v.max(0.001),
                    _ => {}
                }));
            }
            _ => {}
        }
        for (label, field, value, set) in rows {
            length_row(ui, state, &label, tid, field, value, |d, v| {
                if let Some(mut s) = type_section(d) {
                    set(&mut s, v);
                    // Thicknesses stay within the section's box.
                    if let C::Profile { width, depth, shape } = &mut s {
                        *shape = shape.fitted(*width, *depth);
                    }
                    if let C::Pipe { diameter, wall } = &mut s {
                        *wall = wall.min(*diameter * 0.5);
                    }
                    set_type_section(d, s);
                }
            });
        }
    });
}
