use crate::{ElementTypeId, Uid};
use serde::{Deserialize, Serialize};

/// BIM category of an element or element type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Category {
    Wall,
    Door,
    Window,
    Floor,
    Ceiling,
    Roof,
    Room,
    Grid,
    Dimension,
    Text,
    Column,
    Beam,
    Stair,
    Railing,
    Component,
    Foundation,
    Rebar,
    /// Plumbing pipes (water supply, drainage, vent, rainwater).
    Pipe,
    /// Electrical conduits.
    Conduit,
    Ramp,
    /// Imported DWG/DXF drawings (underlays).
    CadImport,
    /// Conceptual masses (massing studies).
    Mass,
    /// Profiles swept along paths (baseboards, moldings, fascias, gutters).
    Sweep,
}

impl Category {
    pub const ALL: [Category; 23] = [
        Category::Wall,
        Category::Door,
        Category::Window,
        Category::Floor,
        Category::Ceiling,
        Category::Roof,
        Category::Column,
        Category::Beam,
        Category::Foundation,
        Category::Rebar,
        Category::Pipe,
        Category::Conduit,
        Category::Stair,
        Category::Ramp,
        Category::Railing,
        Category::Sweep,
        Category::Component,
        Category::Room,
        Category::Grid,
        Category::Dimension,
        Category::Text,
        Category::CadImport,
        Category::Mass,
    ];

    /// Categories with physical elements (shown in legends and 3D).
    pub const MODEL: [Category; 17] = [
        Category::Wall,
        Category::Door,
        Category::Window,
        Category::Floor,
        Category::Ceiling,
        Category::Roof,
        Category::Column,
        Category::Beam,
        Category::Foundation,
        Category::Rebar,
        Category::Pipe,
        Category::Conduit,
        Category::Stair,
        Category::Ramp,
        Category::Railing,
        Category::Sweep,
        Category::Component,
    ];

    /// Categories that can be listed in a schedule.
    pub const SCHEDULABLE: [Category; 19] = [
        Category::Wall,
        Category::Door,
        Category::Window,
        Category::Floor,
        Category::Ceiling,
        Category::Roof,
        Category::Column,
        Category::Beam,
        Category::Foundation,
        Category::Rebar,
        Category::Pipe,
        Category::Conduit,
        Category::Stair,
        Category::Ramp,
        Category::Railing,
        Category::Sweep,
        Category::Component,
        Category::Room,
        Category::Mass,
    ];

    pub fn is_model(self) -> bool {
        Category::MODEL.contains(&self)
    }

    /// Translation key for the category name.
    pub fn key(self) -> &'static str {
        match self {
            Category::Wall => "category-wall",
            Category::Door => "category-door",
            Category::Window => "category-window",
            Category::Floor => "category-floor",
            Category::Ceiling => "category-ceiling",
            Category::Roof => "category-roof",
            Category::Room => "category-room",
            Category::Grid => "category-grid",
            Category::Dimension => "category-dimension",
            Category::Text => "category-text",
            Category::Column => "category-column",
            Category::Beam => "category-beam",
            Category::Stair => "category-stair",
            Category::Ramp => "category-ramp",
            Category::Railing => "category-railing",
            Category::Sweep => "category-sweep",
            Category::Component => "category-component",
            Category::Foundation => "category-foundation",
            Category::Rebar => "category-rebar",
            Category::Pipe => "category-pipe",
            Category::Conduit => "category-conduit",
            Category::CadImport => "category-cad-import",
            Category::Mass => "category-mass",
        }
    }

    /// Whether the category belongs to the building services (MEP).
    pub fn is_mep(self) -> bool {
        matches!(self, Category::Pipe | Category::Conduit)
    }
}

/// Role of a layer in a compound type. The core (structure layers) is the
/// reference for core-based wall location lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum LayerFunction {
    #[default]
    Structure,
    Substrate,
    Insulation,
    Finish,
    Membrane,
}

impl LayerFunction {
    pub const ALL: [LayerFunction; 5] = [
        LayerFunction::Structure,
        LayerFunction::Substrate,
        LayerFunction::Insulation,
        LayerFunction::Finish,
        LayerFunction::Membrane,
    ];

    /// Translation key of the function name.
    pub fn key(self) -> &'static str {
        match self {
            LayerFunction::Structure => "layer-structure",
            LayerFunction::Substrate => "layer-substrate",
            LayerFunction::Insulation => "layer-insulation",
            LayerFunction::Finish => "layer-finish",
            LayerFunction::Membrane => "layer-membrane",
        }
    }
}

