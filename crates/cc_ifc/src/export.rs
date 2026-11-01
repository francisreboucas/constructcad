//! Mapping of the ConstructCAD model to IFC4 entities.

use crate::args;
use crate::guid::{encode, guid};
use crate::schema::{ElementClass, TypeLayout, element_class};
use crate::step::{Header, Id, StepWriter, Value, opt_str, reals};
use cc_core::{Element, ElementId, ElementKind, ElementTypeId, LevelId, Project, RoofShape, TypeData, Uid};
use cc_geom::{FRAME_COLOR, ModelGeometry, WallGeom, opening_parts, polygon};
use glam::{DVec2, DVec3};
use std::collections::{BTreeMap, HashMap};

/// Openings extend this far beyond each wall face so the boolean cut is clean.
const OPENING_CLEARANCE: f64 = 0.05;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExportOptions {
    /// `FILE_NAME` time stamp (ISO 8601). The current UTC time when `None`.
    pub time_stamp: Option<String>,
    /// `FILE_NAME` name, usually the target file name.
    pub file_name: String,
}

/// Serialize `project` as an IFC4 STEP physical file.
pub fn export(project: &Project, options: &ExportOptions) -> String {
    let geom = ModelGeometry::build(project);
    let mut x = Exporter::new(project, &geom);
    x.write_all();
    let generator = format!("ConstructCAD {}", env!("CARGO_PKG_VERSION"));
    let header = Header {
        description: vec!["ViewDefinition [DesignTransferView_V1.0]".into()],
        name: options.file_name.clone(),
        time_stamp: options.time_stamp.clone().unwrap_or_else(now_utc),
        author: vec![project.info.author.clone()],
        organization: vec![String::new()],
        preprocessor_version: generator.clone(),
        originating_system: generator,
        authorization: String::new(),
        schema: "IFC4".into(),
    };
    x.w.finish(&header)
}

