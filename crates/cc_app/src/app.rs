//! Top-level eframe application: layout, dock and keyboard shortcuts.

use crate::i18n::Language;
use crate::state::{AppState, PendingAction};
use crate::theme::PALETTE;
use crate::tools::Tool;
use crate::{actions, browser, canvas, dialogs, properties, ribbon, schedule_ui, sheet, t, theme, view3d};
use cc_core::{Document, Project, ViewCategory, ViewId};
use egui::{Event, Key, Modifiers, Ui, WidgetText};
use egui_dock::tab_viewer::OnCloseResponse;
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};

/// Command-line options.
#[derive(Debug, Default, Clone)]
pub struct Args {
    pub path: Option<std::path::PathBuf>,
    /// Views (by name or tab title) to open at startup.
    pub views: Vec<String>,
    pub language: Option<Language>,
}

pub struct ConstructApp {
    state: AppState,
    dock: DockState<ViewId>,
    /// Document generation the dock layout belongs to.
    dock_generation: u64,
    /// Autosave and crash recovery of the open project.
    recovery: crate::recovery::Recovery,
    /// What the interface last applied of the preferences (theme, zoom, font size).
    look: crate::preferences::Look,
    /// The window title last sent (see [`set_title`]).
    title: String,
}

/// Set the window title when it changed. Every viewport command asks egui
/// for another frame, so sending the title each frame kept the app
/// redrawing without pause while idle (high CPU on slow machines).
fn set_title(ctx: &egui::Context, last: &mut String, title: String) {
    if *last != title {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
        *last = title;
    }
}

impl ConstructApp {
    /// Create the app; see [`Args`] for the startup options.
    pub fn new(cc: &eframe::CreationContext<'_>, args: Args) -> Self {
        let Args { path, views, language } = args;
        crate::theme::apply(&cc.egui_ctx);
        // wgpu panics on an uncaptured GPU error (a validation error or a
        // failed allocation) by default; log it and keep the app (and the
        // open project) alive instead.
        if let Some(rs) = &cc.wgpu_render_state {
            // The first one also goes to the crash log (release builds on Windows have no console).
            let logged = std::sync::atomic::AtomicBool::new(false);
            rs.device.on_uncaptured_error(std::sync::Arc::new(move |e| {
                log::error!("GPU error: {e}");
                if !logged.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    crate::recovery::log_event(&format!("GPU error: {e}"));
                }
            }));
            rs.device.set_device_lost_callback(|reason, message| {
                if reason == wgpu::DeviceLostReason::Unknown {
                    log::error!("GPU device lost: {message}");
                    crate::recovery::log_event(&format!("GPU device lost: {message}"));
                }
            });
            let info = rs.adapter.get_info();
            log::info!("graphics: {} ({:?}, {})", info.name, info.backend, info.driver_info);
        }
        let stored = |key: &str| cc.storage.and_then(|s| s.get_string(key));
        // Command line, then saved preference, then the operating system
        // language; set first, since the placeholder project is named in it.
        crate::preferences::load_language(stored, language);
        let mut state = AppState::new(Document::new(actions::new_project()));
        crate::preferences::load(&mut state, stored, cc.egui_ctx.zoom_factor());
        let mut look = crate::preferences::Look::current(&cc.egui_ctx);
        crate::preferences::sync_look(&cc.egui_ctx, &mut state.prefs, &mut look);
        state.updates.start_if_due(&cc.egui_ctx, state.prefs.check_updates);
        // The executables an update replaced on Windows stay as `.old` until now.
        if let Ok(exe) = std::env::current_exe() {
            crate::update_install::remove_old_executables(&exe);
        }
        match path {
            Some(path) => actions::open_path(&mut state, path),
            None => {
                state.project_open = false;
                state.show_home = true;
            }
        }
        let mut named = Vec::new();
        for name in views {
            let by_title = state.project().views.keys().copied().find(|id| state.view_title(*id) == name);
            let found = by_title.or_else(|| state.project().views.values().find(|v| v.name == name).map(|v| v.id));
            match found {
                Some(id) if !named.contains(&id) => named.push(id),
                Some(_) => {}
                None => log::warn!("view not found: {name}"),
            }
        }
        let dock = if named.is_empty() { initial_dock(state.project()) } else { named_dock(&named) };
        let dock_generation = state.generation;
        let recovery = crate::recovery::Recovery::new(crate::recovery::folder());
        Self { state, dock, dock_generation, recovery, look, title: String::new() }
    }
}

