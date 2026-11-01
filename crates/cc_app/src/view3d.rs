//! Interactive 3D view backed by [`cc_render::Renderer3D`].

use crate::canvas::{ZoomCommand, ZoomNav};
use crate::state::AppState;
use cc_core::{CameraState, ViewId, ViewKind};
use cc_render::{Renderer3D, SceneMesh, pick_ray};
use egui::{PointerButton, Sense, Ui};
use glam::DVec3;
use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

pub struct View3DState {
    pub camera: CameraState,
    renderer: Option<Renderer3D>,
    pub texture: Option<egui::TextureId>,
    mesh_key: Option<(u64, u64, u64)>,
    /// Triangles and edges of the geometry shown, with the revision they were made at.
    tessellation: Option<(u64, cc_render::Tessellation)>,
    /// Camera changed since it was last written back to the model.
    camera_dirty: bool,
    fit_requested: bool,
    /// The camera of the last fit and the aspect it fitted: while the camera
    /// is left as fitted, a resized view fits it again.
    auto_fit: Option<(CameraState, f32)>,
    /// Camera transition started by the navigation cube.
    anim: Option<Transition>,
    /// Earlier and undone cameras for "previous" / "next" pan/zoom.
    back: Vec<CameraState>,
    forward: Vec<CameraState>,
    /// Time of the last wheel zoom: a pause starts a new history step.
    last_wheel: f64,
    /// "Zoom in region" waits for a window; the drag's start once it began.
    zoom_region: bool,
    region_start: Option<egui::Pos2>,
    /// Where a window or crossing selection drag started.
    select_start: Option<egui::Pos2>,
    /// Snap points and previews of the modify tools.
    pub(crate) picks: crate::modify3d::PickCache,
}

/// Cameras kept for "previous" and "next".
const HISTORY: usize = 50;
/// Half the vertical field of view.
const HALF_FOV_DEG: f64 = 22.5;

/// Smooth turn of the camera to new angles.
#[derive(Debug, Clone, Copy)]
struct Transition {
    from_yaw: f64,
    from_pitch: f64,
    /// Yaw change along the shortest way.
    delta_yaw: f64,
    to_pitch: f64,
    start: f64,
}

const TRANSITION_SECONDS: f64 = 0.35;

impl View3DState {
    fn new(camera: CameraState) -> Self {
        Self {
            camera,
            renderer: None,
            texture: None,
            mesh_key: None,
            tessellation: None,
            camera_dirty: false,
            fit_requested: false,
            auto_fit: None,
            anim: None,
            back: Vec::new(),
            forward: Vec::new(),
            last_wheel: f64::NEG_INFINITY,
            zoom_region: false,
            region_start: None,
            select_start: None,
            picks: Default::default(),
        }
    }

    /// Remember the camera before changing it.
    fn remember(&mut self) {
        if self.back.last() != Some(&self.camera) {
            self.back.push(self.camera);
            if self.back.len() > HISTORY {
                self.back.remove(0);
            }
        }
        self.forward.clear();
    }

    /// Frame the model on the next frame, remembering the camera.
    pub fn request_fit(&mut self) {
        self.remember();
        self.fit_requested = true;
    }

    pub fn nav(&self) -> ZoomNav {
        ZoomNav { back: !self.back.is_empty(), forward: !self.forward.is_empty(), sheet_size: false }
    }

    /// Run a zoom command of the view toolbar ("fit all" is handled by the caller).
    pub fn apply(&mut self, cmd: ZoomCommand) {
        self.anim = None;
        match cmd {
            ZoomCommand::Region => {
                self.zoom_region = true;
                self.region_start = None;
            }
            ZoomCommand::OutTwo => {
                self.remember();
                self.camera.distance = (self.camera.distance * 2.0).clamp(0.5, 5000.0);
            }
            ZoomCommand::Fit | ZoomCommand::FitAll => self.request_fit(),
            // No paper scale in 3D.
            ZoomCommand::SheetSize => return,
            ZoomCommand::Previous | ZoomCommand::Next => {
                let (from, to) = if cmd == ZoomCommand::Previous {
                    (&mut self.back, &mut self.forward)
                } else {
                    (&mut self.forward, &mut self.back)
                };
                let Some(camera) = from.pop() else { return };
                to.push(self.camera);
                self.camera = camera;
            }
        }
        self.camera_dirty = true;
    }

