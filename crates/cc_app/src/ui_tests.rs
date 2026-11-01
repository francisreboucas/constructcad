//! Interface tests: the ribbon, the command palette and the icons run
//! headless with egui_kittest, queried through the accessibility tree that
//! the widgets describe (`widget_info`), and compared with screenshots.

use crate::state::{AppState, RibbonTab};
use crate::t;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

/// The sample house, open in the editor.
fn sample_state() -> AppState {
    let project = crate::home::sample_project("casa_simples").unwrap();
    let mut state = AppState::new(cc_core::Document::new(project));
    state.project_open = true;
    state.show_home = false;
    state
}

/// A harness drawing `draw` with the app theme. The theme's fonts load on
/// the frame after they are set, so the first frame (run by the builder)
/// only installs it.
fn themed_harness(
    state: AppState,
    size: egui::Vec2,
    theme: egui::Theme,
    draw: fn(&mut egui::Ui, &mut AppState),
) -> Harness<'static, AppState> {
    let mut themed = false;
    let mut harness = Harness::builder().with_size(size).build_ui_state(
        move |ui, state: &mut AppState| {
            if !themed {
                crate::theme::apply(ui.ctx());
                ui.ctx().set_theme(theme);
                themed = true;
                return;
            }
            crate::theme::sync(ui.ctx());
            draw(ui, state)
        },
        state,
    );
    harness.run();
    harness
}

fn ribbon_harness(state: AppState, width: f32) -> Harness<'static, AppState> {
    ribbon_harness_sized(state, egui::vec2(width, 150.0))
}

fn ribbon_harness_sized(state: AppState, size: egui::Vec2) -> Harness<'static, AppState> {
    themed_harness(state, size, egui::Theme::Light, crate::ribbon::ribbon)
}

/// The command palette over an empty window.
fn palette_harness(state: AppState) -> Harness<'static, AppState> {
    themed_harness(state, egui::vec2(800.0, 480.0), egui::Theme::Light, |ui, state| {
        crate::palette::show(ui.ctx(), state)
    })
}

/// Screenshot tests compare renders with the images in `tests/snapshots`.
/// GPUs differ slightly, so they run on request: `CC_UI_SNAPSHOTS=1 cargo
/// test -p cc_app snapshots` compares, `UPDATE_SNAPSHOTS=1` rewrites them
/// after an intended change (review the new images before committing).
fn snapshots_requested() -> bool {
    std::env::var_os("CC_UI_SNAPSHOTS").is_some() || std::env::var_os("UPDATE_SNAPSHOTS").is_some()
}

#[test]
fn ribbon_snapshots() {
    if !snapshots_requested() {
        return;
    }
    let mut results = egui_kittest::SnapshotResults::new();
    let tabs = [
        RibbonTab::File,
        RibbonTab::Architecture,
        RibbonTab::Structure,
        RibbonTab::Mep,
        RibbonTab::Insert,
        RibbonTab::Modify,
        RibbonTab::Annotate,
        RibbonTab::View,
        RibbonTab::Manage,
    ];
    for tab in tabs {
        let mut state = sample_state();
        state.ribbon_tab = tab;
        let mut harness = ribbon_harness(state, 1100.0);
        results.add(harness.try_snapshot(format!("ribbon_{tab:?}").to_lowercase()));
    }
    // Narrow window: the groups scroll.
    let mut state = sample_state();
    state.ribbon_tab = RibbonTab::Architecture;
    results.add(ribbon_harness(state, 520.0).try_snapshot("ribbon_narrow"));
    // Collapsed, with a tab's panel shown over the views.
    let mut state = sample_state();
    state.ribbon_collapsed = true;
    state.ribbon_peek = true;
    state.ribbon_tab = RibbonTab::Annotate;
    results.add(ribbon_harness_sized(state, egui::vec2(900.0, 220.0)).try_snapshot("ribbon_peek"));
    // The rich tooltip of an unavailable command.
    let mut state = sample_state();
    state.ribbon_tab = RibbonTab::Modify;
    let mut harness = ribbon_harness_sized(state, egui::vec2(900.0, 300.0));
    harness.get_by_label(&t!("tool-move")).hover();
    harness.run();
    results.add(harness.try_snapshot("ribbon_tooltip"));
    // The palette searching.
    let mut state = sample_state();
    crate::palette::toggle(&mut state);
    let mut harness = palette_harness(state);
    harness.get_by_role(Role::TextInput).type_text("planta");
    harness.run();
    results.add(harness.try_snapshot("palette"));
    results.unwrap();
}

/// Every icon in both tones at the ribbon sizes, the badges, and the
/// enabled, selected and disabled states, in the light and dark themes.
fn icon_gallery(ui: &mut egui::Ui, _: &mut AppState) {
    use crate::icons::{self, Badge, Icon, Tones};
    let palette = crate::theme::palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, palette.panel);
    let per_row = 18;
    for (i, icon) in Icon::ALL.iter().enumerate() {
        let origin = rect.min + egui::vec2(8.0 + (i % per_row) as f32 * 52.0, 8.0 + (i / per_row) as f32 * 30.0);
        for (k, side) in [16.0, 22.0].into_iter().enumerate() {
            let r = egui::Rect::from_min_size(origin + egui::vec2(k as f32 * 22.0, 0.0), egui::vec2(side, side));
            icons::paint_toned(ui.painter(), r, *icon, Tones::of(palette, None, true, false), None);
        }
    }
    let top = rect.min.y + 8.0 + Icon::ALL.len().div_ceil(per_row) as f32 * 30.0 + 8.0;
    let badges = [None, Some(Badge::New), Some(Badge::Import), Some(Badge::Export), Some(Badge::Remove)];
    for (row, (enabled, selected)) in [(true, false), (true, true), (false, false)].into_iter().enumerate() {
        for (col, badge) in badges.into_iter().enumerate() {
            let r = egui::Rect::from_min_size(
                egui::pos2(rect.min.x + 8.0 + col as f32 * 36.0, top + row as f32 * 32.0),
                egui::vec2(24.0, 24.0),
            );
            if selected {
                ui.painter().rect_filled(r.expand(4.0), 4.0, palette.accent_soft);
            }
            let tones = Tones::of(palette, badge, enabled, selected);
            icons::paint_toned(ui.painter(), r, Icon::Component, tones, badge);
        }
    }
}

#[test]
fn icon_gallery_snapshots() {
    if !snapshots_requested() {
        return;
    }
    let mut results = egui_kittest::SnapshotResults::new();
    for (theme, name) in [(egui::Theme::Light, "icons_light"), (egui::Theme::Dark, "icons_dark")] {
        let mut harness = themed_harness(sample_state(), egui::vec2(950.0, 260.0), theme, icon_gallery);
        results.add(harness.try_snapshot(name));
    }
    results.unwrap();
}

/// A ribbon wider than the window scrolls with arrows at its ends.
#[test]
fn narrow_ribbon_scrolls() {
    let mut state = sample_state();
    state.ribbon_tab = RibbonTab::Architecture;
    let mut harness = ribbon_harness(state, 520.0);
    assert!(harness.get_by_label(&t!("ribbon-scroll-left")).accesskit_node().is_disabled(), "at the start");
    harness.get_by_label(&t!("ribbon-scroll-right")).click();
    harness.run();
    let left = t!("ribbon-scroll-left");
    assert!(!harness.get_by_label(&left).accesskit_node().is_disabled(), "scrolled away from the start");
    // Wide enough, no arrows.
    let mut state = sample_state();
    state.ribbon_tab = RibbonTab::Structure;
    let harness = ribbon_harness(state, 1100.0);
    assert!(harness.query_by_label(&t!("ribbon-scroll-right")).is_none());
}

/// Double-clicking a tab collapses the ribbon to its tabs; a click shows a
/// tab's panel over the views, and running a command closes it.
#[test]
fn ribbon_collapses_and_peeks() {
    let mut harness = ribbon_harness_sized(sample_state(), egui::vec2(1100.0, 220.0));
    let tab = t!("ribbon-annotate");
    harness.get_by_label(&tab).click();
    harness.run();
    assert_eq!(harness.state().ribbon_tab, RibbonTab::Annotate);
    // The harness clock runs faster than a hand, so allow a slow double click.
    harness.ctx.options_mut(|o| o.input_options.max_double_click_delay = 60.0);
    harness.get_by_label(&tab).click();
    harness.run();
    assert!(harness.state().ribbon_collapsed, "double click collapses");
    assert!(harness.query_by_role_and_label(Role::Button, &t!("tool-text")).is_none(), "no panel while collapsed");
    harness.get_by_label(&tab).click();
    harness.run();
    assert!(harness.state().ribbon_peek);
    harness.get_by_role_and_label(Role::Button, &t!("tool-text")).click();
    harness.run();
    assert!(matches!(harness.state().tool, crate::tools::Tool::Text { .. }));
    assert!(!harness.state().ribbon_peek, "the panel closes after a command");
    assert!(harness.state().ribbon_collapsed);
}