/// Export `project` to an `.ifc` file at `path`.
pub fn export_to_file(project: &Project, path: &std::path::Path) -> std::io::Result<()> {
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let text = export(project, &ExportOptions { time_stamp: None, file_name });
    // Write to a temp file first so a failed export never truncates an existing file.
    let tmp = path.with_extension("ifc.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// A rigid placement: origin plus local X and Z axes (Y = Z × X).
#[derive(Debug, Clone, Copy)]
struct Frame {
    origin: DVec3,
    x: DVec3,
    z: DVec3,
}

impl Frame {
    fn at(origin: DVec3) -> Self {
        Self { origin, x: DVec3::X, z: DVec3::Z }
    }

    /// Placement with Z up and X along `dir` in plan.
    fn horizontal(origin: DVec3, dir: DVec2) -> Self {
        Self { origin, x: DVec3::new(dir.x, dir.y, 0.0), z: DVec3::Z }
    }
}

/// A plan point in the placement coordinates of a wall (X along the wall
/// at its start, Y to its left).
fn wall_local(g: &WallGeom, p: DVec2) -> DVec2 {
    let d = p - g.origin;
    DVec2::new(d.dot(g.dir), d.dot(g.normal))
}

struct Exporter<'a> {
    w: StepWriter,
    project: &'a Project,
    geom: &'a ModelGeometry,
    seed: String,
    // Shared resources.
    origin3: Id,
    origin2: Id,
    dir_z: Id,
    dir_x: Id,
    identity: Id,
    body_ctx: Id,
    axis_ctx: Id,
    // Caches.
    styles: HashMap<String, Id>,
    materials: HashMap<String, Id>,
    layer_sets: BTreeMap<ElementTypeId, Id>,
    types: BTreeMap<ElementTypeId, Id>,
    layer_usages: BTreeMap<ElementTypeId, Id>,
    products: BTreeMap<ElementId, Id>,
    // Relationship collectors, emitted at the end.
    typed: BTreeMap<ElementTypeId, Vec<Id>>,
    /// Roof framing: an `IfcMemberType` per roof type and member name, with its members.
    member_types: BTreeMap<(ElementTypeId, &'static str), (Id, Vec<Id>)>,
    contained: BTreeMap<LevelId, Vec<Id>>,
    usage_objects: BTreeMap<Id, Vec<Id>>,
    /// Storey and its placement per level.
    storeys: BTreeMap<LevelId, (Id, Id)>,
    /// Elements whose `Status` went into a common property set of their own.
    status_written: std::collections::BTreeSet<ElementId>,
}

impl<'a> Exporter<'a> {
    fn new(project: &'a Project, geom: &'a ModelGeometry) -> Self {
        let mut w = StepWriter::new();
        let origin3 = w.add("IFCCARTESIANPOINT", args![reals(&[0.0, 0.0, 0.0])]);
        let origin2 = w.add("IFCCARTESIANPOINT", args![reals(&[0.0, 0.0])]);
        let dir_z = w.add("IFCDIRECTION", args![reals(&[0.0, 0.0, 1.0])]);
        let dir_x = w.add("IFCDIRECTION", args![reals(&[1.0, 0.0, 0.0])]);
        let identity = w.add("IFCAXIS2PLACEMENT3D", args![origin3, Value::Null, Value::Null]);
        let info = &project.info;
        Self {
            w,
            project,
            geom,
            // Ids of derived objects (relationships, openings…) hash the project id with a tag.
            seed: if info.uid.is_nil() { format!("{}\u{1f}{}", info.number, info.name) } else { info.uid.to_string() },
            origin3,
            origin2,
            dir_z,
            dir_x,
            identity,
            body_ctx: Id(0),
            axis_ctx: Id(0),
            styles: HashMap::new(),
            materials: HashMap::new(),
            layer_sets: BTreeMap::new(),
            types: BTreeMap::new(),
            layer_usages: BTreeMap::new(),
            products: BTreeMap::new(),
            usage_objects: BTreeMap::new(),
            typed: BTreeMap::new(),
            member_types: BTreeMap::new(),
            contained: BTreeMap::new(),
            storeys: BTreeMap::new(),
            status_written: Default::default(),
        }
    }

    fn guid(&self, tag: &str) -> Value {
        Value::Str(guid(&self.seed, tag))
    }

    /// GlobalId of an object with a persistent id, falling back to a derived one.
    fn guid_of(&self, uid: Uid, tag: &str) -> Value {
        if uid.is_nil() { self.guid(tag) } else { Value::Str(encode(uid.0)) }
    }

    /// `Pset_*` property set with single values, attached to `objects`.
    fn property_set(&mut self, tag: &str, name: &str, objects: Vec<Id>, props: Vec<(&str, Value)>) {
        self.property_set_status(tag, name, objects, props, None);
    }

    /// [`Self::property_set`] with the enumerated `Status` of a construction
    /// phase (`NEW`, `EXISTING`, `DEMOLISH`, `TEMPORARY`).
    fn property_set_status(
        &mut self,
        tag: &str,
        name: &str,
        objects: Vec<Id>,
        props: Vec<(&str, Value)>,
        status: Option<cc_core::PhaseStatus>,
    ) {
        if (props.is_empty() && status.is_none()) || objects.is_empty() {
            return;
        }
        let n = Value::Null;
        let mut props: Vec<Id> = props
            .into_iter()
            .map(|(name, value)| self.w.add("IFCPROPERTYSINGLEVALUE", args![name, n.clone(), value, n.clone()]))
            .collect();
        if let Some(st) = status {
            let values = vec![Value::Typed("IFCLABEL", Box::new(st.ifc().into()))];
            props.push(self.w.add("IFCPROPERTYENUMERATEDVALUE", args!["Status", n.clone(), values, n.clone()]));
        }
        let gid = self.guid(&format!("pset-{tag}"));
        let set = self.w.add("IFCPROPERTYSET", args![gid, n.clone(), name, n.clone(), props]);
        let gid = self.guid(&format!("pset-rel-{tag}"));
        self.w.add("IFCRELDEFINESBYPROPERTIES", args![gid, n.clone(), n.clone(), n, objects, set]);
    }

    fn write_all(&mut self) {
        let project_id = self.write_project();
        self.write_spatial_structure(project_id);
        let walls: Vec<_> = self.project.walls().map(|(e, _)| e.id).collect();
        for id in walls {
            self.write_wall(id);
        }
        let others: Vec<_> = self
            .project
            .elements
            .values()
            .filter(|e| matches!(e.kind, ElementKind::Floor(_) | ElementKind::Ceiling(_) | ElementKind::Roof(_)))
            .map(|e| e.id)
            .collect();
        for id in others {
            self.write_sketched(id);
        }
        self.write_structure();
        self.write_rebar();
        self.write_components();
        self.write_segments();
        let fittings = self.write_fittings();
        self.write_systems(&fittings);
        self.write_spaces();
        self.write_phasing(project_id);
        self.write_relationships();
    }

    /// Construction phases: the project's phases in order (a set on the
    /// project), and for each element its `Status` relative to the last
    /// phase in its common property set (building elements) and the names
    /// of its phases in [`PHASING_PSET`]. Elements sharing values share
    /// the sets.
    fn write_phasing(&mut self, project_id: Id) {
        let project = self.project;
        if project.phases.is_empty() {
            return;
        }
        let names: Vec<(String, Value)> = project
            .phases
            .iter()
            .enumerate()
            .map(|(i, ph)| ((i + 1).to_string(), Value::Typed("IFCLABEL", Box::new(ph.name.as_str().into()))))
            .collect();
        let props = names.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        self.property_set("phases", PHASES_PSET, vec![project_id], props);
        let mut common: BTreeMap<(&'static str, cc_core::PhaseStatus), Vec<Id>> = BTreeMap::new();
        let mut phasing: BTreeMap<(usize, Option<usize>), Vec<Id>> = BTreeMap::new();
        for (&id, &product) in &self.products {
            let Some(e) = project.element(id) else { continue };
            let Some(status) = ifc_status(project, e) else { continue };
            if !self.status_written.contains(&id)
                && let Some(set) = common_pset(project, e)
            {
                common.entry((set, status)).or_default().push(product);
            }
            phasing.entry((project.created_index(e), project.demolished_index(e))).or_default().push(product);
        }
        for ((set, status), objects) in common {
            self.property_set_status(&format!("status-{set}-{}", status.ifc()), set, objects, Vec::new(), Some(status));
        }
        let label =
            |i: usize| project.phases.get(i).map(|ph| Value::Typed("IFCLABEL", Box::new(ph.name.as_str().into())));
        for ((created, demolished), objects) in phasing {
            let mut props = Vec::new();
            props.extend(label(created).map(|v| ("PhaseCreated", v)));
            props.extend(demolished.and_then(label).map(|v| ("PhaseDemolished", v)));
            let tag = format!("phasing-{created}-{}", demolished.map_or(-1, |d| d as i64));
            self.property_set(&tag, PHASING_PSET, objects, props);
        }
    }

    /// Enclosed rooms as `IfcSpace`s aggregated into their storey.
    fn write_spaces(&mut self) {
        let project = self.project;
        let geom = self.geom;
        let mut by_storey: BTreeMap<LevelId, Vec<Id>> = BTreeMap::new();
        for e in project.elements.values() {
            let ElementKind::Room(room) = &e.kind else { continue };
            let Some(boundary) = geom.rooms.get(&e.id) else { continue };
            let Some(storey) = self.storey_placement(e.level) else { continue };
            let placement = self.local_placement(Some(storey), Frame::at(DVec3::ZERO));
            let profile = polygon::ccw(boundary);
            let body = self.extrude_polygon(&profile, self.identity, self.dir_z, room.height.max(0.01));
            let shape = self.shape(vec![body], None);
            let gid = self.guid_of(e.uid, &format!("element-{}", e.id));
            let n = Value::Null;
            let space = self.w.add(
                "IFCSPACE",
                args![
                    gid,
                    n.clone(),
                    opt_str(&room.number),
                    opt_str(&e.comments),
                    n.clone(),
                    placement,
                    shape,
                    opt_str(&room.name),
                    Value::Enum("ELEMENT"),
                    Value::Enum("SPACE"),
                    n
                ],
            );
            self.products.insert(e.id, space);
            by_storey.entry(e.level).or_default().push(space);
            self.quantities(
                e,
                "Qto_SpaceBaseQuantities",
                &[
                    ("Height", Quantity::Length(room.height)),
                    ("NetPerimeter", Quantity::Length(polygon::perimeter(boundary))),
                    ("NetFloorArea", Quantity::Area(polygon::area(boundary))),
                    ("NetVolume", Quantity::Volume(polygon::area(boundary) * room.height)),
                ],
            );
        }
        for (level, spaces) in by_storey {
            let Some(&(storey, _)) = self.storeys.get(&level) else { continue };
            self.aggregate(&format!("agg-spaces-{level}"), storey, spaces);
        }
    }

    // --- Project, units, contexts, spatial structure ---

    fn write_project(&mut self) -> Id {
        let w = &mut self.w;
        let units: Vec<Id> = [
            ("LENGTHUNIT", "METRE"),
            ("AREAUNIT", "SQUARE_METRE"),
            ("VOLUMEUNIT", "CUBIC_METRE"),
            ("PLANEANGLEUNIT", "RADIAN"),
        ]
        .into_iter()
        .map(|(kind, name)| {
            w.add("IFCSIUNIT", args![Value::Derived, Value::Enum(kind), Value::Null, Value::Enum(name)])
        })
        .collect();
        let units = w.add("IFCUNITASSIGNMENT", args![units]);
        let wcs = w.add("IFCAXIS2PLACEMENT3D", args![self.origin3, self.dir_z, self.dir_x]);
        let north = w.add("IFCDIRECTION", args![reals(&[0.0, 1.0])]);
        let model = w.add("IFCGEOMETRICREPRESENTATIONCONTEXT", args![Value::Null, "Model", 3i64, 1e-5, wcs, north]);
        let sub = |w: &mut StepWriter, id: &str, view: &'static str| {
            let d = Value::Derived;
            let args = args![
                id,
                "Model",
                d.clone(),
                d.clone(),
                d.clone(),
                d,
                model,
                Value::Null,
                Value::Enum(view),
                Value::Null
            ];
            w.add("IFCGEOMETRICREPRESENTATIONSUBCONTEXT", args)
        };
        self.body_ctx = sub(w, "Body", "MODEL_VIEW");
        self.axis_ctx = sub(w, "Axis", "GRAPH_VIEW");
        let info = &self.project.info;
        let name = if info.name.is_empty() { "ConstructCAD" } else { info.name.as_str() };
        let gid = self.guid_of(self.project.info.uid, "project");
        self.w.add(
            "IFCPROJECT",
            args![
                gid,
                Value::Null,
                name,
                Value::Null,
                Value::Null,
                opt_str(&info.number),
                Value::Null,
                vec![model],
                units
            ],
        )
    }

    fn write_spatial_structure(&mut self, project_id: Id) {
        let site_placement = self.local_placement(None, Frame::at(DVec3::ZERO));
        let gid = self.guid("site");
        let n = Value::Null;
        let site = self.w.add(
            "IFCSITE",
            args![
                gid,
                n.clone(),
                n.clone(),
                n.clone(),
                n.clone(),
                site_placement,
                n.clone(),
                n.clone(),
                Value::Enum("ELEMENT"),
                n.clone(),
                n.clone(),
                n.clone(),
                n.clone(),
                n.clone()
            ],
        );
        let address = (!self.project.info.address.is_empty()).then(|| {
            let lines = vec![self.project.info.address.clone()];
            let args = args![
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                lines,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null
            ];
            self.w.add("IFCPOSTALADDRESS", args)
        });
        let building_placement = self.local_placement(Some(site_placement), Frame::at(DVec3::ZERO));
        let gid = self.guid("building");
        let name = opt_str(&self.project.info.name);
        let building = self.w.add(
            "IFCBUILDING",
            args![
                gid,
                n.clone(),
                name,
                n.clone(),
                n.clone(),
                building_placement,
                n.clone(),
                n.clone(),
                Value::Enum("ELEMENT"),
                n.clone(),
                n.clone(),
                address
            ],
        );
        let mut storeys = Vec::new();
        for level in &self.project.levels {
            let placement =
                self.local_placement(Some(building_placement), Frame::at(DVec3::new(0.0, 0.0, level.elevation)));
            let gid = self.guid(&format!("storey-{}", level.id));
            let storey = self.w.add(
                "IFCBUILDINGSTOREY",
                args![
                    gid,
                    n.clone(),
                    level.name.as_str(),
                    n.clone(),
                    n.clone(),
                    placement,
                    n.clone(),
                    n.clone(),
                    Value::Enum("ELEMENT"),
                    level.elevation
                ],
            );
            self.storeys.insert(level.id, (storey, placement));
            storeys.push(storey);
        }
        self.aggregate("agg-project", project_id, vec![site]);
        self.aggregate("agg-site", site, vec![building]);
        if !storeys.is_empty() {
            self.aggregate("agg-building", building, storeys);
        }
    }

    fn aggregate(&mut self, tag: &str, whole: Id, parts: Vec<Id>) {
        let gid = self.guid(tag);
        self.w.add("IFCRELAGGREGATES", args![gid, Value::Null, Value::Null, Value::Null, whole, parts]);
    }

    // --- Geometry helpers ---

    fn point3(&mut self, p: DVec3) -> Id {
        self.w.add("IFCCARTESIANPOINT", args![reals(&[p.x, p.y, p.z])])
    }

    fn point2(&mut self, p: DVec2) -> Id {
        self.w.add("IFCCARTESIANPOINT", args![reals(&[p.x, p.y])])
    }

    fn direction(&mut self, d: DVec3) -> Id {
        if d == DVec3::Z {
            return self.dir_z;
        }
        if d == DVec3::X {
            return self.dir_x;
        }
        self.w.add("IFCDIRECTION", args![reals(&[d.x, d.y, d.z])])
    }

    fn placement3(&mut self, f: Frame) -> Id {
        if f.origin == DVec3::ZERO && f.x == DVec3::X && f.z == DVec3::Z {
            return self.identity;
        }
        let location = if f.origin == DVec3::ZERO { self.origin3 } else { self.point3(f.origin) };
        if f.x == DVec3::X && f.z == DVec3::Z {
            return self.w.add("IFCAXIS2PLACEMENT3D", args![location, Value::Null, Value::Null]);
        }
        let z = self.direction(f.z);
        let x = self.direction(f.x);
        self.w.add("IFCAXIS2PLACEMENT3D", args![location, z, x])
    }

    fn local_placement(&mut self, relative_to: Option<Id>, f: Frame) -> Id {
        let rel = self.placement3(f);
        self.w.add("IFCLOCALPLACEMENT", args![relative_to, rel])
    }

    /// Closed polyline through `pts` (the first point is repeated at the end).
    fn polyline(&mut self, pts: &[DVec2]) -> Id {
        let mut ids: Vec<Id> = pts.iter().map(|&p| self.point2(p)).collect();
        ids.push(ids[0]);
        self.w.add("IFCPOLYLINE", args![ids])
    }

    /// Extrusion of a closed polygon along `dir` (in `position` coordinates).
    fn extrude_polygon(&mut self, pts: &[DVec2], position: Id, dir: Id, depth: f64) -> Id {
        self.extrude_polygon_with_voids(pts, &[], position, dir, depth)
    }

    /// A polygon with `voids` (openings) extruded by `depth` along `dir`.
    fn extrude_polygon_with_voids(
        &mut self,
        pts: &[DVec2],
        voids: &[Vec<DVec2>],
        position: Id,
        dir: Id,
        depth: f64,
    ) -> Id {
        let curve = self.polyline(pts);
        let profile = if voids.is_empty() {
            self.w.add("IFCARBITRARYCLOSEDPROFILEDEF", args![Value::Enum("AREA"), Value::Null, curve])
        } else {
            let inner: Vec<Id> = voids.iter().map(|v| self.polyline(v)).collect();
            self.w.add("IFCARBITRARYPROFILEDEFWITHVOIDS", args![Value::Enum("AREA"), Value::Null, curve, inner])
        };
        self.w.add("IFCEXTRUDEDAREASOLID", args![profile, position, dir, depth])
    }

    /// Axis-aligned box between `min` and `max`, as a rectangle extruded upwards.
    fn extrude_box(&mut self, min: DVec3, max: DVec3) -> Id {
        let center = DVec2::new(min.x + max.x, min.y + max.y) * 0.5;
        let location = self.point2(center);
        let position2 = self.w.add("IFCAXIS2PLACEMENT2D", args![location, Value::Null]);
        let (dx, dy) = (max.x - min.x, max.y - min.y);
        let profile = self.w.add("IFCRECTANGLEPROFILEDEF", args![Value::Enum("AREA"), Value::Null, position2, dx, dy]);
        let position = self.placement3(Frame::at(DVec3::new(0.0, 0.0, min.z)));
        self.w.add("IFCEXTRUDEDAREASOLID", args![profile, position, self.dir_z, max.z - min.z])
    }

    /// Surface style named after `name` with the given shading color.
    fn style(&mut self, name: &str, color: [u8; 3], transparency: f64) -> Id {
        if let Some(&id) = self.styles.get(name) {
            return id;
        }
        let [r, g, b] = color.map(|c| c as f64 / 255.0);
        let rgb = self.w.add("IFCCOLOURRGB", args![name, r, g, b]);
        let shading = self.w.add("IFCSURFACESTYLESHADING", args![rgb, transparency.clamp(0.0, 1.0)]);
        let style = self.w.add("IFCSURFACESTYLE", args![name, Value::Enum("BOTH"), vec![shading]]);
        self.styles.insert(name.to_string(), style);
        style
    }

    fn styled(&mut self, item: Id, style: Id) {
        self.w.add("IFCSTYLEDITEM", args![item, vec![style], Value::Null]);
    }

    fn material_style(&mut self, material: &str) -> Id {
        let color = self.project.material_color(material);
        let transparency = self.project.material_transparency(material);
        self.style(material, color, transparency)
    }

    /// Product shape from body items and an optional 2D axis curve.
    fn shape(&mut self, body: Vec<Id>, axis: Option<Id>) -> Id {
        self.shape_of_type(body, axis, "SweptSolid")
    }

    fn shape_of_type(&mut self, body: Vec<Id>, axis: Option<Id>, kind: &str) -> Id {
        let mut reps = Vec::new();
        if let Some(curve) = axis {
            reps.push(self.w.add("IFCSHAPEREPRESENTATION", args![self.axis_ctx, "Axis", "Curve2D", vec![curve]]));
        }
        reps.push(self.w.add("IFCSHAPEREPRESENTATION", args![self.body_ctx, "Body", kind, body]));
        self.w.add("IFCPRODUCTDEFINITIONSHAPE", args![Value::Null, Value::Null, reps])
    }

    /// A box in a horizontal frame, as a rectangle extruded upwards; `z0` is the
    /// elevation of the element placement.
    fn oriented_box(&mut self, part: &cc_geom::BoxPart, z0: f64) -> Id {
        let (u, v, z) = (part.u_range, part.v_range, part.z_range);
        let center = self.point2(DVec2::new((u.0 + u.1) / 2.0, (v.0 + v.1) / 2.0));
        let position2 = self.w.add("IFCAXIS2PLACEMENT2D", args![center, Value::Null]);
        let profile = self
            .w
            .add("IFCRECTANGLEPROFILEDEF", args![Value::Enum("AREA"), Value::Null, position2, u.1 - u.0, v.1 - v.0]);
        let frame = Frame::horizontal(DVec3::new(part.origin.x, part.origin.y, z.0 - z0), part.u);
        let position = self.placement3(frame);
        self.w.add("IFCEXTRUDEDAREASOLID", args![profile, position, self.dir_z, z.1 - z.0])
    }

    /// The profile of a column or beam section centered at the origin: the
    /// parametric IFC profile when the shape has one, its solid box when its
    /// walls fill it.
    fn section_profile(&mut self, section: cc_core::ColumnSection) -> Id {
        use cc_core::{ColumnSection as C, SectionShape as S};
        let n = Value::Null;
        let position = self.w.add("IFCAXIS2PLACEMENT2D", args![self.origin2, Value::Null]);
        let area = Value::Enum("AREA");
        match section {
            C::Round { diameter } => self.w.add("IFCCIRCLEPROFILEDEF", args![area, n, position, diameter / 2.0]),
            C::Pipe { diameter, wall } if 2.0 * wall < diameter => {
                self.w.add("IFCCIRCLEHOLLOWPROFILEDEF", args![area, n, position, diameter / 2.0, wall])
            }
            C::Pipe { diameter, .. } => self.w.add("IFCCIRCLEPROFILEDEF", args![area, n, position, diameter / 2.0]),
            C::Rectangular { width, depth } | C::Profile { width, depth, shape: S::Solid } => {
                self.w.add("IFCRECTANGLEPROFILEDEF", args![area, n, position, width, depth])
            }
            C::Profile { width, depth, shape } => match shape.fitted(width, depth) {
                S::I { web, flange } if web < width && 2.0 * flange < depth => self.w.add(
                    "IFCISHAPEPROFILEDEF",
                    args![area, n.clone(), position, width, depth, web, flange, n.clone(), n.clone(), n],
                ),
                S::Channel { web, flange } if web < width && 2.0 * flange < depth => self.w.add(
                    "IFCUSHAPEPROFILEDEF",
                    args![area, n.clone(), position, depth, width, web, flange, n.clone(), n.clone(), n],
                ),
                S::Angle { thickness } if thickness < width.min(depth) => self.w.add(
                    "IFCLSHAPEPROFILEDEF",
                    args![area, n.clone(), position, depth, width, thickness, n.clone(), n.clone(), n],
                ),
                S::Tube { wall } if 2.0 * wall < width.min(depth) => self.w.add(
                    "IFCRECTANGLEHOLLOWPROFILEDEF",
                    args![area, n.clone(), position, width, depth, wall, n.clone(), n],
                ),
                // The walls fill the box.
                _ => self.w.add("IFCRECTANGLEPROFILEDEF", args![area, n, position, width, depth]),
            },
        }
    }

    /// Whether joined elements cut part of the element's body away (walls,
    /// which keep their bodies and deduct the shared volume, aside).
    fn cut_by_join(&self, id: ElementId) -> bool {
        !self.geom.walls.contains_key(&id) && self.project.joins.iter().any(|&(_, cut)| cut == id)
    }

    /// The element's body as joins left it: one faceted B-rep per solid,
    /// points relative to `z0`.
    fn cut_body(&mut self, id: ElementId, z0: f64) -> Vec<Id> {
        let geom = self.geom;
        geom.body_of(id).filter(|s| !s.faces.is_empty()).map(|s| self.faceted_brep(&s.faces, z0)).collect()
    }

    /// One faceted B-rep per closed piece of an element.
    fn pieces(&mut self, e: &cc_core::Element, pieces: cc_geom::Pieces, z0: f64) -> Vec<Id> {
        let mut items = Vec::new();
        for piece in pieces {
            let mut s = cc_geom::Solid::new(e.id, e.category(), [0, 0, 0]);
            s.add_polygons(piece);
            if !s.faces.is_empty() {
                items.push(self.faceted_brep(&s.faces, z0));
            }
        }
        items
    }

    /// Closed shell of planar faces (points relative to `z0`).
    fn faceted_brep(&mut self, faces: &[cc_geom::Face], z0: f64) -> Id {
        let mut points: HashMap<(i64, i64, i64), Id> = HashMap::new();
        let mut ifc_faces = Vec::new();
        for face in faces {
            let ids: Vec<Id> = face
                .verts
                .iter()
                .map(|p| {
                    let p = DVec3::new(p.x, p.y, p.z - z0);
                    let key = ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64, (p.z * 1e6).round() as i64);
                    *points
                        .entry(key)
                        .or_insert_with(|| self.w.add("IFCCARTESIANPOINT", args![reals(&[p.x, p.y, p.z])]))
                })
                .collect();
            let lp = self.w.add("IFCPOLYLOOP", args![ids]);
            let bound = self.w.add("IFCFACEOUTERBOUND", args![lp, true]);
            ifc_faces.push(self.w.add("IFCFACE", args![vec![bound]]));
        }
        let shell = self.w.add("IFCCLOSEDSHELL", args![ifc_faces]);
        self.w.add("IFCFACETEDBREP", args![shell])
    }

    /// An imported property or quantity set (`None` when nothing in it can be written).
    fn imported_set(&mut self, tag: &str, set: &cc_core::properties::PropertySet) -> Option<Id> {
        let mut seen = std::collections::HashSet::new();
        let mut items = Vec::new();
        for p in set.properties.iter().filter(|p| seen.insert(p.name.as_str())) {
            let item = if set.quantities { self.imported_quantity(p) } else { self.imported_property(p, 0) };
            items.extend(item);
        }
        if items.is_empty() {
            return None;
        }
        let gid = self.guid(&format!("pset-{tag}"));
        let n = Value::Null;
        let name = set.name.as_str();
        Some(if set.quantities {
            self.w.add("IFCELEMENTQUANTITY", args![gid, n.clone(), name, n.clone(), n, items])
        } else {
            self.w.add("IFCPROPERTYSET", args![gid, n.clone(), name, n, items])
        })
    }

    fn imported_quantity(&mut self, p: &cc_core::properties::Property) -> Option<Id> {
        let cc_core::properties::PropertyValue::Quantity { kind, value } = &p.value else { return None };
        let entity = [
            "IFCQUANTITYLENGTH",
            "IFCQUANTITYAREA",
            "IFCQUANTITYVOLUME",
            "IFCQUANTITYCOUNT",
            "IFCQUANTITYWEIGHT",
            "IFCQUANTITYTIME",
        ]
        .into_iter()
        .find(|k| k == kind)?;
        // Quantities are never negative.
        if !(value.is_finite() && *value >= 0.0) {
            return None;
        }
        let n = Value::Null;
        Some(self.w.add(entity, args![p.name.as_str(), n.clone(), n.clone(), *value, n]))
    }

    fn imported_property(&mut self, p: &cc_core::properties::Property, depth: usize) -> Option<Id> {
        use cc_core::properties::PropertyValue as V;
        let n = Value::Null;
        let name = p.name.as_str();
        let list = |values: &[cc_core::properties::IfcValue]| -> Value {
            let values: Vec<Value> = values.iter().map(typed_value).filter(|v| *v != Value::Null).collect();
            if values.is_empty() { Value::Null } else { Value::List(values) }
        };
        let opt = |v: &Option<cc_core::properties::IfcValue>| v.as_ref().map_or(Value::Null, typed_value);
        Some(match &p.value {
            V::Single(v) => self.w.add("IFCPROPERTYSINGLEVALUE", args![name, n.clone(), opt(v), n]),
            V::Enumerated(values) => self.w.add("IFCPROPERTYENUMERATEDVALUE", args![name, n.clone(), list(values), n]),
            V::List(values) => self.w.add("IFCPROPERTYLISTVALUE", args![name, n.clone(), list(values), n]),
            V::Bounded { lower, upper, set_point } => {
                self.w.add("IFCPROPERTYBOUNDEDVALUE", args![name, n.clone(), opt(upper), opt(lower), n, opt(set_point)])
            }
            V::Table { defining, defined } => {
                // Rows come in pairs.
                let rows = defining.len().min(defined.len());
                let (a, b) = (list(&defining[..rows]), list(&defined[..rows]));
                let args = args![name, n.clone(), a, b, n.clone(), n.clone(), n.clone(), n];
                self.w.add("IFCPROPERTYTABLEVALUE", args)
            }
            V::Complex { usage, properties } if depth < 8 => {
                let mut seen = std::collections::HashSet::new();
                let parts: Vec<Id> = properties
                    .iter()
                    .filter(|q| seen.insert(q.name.as_str()))
                    .filter_map(|q| self.imported_property(q, depth + 1))
                    .collect();
                if parts.is_empty() {
                    return None;
                }
                self.w.add("IFCCOMPLEXPROPERTY", args![name, n, usage.as_str(), parts])
            }
            V::Complex { .. } | V::Quantity { .. } => return None,
        })
    }

    /// Polygons as an `IfcPolygonalFaceSet` (shared corners share a point).
    fn face_set(&mut self, faces: &[Vec<DVec3>]) -> Option<Id> {
        let mut coords: Vec<Value> = Vec::new();
        let mut index: HashMap<(i64, i64, i64), i64> = HashMap::new();
        let mut ifc_faces = Vec::new();
        for face in faces {
            let mut ids: Vec<i64> = Vec::with_capacity(face.len());
            for p in face.iter().filter(|p| p.is_finite()) {
                let key = ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64, (p.z * 1e6).round() as i64);
                let i = *index.entry(key).or_insert_with(|| {
                    coords.push(reals(&[p.x, p.y, p.z]));
                    coords.len() as i64
                });
                if ids.last() != Some(&i) {
                    ids.push(i);
                }
            }
            if ids.len() > 1 && ids.first() == ids.last() {
                ids.pop();
            }
            if ids.len() >= 3 {
                let ids: Vec<Value> = ids.into_iter().map(Value::Int).collect();
                ifc_faces.push(self.w.add("IFCINDEXEDPOLYGONALFACE", args![ids]));
            }
        }
        if ifc_faces.is_empty() {
            return None;
        }
        let points = self.w.add("IFCCARTESIANPOINTLIST3D", args![coords]);
        Some(self.w.add("IFCPOLYGONALFACESET", args![points, Value::Null, ifc_faces, Value::Null]))
    }

    /// Columns, beams, stairs, ramps and railings.
    fn write_structure(&mut self) {
        let project = self.project;
        for e in project.elements.values() {
            let Some(ty) = project.element_type(e.type_id) else { continue };
            let Some(storey) = self.storey_placement(e.level) else { continue };
            let level_z = project.level_elevation(e.level);
            let placement = self.local_placement(Some(storey), Frame::at(DVec3::ZERO));
            let style = self.material_style(ty.main_material());
            let mut axis = None;
            let product: StructureProduct = match (&e.kind, &ty.data) {
                (ElementKind::Column(c), TypeData::Column { section, .. }) => {
                    let (base, top) = project.column_span(e, c);
                    let item = match *section {
                        cc_core::ColumnSection::Rectangular { width, depth } => {
                            let part = cc_geom::BoxPart {
                                origin: c.position,
                                u: DVec2::from_angle(c.rotation_deg.to_radians()),
                                v: DVec2::from_angle(c.rotation_deg.to_radians()).perp(),
                                u_range: (-width / 2.0, width / 2.0),
                                v_range: (-depth / 2.0, depth / 2.0),
                                z_range: (base, top),
                            };
                            self.oriented_box(&part, level_z)
                        }
                        cc_core::ColumnSection::Round { diameter } => {
                            let center = self.point2(c.position);
                            let position2 = self.w.add("IFCAXIS2PLACEMENT2D", args![center, Value::Null]);
                            let profile = self.w.add(
                                "IFCCIRCLEPROFILEDEF",
                                args![Value::Enum("AREA"), Value::Null, position2, diameter / 2.0],
                            );
                            let position = self.placement3(Frame::at(DVec3::new(0.0, 0.0, base - level_z)));
                            self.w.add("IFCEXTRUDEDAREASOLID", args![profile, position, self.dir_z, top - base])
                        }
                        cc_core::ColumnSection::Profile { .. } | cc_core::ColumnSection::Pipe { .. } => {
                            // The profile in the column's rotated frame, extruded up.
                            let profile = self.section_profile(*section);
                            let u = DVec2::from_angle(c.rotation_deg.to_radians());
                            let frame = Frame::horizontal(c.position.extend(base - level_z), u);
                            let position = self.placement3(frame);
                            self.w.add("IFCEXTRUDEDAREASOLID", args![profile, position, self.dir_z, top - base])
                        }
                    };
                    let volume = section_area(*section) * (top - base);
                    let q = vec![("Length", Quantity::Length(top - base)), ("NetVolume", Quantity::Volume(volume))];
                    StructureProduct::new("IFCCOLUMN", "COLUMN", vec![item]).quantities("Qto_ColumnBaseQuantities", q)
                }
                (ElementKind::Beam(_), TypeData::Beam { .. }) => {
                    // The body as framed into columns and other beams.
                    let Some(g) = self.geom.beams.get(&e.id) else { continue };
                    let TypeData::Beam { width, height, shape, .. } = ty.data else { continue };
                    let solid = matches!(shape, cc_core::SectionShape::Solid);
                    let level = g.slope.abs() < 1e-12;
                    let brep = !level || (!solid && !g.is_square());
                    let items = if brep {
                        // Sloped, or a profile with cut ends: each part as a closed shell.
                        g.parts()
                            .into_iter()
                            .map(|(footprint, (z0, z1))| {
                                let mut s = cc_geom::Solid::new(e.id, e.category(), [0; 3]);
                                let (origin, dir, slope, base) = (g.origin, g.dir, g.slope, g.z_range.0);
                                let z = move |p: DVec2| base + slope * (p - origin).dot(dir);
                                s.add_prism(&footprint, |p| z(p) + z0, |p| z(p) + z1);
                                self.faceted_brep(&s.faces, level_z)
                            })
                            .collect()
                    } else if !solid {
                        // A profile square at both ends: the section swept along the axis.
                        let profile =
                            self.section_profile(cc_core::ColumnSection::Profile { width, depth: height, shape });
                        let start = g.world(g.start_ext.0, 0.0);
                        let center = (g.z_range.0 + g.z_range.1) * 0.5 - level_z;
                        let frame =
                            Frame { origin: start.extend(center), x: g.normal.extend(0.0), z: g.dir.extend(0.0) };
                        let position = self.placement3(frame);
                        let length = g.end_ext.0 - g.start_ext.0;
                        vec![self.w.add("IFCEXTRUDEDAREASOLID", args![profile, position, self.dir_z, length])]
                    } else if g.is_square() {
                        let part = cc_geom::BoxPart {
                            origin: g.origin,
                            u: g.dir,
                            v: g.normal,
                            u_range: (g.start_ext.0, g.end_ext.0),
                            v_range: (-g.half, g.half),
                            z_range: g.z_range,
                        };
                        vec![self.oriented_box(&part, level_z)]
                    } else {
                        // Mitered or skewed ends: the footprint in the beam's frame, extruded up.
                        let frame = Frame::horizontal(DVec3::new(g.origin.x, g.origin.y, g.z_range.0 - level_z), g.dir);
                        let position = self.placement3(frame);
                        let (profile, depth) = (g.local_footprint(), g.z_range.1 - g.z_range.0);
                        vec![self.extrude_polygon(&profile, position, self.dir_z, depth)]
                    };
                    // The location line, which keeps the beam's own length.
                    let axis_start = self.point2(g.origin);
                    let axis_end = self.point2(g.world(g.length, 0.0));
                    axis = Some(self.w.add("IFCPOLYLINE", args![vec![axis_start, axis_end]]));
                    let q =
                        vec![("Length", Quantity::Length(g.cut_length())), ("NetVolume", Quantity::Volume(g.volume()))];
                    let product =
                        StructureProduct::new("IFCBEAM", "BEAM", items).quantities("Qto_BeamBaseQuantities", q);
                    if brep { product.brep() } else { product }
                }
                (ElementKind::Foundation(_), TypeData::Foundation { kind, .. }) => {
                    let Some(g) = self.geom.foundations.get(&e.id) else { continue };
                    let frame = Frame::at(DVec3::new(0.0, 0.0, g.z_range.0 - level_z));
                    let position = self.placement3(frame);
                    let item = self.extrude_polygon(&g.footprint, position, self.dir_z, g.z_range.1 - g.z_range.0);
                    let area = polygon::area(&g.footprint);
                    let q = vec![
                        ("Height", Quantity::Length(g.z_range.1 - g.z_range.0)),
                        ("NetVolume", Quantity::Volume(g.volume())),
                    ];
                    match kind {
                        cc_core::FoundationKind::Pad => StructureProduct::new("IFCFOOTING", "PAD_FOOTING", vec![item])
                            .quantities("Qto_FootingBaseQuantities", q),
                        cc_core::FoundationKind::Strip => {
                            StructureProduct::new("IFCFOOTING", "STRIP_FOOTING", vec![item])
                                .quantities("Qto_FootingBaseQuantities", q)
                        }
                        cc_core::FoundationKind::Slab => {
                            let q = vec![
                                ("Width", Quantity::Length(g.z_range.1 - g.z_range.0)),
                                ("NetArea", Quantity::Area(area)),
                                ("NetVolume", Quantity::Volume(g.volume())),
                            ];
                            StructureProduct::new("IFCSLAB", "BASESLAB", vec![item])
                                .quantities("Qto_SlabBaseQuantities", q)
                        }
                    }
                }
                (ElementKind::Stair(s), TypeData::Stair { build, .. }) => {
                    let Some(g) = cc_geom::stair_geom(project, e, s) else { continue };
                    let items = self.pieces(e, g.pieces(build), level_z);
                    let count = |k: usize| Value::Typed("IFCCOUNTMEASURE", Box::new(Value::Int(k as i64)));
                    let length = |x: f64| Value::Typed("IFCPOSITIVELENGTHMEASURE", Box::new(Value::Real(x)));
                    let mut props = vec![
                        ("NumberOfRiser", count(g.risers)),
                        ("NumberOfTreads", count(g.risers.saturating_sub(1))),
                        ("RiserHeight", length(g.riser)),
                        ("TreadLength", length(g.tread)),
                    ];
                    if build.construction == cc_core::StairConstruction::CastInPlace {
                        props.push(("WaistThickness", length(build.waist)));
                    }
                    let winders = s.layout.winders >= 2;
                    let shape = match s.layout.shape {
                        cc_core::StairShape::Straight => "STRAIGHT_RUN_STAIR",
                        cc_core::StairShape::StraightLanding => "TWO_STRAIGHT_RUN_STAIR",
                        cc_core::StairShape::LShaped if winders => "QUARTER_WINDING_STAIR",
                        cc_core::StairShape::LShaped => "QUARTER_TURN_STAIR",
                        cc_core::StairShape::UShaped => "HALF_TURN_STAIR",
                        cc_core::StairShape::ThreeFlight => "TWO_QUARTER_TURN_STAIR",
                        cc_core::StairShape::Spiral => "SPIRAL_STAIR",
                    };
                    StructureProduct {
                        props: Some(("Pset_StairCommon", props)),
                        ..StructureProduct::new("IFCSTAIR", shape, items)
                    }
                    .brep()
                }
                (ElementKind::Ramp(r), TypeData::Ramp { thickness, .. }) => {
                    let Some(g) = cc_geom::ramp_geom(project, e, r) else { continue };
                    let items = self.pieces(e, g.pieces(*thickness), level_z);
                    let shape = match (r.layout.shape, g.check.flights) {
                        (cc_core::RampShape::Straight, 1) => "STRAIGHT_RUN_RAMP",
                        (cc_core::RampShape::Straight, 2) => "TWO_STRAIGHT_RUN_RAMP",
                        (cc_core::RampShape::LShaped, _) => "QUARTER_TURN_RAMP",
                        (cc_core::RampShape::UShaped, 2) => "HALF_TURN_RAMP",
                        (cc_core::RampShape::Spiral, _) => "SPIRAL_RAMP",
                        _ => "NOTDEFINED",
                    };
                    StructureProduct::new("IFCRAMP", shape, items).brep()
                }
                (ElementKind::Railing(r), TypeData::Railing { .. }) => {
                    let items = self.pieces(e, cc_geom::railing_pieces(project, e, r), level_z);
                    let q = vec![("Length", Quantity::Length(cc_geom::railing_length(project, e, r)))];
                    StructureProduct::new("IFCRAILING", "GUARDRAIL", items)
                        .quantities("Qto_RailingBaseQuantities", q)
                        .brep()
                }
                _ => continue,
            };
            if product.items.is_empty() {
                continue;
            }
            // Joined elements cut it: the body as it remains, and its volume.
            let mut product = product;
            if self.cut_by_join(e.id) {
                product.items = self.cut_body(e.id, level_z);
                product.representation = "Brep";
                if let Some((_, quantities)) = &mut product.quantities {
                    for (name, q) in quantities.iter_mut() {
                        if *name == "NetVolume" {
                            *q = Quantity::Volume(self.geom.volume(e.id));
                        }
                    }
                }
            }
            for &item in &product.items {
                self.styled(item, style);
            }
            let shape = self.shape_of_type(product.items, axis, product.representation);
            let mut attrs = self.element_args(e, placement, shape);
            attrs.push(Value::Enum(product.kind));
            let id = self.w.add(product.entity, attrs);
            self.products.insert(e.id, id);
            self.register(e, id);
            if let Some((set, quantities)) = &product.quantities {
                self.quantities(e, set, quantities);
            }
            if let Some((name, props)) = product.props {
                let status = ifc_status(self.project, e);
                self.status_written.insert(e.id);
                self.property_set_status(&format!("pset-{}-{name}", e.id), name, vec![id], props, status);
            }
        }
    }

    /// Reinforcement: one `IfcReinforcingBar` per set, each bar a disk swept
    /// along its center line (stirrups closed).
    fn write_rebar(&mut self) {
        let project = self.project;
        let geom = self.geom;
        for set in geom.rebar.values() {
            let Some(e) = project.element(set.element) else { continue };
            let ElementKind::Rebar(r) = &e.kind else { continue };
            let Some(ty) = project.element_type(e.type_id) else { continue };
            let Some(storey) = self.storey_placement(e.level) else { continue };
            let level_z = project.level_elevation(e.level);
            let items: Vec<Id> = set
                .bars
                .iter()
                .filter(|b| b.points.len() >= 2 && b.length() > 1e-6)
                .map(|bar| {
                    let pts: Vec<Id> = directrix(bar, set.diameter)
                        .into_iter()
                        .map(|p| self.point3(DVec3::new(p.x, p.y, p.z - level_z)))
                        .collect();
                    let line = self.w.add("IFCPOLYLINE", args![pts]);
                    let n = Value::Null;
                    self.w.add("IFCSWEPTDISKSOLID", args![line, set.diameter / 2.0, n.clone(), n.clone(), n])
                })
                .collect();
            if items.is_empty() {
                continue;
            }
            let style = self.material_style(ty.main_material());
            for &item in &items {
                self.styled(item, style);
            }
            let placement = self.local_placement(Some(storey), Frame::at(DVec3::ZERO));
            let shape = self.shape_of_type(items, None, "AdvancedSweptSolid");
            let mut attrs = self.element_args(e, placement, shape);
            let area = std::f64::consts::PI * set.diameter * set.diameter / 4.0;
            let kind = match r.layout {
                cc_core::RebarLayout::Longitudinal { .. } => "MAIN",
                cc_core::RebarLayout::Stirrups { .. } => "LIGATURE",
            };
            attrs.extend(args![
                Value::Null,
                set.diameter,
                area,
                set.bar_length(),
                Value::Enum(kind),
                Value::Enum("TEXTURED")
            ]);
            let product = self.w.add("IFCREINFORCINGBAR", attrs);
            self.products.insert(e.id, product);
            self.register(e, product);
        }
    }

    /// Components: one extrusion per piece in the component's own placement
    /// (insertion point and rotation), so importers recover the instance.
    fn write_components(&mut self) {
        let project = self.project;
        for e in project.elements.values() {
            let ElementKind::Component(c) = &e.kind else { continue };
            let Some(TypeData::Component { def, .. }) = project.element_type(e.type_id).map(|t| &t.data) else {
                continue;
            };
            let Some(storey) = self.storey_placement(e.level) else { continue };
            let rot = DVec2::from_angle(c.rotation_deg.to_radians());
            let placement = self.local_placement(
                Some(storey),
                Frame::horizontal(DVec3::new(c.position.x, c.position.y, c.base_offset), rot),
            );
            let mut parts = cc_geom::resolve_component(def, &c.overrides);
            parts.retain(|p| !p.plan_only);
            // Shapes mix no representation types: with any mesh piece, all pieces are face sets.
            let tessellated = parts.iter().any(|p| !p.faces.is_empty());
            let mut items = Vec::new();
            for part in parts {
                let item = if tessellated {
                    let faces = if part.faces.is_empty() {
                        let mut s = cc_geom::Solid::new(e.id, e.category(), [0; 3]);
                        s.add_prism(&part.footprint, |_| part.z0, |_| part.z1);
                        s.faces.into_iter().map(|f| f.verts).collect()
                    } else {
                        part.faces
                    };
                    let Some(item) = self.face_set(&faces) else { continue };
                    item
                } else {
                    let profile = polygon::ccw(&part.footprint);
                    if profile.len() < 3 || part.z1 - part.z0 < 1e-6 {
                        continue;
                    }
                    let position = self.placement3(Frame::at(DVec3::new(0.0, 0.0, part.z0)));
                    self.extrude_polygon(&profile, position, self.dir_z, part.z1 - part.z0)
                };
                let style = if part.material.is_empty() {
                    self.style("Generic", [200, 200, 200], 0.0)
                } else {
                    self.material_style(&part.material)
                };
                self.styled(item, style);
                items.push(item);
            }
            if items.is_empty() {
                continue;
            }
            let shape =
                if tessellated { self.shape_of_type(items, None, "Tessellation") } else { self.shape(items, None) };
            let class = component_class(def);
            let mut attrs = self.element_args(e, placement, shape);
            attrs.resize(class.attrs, Value::Null);
            if !class.object_type.is_empty() {
                attrs[4] = Value::Str(class.object_type.clone());
            }
            if let Some(i) = class.predefined {
                attrs[i] = class.predefined_value();
            }
            let product = self.w.add(class.entity, attrs);
            self.products.insert(e.id, product);
            self.register(e, product);
            // Instance parameter values, so importers can restore overrides.
            let values = def.values(&c.overrides);
            let props: Vec<(&str, Value)> = def
                .params
                .iter()
                .filter(|p| p.instance)
                .filter_map(|p| {
                    let v = *values.get(&p.name)?;
                    let measure = match p.kind {
                        cc_core::ParamKind::Length => "IFCLENGTHMEASURE",
                        cc_core::ParamKind::Angle => "IFCPLANEANGLEMEASURE",
                        cc_core::ParamKind::Number => "IFCREAL",
                    };
                    // Angles are stored in radians (the file's angle unit).
                    let v = if p.kind == cc_core::ParamKind::Angle { v.to_radians() } else { v };
                    Some((p.name.as_str(), Value::Typed(measure, Box::new(Value::Real(v)))))
                })
                .collect();
            self.property_set(&format!("comp-{}", e.id), COMPONENT_PSET, vec![product], props);
            // Sets carried over from an imported file.
            for (i, set) in c.properties.iter().enumerate() {
                if let Some(set) = self.imported_set(&format!("imported-{}-{i}", e.id), set) {
                    let gid = self.guid(&format!("imported-rel-{}-{i}", e.id));
                    let n = Value::Null;
                    self.w.add("IFCRELDEFINESBYPROPERTIES", args![gid, n.clone(), n.clone(), n, vec![product], set]);
                }
            }
        }
    }

    /// Pipes and conduits: a circle extruded along the run, placed at its start.
    fn write_segments(&mut self) {
        let project = self.project;
        let geom = self.geom;
        for g in geom.segments.values() {
            let Some(e) = project.element(g.element) else { continue };
            let Some(s) = e.as_segment() else { continue };
            let Some(storey) = self.storey_placement(e.level) else { continue };
            let Some(axis) = (g.b - g.a).try_normalize() else { continue };
            let origin = DVec3::new(s.start.x, s.start.y, s.start_offset);
            let frame = Frame { origin, x: axis.any_orthonormal_vector(), z: axis };
            let placement = self.local_placement(Some(storey), frame);
            let profile = self
                .w
                .add("IFCCIRCLEPROFILEDEF", args![Value::Enum("AREA"), Value::Null, Value::Null, g.diameter / 2.0]);
            let item = self.w.add("IFCEXTRUDEDAREASOLID", args![profile, self.identity, self.dir_z, g.length()]);
            let color = project.system_color(e);
            let name = e.systems().first().and_then(|id| project.system(*id)).map_or("Generic", |s| s.name.as_str());
            let style = self.style(&format!("system {name}"), color, 0.0);
            self.styled(item, style);
            let shape = self.shape(vec![item], None);
            let (entity, kind, qto) = match e.kind {
                ElementKind::Conduit(_) => {
                    ("IFCCABLECARRIERSEGMENT", "CONDUITSEGMENT", "Qto_CableCarrierSegmentBaseQuantities")
                }
                _ => ("IFCPIPESEGMENT", "RIGIDSEGMENT", "Qto_PipeSegmentBaseQuantities"),
            };
            let mut attrs = self.element_args(e, placement, shape);
            attrs.push(Value::Enum(kind));
            let product = self.w.add(entity, attrs);
            self.products.insert(e.id, product);
            self.register(e, product);
            self.quantities(e, qto, &[("Length", Quantity::Length(g.length()))]);
        }
    }

    /// Fittings where pipes and conduits meet: a socket (a short circular
    /// extrusion) along each branch, placed at the node; one type object per
    /// kind of fitting. Returns the written fittings with their system.
    fn write_fittings(&mut self) -> Vec<(Option<cc_core::SystemId>, Id)> {
        use cc_geom::mep_net::FittingKind as K;
        let geom = self.geom;
        let n = Value::Null;
        let mut written = Vec::new();
        let mut typed: BTreeMap<(&'static str, &'static str), Vec<Id>> = BTreeMap::new();
        for (i, f) in geom.mep.fittings.iter().enumerate() {
            let Some(storey) = self.storey_placement(f.level) else { continue };
            let level_z = self.project.level_elevation(f.level);
            let pipe = f.category == cc_core::Category::Pipe;
            let (entity, kind, name) = match (pipe, f.kind) {
                (true, K::Elbow { .. }) => ("IFCPIPEFITTING", "BEND", "Elbow"),
                (true, K::Tee | K::Cross) => ("IFCPIPEFITTING", "JUNCTION", "Junction"),
                (true, K::Coupling) => ("IFCPIPEFITTING", "CONNECTOR", "Coupling"),
                (true, K::Reducer) => ("IFCPIPEFITTING", "TRANSITION", "Reducer"),
                (false, K::Elbow { .. }) => ("IFCCABLECARRIERFITTING", "BEND", "Bend"),
                (false, K::Tee) => ("IFCCABLECARRIERFITTING", "TEE", "Tee"),
                (false, K::Cross) => ("IFCCABLECARRIERFITTING", "CROSS", "Cross"),
                (false, K::Reducer) => ("IFCCABLECARRIERFITTING", "REDUCER", "Reducer"),
                (false, K::Coupling) => ("IFCCABLECARRIERFITTING", "NOTDEFINED", "Coupling"),
            };
            let items: Vec<Id> = f
                .branches
                .iter()
                .filter_map(|&(dir, d)| {
                    let x = dir.any_orthonormal_vector();
                    let position = self.placement3(Frame { origin: DVec3::ZERO, x, z: dir });
                    let radius = d * 0.6;
                    (radius > 0.0).then(|| {
                        let profile = self
                            .w
                            .add("IFCCIRCLEPROFILEDEF", args![Value::Enum("AREA"), Value::Null, Value::Null, radius]);
                        self.w.add("IFCEXTRUDEDAREASOLID", args![profile, position, self.dir_z, d * 0.8])
                    })
                })
                .collect();
            if items.is_empty() {
                continue;
            }
            let color = f.system.and_then(|s| self.project.system(s)).map_or([128, 128, 128], |s| s.color());
            let style_name = f.system.and_then(|s| self.project.system(s)).map_or("Generic", |s| s.name.as_str());
            let style = self.style(&format!("system {style_name}"), color, 0.0);
            for &item in &items {
                self.styled(item, style);
            }
            let shape = self.shape(items, None);
            let origin = DVec3::new(f.point.x, f.point.y, f.point.z - level_z);
            let placement = self.local_placement(Some(storey), Frame::at(origin));
            let gid = self.guid(&format!("fitting-{i}-{}", f.node));
            let product = self.w.add(
                entity,
                args![gid, n.clone(), name, n.clone(), n.clone(), placement, shape, n.clone(), Value::Enum(kind)],
            );
            typed.entry((entity, kind)).or_default().push(product);
            self.contained.entry(f.level).or_default().push(product);
            written.push((f.system, product));
        }
        for ((entity, kind), objects) in typed {
            let type_entity =
                if entity == "IFCPIPEFITTING" { "IFCPIPEFITTINGTYPE" } else { "IFCCABLECARRIERFITTINGTYPE" };
            let gid = self.guid(&format!("fitting-type-{entity}-{kind}"));
            let name = format!("{} {}", crate::schema::display_name(entity), kind.to_lowercase());
            let ty = self.w.add(
                type_entity,
                args![
                    gid,
                    n.clone(),
                    name.as_str(),
                    n.clone(),
                    n.clone(),
                    n.clone(),
                    n.clone(),
                    n.clone(),
                    n.clone(),
                    Value::Enum(kind)
                ],
            );
            let gid = self.guid(&format!("fitting-deftype-{entity}-{kind}"));
            self.w.add("IFCRELDEFINESBYTYPE", args![gid, n.clone(), n.clone(), n.clone(), objects, ty]);
        }
        written
    }

    /// Plumbing systems and electrical circuits grouping their written members.
    fn write_systems(&mut self, fittings: &[(Option<cc_core::SystemId>, Id)]) {
        let project = self.project;
        let n = Value::Null;
        for (id, system) in &project.systems {
            let mut members: Vec<Id> =
                project.system_members(*id).filter_map(|e| self.products.get(&e.id).copied()).collect();
            members.extend(fittings.iter().filter(|(s, _)| *s == Some(*id)).map(|(_, p)| *p));
            if let Some(panel) = system.panel.and_then(|p| self.products.get(&p).copied()) {
                members.insert(0, panel);
            }
            if members.is_empty() {
                continue;
            }
            use cc_core::SystemKind as K;
            let (entity, kind) = match system.kind {
                K::ColdWater => ("IFCDISTRIBUTIONSYSTEM", "DOMESTICCOLDWATER"),
                K::HotWater => ("IFCDISTRIBUTIONSYSTEM", "DOMESTICHOTWATER"),
                K::Sanitary => ("IFCDISTRIBUTIONSYSTEM", "SEWAGE"),
                K::Vent => ("IFCDISTRIBUTIONSYSTEM", "VENT"),
                K::Rainwater => ("IFCDISTRIBUTIONSYSTEM", "RAINWATER"),
                K::Power => ("IFCDISTRIBUTIONCIRCUIT", "ELECTRICAL"),
                K::Lighting => ("IFCDISTRIBUTIONCIRCUIT", "LIGHTING"),
            };
            let gid = self.guid(&format!("system-{id}"));
            let group = self.w.add(
                entity,
                args![
                    gid,
                    n.clone(),
                    system.name.as_str(),
                    n.clone(),
                    n.clone(),
                    opt_str(&system.code),
                    Value::Enum(kind)
                ],
            );
            let gid = self.guid(&format!("system-rel-{id}"));
            self.w.add("IFCRELASSIGNSTOGROUP", args![gid, n.clone(), n.clone(), n.clone(), members, n.clone(), group]);
        }
    }

    // --- Materials and types ---

    fn material(&mut self, name: &str) -> Id {
        if let Some(&id) = self.materials.get(name) {
            return id;
        }
        let id = self.w.add("IFCMATERIAL", args![name, Value::Null, Value::Null]);
        self.materials.insert(name.to_string(), id);
        id
    }

    /// Layer set of a layered type, or `None` for single-material types.
    fn layer_set(&mut self, type_id: ElementTypeId) -> Option<Id> {
        if let Some(&id) = self.layer_sets.get(&type_id) {
            return Some(id);
        }
        let ty = self.project.element_type(type_id)?;
        if ty.layers().is_empty() {
            return None;
        }
        let (name, layers) = (ty.name.clone(), ty.layers().to_vec());
        let layers: Vec<Id> = layers
            .iter()
            .map(|l| {
                let m = self.material(&l.material);
                let n = Value::Null;
                self.w.add("IFCMATERIALLAYER", args![m, l.thickness, n.clone(), n.clone(), n.clone(), n.clone(), n])
            })
            .collect();
        let id = self.w.add("IFCMATERIALLAYERSET", args![layers, name, Value::Null]);
        self.layer_sets.insert(type_id, id);
        Some(id)
    }

    /// Assign the layer set of `type_id` to `product`, with layers running in
    /// the negative `direction` from `offset` (one usage shared per type).
    fn layer_usage(&mut self, type_id: ElementTypeId, product: Id, direction: &'static str, offset: f64) {
        let Some(set) = self.layer_set(type_id) else { return };
        let usage = match self.layer_usages.get(&type_id) {
            Some(&usage) => usage,
            None => {
                let args = args![set, Value::Enum(direction), Value::Enum("NEGATIVE"), offset, Value::Null];
                let usage = self.w.add("IFCMATERIALLAYERSETUSAGE", args);
                self.layer_usages.insert(type_id, usage);
                usage
            }
        };
        self.usage_objects.entry(usage).or_default().push(product);
    }

    /// The IFC type object for `type_id`, created with its material on first use.
    fn element_type(&mut self, type_id: ElementTypeId) -> Option<Id> {
        if let Some(&id) = self.types.get(&type_id) {
            return Some(id);
        }
        let ty = self.project.element_type(type_id)?.clone();
        let gid = self.guid_of(ty.uid, &format!("type-{type_id}"));
        let n = Value::Null;
        let description = opt_str(&ty.info.description);
        let mut base =
            args![gid, n.clone(), ty.name.as_str(), description, n.clone(), n.clone(), n.clone(), n.clone(), n.clone()];
        let (entity, extra): (&str, Vec<Value>) = match ty.data {
            // Rooms, grids, dimensions, texts, imported drawings and masses have no IFC type object.
            TypeData::Room {}
            | TypeData::Grid { .. }
            | TypeData::Dimension { .. }
            | TypeData::Text { .. }
            | TypeData::CadImport {}
            | TypeData::Mass { .. } => {
                return None;
            }
            TypeData::Component { ref def, .. } => {
                let class = component_class(def);
                // Classes whose schema type object is abstract go untyped.
                let (entity, layout) = class.type_entity?;
                let sets: Vec<Id> = def
                    .ifc
                    .iter()
                    .flat_map(|ifc| ifc.properties.iter().enumerate())
                    .filter_map(|(i, set)| self.imported_set(&format!("imported-type-{type_id}-{i}"), set))
                    .collect();
                if !sets.is_empty() {
                    base[5] = Value::List(sets.into_iter().map(Value::Ref).collect());
                }
                let predefined = class.predefined_value();
                if matches!(&predefined, Value::EnumName(p) if p == "USERDEFINED") {
                    base[8] = Value::Str(class.object_type.clone());
                }
                let extra = match layout {
                    TypeLayout::Predefined => vec![predefined],
                    TypeLayout::Plain => Vec::new(),
                    // Furniture types also state where they are assembled.
                    TypeLayout::Furniture => vec![Value::Enum("NOTDEFINED"), predefined],
                    TypeLayout::Door | TypeLayout::Window => {
                        vec![predefined, Value::Enum("NOTDEFINED"), Value::Null, Value::Null]
                    }
                };
                (entity, extra)
            }
            TypeData::Pipe { .. } => ("IFCPIPESEGMENTTYPE", args![Value::Enum("RIGIDSEGMENT")]),
            TypeData::Conduit { .. } => ("IFCCABLECARRIERSEGMENTTYPE", args![Value::Enum("CONDUITSEGMENT")]),
            TypeData::Column { .. } => ("IFCCOLUMNTYPE", args![Value::Enum("COLUMN")]),
            TypeData::Beam { .. } => ("IFCBEAMTYPE", args![Value::Enum("BEAM")]),
            TypeData::Rebar { diameter, .. } => {
                let area = std::f64::consts::PI * diameter * diameter / 4.0;
                let n = Value::Null;
                (
                    "IFCREINFORCINGBARTYPE",
                    args![Value::Enum("NOTDEFINED"), diameter, area, n.clone(), n.clone(), n.clone(), n],
                )
            }
            TypeData::Foundation { kind: cc_core::FoundationKind::Pad, .. } => {
                ("IFCFOOTINGTYPE", args![Value::Enum("PAD_FOOTING")])
            }
            TypeData::Foundation { kind: cc_core::FoundationKind::Strip, .. } => {
                ("IFCFOOTINGTYPE", args![Value::Enum("STRIP_FOOTING")])
            }
            TypeData::Foundation { kind: cc_core::FoundationKind::Slab, .. } => {
                ("IFCSLABTYPE", args![Value::Enum("BASESLAB")])
            }
            TypeData::Stair { .. } => ("IFCSTAIRTYPE", args![Value::Enum("STRAIGHT_RUN_STAIR")]),
            TypeData::Ramp { .. } => ("IFCRAMPTYPE", args![Value::Enum("NOTDEFINED")]),
            TypeData::Railing { .. } => ("IFCRAILINGTYPE", args![Value::Enum("GUARDRAIL")]),
            TypeData::Wall { .. } => ("IFCWALLTYPE", args![Value::Enum("STANDARD")]),
            TypeData::Floor { .. } => ("IFCSLABTYPE", args![Value::Enum("FLOOR")]),
            TypeData::Ceiling { .. } => ("IFCCOVERINGTYPE", args![Value::Enum("CEILING")]),
            TypeData::Roof { .. } => ("IFCROOFTYPE", args![Value::Enum("NOTDEFINED")]),
            TypeData::Door { .. } => {
                ("IFCDOORTYPE", args![Value::Enum("DOOR"), Value::Enum("NOTDEFINED"), Value::Null, Value::Null])
            }
            TypeData::Window { .. } => {
                ("IFCWINDOWTYPE", args![Value::Enum("WINDOW"), Value::Enum("SINGLE_PANEL"), Value::Null, Value::Null])
            }
        };
        let id = self.w.add(entity, base.into_iter().chain(extra).collect());
        self.types.insert(type_id, id);
        let info = &ty.info;
        let label = |s: &str| Value::Typed("IFCLABEL", Box::new(s.into()));
        let manufacturer: Vec<(&str, Value)> = [("Manufacturer", &info.manufacturer), ("ModelLabel", &info.model)]
            .into_iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| (k, label(v)))
            .collect();
        self.property_set(&format!("mfr-{type_id}"), "Pset_ManufacturerTypeInformation", vec![id], manufacturer);
        let material = match &ty.data {
            TypeData::Door { material, .. }
            | TypeData::Window { material, .. }
            | TypeData::Ceiling { material, .. }
            | TypeData::Foundation { material, .. }
            | TypeData::Pipe { material, .. }
            | TypeData::Conduit { material, .. }
            | TypeData::Rebar { material, .. } => Some(self.material(material)),
            TypeData::Component { def, .. } if !def.main_material().is_empty() => {
                Some(self.material(def.main_material()))
            }
            TypeData::Component { .. } => None,
            _ => self.layer_set(type_id),
        };
        if let Some(m) = material {
            self.associate_material(&format!("mat-type-{type_id}"), vec![id], m);
        }
        Some(id)
    }

    fn associate_material(&mut self, tag: &str, objects: Vec<Id>, material: Id) {
        let gid = self.guid(tag);
        self.w.add("IFCRELASSOCIATESMATERIAL", args![gid, Value::Null, Value::Null, Value::Null, objects, material]);
    }

    /// Register `product` as an occurrence of its element's type and as contained in its level.
    fn register(&mut self, e: &Element, product: Id) {
        if self.element_type(e.type_id).is_some() {
            self.typed.entry(e.type_id).or_default().push(product);
        }
        self.contained.entry(e.level).or_default().push(product);
    }

    /// Common leading attributes of an element occurrence.
    fn element_args(&self, e: &Element, placement: Id, shape: Id) -> Vec<Value> {
        let gid = self.guid_of(e.uid, &format!("element-{}", e.id));
        let name = self.project.type_name(e.type_id);
        args![gid, Value::Null, name, opt_str(&e.comments), Value::Null, placement, shape, opt_str(&e.mark)]
    }

    fn quantities(&mut self, e: &Element, set_name: &str, quantities: &[(&str, Quantity)]) {
        let qs: Vec<Id> = quantities
            .iter()
            .map(|&(name, q)| {
                let (entity, value) = match q {
                    Quantity::Length(v) => ("IFCQUANTITYLENGTH", v),
                    Quantity::Area(v) => ("IFCQUANTITYAREA", v),
                    Quantity::Volume(v) => ("IFCQUANTITYVOLUME", v),
                };
                self.w.add(entity, args![name, Value::Null, Value::Null, value, Value::Null])
            })
            .collect();
        let gid = self.guid(&format!("qto-{}", e.id));
        let n = Value::Null;
        let set = self.w.add("IFCELEMENTQUANTITY", args![gid, n.clone(), set_name, n.clone(), n, qs]);
        let product = self.products.get(&e.id).copied();
        if let Some(product) = product {
            let gid = self.guid(&format!("qto-rel-{}", e.id));
            self.w.add(
                "IFCRELDEFINESBYPROPERTIES",
                args![gid, Value::Null, Value::Null, Value::Null, vec![product], set],
            );
        }
    }

    // --- Elements ---

    fn storey_placement(&self, level: LevelId) -> Option<Id> {
        self.storeys.get(&level).map(|s| s.1)
    }

    fn write_wall(&mut self, id: ElementId) {
        let project = self.project;
        let Some(e) = project.element(id) else { return };
        let Some(wall) = e.as_wall() else { return };
        let geom = self.geom;
        let Some(g) = geom.walls.get(&id) else { return };
        let Some(storey) = self.storey_placement(e.level) else { return };
        let level_z = project.level_elevation(e.level);
        let placement = self.local_placement(
            Some(storey),
            Frame::horizontal(DVec3::new(g.origin.x, g.origin.y, g.base_z - level_z), g.dir),
        );

        let h = g.half;
        let (body, axis) = if let Some(arc) = g.arc {
            // An arc wall: its outline of arcs and lines extruded, its axis a trimmed circle.
            let left = (g.start_ext.0, g.end_ext.0);
            let right = (g.start_ext.1, g.end_ext.1);
            let profile = self.wall_outline_profile(g, left, right, (h, -h));
            let depth = g.top_z - g.base_z;
            let body = self.w.add("IFCEXTRUDEDAREASOLID", args![profile, self.identity, self.dir_z, depth]);
            let center = self.point2(wall_local(g, arc.center));
            let position = self.w.add("IFCAXIS2PLACEMENT2D", args![center, Value::Null]);
            let circle = self.w.add("IFCCIRCLE", args![position, arc.radius]);
            let end = self.point2(wall_local(g, g.world(g.length, 0.0)));
            let sense = arc.sweep >= 0.0;
            let axis = self
                .w
                .add("IFCTRIMMEDCURVE", args![circle, vec![self.origin2], vec![end], sense, Value::Enum("CARTESIAN")]);
            (body, axis)
        } else {
            let profile = [
                DVec2::new(g.start_ext.1, -h),
                DVec2::new(g.end_ext.1, -h),
                DVec2::new(g.end_ext.0, h),
                DVec2::new(g.start_ext.0, h),
            ];
            let body = self.extrude_polygon(&profile, self.identity, self.dir_z, g.top_z - g.base_z);
            let axis_start = self.origin2;
            let axis_end = self.point2(DVec2::new(g.length, 0.0));
            (body, self.w.add("IFCPOLYLINE", args![vec![axis_start, axis_end]]))
        };
        if let Some(ty) = project.element_type(e.type_id) {
            let style = self.material_style(ty.main_material());
            self.styled(body, style);
        }
        let shape = self.shape(vec![body], Some(axis));
        let mut attrs = self.element_args(e, placement, shape);
        attrs.push(Value::Enum("STANDARD"));
        let product = self.w.add("IFCWALL", attrs);
        self.products.insert(e.id, product);
        self.register(e, product);

        // Layers are listed from the left face (+Y) towards the right.
        self.layer_usage(e.type_id, product, "AXIS2", h);

        let ty = project.element_type(e.type_id);
        let mut props = vec![
            ("Reference", Value::Typed("IFCIDENTIFIER", Box::new(project.type_name(e.type_id).into()))),
            ("LoadBearing", Value::Typed("IFCBOOLEAN", Box::new(wall.structural.into()))),
        ];
        if let Some(function) = ty.and_then(|t| t.wall_function()) {
            props.push(("IsExternal", Value::Typed("IFCBOOLEAN", Box::new(function.is_external().into()))));
        }
        if let Some(rating) = ty.map(|t| t.info.fire_rating.clone()).filter(|r| !r.is_empty()) {
            props.push(("FireRating", Value::Typed("IFCLABEL", Box::new(rating.into()))));
        }
        let status = ifc_status(project, e);
        self.status_written.insert(e.id);
        self.property_set_status(&format!("wall-{}", e.id), "Pset_WallCommon", vec![product], props, status);

        let opening_area: f64 = g.openings.iter().map(|o| (o.u1 - o.u0) * (o.z1 - o.z0)).sum();
        let thickness = 2.0 * h;
        let height = project.wall_height(e, wall);
        let length = project.wall_length(e, wall);
        self.quantities(
            e,
            "Qto_WallBaseQuantities",
            &[
                ("Length", Quantity::Length(length)),
                ("Height", Quantity::Length(height)),
                ("Width", Quantity::Length(thickness)),
                ("GrossSideArea", Quantity::Area(length * height)),
                ("NetSideArea", Quantity::Area((length * height - opening_area).max(0.0))),
                ("NetVolume", Quantity::Volume(self.geom.volume(e.id))),
            ],
        );

        for o in g.openings.clone() {
            self.write_opening(g, placement, product, &o);
        }
    }

    fn write_opening(&mut self, g: &WallGeom, wall_placement: Id, wall: Id, o: &cc_geom::Opening) {
        let project = self.project;
        let Some(e) = project.element(o.element) else { return };
        let Some(ty) = project.element_type(e.type_id) else { return };
        let (width, height) = (o.u1 - o.u0, o.z1 - o.z0);
        let reach = g.half + OPENING_CLEARANCE;

        let (placement, body, fill_frame) = match g.arc {
            // In an arc wall the opening is a sector through the wall (between
            // concentric arcs and the radial jambs); the filling is square to
            // the wall at the opening's center.
            Some(arc) => {
                let at = Frame::at(DVec3::new(0.0, 0.0, o.z0 - g.base_z));
                let placement = self.local_placement(Some(wall_placement), at);
                // Towards the center the opening stops short of it.
                let inward = reach.min(arc.radius - 1e-3);
                let vs = if arc.sweep >= 0.0 { (inward, -reach) } else { (reach, -inward) };
                let profile = self.wall_outline_profile(g, (o.u0, o.u1), (o.u0, o.u1), vs);
                let body = self.w.add("IFCEXTRUDEDAREASOLID", args![profile, self.identity, self.dir_z, height]);
                let (origin, dir, _) = g.frame_at((o.u0 + o.u1) * 0.5);
                let jamb = wall_local(g, origin + dir * o.u0);
                let x = DVec2::new(dir.dot(g.dir), dir.dot(g.normal));
                (placement, body, Frame::horizontal(DVec3::new(jamb.x, jamb.y, 0.0), x))
            }
            None => {
                let at = Frame::at(DVec3::new(o.u0, 0.0, o.z0 - g.base_z));
                let placement = self.local_placement(Some(wall_placement), at);
                let body = self.extrude_box(DVec3::new(0.0, -reach, 0.0), DVec3::new(width, reach, height));
                (placement, body, Frame::at(DVec3::ZERO))
            }
        };
        let shape = self.shape(vec![body], None);
        let gid = self.guid(&format!("opening-{}", e.id));
        let n = Value::Null;
        let opening = self.w.add(
            "IFCOPENINGELEMENT",
            args![gid, n.clone(), "Opening", n.clone(), n.clone(), placement, shape, n, Value::Enum("OPENING")],
        );
        let gid = self.guid(&format!("voids-{}", e.id));
        self.w.add("IFCRELVOIDSELEMENT", args![gid, Value::Null, Value::Null, Value::Null, wall, opening]);

        // The filling shares the opening's frame.
        let fill_placement = self.local_placement(Some(placement), fill_frame);
        let frame_style = self.style("Frame", FRAME_COLOR, 0.0);
        let mut parts = opening_parts(project, e, width, height, g.half);
        parts.retain(|p| !p.plan_only);
        // Shapes mix no representation types: with any faceted piece, all pieces are face sets.
        let tessellated = parts.iter().any(|p| !p.faces.is_empty());
        let mut items = Vec::new();
        for part in parts {
            let item = if tessellated {
                let faces = if part.faces.is_empty() {
                    let mut s = cc_geom::Solid::new(e.id, e.category(), [0; 3]);
                    s.add_prism(&part.footprint, |_| part.z0, |_| part.z1);
                    s.faces.into_iter().map(|f| f.verts).collect()
                } else {
                    part.faces
                };
                let Some(item) = self.face_set(&faces) else { continue };
                item
            } else {
                let profile = polygon::ccw(&part.footprint);
                if profile.len() < 3 || part.z1 - part.z0 < 1e-6 {
                    continue;
                }
                let position = self.placement3(Frame::at(DVec3::new(0.0, 0.0, part.z0)));
                self.extrude_polygon(&profile, position, self.dir_z, part.z1 - part.z0)
            };
            let style = if part.material.is_empty() { frame_style } else { self.material_style(&part.material) };
            self.styled(item, style);
            items.push(item);
        }
        let shape = if tessellated { self.shape_of_type(items, None, "Tessellation") } else { self.shape(items, None) };
        let (type_w, type_h) = ty.opening_size().unwrap_or((width, height));
        let mut attrs = self.element_args(e, fill_placement, shape);
        attrs.extend(args![type_h, type_w]);
        let (entity, set_name) = match e.kind {
            ElementKind::Door(_) => {
                attrs.extend(args![Value::Enum("DOOR"), Value::Enum("NOTDEFINED"), Value::Null]);
                ("IFCDOOR", "Qto_DoorBaseQuantities")
            }
            _ => {
                attrs.extend(args![Value::Enum("WINDOW"), Value::Enum("SINGLE_PANEL"), Value::Null]);
                ("IFCWINDOW", "Qto_WindowBaseQuantities")
            }
        };
        let product = self.w.add(entity, attrs);
        self.products.insert(e.id, product);
        self.register(e, product);
        let gid = self.guid(&format!("fills-{}", e.id));
        self.w.add("IFCRELFILLSELEMENT", args![gid, Value::Null, Value::Null, Value::Null, opening, product]);
        self.quantities(
            e,
            set_name,
            &[
                ("Width", Quantity::Length(type_w)),
                ("Height", Quantity::Length(type_h)),
                ("Area", Quantity::Area(type_w * type_h)),
            ],
        );
    }

    /// Profile of the part of an arc wall whose left side spans `left` and
    /// right side `right` (along the centerline), between the concentric
    /// arcs `v.0` (left) and `v.1` (right) of the centerline, in the wall
    /// placement coordinates: an `IfcIndexedPolyCurve` of two arcs and two lines.
    fn wall_outline_profile(&mut self, g: &WallGeom, left: (f64, f64), right: (f64, f64), v: (f64, f64)) -> Id {
        let pts = [
            g.world(right.0, v.1),
            g.world((right.0 + right.1) * 0.5, v.1),
            g.world(right.1, v.1),
            g.world(left.1, v.0),
            g.world((left.0 + left.1) * 0.5, v.0),
            g.world(left.0, v.0),
        ];
        let coords: Vec<Value> = pts
            .iter()
            .map(|p| {
                let q = wall_local(g, *p);
                reals(&[q.x, q.y])
            })
            .collect();
        let list = self.w.add("IFCCARTESIANPOINTLIST2D", args![coords]);
        let index = |kind: &'static str, ids: &[i64]| {
            Value::Typed(kind, Box::new(Value::List(ids.iter().map(|&i| Value::Int(i)).collect())))
        };
        let segments = vec![
            index("IFCARCINDEX", &[1, 2, 3]),
            index("IFCLINEINDEX", &[3, 4]),
            index("IFCARCINDEX", &[4, 5, 6]),
            index("IFCLINEINDEX", &[6, 1]),
        ];
        let curve = self.w.add("IFCINDEXEDPOLYCURVE", args![list, segments, false]);
        self.w.add("IFCARBITRARYCLOSEDPROFILEDEF", args![Value::Enum("AREA"), Value::Null, curve])
    }

    /// Floors, ceilings and roofs: extruded sketches.
    fn write_sketched(&mut self, id: ElementId) {
        let project = self.project;
        let Some(e) = project.element(id) else { return };
        let Some(ty) = project.element_type(e.type_id) else { return };
        let Some(storey) = self.storey_placement(e.level) else { return };
        let thickness = ty.thickness();
        if thickness < 1e-6 {
            return;
        }

        let (frame, profile, dir, depth, entity, kind) = match &e.kind {
            ElementKind::Floor(f) => {
                let z = f.height_offset - thickness;
                (Frame::at(DVec3::new(0.0, 0.0, z)), polygon::ccw(&f.boundary), DVec3::Z, thickness, "IFCSLAB", "FLOOR")
            }
            ElementKind::Ceiling(c) => {
                let z = c.height_offset;
                (
                    Frame::at(DVec3::new(0.0, 0.0, z)),
                    polygon::ccw(&c.boundary),
                    DVec3::Z,
                    thickness,
                    "IFCCOVERING",
                    "CEILING",
                )
            }
            // Pitched roofs, and any roof with openings, as their computed solid.
            ElementKind::Roof(r)
                if matches!(r.shape, RoofShape::Gable { .. } | RoofShape::Hip { .. }) || !r.holes.is_empty() =>
            {
                self.write_pitched_roof(e, r, storey);
                return;
            }
            ElementKind::Roof(r) => {
                let fp = polygon::ccw(&polygon::offset(&r.footprint, r.overhang));
                match r.shape {
                    RoofShape::Flat => (
                        Frame::at(DVec3::new(0.0, 0.0, r.base_offset)),
                        fp,
                        DVec3::Z,
                        thickness,
                        "IFCROOF",
                        "FLAT_ROOF",
                    ),
                    RoofShape::Gable { .. } | RoofShape::Hip { .. } => return,
                    RoofShape::Shed { slope, direction_deg } => {
                        // The profile lies on the sloped underside; the extrusion is vertical,
                        // which is oblique in the profile's frame.
                        let a = direction_deg.to_radians();
                        let d = DVec2::new(a.cos(), a.sin());
                        let across = d.perp();
                        let min = fp.iter().map(|p| p.dot(d)).fold(f64::INFINITY, f64::min);
                        let stretch = (1.0 + slope * slope).sqrt();
                        let c = 1.0 / stretch;
                        let frame = Frame {
                            origin: DVec3::new(0.0, 0.0, r.base_offset - slope * min),
                            x: DVec3::new(d.x * c, d.y * c, slope * c),
                            z: DVec3::new(-slope * c * d.x, -slope * c * d.y, c),
                        };
                        let profile = fp.iter().map(|p| DVec2::new(p.dot(d) * stretch, p.dot(across))).collect();
                        (frame, profile, DVec3::new(slope * c, 0.0, c), thickness * stretch, "IFCROOF", "SHED_ROOF")
                    }
                }
            }
            _ => return,
        };
        if profile.len() < 3 {
            return;
        }
        let (placement, shape) = if self.cut_by_join(id) {
            // Joined elements cut it: the body as it remains.
            let placement = self.local_placement(Some(storey), Frame::at(DVec3::ZERO));
            let items = self.cut_body(id, project.level_elevation(e.level));
            let style = self.material_style(ty.main_material());
            items.iter().for_each(|&item| self.styled(item, style));
            (placement, self.shape_of_type(items, None, "Brep"))
        } else {
            let placement = self.local_placement(Some(storey), frame);
            let dir = self.direction(dir);
            // Floor openings: voids of the profile, clockwise.
            let voids: Vec<Vec<DVec2>> = match &e.kind {
                ElementKind::Floor(f) => f
                    .openings
                    .iter()
                    .map(|o| {
                        let mut o = polygon::ccw(o);
                        o.reverse();
                        o
                    })
                    .filter(|o| o.len() >= 3)
                    .collect(),
                _ => Vec::new(),
            };
            let body = self.extrude_polygon_with_voids(&profile, &voids, self.identity, dir, depth);
            let style = self.material_style(ty.main_material());
            self.styled(body, style);
            (placement, self.shape(vec![body], None))
        };
        let mut attrs = self.element_args(e, placement, shape);
        attrs.push(Value::Enum(kind));
        let product = self.w.add(entity, attrs);
        self.products.insert(e.id, product);
        self.register(e, product);

        let volume = self.geom.volume(e.id);
        match &e.kind {
            ElementKind::Floor(f) => {
                // Layers are listed top first: start at the top face and go down.
                self.layer_usage(e.type_id, product, "AXIS3", thickness);
                let area = f.area();
                self.quantities(
                    e,
                    "Qto_SlabBaseQuantities",
                    &[
                        ("Width", Quantity::Length(thickness)),
                        ("Perimeter", Quantity::Length(polygon::perimeter(&f.boundary))),
                        ("NetArea", Quantity::Area(area)),
                        ("NetVolume", Quantity::Volume(volume)),
                    ],
                );
            }
            ElementKind::Ceiling(c) => {
                let area = polygon::area(&c.boundary);
                self.quantities(
                    e,
                    "Qto_CoveringBaseQuantities",
                    &[("Width", Quantity::Length(thickness)), ("NetArea", Quantity::Area(area))],
                );
            }
            ElementKind::Roof(r) => {
                let projected = cc_geom::roofs::plan_area(r);
                let slope = match r.shape {
                    RoofShape::Flat => 0.0,
                    RoofShape::Shed { slope, .. } | RoofShape::Gable { slope, .. } | RoofShape::Hip { slope } => slope,
                };
                let gross = projected * (1.0 + slope * slope).sqrt();
                self.quantities(
                    e,
                    "Qto_RoofBaseQuantities",
                    &[("GrossArea", Quantity::Area(gross)), ("ProjectedArea", Quantity::Area(projected))],
                );
                if let TypeData::Roof { covering, framing, .. } = &ty.data {
                    let level_z = project.level_elevation(e.level);
                    self.write_roof_framing(e, product, storey, level_z, *covering, framing);
                }
            }
            _ => {}
        }
    }

    /// Gable and hip roofs (and roofs with openings) as a faceted B-rep of the computed roof solid.
    fn write_pitched_roof(&mut self, e: &Element, r: &cc_core::Roof, storey: Id) {
        let project = self.project;
        let geom = self.geom;
        let Some(ty) = project.element_type(e.type_id) else { return };
        let level_z = project.level_elevation(e.level);
        let faces: Vec<cc_geom::Face> = geom.body_of(e.id).flat_map(|s| s.faces.iter().cloned()).collect();
        if faces.is_empty() {
            return;
        }
        let placement = self.local_placement(Some(storey), Frame::at(DVec3::ZERO));
        let body = self.faceted_brep(&faces, level_z);
        let style = self.material_style(ty.main_material());
        self.styled(body, style);
        let shape = self.shape_of_type(vec![body], None, "Brep");
        let kind = match r.shape {
            RoofShape::Hip { .. } => "HIP_ROOF",
            RoofShape::Gable { .. } => "GABLE_ROOF",
            RoofShape::Shed { .. } => "SHED_ROOF",
            RoofShape::Flat => "FLAT_ROOF",
        };
        let mut attrs = self.element_args(e, placement, shape);
        attrs.push(Value::Enum(kind));
        let product = self.w.add("IFCROOF", attrs);
        self.products.insert(e.id, product);
        self.register(e, product);
        let projected = cc_geom::roofs::plan_area(r);
        let slope = match r.shape {
            RoofShape::Gable { slope, .. } | RoofShape::Hip { slope } | RoofShape::Shed { slope, .. } => slope,
            RoofShape::Flat => 0.0,
        };
        self.quantities(
            e,
            "Qto_RoofBaseQuantities",
            &[
                ("GrossArea", Quantity::Area(projected * (1.0 + slope * slope).sqrt())),
                ("ProjectedArea", Quantity::Area(projected)),
            ],
        );
        if let TypeData::Roof { covering, framing, .. } = &ty.data {
            self.write_roof_framing(e, product, storey, level_z, *covering, framing);
        }
    }

    /// A roof's covering and framing: the type's choices in [`ROOFING_PSET`]
    /// (so importers can restore them) and each framing member as an
    /// `IfcMember` (purlin, rafter, batten, or chord, post and strut of a
    /// truss) aggregated in the roof, as a B-rep in the framing's material.
    fn write_roof_framing(
        &mut self,
        e: &Element,
        roof: Id,
        storey: Id,
        level_z: f64,
        covering: Option<cc_core::roofing::TileKind>,
        framing: &cc_core::roofing::RoofFraming,
    ) {
        use cc_core::roofing::FramingSystem;
        use cc_geom::roof_framing::{MemberKind, TrussPart};
        if covering.is_none() && framing.system == FramingSystem::None {
            return;
        }
        let label = |s: &str| Value::Typed("IFCLABEL", Box::new(s.into()));
        let length = |v: f64| Value::Typed("IFCLENGTHMEASURE", Box::new(Value::Real(v)));
        let mut props = vec![("FramingSystem", label(framing.system.code()))];
        if let Some(c) = covering {
            props.push(("Covering", label(c.code())));
        }
        if !framing.material.is_empty() {
            props.push(("FramingMaterial", label(&framing.material)));
        }
        let sizes = [framing.purlin, framing.rafter, framing.batten, framing.truss];
        for ((_, w, h), size) in ROOFING_SIZES.iter().zip(sizes) {
            props.push((w, length(size.width)));
            props.push((h, length(size.height)));
        }
        props.push(("PurlinSpacing", length(framing.purlin_spacing)));
        props.push(("RafterSpacing", length(framing.rafter_spacing)));
        props.push(("TrussSpacing", length(framing.truss_spacing)));
        self.property_set(&format!("roofing-{}", e.id), ROOFING_PSET, vec![roof], props);

        let geom = self.geom;
        let Some(g) = geom.roof_framing.get(&e.id) else { return };
        if g.members.is_empty() {
            return;
        }
        let style = self.material_style(&framing.material);
        let mut members = Vec::with_capacity(g.members.len());
        for (i, m) in g.members.iter().enumerate() {
            let (name, kind, object_type) = match (m.kind, m.truss) {
                (MemberKind::Purlin, _) => ("Purlin", "PURLIN", None),
                (MemberKind::Rafter, _) => ("Rafter", "RAFTER", None),
                (MemberKind::Batten, _) => ("Batten", "USERDEFINED", Some("BATTEN")),
                (MemberKind::Truss, Some(TrussPart::Post)) => ("King Post", "POST", None),
                (MemberKind::Truss, Some(TrussPart::Strut)) => ("Strut", "STRUT", None),
                (MemberKind::Truss, Some(TrussPart::Tie)) => ("Tie", "CHORD", None),
                (MemberKind::Truss, _) => ("Top Chord", "CHORD", None),
            };
            let placement = self.local_placement(Some(storey), Frame::at(DVec3::ZERO));
            let body = self.faceted_brep(&m.faces(), level_z);
            self.styled(body, style);
            let shape = self.shape_of_type(vec![body], None, "Brep");
            let gid = self.guid(&format!("framing-{}-{i}", e.id));
            let n = Value::Null;
            let object_type = object_type.map_or(n.clone(), |t| t.into());
            let member = self.w.add(
                "IFCMEMBER",
                args![
                    gid,
                    n.clone(),
                    name,
                    n.clone(),
                    object_type.clone(),
                    placement,
                    shape,
                    n.clone(),
                    Value::Enum(kind)
                ],
            );
            members.push(member);
            let key = (e.type_id, name);
            if !self.member_types.contains_key(&key) {
                let gid = self.guid(&format!("member-type-{}-{name}", e.type_id));
                let ty_name = format!("{} - {name}", self.project.type_name(e.type_id));
                let t = self.w.add(
                    "IFCMEMBERTYPE",
                    args![
                        gid,
                        n.clone(),
                        ty_name,
                        n.clone(),
                        n.clone(),
                        n.clone(),
                        n.clone(),
                        n.clone(),
                        object_type.clone(),
                        Value::Enum(kind)
                    ],
                );
                self.member_types.insert(key, (t, Vec::new()));
            }
            if let Some((_, objects)) = self.member_types.get_mut(&key) {
                objects.push(member);
            }
        }
        if !framing.material.is_empty() {
            let material = self.material(&framing.material);
            self.associate_material(&format!("mat-framing-{}", e.id), members.clone(), material);
        }
        self.aggregate(&format!("agg-framing-{}", e.id), roof, members);
    }

    // --- Relationships collected while writing elements ---

    fn write_relationships(&mut self) {
        let n = Value::Null;
        for (type_id, objects) in std::mem::take(&mut self.typed) {
            let Some(&ty) = self.types.get(&type_id) else { continue };
            let gid = self.guid(&format!("deftype-{type_id}"));
            self.w.add("IFCRELDEFINESBYTYPE", args![gid, n.clone(), n.clone(), n.clone(), objects, ty]);
        }
        for ((type_id, name), (ty, objects)) in std::mem::take(&mut self.member_types) {
            let gid = self.guid(&format!("deftype-member-{type_id}-{name}"));
            self.w.add("IFCRELDEFINESBYTYPE", args![gid, n.clone(), n.clone(), n.clone(), objects, ty]);
        }
        for (i, (usage, objects)) in std::mem::take(&mut self.usage_objects).into_iter().enumerate() {
            self.associate_material(&format!("mat-usage-{i}"), objects, usage);
        }
        for (level, objects) in std::mem::take(&mut self.contained) {
            let Some(&(storey, _)) = self.storeys.get(&level) else { continue };
            let gid = self.guid(&format!("contains-{level}"));
            self.w
                .add("IFCRELCONTAINEDINSPATIALSTRUCTURE", args![gid, n.clone(), n.clone(), n.clone(), objects, storey]);
        }
    }
}

