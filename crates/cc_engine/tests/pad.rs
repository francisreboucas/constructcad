//! Building pads: graded into a terrain with cut and fill slopes, shown with
//! their volumes, drawn in plans and sections, listed in schedules, moved
//! and deleted with their terrain.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{ElementId, ViewId};
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

fn square(x0: f64, y0: f64, side: f64) -> Value {
    json!([[x0, y0], [x0 + side, y0], [x0 + side, y0 + side], [x0, y0 + side]])
}

/// The plane z = x / 10 over a 40 x 40 m lot.
fn terrain(s: &mut Session) -> u64 {
    let points = json!([[0.0, 0.0, 0.0], [40.0, 0.0, 4.0], [40.0, 40.0, 4.0], [0.0, 40.0, 0.0], [20.0, 20.0, 2.0]]);
    id(&run(s, "topography_create", json!({"points": points, "boundary": square(0.0, 0.0, 40.0)})))
}

fn pad(s: &mut Session, id: u64) -> Value {
    run(s, "element_get", json!({"id": id}))["derived"]["pad"].clone()
}

#[test]
fn a_pad_grades_its_terrain_with_one_undo_step() {
    let mut s = Session::default();
    let t = terrain(&mut s);
    let depth = s.doc.undo_depth();
    // A 10 x 10 m pad at 2 m, where the ground runs from 1.5 to 2.5: some cut, some fill.
    let p = id(&run(&mut s, "pad_create", json!({"outline": square(15.0, 15.0, 10.0), "elevation": 2.0})));
    assert_eq!(s.doc.undo_depth(), depth + 1);
    let e = run(&mut s, "element_get", json!({"id": p}));
    assert_eq!(e["category"], json!("pad"), "{e}");
    assert_eq!(e["params"]["host"], json!(t), "{e}");
    let g = pad(&mut s, p);
    assert!(num(&g["cut"]) > 0.0 && num(&g["fill"]) > 0.0, "{g}");
    assert!((num(&g["area"]) - 100.0).abs() < 1e-9);
    assert!((num(&g["net"]) - (num(&g["fill"]) - num(&g["cut"]))).abs() < 1e-12);
    assert_eq!(g["warnings"], json!([]));
    // The plane is symmetric about the pad: cut and fill nearly balance
    // (the fill slope is gentler, so it holds more).
    assert!(num(&g["fill"]) > num(&g["cut"]));
    // The terrain sums its pads.
    let terrain_figures = run(&mut s, "element_get", json!({"id": t}))["derived"]["topography"].clone();
    assert!((num(&terrain_figures["cut"]) - num(&g["cut"])).abs() < 1e-9, "{terrain_figures}");

    // Lower, it cuts more and fills less.
    run(&mut s, "element_set", json!({"id": p, "params": {"elevation": 1.0}}));
    let lower = pad(&mut s, p);
    assert!(num(&lower["cut"]) > num(&g["cut"]) && num(&lower["fill"]) < num(&g["fill"]), "{lower}");
    // Steeper own slopes move less earth.
    run(&mut s, "element_set", json!({"id": p, "params": {"cut_slope": 0.5, "fill_slope": 0.5}}));
    let steep = pad(&mut s, p);
    assert!(num(&steep["cut"]) < num(&lower["cut"]) && num(&steep["fill"]) <= num(&lower["fill"]), "{steep} {lower}");
    // A ratio out of range is refused.
    assert_ne!(fails(&mut s, "element_set", json!({"id": p, "params": {"cut_slope": 0.0}})), ErrorCode::Internal);
    // Back to the type's slopes; the type's ratios then count.
    run(&mut s, "element_set", json!({"id": p, "params": {"cut_slope": null, "fill_slope": null}}));
    let ty = run(&mut s, "element_get", json!({"id": p}))["type"]["id"].as_u64().unwrap();
    run(&mut s, "type_set", json!({"type": ty, "params": {"cut_slope": 3.0}}));
    assert!(num(&pad(&mut s, p)["cut"]) > num(&lower["cut"]));

    // Its outline redraws through the sketch commands.
    let curves = json!([{"kind": "rectangle", "corner": [16.0, 16.0], "opposite": [24.0, 24.0]}]);
    run(&mut s, "sketch_set", json!({"id": p, "curves": curves}));
    assert!((num(&pad(&mut s, p)["area"]) - 64.0).abs() < 1e-9);
    assert!(run(&mut s, "sketch_get", json!({"id": p}))["curves"].as_array().is_some_and(|c| !c.is_empty()));
}

