//! The engine without a GUI: the catalog is well formed, and a house can be
//! built, changed, queried and exported through commands alone.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::{CommandKind, ErrorCode, Session, catalog};
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

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ccad-engine-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn catalog_is_well_formed() {
    let commands = catalog();
    let mut names: Vec<&str> = commands.iter().map(|c| c.name).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), commands.len(), "duplicate command names");
    for c in &commands {
        // Usable as tool names by MCP clients and model APIs.
        assert!(c.name.len() <= 64 && c.name.chars().all(|ch| ch.is_ascii_lowercase() || ch == '_'), "{}", c.name);
        assert!(!c.description.is_empty(), "{} has no description", c.name);
        assert_eq!(c.input_schema["type"], "object", "{}", c.name);
        assert!(!c.input_schema.to_string().contains("$ref"), "{} schema has references", c.name);
    }
}

/// Every command answers empty or wrong arguments with an error value, never a panic.
#[test]
fn every_command_survives_bad_arguments() {
    for c in catalog() {
        for args in [json!({}), json!({"bogus": 1}), json!([1, 2]), json!({"id": 999_999, "ids": [999_999]})] {
            let mut s = Session::default();
            if let Err(e) = s.execute(c.name, args.clone()) {
                assert_ne!(e.code, ErrorCode::Internal, "{} with {args}: {}", c.name, e.message);
                assert_ne!(e.code, ErrorCode::UnknownCommand);
            }
        }
    }
    assert_eq!(Session::default().execute("no_such_command", json!({})).unwrap_err().code, ErrorCode::UnknownCommand);
}