/// An IFC product written by `write_structure`: entity, predefined type, body
/// items and optional base quantities and property set.
struct StructureProduct {
    entity: &'static str,
    kind: &'static str,
    items: Vec<Id>,
    /// Representation type of the items: `SweptSolid` or `Brep`.
    representation: &'static str,
    quantities: Option<(&'static str, Vec<(&'static str, Quantity)>)>,
    props: Option<(&'static str, Vec<(&'static str, Value)>)>,
}

impl StructureProduct {
    fn new(entity: &'static str, kind: &'static str, items: Vec<Id>) -> Self {
        Self { entity, kind, items, representation: "SweptSolid", quantities: None, props: None }
    }

    fn quantities(mut self, set: &'static str, quantities: Vec<(&'static str, Quantity)>) -> Self {
        self.quantities = Some((set, quantities));
        self
    }

    fn brep(mut self) -> Self {
        self.representation = "Brep";
        self
    }
}

#[derive(Debug, Clone, Copy)]
enum Quantity {
    Length(f64),
    Area(f64),
    Volume(f64),
}

/// The center line a bar is swept along. A closed stirrup starts in the
/// middle of its first side and stops one diameter short of it, so the swept
/// disk never meets itself.
fn directrix(bar: &cc_geom::rebar::Bar, diameter: f64) -> Vec<DVec3> {
    let pts = &bar.points;
    if !bar.closed || pts.len() < 2 {
        return pts.clone();
    }
    let (a, b) = (pts[0], pts[1]);
    let mid = (a + b) * 0.5;
    let back = (a - b).normalize_or_zero() * diameter.min(a.distance(b) * 0.25);
    std::iter::once(mid).chain(pts[1..].iter().copied()).chain([a, mid + back]).collect()
}

