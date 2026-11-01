use crate::{Category, ElementTypeId, LevelId, ViewId};
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

/// Grouping used by the Project Browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ViewCategory {
    FloorPlan,
    CeilingPlan,
    ThreeD,
    Elevation,
    Section,
    Legend,
    Schedule,
    /// Single-line diagrams of distribution boards.
    Diagram,
    Sheet,
}

impl ViewCategory {
    pub const ALL: [ViewCategory; 9] = [
        ViewCategory::FloorPlan,
        ViewCategory::CeilingPlan,
        ViewCategory::ThreeD,
        ViewCategory::Elevation,
        ViewCategory::Section,
        ViewCategory::Legend,
        ViewCategory::Schedule,
        ViewCategory::Diagram,
        ViewCategory::Sheet,
    ];

    /// Translation key for the browser group heading.
    pub fn key(self) -> &'static str {
        match self {
            ViewCategory::FloorPlan => "browser-floor-plans",
            ViewCategory::CeilingPlan => "browser-ceiling-plans",
            ViewCategory::ThreeD => "browser-3d-views",
            ViewCategory::Elevation => "browser-elevations",
            ViewCategory::Section => "browser-sections",
            ViewCategory::Legend => "browser-legends",
            ViewCategory::Schedule => "browser-schedules",
            ViewCategory::Diagram => "browser-diagrams",
            ViewCategory::Sheet => "browser-sheets",
        }
    }
}

/// Elevation direction, named by the side the viewer stands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    North,
    South,
    East,
    West,
}

