use crate::commands::*;
use crate::error::{EngineError, ErrorCode, Result};
use crate::session::Session;
use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// What a command does to the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandKind {
    /// Reads the model; changes nothing.
    Query,
    /// Changes the model (one undo step).
    Edit,
    /// Works with files or replaces the document.
    File,
}

/// A command: typed parameters and what running it does. The type's doc
/// comment is its description; its fields' doc comments describe the
/// parameters in the generated schema.
pub trait Command: DeserializeOwned + JsonSchema {
    const NAME: &'static str;
    const KIND: CommandKind;
    fn run(self, session: &mut Session) -> Result<Value>;
}

/// A command as listed by the catalog.
#[derive(Debug, Clone, Serialize)]
pub struct CommandInfo {
    pub name: &'static str,
    pub kind: CommandKind,
    /// Area of the catalog (project, queries, elements…).
    pub group: &'static str,
    pub description: String,
    /// JSON Schema (draft 7, no references) of the parameters object.
    pub input_schema: Value,
}

fn info<C: Command>(group: &'static str) -> CommandInfo {
    let generator = schemars::generate::SchemaSettings::draft07()
        .with(|s| {
            s.inline_subschemas = true;
            s.meta_schema = None;
        })
        .into_generator();
    let schema = generator.into_root_schema_for::<C>();
    let mut input_schema = serde_json::to_value(schema).unwrap_or_else(|_| serde_json::json!({"type": "object"}));
    let description = input_schema
        .get("description")
        .and_then(Value::as_str)
        .map(|d| d.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    if let Some(o) = input_schema.as_object_mut() {
        o.remove("title");
        o.remove("description");
        o.entry("properties").or_insert_with(|| serde_json::json!({}));
    }
    CommandInfo { name: C::NAME, kind: C::KIND, group, description, input_schema }
}

fn run<C: Command>(session: &mut Session, args: Value) -> Result<Value> {
    let args = if args.is_null() { serde_json::json!({}) } else { args };
    let command: C = serde_json::from_value(args).map_err(|e| EngineError::invalid(format!("{}: {e}", C::NAME)))?;
    command.run(session)
}

macro_rules! registry {
    ($($group:literal => [$($c:ty),* $(,)?]),* $(,)?) => {
        /// Every command, in catalog order (grouped by area).
        pub fn catalog() -> Vec<CommandInfo> {
            vec![$($(info::<$c>($group)),*),*]
        }

        pub(crate) fn dispatch(session: &mut Session, name: &str, args: Value) -> Result<Value> {
            $($(
                if name == <$c as Command>::NAME {
                    return run::<$c>(session, args);
                }
            )*)*
            Err(EngineError::new(ErrorCode::UnknownCommand, format!("unknown command: {name}")))
        }
    };
}

/// The catalog entry of one command.
pub fn describe(name: &str) -> Option<CommandInfo> {
    catalog().into_iter().find(|c| c.name == name)
}

registry![
    "Project and files" => [
        project::ProjectNew,
        project::ProjectOpen,
        project::ProjectSave,
        project::ProjectInfoGet,
        project::ProjectInfoSet,
        project::Undo,
        project::Redo,
        project::UnitsGet,
        project::UnitsSet,
        project::LanguageSet,
        io::IfcImport,
        io::SkpImport,
        io::SkpComponentLoad,
        io::IfcExport,
        io::SheetExport,
        io::ViewExport,
        io::ScheduleExportCsv,
        io::ComponentLoad,
        io::ComponentLibraryList,
        io::ComponentSave,
        cad::CadImportFile,
        cad::ImageImport,
        cad::CadLayers,
        cad::CadLayersSet,
    ],
    "Queries" => [
        query::CategoriesList,
        query::LevelsList,
        query::TypesList,
        query::TypeGet,
        query::ElementsList,
        query::ElementGet,
        query::GroupsList,
        query::Measure,
        query::ViewsList,
        query::ViewGet,
        query::MaterialsList,
        query::MaterialGet,
        query::ScheduleTable,
    ],
    "Creating elements" => [
        create::WallCreate,
        create::DoorCreate,
        create::WindowCreate,
        create::FloorCreate,
        create::FloorOpeningAdd,
        create::FloorOpeningRemove,
        create::CeilingCreate,
        create::RoofCreate,
        create::RoofEdgeSet,
        create::RoofOpeningRemove,
        create::RoomCreate,
        create::GridCreate,
        create::ColumnCreate,
        create::BeamCreate,
        structure::ColumnsAtGrids,
        structure::BeamsOnGrids,
        structure::BeamsBetweenColumns,
        structure::SlabJoinSupports,
        structure::StructureNumber,
        framing::BraceCreate,
        framing::BracesBetweenColumns,
        framing::TrussCreate,
        rebar::RebarCreate,
        rebar::RebarTypical,
        analytical::AnalyticalModelGet,
        analytical::AnalyticalExport,
        foundation::FootingCreate,
        foundation::StripFootingCreate,
        foundation::FoundationSlabCreate,
        foundation::FootingsUnderColumns,
        foundation::StripFootingsUnderWalls,
        stair::StairCreate,
        stair::StairCheck,
        stair::StairRailingsAdd,
        ramp::RampCreate,
        ramp::RampCheck,
        roofing::RoofCheck,
        mass::MassCreate,
        sketch::SketchGet,
        sketch::SketchSet,
        mass::MassFloorsSet,
        mass::MassFloors,
        mass::MassTypeCreate,
        mass::MassFaceMove,
        mass::MassWallsCreate,
        mass::MassSlabsCreate,
        mass::MassRoofCreate,
        ramp::RampTypeCreate,
        create::RailingCreate,
        create::ComponentPlace,
    ],
    "Building services" => [
        mep::PipeCreate,
        mep::ConduitCreate,
        mep::SystemsList,
        mep::SystemCreate,
        mep::SystemSet,
        mep::SystemDelete,
        mep::SystemAssign,
        mep::CircuitLoads,
        mep::MepFittings,
        mep::MepConnect,
        mep::MepSizing,
        mep::MepSizeApply,
    ],
    "Modifying elements" => [
        modify::ElementsDelete,
        modify::ElementsMove,
        modify::ElementsCopy,
        modify::ElementsRotate,
        modify::ElementsMirror,
        modify::ElementsPin,
        modify::ElementFlip,
        modify::ElementsTurn,
        modify::ElementsExport,
        modify::ElementsPaste,
        modify::WallJoin,
        modify::GeometryJoin,
        modify::GeometryUnjoin,
        modify::GroupCreate,
        modify::GroupUngroup,
        modify::TrimCorner,
        modify::TrimExtend,
        modify::ElementSplit,
        modify::ElementOffset,
        modify::ElementAlign,
        modify::ElementMoveEnd,
        modify::GridsAlignEnds,
        edit::ElementSet,
        edit::ElementsSetType,
    ],
    "Dimensions and text" => [
        annotate::DimensionCreate,
        annotate::TagMove,
        annotate::DimensionChainCreate,
        annotate::DimensionOrdinateCreate,
        annotate::DimensionSetValue,
        annotate::TextCreate,
        annotate::TextAddLeader,
        annotate::TextRemoveLeader,
    ],
    "Types, components, materials" => [
        types::TypeSet,
        types::TypeDuplicate,
        types::TypeDelete,
        types::ComponentCreate,
        component::ComponentGet,
        component::ComponentParamAdd,
        component::ComponentParamSet,
        component::ComponentParamDelete,
        component::ComponentRefPlaneAdd,
        component::ComponentRefPlaneSet,
        component::ComponentRefPlaneDelete,
        component::ComponentRefLineAdd,
        component::ComponentRefLineDelete,
        component::ComponentDimensionAdd,
        component::ComponentDimensionSet,
        component::ComponentDimensionDelete,
        component::ComponentEqualAdd,
        component::ComponentEqualDelete,
        component_piece::ComponentPieceAdd,
        component_piece::ComponentPieceSet,
        component_piece::ComponentPieceDelete,
        materials::MaterialCreate,
        materials::MaterialDuplicate,
        materials::MaterialRename,
        materials::MaterialSet,
        materials::MaterialDelete,
    ],
    "Levels and views" => [
        levels::LevelCreate,
        levels::LevelSet,
        levels::LevelDelete,
        views::ViewCreate,
        views::ViewSet,
        views::ViewDuplicate,
        views::ViewDelete,
        views::SheetPlaceView,
        views::SheetRemoveViewport,
    ],
    "View graphics" => [
        graphics::ViewGraphicsGet,
        graphics::ViewGraphicsSet,
        graphics::ElementOverrideSet,
        graphics::ElementHideInView,
        graphics::FiltersList,
        graphics::FilterCreate,
        graphics::FilterSet,
        graphics::FilterDelete,
        graphics::FilterElements,
        graphics::ViewFilterAdd,
        graphics::ViewFilterSet,
        graphics::ViewFilterRemove,
        graphics::ViewFilterMove,
        graphics::TemplatesList,
        graphics::TemplateCreate,
        graphics::TemplateSet,
        graphics::TemplateDelete,
        graphics::ViewTemplateApply,
        graphics::ViewTemplateAssign,
        graphics::ViewTemporary,
        graphics::ViewTemporaryApply,
    ],
    "Phases" => [
        phases::PhasesList,
        phases::PhaseCreate,
        phases::PhaseSet,
        phases::PhaseDelete,
        phases::ElementsPhaseSet,
        phases::ElementsDemolish,
        phases::PhaseCreationSet,
        phases::PhaseFiltersList,
        phases::PhaseFilterCreate,
        phases::PhaseFilterSet,
        phases::PhaseFilterDelete,
        phases::PhaseGraphicsGet,
        phases::PhaseGraphicsSet,
    ],
];
