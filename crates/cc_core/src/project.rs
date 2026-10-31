use crate::material::Material;
use crate::{
    CameraState, Category, ColumnSection, Direction, Element, ElementId, ElementKind, ElementType, ElementTypeId,
    FoundationKind, Layer, LayerFunction, LevelId, SectionShape, SheetSize, TypeData, View, ViewCategory, ViewId,
    ViewKind, Viewport, WallFunction,
};
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// Persistent unique id of the project.
    #[serde(default)]
    pub uid: crate::Uid,
    pub name: String,
    pub number: String,
    pub client: String,
    pub address: String,
    pub author: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub id: LevelId,
    pub name: String,
    /// Elevation in meters.
    pub elevation: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub info: ProjectInfo,
    /// Sorted by elevation.
    pub levels: Vec<Level>,
    pub materials: Vec<Material>,
    pub types: BTreeMap<ElementTypeId, ElementType>,
    pub elements: BTreeMap<ElementId, Element>,
    pub views: BTreeMap<ViewId, View>,
    /// How quantities are displayed and typed.
    #[serde(default)]
    pub units: crate::Units,
    /// Elements protected from moving and deleting.
    #[serde(default)]
    pub pinned: std::collections::BTreeSet<ElementId>,
    /// Joined geometry as `(cutting, cut)` pairs: where the two overlap,
    /// the first keeps the volume and their cut outlines merge.
    #[serde(default)]
    pub joins: std::collections::BTreeSet<(ElementId, ElementId)>,
    /// Named sets of elements selected, moved and copied together.
    #[serde(default)]
    pub groups: BTreeMap<crate::GroupId, Group>,
    /// Plumbing systems and electrical circuits.
    #[serde(default)]
    pub systems: BTreeMap<crate::SystemId, crate::MepSystem>,
    /// Named view filters (categories and rules), applied by views.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub filters: BTreeMap<crate::FilterId, crate::ViewFilter>,
    /// View templates.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub templates: BTreeMap<crate::TemplateId, crate::ViewTemplate>,
    /// Construction phases, in time order (see [`crate::phase`]).
    #[serde(default)]
    pub phases: Vec<crate::Phase>,
    /// Phase filters views choose from.
    #[serde(default)]
    pub phase_filters: BTreeMap<crate::PhaseFilterId, crate::PhaseFilter>,
    /// Graphics of each phase status, where a phase filter overrides it.
    #[serde(default, skip_serializing_if = "crate::phase::phase_graphics_is_default")]
    pub phase_graphics: crate::PhaseGraphics,
    next_id: u64,
}

/// Elements kept together: selecting one selects them all. An element
/// belongs to one group at most.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub name: String,
    pub members: std::collections::BTreeSet<ElementId>,
}

impl Project {
    /// An empty project with no levels, types or views.
    pub fn empty() -> Self {
        Self {
            info: ProjectInfo::default(),
            levels: Vec::new(),
            materials: Vec::new(),
            types: BTreeMap::new(),
            elements: BTreeMap::new(),
            views: BTreeMap::new(),
            units: crate::Units::default(),
            pinned: Default::default(),
            joins: Default::default(),
            groups: BTreeMap::new(),
            systems: BTreeMap::new(),
            filters: BTreeMap::new(),
            templates: BTreeMap::new(),
            phases: Vec::new(),
            phase_filters: BTreeMap::new(),
            phase_graphics: crate::PhaseGraphics::default(),
            next_id: 1,
        }
    }

    /// A new project from the default architectural template (Brazilian Portuguese names).
    pub fn from_template() -> Self {
        Self::from_template_localized(&|_| None)
    }

