//! SketchUp import: synthetic .skp files written by `cc_skp::create` become
//! component types and elements with the sizes, positions and rotations the
//! file gives.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{ComponentShape, ElementId, ElementKind, TypeData};
use cc_engine::{ErrorCode, Session};
use cc_skp::create::{FaceOptions, GroupOptions, InstanceOptions, create};
use serde_json::{Value, json};

const INCH: f64 = 0.0254;

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ccad-skp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// The six faces of a box from the origin to `(w, d, h)` inches, wound outwards.
fn box_faces(w: f64, d: f64, h: f64) -> [Vec<[f64; 3]>; 6] {
    [
        vec![[0.0, 0.0, 0.0], [0.0, d, 0.0], [w, d, 0.0], [w, 0.0, 0.0]],
        vec![[0.0, 0.0, h], [w, 0.0, h], [w, d, h], [0.0, d, h]],
        vec![[0.0, 0.0, 0.0], [w, 0.0, 0.0], [w, 0.0, h], [0.0, 0.0, h]],
        vec![[0.0, d, 0.0], [0.0, d, h], [w, d, h], [w, d, 0.0]],
        vec![[0.0, 0.0, 0.0], [0.0, 0.0, h], [0.0, d, h], [0.0, d, 0.0]],
        vec![[w, 0.0, 0.0], [w, d, 0.0], [w, d, h], [w, 0.0, h]],
    ]
}

/// A model with a 10 x 20 x 30 in box component (bottom painted red) placed
/// four times (moved, rotated a quarter turn, doubled in size, hidden), a
/// group holding one face and a loose face at the root.
fn sample(path: &std::path::Path) {
    let mut b = create();
    let red = b.add_material("Red", [255, 0, 0], None).unwrap();
    let mut d = b.add_component_definition("Box", &Default::default()).unwrap();
    for (i, face) in box_faces(10.0, 20.0, 30.0).iter().enumerate() {
        let options = FaceOptions { material: (i == 0).then_some(red), ..Default::default() };
        d.add_face(face, &options).unwrap();
    }
    let boxed = d.close().unwrap();
    let mut g = b
        .add_group(&GroupOptions { name: Some("Slab".into()), translation: [100.0, 0.0, 0.0], ..Default::default() })
        .unwrap();
    g.add_face(&[[0.0, 0.0, 5.0], [40.0, 0.0, 5.0], [40.0, 40.0, 5.0], [0.0, 40.0, 5.0]], &Default::default()).unwrap();
    g.close().unwrap();
    let place = |translation, rotation, matrix3x3, hidden| InstanceOptions {
        translation,
        rotation,
        matrix3x3,
        hidden,
        ..Default::default()
    };
    let quarter = Some(([0.0, 0.0, 1.0], std::f64::consts::FRAC_PI_2));
    let double = Some([2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0]);
    b.add_instance(boxed, &place([0.0, 0.0, 10.0], None, None, false)).unwrap();
    b.add_instance(boxed, &place([50.0, 0.0, 0.0], quarter, None, false)).unwrap();
    b.add_instance(boxed, &place([200.0, 0.0, 0.0], None, double, false)).unwrap();
    b.add_instance(boxed, &place([300.0, 0.0, 0.0], None, None, true)).unwrap();
    b.add_face(&[[0.0, 0.0, -1.0], [-10.0, 0.0, -1.0], [-10.0, -10.0, -1.0]], &Default::default()).unwrap();
    b.save(path).unwrap();
}

/// Bounds and signed volume of a type's mesh pieces.
fn mesh_of(s: &Session, type_id: u64) -> ([f64; 3], [f64; 3], f64, usize) {
    let Some(TypeData::Component { def, .. }) =
        s.project().element_type(cc_core::ElementTypeId(type_id)).map(|t| &t.data)
    else {
        panic!("type {type_id} is not a component");
    };
    let (mut lo, mut hi, mut volume) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3], 0.0);
    for item in &def.items {
        let ComponentShape::Mesh { vertices, faces } = &item.shape else { panic!("not a mesh") };
        for v in vertices {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        for f in faces {
            let p = |i: u32| glam::DVec3::from_array(vertices[i as usize]);
            for k in 1..f.len() - 1 {
                volume += p(f[0]).dot(p(f[k]).cross(p(f[k + 1]))) / 6.0;
            }
        }
    }
    (lo, hi, volume, def.items.len())
}

