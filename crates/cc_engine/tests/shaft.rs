//! Shaft openings (issue #30): a shaft cuts the floors, ceilings and roofs
//! it passes through, and the cuts follow it when it moves, changes or goes.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::{ErrorCode, Session};
use cc_views::{LineStyle, Prim};
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

fn square(x: f64, y: f64, s: f64) -> Value {
    json!([[x, y], [x + s, y], [x + s, y + s], [x, y + s]])
}

/// The number of shaft cuts an element keeps.
fn cuts(s: &mut Session, element: u64) -> usize {
    let got = run(s, "element_get", json!({"id": element}));
    got["params"]["cuts"].as_array().map_or(0, Vec::len)
}

#[test]
fn shafts_cut_the_slabs_they_pass_through() {
    let mut s = Session::default();
    let levels = run(&mut s, "levels_list", json!({}));
    let levels: Vec<u64> = levels.as_array().unwrap().iter().map(|l| l["id"].as_u64().unwrap()).collect();
    assert!(levels.len() >= 2, "the template has an upper level");
    let (ground, upper) = (levels[0], levels[1]);
    let slab = square(0.0, 0.0, 10.0);
    let ground_floor = id(&run(&mut s, "floor_create", json!({"boundary": slab, "level": ground})));
    let upper_floor = id(&run(&mut s, "floor_create", json!({"boundary": slab, "level": upper})));
    let ceiling = id(&run(&mut s, "ceiling_create", json!({"boundary": slab, "level": ground})));

    // From the ground floor up to the top level: it cuts the ceiling and the
    // upper floor, not the slab it stands on.
    let depth = s.doc.undo_depth();
    let shaft = id(&run(&mut s, "shaft_create", json!({"outline": square(4.0, 4.0, 2.0), "level": ground})));
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step, the cuts with it");
    assert_eq!((cuts(&mut s, ground_floor), cuts(&mut s, upper_floor), cuts(&mut s, ceiling)), (0, 1, 1));
    let got = run(&mut s, "element_get", json!({"id": shaft}));
    let mut cut: Vec<u64> = got["derived"]["cuts"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    cut.sort();
    let mut expected = vec![upper_floor, ceiling];
    expected.sort();
    assert_eq!(cut, expected);
    assert!((got["derived"]["area"].as_f64().unwrap() - 4.0).abs() < 1e-9);
    // The upper floor loses the shaft's area, and its solid the shaft's volume.
    let floor = run(&mut s, "element_get", json!({"id": upper_floor}));
    let volume = floor["derived"]["volume"].as_f64().unwrap();
    let thickness = volume / 96.0;
    assert!(thickness > 0.05 && thickness < 1.0, "100 m² less 4 m²: {volume}");

    // Moved, the cuts follow; moved across the floor's edge, they go.
    run(&mut s, "elements_move", json!({"ids": [shaft], "delta": [2.0, 0.0]}));
    let floor = run(&mut s, "element_get", json!({"id": upper_floor}));
    assert_eq!(floor["params"]["cuts"][0][0], json!([6.0, 4.0]));
    run(&mut s, "elements_move", json!({"ids": [shaft], "delta": [3.0, 0.0]}));
    assert_eq!((cuts(&mut s, upper_floor), cuts(&mut s, ceiling)), (0, 0));
    run(&mut s, "undo", json!({}));
    assert_eq!(cuts(&mut s, upper_floor), 1);

    // Lowered to below the upper floor, it cuts only the ceiling.
    // (Into the ceiling, but short of the upper floor's underside.)
    let ceiling_at = run(&mut s, "element_get", json!({"id": ceiling}))["params"]["height_offset"].as_f64().unwrap();
    let height = ceiling_at + 0.01;
    run(&mut s, "element_set", json!({"id": shaft, "params": {"top": "Unconnected", "height": height}}));
    assert_eq!((cuts(&mut s, upper_floor), cuts(&mut s, ceiling)), (0, 1));

    // Plans draw it: its outline and diagonals, in the plan of its level.
    let plan = run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]["id"].as_u64().unwrap();
    let d = s.drawing(cc_core::ViewId(plan)).unwrap();
    let mine = |e: &Option<cc_core::ElementId>| *e == Some(cc_core::ElementId(shaft));
    let thin = d
        .prims
        .iter()
        .filter(|p| matches!(p, Prim::Line { style: LineStyle::Thin, element, .. } if mine(element)))
        .count();
    assert_eq!(thin, 2, "two diagonals");

    // Deleted, its cuts go; undo brings them back.
    run(&mut s, "elements_delete", json!({"ids": [shaft]}));
    assert_eq!(cuts(&mut s, ceiling), 0);
    run(&mut s, "undo", json!({}));
    assert_eq!(cuts(&mut s, ceiling), 1);

    // Exported to IFC with the slabs' voids.
    let path = std::env::temp_dir().join(format!("cc_shaft_{}.ifc", std::process::id()));
    run(&mut s, "ifc_export", json!({"path": path.to_string_lossy()}));
    assert!(std::fs::read_to_string(&path).unwrap().contains("IFCSLAB"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn shafts_leave_alone_what_they_do_not_fit_in() {
    let mut s = Session::default();
    let ground = run(&mut s, "levels_list", json!({}))[0]["id"].as_u64().unwrap();
    // A ceiling with a shaft across its edge is not cut; one inside another shaft's cut is not cut twice.
    let ceiling = id(&run(&mut s, "ceiling_create", json!({"boundary": square(0.0, 0.0, 5.0), "level": ground})));
    run(&mut s, "shaft_create", json!({"outline": square(4.0, 1.0, 2.0), "level": ground}));
    assert_eq!(cuts(&mut s, ceiling), 0);
    run(&mut s, "shaft_create", json!({"outline": square(1.0, 1.0, 2.0), "level": ground}));
    run(&mut s, "shaft_create", json!({"outline": square(1.5, 1.5, 1.0), "level": ground}));
    assert_eq!(cuts(&mut s, ceiling), 1);

    // Refused without an undo step.
    let depth = s.doc.undo_depth();
    for args in [
        json!({"outline": [[0, 0], [1, 0]]}),
        json!({"outline": [[0, 0], [1, 0], [2, 0]]}),
        json!({"outline": square(0.0, 0.0, 1.0), "height": -1}),
        json!({"outline": square(0.0, 0.0, 1.0), "level": ground, "top_level": ground}),
        json!({"outline": square(0.0, 0.0, 1.0), "curves": [{"kind": "circle", "center": [0, 0], "radius": 1}]}),
    ] {
        assert_eq!(fails(&mut s, "shaft_create", args.clone()), ErrorCode::InvalidArguments, "{args}");
    }
    assert_eq!(s.doc.undo_depth(), depth);
    // Drawn with curves: a round shaft.
    let round = run(&mut s, "shaft_create", json!({"curves": [{"kind": "circle", "center": [20, 20], "radius": 0.5}]}));
    assert_eq!(round["category"], "shaft");
}
