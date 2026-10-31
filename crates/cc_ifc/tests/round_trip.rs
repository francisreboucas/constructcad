//! Export → import round trips: the imported project must reproduce the
//! levels and the geometry of every importable element.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{Category, ColumnSection, Element, ElementKind, FoundationShape, Project, RoofShape, TypeData, Uid};
use cc_ifc::{ExportOptions, export, import};
use glam::DVec2;
use std::collections::HashMap;

fn load(text: &str) -> Project {
    cc_core::from_ron(text).unwrap()
}

fn round_trip(original: &Project) -> (Project, cc_ifc::ImportReport) {
    let text =
        export(original, &ExportOptions { time_stamp: Some("2026-01-01T00:00:00".into()), file_name: "t.ifc".into() });
    import(&text, Project::from_template()).unwrap()
}

fn by_uid(p: &Project) -> HashMap<Uid, &Element> {
    p.elements.values().map(|e| (e.uid, e)).collect()
}

fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} vs {b}");
}

fn close2(a: DVec2, b: DVec2, tol: f64, what: &str) {
    assert!(a.distance(b) <= tol, "{what}: {a} vs {b}");
}

fn check(original: &Project) {
    let (imported, report) = round_trip(original);
    assert_eq!(report.schema, "IFC4");
    assert_eq!(imported.info.name, original.info.name);
    assert_eq!(imported.info.uid, original.info.uid);
    assert_eq!(imported.levels.len(), original.levels.len());
    for (a, b) in original.levels.iter().zip(&imported.levels) {
        assert_eq!(a.name, b.name);
        close(a.elevation, b.elevation, 1e-9, "level elevation");
    }
    let lookup = by_uid(&imported);
    let level_name = |p: &Project, e: &Element| p.level_name(e.level).to_string();
    const TOL: f64 = 1e-6;
    for e in original.elements.values() {
        let importable = matches!(
            e.category(),
            Category::Wall
                | Category::Door
                | Category::Window
                | Category::Floor
                | Category::Ceiling
                | Category::Roof
                | Category::Room
                | Category::Column
                | Category::Beam
                | Category::Component
        ) || matches!(&e.kind, ElementKind::Foundation(f) if matches!(f.shape, FoundationShape::Slab { .. }));
        if !importable {
            continue;
        }
        let i = lookup.get(&e.uid).unwrap_or_else(|| panic!("{:?} {} not imported", e.category(), e.id));
        assert_eq!(i.category(), e.category());
        assert_eq!(level_name(&imported, i), level_name(original, e), "level of {}", e.id);
        assert_eq!(imported.type_name(i.type_id), original.type_name(e.type_id), "type of {}", e.id);

        if e.category() != Category::Room {
            assert_eq!(i.mark, e.mark);
        }
        match (&e.kind, &i.kind) {
            (ElementKind::Wall(a), ElementKind::Wall(b)) => {
                let (s, t) = original.wall_axis(e, a);
                close2(b.start, s, TOL, "wall start");
                close2(b.end, t, TOL, "wall end");
                close(original.wall_height(e, a), imported.wall_height(i, b), TOL, "wall height");
                close(original.wall_base_z(e, a), imported.wall_base_z(i, b), TOL, "wall base");
                close(original.thickness_of(e), imported.thickness_of(i), TOL, "wall thickness");
                close(a.bulge, b.bulge, TOL, "wall bulge");
                assert_eq!(a.structural, b.structural);
            }
            (ElementKind::Door(a), ElementKind::Door(b)) | (ElementKind::Window(a), ElementKind::Window(b)) => {
                let host = |p: &Project, id| p.element(id).map(|h| h.uid);
                assert_eq!(host(original, a.host), host(&imported, b.host), "host of {}", e.id);
                close(a.offset, b.offset, TOL, "opening offset");
                close(a.sill_height, b.sill_height, TOL, "sill");
            }
            (ElementKind::Floor(a), ElementKind::Floor(b)) => {
                close(cc_geom::polygon::area(&a.boundary), cc_geom::polygon::area(&b.boundary), TOL, "floor area");
                assert_eq!(a.openings.len(), b.openings.len(), "floor openings");
                close(a.area(), b.area(), TOL, "floor net area");
                close(a.height_offset, b.height_offset, TOL, "floor offset");
            }
            (ElementKind::Foundation(a), ElementKind::Foundation(b)) => {
                let area = |s: &FoundationShape| match s {
                    FoundationShape::Slab { boundary } => cc_geom::polygon::area(boundary),
                    other => panic!("foundation came back as {other:?}"),
                };
                close(area(&a.shape), area(&b.shape), TOL, "foundation slab area");
                close(a.top_offset, b.top_offset, TOL, "foundation slab top");
                close(original.thickness_of(e), imported.thickness_of(i), TOL, "foundation slab thickness");
            }
            (ElementKind::Ceiling(a), ElementKind::Ceiling(b)) => {
                close(cc_geom::polygon::area(&a.boundary), cc_geom::polygon::area(&b.boundary), TOL, "ceiling area");
                close(a.height_offset, b.height_offset, TOL, "ceiling offset");
            }
            (ElementKind::Roof(a), ElementKind::Roof(b)) => {
                let fp = cc_geom::roofs::plan_area;
                assert_eq!(a.holes.len(), b.holes.len(), "roof openings");
                match (a.shape, b.shape) {
                    (RoofShape::Flat, RoofShape::Flat) => {
                        close(fp(a), fp(b), TOL, "flat roof area");
                        close(a.base_offset, b.base_offset, TOL, "roof base");
                    }
                    (
                        RoofShape::Shed { slope: s0, direction_deg: d0 },
                        RoofShape::Shed { slope: s1, direction_deg: d1 },
                    ) => {
                        close(s0, s1, TOL, "shed slope");
                        close(d0, d1, 1e-6, "shed direction");
                        close(fp(a), fp(b), TOL, "shed footprint");
                    }
                    (RoofShape::Gable { slope: s0, .. }, RoofShape::Gable { slope: s1, .. })
                    | (RoofShape::Hip { slope: s0 }, RoofShape::Hip { slope: s1 }) => {
                        // Pitched roofs come back from their B-rep: slope and outline (with the overhang).
                        close(s0, s1, 1e-6, "pitched slope");
                        close(fp(a), fp(b), 1e-6 * fp(a), "pitched footprint");
                    }
                    (x, y) => panic!("roof shape {x:?} came back as {y:?}"),
                }
            }
            (ElementKind::Room(a), ElementKind::Room(b)) => {
                assert_eq!(a.number, b.number);
                assert_eq!(a.name, b.name);
                close(a.height, b.height, TOL, "room height");
            }
            (ElementKind::Column(a), ElementKind::Column(b)) => {
                close2(a.position, b.position, TOL, "column position");
                let (b0, t0) = original.column_span(e, a);
                let (b1, t1) = imported.column_span(i, b);
                close(b0, b1, TOL, "column base");
                close(t0, t1, TOL, "column top");
                let section = |p: &Project, x: &Element| match p.element_type(x.type_id).map(|t| &t.data) {
                    Some(TypeData::Column { section, .. }) => *section,
                    _ => panic!("column without column type"),
                };
                match (section(original, e), section(&imported, i)) {
                    (ColumnSection::Rectangular { .. }, ColumnSection::Rectangular { .. }) => {
                        let diff = (a.rotation_deg - b.rotation_deg).rem_euclid(180.0);
                        assert!(
                            diff < 1e-6 || diff > 180.0 - 1e-6,
                            "column rotation {} vs {}",
                            a.rotation_deg,
                            b.rotation_deg
                        );
                    }
                    (ColumnSection::Round { diameter: d0 }, ColumnSection::Round { diameter: d1 }) => {
                        close(d0, d1, TOL, "diameter")
                    }
                    (x, y) => panic!("section {x:?} came back as {y:?}"),
                }
            }
            (ElementKind::Component(a), ElementKind::Component(b)) => {
                close2(a.position, b.position, TOL, "component position");
                let diff = (a.rotation_deg - b.rotation_deg).rem_euclid(360.0);
                assert!(diff < 1e-6 || diff > 360.0 - 1e-6, "rotation {} vs {}", a.rotation_deg, b.rotation_deg);
                close(a.base_offset, b.base_offset, TOL, "component base");
                let in_base = Project::from_template().types.values().any(|t| t.name == original.type_name(e.type_id));
                if in_base {
                    // The parametric type is reused, with the instance overrides.
                    assert!(matches!(
                        imported.element_type(i.type_id).map(|t| &t.data),
                        Some(TypeData::Component { def, .. }) if !def.params.is_empty()
                    ));
                    assert_eq!(a.overrides.keys().collect::<Vec<_>>(), b.overrides.keys().collect::<Vec<_>>());
                    for (k, v) in &a.overrides {
                        close(*v, b.overrides[k], TOL, "override");
                    }
                }
                // Either way the pieces end up in the same place.
                let bounds = |p: &Project, x: &Element, c: &cc_core::Component| {
                    cc_geom::component_parts(p, x, c)
                        .iter()
                        .flat_map(|q| {
                            q.footprint
                                .iter()
                                .map(|v| v.extend(q.z0))
                                .chain(q.footprint.iter().map(|v| v.extend(q.z1)))
                                .collect::<Vec<_>>()
                        })
                        .fold(
                            (glam::DVec3::splat(f64::INFINITY), glam::DVec3::splat(f64::NEG_INFINITY)),
                            |(lo, hi), v| (lo.min(v), hi.max(v)),
                        )
                };
                let (lo0, hi0) = bounds(original, e, a);
                let (lo1, hi1) = bounds(&imported, i, b);
                assert!(
                    (lo0 - lo1).abs().max_element() < 1e-5 && (hi0 - hi1).abs().max_element() < 1e-5,
                    "{lo0} {hi0} vs {lo1} {hi1}"
                );
            }
            (ElementKind::Beam(a), ElementKind::Beam(b)) => {
                close2(a.start, b.start, TOL, "beam start");
                close2(a.end, b.end, TOL, "beam end");
                close(a.top_offset, b.top_offset, TOL, "beam top");
            }
            (x, y) => panic!("{x:?} came back as {y:?}"),
        }
    }
    // The imported project survives its own export and repair.
    let mut again = imported.clone();
    assert_eq!(again.repair(), 0, "imported project needed repairs");
    let text = export(&again, &ExportOptions::default());
    assert!(original.walls().next().is_none() || text.contains("IFCWALL("));
}