impl Direction {
    /// Unit vector the viewer looks along (from their side towards the model).
    pub fn look(self) -> DVec2 {
        match self {
            Direction::North => DVec2::NEG_Y,
            Direction::South => DVec2::Y,
            Direction::East => DVec2::NEG_X,
            Direction::West => DVec2::X,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraState {
    pub target: DVec3,
    /// Rotation around Z, degrees; 0 looks along +Y.
    pub yaw_deg: f64,
    /// Elevation angle above the horizon, degrees.
    pub pitch_deg: f64,
    pub distance: f64,
}

impl Default for CameraState {
    fn default() -> Self {
        Self { target: DVec3::new(5.0, 4.0, 1.5), yaw_deg: 35.0, pitch_deg: 30.0, distance: 25.0 }
    }
}

impl CameraState {
    /// Unit vector from the eye towards the target.
    pub fn forward(&self) -> DVec3 {
        let (yaw, pitch) = (self.yaw_deg.to_radians(), self.pitch_deg.to_radians());
        DVec3::new(yaw.sin() * pitch.cos(), yaw.cos() * pitch.cos(), -pitch.sin())
    }

    pub fn eye(&self) -> DVec3 {
        self.target - self.forward() * self.distance
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SheetSize {
    A0,
    A1,
    A2,
    A3,
    A4,
}

impl SheetSize {
    pub const ALL: [SheetSize; 5] = [SheetSize::A0, SheetSize::A1, SheetSize::A2, SheetSize::A3, SheetSize::A4];

    /// Landscape paper size in millimeters.
    pub fn size_mm(self) -> DVec2 {
        match self {
            SheetSize::A0 => DVec2::new(1189.0, 841.0),
            SheetSize::A1 => DVec2::new(841.0, 594.0),
            SheetSize::A2 => DVec2::new(594.0, 420.0),
            SheetSize::A3 => DVec2::new(420.0, 297.0),
            SheetSize::A4 => DVec2::new(297.0, 210.0),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SheetSize::A0 => "A0",
            SheetSize::A1 => "A1",
            SheetSize::A2 => "A2",
            SheetSize::A3 => "A3",
            SheetSize::A4 => "A4",
        }
    }
}

/// A view placed on a sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub view: ViewId,
    /// Center of the viewport on the sheet, millimeters from the bottom-left corner.
    pub center: DVec2,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ViewKind {
    FloorPlan {
        level: LevelId,
        cut_height: f64,
        /// A structural (formwork) plan: only the structure, with section tags.
        #[serde(default)]
        structural: bool,
        /// Tag beams and columns with their mark and section.
        #[serde(default)]
        structure_tags: bool,
        /// A building services plan: the discipline's pipes or conduits and
        /// devices over the architecture, tagged. Other plans leave them out.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mep: Option<MepDiscipline>,
    },
    CeilingPlan {
        level: LevelId,
        cut_height: f64,
    },
    ThreeD {
        camera: CameraState,
        /// Show the analytical model (member lines and nodes) over a faint model.
        #[serde(default)]
        analytical: bool,
        /// Show the reinforcing bars over a faint model.
        #[serde(default)]
        rebar: bool,
    },
    Elevation {
        direction: Direction,
    },
    /// Section line from `start` to `end`; looks to the right of the line.
    Section {
        start: DVec2,
        end: DVec2,
        depth: f64,
    },
    Legend {
        types: Vec<ElementTypeId>,
    },
    Schedule {
        category: Category,
        #[serde(default)]
        def: crate::ScheduleDef,
    },
    Sheet {
        number: String,
        size: SheetSize,
        viewports: Vec<Viewport>,
    },
    /// The single-line diagram and load schedule of a distribution board.
    Diagram {
        panel: crate::ElementId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub id: ViewId,
    pub name: String,
    /// Scale denominator (50 means 1:50).
    pub scale: u32,
    pub kind: ViewKind,
    /// Category visibility and overrides, filters, element overrides.
    #[serde(default, skip_serializing_if = "crate::ViewGraphics::is_empty")]
    pub graphics: crate::ViewGraphics,
    /// The template the view follows: the properties it controls are
    /// copied from it on every edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<crate::TemplateId>,
    /// The phase the view shows the model in (see [`crate::phase`]).
    #[serde(default)]
    pub phase: crate::PhaseId,
    /// Which statuses the view shows, and how (`None`: all, by category).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_filter: Option<crate::PhaseFilterId>,
    /// Tags moved by hand in this view: their offset in plan from where they
    /// sit by default, by the element they tag.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub tag_offsets: std::collections::BTreeMap<crate::ElementId, glam::DVec2>,
    /// How much of the elements the view draws (see [`DetailLevel`]).
    #[serde(default, skip_serializing_if = "DetailLevel::is_medium")]
    pub detail: DetailLevel,
    /// How the view shows the model's surfaces (`None`: the usual style of
    /// its kind, see [`View::style`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visual_style: Option<VisualStyle>,
}

/// How a view shows the model's surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum VisualStyle {
    /// Every edge, seen through the model; no faces.
    Wireframe,
    /// Faces in the paper's color hide what is behind them; only edges show.
    HiddenLine,
    /// Faces in their material's color, lit by a light from the viewer's side.
    #[default]
    Shaded,
    /// Faces in their material's color, the same on every side (no light).
    ConsistentColors,
    /// Faces in their material's appearance color with its surface pattern, lit.
    Textures,
}

impl VisualStyle {
    pub const ALL: [VisualStyle; 5] = [
        VisualStyle::Wireframe,
        VisualStyle::HiddenLine,
        VisualStyle::Shaded,
        VisualStyle::ConsistentColors,
        VisualStyle::Textures,
    ];

    pub fn key(self) -> &'static str {
        match self {
            VisualStyle::Wireframe => "style-wireframe",
            VisualStyle::HiddenLine => "style-hidden-line",
            VisualStyle::Shaded => "style-shaded",
            VisualStyle::ConsistentColors => "style-consistent-colors",
            VisualStyle::Textures => "style-textures",
        }
    }

    /// The name commands use: `wireframe`, `hidden_line`, `shaded`, `consistent_colors` or `textures`.
    pub fn name(self) -> &'static str {
        match self {
            VisualStyle::Wireframe => "wireframe",
            VisualStyle::HiddenLine => "hidden_line",
            VisualStyle::Shaded => "shaded",
            VisualStyle::ConsistentColors => "consistent_colors",
            VisualStyle::Textures => "textures",
        }
    }

    pub fn from_name(name: &str) -> Option<VisualStyle> {
        let name = name.trim().to_ascii_lowercase().replace([' ', '-'], "_");
        VisualStyle::ALL.into_iter().find(|s| s.name() == name)
    }

    /// Faces are filled (every style but the wireframe).
    pub fn fills(self) -> bool {
        self != VisualStyle::Wireframe
    }

    /// Faces carry their material's color (shaded, consistent colors, textures).
    pub fn colored(self) -> bool {
        matches!(self, VisualStyle::Shaded | VisualStyle::ConsistentColors | VisualStyle::Textures)
    }
}

impl View {
    /// The style the view is drawn in: its own, else plans in hidden line
    /// (as drawn before styles existed) and other views shaded.
    pub fn style(&self) -> VisualStyle {
        self.visual_style.unwrap_or(match self.kind {
            ViewKind::FloorPlan { .. } | ViewKind::CeilingPlan { .. } => VisualStyle::HiddenLine,
            _ => VisualStyle::Shaded,
        })
    }
}

/// How much a view draws of elements: at `Coarse`, layered walls, floors
/// and roofs are cut as one region with a plain fill (no layers, no
/// material patterns); at `Medium` and `Fine` they show their layers and
/// patterns. Component pieces show at the levels they are set for
/// ([`crate::component::DetailLevels`]), so a component can draw a simple
/// shape at `Coarse` and its details at `Fine`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum DetailLevel {
    Coarse,
    #[default]
    Medium,
    Fine,
}

impl DetailLevel {
    pub const ALL: [DetailLevel; 3] = [DetailLevel::Coarse, DetailLevel::Medium, DetailLevel::Fine];

    pub fn key(self) -> &'static str {
        match self {
            DetailLevel::Coarse => "detail-coarse",
            DetailLevel::Medium => "detail-medium",
            DetailLevel::Fine => "detail-fine",
        }
    }

    /// The name commands use: `coarse`, `medium` or `fine`.
    pub fn name(self) -> &'static str {
        match self {
            DetailLevel::Coarse => "coarse",
            DetailLevel::Medium => "medium",
            DetailLevel::Fine => "fine",
        }
    }