/// Area of a column section as written: exact circles, profiles as their parts.
fn section_area(section: cc_core::ColumnSection) -> f64 {
    use std::f64::consts::PI;
    match section {
        cc_core::ColumnSection::Rectangular { width, depth } => width * depth,
        cc_core::ColumnSection::Round { diameter } => PI * diameter * diameter / 4.0,
        cc_core::ColumnSection::Pipe { diameter, wall } => {
            let inner = (diameter / 2.0 - wall).max(0.0);
            PI * (diameter * diameter / 4.0 - inner * inner)
        }
        cc_core::ColumnSection::Profile { width, depth, shape } => {
            cc_geom::section::shape_rects(shape, width, depth).iter().map(|(a, b)| (b.x - a.x) * (b.y - a.y)).sum()
        }
    }
}

/// Property set with the instance parameter values of components.
pub const COMPONENT_PSET: &str = "ConstructCAD_Parameters";
/// Property set of an element's construction phases (`PhaseCreated`, `PhaseDemolished`).
pub const PHASING_PSET: &str = "ConstructCAD_Phasing";
/// Property set of a roof's covering and framing (see `write_roof_framing`).
pub const ROOFING_PSET: &str = "ConstructCAD_Roofing";
/// The framing's member sizes in [`ROOFING_PSET`]: member, width and height properties.
pub const ROOFING_SIZES: [(&str, &str, &str); 4] = [
    ("Purlin", "PurlinWidth", "PurlinHeight"),
    ("Rafter", "RafterWidth", "RafterHeight"),
    ("Batten", "BattenWidth", "BattenHeight"),
    ("Truss", "TrussWidth", "TrussHeight"),
];
/// Property set of the project's phases, in time order (properties `1`, `2`…).
pub const PHASES_PSET: &str = "ConstructCAD_Phases";