#[test]
fn sample_house_round_trips() {
    check(&load(include_str!("../../../examples/casa_simples.ccad")));
}

#[test]
fn feature_fixture_round_trips() {
    check(&load(include_str!("fixtures/features.ccad")));
}

#[test]
fn arc_walls_round_trip() {
    use cc_core::{Document, Hosted, Wall};
    // A half circle (with a window) closed by a straight wall, and a free
    // clockwise quarter circle drawn on its exterior face.
    let mut doc = Document::new(Project::from_template());
    doc.edit("arcs", |e| {
        let level = e.project().levels[0].id;
        let add = |e: &mut cc_core::EditCtx, category, kind| {
            let type_id = e.project().default_type(category).unwrap();
            let id = cc_core::ElementId(e.alloc_id());
            e.add_element(Element {
                id,
                level,
                type_id,
                mark: "A".into(),
                comments: String::new(),
                kind,
                uid: Uid::default(),
                phase_created: Default::default(),
                phase_demolished: None,
            })
        };
        let arc = Wall { start: DVec2::new(4.0, 0.0), end: DVec2::new(-4.0, 0.0), bulge: 1.0, ..Default::default() };
        let arc = add(e, Category::Wall, ElementKind::Wall(arc));
        let chord = Wall { start: DVec2::new(-4.0, 0.0), end: DVec2::new(4.0, 0.0), ..Default::default() };
        add(e, Category::Wall, ElementKind::Wall(chord));
        let window = Hosted { host: arc, offset: 3.0, sill_height: 1.0, flip_facing: false, flip_hand: false };
        add(e, Category::Window, ElementKind::Window(window));
        let free = Wall {
            start: DVec2::new(10.0, 0.0),
            end: DVec2::new(13.0, -3.0),
            bulge: -(std::f64::consts::PI / 8.0).tan(),
            location_line: cc_core::LocationLine::FinishExterior,
            ..Default::default()
        };
        add(e, Category::Wall, ElementKind::Wall(free));
    });
    let original = doc.project;
    check(&original);
    // The export carries arc axes and arc profiles.
    let text =
        export(&original, &ExportOptions { time_stamp: Some("2026-01-01T00:00:00".into()), file_name: "t.ifc".into() });
    assert_eq!(text.matches("IFCTRIMMEDCURVE").count(), 2);
    assert!(text.contains("IFCARCINDEX"));
    // Imported walls have the same volumes (the window cut out of its arc).
    let (imported, _) = round_trip(&original);
    let (a, b) = (cc_geom::ModelGeometry::build(&original), cc_geom::ModelGeometry::build(&imported));
    let volumes = |g: &cc_geom::ModelGeometry, p: &Project| {
        let mut v: Vec<(Uid, f64)> = p.walls().map(|(e, _)| (e.uid, g.volume(e.id))).collect();
        v.sort_by_key(|(uid, _)| *uid);
        v
    };
    for ((ua, va), (ub, vb)) in volumes(&a, &original).iter().zip(&volumes(&b, &imported)) {
        assert_eq!(ua, ub);
        close(*va, *vb, 1e-6, "arc wall volume");
    }
}