#[test]
fn a_house_through_commands() {
    let mut s = Session::default();
    let levels = run(&mut s, "levels_list", json!({}));
    let ground = levels[0]["name"].as_str().unwrap().to_string();

    // Four walls, by level name; the upper one attached to the level above.
    let corners = [[0.0, 0.0], [8.0, 0.0], [8.0, 6.0], [0.0, 6.0]];
    let walls: Vec<u64> = (0..4)
        .map(|i| {
            let w = run(
                &mut s,
                "wall_create",
                json!({"start": corners[i], "end": corners[(i + 1) % 4], "level": ground, "height": 3.0}),
            );
            id(&w)
        })
        .collect();
    let wall = run(&mut s, "element_get", json!({"id": walls[0]}));
    assert_eq!(wall["derived"]["length"], 8.0);

    // A door centered on the first wall, a window by a point near the second.
    let door = run(&mut s, "door_create", json!({"wall": walls[0]}));
    assert!((door["params"]["offset"].as_f64().unwrap() - 4.0).abs() < 1e-9);
    assert_eq!(door["mark"], "1");
    let window = run(&mut s, "window_create", json!({"wall": walls[1], "point": [8.5, 2.0]}));
    assert_eq!(window["params"]["flip_facing"], true, "the point is outside, to the right of the wall");

    // Floor and roof from the walls, a room inside, a grid, a column, a beam, a stair, a railing.
    let floor = run(&mut s, "floor_create", json!({"from_walls": true}));
    assert!(floor["derived"]["volume"].as_f64().unwrap() > 0.0);
    run(&mut s, "roof_create", json!({"from_walls": true}));
    let room = run(&mut s, "room_create", json!({"point": [4.0, 3.0], "name": "Sala"}));
    let area = room["derived"]["area"].as_f64().unwrap();
    assert!((area - 7.85 * 5.85).abs() < 1e-6, "{area}");
    assert_eq!(fails(&mut s, "room_create", json!({"point": [40.0, 3.0]})), ErrorCode::Rejected);
    let grid = run(&mut s, "grid_create", json!({"start": [0.0, -2.0], "end": [0.0, 8.0]}));
    assert_eq!(grid["params"]["name"], "1");
    let column = run(&mut s, "column_create", json!({"position": [4.0, 3.0], "rotation": 30.0}));
    assert!(column["params"]["top"]["Level"].is_object(), "attached to the level above");
    run(&mut s, "beam_create", json!({"start": [0.0, 3.0], "end": [8.0, 3.0]}));
    run(&mut s, "stair_create", json!({"start": [1.0, 1.0], "direction": 90.0}));
    run(&mut s, "railing_create", json!({"path": [[0.0, 7.0], [4.0, 7.0], [4.0, 9.0]]}));
    let types = run(&mut s, "types_list", json!({"category": "component"}));
    let table = types[0]["name"].as_str().unwrap().to_string();
    run(&mut s, "component_place", json!({"position": [2.0, 2.0], "type": table, "rotation": 90.0}));

    // Change things: a parameter, the type, a mark; invalid edits are refused untouched.
    let changed = run(&mut s, "element_set", json!({"id": walls[0], "params": {"height": 2.7}, "mark": "P1"}));
    assert_eq!((changed["params"]["height"].as_f64(), changed["mark"].as_str()), (Some(2.7), Some("P1")));
    let before = run(&mut s, "element_get", json!({"id": door["id"]}));
    assert_eq!(
        fails(&mut s, "element_set", json!({"id": door["id"], "params": {"host": 999_999}})),
        ErrorCode::Rejected
    );
    assert_eq!(
        fails(&mut s, "element_set", json!({"id": walls[0], "params": {"height": "high"}})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(run(&mut s, "element_get", json!({"id": door["id"]})), before);
    let roof_id = run(&mut s, "elements_list", json!({"category": "roof"}))[0]["id"].clone();
    let gable = run(
        &mut s,
        "element_set",
        json!({"id": roof_id, "params": {"shape": {"Gable": {"slope": 0.5, "ridge_deg": 0.0}}}}),
    );
    assert!(gable["params"]["shape"]["Gable"].is_object());

    // Move, copy as an array, rotate, mirror, pin, delete, undo and redo.
    let moved = run(&mut s, "elements_move", json!({"ids": [grid["id"]], "delta": [-1.0, 0.0]}));
    assert_eq!(moved["changed"].as_array().unwrap().len(), 1);
    let copies = run(&mut s, "elements_copy", json!({"ids": [column["id"]], "delta": [2.0, 0.0], "count": 3}));
    assert_eq!(copies["created"].as_array().unwrap().len(), 3);
    run(&mut s, "elements_rotate", json!({"ids": copies["created"], "angle": 90.0}));
    let mirrored = run(&mut s, "elements_mirror", json!({"ids": [column["id"]], "axis_element": walls[1]}));
    let mirror_id = mirrored["created"][0].as_u64().unwrap();
    let x = run(&mut s, "element_get", json!({"id": mirror_id}))["params"]["position"][0].as_f64().unwrap();
    assert!((x - 12.0).abs() < 1e-9, "mirrored across x = 8: {x}");
    run(&mut s, "elements_pin", json!({"ids": [mirror_id], "pinned": true}));
    let deleted = run(&mut s, "elements_delete", json!({"ids": [mirror_id]}));
    assert_eq!(deleted["pinned_skipped"], json!([mirror_id]));
    let count = |s: &mut Session| run(s, "project_info", json!({}))["elements"].as_u64().unwrap();
    let n = count(&mut s);
    run(&mut s, "elements_pin", json!({"ids": [mirror_id], "pinned": false}));
    run(&mut s, "elements_delete", json!({"ids": [mirror_id]}));
    assert_eq!(count(&mut s), n - 1);
    run(&mut s, "undo", json!({}));
    assert_eq!(count(&mut s), n);
    run(&mut s, "redo", json!({}));
    assert_eq!(count(&mut s), n - 1);

    // Wall edits: split, offset copy, trim to a corner, align, move an end, joins.
    let split = run(&mut s, "element_split", json!({"id": walls[2], "point": [4.0, 6.0]}));
    let offset = run(&mut s, "element_offset", json!({"id": split["created"], "distance": -1.0}));
    run(&mut s, "wall_join", json!({"id": walls[3], "end": "start", "joined": false}));
    run(&mut s, "element_move_end", json!({"id": offset["id"], "end": "end", "point": [0.5, 5.0]}));
    assert_eq!(
        fails(&mut s, "element_move_end", json!({"id": offset["id"], "end": "middle", "point": [0.0, 0.0]})),
        ErrorCode::InvalidArguments
    );

    // Types, materials, levels and views.
    let wall_type = run(&mut s, "type_get", json!({"type": wall["type"]["id"]}));
    let copy = run(
        &mut s,
        "type_duplicate",
        json!({"type": wall_type["id"], "name": "Parede grossa", "elements": [walls[0]]}),
    );
    let thick = run(
        &mut s,
        "type_set",
        json!({"type": "Parede grossa", "params": {"layers": [{"material": wall_type["params"]["layers"][0]["material"], "thickness": 0.25, "function": "Structure"}]}}),
    );
    assert_eq!(thick["params"]["layers"].as_array().unwrap().len(), 1);
    assert!(
        (run(&mut s, "element_get", json!({"id": walls[0]}))["derived"]["thickness"].as_f64().unwrap() - 0.25).abs()
            < 1e-9
    );
    assert_eq!(fails(&mut s, "type_delete", json!({"type": copy["id"]})), ErrorCode::Rejected);
    let spare = run(&mut s, "type_duplicate", json!({"type": wall_type["id"]}));
    run(&mut s, "type_delete", json!({"type": spare["id"]}));
    let mat = run(&mut s, "material_create", json!({"name": "Granito", "color": [90, 90, 90]}));
    assert_eq!(mat["name"], "Granito");
    run(&mut s, "material_set", json!({"material": "Granito", "values": {"description": "polido"}}));
    run(&mut s, "material_rename", json!({"material": "Granito", "name": "Granito cinza"}));
    run(&mut s, "material_delete", json!({"material": "Granito cinza"}));
    let level = run(&mut s, "level_create", json!({"name": "Cobertura", "elevation": 6.0}));
    run(&mut s, "level_set", json!({"level": level["id"], "elevation": 6.5}));
    assert_eq!(run(&mut s, "views_list", json!({"category": "floor_plan"})).as_array().unwrap().len(), 3);
    run(&mut s, "level_delete", json!({"level": "Cobertura"}));
    let section = run(&mut s, "view_create", json!({"kind": "section", "start": [-1.0, 3.0], "end": [9.0, 3.0]}));
    let schedule = run(&mut s, "view_create", json!({"kind": "schedule", "category": "wall"}));
    let rows = run(&mut s, "schedule_table", json!({"view": schedule["id"]}));
    assert!(rows["rows"].as_array().unwrap().len() >= 5);
    let sheet = run(&mut s, "view_create", json!({"kind": "sheet"}));
    run(&mut s, "sheet_place_view", json!({"sheet": sheet["id"], "view": section["id"]}));
    assert_eq!(
        fails(&mut s, "sheet_place_view", json!({"sheet": sheet["id"], "view": section["id"]})),
        ErrorCode::Rejected
    );
    run(&mut s, "view_set", json!({"view": section["id"], "scale": 100, "params": {"depth": 4.0}}));
    run(&mut s, "units_set", json!({"values": {"length": {"decimals": 3}}}));

    // Files: save, reopen, IFC round trip, PDF and SVG of the sheet, CSV.
    let path = scratch("house.ccad");
    run(&mut s, "project_save", json!({"path": path}));
    let elements = count(&mut s);
    let mut reopened = Session::default();
    run(&mut reopened, "project_open", json!({"path": path}));
    assert_eq!(count(&mut reopened), elements);
    let ifc = scratch("house.ifc");
    run(&mut s, "ifc_export", json!({"path": ifc}));
    let report = run(&mut reopened, "ifc_import", json!({"path": ifc}));
    assert!(report["imported"]["wall"].as_u64().unwrap() >= 5);
    for (file, magic) in [("sheet.pdf", "%PDF"), ("sheet.svg", "<svg")] {
        let out = scratch(file);
        run(&mut s, "sheet_export", json!({"sheet": sheet["id"], "path": out}));
        let text = String::from_utf8_lossy(&std::fs::read(&out).unwrap()).into_owned();
        assert!(text.contains(magic), "{file}");
    }
    let csv = scratch("walls.csv");
    run(&mut s, "schedule_export_csv", json!({"view": schedule["id"], "path": csv}));
    assert!(std::fs::read_to_string(&csv).unwrap().lines().count() >= 6);

    // Any view as an image: a plan, the section, a 3D view, the sheet.
    let png_size = |bytes: &[u8]| {
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        let be = |i: usize| u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        (be(16), be(20))
    };
    let views = run(&mut s, "views_list", json!({}));
    let id_of = |kind: &str| views.as_array().unwrap().iter().find(|v| v["category"] == kind).unwrap()["id"].clone();
    for (view, file) in
        [(id_of("floor_plan"), "plan.png"), (section["id"].clone(), "section.png"), (id_of("three_d"), "3d.png")]
    {
        let out = scratch(file);
        let r = run(&mut s, "view_export", json!({"view": view, "path": out, "dpi": 96}));
        let (w, h) = png_size(&std::fs::read(&out).unwrap());
        let paper = (r["paper_mm"][0].as_f64().unwrap(), r["paper_mm"][1].as_f64().unwrap());
        assert_eq!((w as f64, h as f64), ((paper.0 / 25.4 * 96.0).round(), (paper.1 / 25.4 * 96.0).round()), "{file}");
        assert!(w > 50 && h > 50, "{file}: {w} x {h}");
    }
    // A sheet keeps its paper: A-size sheets at 50 dpi.
    let out = scratch("sheet.png");
    let r = run(&mut s, "view_export", json!({"view": sheet["id"], "path": out, "dpi": 50}));
    let (w, _) = png_size(&std::fs::read(&out).unwrap());
    assert_eq!(w as f64, (r["paper_mm"][0].as_f64().unwrap() / 25.4 * 50.0).round());
    // The same view as SVG; doubling the resolution doubles the pixels.
    let svg = scratch("plan.svg");
    run(&mut s, "view_export", json!({"view": id_of("floor_plan"), "path": svg}));
    assert!(std::fs::read_to_string(&svg).unwrap().contains("<svg"));
    let (a, b) = (scratch("lo.png"), scratch("hi.png"));
    run(&mut s, "view_export", json!({"view": id_of("floor_plan"), "path": a, "dpi": 40}));
    run(&mut s, "view_export", json!({"view": id_of("floor_plan"), "path": b, "dpi": 80}));
    let (wa, wb) = (png_size(&std::fs::read(&a).unwrap()).0, png_size(&std::fs::read(&b).unwrap()).0);
    assert!((wb as i64 - 2 * wa as i64).abs() <= 1, "{wa} {wb}");
    // Schedules have no drawing; absurd resolutions and unknown formats are refused.
    let bad = |file: &str, extra: Value| {
        let mut args = json!({"view": id_of("floor_plan"), "path": scratch(file)});
        args.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        args
    };
    assert_eq!(
        fails(&mut s, "view_export", json!({"view": schedule["id"], "path": scratch("x.png")})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(fails(&mut s, "view_export", bad("x.png", json!({"dpi": 1e6}))), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "view_export", bad("x.png", json!({"dpi": 0}))), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "view_export", bad("x.bmp", json!({}))), ErrorCode::InvalidArguments);
}

#[test]
fn names_resolve_and_ambiguity_is_reported() {
    let mut s = Session::default();
    let level = run(&mut s, "levels_list", json!({}))[0]["id"].as_u64().unwrap();
    run(&mut s, "level_create", json!({"name": "Mezanino"}));
    run(&mut s, "level_create", json!({"name": "mezanino"}));
    assert_eq!(
        fails(&mut s, "wall_create", json!({"start": [0, 0], "end": [1, 0], "level": "MEZANINO"})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(&mut s, "wall_create", json!({"start": [0, 0], "end": [1, 0], "level": "Nowhere"})),
        ErrorCode::NotFound
    );
    run(&mut s, "wall_create", json!({"start": [0, 0], "end": [1, 0], "level": level}));
    // A type of another category is refused.
    let door_type = run(&mut s, "types_list", json!({"category": "door"}))[0]["id"].clone();
    assert_eq!(
        fails(&mut s, "wall_create", json!({"start": [0, 0], "end": [1, 0], "type": door_type})),
        ErrorCode::InvalidArguments
    );
}

#[test]
fn queries_change_nothing() {
    let mut s = Session::default();
    let revision = s.doc.revision();
    for c in catalog().into_iter().filter(|c| c.kind == CommandKind::Query) {
        let _ = s.execute(c.name, json!({}));
    }
    assert_eq!(s.doc.revision(), revision);
}

#[test]
fn dimensions_follow_and_drive_what_they_measure() {
    let mut s = Session::default();
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/casa_simples.ccad");
    run(&mut s, "project_open", json!({"path": sample}));
    let plan = run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]["id"].clone();
    // A 20 cm column far from the house; measure from 4 m left of it to its left face.
    let column = run(&mut s, "column_create", json!({"position": [50.0, 50.0]}));
    let face = [49.9, 50.0];
    let dim = run(&mut s, "dimension_create", json!({"view": plan, "a": [45.9, 50.0], "b": face, "kind": "linear"}));
    let refs = &dim["params"]["refs"];
    assert!(refs[0].is_null() && refs[1]["target"]["Element"] == column["id"], "{refs}");
    // The dimension follows the column...
    run(&mut s, "elements_move", json!({"ids": [column["id"]], "delta": [1.5, 0.0]}));
    let value = |s: &mut Session| {
        let d = run(s, "element_get", json!({"id": dim["id"]}));
        (d["params"]["b"][0].as_f64().unwrap() - d["params"]["a"][0].as_f64().unwrap()).abs()
    };
    assert!((value(&mut s) - 5.5).abs() < 1e-9);
    // ...and drives it: 4 m puts it back.
    run(&mut s, "dimension_set_value", json!({"id": dim["id"], "value": 4.0}));
    let x = run(&mut s, "element_get", json!({"id": column["id"]}))["params"]["position"][0].as_f64().unwrap();
    assert!((x - 50.0).abs() < 1e-9, "{x}");
    assert_eq!(
        fails(&mut s, "dimension_set_value", json!({"id": dim["id"], "value": -1.0})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(&mut s, "dimension_set_value", json!({"id": column["id"], "value": 1.0})),
        ErrorCode::InvalidArguments
    );
    // A free dimension just takes the value; zero-length ones are refused.
    let free = run(&mut s, "dimension_create", json!({"view": plan, "a": [100.0, 100.0], "b": [103.0, 100.0]}));
    run(&mut s, "dimension_set_value", json!({"id": free["id"], "value": 5.0}));
    assert!(
        (run(&mut s, "element_get", json!({"id": free["id"]}))["params"]["b"][0].as_f64().unwrap() - 105.0).abs()
            < 1e-9
    );
    assert_eq!(
        fails(&mut s, "dimension_create", json!({"view": plan, "a": [1.0, 1.0], "b": [1.0, 1.0]})),
        ErrorCode::InvalidArguments
    );
    let three_d = run(&mut s, "view_create", json!({"kind": "three_d"}));
    assert_eq!(
        fails(&mut s, "dimension_create", json!({"view": three_d["id"], "a": [0, 0], "b": [1, 0]})),
        ErrorCode::InvalidArguments
    );
}

#[test]
fn a_dimension_chain_shares_one_line_and_follows_its_elements() {
    let mut s = Session::default();
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/casa_simples.ccad");
    run(&mut s, "project_open", json!({"path": sample}));
    let plan = run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]["id"].clone();
    // Three 20 cm columns far from the house; measure between their faces.
    let cols: Vec<Value> =
        [50.0, 54.0, 59.0].iter().map(|x| run(&mut s, "column_create", json!({"position": [x, 50.0]}))).collect();
    let depth = s.doc.undo_depth();
    let out = run(
        &mut s,
        "dimension_chain_create",
        json!({"view": plan, "points": [[50.1, 50.0], [53.9, 50.0], [58.9, 50.05]], "kind": "linear", "offset": 2.0}),
    );
    assert_eq!(out["count"], 2);
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    let dims: Vec<Value> =
        out["ids"].as_array().unwrap().iter().map(|id| run(&mut s, "element_get", json!({"id": id}))).collect();
    let f = |v: &Value| v.as_f64().unwrap();
    for (d, value) in dims.iter().zip([3.8, 5.0]) {
        let p = &d["params"];
        assert_eq!(p["kind"]["Linear"]["dir"], json!([1.0, 0.0]), "{p}");
        // Both lines at y = 52.
        assert!((f(&p["a"][1]) + f(&p["offset"]) - 52.0).abs() < 1e-9, "{p}");
        assert!(((f(&p["b"][0]) - f(&p["a"][0])).abs() - value).abs() < 1e-9, "{p}");
    }
    // The shared end follows the middle column in both dimensions.
    let middle = &cols[1]["id"];
    assert_eq!(dims[0]["params"]["refs"][1]["target"]["Element"], *middle);
    assert_eq!(dims[1]["params"]["refs"][0]["target"]["Element"], *middle);
    run(&mut s, "elements_move", json!({"ids": [middle], "delta": [0.5, 0.0]}));
    let x = |s: &mut Session, id: &Value, end: &str| f(&run(s, "element_get", json!({"id": id}))["params"][end][0]);
    assert!((x(&mut s, &out["ids"][0], "b") - 54.4).abs() < 1e-9);
    assert!((x(&mut s, &out["ids"][1], "a") - 54.4).abs() < 1e-9);
    // Undo the move and the chain: both dimensions go at once.
    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));
    assert_eq!(fails(&mut s, "element_get", json!({"id": out["ids"][0]})), ErrorCode::NotFound);
    assert_eq!(fails(&mut s, "element_get", json!({"id": out["ids"][1]})), ErrorCode::NotFound);

    // An aligned chain along a slanted line, at an angle: collinear segments stay aligned.
    let out = run(
        &mut s,
        "dimension_chain_create",
        json!({"view": plan, "points": [[100, 100], [103, 104], [106, 108], [110, 108]], "offset": -1.0}),
    );
    let kinds: Vec<Value> = out["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| run(&mut s, "element_get", json!({"id": id}))["params"]["kind"].clone())
        .collect();
    assert_eq!(kinds[0], json!("Aligned"));
    assert_eq!(kinds[1], json!("Aligned"));
    assert!(kinds[2]["Linear"].is_object(), "{}", kinds[2]);
    // `angle` measures along a given direction: 90° is vertical.
    let out = run(
        &mut s,
        "dimension_chain_create",
        json!({"view": plan, "points": [[200, 0], [201, 2], [200, 5]], "kind": "linear", "angle": 90.0}),
    );
    let d = run(&mut s, "element_get", json!({"id": out["ids"][1]}));
    assert_eq!(d["params"]["kind"]["Linear"]["dir"], json!([0.0, 1.0]));

    // Bad chains are refused and change nothing.
    let depth = s.doc.undo_depth();
    for args in [
        json!({"view": plan, "points": [[0, 0]]}),
        json!({"view": plan, "points": [[0, 0], [2, 0], [2, 3]], "kind": "linear", "direction": "horizontal"}),
        json!({"view": plan, "points": [[0, 0], [2, 0]], "kind": "radial"}),
        json!({"view": plan, "points": [[0, 0], [2, 0]], "direction": "sideways"}),
        json!({"view": plan, "points": [[0, 0], [0, 0]]}),
    ] {
        assert_eq!(fails(&mut s, "dimension_chain_create", args), ErrorCode::InvalidArguments);
    }
    let three_d = run(&mut s, "view_create", json!({"kind": "three_d"}));
    let depth = depth + 1;
    assert_eq!(
        fails(&mut s, "dimension_chain_create", json!({"view": three_d["id"], "points": [[0, 0], [1, 0]]})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(s.doc.undo_depth(), depth);
}

#[test]
fn imperial_units_show_feet_and_inches_and_survive_a_save() {
    let mut s = Session::default();
    let units = run(&mut s, "units_set", json!({"preset": "imperial"}));
    assert_eq!(units["length"]["unit"], "FeetInches");
    assert_eq!(units["length"]["decimals"], 4);
    assert_eq!(units["area"]["unit"], "SquareFoot");
    // Values merge over the preset: precision to 1/8".
    let units = run(&mut s, "units_set", json!({"preset": "imperial", "values": {"length": {"decimals": 3}}}));
    assert_eq!(units["length"]["decimals"], 3);
    // A 12'-6" wall: lengths are still meters in the model and the commands.
    let wall = run(&mut s, "wall_create", json!({"start": [0, 0], "end": [3.81, 0]}));
    let end = run(&mut s, "element_get", json!({"id": wall["id"]}))["params"]["end"][0].as_f64().unwrap();
    assert!((end - 3.81).abs() < 1e-12);
    // A schedule shows feet and inches, areas in square feet.
    let schedule = run(&mut s, "view_create", json!({"kind": "schedule", "category": "wall"}));
    let table = run(&mut s, "schedule_table", json!({"view": schedule["id"]})).to_string();
    assert!(table.contains("12'-6\\\""), "{table}");
    assert!(table.contains("ft²"), "{table}");
    // Saved and reopened, the project keeps its units.
    let path = scratch("imperial.ccad");
    run(&mut s, "project_save", json!({"path": path}));
    let mut reopened = Session::default();
    run(&mut reopened, "project_open", json!({"path": path}));
    assert_eq!(run(&mut reopened, "units_get", json!({}))["length"]["unit"], "FeetInches");
    // Back to metric; unknown presets and units are refused.
    let units = run(&mut s, "units_set", json!({"preset": "metric"}));
    assert_eq!(units["length"]["unit"], "Meter");
    assert_eq!(fails(&mut s, "units_set", json!({"preset": "nautical"})), ErrorCode::InvalidArguments);
    assert_eq!(
        fails(&mut s, "units_set", json!({"values": {"length": {"unit": "Furlong"}}})),
        ErrorCode::InvalidArguments
    );
}

#[test]
fn ordinate_dimensions_show_running_totals_and_follow_the_model() {
    let mut s = Session::default();
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/casa_simples.ccad");
    run(&mut s, "project_open", json!({"path": sample}));
    let plan = run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]["id"].clone();
    // A row of 20 cm studs (columns) far from the house; the origin is the first one's left face.
    let studs: Vec<Value> =
        [50.0, 50.6, 51.2, 52.4].iter().map(|x| run(&mut s, "column_create", json!({"position": [x, 50.0]}))).collect();
    let depth = s.doc.undo_depth();
    let out = run(
        &mut s,
        "dimension_ordinate_create",
        json!({"view": plan, "points": [[49.9, 50.0], [50.5, 50.0], [51.1, 50.05], [52.3, 50.0]], "offset": -1.0}),
    );
    assert_eq!(out["count"], 3);
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    let f = |v: &Value| v.as_f64().unwrap();
    let get = |s: &mut Session, id: &Value| run(s, "element_get", json!({"id": id}))["params"].clone();
    let total = |p: &Value| f(&p["b"][0]) - f(&p["a"][0]);
    for (id, value) in out["ids"].as_array().unwrap().iter().zip([0.6, 1.2, 2.4]) {
        let p = get(&mut s, id);
        assert_eq!(p["kind"]["Ordinate"]["dir"], json!([1.0, 0.0]), "{p}");
        // One origin and one line for the whole set.
        assert_eq!(p["a"], json!([49.9, 50.0]), "{p}");
        assert!((f(&p["offset"]) + 1.0).abs() < 1e-9, "{p}");
        assert!((total(&p) - value).abs() < 1e-9, "{p}");
        assert_eq!(p["refs"][0]["target"]["Element"], studs[0]["id"], "{p}");
    }
    // Moving the origin's stud moves every total; moving one stud only its own.
    run(&mut s, "elements_move", json!({"ids": [studs[0]["id"]], "delta": [-0.1, 0.0]}));
    let p = get(&mut s, &out["ids"][2]);
    assert!((total(&p) - 2.5).abs() < 1e-9, "{p}");
    run(&mut s, "elements_move", json!({"ids": [studs[2]["id"]], "delta": [0.2, 0.0]}));
    assert!((total(&get(&mut s, &out["ids"][1])) - 1.5).abs() < 1e-9);
    assert!((total(&get(&mut s, &out["ids"][2])) - 2.5).abs() < 1e-9);
    // A new value moves the stud at the point, never the origin.
    run(&mut s, "dimension_set_value", json!({"id": out["ids"][1], "value": 1.3}));
    let x = f(&run(&mut s, "element_get", json!({"id": studs[2]["id"]}))["params"]["position"][0]);
    assert!((x - 51.2).abs() < 1e-9, "{x}");
    let x0 = f(&run(&mut s, "element_get", json!({"id": studs[0]["id"]}))["params"]["position"][0]);
    assert!((x0 - 49.9).abs() < 1e-9, "{x0}");
    // Mirrored with its studs, the set keeps measuring from the same (mirrored) origin.
    let mut ids: Vec<Value> = studs.iter().map(|c| c["id"].clone()).collect();
    ids.extend(out["ids"].as_array().unwrap().iter().cloned());
    let copy = run(&mut s, "elements_mirror", json!({"ids": ids, "axis": [[60.0, 0.0], [60.0, 1.0]], "copy": true}));
    let copies: Vec<Value> = copy["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| run(&mut s, "element_get", json!({"id": id})))
        .filter(|e| e["category"] == "dimension")
        .collect();
    assert_eq!(copies.len(), 3);
    for (c, value) in copies.iter().zip([0.7, 1.3, 2.5]) {
        let p = &c["params"];
        assert!((f(&p["a"][0]) - 70.2).abs() < 1e-9, "{p}");
        assert!((total(p) + value).abs() < 1e-9, "{p}");
        assert!(p["refs"][0].is_object() && p["refs"][1].is_object(), "{p}");
    }
    // `dimension_create` places a single one; `angle` measures along any direction.
    let one = run(&mut s, "dimension_create", json!({"view": plan, "a": [0, 100], "b": [0, 103], "kind": "ordinate"}));
    assert_eq!(get(&mut s, &one["id"])["kind"]["Ordinate"]["dir"], json!([0.0, 1.0]));
    let out =
        run(&mut s, "dimension_ordinate_create", json!({"view": plan, "points": [[200, 0], [201, 2]], "angle": 90.0}));
    assert_eq!(get(&mut s, &out["ids"][0])["kind"]["Ordinate"]["dir"], json!([0.0, 1.0]));

    // Bad sets are refused and change nothing.
    let depth = s.doc.undo_depth();
    for args in [
        json!({"view": plan, "points": [[0, 0]]}),
        json!({"view": plan, "points": [[0, 0], [2, 0], [0, 3]], "direction": "horizontal"}),
        json!({"view": plan, "points": [[0, 0], [2, 0]], "direction": "sideways"}),
        json!({"view": plan, "points": [[0, 0], [0, 0]]}),
    ] {
        assert_eq!(fails(&mut s, "dimension_ordinate_create", args), ErrorCode::InvalidArguments);
    }
    assert_eq!(s.doc.undo_depth(), depth);
}

#[test]
fn turning_and_pinned_elements() {
    let mut s = Session::default();
    let wall = run(&mut s, "wall_create", json!({"start": [0, 0], "end": [6, 0]}));
    let door = run(&mut s, "door_create", json!({"wall": wall["id"]}));
    let column = run(&mut s, "column_create", json!({"position": [3, 3], "rotation": 300}));
    let out = run(&mut s, "elements_turn", json!({"ids": [door["id"], column["id"], wall["id"]]}));
    assert_eq!(out["changed"].as_array().unwrap().len(), 2, "walls do not turn");
    assert_eq!(run(&mut s, "element_get", json!({"id": column["id"]}))["params"]["rotation_deg"], 30.0);
    assert_eq!(run(&mut s, "element_get", json!({"id": door["id"]}))["params"]["flip_facing"], true);
    // Pinned elements refuse direct edits and stay out of group ones.
    run(&mut s, "elements_pin", json!({"ids": [wall["id"]], "pinned": true}));
    assert_eq!(fails(&mut s, "element_split", json!({"id": wall["id"], "point": [3, 0]})), ErrorCode::Rejected);
    assert_eq!(
        fails(&mut s, "element_move_end", json!({"id": wall["id"], "end": "end", "point": [8, 0]})),
        ErrorCode::Rejected
    );
    let moved = run(&mut s, "elements_move", json!({"ids": [wall["id"], column["id"]], "delta": [1, 0]}));
    assert_eq!(moved["pinned_skipped"], json!([wall["id"]]));
}

/// A door raised above the wall's base (#70): the opening in the wall
/// moves up with it, and the head is the raised sill plus the door's height.
#[test]
fn raised_doors_cut_their_wall_higher() {
    let mut s = Session::default();
    let wall = run(&mut s, "wall_create", json!({"start": [0, 0], "end": [5, 0]}));
    let door = run(&mut s, "door_create", json!({"wall": wall["id"]}));
    let opening = |s: &mut Session| {
        let o = &s.geometry().walls[&cc_core::ElementId(id(&wall))].openings[0];
        (o.z0, o.z1)
    };
    let (z0, z1) = opening(&mut s);
    run(&mut s, "element_set", json!({"id": door["id"], "params": {"sill_height": 0.3}}));
    let (r0, r1) = opening(&mut s);
    assert!((r0 - z0 - 0.3).abs() < 1e-9 && (r1 - z1 - 0.3).abs() < 1e-9, "({z0}, {z1}) → ({r0}, {r1})");
}

#[test]
fn export_and_paste_elements() {
    let mut s = Session::default();
    let wall = run(&mut s, "wall_create", json!({"start": [0, 0], "end": [5, 0]}));
    run(&mut s, "door_create", json!({"wall": wall["id"]}));
    let set = run(&mut s, "elements_export", json!({"ids": [wall["id"]]}));
    assert_eq!(set["elements"].as_array().unwrap().len(), 2, "the door comes along");
    let pasted = run(&mut s, "elements_paste", json!({"elements": set["elements"], "delta": [0, 4]}));
    let created = pasted["created"].as_array().unwrap();
    assert_eq!(created.len(), 2);
    let y = |s: &mut Session, id: &Value| run(s, "element_get", json!({"id": id}))["params"]["start"][1].as_f64();
    assert!(created.iter().any(|id| y(&mut s, id) == Some(4.0)));
    assert_eq!(fails(&mut s, "elements_paste", json!({"elements": [{"bogus": true}]})), ErrorCode::InvalidArguments);
}

/// Columns copied on one level and pasted on another keep their span: the
/// top moves up as many levels as the base (issue #99: it stayed on the
/// level it was copied with, so the copies ended at their own base). With
/// no level that high, the top is left unconnected at the same height.
#[test]
fn pasted_columns_reach_the_matching_level() {
    let mut s = Session::default();
    let levels = run(&mut s, "levels_list", json!({}));
    let ids: Vec<u64> = levels.as_array().unwrap().iter().map(id).collect();
    assert_eq!(ids.len(), 2, "the template's ground and first floor");
    let roof = id(&run(&mut s, "level_create", json!({"name": "Cobertura", "elevation": 6.0})));
    let column = run(&mut s, "column_create", json!({"position": [0, 0], "level": ids[0], "top_level": ids[1]}));
    let set = run(&mut s, "elements_export", json!({"ids": [id(&column)]}));
    let top_of = |s: &mut Session, c: &Value| {
        let e = run(s, "element_get", json!({"id": c}));
        (e["level"]["id"].as_u64(), e["params"]["top"]["Level"]["level"].as_u64(), e["params"]["height"].as_f64())
    };
    let pasted = run(&mut s, "elements_paste", json!({"elements": set["elements"], "level": ids[1]}));
    let (base, top, _) = top_of(&mut s, &pasted["created"][0]);
    assert_eq!((base, top), (Some(ids[1]), Some(roof)), "one level up, base and top");
    // From the top level there is none above: unconnected, as tall as the original.
    let pasted = run(&mut s, "elements_paste", json!({"elements": set["elements"], "level": roof}));
    let (base, top, height) = top_of(&mut s, &pasted["created"][0]);
    assert_eq!((base, top), (Some(roof), None));
    let floor = run(&mut s, "levels_list", json!({}))[1]["elevation"].as_f64().unwrap();
    assert!((height.unwrap() - floor).abs() < 1e-9, "{height:?} vs {floor}");
    // On its own level nothing changes.
    let pasted = run(&mut s, "elements_paste", json!({"elements": set["elements"], "level": ids[0], "delta": [2, 0]}));
    assert_eq!(top_of(&mut s, &pasted["created"][0]).1, Some(ids[1]));
}

/// A window copied without its wall and pasted on another level goes into
/// the wall there (it used to stay on the wall it was copied from).
#[test]
fn a_lone_window_pasted_on_another_level_takes_the_wall_there() {
    let mut s = Session::default();
    let wall = run(&mut s, "wall_create", json!({"start": [0, 0], "end": [6, 0]}));
    let upper = run(&mut s, "level_create", json!({"name": "Superior", "elevation": 3.0}));
    let set = run(&mut s, "elements_export", json!({"ids": [wall["id"]]}));
    let copy = run(&mut s, "elements_paste", json!({"elements": set["elements"], "level": upper["id"]}));
    let upper_wall = copy["created"][0].clone();
    let window = run(&mut s, "window_create", json!({"wall": wall["id"], "point": [2.0, 0.0]}));
    let set = run(&mut s, "elements_export", json!({"ids": [window["id"]]}));
    let pasted = run(&mut s, "elements_paste", json!({"elements": set["elements"], "level": upper["id"]}));
    let new = &pasted["created"][0];
    let got = run(&mut s, "element_get", json!({"id": new}));
    assert_eq!(got["params"]["host"], upper_wall, "{got}");
    assert_eq!(got["level"]["id"], upper["id"], "{got}");
    // With no wall where it lands, nothing is pasted.
    let empty = run(&mut s, "level_create", json!({"name": "Vazio", "elevation": 6.0}));
    let none = run(&mut s, "elements_paste", json!({"elements": set["elements"], "level": empty["id"]}));
    assert_eq!(none["created"], json!([]));
}
#[test]
fn merged_edits_are_one_undo_step() {
    use cc_engine::commands::edit::ElementSet;
    let mut s = Session::default();
    let wall = run(&mut s, "wall_create", json!({"start": [0, 0], "end": [5, 0], "height": 3.0}));
    let id = wall["id"].as_u64().unwrap();
    let depth = s.doc.undo_depth();
    for h in [3.1, 3.2, 3.3] {
        let set = ElementSet {
            id,
            level: None,
            type_ref: None,
            mark: None,
            comments: None,
            params: Some(json!({"height": h})),
        };
        s.run_merged(7, set).unwrap();
    }
    assert_eq!(s.doc.undo_depth(), depth + 1);
    run(&mut s, "undo", json!({}));
    assert_eq!(run(&mut s, "element_get", json!({"id": id}))["params"]["height"], 3.0);
}

#[test]
fn recording_a_macro() {
    use cc_engine::commands::edit::ElementSet;
    let mut s = Session::default();
    s.start_recording();
    let wall = run(&mut s, "wall_create", json!({"start": [0, 0], "end": [5, 0]}));
    run(&mut s, "elements_list", json!({}));
    assert!(s.execute("element_get", json!({"id": 424242})).is_err());
    let id = wall["id"].as_u64().unwrap();
    for h in [3.1, 3.2, 3.3] {
        let set = ElementSet {
            id,
            level: None,
            type_ref: None,
            mark: None,
            comments: None,
            params: Some(json!({"height": h})),
        };
        s.run_merged(9, set).unwrap();
    }
    // A cancelled dialog's edits are dropped from the recording.
    let depth = s.doc.undo_depth();
    run(&mut s, "level_create", json!({}));
    s.undo_to(depth);
    let steps = s.stop_recording();
    let names: Vec<&str> = steps.iter().map(|st| st.command.as_str()).collect();
    assert_eq!(names, ["wall_create", "element_set"], "queries, failures and cancelled edits are left out");
    assert_eq!(steps[1].args["params"]["height"], 3.3, "a dragged value is recorded once, at its end");
    // The macro replays on a new project.
    let text = cc_engine::script::to_text(&steps);
    let mut other = Session::default();
    cc_engine::script::run(&mut other, &cc_engine::script::parse(&text).unwrap()).unwrap();
    let walls = run(&mut other, "elements_list", json!({"category": "wall"}));
    let replayed = run(&mut other, "element_get", json!({"id": walls[0]["id"]}));
    assert_eq!(replayed["params"]["height"], 3.3);
}

/// The example macro in `examples/` keeps working.
#[test]
fn the_example_macro_builds_a_house() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/small_house.ccmacro");
    let steps = cc_engine::script::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut s = Session::default();
    cc_engine::script::run(&mut s, &steps).unwrap_or_else(|(_, e)| panic!("step {}: {}", e.index + 1, e.error));
    let count = |s: &mut Session, c: &str| run(s, "elements_list", json!({"category": c})).as_array().unwrap().len();
    assert_eq!(
        [count(&mut s, "wall"), count(&mut s, "door"), count(&mut s, "window"), count(&mut s, "room")],
        [5, 2, 3, 2]
    );
    let room = run(&mut s, "elements_list", json!({"category": "room"}))[0]["id"].clone();
    assert!(run(&mut s, "element_get", json!({"id": room}))["derived"]["area"].as_f64().unwrap() > 20.0);
}

/// Floors, roofs and ceilings from walls follow the outer boundary of the
/// walls: here the showcase house, an L with an arc wall, whose interior walls
/// meet the outer corners (once picked as the old 9 x 5 rectangle).
#[test]
fn outlines_from_walls_follow_the_outer_boundary() {
    let mut s = Session::default();
    let walls = [
        json!({"start": [0, 0], "end": [9, 0], "type": 3}),
        json!({"start": [9, 0], "end": [9, 5], "type": 3}),
        json!({"start": [9, 5], "end": [0, 5], "type": 3}),
        json!({"start": [0, 5], "end": [0, 0], "type": 3}),
        json!({"start": [5.5, 0], "end": [5.5, 5], "type": 5}),
        json!({"start": [3, 0], "end": [3, 5], "type": 5}),
        json!({"start": [9, 0], "end": [12, 0], "type": 3}),
        json!({"start": [12, 0], "end": [12, 10], "type": 3}),
        json!({"start": [12, 10], "end": [7, 10], "mid": [9.5, 11.6], "type": 3}),
        json!({"start": [7, 10], "end": [7, 5], "type": 3}),
        json!({"start": [9, 5], "end": [12, 5], "type": 3}),
        json!({"start": [9.5, 5], "end": [9.5, 7.5], "type": 5}),
        json!({"start": [9.5, 7.5], "end": [12, 7.5], "type": 5}),
    ];
    for wall in walls {
        run(&mut s, "wall_create", wall);
    }
    // Type 3 is 15 cm thick.
    let half = 0.075;
    // The L on the wall lines: 12 x 5 plus 5 x 5 beside the arc, plus the arc's segment.
    let arc = run(&mut s, "elements_list", json!({"category": "wall"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|w| run(&mut s, "element_get", json!({"id": w["id"]})))
        .find(|w| w["params"]["bulge"].as_f64().unwrap_or(0.0) != 0.0)
        .unwrap();
    let (r, sweep) =
        (arc["derived"]["radius"].as_f64().unwrap(), arc["derived"]["sweep"].as_f64().unwrap().to_radians());
    let segment = r * r * 0.5 * (sweep.abs() - sweep.abs().sin());
    let on_lines = 12.0 * 5.0 + 5.0 * 5.0 + segment;
    let area = |v: &Value| {
        let outline = if v["params"]["boundary"].is_array() { "boundary" } else { "footprint" };
        let pts: Vec<(f64, f64)> = v["params"][outline]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p[0].as_f64().unwrap(), p[1].as_f64().unwrap()))
            .collect();
        let n = pts.len();
        (0..n).map(|i| pts[i].0 * pts[(i + 1) % n].1 - pts[(i + 1) % n].0 * pts[i].1).sum::<f64>().abs() / 2.0
    };
    // Exterior faces: grown by half a wall all around (perimeter 44 m on the lines, about).
    let floor = run(&mut s, "floor_create", json!({"from_walls": true}));
    let grown = area(&floor);
    assert!(grown > on_lines + 40.0 * half && grown < on_lines + 50.0 * half, "{grown} vs {on_lines}");
    let roof = run(&mut s, "roof_create", json!({"from_walls": true}));
    assert!((area(&roof) - grown).abs() < 1e-6);
    // Interior faces for ceilings.
    let ceiling = run(&mut s, "ceiling_create", json!({"from_walls": true}));
    let shrunk = area(&ceiling);
    assert!(shrunk < on_lines - 40.0 * half && shrunk > on_lines - 50.0 * half, "{shrunk} vs {on_lines}");
}

/// The showcase macro (the project of the README screenshots) keeps working,
/// and the ids its later steps name still point at what they expect.
#[test]
fn the_showcase_macro_builds_its_project() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase.ccmacro");
    let steps = cc_engine::script::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut s = Session::default();
    cc_engine::script::run(&mut s, &steps).unwrap_or_else(|(_, e)| panic!("step {}: {}", e.index + 1, e.error));
    let count = |s: &mut Session, c: &str| run(s, "elements_list", json!({"category": c})).as_array().unwrap().len();
    assert_eq!([count(&mut s, "wall"), count(&mut s, "door"), count(&mut s, "room")], [13, 5, 5]);
    assert!(count(&mut s, "rebar") > 0 && count(&mut s, "pipe") > 0 && count(&mut s, "conduit") > 0);
    let get = |s: &mut Session, id: u64| run(s, "element_get", json!({"id": id}));
    assert!(get(&mut s, 97)["params"]["bulge"].as_f64().unwrap() > 0.0, "the arc wall");
    assert!(get(&mut s, 116)["params"]["shape"]["Hip"].is_object(), "the hip roof");
    assert_eq!(get(&mut s, 94)["phase_demolished"]["id"], 81, "the demolished partition");
    assert_eq!(get(&mut s, 247)["type"]["id"], 58, "the distribution board");
    let views = run(&mut s, "views_list", json!({}));
    let view = |id: u64| views.as_array().unwrap().iter().find(|v| v["id"] == id).map(|v| v["name"].clone());
    assert_eq!(view(334), Some(json!("Board diagram")));
    assert_eq!(view(342), Some(json!("3D North-east")));
    let loads = run(&mut s, "circuit_loads", json!({"panel": 247}));
    assert!(loads.to_string().contains("Kitchen and bath outlets"));
    for filter in [341, 343] {
        assert!(run(&mut s, "filter_elements", json!({"filter": filter, "view": 340}))["count"].as_u64().unwrap() > 0);
    }
}

#[test]
fn text_notes_with_leaders() {
    let mut s = Session::default();
    let plan = run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]["id"].clone();
    // Single-line text, then multiline text with a straight leader and a two-segment one.
    let single = run(&mut s, "text_create", json!({"view": plan, "position": [1, 1], "text": "Living room"}));
    assert!(single["params"]["width"].is_null());
    let multi = run(
        &mut s,
        "text_create",
        json!({
            "view": plan, "position": [5, 5], "text": "Exposed concrete\nwith sealant", "width": 30.0,
            "leaders": [
                {"end": [2, 2]},
                {"end": [9, 2], "elbow": [8, 4], "symbol": "Circle"}
            ]
        }),
    );
    let leaders = &multi["params"]["leaders"];
    assert_eq!(leaders[0]["symbol"], "Arrow", "the type's symbol by default");
    assert_eq!(leaders[1]["elbow"], json!([8.0, 4.0]));
    assert_eq!(leaders[1]["symbol"], "Circle");
    // The view draws the lines of text and the leaders.
    let view = cc_core::ViewId(plan.as_u64().unwrap());
    let drawing = s.drawing(view).unwrap();
    let texts: Vec<&str> = drawing
        .prims
        .iter()
        .filter_map(|p| match p {
            cc_views::Prim::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    for t in ["Living room", "Exposed concrete", "with sealant"] {
        assert!(texts.contains(&t), "{t} in {texts:?}");
    }
    // Edit the text, add and remove leaders, move it.
    let id = multi["id"].clone();
    let edited = run(&mut s, "element_set", json!({"id": id, "params": {"text": "Sealed concrete", "width": null}}));
    assert_eq!(edited["params"]["text"], "Sealed concrete");
    assert!(edited["params"]["width"].is_null(), "now single-line");
    let added = run(&mut s, "text_add_leader", json!({"id": id, "end": [0, 9], "symbol": "Tick"}));
    assert_eq!(added["params"]["leaders"].as_array().unwrap().len(), 3);
    let removed = run(&mut s, "text_remove_leader", json!({"id": id, "index": 0}));
    assert_eq!(removed["params"]["leaders"][0]["symbol"], "Circle");
    assert_eq!(fails(&mut s, "text_remove_leader", json!({"id": id, "index": 9})), ErrorCode::NotFound);
    let moved = run(&mut s, "elements_move", json!({"ids": [id], "delta": [1, 0]}));
    assert_eq!(moved["changed"], json!([id]));
    let after = run(&mut s, "element_get", json!({"id": id}));
    assert_eq!(after["params"]["position"], json!([6.0, 5.0]));
    assert_eq!(after["params"]["leaders"][0]["elbow"], json!([9.0, 4.0]), "leaders move with the text");
    // Texts and leaders survive saving and opening.
    let path = scratch("text.ccad");
    run(&mut s, "project_save", json!({"path": path}));
    let mut reopened = Session::default();
    run(&mut reopened, "project_open", json!({"path": path}));
    assert_eq!(run(&mut reopened, "element_get", json!({"id": id}))["params"], after["params"]);
    // Refusals.
    assert_eq!(
        fails(&mut s, "text_create", json!({"view": plan, "position": [0, 0], "text": "  "})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(
            &mut s,
            "text_create",
            json!({"view": plan, "position": [0, 0], "text": "x", "leaders": [{"end": [1, 1], "symbol": "Star"}]})
        ),
        ErrorCode::InvalidArguments
    );
    let schedule = run(&mut s, "view_create", json!({"kind": "schedule"}));
    assert_eq!(
        fails(&mut s, "text_create", json!({"view": schedule["id"], "position": [0, 0], "text": "x"})),
        ErrorCode::InvalidArguments
    );
}

#[test]
fn joined_geometry_follows_the_cutting_order() {
    let mut s = Session::default();
    // A wall with a structural column in it, and an architectural one.
    let wall = id(&run(&mut s, "wall_create", json!({"start": [0, 0], "end": [6, 0], "height": 3.0})));
    let column = id(&run(&mut s, "column_create", json!({"position": [3, 0], "height": 3.0})));
    let deco = id(&run(&mut s, "column_create", json!({"position": [5, 0], "height": 3.0, "structural": false})));
    let far = id(&run(&mut s, "column_create", json!({"position": [3, 9], "height": 3.0})));
    let volume =
        |s: &mut Session, e: u64| run(s, "element_get", json!({"id": e}))["derived"]["volume"].as_f64().unwrap();
    let wall_v = volume(&mut s, wall);
    let deco_v = volume(&mut s, deco);

    // Structural elements cut hosts, whichever is picked first.
    let joined = run(&mut s, "geometry_join", json!({"first": wall, "second": column}));
    assert_eq!(joined, json!({"cutting": column, "cut": wall}));
    assert!(volume(&mut s, wall) < wall_v - 1e-6, "the wall gives up the column's volume");
    assert_eq!(run(&mut s, "element_get", json!({"id": wall}))["joined_with"], json!([column]));
    // Walls cut architectural columns.
    let joined = run(&mut s, "geometry_join", json!({"first": deco, "second": wall}));
    assert_eq!(joined, json!({"cutting": wall, "cut": deco}));
    assert!(volume(&mut s, deco) < deco_v - 1e-6);

    // Unjoin gives the volume back; undo joins again.
    run(&mut s, "geometry_unjoin", json!({"first": column, "second": wall}));
    run(&mut s, "geometry_unjoin", json!({"first": wall, "second": deco}));
    assert!((volume(&mut s, wall) - wall_v).abs() < 1e-9);
    run(&mut s, "undo", json!({}));
    assert_eq!(run(&mut s, "element_get", json!({"id": deco}))["joined_with"], json!([wall]));

    // Deleting an element drops its joins.
    run(&mut s, "geometry_join", json!({"first": wall, "second": column}));
    run(&mut s, "elements_delete", json!({"ids": [column]}));
    assert_eq!(run(&mut s, "element_get", json!({"id": wall}))["joined_with"], json!([deco]));

    // Refusals: apart, itself, not joined, no body.
    assert_eq!(fails(&mut s, "geometry_join", json!({"first": wall, "second": far})), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "geometry_join", json!({"first": wall, "second": wall})), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "geometry_unjoin", json!({"first": wall, "second": far})), ErrorCode::NotFound);
    let grid = id(&run(&mut s, "grid_create", json!({"start": [0, -1], "end": [6, -1]})));
    assert_eq!(fails(&mut s, "geometry_join", json!({"first": wall, "second": grid})), ErrorCode::InvalidArguments);
}

#[test]
fn instances_of_a_type_shown_in_a_view() {
    let mut s = Session::default();
    let levels = run(&mut s, "levels_list", json!({}));
    let upper = levels[1]["id"].clone();
    let a = id(&run(&mut s, "wall_create", json!({"start": [0, 0], "end": [4, 0]})));
    let b = id(&run(&mut s, "wall_create", json!({"start": [4, 0], "end": [4, 4]})));
    let up = id(&run(&mut s, "wall_create", json!({"start": [0, 0], "end": [4, 0], "level": upper})));
    let ty = run(&mut s, "element_get", json!({"id": a}))["type"]["id"].clone();
    let plan = run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]["id"].clone();
    let ids = |v: Value| -> Vec<u64> { v.as_array().unwrap().iter().map(|e| e["id"].as_u64().unwrap()).collect() };
    let all = ids(run(&mut s, "elements_list", json!({"type": ty})));
    assert!(all.contains(&a) && all.contains(&b) && all.contains(&up));
    let shown = ids(run(&mut s, "elements_list", json!({"type": ty, "view": plan})));
    assert_eq!(shown, vec![a, b], "the upper wall is not in the ground plan");
    let schedule = run(&mut s, "view_create", json!({"kind": "schedule"}));
    let refused = fails(&mut s, "elements_list", json!({"type": ty, "view": schedule["id"]}));
    assert_eq!(refused, ErrorCode::InvalidArguments);
}

