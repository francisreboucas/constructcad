//! Application state shared by all panels and views.

use crate::canvas::View2DState;
use crate::tools::{Interaction, Tool};
use crate::view3d::View3DState;
use cc_core::{Document, ElementId, ElementTypeId, LevelId, Project, ViewId};
use cc_engine::Session;
use cc_views::Drawing2D;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RibbonTab {
    File,
    #[default]
    Architecture,
    Structure,
    /// Building services: plumbing and electrical.
    Mep,
    /// Importing and exporting files, loading components.
    Insert,
    Modify,
    Annotate,
    View,
    Manage,
}

/// An action that discards unsaved changes, waiting for confirmation.
#[derive(Debug, Clone, PartialEq)]
pub enum PendingAction {
    New,
    Open,
    OpenPath(std::path::PathBuf),
    OpenSample(&'static str),
    ImportIfc,
    Close,
    Quit,
}

#[derive(Default)]
pub struct Dialogs {
    pub project_info: bool,
    pub units: bool,
    pub material_browser: crate::material_browser::Browser,
    pub levels: bool,
    pub materials: bool,
    pub type_editor: Option<ElementTypeId>,
    /// Open component editor (above the type dialog when opened from it).
    pub component_editor: Option<crate::component_editor::Editor>,
    /// Type name being edited by "Rename" in the type dialog.
    pub type_rename: Option<String>,
    /// The rename field takes the focus on its next frame.
    pub type_rename_focus: bool,
    /// The layer editor is expanded in the type dialog.
    pub type_layers_open: bool,
    /// Undo depth when the open data dialog started editing (see `dialog::session`).
    pub edit_start: Option<usize>,
    /// Level waiting for delete confirmation.
    pub delete_level: Option<LevelId>,
    /// Schedule whose definition dialog is open.
    pub schedule_def: Option<ViewId>,
    pub confirm: Option<PendingAction>,
    pub error: Option<String>,
    pub about: bool,
    /// Plumbing systems and electrical circuits.
    pub systems: bool,
    /// Sizing of the pipes and circuits.
    pub sizing: bool,
    /// Visibility/Graphics of a view or view template.
    pub graphics: Option<crate::graphics_dialog::GraphicsDialog>,
    /// Graphic overrides of the selected elements in a view.
    pub element_override: Option<crate::graphics_dialog::OverrideDialog>,
    /// The view filter editor.
    pub filters: Option<crate::graphics_dialog::FilterEditor>,
    /// The view template manager.
    pub templates: Option<crate::graphics_dialog::TemplateManager>,
    /// Phases, phase filters and phase graphics.
    pub phases: Option<crate::phases_dialog::PhasesDialog>,
    /// The component library window.
    pub library: Option<crate::library::Library>,
    /// The Preferences window.
    pub preferences: Option<crate::preferences::PreferencesDialog>,
}

pub struct AppState {
    /// The engine session: document, derived geometry, drawings and sheets.
    pub session: Session,
    pub selection: BTreeSet<ElementId>,
    pub tool: Tool,
    pub interaction: Interaction,
    /// Options of the transform tools.
    pub modify: crate::modify::ModifyOptions,
    /// Continuous (chained) placement of aligned and linear dimensions.
    pub dim_continuous: bool,
    /// Options of the truss tool.
    pub truss: crate::tools::TrussOptions,
    /// Options of the pipe, conduit and device tools.
    pub mep: crate::tools::MepOptions,
    pub active_view: Option<ViewId>,
    pub view2d: HashMap<ViewId, View2DState>,
    pub view3d: HashMap<ViewId, View3DState>,
    /// Views the dock should open (or focus) this frame.
    pub open_requests: Vec<ViewId>,
    /// View tabs open (kept by the dock each frame).
    pub open_tabs: usize,
    /// Close every view tab but this one on the next frame.
    pub close_others_than: Option<ViewId>,
    pub dialogs: Dialogs,
    pub ribbon_tab: RibbonTab,
    /// The ribbon shows only its tabs; clicking one shows its panel over the views (`ribbon_peek`).
    pub ribbon_collapsed: bool,
    pub ribbon_peek: bool,
    /// The open command palette.
    pub palette: Option<crate::palette::Palette>,
    /// Browser inline rename in progress.
    pub rename: Option<(ViewId, String)>,
    /// Selected viewport on a sheet: (sheet, viewport index).
    pub sheet_selection: Option<(ViewId, usize)>,
    /// Status bar message.
    pub message: String,
    /// Cursor position in the active 2D view's coordinates.
    pub cursor: Option<glam::DVec2>,
    /// Typed shortcut letters (e.g. "WA").
    pub key_buffer: String,
    /// Typed length for the wall tool.
    pub typed: crate::tools::TypedInput,
    /// Default height for new walls.
    pub wall_height: f64,
    /// Location line for new walls.
    pub wall_location_line: cc_core::LocationLine,
    /// Plan layout for new stairs (shape, turn, landing, winders, radius).
    pub stair_layout: cc_core::StairLayout,
    /// Plan layout for new ramps (shape, turn, slope, landing, radius).
    pub ramp_layout: cc_core::RampLayout,
    /// The wall tool draws arcs (start, end, then a point on the arc).
    pub wall_arc: bool,
    /// Level new walls rise to (`None`: unconnected, `wall_height` tall).
    pub wall_top_level: Option<LevelId>,
    /// Allow the window to close after the user confirmed.
    pub quit_confirmed: bool,
    /// egui textures of closed 3D views, freed by the app when the GPU is available.
    pub textures_to_free: Vec<egui::TextureId>,
    /// Bumped whenever the document is replaced (new/open).
    pub generation: u64,
    /// The home screen is shown instead of the project.
    pub show_home: bool,
    /// A project is open (false after "Close Project" and at startup without
    /// a file; the document is then an unused placeholder).
    pub project_open: bool,
    /// Recently opened or saved projects, most recent first.
    pub recent: Vec<std::path::PathBuf>,
    /// The app preferences (saved with eframe; see `preferences.rs`).
    pub prefs: crate::preferences::Preferences,
    /// Update checks and the notice of a newer release.
    pub updates: crate::update::Updates,
    pub thumbnail_jobs: Vec<crate::thumbnails::Job>,
    /// Samples whose thumbnail was already requested this session.
    pub requested_samples: std::collections::BTreeSet<&'static str>,
    /// Cut or copied elements of this document.
    pub clipboard: crate::clipboard::Clipboard,
    /// Command of the views' zoom button (the last one chosen from its menu).
    pub zoom_default: crate::canvas::ZoomCommand,
    /// Value field open over a view (dimension values).
    pub value_edit: Option<crate::dim_edit::ValueEdit>,
    /// The component modeling environment, when open (it replaces the views).
    pub component_env: Option<crate::component_env::Env>,
    /// The open sketch of an outline (see [`crate::sketch_mode`]).
    pub sketch: Option<crate::sketch_mode::Session>,
    /// Text editor open over a view (text notes).
    pub text_edit: Option<crate::text_tool::TextEditor>,
    /// Options of the text tools.
    pub text_options: crate::text_tool::TextOptions,
    /// Selection seen last frame (see [`AppState::follow_selection`]).
    seen_selection: BTreeSet<ElementId>,
}

impl AppState {
    pub fn new(doc: Document) -> Self {
        let mut session = Session::new(doc);
        session.sync();
        Self {
            session,
            selection: BTreeSet::new(),
            tool: Tool::Select,
            interaction: Interaction::default(),
            modify: Default::default(),
            dim_continuous: false,
            truss: Default::default(),
            mep: Default::default(),
            active_view: None,
            view2d: HashMap::new(),
            view3d: HashMap::new(),
            open_requests: Vec::new(),
            open_tabs: 0,
            close_others_than: None,
            dialogs: Dialogs::default(),
            ribbon_tab: RibbonTab::default(),
            ribbon_collapsed: false,
            ribbon_peek: false,
            palette: None,
            rename: None,
            sheet_selection: None,
            message: String::new(),
            cursor: None,
            key_buffer: String::new(),
            typed: Default::default(),
            wall_height: 3.0,
            wall_location_line: Default::default(),
            stair_layout: Default::default(),
            ramp_layout: Default::default(),
            wall_arc: false,
            wall_top_level: None,
            quit_confirmed: false,
            textures_to_free: Vec::new(),
            generation: 0,
            show_home: false,
            project_open: true,
            recent: Vec::new(),
            prefs: Default::default(),
            updates: Default::default(),
            thumbnail_jobs: Vec::new(),
            requested_samples: Default::default(),
            clipboard: Default::default(),
            zoom_default: Default::default(),
            seen_selection: Default::default(),
            value_edit: None,
            component_env: None,
            sketch: None,
            text_edit: None,
            text_options: Default::default(),
        }
    }