#[test]
fn roof_fixture_round_trips() {
    check(&load(include_str!("fixtures/roofs.ccad")));
}

#[test]
fn arc_fixture_round_trips() {
    check(&load(include_str!("fixtures/arcs.ccad")));
}

#[test]
fn reimport_reuses_template_types() {
    let original = load(include_str!("../../../examples/casa_simples.ccad"));
    let (imported, _) = round_trip(&original);
    // Types with the same name and size are not duplicated.
    for category in [Category::Wall, Category::Door, Category::Window] {
        let mut names: Vec<&str> = imported.types_of(category).map(|t| t.name.as_str()).collect();
        let n = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), n, "duplicate {category:?} type names");
    }
}

#[test]
fn framed_beams_round_trip() {
    use cc_core::{Beam, Column, Document, WallTop};
    // A column with a beam framed into it, mitered where it meets a second beam.
    let mut doc = Document::new(Project::from_template());
    doc.edit("framing", |e| {
        let level = e.project().levels[0].id;
        let add = |e: &mut cc_core::EditCtx, category, kind| {
            let type_id = e.project().default_type(category).unwrap();
            let id = cc_core::ElementId(e.alloc_id());
            e.add_element(Element {
                id,
                level,
                type_id,
                mark: String::new(),
                comments: String::new(),
                kind,
                uid: Uid::default(),
                phase_created: Default::default(),
                phase_demolished: None,
            });
        };
        let column = Column {
            position: DVec2::ZERO,
            rotation_deg: 0.0,
            base_offset: 0.0,
            height: 3.0,
            top: WallTop::Unconnected,
            structural: true,
        };
        add(e, Category::Column, ElementKind::Column(column));
        add(e, Category::Beam, ElementKind::Beam(Beam::new(DVec2::ZERO, DVec2::new(4.0, 0.0), 3.0)));
        add(e, Category::Beam, ElementKind::Beam(Beam::new(DVec2::new(4.0, 0.0), DVec2::new(4.0, 3.0), 3.0)));
        // Away from the frame (the check expects a wall).
        let wall = cc_core::Wall { start: DVec2::new(10.0, 0.0), end: DVec2::new(12.0, 0.0), ..Default::default() };
        add(e, Category::Wall, ElementKind::Wall(wall));
    });
    let original = doc.project;
    let before = cc_geom::ModelGeometry::build(&original);
    assert!(before.beams.values().any(|g| !g.is_square()), "a mitered beam");
    check(&original);
    // The imported beams frame the same way.
    let (imported, _) = round_trip(&original);
    let after = cc_geom::ModelGeometry::build(&imported);
    let footprints = |g: &cc_geom::ModelGeometry, p: &Project| {
        let mut v: Vec<(Uid, [DVec2; 4])> =
            g.beams.values().filter_map(|b| Some((p.element(b.element)?.uid, b.footprint()))).collect();
        v.sort_by_key(|(uid, _)| *uid);
        v
    };
    let (a, b) = (footprints(&before, &original), footprints(&after, &imported));
    assert_eq!(a.len(), 2);
    for ((ua, fa), (ub, fb)) in a.iter().zip(&b) {
        assert_eq!(ua, ub);
        for (p, q) in fa.iter().zip(fb) {
            close2(*p, *q, 1e-6, "framed beam corner");
        }
    }
}