    /// A new project from the template, naming levels, materials, types and views
    /// with `translate(key)` (keys `tpl-*`), falling back to Portuguese.
    pub fn from_template_localized(translate: &dyn Fn(&str) -> Option<String>) -> Self {
        let n = |key: &str| translate(key).unwrap_or_else(|| template_default(key).to_string());
        let mut p = Self::empty();
        p.info = ProjectInfo {
            uid: crate::Uid::default(),
            name: n("tpl-project-name"),
            number: "0001".into(),
            client: String::new(),
            address: String::new(),
            author: String::new(),
        };
        let ground = LevelId(p.alloc_id());
        let first = LevelId(p.alloc_id());
        p.levels = vec![
            Level { id: ground, name: n("tpl-level-ground"), elevation: 0.0 },
            Level { id: first, name: n("tpl-level-first"), elevation: 3.0 },
        ];
        use crate::material::MaterialClass as C;
        p.materials = [
            (&n("tpl-mat-masonry"), [196, 120, 92], C::Masonry),
            (&n("tpl-mat-plaster"), [232, 226, 214], C::Mortar),
            (&n("tpl-mat-concrete"), [170, 170, 165], C::Concrete),
            (&n("tpl-mat-wood"), [150, 104, 62], C::Wood),
            (&n("tpl-mat-glass"), [150, 200, 225], C::Glass),
            (&n("tpl-mat-gypsum"), [245, 245, 240], C::Gypsum),
            (&n("tpl-mat-roof-tile"), [168, 70, 48], C::Ceramic),
            (&n("tpl-mat-screed"), [190, 185, 175], C::Mortar),
            (&n("tpl-mat-fabric"), [120, 132, 150], C::Fabric),
            (&n("tpl-mat-porcelain"), [240, 240, 236], C::Ceramic),
            (&n("tpl-mat-steel"), [140, 146, 152], C::Metal),
            (&n("tpl-mat-pvc"), [236, 236, 230], C::Plastic),
        ]
        .into_iter()
        .map(|(name, color, class)| {
            let mut m = Material::new(name.as_str(), color);
            crate::material::template_properties(&mut m, class);
            m
        })
        .collect();

        let layer = |m: &str, t: f64| Layer { material: m.into(), thickness: t, function: LayerFunction::Structure };
        let finish = |m: &str, t: f64| Layer { material: m.into(), thickness: t, function: LayerFunction::Finish };
        let names = crate::openings::OpeningNames::new(&n);
        let door = |w: f64, h: f64, m: &str| TypeData::Component {
            component: names.door.clone(),
            def: crate::openings::door_def(&names, w, h, m),
        };
        let window = |w: f64, h: f64, sill: f64, m: &str| TypeData::Component {
            component: names.window.clone(),
            def: crate::openings::window_def(&names, w, h, sill, m),
        };
        let types = [
            (
                &n("tpl-type-masonry-15"),
                TypeData::Wall {
                    layers: vec![
                        finish(&n("tpl-mat-plaster"), 0.02),
                        layer(&n("tpl-mat-masonry"), 0.11),
                        finish(&n("tpl-mat-plaster"), 0.02),
                    ],
                    function: WallFunction::Exterior,
                },
            ),
            (
                &n("tpl-type-masonry-25"),
                TypeData::Wall {
                    layers: vec![
                        finish(&n("tpl-mat-plaster"), 0.025),
                        layer(&n("tpl-mat-masonry"), 0.20),
                        finish(&n("tpl-mat-plaster"), 0.025),
                    ],
                    function: WallFunction::Exterior,
                },
            ),
            (
                &n("tpl-type-drywall-10"),
                TypeData::Wall { layers: vec![layer(&n("tpl-mat-gypsum"), 0.10)], function: WallFunction::Interior },
            ),
            (&n("tpl-type-door-80"), door(0.8, 2.1, &n("tpl-mat-wood"))),
            (&n("tpl-type-door-90"), door(0.9, 2.1, &n("tpl-mat-wood"))),
            (&n("tpl-type-window-120"), window(1.2, 1.2, 1.0, &n("tpl-mat-glass"))),
            (&n("tpl-type-window-150"), window(1.5, 1.0, 1.1, &n("tpl-mat-glass"))),
            (
                &n("tpl-type-slab-12"),
                TypeData::Floor {
                    layers: vec![finish(&n("tpl-mat-screed"), 0.03), layer(&n("tpl-mat-concrete"), 0.12)],
                },
            ),
            (&n("tpl-type-ceiling-gypsum"), TypeData::Ceiling { thickness: 0.02, material: n("tpl-mat-gypsum") }),
            (
                &n("tpl-type-column-rect"),
                TypeData::Column {
                    section: ColumnSection::Rectangular { width: 0.2, depth: 0.2 },
                    material: n("tpl-mat-concrete"),
                },
            ),
            (
                &n("tpl-type-column-round"),
                TypeData::Column { section: ColumnSection::Round { diameter: 0.3 }, material: n("tpl-mat-concrete") },
            ),
            (
                &n("tpl-type-beam"),
                TypeData::Beam {
                    width: 0.15,
                    height: 0.4,
                    material: n("tpl-mat-concrete"),
                    shape: SectionShape::Solid,
                },
            ),
            (
                &n("tpl-type-column-steel-h"),
                TypeData::Column {
                    section: ColumnSection::Profile {
                        width: 0.2,
                        depth: 0.2,
                        shape: SectionShape::I { web: 0.009, flange: 0.015 },
                    },
                    material: n("tpl-mat-steel"),
                },
            ),
            (
                &n("tpl-type-column-steel-tube"),
                TypeData::Column {
                    section: ColumnSection::Profile {
                        width: 0.15,
                        depth: 0.15,
                        shape: SectionShape::Tube { wall: 0.006 },
                    },
                    material: n("tpl-mat-steel"),
                },
            ),
            (
                &n("tpl-type-beam-steel-i"),
                TypeData::Beam {
                    width: 0.15,
                    height: 0.3,
                    material: n("tpl-mat-steel"),
                    shape: SectionShape::I { web: 0.0071, flange: 0.0107 },
                },
            ),
            (&n("tpl-type-rebar-5"), TypeData::Rebar { diameter: 0.005, material: n("tpl-mat-steel") }),
            (&n("tpl-type-rebar-8"), TypeData::Rebar { diameter: 0.008, material: n("tpl-mat-steel") }),
            (&n("tpl-type-rebar-10"), TypeData::Rebar { diameter: 0.010, material: n("tpl-mat-steel") }),
            (&n("tpl-type-rebar-12"), TypeData::Rebar { diameter: 0.0125, material: n("tpl-mat-steel") }),
            (&n("tpl-type-rebar-16"), TypeData::Rebar { diameter: 0.016, material: n("tpl-mat-steel") }),
            (
                &n("tpl-type-footing-pad"),
                TypeData::Foundation {
                    kind: FoundationKind::Pad,
                    width: 1.0,
                    length: 1.0,
                    thickness: 0.4,
                    material: n("tpl-mat-concrete"),
                },
            ),
            (
                &n("tpl-type-footing-strip"),
                TypeData::Foundation {
                    kind: FoundationKind::Strip,
                    width: 0.2,
                    length: 1.0,
                    thickness: 0.4,
                    material: n("tpl-mat-concrete"),
                },
            ),
            (
                &n("tpl-type-footing-slab"),
                TypeData::Foundation {
                    kind: FoundationKind::Slab,
                    width: 1.0,
                    length: 1.0,
                    thickness: 0.3,
                    material: n("tpl-mat-concrete"),
                },
            ),
            (
                &n("tpl-type-stair"),
                TypeData::Stair {
                    width: 1.0,
                    max_riser: 0.18,
                    tread: 0.28,
                    material: n("tpl-mat-concrete"),
                    build: Default::default(),
                    rules: Default::default(),
                },
            ),
            (&n("tpl-type-railing"), TypeData::Railing { height: 1.1, post_spacing: 1.2, material: n("tpl-mat-wood") }),
            (&n("tpl-type-pipe-20"), TypeData::Pipe { diameter: 0.020, material: n("tpl-mat-pvc") }),
            (&n("tpl-type-pipe-25"), TypeData::Pipe { diameter: 0.025, material: n("tpl-mat-pvc") }),
            (&n("tpl-type-pipe-32"), TypeData::Pipe { diameter: 0.032, material: n("tpl-mat-pvc") }),
            (&n("tpl-type-pipe-sewer-40"), TypeData::Pipe { diameter: 0.040, material: n("tpl-mat-pvc") }),
            (&n("tpl-type-pipe-sewer-50"), TypeData::Pipe { diameter: 0.050, material: n("tpl-mat-pvc") }),
            (&n("tpl-type-pipe-sewer-75"), TypeData::Pipe { diameter: 0.075, material: n("tpl-mat-pvc") }),
            (&n("tpl-type-pipe-sewer-100"), TypeData::Pipe { diameter: 0.100, material: n("tpl-mat-pvc") }),
            (&n("tpl-type-conduit-20"), TypeData::Conduit { diameter: 0.020, material: n("tpl-mat-pvc") }),
            (&n("tpl-type-conduit-25"), TypeData::Conduit { diameter: 0.025, material: n("tpl-mat-pvc") }),
            (&n("tpl-type-conduit-32"), TypeData::Conduit { diameter: 0.032, material: n("tpl-mat-pvc") }),
            (&n("tpl-type-room"), TypeData::Room {}),
            (&n("tpl-type-grid"), TypeData::Grid { bubble: 8.0 }),
            (&n("tpl-type-dimension"), TypeData::Dimension { text_size: 2.5 }),
            (&n("tpl-type-text"), TypeData::Text { size: 2.5, symbol: crate::LeaderSymbol::Arrow }),
            (&n("tpl-type-text-large"), TypeData::Text { size: 3.5, symbol: crate::LeaderSymbol::Arrow }),
            (
                &n("tpl-type-roof-tile"),
                TypeData::Roof {
                    layers: vec![finish(&n("tpl-mat-roof-tile"), 0.05), layer(&n("tpl-mat-wood"), 0.10)],
                    covering: None,
                    framing: Default::default(),
                },
            ),
        ];
        for (name, data) in types {
            let id = ElementTypeId(p.alloc_id());
            p.types.insert(
                id,
                ElementType { id, name: name.into(), data, info: Default::default(), uid: Default::default() },
            );
        }
        for (component, name, def) in crate::component::library(&n) {
            let id = ElementTypeId(p.alloc_id());
            let data = TypeData::Component { component, def };
            p.types.insert(id, ElementType { id, name, data, info: Default::default(), uid: Default::default() });
        }

        p.add_default_systems(&n);

        let add_view = |p: &mut Project, name: &str, scale: u32, kind: ViewKind| {
            let id = ViewId(p.alloc_id());
            p.views.insert(id, View::new(id, name, scale, kind));
            id
        };
        let plan = add_view(&mut p, &n("tpl-level-ground"), 50, ViewKind::floor_plan(ground));
        add_view(&mut p, &n("tpl-level-first"), 50, ViewKind::floor_plan(first));
        add_view(&mut p, &n("tpl-level-ground"), 50, ViewKind::CeilingPlan { level: ground, cut_height: 1.2 });
        add_view(&mut p, &n("tpl-level-first"), 50, ViewKind::CeilingPlan { level: first, cut_height: 1.2 });
        let threed = add_view(
            &mut p,
            "{3D}",
            100,
            ViewKind::ThreeD { camera: CameraState::default(), analytical: false, rebar: false },
        );
        for (name, direction) in [
            (&n("tpl-elev-north"), Direction::North),
            (&n("tpl-elev-south"), Direction::South),
            (&n("tpl-elev-east"), Direction::East),
            (&n("tpl-elev-west"), Direction::West),
        ] {
            add_view(&mut p, name, 100, ViewKind::Elevation { direction });
        }
        let all_types: Vec<_> = p.types.values().filter(|t| t.category().is_model()).map(|t| t.id).collect();
        add_view(&mut p, &n("tpl-legend"), 50, ViewKind::Legend { types: all_types });
        for (name, category) in [
            (&n("tpl-schedule-doors"), Category::Door),
            (&n("tpl-schedule-windows"), Category::Window),
            (&n("tpl-schedule-walls"), Category::Wall),
            (&n("tpl-schedule-rooms"), Category::Room),
        ] {
            add_view(&mut p, name, 1, ViewKind::Schedule { category, def: Default::default() });
        }
        add_view(
            &mut p,
            &n("tpl-sheet-plans"),
            1,
            ViewKind::Sheet {
                number: "A101".into(),
                size: SheetSize::A3,
                viewports: vec![
                    Viewport { view: plan, center: DVec2::new(130.0, 160.0) },
                    Viewport { view: threed, center: DVec2::new(320.0, 190.0) },
                ],
            },
        );
        p.ensure_phases(&n);
        p.ensure_uids();
        p
    }

