//! Curtain walls (issue #155): wall types made of a grid of panels and
//! mullions, each wall adjusting its own grid and panels.

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

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

/// A curtain type with a 1 m x 1 m grid, 5 x 10 cm mullions and 2 cm glass.
fn curtain_type(s: &mut Session, name: &str) -> u64 {
    let grid = json!({"layout": "FixedDistance", "spacing": 1.0, "justification": "Start"});
    let mullion = json!({"width": 0.05, "depth": 0.1, "material": ""});
    let curtain = json!({
        "vertical": grid,
        "horizontal": grid,
        "panel": {"kind": "Glass", "thickness": 0.02, "material": ""},
        "vertical_mullion": mullion,
        "horizontal_mullion": mullion,
        "border_mullion": mullion,
    });
    id(&run(s, "curtain_type_create", json!({"name": name, "curtain": curtain})))
}

fn wall(s: &mut Session, ty: u64, start: [f64; 2], end: [f64; 2], height: f64) -> u64 {
    id(&run(s, "wall_create", json!({"start": start, "end": end, "type": ty, "height": height})))
}

#[test]
fn curtain_walls_lay_out_panels_and_mullions() {
    let mut s = Session::default();
    let ty = curtain_type(&mut s, "Glazing 1 m");
    let got = run(&mut s, "type_get", json!({"type": ty}));
    assert_eq!(got["params"]["curtain"]["vertical"]["spacing"], json!(1.0));
    // The type is as thick as its deepest mullion.
    let w = wall(&mut s, ty, [0.0, 0.0], [4.0, 0.0], 3.0);
    let g = run(&mut s, "curtain_get", json!({"element": w}));
    assert_eq!(g["vertical"], json!([1.0, 2.0, 3.0]));
    assert_eq!(g["horizontal"], json!([1.0, 2.0]));
    assert_eq!((g["columns"].clone(), g["rows"].clone(), g["panel_count"].clone()), (json!(4), json!(3), json!(12)));
    // Glazed area: each panel stops at the mullions (border 5 cm, interior 2.5 cm).
    let side = |n: usize, len: f64| len - 2.0 * 0.05 - (n - 1) as f64 * 0.05;
    let glazed = side(4, 4.0) * side(3, 3.0);
    assert!((num(&g["glazed_area"]) - glazed).abs() < 1e-6, "{g}");
    // Mullions: borders (2 x 3 + 2 x 4 less the corners) and interior lines.
    let length = num(&g["mullion_length"]);
    assert!(length > 2.0 * 3.0 + 3.0 * 2.9 && length < 30.0, "{length}");
    // Its volume is the panels' and mullions'.
    let el = run(&mut s, "element_get", json!({"id": w}));
    let volume = num(&el["derived"]["volume"]);
    assert!(volume > glazed * 0.02 && volume < glazed * 0.02 + length * 0.05 * 0.1 + 1e-6, "{volume}");

    // A schedule of walls lists panels, glazed area and mullion length.
    let schedule = run(&mut s, "view_create", json!({"kind": "schedule", "category": "wall"}));
    let fields = json!(["Type", "PanelCount", "GlazedArea", "MullionLength"]);
    run(&mut s, "view_set", json!({"view": schedule["id"], "params": {"def": {"fields": fields}}}));
    let table = run(&mut s, "schedule_table", json!({"view": schedule["id"]})).to_string();
    assert!(table.contains("Glazing 1 m") && table.contains("12"), "{table}");

    // The plan cuts the mullions and draws a line along each glass panel.
    let plan = run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]["id"].as_u64().unwrap();
    let d = s.drawing(cc_core::ViewId(plan)).unwrap();
    let mine = |e: &Option<cc_core::ElementId>| *e == Some(cc_core::ElementId(w));
    let cut =
        d.prims.iter().filter(|p| matches!(p, Prim::Line { style: LineStyle::Cut, element, .. } if mine(element)));
    // Five vertical mullions of four sides each and four panel lines.
    assert_eq!(cut.count(), 5 * 4 + 4);

    // Changing the type's spacing moves every wall's lines.
    run(&mut s, "type_set", json!({"type": ty, "params": {"curtain": {"vertical": {"spacing": 2.0}}}}));
    let g = run(&mut s, "curtain_get", json!({"element": w}));
    assert_eq!(g["vertical"], json!([2.0]));
    // Null takes mullions out (a merge patch drops the key: it must not come back as the default).
    let pieces = g["mullions"].as_u64().unwrap();
    run(&mut s, "type_set", json!({"type": ty, "params": {"curtain": {"border_mullion": null}}}));
    let got = run(&mut s, "type_get", json!({"type": ty}));
    assert_eq!(got["params"]["curtain"]["border_mullion"], Value::Null);
    assert!(got["params"]["curtain"]["vertical_mullion"].is_object());
    let g = run(&mut s, "curtain_get", json!({"element": w}));
    assert!(g["mullions"].as_u64().unwrap() < pieces, "{g}");
    let none = json!({"name": "Butt glazed", "curtain": {"vertical_mullion": null, "horizontal_mullion": null}});
    let bare = id(&run(&mut s, "curtain_type_create", none));
    let got = run(&mut s, "type_get", json!({"type": bare}));
    assert_eq!(got["params"]["curtain"]["vertical_mullion"], Value::Null);
    assert!(got["params"]["curtain"]["border_mullion"].is_object());
    // Bad values are refused.
    let bad = json!({"type": ty, "params": {"curtain": {"vertical": {"spacing": 0.0}}}});
    assert_eq!(fails(&mut s, "type_set", bad), ErrorCode::Rejected);
    let bad = json!({"name": "x", "curtain": {"border_mullion": {"width": -1.0}}});
    assert_eq!(fails(&mut s, "curtain_type_create", bad), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "curtain_type_create", json!({"name": "Glazing 1 m"})), ErrorCode::InvalidArguments);
    // Other walls are not curtain walls.
    let plain = id(&run(&mut s, "wall_create", json!({"start": [0.0, 5.0], "end": [4.0, 5.0]})));
    assert_eq!(fails(&mut s, "curtain_get", json!({"element": plain})), ErrorCode::InvalidArguments);
}