    pub fn is_medium(&self) -> bool {
        *self == DetailLevel::Medium
    }

    /// Layered elements show their layers and material patterns.
    pub fn shows_layers(self) -> bool {
        self != DetailLevel::Coarse
    }
}

impl ViewKind {
    /// Whether views of this kind show the model in a phase (plans,
    /// elevations, sections, 3D views and schedules).
    pub fn is_phased(&self) -> bool {
        matches!(
            self,
            ViewKind::FloorPlan { .. }
                | ViewKind::CeilingPlan { .. }
                | ViewKind::ThreeD { .. }
                | ViewKind::Elevation { .. }
                | ViewKind::Section { .. }
                | ViewKind::Schedule { .. }
        )
    }

    /// An ordinary floor plan of `level` cut at 1.20 m.
    pub fn floor_plan(level: LevelId) -> Self {
        ViewKind::FloorPlan { level, cut_height: 1.2, structural: false, structure_tags: false, mep: None }
    }
}

/// The building services shown by a services plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MepDiscipline {
    /// Water supply, drainage and their fixtures.
    Plumbing,
    /// Conduits, outlets, switches, lights and boards.
    Electrical,
}

impl MepDiscipline {
    pub fn key(self) -> &'static str {
        match self {
            MepDiscipline::Plumbing => "discipline-plumbing",
            MepDiscipline::Electrical => "discipline-electrical",
        }
    }
}

impl View {
    /// A view with default graphics, no template, in the (default) last phase.
    pub fn new(id: ViewId, name: impl Into<String>, scale: u32, kind: ViewKind) -> Self {
        Self {
            id,
            name: name.into(),
            scale,
            kind,
            graphics: Default::default(),
            template: None,
            phase: Default::default(),
            phase_filter: None,
            tag_offsets: Default::default(),
            detail: DetailLevel::Medium,
            visual_style: None,
        }
    }

    pub fn category(&self) -> ViewCategory {
        match self.kind {
            ViewKind::FloorPlan { .. } => ViewCategory::FloorPlan,
            ViewKind::CeilingPlan { .. } => ViewCategory::CeilingPlan,
            ViewKind::ThreeD { .. } => ViewCategory::ThreeD,
            ViewKind::Elevation { .. } => ViewCategory::Elevation,
            ViewKind::Section { .. } => ViewCategory::Section,
            ViewKind::Legend { .. } => ViewCategory::Legend,
            ViewKind::Schedule { .. } => ViewCategory::Schedule,
            ViewKind::Sheet { .. } => ViewCategory::Sheet,
            ViewKind::Diagram { .. } => ViewCategory::Diagram,
        }
    }

    /// Level for plan views, where new elements are placed.
    pub fn level(&self) -> Option<LevelId> {
        match self.kind {
            ViewKind::FloorPlan { level, .. } | ViewKind::CeilingPlan { level, .. } => Some(level),
            _ => None,
        }
    }

    /// Whether the view can be placed on a sheet.
    pub fn placeable(&self) -> bool {
        !matches!(self.kind, ViewKind::Sheet { .. } | ViewKind::Schedule { .. })
    }
}