/// Typing in the palette finds commands by name, accents aside, and Enter runs the first.
#[test]
fn palette_runs_commands() {
    let mut state = sample_state();
    crate::palette::toggle(&mut state);
    let mut harness = palette_harness(state);
    harness.get_by_role(Role::TextInput).type_text("parede");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(matches!(harness.state().tool, crate::tools::Tool::Wall { .. }));
    assert!(harness.state().palette.is_none(), "the palette closes after running");
    // A two-letter shortcut finds its command too; unavailable ones explain why.
    crate::palette::toggle(harness.state_mut());
    harness.run();
    harness.get_by_role(Role::TextInput).type_text("MV");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(harness.state().message, t!("msg-select-first"));
}

/// The palette button opens the palette (its own click used to close it in
/// the same frame); a click outside closes it.
#[test]
fn palette_button_opens_the_palette() {
    let mut harness = themed_harness(sample_state(), egui::vec2(1400.0, 480.0), egui::Theme::Light, |ui, state| {
        crate::ribbon::ribbon(ui, state);
        crate::palette::show(ui.ctx(), state);
    });
    harness.get_by_role_and_label(Role::Button, &t!("action-command-palette")).click();
    harness.run();
    assert!(harness.state().palette.is_some(), "the button opens the palette");
    assert!(harness.query_by_role(Role::TextInput).is_some());
    harness.get_by_role_and_label(Role::Button, &t!("action-command-palette")).click();
    harness.run();
    assert!(harness.state().palette.is_none(), "a click outside closes it");
}

/// Engine commands open a form with their arguments and run in the session.
#[test]
fn palette_runs_engine_commands() {
    let mut state = sample_state();
    let levels = state.project().levels.len();
    crate::palette::toggle(&mut state);
    let mut harness = palette_harness(state);
    harness.get_by_role(Role::TextInput).type_text("level_create");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    let editor = harness.get_by_role(Role::MultilineTextInput);
    editor.focus();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.get_by_role(Role::MultilineTextInput).type_text(r#"{"elevation": 9.0, "name": "Roof deck"}"#);
    harness.run();
    harness.get_by_role_and_label(Role::Button, &t!("palette-run")).click();
    harness.run();
    assert_eq!(harness.state().project().levels.len(), levels + 1);
    assert!(harness.state().project().levels.iter().any(|l| l.name == "Roof deck"));
}

/// Every ribbon button tells assistive technology its name, and every
/// interface command shown in a tab can be found by its label.
#[test]
fn ribbon_buttons_are_accessible() {
    use crate::commands::UiCommand;
    let tabs = [
        RibbonTab::File,
        RibbonTab::Architecture,
        RibbonTab::Structure,
        RibbonTab::Mep,
        RibbonTab::Insert,
        RibbonTab::Modify,
        RibbonTab::Annotate,
        RibbonTab::View,
        RibbonTab::Manage,
    ];
    let mut found = std::collections::HashSet::new();
    for tab in tabs {
        let mut state = sample_state();
        state.ribbon_tab = tab;
        let harness = ribbon_harness(state, 1600.0);
        for node in harness.query_all_by_role(Role::Button) {
            let label = node.accesskit_node().label().unwrap_or_default();
            assert!(!label.trim().is_empty(), "{tab:?}: a button without a name");
            found.insert(label);
        }
    }
    // The component environment shows its own ribbon.
    let mut state = sample_state();
    let table = state
        .project()
        .types_of(cc_core::Category::Component)
        .find(|t| matches!(&t.data, cc_core::TypeData::Component { component, .. } if component == "Mesa"))
        .map(|t| t.id)
        .unwrap();
    crate::component_env::open(&mut state, table);
    let harness = themed_harness(state, egui::vec2(1600.0, 140.0), egui::Theme::Light, |ui, state| {
        crate::component_env::ribbon(ui, state)
    });
    for node in harness.query_all_by_role(Role::Button) {
        found.insert(node.accesskit_node().label().unwrap_or_default());
    }
    // So does an open sketch.
    let mut state = sample_state();
    let floor = state.project().default_type(cc_core::Category::Floor).unwrap();
    state.set_tool(crate::tools::Tool::Sketch { category: cc_core::Category::Floor, type_id: floor });
    let harness = themed_harness(state, egui::vec2(1600.0, 140.0), egui::Theme::Light, |ui, state| {
        crate::sketch_mode::ribbon(ui, state)
    });
    for node in harness.query_all_by_role(Role::Button) {
        found.insert(node.accesskit_node().label().unwrap_or_default());
    }
    let state = sample_state();
    let hidden = [UiCommand::CommandPalette, UiCommand::ToggleRibbon];
    for command in UiCommand::all().into_iter().filter(|c| !hidden.contains(c)) {
        assert!(found.contains(&command.label(&state)), "{command:?} is not in the ribbon");
    }
}

/// The wall tool switches between straight and arc walls in the options bar.
#[test]
fn wall_tool_has_an_arc_mode() {
    let mut state = sample_state();
    let wall_type = state.project().default_type(cc_core::Category::Wall).unwrap();
    state.tool = crate::tools::Tool::Wall { type_id: wall_type };
    let mut harness = themed_harness(state, egui::vec2(1600.0, 60.0), egui::Theme::Light, |ui, state| {
        crate::ribbon::options_bar(ui, state)
    });
    assert!(!harness.state().wall_arc);
    harness.get_by_label(&t!("wall-mode-arc")).click();
    harness.run();
    assert!(harness.state().wall_arc);
    harness.get_by_label(&t!("wall-mode-line")).click();
    harness.run();
    assert!(!harness.state().wall_arc);
}

/// Aligned and linear dimensions are placed singly or in a continuous chain.
#[test]
fn dimension_tool_has_a_continuous_mode() {
    use crate::tools::{DimChain, DimensionMode, Tool};
    let mut state = sample_state();
    let type_id = state.project().default_type(cc_core::Category::Dimension).unwrap();
    state.tool = Tool::Dimension { type_id, mode: DimensionMode::Linear };
    let mut harness = themed_harness(state, egui::vec2(1600.0, 60.0), egui::Theme::Light, |ui, state| {
        crate::ribbon::options_bar(ui, state)
    });
    assert!(!harness.state().dim_continuous);
    harness.get_by_label(&t!("dim-continuous")).click();
    harness.run();
    assert!(harness.state().dim_continuous);
    // A chain in progress shows how to continue and end it.
    let view = cc_core::ViewId(1);
    let chain = DimChain {
        view,
        last: glam::DVec2::ZERO,
        dir: glam::DVec2::X,
        line: glam::DVec2::Y,
        linear: true,
        ordinate: false,
    };
    harness.state_mut().interaction.dim_chain = Some(chain);
    harness.run();
    assert!(harness.query_by_label(&t!("hint-dim-chain")).is_some());
    harness.get_by_label(&t!("dim-single")).click();
    harness.run();
    assert!(!harness.state().dim_continuous);
    // Ordinate dimensions always continue from their origin.
    harness.state_mut().tool = Tool::Dimension { type_id, mode: DimensionMode::Ordinate };
    harness.state_mut().interaction.dim_chain = Some(DimChain { ordinate: true, ..chain });
    harness.run();
    assert!(harness.query_by_label(&t!("dim-continuous")).is_none());
    assert!(harness.query_by_label(&t!("hint-dim-ordinate-next")).is_some());
    // Angular dimensions have no chain.
    harness.state_mut().interaction.dim_chain = None;
    harness.state_mut().tool = Tool::Dimension { type_id, mode: DimensionMode::Angular };
    harness.run();
    assert!(harness.query_by_label(&t!("dim-continuous")).is_none());
}

/// A roof gets an opening (a courtyard) sketched inside it; one outside is refused.
#[test]
fn roof_openings_are_added_inside_the_roof() {
    let mut state = sample_state();
    let (roof, footprint) = state
        .project()
        .elements
        .values()
        .find_map(|e| match &e.kind {
            cc_core::ElementKind::Roof(r) => Some((e.id, r.footprint.clone())),
            _ => None,
        })
        .unwrap();
    let lo = footprint.iter().fold(glam::DVec2::splat(f64::MAX), |a, p| a.min(*p));
    let hi = footprint.iter().fold(glam::DVec2::splat(f64::MIN), |a, p| a.max(*p));
    let c = (lo + hi) * 0.5;
    let square = |c: glam::DVec2| {
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().map(|&(x, y)| c + glam::DVec2::new(x, y)).collect()
    };
    let holes = |state: &AppState| match &state.project().element(roof).unwrap().kind {
        cc_core::ElementKind::Roof(r) => r.holes.len(),
        _ => 0,
    };
    state.set_tool(crate::tools::Tool::Opening { host: roof, floor: false });
    crate::tools::add_opening(&mut state, roof, square(hi + glam::DVec2::splat(5.0)));
    assert_eq!(holes(&state), 0, "outside the roof");
    crate::tools::add_opening(&mut state, roof, square(c));
    assert_eq!(holes(&state), 1);
    assert_eq!(state.tool, crate::tools::Tool::Select);
    state.sync();
    assert!(state.session.geom().roof_ridges.contains_key(&roof));
}

/// Visibility/Graphics of the ground plan, with a filter applied.
fn graphics_state() -> (AppState, cc_core::ViewId) {
    let mut state = sample_state();
    let plan = state.project().views_of(cc_core::ViewCategory::FloorPlan).next().unwrap().id;
    state.active_view = Some(plan);
    let rules = vec![cc_engine::commands::graphics::RuleArg {
        param: "length".into(),
        op: "greater".into(),
        value: Some(serde_json::json!(5.0)),
    }];
    let filter = cc_engine::commands::graphics::FilterCreate {
        name: "Long walls".into(),
        categories: vec!["wall".into()],
        rules,
        any: false,
    };
    state.session.run(filter).unwrap();
    let add = cc_engine::commands::graphics::ViewFilterAdd {
        view: Some(cc_engine::Ref::Id(plan.0)),
        filter: cc_engine::Ref::Name("Long walls".into()),
        overrides: Some(serde_json::json!({"cut_lines": {"color": [200, 0, 0], "weight": 0.7}})),
        ..Default::default()
    };
    state.session.run(add).unwrap();
    (state, plan)
}

#[test]
fn graphics_dialog_hides_categories_and_cancel_undoes() {
    let (mut state, plan) = graphics_state();
    let depth = state.session.doc.undo_depth();
    crate::graphics_dialog::open(&mut state, crate::graphics_dialog::Target::View(plan));
    let mut harness = themed_harness(state, egui::vec2(980.0, 680.0), egui::Theme::Light, |ui, state| {
        crate::graphics_dialog::show(ui.ctx(), state)
    });
    let walls = format!("{}: {}", t!("vg-visible"), crate::actions::t_category(cc_core::Category::Wall));
    harness.get_by_label(&walls).click_accesskit();
    harness.run();
    let hidden = |s: &AppState| !s.project().view(plan).unwrap().graphics.category(cc_core::Category::Wall).visible;
    assert!(hidden(harness.state()));
    harness.get_by_label(&t!("action-cancel")).click();
    harness.run();
    assert!(!hidden(harness.state()));
    assert_eq!(harness.state().session.doc.undo_depth(), depth);
    assert!(harness.state().dialogs.graphics.is_none());
}

/// The filters and view templates dialogs fit a tall window: their header
/// and buttons stay on screen (a divider once stretched them past it).
#[test]
fn list_dialogs_fit_a_tall_window() {
    let size = egui::vec2(1600.0, 1400.0);
    let (mut state, _) = graphics_state();
    crate::graphics_dialog::open_filters(&mut state);
    let mut filters =
        themed_harness(state, size, egui::Theme::Light, |ui, state| crate::graphics_dialog::show(ui.ctx(), state));
    let (mut state, plan) = graphics_state();
    let create = cc_engine::commands::graphics::TemplateCreate {
        name: "Working plan".into(),
        view: Some(cc_engine::Ref::Id(plan.0)),
    };
    state.session.run(create).unwrap();
    crate::graphics_dialog::open_templates(&mut state);
    let mut templates =
        themed_harness(state, size, egui::Theme::Light, |ui, state| crate::graphics_dialog::show(ui.ctx(), state));
    // Empty lists: nothing selected.
    let mut state = sample_state();
    crate::graphics_dialog::open_filters(&mut state);
    let mut no_filters =
        themed_harness(state, size, egui::Theme::Light, |ui, state| crate::graphics_dialog::show(ui.ctx(), state));
    let mut state = sample_state();
    crate::graphics_dialog::open_templates(&mut state);
    let mut no_templates =
        themed_harness(state, size, egui::Theme::Light, |ui, state| crate::graphics_dialog::show(ui.ctx(), state));
    // The other dialogs with lists and tabs.
    let (mut state, plan) = graphics_state();
    crate::graphics_dialog::open(&mut state, crate::graphics_dialog::Target::View(plan));
    let mut graphics =
        themed_harness(state, size, egui::Theme::Light, |ui, state| crate::graphics_dialog::show(ui.ctx(), state));
    let mut state = sample_state();
    crate::phases_dialog::open(&mut state);
    let mut phases =
        themed_harness(state, size, egui::Theme::Light, |ui, state| crate::phases_dialog::show(ui.ctx(), state));
    for harness in [&mut filters, &mut templates, &mut no_filters, &mut no_templates, &mut graphics, &mut phases] {
        // The dialog once grew a little every frame, until it left the screen.
        for _ in 0..40 {
            harness.step();
        }
        let ok = harness.get_by_label(&t!("action-ok")).rect();
        let close = harness.get_by_label(&t!("action-close")).rect();
        assert!(close.top() > 0.0 && ok.bottom() < size.y, "{close:?} {ok:?}");
        assert!(ok.bottom() - close.top() < 900.0, "the dialog grew to {}", ok.bottom() - close.top());
    }
}

#[test]
fn graphics_dialog_snapshots() {
    if !snapshots_requested() {
        return;
    }
    let mut results = egui_kittest::SnapshotResults::new();
    let (mut state, plan) = graphics_state();
    crate::graphics_dialog::open(&mut state, crate::graphics_dialog::Target::View(plan));
    let mut harness = themed_harness(state, egui::vec2(980.0, 680.0), egui::Theme::Light, |ui, state| {
        crate::graphics_dialog::show(ui.ctx(), state)
    });
    results.add(harness.try_snapshot("visibility_graphics"));
    harness.get_by_label(&t!("vg-filters")).click_accesskit();
    for _ in 0..4 {
        harness.run();
    }
    results.add(harness.try_snapshot("visibility_graphics_filters"));
    let (mut state, _) = graphics_state();
    crate::graphics_dialog::open_filters(&mut state);
    let mut harness = themed_harness(state, egui::vec2(980.0, 760.0), egui::Theme::Light, |ui, state| {
        crate::graphics_dialog::show(ui.ctx(), state)
    });
    results.add(harness.try_snapshot("filters"));
    let (mut state, plan) = graphics_state();
    let create = cc_engine::commands::graphics::TemplateCreate {
        name: "Working plan".into(),
        view: Some(cc_engine::Ref::Id(plan.0)),
    };
    state.session.run(create).unwrap();
    crate::graphics_dialog::open_templates(&mut state);
    let mut harness = themed_harness(state, egui::vec2(980.0, 680.0), egui::Theme::Light, |ui, state| {
        crate::graphics_dialog::show(ui.ctx(), state)
    });
    results.add(harness.try_snapshot("view_templates"));
    results.unwrap();
}

fn phases_harness(state: AppState) -> Harness<'static, AppState> {
    themed_harness(state, egui::vec2(1000.0, 640.0), egui::Theme::Light, |ui, state| {
        crate::phases_dialog::show(ui.ctx(), state)
    })
}

