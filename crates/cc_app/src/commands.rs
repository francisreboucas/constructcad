//! Interface commands: every ribbon, quick access and palette entry is a
//! [`UiCommand`], so its label, shortcut, description, the engine command it
//! runs and whether it is available now (and why not) are defined once and
//! read by every place that shows it.

use crate::actions;
use crate::icons::{Badge, Icon};
use crate::ribbon::{category_icon, dimension_icon};
use crate::state::{AppState, PendingAction};
use crate::t;
use crate::tools::{DimensionMode, Tool};
use cc_core::{Category, ViewCategory};

/// The user's two-letter keys by command id (empty: no keys), as
/// Preferences → Shortcuts sets them; `preferences::sync_look` keeps it in
/// step with the preferences.
static KEY_OVERRIDES: std::sync::RwLock<std::collections::BTreeMap<String, String>> =
    std::sync::RwLock::new(std::collections::BTreeMap::new());

/// Use `keys` (command id → two letters, or empty) as the custom shortcuts.
pub fn set_key_overrides(keys: &std::collections::BTreeMap<String, String>) {
    if let Ok(mut current) = KEY_OVERRIDES.write()
        && *current != *keys
    {
        current.clone_from(keys);
    }
}

/// Two ASCII capital letters, the form of the keys typed in a view.
pub fn is_letter_keys(keys: &str) -> bool {
    keys.len() == 2 && keys.chars().all(|c| c.is_ascii_uppercase())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiCommand {
    Home,
    New,
    Open,
    Save,
    SaveAs,
    CloseProject,
    ImportIfc,
    /// Import a DWG or DXF drawing into the active view.
    ImportCad,
    /// Import a picture or an SVG drawing into the active view.
    ImportImage,
    /// Insert a SketchUp model into the open project.
    ImportSkp,
    ExportIfc,
    ExportPdf,
    ExportSvg,
    ExportImage,
    ExportDxf,
    About,
    CheckUpdates,
    Preferences,
    Undo,
    Redo,
    Default3D,
    /// The placement tool of a model category.
    Place(Category),
    NewComponent,
    LoadComponent,
    ComponentLibrary,
    /// Open the component modeling environment on the selected component.
    EditComponent,
    /// Environment tools and actions.
    CompExtrusion,
    CompRevolution,
    CompSweep,
    CompBlend,
    CompRefPlane,
    CompDimension,
    CompEqual,
    CompConnector,
    CompSaveFile,
    CompFinish,
    CompCancel,
    /// Sketch mode: drawing tools, picking the walls, finish and cancel.
    SketchLine,
    SketchRect,
    SketchPolygon,
    SketchPolygonOut,
    SketchCircle,
    SketchEllipse,
    SketchArc3,
    SketchArcCenter,
    SketchArcTangent,
    SketchPickWalls,
    SketchFinish,
    SketchCancel,
    /// Open the sketch of the selected floor, ceiling, roof or mass.
    EditBoundary,
    Select,
    SelectInstances,
    ColumnsAtGrids,
    AlignGridEnds,
    BeamsOnGrids,
    BeamsBetweenColumns,
    StructuralSchedules,
    FootingsUnderColumns,
    StripFootingsUnderWalls,
    FoundationSlab,
    SlabJoinSupports,
    AnalyticalView,
    AnalyticalExport,
    Truss,
    BracesX,
    TypicalRebar,
    RebarView,
    SteelSchedule,
    StructuralPlan,
    NumberStructure,
    StructureTags,
    PlaceFixture,
    PlaceDevice,
    Systems,
    PlumbingPlan,
    ElectricalPlan,
    MepSchedules,
    MepConnect,
    MepSizing,
    PanelDiagram,
    Dimension(DimensionMode),
    Text,
    AddLeader,
    Move,
    CopyElements,
    Rotate,
    MirrorPick,
    MirrorDraw,
    Array,
    Align,
    PushPull,
    /// Walls, floors and a roof from the selected masses.
    MassWalls,
    MassSlabs,
    MassRoof,
    Offset,
    TrimCorner,
    TrimExtend,
    Split,
    ToggleJoin,
    Measure,
    JoinGeometry,
    UnjoinGeometry,
    Group,
    Ungroup,
    Pin,
    Unpin,
    Delete,
    Paste,
    Cut,
    ClipboardCopy,
    MatchType,
    FloorPlan,
    CeilingPlan,
    New3D,
    Section,
    Elevations,
    Legend,
    Schedule,
    Sheet,
    ZoomFit,
    /// Close every view tab but the active one.
    CloseInactiveViews,
    VisibilityGraphics,
    ViewFilters,
    ViewTemplates,
    OverrideGraphics,
    HideInView,
    TempHide,
    TempIsolate,
    TempHideCategory,
    TempIsolateCategory,
    TempReset,
    TempPermanent,
    Phases,
    Demolish,
    ProjectInfo,
    Units,
    Levels,
    Materials,
    MacroRecord,
    MacroRun,
    ToggleRibbon,
    CommandPalette,
}

/// Model categories placed by a tool of their own (dimensions and text have their own commands).
const PLACED: [Category; 20] = [
    Category::Wall,
    Category::Door,
    Category::Window,
    Category::Floor,
    Category::Ceiling,
    Category::Roof,
    Category::Stair,
    Category::Ramp,
    Category::Railing,
    Category::Sweep,
    Category::Room,
    Category::Component,
    Category::Grid,
    Category::Column,
    Category::Beam,
    Category::Foundation,
    Category::Pipe,
    Category::Conduit,
    Category::Mass,
    Category::Shaft,
];

impl UiCommand {
    /// Every command, in ribbon order.
    pub fn all() -> Vec<UiCommand> {
        use UiCommand::*;
        let mut all = vec![
            Home,
            New,
            Open,
            Save,
            SaveAs,
            CloseProject,
            ImportIfc,
            ImportCad,
            ImportImage,
            ImportSkp,
            ExportIfc,
            ExportPdf,
            ExportSvg,
            ExportImage,
            ExportDxf,
            About,
            CheckUpdates,
            Preferences,
            Undo,
            Redo,
            Default3D,
        ];
        all.extend(PLACED.map(Place));
        all.extend([NewComponent, LoadComponent, ComponentLibrary, EditComponent, Select, SelectInstances]);
        all.extend([
            CompExtrusion,
            CompRevolution,
            CompSweep,
            CompBlend,
            CompRefPlane,
            CompDimension,
            CompEqual,
            CompConnector,
            CompSaveFile,
            CompFinish,
            CompCancel,
        ]);
        all.extend([
            SketchLine,
            SketchRect,
            SketchPolygon,
            SketchPolygonOut,
            SketchCircle,
            SketchEllipse,
            SketchArc3,
            SketchArcCenter,
            SketchArcTangent,
            SketchPickWalls,
            SketchFinish,
            SketchCancel,
            EditBoundary,
        ]);
        all.extend([AlignGridEnds, ColumnsAtGrids, BeamsOnGrids, BeamsBetweenColumns, StructuralSchedules]);
        all.extend([FootingsUnderColumns, StripFootingsUnderWalls, FoundationSlab, SlabJoinSupports]);
        all.extend([AnalyticalView, AnalyticalExport, Truss, BracesX, TypicalRebar, RebarView, SteelSchedule]);
        all.extend([StructuralPlan, NumberStructure, StructureTags]);
        all.extend([PlaceFixture, PlaceDevice, MepConnect, Systems, MepSizing, PlumbingPlan, ElectricalPlan]);
        all.extend([MepSchedules, PanelDiagram]);
        all.extend(DimensionMode::ALL.map(Dimension));
        all.extend([
            Text,
            AddLeader,
            Move,
            CopyElements,
            Rotate,
            MirrorPick,
            MirrorDraw,
            Array,
            Align,
            PushPull,
            MassWalls,
            MassSlabs,
            MassRoof,
            Offset,
            TrimCorner,
            TrimExtend,
            Split,
            ToggleJoin,
            Measure,
            JoinGeometry,
            UnjoinGeometry,
            Group,
            Ungroup,
            Pin,
            Unpin,
            Delete,
            Paste,
            Cut,
            ClipboardCopy,
            MatchType,
            FloorPlan,
            CeilingPlan,
            New3D,
            Section,
            Elevations,
            Legend,
            Schedule,
            Sheet,
            ZoomFit,
            CloseInactiveViews,
            VisibilityGraphics,
            ViewFilters,
            ViewTemplates,
            OverrideGraphics,
            HideInView,
            TempHide,
            TempIsolate,
            TempHideCategory,
            TempIsolateCategory,
            TempReset,
            TempPermanent,
            Phases,
            Demolish,
            ProjectInfo,
            Units,
            Levels,
            Materials,
            MacroRecord,
            MacroRun,
            ToggleRibbon,
            CommandPalette,
        ]);
        all
    }

    /// The command a two-letter key sequence runs.
    pub fn from_keys(keys: &str) -> Option<UiCommand> {
        Self::all().into_iter().find(|c| c.shortcut().as_deref() == Some(keys))
    }

    /// The command named `id` (see [`UiCommand::id`]).
    pub fn from_id(id: &str) -> Option<UiCommand> {
        Self::all().into_iter().find(|c| c.id() == id)
    }

    pub fn icon(self) -> Icon {
        use UiCommand::*;
        match self {
            Home => Icon::Home,
            New => Icon::New,
            Open | ImportIfc | ImportSkp => Icon::Open,
            Save => Icon::Save,
            SaveAs => Icon::SaveAs,
            CloseProject => Icon::CloseProject,
            ExportIfc => Icon::Export,
            ExportPdf | ExportSvg | Sheet => Icon::Sheet,
            ExportImage => Icon::Image,
            ExportDxf => Icon::CadImport,
            ImportCad => Icon::CadImport,
            ImportImage => Icon::Image,
            About => Icon::About,
            CheckUpdates => Icon::Update,
            Preferences => Icon::Settings,
            Undo => Icon::Undo,
            Redo => Icon::Redo,
            Default3D | New3D => Icon::View3D,
            Place(category) => category_icon(category),
            NewComponent | LoadComponent | EditComponent => Icon::Component,
            CompExtrusion => Icon::Extrusion,
            CompRevolution => Icon::Revolution,
            CompSweep => Icon::Sweep,
            CompBlend => Icon::Blend,
            CompRefPlane => Icon::RefPlane,
            CompDimension => Icon::Dimension,
            CompEqual => Icon::Equal,
            CompConnector => Icon::Connector,
            CompSaveFile => Icon::Save,
            CompFinish => Icon::Finish,
            CompCancel => Icon::Close,
            SketchLine => Icon::DrawLine,
            SketchRect => Icon::DrawRect,
            SketchPolygon => Icon::DrawPolygon,
            SketchPolygonOut => Icon::DrawPolygonOut,
            SketchCircle => Icon::DrawCircle,
            SketchEllipse => Icon::DrawEllipse,
            SketchArc3 => Icon::DrawArc3,
            SketchArcCenter => Icon::DrawArcCenter,
            SketchArcTangent => Icon::DrawArcTangent,
            SketchPickWalls => Icon::Wall,
            SketchFinish => Icon::Finish,
            SketchCancel => Icon::Close,
            EditBoundary => Icon::EditBoundary,
            ComponentLibrary => Icon::Library,
            Select => Icon::Select,
            SelectInstances => Icon::SelectInstances,
            ColumnsAtGrids => Icon::ColumnsAtGrids,
            AlignGridEnds => Icon::AlignGridEnds,
            BeamsOnGrids => Icon::BeamsOnGrids,
            BeamsBetweenColumns => Icon::BeamsBetweenColumns,
            StructuralSchedules => Icon::Schedule,
            FootingsUnderColumns => Icon::FootingsUnderColumns,
            StripFootingsUnderWalls => Icon::StripFooting,
            FoundationSlab => Icon::FoundationSlab,
            SlabJoinSupports => Icon::SlabSupports,
            AnalyticalView | AnalyticalExport => Icon::Analytical,
            Truss => Icon::Truss,
            BracesX => Icon::Brace,
            TypicalRebar => Icon::Rebar,
            RebarView => Icon::RebarView,
            SteelSchedule => Icon::Schedule,
            StructuralPlan => Icon::StructuralPlan,
            NumberStructure => Icon::NumberMarks,
            StructureTags => Icon::StructureTags,
            PlaceFixture => Icon::PlumbingFixture,
            PlaceDevice => Icon::Outlet,
            Systems => Icon::Systems,
            PlumbingPlan => Icon::PlumbingPlan,
            ElectricalPlan => Icon::ElectricalPlan,
            MepSchedules => Icon::Schedule,
            MepConnect => Icon::Connect,
            MepSizing => Icon::Sizing,
            PanelDiagram => Icon::Systems,
            Dimension(mode) => dimension_icon(mode),
            Text => Icon::Text,
            AddLeader => Icon::Leader,
            Move => Icon::Move,
            CopyElements | ClipboardCopy => Icon::Copy,
            Rotate => Icon::Rotate,
            MirrorPick => Icon::MirrorPick,
            MirrorDraw => Icon::MirrorDraw,
            Array => Icon::Array,
            Align => Icon::Align,
            PushPull => Icon::PushPull,
            MassWalls => Icon::Wall,
            MassSlabs => Icon::Floor,
            MassRoof => Icon::Roof,
            Offset => Icon::Offset,
            TrimCorner => Icon::TrimCorner,
            TrimExtend => Icon::TrimExtend,
            Split => Icon::Split,
            ToggleJoin => Icon::Join,
            Measure => Icon::Measure,
            JoinGeometry => Icon::JoinGeometry,
            Group => Icon::Group,
            Ungroup => Icon::Ungroup,
            UnjoinGeometry => Icon::UnjoinGeometry,
            Pin => Icon::Pin,
            Unpin => Icon::Unpin,
            Delete => Icon::Delete,
            Paste => Icon::Paste,
            Cut => Icon::Cut,
            MatchType => Icon::MatchType,
            FloorPlan => Icon::FloorPlan,
            CeilingPlan => Icon::CeilingPlan,
            Section => Icon::Section,
            Elevations => Icon::Elevation,
            Legend => Icon::Legend,
            Schedule => Icon::Schedule,
            ZoomFit => Icon::ZoomFit,
            CloseInactiveViews => Icon::CloseViews,
            VisibilityGraphics => Icon::Visibility,
            ViewFilters => Icon::Filter,
            ViewTemplates => Icon::ViewTemplate,
            OverrideGraphics => Icon::OverrideGraphics,
            HideInView | TempHide | TempHideCategory | TempPermanent => Icon::Hide,
            TempIsolate | TempIsolateCategory => Icon::Isolate,
            TempReset => Icon::ResetTemporary,
            Phases => Icon::Phases,
            Demolish => Icon::Demolish,
            ProjectInfo => Icon::ProjectInfo,
            Units => Icon::Dimension,
            Levels => Icon::Levels,
            Materials => Icon::Materials,
            MacroRecord => Icon::MacroRecord,
            MacroRun => Icon::MacroRun,
            ToggleRibbon => Icon::ChevronUp,
            CommandPalette => Icon::Search,
        }
    }

    /// The mark of the kind of action: creating, importing, exporting.
    pub fn badge(self) -> Option<Badge> {
        use UiCommand::*;
        match self {
            NewComponent | FloorPlan | CeilingPlan | New3D | Legend | Schedule | StructuralSchedules | Sheet
            | SteelSchedule | StructuralPlan | PlumbingPlan | ElectricalPlan | MepSchedules | PanelDiagram => {
                Some(Badge::New)
            }
            ImportIfc | ImportCad | ImportImage | ImportSkp | LoadComponent => Some(Badge::Import),
            ExportPdf | ExportSvg | ExportImage | ExportDxf | AnalyticalExport => Some(Badge::Export),
            Delete => Some(Badge::Remove),
            _ => None,
        }
    }

    /// Button label.
    pub fn label(self, state: &AppState) -> String {
        use UiCommand::*;
        match self {
            Home => t!("action-home"),
            New => t!("action-new"),
            Open => t!("action-open"),
            Save => t!("action-save"),
            SaveAs => t!("action-save-as"),
            CloseProject => t!("action-close-project"),
            ImportIfc => t!("action-import-ifc"),
            ImportCad => t!("action-import-cad"),
            ImportImage => t!("action-import-image"),
            ImportSkp => t!("action-import-skp"),
            ExportIfc => t!("action-export-ifc"),
            ExportPdf => t!("action-export-pdf"),
            ExportSvg => t!("action-export-svg"),
            ExportImage => t!("action-export-image"),
            ExportDxf => t!("action-export-dxf"),
            About => t!("dlg-about"),
            CheckUpdates => t!("action-check-updates"),
            Preferences => t!("action-preferences"),
            Undo => t!("action-undo"),
            Redo => t!("action-redo"),
            Default3D => t!("action-default-3d"),
            Place(category) => place_label(category),
            NewComponent => t!("action-new-component"),
            EditComponent => t!("action-edit-component"),
            CompExtrusion => t!("comp-env-extrusion"),
            CompRevolution => t!("comp-env-revolution"),
            CompSweep => t!("comp-env-sweep"),
            CompBlend => t!("comp-env-blend"),
            CompRefPlane => t!("comp-env-ref-plane"),
            CompDimension => t!("comp-env-dimension"),
            CompEqual => t!("comp-env-equal"),
            CompConnector => t!("comp-env-connector"),
            CompSaveFile => t!("action-save-component"),
            CompFinish => t!("comp-env-finish"),
            CompCancel => t!("comp-env-cancel"),
            SketchLine => t!("sketch-tool-line"),
            SketchRect => t!("sketch-tool-rect"),
            SketchPolygon => t!("sketch-tool-polygon"),
            SketchPolygonOut => t!("sketch-tool-polygon-out"),
            SketchCircle => t!("sketch-tool-circle"),
            SketchEllipse => t!("sketch-tool-ellipse"),
            SketchArc3 => t!("sketch-tool-arc3"),
            SketchArcCenter => t!("sketch-tool-arc-center"),
            SketchArcTangent => t!("sketch-tool-arc-tangent"),
            SketchPickWalls => t!("sketch-from-walls"),
            SketchFinish => t!("sketch-finish-mode"),
            SketchCancel => t!("sketch-cancel-mode"),
            EditBoundary => t!("edit-boundary"),
            LoadComponent => t!("action-load-component"),
            ComponentLibrary => t!("action-component-library"),
            Select => t!("tool-select"),
            SelectInstances => t!("action-select-instances"),
            ColumnsAtGrids => t!("action-columns-at-grids"),
            AlignGridEnds => t!("action-align-grid-ends"),
            BeamsOnGrids => t!("action-beams-on-grids"),
            BeamsBetweenColumns => t!("action-beams-between-columns"),
            StructuralSchedules => t!("action-structural-schedules"),
            FootingsUnderColumns => t!("action-footings-under-columns"),
            StripFootingsUnderWalls => t!("action-strip-footings-under-walls"),
            FoundationSlab => t!("tool-foundation-slab"),
            SlabJoinSupports => t!("action-slab-join-supports"),
            AnalyticalView => t!("action-analytical-view"),
            AnalyticalExport => t!("action-analytical-export"),
            Truss => t!("tool-truss"),
            BracesX => t!("action-braces-x"),
            TypicalRebar => t!("action-rebar-typical"),
            RebarView => t!("action-rebar-view"),
            SteelSchedule => t!("action-steel-schedule"),
            StructuralPlan => t!("action-structural-plan"),
            NumberStructure => t!("action-number-structure"),
            StructureTags => t!("action-structure-tags"),
            PlaceFixture => t!("tool-fixture"),
            PlaceDevice => t!("tool-device"),
            Systems => t!("dlg-systems"),
            PlumbingPlan => t!("action-plumbing-plan"),
            ElectricalPlan => t!("action-electrical-plan"),
            MepSchedules => t!("action-mep-schedules"),
            MepConnect => t!("action-mep-connect"),
            MepSizing => t!("dlg-mep-sizing"),
            PanelDiagram => t!("action-panel-diagram"),
            Dimension(mode) => crate::i18n::tr(mode.key()),
            Text => t!("tool-text"),
            AddLeader => t!("tool-add-leader"),
            Move => t!("tool-move"),
            CopyElements => t!("tool-copy"),
            Rotate => t!("tool-rotate"),
            MirrorPick => t!("tool-mirror-pick"),
            MirrorDraw => t!("tool-mirror-draw"),
            Array => t!("tool-array"),
            Align => t!("tool-align"),
            PushPull => t!("tool-push-pull"),
            MassWalls => t!("action-mass-walls"),
            MassSlabs => t!("action-mass-slabs"),
            MassRoof => t!("action-mass-roof"),
            Offset => t!("tool-offset"),
            TrimCorner => t!("tool-trim-corner"),
            TrimExtend => t!("tool-trim-extend"),
            Split => t!("tool-split"),
            ToggleJoin => t!("tool-toggle-join"),
            Measure => t!("tool-measure"),
            JoinGeometry => t!("tool-join-geometry"),
            Group => t!("action-group"),
            Ungroup => t!("action-ungroup"),
            UnjoinGeometry => t!("tool-unjoin-geometry"),
            Pin => t!("action-pin"),
            Unpin => t!("action-unpin"),
            Delete => t!("action-delete"),
            Paste => t!("action-paste"),
            Cut => t!("action-cut"),
            ClipboardCopy => t!("action-copy"),
            MatchType => t!("tool-match-type"),
            FloorPlan => t!("view-new-floor-plan"),
            CeilingPlan => t!("view-new-ceiling-plan"),
            New3D => t!("view-new-3d"),
            Section => t!("tool-section"),
            Elevations => t!("view-elevations"),
            Legend => t!("view-new-legend"),
            Schedule => t!("view-new-schedule"),
            Sheet => t!("view-new-sheet"),
            ZoomFit => t!("action-zoom-fit"),
            CloseInactiveViews => t!("action-close-inactive-views"),
            VisibilityGraphics => t!("action-visibility-graphics"),
            ViewFilters => t!("action-view-filters"),
            ViewTemplates => t!("action-view-templates"),
            OverrideGraphics => t!("action-override-graphics"),
            HideInView => t!("action-hide-in-view"),
            TempHide => t!("action-temp-hide"),
            TempIsolate => t!("action-temp-isolate"),
            TempHideCategory => t!("action-temp-hide-category"),
            TempIsolateCategory => t!("action-temp-isolate-category"),
            TempReset => t!("action-temp-reset"),
            TempPermanent => t!("action-temp-permanent"),
            Phases => t!("action-phases"),
            Demolish => t!("tool-demolish"),
            ProjectInfo => t!("dlg-project-info"),
            Units => t!("dlg-units"),
            Levels => t!("dlg-levels"),
            Materials => t!("dlg-materials"),
            MacroRecord if state.session.is_recording() => t!("action-macro-stop"),
            MacroRecord => t!("action-macro-record"),
            MacroRun => t!("action-macro-run"),
            ToggleRibbon if state.ribbon_collapsed => t!("action-ribbon-expand"),
            ToggleRibbon => t!("action-ribbon-collapse"),
            CommandPalette => t!("action-command-palette"),
        }
    }

    /// Tooltip title: the label, plus what undo and redo would change.
    pub fn title(self, state: &AppState) -> String {
        let doc = &state.session.doc;
        let step = match self {
            UiCommand::Undo => doc.undo_name(),
            UiCommand::Redo => doc.redo_name(),
            _ => None,
        };
        match step {
            Some(name) => format!("{}: {name}", self.label(state)),
            None => self.label(state),
        }
    }

    /// What the command does, one or two sentences.
    pub fn description(self) -> String {
        use UiCommand::*;
        match self {
            Home => t!("cmd-desc-home"),
            New => t!("cmd-desc-new"),
            Open => t!("cmd-desc-open"),
            Save => t!("cmd-desc-save"),
            SaveAs => t!("cmd-desc-save-as"),
            CloseProject => t!("cmd-desc-close-project"),
            ImportIfc => t!("cmd-desc-import-ifc"),
            ImportCad => t!("cmd-desc-import-cad"),
            ImportImage => t!("cmd-desc-import-image"),
            ImportSkp => t!("cmd-desc-import-skp"),
            ExportIfc => t!("cmd-desc-export-ifc"),
            ExportPdf => t!("cmd-desc-export-pdf"),
            ExportSvg => t!("cmd-desc-export-svg"),
            ExportImage => t!("cmd-desc-export-image"),
            ExportDxf => t!("cmd-desc-export-dxf"),
            About => t!("cmd-desc-about"),
            CheckUpdates => t!("cmd-desc-check-updates"),
            Preferences => t!("cmd-desc-preferences"),
            Undo => t!("cmd-desc-undo"),
            Redo => t!("cmd-desc-redo"),
            Default3D => t!("cmd-desc-default-3d"),
            Place(category) => place_description(category),
            NewComponent => t!("cmd-desc-new-component"),
            EditComponent => t!("cmd-desc-edit-component"),
            CompExtrusion => t!("cmd-desc-comp-extrusion"),
            CompRevolution => t!("cmd-desc-comp-revolution"),
            CompSweep => t!("cmd-desc-comp-sweep"),
            CompBlend => t!("cmd-desc-comp-blend"),
            CompRefPlane => t!("cmd-desc-comp-ref-plane"),
            CompDimension => t!("cmd-desc-comp-dimension"),
            CompEqual => t!("cmd-desc-comp-equal"),
            CompConnector => t!("cmd-desc-comp-connector"),
            CompSaveFile => t!("cmd-desc-comp-save"),
            CompFinish => t!("cmd-desc-comp-finish"),
            CompCancel => t!("cmd-desc-comp-cancel"),
            SketchLine => t!("cmd-desc-sketch-tool-line"),
            SketchRect => t!("cmd-desc-sketch-tool-rect"),
            SketchPolygon => t!("cmd-desc-sketch-tool-polygon"),
            SketchPolygonOut => t!("cmd-desc-sketch-tool-polygon-out"),
            SketchCircle => t!("cmd-desc-sketch-tool-circle"),
            SketchEllipse => t!("cmd-desc-sketch-tool-ellipse"),
            SketchArc3 => t!("cmd-desc-sketch-tool-arc3"),
            SketchArcCenter => t!("cmd-desc-sketch-tool-arc-center"),
            SketchArcTangent => t!("cmd-desc-sketch-tool-arc-tangent"),
            SketchPickWalls => t!("cmd-desc-sketch-from-walls"),
            SketchFinish => t!("cmd-desc-sketch-finish-mode"),
            SketchCancel => t!("cmd-desc-sketch-cancel-mode"),
            EditBoundary => t!("cmd-desc-edit-boundary"),
            LoadComponent => t!("cmd-desc-load-component"),
            ComponentLibrary => t!("cmd-desc-component-library"),
            Select => t!("cmd-desc-select"),
            SelectInstances => t!("cmd-desc-select-instances"),
            ColumnsAtGrids => t!("cmd-desc-columns-at-grids"),
            AlignGridEnds => t!("cmd-desc-align-grid-ends"),
            BeamsOnGrids => t!("cmd-desc-beams-on-grids"),
            BeamsBetweenColumns => t!("cmd-desc-beams-between-columns"),
            StructuralSchedules => t!("cmd-desc-structural-schedules"),
            FootingsUnderColumns => t!("cmd-desc-footings-under-columns"),
            StripFootingsUnderWalls => t!("cmd-desc-strip-footings-under-walls"),
            FoundationSlab => t!("cmd-desc-foundation-slab"),
            SlabJoinSupports => t!("cmd-desc-slab-join-supports"),
            AnalyticalView => t!("cmd-desc-analytical-view"),
            AnalyticalExport => t!("cmd-desc-analytical-export"),
            Truss => t!("cmd-desc-truss"),
            BracesX => t!("cmd-desc-braces-x"),
            TypicalRebar => t!("cmd-desc-rebar-typical"),
            RebarView => t!("cmd-desc-rebar-view"),
            SteelSchedule => t!("cmd-desc-steel-schedule"),
            StructuralPlan => t!("cmd-desc-structural-plan"),
            NumberStructure => t!("cmd-desc-number-structure"),
            StructureTags => t!("cmd-desc-structure-tags"),
            PlaceFixture => t!("cmd-desc-fixture"),
            PlaceDevice => t!("cmd-desc-device"),
            Systems => t!("cmd-desc-systems"),
            PlumbingPlan => t!("cmd-desc-plumbing-plan"),
            ElectricalPlan => t!("cmd-desc-electrical-plan"),
            MepSchedules => t!("cmd-desc-mep-schedules"),
            MepConnect => t!("cmd-desc-mep-connect"),
            MepSizing => t!("cmd-desc-mep-sizing"),
            PanelDiagram => t!("cmd-desc-panel-diagram"),
            Dimension(DimensionMode::Aligned) => t!("cmd-desc-dim-aligned"),
            Dimension(DimensionMode::Linear) => t!("cmd-desc-dim-linear"),
            Dimension(DimensionMode::Ordinate) => t!("cmd-desc-dim-ordinate"),
            Dimension(DimensionMode::Angular) => t!("cmd-desc-dim-angular"),
            Dimension(DimensionMode::Radial) => t!("cmd-desc-dim-radial"),
            Dimension(DimensionMode::Diameter) => t!("cmd-desc-dim-diameter"),
            Text => t!("cmd-desc-text"),
            AddLeader => t!("hint-add-leader-button"),
            Move => t!("cmd-desc-move"),
            CopyElements => t!("cmd-desc-copy-elements"),
            Rotate => t!("cmd-desc-rotate"),
            MirrorPick => t!("cmd-desc-mirror-pick"),
            MirrorDraw => t!("cmd-desc-mirror-draw"),
            Array => t!("cmd-desc-array"),
            Align => t!("cmd-desc-align"),
            PushPull => t!("cmd-desc-push-pull"),
            MassWalls => t!("cmd-desc-mass-walls"),
            MassSlabs => t!("cmd-desc-mass-slabs"),
            MassRoof => t!("cmd-desc-mass-roof"),
            Offset => t!("cmd-desc-offset"),
            TrimCorner => t!("cmd-desc-trim-corner"),
            TrimExtend => t!("cmd-desc-trim-extend"),
            Split => t!("cmd-desc-split"),
            ToggleJoin => t!("cmd-desc-toggle-join"),
            Measure => t!("cmd-desc-measure"),
            JoinGeometry => t!("cmd-desc-join-geometry"),
            Group => t!("cmd-desc-group"),
            Ungroup => t!("cmd-desc-ungroup"),
            UnjoinGeometry => t!("cmd-desc-unjoin-geometry"),
            Pin => t!("cmd-desc-pin"),
            Unpin => t!("cmd-desc-unpin"),
            Delete => t!("cmd-desc-delete"),
            Paste => t!("cmd-desc-paste"),
            Cut => t!("cmd-desc-cut"),
            ClipboardCopy => t!("cmd-desc-copy"),
            MatchType => t!("cmd-desc-match-type"),
            FloorPlan => t!("cmd-desc-floor-plan"),
            CeilingPlan => t!("cmd-desc-ceiling-plan"),
            New3D => t!("cmd-desc-new-3d"),
            Section => t!("cmd-desc-section"),
            Elevations => t!("cmd-desc-elevations"),
            Legend => t!("cmd-desc-legend"),
            Schedule => t!("cmd-desc-schedule"),
            Sheet => t!("cmd-desc-sheet"),
            ZoomFit => t!("cmd-desc-zoom-fit"),
            CloseInactiveViews => t!("cmd-desc-close-inactive-views"),
            VisibilityGraphics => t!("cmd-desc-visibility-graphics"),
            ViewFilters => t!("cmd-desc-view-filters"),
            ViewTemplates => t!("cmd-desc-view-templates"),
            OverrideGraphics => t!("cmd-desc-override-graphics"),
            HideInView => t!("cmd-desc-hide-in-view"),
            TempHide => t!("cmd-desc-temp-hide"),
            TempIsolate => t!("cmd-desc-temp-isolate"),
            TempHideCategory => t!("cmd-desc-temp-hide-category"),
            TempIsolateCategory => t!("cmd-desc-temp-isolate-category"),
            TempReset => t!("cmd-desc-temp-reset"),
            TempPermanent => t!("cmd-desc-temp-permanent"),
            Phases => t!("cmd-desc-phases"),
            Demolish => t!("cmd-desc-demolish"),
            ProjectInfo => t!("cmd-desc-project-info"),
            Units => t!("cmd-desc-units"),
            Levels => t!("cmd-desc-levels"),
            Materials => t!("cmd-desc-materials"),
            MacroRecord => t!("hint-macro-record"),
            MacroRun => t!("hint-macro-run"),
            ToggleRibbon => t!("cmd-desc-toggle-ribbon"),
            CommandPalette => t!("cmd-desc-command-palette"),
        }
    }

    /// Keyboard shortcut: a Ctrl chord or a two-letter sequence typed in a
    /// view; two-letter keys follow the user's choices (Preferences →
    /// Shortcuts).
    pub fn shortcut(self) -> Option<String> {
        let default = self.default_shortcut();
        if !self.customizable() {
            return default.map(str::to_string);
        }
        let custom = KEY_OVERRIDES.read().ok().and_then(|keys| keys.get(&self.id()).cloned());
        match custom {
            Some(keys) if keys.is_empty() => None,
            Some(keys) => Some(keys),
            None => default.map(str::to_string),
        }
    }

    /// A stable name of the command, under which its custom keys are kept.
    pub fn id(self) -> String {
        format!("{self:?}")
    }

    /// Whether the user may change the keys: two-letter ones and commands
    /// without keys (Ctrl chords, Delete and Esc are fixed).
    pub fn customizable(self) -> bool {
        self.default_shortcut().is_none_or(is_letter_keys)
    }

    /// The keys the command has before any change by the user.
    pub fn default_shortcut(self) -> Option<&'static str> {
        use UiCommand::*;
        Some(match self {
            New => "Ctrl+N",
            Open => "Ctrl+O",
            Save => "Ctrl+S",
            SaveAs => "Ctrl+Shift+S",
            CloseProject => "Ctrl+W",
            Undo => "Ctrl+Z",
            Redo => "Ctrl+Y",
            Place(Category::Wall) => "WA",
            Place(Category::Door) => "DR",
            Place(Category::Window) => "WN",
            Place(Category::Floor) => "SB",
            Place(Category::Ceiling) => "FO",
            Place(Category::Roof) => "TE",
            Place(Category::Stair) => "ST",
            Place(Category::Ramp) => "RP",
            Place(Category::Mass) => "MS",
            Place(Category::Shaft) => "SF",
            Place(Category::Railing) => "RA",
            Place(Category::Sweep) => "SW",
            Place(Category::Room) => "RM",
            Place(Category::Component) => "CM",
            Place(Category::Grid) => "GR",
            Place(Category::Column) => "CL",
            Place(Category::Beam) => "BM",
            Place(Category::Foundation) => "FT",
            Place(Category::Pipe) => "PI",
            Place(Category::Conduit) => "CN",
            Select => "MD",
            SelectInstances => "SA",
            Group => "GP",
            Measure => "ME",
            Ungroup => "UG",
            Dimension(DimensionMode::Aligned) => "DI",
            Text => "TT",
            Move => "MV",
            CopyElements => "CO",
            Rotate => "RO",
            MirrorPick => "MM",
            MirrorDraw => "DM",
            Array => "AR",
            Align => "AL",
            PushPull => "PP",
            Offset => "OF",
            TrimCorner => "TR",
            TrimExtend => "TX",
            Split => "SL",
            Pin => "PN",
            Unpin => "UP",
            Delete => "Delete",
            Paste => "Ctrl+V",
            Cut => "Ctrl+X",
            ClipboardCopy => "Ctrl+C",
            MatchType => "MA",
            Section => "CT",
            ZoomFit => "ZE",
            VisibilityGraphics => "VG",
            HideInView => "EH",
            TempHide => "HH",
            TempIsolate => "HI",
            TempHideCategory => "HC",
            TempIsolateCategory => "IC",
            TempReset => "HR",
            Demolish => "DE",
            ToggleRibbon => "Ctrl+F1",
            Preferences => "Ctrl+,",
            CommandPalette => "Ctrl+Shift+P",
            _ => return None,
        })
    }

    /// The engine command that makes the change, as scripts and the MCP server call it.
    pub fn engine(self) -> Option<&'static str> {
        use UiCommand::*;
        Some(match self {
            New => "project_new",
            Open => "project_open",
            Save | SaveAs => "project_save",
            ImportIfc => "ifc_import",
            ImportCad => "cad_import",
            ImportImage => "image_import",
            ImportSkp => "skp_import",
            ExportIfc => "ifc_export",
            ExportPdf | ExportSvg => "sheet_export",
            ExportImage | ExportDxf => "view_export",
            Undo => "undo",
            SelectInstances => "elements_list",
            ColumnsAtGrids => "columns_at_grids",
            AlignGridEnds => "grids_align_ends",
            BeamsOnGrids => "beams_on_grids",
            BeamsBetweenColumns => "beams_between_columns",
            Redo => "redo",
            Place(Category::Wall) => "wall_create",
            Place(Category::Door) => "door_create",
            Place(Category::Window) => "window_create",
            Place(Category::Floor) => "floor_create",
            Place(Category::Ceiling) => "ceiling_create",
            Place(Category::Roof) => "roof_create",
            Place(Category::Stair) => "stair_create",
            Place(Category::Ramp) => "ramp_create",
            Place(Category::Mass) => "mass_create",
            Place(Category::Shaft) => "shaft_create",
            Place(Category::Railing) => "railing_create",
            Place(Category::Sweep) => "sweep_create",
            Place(Category::Room) => "room_create",
            Place(Category::Grid) => "grid_create",
            Place(Category::Column) => "column_create",
            Place(Category::Beam) => "beam_create",
            Place(Category::Foundation) => "footing_create",
            Place(Category::Pipe) => "pipe_create",
            Place(Category::Conduit) => "conduit_create",
            PlaceFixture | PlaceDevice => "component_place",
            Systems => "system_set",
            MepConnect => "mep_connect",
            MepSizing => "mep_sizing",
            PanelDiagram => "view_create",
            PlumbingPlan | ElectricalPlan | MepSchedules => "view_create",
            FootingsUnderColumns => "footings_under_columns",
            StripFootingsUnderWalls => "strip_footings_under_walls",
            FoundationSlab => "foundation_slab_create",
            SlabJoinSupports => "slab_join_supports",
            AnalyticalView => "view_create",
            AnalyticalExport => "analytical_export",
            Truss => "truss_create",
            BracesX => "braces_between_columns",
            TypicalRebar => "rebar_typical",
            RebarView | SteelSchedule | StructuralPlan => "view_create",
            NumberStructure => "structure_number",
            StructureTags => "view_set",
            Place(_) => "component_place",
            NewComponent => "component_create",
            EditComponent => "component_get",
            CompExtrusion | CompRevolution | CompSweep | CompBlend => "component_piece_add",
            CompRefPlane => "component_ref_plane_add",
            CompDimension => "component_dimension_add",
            CompEqual => "component_equal_add",
            CompConnector => "component_connector_add",
            SketchFinish | EditBoundary => "sketch_set",
            CompSaveFile => "component_save",
            LoadComponent => "component_load",
            ComponentLibrary => "component_library_list",
            Dimension(_) => "dimension_create",
            Text => "text_create",
            AddLeader => "text_add_leader",
            Move => "elements_move",
            CopyElements | Array => "elements_copy",
            Rotate => "elements_rotate",
            MirrorPick | MirrorDraw => "elements_mirror",
            Align => "element_align",
            PushPull => "mass_face_move",
            MassWalls => "mass_walls_create",
            MassSlabs => "mass_slabs_create",
            MassRoof => "mass_roof_create",
            Offset => "element_offset",
            TrimCorner => "trim_corner",
            TrimExtend => "trim_extend",
            Split => "element_split",
            ToggleJoin => "wall_join",
            JoinGeometry => "geometry_join",
            Measure => "measure",
            Group => "group_create",
            Ungroup => "group_ungroup",
            UnjoinGeometry => "geometry_unjoin",
            Pin | Unpin => "elements_pin",
            Delete | Cut => "elements_delete",
            Paste => "elements_paste",
            ClipboardCopy => "elements_export",
            MatchType => "elements_set_type",
            FloorPlan | CeilingPlan | New3D | Section | Legend | Schedule | Sheet => "view_create",
            ProjectInfo => "project_info_set",
            VisibilityGraphics => "view_graphics_set",
            ViewFilters => "filter_set",
            ViewTemplates => "template_set",
            OverrideGraphics => "element_override_set",
            HideInView => "element_hide_in_view",
            TempHide | TempIsolate | TempHideCategory | TempIsolateCategory | TempReset => "view_temporary",
            TempPermanent => "view_temporary_apply",
            Phases => "phase_set",
            Demolish => "elements_demolish",
            Units => "units_set",
            Levels => "level_set",
            Materials => "material_set",
            _ => return None,
        })
    }

    /// Why the command cannot run now, or `None` when it can.
    pub fn unavailable(self, state: &AppState) -> Option<String> {
        use UiCommand::*;
        let project_open = state.project_open && !state.show_home;
        match self {
            Home | New | Open | ImportIfc | About | CheckUpdates | Preferences | ToggleRibbon | CommandPalette => {
                return None;
            }
            _ if !project_open => return Some(t!("why-no-project")),
            _ => {}
        }
        let has_sel = !state.selection.is_empty();
        let project = state.project();
        match self {
            Undo if state.sketch.as_ref().is_some_and(|s| !s.can_undo()) => Some(t!("why-nothing-undo")),
            Redo if state.sketch.as_ref().is_some_and(|s| !s.can_redo()) => Some(t!("why-nothing-redo")),
            Undo | Redo if state.sketch.is_some() => None,
            Undo if !state.session.doc.can_undo() => Some(t!("why-nothing-undo")),
            Redo if !state.session.doc.can_redo() => Some(t!("why-nothing-redo")),
            SketchLine | SketchRect | SketchPolygon | SketchPolygonOut | SketchCircle | SketchEllipse | SketchArc3
            | SketchArcCenter | SketchArcTangent | SketchPickWalls | SketchFinish | SketchCancel
                if state.sketch.is_none() =>
            {
                Some(t!("why-no-sketch"))
            }
            SketchPickWalls if state.active_level().is_none() || crate::component_sketch::piece(state).is_some() => {
                Some(t!("msg-open-plan"))
            }
            // In a sketch the modify tools act on its curves.
            Move | CopyElements | Rotate | MirrorPick | MirrorDraw | Offset | TrimCorner | TrimExtend | Split
                if state.sketch.is_some() =>
            {
                let needs_selection = sketch_draw(self).is_some_and(|d| d.on_selection());
                state
                    .sketch
                    .as_ref()
                    .is_some_and(|s| needs_selection && s.selected.is_empty() && !self.selected(state))
                    .then(|| t!("msg-select-first"))
            }
            Delete if state.sketch.is_some() => {
                state.sketch.as_ref().is_some_and(|s| s.selected.is_empty()).then(|| t!("msg-select-first"))
            }
            EditBoundary if crate::sketch_mode::boundary_target(state).is_none() => Some(t!("why-select-outline")),
            ExportPdf | ExportSvg if actions::active_sheet(state).is_none() => Some(t!("hint-export-sheet")),
            ExportImage if actions::image_view(state).is_none() => Some(t!("hint-export-image")),
            ExportDxf if actions::image_view(state).is_none() => Some(t!("hint-export-dxf")),
            ImportCad | ImportImage if crate::cad_ui::cad_view(state).is_none() => Some(t!("why-cad-view")),
            // A project without a ramp, mass or sweep type gets one when the tool starts.
            Place(category)
                if !matches!(category, Category::Ramp | Category::Mass | Category::Sweep | Category::Shaft)
                    && Tool::for_category(project, category).is_none() =>
            {
                Some(t!("msg-no-type"))
            }
            Dimension(_) if Tool::for_category(project, Category::Dimension).is_none() => Some(t!("msg-no-type")),
            Text if Tool::for_category(project, Category::Text).is_none() => Some(t!("msg-no-type")),
            AddLeader if crate::ribbon::selected_text(state).is_none() && !self.selected(state) => {
                Some(t!("why-select-text"))
            }
            Move | CopyElements | Rotate | MirrorPick | MirrorDraw | Array if !has_sel && !self.selected(state) => {
                Some(t!("msg-select-first"))
            }
            Pin | Unpin | Delete | Cut | ClipboardCopy | SelectInstances if !has_sel => Some(t!("msg-select-first")),
            SelectInstances if state.active_view.is_none() => Some(t!("why-no-view")),
            Group if state.selection.len() < 2 => Some(t!("msg-group-two")),
            ColumnsAtGrids if crate::actions::grids_for(state).len() < 2 => Some(t!("msg-grids-two")),
            AlignGridEnds if crate::actions::reference_grid(state).is_none() => Some(t!("why-select-one-grid")),
            BeamsOnGrids if crate::actions::grids_for(state).is_empty() => Some(t!("why-no-grids")),
            BeamsBetweenColumns if crate::actions::selected_columns(state).len() < 2 => Some(t!("msg-columns-two")),
            FootingsUnderColumns if Tool::for_category(project, Category::Foundation).is_none() => {
                Some(t!("msg-no-type"))
            }
            StripFootingsUnderWalls
                if crate::tools::foundation_type(project, cc_core::FoundationKind::Strip).is_none() =>
            {
                Some(t!("msg-no-type"))
            }
            FoundationSlab if crate::tools::foundation_type(project, cc_core::FoundationKind::Slab).is_none() => {
                Some(t!("msg-no-type"))
            }
            Truss if project.default_type(Category::Beam).is_none() => Some(t!("msg-no-type")),
            BracesX if crate::actions::selected_columns(state).len() != 2 => Some(t!("msg-columns-two-exactly")),
            TypicalRebar if project.default_type(Category::Rebar).is_none() => Some(t!("msg-no-rebar-type")),
            StructuralPlan | PlumbingPlan | ElectricalPlan if project.levels.is_empty() => Some(t!("why-no-levels")),
            PlaceFixture if crate::tools::mep_component_type(project, false).is_none() => Some(t!("msg-no-type")),
            PanelDiagram if crate::systems_dialog::panels(state).is_empty() => Some(t!("why-no-panel")),
            PlaceDevice if crate::tools::mep_component_type(project, true).is_none() => Some(t!("msg-no-type")),
            MepConnect
                if !state
                    .selection
                    .iter()
                    .any(|id| project.element(*id).is_some_and(|e| project.mep_data(e).is_some())) =>
            {
                Some(t!("why-select-fixtures"))
            }
            StructureTags if crate::actions::active_floor_plan(state).is_none() => Some(t!("msg-plan-only")),
            PushPull if !crate::modify3d::active_3d(state) => Some(t!("why-push-pull-3d")),
            MassWalls | MassSlabs | MassRoof if crate::actions::selected_masses(state).is_empty() => {
                Some(t!("why-select-mass"))
            }
            Ungroup if !state.selection.iter().any(|id| project.group_of(*id).is_some()) => Some(t!("why-no-group")),
            Paste if state.clipboard.is_empty() => Some(t!("why-clipboard-empty")),
            FloorPlan | CeilingPlan if project.levels.is_empty() => Some(t!("why-no-levels")),
            Elevations if project.views_of(ViewCategory::Elevation).next().is_none() => Some(t!("why-no-elevations")),
            ZoomFit | Measure if state.active_view.is_none() => Some(t!("why-no-view")),
            CloseInactiveViews if state.open_tabs <= 1 => Some(t!("why-one-view-open")),
            VisibilityGraphics | OverrideGraphics | HideInView | TempHide | TempIsolate | TempHideCategory
            | TempIsolateCategory | TempReset | TempPermanent
                if graphics_view(state).is_none() =>
            {
                Some(t!("why-no-graphics-view"))
            }
            OverrideGraphics | HideInView | TempHide | TempIsolate | TempHideCategory | TempIsolateCategory
                if !has_sel =>
            {
                Some(t!("msg-select-first"))
            }
            TempReset | TempPermanent if graphics_view(state).and_then(|v| state.session.temporary(v)).is_none() => {
                Some(t!("msg-nothing-temporary"))
            }
            Demolish if phase_view(state).is_none() => Some(t!("why-no-phase-view")),
            // These pick ends and sides on plan linework; the other modify tools work in 3D too.
            TrimCorner | TrimExtend | Split | Offset | ToggleJoin if crate::modify3d::active_3d(state) => {
                Some(t!("msg-plan-only"))
            }
            MacroRun if state.session.is_recording() => Some(t!("why-recording")),
            EditComponent if crate::component_env::target_type(state).is_none() => Some(t!("why-select-component")),
            CompExtrusion | CompRevolution | CompSweep | CompBlend | CompRefPlane | CompDimension | CompEqual
            | CompConnector | CompSaveFile | CompFinish | CompCancel
                if state.component_env.is_none() =>
            {
                Some(t!("why-no-component-env"))
            }
            _ => None,
        }
    }

    /// Whether the command's tool or mode is the active one.
    pub fn selected(self, state: &AppState) -> bool {
        use UiCommand::*;
        let tool = &state.tool;
        match self {
            Place(Category::Component) => {
                matches!(tool, Tool::Component { type_id } if mep_kind(state, *type_id).is_none())
            }
            Place(category) => tool.category() == Some(category) && !matches!(tool, Tool::Section | Tool::Truss { .. }),
            PlaceFixture => matches!(tool, Tool::Component { type_id } if mep_kind(state, *type_id) == Some(false)),
            PlaceDevice => matches!(tool, Tool::Component { type_id } if mep_kind(state, *type_id) == Some(true)),
            Truss => matches!(tool, Tool::Truss { .. }),
            StructureTags => crate::actions::active_floor_plan(state).is_some_and(|(_, tags)| tags),
            Select if state.sketch.is_some() => {
                state.sketch.as_ref().is_some_and(|s| s.draw == crate::sketch_mode::Draw::Select)
            }
            Select if state.component_env.is_some() => {
                state.component_env.as_ref().is_some_and(|e| e.tool == crate::component_env::EnvTool::Select)
            }
            Select => *tool == Tool::Select,
            SketchLine | SketchRect | SketchPolygon | SketchPolygonOut | SketchCircle | SketchEllipse | SketchArc3
            | SketchArcCenter | SketchArcTangent => {
                state.sketch.as_ref().is_some_and(|s| sketch_draw(self) == Some(s.draw))
            }
            Move | CopyElements | Rotate | MirrorPick | MirrorDraw | Offset | TrimCorner | TrimExtend | Split
                if state.sketch.is_some() =>
            {
                state.sketch.as_ref().is_some_and(|s| sketch_draw(self) == Some(s.draw))
            }
            CompExtrusion | CompRevolution | CompSweep | CompBlend => {
                let want = match self {
                    CompExtrusion => crate::component_sketch::PieceKind::Extrusion,
                    CompRevolution => crate::component_sketch::PieceKind::Revolution,
                    CompSweep => crate::component_sketch::PieceKind::Sweep,
                    _ => crate::component_sketch::PieceKind::Blend,
                };
                crate::component_sketch::piece(state).is_some_and(|p| p.kind == want)
            }
            CompRefPlane => state
                .component_env
                .as_ref()
                .is_some_and(|e| matches!(e.tool, crate::component_env::EnvTool::RefPlane { .. })),
            CompConnector => {
                state.component_env.as_ref().is_some_and(|e| matches!(e.tool, crate::component_env::EnvTool::Connector))
            }
            CompEqual => state
                .component_env
                .as_ref()
                .is_some_and(|e| matches!(e.tool, crate::component_env::EnvTool::Equal { .. })),
            CompDimension => state
                .component_env
                .as_ref()
                .is_some_and(|e| matches!(e.tool, crate::component_env::EnvTool::Dimension { .. })),
            Dimension(mode) => matches!(tool, Tool::Dimension { mode: m, .. } if *m == mode),
            Text => matches!(tool, Tool::Text { .. }),
            AddLeader => matches!(tool, Tool::AddLeader { .. }),
            Move => *tool == Tool::Move,
            CopyElements => *tool == Tool::Copy,
            Rotate => *tool == Tool::Rotate,
            MirrorPick => *tool == Tool::Mirror { draw: false },
            MirrorDraw => *tool == Tool::Mirror { draw: true },
            Array => *tool == Tool::Array,
            Align => *tool == Tool::Align,
            PushPull => *tool == Tool::PushPull,
            Offset => *tool == Tool::Offset,
            TrimCorner => *tool == Tool::TrimCorner,
            TrimExtend => *tool == Tool::TrimExtend,
            Split => *tool == Tool::Split,
            ToggleJoin => *tool == Tool::ToggleJoin,
            Measure => *tool == Tool::Measure,
            JoinGeometry => *tool == Tool::JoinGeometry { unjoin: false },
            UnjoinGeometry => *tool == Tool::JoinGeometry { unjoin: true },
            Paste => *tool == Tool::Paste,
            MatchType => *tool == Tool::MatchType,
            Demolish => *tool == Tool::Demolish,
            Section => *tool == Tool::Section,
            FoundationSlab => matches!(tool, Tool::Sketch { category: Category::Foundation, .. }),
            MacroRecord => state.session.is_recording(),
            ToggleRibbon => state.ribbon_collapsed,
            _ => false,
        }
    }
}