#[test]
fn groups_keep_elements_together() {
    let mut s = Session::default();
    let a = id(&run(&mut s, "wall_create", json!({"start": [0, 0], "end": [4, 0]})));
    let b = id(&run(&mut s, "wall_create", json!({"start": [4, 0], "end": [4, 4]})));
    let c = id(&run(&mut s, "column_create", json!({"position": [8, 8]})));
    let group = run(&mut s, "group_create", json!({"ids": [a, b]}));
    let g = id(&group);
    assert_eq!(group["members"], json!([a, b]));
    assert_eq!(run(&mut s, "element_get", json!({"id": a}))["group"], g);
    assert_eq!(run(&mut s, "groups_list", json!({}))[0]["name"], group["name"]);
    // An element joins one group at most: regrouping takes it out of the first.
    let other = run(&mut s, "group_create", json!({"ids": [b, c], "name": "Core"}));
    assert_eq!(run(&mut s, "element_get", json!({"id": b}))["group"], other["id"]);
    // The first group is left with one member; deleting it empties and removes the group.
    run(&mut s, "elements_delete", json!({"ids": [a]}));
    assert_eq!(run(&mut s, "groups_list", json!({})).as_array().unwrap().len(), 1);
    run(&mut s, "group_ungroup", json!({"group": id(&other)}));
    assert!(run(&mut s, "element_get", json!({"id": b}))["group"].is_null());
    run(&mut s, "undo", json!({}));
    assert_eq!(run(&mut s, "element_get", json!({"id": c}))["group"], other["id"]);
    assert_eq!(fails(&mut s, "group_create", json!({"ids": [c]})), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "group_ungroup", json!({"group": 999})), ErrorCode::NotFound);
}

#[test]
fn measuring_creates_nothing() {
    let mut s = Session::default();
    let before = run(&mut s, "project_info", json!({}));
    let m = run(&mut s, "measure", json!({"a": [1, 1], "b": [4, 5]}));
    assert!((m["distance"].as_f64().unwrap() - 5.0).abs() < 1e-12);
    assert_eq!((m["dx"].clone(), m["dy"].clone()), (json!(3.0), json!(4.0)));
    assert!((m["angle"].as_f64().unwrap() - 53.130102354).abs() < 1e-6);
    assert!(m["between_lines"].is_null());
    // Two parallel wall faces 0.2 apart, measured on a slant.
    let m = run(
        &mut s,
        "measure",
        json!({"a": [0, 0], "b": [3, 0.2], "line_a": [[-1, 0], [5, 0]], "line_b": [[0, 0.2], [9, 0.2]]}),
    );
    assert!((m["between_lines"].as_f64().unwrap() - 0.2).abs() < 1e-12);
    assert_eq!(run(&mut s, "project_info", json!({})), before);
}

#[test]
fn beams_frame_into_columns_and_each_other() {
    let mut s = Session::default();
    // Two 0.2 m columns 5 m apart with a 0.15 m beam between their centers,
    // a beam going north from the second and one along the north side.
    let c1 = id(&run(&mut s, "column_create", json!({"position": [0, 0]})));
    let c2 = id(&run(&mut s, "column_create", json!({"position": [5, 0]})));
    let span = id(&run(&mut s, "beam_create", json!({"start": [0, 0], "end": [5, 0]})));
    let side = id(&run(&mut s, "beam_create", json!({"start": [5, 0], "end": [5, 4]})));
    let north = id(&run(&mut s, "beam_create", json!({"start": [0, 4], "end": [5, 4]})));
    let derived = |s: &mut Session, beam: u64| run(s, "element_get", json!({"id": beam}))["derived"].clone();

    let d = derived(&mut s, span);
    assert_eq!((d["start_join"].clone(), d["end_join"].clone()), (json!({"column": c1}), json!({"column": c2})));
    assert!((d["length"].as_f64().unwrap() - 5.0).abs() < 1e-9);
    assert!((d["cut_length"].as_f64().unwrap() - 4.8).abs() < 1e-9, "{d}");
    assert!((d["volume"].as_f64().unwrap() - 0.15 * 0.4 * 4.8).abs() < 1e-9, "{d}");
    // A mitered corner between the side and north beams.
    let d = derived(&mut s, side);
    assert_eq!((d["start_join"].clone(), d["end_join"].clone()), (json!({"column": c2}), json!("beam")));
    assert_eq!(derived(&mut s, north)["end_join"], json!("beam"));

    // A disallowed end stays square at its location point, and undoes.
    run(&mut s, "wall_join", json!({"id": span, "end": "start", "joined": false}));
    let d = derived(&mut s, span);
    assert_eq!(d["start_join"], json!("free"));
    assert!((d["cut_length"].as_f64().unwrap() - 4.9).abs() < 1e-9, "{d}");
    assert_eq!(run(&mut s, "element_get", json!({"id": span}))["params"]["join_start"], json!(false));
    run(&mut s, "undo", json!({}));
    assert_eq!(derived(&mut s, span)["start_join"], json!({"column": c1}));
    assert_eq!(
        fails(&mut s, "wall_join", json!({"id": c1, "end": "start", "joined": false})),
        ErrorCode::InvalidArguments
    );
}