/// A floor with a shaft and a stairwell, and a foundation slab, come back
/// as native elements; the floor's openings are voids of its profile.
#[test]
fn floor_openings_and_foundation_slabs_round_trip() {
    let mut project = Project::from_template();
    let level = project.levels[0].id;
    let floor_t = project.default_type(Category::Floor).unwrap();
    let slab_t = project
        .types_of(Category::Foundation)
        .find(|t| matches!(t.data, TypeData::Foundation { kind: cc_core::FoundationKind::Slab, .. }))
        .map(|t| t.id)
        .unwrap();
    let square = |x: f64, y: f64, s: f64| {
        vec![DVec2::new(x, y), DVec2::new(x + s, y), DVec2::new(x + s, y + s), DVec2::new(x, y + s)]
    };
    let mut floor = cc_core::Floor::new(square(0.0, 0.0, 10.0), 3.0);
    floor.openings = vec![square(1.0, 1.0, 2.0), square(5.0, 5.0, 3.0)];
    let raft =
        cc_core::Foundation { shape: FoundationShape::Slab { boundary: square(0.0, 0.0, 12.0) }, top_offset: -0.1 };
    for (type_id, kind) in [(floor_t, ElementKind::Floor(floor)), (slab_t, ElementKind::Foundation(raft))] {
        let id = cc_core::ElementId(project.alloc_id());
        let e = Element {
            id,
            level,
            type_id,
            mark: String::new(),
            comments: String::new(),
            kind,
            uid: Uid::generate(),
            phase_created: Default::default(),
            phase_demolished: None,
        };
        project.elements.insert(id, e);
    }
    let text = export(&project, &ExportOptions::default());
    assert!(text.contains("IFCARBITRARYPROFILEDEFWITHVOIDS"));
    assert!(text.contains(".BASESLAB."));
    check(&project);
}