/// First floor plan on the left, the 3D view on the right.
fn initial_dock(project: &Project) -> DockState<ViewId> {
    let plan = project.views_of(ViewCategory::FloorPlan).next().map(|v| v.id);
    let threed = project.views_of(ViewCategory::ThreeD).next().map(|v| v.id);
    let mut dock = DockState::new(plan.into_iter().collect());
    if let Some(t3) = threed {
        dock.main_surface_mut().split_right(NodeIndex::root(), 0.55, vec![t3]);
    }
    // Focus the plan so the properties palette shows it from the start.
    if let Some(path) = plan.and_then(|p| dock.find_tab(&p)) {
        dock.set_focused_node_and_surface(path.node_path());
    }
    dock
}

/// Views named on the command line: the first on the left, the second on
/// the right, any others as tabs next to the first.
fn named_dock(views: &[ViewId]) -> DockState<ViewId> {
    let left = views.iter().enumerate().filter(|(i, _)| *i != 1).map(|(_, v)| *v).collect();
    let mut dock = DockState::new(left);
    if let Some(&second) = views.get(1) {
        dock.main_surface_mut().split_right(NodeIndex::root(), 0.5, vec![second]);
    }
    if let Some(path) = views.first().and_then(|v| dock.find_tab(v)) {
        let _ = dock.set_active_tab(path);
        dock.set_focused_node_and_surface(path.node_path());
    }
    dock
}

/// Active tab of the focused leaf, if any.
fn active_tab(dock: &mut DockState<ViewId>) -> Option<ViewId> {
    // With no tabs left the main surface has no tree, and egui_dock's focus
    // lookups panic on it, so only ask when a tab exists.
    dock.iter_all_tabs().next()?;
    dock.find_active_focused().map(|(_, tab)| *tab)
}

/// Zoom the active view to fit its contents.
pub fn zoom_fit(state: &mut AppState) {
    let Some(view) = state.active_view else { return };
    if let Some(v) = state.view2d.get_mut(&view) {
        v.request_fit();
    }
    if let Some(v) = state.view3d.get_mut(&view) {
        v.request_fit();
    }
}

struct Tabs<'a> {
    state: &'a mut AppState,
    render_state: Option<&'a egui_wgpu::RenderState>,
}

impl TabViewer for Tabs<'_> {
    type Tab = ViewId;

    fn id(&mut self, tab: &mut ViewId) -> egui::Id {
        egui::Id::new(("view-tab", tab.0))
    }

    fn title(&mut self, tab: &mut ViewId) -> WidgetText {
        self.state.view_title(*tab).into()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut ViewId) {
        let Some(category) = self.state.project().view(*tab).map(|v| v.category()) else { return };
        match category {
            ViewCategory::ThreeD => view3d::show(ui, self.state, *tab, self.render_state),
            ViewCategory::Schedule => schedule_ui::show(ui, self.state, *tab),
            ViewCategory::Sheet => sheet::show(ui, self.state, *tab),
            _ => canvas::show(ui, self.state, *tab),
        }
    }

    fn on_close(&mut self, tab: &mut ViewId) -> OnCloseResponse {
        if let Some(v) = self.state.view3d.remove(tab) {
            self.state.textures_to_free.extend(v.texture);
        }
        OnCloseResponse::Close
    }

    /// Right-click on a tab: close the other views.
    fn context_menu(&mut self, ui: &mut Ui, tab: &mut ViewId, _path: egui_dock::NodePath) {
        let enabled = self.state.open_tabs > 1;
        if ui.add_enabled(enabled, egui::Button::new(t!("action-close-other-views"))).clicked() {
            self.state.close_others_than = Some(*tab);
            ui.close();
        }
    }

    fn scroll_bars(&self, _tab: &ViewId) -> [bool; 2] {
        [false, false]
    }
}