/// An element's IFC `Status` relative to the project's last phase (an
/// element demolished before it reads as demolished); `None` for
/// annotations and projects without phases.
pub fn ifc_status(project: &Project, e: &Element) -> Option<cc_core::PhaseStatus> {
    let last = project.phases.len().checked_sub(1)?;
    match project.phase_state_at(e, last) {
        cc_core::PhaseState::Status(st) => Some(st),
        cc_core::PhaseState::Absent => Some(cc_core::PhaseStatus::Demolished),
        cc_core::PhaseState::Unphased => None,
    }
}

/// The IFC4 common property set holding the `Status` of an element's
/// class (building elements as this program writes them; components and
/// building services keep theirs in [`PHASING_PSET`] only).
fn common_pset(project: &Project, e: &Element) -> Option<&'static str> {
    Some(match &e.kind {
        ElementKind::Wall(_) => "Pset_WallCommon",
        ElementKind::Door(_) => "Pset_DoorCommon",
        ElementKind::Window(_) => "Pset_WindowCommon",
        ElementKind::Floor(_) => "Pset_SlabCommon",
        ElementKind::Ceiling(_) => "Pset_CoveringCommon",
        ElementKind::Roof(_) => "Pset_RoofCommon",
        ElementKind::Column(_) => "Pset_ColumnCommon",
        ElementKind::Beam(_) => "Pset_BeamCommon",
        ElementKind::Stair(_) => "Pset_StairCommon",
        ElementKind::Ramp(_) => "Pset_RampCommon",
        ElementKind::Railing(_) => "Pset_RailingCommon",
        ElementKind::Foundation(_) => match project.element_type(e.type_id).map(|t| &t.data) {
            Some(TypeData::Foundation { kind: cc_core::FoundationKind::Slab, .. }) => "Pset_SlabCommon",
            _ => "Pset_FootingCommon",
        },
        _ => return None,
    })
}

