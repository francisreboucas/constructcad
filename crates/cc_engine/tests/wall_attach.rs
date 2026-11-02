//! Walls attached to a roof (issue #140): under the roof a wall rises or
//! drops to its underside, following its slope; elsewhere it keeps its top.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::{ErrorCode, Session};
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().unwrap_or_else(|| panic!("no id in {v}"))
}

fn volume(s: &mut Session, id: u64) -> f64 {
    run(s, "element_get", json!({"id": id}))["derived"]["volume"].as_f64().unwrap()
}

/// The highest point of an element's solids.
fn top(s: &mut Session, id: u64) -> f64 {
    s.geometry()
        .solids_of(cc_core::ElementId(id))
        .flat_map(|solid| solid.faces.iter().flat_map(|f| f.verts.iter().map(|p| p.z)))
        .fold(f64::NEG_INFINITY, f64::max)
}

/// Four 3 m walls round an 8 × 6 m house and a gable roof on them (ridge
/// along x, half pitch, no overhang): the gable ends are x = 0 and x = 8.
fn house(s: &mut Session, height: f64) -> (Vec<u64>, u64) {
    let corners = [[0.0, 0.0], [8.0, 0.0], [8.0, 6.0], [0.0, 6.0]];
    let walls = (0..4)
        .map(|i| {
            id(&run(s, "wall_create", json!({"start": corners[i], "end": corners[(i + 1) % 4], "height": height})))
        })
        .collect();
    let roof = id(&run(s, "roof_create", json!({"boundary": corners, "offset": 3.0, "overhang": 0.0})));
    run(s, "element_set", json!({"id": roof, "params": {"shape": {"Gable": {"slope": 0.5, "ridge_deg": 0.0}}}}));
    (walls, roof)
}

#[test]
fn gable_walls_rise_to_the_roof() {
    let mut s = Session::default();
    let (walls, roof) = house(&mut s, 3.0);
    let (eave, gable) = (walls[0], walls[1]);
    let before = volume(&mut s, gable);
    assert!((top(&mut s, gable) - 3.0).abs() < 1e-6);
    let depth = s.doc.undo_depth();
    let out = run(&mut s, "wall_attach", json!({"ids": walls, "top": roof}));
    assert_eq!(out["top"].as_u64(), Some(roof));
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    // The gable wall ends in a triangle up to the ridge's underside (3 m + 1.5 m at mid-span).
    let ridge = top(&mut s, gable);
    assert!(ridge > 4.3 && ridge < 4.51, "{ridge}");
    let thickness = run(&mut s, "element_get", json!({"id": gable}))["derived"]["thickness"].as_f64().unwrap();
    let gained = volume(&mut s, gable) - before;
    let triangle = 6.0 * 1.5 / 2.0 * thickness;
    assert!((gained - triangle).abs() < 0.25 * triangle, "{gained} vs {triangle}");
    // The eave wall barely changes: the roof starts at its top.
    assert!(top(&mut s, eave) < 3.2, "{}", top(&mut s, eave));
    assert_eq!(run(&mut s, "element_get", json!({"id": gable}))["params"]["attach_top"].as_u64(), Some(roof));

    // The roof steeper: the wall follows.
    run(&mut s, "element_set", json!({"id": roof, "params": {"shape": {"Gable": {"slope": 1.0, "ridge_deg": 0.0}}}}));
    let steep = top(&mut s, gable);
    assert!(steep > 5.8 && steep < 6.01, "{steep}");

    // Detached, the wall is back to its own top; undo attaches it again.
    run(&mut s, "wall_attach", json!({"ids": [gable], "detach": true}));
    assert!((top(&mut s, gable) - 3.0).abs() < 1e-6);
    assert!((volume(&mut s, gable) - before).abs() < 1e-9);
    run(&mut s, "undo", json!({}));
    assert!(top(&mut s, gable) > 5.8);

    // The roof deleted: the walls keep their own tops (and saving drops the reference).
    run(&mut s, "elements_delete", json!({"ids": [roof]}));
    assert!((top(&mut s, gable) - 3.0).abs() < 1e-6);
}

#[test]
fn tall_walls_drop_to_the_roof_and_bad_targets_are_refused() {
    let mut s = Session::default();
    // Walls 5 m tall under a roof whose eaves are at 3 m: attached, they drop to it.
    let (walls, roof) = house(&mut s, 5.0);
    run(&mut s, "wall_attach", json!({"ids": walls, "top": roof}));
    let eave = walls[0];
    assert!(top(&mut s, eave) < 3.3, "{}", top(&mut s, eave));
    assert!(volume(&mut s, eave) > 0.0);

    let depth = s.doc.undo_depth();
    for args in [
        json!({"ids": [walls[0]], "top": walls[1]}),
        json!({"ids": [roof], "top": roof}),
        json!({"ids": [walls[0]]}),
        json!({"ids": [walls[0]], "top": roof, "detach": true}),
        json!({"ids": [], "top": roof}),
    ] {
        assert_eq!(fails(&mut s, "wall_attach", args.clone()), ErrorCode::InvalidArguments, "{args}");
    }
    assert_eq!(s.doc.undo_depth(), depth);
}