/// The active view when it is a 2D view with a phase (plans, elevations, sections).
pub fn phase_view(state: &AppState) -> Option<cc_core::ViewId> {
    state.active_view.filter(|v| {
        state.project().view(*v).is_some_and(|v| {
            matches!(
                v.kind,
                cc_core::ViewKind::FloorPlan { .. }
                    | cc_core::ViewKind::CeilingPlan { .. }
                    | cc_core::ViewKind::Elevation { .. }
                    | cc_core::ViewKind::Section { .. }
                    | cc_core::ViewKind::ThreeD { .. }
            )
        })
    })
}

/// The active view when it has visibility/graphics (plans, elevations, sections, 3D).
pub fn graphics_view(state: &AppState) -> Option<cc_core::ViewId> {
    state.active_view.filter(|v| state.project().view(*v).is_some_and(|v| cc_core::ViewTemplate::applies_to(&v.kind)))
}

/// Whether a component type is an electrical device (`Some(true)`), a plumbing fixture or neither.
fn mep_kind(state: &AppState, type_id: cc_core::ElementTypeId) -> Option<bool> {
    match state.project().element_type(type_id).map(|t| &t.data) {
        Some(cc_core::TypeData::Component { def, .. }) => def.mep.map(|m| matches!(m, cc_core::MepData::Device { .. })),
        _ => None,
    }
}

