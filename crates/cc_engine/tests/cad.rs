//! Importing DWG/DXF drawings as underlays through commands: placement,
//! layers, editing, undo, saving, drawing and a large file.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::{ErrorCode, Session};
use cc_views::{LineStyle, Prim};
use glam::DVec2;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ccad-cad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn pairs(groups: &[(i32, &str)]) -> String {
    groups.iter().map(|(c, v)| format!("{c:>3}\n{v}\n")).collect()
}

/// A millimeter drawing: a 4 m line on `Walls`, a quarter arc, a bulged
/// polyline, a text, a line on the frozen layer `Notes`, and block `B`
/// (a 1 m line from its base) inserted turned 90° and scaled 2.
fn sample_dxf() -> String {
    let mut s = pairs(&[(0, "SECTION"), (2, "HEADER"), (9, "$INSUNITS"), (70, "4"), (0, "ENDSEC")]);
    s += &pairs(&[(0, "SECTION"), (2, "TABLES"), (0, "TABLE"), (2, "LAYER")]);
    s += &pairs(&[(0, "LAYER"), (2, "0"), (70, "0"), (62, "7")]);
    s += &pairs(&[(0, "LAYER"), (2, "Walls"), (70, "0"), (62, "1")]);
    s += &pairs(&[(0, "LAYER"), (2, "Notes"), (70, "1"), (62, "3")]);
    s += &pairs(&[(0, "ENDTAB"), (0, "ENDSEC"), (0, "SECTION"), (2, "BLOCKS")]);
    s += &pairs(&[(0, "BLOCK"), (2, "B"), (70, "0"), (10, "500"), (20, "0"), (30, "0")]);
    s += &pairs(&[(0, "LINE"), (8, "0"), (10, "500"), (20, "0"), (30, "0"), (11, "1500"), (21, "0"), (31, "0")]);
    s += &pairs(&[(0, "ENDBLK"), (0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")]);
    s += &pairs(&[(0, "LINE"), (8, "Walls"), (10, "0"), (20, "0"), (30, "0"), (11, "4000"), (21, "0"), (31, "0")]);
    s += &pairs(&[(0, "ARC"), (8, "Walls"), (10, "0"), (20, "0"), (30, "0"), (40, "1000"), (50, "0"), (51, "90")]);
    s += &pairs(&[
        (0, "LWPOLYLINE"),
        (8, "Walls"),
        (90, "2"),
        (70, "0"),
        (10, "0"),
        (20, "3000"),
        (42, "1"),
        (10, "2000"),
        (20, "3000"),
    ]);
    s += &pairs(&[(0, "TEXT"), (8, "0"), (10, "100"), (20, "200"), (30, "0"), (40, "250"), (1, "Sala")]);
    s += &pairs(&[(0, "LINE"), (8, "Notes"), (10, "0"), (20, "-500"), (30, "0"), (11, "100"), (21, "-500"), (31, "0")]);
    s += &pairs(&[
        (0, "INSERT"),
        (8, "Walls"),
        (2, "B"),
        (10, "6000"),
        (20, "0"),
        (30, "0"),
        (41, "2"),
        (42, "2"),
        (50, "90"),
    ]);
    s += &pairs(&[(0, "3DSOLID"), (8, "0")]);
    s += &pairs(&[(0, "ENDSEC"), (0, "EOF")]);
    s
}

fn plan(s: &mut Session) -> u64 {
    let views = run(s, "views_list", json!({"category": "floor_plan"}));
    views[0]["id"].as_u64().unwrap()
}

/// Lines of element `id` in the view's drawing.
fn lines_of(s: &mut Session, view: u64, id: u64) -> Vec<(DVec2, DVec2)> {
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    d.prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { a, b, element: Some(e), .. } if e.0 == id => Some((*a, *b)),
            _ => None,
        })
        .collect()
}

fn has_line(lines: &[(DVec2, DVec2)], a: DVec2, b: DVec2) -> bool {
    lines
        .iter()
        .any(|(p, q)| (p.distance(a) < 1e-9 && q.distance(b) < 1e-9) || (p.distance(b) < 1e-9 && q.distance(a) < 1e-9))
}

#[test]
fn import_place_edit_and_undo() {
    let path = scratch("sample.dxf");
    std::fs::write(&path, sample_dxf()).unwrap();
    let mut s = Session::default();
    let view = plan(&mut s);
    let out = run(&mut s, "cad_import", json!({"path": path, "view": view, "position": [10.0, 20.0]}));
    let id = out["id"].as_u64().unwrap();
    assert_eq!(out["format"], "dxf");
    assert_eq!(out["units"], "mm");
    assert_eq!(out["units_assumed"], false);
    assert_eq!(out["entities"], 7);
    assert_eq!(out["blocks"], 1);
    assert_eq!(out["layers"], 3);
    assert_eq!(out["skipped"]["3DSOLID"], 1);
    assert_eq!(out["truncated"], false);

    let e = run(&mut s, "element_get", json!({"id": id}));
    assert_eq!(e["category"], "cadimport");
    assert_eq!(e["pinned"], true, "imported drawings start pinned");
    assert!(e["params"].get("drawing").is_none(), "the drawing is not a parameter: {}", e["params"]);
    assert_eq!(e["params"]["name"], "sample.dxf");
    assert_eq!(e["derived"]["primitives"], 6);

    // The 4 m line, millimeters to meters, at the insertion point.
    let lines = lines_of(&mut s, view, id);
    assert!(has_line(&lines, DVec2::new(10.0, 20.0), DVec2::new(14.0, 20.0)), "{lines:?}");
    // The block line: (500,0)-(1500,0) less the base, scaled 2, turned 90°, at (6000, 0) mm.
    assert!(has_line(&lines, DVec2::new(16.0, 20.0), DVec2::new(16.0, 22.0)), "{lines:?}");
    // The frozen layer is hidden.
    assert!(!lines.iter().any(|(a, _)| (a.y - 19.5).abs() < 1e-9));
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    assert!(d.background.contains(&cc_core::ElementId(id)));
    assert!(
        matches!(&d.prims[0], Prim::Line { style: LineStyle::Thin, element: Some(e), .. } if e.0 == id),
        "drawn first, under the model"
    );
    let text = d.prims.iter().find_map(|p| match p {
        Prim::Text { text, size, element: Some(e), .. } if e.0 == id => Some((text.clone(), *size)),
        _ => None,
    });
    let (text, size) = text.unwrap();
    assert_eq!(text, "Sala");
    let scale = run(&mut s, "view_get", json!({"view": view}))["scale"].as_f64().unwrap();
    assert!((size - 0.25 * 1000.0 / scale).abs() < 1e-9);

    // Layers.
    let layers = run(&mut s, "cad_layers", json!({"id": id}));
    let notes = layers.as_array().unwrap().iter().find(|l| l["name"] == "Notes").unwrap();
    assert_eq!((notes["visible"].clone(), notes["on_in_file"].clone()), (json!(false), json!(false)));
    run(&mut s, "cad_layers_set", json!({"id": id, "layers": ["notes"], "visible": true}));
    let lines = lines_of(&mut s, view, id);
    assert!(has_line(&lines, DVec2::new(10.0, 19.5), DVec2::new(10.1, 19.5)));
    run(&mut s, "cad_layers_set", json!({"id": id, "visible": false}));
    assert!(lines_of(&mut s, view, id).is_empty());
    assert_eq!(
        fails(&mut s, "cad_layers_set", json!({"id": id, "layers": ["Nope"], "visible": true})),
        ErrorCode::NotFound
    );
    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));

    // Placement through element_set: meters → the 4 m line becomes 4 km; then scale and rotation.
    run(&mut s, "element_set", json!({"id": id, "params": {"units": "Centimeters", "rotation_deg": 90.0}}));
    let lines = lines_of(&mut s, view, id);
    assert!(has_line(&lines, DVec2::new(10.0, 20.0), DVec2::new(10.0, 60.0)), "{lines:?}");
    assert_ne!(fails(&mut s, "element_set", json!({"id": id, "params": {"scale": -1.0}})), ErrorCode::Internal);
    assert_ne!(fails(&mut s, "element_set", json!({"id": id, "params": {"drawing": null}})), ErrorCode::Internal);
    run(&mut s, "undo", json!({}));

    // Pinned: moving skips it until unpinned.
    let moved = run(&mut s, "elements_move", json!({"ids": [id], "delta": [1.0, 0.0]}));
    assert_eq!(moved["pinned_skipped"], json!([id]));
    run(&mut s, "elements_pin", json!({"ids": [id], "pinned": false}));
    run(&mut s, "elements_move", json!({"ids": [id], "delta": [1.0, 0.0]}));
    run(&mut s, "elements_rotate", json!({"ids": [id], "center": [11.0, 20.0], "angle": 90.0}));
    let lines = lines_of(&mut s, view, id);
    assert!(has_line(&lines, DVec2::new(11.0, 20.0), DVec2::new(11.0, 24.0)), "{lines:?}");

    // Undo back to before the import removes it in one step.
    while s.doc.project.element(cc_core::ElementId(id)).is_some() {
        run(&mut s, "undo", json!({}));
    }
    assert!(s.doc.project.types.values().all(|t| t.category() != cc_core::Category::CadImport));
}

