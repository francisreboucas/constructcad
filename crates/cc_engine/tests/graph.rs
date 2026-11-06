//! Visual programming graphs: command nodes, lacing, Lua nodes, and
//! elements bound to the calls that made them, so running a graph again
//! changes them in place.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn walls(s: &mut Session) -> Vec<u64> {
    let list = run(s, "elements_list", json!({"category": "wall"}));
    list.as_array().unwrap().iter().map(|e| e["id"].as_u64().unwrap()).collect()
}

fn count(s: &mut Session, category: &str) -> usize {
    run(s, "elements_list", json!({"category": category})).as_array().unwrap().len()
}

/// Walls 4 m long along y, at x = 0, 5, … up to the input `end`, each with
/// a door.
fn wall_row() -> Value {
    json!({
        "uid": "0123456789abcdef0123456789abcdef",
        "name": "row",
        "nodes": [
            {"id": 1, "kind": {"builtin": "number"}, "name": "end", "values": {"value": 15}},
            {"id": 2, "kind": {"builtin": "range"}, "values": {"step": 5}},
            {"id": 3, "kind": {"builtin": "point"}},
            {"id": 4, "kind": {"builtin": "point"}, "values": {"y": 4}},
            {"id": 5, "kind": {"command": "wall_create"}, "values": {"height": 2.8}},
            {"id": 6, "kind": {"command": "door_create"}},
            {"id": 7, "kind": {"builtin": "count"}},
            {"id": 8, "kind": {"builtin": "output"}, "name": "walls"}
        ],
        "wires": [
            {"from": [1, "value"], "to": [2, "end"]},
            {"from": [2, "list"], "to": [3, "x"]},
            {"from": [2, "list"], "to": [4, "x"]},
            {"from": [3, "point"], "to": [5, "start"]},
            {"from": [4, "point"], "to": [5, "end"]},
            {"from": [5, "element"], "to": [6, "wall"]},
            {"from": [5, "element"], "to": [7, "list"]},
            {"from": [7, "count"], "to": [8, "value"]}
        ]
    })
}

#[test]
fn lists_map_over_a_command_and_feed_another() {
    let mut s = Session::default();
    let r = run(&mut s, "graph_run", json!({"graph": wall_row()}));
    assert_eq!(r["errors"], 0, "{r}");
    assert_eq!(r["outputs"]["walls"], 4);
    assert_eq!(r["created"], 8);
    assert_eq!(walls(&mut s).len(), 4);
    assert_eq!(count(&mut s, "door"), 4);
    let w = walls(&mut s);
    let wall = run(&mut s, "element_get", json!({"id": w[1]}));
    assert!((wall["params"]["height"].as_f64().unwrap() - 2.8).abs() < 1e-9, "{wall}");
    // One undo step for the whole run.
    assert!(s.execute("undo", json!({})).is_ok());
    assert_eq!(walls(&mut s).len(), 0);
}

#[test]
fn running_again_updates_in_place() {
    let mut s = Session::default();
    run(&mut s, "graph_run", json!({"graph": wall_row()}));
    let before = walls(&mut s);
    let mark = before[0];
    run(&mut s, "element_set", json!({"id": mark, "comments": "by hand"}));

    // Nothing changed: nothing happens, and no undo step is added.
    let depth = s.doc.undo_depth();
    let r = run(&mut s, "graph_run", json!({"graph": wall_row()}));
    assert_eq!(
        (r["created"].as_u64(), r["changed"].as_u64(), r["deleted"].as_u64()),
        (Some(0), Some(0), Some(0)),
        "{r}"
    );
    assert_eq!(s.doc.undo_depth(), depth);
    assert_eq!(walls(&mut s), before);

    // A shorter list: the first walls stay (same ids), the last goes with its door.
    let r = run(&mut s, "graph_run", json!({"graph": wall_row(), "inputs": {"end": 10}}));
    assert_eq!(r["errors"], 0, "{r}");
    assert_eq!(walls(&mut s), before[..3].to_vec());
    assert_eq!(count(&mut s, "door"), 3);
    let e = run(&mut s, "element_get", json!({"id": mark}));
    assert_eq!(e["comments"], "by hand");

    // A changed value moves the bound walls, keeping their ids.
    let mut g = wall_row();
    g["nodes"][3]["values"]["y"] = json!(6);
    run(&mut s, "graph_run", json!({"graph": g, "inputs": {"end": 10}}));
    assert_eq!(walls(&mut s), before[..3].to_vec());
    let len = run(&mut s, "element_get", json!({"id": before[2]}))["derived"]["length"].as_f64().unwrap();
    assert!((len - 6.0).abs() < 1e-6, "length {len}");

    // Undo brings back the previous run's walls and their bindings.
    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));
    assert_eq!(walls(&mut s), before);
    assert_eq!(
        run(&mut s, "graph_elements", json!({"graph": "0123456789abcdef0123456789abcdef"})).as_array().unwrap().len(),
        8
    );
}

