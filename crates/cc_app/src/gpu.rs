//! Graphics start-up: which wgpu backends the window tries, in what order,
//! and what its device asks of the adapter.
//!
//! The native APIs (Vulkan, DirectX 12, Metal) are tried first and OpenGL
//! after them, unless `WGPU_BACKEND` names the backends to use. A start-up
//! that fails with an error moves on to the next API in the same process; one
//! that panics before the first frame restarts the app once; one that kills
//! the process (a driver crash) leaves a marker file behind, so the next start
//! tries the other API first. Once OpenGL has been needed it stays first
//! (`gpu-backend` in the data folder; delete it to try the native APIs again).
//! Every failure is written to the crash log.

use crate::recovery;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// Marker written before a start-up and removed after the first frame.
const MARKER_FILE: &str = "gpu-startup";
/// The API to try first, kept once OpenGL was needed.
const SAVED_FILE: &str = "gpu-backend";
/// Set in the environment of an app restarted after a start-up panic.
const RESTARTED_ENV: &str = "CONSTRUCTCAD_GPU_RESTARTED";

/// A family of graphics backends to start the window with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Api {
    /// The platform's native APIs (Vulkan, DirectX 12, Metal).
    Native,
    /// OpenGL.
    Gl,
    /// The backends named by the `WGPU_BACKEND` environment variable.
    Env(wgpu::Backends),
}

impl Api {
    pub fn backends(self) -> wgpu::Backends {
        match self {
            Api::Native => wgpu::Backends::PRIMARY,
            Api::Gl => wgpu::Backends::GL,
            Api::Env(backends) => backends,
        }
    }

    fn code(self) -> &'static str {
        match self {
            Api::Native => "native",
            Api::Gl => "gl",
            Api::Env(_) => "env",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        match code.trim() {
            "native" => Some(Api::Native),
            "gl" => Some(Api::Gl),
            _ => None,
        }
    }

    fn describe(self) -> String {
        match self {
            Api::Native => "the native graphics API (Vulkan/DirectX 12/Metal)".to_string(),
            Api::Gl => "OpenGL".to_string(),
            Api::Env(backends) => format!("WGPU_BACKEND ({backends:?})"),
        }
    }
}

/// The APIs to try, in order: only those of `WGPU_BACKEND` when it names any
/// (`env`); otherwise the native APIs and OpenGL, the other API first when the
/// last start-up died with one (`interrupted`), else OpenGL first once it was
/// needed (`saved`).
pub fn plan(env: Option<wgpu::Backends>, saved: Option<Api>, interrupted: Option<Api>) -> Vec<Api> {
    if let Some(backends) = env.filter(|b| !b.is_empty()) {
        return vec![Api::Env(backends)];
    }
    let first = match (interrupted, saved) {
        (Some(Api::Native), _) => Api::Gl,
        (Some(Api::Gl), _) => Api::Native,
        (_, Some(Api::Gl)) => Api::Gl,
        _ => Api::Native,
    };
    let second = if first == Api::Native { Api::Gl } else { Api::Native };
    vec![first, second]
}

/// The limits the device asks for: wgpu's defaults (WebGL 2's on OpenGL) with
/// textures up to 8192 px (a depth buffer as large as a 4K+ window), lowered to
/// what the adapter supports, since asking for more than it has fails the start.
pub fn required_limits(backend: wgpu::Backend, adapter: &wgpu::Limits) -> wgpu::Limits {
    let base =
        if backend == wgpu::Backend::Gl { wgpu::Limits::downlevel_webgl2_defaults() } else { wgpu::Limits::default() };
    wgpu::Limits { max_texture_dimension_2d: 8192, ..base }.or_worse_values_from(adapter)
}

/// The device of the window: limits within the adapter's, and small memory
/// blocks (the default asks for blocks of 128 to 256 MB, as much as a small
/// integrated GPU has).
fn device_descriptor(adapter: &wgpu::Adapter) -> wgpu::DeviceDescriptor<'static> {
    wgpu::DeviceDescriptor {
        label: Some("ConstructCraft device"),
        required_limits: required_limits(adapter.get_info().backend, &adapter.limits()),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        ..Default::default()
    }
}

