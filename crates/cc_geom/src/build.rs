use crate::beam::{BeamGeom, BeamInput, ColumnInput, resolve_beams};
use crate::polygon;
use crate::wall::{Opening, WallGeom, WallLine, resolve_join};
use crate::{Ray, Solid};
use cc_core::{Category, ElementId, ElementKind, Project, RoofShape, TypeData};
use glam::{DVec2, DVec3};
use std::collections::BTreeMap;

const FRAME: f64 = 0.05;
/// Shading color of door and window frames.
pub const FRAME_COLOR: [u8; 3] = [236, 236, 232];

/// One box of a door or window, in opening-local coordinates: `x` along the
/// wall from the start jamb, `y` across the wall (positive on its left), `z`
/// up from the sill.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HostedPart {
    pub min: DVec3,
    pub max: DVec3,
    /// Leaf or glazing (colored with the type material) rather than frame.
    pub panel: bool,
}

/// The pieces of the door or window `element` (of `width` x `height`, in a
/// wall of half thickness `half`) in opening-local coordinates like
/// [`HostedPart`]s: `x` along the wall from the start jamb, `y` across it
/// (positive on its left), `z` up from the sill. A door or window component
/// is resolved with the wall's thickness, centered in the opening and
/// mirrored by the element's flips; other types give their boxes, frames
/// with no material and panels of the type's material.
pub fn opening_parts(
    project: &Project,
    element: &cc_core::Element,
    width: f64,
    height: f64,
    half: f64,
) -> Vec<cc_core::component::ComponentPart> {
    let Some(ty) = project.element_type(element.type_id) else { return Vec::new() };
    let TypeData::Component { def, .. } = &ty.data else {
        return hosted_parts(&ty.data, width, height, half)
            .into_iter()
            .map(|p| cc_core::component::ComponentPart {
                item: 0,
                footprint: vec![
                    DVec2::new(p.min.x, p.min.y),
                    DVec2::new(p.max.x, p.min.y),
                    DVec2::new(p.max.x, p.max.y),
                    DVec2::new(p.min.x, p.max.y),
                ],
                z0: p.min.z,
                z1: p.max.z,
                material: if p.panel { ty.main_material().to_string() } else { String::new() },
                in_plan: false,
                void: false,
                plan_only: false,
                detail: Default::default(),
                faces: Vec::new(),
            })
            .collect();
    };
    let thickness = [(cc_core::component::WALL_THICKNESS, 2.0 * half)];
    let parts = crate::resolve_component_in(def, &Default::default(), &thickness);
    let (fx, fy) = match element.as_hosted() {
        Some(h) => (if h.flip_hand { -1.0 } else { 1.0 }, if h.flip_facing { -1.0 } else { 1.0 }),
        None => (1.0, 1.0),
    };
    let map = |p: DVec2| DVec2::new(p.x * fx + width * 0.5, p.y * fy);
    // One mirror turns faces inside out: reverse them.
    let mirrored = fx * fy < 0.0;
    parts
        .into_iter()
        .map(|mut p| {
            p.footprint = p.footprint.iter().map(|q| map(*q)).collect();
            for face in &mut p.faces {
                for v in face.iter_mut() {
                    *v = map(v.truncate()).extend(v.z);
                }
                if mirrored {
                    face.reverse();
                }
            }
            if mirrored {
                p.footprint.reverse();
            }
            p
        })
        .collect()
}

/// The boxes making up a door or window of `width` x `height` in a wall of
/// half thickness `half`. Empty for non-hosted types.
pub fn hosted_parts(data: &TypeData, width: f64, height: f64, half: f64) -> Vec<HostedPart> {
    let part = |x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64, panel: bool| HostedPart {
        min: DVec3::new(x0, y0, z0),
        max: DVec3::new(x1, y1, z1),
        panel,
    };
    let (w, h) = (width, height);
    match data {
        TypeData::Door { .. } => vec![
            part(0.0, FRAME, -half, half, 0.0, h, false),
            part(w - FRAME, w, -half, half, 0.0, h, false),
            part(FRAME, w - FRAME, -half, half, h - FRAME, h, false),
            part(FRAME, w - FRAME, -0.02, 0.02, 0.0, h - FRAME, true),
        ],
        TypeData::Window { .. } => {
            let d = 0.04;
            vec![
                part(0.0, FRAME, -d, d, 0.0, h, false),
                part(w - FRAME, w, -d, d, 0.0, h, false),
                part(FRAME, w - FRAME, -d, d, 0.0, FRAME, false),
                part(FRAME, w - FRAME, -d, d, h - FRAME, h, false),
                // Sill board spanning the wall depth.
                part(0.0, w, -half - 0.02, half, -0.02, 0.0, false),
                part(FRAME, w - FRAME, -0.006, 0.006, FRAME, h - FRAME, true),
            ]
        }
        _ => Vec::new(),
    }
}

/// All derived geometry of a project.
#[derive(Debug, Clone, Default)]
pub struct ModelGeometry {
    pub solids: Vec<Solid>,
    pub walls: BTreeMap<ElementId, WallGeom>,
    /// Beams with their ends framed into columns and other beams.
    pub beams: BTreeMap<ElementId, BeamGeom>,
    /// Reinforcement sets: their bars, placed in their hosts.
    pub rebar: BTreeMap<ElementId, crate::rebar::RebarGeom>,
    /// Pipes and conduits: their center lines and diameters.
    pub segments: BTreeMap<ElementId, crate::mep::SegmentGeom>,
    /// Where pipes and conduits meet: nodes, fittings and attached fixtures and devices.
    pub mep: crate::mep_net::MepNetwork,
    /// Foundations: plan outline and elevations (strips meet at corners like beams).
    pub foundations: BTreeMap<ElementId, FoundationGeom>,
    /// Boundary of each enclosed room (missing when the room is not enclosed).
    pub rooms: BTreeMap<ElementId, Vec<DVec2>>,
    /// Ridge and hip lines of pitched roofs, in plan.
    pub roof_ridges: BTreeMap<ElementId, Vec<(DVec2, DVec2)>>,
    /// Volume a joined wall shares with the elements that cut it, taken
    /// off its quantity (a wall's body stays whole; see [`Self::volume`]).
    pub join_deductions: BTreeMap<ElementId, f64>,
    /// Solid pieces of each placed component in the world, voids cut out.
    pub components: BTreeMap<ElementId, Vec<cc_core::component::ComponentPart>>,
    /// Pieces of each door and window in the world (see [`opening_parts`]).
    pub opening_fills: BTreeMap<ElementId, Vec<cc_core::component::ComponentPart>>,
    /// Conceptual masses: body, volume and mass floors.
    pub masses: BTreeMap<ElementId, crate::mass::MassGeom>,
    /// Roofs: their framing members and covering figures.
    pub roof_framing: BTreeMap<ElementId, crate::roof_framing::RoofFramingGeom>,
}

/// A foundation's body: a vertical prism.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundationGeom {
    pub element: ElementId,
    /// Plan outline, counter-clockwise.
    pub footprint: Vec<DVec2>,
    /// Bottom and top elevations.
    pub z_range: (f64, f64),
}

impl FoundationGeom {
    pub fn volume(&self) -> f64 {
        polygon::area(&self.footprint) * (self.z_range.1 - self.z_range.0)
    }
}

