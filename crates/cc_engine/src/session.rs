use crate::error::{EngineError, ErrorCode, Result};
use crate::t;
use cc_core::{Document, Project, ViewId, ViewKind};
use cc_geom::ModelGeometry;
use cc_views::Drawing2D;
use serde_json::Value;
use std::collections::HashMap;
use std::rc::Rc;

/// An open project with its derived geometry and drawings, rebuilt lazily
/// when the document changes.
pub struct Session {
    pub doc: Document,
    /// Merge key for the next edit (see [`Session::run_merged`]).
    merge_key: Option<u64>,
    /// Commands recorded as a macro, while recording.
    recording: Option<Vec<Recorded>>,
    geom: ModelGeometry,
    geom_revision: Option<u64>,
    /// Geometry of the phases (by index) whose views leave elements out,
    /// with the revision it was built at.
    phase_geoms: HashMap<usize, (u64, ModelGeometry)>,
    drawings: HashMap<ViewId, (u64, Rc<Drawing2D>)>,
    sheets: HashMap<ViewId, (u64, Rc<cc_views::Sheet>)>,
    /// Elements temporarily hidden or isolated per view: display state, not
    /// part of the model (not saved, not undone).
    temporary: HashMap<ViewId, cc_core::TemporaryVisibility>,
    /// Counts changes of `temporary` (3D views rebuild their meshes on it).
    temporary_revision: u64,
}

/// A command recorded in a macro.
#[derive(Debug, Clone)]
struct Recorded {
    step: crate::script::Step,
    merge_key: Option<u64>,
    /// Undo depth before the command ran.
    depth: usize,
}

/// A new project from the template, named in the active language.
pub fn new_project() -> Project {
    Project::from_template_localized(&crate::i18n::try_tr)
}

impl Default for Session {
    fn default() -> Self {
        Self::new(Document::new(new_project()))
    }
}

impl Session {
    pub fn new(doc: Document) -> Self {
        Self {
            doc,
            merge_key: None,
            recording: None,
            geom: ModelGeometry::default(),
            geom_revision: None,
            phase_geoms: HashMap::new(),
            drawings: HashMap::new(),
            sheets: HashMap::new(),
            temporary: HashMap::new(),
            temporary_revision: 0,
        }
    }

    /// Open a `.ccad` file; returns the session and the number of repairs made.
    pub fn open(path: &std::path::Path) -> Result<(Self, usize)> {
        let (project, repairs) = guard(|| cc_core::load_with_repairs(path))??;
        let mut doc = Document::new(project);
        doc.path = Some(path.to_path_buf());
        Ok((Self::new(doc), repairs))
    }

    /// Replace the document (new, open, import), dropping derived data.
    pub fn replace(&mut self, doc: Document) {
        *self = Self::new(doc);
    }

    pub fn project(&self) -> &Project {
        &self.doc.project
    }

    /// Format quantities with the project's units (drawings, schedules).
    fn sync_units(&self) {
        cc_views::format::set_units(&self.doc.project.units);
    }

    /// Rebuild derived geometry if the document changed; returns whether it did.
    pub fn sync(&mut self) -> bool {
        let rev = self.doc.revision();
        if self.geom_revision == Some(rev) {
            return false;
        }
        self.geom = ModelGeometry::build(&self.doc.project);
        self.geom_revision = Some(rev);
        self.phase_geoms.retain(|_, (r, _)| *r == rev);
        true
    }

    /// The phase (an index) whose own geometry `view` shows, when its phase
    /// leaves elements out (see [`ModelGeometry::build_phase`]).
    fn phase_of_view(&self, view: ViewId) -> Option<usize> {
        let p = &self.doc.project;
        let v = p.view(view)?;
        if !v.kind.is_phased() {
            return None;
        }
        let index = p.view_phase(v).and_then(|ph| p.phase_index(ph))?;
        p.phase_differs(index).then_some(index)
    }

    /// Build (if needed) the geometry `view` shows; returns its phase key.
    fn sync_view(&mut self, view: ViewId) -> Option<usize> {
        self.sync();
        let index = self.phase_of_view(view)?;
        let rev = self.doc.revision();
        if self.phase_geoms.get(&index).is_none_or(|(r, _)| *r != rev) {
            let g = ModelGeometry::build_phase(&self.doc.project, index, &self.geom);
            self.phase_geoms.insert(index, (rev, g));
        }
        Some(index)
    }

    fn geom_of(&self, key: Option<usize>) -> &ModelGeometry {
        key.and_then(|k| self.phase_geoms.get(&k)).map_or(&self.geom, |(_, g)| g)
    }

    /// Geometry of the model as `view` shows it, in its phase: only the
    /// elements of that phase join, open walls and bound rooms. The whole
    /// model's geometry when the phase leaves nothing out.
    pub fn view_geometry(&mut self, view: ViewId) -> &ModelGeometry {
        let key = self.sync_view(view);
        self.geom_of(key)
    }