#[test]
fn columns_and_beams_from_grids() {
    let mut s = Session::default();
    // Two grids each way: a 6 x 4 m bay, plus a short grid crossing nothing.
    let a = id(&run(&mut s, "grid_create", json!({"start": [-1, 0], "end": [7, 0]})));
    let b = id(&run(&mut s, "grid_create", json!({"start": [-1, 4], "end": [7, 4]})));
    let one = id(&run(&mut s, "grid_create", json!({"start": [0, -1], "end": [0, 5]})));
    let two = id(&run(&mut s, "grid_create", json!({"start": [6, -1], "end": [6, 5]})));
    run(&mut s, "grid_create", json!({"start": [20, 20], "end": [21, 20]}));
    let columns = |s: &mut Session| run(s, "elements_list", json!({"category": "column"})).as_array().unwrap().len();

    let out = run(&mut s, "columns_at_grids", json!({}));
    assert_eq!(out["created"].as_array().unwrap().len(), 4, "{out}");
    assert_eq!(columns(&mut s), 4);
    // Once is enough: the intersections are taken now.
    let again = run(&mut s, "columns_at_grids", json!({"grids": [a, b, one, two]}));
    assert_eq!((again["created"].as_array().unwrap().len(), again["skipped"].as_u64()), (0, Some(4)));
    // All four columns came in one undo step.
    run(&mut s, "undo", json!({}));
    assert_eq!(columns(&mut s), 0);
    run(&mut s, "redo", json!({}));
    assert_eq!(fails(&mut s, "columns_at_grids", json!({"grids": [a]})), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "columns_at_grids", json!({"grids": [a, 999_999]})), ErrorCode::NotFound);

    // A beam between the neighboring columns on each grid, framed into them.
    let out = run(&mut s, "beams_on_grids", json!({}));
    let beams: Vec<u64> = out["created"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    assert_eq!(beams.len(), 4, "{out}");
    let d = run(&mut s, "element_get", json!({"id": beams[0]}))["derived"].clone();
    assert!(d["start_join"]["column"].is_u64() && d["end_join"]["column"].is_u64(), "{d}");
    let again = run(&mut s, "beams_on_grids", json!({"grids": [a]}));
    assert_eq!((again["created"].as_array().unwrap().len(), again["skipped"].as_u64()), (0, Some(1)));

    // Beams in a chain through chosen columns: the diagonal is new, the side exists.
    let ids: Vec<u64> = run(&mut s, "elements_list", json!({"category": "column"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_u64().unwrap())
        .collect();
    let at = |s: &mut Session, id: u64| run(s, "element_get", json!({"id": id}))["params"]["position"].clone();
    let origin = *ids.iter().find(|&&c| at(&mut s, c) == json!([0.0, 0.0])).unwrap();
    let far = *ids.iter().find(|&&c| at(&mut s, c) == json!([6.0, 4.0])).unwrap();
    let side = *ids.iter().find(|&&c| at(&mut s, c) == json!([6.0, 0.0])).unwrap();
    let out = run(&mut s, "beams_between_columns", json!({"columns": [far, origin, side], "top_offset": 2.5}));
    assert_eq!((out["created"].as_array().unwrap().len(), out["skipped"].as_u64()), (1, Some(1)), "{out}");
    let diagonal = run(&mut s, "element_get", json!({"id": out["created"][0]}));
    assert_eq!(diagonal["params"]["top_offset"], json!(2.5));
    assert_eq!(fails(&mut s, "beams_between_columns", json!({"columns": [far]})), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "beams_between_columns", json!({"columns": [far, a]})), ErrorCode::InvalidArguments);
}

#[test]
fn beam_justification_moves_the_body() {
    let mut s = Session::default();
    let beam =
        id(&run(&mut s, "beam_create", json!({"start": [0, 0], "end": [4, 0], "top_offset": 3.0, "lateral": "left"})));
    let justified = |s: &mut Session| {
        let b = run(s, "element_get", json!({"id": beam}))["params"].clone();
        (b["lateral"].clone(), b["vertical"].clone())
    };
    assert_eq!(justified(&mut s), (json!("Left"), json!("Top")));
    run(&mut s, "element_set", json!({"id": beam, "params": {"vertical": "Bottom"}}));
    assert_eq!(justified(&mut s), (json!("Left"), json!("Bottom")));
    let d = run(&mut s, "element_get", json!({"id": beam}))["derived"].clone();
    assert!((d["volume"].as_f64().unwrap() - 0.15 * 0.4 * 4.0).abs() < 1e-9, "{d}");
    assert_eq!(
        fails(&mut s, "beam_create", json!({"start": [0, 0], "end": [1, 0], "lateral": "middle"})),
        ErrorCode::InvalidArguments
    );
}

#[test]
fn foundations_under_columns_and_walls() {
    let mut s = Session::default();
    let volume =
        |s: &mut Session, id: u64| run(s, "element_get", json!({"id": id}))["derived"]["volume"].as_f64().unwrap();
    // Two columns: a 1.00 x 1.00 x 0.40 pad under each, once.
    for x in [0, 5] {
        run(&mut s, "column_create", json!({"position": [x, 0]}));
    }
    let out = run(&mut s, "footings_under_columns", json!({}));
    let pads: Vec<u64> = out["created"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    assert_eq!(pads.len(), 2, "{out}");
    assert!((volume(&mut s, pads[0]) - 0.4).abs() < 1e-9);
    let pad = run(&mut s, "element_get", json!({"id": pads[0]}));
    assert_eq!(pad["category"], json!("foundation"));
    let again = run(&mut s, "footings_under_columns", json!({}));
    assert_eq!((again["created"].as_array().unwrap().len(), again["skipped"].as_u64()), (0, Some(2)));

    // A 6 x 4 m loop of walls: 0.20 m strips meeting at the corners tile 20 m of centerline.
    let corners = [[10, 0], [16, 0], [16, 4], [10, 4]];
    for i in 0..4 {
        run(&mut s, "wall_create", json!({"start": corners[i], "end": corners[(i + 1) % 4]}));
    }
    let out = run(&mut s, "strip_footings_under_walls", json!({}));
    let strips: Vec<u64> = out["created"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    assert_eq!(strips.len(), 4, "{out}");
    let total: f64 = strips.iter().map(|id| volume(&mut s, *id)).sum();
    assert!((total - 20.0 * 0.2 * 0.4).abs() < 1e-9, "{total}");

    // A raft under the loop of walls, 0.30 m thick.
    let raft = run(&mut s, "foundation_slab_create", json!({"boundary": [[10, 0], [16, 0], [16, 4], [10, 4]]}));
    assert!((volume(&mut s, id(&raft)) - 24.0 * 0.3).abs() < 1e-9);
    // A pad needs a pad type.
    assert_eq!(
        fails(&mut s, "footing_create", json!({"position": [0, 9], "type": "Radier 30 cm"})),
        ErrorCode::InvalidArguments
    );
    let pad = run(&mut s, "footing_create", json!({"position": [0, 9], "rotation": 45, "top_offset": -0.5}));
    assert_eq!(pad["params"]["top_offset"], json!(-0.5));
}

#[test]
fn sloped_beams_and_steel_sections() {
    let mut s = Session::default();
    let derived = |s: &mut Session, id: u64| run(s, "element_get", json!({"id": id}))["derived"].clone();
    // A beam rising 1 m over 4 m: its 0.15 x 0.40 section runs the true length.
    let beam =
        id(&run(&mut s, "beam_create", json!({"start": [0, 0], "end": [4, 0], "top_offset": 3.0, "end_offset": 4.0})));
    let d = derived(&mut s, beam);
    let true_length = 17f64.sqrt();
    assert!((d["true_length"].as_f64().unwrap() - true_length).abs() < 1e-9, "{d}");
    assert!((d["volume"].as_f64().unwrap() - 0.15 * 0.4 * true_length).abs() < 1e-9, "{d}");
    // Splitting keeps the slope: the parts meet at the elevation in between.
    let second = run(&mut s, "element_split", json!({"id": beam, "point": [2, 0]}));
    let first = run(&mut s, "element_get", json!({"id": beam}));
    assert_eq!(first["params"]["end_offset"], json!(3.5));
    assert_eq!(run(&mut s, "element_get", json!({"id": second["created"]}))["params"]["top_offset"], json!(3.5));

    // An I profile: two 0.02 flanges and a 0.01 web in a 0.15 x 0.40 box.
    let ty = run(&mut s, "type_duplicate", json!({"type": "Viga 15 x 40 cm", "name": "Perfil I", "elements": [beam]}));
    run(&mut s, "type_set", json!({"type": ty["id"], "params": {"shape": {"I": {"web": 0.01, "flange": 0.02}}}}));
    let area = 2.0 * 0.15 * 0.02 + 0.01 * (0.4 - 0.04);
    let d = derived(&mut s, beam);
    assert!((d["volume"].as_f64().unwrap() - area * true_length / 2.0).abs() < 1e-9, "{d}");
    // Back to a solid section: the variant is replaced, not merged.
    run(&mut s, "type_set", json!({"type": ty["id"], "params": {"shape": "Solid"}}));

    // A column switching to a tube profile and to a pipe.
    let column = id(&run(&mut s, "column_create", json!({"position": [9, 9], "height": 3.0})));
    let ty = run(&mut s, "type_duplicate", json!({"type": "Pilar 20 x 20 cm", "name": "Tubo", "elements": [column]}));
    let tube = json!({"Profile": {"width": 0.2, "depth": 0.2, "shape": {"Tube": {"wall": 0.01}}}});
    run(&mut s, "type_set", json!({"type": ty["id"], "params": {"section": tube}}));
    let volume = derived(&mut s, column)["volume"].as_f64().unwrap();
    assert!((volume - (0.04 - 0.18 * 0.18) * 3.0).abs() < 1e-9, "{volume}");
    run(
        &mut s,
        "type_set",
        json!({"type": ty["id"], "params": {"section": {"Pipe": {"diameter": 0.2, "wall": 0.01}}}}),
    );
    assert!(derived(&mut s, column)["volume"].as_f64().unwrap() < volume);
}

#[test]
fn slabs_join_their_supports_and_the_analytical_model() {
    let mut s = Session::default();
    // A slab over two columns and a beam hanging from it, and a beam well below it.
    for x in [0, 6] {
        run(&mut s, "column_create", json!({"position": [x, 0], "height": 3.0}));
    }
    let beam = id(&run(&mut s, "beam_create", json!({"start": [0, 0], "end": [6, 0], "top_offset": 3.0})));
    let low = id(&run(&mut s, "beam_create", json!({"start": [0, 2], "end": [6, 2], "top_offset": 1.5})));
    let slab = id(&run(
        &mut s,
        "floor_create",
        json!({"boundary": [[-1, -1], [7, -1], [7, 3], [-1, 3]], "level": "Pavimento 1", "offset": 0.0}),
    ));
    let before = run(&mut s, "element_get", json!({"id": slab}))["derived"]["volume"].as_f64().unwrap();
    let out = run(&mut s, "slab_join_supports", json!({}));
    let joined = out["joined"].as_array().unwrap();
    assert!(joined.iter().any(|j| j["cutting"] == json!(beam) && j["cut"] == json!(slab)), "{out}");
    assert!(!joined.iter().any(|j| j["cutting"] == json!(low)), "the low beam does not touch the slab");
    let after = run(&mut s, "element_get", json!({"id": slab}))["derived"]["volume"].as_f64().unwrap();
    assert!(after < before - 1e-6, "the beam took its share: {before} -> {after}");
    // Running it again joins nothing new.
    assert_eq!(run(&mut s, "slab_join_supports", json!({}))["joined"], json!([]));

    // The analytical model: the beam between the column axes, splitting both columns.
    let m = run(&mut s, "analytical_model", json!({}));
    let members = m["members"].as_array().unwrap();
    let of = |id: u64| members.iter().filter(|x| x["element"] == json!(id)).count();
    assert_eq!(of(beam), 1);
    assert_eq!(members.iter().filter(|x| x["kind"] == json!("column")).count(), 4, "{m}");
    let node = |i: &Value| m["nodes"][i.as_u64().unwrap() as usize].clone();
    let main = members.iter().find(|x| x["element"] == json!(beam)).unwrap();
    assert_eq!((node(&main["start"])["x"].clone(), node(&main["end"])["x"].clone()), (json!(0.0), json!(6.0)));
    assert!((main["length"].as_f64().unwrap() - 6.0).abs() < 1e-9);
    assert_eq!(m["nodes"].as_array().unwrap().iter().filter(|n| n["support"] == json!(true)).count(), 2);
    let path = scratch("model.analytical.json");
    let out = run(&mut s, "analytical_export", json!({"path": path}));
    assert_eq!(out["members"].as_u64(), Some(members.len() as u64));
    let written: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(written["format"], json!("constructcad-analytical"));
    // An analytical 3D view.
    let view = run(&mut s, "view_create", json!({"kind": "analytical"}));
    assert_eq!(view["params"]["analytical"], json!(true), "{view}");
}

#[test]
fn reinforcement_of_beams_and_columns() {
    let mut s = Session::default();
    // A 0.15 x 0.40 beam, 5 m long, free; a 0.20 x 0.20 column, 3 m tall.
    let beam = id(&run(&mut s, "beam_create", json!({"start": [0, 0], "end": [5, 0], "top_offset": 3.0})));
    let column = id(&run(&mut s, "column_create", json!({"position": [9, 9], "height": 3.0})));
    let out = run(&mut s, "rebar_typical", json!({"elements": [beam, column]}));
    // Beam: bottom, top and stirrups; column: longitudinal and ties.
    assert_eq!(out["created"].as_array().unwrap().len(), 5, "{out}");
    let again = run(&mut s, "rebar_typical", json!({"elements": [beam, column]}));
    assert_eq!(again["skipped"].as_u64(), Some(2));

    let sets: Vec<Value> =
        out["created"].as_array().unwrap().iter().map(|i| run(&mut s, "element_get", json!({"id": i}))).collect();
    let bottom = sets.iter().find(|x| x["params"]["layout"]["Longitudinal"]["face"] == json!("Bottom")).unwrap();
    // Two bars from cover to cover: 5 m - 2 x 0.025.
    assert_eq!(bottom["derived"]["count"], json!(2));
    assert!((bottom["derived"]["bar_length"].as_f64().unwrap() - 4.95).abs() < 1e-9, "{bottom}");
    // 10 mm bars weigh 0.617 kg/m.
    let weight = bottom["derived"]["weight"].as_f64().unwrap();
    assert!((weight - 2.0 * 4.95 * std::f64::consts::PI * 0.01 * 0.01 / 4.0 * 7850.0).abs() < 1e-9);
    // Stirrups every 0.15 over 4.95 m: 33 spaces, 34 stirrups, each around a 0.10 x 0.35 loop.
    let stirrups =
        sets.iter().find(|x| x["params"]["layout"]["Stirrups"].is_object() && x["params"]["host"] == json!(beam));
    let stirrups = stirrups.unwrap();
    assert_eq!(stirrups["derived"]["count"], json!(34));
    assert!((stirrups["derived"]["bar_length"].as_f64().unwrap() - 2.0 * (0.095 + 0.345)).abs() < 1e-9, "{stirrups}");
    // A column's bars run its height.
    let bars =
        sets.iter().find(|x| x["params"]["host"] == json!(column) && x["params"]["layout"]["Longitudinal"].is_object());
    assert!((bars.unwrap()["derived"]["bar_length"].as_f64().unwrap() - 3.0).abs() < 1e-9);

    // Bars go with their host.
    let n = |s: &mut Session| run(s, "elements_list", json!({"category": "rebar"})).as_array().unwrap().len();
    assert_eq!(n(&mut s), 5);
    run(&mut s, "elements_delete", json!({"ids": [beam]}));
    assert_eq!(n(&mut s), 2);
    // Only beams and columns take reinforcement; layouts are checked.
    let wall = id(&run(&mut s, "wall_create", json!({"start": [0, 5], "end": [4, 5]})));
    assert_eq!(fails(&mut s, "rebar_create", json!({"host": wall, "layout": "top"})), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "rebar_create", json!({"host": column, "layout": "zigzag"})), ErrorCode::InvalidArguments);
    let ties = run(&mut s, "rebar_create", json!({"host": column, "layout": "stirrups", "spacing": 0.2}));
    assert_eq!(ties["params"]["layout"], json!({"Stirrups": {"spacing": 0.2}}));
    // The steel schedule.
    run(&mut s, "view_create", json!({"kind": "schedule", "category": "rebar"}));
}

#[test]
fn trusses_braces_numbering_and_structural_plans() {
    let mut s = Session::default();
    // A Pratt truss, 6 m in 4 panels, 1 m deep: 2 chords, 4 diagonals, 5 posts.
    let out = run(
        &mut s,
        "truss_create",
        json!({"start": [0, 0], "end": [6, 0], "height": 1.0, "panels": 4, "bottom_offset": 2.0}),
    );
    assert_eq!(out["created"].as_array().unwrap().len(), 11, "{out}");
    let group = run(&mut s, "groups_list", json!({}));
    assert_eq!(group.as_array().unwrap().len(), 1);
    // Its analytical model: all members meet at the 10 panel points.
    let m = run(&mut s, "analytical_model", json!({}));
    assert_eq!(m["nodes"].as_array().unwrap().len(), 10, "{m}");
    assert_eq!(
        fails(&mut s, "truss_create", json!({"start": [0, 0], "end": [6, 0], "height": 1.0, "pattern": "k"})),
        ErrorCode::InvalidArguments
    );

    // X bracing between two columns: two members from base to top.
    let a = id(&run(&mut s, "column_create", json!({"position": [0, 10], "height": 3.0})));
    let b = id(&run(&mut s, "column_create", json!({"position": [4, 10], "height": 3.0})));
    let out = run(&mut s, "braces_between_columns", json!({"columns": [a, b]}));
    let braces = out["created"].as_array().unwrap();
    assert_eq!(braces.len(), 2);
    let brace = run(&mut s, "element_get", json!({"id": braces[0]}));
    assert_eq!(
        (brace["params"]["top_offset"].clone(), brace["params"]["end_offset"].clone()),
        (json!(0.0), json!(3.0))
    );
    assert_eq!(fails(&mut s, "braces_between_columns", json!({"columns": [a]})), ErrorCode::InvalidArguments);
    assert_eq!(
        fails(&mut s, "brace_create", json!({"start": [1, 1, 0], "end": [1, 1, 3]})),
        ErrorCode::InvalidArguments
    );

    // Numbering in reading order: the upper column first.
    let c = id(&run(&mut s, "column_create", json!({"position": [0, 20], "height": 3.0})));
    let out = run(&mut s, "structure_number", json!({"categories": ["column"]}));
    assert!(out["numbered"].as_u64().unwrap() >= 3);
    let mark = |s: &mut Session, id: u64| run(s, "element_get", json!({"id": id}))["mark"].clone();
    assert_eq!(mark(&mut s, c), json!("P1"));
    assert_eq!((mark(&mut s, a), mark(&mut s, b)), (json!("P2"), json!("P3")));

    // A structural plan: tagged with marks and sections.
    let plan = run(&mut s, "view_create", json!({"kind": "structural_plan"}));
    assert_eq!(plan["params"]["structural"], json!(true), "{plan}");
}

/// The component type of the template whose MEP data names `device` (or a fixture, for `"Fixture"`).
fn mep_type(s: &mut Session, device: &str) -> u64 {
    let types = run(s, "types_list", json!({"category": "component"}));
    types
        .as_array()
        .unwrap()
        .iter()
        .map(id)
        .find(|t| {
            let ty = run(s, "type_get", json!({"type": t}));
            let mep = &ty["params"]["def"]["mep"];
            mep["Device"]["device"] == json!(device) || (device == "Fixture" && mep["Fixture"].is_object())
        })
        .unwrap_or_else(|| panic!("no {device} type"))
}

#[test]
fn building_services_through_commands() {
    let mut s = Session::default();
    // The template's plumbing systems.
    let systems = run(&mut s, "systems_list", json!({}));
    assert_eq!(systems.as_array().unwrap().len(), 5, "{systems}");
    let sewer_code = systems.as_array().unwrap().iter().find(|x| x["kind"] == json!("sanitary")).unwrap()["code"]
        .as_str()
        .unwrap()
        .to_string();
    let pipes = run(&mut s, "types_list", json!({"category": "pipe"}));
    let drain_100 = pipes.as_array().unwrap().iter().map(id).find(|t| {
        (run(&mut s, "type_get", json!({"type": t}))["params"]["diameter"].as_f64().unwrap() - 0.1).abs() < 1e-9
    });

    // A 100 mm drain falls 1 % from under the floor; a riser goes up from its end.
    let drain = run(
        &mut s,
        "pipe_create",
        json!({"start": [0, 0], "end": [5, 0], "system": sewer_code, "type": drain_100.unwrap()}),
    );
    assert_eq!(drain["params"]["start_offset"], json!(-0.3), "{drain}");
    assert!((drain["params"]["end_offset"].as_f64().unwrap() + 0.35).abs() < 1e-9, "{drain}");
    assert!((drain["derived"]["slope"].as_f64().unwrap() - 0.01).abs() < 1e-9);
    let riser = run(
        &mut s,
        "pipe_create",
        json!({"start": [5, 0], "system": sewer_code, "offset": -0.35, "end_offset": 2.8, "type": drain_100.unwrap()}),
    );
    assert!((riser["derived"]["length"].as_f64().unwrap() - 3.15).abs() < 1e-9, "{riser}");
    // A supply pipe is level, in the wall.
    let supply = run(&mut s, "pipe_create", json!({"start": [0, 1], "end": [3, 1], "system": "cold_water"}));
    assert_eq!(
        (supply["params"]["start_offset"].clone(), supply["params"]["end_offset"].clone()),
        (json!(0.6), json!(0.6))
    );
    assert_eq!(fails(&mut s, "pipe_create", json!({"start": [0, 0]})), ErrorCode::InvalidArguments);

    // A board, a lighting circuit on it, a light and two outlets.
    let panel_t = mep_type(&mut s, "Panel");
    let panel = id(&run(&mut s, "component_place", json!({"position": [0, 3], "type": panel_t})));
    let circuit =
        run(&mut s, "system_create", json!({"kind": "lighting", "panel": panel, "voltage": 127.0, "name": "Luz"}));
    assert_eq!(circuit["code"], json!("1"));
    let power = run(&mut s, "system_create", json!({"kind": "power", "panel": panel}));
    assert_eq!(power["code"], json!("2"));
    let light_t = mep_type(&mut s, "CeilingLight");
    let light = run(&mut s, "component_place", json!({"position": [2, 2], "type": light_t, "system": id(&circuit)}));
    // Lights go on the ceiling: the template's levels are 3 m apart.
    assert_eq!(light["params"]["base_offset"], json!(3.0), "{light}");
    let outlet_t = mep_type(&mut s, "Outlet");
    for x in [1.0, 3.0] {
        let o = run(&mut s, "component_place", json!({"position": [x, 0], "type": outlet_t, "system": id(&power)}));
        assert_eq!(o["params"]["base_offset"], json!(0.3));
    }
    // Circuits take devices and conduits, not pipes.
    assert_eq!(
        fails(&mut s, "system_assign", json!({"elements": [id(&drain)], "system": id(&power)})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(&mut s, "component_place", json!({"position": [1, 1], "type": outlet_t, "system": sewer_code})),
        ErrorCode::InvalidArguments
    );
    let conduit = run(&mut s, "conduit_create", json!({"start": [0, 3], "end": [2, 2], "circuits": ["1", "2"]}));
    assert_eq!(conduit["params"]["systems"].as_array().unwrap().len(), 2);
    assert_eq!(conduit["params"]["start_offset"], json!(3.0));
    assert_eq!(
        fails(&mut s, "conduit_create", json!({"start": [0, 3], "end": [2, 2], "circuits": [sewer_code]})),
        ErrorCode::InvalidArguments
    );

    // The load schedule: 100 VA of lighting, 2 x 100 VA of outlets at 127 V.
    let loads = run(&mut s, "circuit_loads", json!({"panel": panel}));
    let rows = loads["circuits"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[0]["power_va"].clone(), rows[1]["power_va"].clone()), (json!(100.0), json!(200.0)));
    assert!((rows[1]["current_a"].as_f64().unwrap() - 200.0 / 127.0).abs() < 1e-9);
    assert_eq!(loads["panels"][0]["power_va"], json!(300.0));

    // Fixture units add up in their system.
    let fixture_t = mep_type(&mut s, "Fixture");
    let wc = id(&run(&mut s, "component_place", json!({"position": [4, 4], "type": fixture_t})));
    run(&mut s, "system_assign", json!({"elements": [wc], "system": sewer_code}));
    let systems = run(&mut s, "systems_list", json!({}));
    let sewer = systems.as_array().unwrap().iter().find(|x| x["code"] == json!(sewer_code)).unwrap().clone();
    assert_eq!(sewer["elements"], json!(3), "{sewer}");
    assert!(sewer["drain_units"].as_f64().unwrap() > 0.0);
    assert!((sewer["length"].as_f64().unwrap() - (5.0f64.hypot(0.05) + 3.15)).abs() < 1e-9);

    // Changing and deleting systems; members stay, outside it.
    let set = run(&mut s, "system_set", json!({"system": "2", "params": {"name": "Tomadas", "voltage": 220.0}}));
    assert_eq!((set["name"].clone(), set["voltage"].clone()), (json!("Tomadas"), json!(220.0)));
    assert_eq!(
        fails(&mut s, "system_set", json!({"system": "2", "params": {"kind": "cold_water"}})),
        ErrorCode::Rejected
    );
    run(&mut s, "system_delete", json!({"system": "Tomadas"}));
    let after = run(&mut s, "element_get", json!({"id": id(&conduit)}));
    assert_eq!(after["params"]["systems"].as_array().unwrap().len(), 1, "{after}");
    run(&mut s, "undo", json!({}));
    assert_eq!(run(&mut s, "systems_list", json!({})).as_array().unwrap().len(), 7);

    // Deleting the board leaves its circuits unfed.
    run(&mut s, "elements_delete", json!({"ids": [panel]}));
    let loads = run(&mut s, "circuit_loads", json!({}));
    assert!(loads["circuits"].as_array().unwrap().iter().all(|c| c["panel"].is_null()), "{loads}");

    // Services plans and schedules; IFC export with the systems.
    let plan = run(&mut s, "view_create", json!({"kind": "plumbing_plan"}));
    assert_eq!(plan["params"]["mep"], json!("Plumbing"), "{plan}");
    run(&mut s, "view_create", json!({"kind": "electrical_plan"}));
    let table = run(&mut s, "view_create", json!({"kind": "schedule", "category": "pipe"}));
    let rows = run(&mut s, "schedule_table", json!({"view": id(&table)}));
    assert_eq!(rows["rows"].as_array().unwrap().len(), 3, "{rows}");
    let path = scratch("mep.ifc");
    run(&mut s, "ifc_export", json!({"path": path.to_string_lossy()}));
    let ifc = std::fs::read_to_string(&path).unwrap();
    for entity in [
        "IFCPIPESEGMENT(",
        "IFCCABLECARRIERSEGMENT(",
        "IFCDISTRIBUTIONSYSTEM(",
        "IFCDISTRIBUTIONCIRCUIT(",
        "IFCOUTLET(",
        "IFCLIGHTFIXTURE(",
        "IFCRELASSIGNSTOGROUP(",
    ] {
        assert!(ifc.contains(entity), "{entity} missing");
    }
}

#[test]
fn fixtures_and_devices_connect_to_the_networks() {
    let mut s = Session::default();
    // A drain main under the floor and a cold water pipe in the wall, along y = 0.
    let drain_100 = run(&mut s, "types_list", json!({"category": "pipe"}))
        .as_array()
        .unwrap()
        .iter()
        .map(id)
        .find(|t| {
            (run(&mut s, "type_get", json!({"type": t}))["params"]["diameter"].as_f64().unwrap() - 0.1).abs() < 1e-9
        })
        .unwrap();
    run(&mut s, "pipe_create", json!({"start": [0, 0], "end": [6, 0], "system": "sanitary", "type": drain_100}));
    run(&mut s, "pipe_create", json!({"start": [0, 0.1], "end": [6, 0.1], "system": "cold_water"}));
    let toilet = mep_type(&mut s, "Fixture");
    let wc = id(&run(&mut s, "component_place", json!({"position": [3, 1.5], "type": toilet})));
    let out = run(&mut s, "mep_connect", json!({"elements": [wc]}));
    // The toilet's connectors: the sewer outlet 30 cm from the wall on the
    // floor (a drop and a branch falling to the main), and the cold water
    // inlet beside the tank, 20 cm up (across at that height, then down onto
    // the pipe).
    let created = out["created"].as_array().unwrap();
    assert_eq!(created.len(), 4, "{out}");
    let runs: Vec<Value> = created.iter().map(|r| run(&mut s, "element_get", json!({"id": r}))).collect();
    let start = |r: &Value| (r["params"]["start"][0].as_f64().unwrap(), r["params"]["start"][1].as_f64().unwrap());
    let near = |(x, y): (f64, f64), (u, v): (f64, f64)| (x - u).abs() < 1e-9 && (y - v).abs() < 1e-9;
    assert!(near(start(&runs[0]), (3.0, 1.525)) && runs[0]["params"]["start_offset"] == json!(0.0), "{}", runs[0]);
    assert!(near(start(&runs[2]), (2.85, 1.825)) && runs[2]["params"]["start_offset"] == json!(0.2), "{}", runs[2]);
    assert_eq!(runs[2]["params"]["end_offset"], json!(0.2), "across at the inlet's height");
    let fittings = run(&mut s, "mep_fittings", json!({}));
    let kinds: Vec<&str> =
        fittings["fittings"].as_array().unwrap().iter().map(|f| f["kind"].as_str().unwrap()).collect();
    // Tees on both mains and the elbow at the bottom of the drop.
    assert_eq!(kinds.iter().filter(|k| **k == "tee").count(), 2, "{fittings}");
    assert!(kinds.contains(&"elbow"), "{fittings}");
    // The toilet's branch is 100 mm: no reducer at the drain tee.
    assert!(!kinds.contains(&"reducer"), "{fittings}");

    // A device drops from its height to the conduit in the slab, on its circuit.
    let panel_t = mep_type(&mut s, "Panel");
    run(&mut s, "component_place", json!({"position": [0, 3], "type": panel_t}));
    let circuit = id(&run(&mut s, "system_create", json!({"kind": "power"})));
    run(&mut s, "conduit_create", json!({"start": [0, 3], "end": [6, 3], "circuits": [circuit]}));
    let outlet_t = mep_type(&mut s, "Outlet");
    let outlet = id(&run(&mut s, "component_place", json!({"position": [4, 4], "type": outlet_t, "system": circuit})));
    let out = run(&mut s, "mep_connect", json!({"elements": [outlet]}));
    let created = out["created"].as_array().unwrap();
    assert_eq!(created.len(), 2, "{out}");
    let drop = run(&mut s, "element_get", json!({"id": created[0]}));
    assert_eq!(
        (drop["params"]["start_offset"].clone(), drop["params"]["end_offset"].clone()),
        (json!(0.3), json!(3.0))
    );
    assert_eq!(drop["params"]["systems"], json!([circuit]));
    // Walls are not fixtures.
    let wall = id(&run(&mut s, "wall_create", json!({"start": [0, 8], "end": [4, 8]})));
    assert_eq!(fails(&mut s, "mep_connect", json!({"elements": [wall]})), ErrorCode::InvalidArguments);
}

#[test]
fn components_from_profiles_revolutions_and_voids() {
    let mut s = Session::default();
    // A countertop basin: a slab drawn from the front with a rounded front
    // edge (seen from the side), and a round bowl turned about the vertical
    // axis, cut out of it.
    let def = json!({
        "kind": "Plumbing",
        "params": [
            {"name": "W", "value": 0.6, "kind": "Length"},
            {"name": "T", "value": 0.2, "kind": "Length"},
            {"name": "R", "value": 0.15, "kind": "Length"}
        ],
        "items": [
            {"name": "slab", "material": "", "shape": {"Profile": {
                "plane": "Front",
                "points": [{"u": "-W / 2", "v": "0"}, {"u": "W / 2", "v": "0"}, {"u": "W / 2", "v": "T"}, {"u": "-W / 2", "v": "T"}],
                "offset": "-0.25", "depth": "0.5"}}},
            {"name": "bowl", "void": true, "shape": {"Revolution": {
                "plane": "Front",
                "points": [{"u": "0", "v": "0.05"}, {"u": "R", "v": "0.05"}, {"u": "R", "v": "T + 0.1"}, {"u": "0", "v": "T + 0.1"}],
                "offset": "0", "axis": "0", "angle": "360"}}}
        ]
    });
    let ty = run(&mut s, "component_create", json!({"name": "Bancada com cuba", "def": def}));
    let shape = &ty["params"]["def"]["items"][1];
    assert_eq!(shape["void"], json!(true), "{ty}");
    assert_eq!(shape["shape"]["Revolution"]["plane"], json!("Front"));
    let basin = id(&run(&mut s, "component_place", json!({"position": [2, 2], "type": id(&ty)})));
    // 0.6 x 0.2 x 0.5 less a 48-sided bowl of radius 0.15, 0.15 deep.
    let volume = run(&mut s, "element_get", json!({"id": basin}))["derived"]["volume"].as_f64().unwrap();
    let bowl = 0.5 * 48.0 * 0.15f64.powi(2) * (std::f64::consts::TAU / 48.0).sin() * 0.15;
    assert!((volume - (0.06 - bowl)).abs() < 1e-6, "{volume}");

    // An arc on the side plane: a quarter round nosing of radius T; a
    // profile crossing its revolution axis is refused.
    let nosing = json!({"params": [{"name": "T", "value": 0.2, "kind": "Length"}], "items": [{"name": "n", "shape": {"Profile": {
        "plane": "Side",
        "points": [{"u": "0", "v": "0"}, {"u": "T", "v": "0", "radius": "T"}, {"u": "0", "v": "T"}],
        "offset": "0", "depth": "1"}}}]});
    let nosing = id(&run(&mut s, "component_create", json!({"name": "Boleado", "def": nosing})));
    let placed = id(&run(&mut s, "component_place", json!({"position": [5, 5], "type": nosing})));
    let volume = run(&mut s, "element_get", json!({"id": placed}))["derived"]["volume"].as_f64().unwrap();
    let quarter = std::f64::consts::PI * 0.04 / 4.0;
    assert!(volume < quarter && volume > quarter * 0.99, "{volume}");
    let crossing = json!({"items": [{"name": "x", "shape": {"Revolution": {
        "plane": "Front", "points": [{"u": "-1", "v": "0"}, {"u": "1", "v": "0"}, {"u": "1", "v": "1"}],
        "offset": "0", "axis": "0", "angle": "360"}}}]});
    assert_eq!(
        fails(&mut s, "component_create", json!({"name": "Errado", "def": crossing})),
        ErrorCode::InvalidArguments
    );
}

#[test]
fn runs_connect_through_risers_between_floors() {
    let mut s = Session::default();
    let first = run(&mut s, "levels_list", json!({}))[1]["id"].as_u64().unwrap();
    let elevation = |s: &mut Session, out: &Value| {
        let p = &run(s, "element_get", json!({"id": id(out)}))["params"];
        (p["start_offset"].as_f64().unwrap(), p["end_offset"].as_f64().unwrap())
    };
    // A cold water run on the ground floor; a riser from its end takes its elevation
    // and goes up to 0.6 m above the first floor (3 m).
    run(&mut s, "pipe_create", json!({"start": [0, 0], "end": [4, 0], "system": "cold_water", "offset": 0.4}));
    let riser = run(&mut s, "pipe_create", json!({"start": [4, 0], "system": "cold_water", "end_offset": 3.6}));
    assert_eq!(elevation(&mut s, &riser), (0.4, 3.6));
    // Upstairs, a run from the riser starts at its top, seen from the first floor.
    let upper =
        run(&mut s, "pipe_create", json!({"start": [4, 0], "end": [4, 3], "system": "cold_water", "level": first}));
    assert!((elevation(&mut s, &upper).0 - 0.6).abs() < 1e-9, "{upper}");
    let fittings = run(&mut s, "mep_fittings", json!({"level": first}));
    assert_eq!(fittings["fittings"][0]["kind"], json!("elbow"), "{fittings}");
    assert_eq!(run(&mut s, "mep_fittings", json!({}))["total"], json!(2));

    // A drain stack from under the ground floor to the first floor; a fixture
    // upstairs connects to it, and a conduit drop leaves a run at its end.
    run(&mut s, "pipe_create", json!({"start": [6, 0], "system": "sanitary", "offset": -0.3, "end_offset": 3.5}));
    let fixture_t = mep_type(&mut s, "Fixture");
    let wc = id(&run(&mut s, "component_place", json!({"position": [6, 1.5], "type": fixture_t, "level": first})));
    let out = run(&mut s, "mep_connect", json!({"elements": [wc], "system": "sanitary"}));
    assert_eq!(out["skipped"], json!(0), "{out}");
    let branch = out["created"].as_array().unwrap().last().unwrap().clone();
    let branch = run(&mut s, "element_get", json!({"id": branch}));
    assert_eq!(branch["params"]["end"], json!([6.0, 0.0]), "{branch}");
    let fittings = run(&mut s, "mep_fittings", json!({"level": first}));
    assert!(fittings["fittings"].as_array().unwrap().iter().any(|f| f["kind"] == json!("tee")), "{fittings}");

    run(&mut s, "conduit_create", json!({"start": [0, 5], "end": [3, 5], "offset": 2.9}));
    let drop = run(&mut s, "conduit_create", json!({"start": [3, 5], "end_offset": 0.3}));
    assert_eq!(elevation(&mut s, &drop), (2.9, 0.3));
}

#[test]
fn sizing_drains_water_pipes_and_circuits() {
    let mut s = Session::default();
    let pipe_types: Vec<(u64, f64)> = run(&mut s, "types_list", json!({"category": "pipe"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|t| (id(t), run(&mut s, "type_get", json!({"type": id(t)}))["params"]["diameter"].as_f64().unwrap()))
        .collect();
    let of = |d: f64| pipe_types.iter().find(|t| (t.1 - d).abs() < 1e-9).unwrap().0;
    // A 40 mm drain and a 20 mm cold water main, both too small for what they will carry.
    let drain = id(&run(
        &mut s,
        "pipe_create",
        json!({"start": [0, 0], "end": [8, 0], "system": "sanitary", "type": of(0.04)}),
    ));
    let water = id(&run(
        &mut s,
        "pipe_create",
        json!({"start": [0, 0.1], "end": [8, 0.1], "system": "cold_water", "type": of(0.02)}),
    ));
    let toilet = mep_type(&mut s, "Fixture");
    let mut fixtures = Vec::new();
    for x in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0] {
        fixtures.push(id(&run(&mut s, "component_place", json!({"position": [x, 1.5], "type": toilet}))));
    }
    run(&mut s, "mep_connect", json!({"elements": fixtures}));
    let sizing = run(&mut s, "mep_sizing", json!({}));
    let pipe = |id: u64| sizing["pipes"].as_array().unwrap().iter().find(|p| p["id"] == json!(id)).unwrap().clone();
    // Seven toilets: 42 fixture units at the outlet, 100 mm.
    let main = pipe(drain);
    assert_eq!(main["load"], json!(42.0), "{main}");
    assert_eq!((main["required_mm"].clone(), main["ok"].clone()), (json!(100.0), json!(false)));
    // Their weights add up to 2.1: 0.43 l/s, still within 20 mm at the source.
    let supply = pipe(water);
    assert!((supply["load"].as_f64().unwrap() - 2.1).abs() < 1e-9, "{supply}");
    assert!((supply["flow_l_s"].as_f64().unwrap() - 0.3 * 2.1f64.sqrt()).abs() < 1e-9);
    assert_eq!(supply["required_mm"], json!(20.0));
    // Applying widens the drain main; the water main stays.
    let applied = run(&mut s, "mep_size_apply", json!({}));
    assert!(applied["changed"].as_array().unwrap().contains(&json!(drain)), "{applied}");
    assert_eq!(run(&mut s, "element_get", json!({"id": drain}))["type"]["id"], json!(of(0.1)));
    assert_eq!(run(&mut s, "element_get", json!({"id": water}))["type"]["id"], json!(of(0.02)));

    // A 3000 VA circuit 51 m from its board (no conduits: measured straight).
    let panel_t = mep_type(&mut s, "Panel");
    run(&mut s, "component_place", json!({"position": [0, 0], "type": panel_t, "base_offset": 1.3}));
    let circuit = id(&run(&mut s, "system_create", json!({"kind": "power"})));
    let outlet = mep_type(&mut s, "DedicatedOutlet");
    for _ in 0..2 {
        run(&mut s, "component_place", json!({"position": [40, 10], "type": outlet, "system": circuit}));
    }
    let sizing = run(&mut s, "mep_sizing", json!({}));
    let c = sizing["circuits"].as_array().unwrap().iter().find(|c| c["id"] == json!(circuit)).unwrap().clone();
    // 23.6 A: a 25 A breaker; 4 mm² would carry it but drops 8.5 %, 10 mm² drops 3.4 %.
    assert_eq!((c["breaker_a"].clone(), c["conductor_mm2"].clone()), (json!(25.0), json!(10.0)), "{c}");
    assert!((c["length_m"].as_f64().unwrap() - 51.0).abs() < 1e-9);
    assert!(c["voltage_drop_pct"].as_f64().unwrap() < 4.0 && c["ok"] == json!(true));
}

#[test]
fn panel_diagrams_follow_their_board() {
    let mut s = Session::default();
    assert_eq!(fails(&mut s, "view_create", json!({"kind": "panel_diagram"})), ErrorCode::InvalidArguments);
    let panel_t = mep_type(&mut s, "Panel");
    let panel = id(&run(&mut s, "component_place", json!({"position": [0, 0], "type": panel_t})));
    run(&mut s, "system_create", json!({"kind": "lighting"}));
    let diagram = run(&mut s, "view_create", json!({"kind": "panel_diagram"}));
    assert_eq!(diagram["params"]["panel"], json!(panel), "{diagram}");
    // It goes on a sheet like any drawing.
    let sheet = id(&run(&mut s, "view_create", json!({"kind": "sheet"})));
    run(&mut s, "sheet_place_view", json!({"sheet": sheet, "view": id(&diagram)}));
    // Deleting the board deletes its diagram (and the viewport).
    run(&mut s, "elements_delete", json!({"ids": [panel]}));
    assert_eq!(fails(&mut s, "view_get", json!({"view": id(&diagram)})), ErrorCode::NotFound);
    run(&mut s, "undo", json!({}));
    assert_eq!(run(&mut s, "view_get", json!({"view": id(&diagram)}))["params"]["panel"], json!(panel));
}

/// Arc walls: three points or a bulge; quantities along the arc; doors on
/// the arc; a room and floor from walls that include an arc; edits.
#[test]
fn arc_walls_through_commands() {
    use std::f64::consts::PI;
    let mut s = Session::default();
    // A half circle through (0, 4) closed by a straight wall: a "D".
    let arc = run(&mut s, "wall_create", json!({"start": [4.0, 0.0], "end": [-4.0, 0.0], "mid": [0.0, 4.0]}));
    let thickness = arc["derived"]["thickness"].as_f64().unwrap();
    assert!((arc["params"]["bulge"].as_f64().unwrap() - 1.0).abs() < 1e-9, "{arc}");
    assert!((arc["derived"]["radius"].as_f64().unwrap() - 4.0).abs() < 1e-9);
    assert!((arc["derived"]["sweep"].as_f64().unwrap() - 180.0).abs() < 1e-9);
    assert!((arc["derived"]["length"].as_f64().unwrap() - 4.0 * PI).abs() < 1e-9);
    let height = arc["derived"]["height"].as_f64().unwrap();
    let volume = arc["derived"]["volume"].as_f64().unwrap();
    assert!((volume - 4.0 * PI * thickness * height).abs() < 1e-9, "a free arc: length x thickness x height");
    run(&mut s, "wall_create", json!({"start": [-4.0, 0.0], "end": [4.0, 0.0]}));

    // A door by a point near the top of the arc lands there, centered on the arc.
    let door = run(&mut s, "door_create", json!({"point": [0.3, 4.5]}));
    let offset = door["params"]["offset"].as_f64().unwrap();
    let expected = (PI / 2.0 - (0.3f64 / 4.5).atan()) * 4.0;
    assert!((offset - expected).abs() < 1e-6, "{offset} vs {expected}");
    assert_eq!(door["params"]["host"], arc["id"]);
    assert_eq!(door["params"]["flip_facing"], true, "outside the arc is its right side");

    // The room inside follows the inner face of the arc.
    let room = run(&mut s, "room_create", json!({"point": [0.0, 1.0]}));
    let (r, d) = (4.0 - thickness / 2.0, thickness / 2.0);
    let segment = r * r * (d / r).acos() - d * (r * r - d * d).sqrt();
    let area = room["derived"]["area"].as_f64().unwrap();
    assert!((area - segment).abs() < 1e-3 * segment, "{area} vs {segment}");
    // A floor from the walls follows the arc too (its exterior faces).
    let floor = run(&mut s, "floor_create", json!({"from_walls": true}));
    let outer = PI * (4.0 + d) * (4.0 + d) / 2.0;
    let floor_el = run(&mut s, "element_get", json!({"id": id(&floor)}));
    let boundary = floor_el["params"]["boundary"].as_array().unwrap();
    assert!(boundary.len() > 30, "the arc is followed by many points");
    let pts: Vec<(f64, f64)> = boundary.iter().map(|p| (p[0].as_f64().unwrap(), p[1].as_f64().unwrap())).collect();
    let n = pts.len();
    let shoelace: f64 =
        (0..n).map(|i| pts[i].0 * pts[(i + 1) % n].1 - pts[(i + 1) % n].0 * pts[i].1).sum::<f64>() / 2.0;
    let expected_floor = outer + 2.0 * (4.0 + d) * d;
    assert!((shoelace.abs() - expected_floor).abs() < 5e-3 * expected_floor, "{} vs {expected_floor}", shoelace.abs());

    // Bulge directly, and a clockwise arc; set back to straight with element_set.
    let cw = run(&mut s, "wall_create", json!({"start": [10.0, 0.0], "end": [14.0, 0.0], "bulge": -1.0}));
    assert!((cw["derived"]["sweep"].as_f64().unwrap() + 180.0).abs() < 1e-9);
    let straight = run(&mut s, "element_set", json!({"id": id(&cw), "params": {"bulge": 0.0}}));
    assert!((straight["derived"]["length"].as_f64().unwrap() - 4.0).abs() < 1e-9, "{straight}");
    assert!(straight["derived"]["radius"].is_null());

    // A radius: the shorter arc (or the longer), its sign the turn.
    let r = run(&mut s, "wall_create", json!({"start": [20.0, 0.0], "end": [24.0, 0.0], "radius": -2.0}));
    assert!((r["derived"]["sweep"].as_f64().unwrap() + 180.0).abs() < 1e-9, "{r}");
    let r = run(&mut s, "wall_create", json!({"start": [20.0, 5.0], "end": [24.0, 5.0], "radius": 4.0}));
    assert!((r["derived"]["sweep"].as_f64().unwrap() - 60.0).abs() < 1e-9, "{r}");
    assert!((r["derived"]["radius"].as_f64().unwrap() - 4.0).abs() < 1e-9, "{r}");
    let major = json!({"start": [20.0, 10.0], "end": [24.0, 10.0], "radius": 4.0, "major_arc": true});
    let r = run(&mut s, "wall_create", major);
    assert!((r["derived"]["sweep"].as_f64().unwrap() - 300.0).abs() < 1e-9, "{r}");
    let short = json!({"start": [20.0, 15.0], "end": [24.0, 15.0], "radius": 1.5});
    assert_eq!(fails(&mut s, "wall_create", short), ErrorCode::InvalidArguments);

    // Bad arcs are refused: collinear points, a bulge out of range, an arc too tight for the wall.
    let bad = json!({"start": [0.0, 10.0], "end": [4.0, 10.0], "mid": [2.0, 10.0]});
    assert_eq!(fails(&mut s, "wall_create", bad), ErrorCode::InvalidArguments);
    let bad = json!({"start": [0.0, 10.0], "end": [4.0, 10.0], "bulge": 50.0});
    assert_eq!(fails(&mut s, "wall_create", bad), ErrorCode::InvalidArguments);
    let tight = json!({"start": [0.0, 10.0], "end": [0.1, 10.0], "bulge": 1.0});
    assert_eq!(fails(&mut s, "wall_create", tight), ErrorCode::Rejected);

    // Split on the arc: two quarter circles.
    let split = run(&mut s, "element_split", json!({"id": id(&arc), "point": [0.0, 3.0]}));
    let parts = [split["original"].as_u64().unwrap(), split["created"].as_u64().unwrap()];
    for part in parts {
        let w = run(&mut s, "element_get", json!({"id": part}));
        assert!((w["derived"]["sweep"].as_f64().unwrap() - 90.0).abs() < 1e-9, "{w}");
    }

    // Away from the "D": an arc wall trims to a line along its circle, a line
    // extends to an arc's circle, and an arc aligns onto a concentric one.
    let quarter =
        run(&mut s, "wall_create", json!({"start": [24.0, 0.0], "end": [20.0, 4.0], "bulge": (PI / 8.0).tan()}));
    let post = run(&mut s, "wall_create", json!({"start": [22.0, -3.0], "end": [22.0, -1.0]}));
    let point = |w: &Value, end: &str| (w["params"][end][0].as_f64().unwrap(), w["params"][end][1].as_f64().unwrap());
    let near = |(x, y): (f64, f64), (u, v): (f64, f64)| (x - u).abs() < 1e-9 && (y - v).abs() < 1e-9;
    let r12 = 12f64.sqrt();
    let trim = json!({"id": id(&quarter), "pick": [20.0, 4.0], "reference": id(&post)});
    let trimmed = run(&mut s, "trim_extend", trim);
    assert!(near(point(&trimmed, "start"), (22.0, r12)) && near(point(&trimmed, "end"), (20.0, 4.0)), "{trimmed}");
    assert!((trimmed["derived"]["radius"].as_f64().unwrap() - 4.0).abs() < 1e-9);
    let extend = json!({"id": id(&post), "pick": [22.0, -2.0], "reference": id(&quarter)});
    let extended = run(&mut s, "trim_extend", extend);
    assert!(near(point(&extended, "end"), (22.0, r12)), "{extended}");
    let outer = run(&mut s, "wall_create", json!({"start": [26.0, 0.0], "end": [14.0, 0.0], "mid": [20.0, 6.0]}));
    let aligned = run(&mut s, "element_align", json!({"id": id(&quarter), "reference": id(&outer)}));
    assert!((aligned["derived"]["radius"].as_f64().unwrap() - 6.0).abs() < 1e-9, "{aligned}");
    let far = run(&mut s, "wall_create", json!({"start": [30.0, -1.0], "end": [30.0, 1.0]}));
    let corner = json!({"a": id(&far), "pick_a": [30.0, 0.0], "b": id(&outer), "pick_b": [20.0, 6.0]});
    assert_eq!(fails(&mut s, "trim_corner", corner), ErrorCode::Rejected, "x = 30 misses the circle of radius 6");
    // Rotating and mirroring keep the arc and its door.
    run(&mut s, "elements_rotate", json!({"ids": parts, "center": [0.0, 0.0], "angle": 90.0}));
    run(&mut s, "elements_mirror", json!({"ids": parts, "axis": [[0.0, 0.0], [0.0, 1.0]]}));
    for part in parts {
        let w = run(&mut s, "element_get", json!({"id": part}));
        assert!((w["derived"]["radius"].as_f64().unwrap() - 4.0).abs() < 1e-9, "{w}");
    }
    let door = run(&mut s, "element_get", json!({"id": id(&door)}));
    let host = run(&mut s, "element_get", json!({"id": door["params"]["host"]}));
    assert!(host["derived"]["radius"].is_number());
}

/// Align by faces, as picked (#86): a wall's face goes onto another wall's
/// face, not its location line onto the other's; a wall at an angle turns
/// until parallel, then moves.
#[test]
fn align_walls_by_the_faces_picked() {
    let mut s = Session::default();
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let a = id(&run(&mut s, "wall_create", json!({"start": [0.0, 0.0], "end": [5.0, 0.0]})));
    let half = run(&mut s, "element_get", json!({"id": a}))["derived"]["thickness"].as_f64().unwrap() / 2.0;
    let b = id(&run(&mut s, "wall_create", json!({"start": [0.0, 3.0], "end": [5.0, 3.0]})));
    // B's south face onto A's north face: B's line ends a whole thickness from A's.
    let args = json!({"id": b, "reference": a, "pick": [2.0, 3.0 - half], "reference_pick": [2.0, half]});
    run(&mut s, "element_align", args);
    let w = run(&mut s, "element_get", json!({"id": b}));
    assert!(near(w["params"]["start"][1].as_f64().unwrap(), 2.0 * half), "{w}");
    assert!(near(w["params"]["end"][1].as_f64().unwrap(), 2.0 * half), "{w}");

    // A wall at an angle turns about the point picked, then its face lands on A's south face.
    let c = id(&run(&mut s, "wall_create", json!({"start": [0.0, -6.0], "end": [4.0, -5.0]})));
    let dir = glam::DVec2::new(4.0, 1.0).normalize();
    let north = dir.perp() * half;
    let pick = glam::DVec2::new(2.0, -5.5) + north;
    run(
        &mut s,
        "element_align",
        json!({"id": c, "reference": a, "pick": pick.to_array(), "reference_pick": [2.0, -half]}),
    );
    let w = run(&mut s, "element_get", json!({"id": c}));
    let (y0, y1) = (w["params"]["start"][1].as_f64().unwrap(), w["params"]["end"][1].as_f64().unwrap());
    assert!(near(y0, y1), "parallel now: {w}");
    assert!(near(y0, -2.0 * half), "its north face on A's south face: {w}");
}

/// Align works in plan, whatever the levels: a column of the upper floor
/// moves onto the faces of the column below by the outline edges picked,
/// onto a grid by its center, and a wall onto a column face.
#[test]
fn align_columns_across_levels() {
    let mut s = Session::default();
    let position = |s: &mut Session, id: u64| {
        let e = run(s, "element_get", json!({"id": id}));
        let p = &e["params"]["position"];
        (p[0].as_f64().unwrap(), p[1].as_f64().unwrap(), e["level"]["id"].as_u64().unwrap())
    };
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    // 20 x 20 cm columns: one on the ground floor, one above it, off by (13, 7) cm.
    let below = id(&run(&mut s, "column_create", json!({"position": [0.0, 0.0]})));
    let above = id(&run(&mut s, "column_create", json!({"position": [0.13, 0.07], "level": 2})));
    let (_, _, level) = position(&mut s, above);
    assert_eq!(level, 2);

    // East face onto east face, then north face onto north face: centered over the column below.
    let depth = s.doc.undo_depth();
    let east = json!({"id": above, "reference": below, "pick": [0.3, 0.07], "reference_pick": [0.2, 0.0]});
    run(&mut s, "element_align", east);
    assert_eq!(s.doc.undo_depth(), depth + 1);
    let (x, y, level) = position(&mut s, above);
    assert!(near(x, 0.0) && near(y, 0.07) && level == 2, "({x}, {y}) on {level}");
    let north = json!({"id": above, "reference": below, "pick": [0.0, 0.25], "reference_pick": [0.0, 0.2]});
    run(&mut s, "element_align", north);
    let (x, y, _) = position(&mut s, above);
    assert!(near(x, 0.0) && near(y, 0.0), "({x}, {y})");

    // A column reference needs the edge picked.
    let bare = json!({"id": above, "reference": below});
    assert_eq!(fails(&mut s, "element_align", bare), ErrorCode::InvalidArguments);
    // Without a pick, the column's center goes onto a grid line.
    let grid = id(&run(&mut s, "grid_create", json!({"start": [2.0, -5.0], "end": [2.0, 5.0]})));
    run(&mut s, "element_align", json!({"id": above, "reference": grid}));
    let (x, y, _) = position(&mut s, above);
    assert!(near(x, 2.0) && near(y, 0.0), "({x}, {y})");

    // A turned column turns until the side picked is parallel to the face, then moves onto it (#86).
    run(&mut s, "element_set", json!({"id": above, "params": {"rotation_deg": 30.0}}));
    let side = [2.0 + 0.15 * 30f64.to_radians().cos(), 0.15 * 30f64.to_radians().sin()];
    let turned = json!({"id": above, "reference": below, "pick": side, "reference_pick": [0.2, 0.0]});
    run(&mut s, "element_align", turned);
    let (x, _, _) = position(&mut s, above);
    let rotation = run(&mut s, "element_get", json!({"id": above}))["params"]["rotation_deg"].as_f64().unwrap();
    assert!(near(rotation.rem_euclid(90.0).min(90.0 - rotation.rem_euclid(90.0)), 0.0), "{rotation}");
    assert!(near(x, 0.0), "its east side on the east face: {x}");

    // A wall moves its line onto the column's west face.
    let wall = id(&run(&mut s, "wall_create", json!({"start": [-3.0, -1.0], "end": [-3.0, 1.0]})));
    run(&mut s, "element_align", json!({"id": wall, "reference": below, "reference_pick": [-0.2, 0.0]}));
    let w = run(&mut s, "element_get", json!({"id": wall}));
    assert!(near(w["params"]["start"][0].as_f64().unwrap(), -0.1), "{w}");
    assert!(near(w["params"]["end"][0].as_f64().unwrap(), -0.1), "{w}");
}

/// Pitched roofs over concave outlines and around courtyards: analytic
/// heights, a valley, exact volumes; bad openings are refused.
#[test]
fn roofs_over_any_footprint() {
    let mut s = Session::default();
    let thickness = |s: &mut Session, roof: &Value| {
        let t = run(s, "type_get", json!({"type": roof["type"]["id"]}));
        t["params"]["layers"].as_array().unwrap().iter().map(|l| l["thickness"].as_f64().unwrap()).sum::<f64>()
    };
    // An L: arms 4 m wide, a hip roof at 50 %.
    let l = json!([[0, 0], [10, 0], [10, 4], [4, 4], [4, 10], [0, 10]]);
    let roof = run(&mut s, "roof_create", json!({"boundary": l, "offset": 3.0, "overhang": 0.0}));
    let roof = run(&mut s, "element_set", json!({"id": id(&roof), "params": {"shape": {"Hip": {"slope": 0.5}}}}));
    let ridges = roof["derived"]["ridges"].as_array().unwrap();
    let valley = ridges.iter().any(|r| {
        let (a, b) = (
            (r[0][0].as_f64().unwrap(), r[0][1].as_f64().unwrap()),
            (r[1][0].as_f64().unwrap(), r[1][1].as_f64().unwrap()),
        );
        let at = |p: (f64, f64), q: (f64, f64)| (p.0 - q.0).abs() < 1e-9 && (p.1 - q.1).abs() < 1e-9;
        (at(a, (4.0, 4.0)) && at(b, (2.0, 2.0))) || (at(b, (4.0, 4.0)) && at(a, (2.0, 2.0)))
    });
    assert!(valley, "a valley from the inner corner: {ridges:?}");
    let t = thickness(&mut s, &roof);
    let volume = roof["derived"]["volume"].as_f64().unwrap();
    let expected = 64.0 * t * (1.0f64 + 0.25).sqrt();
    assert!((volume - expected).abs() < 1e-6, "{volume} vs {expected}");

    // A courtyard: a 12 m square around a 4 m one, with no overhang.
    let outer = json!([[20, 0], [32, 0], [32, 12], [20, 12]]);
    let hole = json!([[24, 4], [28, 4], [28, 8], [24, 8]]);
    let roof = run(&mut s, "roof_create", json!({"boundary": outer, "holes": [hole], "offset": 3.0, "overhang": 0.0}));
    assert!((roof["derived"]["plan_area"].as_f64().unwrap() - 128.0).abs() < 1e-9);
    let flat_volume = roof["derived"]["volume"].as_f64().unwrap();
    assert!((flat_volume - 128.0 * t).abs() < 1e-6, "{flat_volume}");
    let roof = run(&mut s, "element_set", json!({"id": id(&roof), "params": {"shape": {"Hip": {"slope": 1.0}}}}));
    let volume = roof["derived"]["volume"].as_f64().unwrap();
    assert!((volume - 128.0 * t * 2f64.sqrt()).abs() < 1e-6, "{volume}");
    // The ridge runs 2 m above the eaves, midway between them: around the courtyard.
    let ridges = roof["derived"]["ridges"].as_array().unwrap();
    assert!(ridges.len() >= 8, "{ridges:?}");

    // Openings must lie inside, apart.
    let outer = json!([[0, 20], [10, 20], [10, 30], [0, 30]]);
    let outside = json!({"boundary": outer, "holes": [[[8, 22], [12, 22], [12, 24], [8, 24]]]});
    assert_eq!(fails(&mut s, "roof_create", outside), ErrorCode::InvalidArguments);
    let overlapping = json!({"boundary": outer, "holes": [[[2, 22], [5, 22], [5, 25], [2, 25]], [[4, 24], [7, 24], [7, 27], [4, 27]]]});
    assert_eq!(fails(&mut s, "roof_create", overlapping), ErrorCode::InvalidArguments);
}

/// Floors with openings (shafts, stairwells): created with them, added and
/// removed one at a time; the area and volume leave them out.
#[test]
fn floor_openings() {
    let mut s = Session::default();
    let shaft = json!([[1, 1], [3, 1], [3, 3], [1, 3]]);
    let floor =
        run(&mut s, "floor_create", json!({"boundary": [[0, 0], [10, 0], [10, 6], [0, 6]], "openings": [shaft]}));
    let floor_id = id(&floor);
    let volume = |v: &Value| v["derived"]["volume"].as_f64().unwrap();
    let full = volume(&run(&mut s, "floor_create", json!({"boundary": [[20, 0], [30, 0], [30, 6], [20, 6]]})));
    assert!((volume(&floor) - full * 56.0 / 60.0).abs() < 1e-9, "{} vs {full}", volume(&floor));
    // A stairwell beside it; one overlapping or outside is refused.
    let stair = json!({"id": floor_id, "outline": [[5, 1], [9, 1], [9, 3], [5, 3]]});
    let floor = run(&mut s, "floor_opening_add", stair);
    assert_eq!(floor["params"]["openings"].as_array().unwrap().len(), 2);
    let overlapping = json!({"id": floor_id, "outline": [[2, 2], [6, 2], [6, 4], [2, 4]]});
    assert_eq!(fails(&mut s, "floor_opening_add", overlapping), ErrorCode::InvalidArguments);
    let outside = json!({"id": floor_id, "outline": [[9, 4], [12, 4], [12, 5], [9, 5]]});
    assert_eq!(fails(&mut s, "floor_opening_add", outside), ErrorCode::InvalidArguments);
    let floor = run(&mut s, "floor_opening_remove", json!({"id": floor_id, "index": 0}));
    assert_eq!(floor["params"]["openings"], json!([[[5.0, 1.0], [9.0, 1.0], [9.0, 3.0], [5.0, 3.0]]]));
    assert_eq!(fails(&mut s, "floor_opening_remove", json!({"id": floor_id, "index": 1})), ErrorCode::InvalidArguments);
    // An opening across the edge through element_set is refused too.
    let across = json!({"id": floor_id, "params": {"openings": [[[-1, 1], [1, 1], [1, 2], [-1, 2]]]}});
    assert_eq!(fails(&mut s, "element_set", across), ErrorCode::InvalidArguments);
}

/// A slope per edge: a hip roof with a gable end, a steeper eave; one
/// opening removed at a time.
#[test]
fn roof_edge_slopes_and_openings() {
    let mut s = Session::default();
    let rect = json!([[0, 0], [10, 0], [10, 6], [0, 6]]);
    let holes = json!([[[1, 1], [3, 1], [3, 3], [1, 3]], [[6, 2], [8, 2], [8, 4], [6, 4]]]);
    let roof = run(&mut s, "roof_create", json!({"boundary": rect, "holes": holes, "offset": 3.0, "overhang": 0.0}));
    let roof_id = id(&roof);
    // Flat roofs have no edge slopes.
    let edge = json!({"id": roof_id, "edge": 1, "slope": 0.0});
    assert_eq!(fails(&mut s, "roof_edge_set", edge.clone()), ErrorCode::InvalidArguments);

    // One opening goes, the other stays.
    let roof = run(&mut s, "roof_opening_remove", json!({"id": roof_id, "index": 0}));
    let left = roof["params"]["holes"].as_array().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0][0], json!([6.0, 2.0]));
    assert_eq!(fails(&mut s, "roof_opening_remove", json!({"id": roof_id, "index": 1})), ErrorCode::InvalidArguments);
    run(&mut s, "roof_opening_remove", json!({"id": roof_id, "index": 0}));

    // A hip roof at 50 %, then a gable at its east end: the peak stays at 1.5 m.
    run(&mut s, "element_set", json!({"id": roof_id, "params": {"shape": {"Hip": {"slope": 0.5}}}}));
    let roof = run(&mut s, "roof_edge_set", edge);
    assert_eq!(roof["params"]["edge_slopes"], json!([null, 0.0]));
    let top = |s: &mut Session| {
        let b = run(s, "element_get", json!({"id": roof_id}));
        b["derived"]["ridges"].as_array().unwrap().iter().flat_map(|r| r.as_array().unwrap().clone()).count()
    };
    assert!(top(&mut s) >= 3, "a ridge and two hips");
    // A steeper south eave on a gable roof along X: the planes meet 2 m from it, 2 m up.
    run(&mut s, "roof_edge_set", json!({"id": roof_id, "edge": 1, "slope": null}));
    run(
        &mut s,
        "element_set",
        json!({"id": roof_id, "params": {"shape": {"Gable": {"slope": 0.5, "ridge_deg": 0.0}}}}),
    );
    let roof = run(&mut s, "roof_edge_set", json!({"id": roof_id, "edge": 0, "slope": 1.0}));
    assert_eq!(roof["params"]["edge_slopes"], json!([1.0]));
    let ridges = roof["derived"]["ridges"].as_array().unwrap();
    assert!(
        ridges
            .iter()
            .any(|r| (r[0][1].as_f64().unwrap() - 2.0).abs() < 1e-9 && (r[1][1].as_f64().unwrap() - 2.0).abs() < 1e-9),
        "{ridges:?}"
    );
    assert_eq!(
        fails(&mut s, "roof_edge_set", json!({"id": roof_id, "edge": 4, "slope": 1.0})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(&mut s, "roof_edge_set", json!({"id": roof_id, "edge": 0, "slope": -1.0})),
        ErrorCode::InvalidArguments
    );
}

/// Doors and windows are components: the template's types are, a new door
/// component (a double leaf drawing its own plan symbol) cuts its opening in
/// the wall, mirrors with the door's flips, and saves to and loads from a
/// component file as a door type.
#[test]
fn doors_and_windows_are_components() {
    let mut s = Session::default();
    let doors = run(&mut s, "types_list", json!({"category": "door"}));
    let template_door = &doors.as_array().unwrap()[0];
    let t = run(&mut s, "type_get", json!({"type": template_door["id"]}));
    assert_eq!(t["params"]["def"]["kind"], "Door", "{t}");

    let def = json!({
        "kind": "Door",
        "params": [
            {"name": "L", "value": 1.6, "kind": "Length"},
            {"name": "A", "value": 2.1, "kind": "Length"}
        ],
        "items": [
            {"name": "Folha 1", "material": "", "in_plan": true,
             "shape": {"Box": {"x": "-L / 2", "y": "0", "z": "0", "width": "L / 2", "depth": "0.04", "height": "A"}}},
            {"name": "Folha 2", "material": "", "in_plan": true,
             "shape": {"Box": {"x": "0", "y": "0", "z": "0", "width": "L / 2", "depth": "0.04", "height": "A"}}},
            {"name": "Batente", "material": "", "in_plan": false,
             "shape": {"Box": {"x": "-L / 2", "y": "-WallThickness / 2", "z": "A", "width": "L", "depth": "WallThickness", "height": "0.05"}}},
            {"name": "Giro", "plan_only": true,
             "shape": {"Profile": {"plane": "Plan", "offset": "0", "depth": "0.001", "points": [
                 {"u": "-L / 2", "v": "0", "radius": ""},
                 {"u": "-L / 2", "v": "L / 2", "radius": "L / 2"},
                 {"u": "0", "v": "0", "radius": ""}]}}}
        ],
        "opening": {"width": "L", "height": "A", "own_symbol": true}
    });
    let double = run(&mut s, "component_create", json!({"name": "Porta dupla", "def": def}));
    assert_eq!(double["category"], "door", "{double}");
    // Without an opening it would not be a door.
    let mut bad = def.clone();
    bad["opening"]["width"] = json!("0");
    assert_eq!(fails(&mut s, "component_create", json!({"name": "X", "def": bad})), ErrorCode::InvalidArguments);

    let wall = run(&mut s, "wall_create", json!({"start": [0, 0], "end": [6, 0]}));
    let thickness = wall["derived"]["thickness"].as_f64().unwrap();
    let full = wall["derived"]["volume"].as_f64().unwrap();
    let door = run(&mut s, "door_create", json!({"wall": id(&wall), "offset": 3.0, "type": id(&double)}));
    let wall = run(&mut s, "element_get", json!({"id": id(&wall)}));
    let cut = full - wall["derived"]["volume"].as_f64().unwrap();
    assert!((cut - 1.6 * 2.1 * thickness).abs() < 1e-6, "the opening is cut: {cut}");

    // The leaves stand on the side the door opens to; flipping moves them across.
    let side = |s: &mut Session| {
        let fills = &s.geometry().opening_fills[&cc_core::ElementId(id(&door))];
        assert_eq!(fills.len(), 4);
        let leaves = fills.iter().filter(|p| p.in_plan && !p.plan_only);
        let ys: Vec<f64> = leaves.flat_map(|p| p.footprint.iter().map(|q| q.y)).collect();
        ys.iter().sum::<f64>() / ys.len() as f64
    };
    assert!(side(&mut s) > 0.0);
    run(&mut s, "element_flip", json!({"id": id(&door), "facing": true}));
    assert!(side(&mut s) < 0.0);

    // Plans draw the two leaves (its own symbol): two rectangles, no swing arc.
    let plan = cc_core::ViewId(run(&mut s, "views_list", json!({}))[0]["id"].as_u64().unwrap());
    let drawing = s.drawing(plan).unwrap();
    let lines = drawing
        .prims
        .iter()
        .filter(|p| {
            matches!(p, cc_views::Prim::Line { element: Some(e), style: cc_views::LineStyle::Projection, .. }
                if e.0 == id(&door))
        })
        .count();
    assert!(lines > 8, "the leaves' outlines and the swing symbol (the tag draws annotation lines): {lines}");
    // The swing is a plan symbol only: three solids (two leaves and the head).
    assert_eq!(s.geometry().solids_of(cc_core::ElementId(id(&door))).count(), 3);

    // A component file carries the door type.
    let path = scratch("porta-dupla.ccomp");
    run(&mut s, "component_save", json!({"component": "Porta dupla", "path": path}));
    let mut other = Session::default();
    run(&mut other, "component_load", json!({"path": path}));
    let doors = run(&mut other, "types_list", json!({"category": "door"}));
    assert!(doors.as_array().unwrap().iter().any(|t| t["name"] == "Porta dupla"), "{doors}");
}

/// The component library: component files in a folder and its subfolders,
/// searched by name; broken files are reported, not fatal.
#[test]
fn component_library_lists_folders() {
    let mut s = Session::default();
    let dir = scratch("library");
    let sub = dir.join("portas");
    std::fs::create_dir_all(&sub).unwrap();
    let names = |s: &mut Session, ids: &str| -> Vec<String> {
        run(s, "types_list", json!({"category": ids}))
            .as_array()
            .unwrap()
            .iter()
            .map(|t| run(s, "type_get", json!({"type": t["id"]}))["params"]["component"].as_str().unwrap().to_string())
            .collect()
    };
    let furniture = names(&mut s, "component")[0].clone();
    let door = names(&mut s, "door")[0].clone();
    run(&mut s, "component_save", json!({"component": furniture, "path": dir.join("movel.ccomp")}));
    run(&mut s, "component_save", json!({"component": door, "path": sub.join("porta.ccomp")}));
    std::fs::write(dir.join("quebrado.ccomp"), "not a component").unwrap();

    let all = run(&mut s, "component_library_list", json!({"folders": [dir]}));
    let listed = all["components"].as_array().unwrap();
    assert_eq!(listed.len(), 2, "{all}");
    assert!(listed.iter().any(|c| c["component"] == door.as_str() && c["category"] == "door"), "{all}");
    assert_eq!(all["errors"].as_array().unwrap().len(), 1);
    let found = run(&mut s, "component_library_list", json!({"folders": [dir], "search": door.to_uppercase()}));
    assert_eq!(found["components"].as_array().unwrap().len(), 1, "{found}");
}

#[test]
fn grids_are_named_in_series() {
    let mut s = Session::default();
    let grid = |s: &mut Session, args: Value| run(s, "grid_create", args)["params"]["name"].clone();
    // Vertical grids count 1, 2; across them a new series starts with letters.
    assert_eq!(grid(&mut s, json!({"start": [0, 0], "end": [0, 10]})), "1");
    assert_eq!(grid(&mut s, json!({"start": [6, 0], "end": [6, 10]})), "2");
    assert_eq!(grid(&mut s, json!({"start": [-1, 1], "end": [13, 1]})), "A");
    // Each series goes on after its last grid, whichever way it is drawn.
    assert_eq!(grid(&mut s, json!({"start": [12, 10], "end": [12, 0]})), "3");
    assert_eq!(grid(&mut s, json!({"start": [-1, 5], "end": [13, 5]})), "B");
    // A given name is kept and the series continues from it.
    assert_eq!(grid(&mut s, json!({"start": [-1, 7], "end": [13, 7], "name": "Y"})), "Y");
    assert_eq!(grid(&mut s, json!({"start": [-1, 8], "end": [13, 8]})), "Z");
    assert_eq!(grid(&mut s, json!({"start": [-1, 9], "end": [13, 9]})), "AA");
    assert_eq!(grid(&mut s, json!({"start": [3, 0], "end": [3, 10], "name": "1.1"})), "1.1");
    assert_eq!(grid(&mut s, json!({"start": [4, 0], "end": [4, 10]})), "1.2");
    // Names stay unique: a taken successor is skipped.
    assert_eq!(grid(&mut s, json!({"start": [20, 0], "end": [20, 10], "name": "5"})), "5");
    assert_eq!(grid(&mut s, json!({"start": [15, 0], "end": [15, 10], "name": "4"})), "4");
    assert_eq!(grid(&mut s, json!({"start": [16, 0], "end": [16, 10]})), "6");
    let ids: Vec<Value> = run(&mut s, "elements_list", json!({"category": "grid"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].clone())
        .collect();
    let mut names: Vec<String> = ids
        .iter()
        .map(|id| run(&mut s, "element_get", json!({"id": id}))["params"]["name"].as_str().unwrap().to_string())
        .collect();
    let count = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), count, "{names:?}");
}

#[test]
fn grid_ends_align_to_a_reference() {
    let mut s = Session::default();
    let point = |s: &mut Session, id: u64, end: &str| run(s, "element_get", json!({"id": id}))["params"][end].clone();
    let reference = id(&run(&mut s, "grid_create", json!({"start": [0, -1], "end": [0, 10]})));
    let short = id(&run(&mut s, "grid_create", json!({"start": [5, 0.5], "end": [5, 7]})));
    // Drawn the other way: its start goes to the reference's end.
    let back = id(&run(&mut s, "grid_create", json!({"start": [9, 8], "end": [9, 2]})));
    let across = id(&run(&mut s, "grid_create", json!({"start": [-2, 3], "end": [11, 3]})));
    let pinned = id(&run(&mut s, "grid_create", json!({"start": [12, 0], "end": [12, 9]})));
    run(&mut s, "elements_pin", json!({"ids": [pinned], "pinned": true}));

    // The bubble ends only.
    let out = run(&mut s, "grids_align_ends", json!({"reference": reference, "ends": "start"}));
    assert_eq!(out["changed"], json!([short, back]), "{out}");
    assert_eq!(out["pinned_skipped"], json!([pinned]));
    assert_eq!(out["not_parallel"], json!([]));
    assert_eq!(point(&mut s, short, "start"), json!([5.0, -1.0]));
    assert_eq!(point(&mut s, short, "end"), json!([5.0, 7.0]));
    assert_eq!(point(&mut s, back, "start"), json!([9.0, 10.0]));
    // Then both ends, in one undo step.
    run(&mut s, "grids_align_ends", json!({"reference": reference}));
    assert_eq!(point(&mut s, short, "end"), json!([5.0, 10.0]));
    assert_eq!(point(&mut s, back, "end"), json!([9.0, -1.0]));
    assert_eq!(point(&mut s, across, "start"), json!([-2.0, 3.0]));
    run(&mut s, "undo", json!({}));
    assert_eq!(point(&mut s, short, "end"), json!([5.0, 7.0]));
    assert_eq!(point(&mut s, back, "end"), json!([9.0, 2.0]));
    // Aligned already: nothing changes.
    let again = run(&mut s, "grids_align_ends", json!({"reference": reference, "ends": "start"}));
    assert_eq!(again["changed"], json!([]));
    // Explicit grids that are not parallel are reported, not moved.
    let out = run(&mut s, "grids_align_ends", json!({"reference": reference, "grids": [across, short]}));
    assert_eq!((out["changed"].clone(), out["not_parallel"].clone()), (json!([short]), json!([across])), "{out}");
    // Bad arguments.
    let wall = id(&run(&mut s, "wall_create", json!({"start": [0, 20], "end": [5, 20]})));
    assert_eq!(fails(&mut s, "grids_align_ends", json!({"reference": wall})), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "grids_align_ends", json!({"reference": 999_999})), ErrorCode::NotFound);
    let bad = json!({"reference": reference, "grids": [wall]});
    assert_eq!(fails(&mut s, "grids_align_ends", bad), ErrorCode::InvalidArguments);
    let bad = json!({"reference": reference, "ends": "middle"});
    assert_eq!(fails(&mut s, "grids_align_ends", bad), ErrorCode::InvalidArguments);
}