/// An axis-aligned box in a horizontal frame: `origin`, plan axes `u`/`v`
/// (unit, perpendicular) and extents along them and in absolute Z.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxPart {
    pub origin: DVec2,
    pub u: DVec2,
    pub v: DVec2,
    pub u_range: (f64, f64),
    pub v_range: (f64, f64),
    pub z_range: (f64, f64),
}

impl BoxPart {
    pub fn add_to(&self, s: &mut Solid) {
        let (u, v, z) = (self.u_range, self.v_range, self.z_range);
        s.add_box(self.origin, self.u, self.v, u.0, u.1, v.0, v.1, z.0, z.1);
    }
}

/// Plan outline of a column section's bounding shape (counter-clockwise):
/// its box, or its circle for round sections.
pub fn column_outline(section: cc_core::ColumnSection, position: DVec2, rotation_deg: f64) -> Vec<DVec2> {
    let u = DVec2::from_angle(rotation_deg.to_radians());
    let v = u.perp();
    match section {
        cc_core::ColumnSection::Rectangular { width, depth } | cc_core::ColumnSection::Profile { width, depth, .. } => {
            let (w, d) = (width / 2.0, depth / 2.0);
            [(-w, -d), (w, -d), (w, d), (-w, d)].iter().map(|&(a, b)| position + u * a + v * b).collect()
        }
        cc_core::ColumnSection::Round { diameter } | cc_core::ColumnSection::Pipe { diameter, .. } => (0..24)
            .map(|i| position + DVec2::from_angle(i as f64 / 24.0 * std::f64::consts::TAU) * (diameter / 2.0))
            .collect(),
    }
}

/// Solid pieces of a placed component in world coordinates (voids cut
/// out): footprints rotated and moved to the insertion point, elevations
/// absolute. Pieces whose expressions fail are left out (the component
/// editor reports them). [`ModelGeometry::components`] holds them for every
/// component of the model.
pub fn component_parts(
    project: &Project,
    element: &cc_core::Element,
    component: &cc_core::Component,
) -> Vec<cc_core::component::ComponentPart> {
    let Some(TypeData::Component { def, .. }) = project.element_type(element.type_id).map(|t| &t.data) else {
        return Vec::new();
    };
    place_parts(project, element, component, crate::resolve_component(def, &component.overrides))
}

/// A component's connector in the world: where a run joins it (`point`, its
/// z the absolute elevation) and the way the run leaves (`direction`).
#[derive(Debug, Clone, PartialEq)]
pub struct WorldConnector {
    pub index: usize,
    pub name: String,
    pub system: cc_core::SystemKind,
    pub point: DVec3,
    pub direction: DVec3,
    /// Whether the run leaves vertically (down or up).
    pub vertical: bool,
    pub diameter: Option<f64>,
}

/// The connectors of a placed component, in the world.
pub fn component_connectors(project: &Project, element: &cc_core::Element) -> Vec<WorldConnector> {
    let ElementKind::Component(c) = &element.kind else { return Vec::new() };
    let Some(TypeData::Component { def, .. }) = project.element_type(element.type_id).map(|t| &t.data) else {
        return Vec::new();
    };
    let rot = DVec2::from_angle(c.rotation_deg.to_radians());
    let z = project.level_elevation(element.level) + c.base_offset;
    def.resolve_connectors(&c.overrides)
        .into_iter()
        .map(|k| {
            let d = k.direction.vector();
            let plan = c.position + rot.rotate(k.point.truncate());
            WorldConnector {
                index: k.index,
                name: k.name,
                system: k.system,
                point: plan.extend(z + k.point.z),
                direction: rot.rotate(d.truncate()).extend(d.z),
                vertical: k.direction.is_vertical(),
                diameter: k.diameter,
            }
        })
        .filter(|k| k.point.is_finite())
        .collect()
}

/// Pieces in a component's frame moved to where `component` stands.
fn place_parts(
    project: &Project,
    element: &cc_core::Element,
    component: &cc_core::Component,
    parts: Vec<cc_core::component::ComponentPart>,
) -> Vec<cc_core::component::ComponentPart> {
    let rot = DVec2::from_angle(component.rotation_deg.to_radians());
    let z = project.level_elevation(element.level) + component.base_offset;
    parts
        .into_iter()
        .map(|mut p| {
            p.footprint = p.footprint.iter().map(|q| component.position + rot.rotate(*q)).collect();
            p.z0 += z;
            p.z1 += z;
            for face in &mut p.faces {
                for v in face.iter_mut() {
                    *v = (component.position + rot.rotate(v.truncate())).extend(v.z + z);
                }
            }
            p
        })
        .filter(|p| p.footprint.iter().all(|q| q.is_finite()) && p.z0.is_finite() && p.z1.is_finite())
        .collect()
}

impl ModelGeometry {
    pub fn build(project: &Project) -> Self {
        let mut geom = Self::default();
        geom.build_walls(project);
        geom.build_hosted(project);
        geom.build_slabs(project);
        geom.build_beams(project);
        geom.build_foundations(project);
        geom.build_structure(project);
        geom.rebar = crate::rebar::build_rebar(project, &geom.beams);
        geom.build_components(project);
        geom.build_masses(project);
        geom.build_mep(project);
        geom.apply_joins(project);
        geom.build_rooms(project);
        geom
    }

    /// The closed meshes of an element's body, one per solid. A wall's solid
    /// leaves its joints open, so it is rebuilt with both ends capped.
    fn closed_meshes(&self, id: ElementId) -> Vec<Vec<Vec<DVec3>>> {
        let mesh = |s: &Solid| s.faces.iter().map(|f| f.verts.clone()).collect::<Vec<_>>();
        if let Some(wall) = self.walls.get(&id) {
            let mut closed = wall.clone();
            closed.start_joined = false;
            closed.end_joined = false;
            closed.face_gaps.clear();
            return vec![mesh(&closed.to_solid([0, 0, 0]))];
        }
        self.body_of(id).map(mesh).collect()
    }

    /// Volume the bodies of `a` and `b` share (0 when they only touch). Bodies
    /// already cut by a join have lost what they shared.
    pub fn shared_volume(&self, a: ElementId, b: ElementId) -> f64 {
        let (ma, mb) = (self.closed_meshes(a), self.closed_meshes(b));
        let mut total = 0.0;
        for x in &ma {
            for y in mb.iter().filter(|y| crate::csg::bounds_overlap(x, y)) {
                if let Some(common) = crate::csg::intersect(x, y) {
                    let mut s = Solid::new(a, Category::Wall, [0, 0, 0]);
                    s.add_polygons(common);
                    total += s.volume().max(0.0);
                }
            }
        }
        total
    }

    /// Joined geometry: the cut element loses the volume it shares with
    /// the element that cuts it. Bodies are subtracted, except walls, whose
    /// plan and joints are analytic: their shared volume is deducted from
    /// their quantity instead, and views merge the outlines.
    fn apply_joins(&mut self, project: &Project) {
        for &(cutting, cut) in &project.joins {
            let cutters = self.closed_meshes(cutting);
            if self.walls.contains_key(&cut) {
                let Some(body) = self.closed_meshes(cut).into_iter().next() else { continue };
                let shared: f64 = cutters
                    .iter()
                    .filter(|c| crate::csg::bounds_overlap(&body, c))
                    .filter_map(|c| crate::csg::intersect(&body, c))
                    .map(|common| {
                        let mut s = Solid::new(cut, Category::Wall, [0, 0, 0]);
                        s.add_polygons(common);
                        s.volume().max(0.0)
                    })
                    .sum();
                *self.join_deductions.entry(cut).or_default() += shared;
                continue;
            }
            for solid in self.solids.iter_mut().filter(|s| s.element == cut && !s.secondary) {
                for cutter in &cutters {
                    let body: Vec<Vec<DVec3>> = solid.faces.iter().map(|f| f.verts.clone()).collect();
                    if !crate::csg::bounds_overlap(&body, cutter) {
                        continue;
                    }
                    // Past the operation budget the body stays whole.
                    if let Some(rest) = crate::csg::subtract(&body, cutter) {
                        solid.faces.clear();
                        solid.add_polygons(rest);
                    }
                }
            }
        }
    }