#[test]
fn only_2d_views_with_a_plane_take_drawings() {
    let path = scratch("views.dxf");
    std::fs::write(&path, sample_dxf()).unwrap();
    let mut s = Session::default();
    let three_d = run(&mut s, "views_list", json!({"category": "three_d"}));
    let three_d = three_d[0]["id"].as_u64().unwrap();
    assert_eq!(fails(&mut s, "cad_import", json!({"path": path, "view": three_d})), ErrorCode::InvalidArguments);
    let elevation = run(&mut s, "views_list", json!({"category": "elevation"}))[0]["id"].as_u64().unwrap();
    let out = run(
        &mut s,
        "cad_import",
        json!({"path": path, "view": elevation, "units": "m", "align": "center", "position": [0.0, 0.0]}),
    );
    assert_eq!(out["units"], "m");
    // Centered: the extents straddle the given point.
    let ext = &out["extents"];
    let (lo, hi) = (ext[0][0].as_f64().unwrap(), ext[1][0].as_f64().unwrap());
    assert!((lo + hi).abs() < 1e-9, "{ext}");
    // Not in the plan of the same project.
    let plan = plan(&mut s);
    let id = out["id"].as_u64().unwrap();
    assert!(lines_of(&mut s, plan, id).is_empty());
    assert!(!lines_of(&mut s, elevation, id).is_empty());
    // Bad files are errors.
    let bad = scratch("bad.dxf");
    std::fs::write(&bad, "hello").unwrap();
    assert_eq!(fails(&mut s, "cad_import", json!({"path": bad, "view": plan})), ErrorCode::InvalidArguments);
    let empty = scratch("empty.dxf");
    std::fs::write(&empty, pairs(&[(0, "SECTION"), (2, "ENTITIES"), (0, "ENDSEC"), (0, "EOF")])).unwrap();
    assert_eq!(fails(&mut s, "cad_import", json!({"path": empty, "view": plan})), ErrorCode::InvalidArguments);
    assert_ne!(fails(&mut s, "cad_import", json!({"path": scratch("missing.dxf"), "view": plan})), ErrorCode::Internal);
}