#[test]
fn phases_dialog_adds_a_phase_and_cancel_undoes() {
    let mut state = sample_state();
    let depth = state.session.doc.undo_depth();
    let phases = state.project().phases.len();
    crate::phases_dialog::open(&mut state);
    let mut harness = phases_harness(state);
    harness.get_by_label(&t!("ph-add")).click();
    harness.run();
    assert_eq!(harness.state().project().phases.len(), phases + 1);
    harness.get_by_label(&t!("action-cancel")).click();
    harness.run();
    assert_eq!(harness.state().project().phases.len(), phases);
    assert_eq!(harness.state().session.doc.undo_depth(), depth);
    assert!(harness.state().dialogs.phases.is_none());
}

/// Demolish acts on a selection at once, also from a 3D view; with nothing
/// demolishable selected it starts the tool.
#[test]
fn demolish_takes_the_selection_in_3d() {
    let mut state = sample_state();
    let view = state
        .project()
        .views
        .values()
        .find(|v| matches!(v.kind, cc_core::ViewKind::ThreeD { .. }))
        .map(|v| v.id)
        .unwrap();
    state.active_view = Some(view);
    assert!(crate::commands::phase_view(&state).is_some());
    let walls: Vec<_> =
        state.project().elements.values().filter(|e| e.category() == cc_core::Category::Wall).map(|e| e.id).collect();
    assert!(!walls.is_empty());
    let depth = state.session.doc.undo_depth();
    state.selection = walls.iter().copied().collect();
    crate::phases_dialog::start_demolish(&mut state);
    assert_eq!(state.session.doc.undo_depth(), depth + 1);
    assert!(walls.iter().all(|w| state.project().element(*w).unwrap().phase_demolished.is_some()));
    assert!(state.selection.is_empty());
    assert_eq!(state.tool, crate::tools::Tool::Select);
    // Demolished walls are not demolishable again: the tool starts instead.
    state.selection = walls.iter().copied().collect();
    assert!(crate::phases_dialog::demolishable(&state, view, walls.iter().copied()).is_empty());
    crate::phases_dialog::start_demolish(&mut state);
    assert_eq!(state.session.doc.undo_depth(), depth + 1);
    assert_eq!(state.tool, crate::tools::Tool::Demolish);
}

