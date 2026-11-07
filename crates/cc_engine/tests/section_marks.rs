//! Section marks in plans: the NBR 6492 head, a split line, moving and
//! flipping a section through `view_set`.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{ViewId, ViewKind};
use cc_engine::Session;
use cc_views::{Prim, SectionHandle};
use glam::DVec2;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn first_plan(s: &mut Session) -> ViewId {
    let plans = run(s, "views_list", json!({"category": "floor_plan"}));
    ViewId(plans[0]["id"].as_u64().unwrap())
}

fn mark(s: &mut Session, plan: ViewId, section: ViewId) -> SectionHandle {
    let d = s.drawing(plan).unwrap();
    d.sections.iter().find(|h| h.view == section).cloned().expect("the section's mark")
}

fn texts(s: &mut Session, plan: ViewId) -> Vec<String> {
    let d = s.drawing(plan).unwrap();
    d.prims
        .iter()
        .filter_map(|p| match p {
            Prim::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_section_mark_is_split_moved_and_flipped() {
    let mut s = Session::default();
    let plan = first_plan(&mut s);
    run(&mut s, "view_set", json!({"view": plan.0, "scale": 100}));
    let out = run(&mut s, "view_create", json!({"kind": "section", "start": [0.0, 0.0], "end": [10.0, 0.0]}));
    let section = ViewId(out["id"].as_u64().unwrap());

    // A 12 mm head at 1:100 is 0.6 m across; the line starts at the
    // triangle's base, twice the circle wide.
    let m = mark(&mut s, plan, section);
    assert!((m.radius - 0.6).abs() < 1e-9, "{}", m.radius);
    assert_eq!(m.pieces.len(), 1);
    assert!((m.pieces[0].0.x - 1.2).abs() < 1e-9 && (m.pieces[0].1.x - 8.8).abs() < 1e-9, "{:?}", m.pieces);
    assert!(m.hits(DVec2::new(5.0, 0.05), 0.1), "on the line");
    assert!(m.hits(DVec2::new(10.3, 0.3), 0.0), "in a head");

    // Split: a piece next to each head, nothing in the middle.
    run(&mut s, "view_set", json!({"view": section.0, "params": {"segments": [2.5, 3.0]}}));
    let m = mark(&mut s, plan, section);
    assert_eq!(m.pieces.len(), 2, "{:?}", m.pieces);
    assert!(!m.hits(DVec2::new(5.0, 0.0), 0.1), "the gap is not the mark");
    assert!(m.hits(DVec2::new(2.0, 0.0), 0.1));
    assert_eq!(m.segment_ends(), Some([DVec2::new(2.5, 0.0), DVec2::new(7.0, 0.0)]));

    // Moved: both ends by the same step, the split kept.
    run(&mut s, "view_set", json!({"view": section.0, "params": {"start": [0.0, 2.0], "end": [10.0, 2.0]}}));
    let m = mark(&mut s, plan, section);
    assert_eq!((m.start, m.end, m.segments), (DVec2::new(0.0, 2.0), DVec2::new(10.0, 2.0), Some([2.5, 3.0])));

    // Flipped: the ends swap and each piece stays by its head.
    let mut kind = s.project().view(section).unwrap().kind.clone();
    assert!(kind.flip_section());
    let ViewKind::Section { start, end, segments, .. } = kind else { panic!("a section") };
    assert_eq!((start, end, segments), (DVec2::new(10.0, 2.0), DVec2::new(0.0, 2.0), Some([3.0, 2.5])));
    let params = json!({"start": start.to_array(), "end": end.to_array(), "segments": segments});
    run(&mut s, "view_set", json!({"view": section.0, "params": params}));
    let m = mark(&mut s, plan, section);
    assert_eq!(m.segment_ends(), Some([DVec2::new(7.0, 2.0), DVec2::new(2.5, 2.0)]));

    // Whole again.
    run(&mut s, "view_set", json!({"view": section.0, "params": {"segments": null}}));
    assert_eq!(mark(&mut s, plan, section).pieces.len(), 1);

    // A split piece cannot be negative.
    assert!(s.execute("view_set", json!({"view": section.0, "params": {"segments": [-1.0, 2.0]}})).is_err());
}

#[test]
fn a_section_head_shows_its_drawing_and_sheet_numbers() {
    let mut s = Session::default();
    let plan = first_plan(&mut s);
    let out = run(
        &mut s,
        "view_create",
        json!({"kind": "section", "name": "Corte A", "start": [0.0, 0.0], "end": [8.0, 0.0]}),
    );
    let section = ViewId(out["id"].as_u64().unwrap());
    // On no sheet: its letter over a dash.
    let t = texts(&mut s, plan);
    assert_eq!(t.iter().filter(|x| *x == "A").count(), 2, "{t:?}");
    assert!(t.iter().any(|x| x == "-"));
    // On a sheet: the drawing's number over the sheet's.
    let sheet = run(&mut s, "view_create", json!({"kind": "sheet"}));
    run(&mut s, "sheet_place_view", json!({"sheet": sheet["id"], "view": section.0}));
    let number = s.project().view(ViewId(sheet["id"].as_u64().unwrap())).and_then(|v| match &v.kind {
        ViewKind::Sheet { number, .. } => Some(number.clone()),
        _ => None,
    });
    let t = texts(&mut s, plan);
    assert_eq!(t.iter().filter(|x| *x == "1").count(), 2, "{t:?}");
    assert_eq!(t.iter().filter(|x| Some(*x) == number.as_ref()).count(), 2, "{t:?}");
}