    /// Zoom so that `window` (screen) fills the view: its center becomes the
    /// target (moved in the plane through the target facing the camera) and
    /// the distance shrinks with the window.
    fn zoom_to_window(&mut self, window: egui::Rect, view: egui::Rect) {
        let cam = &mut self.camera;
        let fwd = cam.forward();
        let right = fwd.cross(DVec3::Z).normalize_or_zero();
        let up = right.cross(fwd);
        let k = cam.distance * 2.0 * HALF_FOV_DEG.to_radians().tan() / view.height().max(1.0) as f64;
        let d = window.center() - view.center();
        cam.target += (right * d.x as f64 - up * d.y as f64) * k;
        let fraction = (window.width() / view.width().max(1.0)).max(window.height() / view.height().max(1.0));
        cam.distance = (cam.distance * fraction as f64).clamp(0.5, 5000.0);
        self.camera_dirty = true;
    }

    /// "Zoom in region": drag a window; a click or Esc cancels. Returns true
    /// while it takes the pointer (orbit and picking wait meanwhile).
    fn region(&mut self, ui: &Ui, resp: &egui::Response, painter: &egui::Painter, rect: egui::Rect) -> bool {
        if !self.zoom_region {
            return false;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) || resp.clicked() {
            self.zoom_region = false;
            self.region_start = None;
            return true;
        }
        if resp.drag_started_by(PointerButton::Primary) {
            self.region_start = ui.input(|i| i.pointer.press_origin());
        }
        let (Some(a), Some(b)) = (self.region_start, resp.interact_pointer_pos().or(resp.hover_pos())) else {
            return true;
        };
        let window = egui::Rect::from_two_pos(a, b);
        let accent = crate::theme::PALETTE.accent;
        painter.rect_filled(window, 0.0, accent.gamma_multiply(0.08));
        painter.rect_stroke(window, 0.0, egui::Stroke::new(1.0, accent), egui::StrokeKind::Middle);
        if resp.drag_stopped() {
            self.zoom_region = false;
            self.region_start = None;
            if window.width() > 4.0 && window.height() > 4.0 {
                self.remember();
                self.zoom_to_window(window, rect);
            }
        }
        true
    }

    fn turn_to(&mut self, yaw: f64, pitch: f64, now: f64) {
        let delta_yaw = (yaw - self.camera.yaw_deg + 540.0).rem_euclid(360.0) - 180.0;
        self.anim = Some(Transition {
            from_yaw: self.camera.yaw_deg,
            from_pitch: self.camera.pitch_deg,
            delta_yaw,
            to_pitch: pitch,
            start: now,
        });
    }

    /// Advance a running transition; returns whether one is running.
    fn animate(&mut self, now: f64) -> bool {
        let Some(a) = self.anim else { return false };
        let t = ((now - a.start) / TRANSITION_SECONDS).clamp(0.0, 1.0);
        let e = t * t * (3.0 - 2.0 * t);
        self.camera.yaw_deg = a.from_yaw + a.delta_yaw * e;
        self.camera.pitch_deg = a.from_pitch + (a.to_pitch - a.from_pitch) * e;
        self.camera_dirty = true;
        if t >= 1.0 {
            self.camera.yaw_deg = self.camera.yaw_deg.rem_euclid(360.0);
            self.anim = None;
        }
        true
    }
}

/// How view graphics show in 3D: the surface color (a solid surface fill or
/// its color), transparency, halftone and the projection line color for edges.
fn solid_style(g: &cc_core::GraphicOverride) -> cc_render::SolidStyle {
    let fill = g.surface_fill;
    let color = match fill.pattern {
        Some(cc_core::Pattern::None) => Some([255, 255, 255]),
        _ => fill.color,
    };
    cc_render::SolidStyle {
        color,
        transparency: g.transparency,
        halftone: g.halftone == Some(true),
        edges: g.projection_lines.color,
    }
}

fn selection_hash(sel: &BTreeSet<cc_core::ElementId>) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    sel.hash(&mut h);
    h.finish()
}

/// Zoom the camera to frame the whole model.
pub fn fit_camera(camera: &mut CameraState, geom: &cc_geom::ModelGeometry, aspect: f32) {
    let Some((lo, hi)) = geom.bounds() else { return };
    camera.target = (lo + hi) * 0.5;
    let radius = ((hi - lo).length() * 0.5).max(1.0);
    // The narrower of the vertical (45°) and horizontal fields of view limits the fit.
    let half_v = HALF_FOV_DEG.to_radians();
    let half_h = (half_v.tan() * aspect as f64).atan();
    camera.distance = radius / half_v.min(half_h).sin() * 1.05;
}

