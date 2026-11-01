//! Detail levels (issue #24): views and templates set how much of the
//! model they draw; layered walls lose their layers at the coarse level,
//! and component pieces show at the levels they are set for.

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

fn plan(s: &mut Session) -> u64 {
    run(s, "views_list", json!({"category": "floor_plan"}))[0]["id"].as_u64().unwrap()
}

/// The fills and hatch or layer lines element `id` has in the view's drawing.
fn wall_marks(s: &mut Session, view: u64, id: u64) -> (usize, usize) {
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    let mine = |e: &Option<cc_core::ElementId>| *e == Some(cc_core::ElementId(id));
    let fills = d.prims.iter().filter(|p| matches!(p, Prim::Fill { element, cut: true, .. } if mine(element))).count();
    let inner = d
        .prims
        .iter()
        .filter(|p| matches!(p, Prim::Line { style: LineStyle::Hatch | LineStyle::Thin, element, .. } if mine(element)))
        .count();
    (fills, inner)
}

#[test]
fn coarse_views_cut_layered_walls_as_one_region() {
    let mut s = Session::default();
    let view = plan(&mut s);
    let wall = run(&mut s, "wall_create", json!({"start": [0, 0], "end": [5, 0]}));
    let id = wall["id"].as_u64().unwrap();
    // Three layers: plaster, block, plaster.
    let ty = run(&mut s, "type_get", json!({"type": wall["type"]["id"]}));
    let material = ty["params"]["layers"][0]["material"].clone();
    let layer = |t: f64, f: &str| json!({"material": material, "thickness": t, "function": f});
    run(
        &mut s,
        "type_set",
        json!({"type": ty["id"], "params": {"layers": [layer(0.02, "Finish"), layer(0.14, "Structure"), layer(0.02, "Finish")]}}),
    );
    assert_eq!(run(&mut s, "view_get", json!({"view": view}))["detail"], "medium");
    let (fills, inner) = wall_marks(&mut s, view, id);
    assert_eq!(fills, 3, "one fill per layer");
    assert!(inner >= 2, "layer lines between the layers");
    run(&mut s, "view_set", json!({"view": view, "detail": "coarse"}));
    assert_eq!(wall_marks(&mut s, view, id), (1, 0), "one plain region, no layers or patterns");
    // Undo brings the layers back; a bad level is refused.
    run(&mut s, "undo", json!({}));
    assert_eq!(wall_marks(&mut s, view, id).0, 3);
    assert_eq!(fails(&mut s, "view_set", json!({"view": view, "detail": "high"})), ErrorCode::InvalidArguments);
}

#[test]
fn templates_set_the_detail_level_of_their_views() {
    let mut s = Session::default();
    let view = plan(&mut s);
    let t = run(&mut s, "template_create", json!({"name": "Coarse plans", "view": view}))["id"].as_u64().unwrap();
    let out = run(&mut s, "template_set", json!({"template": t, "detail": "coarse"}));
    assert_eq!((out["detail"].clone(), out["include"]["detail"].clone()), (json!("coarse"), json!(true)));
    run(&mut s, "view_template_assign", json!({"views": [view], "template": t}));
    assert_eq!(run(&mut s, "view_get", json!({"view": view}))["detail"], "coarse");
    // Controlled by the template: the view cannot change it, until the template lets go.
    assert_eq!(fails(&mut s, "view_set", json!({"view": view, "detail": "fine"})), ErrorCode::Rejected);
    run(&mut s, "template_set", json!({"template": t, "include": {"detail": false}}));
    run(&mut s, "view_set", json!({"view": view, "detail": "fine"}));
    assert_eq!(run(&mut s, "view_get", json!({"view": view}))["detail"], "fine");
}

#[test]
fn component_pieces_show_at_their_levels() {
    let mut s = Session::default();
    let view = plan(&mut s);
    // A table: a top at every level, a simple block for coarse plans and a
    // detailed leg for fine ones.
    let piece = |name: &str, x: f64, w: f64| {
        json!({"name": name, "shape": {"Box": {
            "x": x.to_string(), "y": "0", "z": "0", "width": w.to_string(), "depth": "0.6", "height": "0.75"
        }}})
    };
    let def = json!({"items": [piece("Top", 0.0, 1.2), piece("Block", 2.0, 0.3), piece("Leg", 3.0, 0.05)]});
    let ty = run(&mut s, "component_create", json!({"name": "Mesa", "def": def}))["id"].clone();
    run(&mut s, "component_piece_set", json!({"type": ty, "index": 1, "detail": ["coarse"]}));
    run(&mut s, "component_piece_set", json!({"type": ty, "index": 2, "detail": ["fine"]}));
    let c = run(&mut s, "component_get", json!({"type": ty}));
    assert_eq!(c["def"]["items"][1]["detail"], json!({"coarse": true, "medium": false, "fine": false}));
    assert!(c["def"]["items"][0].get("detail").is_none(), "all levels is the default");
    let id = run(&mut s, "component_place", json!({"type": ty, "position": [10, 10]}))["id"].as_u64().unwrap();
    // The x of every line of the table in the plan tells which pieces it draws.
    let xs = |s: &mut Session| {
        let d = s.drawing(cc_core::ViewId(view)).unwrap();
        let mut xs: Vec<i64> = d
            .prims
            .iter()
            .filter_map(|p| match p {
                Prim::Line { a, element: Some(e), .. } if e.0 == id => Some((a.x - 10.0).round() as i64),
                _ => None,
            })
            .collect();
        xs.sort_unstable();
        xs.dedup();
        xs
    };
    assert_eq!(xs(&mut s), vec![0, 1], "medium: the top only");
    run(&mut s, "view_set", json!({"view": view, "detail": "coarse"}));
    assert_eq!(xs(&mut s), vec![0, 1, 2], "coarse: the top and the block");
    run(&mut s, "view_set", json!({"view": view, "detail": "fine"}));
    assert_eq!(xs(&mut s), vec![0, 1, 3], "fine: the top and the leg");
    assert_eq!(
        fails(&mut s, "component_piece_set", json!({"type": ty, "index": 2, "detail": ["fine", "best"]})),
        ErrorCode::InvalidArguments
    );
}
