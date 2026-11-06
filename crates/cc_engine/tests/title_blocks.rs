//! Title blocks: sheet paper, project and sheet fields, title block
//! components and their pieces, from engine commands.

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
fn project_fields_are_set_renamed_and_removed() {
    let mut s = Session::default();
    run(&mut s, "project_info_set", json!({"fields": {"Responsável técnico": "Ana", "CAU": "A123"}}));
    let info = run(&mut s, "project_info", json!({}));
    assert_eq!(info["fields"]["Responsável técnico"], "Ana");
    run(&mut s, "project_info_set", json!({"fields": {"CAU": null, "Etapa": "Executivo"}}));
    let info = run(&mut s, "project_info", json!({}));
    assert!(info["fields"].get("CAU").is_none());
    assert_eq!(info["fields"]["Etapa"], "Executivo");
    assert_eq!(fails(&mut s, "project_info_set", json!({"fields": {"  ": "x"}})), ErrorCode::InvalidArguments);
}

#[test]
fn template_sheets_draw_the_standard_title_block_with_their_fields() {
    let mut s = Session::default();
    let sheet = s.project().sheets_in_order()[0];
    let name = s.project().info.name.clone();
    let drawing = &s.sheet(sheet).unwrap().drawing;
    let texts: Vec<&str> = drawing
        .prims
        .iter()
        .filter_map(|p| match p {
            cc_views::Prim::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    for t in ["A101", name.as_str(), "Proj. nº 0001"] {
        assert!(texts.contains(&t), "{t} in {texts:?}");
    }
    // Every field is filled in (a 3D view's name, "{3D}", has braces of its own).
    let unfilled = |t: &&str| cc_views::sheet_fields::KEYS.iter().any(|k| t.contains(&format!("{{{k}}}")));
    assert!(!texts.iter().any(unfilled), "fields filled in: {texts:?}");
}
