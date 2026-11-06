//! Terrain surfaces: created from points and a boundary, edited point by
//! point, shown with their figures, cut in sections and listed in schedules.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::ViewId;
use cc_engine::{ErrorCode, Session};
use cc_views::Prim;
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

fn rect(w: f64, h: f64) -> Value {
    json!([[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]])
}

/// A slope from 0 at x = 0 to 2 m at x = 20, over a 20 x 10 m lot.
fn slope_points() -> Value {
    json!([[0.0, 0.0, 0.0], [20.0, 0.0, 2.0], [20.0, 10.0, 2.0], [0.0, 10.0, 0.0], [10.0, 5.0, 1.0]])
}

fn derived(s: &mut Session, id: u64) -> Value {
    run(s, "element_get", json!({"id": id}))["derived"].clone()
}

#[test]
fn terrain_is_created_with_its_figures_and_one_undo_step() {
    let mut s = Session::default();
    let depth = s.doc.undo_depth();
    let t = id(&run(&mut s, "topography_create", json!({"points": slope_points(), "boundary": rect(20.0, 10.0)})));
    assert_eq!(s.doc.undo_depth(), depth + 1);
    let d = derived(&mut s, t);
    assert_eq!((d["min_z"].as_f64(), d["max_z"].as_f64()), (Some(0.0), Some(2.0)), "{d}");
    assert_eq!(d["base_z"].as_f64(), Some(-2.0));
    assert!((d["area"].as_f64().unwrap() - 200.0).abs() < 1e-9);
    // Mean height 1 m above 0, plus the 2 m below: 3 m over 200 m².
    assert!((d["volume"].as_f64().unwrap() - 600.0).abs() < 1e-6, "{d}");
    assert_eq!(d["warnings"], json!([]));
    // The template's terrain type and earth material.
    let e = run(&mut s, "element_get", json!({"id": t}));
    assert_eq!(e["category"], json!("topography"), "{e}");

    // A schedule lists it with its area and volume.
    let schedule = run(&mut s, "view_create", json!({"kind": "schedule", "category": "topography"}));
    let table = run(&mut s, "schedule_table", json!({"view": schedule["id"]}));
    assert_eq!(table["rows"].as_array().unwrap().len(), 1, "{table}");

    // With no boundary, the convex hull of the points; with too few points, an error.
    let hull = id(&run(&mut s, "topography_create", json!({"points": slope_points()})));
    assert!((derived(&mut s, hull)["area"].as_f64().unwrap() - 200.0).abs() < 1e-9);
    assert_eq!(
        fails(&mut s, "topography_create", json!({"points": [[0, 0, 0], [1, 1, 1]]})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(&mut s, "topography_create", json!({"points": [[0, 0, "x"]], "boundary": rect(1.0, 1.0)})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(&mut s, "topography_create", json!({"boundary": rect(1.0, 1.0), "base_depth": -1})),
        ErrorCode::InvalidArguments
    );
}

#[test]
fn a_project_without_a_terrain_type_gets_one_and_an_earth_material() {
    // A project (an older file) without a terrain type or an earth material.
    let mut p = cc_core::Project::from_template();
    p.types.retain(|_, t| t.category() != cc_core::Category::Topography);
    p.materials.retain(|m| m.identity.class != cc_core::material::MaterialClass::Earth);
    // As a file loads: the default-type reference to the removed type goes.
    p.repair();
    let mut s = Session::new(cc_core::Document::new(p));
    let t = id(&run(&mut s, "topography_create", json!({"boundary": rect(5.0, 5.0)})));
    let e = run(&mut s, "element_get", json!({"id": t}));
    let ty = run(&mut s, "type_get", json!({"type": e["type"]["id"]}));
    let material = ty["params"]["material"].as_str().unwrap().to_string();
    let materials = run(&mut s, "materials_list", json!({}));
    assert!(materials.as_array().unwrap().iter().any(|m| m["name"] == json!(material)), "{materials}");
    // No points: a flat surface at the level, with a warning.
    assert_eq!(derived(&mut s, t)["warnings"], json!(["few-points"]));

    // A type of its own, with contours every half meter.
    let ty = run(&mut s, "topography_type_create", json!({"name": "Lote", "contour_interval": 0.5, "major_every": 4}));
    let got = run(&mut s, "type_get", json!({"type": ty["id"]}));
    assert_eq!(got["params"]["contour_interval"], json!(0.5), "{got}");
    assert_eq!(
        fails(&mut s, "topography_type_create", json!({"contour_interval": 0.001})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(fails(&mut s, "topography_type_create", json!({"major_every": 0})), ErrorCode::InvalidArguments);
}

#[test]
fn points_are_edited_by_index() {
    let mut s = Session::default();
    let t = id(&run(&mut s, "topography_create", json!({"points": slope_points(), "boundary": rect(20.0, 10.0)})));
    // Raise the middle point, drop a corner, add one; a repeated point is dropped.
    let out = run(
        &mut s,
        "topography_points_set",
        json!({"id": t, "set": [{"index": 4, "z": 3.0}], "remove": [0], "add": [[5.0, 5.0, 1.0], [5.0, 5.0, 9.0]]}),
    );
    assert_eq!((out["points"].as_u64(), out["dropped"].as_u64()), (Some(5), Some(1)), "{out}");
    let points = run(&mut s, "element_get", json!({"id": t}))["params"]["points"].clone();
    let points = points.as_array().unwrap();
    assert_eq!(points.len(), 5);
    assert_eq!(points[3], json!([10.0, 5.0, 3.0]));
    assert_eq!(derived(&mut s, t)["max_z"].as_f64(), Some(3.0));
    // Out-of-range indices and bad numbers are refused, the points unchanged.
    let depth = s.doc.undo_depth();
    assert_eq!(fails(&mut s, "topography_points_set", json!({"id": t, "remove": [9]})), ErrorCode::InvalidArguments);
    assert_eq!(
        fails(&mut s, "topography_points_set", json!({"id": t, "set": [{"index": 7, "z": 1.0}]})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(s.doc.undo_depth(), depth);
    assert_eq!(fails(&mut s, "topography_points_set", json!({"id": 999999, "add": [[1, 1, 1]]})), ErrorCode::NotFound);
    // Replace them all.
    run(&mut s, "topography_points_set", json!({"id": t, "points": [[10.0, 5.0, 4.0]]}));
    assert_eq!(derived(&mut s, t)["warnings"], json!(["few-points"]));
    assert_eq!(derived(&mut s, t)["max_z"].as_f64(), Some(4.0));
    // The base depth through element_set; the boundary through sketch_set.
    run(&mut s, "element_set", json!({"id": t, "params": {"base_depth": 5.0}}));
    assert_eq!(derived(&mut s, t)["base_z"].as_f64(), Some(-1.0));
    run(
        &mut s,
        "sketch_set",
        json!({"id": t, "curves": [{"kind": "rectangle", "corner": [0, 0], "opposite": [10, 10]}]}),
    );
    assert!((derived(&mut s, t)["area"].as_f64().unwrap() - 100.0).abs() < 1e-9);
}

#[test]
fn a_boundary_and_points_edited_together_are_one_step_or_none() {
    let mut s = Session::default();
    let t = id(&run(&mut s, "topography_create", json!({"points": slope_points(), "boundary": rect(20.0, 10.0)})));
    let square = json!({"id": t, "curves": [{"kind": "rectangle", "corner": [0, 0], "opposite": [10, 10]}]});
    // The points edit is refused: the new boundary is undone with it.
    let depth = s.doc.undo_depth();
    let failed = s.group("edit", |s| {
        s.execute("sketch_set", square.clone())?;
        s.execute("topography_points_set", json!({"id": t, "remove": [99]}))
    });
    assert!(failed.is_err());
    assert_eq!(s.doc.undo_depth(), depth);
    assert!((derived(&mut s, t)["area"].as_f64().unwrap() - 200.0).abs() < 1e-9);
    // Both succeed: one undo step brings back the boundary and the points.
    s.group("edit", |s| {
        s.execute("sketch_set", square.clone())?;
        s.execute("topography_points_set", json!({"id": t, "add": [[5.0, 5.0, 7.0]]}))
    })
    .unwrap();
    assert_eq!(s.doc.undo_depth(), depth + 1);
    assert_eq!(derived(&mut s, t)["max_z"].as_f64(), Some(7.0));
    run(&mut s, "undo", json!({}));
    let d = derived(&mut s, t);
    assert!((d["area"].as_f64().unwrap() - 200.0).abs() < 1e-9, "{d}");
    assert_eq!(d["max_z"].as_f64(), Some(2.0));
}

#[test]
fn sections_cut_the_earth_and_moves_carry_the_points() {
    let mut s = Session::default();
    let t = id(&run(&mut s, "topography_create", json!({"points": slope_points(), "boundary": rect(20.0, 10.0)})));
    let section = run(&mut s, "view_create", json!({"kind": "section", "start": [-1.0, 5.0], "end": [21.0, 5.0]}));
    let view = ViewId(id(&section));
    let d = s.drawing(view).unwrap();
    let cut =
        d.prims.iter().filter(|p| matches!(p, Prim::Fill { element: Some(e), cut: true, .. } if e.0 == t)).count();
    assert!(cut > 0, "the section cuts the terrain");
    // Moving the terrain moves its points with the boundary.
    run(&mut s, "elements_move", json!({"ids": [t], "delta": [100.0, 0.0]}));
    let params = run(&mut s, "element_get", json!({"id": t}))["params"].clone();
    assert_eq!(params["points"][4], json!([110.0, 5.0, 1.0]), "{params}");
    assert_eq!(params["boundary"][0], json!([100.0, 0.0]));
}