#[test]
fn saved_with_the_project_and_hidden_by_category() {
    let path = scratch("saved.dxf");
    std::fs::write(&path, sample_dxf()).unwrap();
    let mut s = Session::default();
    let view = plan(&mut s);
    let id = run(&mut s, "cad_import", json!({"path": path, "view": view}))["id"].as_u64().unwrap();
    let file = scratch("saved.ccad");
    run(&mut s, "project_save", json!({"path": file}));
    let mut reopened = Session::default();
    let out = run(&mut reopened, "project_open", json!({"path": file}));
    assert_eq!(out["repairs"], 0);
    let e = run(&mut reopened, "element_get", json!({"id": id}));
    assert_eq!(e["derived"]["primitives"], 6);
    assert_eq!(lines_of(&mut reopened, view, id), lines_of(&mut s, view, id));
    // The category can be hidden in a view's graphics.
    run(&mut s, "view_graphics_set", json!({"view": view, "category": "cadimport", "visible": false}));
    assert!(lines_of(&mut s, view, id).is_empty());
}

/// A drawing of 50 000 entities imports and draws in reasonable time.
#[test]
fn large_drawing_imports_quickly() {
    let mut s = pairs(&[(0, "SECTION"), (2, "ENTITIES")]);
    for i in 0..50_000 {
        let (x, y) = ((i % 250) as f64 * 100.0, (i / 250) as f64 * 100.0);
        if i % 5 == 0 {
            s += &format!("  0\nCIRCLE\n  8\nC\n 10\n{x}\n 20\n{y}\n 30\n0\n 40\n30\n");
        } else {
            s += &format!(
                "  0\nLINE\n  8\nL{}\n 10\n{x}\n 20\n{y}\n 30\n0\n 11\n{}\n 21\n{}\n 31\n0\n",
                i % 7,
                x + 80.0,
                y + 40.0
            );
        }
    }
    s += &pairs(&[(0, "ENDSEC"), (0, "EOF")]);
    let path = scratch("large.dxf");
    std::fs::write(&path, s).unwrap();
    let mut session = Session::default();
    let view = plan(&mut session);
    let start = std::time::Instant::now();
    let out = run(&mut session, "cad_import", json!({"path": path, "view": view, "units": "mm"}));
    let import = start.elapsed();
    assert_eq!(out["primitives"], 50_000);
    let start = std::time::Instant::now();
    let d = session.drawing(cc_core::ViewId(view)).unwrap();
    let draw = start.elapsed();
    assert!(d.prims.len() > 50_000);
    // Generous bounds for unoptimized test builds on slow machines.
    assert!(import.as_secs_f64() < 30.0, "import took {import:?}");
    assert!(draw.as_secs_f64() < 30.0, "drawing took {draw:?}");
    // A model edit elsewhere does not copy the drawing (it is shared).
    let start = std::time::Instant::now();
    run(&mut session, "wall_create", json!({"start": [0.0, -5.0], "end": [5.0, -5.0]}));
    assert!(start.elapsed().as_secs_f64() < 30.0);
    eprintln!("import {import:?}, drawing {draw:?}");
}