impl ConstructApp {
    fn shortcuts(&mut self, ctx: &egui::Context) {
        shortcuts(ctx, &mut self.state);
    }
}

/// The keyboard shortcuts of the app: Ctrl ones, plain keys and two-letter tool keys.
pub(crate) fn shortcuts(ctx: &egui::Context, state: &mut AppState) {
    let cmd = |i: &mut egui::InputState, k| i.consume_key(Modifiers::COMMAND, k);
    let cmd_shift = |i: &mut egui::InputState, k| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, k);
    let (undo, redo, save, save_as, open, new, close) = ctx.input_mut(|i| {
        let redo = cmd(i, Key::Y) || cmd_shift(i, Key::Z);
        let close = cmd(i, Key::W);
        (cmd(i, Key::Z), redo, cmd(i, Key::S), cmd_shift(i, Key::S), cmd(i, Key::O), cmd(i, Key::N), close)
    });
    if open {
        actions::guarded(state, PendingAction::Open);
    }
    if ctx.input_mut(|i| cmd_shift(i, Key::P)) {
        crate::palette::toggle(state);
    }
    if ctx.input_mut(|i| cmd(i, Key::Comma)) {
        crate::commands::run(state, ctx, crate::commands::UiCommand::Preferences);
    }
    if ctx.input_mut(|i| cmd(i, Key::F1)) {
        crate::commands::run(state, ctx, crate::commands::UiCommand::ToggleRibbon);
    }
    if new {
        actions::guarded(state, PendingAction::New);
    }
    // The remaining commands act on the project, which the home screen hides.
    if state.show_home || !state.project_open {
        return;
    }
    if close {
        actions::guarded(state, PendingAction::Close);
    }
    if undo && state.interaction.is_idle() {
        let _ = state.session.run(cc_engine::commands::project::Undo {});
    }
    if redo {
        let _ = state.session.run(cc_engine::commands::project::Redo {});
    }
    if save_as {
        actions::save(state, true);
    } else if save {
        actions::save(state, false);
    }

    // Plain keys only when no text field has focus.
    if ctx.memory(|m| m.focused().is_some()) {
        return;
    }
    let (escape, delete, backspace, space, events) = ctx.input(|i| {
        let keys = (i.key_pressed(Key::Escape), i.key_pressed(Key::Delete), i.key_pressed(Key::Backspace));
        (keys.0, keys.1, keys.2, i.key_pressed(Key::Space), i.events.clone())
    });
    // Space turns or flips the selection (tools use it for their own preview).
    if space && state.tool == Tool::Select && crate::tools::rotate_selection(state) {
        state.message = t!("msg-rotated");
    }
    // An open sketch has its own picks and selection.
    if state.sketch.is_some() {
        if escape {
            crate::sketch_mode::escape(state);
        }
        if delete && let Some(s) = state.sketch.as_mut() {
            s.delete_selected();
        }
        return;
    }
    // The component environment has its own tools and selection.
    if state.component_env.is_some() {
        if escape {
            crate::component_env::escape(state);
        }
        if delete {
            crate::component_env::delete_pick(state);
        }
        return;
    }
    if escape {
        if !state.typed.is_empty() || state.typed.angle_active {
            state.typed.clear();
        } else if !state.interaction.is_idle() {
            state.interaction = Default::default();
        } else if state.tool != Tool::Select {
            state.set_tool(Tool::Select);
        } else {
            state.selection.clear();
            state.sheet_selection = None;
        }
        state.key_buffer.clear();
    }
    if delete {
        match state.sheet_selection {
            Some((sheet, index)) if state.active_view == Some(sheet) => actions::remove_viewport(state, sheet, index),
            _ => state.delete_selection(),
        }
    }
    let typing_length = state.typing_segment();
    // Backspace edits a typed length, or else steps a chain back one segment.
    if backspace {
        if typing_length && !state.typed.is_empty() {
            state.typed.pop();
        } else {
            crate::tools::chain_back(state);
        }
    }
    for event in &events {
        match event {
            Event::Copy => {
                crate::clipboard::copy(ctx, state);
            }
            Event::Cut => crate::clipboard::cut(ctx, state),
            Event::Paste(_) => crate::clipboard::paste(state),
            _ => {}
        }
    }
    for event in events {
        let Event::Text(text) = event else { continue };
        for ch in text.chars() {
            if typing_length && (ch.is_ascii_digit() || ch == ',' || ch == '.') {
                state.typed.push(ch);
                continue;
            }
            // Feet and inch marks and fractions (`12'6 1/2"`); a space or a
            // dash only inside a length already started.
            let started = !state.typed.length.is_empty() && !state.typed.angle_active;
            if typing_length
                && cc_views::format::imperial()
                && (matches!(ch, '\'' | '"' | '/') || (started && matches!(ch, ' ' | '-')))
            {
                state.typed.push(ch);
                continue;
            }
            if !ch.is_ascii_alphabetic() {
                continue;
            }
            state.key_buffer.push(ch.to_ascii_uppercase());
            if state.key_buffer.len() > 2 {
                state.key_buffer.remove(0);
            }
            // Two-letter shortcuts come from the command table, as the tooltips show them.
            if let Some(command) = crate::commands::UiCommand::from_keys(&state.key_buffer) {
                crate::commands::invoke(state, ctx, command);
                state.key_buffer.clear();
            }
        }
    }
}