    /// Assign persistent unique ids to the project, types and elements that
    /// lack one, and replace duplicates (e.g. from copied elements).
    pub fn ensure_uids(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let mut fresh = |uid: &mut crate::Uid| {
            if uid.is_nil() || !seen.insert(*uid) {
                *uid = crate::Uid::generate();
                seen.insert(*uid);
            }
        };
        fresh(&mut self.info.uid);
        self.types.values_mut().for_each(|t| fresh(&mut t.uid));
        self.elements.values_mut().for_each(|e| fresh(&mut e.uid));
    }

    /// Add default room, grid and dimension types to projects created before
    /// those categories existed.
    pub fn ensure_annotation_types(&mut self) {
        let defaults = [
            (
                Category::Column,
                "tpl-type-column-rect",
                TypeData::Column {
                    section: ColumnSection::Rectangular { width: 0.2, depth: 0.2 },
                    material: template_default("tpl-mat-concrete").to_string(),
                },
            ),
            (
                Category::Beam,
                "tpl-type-beam",
                TypeData::Beam {
                    width: 0.15,
                    height: 0.4,
                    material: template_default("tpl-mat-concrete").to_string(),
                    shape: SectionShape::Solid,
                },
            ),
            (
                Category::Stair,
                "tpl-type-stair",
                TypeData::Stair {
                    width: 1.0,
                    max_riser: 0.18,
                    tread: 0.28,
                    material: template_default("tpl-mat-concrete").to_string(),
                    build: Default::default(),
                    rules: Default::default(),
                },
            ),
            (
                Category::Railing,
                "tpl-type-railing",
                TypeData::Railing {
                    height: 1.1,
                    post_spacing: 1.2,
                    material: template_default("tpl-mat-wood").to_string(),
                },
            ),
            (Category::Room, "tpl-type-room", TypeData::Room {}),
            (Category::Grid, "tpl-type-grid", TypeData::Grid { bubble: 8.0 }),
            (Category::Dimension, "tpl-type-dimension", TypeData::Dimension { text_size: 2.5 }),
            (Category::Text, "tpl-type-text", TypeData::Text { size: 2.5, symbol: crate::LeaderSymbol::Arrow }),
        ];
        for (category, key, data) in defaults {
            if self.default_type(category).is_none() {
                let id = ElementTypeId(self.alloc_id());
                let name = template_default(key).to_string();
                self.types
                    .insert(id, ElementType { id, name, data, info: Default::default(), uid: crate::Uid::generate() });
            }
        }
        // Projects created before foundations get a pad, a strip and a slab type.
        if self.default_type(Category::Foundation).is_none() {
            let concrete = template_default("tpl-mat-concrete").to_string();
            for (key, kind, width, thickness) in [
                ("tpl-type-footing-pad", crate::FoundationKind::Pad, 1.0, 0.4),
                ("tpl-type-footing-strip", crate::FoundationKind::Strip, 0.2, 0.4),
                ("tpl-type-footing-slab", crate::FoundationKind::Slab, 1.0, 0.3),
            ] {
                let id = ElementTypeId(self.alloc_id());
                let data = TypeData::Foundation { kind, width, length: 1.0, thickness, material: concrete.clone() };
                let name = template_default(key).to_string();
                self.types
                    .insert(id, ElementType { id, name, data, info: Default::default(), uid: crate::Uid::generate() });
            }
        }
        // Projects created before reinforcement get the usual bar diameters, in steel.
        if self.default_type(Category::Rebar).is_none() {
            let steel = template_default("tpl-mat-steel").to_string();
            if !self.materials.iter().any(|m| m.name == steel) {
                let mut m = Material::new(steel.as_str(), [140, 146, 152]);
                crate::material::template_properties(&mut m, crate::material::MaterialClass::Metal);
                self.materials.push(m);
            }
            for (key, diameter) in [
                ("tpl-type-rebar-5", 0.005),
                ("tpl-type-rebar-8", 0.008),
                ("tpl-type-rebar-10", 0.010),
                ("tpl-type-rebar-12", 0.0125),
                ("tpl-type-rebar-16", 0.016),
            ] {
                let id = ElementTypeId(self.alloc_id());
                let data = TypeData::Rebar { diameter, material: steel.clone() };
                let name = template_default(key).to_string();
                self.types
                    .insert(id, ElementType { id, name, data, info: Default::default(), uid: crate::Uid::generate() });
            }
        }
        // Projects created before building services get pipes, conduits, the
        // fixtures and devices of the library and the usual plumbing systems.
        if self.default_type(Category::Pipe).is_none() {
            let n = |key: &str| template_default(key).to_string();
            let pvc = n("tpl-mat-pvc");
            if !self.materials.iter().any(|m| m.name == pvc) {
                let mut m = Material::new(pvc.as_str(), [236, 236, 230]);
                crate::material::template_properties(&mut m, crate::material::MaterialClass::Plastic);
                self.materials.push(m);
            }
            let types = [
                ("tpl-type-pipe-20", TypeData::Pipe { diameter: 0.020, material: pvc.clone() }),
                ("tpl-type-pipe-25", TypeData::Pipe { diameter: 0.025, material: pvc.clone() }),
                ("tpl-type-pipe-sewer-50", TypeData::Pipe { diameter: 0.050, material: pvc.clone() }),
                ("tpl-type-pipe-sewer-100", TypeData::Pipe { diameter: 0.100, material: pvc.clone() }),
            ];
            for (key, data) in types {
                let id = ElementTypeId(self.alloc_id());
                self.types.insert(
                    id,
                    ElementType { id, name: n(key), data, info: Default::default(), uid: crate::Uid::generate() },
                );
            }
            // Projects without the library get all of it below.
            if self.default_type(Category::Component).is_some() {
                for (component, name, def) in crate::component::mep_library(&n) {
                    let id = ElementTypeId(self.alloc_id());
                    let data = TypeData::Component { component, def };
                    self.types.insert(
                        id,
                        ElementType { id, name, data, info: Default::default(), uid: crate::Uid::generate() },
                    );
                }
            }
            if self.systems.is_empty() {
                self.add_default_systems(&n);
            }
        }
        if self.default_type(Category::Conduit).is_none() {
            let pvc = template_default("tpl-mat-pvc").to_string();
            for (key, diameter) in [("tpl-type-conduit-20", 0.020), ("tpl-type-conduit-25", 0.025)] {
                let id = ElementTypeId(self.alloc_id());
                let data = TypeData::Conduit { diameter, material: pvc.clone() };
                let name = template_default(key).to_string();
                self.types
                    .insert(id, ElementType { id, name, data, info: Default::default(), uid: crate::Uid::generate() });
            }
        }
        // Projects created before components get the default library.
        if self.default_type(Category::Component).is_none() {
            let n = |key: &str| template_default(key).to_string();
            use crate::material::MaterialClass as C;
            for (key, color, class) in
                [("tpl-mat-fabric", [120, 132, 150], C::Fabric), ("tpl-mat-porcelain", [240, 240, 236], C::Ceramic)]
            {
                let name = n(key);
                if !self.materials.iter().any(|m| m.name == name) {
                    let mut m = Material::new(name, color);
                    crate::material::template_properties(&mut m, class);
                    self.materials.push(m);
                }
            }
            for (component, name, def) in crate::component::library(&n) {
                let id = ElementTypeId(self.alloc_id());
                let data = TypeData::Component { component, def };
                self.types
                    .insert(id, ElementType { id, name, data, info: Default::default(), uid: crate::Uid::generate() });
            }
        }
    }

