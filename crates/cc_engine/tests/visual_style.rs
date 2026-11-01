//! Visual styles (issue #25): every view that shows the model draws it in
//! wireframe, hidden line, shaded, consistent colors or textures; templates
//! set the style for the views that follow them.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::{ErrorCode, Session};
use cc_views::Prim;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

fn first_view(s: &mut Session, category: &str) -> u64 {
    run(s, "views_list", json!({"category": category}))[0]["id"].as_u64().unwrap()
}

/// The opaque fills of the model in a view's drawing (room areas, which are
/// invisible, left out), and its lines.
fn marks(s: &mut Session, view: u64) -> (Vec<[u8; 4]>, usize) {
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    let fills = d
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Fill { color, element: Some(_), .. } if color[3] > 0 => Some(*color),
            _ => None,
        })
        .collect();
    let lines = d.prims.iter().filter(|p| matches!(p, Prim::Line { .. })).count();
    (fills, lines)
}

/// A small house: four walls and a floor.
fn house(s: &mut Session) {
    for (a, b) in [([0, 0], [6, 0]), ([6, 0], [6, 4]), ([6, 4], [0, 4]), ([0, 4], [0, 0])] {
        run(s, "wall_create", json!({"start": a, "end": b}));
    }
    run(s, "floor_create", json!({"boundary": [[0, 0], [6, 0], [6, 4], [0, 4]]}));
}

#[test]
fn views_draw_the_model_in_their_style() {
    let mut s = Session::default();
    house(&mut s);
    let plan = first_view(&mut s, "floor_plan");
    let elevation = first_view(&mut s, "elevation");
    // Plans keep their hidden line look; other views are shaded.
    assert_eq!(run(&mut s, "view_get", json!({"view": plan}))["visual_style"], "hidden_line");
    assert_eq!(run(&mut s, "view_get", json!({"view": elevation}))["visual_style"], "shaded");

    // Elevation: the wireframe has no fills and more lines (the far side shows through).
    let (shaded, shaded_lines) = marks(&mut s, elevation);
    assert!(!shaded.is_empty());
    run(&mut s, "view_set", json!({"view": elevation, "visual_style": "wireframe"}));
    let (fills, lines) = marks(&mut s, elevation);
    assert!(fills.is_empty() && lines > shaded_lines, "{lines} vs {shaded_lines}");
    run(&mut s, "view_set", json!({"view": elevation, "visual_style": "hidden_line"}));
    assert!(marks(&mut s, elevation).0.iter().all(|c| c[..3] == [255, 255, 255]));

    // Plan: colored styles lay the faces below the cut (the floor) under the linework.
    let (hidden, _) = marks(&mut s, plan);
    run(&mut s, "view_set", json!({"view": plan, "visual_style": "consistent_colors"}));
    let (colored, _) = marks(&mut s, plan);
    assert!(colored.len() > hidden.len(), "{} vs {}", colored.len(), hidden.len());
    run(&mut s, "view_set", json!({"view": plan, "visual_style": "wireframe"}));
    assert!(marks(&mut s, plan).0.is_empty(), "no fills in the wireframe");
    // Back to the usual style; undo steps through.
    run(&mut s, "view_set", json!({"view": plan, "visual_style": "default"}));
    assert_eq!(run(&mut s, "view_get", json!({"view": plan}))["visual_style"], "hidden_line");
    run(&mut s, "undo", json!({}));
    assert_eq!(run(&mut s, "view_get", json!({"view": plan}))["visual_style"], "wireframe");

    // Refused: an unknown style, a view that does not show the model.
    assert_eq!(fails(&mut s, "view_set", json!({"view": plan, "visual_style": "toon"})), ErrorCode::InvalidArguments);
    let legend = run(&mut s, "view_create", json!({"kind": "legend", "name": "Legend"}));
    assert_eq!(
        fails(&mut s, "view_set", json!({"view": legend["id"], "visual_style": "shaded"})),
        ErrorCode::InvalidArguments
    );
}

#[test]
fn templates_set_the_style_of_their_views() {
    let mut s = Session::default();
    house(&mut s);
    let elevation = first_view(&mut s, "elevation");
    let template = run(&mut s, "template_create", json!({"name": "Presentation", "view": elevation}));
    let set = run(&mut s, "template_set", json!({"template": template["id"], "visual_style": "textures"}));
    assert_eq!(set["visual_style"], "textures");
    assert_eq!(set["include"]["visual_style"], true);
    run(&mut s, "view_template_assign", json!({"views": [elevation], "template": template["id"]}));
    assert_eq!(run(&mut s, "view_get", json!({"view": elevation}))["visual_style"], "textures");
    // The template controls it: the view cannot change it on its own.
    assert_eq!(fails(&mut s, "view_set", json!({"view": elevation, "visual_style": "wireframe"})), ErrorCode::Rejected);
    // Not included, the view chooses again.
    run(&mut s, "template_set", json!({"template": template["id"], "include": {"visual_style": false}}));
    run(&mut s, "view_set", json!({"view": elevation, "visual_style": "wireframe"}));
    assert_eq!(run(&mut s, "view_get", json!({"view": elevation}))["visual_style"], "wireframe");
}