/// One material layer of a compound wall/floor/roof type, listed exterior first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub material: String,
    /// Thickness in meters.
    pub thickness: f64,
    #[serde(default)]
    pub function: LayerFunction,
}

/// Purpose of a wall type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum WallFunction {
    #[default]
    Interior,
    Exterior,
    Foundation,
    Retaining,
}

impl WallFunction {
    pub const ALL: [WallFunction; 4] =
        [WallFunction::Interior, WallFunction::Exterior, WallFunction::Foundation, WallFunction::Retaining];

    pub fn key(self) -> &'static str {
        match self {
            WallFunction::Interior => "wall-fn-interior",
            WallFunction::Exterior => "wall-fn-exterior",
            WallFunction::Foundation => "wall-fn-foundation",
            WallFunction::Retaining => "wall-fn-retaining",
        }
    }

    /// Whether walls of this function face the outside of the building.
    pub fn is_external(self) -> bool {
        !matches!(self, WallFunction::Interior)
    }
}

/// Descriptive data shared by all instances of a type.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TypeInfo {
    /// Short code shown in tags and schedules.
    pub type_mark: String,
    pub description: String,
    pub manufacturer: String,
    pub model: String,
    pub url: String,
    pub keynote: String,
    /// Classification or assembly code (e.g. a national construction classification).
    pub classification: String,
    pub fire_rating: String,
    pub comments: String,
    /// Unit cost in the project currency.
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TypeData {
    Wall {
        layers: Vec<Layer>,
        #[serde(default)]
        function: WallFunction,
    },
    Door {
        width: f64,
        height: f64,
        material: String,
    },
    Window {
        width: f64,
        height: f64,
        default_sill: f64,
        material: String,
    },
    Floor {
        layers: Vec<Layer>,
    },
    Ceiling {
        thickness: f64,
        material: String,
    },
    Roof {
        layers: Vec<Layer>,
        /// The tiles or sheets it is covered with.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        covering: Option<crate::roofing::TileKind>,
        /// Purlins, rafters and battens under its sloped faces.
        #[serde(default)]
        framing: crate::roofing::RoofFraming,
    },
    /// Rooms have no type parameters yet; the type groups them in schedules.
    Room {},
    /// Grid line with a bubble of `bubble` paper millimeters.
    Grid {
        bubble: f64,
    },
    /// Linear dimension with text of `text_size` paper millimeters.
    Dimension {
        text_size: f64,
    },
    /// Text notes of `size` paper millimeters; new leaders get `symbol`.
    Text {
        size: f64,
        #[serde(default)]
        symbol: crate::LeaderSymbol,
    },
    Column {
        section: ColumnSection,
        material: String,
    },
    /// Beam section `width` x `height`, solid or a profile within that box.
    Beam {
        width: f64,
        height: f64,
        material: String,
        #[serde(default)]
        shape: SectionShape,
    },
    /// Reinforcing bars of `diameter`.
    Rebar {
        diameter: f64,
        material: String,
    },
    /// Pipes of outside `diameter`.
    Pipe {
        diameter: f64,
        material: String,
    },
    /// Conduits of outside `diameter`.
    Conduit {
        diameter: f64,
        material: String,
    },
    /// Foundation: a pad `width` x `length`, a strip `width` wide or a slab,
    /// each `thickness` deep.
    Foundation {
        kind: FoundationKind,
        width: f64,
        length: f64,
        thickness: f64,
        material: String,
    },
    /// Stair: risers at most `max_riser` high, treads `tread` deep (or from
    /// Blondel's formula, see `rules`), built as `build` says.
    Stair {
        width: f64,
        max_riser: f64,
        tread: f64,
        material: String,
        #[serde(default)]
        build: crate::StairBuild,
        #[serde(default)]
        rules: crate::StairRules,
    },
    /// Ramp: a slab `thickness` thick, `width` wide, within `rules`.
    Ramp {
        width: f64,
        thickness: f64,
        material: String,
        #[serde(default)]
        rules: crate::RampRules,
    },
    /// Railing with a top rail at `height` and posts every `post_spacing`.
    Railing {
        height: f64,
        post_spacing: f64,
        material: String,
    },
    /// Parametric component: `component` names the definition shared by its
    /// types; the parameter values in `def` are this type's.
    Component {
        component: String,
        def: crate::component::ComponentDef,
    },
    /// Imported CAD drawings have no type parameters; the type names them.
    CadImport {},
    /// Conceptual masses, shown in `material`.
    Mass {
        material: String,
    },
    /// Swept profiles: the closed `profile` (x across, to the left of the
    /// path; y up), shown in `material`.
    Sweep {
        profile: Vec<glam::DVec2>,
        material: String,
    },
}