/// eframe's wgpu options for a start-up with `api`.
pub fn wgpu_options(api: Api) -> egui_wgpu::WgpuConfiguration {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = api.backends();
    setup.device_descriptor = Arc::new(device_descriptor);
    egui_wgpu::WgpuConfiguration { wgpu_setup: setup.into(), ..Default::default() }
}

/// The start-up under way (the API and the data folder of its marker).
static CURRENT: Mutex<Option<(Api, Option<PathBuf>)>> = Mutex::new(None);
/// Frames shown since the start-up began.
static FRAMES: AtomicU32 = AtomicU32::new(0);

/// The APIs to try at this start, from `WGPU_BACKEND` and the files of earlier
/// start-ups (a marker left behind by one that died is logged).
pub fn startup_plan() -> Vec<Api> {
    let dir = recovery::data_dir();
    let interrupted = dir.as_deref().and_then(read_marker);
    let saved = dir.as_deref().and_then(|d| read_api(&d.join(SAVED_FILE)));
    let env = wgpu::Backends::from_env();
    let plan = plan(env, saved, interrupted);
    if let Some(api) = interrupted {
        let next = plan.first().map_or_else(String::new, |a| a.describe());
        recovery::log_event(&format!(
            "The previous start ended while starting the graphics with {}; trying {next} first.",
            api.describe()
        ));
    }
    plan
}

/// A start-up with `api` begins: write its marker.
pub fn begin(api: Api) {
    let dir = recovery::data_dir();
    if let Some(dir) = &dir {
        write_marker(dir, api);
    }
    FRAMES.store(0, Ordering::SeqCst);
    if let Ok(mut current) = CURRENT.lock() {
        *current = Some((api, dir));
    }
}

/// Whether the start-up under way has shown its first frame.
pub fn started() -> bool {
    FRAMES.load(Ordering::SeqCst) >= 2
}

/// Called at the start of every frame: at the second, the first has been
/// shown, so the start-up worked; remove its marker and keep OpenGL first if
/// it took OpenGL.
pub fn note_frame() {
    if FRAMES.fetch_add(1, Ordering::SeqCst) != 1 {
        return;
    }
    let Ok(current) = CURRENT.lock() else { return };
    if let Some((api, Some(dir))) = current.as_ref() {
        succeeded(dir, *api);
    }
}

/// A start-up with `api` failed with an error: log it and remove its marker.
pub fn failed(api: Api, error: &str) {
    recovery::log_event(&format!("Could not start the graphics with {}: {error}", api.describe()));
    if let Some(dir) = recovery::data_dir() {
        let _ = std::fs::remove_file(dir.join(MARKER_FILE));
    }
}