    /// [`Session::view_geometry`] as of the last sync (the GUI calls
    /// [`Session::view_geometry`] once a frame per open view first); falls
    /// back to the whole model's geometry when the phase's is not built.
    pub fn geom_for(&self, view: ViewId) -> &ModelGeometry {
        let rev = self.doc.revision();
        match self.phase_of_view(view).and_then(|k| self.phase_geoms.get(&k)) {
            Some((r, g)) if *r == rev => g,
            _ => &self.geom,
        }
    }

    /// Derived geometry as of the last [`Session::sync`] (callers that keep
    /// it current, like the GUI once a frame, read it without `&mut`).
    pub fn geom(&self) -> &ModelGeometry {
        &self.geom
    }

    /// Derived geometry of the current revision.
    pub fn geometry(&mut self) -> &ModelGeometry {
        self.sync();
        &self.geom
    }

    /// The project with its current geometry.
    pub fn project_and_geometry(&mut self) -> (&Project, &ModelGeometry) {
        self.sync();
        (&self.doc.project, &self.geom)
    }

    /// Drawing of a graphical view, with what is temporarily hidden or
    /// isolated in it left out (cached per revision).
    pub fn drawing(&mut self, view: ViewId) -> Option<Rc<Drawing2D>> {
        let rev = self.doc.revision();
        if let Some((r, d)) = self.drawings.get(&view)
            && *r == rev
        {
            return Some(d.clone());
        }
        self.sync_units();
        let key = self.sync_view(view);
        let v = self.doc.project.view(view)?;
        let temporary = self.temporary.get(&view);
        let geom = self.geom_of(key);
        let d = Rc::new(cc_views::generate_temporary(&self.doc.project, geom, v, &crate::i18n::tr, temporary)?);
        self.drawings.insert(view, (rev, d.clone()));
        Some(d)
    }

    /// Drawing of a view as printed: without temporary hiding or isolation,
    /// and without its underlay unless the view prints it.
    pub fn printed_drawing(&mut self, view: ViewId) -> Option<Rc<Drawing2D>> {
        let d = if self.temporary.contains_key(&view) {
            self.sync_units();
            let key = self.sync_view(view);
            let v = self.doc.project.view(view)?;
            Rc::new(cc_views::generate_labeled(&self.doc.project, self.geom_of(key), v, &crate::i18n::tr)?)
        } else {
            self.drawing(view)?
        };
        let printed = self.doc.project.view(view).and_then(|v| v.kind.underlay()).is_some_and(|u| u.printed);
        Some(if d.underlay > 0 && !printed { Rc::new(d.without_underlay()) } else { d })
    }

    /// What is temporarily hidden or isolated in `view`.
    pub fn temporary(&self, view: ViewId) -> Option<&cc_core::TemporaryVisibility> {
        self.temporary.get(&view)
    }

    /// Set (or, with `None` or an empty set, reset) what is temporarily
    /// hidden or isolated in `view`. Not an edit: nothing to undo or save.
    pub fn set_temporary(&mut self, view: ViewId, temporary: Option<cc_core::TemporaryVisibility>) {
        match temporary.filter(|t| !t.is_empty()) {
            Some(t) => self.temporary.insert(view, t),
            None => self.temporary.remove(&view),
        };
        self.drawings.remove(&view);
        self.temporary_revision += 1;
    }

    /// Keep the camera of 3D view `view` (not undoable). The model's geometry
    /// stays: only the view's drawing and the sheets showing it are redrawn.
    pub fn set_view_camera(&mut self, view: ViewId, camera: cc_core::CameraState) {
        self.doc.set_view_state(|p| {
            if let Some(ViewKind::ThreeD { camera: c, .. }) = p.views.get_mut(&view).map(|v| &mut v.kind) {
                *c = camera;
            }
        });
        self.drawings.remove(&view);
        let project = &self.doc.project;
        self.sheets.retain(|sheet, _| {
            !matches!(project.view(*sheet).map(|v| &v.kind),
                Some(ViewKind::Sheet { viewports, .. }) if viewports.iter().any(|vp| vp.view == view))
        });
    }

    /// Changes whenever any view's temporary visibility changes.
    pub fn temporary_revision(&self) -> u64 {
        self.temporary_revision
    }

    /// Drop cached drawings (after a language change alters their text).
    pub fn invalidate_drawings(&mut self) {
        self.drawings.clear();
        self.sheets.clear();
    }