#[test]
fn structure_fixture_exports_foundations_profiles_and_cut_slabs() {
    let project = load(include_str!("fixtures/structure.ccad"));
    let text = export(&project, &ExportOptions::default());
    let lines = |entity: &str| text.lines().filter(|l| l.contains(&format!("={entity}("))).collect::<Vec<_>>();
    // Pads under the four columns and strips under the four walls; a raft as a base slab.
    let footings = lines("IFCFOOTING");
    assert_eq!(footings.len(), 8);
    assert_eq!(footings.iter().filter(|l| l.contains(".PAD_FOOTING.")).count(), 4);
    assert!(lines("IFCSLAB").iter().any(|l| l.contains(".BASESLAB.")));
    // Steel profiles as parametric profiles.
    for profile in [
        "IFCISHAPEPROFILEDEF",
        "IFCUSHAPEPROFILEDEF",
        "IFCLSHAPEPROFILEDEF",
        "IFCRECTANGLEHOLLOWPROFILEDEF",
        "IFCCIRCLEHOLLOWPROFILEDEF",
    ] {
        assert!(!lines(profile).is_empty(), "{profile}");
    }
    // The sloped beams and the slab cut by its supports are B-reps, in `Brep` representations only.
    let breps: Vec<String> =
        lines("IFCFACETEDBREP").iter().filter_map(|l| l.split('=').next()).map(str::to_string).collect();
    assert!(!breps.is_empty());
    let uses = |l: &str| breps.iter().any(|b| l.contains(&format!("{b},")) || l.contains(&format!("{b})")));
    let representations = lines("IFCSHAPEREPRESENTATION");
    assert!(representations.iter().filter(|l| uses(l)).all(|l| l.contains("'Brep'")));
}

/// Phases of an element by name: (created, demolished).
fn phase_names(p: &Project, e: &Element) -> (String, Option<String>) {
    let created = p.phases.get(p.created_index(e)).map(|ph| ph.name.clone()).unwrap_or_default();
    (created, e.phase_demolished.map(|d| p.phase_name(d)))
}

#[test]
fn phases_round_trip() {
    let original = load(include_str!("fixtures/phases.ccad"));
    check(&original);
    let text = export(&original, &ExportOptions::default());
    // Statuses relative to the last phase, in the common property sets.
    for status in ["'EXISTING'", "'DEMOLISH'", "'NEW'", "'TEMPORARY'"] {
        assert!(text.contains(&format!("IFCPROPERTYENUMERATEDVALUE('Status',$,(IFCLABEL({status}))")), "{status}");
    }
    let (imported, _) = import(&text, Project::from_template()).unwrap();
    let names = |p: &Project| p.phases.iter().map(|ph| ph.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&imported), names(&original));
    let lookup = by_uid(&imported);
    let mut checked = 0;
    for e in original.elements.values().filter(|e| e.category().is_phased()) {
        let Some(i) = lookup.get(&e.uid) else { continue };
        assert_eq!(phase_names(&imported, i), phase_names(&original, e), "phases of {:?} {}", e.category(), e.id);
        checked += 1;
    }
    assert!(checked >= 12, "{checked}");
    assert_eq!(imported.clone().repair(), 0);
}