#[test]
fn deleted_elements_come_back_and_released_ones_stay() {
    let mut s = Session::default();
    run(&mut s, "graph_run", json!({"graph": wall_row()}));
    let w = walls(&mut s);
    run(&mut s, "elements_delete", json!({"ids": [w[0]]}));
    assert_eq!(walls(&mut s).len(), 3);
    run(&mut s, "graph_run", json!({"graph": wall_row()}));
    assert_eq!(walls(&mut s).len(), 4);
    assert_eq!(count(&mut s, "door"), 4);

    let r = run(&mut s, "graph_release", json!({"graph": "0123456789abcdef0123456789abcdef"}));
    assert_eq!(r["released"], 8);
    run(&mut s, "graph_run", json!({"graph": wall_row()}));
    assert_eq!(walls(&mut s).len(), 8);
}

#[test]
fn removing_a_node_deletes_its_elements() {
    let mut s = Session::default();
    run(&mut s, "graph_run", json!({"graph": wall_row()}));
    let mut g = wall_row();
    g["nodes"].as_array_mut().unwrap().retain(|n| n["id"] != 6);
    g["wires"].as_array_mut().unwrap().retain(|w| w["to"][0] != 6);
    let r = run(&mut s, "graph_run", json!({"graph": g}));
    assert_eq!(r["deleted"], 4, "{r}");
    assert_eq!(count(&mut s, "door"), 0);
    assert_eq!(walls(&mut s).len(), 4);
}

#[test]
fn a_failing_node_keeps_its_elements_and_other_branches_run() {
    let mut s = Session::default();
    run(&mut s, "graph_run", json!({"graph": wall_row()}));
    // The door's wall input now gets a text: the door node fails.
    let mut g = wall_row();
    g["wires"].as_array_mut().unwrap().retain(|w| w["to"][0] != 6);
    g["nodes"][5]["values"] = json!({"wall": 999_999});
    let r = run(&mut s, "graph_run", json!({"graph": g}));
    assert!(r["nodes"]["6"]["error"].is_string(), "{r}");
    assert_eq!(r["errors"], 1);
    assert_eq!(count(&mut s, "door"), 4);
    assert_eq!(r["outputs"]["walls"], 4);
}

#[test]
fn lua_nodes_compute_values() {
    let mut s = Session::default();
    let g = json!({
        "nodes": [
            {"id": 1, "kind": {"builtin": "integer"}, "values": {"value": 3}},
            {"id": 2, "kind": {"lua": "local t = {}\nfor i = 1, n do t[i] = i * 2 end\nreturn t, #t"}, "inputs": ["n"], "outputs": ["list", "size"]},
            {"id": 3, "kind": {"builtin": "output"}, "name": "list"},
            {"id": 4, "kind": {"builtin": "output"}, "name": "size"},
            {"id": 5, "kind": {"lua": "while true do end"}, "inputs": [], "outputs": ["x"]},
            {"id": 6, "kind": {"lua": "print('hi') return {a = 1, b = 2}"}, "inputs": [], "outputs": ["a", "b"]}
        ],
        "wires": [
            {"from": [1, "value"], "to": [2, "n"]},
            {"from": [2, "list"], "to": [3, "value"]},
            {"from": [2, "size"], "to": [4, "value"]}
        ]
    });
    let r = run(&mut s, "graph_run", json!({"graph": g}));
    assert_eq!(r["outputs"]["list"], json!([2, 4, 6]));
    assert_eq!(r["outputs"]["size"], 3);
    assert!(r["nodes"]["5"]["error"].as_str().unwrap().contains("instruction"), "{r}");
    assert_eq!(r["nodes"]["6"]["outputs"]["b"], 2);
    assert_eq!(r["messages"], json!(["hi"]));
}

#[test]
fn nodes_that_cannot_run_say_why() {
    let mut s = Session::default();
    let g = json!({
        "nodes": [
            {"id": 1, "kind": {"command": "project_open"}},
            {"id": 2, "kind": {"command": "ifc_export"}},
            {"id": 3, "kind": {"command": "no_such_command"}},
            {"id": 4, "kind": {"command": "wall_create"}},
            {"id": 5, "kind": {"builtin": "add"}, "values": {"a": 1, "b": "x"}}
        ],
        "wires": [{"from": [5, "result"], "to": [4, "start"]}]
    });
    let check = run(&mut s, "graph_check", json!({"graph": g}));
    let problems = check["problems"].as_array().unwrap();
    assert!(problems.len() >= 4, "{check}");
    let r = run(&mut s, "graph_run", json!({"graph": g}));
    assert!(r["nodes"]["1"]["error"].as_str().unwrap().contains("cannot run"));
    assert!(r["nodes"]["2"]["error"].as_str().unwrap().contains("files"));
    assert!(r["nodes"]["3"]["error"].is_string());
    assert!(r["nodes"]["5"]["error"].is_string());
    assert!(r["nodes"]["4"]["skipped"].is_string(), "{r}");
    assert_eq!(walls(&mut s).len(), 0);
}

