//! The component modeling commands: parameters, reference planes and lines,
//! and dimensions that parameters drive.

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

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

/// A bench: one box piece, 1.2 m wide, centered.
fn bench(s: &mut Session) -> Value {
    let def = json!({
        "params": [{"name": "Width", "value": 1.2}],
        "items": [{"name": "Seat", "shape": {"Box": {
            "x": "-Width/2", "y": "-0.2", "z": "0.4", "width": "Width", "depth": "0.4", "height": "0.05"
        }}}]
    });
    run(s, "component_create", json!({"name": "Bench", "def": def}))
}

fn plane(c: &Value, name: &str) -> f64 {
    let planes = c["planes"].as_array().unwrap();
    f(&planes.iter().find(|p| p["name"] == name).unwrap_or_else(|| panic!("no plane {name}"))["value"])
}

#[test]
fn reference_planes_driven_by_labeled_dimensions() {
    let mut s = Session::default();
    let ty = bench(&mut s)["id"].clone();
    // The origin planes are always there.
    let c = run(&mut s, "component_get", json!({"type": ty}));
    assert_eq!(c["component"], "Bench");
    for name in ["CenterX", "CenterY", "Base"] {
        assert_eq!(plane(&c, name), 0.0);
    }
    // Left and right planes, the right one reading the left.
    let left = run(&mut s, "component_ref_plane_add", json!({"type": ty, "axis": "x", "at": "-0.6", "name": "Left"}));
    assert_eq!(left["name"], "Left");
    let right = run(&mut s, "component_ref_plane_add", json!({"type": ty, "axis": "x", "at": "Left + 1"}));
    assert_eq!(right["name"], "Plane1");
    run(&mut s, "component_ref_plane_set", json!({"type": ty, "name": "Plane1", "rename": "Right"}));
    let c = run(&mut s, "component_get", json!({"type": ty}));
    assert_eq!(plane(&c, "Right"), 0.4);

    // A dimension labeled with Width drives the right plane.
    let depth = s.doc.undo_depth();
    let dim = run(
        &mut s,
        "component_dimension_add",
        json!({"type": ty, "view": "plan", "from": "Left", "to": "Right", "label": "Width", "place": -0.5}),
    );
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    let c = run(&mut s, "component_get", json!({"type": ty}));
    assert!((plane(&c, "Right") - 0.6).abs() < 1e-12, "{c}");
    assert!((f(&c["dimensions"][0]["value"]) - 1.2).abs() < 1e-12);
    assert!(c["problems"].as_array().unwrap().is_empty(), "{}", c["problems"]);
    // A new value for the dimension changes the parameter, and the plane follows.
    run(&mut s, "component_dimension_set", json!({"type": ty, "index": dim["index"], "value": 1.5}));
    let c = run(&mut s, "component_get", json!({"type": ty}));
    assert!((plane(&c, "Right") - 0.9).abs() < 1e-12);
    assert_eq!(c["types"][0]["values"]["Width"], 1.5);

    // A label that does not exist yet is created with the current distance.
    run(&mut s, "component_ref_plane_add", json!({"type": ty, "axis": "z", "at": "0.45", "name": "Top"}));
    run(&mut s, "component_dimension_add", json!({"type": ty, "view": "front", "from": "Top", "to": "Base"}));
    run(&mut s, "component_dimension_set", json!({"type": ty, "index": 1, "label": "Height"}));
    let c = run(&mut s, "component_get", json!({"type": ty}));
    // The origin plane stays: the label moves Top.
    assert_eq!(c["types"][0]["values"]["Height"], 0.45);
    let top = c["def"]["ref_planes"].as_array().unwrap().iter().find(|p| p["name"] == "Top").unwrap().clone();
    assert_eq!(top["at"], "Base + Height");
    // Taking the label off keeps the plane where it is.
    run(&mut s, "component_param_set", json!({"type": ty, "name": "Height", "value": 0.5}));
    run(&mut s, "component_dimension_set", json!({"type": ty, "index": 1, "label": ""}));
    let c = run(&mut s, "component_get", json!({"type": ty}));
    assert!((plane(&c, "Top") - 0.5).abs() < 1e-12);
    assert!(c["dimensions"][1]["dimension"]["label"].is_null());
    // An unlabeled dimension moves its plane directly.
    run(&mut s, "component_dimension_set", json!({"type": ty, "index": 1, "value": 0.42}));
    assert!((plane(&run(&mut s, "component_get", json!({"type": ty})), "Top") - 0.42).abs() < 1e-12);
}