#[test]
fn ifc_status_maps_to_phases() {
    // Without this program's phase sets, `Status` alone decides: existing
    // elements are built in the phase before the last, demolished ones are
    // demolished in the last, new and temporary ones are built in it.
    let original = load(include_str!("fixtures/phases.ccad"));
    let text = export(&original, &ExportOptions::default())
        .replace("'ConstructCAD_Phasing'", "'Other_Phasing'")
        .replace("'ConstructCAD_Phases'", "'Other_Phases'");
    let (imported, _) = import(&text, Project::from_template()).unwrap();
    assert_eq!(imported.phases.len(), 2);
    let (first, last) = (imported.phases[0].id, imported.phases[1].id);
    let lookup = by_uid(&imported);
    let mut seen = std::collections::BTreeSet::new();
    for e in original.elements.values().filter(|e| matches!(e.category(), Category::Wall | Category::Door)) {
        let i = lookup[&e.uid];
        let expected = match cc_ifc::ifc_status(&original, e).unwrap() {
            cc_core::PhaseStatus::Existing => (first, None),
            cc_core::PhaseStatus::Demolished => (first, Some(last)),
            cc_core::PhaseStatus::New => (last, None),
            cc_core::PhaseStatus::Temporary => (last, Some(last)),
        };
        seen.insert(cc_ifc::ifc_status(&original, e).unwrap());
        assert_eq!((i.phase_created, i.phase_demolished), expected, "{:?} {}", e.category(), e.id);
    }
    assert_eq!(seen.len(), 4);
    // The status is read as phases, not kept as an imported property.
    assert!(imported.elements.values().all(|e| match &e.kind {
        ElementKind::Component(c) => c.properties.iter().all(|s| s.properties.iter().all(|p| p.name != "Status")),
        _ => true,
    }));
}

/// Roof framing: each member is an `IfcMember` aggregated in its roof, and
/// the covering and framing come back on the roof's type, the members drawn
/// again rather than imported as objects of their own.
#[test]
fn roof_framing_round_trips() {
    use cc_core::roofing::{FramingSystem, TileKind};
    let mut original = load(include_str!("fixtures/roofs.ccad"));
    original.materials.push(cc_core::Material::new("Pinus", [190, 150, 100]));
    for t in original.types.values_mut() {
        if let TypeData::Roof { covering, framing, .. } = &mut t.data {
            *covering = Some(TileKind::CeramicRoman);
            framing.system = FramingSystem::TimberTrusses;
            framing.material = "Pinus".into();
            framing.truss_spacing = 2.4;
            framing.rafter = cc_core::roofing::MemberSize::new(0.05, 0.07);
        }
    }
    let geom = cc_geom::ModelGeometry::build(&original);
    let members: usize = geom.roof_framing.values().map(|g| g.members.len()).sum();
    assert!(members > 0, "the fixture's roofs are framed");
    let text = export(&original, &ExportOptions { time_stamp: None, file_name: "t.ifc".into() });
    assert_eq!(text.matches("IFCMEMBER(").count(), members);
    assert!(
        text.contains(".PURLIN.") && text.contains(".RAFTER.") && text.contains(".CHORD.") && text.contains(".POST.")
    );
    assert!(text.contains("'ConstructCAD_Roofing'"));

    let (imported, report) = import(&text, Project::from_template()).unwrap();
    assert!(!report.skipped.contains_key("IFCMEMBER"), "{:?}", report.skipped);
    let count = |p: &Project, c: Category| p.elements.values().filter(|e| e.category() == c).count();
    assert_eq!(count(&imported, Category::Roof), count(&original, Category::Roof));
    assert_eq!(count(&imported, Category::Component), count(&original, Category::Component), "no members as objects");
    for e in imported.elements.values().filter(|e| e.category() == Category::Roof) {
        let Some(TypeData::Roof { covering, framing, .. }) = imported.element_type(e.type_id).map(|t| &t.data) else {
            panic!("a roof type");
        };
        assert_eq!(*covering, Some(TileKind::CeramicRoman));
        assert_eq!((framing.system, framing.material.as_str()), (FramingSystem::TimberTrusses, "Pinus"));
        close(framing.truss_spacing, 2.4, 1e-9, "truss spacing");
        close(framing.rafter.height, 0.07, 1e-9, "rafter height");
    }
    assert!(imported.materials.iter().any(|m| m.name == "Pinus"));
    let geom = cc_geom::ModelGeometry::build(&imported);
    assert_eq!(geom.roof_framing.values().map(|g| g.members.len()).sum::<usize>(), members);
}