impl TypeData {
    /// Material names referenced by this type.
    pub fn materials(&self) -> Vec<&str> {
        match self {
            TypeData::Wall { layers, .. } | TypeData::Floor { layers } => {
                layers.iter().map(|l| l.material.as_str()).collect()
            }
            TypeData::Roof { layers, framing, .. } => {
                let framed = (!framing.material.is_empty()).then_some(framing.material.as_str());
                layers.iter().map(|l| l.material.as_str()).chain(framed).collect()
            }
            TypeData::Door { material, .. }
            | TypeData::Window { material, .. }
            | TypeData::Ceiling { material, .. }
            | TypeData::Column { material, .. }
            | TypeData::Beam { material, .. }
            | TypeData::Foundation { material, .. }
            | TypeData::Rebar { material, .. }
            | TypeData::Pipe { material, .. }
            | TypeData::Conduit { material, .. }
            | TypeData::Stair { material, .. }
            | TypeData::Ramp { material, .. }
            | TypeData::Railing { material, .. }
            | TypeData::Mass { material }
            | TypeData::Sweep { material, .. } => vec![material.as_str()],
            TypeData::Component { def, .. } => def.items.iter().map(|i| i.material.as_str()).collect(),
            TypeData::Room {}
            | TypeData::Grid { .. }
            | TypeData::Dimension { .. }
            | TypeData::Text { .. }
            | TypeData::CadImport {} => Vec::new(),
        }
    }

    /// Material shown where the type is cut as a whole (sections): the first
    /// structure layer of layered types (else the thickest), or the main material.
    pub fn cut_material(&self) -> &str {
        match self {
            TypeData::Wall { layers, .. } | TypeData::Floor { layers } | TypeData::Roof { layers, .. } => layers
                .iter()
                .find(|l| l.function == LayerFunction::Structure)
                .or_else(|| layers.iter().max_by(|a, b| a.thickness.total_cmp(&b.thickness)))
                .map_or("", |l| l.material.as_str()),
            other => other.materials().first().copied().unwrap_or(""),
        }
    }

    pub fn uses_material(&self, name: &str) -> bool {
        self.materials().contains(&name)
    }

    /// Point references to material `old` at `new`.
    pub fn rename_material(&mut self, old: &str, new: &str) {
        let fix = |m: &mut String| {
            if m == old {
                *m = new.to_string();
            }
        };
        match self {
            TypeData::Wall { layers, .. } | TypeData::Floor { layers } => {
                layers.iter_mut().for_each(|l| fix(&mut l.material))
            }
            TypeData::Roof { layers, framing, .. } => {
                layers.iter_mut().for_each(|l| fix(&mut l.material));
                fix(&mut framing.material);
            }
            TypeData::Door { material, .. }
            | TypeData::Window { material, .. }
            | TypeData::Ceiling { material, .. }
            | TypeData::Column { material, .. }
            | TypeData::Beam { material, .. }
            | TypeData::Foundation { material, .. }
            | TypeData::Rebar { material, .. }
            | TypeData::Pipe { material, .. }
            | TypeData::Conduit { material, .. }
            | TypeData::Stair { material, .. }
            | TypeData::Ramp { material, .. }
            | TypeData::Railing { material, .. }
            | TypeData::Mass { material }
            | TypeData::Sweep { material, .. } => fix(material),
            TypeData::Component { def, .. } => def.items.iter_mut().for_each(|i| fix(&mut i.material)),
            TypeData::Room {}
            | TypeData::Grid { .. }
            | TypeData::Dimension { .. }
            | TypeData::Text { .. }
            | TypeData::CadImport {} => {}
        }
    }
}

/// Cross-section of a column type.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ColumnSection {
    Rectangular {
        width: f64,
        depth: f64,
    },
    Round {
        diameter: f64,
    },
    /// A profile (I, channel, angle, tube) within a `width` x `depth` box.
    Profile {
        width: f64,
        depth: f64,
        shape: SectionShape,
    },
    /// A circular hollow section.
    Pipe {
        diameter: f64,
        wall: f64,
    },
}