    /// One solid per resolved piece of each component, colored by its
    /// material. Components of one type and the same instance values share
    /// the resolution (and its boolean cuts).
    fn build_components(&mut self, project: &Project) {
        type Key = (cc_core::ElementTypeId, Vec<(String, u64)>);
        let mut resolved: std::collections::HashMap<Key, Vec<cc_core::component::ComponentPart>> = Default::default();
        for e in project.elements.values() {
            let ElementKind::Component(c) = &e.kind else { continue };
            let Some(TypeData::Component { def, .. }) = project.element_type(e.type_id).map(|t| &t.data) else {
                continue;
            };
            let key = (e.type_id, c.overrides.iter().map(|(k, v)| (k.clone(), v.to_bits())).collect());
            let local = resolved.entry(key).or_insert_with(|| crate::resolve_component(def, &c.overrides)).clone();
            let parts = place_parts(project, e, c, local);
            for part in parts.iter().filter(|p| !p.plan_only) {
                let mut s = Solid::of_material(e.id, e.category(), project, &part.material);
                s.detail = part.detail;
                if part.faces.is_empty() {
                    s.add_prism(&part.footprint, |_| part.z0, |_| part.z1);
                } else {
                    s.add_polygons(part.faces.clone());
                }
                if !s.faces.is_empty() {
                    self.solids.push(s);
                }
            }
            self.components.insert(e.id, parts);
        }
    }

    /// Conceptual masses: see-through bodies, and their mass floors.
    fn build_masses(&mut self, project: &Project) {
        for e in project.elements.values() {
            let ElementKind::Mass(m) = &e.kind else { continue };
            let Some(g) = crate::mass::mass_geom(project, e, m) else { continue };
            let material = project.element_type(e.type_id).map_or("", |t| t.main_material());
            let mut s = Solid::of_material(e.id, e.category(), project, material);
            // Masses are studied through: at least partly transparent.
            s.transparency = s.transparency.max(0.4);
            s.add_polygons(g.faces.clone());
            self.solids.push(s);
            self.masses.insert(e.id, g);
        }
    }

    /// Columns, beams, stairs, ramps, railings and swept profiles.
    fn build_structure(&mut self, project: &Project) {
        for e in project.elements.values() {
            let Some(ty) = project.element_type(e.type_id) else { continue };
            let mut s = Solid::of_material(e.id, e.category(), project, ty.main_material());
            match (&e.kind, &ty.data) {
                (ElementKind::Column(c), TypeData::Column { section, .. }) => {
                    let (base, top) = project.column_span(e, c);
                    for part in crate::section::column_parts(*section, c.position, c.rotation_deg) {
                        s.add_prism(&part, |_| base, |_| top);
                    }
                }
                (ElementKind::Beam(_), TypeData::Beam { .. }) => {
                    let Some(g) = self.beams.get(&e.id) else { continue };
                    g.add_to(&mut s);
                }
                (ElementKind::Stair(st), TypeData::Stair { build, .. }) => {
                    let Some(g) = crate::stair_geom(project, e, st) else { continue };
                    g.add_to(build, &mut s);
                }
                (ElementKind::Ramp(r), TypeData::Ramp { thickness, .. }) => {
                    let Some(g) = crate::ramp_geom(project, e, r) else { continue };
                    g.add_to(*thickness, &mut s);
                }
                (ElementKind::Railing(r), TypeData::Railing { .. }) => crate::add_railing(project, e, r, &mut s),
                (ElementKind::Sweep(sw), TypeData::Sweep { .. }) => {
                    let Some(faces) = crate::profile_sweep::sweep_faces(project, e, sw) else { continue };
                    s.add_polygons(faces);
                }
                _ => continue,
            }
            if !s.faces.is_empty() {
                self.solids.push(s);
            }
        }
    }

    /// Beams framed into the structural columns and the other beams they meet.
    fn build_beams(&mut self, project: &Project) {
        let mut beams = Vec::new();
        let mut columns = Vec::new();
        for e in project.elements.values() {
            match (&e.kind, project.element_type(e.type_id).map(|t| &t.data)) {
                (ElementKind::Beam(b), Some(TypeData::Beam { width, height, shape, .. })) if b.length() > 1e-6 => {
                    // Joins work on the body centerline, whatever the justification.
                    let (start, end) = project.beam_axis(e, b);
                    let section = crate::section::shape_rects(*shape, *width, *height)
                        .into_iter()
                        .map(|(a, b)| (DVec2::new(a.x, a.y + height / 2.0), DVec2::new(b.x, b.y + height / 2.0)))
                        .collect();
                    beams.push(BeamInput {
                        element: e.id,
                        start,
                        end,
                        half: width / 2.0,
                        z_range: project.beam_z(e, b),
                        slope: b.rise() / b.length(),
                        section,
                        join_start: b.join_start,
                        join_end: b.join_end,
                    });
                }
                (ElementKind::Column(c), Some(TypeData::Column { section, .. })) if c.structural => {
                    columns.push(ColumnInput {
                        element: e.id,
                        outline: column_outline(*section, c.position, c.rotation_deg),
                        z_range: project.column_span(e, c),
                    });
                }
                _ => {}
            }
        }
        self.beams = resolve_beams(&beams, &columns).into_iter().map(|g| (g.element, g)).collect();
    }

    /// Pads, strips (meeting at corners and tees like beams) and slabs.
    fn build_foundations(&mut self, project: &Project) {
        let mut strips = Vec::new();
        for e in project.elements.values() {
            let ElementKind::Foundation(f) = &e.kind else { continue };
            let Some(TypeData::Foundation { width, length, thickness, .. }) =
                project.element_type(e.type_id).map(|t| &t.data)
            else {
                continue;
            };
            let top = project.level_elevation(e.level) + f.top_offset;
            let z_range = (top - thickness, top);
            let footprint = match &f.shape {
                cc_core::FoundationShape::Pad { position, rotation_deg } => {
                    let section = cc_core::ColumnSection::Rectangular { width: *width, depth: *length };
                    column_outline(section, *position, *rotation_deg)
                }
                cc_core::FoundationShape::Slab { boundary } => polygon::ccw(boundary),
                cc_core::FoundationShape::Strip { start, end } => {
                    if start.distance(*end) > 1e-6 {
                        strips.push(BeamInput {
                            element: e.id,
                            start: *start,
                            end: *end,
                            half: width / 2.0,
                            z_range,
                            slope: 0.0,
                            section: Vec::new(),
                            join_start: true,
                            join_end: true,
                        });
                    }
                    continue;
                }
            };
            self.foundations.insert(e.id, FoundationGeom { element: e.id, footprint, z_range });
        }
        for g in resolve_beams(&strips, &[]) {
            let footprint = g.footprint().to_vec();
            self.foundations.insert(g.element, FoundationGeom { element: g.element, footprint, z_range: g.z_range });
        }
        for (id, g) in &self.foundations {
            let Some(e) = project.element(*id) else { continue };
            let material = project.element_type(e.type_id).map_or("", |t| t.main_material());
            let mut s = Solid::of_material(*id, Category::Foundation, project, material);
            let (z0, z1) = g.z_range;
            s.add_prism(&g.footprint, |_| z0, |_| z1);
            if !s.faces.is_empty() {
                self.solids.push(s);
            }
        }
    }