impl ConstructApp {
    fn handle_close(&mut self, ctx: &egui::Context) {
        let state = &mut self.state;
        if state.quit_confirmed {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if ctx.input(|i| i.viewport().close_requested()) && state.session.doc.is_dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            state.dialogs.confirm = Some(PendingAction::Quit);
        }
    }

    fn sync_dock(&mut self) {
        sync_tabs(&mut self.dock, &mut self.state);
    }
}

impl eframe::App for ConstructApp {
    /// While a segment is drawn, Tab switches the typed field (length or
    /// angle) instead of moving keyboard focus between widgets.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if !self.state.typing_segment() || ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let is_tab = |e: &Event| matches!(e, Event::Key { key: Key::Tab, .. });
        let presses =
            raw_input.events.iter().filter(|e| matches!(e, Event::Key { key: Key::Tab, pressed: true, .. })).count();
        raw_input.events.retain(|e| !is_tab(e));
        if presses % 2 == 1 {
            self.state.typed.angle_active = !self.state.typed.angle_active;
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        crate::preferences::save(&self.state, |key, value| storage.set_string(key, value));
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        crate::gpu::note_frame();
        // A panic in a frame ends the app, as before, but writes a last
        // recovery copy of the open project first (the panic hook has
        // logged it to the crash log).
        let shown = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.frame(ui, frame)));
        if let Err(payload) = shown {
            self.recovery.write_now(&self.state);
            std::panic::resume_unwind(payload);
        }
    }

    /// A clean exit: the changes were saved or discarded, so the copy goes.
    fn on_exit(&mut self) {
        self.recovery.exit();
        // Update Now left the new version to start, or its installer to run.
        if let Some(next) = self.state.updates.on_exit.take() {
            crate::update_install::finish(&next);
        }
    }
}