/// Stairs in every shape and construction: their steps are checked against
/// the type's rules (Blondel's formula), and railings added along them
/// follow the stair when it is copied, mirrored or deleted.
#[test]
fn stairs_shapes_rules_and_railings() {
    let mut s = Session::default();
    let shapes = ["straight", "straight_landing", "l_shaped", "u_shaped", "spiral"];
    for (k, shape) in shapes.iter().enumerate() {
        let st = run(&mut s, "stair_create", json!({"start": [k as f64 * 5.0, 0.0], "shape": shape}));
        assert_eq!(st["params"]["layout"]["shape"], json!(shape_name(shape)), "{st}");
        let check = run(&mut s, "stair_check", json!({"id": id(&st)}));
        assert_eq!(check["risers"], 17, "{shape}: {check}");
        let volume = run(&mut s, "element_get", json!({"id": id(&st)}))["derived"]["volume"].as_f64();
        assert!(volume.is_some_and(|v| v > 0.0), "{shape}");
    }
    assert_eq!(fails(&mut s, "stair_create", json!({"start": [0, 0], "shape": "zigzag"})), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "stair_create", json!({"start": [0, 0], "winders": 1})), ErrorCode::InvalidArguments);

    // The template's type up to the level above: 3.00 m in 17 risers on 28 cm treads, 2R + T = 63.3 cm.
    let stair = run(&mut s, "stair_create", json!({"start": [0, 10], "shape": "l_shaped", "winders": 3}));
    let sid = id(&stair);
    let check = run(&mut s, "stair_check", json!({"id": sid}));
    assert!(check["ok"].as_bool().unwrap(), "{check}");
    assert!((check["blondel"].as_f64().unwrap() - (6.0 / 17.0 + 0.28)).abs() < 1e-9);
    // Wider treads break Blondel's range; the Blondel rule brings them back.
    let ty = stair["type"]["id"].clone();
    let wide = run(&mut s, "type_duplicate", json!({"type": ty, "name": "Wide treads", "elements": [sid]}));
    run(&mut s, "type_set", json!({"type": wide["id"], "params": {"tread": 0.34}}));
    let check = run(&mut s, "stair_check", json!({"id": sid}));
    assert_eq!(check["issues"][0]["detail"]["issue"], "blondel_out_of_range", "{check}");
    assert!(check["issues"][0]["message"].as_str().is_some_and(|m| !m.is_empty()));
    let rules = json!({"rules": {"tread_rule": "Blondel"}, "build": {"construction": "CastInPlace", "waist": 0.12}});
    run(&mut s, "type_set", json!({"type": wide["id"], "params": rules}));
    let check = run(&mut s, "stair_check", json!({"id": sid}));
    let tread = check["tread"].as_f64().unwrap();
    assert!(check["ok"].as_bool().unwrap() && (tread - (0.64 - 6.0 / 17.0)).abs() < 1e-9, "{check}");
    let cast = run(&mut s, "element_get", json!({"id": sid}))["derived"]["volume"].as_f64().unwrap();
    run(&mut s, "type_set", json!({"type": wide["id"], "params": {"build": {"construction": "Assembled"}}}));
    let assembled = run(&mut s, "element_get", json!({"id": sid}))["derived"]["volume"].as_f64().unwrap();
    assert!(assembled > 0.0 && assembled < cast, "{assembled} vs {cast}");

    // Railings on both sides, rising with the stair.
    let railings = run(&mut s, "stair_railings_add", json!({"id": sid}));
    let ids: Vec<u64> = railings["ids"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    assert_eq!(ids.len(), 2);
    let length = |s: &mut Session, id: u64| run(s, "element_get", json!({"id": id}))["derived"]["length"].as_f64();
    let sloped = length(&mut s, ids[0]).unwrap();
    // Every post stands on a tread, a landing or a floor, not on the pitch line above them.
    {
        let p = s.project();
        let stair_el = p.element(cc_core::ElementId(sid)).unwrap();
        let cc_core::ElementKind::Stair(st) = &stair_el.kind else { panic!("not a stair") };
        let floors: Vec<f64> = cc_geom::stair_geom(p, stair_el, st)
            .unwrap()
            .steps
            .iter()
            .map(|step| step.z)
            .chain([p.level_elevation(stair_el.level), p.stair_span(stair_el, st).1])
            .collect();
        for &rail in &ids {
            let e = p.element(cc_core::ElementId(rail)).unwrap();
            let cc_core::ElementKind::Railing(r) = &e.kind else { panic!("not a railing") };
            let mut posts = 0;
            for piece in cc_geom::railing_pieces(p, e, r) {
                let pts: Vec<_> = piece.iter().flatten().collect();
                let (lo, hi) =
                    pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| (lo.min(q.z), hi.max(q.z)));
                let flat = pts.iter().all(|q| q.truncate().distance(pts[0].truncate()) < 0.06);
                if flat && hi - lo > 0.3 {
                    posts += 1;
                    assert!(floors.iter().any(|f| (f - lo).abs() < 1e-9), "a post starts at {lo}, off the steps");
                }
            }
            assert!(posts >= 3, "{posts} posts");
        }
    }
    assert!(sloped > 2.88, "a railing climbs the whole height: {sloped}");
    let count = |s: &mut Session| run(s, "elements_list", json!({"category": "railing"})).as_array().unwrap().len();
    // A copy takes its railings along; a mirror turns the other way.
    run(&mut s, "elements_copy", json!({"ids": [sid], "delta": [0.0, 10.0]}));
    assert_eq!(count(&mut s), 4);
    let mirrored =
        run(&mut s, "elements_mirror", json!({"ids": [sid], "axis": [[20.0, 0.0], [20.0, 1.0]], "copy": true}));
    let copy = mirrored["created"][0].as_u64().unwrap();
    assert_eq!(run(&mut s, "element_get", json!({"id": copy}))["params"]["layout"]["turn"], "Right");
    assert_eq!(count(&mut s), 6);
    // Deleting a stair deletes its railings; undo brings them back.
    run(&mut s, "elements_delete", json!({"ids": [sid]}));
    assert_eq!(count(&mut s), 4);
    run(&mut s, "undo", json!({}));
    assert_eq!(count(&mut s), 6);
    assert!(length(&mut s, ids[1]).is_some_and(|l| l > 2.88));

    // Exported to IFC, stairs and railings are B-reps declared as such.
    let ifc = scratch("stairs.ifc");
    run(&mut s, "ifc_export", json!({"path": ifc.to_string_lossy()}));
    assert_representation_types(&std::fs::read_to_string(&ifc).unwrap());

    // Saved and loaded, everything comes back.
    let path = scratch("stairs.ccad");
    run(&mut s, "project_save", json!({"path": path}));
    let mut t = Session::default();
    run(&mut t, "project_open", json!({"path": path}));
    assert_eq!(count(&mut t), 6);
    let back = run(&mut t, "stair_check", json!({"id": sid}));
    assert_eq!(back["risers"], 17);
}