    /// Add the usual plumbing systems (names and codes from `n`, keys `tpl-sys-*`).
    fn add_default_systems(&mut self, n: &dyn Fn(&str) -> String) {
        use crate::SystemKind as K;
        for (name, code, kind) in [
            ("tpl-sys-cold-water", "tpl-sys-cold-water-code", K::ColdWater),
            ("tpl-sys-hot-water", "tpl-sys-hot-water-code", K::HotWater),
            ("tpl-sys-sanitary", "tpl-sys-sanitary-code", K::Sanitary),
            ("tpl-sys-vent", "tpl-sys-vent-code", K::Vent),
            ("tpl-sys-rainwater", "tpl-sys-rainwater-code", K::Rainwater),
        ] {
            let id = crate::SystemId(self.alloc_id());
            self.systems.insert(id, crate::MepSystem::new(n(name), n(code), kind));
        }
    }

    pub fn system(&self, id: crate::SystemId) -> Option<&crate::MepSystem> {
        self.systems.get(&id)
    }

    /// Display color of a pipe, conduit or device: its (first) system's, or gray.
    pub fn system_color(&self, element: &Element) -> [u8; 3] {
        element.systems().first().and_then(|s| self.system(*s)).map_or([128, 128, 128], crate::MepSystem::color)
    }

    /// The MEP data of a component element's type.
    pub fn mep_data(&self, element: &Element) -> Option<crate::MepData> {
        match (&element.kind, self.element_type(element.type_id).map(|t| &t.data)) {
            (ElementKind::Component(_), Some(TypeData::Component { def, .. })) => def.mep,
            _ => None,
        }
    }