fn place_label(category: Category) -> String {
    match category {
        Category::Wall => t!("tool-wall"),
        Category::Door => t!("tool-door"),
        Category::Window => t!("tool-window"),
        Category::Floor => t!("tool-floor"),
        Category::Ceiling => t!("tool-ceiling"),
        Category::Roof => t!("tool-roof"),
        Category::Stair => t!("tool-stair"),
        Category::Ramp => t!("tool-ramp"),
        Category::Railing => t!("tool-railing"),
        Category::Sweep => t!("tool-sweep"),
        Category::Room => t!("tool-room"),
        Category::Grid => t!("tool-grid"),
        Category::Column => t!("tool-column"),
        Category::Beam => t!("tool-beam"),
        Category::Foundation => t!("tool-footing"),
        Category::Rebar => t!("category-rebar"),
        Category::Pipe => t!("tool-pipe"),
        Category::Conduit => t!("tool-conduit"),
        Category::Dimension => t!("tool-dimension"),
        Category::Text => t!("tool-text"),
        Category::Component => t!("tool-component"),
        Category::CadImport => t!("action-import-cad"),
        Category::Mass => t!("tool-mass"),
        Category::Shaft => t!("tool-shaft"),
    }
}

fn place_description(category: Category) -> String {
    match category {
        Category::Wall => t!("cmd-desc-wall"),
        Category::Door => t!("cmd-desc-door"),
        Category::Window => t!("cmd-desc-window"),
        Category::Floor => t!("cmd-desc-floor"),
        Category::Ceiling => t!("cmd-desc-ceiling"),
        Category::Roof => t!("cmd-desc-roof"),
        Category::Stair => t!("cmd-desc-stair"),
        Category::Ramp => t!("cmd-desc-ramp"),
        Category::Railing => t!("cmd-desc-railing"),
        Category::Sweep => t!("cmd-desc-sweep"),
        Category::Room => t!("cmd-desc-room"),
        Category::Grid => t!("cmd-desc-grid"),
        Category::Column => t!("cmd-desc-column"),
        Category::Beam => t!("cmd-desc-beam"),
        Category::Foundation => t!("cmd-desc-footing"),
        Category::Rebar => t!("cmd-desc-rebar-typical"),
        Category::Pipe => t!("cmd-desc-pipe"),
        Category::Conduit => t!("cmd-desc-conduit"),
        Category::Dimension => t!("cmd-desc-dim-aligned"),
        Category::Text => t!("cmd-desc-text"),
        Category::Component => t!("cmd-desc-component"),
        Category::CadImport => t!("cmd-desc-import-cad"),
        Category::Mass => t!("cmd-desc-mass"),
        Category::Shaft => t!("cmd-desc-shaft"),
    }
}