#[test]
fn phases_dialog_ok_keeps_one_undo_step() {
    let mut state = sample_state();
    let depth = state.session.doc.undo_depth();
    let filters = state.project().phase_filters.len();
    crate::phases_dialog::open(&mut state);
    let mut harness = phases_harness(state);
    harness.get_by_label(&t!("ph-add")).click();
    harness.run();
    harness.get_by_label(&t!("ph-tab-filters")).click();
    harness.run();
    harness.get_by_label(&t!("ph-new-filter")).click();
    harness.run();
    assert_eq!(harness.state().project().phase_filters.len(), filters + 1);
    harness.get_by_label(&t!("action-ok")).click();
    harness.run();
    assert!(harness.state().dialogs.phases.is_none());
    assert_eq!(harness.state().project().phase_filters.len(), filters + 1);
    // OK keeps both edits.
    assert_eq!(harness.state().session.doc.undo_depth(), depth + 2);
}

#[test]
fn phases_dialog_snapshots() {
    if !snapshots_requested() {
        return;
    }
    let mut results = egui_kittest::SnapshotResults::new();
    // The dialog opens on the phases tab.
    for (tab, name) in
        [(None, "phases"), (Some("ph-tab-filters"), "phase_filters"), (Some("ph-tab-graphics"), "phase_graphics")]
    {
        let mut state = sample_state();
        crate::phases_dialog::open(&mut state);
        let mut harness = phases_harness(state);
        if let Some(tab) = tab {
            harness.get_by_label(&crate::i18n::tr(tab)).click();
        }
        for _ in 0..3 {
            harness.run();
        }
        results.add(harness.try_snapshot(name));
    }
    results.unwrap();
}

fn type_dialog_harness(state: AppState) -> Harness<'static, AppState> {
    themed_harness(state, egui::vec2(900.0, 900.0), egui::Theme::Light, |ui, state| {
        crate::type_dialog::show(ui.ctx(), state)
    })
}

/// Duplicate a window type, type a name and press OK: the copy keeps the
/// typed name (before, OK dropped a rename that Enter had not confirmed).
#[test]
fn duplicated_type_keeps_the_typed_name() {
    let mut state = sample_state();
    let window = state.project().default_type(cc_core::Category::Window).unwrap();
    state.dialogs.type_editor = Some(window);
    let mut harness = type_dialog_harness(state);
    harness.get_by_label(&t!("type-duplicate")).click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.query_all_by_role(Role::TextInput).find(|n| n.is_focused()).unwrap().type_text("Janela 2.5 x 1.8");
    harness.run();
    harness.get_by_label(&t!("action-ok")).click();
    harness.run();
    let names: Vec<_> = harness.state().project().types_of(cc_core::Category::Window).map(|t| t.name.clone()).collect();
    assert!(names.iter().any(|n| n == "Janela 2.5 x 1.8"), "{names:?}");
    assert!(harness.state().dialogs.type_editor.is_none());
}

/// Typing a name and moving on to another field confirms the name.
#[test]
fn type_name_is_kept_when_another_field_is_clicked() {
    let mut state = sample_state();
    let window = state.project().default_type(cc_core::Category::Window).unwrap();
    state.dialogs.type_editor = Some(window);
    let mut harness = type_dialog_harness(state);
    harness.get_by_label(&t!("type-rename")).click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.query_all_by_role(Role::TextInput).find(|n| n.is_focused()).unwrap().type_text("Janela alta");
    harness.run();
    harness.query_all_by_role(Role::TextInput).find(|n| !n.is_focused()).unwrap().click();
    harness.run();
    harness.run();
    assert_eq!(harness.state().project().element_type(window).unwrap().name, "Janela alta");
    assert!(harness.state().dialogs.type_rename.is_none());
}

/// The component library lists a folder's component files and loads the
/// one picked into the project.
#[test]
fn component_library_loads_a_component() {
    let mut state = sample_state();
    let dir = std::env::temp_dir().join(format!("ccad-library-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let door = state
        .project()
        .component_types()
        .find(|t| t.category() == cc_core::Category::Door)
        .and_then(|t| match &t.data {
            cc_core::TypeData::Component { component, .. } => Some(component.clone()),
            _ => None,
        })
        .unwrap();
    let path = dir.join("porta.ccomp");
    state.session.run(cc_engine::commands::io::ComponentSave { component: door.clone(), path: path.clone() }).unwrap();
    state.prefs.library_folders = vec![dir];
    crate::library::open(&mut state);
    assert_eq!(state.dialogs.library.as_ref().unwrap().entries.len(), 1);
    let before = state.project().types.len();
    let mut harness = themed_harness(state, egui::vec2(1000.0, 760.0), egui::Theme::Light, |ui, state| {
        crate::library::show(ui.ctx(), state)
    });
    harness.get_by_label_contains(&door).click();
    harness.run();
    harness.get_by_label(&t!("lib-load")).click();
    harness.run();
    assert!(harness.state().dialogs.library.is_none());
    assert!(harness.state().project().types.len() > before, "the door's types joined the project");
    assert!(matches!(harness.state().tool, crate::tools::Tool::Hosted { .. }), "doors are placed in walls");
}

/// The ribbon, the app's shortcuts and the dialogs (Preferences among them).
fn app_harness(state: AppState) -> Harness<'static, AppState> {
    themed_harness(state, egui::vec2(1200.0, 760.0), egui::Theme::Light, |ui, state| {
        crate::app::shortcuts(ui.ctx(), state);
        crate::ribbon::ribbon(ui, state);
        crate::dialogs::show(ui.ctx(), state);
    })
}

/// Preferences opens from the File tab and with Ctrl+,.
#[test]
fn preferences_open_from_the_file_tab_and_the_shortcut() {
    let mut state = sample_state();
    state.ribbon_tab = RibbonTab::File;
    let mut harness = app_harness(state);
    harness.get_by_role_and_label(Role::Button, &t!("action-preferences")).click();
    harness.run();
    assert!(harness.state().dialogs.preferences.is_some(), "the File tab button opens it");
    assert!(harness.query_by_label(&t!("pref-file-locations")).is_some(), "its sections are listed");
    harness.get_by_label(&t!("action-cancel")).click();
    harness.run();
    assert!(harness.state().dialogs.preferences.is_none());
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Comma);
    harness.run();
    assert!(harness.state().dialogs.preferences.is_some(), "Ctrl+, opens it");
}

/// Changes show at once; Cancel brings back the values the window opened
/// with, and OK keeps them.
#[test]
fn preferences_cancel_restores_and_ok_keeps() {
    let mut state = sample_state();
    crate::preferences::open(&mut state, crate::preferences::Section::Interface);
    let mut harness = app_harness(state);
    let tooltips = t!("pref-detailed-tooltips");
    let ribbon = t!("pref-ribbon-collapsed");
    harness.get_by_role_and_label(Role::CheckBox, &tooltips).click();
    harness.get_by_role_and_label(Role::CheckBox, &ribbon).click();
    harness.run();
    assert!(!harness.state().prefs.detailed_tooltips);
    assert!(harness.state().prefs.ribbon_collapsed);
    assert!(harness.state().ribbon_collapsed, "the ribbon collapses at once");
    harness.get_by_label(&t!("action-cancel")).click();
    harness.run();
    assert!(harness.state().dialogs.preferences.is_none());
    assert!(harness.state().prefs.detailed_tooltips, "Cancel restores");
    assert!(!harness.state().prefs.ribbon_collapsed && !harness.state().ribbon_collapsed);
    // Again, with OK.
    crate::preferences::open(harness.state_mut(), crate::preferences::Section::Interface);
    harness.run();
    harness.get_by_role_and_label(Role::CheckBox, &tooltips).click();
    harness.run();
    harness.get_by_label(&t!("action-ok")).click();
    harness.run();
    assert!(harness.state().dialogs.preferences.is_none());
    assert!(!harness.state().prefs.detailed_tooltips, "OK keeps");
}

/// The library window's Manage Folders opens Preferences on the file
/// locations, where the folder list now lives.
#[test]
fn library_manages_folders_in_preferences() {
    let mut state = sample_state();
    state.prefs.library_folders = vec![std::path::PathBuf::from("/no/such/library/ccad")];
    crate::library::open(&mut state);
    let mut harness = app_harness(state);
    harness.get_by_label(&t!("lib-manage-folders")).click();
    harness.run();
    let dialog = harness.state().dialogs.preferences.as_ref().map(|d| d.section);
    assert_eq!(dialog, Some(crate::preferences::Section::FileLocations));
    let remove = format!("{} {}", t!("action-remove"), std::path::Path::new("/no/such/library/ccad").display());
    harness.get_by_label(&remove).click();
    harness.run();
    assert!(harness.state().prefs.library_folders.is_empty());
    harness.get_by_label(&t!("action-ok")).click();
    harness.run();
    assert!(harness.state().dialogs.library.is_some(), "the library stays open");
    assert!(harness.state().prefs.library_folders.is_empty());
}

