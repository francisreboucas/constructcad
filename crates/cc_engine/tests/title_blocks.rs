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

fn first_sheet(s: &mut Session) -> u64 {
    let v = run(s, "view_create", json!({"kind": "sheet", "name": "Plantas"}));
    v["id"].as_u64().unwrap()
}

#[test]
fn new_sheets_have_the_standard_title_block_and_fill_fields() {
    let mut s = Session::default();
    let sheet = first_sheet(&mut s);
    let standard = s.project().title_blocks()[0];
    let view = run(&mut s, "view_get", json!({"view": sheet}));
    assert_eq!(view["params"]["title_block"], standard.0);
    let f = run(&mut s, "sheet_fields", json!({"sheet": sheet}));
    assert_eq!(f["fields"]["sheet_name"], "Plantas");
    assert!(f["fields"]["sheet_count"].as_str().unwrap().parse::<u32>().unwrap() >= 1);
    assert_eq!(f["fields"]["issue_date"], "");
    assert_eq!(f["errors"], json!([]));
    let keys: Vec<&String> = f["fields"].as_object().unwrap().keys().collect();
    let date = keys.iter().position(|k| *k == "date").unwrap();
    assert_eq!(keys[date + 1], "issue_date");
    run(
        &mut s,
        "sheet_set",
        json!({"sheet": sheet, "number": "A7", "size": {"width": 1000, "height": 594}, "portrait": true, "issue_date": "2026-11-06"}),
    );
    let f = run(&mut s, "sheet_fields", json!({"sheet": sheet}));
    assert_eq!(f["fields"]["sheet_number"], "A7");
    assert_eq!(f["fields"]["paper"], "1000 × 594 mm");
    assert!(!f["fields"]["issue_date"].as_str().unwrap().is_empty());
    let view = run(&mut s, "view_get", json!({"view": sheet}));
    assert_eq!(view["params"]["portrait"], true);
    assert_eq!(view["params"]["issue_date"], "2026-11-06");
    run(&mut s, "sheet_set", json!({"sheet": sheet, "size": "a2", "issue_date": ""}));
    let view = run(&mut s, "view_get", json!({"view": sheet}));
    assert_eq!(view["params"]["size"], "A2");
    assert_eq!(view["params"]["issue_date"].as_str().unwrap_or_default(), "");
    assert_eq!(
        fails(&mut s, "sheet_set", json!({"sheet": sheet, "issue_date": "31/02/2026"})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(&mut s, "sheet_set", json!({"sheet": sheet, "issue_date": "2026-02-31"})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(&mut s, "sheet_set", json!({"sheet": sheet, "size": {"width": 10, "height": 10}})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(fails(&mut s, "sheet_set", json!({"sheet": sheet, "size": "B4"})), ErrorCode::InvalidArguments);
    assert_eq!(
        fails(&mut s, "sheet_set", json!({"sheet": sheet, "title_block": "Alvenaria 15 cm"})),
        ErrorCode::InvalidArguments
    );
    let plan = s.project().views.values().find(|v| v.category() == cc_core::ViewCategory::FloorPlan).unwrap().id.0;
    assert_eq!(fails(&mut s, "sheet_set", json!({"sheet": plan, "number": "X"})), ErrorCode::InvalidArguments);
    run(&mut s, "sheet_set", json!({"sheet": sheet, "title_block": null}));
    let view = run(&mut s, "view_get", json!({"view": sheet}));
    assert!(view["params"]["title_block"].is_null());
    let f = run(&mut s, "sheet_fields", json!({"sheet": sheet}));
    assert_eq!(f["params"], json!([]));
    run(&mut s, "sheet_set", json!({"sheet": sheet, "title_block": standard.0}));
    let view = run(&mut s, "view_get", json!({"view": sheet}));
    assert_eq!(view["params"]["title_block"], standard.0);
}

#[test]
fn text_parameters_are_added_by_command() {
    let mut s = Session::default();
    let tb = run(&mut s, "title_block_create", json!({"name": "Carimbo B"}))["id"].as_u64().unwrap();
    run(
        &mut s,
        "component_param_add",
        json!({"type": tb, "name": "Drawn", "kind": "text", "instance": true, "text": "Equipe"}),
    );
    let got = run(&mut s, "component_get", json!({"type": tb}));
    let param = got["def"]["params"].as_array().unwrap().iter().find(|p| p["name"] == "Drawn").unwrap().clone();
    assert_eq!(param["kind"], "Text");
    assert_eq!(param["text"], "Equipe");
    assert_eq!(param["instance"], true);
    // Only text parameters take a text, and choices are declared by scripts.
    assert_eq!(
        fails(&mut s, "component_param_add", json!({"type": tb, "name": "W", "kind": "length", "text": "x"})),
        ErrorCode::InvalidArguments
    );
    assert_eq!(
        fails(&mut s, "component_param_add", json!({"type": tb, "name": "C", "kind": "choice"})),
        ErrorCode::InvalidArguments
    );
    // The copy of the standard block has its pieces.
    assert!(!got["def"]["sheet"].as_array().unwrap().is_empty());
}

#[test]
fn a_title_block_is_built_from_commands_and_saved() {
    let mut s = Session::default();
    let tb = run(&mut s, "title_block_create", json!({"name": "Meu carimbo", "empty": true}));
    let tb = tb["id"].as_u64().unwrap();
    run(
        &mut s,
        "component_param_add",
        json!({"type": tb, "name": "Drawn", "kind": "text", "instance": true, "text": "Equipe"}),
    );
    let i = run(
        &mut s,
        "component_sheet_piece_add",
        json!({"type": tb, "piece": {"kind": "stroke", "closed": true,
        "points": [{"u": 10, "v": 10}, {"u": "SheetWidth - 10", "v": 10}, {"u": "SheetWidth - 10", "v": "SheetHeight - 10"}, {"u": 10, "v": "SheetHeight - 10"}]}}),
    );
    assert_eq!(i["index"], 0);
    run(
        &mut s,
        "component_sheet_piece_add",
        json!({"type": tb, "piece": {"kind": "text", "x": 20, "y": 20, "text": "{param:Drawn} {sheet_number}", "anchor": "left"}}),
    );
    // 1 × 1 PNG.
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    run(
        &mut s,
        "component_sheet_piece_add",
        json!({"type": tb, "piece": {"kind": "image", "x": 30, "y": 30, "width": 20, "height": 20, "image": png}}),
    );
    let not_png = json!({"kind": "image", "x": 0, "y": 0, "width": 1, "height": 1, "image": "bm90IGFuIGltYWdl"});
    assert_eq!(
        fails(&mut s, "component_sheet_piece_add", json!({"type": tb, "piece": not_png})),
        ErrorCode::InvalidArguments
    );
    let bad_anchor = json!({"kind": "text", "x": 0, "y": 0, "text": "x", "anchor": "top"});
    assert_eq!(
        fails(&mut s, "component_sheet_piece_add", json!({"type": tb, "piece": bad_anchor})),
        ErrorCode::InvalidArguments
    );
    run(&mut s, "component_sheet_piece_set", json!({"type": tb, "index": 1, "piece": {"Text": {"size": "4"}}}));
    let sheet = first_sheet(&mut s);
    run(&mut s, "sheet_set", json!({"sheet": sheet, "title_block": tb, "number": "B2", "fields": {"Drawn": "Bia"}}));
    let f = run(&mut s, "sheet_fields", json!({"sheet": sheet}));
    assert_eq!(f["params"][0]["name"], "Drawn");
    assert_eq!(f["params"][0]["value"], "Bia");
    assert_eq!(f["params"][0]["default"], "Equipe");
    // The composed sheet shows the expanded text.
    let composed = s.sheet(cc_core::ViewId(sheet)).unwrap();
    let texts: Vec<(&str, f64)> = composed
        .drawing
        .prims
        .iter()
        .filter_map(|p| match p {
            cc_views::Prim::Text { text, size, .. } => Some((text.as_str(), *size)),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|(t, h)| *t == "Bia B2" && (*h - 4.0).abs() < 1e-9), "{texts:?}");
    assert!(composed.drawing.prims.iter().any(|p| matches!(p, cc_views::Prim::Image { .. })));
    // A field cleared falls back to the default.
    run(&mut s, "sheet_set", json!({"sheet": sheet, "fields": {"Drawn": null}}));
    let f = run(&mut s, "sheet_fields", json!({"sheet": sheet}));
    assert_eq!(f["params"][0]["value"], "Equipe");
    // A .ccomp round trip keeps the picture.
    let dir = std::env::temp_dir().join(format!("ccad-tb-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tb.ccomp");
    run(&mut s, "component_save", json!({"component": "Meu carimbo", "path": path}));
    let mut t = Session::default();
    let loaded = run(&mut t, "component_load", json!({"path": path}));
    let got = run(&mut t, "component_get", json!({"type": loaded["types"][0]}));
    assert!(got.to_string().contains("Image"));
    assert_eq!(t.project().title_blocks().len(), 2);
    std::fs::remove_dir_all(&dir).ok();
    run(&mut s, "component_sheet_piece_set", json!({"type": tb, "index": 0, "name": "Border"}));
    let got = run(&mut s, "component_get", json!({"type": tb}));
    assert_eq!(got["def"]["sheet"][0]["name"], "Border");
    run(&mut s, "component_sheet_piece_delete", json!({"type": tb, "index": 2}));
    let got = run(&mut s, "component_get", json!({"type": tb}));
    assert_eq!(got["def"]["sheet"].as_array().unwrap().len(), 2);
    assert_eq!(
        fails(&mut s, "component_sheet_piece_delete", json!({"type": tb, "index": 9})),
        ErrorCode::InvalidArguments
    );
    // A patch that leaves a stroke without points is refused.
    assert_eq!(
        fails(
            &mut s,
            "component_sheet_piece_set",
            json!({"type": tb, "index": 0, "piece": {"Stroke": {"points": []}}})
        ),
        ErrorCode::InvalidArguments
    );
    // Sheet pieces only go into title blocks.
    let door = run(&mut s, "type_get", json!({"type": "Porta 80x210"}));
    let text = json!({"kind": "text", "x": 0, "y": 0, "text": "x"});
    assert_eq!(
        fails(&mut s, "component_sheet_piece_add", json!({"type": door["id"], "piece": text})),
        ErrorCode::InvalidArguments
    );
}

#[test]
fn component_create_takes_a_title_block_with_sheet_pieces() {
    let mut s = Session::default();
    let text = json!({"Text": {"x": "5", "y": "5", "text": "{sheet_name}", "size": "3"}});
    let def = json!({"kind": "TitleBlock", "sheet": [{"name": "T", "shape": text}]});
    let tb = run(&mut s, "component_create", json!({"name": "Simples", "def": def}))["id"].as_u64().unwrap();
    assert!(s.project().title_blocks().contains(&cc_core::ElementTypeId(tb)));
    // A title block has no 3D pieces.
    let cube = json!({"Box": {"x": "0", "y": "0", "z": "0", "width": "1", "depth": "1", "height": "1"}});
    let bad = json!({"kind": "TitleBlock", "items": [{"name": "B", "shape": cube}]});
    assert_eq!(fails(&mut s, "component_create", json!({"name": "Mau", "def": bad})), ErrorCode::InvalidArguments);
}

#[test]
fn title_block_create_refuses_names_in_use() {
    let mut s = Session::default();
    let cube = json!({"Box": {"x": "0", "y": "0", "z": "0", "width": "1", "depth": "1", "height": "1"}});
    let def = json!({"items": [{"name": "B", "shape": cube}]});
    let id = run(&mut s, "component_create", json!({"name": "Mesa", "def": def}))["id"].clone();
    run(&mut s, "type_set", json!({"type": id, "name": "Mesa 80"}));
    // The component's name, and a type's name, are taken.
    assert_eq!(fails(&mut s, "title_block_create", json!({"name": "Mesa"})), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "title_block_create", json!({"name": " Mesa 80 "})), ErrorCode::InvalidArguments);
    assert_eq!(fails(&mut s, "title_block_create", json!({"name": "Alvenaria 15 cm"})), ErrorCode::InvalidArguments);
    let got = run(&mut s, "component_get", json!({"type": "Mesa 80"}));
    assert_eq!(got["def"]["items"].as_array().unwrap().len(), 1, "the other component is untouched");
    assert!(got["def"]["sheet"].as_array().is_none_or(Vec::is_empty));
    run(&mut s, "title_block_create", json!({"name": "Carimbo novo"}));
}

#[test]
fn deleting_a_title_block_moves_its_sheets_to_another() {
    let mut s = Session::default();
    let standard = s.project().title_blocks()[0].0;
    let other = run(&mut s, "title_block_create", json!({"name": "Carimbo B"}))["id"].as_u64().unwrap();
    let sheet = first_sheet(&mut s);
    run(&mut s, "sheet_set", json!({"sheet": sheet, "title_block": other}));
    run(&mut s, "type_delete", json!({"type": other}));
    let view = run(&mut s, "view_get", json!({"view": sheet}));
    assert_eq!(view["params"]["title_block"], standard, "on the remaining title block");
    // Without title block types left, sheets have none.
    run(&mut s, "type_delete", json!({"type": standard}));
    assert!(s.project().title_blocks().is_empty());
    let view = run(&mut s, "view_get", json!({"view": sheet}));
    assert!(view["params"]["title_block"].is_null());
    run(&mut s, "undo", json!({}));
    let view = run(&mut s, "view_get", json!({"view": sheet}));
    assert_eq!(view["params"]["title_block"], standard, "one undo step");
}

#[test]
fn renaming_a_text_parameter_keeps_the_sheets_values() {
    let mut s = Session::default();
    let tb = run(&mut s, "title_block_create", json!({"name": "Carimbo R", "empty": true}))["id"].as_u64().unwrap();
    run(&mut s, "component_param_add", json!({"type": tb, "name": "Drawn", "kind": "text", "instance": true}));
    run(
        &mut s,
        "component_sheet_piece_add",
        json!({"type": tb, "piece": {"kind": "text", "x": 20, "y": 20, "text": "{param:Drawn}"}}),
    );
    let sheet = first_sheet(&mut s);
    run(&mut s, "sheet_set", json!({"sheet": sheet, "title_block": tb, "fields": {"Drawn": "Bia"}}));
    run(&mut s, "component_param_set", json!({"type": tb, "name": "Drawn", "rename": "Author"}));
    let got = run(&mut s, "component_get", json!({"type": tb}));
    assert_eq!(got["def"]["sheet"][0]["shape"]["Text"]["text"], "{param:Author}");
    let view = run(&mut s, "view_get", json!({"view": sheet}));
    assert_eq!(view["params"]["fields"], json!({"Author": "Bia"}));
    let f = run(&mut s, "sheet_fields", json!({"sheet": sheet}));
    assert_eq!(f["params"][0]["value"], "Bia");
    // One undo step brings both back.
    run(&mut s, "undo", json!({}));
    let view = run(&mut s, "view_get", json!({"view": sheet}));
    assert_eq!(view["params"]["fields"], json!({"Drawn": "Bia"}));
}

#[test]
fn title_blocks_are_a_category_by_name() {
    let mut s = Session::default();
    let types = run(&mut s, "types_list", json!({"category": "titleblock"}));
    let types = types.as_array().unwrap();
    assert_eq!(types.len(), 1);
    assert_eq!(types[0]["category"], "titleblock");
    let cats = run(&mut s, "categories_list", json!({}));
    let tb = cats.as_array().unwrap().iter().find(|c| c["category"] == "titleblock").unwrap();
    assert_eq!(tb["types"], 1);
    // Views neither draw nor filter title blocks.
    let plan = s.project().views.values().find(|v| v.category() == cc_core::ViewCategory::FloorPlan).unwrap().id.0;
    assert_eq!(
        fails(&mut s, "view_graphics_set", json!({"view": plan, "category": "titleblock", "visible": false})),
        ErrorCode::InvalidArguments
    );
}

/// The picture of piece `index` of title block type `tb`.
fn picture(s: &Session, tb: u64, index: usize) -> std::sync::Arc<cc_core::raster::RasterImage> {
    let t = s.project().element_type(cc_core::ElementTypeId(tb)).unwrap();
    let cc_core::TypeData::Component { def, .. } = &t.data else { panic!("not a component") };
    match &def.sheet[index].shape {
        cc_core::title_block::SheetShape::Image { image, .. } => image.clone(),
        other => panic!("not an image: {other:?}"),
    }
}

#[test]
fn image_pieces_are_patched_without_their_bytes() {
    let mut s = Session::default();
    let tb = run(&mut s, "title_block_create", json!({"name": "Logo", "empty": true}))["id"].as_u64().unwrap();
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    let image = json!({"kind": "image", "x": 30, "y": 30, "width": 20, "height": 20, "image": png});
    run(&mut s, "component_sheet_piece_add", json!({"type": tb, "piece": image}));
    run(
        &mut s,
        "component_sheet_piece_add",
        json!({"type": tb, "piece": {"kind": "text", "x": 0, "y": 0, "text": "x"}}),
    );
    let before = picture(&s, tb, 0);
    run(&mut s, "component_sheet_piece_set", json!({"type": tb, "index": 0, "piece": {"Image": {"x": "40"}}}));
    let after = picture(&s, tb, 0);
    assert!(std::sync::Arc::ptr_eq(&before, &after), "the same picture, not a copy");
    let got = run(&mut s, "component_get", json!({"type": tb}));
    assert_eq!(got["def"]["sheet"][0]["shape"]["Image"]["x"], "40");
    // Queries summarize the picture instead of its bytes.
    let summary = json!({"format": "Png", "width": 1, "height": 1, "bytes": before.data.len()});
    assert_eq!(got["def"]["sheet"][0]["shape"]["Image"]["image"], summary);
    let ty = run(&mut s, "type_get", json!({"type": tb}));
    assert_eq!(ty["params"]["def"]["sheet"][0]["shape"]["Image"]["image"], summary);
    // A patch does not carry a picture; `image` or `image_path` replaces it.
    let raw = json!({"format": "Png", "width": 1, "height": 1, "data": png});
    assert_eq!(
        fails(&mut s, "component_sheet_piece_set", json!({"type": tb, "index": 0, "piece": {"Image": {"image": raw}}})),
        ErrorCode::InvalidArguments
    );
    assert!(std::sync::Arc::ptr_eq(&before, &picture(&s, tb, 0)));
    run(&mut s, "component_sheet_piece_set", json!({"type": tb, "index": 0, "image": png}));
    assert!(!std::sync::Arc::ptr_eq(&before, &picture(&s, tb, 0)));
    assert_eq!(*before, *picture(&s, tb, 0));
    assert_eq!(
        fails(&mut s, "component_sheet_piece_set", json!({"type": tb, "index": 1, "image": png})),
        ErrorCode::InvalidArguments
    );
}