    /// Composition of a sheet in paper millimeters (cached per revision).
    pub fn sheet(&mut self, view: ViewId) -> Option<Rc<cc_views::Sheet>> {
        let rev = self.doc.revision();
        if let Some((r, s)) = self.sheets.get(&view)
            && *r == rev
        {
            return Some(s.clone());
        }
        let sheet_view = self.doc.project.view(view)?.clone();
        let labels = cc_views::TitleLabels {
            project: t!("title-project"),
            sheet: t!("title-sheet"),
            client: t!("title-client"),
            number: t!("title-number"),
            project_number: t!("title-project-number"),
            author: t!("title-author"),
        };
        let ViewKind::Sheet { viewports, .. } = &sheet_view.kind else { return None };
        let drawings: HashMap<ViewId, Rc<Drawing2D>> =
            viewports.iter().filter_map(|vp| Some((vp.view, self.printed_drawing(vp.view)?))).collect();
        let s =
            Rc::new(cc_views::compose_sheet(&self.doc.project, &sheet_view, &labels, |v| drawings.get(&v).cloned())?);
        self.sheets.insert(view, (rev, s.clone()));
        Some(s)
    }

    /// A schedule table of a schedule view, with the project's units: the
    /// elements its phase and phase filter show.
    pub fn schedule(&mut self, view: ViewId) -> Option<cc_views::Table> {
        let (category, def) = match &self.doc.project.view(view)?.kind {
            ViewKind::Schedule { category, def } => (*category, def.clone()),
            _ => return None,
        };
        self.sync_units();
        self.geometry();
        let p = &self.doc.project;
        let phase = cc_views::graphics::PhaseView::of(p, p.view(view)?);
        Some(cc_views::schedule_where(p, &self.geom, category, &def, &mut |e| phase.is_none_or(|ph| ph.shows(p, e))))
    }

    /// Run a typed command (the way the GUI calls the engine): it goes
    /// through [`Session::execute`] like a command from the CLI or MCP.
    pub fn run<C: crate::Command + serde::Serialize>(&mut self, command: C) -> Result<Value> {
        let args = serde_json::to_value(&command).map_err(|e| EngineError::invalid(e.to_string()))?;
        self.execute(C::NAME, args)
    }

    /// Run a typed command whose edit merges with the previous one of the
    /// same `key` into one undo step (a value being dragged); a different
    /// key, an unmerged edit or [`Document::break_merge`] starts a new step.
    pub fn run_merged<C: crate::Command + serde::Serialize>(&mut self, key: u64, command: C) -> Result<Value> {
        self.merge_key = Some(key);
        let result = self.run(command);
        self.merge_key = None;
        result
    }

    /// The merge key for the edit being run.
    pub(crate) fn take_merge_key(&mut self) -> Option<u64> {
        self.merge_key.take()
    }

    /// Start recording the commands that change the model (a macro).
    pub fn start_recording(&mut self) {
        self.recording = Some(Vec::new());
    }

    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    /// Stop recording; returns the recorded steps (a dragged value counts once,
    /// with its final value).
    pub fn stop_recording(&mut self) -> Vec<crate::script::Step> {
        self.recording.take().unwrap_or_default().into_iter().map(|r| r.step).collect()
    }

    /// Undo back to undo depth `depth` (cancelling a dialog), dropping what
    /// was recorded since.
    pub fn undo_to(&mut self, depth: usize) {
        while self.doc.undo_depth() > depth && self.doc.undo() {}
        if let Some(rec) = &mut self.recording {
            rec.retain(|r| r.depth < depth);
        }
    }

    fn record(&mut self, name: &str, args: &Value, merge_key: Option<u64>, depth: usize) {
        let kind = crate::describe(name).map(|c| c.kind);
        let Some(rec) = &mut self.recording else { return };
        // Edits only: replaying a macro should not open, save or export files.
        if kind != Some(crate::CommandKind::Edit) {
            return;
        }
        let step = crate::script::Step { command: name.to_string(), args: args.clone() };
        match rec.last_mut() {
            // A value being dragged: keep its last state only.
            Some(last) if merge_key.is_some() && last.merge_key == merge_key && last.step.command == name => {
                last.step = step;
            }
            _ => rec.push(Recorded { step, merge_key, depth }),
        }
    }

    /// Run the command `name` with JSON `args`. A panic inside the command
    /// becomes an internal error and the document is kept.
    pub fn execute(&mut self, name: &str, args: Value) -> Result<Value> {
        let depth = self.doc.undo_depth();
        let merge_key = self.merge_key;
        let recorded_args = self.recording.is_some().then(|| args.clone());
        let result = guard(|| crate::registry::dispatch(self, name, args)).and_then(|r| r);
        if let (Ok(_), Some(args)) = (&result, recorded_args) {
            self.record(name, &args, merge_key, depth);
        }
        if let Err(EngineError { code: ErrorCode::Internal, .. }) = &result {
            // Undo whatever the failed command left behind.
            while self.doc.undo_depth() > depth && self.doc.undo() {}
        }
        result
    }
}

/// Last-resort guard: a panic becomes an internal error.
pub(crate) fn guard<T>(f: impl FnOnce() -> T) -> Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|payload| {
        let detail = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        EngineError::new(ErrorCode::Internal, format!("internal error: {detail}"))
    })
}