#[test]
fn walls_adjust_their_own_grid() {
    let mut s = Session::default();
    let ty = curtain_type(&mut s, "Grid");
    let w = wall(&mut s, ty, [0.0, 0.0], [4.0, 0.0], 3.0);
    let grid = |s: &mut Session| run(s, "curtain_get", json!({"element": w}));

    // Add, move and remove vertical lines: the wall keeps its own.
    run(&mut s, "curtain_grid_set", json!({"element": w, "axis": "Vertical", "add": [0.5]}));
    assert_eq!(grid(&mut s)["vertical"], json!([0.5, 1.0, 2.0, 3.0]));
    run(&mut s, "curtain_grid_set", json!({"element": w, "axis": "Vertical", "move": [[1, 1.5]], "remove": [3]}));
    assert_eq!(grid(&mut s)["vertical"], json!([0.5, 1.5, 2.0]));
    // Type changes no longer move them.
    run(&mut s, "type_set", json!({"type": ty, "params": {"curtain": {"vertical": {"spacing": 2.0}}}}));
    assert_eq!(grid(&mut s)["vertical"], json!([0.5, 1.5, 2.0]));
    // Horizontal lines are heights above the base.
    run(&mut s, "curtain_grid_set", json!({"element": w, "axis": "Horizontal", "lines": [2.2]}));
    let g = grid(&mut s);
    assert_eq!((g["horizontal"].clone(), g["rows"].clone()), (json!([2.2]), json!(2)));

    // Removing the lower piece of the line at 1.5 merges the cells beside it.
    let before = num(&grid(&mut s)["panel_count"]);
    run(&mut s, "curtain_grid_set", json!({"element": w, "axis": "Vertical", "remove_segments": [[1, 0]]}));
    let g = grid(&mut s);
    assert_eq!(num(&g["panel_count"]), before - 1.0);
    let merged = g["panels"].as_array().unwrap().iter().find(|p| p["column"] == 1 && p["row"] == 0).unwrap().clone();
    assert!(num(&merged["u"][1]) > 1.9, "the panel runs past the removed piece: {merged}");
    // Moving the line keeps the piece removed; putting it back splits the panel again.
    run(&mut s, "curtain_grid_set", json!({"element": w, "axis": "Vertical", "move": [[1, 1.2]]}));
    assert_eq!(num(&grid(&mut s)["panel_count"]), before - 1.0);
    run(&mut s, "curtain_grid_set", json!({"element": w, "axis": "Vertical", "restore_segments": [[1, 0]]}));
    assert_eq!(num(&grid(&mut s)["panel_count"]), before);

    // Back to the type's lines.
    run(&mut s, "curtain_grid_set", json!({"element": w, "axis": "Vertical", "reset": true}));
    assert_eq!(grid(&mut s)["vertical"], json!([2.0]));
    // Each change is one undo step.
    run(&mut s, "undo", json!({}));
    assert_eq!(grid(&mut s)["vertical"], json!([0.5, 1.2, 2.0]));

    // Out of range or unknown lines are refused.
    let out = json!({"element": w, "axis": "Vertical", "add": [4.0]});
    assert_eq!(fails(&mut s, "curtain_grid_set", out), ErrorCode::InvalidArguments);
    let unknown = json!({"element": w, "axis": "Horizontal", "remove": [5]});
    assert_eq!(fails(&mut s, "curtain_grid_set", unknown), ErrorCode::InvalidArguments);
    let both = json!({"element": w, "axis": "Vertical", "reset": true, "add": [1.0]});
    assert_eq!(fails(&mut s, "curtain_grid_set", both), ErrorCode::InvalidArguments);
    let nothing = json!({"element": w, "axis": "Vertical"});
    assert_eq!(fails(&mut s, "curtain_grid_set", nothing), ErrorCode::InvalidArguments);

    // Mirrored, the wall walks the other way and its lines follow.
    run(&mut s, "elements_mirror", json!({"ids": [w], "axis": [[2.0, -1.0], [2.0, 1.0]], "copy": false}));
    let g = grid(&mut s);
    assert_eq!(g["vertical"], json!([2.0, 2.8, 3.5]), "{g}");
}