/// IFC entity and predefined type of a component kind.
/// How a component is written: its stored IFC class when the schema table
/// knows it, a proxy naming the class otherwise, or a class from its kind.
struct WrittenClass {
    entity: &'static str,
    attrs: usize,
    predefined: Option<usize>,
    /// Predefined type to write (empty: `NOTDEFINED`).
    value: String,
    object_type: String,
    type_entity: Option<(&'static str, TypeLayout)>,
}

impl WrittenClass {
    fn predefined_value(&self) -> Value {
        // A user-defined type must name itself in the object type.
        if self.value.is_empty() || (self.value == "USERDEFINED" && self.object_type.is_empty()) {
            Value::Enum("NOTDEFINED")
        } else {
            Value::EnumName(self.value.clone())
        }
    }
}

fn component_class(def: &cc_core::ComponentDef) -> WrittenClass {
    let written = |class: &ElementClass, value: &str, object_type: &str| WrittenClass {
        entity: class.entity,
        attrs: class.attrs,
        predefined: class.predefined,
        value: value.to_string(),
        object_type: object_type.to_string(),
        type_entity: class.type_entity,
    };
    let proxy = element_class("IFCBUILDINGELEMENTPROXY");
    match (&def.ifc, proxy) {
        (Some(ifc), _) if let Some(class) = element_class(&ifc.entity) => {
            written(class, &ifc.predefined, &ifc.object_type)
        }
        (Some(ifc), Some(proxy)) => written(proxy, "USERDEFINED", &crate::schema::display_name(&ifc.entity)),
        _ => {
            let (entity, kind) = component_entity(def);
            match element_class(entity) {
                Some(class) => written(class, kind, ""),
                None => WrittenClass {
                    entity,
                    attrs: 9,
                    predefined: Some(8),
                    value: kind.to_string(),
                    object_type: String::new(),
                    type_entity: None,
                },
            }
        }
    }
}

/// An imported value with its IFC type (a value whose type name is unusable
/// becomes a label holding its text).
fn typed_value(v: &cc_core::properties::IfcValue) -> Value {
    use cc_core::properties::Atom;
    let inner = match &v.value {
        Atom::Text(s) => Value::Str(s.clone()),
        Atom::Real(x) if x.is_finite() => Value::Real(*x),
        Atom::Real(_) => return Value::Null,
        Atom::Integer(i) => Value::Int(*i),
        Atom::Bool(b) => Value::Bool(*b),
        Atom::Enum(e) => Value::EnumName(e.clone()),
    };
    if crate::step::is_type_name(&v.measure) {
        Value::TypedName(v.measure.clone(), Box::new(inner))
    } else {
        let text = match &v.value {
            Atom::Text(s) | Atom::Enum(s) => s.clone(),
            Atom::Real(x) => x.to_string(),
            Atom::Integer(i) => i.to_string(),
            Atom::Bool(b) => b.to_string(),
        };
        Value::Typed("IFCLABEL", Box::new(Value::Str(text)))
    }
}

/// IFC entity and predefined type of a native component: by its device
/// kind for electrical devices, else by its component kind.
fn component_entity(def: &cc_core::ComponentDef) -> (&'static str, &'static str) {
    use cc_core::DeviceKind as D;
    if let Some(device) = def.mep.and_then(|m| m.device()) {
        return match device {
            D::Outlet | D::DedicatedOutlet => ("IFCOUTLET", "POWEROUTLET"),
            D::Switch => ("IFCSWITCHINGDEVICE", "TOGGLESWITCH"),
            D::CeilingLight | D::WallLight => ("IFCLIGHTFIXTURE", "POINTSOURCE"),
            D::Panel => ("IFCELECTRICDISTRIBUTIONBOARD", "DISTRIBUTIONBOARD"),
        };
    }
    match def.kind {
        cc_core::ComponentKind::Furniture | cc_core::ComponentKind::Casework => ("IFCFURNITURE", "NOTDEFINED"),
        cc_core::ComponentKind::Plumbing => ("IFCSANITARYTERMINAL", "NOTDEFINED"),
        cc_core::ComponentKind::Lighting => ("IFCLIGHTFIXTURE", "NOTDEFINED"),
        cc_core::ComponentKind::Electrical => ("IFCBUILDINGELEMENTPROXY", "NOTDEFINED"),
        cc_core::ComponentKind::Generic => ("IFCBUILDINGELEMENTPROXY", "NOTDEFINED"),
        cc_core::ComponentKind::Door => ("IFCDOOR", "DOOR"),
        cc_core::ComponentKind::Window => ("IFCWINDOW", "WINDOW"),
    }
}

/// Current UTC time as `YYYY-MM-DDThh:mm:ss`.
fn now_utc() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970-01-01 to civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::Category;
    use std::collections::HashSet;

