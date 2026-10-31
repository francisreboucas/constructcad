//! ConstructCAD — open source BIM authoring.

// Release builds on Windows open no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app;
mod browser;
mod cad_ui;
mod canvas;
mod clipboard;
mod commands;
mod component_editor;
mod component_env;
mod component_sketch;
mod dialog;
mod dialogs;
mod dim_edit;
mod engine_edit;
mod gpu;
mod graphics_dialog;
mod home;
mod i18n;
mod icons;
mod library;
mod material_browser;
mod modify;
mod modify3d;
mod paint;
mod palette;
mod phases_dialog;
mod preferences;
mod properties;
mod recovery;
mod ribbon;
mod schedule_ui;
mod sheet;
mod sketch_mode;
mod stair_ui;
mod state;
mod systems_dialog;
mod table;
mod text_tool;
mod theme;
mod thumbnails;
mod tools;
mod type_dialog;
#[cfg(test)]
mod ui_tests;
mod units_dialog;
mod update;
mod update_install;
mod view3d;
mod view_cube;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    // Panics are logged to the crash log (release builds on Windows have no console).
    recovery::install_panic_hook();
    let args = parse_args(std::env::args().skip(1));
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([900.0, 600.0])
        .with_title("ConstructCAD")
        .with_icon(app_icon());
    // The Wayland app id / X11 class names the desktop file (packaging/linux),
    // so desktops show its icon. eframe keeps its settings under the same
    // (lowercased) folder name on Linux, so nothing moves.
    #[cfg(target_os = "linux")]
    let viewport = viewport.with_app_id("constructcad");
    // The language of a start-up error message (the app then applies the saved preference).
    i18n::set_language(args.language.unwrap_or_else(i18n::Language::from_system));
    // Try the graphics APIs in turn until one shows the window (issue #74).
    let mut last_error = None;
    for api in gpu::startup_plan() {
        gpu::begin(api);
        let options = eframe::NativeOptions {
            viewport: viewport.clone(),
            renderer: eframe::Renderer::Wgpu,
            wgpu_options: gpu::wgpu_options(api),
            ..Default::default()
        };
        let args = args.clone();
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            eframe::run_native("ConstructCAD", options, Box::new(|cc| Ok(Box::new(app::ConstructApp::new(cc, args)))))
        }));
        match run {
            Ok(Ok(())) => return Ok(()),
            // eframe returned cleanly before the first frame: try the next API.
            Ok(Err(error)) if !gpu::started() => {
                gpu::failed(api, &error.to_string());
                last_error = Some(error);
            }
            Ok(Err(error)) => {
                recovery::log_event(&format!("The app stopped with an error: {error}"));
                return Err(error);
            }
            // A panic (already in the crash log) before the first frame:
            // restart the app, which tries the next API first.
            Err(_) if !gpu::started() && gpu::restart(api) => return Ok(()),
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }
    let error = last_error.unwrap_or_else(|| eframe::Error::AppCreation("no graphics API to try".into()));
    gpu::report_failure(&error.to_string());
    Err(error)
}

/// Window and taskbar icon.
fn app_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(theme::APP_ICON_PNG).unwrap_or_default()
}

/// `constructcad [project.ccad] [--view NAME]... [--lang en|pt-BR|es|fr|de|it|lo|pl|uk-UA|zh-TW|zh-CN]`
///
/// Named views replace the default layout (the first floor plan beside the
/// 3D view): the first on the left, the second on the right, others as tabs.
fn parse_args(args: impl Iterator<Item = String>) -> app::Args {
    let mut out = app::Args::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--view" => out.views.extend(args.next()),
            "--lang" => out.language = args.next().and_then(|c| i18n::Language::from_code(&c)),
            _ if out.path.is_none() => out.path = Some(std::path::PathBuf::from(arg)),
            _ => {}
        }
    }
    out
}
