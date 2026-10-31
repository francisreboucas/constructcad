//! Importing pictures and SVG drawings into 2D views: size from the file's
//! resolution, placement, scale by width, drawing, saving and exporting.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::ElementKind;
use cc_engine::{ErrorCode, Session};
use cc_views::{Drawing2D, Prim};
use glam::DVec2;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn fails(s: &mut Session, name: &str, args: Value) -> ErrorCode {
    s.execute(name, args).map(|v| panic!("{name} should fail, gave {v}")).unwrap_err().code
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ccad-image-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn plan(s: &mut Session) -> u64 {
    run(s, "views_list", json!({"category": "floor_plan"}))[0]["id"].as_u64().unwrap()
}

/// A PNG 100 x 50 pixels at 100 dpi (so 25.4 x 12.7 mm on paper): a filled
/// square drawn on white by the raster exporter, which records its resolution.
fn sample_png(path: &std::path::Path) {
    let mut d = Drawing2D::default();
    d.prims.push(Prim::Fill {
        pts: vec![DVec2::new(2.0, 2.0), DVec2::new(10.0, 2.0), DVec2::new(10.0, 10.0), DVec2::new(2.0, 10.0)],
        color: [200, 30, 30, 255],
        element: None,
        cut: false,
    });
    let png = cc_views::export::to_png(&d, DVec2::new(25.4, 12.7), 100.0).unwrap();
    std::fs::write(path, png).unwrap();
}

fn extents(out: &Value) -> (DVec2, DVec2) {
    let e = &out["extents"];
    let p = |v: &Value| DVec2::new(v[0].as_f64().unwrap(), v[1].as_f64().unwrap());
    (p(&e[0]), p(&e[1]))
}

fn close(a: DVec2, b: DVec2) -> bool {
    a.distance(b) < 1e-6
}

#[test]
fn picture_is_sized_by_its_resolution_and_placed() {
    let path = scratch("square.png");
    sample_png(&path);
    let mut s = Session::default();
    let view = plan(&mut s);
    let out = run(&mut s, "image_import", json!({"path": path, "view": view, "position": [3.0, 4.0]}));
    assert_eq!(out["kind"], "picture");
    assert_eq!(out["pixels"], json!([100, 50]));
    assert!((out["dpi"].as_f64().unwrap() - 100.0).abs() < 0.1, "{out}");
    // 25.4 x 12.7 mm, centered on the position.
    let (lo, hi) = extents(&out);
    assert!(close(hi - lo, DVec2::new(0.0254, 0.0127)), "{lo} {hi}");
    assert!(close((lo + hi) * 0.5, DVec2::new(3.0, 4.0)));

    // The view draws it as a picture of the element, pickable inside it.
    let id = out["id"].as_u64().unwrap();
    let d = s.drawing(cc_core::ViewId(view)).unwrap();
    let image = d.prims.iter().find_map(|p| match p {
        Prim::Image { origin, u, v, element, .. } if *element == Some(cc_core::ElementId(id)) => {
            Some((*origin, *u, *v))
        }
        _ => None,
    });
    let (origin, u, v) = image.unwrap();
    assert!(close(origin, lo) && close(u, DVec2::new(0.0254, 0.0)) && close(v, DVec2::new(0.0, 0.0127)));
    assert_eq!(d.hit_test(DVec2::new(3.0, 4.0), 1e-4), Some(cc_core::ElementId(id)));

    // A width in the view scales it; the origin goes at the position.
    let out = run(
        &mut s,
        "image_import",
        json!({"path": path, "view": view, "width": 10.0, "align": "origin", "position": [0.0, 0.0], "rotation": 90.0}),
    );
    let (lo, hi) = extents(&out);
    // (The PNG stores its resolution in whole dots per meter: 100 dpi reads back as 99.9998.)
    assert!((out["scale"].as_f64().unwrap() / (10.0 / 0.0254) - 1.0).abs() < 1e-4);
    // Turned a quarter: 10 m wide becomes 10 m tall, to the left of the origin.
    assert!(close(lo, DVec2::new(-5.0, 0.0)) && close(hi, DVec2::new(0.0, 10.0)), "{lo} {hi}");

    // One undo step each.
    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));
    assert!(!s.project().elements.values().any(|e| matches!(e.kind, ElementKind::CadImport(_))));
}