fn shape_name(shape: &str) -> &'static str {
    match shape {
        "straight_landing" => "StraightLanding",
        "l_shaped" => "LShaped",
        "u_shaped" => "UShaped",
        "spiral" => "Spiral",
        _ => "Straight",
    }
}

/// Every shape representation of an IFC file names the kind of its items
/// (`SweptSolid` for extrusions, `Brep` for faceted B-reps), as the IFC
/// schema's `IfcShapeRepresentationTypes` rule checks.
fn assert_representation_types(ifc: &str) {
    let mut entities = std::collections::HashMap::new();
    for line in ifc.lines() {
        if let Some((id, rest)) = line.strip_prefix('#').and_then(|l| l.split_once('=')) {
            entities.insert(format!("#{}", id.trim()), rest.trim().to_string());
        }
    }
    for body in entities.values().filter(|e| e.starts_with("IFCSHAPEREPRESENTATION(")) {
        let kind = if body.contains("'SweptSolid'") {
            "SweptSolid"
        } else if body.contains("'Brep'") {
            "Brep"
        } else {
            continue;
        };
        let items = body.rsplit_once('(').map_or("", |(_, i)| i);
        for item in items.trim_end_matches([')', ';']).split(',').map(str::trim) {
            let Some(entity) = entities.get(item) else { continue };
            let brep = entity.starts_with("IFCFACETEDBREP(");
            assert_eq!(brep, kind == "Brep", "{kind} representation with {entity}");
        }
    }
}