/// Deleting a view deletes what it owns (imported drawings, texts) in the same step.
#[test]
fn deleting_the_view_deletes_its_drawings() {
    let path = scratch("owned.dxf");
    std::fs::write(&path, sample_dxf()).unwrap();
    let mut s = Session::default();
    let view = run(&mut s, "view_create", json!({"kind": "legend", "name": "CAD"}))["id"].as_u64().unwrap();
    let id = run(&mut s, "cad_import", json!({"path": path, "view": view}))["id"].as_u64().unwrap();
    let text = run(&mut s, "text_create", json!({"view": view, "position": [0.0, 0.0], "text": "Note"}));
    run(&mut s, "view_delete", json!({"view": view}));
    assert!(s.doc.project.element(cc_core::ElementId(id)).is_none());
    assert!(s.doc.project.element(cc_core::ElementId(text["id"].as_u64().unwrap())).is_none());
    run(&mut s, "undo", json!({}));
    assert!(s.doc.project.element(cc_core::ElementId(id)).is_some());
}

/// A drawing in survey coordinates is centered on the view with `auto`.
#[test]
fn auto_alignment_centers_far_drawings() {
    let far = pairs(&[(0, "SECTION"), (2, "ENTITIES")])
        + &pairs(&[(0, "LINE"), (8, "0"), (10, "500000"), (20, "7000000"), (11, "500010"), (21, "7000000")])
        + &pairs(&[(0, "ENDSEC"), (0, "EOF")]);
    let path = scratch("far.dxf");
    std::fs::write(&path, far).unwrap();
    let mut s = Session::default();
    let view = plan(&mut s);
    let out = run(&mut s, "cad_import", json!({"path": path, "view": view, "units": "m", "align": "auto"}));
    let ext = &out["extents"];
    assert!(ext[0][0].as_f64().unwrap().abs() < 1000.0, "{ext}");
    let near = scratch("near.dxf");
    std::fs::write(&near, sample_dxf()).unwrap();
    let out = run(&mut s, "cad_import", json!({"path": near, "view": view, "align": "auto"}));
    let e = run(&mut s, "element_get", json!({"id": out["id"]}));
    assert_eq!(e["params"]["position"], json!([0.0, 0.0]), "a drawing near the origin stays there");
}

