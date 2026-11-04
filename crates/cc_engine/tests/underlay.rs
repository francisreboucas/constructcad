//! Plan underlays (issue #154): a plan shows the plan of another level
//! halftone under its own, on screen and, when asked, on paper.

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

/// The floor plan of the level named `level`.
fn plan_of(s: &mut Session, level: &str) -> u64 {
    let views = run(s, "views_list", json!({"category": "floor_plan"}));
    let views = views.as_array().unwrap().clone();
    views
        .iter()
        .map(|v| v["id"].as_u64().unwrap())
        .find(|id| run(s, "view_get", json!({"view": id}))["params"]["level"] == json!(level_id(s, level)))
        .unwrap()
}

fn level_id(s: &mut Session, name: &str) -> u64 {
    let levels = run(s, "levels_list", json!({}));
    levels.as_array().unwrap().iter().find(|l| l["name"] == name).unwrap()["id"].as_u64().unwrap()
}

/// How many primitives the view's screen drawing, and its printed one, have under it.
fn underlay(s: &mut Session, view: u64) -> (usize, usize) {
    let id = cc_core::ViewId(view);
    (s.drawing(id).unwrap().underlay, s.printed_drawing(id).unwrap().underlay)
}

#[test]
fn plans_show_another_level_under_them() {
    let mut s = Session::default();
    run(
        &mut s,
        "project_open",
        json!({"path": concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/casa_simples.ccad")}),
    );
    let levels = run(&mut s, "levels_list", json!({}));
    let (ground, first) =
        (levels[0]["name"].as_str().unwrap().to_string(), levels[1]["name"].as_str().unwrap().to_string());
    let upper = plan_of(&mut s, &first);
    assert_eq!(underlay(&mut s, upper), (0, 0));

    // The next level below, looking down: on screen only.
    let below = json!({"underlay": {"below": {"level": "Next", "looking": "Down"}}});
    run(&mut s, "view_set", json!({"view": upper, "params": below}));
    let got = run(&mut s, "view_get", json!({"view": upper}));
    assert_eq!(got["params"]["underlay"]["below"], json!({"level": "Next", "looking": "Down"}));
    let (screen, printed) = underlay(&mut s, upper);
    assert!(screen > 20 && printed == 0, "{screen} {printed}");
    // Printed too, when asked.
    run(&mut s, "view_set", json!({"view": upper, "params": {"underlay": {"printed": true}}}));
    assert_eq!(underlay(&mut s, upper), (screen, screen));

    // A given level, looking up (the ground floor's ceiling plan, its walls
    // cut without its floor), under the upper plan in place of the next one.
    let lower = plan_of(&mut s, &ground);
    let ground_id = level_id(&mut s, &ground);
    let up = json!({"underlay": {"below": {"level": {"Level": ground_id}, "looking": "Up"}}});
    run(&mut s, "view_set", json!({"view": upper, "params": up}));
    let seen_up = underlay(&mut s, upper).0;
    assert!(seen_up > 0 && seen_up != screen, "{seen_up} vs {screen}");
    // Nothing is above to show when the level above is empty.
    let first_id = level_id(&mut s, &first);
    let above = json!({"underlay": {"above": {"level": {"Level": first_id}}}});
    run(&mut s, "view_set", json!({"view": lower, "params": above}));
    assert_eq!(underlay(&mut s, lower).0, 0);
    // A level that does not exist is refused.
    let bad = json!({"underlay": {"above": {"level": {"Level": 99999}}}});
    assert_eq!(fails(&mut s, "view_set", json!({"view": lower, "params": bad})), ErrorCode::NotFound);

    // A template carries the underlay to the views that follow it.
    run(&mut s, "view_set", json!({"view": upper, "params": below}));
    let t = run(&mut s, "template_create", json!({"name": "With the level below", "view": upper}))["id"].clone();
    assert_eq!(run(&mut s, "templates_list", json!({}))[0]["underlay"]["below"]["level"], "Next");
    let mezzanine = run(&mut s, "level_create", json!({"name": "Mezanino", "elevation": 1.5}))["id"].as_u64().unwrap();
    let plans = run(&mut s, "views_list", json!({"category": "floor_plan"}));
    let mezz_plan = plans
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_u64().unwrap())
        .find(|id| run(&mut s, "view_get", json!({"view": id}))["params"]["level"] == json!(mezzanine))
        .unwrap();
    run(&mut s, "view_template_assign", json!({"views": [mezz_plan], "template": t}));
    assert!(underlay(&mut s, mezz_plan).0 > 0, "the ground floor under the mezzanine");
    // The view cannot change what the template controls.
    let none = json!({"underlay": {"below": null}});
    assert_eq!(fails(&mut s, "view_set", json!({"view": mezz_plan, "params": none})), ErrorCode::Rejected);

    // Deleting a level drops it from the underlays that name it.
    run(&mut s, "view_set", json!({"view": lower, "params": {"underlay": {"above": {"level": {"Level": mezzanine}}}}}));
    run(&mut s, "view_template_assign", json!({"views": [mezz_plan], "template": null}));
    run(&mut s, "level_delete", json!({"level": mezzanine}));
    assert!(run(&mut s, "view_get", json!({"view": lower}))["params"]["underlay"].get("above").is_none());
}