#[test]
fn walls_swap_their_panels() {
    let mut s = Session::default();
    let ty = curtain_type(&mut s, "Panels");
    let w = wall(&mut s, ty, [0.0, 0.0], [3.0, 0.0], 3.0);
    let panel = |s: &mut Session, c: u64, r: u64| {
        let g = run(s, "curtain_get", json!({"element": w}));
        g["panels"].as_array().unwrap().iter().find(|p| p["column"] == c && p["row"] == r).unwrap().clone()
    };
    run(&mut s, "curtain_panel_set", json!({"element": w, "cells": [[0, 0], [1, 0]], "panel": {"kind": "Solid"}}));
    let p = panel(&mut s, 0, 0);
    assert_eq!((p["kind"].clone(), p["swapped"].clone()), (json!("solid"), json!(true)));
    assert_eq!(p["thickness"], json!(0.02), "the type panel's thickness by default");
    run(&mut s, "curtain_panel_set", json!({"element": w, "cells": [[2, 2]], "panel": {"kind": "Empty"}}));
    let g = run(&mut s, "curtain_get", json!({"element": w}));
    assert_eq!(g["panel_count"], json!(8));
    // Back to the type's panel.
    run(&mut s, "curtain_panel_set", json!({"element": w, "cells": [[0, 0]], "panel": null}));
    assert_eq!(panel(&mut s, 0, 0)["kind"], json!("glass"));
    assert_eq!(panel(&mut s, 1, 0)["kind"], json!("solid"));

    // A door panel: the template's door component fills the cell.
    let doors = run(&mut s, "types_list", json!({"category": "door"}));
    let door = doors.as_array().unwrap()[0]["id"].as_u64().unwrap();
    run(
        &mut s,
        "curtain_panel_set",
        json!({"element": w, "cells": [[1, 0]], "panel": {"kind": "Component", "component": door}}),
    );
    let p = panel(&mut s, 1, 0);
    assert_eq!((p["kind"].clone(), p["component"].clone()), (json!("component"), json!(door)));
    // A type a panel uses cannot be deleted.
    assert_eq!(fails(&mut s, "type_delete", json!({"type": door})), ErrorCode::Rejected);
    // Panels must be components, cells must exist.
    let wall_type = run(&mut s, "types_list", json!({"category": "wall"}))[0]["id"].as_u64().unwrap();
    let bad = json!({"element": w, "cells": [[0, 1]], "panel": {"kind": "Component", "component": wall_type}});
    assert_eq!(fails(&mut s, "curtain_panel_set", bad), ErrorCode::InvalidArguments);
    let bad = json!({"element": w, "cells": [[9, 0]], "panel": {"kind": "Empty"}});
    assert_eq!(fails(&mut s, "curtain_panel_set", bad), ErrorCode::InvalidArguments);
}

