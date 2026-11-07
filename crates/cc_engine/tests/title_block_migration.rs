//! Opening a format-20 file gives its sheets the standard title block, with
//! captions in the interface language. Its own test binary: the language is
//! process-wide.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{Category, Project, ViewKind};
use cc_engine::Session;
use cc_engine::i18n::{Language, set_language};

#[test]
fn localize_migrated_captions() {
    let mut p = Project::from_template();
    p.types.retain(|_, t| t.category() != Category::TitleBlock);
    for v in p.views.values_mut() {
        if let ViewKind::Sheet { title_block, .. } = &mut v.kind {
            *title_block = None;
        }
    }
    let text = cc_core::to_ron(&p)
        .unwrap()
        .replace(&format!("format_version: {}", cc_core::FORMAT_VERSION), "format_version: 20");
    let dir = std::env::temp_dir().join(format!("ccad-tb-migration-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("old.ccad");
    std::fs::write(&path, text).unwrap();

    set_language(Language::En);
    let (mut s, repairs) = Session::open(&path).unwrap();
    set_language(Language::PtBr);
    std::fs::remove_dir_all(&dir).ok();
    assert!(repairs >= 2, "the type and the sheet: {repairs}");
    let blocks = s.project().title_blocks();
    assert_eq!(blocks.len(), 1);
    assert_eq!(s.project().element_type(blocks[0]).unwrap().name, "Standard title block");
    let sheet = s.project().sheets_in_order()[0];
    let texts: Vec<String> = s
        .sheet(sheet)
        .unwrap()
        .drawing
        .prims
        .iter()
        .filter_map(|p| match p {
            cc_views::Prim::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    for t in ["PROJECT", "SHEET", "Project no. 0001"] {
        assert!(texts.iter().any(|x| x == t), "{t} in {texts:?}");
    }
}