    fn sample() -> Project {
        cc_core::from_ron(include_str!("../../../examples/casa_simples.ccad")).unwrap()
    }

    fn export_sample() -> String {
        let options = ExportOptions { time_stamp: Some("2026-01-01T00:00:00".into()), file_name: "casa.ifc".into() };
        export(&sample(), &options)
    }

    fn count(text: &str, entity: &str) -> usize {
        text.matches(&format!("={entity}(")).count()
    }

    #[test]
    fn exports_every_element() {
        let project = sample();
        let text = export_sample();
        assert!(text.contains("FILE_SCHEMA(('IFC4'));"));
        assert!(text.contains("FILE_NAME('casa.ifc','2026-01-01T00:00:00',"));
        assert_eq!(count(&text, "IFCPROJECT"), 1);
        assert_eq!(count(&text, "IFCBUILDINGSTOREY"), project.levels.len());
        assert_eq!(count(&text, "IFCWALL"), project.elements_of(Category::Wall).count());
        let doors = project.elements_of(Category::Door).count();
        let windows = project.elements_of(Category::Window).count();
        assert!(doors > 0 && windows > 0);
        assert_eq!(count(&text, "IFCDOOR"), doors);
        assert_eq!(count(&text, "IFCWINDOW"), windows);
        assert_eq!(count(&text, "IFCOPENINGELEMENT"), doors + windows);
        assert_eq!(count(&text, "IFCRELFILLSELEMENT"), doors + windows);
        assert_eq!(count(&text, "IFCSLAB"), project.elements_of(Category::Floor).count());
        assert_eq!(count(&text, "IFCCOVERING"), project.elements_of(Category::Ceiling).count());
        assert_eq!(count(&text, "IFCROOF"), project.elements_of(Category::Roof).count());
        assert_eq!(count(&text, "IFCSPACE"), project.elements_of(Category::Room).count());
    }