/// Shortcuts: typing two letters gives a command new keys, and Default
/// brings the old ones back.
#[test]
fn preferences_change_shortcuts() {
    let mut state = sample_state();
    crate::preferences::open(&mut state, crate::preferences::Section::Shortcuts);
    let mut harness = app_harness(state);
    let label = crate::commands::UiCommand::Demolish.label(harness.state());
    harness.run();
    // The search field has the focus; one command is left.
    harness.query_all_by_role(Role::TextInput).find(|n| n.is_focused()).unwrap().type_text(&label);
    harness.run();
    harness.query_all_by_role(Role::TextInput).find(|n| n.value().as_deref() == Some("DE")).unwrap().click();
    harness.run();
    harness.query_all_by_role(Role::TextInput).find(|n| n.is_focused()).unwrap().type_text("QZ");
    harness.run();
    assert_eq!(harness.state().prefs.shortcut_keys.get("Demolish").map(String::as_str), Some("QZ"));
    harness.get_by_label(&format!("{} {label}", t!("pref-shortcut-reset"))).click();
    harness.run();
    assert!(harness.state().prefs.shortcut_keys.is_empty(), "Default restores the keys");
    harness.get_by_label(&t!("action-ok")).click();
    harness.run();
    crate::commands::set_key_overrides(&Default::default());
}

#[test]
fn preferences_snapshots() {
    if !snapshots_requested() {
        return;
    }
    let mut results = egui_kittest::SnapshotResults::new();
    for section in crate::preferences::Section::ALL {
        let mut state = sample_state();
        state.prefs.library_folders = vec![std::path::PathBuf::from("/no/such/library/ccad")];
        crate::preferences::open(&mut state, section);
        let mut harness = themed_harness(state, egui::vec2(940.0, 600.0), egui::Theme::Light, |ui, state| {
            crate::preferences::show(ui.ctx(), state)
        });
        harness.run();
        results.add(harness.try_snapshot(format!("preferences_{section:?}").to_lowercase()));
    }
    results.unwrap();
}

/// Run Macro is a menu of the macros in the macros folder, plus Browse….
#[test]
fn macro_menu_lists_the_macros_folder() {
    let dir = std::env::temp_dir().join(format!("ccad-macro-menu-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("walls.ccmacro"), "").unwrap();
    std::fs::write(dir.join("doors.ccmacro"), "").unwrap();
    std::fs::write(dir.join("notes.txt"), "").unwrap();
    let mut state = sample_state();
    state.ribbon_tab = RibbonTab::Manage;
    state.prefs.macro_folder = Some(dir.clone());
    let mut harness = ribbon_harness_sized(state, egui::vec2(1100.0, 400.0));
    harness.get_by_role_and_label(Role::Button, &t!("action-macro-run")).click();
    harness.run();
    assert!(harness.query_by_label("walls").is_some());
    assert!(harness.query_by_label("doors").is_some());
    assert!(harness.query_by_label("notes").is_none(), "only macro files");
    assert!(harness.query_by_label(&t!("pref-browse")).is_some());
    let _ = std::fs::remove_dir_all(&dir);
    // Without a folder: a hint and Browse….
    let mut state = sample_state();
    state.ribbon_tab = RibbonTab::Manage;
    let mut harness = ribbon_harness_sized(state, egui::vec2(1100.0, 400.0));
    harness.get_by_role_and_label(Role::Button, &t!("action-macro-run")).click();
    harness.run();
    assert!(harness.query_by_label(&t!("macro-no-folder")).is_some());
    assert!(harness.query_by_label(&t!("pref-browse")).is_some());
}

/// The component editor on the sample house's window (a door or window: the
/// opening row shows) and on a new component.
#[test]
fn component_editor_snapshots() {
    if !snapshots_requested() {
        return;
    }
    let mut results = egui_kittest::SnapshotResults::new();
    let mut state = sample_state();
    let window = state.project().component_types().find(|t| t.category() == cc_core::Category::Window).unwrap().id;
    state.dialogs.component_editor = crate::component_editor::Editor::open(&state, window);
    let mut harness = themed_harness(state, egui::vec2(1280.0, 900.0), egui::Theme::Light, |ui, state| {
        crate::component_editor::show(ui.ctx(), state)
    });
    for _ in 0..4 {
        harness.run();
    }
    results.add(harness.try_snapshot("component_editor"));
    let mut state = sample_state();
    state.dialogs.component_editor = Some(crate::component_editor::Editor::new_component());
    let mut harness = themed_harness(state, egui::vec2(1280.0, 900.0), egui::Theme::Light, |ui, state| {
        crate::component_editor::show(ui.ctx(), state)
    });
    for _ in 0..4 {
        harness.run();
    }
    results.add(harness.try_snapshot("component_editor_new"));
    results.unwrap();
}

/// The structure tab's automation, the way its buttons run it, with the
/// panels and every 2D view drawn after each step and the 3D mesh rebuilt
/// (issue #51: a crash while placing columns, beams, footings and slabs).
#[test]
fn structure_automation_redraws_panels_and_views() {
    let mut state = sample_state();
    for (a, b) in [([-1.0, 0.0], [12.0, 0.0]), ([-1.0, 6.0], [12.0, 6.0]), ([0.0, -1.0], [0.0, 8.0])] {
        state.session.execute("grid_create", serde_json::json!({"start": a, "end": b})).unwrap();
    }
    state.session.execute("grid_create", serde_json::json!({"start": [10.0, -1.0], "end": [10.0, 8.0]})).unwrap();
    state.session.execute("view_create", serde_json::json!({"kind": "structural_plan"})).unwrap();
    let mut harness = themed_harness(state, egui::vec2(1400.0, 900.0), egui::Theme::Light, |ui, state| {
        state.sync();
        state.follow_selection();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(300.0);
                crate::browser::show(ui, state);
                crate::properties::show(ui, state);
            });
            if let Some(view) = state.active_view
                && state.project().view(view).is_some_and(|v| !matches!(v.kind, cc_core::ViewKind::ThreeD { .. }))
            {
                crate::canvas::show(ui, state, view);
            }
        });
    });
    let steps: [fn(&mut AppState); 9] = [
        crate::actions::columns_at_grids,
        crate::actions::beams_on_grids,
        crate::actions::footings_under_columns,
        crate::actions::strip_footings_under_walls,
        |s| drop(s.session.execute("floor_create", serde_json::json!({"from_walls": true}))),
        |s| drop(s.session.execute("foundation_slab_create", serde_json::json!({"from_walls": true}))),
        crate::actions::slab_join_supports,
        crate::actions::number_structure,
        crate::actions::beams_between_columns,
    ];
    let views: Vec<cc_core::ViewId> = harness.state().project().views.keys().copied().collect();
    for round in 0..2 {
        for step in steps {
            step(harness.state_mut());
            for &view in &views {
                harness.state_mut().active_view = Some(view);
                harness.run();
            }
            let state = harness.state_mut();
            let selection = state.selection.clone();
            let tess = cc_render::Tessellation::new(state.session.geometry());
            cc_render::SceneMesh::build_plain(&tess, &selection);
        }
        if round == 0 {
            // Undo all of it, then redo, as Ctrl+Z / Ctrl+Y would.
            while harness.state_mut().session.execute("undo", serde_json::json!({})).is_ok() {
                harness.run();
            }
            while harness.state_mut().session.execute("redo", serde_json::json!({})).is_ok() {
                harness.run();
            }
        }
    }
}

/// Renaming a view in the project browser ends with Enter or a click
/// elsewhere (issue #82: the field kept the focus for good, so the name
/// never changed and no other field took typing).
#[test]
fn renaming_a_view_in_the_browser_ends() {
    let mut state = sample_state();
    let original = state.project().views_of(cc_core::ViewCategory::ThreeD).next().map(|v| v.id).unwrap();
    let out = state.session.execute("view_duplicate", serde_json::json!({"view": original.0})).unwrap();
    let copy = cc_core::ViewId(out["id"].as_u64().unwrap());
    state.rename = Some((copy, String::new()));
    let mut harness = themed_harness(state, egui::vec2(400.0, 900.0), egui::Theme::Light, |ui, state| {
        egui::ScrollArea::vertical().show(ui, |ui| crate::browser::show(ui, state));
    });
    harness.run();
    harness.query_all_by_role(Role::TextInput).find(|n| n.is_focused()).unwrap().type_text("Perspectiva");
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(harness.state().rename.is_none(), "Enter ends the rename");
    assert_eq!(harness.state().project().view(copy).map(|v| v.name.as_str()), Some("Perspectiva"));

    // A click on another row ends it too and keeps the name.
    harness.state_mut().rename = Some((copy, "Vista externa".into()));
    harness.run();
    let other = harness.state().project().view(original).map(|v| v.name.clone()).unwrap();
    harness.get_by_label(&other).click();
    harness.run();
    harness.run();
    assert!(harness.state().rename.is_none(), "a click elsewhere ends the rename");
    assert_eq!(harness.state().project().view(copy).map(|v| v.name.as_str()), Some("Vista externa"));
}