    /// A pipe or conduit run as seen from `level`, with its elevations above
    /// that level: the level's own runs, and the vertical runs of other levels
    /// that pass through it (risers between floors, reaching at least its floor
    /// and starting below its top). `None` for other elements and runs.
    pub fn segment_at(&self, element: &Element, level: LevelId) -> Option<crate::MepSegment> {
        let s = element.as_segment()?;
        if element.level == level {
            return Some(s.clone());
        }
        if !s.is_vertical() || self.level(level).is_none() {
            return None;
        }
        let shift = self.level_elevation(element.level) - self.level_elevation(level);
        let (lo, hi) = (s.start_offset.min(s.end_offset) + shift, s.start_offset.max(s.end_offset) + shift);
        let top = self.level_height(level, 3.0);
        (hi >= -0.01 && lo < top - 0.01 && shift.is_finite()).then(|| crate::MepSegment {
            start_offset: s.start_offset + shift,
            end_offset: s.end_offset + shift,
            ..s.clone()
        })
    }

    /// The pipe and conduit runs seen from `level` (see [`Project::segment_at`]).
    pub fn segments_at(&self, level: LevelId) -> impl Iterator<Item = (&Element, crate::MepSegment)> {
        self.elements.values().filter_map(move |e| Some((e, self.segment_at(e, level)?)))
    }

    /// Elements in system `id`: its pipes, the conduits carrying the circuit
    /// and the components connected to it.
    pub fn system_members(&self, id: crate::SystemId) -> impl Iterator<Item = &Element> {
        self.elements.values().filter(move |e| e.systems().contains(&id))
    }

    /// The level above `id`, or `id` itself plus `fallback` meters: the default upper limit of rooms.
    pub fn level_height(&self, id: LevelId, fallback: f64) -> f64 {
        self.level_above(id).map_or(fallback, |above| above.elevation - self.level_elevation(id))
    }

    /// Base elevation and top elevation of a column (top at least 1 cm above).
    pub fn column_span(&self, element: &Element, column: &crate::Column) -> (f64, f64) {
        let base = self.level_elevation(element.level) + column.base_offset;
        let top = match column.top {
            crate::WallTop::Unconnected => base + column.height,
            crate::WallTop::Level { level, offset } => self.level_elevation(level) + offset,
        };
        (base, top.max(base + 0.01))
    }

    /// Base and top elevation of a stair (the top at least one riser above).
    pub fn stair_span(&self, element: &Element, stair: &crate::Stair) -> (f64, f64) {
        let base = self.level_elevation(element.level) + stair.base_offset;
        let top = match stair.top {
            crate::WallTop::Unconnected => base + stair.height,
            crate::WallTop::Level { level, offset } => self.level_elevation(level) + offset,
        };
        (base, top.max(base + 0.05))
    }

    /// Base and top elevation of a ramp (the top at least 1 cm above).
    pub fn ramp_span(&self, element: &Element, ramp: &crate::Ramp) -> (f64, f64) {
        let base = self.level_elevation(element.level) + ramp.base_offset;
        let top = match ramp.top {
            crate::WallTop::Unconnected => base + ramp.height,
            crate::WallTop::Level { level, offset } => self.level_elevation(level) + offset,
        };
        (base, top.max(base + 0.01))
    }

    /// Elevation of a wall's base.
    pub fn wall_base_z(&self, element: &Element, wall: &crate::Wall) -> f64 {
        self.level_elevation(element.level) + wall.base_offset
    }

    /// Elevation of a wall's top, from its top constraint (at least 1 cm above the base).
    pub fn wall_top_z(&self, element: &Element, wall: &crate::Wall) -> f64 {
        let base = self.wall_base_z(element, wall);
        let top = match wall.top {
            crate::WallTop::Unconnected => base + wall.height,
            crate::WallTop::Level { level, offset } => self.level_elevation(level) + offset,
        };
        top.max(base + 0.01)
    }

    /// Effective height of a wall.
    pub fn wall_height(&self, element: &Element, wall: &crate::Wall) -> f64 {
        self.wall_top_z(element, wall) - self.wall_base_z(element, wall)
    }

    /// Start and end of a wall's body centerline: the location line shifted
    /// across the wall according to `location_line` and the type's layers
    /// (listed from the exterior face, on the left of the line). An arc
    /// wall's centerline is concentric with its location line and has the
    /// same bulge.
    pub fn wall_axis(&self, element: &Element, wall: &crate::Wall) -> (DVec2, DVec2) {
        wall.offset_line(-self.wall_line_offset(element, wall))
    }

    /// Length of a wall's body centerline (the location line's length for
    /// straight walls; arc walls measure along the centerline arc).
    pub fn wall_length(&self, element: &Element, wall: &crate::Wall) -> f64 {
        let (start, end) = self.wall_axis(element, wall);
        crate::Wall { start, end, ..wall.clone() }.length()
    }

    /// Position of a wall's location line across the wall, measured from the
    /// centerline towards the exterior (left) face.
    fn wall_line_offset(&self, element: &Element, wall: &crate::Wall) -> f64 {
        use crate::LocationLine as L;
        let Some(ty) = self.element_type(element.type_id) else { return 0.0 };
        let half = ty.thickness() * 0.5;
        let (core_ext, core_int) = ty.core_range();
        match wall.location_line {
            L::Centerline => 0.0,
            L::FinishExterior => half,
            L::FinishInterior => -half,
            L::CoreExterior => half - core_ext,
            L::CoreInterior => half - core_int,
            L::CoreCenter => half - (core_ext + core_int) * 0.5,
        }
    }

    /// The beam's body centerline: its location line moved across by the
    /// lateral justification.
    pub fn beam_axis(&self, element: &Element, beam: &crate::Beam) -> (DVec2, DVec2) {
        let width = match self.element_type(element.type_id).map(|t| &t.data) {
            Some(crate::TypeData::Beam { width, .. }) => *width,
            _ => 0.0,
        };
        let normal = (beam.end - beam.start).normalize_or_zero().perp();
        let shift = match beam.lateral {
            crate::BeamLateral::Center => DVec2::ZERO,
            // The location line runs along the left face: the body lies to its right.
            crate::BeamLateral::Left => -normal * (width * 0.5),
            crate::BeamLateral::Right => normal * (width * 0.5),
        };
        (beam.start + shift, beam.end + shift)
    }