    /// Replace the document (new/open), resetting view state.
    pub fn replace_document(&mut self, doc: Document) {
        let message = std::mem::take(&mut self.message);
        let wall_height = self.wall_height;
        let mut textures: Vec<_> = self.view3d.values().filter_map(|v| v.texture).collect();
        textures.append(&mut self.textures_to_free);
        let generation = self.generation + 1;
        let recent = std::mem::take(&mut self.recent);
        let updates = std::mem::take(&mut self.updates);
        let prefs = std::mem::take(&mut self.prefs);
        let ribbon_collapsed = self.ribbon_collapsed;
        let jobs = std::mem::take(&mut self.thumbnail_jobs);
        let requested = std::mem::take(&mut self.requested_samples);
        *self = Self::new(doc);
        self.recent = recent;
        self.updates = updates;
        self.prefs = prefs;
        self.ribbon_collapsed = ribbon_collapsed;
        self.thumbnail_jobs = jobs;
        self.requested_samples = requested;
        self.textures_to_free = textures;
        self.generation = generation;
        self.message = message;
        self.wall_height = wall_height;
    }

    /// Rebuild derived geometry after model changes and drop stale selection.
    pub fn sync(&mut self) {
        // Drawings, schedules and fields format quantities with the project units.
        cc_views::format::set_units(&self.session.doc.project.units);
        // New elements are built in the active view's phase.
        let project = &self.session.doc.project;
        let view = self.active_view.and_then(|v| project.view(v)).filter(|v| v.kind.is_phased());
        let phase = view.and_then(|v| project.view_phase(v));
        if phase != self.session.doc.creation_phase {
            let phase = phase.map(|p| cc_engine::Ref::Id(p.0));
            let _ = self.session.run(cc_engine::commands::phases::PhaseCreationSet { phase, view: None });
        }
        if self.session.sync() {
            let project = &self.session.doc.project;
            self.selection.retain(|id| project.elements.contains_key(id));
            if let Some((sheet, _)) = self.sheet_selection
                && project.view(sheet).is_none()
            {
                self.sheet_selection = None;
            }
        }
    }