impl ColumnSection {
    /// The bounding box, `(width, depth)`.
    pub fn size(&self) -> (f64, f64) {
        match *self {
            ColumnSection::Rectangular { width, depth } | ColumnSection::Profile { width, depth, .. } => (width, depth),
            ColumnSection::Round { diameter } | ColumnSection::Pipe { diameter, .. } => (diameter, diameter),
        }
    }
}

/// What fills a section's `width` x `depth` box. Profiles are laid out as in
/// steel tables: the web of an I or channel runs along the depth, the legs of
/// an angle lie on the left and bottom sides of the box, and the flanges of a
/// channel point to the right.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum SectionShape {
    #[default]
    Solid,
    /// I or H: two flanges `flange` thick joined by a web `web` thick.
    I { web: f64, flange: f64 },
    /// Channel (U): a web on the left and two flanges.
    Channel { web: f64, flange: f64 },
    /// Angle (L): two legs `thickness` thick.
    Angle { thickness: f64 },
    /// Rectangular hollow section with walls `wall` thick.
    Tube { wall: f64 },
}

impl SectionShape {
    pub fn key(&self) -> &'static str {
        match self {
            SectionShape::Solid => "section-solid",
            SectionShape::I { .. } => "section-i",
            SectionShape::Channel { .. } => "section-channel",
            SectionShape::Angle { .. } => "section-angle",
            SectionShape::Tube { .. } => "section-tube",
        }
    }

    /// The same shape with each thickness clamped to fit a `width` x `depth` box.
    pub fn fitted(self, width: f64, depth: f64) -> Self {
        let fit = |t: f64, max: f64| if t.is_finite() { t.clamp(max.min(0.001), max) } else { max };
        match self {
            SectionShape::Solid => self,
            SectionShape::I { web, flange } => {
                SectionShape::I { web: fit(web, width), flange: fit(flange, depth * 0.5) }
            }
            SectionShape::Channel { web, flange } => {
                SectionShape::Channel { web: fit(web, width), flange: fit(flange, depth * 0.5) }
            }
            SectionShape::Angle { thickness } => SectionShape::Angle { thickness: fit(thickness, width.min(depth)) },
            SectionShape::Tube { wall } => SectionShape::Tube { wall: fit(wall, width.min(depth) * 0.5) },
        }
    }
}

/// What a foundation type is: a pad under a column, a strip under a wall or a slab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum FoundationKind {
    #[default]
    Pad,
    Strip,
    Slab,
}

impl FoundationKind {
    pub const ALL: [FoundationKind; 3] = [FoundationKind::Pad, FoundationKind::Strip, FoundationKind::Slab];

    pub fn key(self) -> &'static str {
        match self {
            FoundationKind::Pad => "foundation-pad",
            FoundationKind::Strip => "foundation-strip",
            FoundationKind::Slab => "foundation-slab",
        }
    }
}

/// An element type: the shared parameters of a group of instances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementType {
    pub id: ElementTypeId,
    pub name: String,
    pub data: TypeData,
    #[serde(default)]
    pub info: TypeInfo,
    #[serde(default)]
    pub uid: Uid,
}

impl ElementType {
    pub fn category(&self) -> Category {
        match &self.data {
            TypeData::Wall { .. } => Category::Wall,
            TypeData::Door { .. } => Category::Door,
            TypeData::Window { .. } => Category::Window,
            TypeData::Floor { .. } => Category::Floor,
            TypeData::Ceiling { .. } => Category::Ceiling,
            TypeData::Roof { .. } => Category::Roof,
            TypeData::Room {} => Category::Room,
            TypeData::Grid { .. } => Category::Grid,
            TypeData::Dimension { .. } => Category::Dimension,
            TypeData::Text { .. } => Category::Text,
            TypeData::Column { .. } => Category::Column,
            TypeData::Beam { .. } => Category::Beam,
            TypeData::Foundation { .. } => Category::Foundation,
            TypeData::Rebar { .. } => Category::Rebar,
            TypeData::Pipe { .. } => Category::Pipe,
            TypeData::Conduit { .. } => Category::Conduit,
            TypeData::Stair { .. } => Category::Stair,
            TypeData::Ramp { .. } => Category::Ramp,
            TypeData::Railing { .. } => Category::Railing,
            TypeData::Component { def, .. } => def.category(),
            TypeData::CadImport {} => Category::CadImport,
            TypeData::Mass { .. } => Category::Mass,
            TypeData::Sweep { .. } => Category::Sweep,
        }
    }

