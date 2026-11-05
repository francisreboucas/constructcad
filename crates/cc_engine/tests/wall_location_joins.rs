//! Walls drawn on a face or core line join at their corners like walls drawn
//! on their centerline (issue 165).

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

/// The template's 25 cm wall type: 2.5 cm finish, 20 cm core, 2.5 cm finish.
fn layered_type(s: &mut Session) -> u64 {
    let types = run(s, "types_list", json!({"category": "wall"}));
    for t in types.as_array().unwrap() {
        let id = t["id"].as_u64().unwrap();
        let layers = run(s, "type_get", json!({"type": id}))["params"]["layers"].clone();
        let thicknesses: Vec<f64> =
            layers.as_array().into_iter().flatten().map(|l| l["thickness"].as_f64().unwrap()).collect();
        if thicknesses.len() == 3 && (thicknesses.iter().sum::<f64>() - 0.25).abs() < 1e-9 {
            return id;
        }
    }
    panic!("no layered 25 cm wall type in {types}");
}

fn volume(s: &mut Session, id: u64) -> f64 {
    run(s, "element_get", json!({"id": id}))["derived"]["volume"].as_f64().unwrap()
}

#[test]
fn corners_close_whatever_the_location_line() {
    // Drawn clockwise, so the exterior (left) face is outside; the outer
    // face lies `outside` beyond the drawn line.
    let lines = [
        ("FinishExterior", 0.0),
        ("CoreExterior", 0.025),
        ("Centerline", 0.125),
        ("CoreCenter", 0.125),
        ("CoreInterior", 0.225),
        ("FinishInterior", 0.25),
    ];
    for (line, outside) in lines {
        let mut s = Session::default();
        let ty = layered_type(&mut s);
        let corners = [[0.0, 0.0], [5.0, 0.0], [5.0, -4.0], [0.0, -4.0]];
        let walls: Vec<u64> = (0..4)
            .map(|i| {
                let w = run(
                    &mut s,
                    "wall_create",
                    json!({"start": corners[i], "end": corners[(i + 1) % 4], "type": ty,
                           "location_line": line, "height": 3.0}),
                );
                w["id"].as_u64().unwrap()
            })
            .collect();
        // The ring between the outer rectangle and the inner one, 0.25 m in.
        let (w, d) = (5.0 + 2.0 * outside, 4.0 + 2.0 * outside);
        let area = w * d - (w - 0.5) * (d - 0.5);
        let total: f64 = walls.iter().map(|&id| volume(&mut s, id)).sum();
        assert!((total - area * 3.0).abs() < 1e-6, "{line}: {total} != {}", area * 3.0);
    }
}

#[test]
fn an_open_corner_on_the_core_line_is_mitered() {
    let mut s = Session::default();
    let ty = layered_type(&mut s);
    let a = run(
        &mut s,
        "wall_create",
        json!({"start": [0, 0], "end": [5, 0], "type": ty, "location_line": "CoreExterior", "height": 3.0}),
    );
    let b = run(
        &mut s,
        "wall_create",
        json!({"start": [5, 0], "end": [5, -4], "type": ty, "location_line": "CoreExterior", "height": 3.0}),
    );
    // Faces 0.025 m outside and 0.225 m inside the drawn lines; the corner
    // runs out to the outer faces: an L of arms 5.025 and 4.025 m, 0.25 m thick.
    let area = 0.25 * (5.025 + 4.025) - 0.25 * 0.25;
    let total = volume(&mut s, a["id"].as_u64().unwrap()) + volume(&mut s, b["id"].as_u64().unwrap());
    assert!((total - area * 3.0).abs() < 1e-6, "{total} != {}", area * 3.0);
}