#[test]
fn every_command_is_a_node() {
    let mut s = Session::default();
    let nodes = run(&mut s, "graph_nodes", json!({}));
    let nodes = nodes.as_array().unwrap();
    let commands = s.commands().len();
    let command_nodes = nodes.iter().filter(|n| n["kind"].get("command").is_some()).count();
    // Every command but the few a graph never runs.
    assert!(command_nodes + 7 >= commands, "{command_nodes} of {commands}");
    let wall = nodes.iter().find(|n| n["name"] == "wall_create").unwrap();
    let start = wall["inputs"].as_array().unwrap().iter().find(|p| p["name"] == "start").unwrap();
    assert_eq!(start["type"], "point");
    assert_eq!(start["required"], true);
    assert_eq!(wall["outputs"][0]["name"], "element");
    let floor = nodes.iter().find(|n| n["name"] == "floor_create").unwrap();
    let curves = floor["inputs"].as_array().unwrap().iter().find(|p| p["name"] == "curves").unwrap();
    assert_eq!(curves["type"], json!({"list": "object"}));
    let found = run(&mut s, "graph_nodes", json!({"search": "polygon"}));
    assert!(found.as_array().unwrap().iter().any(|n| n["name"] == "polygon_points"));
}

#[test]
fn curves_and_lists_of_outlines() {
    let mut s = Session::default();
    // Two floors from a list of two outlines, and one from a single curve.
    let g = json!({
        "nodes": [
            {"id": 1, "kind": {"lua": "return {{{0,0},{4,0},{4,3}}, {{10,0},{14,0},{14,3}}}"}, "inputs": [], "outputs": ["outlines"]},
            {"id": 2, "kind": {"command": "floor_create"}},
            {"id": 3, "kind": {"builtin": "circle"}, "values": {"center": [20, 0], "radius": 2}},
            {"id": 4, "kind": {"command": "floor_create"}}
        ],
        "wires": [
            {"from": [1, "outlines"], "to": [2, "boundary"]},
            {"from": [3, "curve"], "to": [4, "curves"]}
        ]
    });
    let r = run(&mut s, "graph_run", json!({"graph": g, "brief": true}));
    assert_eq!(r["errors"], 0, "{r}");
    assert_eq!(count(&mut s, "floor"), 3);
}

#[test]
fn hostile_graphs_fail_cleanly() {
    let mut s = Session::default();
    assert!(s.execute("graph_run", json!({})).is_err());
    assert!(s.execute("graph_run", json!({"graph": [1, 2]})).is_err());
    assert!(s.execute("graph_run", json!({"path": "no/such/file.ccgraph"})).is_err());
    // A graph run inside a graph is refused.
    let g = json!({"nodes": [{"id": 1, "kind": {"command": "graph_run"}, "values": {"graph": {"nodes": []}}}]});
    let r = run(&mut s, "graph_run", json!({"graph": g}));
    assert!(r["nodes"]["1"]["error"].is_string());
    // Deep lists and too many calls are errors, not hangs.
    let g = json!({
        "nodes": [
            {"id": 1, "kind": {"builtin": "sequence"}, "values": {"count": 100000}},
            {"id": 2, "kind": {"builtin": "sequence"}, "values": {"count": 100}},
            {"id": 3, "kind": {"builtin": "add"}, "lacing": "cross"}
        ],
        "wires": [{"from": [1, "list"], "to": [3, "a"]}, {"from": [2, "list"], "to": [3, "b"]}]
    });
    let r = run(&mut s, "graph_run", json!({"graph": g, "brief": true}));
    assert!(r["nodes"]["3"]["error"].as_str().unwrap().contains("too many"), "{r}");
}

#[test]
fn the_example_house_graph_runs_and_follows_its_inputs() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/graphs/house.ccgraph");
    let mut s = Session::default();
    let r = run(&mut s, "graph_run", json!({"path": path, "brief": true}));
    assert_eq!(r["errors"], 0, "{r}");
    assert_eq!(r["outputs"]["area"], 70.0);
    assert_eq!(walls(&mut s).len(), 4);
    assert_eq!((count(&mut s, "window"), count(&mut s, "floor"), count(&mut s, "roof")), (4, 1, 1));
    let before = walls(&mut s);
    let r = run(&mut s, "graph_run", json!({"path": path, "inputs": {"width": 12}, "brief": true}));
    assert_eq!(r["errors"], 0, "{r}");
    assert_eq!(r["created"], 0, "{r}");
    assert_eq!(walls(&mut s), before);
    assert_eq!(r["outputs"]["area"], 84.0);
}