/// Ramps in every shape: a project without a ramp type gets one, flights and
/// landings follow the type's rules, broken rules are reported, railings
/// follow the ramp, and IFC export writes `IfcRamp`.
#[test]
fn ramps_shapes_rules_and_railings() {
    let mut s = Session::default();
    let types = |s: &mut Session| run(s, "types_list", json!({"category": "ramp"})).as_array().unwrap().len();
    assert_eq!(types(&mut s), 0);
    // Up to the level above (3.00 m) at 8.33 %: 4 flights of 0.75 m.
    let ramp = run(&mut s, "ramp_create", json!({"start": [0, 0]}));
    assert_eq!(types(&mut s), 1, "the first ramp makes a ramp type");
    let rid = id(&ramp);
    let check = run(&mut s, "ramp_check", json!({"id": rid}));
    assert_eq!(check["flights"], 4, "{check}");
    assert!(check["ok"].as_bool().unwrap(), "{check}");
    assert!((check["slope"].as_f64().unwrap() - 0.0833).abs() < 1e-9);
    let run_of =
        |s: &mut Session, id: u64| run(s, "element_get", json!({"id": id}))["derived"]["run"].as_f64().unwrap();
    let flight = 0.75 / 0.0833;
    assert!((run_of(&mut s, rid) - (4.0 * flight + 3.0 * 1.2)).abs() < 1e-6);
    for (k, shape) in ["l_shaped", "u_shaped", "spiral"].iter().enumerate() {
        let r =
            run(&mut s, "ramp_create", json!({"start": [0.0, 10.0 * (k + 1) as f64], "shape": shape, "turn": "right"}));
        let volume = run(&mut s, "element_get", json!({"id": id(&r)}))["derived"]["volume"].as_f64();
        assert!(volume.is_some_and(|v| v > 0.0), "{shape}");
    }
    assert_eq!(types(&mut s), 1);
    // An L-shaped ramp rising 3 m has flights too high; a steeper slope and a short landing break rules too.
    let l = run(&mut s, "ramp_create", json!({"start": [50, 0], "shape": "l_shaped"}));
    let check = run(&mut s, "ramp_check", json!({"id": id(&l)}));
    assert_eq!(check["issues"][0]["detail"]["issue"], "flight_too_high", "{check}");
    let steep = run(&mut s, "ramp_create", json!({"start": [60, 0], "slope": 12.5, "landing": 1.0}));
    let check = run(&mut s, "ramp_check", json!({"id": id(&steep)}));
    let issues: Vec<_> = check["issues"].as_array().unwrap().iter().map(|i| i["detail"]["issue"].clone()).collect();
    assert_eq!(issues, [json!("too_steep"), json!("landing_too_short")]);
    assert_eq!(fails(&mut s, "ramp_create", json!({"start": [0, 0], "shape": "zigzag"})), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "ramp_create", json!({"start": [0, 0], "slope": -2})), ErrorCode::InvalidArguments);
    // The type's rules decide: a gentler maximum makes longer flights.
    let ty = ramp["type"]["id"].clone();
    run(&mut s, "type_set", json!({"type": ty, "params": {"rules": {"max_slope": 0.05}}}));
    assert!(run_of(&mut s, rid) > 4.0 * 0.75 / 0.05);
    // A new type through its command.
    let custom = run(
        &mut s,
        "ramp_type_create",
        json!({"name": "Garage ramp", "width": 3.0, "max_slope": 20, "max_flight_rise": 4}),
    );
    let garage = run(&mut s, "ramp_create", json!({"start": [80, 0], "type": custom["id"], "shape": "spiral"}));
    assert!(run(&mut s, "ramp_check", json!({"id": id(&garage)}))["ok"].as_bool().unwrap());

    // Railings follow the ramp up, and go with it.
    let railings = run(&mut s, "stair_railings_add", json!({"id": rid}));
    let rail = railings["ids"][0].as_u64().unwrap();
    let length = run(&mut s, "element_get", json!({"id": rail}))["derived"]["length"].as_f64().unwrap();
    assert!(length > run_of(&mut s, rid), "{length}");
    let count = |s: &mut Session| run(s, "elements_list", json!({"category": "railing"})).as_array().unwrap().len();
    let mirrored =
        run(&mut s, "elements_mirror", json!({"ids": [rid], "axis": [[-5.0, 0.0], [-5.0, 1.0]], "copy": true}));
    let copy = mirrored["created"][0].as_u64().unwrap();
    assert_eq!(run(&mut s, "element_get", json!({"id": copy}))["params"]["layout"]["turn"], "Right");
    assert_eq!(count(&mut s), 4);
    run(&mut s, "elements_delete", json!({"ids": [rid]}));
    assert_eq!(count(&mut s), 2);

    // IFC export writes the ramps.
    let path = scratch("ramps.ifc");
    run(&mut s, "ifc_export", json!({"path": path.to_string_lossy()}));
    let ifc = std::fs::read_to_string(&path).unwrap();
    assert!(ifc.contains("IFCRAMP(") && ifc.contains(".SPIRAL_RAMP.") && ifc.contains(".QUARTER_TURN_RAMP."));
    assert!(ifc.contains("IFCRAMPTYPE("));
    assert_representation_types(&ifc);
}