/// Typed one key per frame, as people type, a name keeps its spaces and
/// can be emptied and typed again, though the command trims the name and
/// refuses an empty one at every key (issue #82: fields went back to the
/// model's value under the cursor).
#[test]
fn names_typed_key_by_key_keep_spaces_and_survive_emptying() {
    let mut state = sample_state();
    state.dialogs.levels = true;
    let (level, old) = state.project().levels.last().map(|l| (l.id, l.name.clone())).unwrap();
    let mut harness = app_harness(state);
    harness.run();
    harness.query_all_by_role(Role::TextInput).find(|n| n.value().as_deref() == Some(old.as_str())).unwrap().click();
    harness.run();
    for _ in 0..old.chars().count() {
        harness.key_press(egui::Key::End);
        harness.key_press(egui::Key::Backspace);
        harness.run();
    }
    for ch in "Piso 2 B".chars() {
        harness.query_all_by_role(Role::TextInput).find(|n| n.is_focused()).unwrap().type_text(&ch.to_string());
        harness.run();
    }
    assert_eq!(harness.state().project().level_name(level), "Piso 2 B");
}

/// A view placed on a sheet shows its own properties when picked on the
/// sheet, so its scale or graphics change there (#90).
#[test]
fn a_view_picked_on_a_sheet_shows_its_properties() {
    let mut state = sample_state();
    let sheet = state
        .project()
        .views
        .values()
        .find_map(|v| match &v.kind {
            cc_core::ViewKind::Sheet { viewports, .. } => viewports.first().map(|_| v.id),
            _ => None,
        })
        .expect("the sample has a sheet with a view");
    state.active_view = Some(sheet);
    state.sheet_selection = Some((sheet, 0));
    let mut harness = themed_harness(state, egui::vec2(360.0, 1400.0), egui::Theme::Light, |ui, state| {
        crate::properties::show(ui, state);
    });
    harness.run();
    assert!(harness.query_by_label(&t!("section-position")).is_some(), "the viewport's position");
    assert!(harness.query_by_label(&t!("section-graphics")).is_some(), "and the view's own graphics");
}

/// A view dragged from the project browser and dropped on a sheet is
/// placed there, where it was dropped, and picked.
#[test]
fn a_view_dragged_from_the_browser_lands_on_the_sheet() {
    let mut state = sample_state();
    let project = state.project();
    let sheet = project
        .views
        .values()
        .find(|v| matches!(v.kind, cc_core::ViewKind::Sheet { .. }))
        .map(|v| v.id)
        .expect("the sample has a sheet");
    // A view on no sheet yet, whose name no other view has (its browser row is found by name).
    let unique = |name: &str| project.views.values().filter(|v| v.name == name).count() == 1;
    let (view, name) = project
        .views
        .values()
        .filter(|v| v.placeable() && crate::actions::sheet_of(project, v.id).is_none())
        .filter(|v| !matches!(v.category(), cc_core::ViewCategory::Legend) && unique(&v.name))
        .map(|v| (v.id, v.name.clone()))
        .next()
        .expect("a view to place");
    state.active_view = Some(sheet);
    let mut harness = themed_harness(state, egui::vec2(1100.0, 1000.0), egui::Theme::Light, |ui, state| {
        let Some(sheet) = state.active_view else { return };
        ui.horizontal_top(|ui| {
            ui.allocate_ui(egui::vec2(320.0, 980.0), |ui| crate::browser::show(ui, state));
            ui.allocate_ui(ui.available_size(), |ui| crate::sheet::show(ui, state, sheet));
        });
    });
    harness.run();
    let start = harness.get_by_label(&name).rect().center();
    let target = egui::pos2(720.0, 500.0);
    let pointer = |h: &mut Harness<'static, AppState>, pos: egui::Pos2, pressed: Option<bool>| {
        h.input_mut().events.push(egui::Event::PointerMoved(pos));
        if let Some(pressed) = pressed {
            h.input_mut().events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.run();
    };
    pointer(&mut harness, start, None);
    pointer(&mut harness, start, Some(true));
    for i in 1..=10 {
        pointer(&mut harness, start + (target - start) * (i as f32 / 10.0), None);
    }
    pointer(&mut harness, target, Some(false));
    harness.run();
    let state = harness.state_mut();
    let placed = match state.project().view(sheet).map(|v| &v.kind) {
        Some(cc_core::ViewKind::Sheet { viewports, .. }) => viewports.iter().position(|vp| vp.view == view),
        _ => None,
    };
    let index = placed.expect("the dropped view is on the sheet");
    assert_eq!(state.sheet_selection, Some((sheet, index)), "and picked");
    let paper = state.sheet(sheet).map(|s| s.paper).unwrap();
    let Some(cc_core::ViewKind::Sheet { viewports, .. }) = state.project().view(sheet).map(|v| &v.kind) else {
        panic!("not a sheet")
    };
    let center = viewports[index].center;
    assert!(center.cmpge(glam::DVec2::ZERO).all() && center.cmple(paper).all(), "{center} on {paper}");
}

/// The home screen in both themes, the Open button under the pointer: the
/// sidebar buttons take the theme's colors (they stayed white in the dark
/// theme, their light text unreadable).
#[test]
fn home_snapshots() {
    if !snapshots_requested() {
        return;
    }
    let mut results = egui_kittest::SnapshotResults::new();
    for (theme, name) in [(egui::Theme::Light, "home_light"), (egui::Theme::Dark, "home_dark")] {
        let mut state = sample_state();
        state.show_home = true;
        let mut harness = themed_harness(state, egui::vec2(900.0, 500.0), theme, |ui, state| {
            crate::home::show(ui, state);
        });
        harness.run();
        let label = t!("action-open");
        harness.get_by_label(&label).hover();
        harness.run();
        results.add(harness.try_snapshot(name));
    }
    results.unwrap();
}

/// Update Now's dialog: a failed update offers the download; an installer
/// ready to run is kept for when the app closes, "On Exit" or not.
#[test]
fn update_install_dialog_offers_what_is_left_to_do() {
    use crate::update::{Install, Version};
    let mut state = sample_state();
    let version = Version { major: 9, minor: 0, patch: 0 };
    state.updates.install = Some(Install::Failed("checksum".into(), "https://example.com/r".into()));
    let mut harness = app_harness(state);
    harness.run();
    assert!(harness.query_by_label(&t!("update-download")).is_some());
    harness.query_all_by_label(&t!("action-close")).last().unwrap().click();
    harness.run();
    assert!(harness.state().updates.install.is_none());

    let msi = std::path::PathBuf::from("C:/Temp/constructcad-v9.0.0-x86_64.msi");
    let next = crate::update_install::Next::Installer(msi);
    harness.state_mut().updates.install = Some(Install::Ready(version, next.clone()));
    harness.run();
    harness.get_by_label(&t!("update-install-on-exit")).click();
    harness.run();
    assert!(harness.state().updates.install.is_none());
    assert_eq!(harness.state().updates.on_exit, Some(next), "the installer runs when the app closes");
    assert!(!harness.state().quit_confirmed, "On Exit does not close the app");
}

/// A mass's properties list a box per level it spans with that floor's area;
/// clearing one takes the mass floor away (#31).
#[test]
fn mass_floors_are_chosen_in_the_properties() {
    let mut state = sample_state();
    let out = state
        .session
        .execute("mass_create", serde_json::json!({"outline": [[0, 0], [10, 0], [10, 6], [0, 6]], "height": 5.5}))
        .unwrap();
    let mass = cc_core::ElementId(out["id"].as_u64().unwrap());
    state.selection = [mass].into_iter().collect();
    let ground = state.project().levels[0].name.clone();
    let mut harness = themed_harness(state, egui::vec2(360.0, 1400.0), egui::Theme::Light, |ui, state| {
        crate::properties::show(ui, state);
    });
    harness.run();
    assert!(harness.query_by_label(&t!("section-mass-floors")).is_some());
    let floors = |h: &Harness<'_, AppState>| match &h.state().project().element(mass).unwrap().kind {
        cc_core::ElementKind::Mass(m) => m.floors.len(),
        _ => 0,
    };
    let before = floors(&harness);
    assert!(before >= 1, "the mass spans the ground level at least");
    harness.get_by_role_and_label(Role::CheckBox, &ground).click();
    harness.run();
    assert_eq!(floors(&harness), before - 1);
}

