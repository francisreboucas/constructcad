//! Scripted components through engine commands: writing the script,
//! parameters of types and elements, hotspots, errors and undo.

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

const CABINET: &str = r##"
param.length  { "depth", label = "Depth", default = 0.5 }
param.length  { "width", label = "Width", default = 1, instance = true }
param.boolean { "top", default = true, instance = true }
param.choice  { "finish", { {"oak", "Oak"}, {"pine", "Pine"} }, instance = true }
param.material { "body", default = "", instance = true }
hotspot { "reach", 0, 0, 1, label = "Height", shape = "diamond", color = "#ff8800" }

function geometry(p)
  assert(p.width > 0, "the width must be positive")
  local pieces = { box { 0, 0, 0, p.width, p.depth, p.reach.z, name = "body", material = p.body } }
  if p.top then
    pieces[#pieces + 1] = box { 0, 0, p.reach.z, p.width, p.depth, 0.1, name = "top" }
  end
  return pieces
end
"##;

/// A cabinet component (its first piece a script) and one element of it.
fn cabinet(s: &mut Session) -> (Value, u64) {
    let def = json!({ "items": [{ "name": "Script", "shape": { "Script": { "source": CABINET } } }] });
    let ty = run(s, "component_create", json!({ "name": "Cabinet", "def": def }))["id"].clone();
    let element = run(s, "component_place", json!({ "type": ty, "position": [0, 0] }))["id"].as_u64().unwrap();
    (ty, element)
}

fn volume(s: &mut Session, id: u64) -> f64 {
    run(s, "element_get", json!({ "id": id }))["derived"]["volume"].as_f64().unwrap_or(0.0)
}

fn param<'a>(v: &'a Value, name: &str) -> &'a Value {
    v["params"].as_array().unwrap().iter().find(|p| p["name"] == name).unwrap_or_else(|| panic!("no param {name}"))
}

#[test]
fn a_script_makes_parameters_and_geometry() {
    let mut s = Session::default();
    let (ty, e) = cabinet(&mut s);
    let params = run(&mut s, "component_params_get", json!({ "type": ty }));
    assert_eq!(param(&params, "depth")["value"], 0.5);
    assert_eq!(param(&params, "top")["value"], true);
    assert_eq!(param(&params, "finish")["value"], "oak");
    assert_eq!(param(&params, "finish")["choices"][1]["label"], "Pine");
    assert_eq!(params["hotspots"][0]["shape"], "diamond");
    assert_eq!(params["hotspots"][0]["color"], "#ff8800ff");
    // 1 x 0.5 x 1 body and a 0.1 top.
    assert!((volume(&mut s, e) - 0.55).abs() < 1e-9, "{}", volume(&mut s, e));

    // Instance values change only the element; the type's depth changes all.
    run(&mut s, "component_params_set", json!({ "elements": [e], "name": "width", "value": 2 }));
    run(&mut s, "component_params_set", json!({ "elements": [e], "name": "top", "value": false }));
    assert!((volume(&mut s, e) - 1.0).abs() < 1e-9);
    run(&mut s, "component_params_set", json!({ "type": ty, "name": "depth", "value": 0.25 }));
    assert!((volume(&mut s, e) - 0.5).abs() < 1e-9);
    run(&mut s, "component_hotspot_move", json!({ "element": e, "name": "reach", "point": [0, 0, 2] }));
    assert!((volume(&mut s, e) - 1.0).abs() < 1e-9);
    let got = run(&mut s, "component_params_get", json!({ "elements": [e] }));
    assert_eq!(param(&got, "width")["value"], 2.0);
    assert_eq!(got["hotspots"][0]["position"], json!([0.0, 0.0, 2.0]));

    // Each edit is one undo step.
    run(&mut s, "undo", json!({}));
    assert!((volume(&mut s, e) - 0.5).abs() < 1e-9);
}