fn component(s: &Session, id: u64) -> (u64, cc_core::Component) {
    let e = s.project().element(ElementId(id)).unwrap();
    let ElementKind::Component(c) = &e.kind else { panic!("not a component") };
    (e.type_id.0, c.clone())
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn skp_model_becomes_components() {
    let path = scratch("sample.skp");
    sample(&path);
    let mut s = Session::default();
    let before = s.project().elements.len();
    let out = run(&mut s, "skp_import", json!({ "path": path }));
    let elements: Vec<u64> = out["elements"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    // Loose faces, the group and three boxes; the hidden box is left out.
    assert_eq!(elements.len(), 5, "{out}");
    assert_eq!(out["types"].as_array().unwrap().len(), 4, "{out}");
    assert_eq!(out["hidden"], 1);
    assert!(s.project().materials.iter().any(|m| m.name == "Red" && m.color == [255, 0, 0]));

    let (loose_type, loose) = component(&s, elements[0]);
    assert_eq!(s.project().element_type(cc_core::ElementTypeId(loose_type)).unwrap().name, "sample");
    assert!(close(loose.rotation_deg, 0.0) && loose.position.length() < 1e-9);
    let (_, hi, _, _) = mesh_of(&s, loose_type);
    assert!(close(hi[2], -INCH), "{hi:?}");

    // The group: one type named after the group, at its translation.
    let (slab_type, slab) = component(&s, elements[1]);
    assert_eq!(s.project().element_type(cc_core::ElementTypeId(slab_type)).unwrap().name, "Slab");
    assert!(close(slab.position.x, 100.0 * INCH));

    // A moved box and a turned box share a type; its pieces are the red bottom and the rest.
    let (box_type, first) = component(&s, elements[2]);
    let (turned_type, turned) = component(&s, elements[3]);
    assert_eq!(box_type, turned_type);
    assert!(close(first.base_offset, 10.0 * INCH) && close(first.rotation_deg, 0.0));
    assert!(close(turned.position.x, 50.0 * INCH) && close(turned.rotation_deg, 90.0), "{turned:?}");
    let (lo, hi, volume, pieces) = mesh_of(&s, box_type);
    assert_eq!(pieces, 2);
    assert!(lo.iter().all(|v| close(*v, 0.0)), "{lo:?}");
    assert!(close(hi[0], 10.0 * INCH) && close(hi[1], 20.0 * INCH) && close(hi[2], 30.0 * INCH), "{hi:?}");
    // Faces wound outwards: a positive volume.
    assert!(close(volume, 6000.0 * INCH.powi(3)), "{volume}");

    // The doubled box has its own type, twice the size.
    let (big_type, _) = component(&s, elements[4]);
    assert_ne!(big_type, box_type);
    let (_, hi, volume, _) = mesh_of(&s, big_type);
    assert!(close(hi[2], 60.0 * INCH) && close(volume, 48000.0 * INCH.powi(3)), "{hi:?} {volume}");
    assert_eq!(s.project().element_type(cc_core::ElementTypeId(big_type)).unwrap().name, "Box 2");

    // One undo step.
    run(&mut s, "undo", json!({}));
    assert_eq!(s.project().elements.len(), before);
}

#[test]
fn skp_import_moves_and_turns_the_model() {
    let path = scratch("moved.skp");
    sample(&path);
    let mut s = Session::default();
    let out = run(&mut s, "skp_import", json!({ "path": path, "position": [10.0, 5.0], "rotation": 90.0 }));
    let id = out["elements"][1].as_u64().unwrap();
    let (_, slab) = component(&s, id);
    // The group's origin (100 in along x) turns onto the y axis.
    assert!(close(slab.position.x, 10.0) && close(slab.position.y, 5.0 + 100.0 * INCH), "{slab:?}");
    assert!(close(slab.rotation_deg, 90.0));
}

#[test]
fn skp_file_loads_as_one_component() {
    let path = scratch("chair.skp");
    sample(&path);
    let mut s = Session::default();
    let out = run(&mut s, "skp_component_load", json!({ "path": path }));
    assert_eq!(out["component"], "chair");
    let types = out["types"].as_array().unwrap();
    assert_eq!(types.len(), 1);
    let (lo, hi, _, _) = mesh_of(&s, types[0].as_u64().unwrap());
    // From the loose face (-10 in) to the far side of the doubled box (220 in).
    assert!(close(lo[0], -10.0 * INCH) && close(hi[0], 220.0 * INCH), "{lo:?} {hi:?}");
    // Loading twice keeps the names apart.
    let again = run(&mut s, "skp_component_load", json!({ "path": path }));
    assert_eq!(again["component"], "chair 2");
}

#[test]
fn bad_skp_files_are_errors() {
    let mut s = Session::default();
    let garbage = scratch("garbage.skp");
    std::fs::write(&garbage, b"PK\x03\x04 definitely not a model").unwrap();
    let err = s.execute("skp_import", json!({ "path": garbage })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Io, "{err:?}");
    let missing = scratch("missing.skp");
    assert!(s.execute("skp_component_load", json!({ "path": missing })).is_err());
    // A file that is a model but has nothing visible.
    let empty = scratch("empty.skp");
    let mut b = create();
    b.add_face(
        &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]],
        &FaceOptions { hidden: true, ..Default::default() },
    )
    .unwrap();
    b.save(&empty).unwrap();
    let err = s.execute("skp_import", json!({ "path": empty })).unwrap_err();
    assert_eq!(err.code, ErrorCode::Rejected, "{err:?}");
}