    pub fn project(&self) -> &Project {
        &self.session.doc.project
    }

    /// Cached drawing for a graphical view.
    pub fn drawing(&mut self, view: ViewId) -> Option<Rc<Drawing2D>> {
        self.session.drawing(view)
    }

    /// Drop cached drawings (e.g. after a language change alters their text).
    pub fn invalidate_drawings(&mut self) {
        self.session.invalidate_drawings();
    }

    /// Cached composition of a sheet view (paper millimeters).
    pub fn sheet(&mut self, view: ViewId) -> Option<Rc<cc_views::Sheet>> {
        self.session.sheet(view)
    }

    pub fn open_view(&mut self, view: ViewId) {
        self.open_requests.push(view);
    }

    /// Picking objects with the Select tool opens the Modify tab (drawing
    /// tools, which select what they create, keep their tab).
    pub fn follow_selection(&mut self) {
        if self.selection != self.seen_selection {
            // Picking one member of a group picks the whole group.
            if self.tool == Tool::Select {
                let whole = self.project().with_groups(self.selection.iter().copied());
                if whole != self.selection {
                    self.selection = whole;
                }
            }
            if !self.selection.is_empty() && self.tool == Tool::Select {
                self.ribbon_tab = RibbonTab::Modify;
            }
            self.seen_selection = self.selection.clone();
        }
    }

    /// A segment is being drawn with typed input (wall, move, copy, array).
    pub fn typing_segment(&self) -> bool {
        matches!(self.tool, Tool::Wall { .. } | Tool::Move | Tool::Copy | Tool::Array | Tool::PushPull)
            && self.interaction.anchor.is_some()
    }

    pub fn set_tool(&mut self, tool: Tool) {
        // Sketch tools open a sketch (an outline being edited keeps its own); others close it.
        match &tool {
            Tool::Sketch { category, type_id } => {
                let editing =
                    self.sketch.as_ref().is_some_and(|s| matches!(s.target, crate::sketch_mode::Target::Edit { .. }));
                if !editing {
                    let target = crate::sketch_mode::Target::New { category: *category, type_id: *type_id };
                    match self.sketch.as_mut() {
                        Some(s) if matches!(s.target, crate::sketch_mode::Target::New { category: c, .. } if c == *category) => {
                            s.target = target
                        }
                        _ => self.sketch = Some(crate::sketch_mode::Session::new(target, Vec::new())),
                    }
                }
            }
            _ => self.sketch = None,
        }
        self.tool = tool;
        self.interaction = Interaction::default();
        self.typed.clear();
    }