#[test]
fn values_are_checked_and_batches_change_all_or_nothing() {
    let mut s = Session::default();
    let (ty, a) = cabinet(&mut s);
    let b = run(&mut s, "component_place", json!({ "type": ty, "position": [3, 0] }))["id"].as_u64().unwrap();
    let set = |name: &str, value: Value| json!({ "elements": [a, b], "name": name, "value": value });
    assert_eq!(fails(&mut s, "component_params_set", set("finish", json!("steel"))), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "component_params_set", set("top", json!("yes"))), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "component_params_set", set("depth", json!(1))), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "component_params_set", set("body", json!("Nothing"))), ErrorCode::NotFound);
    run(&mut s, "component_params_set", set("finish", json!("pine")));
    run(&mut s, "component_params_set", json!({ "elements": [a], "name": "width", "value": 3 }));
    let got = run(&mut s, "component_params_get", json!({ "elements": [a, b] }));
    assert_eq!(param(&got, "finish")["value"], "pine");
    assert_eq!(param(&got, "width")["mixed"], true);
    assert_eq!(param(&got, "width")["value"], Value::Null);

    // A material named by a parameter cannot be deleted while it is used.
    run(&mut s, "material_create", json!({ "name": "Lacquer" }));
    run(&mut s, "component_params_set", json!({ "elements": [a], "name": "body", "value": "Lacquer" }));
    assert_eq!(fails(&mut s, "material_delete", json!({ "material": "Lacquer" })), ErrorCode::Rejected);
    run(&mut s, "material_rename", json!({ "material": "Lacquer", "name": "Gloss" }));
    let got = run(&mut s, "component_params_get", json!({ "elements": [a] }));
    assert_eq!(param(&got, "body")["value"], "Gloss");
}

#[test]
fn a_failing_script_keeps_the_last_pieces_and_says_why() {
    let mut s = Session::default();
    let (_, e) = cabinet(&mut s);
    let before = volume(&mut s, e);
    run(&mut s, "component_params_set", json!({ "elements": [e], "name": "width", "value": -1 }));
    let got = run(&mut s, "element_get", json!({ "id": e }));
    let errors = got["derived"]["script_errors"].as_array().unwrap();
    assert!(errors[0].as_str().unwrap().contains("the width must be positive"), "{errors:?}");
    assert!((volume(&mut s, e) - before).abs() < 1e-9, "the last good pieces stay");
    run(&mut s, "component_params_set", json!({ "elements": [e], "name": "width", "value": 2 }));
    let got = run(&mut s, "element_get", json!({ "id": e }));
    assert!(got["derived"]["script_errors"].as_array().unwrap().is_empty());
}

#[test]
fn editing_the_script_keeps_values_and_refuses_what_does_not_load() {
    let mut s = Session::default();
    let (ty, e) = cabinet(&mut s);
    run(&mut s, "component_params_set", json!({ "type": ty, "name": "depth", "value": 0.8 }));
    let depth = s.doc.undo_depth();
    // A syntax error or a bad declaration is refused without an undo step.
    assert_eq!(
        fails(&mut s, "component_script_set", json!({ "type": ty, "piece": "Script", "source": "function geometry(" })),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(
            &mut s,
            "component_script_set",
            json!({ "type": ty, "piece": "Script", "source": "param.length { 'a' } param.text { 'a' }" })
        ),
        ErrorCode::InvalidArguments
    );
    assert_eq!(s.doc.undo_depth(), depth);
    // A new version without `top` and with a label change keeps depth's value.
    let edited = CABINET
        .replace("label = \"Depth\"", "label = \"Deep\"")
        .replace("param.boolean { \"top\", default = true, instance = true }", "local _ = 0")
        .replace("if p.top then", "if false then");
    let out = run(&mut s, "component_script_set", json!({ "type": ty, "piece": "Script", "source": edited }));
    assert!(out["errors"].as_array().unwrap().is_empty(), "{out}");
    let got = run(&mut s, "component_params_get", json!({ "type": ty }));
    assert_eq!(
        (param(&got, "depth")["value"].clone(), param(&got, "depth")["label"].clone()),
        (json!(0.8), json!("Deep"))
    );
    assert!(got["params"].as_array().unwrap().iter().all(|p| p["name"] != "top"));
    assert!((volume(&mut s, e) - 0.8).abs() < 1e-9);
    // Script parameters are changed in the script, not by hand.
    let rename = json!({ "type": ty, "name": "depth", "rename": "d" });
    assert_eq!(fails(&mut s, "component_param_set", rename), ErrorCode::Rejected);
    assert_eq!(fails(&mut s, "component_param_delete", json!({ "type": ty, "name": "depth" })), ErrorCode::Rejected);
}