    #[test]
    fn references_are_defined_and_ids_unique() {
        const ROOTED: &[&str] = &[
            "IFCPROJECT",
            "IFCSITE",
            "IFCBUILDING",
            "IFCBUILDINGSTOREY",
            "IFCWALL",
            "IFCOPENINGELEMENT",
            "IFCDOOR",
            "IFCWINDOW",
            "IFCSLAB",
            "IFCCOVERING",
            "IFCROOF",
            "IFCELEMENTQUANTITY",
        ];
        let text = export_sample();
        let data = &text[text.find("DATA;\n").unwrap()..];
        let mut defined = HashSet::new();
        let mut guids = HashSet::new();
        for line in data.lines().filter(|l| l.starts_with('#')) {
            let (name, rest) = line.split_once('=').unwrap();
            assert!(defined.insert(name.to_string()), "duplicate {name}");
            let (entity, args) = rest.split_once('(').unwrap();
            let rooted = entity.starts_with("IFCREL") || entity.ends_with("TYPE") || ROOTED.contains(&entity);
            if rooted {
                let gid = &args[1..23];
                assert_eq!(&args[23..24], "'", "{line}");
                assert!(guids.insert(gid.to_string()), "duplicate GlobalId {gid} in {line}");
            }
        }
        for line in data.lines().filter(|l| l.starts_with('#')) {
            let body = line.split_once('=').unwrap().1;
            let bytes = body.as_bytes();
            let mut i = 0;
            let mut in_str = false;
            while i < bytes.len() {
                match bytes[i] {
                    b'\'' => in_str = !in_str,
                    b'#' if !in_str => {
                        let end = (i + 1..bytes.len()).find(|&j| !bytes[j].is_ascii_digit()).unwrap_or(bytes.len());
                        let name = &body[i..end];
                        assert!(defined.contains(name), "undefined {name} in {line}");
                        i = end;
                        continue;
                    }
                    _ => {}
                }
                i += 1;
            }
        }
    }

    #[test]
    fn global_ids_come_from_persistent_ids() {
        let project = sample();
        let text = export(&project, &ExportOptions::default());
        let wall = project.elements_of(Category::Wall).next().unwrap();
        assert!(text.contains(&format!("IFCWALL('{}'", encode(wall.uid.0))));
        assert!(text.contains(&format!("IFCPROJECT('{}'", encode(project.info.uid.0))));
        assert!(text.contains("'Pset_WallCommon'"));
    }

    #[test]
    fn export_is_deterministic() {
        assert_eq!(export_sample(), export_sample());
    }

    #[test]
    fn non_ascii_names_are_encoded() {
        let text = export_sample();
        assert!(text.is_ascii());
        // "Térreo", the ground level of the sample.
        assert!(text.contains(r"'T\X2\00E9\X0\rreo'"));
    }

    #[test]
    fn time_stamp_format() {
        let ts = now_utc();
        assert_eq!(ts.len(), 19);
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[10..11], "T");
    }
}