/// Tags move by hand in a plan (door, window, beam, column tags): the tag
/// is drawn where it was put, with a leader back to its element, each plan
/// keeping its own; reset puts it back, and deleting the element drops it.
#[test]
fn tags_are_moved_in_their_plan() {
    let mut s = Session::default();
    let wall = run(&mut s, "wall_create", json!({"start": [0, 0], "end": [6, 0]}));
    let window = id(&run(&mut s, "window_create", json!({"wall": wall["id"], "mark": "J1"})));
    let plans = run(&mut s, "views_list", json!({"category": "floor_plan"}));
    let plan = cc_core::ViewId(id(&plans[0]));
    let tag = |s: &mut Session| {
        let d = s.drawing(plan).unwrap();
        let t = d.tags.iter().find(|t| t.element.0 == window).copied().unwrap();
        let leader = d.prims.iter().filter(|p| matches!(p, cc_views::Prim::Line { style: cc_views::LineStyle::Annotation, element: Some(e), .. } if e.0 == window)).count();
        (t.center, leader)
    };
    let (home, lines_home) = tag(&mut s);
    run(&mut s, "tag_move", json!({"view": plan.0, "id": window, "delta": [0.5, 2.0]}));
    let (moved, lines_moved) = tag(&mut s);
    assert!((moved - home - glam::DVec2::new(0.5, 2.0)).length() < 1e-9, "{home} → {moved}");
    assert_eq!(lines_moved, lines_home + 1, "a leader to the window");
    // A duplicate plan starts with the same tag positions, then each keeps its own.
    let other = id(&run(&mut s, "view_duplicate", json!({"view": plan.0})));
    run(&mut s, "tag_move", json!({"view": other, "id": window, "reset": true}));
    let d = s.drawing(cc_core::ViewId(other)).unwrap();
    assert!((d.tags.iter().find(|t| t.element.0 == window).unwrap().center - home).length() < 1e-9);
    assert!((tag(&mut s).0 - moved).length() < 1e-9, "the first plan keeps its tag");
    // Back home, then moved again and the window deleted: the offset goes with it.
    run(&mut s, "tag_move", json!({"view": plan.0, "id": window, "reset": true}));
    assert!((tag(&mut s).0 - home).length() < 1e-9);
    run(&mut s, "tag_move", json!({"view": plan.0, "id": window, "offset": [1, 1]}));
    run(&mut s, "elements_delete", json!({"ids": [window]}));
    assert!(s.project().view(plan).unwrap().tag_offsets.is_empty());
    // Bad requests.
    let three_d = run(&mut s, "view_create", json!({"kind": "three_d"}));
    assert_eq!(
        fails(&mut s, "tag_move", json!({"view": three_d["id"], "id": id(&wall), "reset": true})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(fails(&mut s, "tag_move", json!({"view": plan.0, "id": id(&wall)})), ErrorCode::InvalidArguments);
    assert_eq!(
        fails(&mut s, "tag_move", json!({"view": plan.0, "id": id(&wall), "offset": [5000, 0]})),
        ErrorCode::InvalidArguments
    );
}

/// Conceptual masses (#31): an extrusion and a loft, their mass floors at
/// the levels they span with areas, perimeters and volume, choosing the
/// floors, schedules, moving, undo, saving, and a level deleted under them.
#[test]
fn masses_and_mass_floors() {
    let mut s = Session::default();
    let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
    let levels = run(&mut s, "levels_list", json!({}));
    let ground = id(&levels[0]);
    let first_z = levels[1]["elevation"].as_f64().unwrap();
    let top_level = id(&run(&mut s, "level_create", json!({"name": "Cobertura", "elevation": 2.0 * first_z})));
    // 10 x 6 m, three storeys high: floors at the three levels it spans.
    let outline = json!([[0, 0], [10, 0], [10, 6], [0, 6]]);
    let mass = run(&mut s, "mass_create", json!({"outline": outline, "height": 3.0 * first_z}));
    let m = id(&mass);
    assert_eq!(mass["category"], "mass");
    let floors = run(&mut s, "mass_floors", json!({"id": m}));
    assert_eq!(floors["floors"].as_array().unwrap().len(), 3);
    assert!(near(floors["floor_area"].as_f64().unwrap(), 180.0), "{floors}");
    assert!(near(floors["volume"].as_f64().unwrap(), 60.0 * 3.0 * first_z));
    assert!(near(floors["floors"][0]["perimeter"].as_f64().unwrap(), 32.0));
    let e = run(&mut s, "element_get", json!({"id": m}));
    assert!(near(e["derived"]["floor_area"].as_f64().unwrap(), 180.0));
    // Plans: the upper floor's plan cuts the mass (it stands on the ground
    // level) and labels the mass floor of that level with its area.
    let first_name = levels[1]["name"].clone();
    let plans = run(&mut s, "views_list", json!({"category": "floor_plan"}));
    let upper_plan = plans.as_array().unwrap().iter().find(|v| v["name"] == first_name).map(id).unwrap();
    let d = s.drawing(cc_core::ViewId(upper_plan)).unwrap();
    let mine = |e: &Option<cc_core::ElementId>| e.is_some_and(|e| e.0 == m);
    assert!(
        d.prims.iter().any(
            |p| matches!(p, cc_views::Prim::Line { style: cc_views::LineStyle::Cut, element, .. } if mine(element))
        )
    );
    assert!(
        d.prims
            .iter()
            .any(|p| matches!(p, cc_views::Prim::Text { text, element, .. } if mine(element) && text.contains("60")))
    );

    // Only the ground and the roof level.
    run(&mut s, "mass_floors_set", json!({"id": m, "levels": [ground, "Cobertura"]}));
    assert!(near(run(&mut s, "mass_floors", json!({"id": m}))["floor_area"].as_f64().unwrap(), 120.0));
    // No floors, then all again.
    run(&mut s, "mass_floors_set", json!({"id": m, "levels": []}));
    assert_eq!(run(&mut s, "mass_floors", json!({"id": m}))["floors"], json!([]));
    run(&mut s, "mass_floors_set", json!({"id": m}));
    assert_eq!(run(&mut s, "mass_floors", json!({"id": m}))["floors"].as_array().unwrap().len(), 3);

    // A loft set in by 1 m: 10 x 6 at the base, 8 x 4 at the top; halfway 9 x 5.
    let loft = id(&run(
        &mut s,
        "mass_create",
        json!({"outline": [[20, 0], [30, 0], [30, 6], [20, 6]], "height": 2.0 * first_z, "top_inset": 1.0}),
    ));
    let floors = run(&mut s, "mass_floors", json!({"id": loft}));
    let areas: Vec<f64> = floors["floors"].as_array().unwrap().iter().map(|f| f["area"].as_f64().unwrap()).collect();
    assert_eq!(areas.len(), 2, "the top level is not a floor of a mass that ends there");
    assert!(near(areas[0], 60.0) && near(areas[1], 45.0), "{areas:?}");

    // A schedule of masses: floors, gross floor area, volume.
    let schedule = run(&mut s, "view_create", json!({"kind": "schedule", "category": "mass"}));
    let table = run(&mut s, "schedule_table", json!({"view": schedule["id"]})).to_string();
    assert!(table.contains("180"), "{table}");

    // Moving moves both outlines; undo puts it back.
    run(&mut s, "elements_move", json!({"ids": [loft], "delta": [0, 10]}));
    let e = run(&mut s, "element_get", json!({"id": loft}));
    assert!(near(e["params"]["outline"][0][1].as_f64().unwrap(), 10.0), "{e}");
    assert!(near(e["params"]["top"][0][1].as_f64().unwrap(), 11.0), "{e}");
    run(&mut s, "undo", json!({}));

    // Saved and reopened.
    let path = scratch("masses.ccad");
    run(&mut s, "project_save", json!({"path": path}));
    let mut reopened = Session::default();
    run(&mut reopened, "project_open", json!({"path": path}));
    assert!(near(run(&mut reopened, "mass_floors", json!({"id": m}))["floor_area"].as_f64().unwrap(), 180.0));

    // Deleting a level takes its mass floors along.
    run(&mut s, "level_delete", json!({"level": top_level}));
    assert_eq!(run(&mut s, "mass_floors", json!({"id": m}))["floors"].as_array().unwrap().len(), 2);

    // Bad input is refused.
    for args in [
        json!({"outline": [[0, 0], [1, 0]]}),
        json!({"outline": [[0, 0], [1, 0], [2, 0]]}),
        json!({"outline": outline, "height": -1}),
        json!({"outline": outline, "top_inset": 10}),
        json!({"outline": outline, "top_inset": 1, "top": outline}),
    ] {
        assert_eq!(fails(&mut s, "mass_create", args), ErrorCode::InvalidArguments);
    }
    assert_eq!(fails(&mut s, "mass_floors", json!({"id": id(&levels[0])})), ErrorCode::NotFound);
}

/// Push/pull on masses (#71): the top raises the mass (and gains the mass
/// floors of the levels it now spans), the bottom lowers its base, a side
/// moves out keeping its neighbors' directions; lofts keep their sides and
/// a face pushed through the mass is refused.
#[test]
fn mass_faces_push_and_pull() {
    let mut s = Session::default();
    let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
    let levels = run(&mut s, "levels_list", json!({}));
    let first_z = levels[1]["elevation"].as_f64().unwrap();
    let m = id(&run(
        &mut s,
        "mass_create",
        json!({"outline": [[0, 0], [10, 0], [10, 6], [0, 6]], "height": first_z / 2.0}),
    ));
    assert_eq!(run(&mut s, "mass_floors", json!({"id": m}))["floors"].as_array().unwrap().len(), 1);
    // The top up past the first floor: two floors now.
    run(&mut s, "mass_face_move", json!({"id": m, "face": "top", "distance": first_z}));
    let floors = run(&mut s, "mass_floors", json!({"id": m}));
    assert_eq!(floors["floors"].as_array().unwrap().len(), 2, "{floors}");
    // The east side out by 2 m: 12 x 6 on both floors.
    run(&mut s, "mass_face_move", json!({"id": m, "face": "side", "side": 1, "distance": 2.0}));
    assert!(near(run(&mut s, "mass_floors", json!({"id": m}))["floor_area"].as_f64().unwrap(), 144.0));
    // The bottom down 1 m.
    run(&mut s, "mass_face_move", json!({"id": m, "face": "bottom", "distance": 1.0}));
    let e = run(&mut s, "element_get", json!({"id": m}));
    assert!(near(e["params"]["base_offset"].as_f64().unwrap(), -1.0), "{e}");
    // Undo goes back one push at a time.
    run(&mut s, "undo", json!({}));
    assert!(near(run(&mut s, "element_get", json!({"id": m}))["params"]["base_offset"].as_f64().unwrap(), 0.0));
    // Through the mass, a missing side, a loft's side: refused.
    assert_eq!(fails(&mut s, "mass_face_move", json!({"id": m, "face": "top", "distance": -100})), ErrorCode::Rejected);
    assert_eq!(
        fails(&mut s, "mass_face_move", json!({"id": m, "face": "side", "side": 7, "distance": 1})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(&mut s, "mass_face_move", json!({"id": m, "face": "front", "distance": 1})),
        ErrorCode::InvalidArguments
    );
    let loft =
        id(&run(&mut s, "mass_create", json!({"outline": [[20, 0], [30, 0], [30, 6], [20, 6]], "top_inset": 1.0})));
    assert_eq!(
        fails(&mut s, "mass_face_move", json!({"id": loft, "face": "side", "side": 0, "distance": 1})),
        ErrorCode::Rejected
    );
    run(&mut s, "mass_face_move", json!({"id": loft, "face": "top", "distance": 1.0}));
}

/// Walls, floors and a roof from a mass (#31): walls on its sides with
/// their exterior faces on the mass's, joined at the corners; a floor on
/// each mass floor; a flat roof on its top. Lofts take no walls.
#[test]
fn walls_floors_and_roof_by_face() {
    let mut s = Session::default();
    let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
    let levels = run(&mut s, "levels_list", json!({}));
    let first_z = levels[1]["elevation"].as_f64().unwrap();
    let outline = json!([[0, 0], [10, 0], [10, 6], [0, 6]]);
    let m = id(&run(&mut s, "mass_create", json!({"outline": outline, "height": 2.0 * first_z})));
    let walls = run(&mut s, "mass_walls_create", json!({"id": m}));
    let walls: Vec<u64> = walls["created"].as_array().unwrap().iter().filter_map(Value::as_u64).collect();
    assert_eq!(walls.len(), 4);
    let geom = s.geometry().clone();
    for w in &walls {
        let g = &geom.walls[&cc_core::ElementId(*w)];
        assert!(g.start_joined && g.end_joined, "wall {w} joins at both corners");
        // The exterior face lies on the mass's outline: every corner of the
        // body is within the 10 x 6 rectangle.
        for f in geom.solids_of(cc_core::ElementId(*w)).flat_map(|s| s.faces.iter()) {
            for v in &f.verts {
                assert!(v.x > -1e-6 && v.x < 10.0 + 1e-6 && v.y > -1e-6 && v.y < 6.0 + 1e-6, "{v}");
            }
        }
        let e = run(&mut s, "element_get", json!({"id": w}));
        assert!(near(e["params"]["height"].as_f64().unwrap(), 2.0 * first_z));
    }
    // Two mass floors: two floors of 60 m².
    let slabs = run(&mut s, "mass_slabs_create", json!({"id": m}));
    let slabs = slabs["created"].as_array().unwrap();
    assert_eq!(slabs.len(), 2);
    let floor = run(&mut s, "element_get", json!({"id": slabs[1]}));
    assert_eq!(floor["level"]["id"], levels[1]["id"]);
    // A flat roof on the top.
    let roof = run(&mut s, "mass_roof_create", json!({"id": m}));
    assert_eq!(roof["category"], "roof");
    assert!(near(roof["params"]["base_offset"].as_f64().unwrap(), 2.0 * first_z));
    // One undo step each.
    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));
    assert_eq!(run(&mut s, "elements_list", json!({"category": "floor"})).as_array().unwrap().len(), 0);
    // Lofts take no walls, some sides only, bad sides refused.
    let loft =
        id(&run(&mut s, "mass_create", json!({"outline": [[20, 0], [30, 0], [30, 6], [20, 6]], "top_inset": 1.0})));
    assert_eq!(fails(&mut s, "mass_walls_create", json!({"id": loft})), ErrorCode::Rejected);
    run(&mut s, "mass_roof_create", json!({"id": loft}));
    let two = run(&mut s, "mass_walls_create", json!({"id": m, "sides": [0, 2]}));
    assert_eq!(two["created"].as_array().unwrap().len(), 2);
    assert_eq!(fails(&mut s, "mass_walls_create", json!({"id": m, "sides": [4]})), ErrorCode::InvalidArguments);
}

/// Roof coverings and framing: a gable roof with French tiles on timber
/// purlins, rafters and battens; the check against the tiles' least slope,
/// the tiles and weight, the framing figures and its own solid (left out of
/// the roof's volume); steel purlins for fiber cement sheets.
#[test]
fn roof_tiles_and_framing() {
    let mut s = Session::default();
    let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
    let roof = id(&run(&mut s, "roof_create", json!({"boundary": [[0, 0], [10, 0], [10, 8], [0, 8]], "overhang": 0})));
    run(&mut s, "element_set", json!({"id": roof, "params": {"shape": {"Gable": {"slope": 0.35, "ridge_deg": 0}}}}));
    let ty = run(&mut s, "element_get", json!({"id": roof}))["type"]["id"].as_u64().unwrap();
    let volume_bare = run(&mut s, "element_get", json!({"id": roof}))["derived"]["volume"].as_f64().unwrap();

    // A covering brings the framing it usually lies on when none was chosen.
    run(&mut s, "type_set", json!({"type": ty, "params": {"covering": "CeramicFrench"}}));
    assert_eq!(run(&mut s, "roof_check", json!({"id": roof}))["framing"]["system"], "Timber");
    // Tiles only: the check and the figures, no members.
    run(&mut s, "type_set", json!({"type": ty, "params": {"framing": {"system": "None"}}}));
    let check = run(&mut s, "roof_check", json!({"id": roof}));
    assert_eq!(check["ok"], true, "{check}");
    assert!(near(check["slopes"][0].as_f64().unwrap(), 0.35));
    let area = check["sloped_area"].as_f64().unwrap();
    assert!(area > 80.0 && area < 90.0, "about 80 m² of plan at 35 %: {area}");
    assert_eq!(check["tiles"].as_f64().unwrap(), (16.0 * area).ceil());
    assert!(near(check["weight"].as_f64().unwrap(), 45.0 * area));
    assert_eq!(check["framing"]["members"], 0);

    // Timber framing: purlins, rafters and battens, in their own solid.
    run(&mut s, "type_set", json!({"type": ty, "params": {"framing": {"system": "Timber"}}}));
    let check = run(&mut s, "roof_check", json!({"id": roof}));
    for k in ["purlins", "rafters", "battens"] {
        assert!(check["framing"][k].as_f64().unwrap() > 10.0, "{k}: {check}");
    }
    assert!(check["framing"]["volume"].as_f64().unwrap() > 0.1);
    let e = run(&mut s, "element_get", json!({"id": roof}));
    assert!(near(e["derived"]["volume"].as_f64().unwrap(), volume_bare), "framing is not the roof's volume");
    assert_eq!(e["derived"]["roofing"]["ok"], true);
    assert!(s.geometry().solids_of(cc_core::ElementId(roof)).any(|x| x.category == cc_core::Category::Beam));
    // Not the roof's body (joins, IFC export).
    assert!(s.geometry().body_of(cc_core::ElementId(roof)).all(|x| !x.secondary));
    assert_eq!(check["framing"]["trusses"], 0);

    // Timber trusses under the purlins, no farther apart than their spacing along the 10 m ridge.
    run(
        &mut s,
        "type_set",
        json!({"type": ty, "params": {"framing": {"system": "TimberTrusses", "truss_spacing": 2.5}}}),
    );
    let trussed = run(&mut s, "roof_check", json!({"id": roof}));
    let n = trussed["framing"]["trusses"].as_u64().unwrap();
    assert!((5..=6).contains(&n), "{trussed}");
    assert!(trussed["framing"]["truss_members"].as_f64().unwrap() > n as f64 * 8.0, "chords and tie span 8 m");
    assert!(trussed["framing"]["volume"].as_f64().unwrap() > check["framing"]["volume"].as_f64().unwrap());
    run(&mut s, "type_set", json!({"type": ty, "params": {"framing": {"system": "Timber"}}}));

    // A schedule lists the tiles and the framing.
    let schedule = run(&mut s, "view_create", json!({"kind": "schedule", "category": "roof"}));
    run(
        &mut s,
        "view_set",
        json!({"view": schedule["id"], "params": {"def": {"fields": ["Type", "TileCount", "Weight", "FramingLength"]}}}),
    );
    let table = run(&mut s, "schedule_table", json!({"view": schedule["id"]})).to_string();
    assert!(table.contains(&format!("{}", (16.0 * area).ceil() as u64)), "{table}");

    // Too flat for French tiles; a flat roof with tiles.
    run(&mut s, "element_set", json!({"id": roof, "params": {"shape": {"Gable": {"slope": 0.2, "ridge_deg": 0}}}}));
    let check = run(&mut s, "roof_check", json!({"id": roof}));
    assert_eq!(check["ok"], false);
    assert_eq!(check["issues"].as_array().unwrap().len(), 1);
    run(&mut s, "element_set", json!({"id": roof, "params": {"shape": "Flat"}}));
    let check = run(&mut s, "roof_check", json!({"id": roof}));
    assert_eq!((check["ok"].clone(), check["framing"]["members"].clone()), (json!(false), json!(0)));

    // Fiber cement on steel purlins: purlins only.
    run(&mut s, "element_set", json!({"id": roof, "params": {"shape": {"Gable": {"slope": 0.2, "ridge_deg": 0}}}}));
    run(&mut s, "type_set", json!({"type": ty, "params": {"covering": "FiberCement", "framing": {"system": "Steel"}}}));
    let check = run(&mut s, "roof_check", json!({"id": roof}));
    assert_eq!(check["ok"], true, "{check}");
    assert!(check["framing"]["purlins"].as_f64().unwrap() > 0.0);
    assert_eq!((check["framing"]["rafters"].as_f64(), check["framing"]["battens"].as_f64()), (Some(0.0), Some(0.0)));
    assert_eq!(fails(&mut s, "roof_check", json!({"id": ty})), ErrorCode::NotFound);
}