    /// Level of the active plan view, if the active view is a plan.
    pub fn active_level(&self) -> Option<LevelId> {
        self.active_view.and_then(|v| self.session.doc.project.view(v)).and_then(|v| v.level())
    }

    pub fn delete_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        let ids = self.selection.iter().map(|i| i.0).collect();
        match self.session.run(cc_engine::commands::modify::ElementsDelete { ids }) {
            Ok(out) => {
                let count = |k: &str| out[k].as_array().map_or(0, Vec::len);
                let (deleted, pinned) = (count("deleted"), count("pinned_skipped"));
                self.message = if deleted == 0 {
                    crate::t!("msg-pinned-not-deleted", "count" => pinned)
                } else {
                    crate::t!("msg-deleted", "count" => deleted)
                };
                if deleted > 0 {
                    self.selection.clear();
                }
            }
            Err(e) => self.message = e.message,
        }
    }

    /// Create an element in one transaction and select it (test setup; the
    /// tools create elements through engine commands).
    #[cfg(test)]
    pub fn create_element(
        &mut self,
        name: String,
        level: LevelId,
        type_id: ElementTypeId,
        kind: cc_core::ElementKind,
    ) -> ElementId {
        let category = cc_core::Element {
            id: ElementId(0),
            level,
            type_id,
            mark: String::new(),
            comments: String::new(),
            uid: Default::default(),
            kind: kind.clone(),
            phase_created: Default::default(),
            phase_demolished: None,
        }
        .category();
        let id = self.session.doc.edit(name, |e| {
            let mark = if matches!(category, cc_core::Category::Door | cc_core::Category::Window) {
                e.project().next_mark(category)
            } else {
                String::new()
            };
            let id = ElementId(e.alloc_id());
            e.add_element(cc_core::Element {
                id,
                level,
                type_id,
                mark,
                comments: String::new(),
                kind,
                uid: Default::default(),
                phase_created: Default::default(),
                phase_demolished: None,
            })
        });
        self.selection = BTreeSet::from([id]);
        id
    }

    /// Title shown in tabs and window title for a view.
    pub fn view_title(&self, view: ViewId) -> String {
        let Some(v) = self.session.doc.project.view(view) else { return "?".into() };
        match &v.kind {
            cc_core::ViewKind::Sheet { number, .. } => format!("{number} - {}", v.name),
            cc_core::ViewKind::FloorPlan { .. } => crate::t!("tab-floor-plan", "name" => v.name),
            cc_core::ViewKind::CeilingPlan { .. } => crate::t!("tab-ceiling-plan", "name" => v.name),
            cc_core::ViewKind::Elevation { .. } => crate::t!("tab-elevation", "name" => v.name),
            cc_core::ViewKind::Section { .. } => crate::t!("tab-section", "name" => v.name),
            _ => v.name.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selecting_objects_opens_the_modify_tab() {
        let doc = Document::new(crate::home::sample_project("casa_simples").unwrap());
        let mut state = AppState::new(doc);
        let wall = state.project().walls().next().map(|(e, _)| e.id).unwrap();
        state.follow_selection();
        assert_eq!(state.ribbon_tab, RibbonTab::Architecture);
        state.selection.insert(wall);
        state.follow_selection();
        assert_eq!(state.ribbon_tab, RibbonTab::Modify);
        // Once the user picks another tab, the same selection leaves it alone.
        state.ribbon_tab = RibbonTab::View;
        state.follow_selection();
        assert_eq!(state.ribbon_tab, RibbonTab::View);
        // Drawing tools select what they create without switching tabs.
        state.ribbon_tab = RibbonTab::Architecture;
        state.tool = Tool::Wall { type_id: state.project().default_type(cc_core::Category::Wall).unwrap() };
        state.selection.clear();
        state.follow_selection();
        state.selection.insert(wall);
        state.follow_selection();
        assert_eq!(state.ribbon_tab, RibbonTab::Architecture);
    }
}