    pub fn layers(&self) -> &[Layer] {
        match &self.data {
            TypeData::Wall { layers, .. } | TypeData::Floor { layers } | TypeData::Roof { layers, .. } => layers,
            _ => &[],
        }
    }

    /// Total thickness for layered types, 0 for openings.
    pub fn thickness(&self) -> f64 {
        match &self.data {
            TypeData::Ceiling { thickness, .. } | TypeData::Foundation { thickness, .. } => *thickness,
            _ => self.layers().iter().map(|l| l.thickness).sum(),
        }
    }

    /// Outside diameter of pipe, conduit and rebar types.
    pub fn diameter(&self) -> Option<f64> {
        match self.data {
            TypeData::Pipe { diameter, .. } | TypeData::Conduit { diameter, .. } | TypeData::Rebar { diameter, .. } => {
                Some(diameter)
            }
            _ => None,
        }
    }

    /// Wall function, for wall types.
    pub fn wall_function(&self) -> Option<WallFunction> {
        match self.data {
            TypeData::Wall { function, .. } => Some(function),
            _ => None,
        }
    }

    /// Distances from the exterior (first) face to the exterior and interior
    /// faces of the core: the span from the first to the last structure layer,
    /// or the whole thickness when there is none.
    pub fn core_range(&self) -> (f64, f64) {
        let layers = self.layers();
        let first = layers.iter().position(|l| l.function == LayerFunction::Structure);
        let last = layers.iter().rposition(|l| l.function == LayerFunction::Structure);
        match (first, last) {
            (Some(a), Some(b)) => {
                let before: f64 = layers[..a].iter().map(|l| l.thickness).sum();
                let core: f64 = layers[a..=b].iter().map(|l| l.thickness).sum();
                (before, before + core)
            }
            _ => (0.0, self.thickness()),
        }
    }

    /// Width/height of an opening type (doors, windows).
    pub fn opening_size(&self) -> Option<(f64, f64)> {
        match &self.data {
            TypeData::Door { width, height, .. } | TypeData::Window { width, height, .. } => Some((*width, *height)),
            TypeData::Component { def, .. } => def.opening_size(),
            _ => None,
        }
    }

    /// Sill height of new elements of a door or window type.
    pub fn default_sill(&self) -> f64 {
        match &self.data {
            TypeData::Window { default_sill, .. } => *default_sill,
            TypeData::Component { def, .. } => def.default_sill(),
            _ => 0.0,
        }
    }

    /// Material used to color the element in shaded views: the exterior finish
    /// layer for walls and roofs, the thickest layer for floors.
    pub fn main_material(&self) -> &str {
        match &self.data {
            TypeData::Door { material, .. }
            | TypeData::Window { material, .. }
            | TypeData::Ceiling { material, .. } => material,
            TypeData::Wall { layers, .. } | TypeData::Roof { layers, .. } => {
                layers.first().map_or("", |l| l.material.as_str())
            }
            TypeData::Room {}
            | TypeData::Grid { .. }
            | TypeData::Dimension { .. }
            | TypeData::Text { .. }
            | TypeData::CadImport {} => "",
            TypeData::Column { material, .. }
            | TypeData::Beam { material, .. }
            | TypeData::Foundation { material, .. }
            | TypeData::Rebar { material, .. }
            | TypeData::Pipe { material, .. }
            | TypeData::Conduit { material, .. }
            | TypeData::Stair { material, .. }
            | TypeData::Ramp { material, .. }
            | TypeData::Railing { material, .. }
            | TypeData::Mass { material }
            | TypeData::Sweep { material, .. } => material,
            TypeData::Component { def, .. } => def.main_material(),
            TypeData::Floor { .. } => self
                .layers()
                .iter()
                .max_by(|a, b| a.thickness.total_cmp(&b.thickness))
                .map(|l| l.material.as_str())
                .unwrap_or(""),
        }
    }
}

/// Riser count, riser height and run length of a straight stair of `height`
/// with at most `max_riser` per riser and `tread`-deep treads (one tread fewer
/// than risers: the last riser lands on the upper floor).
pub fn stair_layout(height: f64, max_riser: f64, tread: f64) -> (usize, f64, f64) {
    let max_riser = if max_riser.is_finite() && max_riser > 0.01 { max_riser } else { 0.18 };
    let risers = ((height / max_riser).ceil().max(1.0) as usize).min(200);
    let riser = height / risers as f64;
    (risers, riser, tread.max(0.0) * (risers.saturating_sub(1)) as f64)
}