    /// Footprints of the room-bounding walls of `level`; with a phase (an
    /// index into the project's phases), only of the walls standing at its
    /// end (built by then and not demolished in it or before).
    pub fn bounding_footprints(
        &self,
        project: &Project,
        level: cc_core::LevelId,
        phase: Option<usize>,
    ) -> Vec<Vec<DVec2>> {
        project
            .walls()
            .filter(|(e, w)| e.level == level && w.room_bounding)
            .filter(|(e, _)| phase.is_none_or(|ph| project.stands_after(e, ph)))
            .filter_map(|(e, _)| self.walls.get(&e.id))
            .map(|g| g.footprint())
            .collect()
    }

    /// Boundary of the room that would enclose `point` on `level`, in
    /// `phase` (unknown: the last phase).
    pub fn room_at(
        &self,
        project: &Project,
        level: cc_core::LevelId,
        point: DVec2,
        phase: Option<cc_core::PhaseId>,
    ) -> Option<Vec<DVec2>> {
        let phase = phase.and_then(|ph| project.phase_index(ph)).or(project.phases.len().checked_sub(1));
        crate::room_boundary(&self.bounding_footprints(project, level, phase), point)
    }

    /// Rooms are bounded by the walls standing at the end of the phase
    /// they are created in.
    fn build_rooms(&mut self, project: &Project) {
        let phased = !project.phases.is_empty();
        let mut footprints: BTreeMap<(cc_core::LevelId, usize), Vec<Vec<DVec2>>> = BTreeMap::new();
        for e in project.elements.values() {
            let ElementKind::Room(room) = &e.kind else { continue };
            let phase = project.created_index(e);
            let walls = footprints
                .entry((e.level, phase))
                .or_insert_with(|| self.bounding_footprints(project, e.level, phased.then_some(phase)));
            if let Some(boundary) = crate::room_boundary(walls, room.position) {
                self.rooms.insert(e.id, boundary);
            }
        }
    }

    /// Geometry of the model as a view of phase number `phase` shows it:
    /// only the elements standing in it or built in it, so doors of later
    /// phases do not open existing walls and walls join only with walls of
    /// the phase. Rooms of earlier phases keep their boundaries from `full`
    /// (the geometry of the whole model).
    pub fn build_phase(project: &Project, phase: usize, full: &ModelGeometry) -> Self {
        let mut present = project.clone();
        present.elements.retain(|_, e| project.present_at(e, phase));
        present.joins.retain(|(a, b)| present.elements.contains_key(a) && present.elements.contains_key(b));
        let mut geom = Self::build(&present);
        for (id, boundary) in &full.rooms {
            if present.elements.get(id).is_some_and(|e| present.created_index(e) < phase) {
                geom.rooms.insert(*id, boundary.clone());
            }
        }
        geom
    }

    fn build_walls(&mut self, project: &Project) {
        // Joins are resolved among walls of the same level.
        let mut by_level: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (e, w) in project.walls() {
            if w.length() > 1e-6 {
                by_level.entry(e.level).or_default().push((e, w));
            }
        }
        for walls in by_level.values() {
            // Joins work on the body centerlines (location lines shifted by the location line setting).
            let lines: Vec<WallLine> = walls
                .iter()
                .map(|(e, w)| {
                    let (start, end) = project.wall_axis(e, w);
                    WallLine {
                        start,
                        end,
                        half: project.thickness_of(e) * 0.5,
                        join_start: w.join_start,
                        join_end: w.join_end,
                        bulge: w.bulge,
                    }
                })
                .collect();
            let mut gaps: Vec<(usize, [DVec2; 2])> = Vec::new();
            let mut ids = Vec::new();
            for (i, (e, w)) in walls.iter().enumerate() {
                let base_z = project.wall_base_z(e, w);
                let top_z = project.wall_top_z(e, w);
                let ty = project.element_type(e.type_id);
                let mut layers = Vec::new();
                let mut v = lines[i].half;
                for layer in ty.map(|t| t.layers()).unwrap_or_default() {
                    layers.push((v, v - layer.thickness, layer.material.clone()));
                    v -= layer.thickness;
                }
                let (start_join, end_join) = (resolve_join(&lines, i, true), resolve_join(&lines, i, false));
                let mut g = WallGeom::new(e.id, lines[i], base_z, top_z, start_join, end_join, layers);
                // Where this wall meets another in a T, that wall's face opens.
                for (join, u_left, u_right) in
                    [(start_join, g.start_ext.0, g.start_ext.1), (end_join, g.end_ext.0, g.end_ext.1)]
                {
                    if let Some(host) = join.and_then(|j| j.host) {
                        let pts = [g.world(u_left, g.half), g.world(u_right, -g.half)];
                        gaps.push((host, pts));
                    }
                }
                let openings = project
                    .hosted_by(e.id)
                    .filter_map(|h| {
                        let host = h.as_hosted()?;
                        let (width, height) = project.element_type(h.type_id)?.opening_size()?;
                        let z0 = base_z + host.sill_height;
                        Some(Opening {
                            element: h.id,
                            u0: host.offset - width * 0.5,
                            u1: host.offset + width * 0.5,
                            z0,
                            z1: z0 + height,
                        })
                    })
                    .collect();
                g.set_openings(openings);
                let material = ty.map_or("", |t| t.main_material());
                let mut solid = g.to_solid(project.material_color(material));
                solid.set_appearance(project, material);
                solid.transparency = project.material_transparency(material);
                self.solids.push(solid);
                ids.push(e.id);
                self.walls.insert(e.id, g);
            }
            for (host, pts) in gaps {
                let Some(g) = ids.get(host).and_then(|id| self.walls.get_mut(id)) else { continue };
                let us: Vec<f64> = pts.iter().map(|p| g.local(*p).0).collect();
                let side = if g.local(pts[0]).1 >= 0.0 { 1.0 } else { -1.0 };
                g.face_gaps.push((side, us[0].min(us[1]), us[0].max(us[1])));
            }
        }
    }

    fn build_hosted(&mut self, project: &Project) {
        let mut fills = BTreeMap::new();
        for wall in self.walls.values() {
            for o in &wall.openings {
                let Some(e) = project.element(o.element) else { continue };
                let parts = opening_parts(project, e, o.u1 - o.u0, o.z1 - o.z0, wall.half);
                // In an arc wall the pieces are square to the wall at the opening's center.
                let (origin, dir, normal) = wall.frame_at((o.u0 + o.u1) * 0.5);
                let world = |p: DVec2| origin + dir * (o.u0 + p.x) + normal * p.y;
                let mut placed = Vec::with_capacity(parts.len());
                for mut p in parts {
                    p.footprint = p.footprint.iter().map(|q| world(*q)).collect();
                    p.z0 += o.z0;
                    p.z1 += o.z0;
                    for face in &mut p.faces {
                        for v in face.iter_mut() {
                            *v = world(v.truncate()).extend(v.z + o.z0);
                        }
                    }
                    if p.plan_only {
                        placed.push(p);
                        continue;
                    }
                    // Frames without a material keep the frame color.
                    let mut s = if p.material.is_empty() {
                        Solid::new(e.id, e.category(), FRAME_COLOR)
                    } else {
                        Solid::of_material(e.id, e.category(), project, &p.material)
                    };
                    if p.faces.is_empty() {
                        s.add_prism(&p.footprint, |_| p.z0, |_| p.z1);
                    } else {
                        s.add_polygons(p.faces.clone());
                    }
                    s.detail = p.detail;
                    if !s.faces.is_empty() {
                        self.solids.push(s);
                    }
                    placed.push(p);
                }
                fills.insert(e.id, placed);
            }
        }
        self.opening_fills = fills;
    }