/// Run `command` if it is available; otherwise say why in the status bar.
pub fn invoke(state: &mut AppState, ctx: &egui::Context, command: UiCommand) {
    match command.unavailable(state) {
        Some(why) => state.message = why,
        None => run(state, ctx, command),
    }
}

/// Run `command` (callers check availability first). Menu commands act on
/// their most likely choice: the active level, the first elevation.
/// The drawing tool a sketch command picks.
fn sketch_draw(command: UiCommand) -> Option<crate::sketch_mode::Draw> {
    use crate::sketch_mode::Draw;
    Some(match command {
        UiCommand::SketchLine => Draw::Line,
        UiCommand::SketchRect => Draw::Rect,
        UiCommand::SketchPolygon => Draw::Polygon,
        UiCommand::SketchPolygonOut => Draw::PolygonOut,
        UiCommand::SketchCircle => Draw::Circle,
        UiCommand::SketchEllipse => Draw::Ellipse,
        UiCommand::SketchArc3 => Draw::Arc3,
        UiCommand::SketchArcCenter => Draw::ArcCenter,
        UiCommand::SketchArcTangent => Draw::ArcTangent,
        UiCommand::Move => Draw::Move,
        UiCommand::CopyElements => Draw::Copy,
        UiCommand::Rotate => Draw::Rotate,
        UiCommand::MirrorPick => Draw::MirrorPick,
        UiCommand::MirrorDraw => Draw::MirrorAxis,
        UiCommand::Offset => Draw::Offset,
        UiCommand::TrimCorner => Draw::TrimCorner,
        UiCommand::TrimExtend => Draw::TrimExtend,
        UiCommand::Split => Draw::Split,
        _ => return None,
    })
}