pub fn show(ui: &mut Ui, state: &mut AppState, view_id: ViewId, render_state: Option<&egui_wgpu::RenderState>) {
    let Some(ViewKind::ThreeD { camera, analytical, rebar }) = state.project().view(view_id).map(|v| v.kind.clone())
    else {
        return;
    };
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let rect = resp.rect;
    let aspect = rect.width() / rect.height().max(1.0);
    let Some(rs) = render_state else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            crate::t!("msg-no-gpu"),
            egui::FontId::proportional(16.0),
            egui::Color32::GRAY,
        );
        return;
    };

    // The geometry of the view's phase.
    state.session.view_geometry(view_id);
    let geom = state.session.geom_for(view_id);
    let v3 = state.view3d.entry(view_id).or_insert_with(|| {
        let mut v = View3DState::new(camera);
        if geom.bounds().is_some() && camera == CameraState::default() {
            fit_camera(&mut v.camera, geom, aspect);
            v.auto_fit = Some((v.camera, aspect));
        }
        v
    });
    if v3.fit_requested {
        fit_camera(&mut v3.camera, geom, aspect);
        v3.auto_fit = Some((v3.camera, aspect));
        v3.fit_requested = false;
        v3.camera_dirty = true;
    } else if let Some((camera, fitted)) = v3.auto_fit
        && camera == v3.camera
        && (fitted - aspect).abs() > 1e-3
    {
        fit_camera(&mut v3.camera, geom, aspect);
        v3.auto_fit = Some((v3.camera, aspect));
    }

    // Navigation: left-drag or Shift+middle orbits, middle/right-drag pans, wheel zooms.
    let now = ui.input(|i| i.time);
    if v3.animate(now) {
        ui.ctx().request_repaint();
    }
    let zooming = v3.region(ui, &resp, &painter, rect);
    if resp.dragged() {
        v3.anim = None;
    }
    // A selection box: a left drag with the Select tool, or Shift+drag, as the preference says.
    let shift = ui.input(|i| i.modifiers.shift);
    if resp.drag_started_by(PointerButton::Primary)
        && !zooming
        && state.tool == crate::tools::Tool::Select
        && state.prefs.select_drag_3d != shift
    {
        v3.select_start = ui.input(|i| i.pointer.press_origin());
    }
    if resp.drag_started() && !zooming && v3.select_start.is_none() {
        v3.remember();
    }
    let scroll = if resp.hovered() { ui.input(|i| i.smooth_scroll_delta.y) as f64 } else { 0.0 };
    if scroll != 0.0 {
        if now - v3.last_wheel > 0.6 {
            v3.remember();
        }
        v3.last_wheel = now;
    }
    if resp.double_clicked_by(PointerButton::Middle) {
        v3.remember();
    }
    let cam = &mut v3.camera;
    let d = resp.drag_delta();
    if zooming || v3.select_start.is_some() {
        // The zoom window and the selection box are not an orbit.
    } else if resp.dragged_by(PointerButton::Primary) || (shift && resp.dragged_by(PointerButton::Middle)) {
        cam.yaw_deg += d.x as f64 * 0.4;
        cam.pitch_deg = (cam.pitch_deg + d.y as f64 * 0.4).clamp(-89.0, 89.0);
        v3.camera_dirty = true;
    } else if resp.dragged_by(PointerButton::Middle) || resp.dragged_by(PointerButton::Secondary) {
        let fwd = cam.forward();
        let right = fwd.cross(DVec3::Z).normalize_or_zero();
        let up = right.cross(fwd);
        let k = cam.distance * 2.0 * HALF_FOV_DEG.to_radians().tan() / rect.height().max(1.0) as f64;
        cam.target += (-right * d.x as f64 + up * d.y as f64) * k;
        v3.camera_dirty = true;
    }
    if scroll != 0.0 {
        cam.distance = (cam.distance * (-scroll * 0.0025).exp()).clamp(0.5, 5000.0);
        v3.camera_dirty = true;
    }
    if resp.double_clicked_by(PointerButton::Middle) {
        fit_camera(cam, geom, aspect);
        v3.camera_dirty = true;
    }

    // Picking (the Demolish tool takes the clicked element instead, once the view state is released).
    let mut demolish_pick = None;
    let mut opening_pick = None;
    // The modify tools that work in 3D take the clicks themselves (see `modify3d`).
    let modifying = crate::modify3d::works_in_3d(&state.tool);
    if resp.clicked()
        && !zooming
        && !modifying
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let nx = (pos.x - rect.left()) / rect.width() * 2.0 - 1.0;
        let ny = 1.0 - (pos.y - rect.top()) / rect.height() * 2.0;
        let ray = pick_ray(&v3.camera, aspect, nx, ny);
        let ctrl = ui.input(|i| i.modifiers.command);
        let (project, temporary) = (&state.session.doc.project, state.session.temporary(view_id));
        let picked = match project.view(view_id) {
            Some(view) => {
                let mut resolver = cc_views::graphics::Resolver::new(project, geom, view, temporary);
                geom.pick_where(ray, &mut |e| resolver.style(e).is_some())
            }
            None => geom.pick(ray, &[]),
        };
        let demolishing = state.tool == crate::tools::Tool::Demolish;
        let placing = matches!(state.tool, crate::tools::Tool::Hosted { .. });
        match picked {
            _ if demolishing => demolish_pick = Some(picked.map(|(id, _)| id)),
            Some((id, t)) if placing && project.element(id).is_some_and(|e| e.as_wall().is_some()) => {
                let hit = ray.origin + ray.dir * t;
                opening_pick = Some((id, glam::DVec2::new(hit.x, hit.y)));
            }
            _ if placing => state.message = crate::t!("hint-hosted"),
            Some((id, _)) if ctrl => {
                if !state.selection.remove(&id) {
                    state.selection.insert(id);
                }
            }
            Some((id, _)) => state.selection = BTreeSet::from([id]),
            None if !ctrl => state.selection.clear(),
            None => {}
        }
    }

    // Upload geometry when the model or selection changed; a new selection
    // or temporary visibility only repaints the triangles and edges kept.
    let revision = state.session.doc.revision();
    let key = (revision, selection_hash(&state.selection), state.session.temporary_revision());
    let renderer = v3.renderer.get_or_insert_with(|| Renderer3D::new(&rs.device));
    if v3.mesh_key != Some(key) {
        let geom = state.session.geom_for(view_id);
        if v3.tessellation.as_ref().is_none_or(|(r, _)| *r != revision) {
            v3.tessellation = Some((revision, cc_render::Tessellation::new(geom)));
        }
        let Some((_, tess)) = &v3.tessellation else { return };
        let mesh = if analytical {
            let model = cc_geom::analytical::analytical_model(&state.session.doc.project, geom);
            SceneMesh::build_analytical(tess, &model, &state.selection)
        } else if rebar {
            SceneMesh::build_rebar(geom, tess, &state.selection)
        } else {
            let project = &state.session.doc.project;
            match project.view(view_id) {
                Some(view) => {
                    let temporary = state.session.temporary(view_id);
                    let mut resolver = cc_views::graphics::Resolver::new(project, geom, view, temporary);
                    let style = &mut |e| resolver.style(e).map(|g| solid_style(&g));
                    SceneMesh::build_styled(tess, &state.selection, style, view.detail, view.style())
                }
                None => SceneMesh::build_plain(tess, &state.selection),
            }
        };
        renderer.upload(&rs.device, &mesh);
        v3.mesh_key = Some(key);
    }
    let ppp = ui.ctx().pixels_per_point();
    let size = [(rect.width() * ppp).round() as u32, (rect.height() * ppp).round() as u32];
    let Some((texture_view, recreated)) = renderer.render(&rs.device, &rs.queue, size, &v3.camera) else { return };
    {
        let mut egui_renderer = rs.renderer.write();
        match v3.texture {
            None => {
                v3.texture =
                    Some(egui_renderer.register_native_texture(&rs.device, texture_view, wgpu::FilterMode::Linear))
            }
            Some(id) if recreated => egui_renderer.update_egui_texture_from_wgpu_texture(
                &rs.device,
                texture_view,
                wgpu::FilterMode::Linear,
                id,
            ),
            Some(_) => {}
        }
    }
    if let Some(tex) = v3.texture {
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        painter.image(tex, rect, uv, egui::Color32::WHITE);
    }
    if let Some(start) = v3.select_start {
        let cur = resp.interact_pointer_pos().or(resp.hover_pos()).unwrap_or(start);
        crate::tools::select_box(&painter, start, cur);
        if resp.drag_stopped() {
            v3.select_start = None;
            let ndc = |p: egui::Pos2| {
                let x = (p.x - rect.left()) / rect.width().max(1.0) * 2.0 - 1.0;
                let y = 1.0 - (p.y - rect.top()) / rect.height().max(1.0) * 2.0;
                glam::DVec2::new(x as f64, y as f64)
            };
            let (a, b, window) = (ndc(start), ndc(cur), cur.x >= start.x);
            let (project, temporary) = (&state.session.doc.project, state.session.temporary(view_id));
            let geom = state.session.geom_for(view_id);
            let ids = match project.view(view_id) {
                Some(view) => {
                    let mut resolver = cc_views::graphics::Resolver::new(project, geom, view, temporary);
                    let mut shown = |e| resolver.style(e).is_some();
                    cc_render::elements_in_rect(&geom.solids, &v3.camera, aspect, a, b, window, &mut shown)
                }
                None => cc_render::elements_in_rect(&geom.solids, &v3.camera, aspect, a, b, window, &mut |_| true),
            };
            if !ui.input(|i| i.modifiers.command) {
                state.selection.clear();
            }
            state.selection.extend(ids);
        }
    }
    let cube = crate::view_cube::show(ui, rect, &v3.camera);
    if cube.look_from.is_some() || cube.home {
        v3.remember();
    }
    if let Some(dir) = cube.look_from {
        let (yaw, pitch) = crate::view_cube::angles_from(dir, v3.camera.yaw_deg);
        v3.turn_to(yaw, pitch, now);
        ui.ctx().request_repaint();
    }
    if cube.home {
        let home = CameraState::default();
        fit_camera(&mut v3.camera, state.session.geom_for(view_id), aspect);
        v3.turn_to(home.yaw_deg, home.pitch_deg, now);
        ui.ctx().request_repaint();
    }
    if let Some(d) = cube.orbit {
        v3.anim = None;
        v3.camera.yaw_deg += d.x as f64 * 0.4;
        v3.camera.pitch_deg = (v3.camera.pitch_deg + d.y as f64 * 0.4).clamp(-89.0, 89.0);
        v3.camera_dirty = true;
    }
    if cube.turn != 0.0 {
        v3.anim = None;
        v3.camera.yaw_deg += cube.turn;
        v3.camera_dirty = true;
    }

    let nav = v3.nav();
    let camera_now = v3.camera;
    if modifying && !zooming {
        crate::modify3d::handle(ui, &resp, &painter, state, view_id, crate::modify3d::Frame::new(rect, camera_now));
    }
    if let Some((wall, point)) = opening_pick {
        let tool = state.tool.clone();
        crate::tools::place_opening_3d(state, &tool, wall, point);
    }
    if let Some(picked) = demolish_pick {
        let ids = crate::phases_dialog::demolishable(state, view_id, picked);
        crate::phases_dialog::demolish(state, view_id, &ids);
    }
    crate::canvas::temporary_frame(ui, state, rect, view_id);
    if let Some(cmd) = crate::canvas::view_toolbar(ui, state.zoom_default, rect, view_id, None, nav) {
        crate::canvas::zoom(state, view_id, cmd, 1.0);
        ui.ctx().request_repaint();
    }
    let Some(v3) = state.view3d.get_mut(&view_id) else { return };

    // Persist the camera into the view when the interaction ends (not undoable).
    let interacting = resp.dragged()
        || v3.anim.is_some()
        || ui.input(|i| i.smooth_scroll_delta.y != 0.0)
        || cube.orbit.is_some()
        || cube.turn != 0.0;
    if v3.camera_dirty && !interacting {
        v3.camera_dirty = false;
        let camera = v3.camera;
        state.session.set_view_camera(view_id, camera);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_commands_move_the_camera_and_walk_back() {
        let mut v = View3DState::new(CameraState::default());
        let start = v.camera;
        v.apply(ZoomCommand::OutTwo);
        assert!((v.camera.distance - start.distance * 2.0).abs() < 1e-9);
        // A window around the view center halves the distance, keeping the target.
        let view = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        v.remember();
        v.zoom_to_window(egui::Rect::from_center_size(view.center(), egui::vec2(400.0, 300.0)), view);
        assert!((v.camera.distance - start.distance).abs() < 1e-9);
        assert!(v.camera.target.distance(start.target) < 1e-9);
        v.apply(ZoomCommand::Previous);
        v.apply(ZoomCommand::Previous);
        assert_eq!(v.camera, start);
        assert!(!v.nav().back && v.nav().forward);
        v.apply(ZoomCommand::Next);
        assert!((v.camera.distance - start.distance * 2.0).abs() < 1e-9);
        // No paper scale in 3D.
        v.apply(ZoomCommand::SheetSize);
        assert!((v.camera.distance - start.distance * 2.0).abs() < 1e-9);
        assert!(!v.nav().sheet_size);
    }
}