/// Walls, floors and roofs from masses need a mass selected, and select
/// what they make (#31).
#[test]
fn walls_floors_and_roofs_come_from_selected_masses() {
    use crate::commands::UiCommand;
    let mut state = sample_state();
    assert!(UiCommand::MassWalls.unavailable(&state).is_some(), "no mass selected");
    let out = state
        .session
        .execute("mass_create", serde_json::json!({"outline": [[40, 0], [50, 0], [50, 6], [40, 6]], "height": 3.0}))
        .unwrap();
    let mass = cc_core::ElementId(out["id"].as_u64().unwrap());
    state.selection = [mass].into_iter().collect();
    assert_eq!(UiCommand::MassWalls.unavailable(&state), None);
    let ctx = egui::Context::default();
    crate::commands::run(&mut state, &ctx, UiCommand::MassWalls);
    assert_eq!(state.selection.len(), 4, "the four walls are selected");
    assert!(
        state
            .selection
            .iter()
            .all(|id| state.project().element(*id).is_some_and(|e| e.category() == cc_core::Category::Wall))
    );
    state.selection = [mass].into_iter().collect();
    crate::commands::run(&mut state, &ctx, UiCommand::MassRoof);
    assert!(
        state
            .selection
            .iter()
            .all(|id| state.project().element(*id).is_some_and(|e| e.category() == cc_core::Category::Roof))
    );
}

/// A roof type's dialog has the covering and framing; with tiles and timber
/// framing, a roof's properties show them.
#[test]
fn roof_types_take_tiles_and_framing() {
    let mut state = sample_state();
    let roof_type = state.project().default_type(cc_core::Category::Roof).unwrap();
    state.dialogs.type_editor = Some(roof_type);
    let mut harness = app_harness(state);
    harness.run();
    assert!(harness.query_by_label(&t!("section-roofing")).is_some());

    let mut state = sample_state();
    let params = serde_json::json!({"covering": "CeramicColonial", "framing": {"system": "Timber"}});
    state.session.execute("type_set", serde_json::json!({"type": roof_type.0, "params": params})).unwrap();
    let roof = state.project().elements.values().find(|e| e.type_id == roof_type).map(|e| e.id).unwrap();
    state.selection = [roof].into_iter().collect();
    let mut harness = themed_harness(state, egui::vec2(360.0, 1800.0), egui::Theme::Light, |ui, state| {
        crate::properties::show(ui, state);
    });
    harness.run();
    assert!(harness.query_by_label(&t!("section-roofing")).is_some(), "the roof shows its covering and framing");
}

/// Sketch mode end to end: a mass sketched as a circle, its boundary edited
/// (the circle's radius in the properties' place), Escape and Delete inside
/// the sketch, and Cancel leaving the model as it was.
#[test]
fn sketch_mode_creates_and_edits_outlines() {
    use crate::commands::UiCommand;
    use crate::sketch_mode::{Draw, Target};
    use cc_core::sketch::SketchCurve;
    let mut state = sample_state();
    let plan = state.project().views.values().find(|v| v.level().is_some()).map(|v| v.id).unwrap();
    state.active_view = Some(plan);
    crate::actions::activate_category_tool(&mut state, cc_core::Category::Mass);
    assert!(matches!(state.sketch.as_ref().map(|s| s.target), Some(Target::New { .. })));
    let circle = SketchCurve::Circle { center: glam::DVec2::new(30.0, 30.0), radius: 3.0 };
    state.sketch.as_mut().unwrap().add(vec![circle]);
    crate::commands::run(&mut state, &egui::Context::default(), UiCommand::SketchFinish);
    assert!(state.sketch.is_none(), "{}", state.message);
    let mass = *state.selection.iter().next().unwrap();
    let drawn = state.project().element(mass).and_then(|e| e.drawn_sketch()).cloned().unwrap();
    assert_eq!(drawn.curves, vec![circle]);

    // Edit Boundary opens the curves; a bigger circle draws the outline again.
    assert_eq!(UiCommand::EditBoundary.unavailable(&state), None);
    crate::commands::run(&mut state, &egui::Context::default(), UiCommand::EditBoundary);
    let s = state.sketch.as_mut().unwrap();
    assert_eq!((s.target, s.draw, s.curves.clone()), (Target::Edit { id: mass }, Draw::Select, vec![circle]));
    let bigger = SketchCurve::Circle { center: glam::DVec2::new(30.0, 30.0), radius: 4.0 };
    s.set_curve(0, bigger);
    crate::commands::run(&mut state, &egui::Context::default(), UiCommand::SketchFinish);
    let e = state.project().element(mass).unwrap();
    assert_eq!(e.drawn_sketch().unwrap().curves, vec![bigger]);

    // Inside a sketch, undo undoes the sketch's own changes; an open sketch does not finish.
    crate::commands::run(&mut state, &egui::Context::default(), UiCommand::EditBoundary);
    let s = state.sketch.as_mut().unwrap();
    s.selected.insert(0);
    crate::commands::run(&mut state, &egui::Context::default(), UiCommand::Delete);
    assert!(state.sketch.as_ref().unwrap().curves.is_empty());
    crate::commands::run(&mut state, &egui::Context::default(), UiCommand::Undo);
    assert_eq!(state.sketch.as_ref().unwrap().curves, vec![bigger]);
    state
        .sketch
        .as_mut()
        .unwrap()
        .add(vec![SketchCurve::Line { a: glam::DVec2::new(40.0, 40.0), b: glam::DVec2::new(41.0, 40.0) }]);
    crate::commands::run(&mut state, &egui::Context::default(), UiCommand::SketchFinish);
    assert!(state.sketch.is_some(), "an open line keeps the sketch open");
    assert_eq!(state.message, t!("sketch-open-end"));
    // In a sketch the modify tools act on its curves: those on the picked
    // curves need some, the others pick curves themselves.
    let s = state.sketch.as_mut().unwrap();
    s.selected.clear();
    assert_eq!(UiCommand::Move.unavailable(&state), Some(t!("msg-select-first")));
    assert_eq!(UiCommand::Offset.unavailable(&state), None);
    crate::commands::run(&mut state, &egui::Context::default(), UiCommand::TrimCorner);
    assert_eq!(state.sketch.as_ref().unwrap().draw, Draw::TrimCorner);
    assert!(UiCommand::TrimCorner.selected(&state) && !UiCommand::Split.selected(&state));
    state.sketch.as_mut().unwrap().selected.insert(0);
    crate::commands::run(&mut state, &egui::Context::default(), UiCommand::Rotate);
    assert_eq!(state.sketch.as_ref().unwrap().draw, Draw::Rotate);
    assert!(matches!(state.tool, crate::tools::Tool::Sketch { .. }), "the element tools stay out of the sketch");
    let revision = state.session.doc.revision();
    crate::commands::run(&mut state, &egui::Context::default(), UiCommand::SketchCancel);
    assert!(state.sketch.is_none() && state.tool == crate::tools::Tool::Select);
    assert_eq!(state.session.doc.revision(), revision, "cancel changes nothing");
}

/// A piece of the component environment is sketched in the same sketch as
/// outlines in plans: its ribbon, options, picked curve and Finish.
#[test]
fn component_pieces_are_sketched_with_the_sketch_tools() {
    use crate::commands::UiCommand;
    let mut state = sample_state();
    let table = state
        .project()
        .types_of(cc_core::Category::Component)
        .find(|t| matches!(&t.data, cc_core::TypeData::Component { component, .. } if component == "Mesa"))
        .map(|t| t.id)
        .unwrap();
    crate::component_env::open(&mut state, table);
    crate::commands::run(&mut state, &egui::Context::default(), UiCommand::CompExtrusion);
    assert!(UiCommand::CompExtrusion.selected(&state) && UiCommand::SketchEllipse.unavailable(&state).is_none());
    assert!(UiCommand::SketchPickWalls.unavailable(&state).is_some(), "walls are picked in plans only");
    let s = state.sketch.as_mut().unwrap();
    s.curves = vec![
        cc_core::sketch::SketchCurve::ellipse(glam::DVec2::new(0.0, 0.0), glam::DVec2::new(0.5, 0.3), 15.0),
        cc_core::sketch::SketchCurve::Circle { center: glam::DVec2::new(0.6, 0.0), radius: 0.1 },
    ];
    s.selected.insert(0);
    s.piece.as_mut().unwrap().view = Some(cc_views::ComponentViewKind::Plan);
    let mut harness = themed_harness(state, egui::vec2(1280.0, 820.0), egui::Theme::Light, |ui, state| {
        egui::Panel::top("ribbon").show(ui, |ui| crate::sketch_mode::ribbon(ui, state));
        egui::Panel::top("options").show(ui, |ui| ui.horizontal(|ui| crate::sketch_mode::options(ui, state)));
        egui::Panel::left("left").default_size(290.0).show(ui, |ui| crate::component_env::panel(ui, state));
        egui::CentralPanel::default().show(ui, |ui| crate::component_env::central(ui, state));
    });
    for _ in 0..3 {
        harness.run();
    }
    if let Some(path) = std::env::var_os("CC_RENDER_PIECE_SKETCH") {
        harness.render().unwrap().save(path).unwrap();
    }
    let title = t!("sketch-title-piece", "kind" => t!("comp-env-extrusion"));
    assert!(harness.query_by_label(&title).is_some(), "{title}");
    // The ribbon's Ellipse tool, and the picked ellipse's section in the panel.
    assert_eq!(harness.query_all_by_label(&t!("sketch-ellipse")).count(), 2);
    assert!(!UiCommand::Select.selected(harness.state()), "the sketch's tool, not the environment's");
    assert!(harness.query_by_label(&t!("comp-sketch-void")).is_some(), "the piece's options");
    // Two loops apart make no profile: the sketch stays open with the reason.
    crate::commands::run(harness.state_mut(), &egui::Context::default(), UiCommand::SketchFinish);
    assert!(harness.state().sketch.is_some());
    assert_eq!(harness.state().message, t!("sketch-apart"));
}