#[test]
fn curtain_walls_open_the_walls_they_stand_in() {
    let mut s = Session::default();
    let ty = curtain_type(&mut s, "Storefront");
    let host = id(&run(&mut s, "wall_create", json!({"start": [0.0, 0.0], "end": [10.0, 0.0], "height": 3.0})));
    let volume = |s: &mut Session, id: u64| num(&run(s, "element_get", json!({"id": id}))["derived"]["volume"]);
    let whole = volume(&mut s, host);
    let curtain = wall(&mut s, ty, [3.0, 0.0], [6.0, 0.0], 2.5);
    let thickness = whole / (10.0 * 3.0);
    let opened = volume(&mut s, host);
    assert!((whole - opened - 3.0 * 2.5 * thickness).abs() < 1e-6, "{whole} -> {opened}");
    // The curtain wall stays square (it joins nothing) and keeps its own grid.
    let g = run(&mut s, "curtain_get", json!({"element": curtain}));
    assert_eq!((g["start"].clone(), g["end"].clone()), (json!(0.0), json!(3.0)));
    // Moved out of the wall, the opening closes.
    run(&mut s, "elements_move", json!({"ids": [curtain], "delta": [0.0, 2.0]}));
    assert!((volume(&mut s, host) - whole).abs() < 1e-9);
}

#[test]
fn arc_curtain_walls_have_flat_panels_on_the_arc() {
    let mut s = Session::default();
    let ty = curtain_type(&mut s, "Round");
    // A half circle of radius 4 around the origin.
    let w = id(&run(&mut s, "wall_create", json!({"start": [4.0, 0.0], "end": [-4.0, 0.0], "bulge": 1.0, "type": ty})));
    let g = run(&mut s, "curtain_get", json!({"element": w}));
    let length = num(&g["end"]) - num(&g["start"]);
    assert!((length - 4.0 * std::f64::consts::PI).abs() < 1e-6, "{g}");
    assert_eq!(g["columns"], json!(13), "a line every meter along the arc");
    let geom = s.geometry();
    let c = geom.walls[&cc_core::ElementId(w)].curtain.as_ref().unwrap();
    for p in &c.panels {
        // Each panel's ends lie on the arc: chords, not curves.
        for q in [p.start, p.end] {
            assert!((q.length() - 4.0).abs() < 1e-9, "{q}");
        }
    }
}