pub fn run(state: &mut AppState, ctx: &egui::Context, command: UiCommand) {
    use UiCommand::*;
    // A ribbon panel shown over the views closes once a command runs.
    state.ribbon_peek = false;
    match command {
        Home => state.show_home = true,
        New => actions::guarded(state, PendingAction::New),
        Open => actions::guarded(state, PendingAction::Open),
        Save => {
            actions::save(state, false);
        }
        SaveAs => {
            actions::save(state, true);
        }
        CloseProject => actions::guarded(state, PendingAction::Close),
        ImportIfc => actions::guarded(state, PendingAction::ImportIfc),
        ImportCad => crate::cad_ui::import_cad(state),
        ImportImage => crate::cad_ui::import_image(state),
        ImportSkp => actions::import_skp(state),
        ExportIfc => actions::export_ifc(state),
        ExportPdf => actions::export_sheet(state, actions::SheetFormat::Pdf),
        ExportSvg => actions::export_sheet(state, actions::SheetFormat::Svg),
        ExportImage => actions::export_image(state),
        ExportDxf => actions::export_dxf(state),
        About => state.dialogs.about = true,
        CheckUpdates => state.updates.start(ctx, true),
        Preferences => crate::preferences::open(state, crate::preferences::Section::General),
        Undo => match state.sketch.as_mut() {
            Some(s) => s.undo(),
            None => {
                // While a chain is drawn, undo steps it back and the tool goes on.
                if !crate::tools::chain_back(state) {
                    actions::run_command(state, cc_engine::commands::project::Undo {});
                }
            }
        },
        Redo => match state.sketch.as_mut() {
            Some(s) => s.redo(),
            None => {
                if !crate::tools::chain_forward(state) {
                    actions::run_command(state, cc_engine::commands::project::Redo {});
                }
            }
        },
        Default3D => actions::open_default_3d(state),
        Place(category) => actions::activate_category_tool(state, category),
        NewComponent => {
            state.dialogs.component_editor = Some(crate::component_editor::Editor::new_component());
        }
        LoadComponent => actions::load_component(state),
        EditComponent => {
            if let Some(t) = crate::component_env::target_type(state) {
                crate::component_env::open(state, t);
            }
        }
        CompExtrusion => crate::component_sketch::start(state, crate::component_sketch::PieceKind::Extrusion),
        CompRevolution => crate::component_sketch::start(state, crate::component_sketch::PieceKind::Revolution),
        CompSweep => crate::component_sketch::start(state, crate::component_sketch::PieceKind::Sweep),
        CompBlend => crate::component_sketch::start(state, crate::component_sketch::PieceKind::Blend),
        CompRefPlane => {
            crate::component_env::set_tool(state, crate::component_env::EnvTool::RefPlane { first: None });
        }
        CompDimension => {
            crate::component_env::set_tool(state, crate::component_env::EnvTool::Dimension { first: None });
        }
        CompConnector => crate::component_env::set_tool(state, crate::component_env::EnvTool::Connector),
        CompEqual => crate::component_env::set_tool(
            state,
            crate::component_env::EnvTool::Equal { picked: Vec::new(), view: None },
        ),
        CompSaveFile => crate::component_env::save_file(state),
        CompFinish => crate::component_env::finish(state),
        CompCancel => crate::component_env::cancel(state),
        ComponentLibrary => crate::library::open(state),
        Select if state.sketch.is_some() => crate::sketch_mode::set_draw(state, crate::sketch_mode::Draw::Select),
        Select if state.component_env.is_some() => {
            crate::component_env::set_tool(state, crate::component_env::EnvTool::Select);
        }
        Select => state.set_tool(Tool::Select),
        SketchLine | SketchRect | SketchPolygon | SketchPolygonOut | SketchCircle | SketchEllipse | SketchArc3
        | SketchArcCenter | SketchArcTangent => {
            if let Some(draw) = sketch_draw(command) {
                crate::sketch_mode::set_draw(state, draw);
            }
        }
        Move | CopyElements | Rotate | MirrorPick | MirrorDraw | Offset | TrimCorner | TrimExtend | Split
            if state.sketch.is_some() =>
        {
            if let Some(draw) = sketch_draw(command) {
                crate::sketch_mode::set_draw(state, draw);
            }
        }
        SketchPickWalls => crate::sketch_mode::pick_walls(state),
        SketchFinish => crate::sketch_mode::finish(state),
        SketchCancel => crate::sketch_mode::cancel(state),
        EditBoundary => crate::sketch_mode::edit_boundary(state),
        SelectInstances => crate::actions::select_instances(state),
        Group => crate::actions::group_selection(state),
        ColumnsAtGrids => crate::actions::columns_at_grids(state),
        AlignGridEnds => crate::actions::align_grid_ends(state),
        BeamsOnGrids => crate::actions::beams_on_grids(state),
        BeamsBetweenColumns => crate::actions::beams_between_columns(state),
        FootingsUnderColumns => crate::actions::footings_under_columns(state),
        StripFootingsUnderWalls => crate::actions::strip_footings_under_walls(state),
        FoundationSlab => {
            if let Some(type_id) = crate::tools::foundation_type(state.project(), cc_core::FoundationKind::Slab) {
                state.set_tool(Tool::Sketch { category: Category::Foundation, type_id });
                state.message = t!("hint-foundation-slab");
            }
        }
        SlabJoinSupports => crate::actions::slab_join_supports(state),
        AnalyticalView => crate::actions::open_analytical_view(state),
        AnalyticalExport => crate::actions::export_analytical(state),
        Truss => {
            if let Some(type_id) = state.project().default_type(Category::Beam) {
                state.set_tool(Tool::Truss { type_id });
                state.message = t!("hint-truss");
            }
        }
        BracesX => crate::actions::braces_x(state),
        TypicalRebar => crate::actions::typical_rebar(state),
        RebarView => crate::actions::open_rebar_view(state),
        SteelSchedule => {
            actions::new_schedule(state, Category::Rebar);
        }
        StructuralPlan => crate::actions::open_structural_plan(state),
        NumberStructure => crate::actions::number_structure(state),
        StructureTags => crate::actions::toggle_structure_tags(state),
        PlaceFixture | PlaceDevice => {
            let device = command == PlaceDevice;
            if let Some(type_id) = crate::tools::mep_component_type(state.project(), device) {
                state.set_tool(Tool::Component { type_id });
                state.message = if device { t!("hint-device") } else { t!("hint-component") };
            }
        }
        Systems => state.dialogs.systems = true,
        MepConnect => crate::actions::mep_connect(state),
        MepSizing => state.dialogs.sizing = true,
        PanelDiagram => crate::actions::open_panel_diagram(state),
        PlumbingPlan => crate::actions::open_mep_plan(state, cc_core::MepDiscipline::Plumbing),
        ElectricalPlan => crate::actions::open_mep_plan(state, cc_core::MepDiscipline::Electrical),
        MepSchedules => {
            actions::new_schedule(state, Category::Pipe);
            actions::new_schedule(state, Category::Conduit);
        }
        StructuralSchedules => {
            actions::new_schedule(state, Category::Column);
            actions::new_schedule(state, Category::Beam);
        }
        Ungroup => crate::actions::ungroup_selection(state),
        Dimension(mode) => {
            if let Some(Tool::Dimension { type_id, .. }) = Tool::for_category(state.project(), Category::Dimension) {
                state.set_tool(Tool::Dimension { type_id, mode });
                state.message = crate::i18n::tr(mode.hint_key());
            }
        }
        Text => {
            if let Some(tool) = Tool::for_category(state.project(), Category::Text) {
                state.set_tool(tool);
                state.message = t!("hint-text");
            }
        }
        AddLeader => {
            if let Some(id) = crate::ribbon::selected_text(state) {
                state.set_tool(Tool::AddLeader { id });
                state.message = t!("hint-add-leader");
            }
        }
        Move => crate::modify::start(state, Tool::Move),
        CopyElements => crate::modify::start(state, Tool::Copy),
        Rotate => crate::modify::start(state, Tool::Rotate),
        MirrorPick => crate::modify::start(state, Tool::Mirror { draw: false }),
        MirrorDraw => crate::modify::start(state, Tool::Mirror { draw: true }),
        Array => crate::modify::start(state, Tool::Array),
        Align => crate::modify::start_pick(state, Tool::Align),
        MassWalls => crate::actions::from_masses(state, "walls"),
        MassSlabs => crate::actions::from_masses(state, "slabs"),
        MassRoof => crate::actions::from_masses(state, "roof"),
        PushPull => {
            state.set_tool(Tool::PushPull);
            state.message = t!("hint-push-pull");
        }
        Offset => crate::modify::start_pick(state, Tool::Offset),
        TrimCorner => crate::modify::start_pick(state, Tool::TrimCorner),
        TrimExtend => crate::modify::start_pick(state, Tool::TrimExtend),
        Split => crate::modify::start_pick(state, Tool::Split),
        ToggleJoin => crate::modify::start_pick(state, Tool::ToggleJoin),
        Measure => {
            state.set_tool(Tool::Measure);
            state.message = t!("hint-measure");
        }
        JoinGeometry | UnjoinGeometry => {
            state.set_tool(Tool::JoinGeometry { unjoin: command == UnjoinGeometry });
            state.message = t!("hint-join-pick");
        }
        Pin => crate::modify::set_pinned(state, true),
        Unpin => crate::modify::set_pinned(state, false),
        Delete => match state.sketch.as_mut() {
            Some(s) => s.delete_selected(),
            None => state.delete_selection(),
        },
        Paste => crate::clipboard::paste(state),
        Cut => crate::clipboard::cut(ctx, state),
        ClipboardCopy => {
            crate::clipboard::copy(ctx, state);
        }
        MatchType => crate::clipboard::start_match_type(state),
        FloorPlan | CeilingPlan => {
            let level = state.active_level().or_else(|| state.project().levels.first().map(|l| l.id));
            if let Some(level) = level {
                actions::new_plan(state, level, command == CeilingPlan);
            }
        }
        New3D => {
            actions::new_3d(state);
        }
        Section => {
            state.set_tool(Tool::Section);
            state.message = t!("msg-section-hint");
        }
        Elevations => {
            let first = state.project().views_of(ViewCategory::Elevation).next().map(|v| v.id);
            if let Some(id) = first {
                state.open_view(id);
            }
        }
        Legend => {
            actions::new_legend(state);
        }
        Schedule => {
            actions::new_schedule(state, Category::Wall);
        }
        Sheet => {
            actions::new_sheet(state);
        }
        ZoomFit => crate::app::zoom_fit(state),
        CloseInactiveViews => state.close_others_than = state.active_view,
        VisibilityGraphics => {
            if let Some(view) = graphics_view(state) {
                crate::graphics_dialog::open(state, crate::graphics_dialog::Target::View(view));
            }
        }
        ViewFilters => crate::graphics_dialog::open_filters(state),
        ViewTemplates => crate::graphics_dialog::open_templates(state),
        OverrideGraphics => crate::graphics_dialog::open_overrides(state),
        HideInView => actions::hide_in_view(state),
        TempHide => actions::temporary(state, "hide"),
        TempIsolate => actions::temporary(state, "isolate"),
        TempHideCategory => actions::temporary(state, "hide_category"),
        TempIsolateCategory => actions::temporary(state, "isolate_category"),
        TempReset => actions::temporary(state, "reset"),
        TempPermanent => actions::temporary_permanent(state),
        Phases => crate::phases_dialog::open(state),
        Demolish => crate::phases_dialog::start_demolish(state),
        ProjectInfo => state.dialogs.project_info = true,
        Units => state.dialogs.units = true,
        Levels => state.dialogs.levels = true,
        Materials => state.dialogs.materials = true,
        MacroRecord => actions::toggle_macro_recording(state),
        MacroRun => actions::run_macro(state),
        ToggleRibbon => state.ribbon_collapsed = !state.ribbon_collapsed,
        CommandPalette => crate::palette::toggle(state),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn engine_commands_exist() {
        let names: HashSet<String> = cc_engine::catalog().into_iter().map(|c| c.name.to_string()).collect();
        for command in UiCommand::all() {
            if let Some(name) = command.engine() {
                assert!(names.contains(name), "{command:?} runs unknown engine command {name}");
            }
        }
    }

    #[test]
    fn shortcuts_are_unique() {
        let mut seen = HashSet::new();
        for command in UiCommand::all() {
            if let Some(keys) = command.default_shortcut() {
                assert!(seen.insert(keys), "{keys} is used twice");
                if keys.len() == 2 {
                    assert_eq!(UiCommand::from_keys(keys), Some(command));
                }
            }
        }
        let all = UiCommand::all();
        assert_eq!(all.iter().collect::<HashSet<_>>().len(), all.len(), "a command is listed twice");
    }

    #[test]
    fn availability_follows_the_state() {
        let mut state = AppState::new(cc_core::Document::new(crate::home::sample_project("casa_simples").unwrap()));
        state.project_open = true;
        state.show_home = false;
        assert_eq!(UiCommand::Undo.unavailable(&state), Some(t!("why-nothing-undo")));
        assert_eq!(UiCommand::Move.unavailable(&state), Some(t!("msg-select-first")));
        assert_eq!(UiCommand::Place(Category::Wall).unavailable(&state), None);
        let wall = state.project().walls().next().map(|(e, _)| e.id).unwrap();
        state.selection = [wall].into_iter().collect();
        assert_eq!(UiCommand::Move.unavailable(&state), None);
        state.show_home = true;
        assert_eq!(UiCommand::Save.unavailable(&state), Some(t!("why-no-project")));
        assert_eq!(UiCommand::Open.unavailable(&state), None);
    }

    /// In a 3D view the transform tools, align and join stay available; the
    /// tools that pick ends and sides on plan linework say they need a plan.
    #[test]
    fn modify_tools_in_3d() {
        let mut state = AppState::new(cc_core::Document::new(crate::home::sample_project("casa_simples").unwrap()));
        state.project_open = true;
        state.show_home = false;
        let wall = state.project().walls().next().map(|(e, _)| e.id).unwrap();
        state.selection = [wall].into_iter().collect();
        assert_eq!(UiCommand::TrimExtend.unavailable(&state), None);
        let view = state.project().views_of(ViewCategory::ThreeD).next().map(|v| v.id);
        assert!(view.is_some(), "the sample has a 3D view");
        state.active_view = view;
        assert!(crate::modify3d::active_3d(&state));
        for command in [UiCommand::Move, UiCommand::CopyElements, UiCommand::Rotate, UiCommand::MirrorPick] {
            assert_eq!(command.unavailable(&state), None, "{command:?}");
        }
        assert_eq!(UiCommand::Align.unavailable(&state), None);
        assert_eq!(UiCommand::JoinGeometry.unavailable(&state), None);
        for command in [UiCommand::TrimCorner, UiCommand::TrimExtend, UiCommand::Split, UiCommand::Offset] {
            assert_eq!(command.unavailable(&state), Some(t!("msg-plan-only")), "{command:?}");
        }
        // Align explains how it picks in 3D.
        let ctx = egui::Context::default();
        invoke(&mut state, &ctx, UiCommand::Align);
        assert_eq!(state.tool, Tool::Align);
        assert_eq!(state.message, t!("hint-align-3d"));
        assert!(crate::modify3d::works_in_3d(&state.tool));
    }

    #[test]
    fn select_all_instances_in_the_view() {
        let mut state = AppState::new(cc_core::Document::new(crate::home::sample_project("casa_simples").unwrap()));
        state.project_open = true;
        state.show_home = false;
        let level = state.project().levels[0].id;
        let plan = state.project().views_of(ViewCategory::FloorPlan).find(|v| v.level() == Some(level)).unwrap().id;
        state.active_view = Some(plan);
        assert_eq!(UiCommand::SelectInstances.unavailable(&state), Some(t!("msg-select-first")));
        let (wall, ty) =
            state.project().walls().find(|(e, _)| e.level == level).map(|(e, _)| (e.id, e.type_id)).unwrap();
        state.selection = [wall].into_iter().collect();
        let ctx = egui::Context::default();
        invoke(&mut state, &ctx, UiCommand::SelectInstances);
        let expected: std::collections::BTreeSet<_> =
            state.project().elements.values().filter(|e| e.type_id == ty && e.level == level).map(|e| e.id).collect();
        assert!(expected.len() > 1);
        assert_eq!(state.selection, expected);
    }

    #[test]
    fn view_graphics_commands_follow_the_active_view() {
        let mut state = AppState::new(cc_core::Document::new(crate::home::sample_project("casa_simples").unwrap()));
        state.project_open = true;
        state.show_home = false;
        let ctx = egui::Context::default();
        assert_eq!(UiCommand::VisibilityGraphics.unavailable(&state), Some(t!("why-no-graphics-view")));
        let plan = state.project().views_of(ViewCategory::FloorPlan).next().unwrap().id;
        state.active_view = Some(plan);
        assert_eq!(UiCommand::TempHide.unavailable(&state), Some(t!("msg-select-first")));
        assert_eq!(UiCommand::TempReset.unavailable(&state), Some(t!("msg-nothing-temporary")));
        let wall = state.project().walls().next().map(|(e, _)| e.id).unwrap();
        state.selection = [wall].into_iter().collect();
        invoke(&mut state, &ctx, UiCommand::TempHide);
        assert!(state.session.temporary(plan).is_some_and(|t| t.elements.contains(&wall)));
        assert!(state.selection.is_empty());
        assert!(!state.session.doc.can_undo(), "temporary hiding is not an edit");
        invoke(&mut state, &ctx, UiCommand::TempPermanent);
        assert!(state.session.temporary(plan).is_none());
        assert!(state.project().view(plan).unwrap().graphics.hidden.contains(&wall));
        // Visibility/Graphics opens on the view and Cancel undoes its edits.
        invoke(&mut state, &ctx, UiCommand::VisibilityGraphics);
        assert!(
            state.dialogs.graphics.as_ref().is_some_and(|d| d.target == crate::graphics_dialog::Target::View(plan))
        );
    }

    #[test]
    fn group_selects_together_and_ungroup_releases() {
        let mut state = AppState::new(cc_core::Document::new(crate::home::sample_project("casa_simples").unwrap()));
        state.project_open = true;
        state.show_home = false;
        let walls: Vec<_> = state.project().walls().map(|(e, _)| e.id).take(2).collect();
        let ctx = egui::Context::default();
        state.selection = [walls[0]].into_iter().collect();
        assert_eq!(UiCommand::Group.unavailable(&state), Some(t!("msg-group-two")));
        state.selection = walls.iter().copied().collect();
        invoke(&mut state, &ctx, UiCommand::Group);
        // Picking one member picks the group.
        state.selection = [walls[1]].into_iter().collect();
        state.follow_selection();
        assert_eq!(state.selection, walls.iter().copied().collect());
        invoke(&mut state, &ctx, UiCommand::Ungroup);
        assert!(state.project().groups.is_empty());
        state.selection = [walls[1]].into_iter().collect();
        state.follow_selection();
        assert_eq!(state.selection.len(), 1);
    }
}