/// The component environment's side panel keeps its fields and buttons
/// clear of the panel's left border (they were drawn against it, cut).
#[test]
fn component_panel_is_inset() {
    let mut state = sample_state();
    let table = state
        .project()
        .types_of(cc_core::Category::Component)
        .find(|t| matches!(&t.data, cc_core::TypeData::Component { component, .. } if component == "Mesa"))
        .map(|t| t.id)
        .unwrap();
    crate::component_env::open(&mut state, table);
    if let Some(e) = state.component_env.as_mut() {
        e.pick = Some(crate::component_env::Pick::Piece(0));
    }
    let harness = themed_harness(state, egui::vec2(700.0, 700.0), egui::Theme::Light, |ui, state| {
        // The app's side panel has no margin of its own.
        egui::Panel::left("left").frame(egui::Frame::new()).default_size(290.0).show(ui, |ui| {
            let left = ui.max_rect().left();
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("panel-left"), left));
            crate::component_env::panel(ui, state)
        });
    });
    let panel_left: f32 = harness.ctx.data(|d| d.get_temp(egui::Id::new("panel-left"))).unwrap();
    for label in [format!("+ {}", t!("comp-env-add-param")), t!("action-delete")] {
        let inset = harness.get_by_label(&label).rect().min.x - panel_left;
        assert!(inset >= 6.0, "{label} starts {inset} from the panel's border");
    }
}

/// Issues #47 and #69: several elements of one category show their type,
/// level and shared parameters, a differing value marked; a change goes to
/// all of them in one step.
#[test]
fn several_elements_are_edited_together() {
    let mut state = sample_state();
    let columns: Vec<cc_core::ElementId> =
        state.project().elements.values().filter(|e| e.category() == cc_core::Category::Column).map(|e| e.id).collect();
    assert!(columns.len() >= 2, "the sample has columns");
    // One column lower than the others: the base offset differs.
    crate::engine_edit::element(&mut state, columns[0], None, |e| {
        if let cc_core::ElementKind::Column(c) = &mut e.kind {
            c.base_offset = -0.5;
        }
    });
    state.selection = columns.iter().copied().collect();
    let mut harness = themed_harness(state, egui::vec2(320.0, 700.0), egui::Theme::Light, |ui, state| {
        egui::Panel::left("left").default_size(300.0).show(ui, |ui| crate::properties::show(ui, state));
    });
    harness.run();
    if let Some(path) = std::env::var_os("CC_RENDER_MULTI") {
        harness.render().unwrap().save(path).unwrap();
    }
    assert!(harness.query_by_label(&t!("section-type")).is_some(), "the shared type and level");
    assert!(harness.query_by_label(&t!("section-constraints")).is_some(), "the shared parameters");
    // Down to the footings: every column, one undo step.
    let state = harness.state_mut();
    let depth = state.session.doc.undo_depth();
    let patch = serde_json::json!({"base_offset": -1.5});
    assert!(crate::engine_edit::elements(state, &columns, None, None, Some(patch)));
    assert_eq!(state.session.doc.undo_depth(), depth + 1);
    for id in &columns {
        let Some(cc_core::ElementKind::Column(c)) = state.project().element(*id).map(|e| &e.kind) else { panic!() };
        assert_eq!(c.base_offset, -1.5);
    }
}

/// The sweep tool: a project without a sweep type gets a baseboard when the
/// tool starts; the clicked points (closed, flipped, raised by the options)
/// make the element, and its properties show it.
#[test]
fn the_sweep_tool_draws_a_baseboard_around_a_room() {
    use crate::commands::UiCommand;
    let mut state = sample_state();
    let plan = state.project().views.values().find(|v| v.level().is_some()).map(|v| v.id).unwrap();
    state.active_view = Some(plan);
    assert!(state.project().default_type(cc_core::Category::Sweep).is_none());
    assert_eq!(UiCommand::Place(cc_core::Category::Sweep).unavailable(&state), None);
    crate::actions::activate_category_tool(&mut state, cc_core::Category::Sweep);
    let crate::tools::Tool::Sweep { type_id } = state.tool.clone() else {
        panic!("not the sweep tool: {:?}", state.tool)
    };
    state.sweep = crate::tools::SweepOptions { base_offset: 0.05, closed: true, flip: true };
    state.interaction.points =
        vec![glam::DVec2::new(60.0, 0.0), glam::DVec2::new(64.0, 0.0), glam::DVec2::new(64.0, 3.0)];
    let level = state.active_level().unwrap();
    crate::tools::finish_sweep(&mut state, level, type_id);
    assert!(state.interaction.points.is_empty());
    let sweep = *state.selection.iter().next().unwrap_or_else(|| panic!("{}", state.message));
    let e = state.project().element(sweep).unwrap();
    let cc_core::ElementKind::Sweep(s) = &e.kind else { panic!("not a sweep") };
    assert!(s.closed && s.flip && (s.base_offset - 0.05).abs() < 1e-12 && s.path.len() == 3);
    assert!(state.session.geom().solids.iter().any(|solid| solid.element == sweep));
    let mut harness = themed_harness(state, egui::vec2(360.0, 1600.0), egui::Theme::Light, |ui, state| {
        crate::properties::show(ui, state);
    });
    harness.run();
    // The sweep's own rows: its constraints and dimensions, with the two checkboxes (closed, flip).
    for key in ["section-constraints", "section-dimensions"] {
        assert!(harness.query_by_label(&crate::i18n::tr(key)).is_some(), "{key}");
    }
    assert!(harness.query_all_by_role(Role::CheckBox).count() >= 2);
}

/// A view's visual style in its properties: the active view with nothing
/// selected shows a combo box with its style, and a change made through the
/// app is an engine edit (one undo step).
#[test]
fn a_views_visual_style_is_chosen_in_its_properties() {
    let mut state = sample_state();
    let view = state.project().views.values().find(|v| matches!(v.kind, cc_core::ViewKind::ThreeD { .. })).unwrap().id;
    state.active_view = Some(view);
    state.selection.clear();
    assert_eq!(state.project().view(view).unwrap().style(), cc_core::VisualStyle::Shaded);
    crate::engine_edit::view(&mut state, view, None, |v| v.visual_style = Some(cc_core::VisualStyle::Textures));
    assert_eq!(state.project().view(view).unwrap().style(), cc_core::VisualStyle::Textures);
    let mut harness = themed_harness(state, egui::vec2(360.0, 1400.0), egui::Theme::Light, |ui, state| {
        crate::properties::show(ui, state);
    });
    harness.run();
    let shown = t!("style-textures");
    let combo = harness.query_all_by_role(Role::ComboBox).find(|n| n.value().as_deref() == Some(shown.as_str()));
    assert!(combo.is_some(), "the combo shows the style");
    harness.state_mut().session.execute("undo", serde_json::json!({})).unwrap();
    assert_eq!(harness.state().project().view(view).unwrap().style(), cc_core::VisualStyle::Shaded);
}

/// A railing on a stair: the railing tool recognizes stairs and ramps as
/// hosts, and the railing's properties take it off its host.
#[test]
fn a_hosted_railing_is_detached_from_its_properties() {
    let mut state = sample_state();
    let stair = state.session.execute("stair_create", serde_json::json!({"start": [70, 0]})).unwrap();
    let stair = cc_core::ElementId(stair["id"].as_u64().unwrap());
    assert!(crate::tools::railing_host(&state, stair));
    let added =
        state.session.execute("stair_railings_add", serde_json::json!({"id": stair.0, "near": [71.0, 5.0]})).unwrap();
    let railing = cc_core::ElementId(added["ids"][0].as_u64().unwrap());
    assert!(!crate::tools::railing_host(&state, railing));
    state.selection = [railing].into_iter().collect();
    let mut harness = themed_harness(state, egui::vec2(360.0, 1200.0), egui::Theme::Light, |ui, state| {
        crate::properties::show(ui, state);
    });
    harness.run();
    harness.get_by_role_and_label(Role::Button, &t!("action-railing-detach")).click();
    harness.run();
    let hosted = |h: &Harness<'_, AppState>| match &h.state().project().element(railing).unwrap().kind {
        cc_core::ElementKind::Railing(r) => r.stair.is_some(),
        _ => true,
    };
    assert!(!hosted(&harness));
}