    /// Bottom and top elevations of a beam at its start, from its location
    /// elevation and vertical justification. A sloped beam's section stays
    /// square to its axis, so its vertical depth is a little more than its height.
    pub fn beam_z(&self, element: &Element, beam: &crate::Beam) -> (f64, f64) {
        let height = match self.element_type(element.type_id).map(|t| &t.data) {
            Some(crate::TypeData::Beam { height, .. }) => *height,
            _ => 0.0,
        };
        let length = beam.length();
        let slope = if length > 1e-9 { beam.rise() / length } else { 0.0 };
        let height = height * slope.hypot(1.0);
        let z = self.level_elevation(element.level) + beam.top_offset;
        match beam.vertical {
            crate::BeamVertical::Top => (z - height, z),
            crate::BeamVertical::Center => (z - height * 0.5, z + height * 0.5),
            crate::BeamVertical::Bottom => (z, z + height),
        }
    }

    /// Allocate a new unique id (shared by all id kinds).
    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn level(&self, id: LevelId) -> Option<&Level> {
        self.levels.iter().find(|l| l.id == id)
    }

    pub fn level_elevation(&self, id: LevelId) -> f64 {
        self.level(id).map_or(0.0, |l| l.elevation)
    }

    pub fn level_name(&self, id: LevelId) -> &str {
        self.level(id).map_or("?", |l| l.name.as_str())
    }

    pub fn is_pinned(&self, id: ElementId) -> bool {
        self.pinned.contains(&id)
    }

    /// The join between two elements as `(cutting, cut)`, if they are joined.
    pub fn join_of(&self, a: ElementId, b: ElementId) -> Option<(ElementId, ElementId)> {
        [(a, b), (b, a)].into_iter().find(|pair| self.joins.contains(pair))
    }

    /// The group an element belongs to.
    pub fn group_of(&self, id: ElementId) -> Option<crate::GroupId> {
        self.groups.iter().find(|(_, g)| g.members.contains(&id)).map(|(gid, _)| *gid)
    }

    /// `ids` plus every member of the groups they belong to.
    pub fn with_groups(&self, ids: impl IntoIterator<Item = ElementId>) -> std::collections::BTreeSet<ElementId> {
        let mut out = std::collections::BTreeSet::new();
        for id in ids {
            match self.group_of(id).and_then(|g| self.groups.get(&g)) {
                Some(group) => out.extend(group.members.iter().copied()),
                None => {
                    out.insert(id);
                }
            }
        }
        out
    }

    /// Elements whose geometry is joined with `id`.
    pub fn joined_with(&self, id: ElementId) -> impl Iterator<Item = ElementId> + '_ {
        self.joins.iter().filter_map(move |&(a, b)| (a == id).then_some(b).or((b == id).then_some(a)))
    }

    pub fn element(&self, id: ElementId) -> Option<&Element> {
        self.elements.get(&id)
    }

    pub fn element_type(&self, id: ElementTypeId) -> Option<&ElementType> {
        self.types.get(&id)
    }

    pub fn type_name(&self, id: ElementTypeId) -> &str {
        self.types.get(&id).map_or("?", |t| t.name.as_str())
    }

    /// Wall thickness for an element's type (0 if not a layered type).
    pub fn thickness_of(&self, element: &Element) -> f64 {
        self.element_type(element.type_id).map_or(0.0, |t| t.thickness())
    }

    pub fn types_of(&self, category: Category) -> impl Iterator<Item = &ElementType> {
        self.types.values().filter(move |t| t.category() == category)
    }

    pub fn default_type(&self, category: Category) -> Option<ElementTypeId> {
        self.types_of(category).next().map(|t| t.id)
    }

    pub fn material_color(&self, name: &str) -> [u8; 3] {
        self.material(name).map_or([200, 200, 200], |m| m.shading_color())
    }

    /// Transparency of material `name` (0 opaque, 1 invisible).
    pub fn material_transparency(&self, name: &str) -> f64 {
        self.material(name).map_or(0.0, |m| m.graphics.transparency.clamp(0.0, 1.0))
    }

    pub fn material(&self, name: &str) -> Option<&Material> {
        self.materials.iter().find(|m| m.name == name)
    }

    /// Types that use the material `name`.
    pub fn material_users(&self, name: &str) -> Vec<ElementTypeId> {
        self.types.values().filter(|t| t.data.uses_material(name)).map(|t| t.id).collect()
    }

    /// Doors and windows hosted by `wall`.
    pub fn hosted_by(&self, wall: ElementId) -> impl Iterator<Item = &Element> {
        self.elements.values().filter(move |e| e.as_hosted().is_some_and(|h| h.host == wall))
    }

    /// The railings that follow stair `host`.
    pub fn railings_of(&self, host: ElementId) -> impl Iterator<Item = &Element> {
        self.elements
            .values()
            .filter(move |e| matches!(&e.kind, ElementKind::Railing(r) if r.stair.is_some_and(|h| h.host == host)))
    }

    /// The reinforcement sets of beam or column `host`.
    pub fn rebar_of(&self, host: ElementId) -> impl Iterator<Item = (&Element, &crate::Rebar)> {
        self.elements.values().filter_map(move |e| match &e.kind {
            ElementKind::Rebar(r) if r.host == host => Some((e, r)),
            _ => None,
        })
    }

    pub fn elements_of(&self, category: Category) -> impl Iterator<Item = &Element> {
        self.elements.values().filter(move |e| e.category() == category)
    }

    pub fn walls(&self) -> impl Iterator<Item = (&Element, &crate::Wall)> {
        self.elements.values().filter_map(|e| match &e.kind {
            ElementKind::Wall(w) => Some((e, w)),
            _ => None,
        })
    }

    pub fn views_of(&self, category: ViewCategory) -> impl Iterator<Item = &View> {
        self.views.values().filter(move |v| v.category() == category)
    }

    pub fn view(&self, id: ViewId) -> Option<&View> {
        self.views.get(&id)
    }

    pub fn filter(&self, id: crate::FilterId) -> Option<&crate::ViewFilter> {
        self.filters.get(&id)
    }

    pub fn template(&self, id: crate::TemplateId) -> Option<&crate::ViewTemplate> {
        self.templates.get(&id)
    }

    /// A fresh mark for a new element: the next number among its category.
    pub fn next_mark(&self, category: Category) -> String {
        let max = self.elements_of(category).filter_map(|e| e.mark.parse::<u32>().ok()).max().unwrap_or(0);
        (max + 1).to_string()
    }

    /// The level directly above `id`, if any.
    pub fn level_above(&self, id: LevelId) -> Option<&Level> {
        let elevation = self.level_elevation(id);
        self.levels.iter().filter(|l| l.elevation > elevation + 1e-9).min_by(|a, b| a.elevation.total_cmp(&b.elevation))
    }

    /// Before `e` moves from its level to `to`: a top attached to a level
    /// moves up or down as many levels as the base (a column from the ground
    /// floor to the first floor pasted on the first floor reaches the second).
    /// When there is no such level, or it is not above the new base, the top
    /// is unconnected at the element's height.
    pub fn move_top_with_base(&self, e: &mut crate::Element, to: LevelId) {
        let from = e.level;
        if from == to {
            return;
        }
        let p = self;
        let span = |(base, top): (f64, f64)| top - base;
        let height = match &e.kind {
            crate::ElementKind::Wall(w) => p.wall_height(e, w),
            crate::ElementKind::Column(c) => span(p.column_span(e, c)),
            crate::ElementKind::Stair(s) => span(p.stair_span(e, s)),
            crate::ElementKind::Ramp(r) => span(p.ramp_span(e, r)),
            _ => return,
        };
        let (top, own_height, base_offset) = match &mut e.kind {
            crate::ElementKind::Wall(w) => (&mut w.top, &mut w.height, w.base_offset),
            crate::ElementKind::Column(c) => (&mut c.top, &mut c.height, c.base_offset),
            crate::ElementKind::Stair(s) => (&mut s.top, &mut s.height, s.base_offset),
            crate::ElementKind::Ramp(r) => (&mut r.top, &mut r.height, r.base_offset),
            _ => return,
        };
        let crate::WallTop::Level { level: top_level, offset } = *top else { return };
        let mut order: Vec<&crate::Level> = p.levels.iter().collect();
        order.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
        let index = |id: LevelId| order.iter().position(|l| l.id == id);
        let shifted = match (index(from), index(top_level), index(to)) {
            (Some(f), Some(t), Some(n)) => (n + t).checked_sub(f).and_then(|i| order.get(i)),
            _ => None,
        };
        let base_z = p.level_elevation(to) + base_offset;
        match shifted {
            Some(l) if l.elevation + offset > base_z + 1e-6 => *top = crate::WallTop::Level { level: l.id, offset },
            _ => {
                *top = crate::WallTop::Unconnected;
                if height.is_finite() && height > 0.0 {
                    *own_height = height;
                }
            }
        }
    }

    pub fn sort_levels(&mut self) {
        self.levels.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
    }
}