#[test]
fn pieces_follow_planes_and_renames_reach_every_expression() {
    let mut s = Session::default();
    let ty = bench(&mut s)["id"].clone();
    run(&mut s, "component_ref_plane_add", json!({"type": ty, "axis": "x", "at": "-Width/2", "name": "Left"}));
    // A piece reads the plane (pieces are edited through the definition until the sketch tools).
    let mut def = run(&mut s, "component_get", json!({"type": ty}))["def"].clone();
    def["items"][0]["shape"]["Box"]["x"] = json!("Left");
    run(&mut s, "type_set", json!({"type": ty, "params": {"def": def}}));
    run(&mut s, "component_ref_plane_set", json!({"type": ty, "name": "Left", "rename": "LeftFace"}));
    run(&mut s, "component_param_set", json!({"type": ty, "name": "Width", "rename": "W"}));
    let c = run(&mut s, "component_get", json!({"type": ty}));
    assert_eq!(c["def"]["items"][0]["shape"]["Box"]["x"], "LeftFace");
    assert_eq!(c["def"]["ref_planes"][0]["at"], "-W/2");
    // A plane or parameter in use cannot go.
    assert_eq!(
        fails(&mut s, "component_ref_plane_delete", json!({"type": ty, "name": "LeftFace"})),
        ErrorCode::Rejected
    );
    assert_eq!(fails(&mut s, "component_param_delete", json!({"type": ty, "name": "W"})), ErrorCode::Rejected);
    // Origin planes cannot be moved or removed.
    assert_eq!(
        fails(&mut s, "component_ref_plane_set", json!({"type": ty, "name": "Base", "at": "1"})),
        ErrorCode::Rejected
    );
    assert_eq!(
        fails(&mut s, "component_ref_plane_delete", json!({"type": ty, "name": "CenterX"})),
        ErrorCode::Rejected
    );
    // Names must be valid and free; expressions must evaluate; no cycles.
    let depth = s.doc.undo_depth();
    for (cmd, args) in [
        ("component_ref_plane_add", json!({"type": ty, "axis": "x", "at": "0", "name": "W"})),
        ("component_ref_plane_add", json!({"type": ty, "axis": "x", "at": "0", "name": "Base"})),
        ("component_ref_plane_add", json!({"type": ty, "axis": "x", "at": "0", "name": "2nd"})),
        ("component_ref_plane_add", json!({"type": ty, "axis": "w", "at": "0"})),
        ("component_ref_plane_add", json!({"type": ty, "axis": "x", "at": "Missing + 1"})),
        ("component_ref_plane_set", json!({"type": ty, "name": "LeftFace", "at": "LeftFace + 1"})),
        ("component_param_add", json!({"type": ty, "name": "LeftFace"})),
        ("component_dimension_add", json!({"type": ty, "view": "side", "from": "LeftFace", "to": "CenterX"})),
        ("component_dimension_add", json!({"type": ty, "view": "plan", "from": "LeftFace", "to": "Base"})),
        ("component_ref_line_add", json!({"type": ty, "plane": "front", "start": ["0", "0"], "end": ["Nope", "1"]})),
    ] {
        assert_eq!(fails(&mut s, cmd, args.clone()), ErrorCode::InvalidArguments, "{cmd} {args}");
    }
    assert_eq!(s.doc.undo_depth(), depth, "refused commands change nothing");
    // A reference line on the front plane, then removed.
    let line = run(
        &mut s,
        "component_ref_line_add",
        json!({"type": ty, "plane": "front", "start": ["LeftFace", "0"], "end": ["0", "0.4"]}),
    );
    assert_eq!(line["name"], "Line1");
    run(&mut s, "component_ref_line_delete", json!({"type": ty, "name": "Line1"}));
    assert!(run(&mut s, "component_get", json!({"type": ty}))["def"]["ref_lines"].is_null());
}