#[test]
fn hotspots_move_in_the_world_too_and_survive_a_save() {
    let mut s = Session::default();
    let (ty, e) = cabinet(&mut s);
    run(&mut s, "elements_move", json!({ "ids": [e], "delta": [5, 0] }));
    run(
        &mut s,
        "component_hotspot_move",
        json!({ "element": e, "name": "reach", "point": [5.5, 0, 1.5], "world": true }),
    );
    let got = run(&mut s, "component_params_get", json!({ "elements": [e] }));
    assert_eq!(got["hotspots"][0]["position"], json!([0.5, 0.0, 1.5]));
    assert_eq!(
        fails(&mut s, "component_hotspot_move", json!({ "element": e, "name": "x", "point": [0, 0, 0] })),
        ErrorCode::NotFound
    );

    let dir = std::env::temp_dir().join(format!("cc_script_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cabinet.ccad");
    run(&mut s, "project_save", json!({ "path": path }));
    let mut t = Session::default();
    run(&mut t, "project_open", json!({ "path": path }));
    let got = run(&mut t, "component_params_get", json!({ "elements": [e] }));
    assert_eq!(got["hotspots"][0]["position"], json!([0.5, 0.0, 1.5]));
    let typed = run(&mut t, "component_params_get", json!({ "type": ty }));
    assert_eq!(param(&typed, "finish")["choices"].as_array().unwrap().len(), 2);
    assert!((volume(&mut t, e) - 0.8).abs() < 1e-9, "{}", volume(&mut t, e));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_example_cabinet_loads_and_survives_ifc() {
    let mut s = Session::default();
    let source = include_str!("../../../examples/components/cabinet.lua");
    let def = json!({ "items": [{ "name": "Script", "shape": { "Script": { "source": "" } } }] });
    let ty = run(&mut s, "component_create", json!({ "name": "Cabinet", "def": def }))["id"].clone();
    let out = run(&mut s, "component_script_set", json!({ "type": ty, "piece": "Script", "source": source }));
    assert!(out["errors"].as_array().unwrap().is_empty(), "{out}");
    let e = run(&mut s, "component_place", json!({ "type": ty, "position": [0, 0] }))["id"].as_u64().unwrap();
    let two = volume(&mut s, e);
    assert!(two > 0.01, "{two}");
    run(&mut s, "component_params_set", json!({ "elements": [e], "name": "doors", "value": 3 }));
    run(&mut s, "component_params_set", json!({ "type": ty, "name": "handle", "value": "none" }));
    assert!(volume(&mut s, e) > 0.01);

    // IFC export writes the pieces the script made.
    let dir = std::env::temp_dir().join(format!("cc_script_ifc_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cabinet.ifc");
    run(&mut s, "ifc_export", json!({ "path": path }));
    let mut t = Session::default();
    run(&mut t, "ifc_import", json!({ "path": path }));
    let list = run(&mut t, "elements_list", json!({ "category": "component" }));
    let ids: Vec<u64> = list.as_array().unwrap().iter().filter_map(|x| x["id"].as_u64()).collect();
    assert_eq!(ids.len(), 1, "{list}");
    let imported = volume(&mut t, ids[0]);
    assert!((imported - volume(&mut s, e)).abs() < 1e-3, "{imported}");
    let _ = std::fs::remove_dir_all(&dir);
}