impl ConstructApp {
    fn frame(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        crate::preferences::sync_look(&ctx, &mut self.state.prefs, &mut self.look);
        crate::theme::sync(&ctx);
        self.state.sync();
        self.recovery.set_interval(self.state.prefs.autosave_interval());
        self.recovery.tick(&self.state);
        self.state.updates.poll();
        self.state.follow_selection();
        if !self.state.textures_to_free.is_empty()
            && let Some(rs) = frame.wgpu_render_state()
        {
            let mut r = rs.renderer.write();
            for tex in self.state.textures_to_free.drain(..) {
                r.free_texture(&tex);
            }
        }
        if let Some(rs) = frame.wgpu_render_state() {
            crate::thumbnails::process(&mut self.state, rs);
        }
        self.shortcuts(&ctx);
        crate::cad_ui::dropped_files(&ctx, &mut self.state);
        if self.dock_generation != self.state.generation {
            self.dock = initial_dock(self.state.project());
            self.dock_generation = self.state.generation;
        }
        self.handle_close(&ctx);
        self.sync_dock();

        let title = if self.state.project_open {
            let dirty = if self.state.session.doc.is_dirty() { " *" } else { "" };
            format!("ConstructCAD — {}{dirty}", self.state.project().info.name)
        } else {
            "ConstructCAD".to_string()
        };
        set_title(&ctx, &mut self.title, title);

        if self.state.show_home {
            crate::home::show(ui, &mut self.state);
            dialogs::show(&ctx, &mut self.state);
            self.recovery.dialog(&ctx, &mut self.state);
            crate::palette::show(&ctx, &mut self.state);
            return;
        }
        let state = &mut self.state;
        let bar = |fill, y| egui::Frame::new().fill(fill).inner_margin(egui::Margin::symmetric(10, y));
        let env = state.component_env.is_some();
        egui::Panel::top("ribbon").frame(bar(PALETTE.panel, 6).stroke(egui::Stroke::new(1.0, PALETTE.border))).show(
            ui,
            |ui| {
                if state.sketch.is_some() {
                    crate::sketch_mode::ribbon(ui, state)
                } else if env {
                    crate::component_env::ribbon(ui, state)
                } else {
                    ribbon::ribbon(ui, state)
                }
            },
        );
        egui::Panel::top("options").frame(bar(PALETTE.surface, 5).stroke(egui::Stroke::new(1.0, PALETTE.border))).show(
            ui,
            |ui| {
                if env && state.sketch.is_some() {
                    ui.horizontal(|ui| crate::sketch_mode::options(ui, state));
                } else if env {
                    crate::component_env::options(ui, state)
                } else {
                    ribbon::options_bar(ui, state)
                }
            },
        );
        egui::Panel::bottom("status")
            .frame(bar(PALETTE.surface, 3).stroke(egui::Stroke::new(1.0, PALETTE.border)))
            .show(ui, |ui| ribbon::status_bar(ui, state));
        egui::Panel::left("left")
            .frame(egui::Frame::new().fill(PALETTE.panel).stroke(egui::Stroke::new(1.0, PALETTE.border)))
            .resizable(true)
            .default_size(290.0)
            .size_range(220.0..=520.0)
            .show(ui, |ui| {
                if env {
                    crate::component_env::panel(ui, state);
                    return;
                }
                let h = ui.available_height();
                egui::Panel::top("browser")
                    .frame(egui::Frame::new().fill(PALETTE.panel))
                    .resizable(true)
                    .default_size(h * 0.5)
                    .size_range(120.0..=h - 120.0)
                    .show(ui, |ui| browser::show(ui, state));
                properties::show(ui, state);
            });
        let render_state = frame.wgpu_render_state();
        let central = egui::Frame::new().fill(PALETTE.app_bg).inner_margin(egui::Margin::same(4));
        egui::CentralPanel::no_frame().frame(central).show(ui, |ui| {
            if self.state.component_env.is_some() {
                crate::component_env::central(ui, &mut self.state);
                return;
            }
            if self.dock.iter_all_tabs().next().is_none() {
                ui.centered_and_justified(|ui| ui.label(t!("msg-no-views-open")));
            }
            DockArea::new(&mut self.dock)
                .style(theme::dock_style(ui.style()))
                .show_leaf_collapse_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_inside(ui, &mut Tabs { state: &mut self.state, render_state });
        });
        dialogs::show(&ctx, &mut self.state);
        self.recovery.dialog(&ctx, &mut self.state);
        crate::palette::show(&ctx, &mut self.state);
    }
}