#[test]
fn every_type_of_the_component_follows_but_keeps_its_values() {
    let mut s = Session::default();
    let ty = bench(&mut s)["id"].clone();
    let wide = run(&mut s, "type_duplicate", json!({"type": ty, "name": "Bench wide"}));
    run(&mut s, "component_param_set", json!({"type": wide["id"], "name": "Width", "value": 2.0}));
    run(&mut s, "component_param_add", json!({"type": ty, "name": "Depth", "value": 0.4, "instance": true}));
    run(&mut s, "component_param_set", json!({"type": ty, "name": "Width", "rename": "W"}));
    let c = run(&mut s, "component_get", json!({"type": ty}));
    let types = c["types"].as_array().unwrap();
    assert_eq!(types.len(), 2);
    assert_eq!(types[0]["values"], json!({"W": 1.2, "Depth": 0.4}));
    assert_eq!(types[1]["values"], json!({"W": 2.0, "Depth": 0.4}));
    // One undo takes the rename back from both.
    run(&mut s, "undo", json!({}));
    let c = run(&mut s, "component_get", json!({"type": wide["id"]}));
    assert_eq!(c["types"][0]["values"]["Width"], 2.0);
    // Not a component type.
    let wall = run(&mut s, "types_list", json!({"category": "wall"}))[0]["id"].clone();
    assert_eq!(fails(&mut s, "component_get", json!({"type": wall})), ErrorCode::InvalidArguments);
}

