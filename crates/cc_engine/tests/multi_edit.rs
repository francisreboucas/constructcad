//! Changing several elements at once (issues #47 and #69).

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

#[test]
fn many_elements_change_in_one_step() {
    let mut s = Session::default();
    let ids: Vec<u64> = [0.0, 4.0, 8.0]
        .iter()
        .map(|x| run(&mut s, "column_create", json!({"position": [x, 0.0]}))["id"].as_u64().unwrap())
        .collect();
    let base = |s: &mut Session, id: u64| run(s, "element_get", json!({"id": id}))["params"]["base_offset"].clone();
    // Down to the footings, all three in one undo step.
    let depth = s.doc.undo_depth();
    run(&mut s, "elements_set", json!({"ids": ids, "params": {"base_offset": -1.5}}));
    assert_eq!(s.doc.undo_depth(), depth + 1);
    for id in &ids {
        assert_eq!(base(&mut s, *id), json!(-1.5));
    }
    run(&mut s, "undo", json!({}));
    assert!(ids.iter().all(|id| base(&mut s, *id) == json!(0.0)));

    // A new level for walls takes their doors along.
    let level = run(&mut s, "level_create", json!({"name": "Mezanino", "elevation": 1.5}))["id"].as_u64().unwrap();
    let wall = run(&mut s, "wall_create", json!({"start": [0, 5], "end": [6, 5]}))["id"].as_u64().unwrap();
    let door = run(&mut s, "door_create", json!({"wall": wall, "offset": 2.0}))["id"].as_u64().unwrap();
    run(&mut s, "elements_set", json!({"ids": [wall, ids[0]], "level": level}));
    for id in [wall, door, ids[0]] {
        assert_eq!(run(&mut s, "element_get", json!({"id": id}))["level"]["id"].as_u64(), Some(level), "{id}");
    }

    // Nothing changes when the request is wrong.
    let depth = s.doc.undo_depth();
    assert_eq!(fails(&mut s, "elements_set", json!({"ids": ids, "level": "Nope"})), ErrorCode::NotFound);
    assert_eq!(fails(&mut s, "elements_set", json!({"ids": [], "params": {}})), ErrorCode::InvalidArguments);
    assert_eq!(
        fails(&mut s, "elements_set", json!({"ids": [wall], "params": {"bulge": 50}})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(s.doc.undo_depth(), depth);
}