#[test]
fn picture_survives_saving_and_exports() {
    let path = scratch("saved.png");
    sample_png(&path);
    let bytes = std::fs::read(&path).unwrap();
    let mut s = Session::default();
    let view = plan(&mut s);
    run(&mut s, "image_import", json!({"path": path, "view": view, "width": 4.0}));
    let file = scratch("with-image.ccad");
    run(&mut s, "project_save", json!({"path": file}));
    let mut reopened = Session::default();
    run(&mut reopened, "project_open", json!({"path": file}));
    let kept = reopened.project().elements.values().find_map(|e| match &e.kind {
        ElementKind::CadImport(c) => c.drawing.images.first().map(|i| i.image.data.clone()),
        _ => None,
    });
    assert_eq!(kept.as_deref(), Some(&bytes[..]), "the file's bytes are kept unchanged");

    // Exports carry the picture.
    let svg = scratch("plan.svg");
    run(&mut reopened, "view_export", json!({"view": view, "path": svg}));
    assert!(std::fs::read_to_string(&svg).unwrap().contains("data:image/png;base64,"));
    let pdf = scratch("plan.pdf");
    run(&mut reopened, "view_export", json!({"view": view, "path": pdf}));
    let pdf = std::fs::read(&pdf).unwrap();
    let has = |needle: &[u8]| pdf.windows(needle.len()).any(|w| w == needle);
    assert!(has(b"/Subtype /Image") && has(b"/Im0 Do"));
    let png = scratch("plan.png");
    run(&mut reopened, "view_export", json!({"view": view, "path": png, "dpi": 50}));
    assert!(std::fs::metadata(&png).unwrap().len() > 0);
}

#[test]
fn svg_is_read_as_a_drawing() {
    let path = scratch("plan.svg");
    std::fs::write(
        &path,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96">
            <rect x="0" y="0" width="96" height="96" fill="none" stroke="rgb(0,128,0)"/>
            <path d="M 0 96 L 96 0" stroke="black"/></svg>"#,
    )
    .unwrap();
    let mut s = Session::default();
    let view = plan(&mut s);
    let out = run(&mut s, "image_import", json!({"path": path, "view": view, "scale": 100.0, "align": "origin"}));
    assert_eq!(out["kind"], "svg");
    assert_eq!(out["primitives"], 2);
    // One inch on paper at 1:100: 2.54 m.
    let (lo, hi) = extents(&out);
    assert!(close(lo, DVec2::ZERO) && close(hi, DVec2::splat(2.54)), "{lo} {hi}");
    // Drawn in the file's own colors.
    let c = s.project().elements.values().find_map(|e| match &e.kind {
        ElementKind::CadImport(c) => Some(c.color_mode),
        _ => None,
    });
    assert_eq!(c, Some(cc_core::cad::CadColorMode::Layers));
}

#[test]
fn bad_files_and_views_are_refused() {
    let mut s = Session::default();
    let view = plan(&mut s);
    let text = scratch("notes.png");
    std::fs::write(&text, b"just some text").unwrap();
    assert_eq!(fails(&mut s, "image_import", json!({"path": text, "view": view})), ErrorCode::InvalidArguments);
    // A PNG cut short: its header promises pixels that are not there.
    let full = scratch("full.png");
    sample_png(&full);
    let mut cut = std::fs::read(&full).unwrap();
    cut.truncate(40);
    let cut_path = scratch("cut.png");
    std::fs::write(&cut_path, cut).unwrap();
    let _ = s.execute("image_import", json!({"path": cut_path, "view": view}));
    // Whatever it made still draws (as a grey box at worst) and saves.
    let _ = s.drawing(cc_core::ViewId(view));
    // Not in a 3D view, and not with a zero width.
    let three_d = run(&mut s, "views_list", json!({"category": "three_d"}))[0]["id"].as_u64().unwrap();
    assert_eq!(fails(&mut s, "image_import", json!({"path": full, "view": three_d})), ErrorCode::InvalidArguments);
    assert_eq!(
        fails(&mut s, "image_import", json!({"path": full, "view": view, "width": 0.0})),
        ErrorCode::InvalidArguments
    );
    assert!(s.execute("image_import", json!({"path": scratch("missing.png"), "view": view})).is_err());
}