    fn build_slabs(&mut self, project: &Project) {
        for e in project.elements.values() {
            let Some(ty) = project.element_type(e.type_id) else { continue };
            let level_z = project.level_elevation(e.level);
            let thk = ty.thickness();
            let mut s = Solid::of_material(e.id, e.category(), project, ty.main_material());
            match &e.kind {
                ElementKind::Floor(f) => {
                    let top = level_z + f.height_offset;
                    // Openings clockwise inside the counter-clockwise boundary.
                    let holes: Vec<Vec<DVec2>> = f
                        .all_openings()
                        .map(|o| {
                            let mut o = polygon::ccw(o);
                            o.reverse();
                            o
                        })
                        .filter(|o| o.len() >= 3)
                        .collect();
                    if holes.is_empty() {
                        s.add_prism(&f.boundary, |_| top - thk, |_| top);
                    } else {
                        s.add_prism_with_holes(&polygon::ccw(&f.boundary), &holes, |_| top - thk, |_| top);
                    }
                }
                ElementKind::Ceiling(c) => {
                    let bottom = level_z + c.height_offset;
                    // Shafts cut through, clockwise inside the counter-clockwise outline.
                    let holes: Vec<Vec<DVec2>> = c
                        .cuts
                        .iter()
                        .map(|o| {
                            let mut o = polygon::ccw(o);
                            o.reverse();
                            o
                        })
                        .filter(|o| o.len() >= 3)
                        .collect();
                    if holes.is_empty() {
                        s.add_prism(&c.boundary, |_| bottom, |_| bottom + thk);
                    } else {
                        s.add_prism_with_holes(&polygon::ccw(&c.boundary), &holes, |_| bottom, |_| bottom + thk);
                    }
                }
                ElementKind::Roof(r) => {
                    let fp = polygon::ccw(&polygon::offset(&r.footprint, r.overhang));
                    // Openings shrink by the overhang (the eaves reach over them), clockwise.
                    let holes: Vec<Vec<DVec2>> = r
                        .holes
                        .iter()
                        .map(|h| {
                            let mut h = polygon::offset(h, -r.overhang);
                            h.reverse();
                            h
                        })
                        .filter(|h| h.len() >= 3 && polygon::signed_area(h) < 0.0)
                        // Shafts cut their own outline: no eaves reach over them.
                        .chain(r.cuts.iter().map(|c| {
                            let mut c = polygon::ccw(c);
                            c.reverse();
                            c
                        }))
                        .collect();
                    let base = level_z + r.base_offset;
                    match r.shape {
                        RoofShape::Flat => s.add_prism_with_holes(&fp, &holes, |_| base, |_| base + thk),
                        RoofShape::Shed { slope, direction_deg } => {
                            let a = direction_deg.to_radians();
                            let d = DVec2::new(a.cos(), a.sin());
                            let min = fp.iter().map(|p| p.dot(d)).fold(f64::INFINITY, f64::min);
                            let vertical_thk = thk * (1.0 + slope * slope).sqrt();
                            let bottom = move |p: DVec2| base + slope * (p.dot(d) - min);
                            s.add_prism_with_holes(&fp, &holes, bottom, move |p| bottom(p) + vertical_thk);
                        }
                        RoofShape::Gable { slope, .. } | RoofShape::Hip { slope } => {
                            let ridge = match r.shape {
                                RoofShape::Gable { ridge_deg, .. } => Some(DVec2::from_angle(ridge_deg.to_radians())),
                                _ => None,
                            };
                            let vertical_thk = thk * (1.0 + slope * slope).sqrt();
                            let mut loops = vec![fp.clone()];
                            loops.extend(holes.iter().cloned());
                            let own = crate::roofs::ccw_edge_slopes(r);
                            let weights = crate::roofs::eave_weights(&loops, slope, ridge, &own);
                            let sloped = weights.iter().flatten().any(|w| *w > 0.0);
                            match crate::skeleton::skeleton(&loops, &weights) {
                                Ok(sk) if sloped => {
                                    crate::roofs::add_skeleton_roof(&mut s, &sk, base, vertical_thk);
                                    self.roof_ridges.insert(e.id, sk.lines);
                                }
                                // Should the skeleton fail, the lowest planes of the outline.
                                Err(_) if sloped && holes.is_empty() => {
                                    let slopes = crate::roofs::edge_slopes(&fp, slope, ridge, &own);
                                    let eaves = crate::roofs::sloped_eaves(&fp, &slopes);
                                    crate::roofs::add_pitched_roof(&mut s, &fp, &eaves, base, vertical_thk);
                                    let regions = crate::roofs::roof_regions(&fp, &eaves);
                                    self.roof_ridges.insert(e.id, crate::roofs::ridge_lines(&fp, &regions));
                                }
                                _ => s.add_prism_with_holes(&fp, &holes, |_| base, |_| base + thk),
                            }
                        }
                    }
                }
                _ => continue,
            }
            // A roof's framing and covering figures, from its sloped faces.
            if let (ElementKind::Roof(_), TypeData::Roof { covering, framing, .. }) = (&e.kind, &ty.data) {
                let g = crate::roof_framing::roof_framing(&s, framing, *covering);
                if !g.members.is_empty() {
                    // Its own solid, in the framing's material, apart from the
                    // roof's body (left out of its volume, joins and IFC body).
                    let mut members = Solid::of_material(e.id, Category::Beam, project, &framing.material);
                    members.secondary = true;
                    for m in &g.members {
                        m.add_to(&mut members);
                    }
                    self.solids.push(members);
                }
                self.roof_framing.insert(e.id, g);
            }
            self.solids.push(s);
        }
    }

    pub fn solids_of(&self, id: ElementId) -> impl Iterator<Item = &Solid> {
        self.solids.iter().filter(move |s| s.element == id)
    }

    /// The solids of an element's body: its solids without secondary parts
    /// such as a roof's framing.
    pub fn body_of(&self, id: ElementId) -> impl Iterator<Item = &Solid> {
        self.solids_of(id).filter(|s| !s.secondary)
    }

    pub fn bounds(&self) -> Option<(DVec3, DVec3)> {
        self.solids.iter().filter_map(Solid::bounds).reduce(|(a, b), (c, d)| (a.min(c), b.max(d)))
    }