/// The example bench, modeled with commands only: sketched pieces locked to
/// planes follow the parameters, and the void cuts the seat.
#[test]
fn the_example_bench_is_built_and_follows_its_parameters() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/component_bench.ccmacro");
    let steps = cc_engine::script::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut s = Session::default();
    cc_engine::script::run(&mut s, &steps).unwrap_or_else(|(_, e)| panic!("step {}: {}", e.index + 1, e.error));
    let c = run(&mut s, "component_get", json!({"type": "Bench"}));
    assert!(c["problems"].as_array().unwrap().is_empty(), "{}", c["problems"]);
    let items = c["def"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    assert_eq!(items[0]["shape"]["Profile"]["offset"], "Under");
    assert_eq!(items[0]["shape"]["Profile"]["depth"], "Top - Under");
    let width = |s: &mut Session| {
        let ty = s.project().types.values().find(|t| t.name == "Bench").unwrap().clone();
        let cc_core::TypeData::Component { def, .. } = &ty.data else { panic!() };
        let solids = cc_geom::resolve_component(def, &Default::default());
        let xs = solids
            .iter()
            .flat_map(|p| p.footprint.iter().map(|q| q.x))
            .chain(solids.iter().flat_map(|p| p.faces.iter().flatten().map(|q| q.x)));
        let (lo, hi) = xs.fold((f64::MAX, f64::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
        (hi - lo, solids.len())
    };
    let (w, solids) = width(&mut s);
    assert!((w - 1.2).abs() < 1e-9, "{w}");
    // Seat (cut by the hole) and two legs.
    assert!(solids >= 3, "{solids}");
    run(&mut s, "component_param_set", json!({"type": "Bench", "name": "Width", "value": 1.6}));
    assert!((width(&mut s).0 - 1.6).abs() < 1e-9);
    let c = run(&mut s, "component_get", json!({"type": "Bench"}));
    assert!((plane(&c, "Right") - 0.8).abs() < 1e-12);
}

#[test]
fn sketched_pieces_extrude_revolve_and_change() {
    let mut s = Session::default();
    let ty = bench(&mut s)["id"].clone();
    run(&mut s, "component_ref_plane_add", json!({"type": ty, "axis": "z", "at": "0.4", "name": "Seat"}));
    run(&mut s, "component_ref_plane_add", json!({"type": ty, "axis": "x", "at": "0.5", "name": "Post"}));
    // A turned foot: a rectangle on the front plane revolved about the Post plane.
    let foot = run(
        &mut s,
        "component_piece_add",
        json!({"type": ty, "kind": "revolution", "plane": "front", "axis": "Post",
               "points": [{"u": "Post", "v": 0}, {"u": "Post + 0.03", "v": 0}, {"u": "Post + 0.03", "v": "Seat"}, {"u": "Post", "v": "Seat"}]}),
    );
    assert_eq!(foot["index"], 1);
    // An extrusion's end can be moved alone; its start stays.
    let slab = run(
        &mut s,
        "component_piece_add",
        json!({"type": ty, "plane": "plan", "start": "Base", "end": "Seat",
               "points": [{"u": -0.1, "v": -0.1}, {"u": 0.1, "v": -0.1}, {"u": 0.1, "v": 0.1}, {"u": -0.1, "v": 0.1}]}),
    );
    run(
        &mut s,
        "component_piece_set",
        json!({"type": ty, "index": slab["index"], "end": "Seat - 0.05", "void": true, "name": "Pocket"}),
    );
    let c = run(&mut s, "component_get", json!({"type": ty}));
    let item = &c["def"]["items"][2];
    assert_eq!(item["name"], "Pocket");
    assert_eq!(item["void"], true);
    assert_eq!(item["shape"]["Profile"]["offset"], "Base");
    assert_eq!(item["shape"]["Profile"]["depth"], "(Seat - 0.05) - Base");
    run(&mut s, "component_piece_set", json!({"type": ty, "index": 2, "start": 0.1}));
    let c = run(&mut s, "component_get", json!({"type": ty}));
    assert_eq!(c["def"]["items"][2]["shape"]["Profile"]["depth"], "(Seat - 0.05) - 0.1");
    assert!(c["problems"].as_array().unwrap().is_empty(), "{}", c["problems"]);
    // Moving the post plane moves the turned foot.
    run(&mut s, "component_ref_plane_set", json!({"type": ty, "name": "Post", "at": "0.55"}));
    // Refused: bad kinds, too few points, unknown names, a revolution without an axis,
    // axis on an extrusion, a box given a sketch, deleting the last piece.
    let depth = s.doc.undo_depth();
    for (cmd, args) in [
        (
            "component_piece_add",
            json!({"type": ty, "kind": "loft", "plane": "plan", "points": [{"u": 0, "v": 0}, {"u": 1, "v": 0}]}),
        ),
        ("component_piece_add", json!({"type": ty, "plane": "plan", "points": [{"u": 0, "v": 0}]})),
        (
            "component_piece_add",
            json!({"type": ty, "plane": "plan", "points": [{"u": "Nope", "v": 0}, {"u": 1, "v": 0}, {"u": 1, "v": 1}]}),
        ),
        (
            "component_piece_add",
            json!({"type": ty, "kind": "revolution", "plane": "front", "points": [{"u": 0.1, "v": 0}, {"u": 0.2, "v": 0}, {"u": 0.2, "v": 1}]}),
        ),
        ("component_piece_set", json!({"type": ty, "index": 2, "axis": 0})),
        ("component_piece_set", json!({"type": ty, "index": 0, "end": 1})),
        ("component_piece_set", json!({"type": ty, "index": 9, "void": true})),
    ] {
        let code = fails(&mut s, cmd, args.clone());
        assert!(matches!(code, ErrorCode::InvalidArguments | ErrorCode::NotFound), "{cmd} {args}: {code:?}");
    }
    run(&mut s, "component_piece_delete", json!({"type": ty, "index": 2}));
    run(&mut s, "component_piece_delete", json!({"type": ty, "index": 1}));
    assert_eq!(fails(&mut s, "component_piece_delete", json!({"type": ty, "index": 0})), ErrorCode::InvalidArguments);
    assert_eq!(s.doc.undo_depth(), depth + 2);
}

#[test]
fn sweeps_follow_paths_and_blends_join_outlines() {
    let mut s = Session::default();
    let ty = bench(&mut s)["id"].clone();
    run(&mut s, "component_ref_plane_add", json!({"type": ty, "axis": "x", "at": "0.5", "name": "Edge"}));
    let square = |h: f64| json!([{"u": -h, "v": -h}, {"u": h, "v": -h}, {"u": h, "v": h}, {"u": -h, "v": h}]);
    // A rail: a 4 cm square section along an L path that ends on the Edge plane.
    let rail = run(
        &mut s,
        "component_piece_add",
        json!({"type": ty, "kind": "sweep", "plane": "plan", "start": 0.6, "points": square(0.02),
               "path": [{"u": -0.5, "v": 0}, {"u": 0, "v": 0, "radius": 0.4}, {"u": "Edge", "v": 0.3}]}),
    );
    // A tapered foot: a square blended into a circle.
    let foot = run(
        &mut s,
        "component_piece_add",
        json!({"type": ty, "kind": "blend", "plane": "plan", "start": 0, "end": 0.3, "points": square(0.05),
               "top": [{"u": -0.02, "v": 0, "radius": 0.02}, {"u": 0.02, "v": 0, "radius": 0.02}]}),
    );
    let c = run(&mut s, "component_get", json!({"type": ty}));
    assert!(c["problems"].as_array().unwrap().is_empty(), "{}", c["problems"]);
    assert!(c["def"]["items"][rail["index"].as_u64().unwrap() as usize]["shape"]["Sweep"].is_object());
    let blend = &c["def"]["items"][foot["index"].as_u64().unwrap() as usize]["shape"]["Blend"];
    assert_eq!(blend["depth"], "0.3");
    // The pieces resolve to closed solids with volume.
    let ty_data = s.project().types.values().find(|t| t.name == "Bench").unwrap().data.clone();
    let cc_core::TypeData::Component { def, .. } = ty_data else { panic!() };
    let (parts, errors) = def.resolve(&Default::default());
    assert!(errors.is_empty(), "{errors:?}");
    for p in parts.iter().filter(|p| !p.faces.is_empty()) {
        assert!(cc_core::sweep::volume(&p.faces) > 0.0);
    }
    // Moving the Edge plane moves the rail's end; the blend's top and end change.
    run(&mut s, "component_ref_plane_set", json!({"type": ty, "name": "Edge", "at": "0.6"}));
    run(&mut s, "component_piece_set", json!({"type": ty, "index": foot["index"], "end": 0.4, "top": square(0.03)}));
    run(
        &mut s,
        "component_piece_set",
        json!({"type": ty, "index": rail["index"], "path": [{"u": 0, "v": 0}, {"u": 0, "v": 0.4}]}),
    );
    let c = run(&mut s, "component_get", json!({"type": ty}));
    assert!(c["problems"].as_array().unwrap().is_empty(), "{}", c["problems"]);
    // Paths only on sweeps, tops only on blends; a sweep needs a path, a blend a top.
    for (cmd, args) in [
        ("component_piece_set", json!({"type": ty, "index": 0, "path": [{"u": 0, "v": 0}, {"u": 1, "v": 0}]})),
        ("component_piece_set", json!({"type": ty, "index": rail["index"], "top": square(0.1)})),
        ("component_piece_set", json!({"type": ty, "index": rail["index"], "end": 1})),
        ("component_piece_add", json!({"type": ty, "kind": "sweep", "plane": "plan", "points": square(0.02)})),
        ("component_piece_add", json!({"type": ty, "kind": "blend", "plane": "plan", "points": square(0.02)})),
        (
            "component_piece_add",
            json!({"type": ty, "kind": "sweep", "plane": "plan", "points": square(0.02),
                                        "path": [{"u": 0, "v": 0}, {"u": 0, "v": 0}]}),
        ),
    ] {
        assert_eq!(fails(&mut s, cmd, args.clone()), ErrorCode::InvalidArguments, "{cmd} {args}");
    }
}

#[test]
fn equalities_keep_a_plane_halfway_as_the_others_move() {
    let mut s = Session::default();
    let ty = bench(&mut s)["id"].clone();
    for (name, at) in [("Left", "-0.6"), ("Right", "0.4"), ("Mid", "0")] {
        run(&mut s, "component_ref_plane_add", json!({"type": ty, "axis": "x", "at": at, "name": name}));
    }
    let depth = s.doc.undo_depth();
    let eq = run(
        &mut s,
        "component_equal_add",
        json!({"type": ty, "view": "plan", "a": "Left", "middle": "Mid", "b": "Right", "place": 0.5}),
    );
    assert_eq!(eq["index"], 0);
    assert_eq!(s.doc.undo_depth(), depth + 1);
    let c = run(&mut s, "component_get", json!({"type": ty}));
    assert!((plane(&c, "Mid") + 0.1).abs() < 1e-12);
    // A labeled dimension drives Right; Mid follows.
    run(
        &mut s,
        "component_dimension_add",
        json!({"type": ty, "view": "plan", "from": "Left", "to": "Right", "label": "Width"}),
    );
    let c = run(&mut s, "component_get", json!({"type": ty}));
    assert!((plane(&c, "Right") - 0.6).abs() < 1e-12 && plane(&c, "Mid").abs() < 1e-12, "{c}");
    assert!(c["problems"].as_array().unwrap().is_empty(), "{}", c["problems"]);
    // Moving the middle by hand would break it: refused. Origin planes cannot be the middle.
    assert_eq!(
        fails(&mut s, "component_ref_plane_set", json!({"type": ty, "name": "Mid", "at": "0.2"})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(
            &mut s,
            "component_equal_add",
            json!({"type": ty, "view": "plan", "a": "Left", "middle": "CenterX", "b": "Right"})
        ),
        ErrorCode::Rejected
    );
    assert_eq!(
        fails(
            &mut s,
            "component_equal_add",
            json!({"type": ty, "view": "front", "a": "Left", "middle": "Mid", "b": "Mid"})
        ),
        ErrorCode::InvalidArguments
    );
    // Deleting the equality keeps Mid halfway; deleting one of its planes takes the equality too.
    run(&mut s, "component_equal_delete", json!({"type": ty, "index": 0}));
    run(&mut s, "component_ref_plane_set", json!({"type": ty, "name": "Mid", "at": "0.2"}));
    run(&mut s, "component_equal_add", json!({"type": ty, "view": "plan", "a": "Left", "middle": "Mid", "b": "Right"}));
    run(&mut s, "component_ref_plane_delete", json!({"type": ty, "name": "Mid"}));
    assert!(run(&mut s, "component_get", json!({"type": ty}))["def"]["equals"].is_null());
}

#[test]
fn pieces_take_parts_of_ellipses() {
    let mut s = Session::default();
    let ty = bench(&mut s)["id"].clone();
    // An elliptical plate 2 Width by 0.6, drawn as two halves of the ellipse.
    let half = |u: &str| json!({"u": u, "v": 0, "ellipse": {"u": 0, "v": 0, "rx": "Width", "ry": 0.3}});
    let plate = run(
        &mut s,
        "component_piece_add",
        json!({"type": ty, "plane": "plan", "start": "0", "end": "0.02", "points": [half("Width"), half("-Width")]}),
    );
    let c = run(&mut s, "component_get", json!({"type": ty}));
    let points = &c["def"]["items"][plate["index"].as_u64().unwrap() as usize]["shape"]["Profile"]["points"];
    assert_eq!(points[0]["ellipse"]["rx"], "Width", "{c}");
    // Renaming the parameter reaches the ellipse.
    run(&mut s, "component_param_set", json!({"type": ty, "name": "Width", "rename": "W"}));
    let c = run(&mut s, "component_get", json!({"type": ty}));
    let points = &c["def"]["items"][plate["index"].as_u64().unwrap() as usize]["shape"]["Profile"]["points"];
    assert_eq!((points[0]["u"].clone(), points[1]["ellipse"]["rx"].clone()), (json!("W"), json!("W")));
    // A bad expression in an ellipse is refused.
    let bad = json!({"u": 1, "v": 0, "ellipse": {"u": 0, "v": 0, "rx": "Nope", "ry": 0.3}});
    assert_eq!(
        fails(&mut s, "component_piece_add", json!({"type": ty, "plane": "plan", "points": [bad, half("-1")]})),
        ErrorCode::InvalidArguments
    );
}