#[test]
fn the_properties_panel_patch_with_every_param_applies() {
    // The app sends back all of `element_get`'s params, the view included.
    let path = scratch("panel.dxf");
    std::fs::write(&path, sample_dxf()).unwrap();
    let mut s = Session::default();
    let view = plan(&mut s);
    let id = run(&mut s, "cad_import", json!({"path": path, "view": view}))["id"].as_u64().unwrap();
    let mut params = run(&mut s, "element_get", json!({"id": id}))["params"].clone();
    params["units"] = json!("Centimeters");
    run(&mut s, "element_set", json!({"id": id, "params": params}));
    assert_eq!(run(&mut s, "element_get", json!({"id": id}))["params"]["units"], "Centimeters");
    params["view"] = json!(view + 1000);
    assert_ne!(fails(&mut s, "element_set", json!({"id": id, "params": params})), ErrorCode::Internal);

    // Pinned on import: deleting skips it until it is unpinned.
    let out = run(&mut s, "elements_delete", json!({"ids": [id]}));
    assert_eq!(out["pinned_skipped"], json!([id]));
    run(&mut s, "elements_pin", json!({"ids": [id], "pinned": false}));
    run(&mut s, "elements_delete", json!({"ids": [id]}));
    assert!(s.doc.project.element(cc_core::ElementId(id)).is_none());
}

/// Issue #27: a plan exported as DXF and imported back lies where the view
/// drew it, at full size, each element on the layer of its category.
#[test]
fn views_export_as_dxf_and_read_back() {
    let mut s = Session::default();
    let view = plan(&mut s);
    let wall = run(&mut s, "wall_create", json!({"start": [0, 0], "end": [5, 0]}))["id"].as_u64().unwrap();
    let wall_lines = lines_of(&mut s, view, wall);
    assert!(!wall_lines.is_empty());
    let path = scratch("plan.dxf");
    let out = run(&mut s, "view_export", json!({"view": view, "path": path}));
    assert_eq!(out["format"], "dxf");
    let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).to_string();
    assert!(text.contains("AC1009") && text.trim_end().ends_with("EOF"));
    // Read back with the importer: millimeters, at the origin.
    let back = run(&mut s, "cad_import", json!({"path": path, "view": view, "position": [0, 0]}));
    assert_eq!(back["units"], "mm");
    let id = back["id"].as_u64().unwrap();
    let lines = lines_of(&mut s, view, id);
    for (a, b) in &wall_lines {
        let near = |p: &DVec2, q: &DVec2| p.distance(*q) < 1e-6;
        assert!(lines.iter().any(|(p, q)| (near(p, a) && near(q, b)) || (near(p, b) && near(q, a))), "{a} {b}");
    }
    let layers = run(&mut s, "cad_layers", json!({"id": id}));
    assert!(layers.to_string().contains("\"WALL\""), "{layers}");
    assert_eq!(
        fails(&mut s, "view_export", json!({"view": view, "path": scratch("plan.xyz")})),
        ErrorCode::InvalidArguments
    );
}