/// After a panic before the first frame: start the app again (once), which
/// then tries the other API first (the marker is still there). Returns whether
/// it was started.
pub fn restart(api: Api) -> bool {
    if matches!(api, Api::Env(_)) || std::env::var_os(RESTARTED_ENV).is_some() {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else { return false };
    recovery::log_event(&format!("The graphics start-up with {} panicked; restarting.", api.describe()));
    std::process::Command::new(exe).args(std::env::args_os().skip(1)).env(RESTARTED_ENV, "1").spawn().is_ok()
}

/// Every API failed: log it and tell the user (the app has no window).
pub fn report_failure(error: &str) {
    recovery::log_event(&format!("Could not start the graphics with any API: {error}"));
    let log = recovery::crash_log().map(|p| p.display().to_string()).unwrap_or_default();
    let _ = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(crate::t!("gpu-failed-title"))
        .set_description(crate::t!("gpu-failed-text", "error" => error, "log" => log))
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

fn read_api(path: &Path) -> Option<Api> {
    std::fs::read_to_string(path).ok().and_then(|s| Api::from_code(&s))
}

fn read_marker(dir: &Path) -> Option<Api> {
    read_api(&dir.join(MARKER_FILE))
}

fn write_marker(dir: &Path, api: Api) {
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(dir.join(MARKER_FILE), api.code());
}

/// A start-up with `api` showed its first frame.
fn succeeded(dir: &Path, api: Api) {
    let _ = std::fs::remove_file(dir.join(MARKER_FILE));
    match api {
        Api::Gl => {
            let _ = std::fs::write(dir.join(SAVED_FILE), api.code());
        }
        Api::Native => {
            let _ = std::fs::remove_file(dir.join(SAVED_FILE));
        }
        Api::Env(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::Backends;

    #[test]
    fn native_apis_come_first_then_opengl() {
        assert_eq!(plan(None, None, None), vec![Api::Native, Api::Gl]);
        assert_eq!(plan(None, Some(Api::Native), None), vec![Api::Native, Api::Gl]);
    }

    #[test]
    fn wgpu_backend_is_honored_alone() {
        assert_eq!(plan(Some(Backends::GL), Some(Api::Native), Some(Api::Gl)), vec![Api::Env(Backends::GL)]);
        assert_eq!(plan(Some(Backends::DX12), None, None), vec![Api::Env(Backends::DX12)]);
        // An unknown name selects no backend: ignore it.
        assert_eq!(plan(Some(Backends::empty()), None, None), vec![Api::Native, Api::Gl]);
    }

    #[test]
    fn a_start_up_that_died_is_tried_last() {
        assert_eq!(plan(None, None, Some(Api::Native)), vec![Api::Gl, Api::Native]);
        // OpenGL died too, though it was kept first: the native APIs again.
        assert_eq!(plan(None, Some(Api::Gl), Some(Api::Gl)), vec![Api::Native, Api::Gl]);
    }

    #[test]
    fn opengl_stays_first_once_needed() {
        assert_eq!(plan(None, Some(Api::Gl), None), vec![Api::Gl, Api::Native]);
    }

    #[test]
    fn limits_never_exceed_the_adapter() {
        // A weak adapter: smaller textures and buffers than wgpu's defaults.
        let weak = wgpu::Limits {
            max_texture_dimension_2d: 4096,
            max_buffer_size: 128 << 20,
            max_storage_buffers_per_shader_stage: 4,
            min_uniform_buffer_offset_alignment: 512,
            ..wgpu::Limits::default()
        };
        for backend in [wgpu::Backend::Dx12, wgpu::Backend::Vulkan, wgpu::Backend::Gl] {
            let limits = required_limits(backend, &weak);
            assert!(limits.check_limits(&weak), "{backend:?}: {limits:?}");
            assert_eq!(limits.max_texture_dimension_2d, 4096);
            assert_eq!(limits.min_uniform_buffer_offset_alignment, 512);
        }
        // A capable adapter gets the defaults with 8192 px textures.
        let strong = wgpu::Limits { max_texture_dimension_2d: 16384, ..wgpu::Limits::default() };
        let limits = required_limits(wgpu::Backend::Dx12, &strong);
        assert_eq!(limits.max_texture_dimension_2d, 8192);
        assert_eq!(limits.max_buffer_size, wgpu::Limits::default().max_buffer_size);
        let gl = required_limits(wgpu::Backend::Gl, &strong);
        assert_eq!(gl.max_storage_buffers_per_shader_stage, 0);
        assert_eq!(gl.max_texture_dimension_2d, 8192);
    }

    #[test]
    fn markers_record_start_ups_and_opengl_is_kept() {
        let dir = std::env::temp_dir().join(format!("ccad-app-gpu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_marker(&dir, Api::Native);
        assert_eq!(read_marker(&dir), Some(Api::Native));
        // The native start-up died; OpenGL worked and stays first.
        succeeded(&dir, Api::Gl);
        assert_eq!(read_marker(&dir), None);
        assert_eq!(read_api(&dir.join(SAVED_FILE)), Some(Api::Gl));
        // The native APIs work again: back to the default order.
        succeeded(&dir, Api::Native);
        assert_eq!(read_api(&dir.join(SAVED_FILE)), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