    /// The vertical faces of an element seen from above: each one is a plan
    /// segment (the sides of a column, the faces of a component).
    pub fn plan_edges(&self, id: ElementId) -> Vec<(DVec2, DVec2)> {
        let mut out = Vec::new();
        for face in self.body_of(id).flat_map(|s| s.faces.iter()) {
            let n = DVec2::new(face.normal.x, face.normal.y);
            if face.normal.z.abs() > 1e-6 || n.length_squared() < 0.5 {
                continue;
            }
            let (n, d) = (n.normalize_or_zero(), n.normalize_or_zero().perp());
            let Some(first) = face.verts.first() else { continue };
            let offset = DVec2::new(first.x, first.y).dot(n);
            let along = face.verts.iter().map(|v| DVec2::new(v.x, v.y).dot(d));
            let (lo, hi) = along.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), t| (lo.min(t), hi.max(t)));
            if hi - lo > 1e-6 && lo.is_finite() && hi.is_finite() {
                out.push((n * offset + d * lo, n * offset + d * hi));
            }
        }
        out
    }

    /// The plan segment of an element's vertical faces nearest to `p` (see [`Self::plan_edges`]).
    pub fn nearest_plan_edge(&self, id: ElementId, p: DVec2) -> Option<(DVec2, DVec2)> {
        let dist = |e: &(DVec2, DVec2)| polygon::closest_on_segment(e.0, e.1, p).0.distance(p);
        self.plan_edges(id).into_iter().min_by(|x, y| dist(x).total_cmp(&dist(y)))
    }

    /// Volume of an element's main body (frames/glass excluded for openings).
    pub fn volume(&self, id: ElementId) -> f64 {
        if let Some(wall) = self.walls.get(&id) {
            let shared = self.join_deductions.get(&id).copied().unwrap_or(0.0);
            return (wall.volume() - shared).max(0.0);
        }
        self.body_of(id).map(Solid::volume).sum()
    }

    /// The nearest solid along `ray` among the elements `shown` accepts.
    pub fn pick_where(&self, ray: Ray, shown: &mut dyn FnMut(ElementId) -> bool) -> Option<(ElementId, f64)> {
        self.nearest(ray, &mut |s| shown(s.element))
    }

    /// Closest element hit by `ray`, skipping categories in `hidden`.
    pub fn pick(&self, ray: Ray, hidden: &[Category]) -> Option<(ElementId, f64)> {
        self.nearest(ray, &mut |s| !hidden.contains(&s.category))
    }

    /// The nearest hit among the solids `accept` takes (on a tie, the first
    /// solid). Solids are tried in the order the ray meets their boxes, until
    /// the next box lies beyond the nearest hit.
    fn nearest(&self, ray: Ray, accept: &mut dyn FnMut(&Solid) -> bool) -> Option<(ElementId, f64)> {
        let mut boxes: Vec<(f64, usize)> = (self.solids.iter().enumerate())
            .filter_map(|(i, s)| {
                let (lo, hi) = s.bounds()?;
                Some((crate::solid::ray_box(ray, lo, hi)?, i))
            })
            .collect();
        boxes.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut best: Option<(f64, usize)> = None;
        for (entry, i) in boxes {
            let limit = best.map_or(f64::INFINITY, |(t, _)| t);
            if entry > limit {
                break;
            }
            let Some(solid) = self.solids.get(i) else { continue };
            let Some(t) = solid.raycast_within(ray, limit) else { continue };
            let closer = best.is_none_or(|(bt, bi)| t < bt || (t == bt && i < bi));
            if closer && accept(solid) {
                best = Some((t, i));
            }
        }
        best.and_then(|(t, i)| Some((self.solids.get(i)?.element, t)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::{Document, Element, Floor, Hosted, Wall};

    pub(crate) fn sample_house() -> Document {
        let mut doc = Document::new(Project::from_template());
        doc.edit("house", |e| {
            let level = e.project().levels[0].id;
            let wall_t = e.project().default_type(Category::Wall).unwrap();
            let door_t = e.project().default_type(Category::Door).unwrap();
            let floor_t = e.project().default_type(Category::Floor).unwrap();
            let pts = [DVec2::new(0.0, 0.0), DVec2::new(8.0, 0.0), DVec2::new(8.0, 6.0), DVec2::new(0.0, 6.0)];
            let mut walls = vec![];
            for i in 0..4 {
                let id = ElementId(e.alloc_id());
                walls.push(id);
                e.add_element(Element {
                    id,
                    level,
                    type_id: wall_t,
                    mark: String::new(),
                    comments: String::new(),
                    uid: Default::default(),
                    kind: ElementKind::Wall(Wall {
                        start: pts[i],
                        end: pts[(i + 1) % 4],
                        base_offset: 0.0,
                        height: 3.0,
                        ..Default::default()
                    }),
                    phase_created: Default::default(),
                    phase_demolished: None,
                });
            }
            let id = ElementId(e.alloc_id());
            e.add_element(Element {
                id,
                level,
                type_id: door_t,
                mark: "1".into(),
                comments: String::new(),
                uid: Default::default(),
                kind: ElementKind::Door(Hosted {
                    host: walls[0],
                    offset: 2.0,
                    sill_height: 0.0,
                    flip_facing: false,
                    flip_hand: false,
                }),
                phase_created: Default::default(),
                phase_demolished: None,
            });
            let id = ElementId(e.alloc_id());
            e.add_element(Element {
                id,
                level,
                type_id: floor_t,
                mark: String::new(),
                comments: String::new(),
                uid: Default::default(),
                kind: ElementKind::Floor(Floor::new(pts.to_vec(), 0.0)),
                phase_created: Default::default(),
                phase_demolished: None,
            });
        });
        doc
    }

    /// A beam at the top of the walls of the sample house, from the middle
    /// of the west wall to the middle of the east wall.
    fn house_with_beam(top_offset: f64) -> (Document, ElementId) {
        let mut doc = sample_house();
        let mut beam = ElementId(0);
        doc.edit("beam", |e| {
            let level = e.project().levels[0].id;
            let beam_t = e.project().default_type(Category::Beam).unwrap();
            beam = ElementId(e.alloc_id());
            e.add_element(Element {
                id: beam,
                level,
                type_id: beam_t,
                mark: String::new(),
                comments: String::new(),
                uid: Default::default(),
                kind: ElementKind::Beam(cc_core::Beam::new(DVec2::new(0.0, 3.0), DVec2::new(8.0, 3.0), top_offset)),
                phase_created: Default::default(),
                phase_demolished: None,
            });
        });
        (doc, beam)
    }

    fn beam_size(project: &Project, beam: ElementId) -> (f64, f64) {
        let ty = project.element(beam).and_then(|e| project.element_type(e.type_id)).unwrap();
        let TypeData::Beam { width, height, .. } = ty.data else { panic!("beam type") };
        (width, height)
    }

    #[test]
    fn joined_geometry_counts_shared_volume_once() {
        // A beam hanging from the floor slab: the beam cuts the slab.
        let (mut doc, beam) = house_with_beam(0.0);
        let floor = doc.project.elements.values().find(|e| e.category() == Category::Floor).unwrap().id;
        let before = ModelGeometry::build(&doc.project);
        let (floor_v, beam_v) = (before.volume(floor), before.volume(beam));
        let (width, height) = beam_size(&doc.project, beam);
        let slab = floor_v / 48.0;
        assert!(slab < height, "the slab is thinner than the beam");
        doc.edit("join", |e| e.set_joined(beam, floor, true));
        let after = ModelGeometry::build(&doc.project);
        let shared = width * slab * 8.0;
        assert!(
            (after.volume(floor) - (floor_v - shared)).abs() < 1e-6,
            "{} vs {}",
            after.volume(floor),
            floor_v - shared
        );
        assert!((after.volume(beam) - beam_v).abs() < 1e-9, "the cutting element keeps its volume");

        // A beam inside the walls' height: it cuts the two walls it enters
        // (half their thickness each), whose bodies stay whole.
        let (mut doc, beam) = house_with_beam(3.0);
        let walls: Vec<ElementId> = doc.project.walls().map(|(e, _)| e.id).collect();
        let before = ModelGeometry::build(&doc.project);
        let total: f64 = walls.iter().map(|w| before.volume(*w)).sum();
        doc.edit("join", |e| {
            for w in &walls {
                e.set_joined(beam, *w, true);
            }
        });
        let after = ModelGeometry::build(&doc.project);
        let joined: f64 = walls.iter().map(|w| after.volume(*w)).sum();
        let shared = 2.0 * width * height * 0.075;
        assert!((total - joined - shared).abs() < 1e-6, "{} vs {}", total - joined, shared);
        // Unjoined, the volumes come back.
        doc.edit("unjoin", |e| {
            for w in &walls {
                e.set_joined(beam, *w, false);
            }
        });
        let again: f64 = walls.iter().map(|w| ModelGeometry::build(&doc.project).volume(*w)).sum();
        assert!((again - total).abs() < 1e-9);
    }

    #[test]
    fn house_builds_closed_walls() {
        let doc = sample_house();
        let g = ModelGeometry::build(&doc.project);
        assert_eq!(g.walls.len(), 4);
        assert!(g.walls.values().all(|w| w.start_joined && w.end_joined));
        // Mitered rectangle: total wall volume = (outer area - inner area) * height - door.
        let wall_volume: f64 = g.walls.keys().map(|id| g.volume(*id)).sum();
        let ring = 8.15 * 6.15 - 7.85 * 5.85;
        let expected = ring * 3.0 - 0.8 * 2.1 * 0.15;
        assert!((wall_volume - expected).abs() < 1e-6, "{wall_volume} vs {expected}");
        let (lo, hi) = g.bounds().unwrap();
        assert!(lo.z < 0.0 && (hi.z - 3.0).abs() < 1e-9);
    }

    /// A "D": a half circle of radius 4 (centerline) closed by a straight wall,
    /// with a door in the middle of the arc and a room inside.
    fn d_shaped_room() -> (Document, ElementId, ElementId, ElementId) {
        let mut doc = Document::new(Project::from_template());
        let ids = doc.edit("d", |e| {
            let level = e.project().levels[0].id;
            let wall_t = e.project().default_type(Category::Wall).unwrap();
            let door_t = e.project().default_type(Category::Door).unwrap();
            let room_t = e.project().default_type(Category::Room).unwrap();
            let add = |e: &mut cc_core::EditCtx, type_id, kind| {
                let id = ElementId(e.alloc_id());
                e.add_element(Element {
                    id,
                    level,
                    type_id,
                    mark: String::new(),
                    comments: String::new(),
                    uid: Default::default(),
                    kind,
                    phase_created: Default::default(),
                    phase_demolished: None,
                })
            };
            let arc =
                Wall { start: DVec2::new(4.0, 0.0), end: DVec2::new(-4.0, 0.0), bulge: 1.0, ..Default::default() };
            let arc = add(e, wall_t, ElementKind::Wall(arc));
            let chord = Wall { start: DVec2::new(-4.0, 0.0), end: DVec2::new(4.0, 0.0), ..Default::default() };
            add(e, wall_t, ElementKind::Wall(chord));
            let hosted = Hosted {
                host: arc,
                offset: 2.0 * std::f64::consts::PI,
                sill_height: 0.0,
                flip_facing: false,
                flip_hand: false,
            };
            let door = add(e, door_t, ElementKind::Door(hosted));
            let room =
                cc_core::Room { position: DVec2::new(0.0, 1.0), number: "1".into(), name: "D".into(), height: 3.0 };
            let room = add(e, room_t, ElementKind::Room(room));
            (arc, door, room)
        });
        (doc, ids.0, ids.1, ids.2)
    }

    #[test]
    fn arc_walls_bound_rooms_and_host_doors() {
        let (doc, arc, door, room) = d_shaped_room();
        let g = ModelGeometry::build(&doc.project);
        let wall = &g.walls[&arc];
        assert!(wall.arc.is_some() && wall.start_joined && wall.end_joined);
        let h = wall.half;
        // The room: the inner circle's segment above the straight wall's inner face.
        let boundary = g.rooms.get(&room).expect("the D encloses the room");
        let (r, d) = (4.0 - h, h);
        let segment = r * r * (d / r).acos() - d * (r * r - d * d).sqrt();
        let area = polygon::area(boundary);
        assert!((area - segment).abs() < 1e-3 * segment, "{area} vs {segment}");
        // The door sits at the top of the arc, square to it.
        let o = &wall.openings[0];
        assert!(((o.u0 + o.u1) * 0.5 - 2.0 * std::f64::consts::PI).abs() < 1e-9);
        let (lo, hi) = g.solids_of(door).filter_map(Solid::bounds).reduce(|a, b| (a.0.min(b.0), a.1.max(b.1))).unwrap();
        assert!((lo.y + hi.y) * 0.5 > 3.8 && ((lo.x + hi.x) * 0.5).abs() < 1e-6, "{lo} {hi}");
        // Its volume: the analytic arc body less the opening (an annular sector).
        let ty = doc.project.element_type(doc.project.element(door).unwrap().type_id).unwrap();
        let (width, height) = ty.opening_size().unwrap();
        let thickness = 2.0 * h;
        let body = wall.volume() + width * height * thickness;
        let whole = wall.outline_area((wall.start_ext.0, wall.end_ext.0), (wall.start_ext.1, wall.end_ext.1)) * 3.0;
        assert!((body - whole).abs() < 1e-9, "{body} vs {whole}");
    }

    #[test]
    fn each_wall_volume_is_exact() {
        let doc = sample_house();
        let g = ModelGeometry::build(&doc.project);
        let door = 0.8 * 2.1 * 0.15;
        // Mitered corners: each wall is a trapezoid with the centerline length as mean width.
        let expected = [8.0 * 0.15 * 3.0 - door, 6.0 * 0.15 * 3.0, 8.0 * 0.15 * 3.0, 6.0 * 0.15 * 3.0];
        for (id, want) in g.walls.keys().zip(expected) {
            let got = g.volume(*id);
            assert!((got - want).abs() < 1e-9, "{id}: {got} vs {want}");
        }
    }

    #[test]
    fn walls_follow_top_constraint_and_location_line() {
        use cc_core::{LocationLine, WallTop};
        let mut doc = sample_house();
        let wall_id = *ModelGeometry::build(&doc.project).walls.keys().next().unwrap();
        let upper = doc.project.levels[1].clone();
        doc.edit("constrain", |e| {
            if let Some(ElementKind::Wall(w)) = e.element_mut(wall_id).map(|x| &mut x.kind) {
                w.top = WallTop::Level { level: upper.id, offset: -0.5 };
                w.location_line = LocationLine::FinishExterior;
            }
        });
        let g = ModelGeometry::build(&doc.project);
        let wall = &g.walls[&wall_id];
        assert!((wall.top_z - (upper.elevation - 0.5)).abs() < 1e-9);
        // The first wall runs along +X from the origin; its exterior face (left, +Y)
        // now lies on the drawn line, so the body is centered half a thickness to the right.
        assert!((wall.origin.y - -0.075).abs() < 1e-9, "{}", wall.origin.y);

        // Moving the upper level moves the wall top with it.
        doc.edit("raise", |e| e.levels_mut()[1].elevation += 1.0);
        let g = ModelGeometry::build(&doc.project);
        assert!((g.walls[&wall_id].top_z - (upper.elevation + 0.5)).abs() < 1e-9);
    }

    #[test]
    fn stairs_columns_and_beams_have_exact_volumes() {
        use cc_core::{Beam, Column, Stair, WallTop};
        let mut doc = Document::new(Project::from_template());
        let (stair, column, beam) = doc.edit("structure", |e| {
            let level = e.project().levels[0].id;
            let upper = e.project().levels[1].id;
            let add = |e: &mut cc_core::EditCtx, category, kind| {
                let type_id = e.project().default_type(category).unwrap();
                let id = ElementId(e.alloc_id());
                let mark = String::new();
                e.add_element(Element {
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
            };
            let top = WallTop::Level { level: upper, offset: 0.0 };
            let stair = add(
                e,
                Category::Stair,
                ElementKind::Stair(Stair {
                    start: DVec2::ZERO,
                    direction_deg: 0.0,
                    base_offset: 0.0,
                    height: 3.0,
                    top,
                    layout: Default::default(),
                }),
            );
            let column = add(
                e,
                Category::Column,
                ElementKind::Column(Column {
                    position: DVec2::ZERO,
                    rotation_deg: 30.0,
                    base_offset: 0.0,
                    height: 3.0,
                    top,
                    structural: true,
                }),
            );
            let beam = add(
                e,
                Category::Beam,
                // Clear of the column, so its length stays whole.
                ElementKind::Beam(Beam::new(DVec2::new(1.0, 0.0), DVec2::new(4.0, 4.0), 3.0)),
            );
            (stair, column, beam)
        });
        let g = ModelGeometry::build(&doc.project);
        // 3.00 m at most 0.18 per riser: 17 risers of 3/17 and 16 treads of 0.28.
        let stair_el = doc.project.element(stair).unwrap();
        let ElementKind::Stair(st) = &stair_el.kind else { panic!("not a stair") };
        let sg = crate::stair_geom(&doc.project, stair_el, st).unwrap();
        assert_eq!(sg.risers, 17);
        assert!((sg.run() - 16.0 * 0.28).abs() < 1e-12);
        let expected = 0.28 * 1.0 * (3.0 / 17.0) * (1..=16).sum::<usize>() as f64;
        assert!((g.volume(stair) - expected).abs() < 1e-9, "{} vs {expected}", g.volume(stair));
        assert!((g.volume(column) - 0.2 * 0.2 * 3.0).abs() < 1e-9);
        assert!((g.volume(beam) - 0.15 * 0.4 * 5.0).abs() < 1e-9);
    }

    #[test]
    fn beam_justification_moves_the_body() {
        use cc_core::{Beam, BeamLateral, BeamVertical};
        let (mut doc, beam) = house_with_beam(3.0);
        let (width, height) = beam_size(&doc.project, beam);
        let body = |doc: &Document| {
            let g = ModelGeometry::build(&doc.project).beams[&beam].clone();
            let ys: Vec<f64> = g.footprint().iter().map(|p| p.y).collect();
            let lo = ys.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            ((lo, hi), g.z_range)
        };
        let close = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9;
        // Centered, with the top at the location elevation (the beam runs along +X at y = 3).
        let (y, z) = body(&doc);
        assert!(close(y, (3.0 - width / 2.0, 3.0 + width / 2.0)) && close(z, (3.0 - height, 3.0)), "{y:?} {z:?}");
        let set = |doc: &mut Document, lateral, vertical| {
            doc.edit("justify", |e| {
                if let Some(ElementKind::Beam(b)) = e.element_mut(beam).map(|x| &mut x.kind) {
                    (b.lateral, b.vertical) = (lateral, vertical);
                }
            });
        };
        // Left face on the line: the body lies to its right (towards -Y).
        set(&mut doc, BeamLateral::Left, BeamVertical::Bottom);
        let (y, z) = body(&doc);
        assert!(close(y, (3.0 - width, 3.0)) && close(z, (3.0, 3.0 + height)), "{y:?} {z:?}");
        set(&mut doc, BeamLateral::Right, BeamVertical::Center);
        let (y, z) = body(&doc);
        assert!(close(y, (3.0, 3.0 + width)) && close(z, (3.0 - height / 2.0, 3.0 + height / 2.0)), "{y:?} {z:?}");
        // A mirror across X keeps the body on the same side of the mirrored line.
        let mut b = Beam::new(DVec2::ZERO, DVec2::X, 0.0);
        b.lateral = BeamLateral::Left;
        let mut e = doc.project.element(beam).unwrap().clone();
        e.kind = ElementKind::Beam(b);
        e.transform(&cc_core::PlanTransform::mirror_across(DVec2::ZERO, DVec2::X));
        let ElementKind::Beam(m) = &e.kind else { panic!("beam") };
        assert_eq!(m.lateral, BeamLateral::Right);
    }

    #[test]
    fn pick_hits_wall() {
        let doc = sample_house();
        let g = ModelGeometry::build(&doc.project);
        let ray = Ray { origin: DVec3::new(5.0, -10.0, 1.5), dir: DVec3::Y };
        let (id, t) = g.pick(ray, &[]).unwrap();
        assert_eq!(doc.project.element(id).unwrap().category(), Category::Wall);
        assert!((t - (10.0 - 0.075)).abs() < 1e-9);
    }

    #[test]
    fn pick_matches_testing_every_solid() {
        let doc = sample_house();
        let g = ModelGeometry::build(&doc.project);
        let walls: Vec<ElementId> =
            doc.project.elements.values().filter(|e| e.category() == Category::Wall).map(|e| e.id).collect();
        // The first solid hit (in order) among those shown, nearest first.
        let brute = |ray: Ray, shown: &dyn Fn(ElementId) -> bool| {
            g.solids
                .iter()
                .filter_map(|s| s.raycast(ray).map(|t| (s.element, t)))
                .filter(|(e, _)| shown(*e))
                .min_by(|a, b| a.1.total_cmp(&b.1))
        };
        let mut hits = 0;
        for i in 0..15 {
            for j in 0..15 {
                // Rays from outside, from above and along the axes (zero direction components).
                let (x, y) = (-2.0 + i as f64 * 0.8, -2.0 + j as f64 * 0.7);
                for ray in [
                    Ray { origin: DVec3::new(-20.0, y, x * 0.3), dir: DVec3::new(1.0, 0.07, 0.01) },
                    Ray { origin: DVec3::new(x, y, 20.0), dir: DVec3::new(0.0, 0.0, -1.0) },
                    Ray { origin: DVec3::new(x, -20.0, 1.2), dir: DVec3::Y },
                ] {
                    let all = brute(ray, &|_| true);
                    hits += usize::from(all.is_some());
                    assert_eq!(g.pick_where(ray, &mut |_| true), all);
                    assert_eq!(g.pick(ray, &[]), all);
                    // Walls hidden: the ray goes through to what lies behind.
                    let no_walls = brute(ray, &|e| !walls.contains(&e));
                    assert_eq!(g.pick_where(ray, &mut |e| !walls.contains(&e)), no_walls);
                    assert_eq!(g.pick(ray, &[Category::Wall]), no_walls);
                }
            }
        }
        assert!(hits > 100, "{hits}");
    }
}