#[test]
fn hosts_are_found_or_refused() {
    let mut s = Session::default();
    // No terrain under the outline.
    let args = json!({"outline": square(0.0, 0.0, 5.0), "elevation": 0.0});
    assert_eq!(fails(&mut s, "pad_create", args.clone()), ErrorCode::InvalidArguments);
    let t = terrain(&mut s);
    // Off the terrain, or a host that is not one.
    let off = json!({"outline": square(100.0, 100.0, 5.0), "elevation": 0.0, "host": t});
    assert_eq!(fails(&mut s, "pad_create", off), ErrorCode::InvalidArguments);
    let wall = id(&run(&mut s, "wall_create", json!({"start": [0.0, 0.0], "end": [5.0, 0.0]})));
    let bad = json!({"outline": square(1.0, 1.0, 5.0), "elevation": 0.0, "host": wall});
    assert_eq!(fails(&mut s, "pad_create", bad), ErrorCode::InvalidArguments);
    let steep = json!({"outline": square(1.0, 1.0, 5.0), "elevation": 0.0, "cut_slope": 1000.0});
    assert_eq!(fails(&mut s, "pad_create", steep), ErrorCode::InvalidArguments);
    assert_eq!(
        fails(&mut s, "pad_create", json!({"outline": square(1.0, 1.0, 5.0), "elevation": "high"})),
        ErrorCode::InvalidArguments
    );
    // Partly over the terrain is fine (it grades the part that is), with a warning.
    let edge = id(&run(&mut s, "pad_create", json!({"outline": square(-2.0, 10.0, 5.0), "elevation": 0.0})));
    assert_eq!(pad(&mut s, edge)["warnings"], json!(["off-terrain"]));
    assert_eq!(run(&mut s, "element_get", json!({"id": edge}))["params"]["host"], json!(t));
    // A type with its own ratios.
    let ty = id(&run(&mut s, "pad_type_create", json!({"name": "Rock", "cut_slope": 0.25, "fill_slope": 1.0})));
    let p = id(&run(&mut s, "pad_create", json!({"outline": square(20.0, 20.0, 5.0), "elevation": 2.0, "type": ty})));
    assert_eq!(run(&mut s, "element_get", json!({"id": p}))["type"]["name"], json!("Rock"));
    assert_eq!(fails(&mut s, "pad_type_create", json!({"fill_slope": 0.01})), ErrorCode::InvalidArguments);
}

#[test]
fn pads_are_drawn_scheduled_and_go_with_their_terrain() {
    let mut s = Session::default();
    let t = terrain(&mut s);
    let a = id(&run(&mut s, "pad_create", json!({"outline": square(5.0, 5.0, 8.0), "elevation": 0.5})));
    let b = id(&run(&mut s, "pad_create", json!({"outline": square(25.0, 25.0, 8.0), "elevation": 3.5})));

    // The plan: outline, elevation, slope ticks and the old ground dashed.
    let plan = run(&mut s, "views_list", json!({"category": "floor_plan"}))[0]["id"].as_u64().unwrap();
    let d = s.drawing(ViewId(plan)).unwrap();
    let of = |id: u64, style: LineStyle| {
        d.prims
            .iter()
            .filter(
                |p| matches!(p, Prim::Line { style: st, element: Some(e), .. } if *e == ElementId(id) && *st == style),
            )
            .count()
    };
    assert!(of(a, LineStyle::Projection) >= 4 && of(a, LineStyle::Thin) > 10, "{}", of(a, LineStyle::Thin));
    assert!(of(t, LineStyle::Hidden) > 0);
    let texts: Vec<&str> = d
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Text { text, element: Some(e), .. } if *e == ElementId(a) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts.len(), 1, "{texts:?}");

    // A section across both pads shows the old ground dashed.
    let section = run(&mut s, "view_create", json!({"kind": "section", "start": [-1.0, 9.0], "end": [41.0, 9.0]}));
    let d = s.drawing(ViewId(id(&section))).unwrap();
    let hidden = d
        .prims
        .iter()
        .filter(|p| matches!(p, Prim::Line { style: LineStyle::Hidden, element: Some(e), .. } if *e == ElementId(t)))
        .count();
    assert!(hidden > 0);

    // A schedule of pads totals their volumes.
    let schedule = run(&mut s, "view_create", json!({"kind": "schedule", "category": "pad"}));
    let table = run(&mut s, "schedule_table", json!({"view": schedule["id"]}));
    assert_eq!(table["rows"].as_array().unwrap().len(), 2, "{table}");
    // Mark, Type, Level, Elevation, Area, Cut Volume, Fill Volume: the volumes and the area total.
    assert_eq!(table["headers"].as_array().map(Vec::len), Some(7), "{table}");
    for column in [4, 5, 6] {
        assert!(
            table["totals"][column]
                .as_str()
                .is_some_and(|t| t.starts_with(|c: char| c.is_ascii_digit()) && !t.starts_with("0,00")),
            "{table}"
        );
    }

    // Moved with the terrain.
    let before = run(&mut s, "element_get", json!({"id": a}))["params"]["outline"][0].clone();
    run(&mut s, "elements_move", json!({"ids": [t], "delta": [1.0, 2.0]}));
    let after = run(&mut s, "element_get", json!({"id": a}))["params"]["outline"][0].clone();
    assert_eq!(after, json!([num(&before[0]) + 1.0, num(&before[1]) + 2.0]));
    // Deleted with it, in one step.
    run(&mut s, "elements_delete", json!({"ids": [t]}));
    for p in [a, b] {
        assert_eq!(fails(&mut s, "element_get", json!({"id": p})), ErrorCode::NotFound);
    }
    run(&mut s, "undo", json!({}));
    assert!(pad(&mut s, b)["fill"].as_f64().is_some());
}