/// Portuguese names of the default template, keyed like the translation files.
pub(crate) fn template_default(key: &str) -> &'static str {
    match key {
        "tpl-project-name" => "Novo projeto",
        "tpl-level-ground" => "Térreo",
        "tpl-level-first" => "Pavimento 1",
        "tpl-type-masonry-15" => "Alvenaria 15 cm",
        "tpl-type-masonry-25" => "Alvenaria 25 cm",
        "tpl-type-drywall-10" => "Divisória gesso 10 cm",
        "tpl-type-door-80" => "Porta 80x210",
        "tpl-type-door-90" => "Porta 90x210",
        "tpl-type-window-120" => "Janela 120x120",
        "tpl-type-window-150" => "Janela 150x100",
        "tpl-comp-door" => "Porta de giro",
        "tpl-conn-sewer" => "Esgoto",
        "tpl-conn-cold-water" => "Água fria",
        "tpl-conn-hot-water" => "Água quente",
        "tpl-comp-window" => "Janela",
        "tpl-param-sill" => "Peitoril",
        "tpl-piece-frame" => "Batente",
        "tpl-piece-leaf" => "Folha",
        "tpl-piece-sash" => "Caixilho",
        "tpl-piece-glass" => "Vidro",
        "tpl-piece-sill-board" => "Pingadeira",
        "tpl-type-slab-12" => "Laje 12 cm",
        "tpl-type-ceiling-gypsum" => "Forro de gesso",
        "tpl-type-roof-tile" => "Telhado cerâmico",
        "tpl-mat-roof-tile" => "Telha cerâmica",
        "tpl-mat-masonry" => "Alvenaria",
        "tpl-mat-plaster" => "Reboco",
        "tpl-mat-concrete" => "Concreto",
        "tpl-mat-wood" => "Madeira",
        "tpl-mat-glass" => "Vidro",
        "tpl-mat-gypsum" => "Gesso",
        "tpl-mat-screed" => "Contrapiso",
        "tpl-elev-north" => "Norte",
        "tpl-elev-south" => "Sul",
        "tpl-elev-east" => "Leste",
        "tpl-elev-west" => "Oeste",
        "tpl-legend" => "Legenda de tipos",
        "tpl-schedule-doors" => "Tabela de portas",
        "tpl-schedule-windows" => "Tabela de janelas",
        "tpl-schedule-walls" => "Tabela de paredes",
        "tpl-sheet-plans" => "Plantas",
        "tpl-type-room" => "Ambiente",
        "tpl-type-column-rect" => "Pilar 20 x 20 cm",
        "tpl-type-column-round" => "Pilar circular 30 cm",
        "tpl-type-beam" => "Viga 15 x 40 cm",
        "tpl-type-stair" => "Escada 1,00 m",
        "tpl-type-railing" => "Guarda-corpo 1,10 m",
        "tpl-type-grid" => "Eixo 8 mm",
        "tpl-type-dimension" => "Cota linear 2,5 mm",
        "tpl-type-text" => "Texto 2,5 mm",
        "tpl-type-text-large" => "Texto 3,5 mm",
        "tpl-schedule-rooms" => "Tabela de ambientes",
        "tpl-room-name" => "Ambiente",
        "tpl-mat-fabric" => "Tecido",
        "tpl-mat-porcelain" => "Louça",
        "tpl-mat-steel" => "Aço",
        "tpl-type-column-steel-h" => "Pilar metálico H 200 x 200",
        "tpl-type-column-steel-tube" => "Pilar metálico tubo 150 x 150 x 6",
        "tpl-type-beam-steel-i" => "Viga metálica I 300 x 150",
        "tpl-type-footing-pad" => "Sapata 100 x 100 x 40 cm",
        "tpl-type-footing-strip" => "Viga baldrame 20 x 40 cm",
        "tpl-type-footing-slab" => "Radier 30 cm",
        "tpl-type-rebar-5" => "Barra Ø 5,0 mm",
        "tpl-type-rebar-8" => "Barra Ø 8,0 mm",
        "tpl-type-rebar-10" => "Barra Ø 10,0 mm",
        "tpl-type-rebar-12" => "Barra Ø 12,5 mm",
        "tpl-type-rebar-16" => "Barra Ø 16,0 mm",
        "tpl-param-width" => "Largura",
        "tpl-param-depth" => "Profundidade",
        "tpl-param-height" => "Altura",
        "tpl-param-seat-height" => "AlturaAssento",
        "tpl-part-top" => "Tampo",
        "tpl-part-leg" => "Perna",
        "tpl-part-seat" => "Assento",
        "tpl-part-back" => "Encosto",
        "tpl-part-arm" => "Braço",
        "tpl-part-mattress" => "Colchão",
        "tpl-part-headboard" => "Cabeceira",
        "tpl-part-bowl" => "Bacia",
        "tpl-part-tank" => "Caixa acoplada",
        "tpl-part-basin" => "Cuba",
        "tpl-part-pedestal" => "Coluna",
        "tpl-part-body" => "Corpo",
        "tpl-comp-table" => "Mesa",
        "tpl-comp-chair" => "Cadeira",
        "tpl-comp-bed" => "Cama",
        "tpl-comp-sofa" => "Sofá",
        "tpl-comp-toilet" => "Bacia sanitária",
        "tpl-comp-washbasin" => "Lavatório",
        "tpl-comp-wardrobe" => "Armário",
        "tpl-type-table" => "Mesa 120 x 80",
        "tpl-type-chair" => "Cadeira simples",
        "tpl-type-bed" => "Cama casal 140 x 190",
        "tpl-type-sofa" => "Sofá 3 lugares",
        "tpl-type-toilet" => "Bacia com caixa acoplada",
        "tpl-type-washbasin" => "Lavatório de coluna",
        "tpl-type-wardrobe" => "Armário 160 cm",
        "tpl-mat-pvc" => "PVC",
        "tpl-type-pipe-20" => "Tubo PVC soldável 20 mm",
        "tpl-type-pipe-25" => "Tubo PVC soldável 25 mm",
        "tpl-type-pipe-32" => "Tubo PVC soldável 32 mm",
        "tpl-type-pipe-sewer-40" => "Tubo PVC esgoto 40 mm",
        "tpl-type-pipe-sewer-50" => "Tubo PVC esgoto 50 mm",
        "tpl-type-pipe-sewer-75" => "Tubo PVC esgoto 75 mm",
        "tpl-type-pipe-sewer-100" => "Tubo PVC esgoto 100 mm",
        "tpl-type-conduit-20" => "Eletroduto flexível 20 mm",
        "tpl-type-conduit-25" => "Eletroduto flexível 25 mm",
        "tpl-type-conduit-32" => "Eletroduto flexível 32 mm",
        "tpl-sys-cold-water" => "Água fria",
        "tpl-sys-cold-water-code" => "AF",
        "tpl-sys-hot-water" => "Água quente",
        "tpl-sys-hot-water-code" => "AQ",
        "tpl-sys-sanitary" => "Esgoto sanitário",
        "tpl-sys-sanitary-code" => "ES",
        "tpl-sys-vent" => "Ventilação",
        "tpl-sys-vent-code" => "V",
        "tpl-sys-rainwater" => "Águas pluviais",
        "tpl-sys-rainwater-code" => "AP",
        "tpl-part-head" => "Crivo",
        "tpl-part-grate" => "Grelha",
        "tpl-comp-shower" => "Chuveiro",
        "tpl-comp-kitchen-sink" => "Pia de cozinha",
        "tpl-comp-floor-drain" => "Ralo sifonado",
        "tpl-comp-outlet" => "Tomada de uso geral",
        "tpl-comp-dedicated-outlet" => "Tomada de uso específico",
        "tpl-comp-switch" => "Interruptor",
        "tpl-comp-wall-light" => "Arandela",
        "tpl-comp-panel" => "Quadro de distribuição",
        "tpl-comp-ceiling-light" => "Ponto de luz no teto",
        "tpl-type-shower" => "Chuveiro de parede",
        "tpl-type-kitchen-sink" => "Pia inox 120 cm",
        "tpl-type-floor-drain" => "Ralo sifonado 150 mm",
        "tpl-type-outlet" => "Tomada 2P+T 10 A",
        "tpl-type-dedicated-outlet" => "Tomada 2P+T 20 A",
        "tpl-type-switch" => "Interruptor simples",
        "tpl-type-wall-light" => "Arandela 60 VA",
        "tpl-type-panel" => "Quadro de distribuição de embutir",
        "tpl-type-ceiling-light" => "Luminária de teto 100 VA",
        "tpl-phase-existing" => "Existente",
        "tpl-phase-new" => "Construção nova",
        "tpl-phase-filter-all" => "Mostrar tudo",
        "tpl-phase-filter-complete" => "Mostrar completo",
        "tpl-phase-filter-new" => "Mostrar novo",
        "tpl-phase-filter-previous-demo" => "Mostrar anterior + demolição",
        "tpl-phase-filter-previous-new" => "Mostrar anterior + novo",
        "tpl-phase-filter-previous" => "Mostrar fase anterior",
        "tpl-phase-filter-demo-new" => "Mostrar demolição + novo",
        _ => "?",
    }
}

/// Whether elements of `category` have a body whose geometry can be joined.
pub fn can_join_geometry(category: crate::Category) -> bool {
    use crate::Category::*;
    matches!(category, Wall | Floor | Ceiling | Roof | Column | Beam | Stair | Railing | Component)
}

/// How strongly an element cuts the elements it is joined with: of two
/// joined elements, the higher cuts the lower. Structural columns and beams
/// cut hosts (walls, roofs, ceilings, floors); floors, ceilings and roofs cut
/// walls; walls cut architectural columns.
pub fn join_priority(element: &crate::Element) -> u8 {
    use crate::ElementKind::*;
    match &element.kind {
        Column(c) if c.structural => 7,
        Beam(_) => 6,
        Floor(_) => 5,
        Roof(_) => 4,
        Ceiling(_) => 3,
        Wall(_) => 2,
        Column(_) => 1,
        _ => 0,
    }
}