/// Bring the view tabs in step with the app: drop tabs of deleted views
/// (and, after Close Inactive Views, all but one), open requested views,
/// and record the active view and how many tabs are open.
fn sync_tabs(dock: &mut DockState<ViewId>, state: &mut AppState) {
    // Drop tabs whose view no longer exists (deleted or undone).
    let project = &state.session.doc.project;
    // Close Inactive Views (or Close Other Views on a tab) keeps one tab.
    let only = state.close_others_than.take();
    let mut removed = Vec::new();
    dock.retain_tabs(|tab| {
        let keep = project.views.contains_key(tab) && only.is_none_or(|v| v == *tab);
        if !keep {
            removed.push(*tab);
        }
        keep
    });
    for tab in removed {
        if let Some(v) = state.view3d.remove(&tab) {
            state.textures_to_free.extend(v.texture);
        }
    }
    // The tab kept takes the focus (closing the others may leave none focused).
    if let Some(kept) = only {
        state.open_requests.insert(0, kept);
    }
    for id in std::mem::take(&mut state.open_requests) {
        if state.project().view(id).is_none() {
            continue;
        }
        match dock.find_tab(&id) {
            Some(path) => {
                let _ = dock.set_active_tab(path);
                dock.set_focused_node_and_surface(path.node_path());
            }
            None => dock.push_to_focused_leaf(id),
        }
    }
    state.active_view = active_tab(dock);
    state.open_tabs = dock.iter_all_tabs().count();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An idle app settles: the title is sent only when it changes. Sent
    /// every frame, as it was, each frame asked for the next one and the
    /// app redrew without pause.
    #[test]
    fn an_unchanged_title_lets_the_app_settle() {
        // Frames until egui asks for no other (at most 10).
        let settle = |send: &mut dyn FnMut(&egui::Context)| {
            let ctx = egui::Context::default();
            (1..=10).find(|_| {
                let mut out = ctx.run_ui(Default::default(), |ui| send(ui.ctx()));
                out.textures_delta.clear();
                !ctx.has_requested_repaint()
            })
        };
        let mut last = String::new();
        assert!(settle(&mut |ctx| set_title(ctx, &mut last, "ConstructCAD".into())).is_some_and(|n| n <= 3));
        let every_frame =
            |ctx: &egui::Context| ctx.send_viewport_cmd(egui::ViewportCommand::Title("ConstructCAD".into()));
        assert_eq!(settle(&mut { every_frame }), None, "sending it every frame never settles");
    }

    #[test]
    fn closing_the_last_tab_leaves_no_active_view() {
        let mut dock = DockState::new(vec![ViewId(1)]);
        let path = dock.find_tab(&ViewId(1)).unwrap();
        dock.set_focused_node_and_surface(path.node_path());
        assert_eq!(active_tab(&mut dock), Some(ViewId(1)));
        dock.remove_tab(path);
        // `sync_dock` prunes tabs every frame, which turns the emptied main surface into `Empty`.
        dock.retain_tabs(|_| true);
        assert_eq!(active_tab(&mut dock), None);
        // Opening a view again recreates the tree.
        dock.push_to_focused_leaf(ViewId(2));
        assert_eq!(active_tab(&mut dock), Some(ViewId(2)));
    }

    /// Close Inactive Views keeps the active tab only; Close Other Views on a
    /// tab keeps that one (#89).
    #[test]
    fn closing_inactive_views_keeps_one_tab() {
        let mut state = AppState::new(cc_core::Document::new(crate::home::sample_project("casa_simples").unwrap()));
        state.project_open = true;
        state.show_home = false;
        let views: Vec<ViewId> = state.project().views.keys().copied().take(4).collect();
        let mut dock = DockState::new(vec![views[0]]);
        let first = dock.find_tab(&views[0]).unwrap();
        dock.set_focused_node_and_surface(first.node_path());
        for v in &views {
            state.open_view(*v);
        }
        sync_tabs(&mut dock, &mut state);
        assert_eq!(state.open_tabs, 4);
        let active = state.active_view.unwrap();
        crate::commands::run(&mut state, &egui::Context::default(), crate::commands::UiCommand::CloseInactiveViews);
        sync_tabs(&mut dock, &mut state);
        assert_eq!(dock.iter_all_tabs().map(|(_, t)| *t).collect::<Vec<_>>(), [active]);
        assert_eq!((state.open_tabs, state.active_view), (1, Some(active)));
        assert!(crate::commands::UiCommand::CloseInactiveViews.unavailable(&state).is_some(), "one view left");
        // From a tab's menu: that tab stays, even if it is not the active one.
        for v in &views {
            state.open_view(*v);
        }
        sync_tabs(&mut dock, &mut state);
        state.close_others_than = Some(views[1]);
        sync_tabs(&mut dock, &mut state);
        assert_eq!(dock.iter_all_tabs().map(|(_, t)| *t).collect::<Vec<_>>(), [views[1]]);
    }
}
