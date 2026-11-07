//! One-click updates: download the package of a newer release that fits
//! how this copy was installed, verify it, and put it in place.
//!
//! Verification is never skipped: the package's SHA-256 must match the
//! release's `SHA256SUMS.txt`, both fetched over HTTPS from the release's
//! own download links on GitHub. A package that does not match is deleted
//! and nothing is replaced. What "put in place" means:
//!
//! - AppImage: written next to the running AppImage, then renamed over it
//!   (one file system, so the swap is atomic; the running copy keeps its
//!   file), and the app offers to restart.
//! - Windows installer (`.msi`, under Program Files): run `msiexec /passive`
//!   when the app closes; ConstructCraft starts again when it is done.
//! - Windows `.zip` (anywhere else on Windows, `ccad.exe` beside the app):
//!   the archive is unpacked in a staging folder, the running executables
//!   are renamed to `.old` (Windows allows renaming a running program) and
//!   the new ones take their names; the `.old` files go at the next start.
//! - macOS app: the `.dmg` is opened; the user drags the app to Applications.
//! - `.deb`: never replaced; the window shows the `apt` command.
//! - Anything else (`.tar.gz`, builds from source): Download only.

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

/// Packages and the checksum list must come from this repository's releases.
pub const DOWNLOAD_PREFIX: &str = "https://github.com/francisreboucas/constructcraft/releases/download/";
/// The same repository under its former name: GitHub answers with links under
/// it until the repository is renamed, so both are accepted.
const FORMER_DOWNLOAD_PREFIX: &str = "https://github.com/francisreboucas/constructcad/releases/download/";

/// What follows the release download prefix in `url`, if it has one.
fn release_path(url: &str) -> Option<&str> {
    url.strip_prefix(DOWNLOAD_PREFIX).or_else(|| url.strip_prefix(FORMER_DOWNLOAD_PREFIX))
}
/// The checksum list every release carries.
pub const SUMS: &str = "SHA256SUMS.txt";
/// No package is bigger than this (they are tens of megabytes).
const MAX_PACKAGE: u64 = 1 << 30;
/// Nor the checksum list.
const MAX_SUMS: u64 = 64 * 1024;
/// A stalled download gives up after this long without data.
const READ_TIMEOUT: Duration = Duration::from_secs(60);
/// The release response lists at most this many assets.
const MAX_ASSETS: usize = 200;

/// A file attached to a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
}

/// The assets of a release response that can be downloaded safely: plain
/// file names, links under [`DOWNLOAD_PREFIX`].
pub fn assets_of(release: &serde_json::Value) -> Vec<Asset> {
    let Some(list) = release.get("assets").and_then(|a| a.as_array()) else { return Vec::new() };
    list.iter()
        .take(MAX_ASSETS)
        .filter_map(|a| {
            let name = a.get("name")?.as_str()?;
            let url = a.get("browser_download_url")?.as_str()?;
            let plain = !name.is_empty()
                && name.len() < 200
                && !name.starts_with('.')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || "._-+~".contains(c));
            let ours = release_path(url).is_some_and(|rest| {
                rest.split_once('/').is_some_and(|(tag, file)| !tag.is_empty() && !tag.contains("..") && file == name)
            });
            (plain && ours).then(|| Asset { name: name.to_string(), url: url.to_string() })
        })
        .collect()
}

/// How this copy of ConstructCraft was installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallKind {
    /// An AppImage (the file).
    AppImage(PathBuf),
    /// The Windows installer, under Program Files.
    Msi,
    /// The Windows `.zip`: this executable (with `ccad.exe` beside it).
    Portable(PathBuf),
    /// A macOS `.app` (from the `.dmg`).
    MacApp,
    /// The Debian package, under `/usr`.
    Deb,
    /// Anything else: Download only.
    Other,
}

/// Tell how a copy was installed from the OS (`std::env::consts::OS`), its
/// executable and the environment (`env`).
pub fn detect(os: &str, exe: Option<&Path>, env: impl Fn(&str) -> Option<String>) -> InstallKind {
    let set = |name: &str| env(name).filter(|v| !v.is_empty());
    match os {
        "linux" => {
            if let Some(image) = set("APPIMAGE") {
                return InstallKind::AppImage(PathBuf::from(image));
            }
            match exe {
                Some(exe) if exe.starts_with("/usr") => InstallKind::Deb,
                _ => InstallKind::Other,
            }
        }
        "windows" => {
            let Some(exe) = exe else { return InstallKind::Other };
            let program_files = ["ProgramW6432", "ProgramFiles"].iter().filter_map(|v| set(v)).map(PathBuf::from);
            let lower = |p: &Path| p.to_string_lossy().to_lowercase();
            if program_files.into_iter().any(|dir| lower(exe).starts_with(&lower(&dir))) {
                return InstallKind::Msi;
            }
            match exe.parent() {
                Some(dir) if dir.join("ccad.exe").is_file() => InstallKind::Portable(exe.to_path_buf()),
                _ => InstallKind::Other,
            }
        }
        "macos" => match exe {
            Some(exe) if exe.ancestors().any(|a| a.extension().is_some_and(|e| e == "app")) => InstallKind::MacApp,
            _ => InstallKind::Other,
        },
        _ => InstallKind::Other,
    }
}

/// This copy's installation.
pub fn this_install() -> InstallKind {
    let exe = std::env::current_exe().ok();
    detect(std::env::consts::OS, exe.as_deref(), |name| std::env::var(name).ok())
}

/// The release package for `kind` on `arch` (`std::env::consts::ARCH`), by the
/// end of its name (release names carry the version).
pub fn package_for<'a>(kind: &InstallKind, arch: &str, assets: &'a [Asset]) -> Option<&'a Asset> {
    let suffix = match (kind, arch) {
        (InstallKind::AppImage(_), "x86_64" | "aarch64") => format!("-{arch}.AppImage"),
        (InstallKind::Msi, "x86_64") => "-x86_64.msi".to_string(),
        (InstallKind::Portable(_), "x86_64") => "-x86_64-pc-windows-msvc.zip".to_string(),
        (InstallKind::MacApp, "x86_64" | "aarch64") => format!("-{arch}-apple-darwin.dmg"),
        (InstallKind::Deb, "x86_64") => "_amd64.deb".to_string(),
        (InstallKind::Deb, "aarch64") => "_arm64.deb".to_string(),
        _ => return None,
    };
    assets.iter().find(|a| a.name.ends_with(&suffix))
}

/// Whether Update Now can install for `kind` (the others get Download, or a command).
pub fn installs_itself(kind: &InstallKind) -> bool {
    matches!(kind, InstallKind::AppImage(_) | InstallKind::Msi | InstallKind::Portable(_) | InstallKind::MacApp)
}

/// The command that installs a downloaded `.deb`.
pub fn deb_command(asset: &Asset) -> String {
    format!("sudo apt install ./{}", asset.name)
}

/// The lowercase hex SHA-256 that `SHA256SUMS.txt` (`<hex>  <name>` or
/// `<hex> *<name>` lines) gives for `name`.
pub fn expected_sha256(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hex, file) = line.trim_end_matches('\r').split_once(char::is_whitespace)?;
        let file = file.trim_start().trim_start_matches('*');
        let valid = hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit());
        (valid && file == name).then(|| hex.to_ascii_lowercase())
    })
}

/// The lowercase hex of a digest.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Progress of an update, shared with the window that shows it.
#[derive(Debug, Default)]
pub struct Progress {
    pub done: AtomicU64,
    /// The package's size when the server gives it (0: unknown).
    pub total: AtomicU64,
    pub cancel: AtomicBool,
}

/// What to do once the package is in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    /// The new version replaced this one: start this program when the app closes.
    Restart(PathBuf),
    /// Run this installer (`.msi`) when the app closes, then start the app.
    Installer(PathBuf),
    /// The disk image was opened: the user finishes in the Finder.
    Opened,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_body(Some(READ_TIMEOUT))
        .user_agent(format!("ConstructCraft/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// GET `url` (under [`DOWNLOAD_PREFIX`]) as text, at most `limit` bytes.
fn get_text(agent: &ureq::Agent, url: &str, limit: u64) -> Result<String, String> {
    if release_path(url).is_none() {
        return Err(format!("not a release link: {url}"));
    }
    let mut response = agent.get(url).call().map_err(|e| e.to_string())?;
    response.body_mut().with_config().limit(limit).read_to_string().map_err(|e| e.to_string())
}

/// Download `url` to `path`, reporting progress, and return its SHA-256.
fn download(agent: &ureq::Agent, url: &str, path: &Path, progress: &Progress) -> Result<String, String> {
    if release_path(url).is_none() {
        return Err(format!("not a release link: {url}"));
    }
    let mut response = agent.get(url).call().map_err(|e| e.to_string())?;
    let total = response.body().content_length().unwrap_or(0);
    if total > MAX_PACKAGE {
        return Err(format!("the package is too large ({total} bytes)"));
    }
    progress.total.store(total, Ordering::Relaxed);
    let mut reader = response.body_mut().with_config().limit(MAX_PACKAGE).reader();
    let mut file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        if progress.cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        let Some(chunk) = buf.get(..n) else { break };
        if chunk.is_empty() {
            break;
        }
        hasher.update(chunk);
        file.write_all(chunk).map_err(|e| format!("{}: {e}", path.display()))?;
        progress.done.fetch_add(n as u64, Ordering::Relaxed);
    }
    file.sync_all().map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(hex(&hasher.finalize()))
}

/// A folder for downloads that are not written next to their target.
fn staging() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("constructcraft-update-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

/// `path` with `suffix` added to its file name.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// Download, verify and put in place `package` of the release whose checksum
/// list is `sums_url`, for this installation. Runs on a background thread.
pub fn install(kind: &InstallKind, package: &Asset, sums_url: &str, progress: &Progress) -> Result<Next, String> {
    let agent = agent();
    let sums = get_text(&agent, sums_url, MAX_SUMS)?;
    let expected =
        expected_sha256(&sums, &package.name).ok_or_else(|| format!("{SUMS} does not list {}", package.name))?;
    let part = match kind {
        InstallKind::AppImage(target) => with_suffix(target, ".update"),
        _ => staging()?.join(&package.name),
    };
    let fail = |e: String| {
        let _ = std::fs::remove_file(&part);
        e
    };
    let actual = download(&agent, &package.url, &part, progress).map_err(fail)?;
    if actual != expected {
        return Err(fail(format!("{} does not match {SUMS}: the download was not installed", package.name)));
    }
    put_in_place(kind, &part).map_err(fail)
}

/// Put a verified package in place for `kind`.
fn put_in_place(kind: &InstallKind, package: &Path) -> Result<Next, String> {
    match kind {
        InstallKind::AppImage(target) => {
            make_executable(package)?;
            std::fs::rename(package, target).map_err(|e| format!("{}: {e}", target.display()))?;
            Ok(Next::Restart(target.clone()))
        }
        InstallKind::Msi => Ok(Next::Installer(package.to_path_buf())),
        InstallKind::Portable(exe) => {
            let dir = package.with_extension("unpacked");
            unzip(package, &dir)?;
            let new = find_file(&dir, "constructcraft.exe", 3).ok_or("the package has no constructcraft.exe")?;
            swap_executable(exe, &new)?;
            if let (Some(cli), Some(new_cli)) = (exe.parent().map(|d| d.join("ccad.exe")), new.parent()) {
                let new_cli = new_cli.join("ccad.exe");
                if cli.is_file() && new_cli.is_file() {
                    swap_executable(&cli, &new_cli)?;
                }
            }
            let _ = std::fs::remove_dir_all(&dir);
            Ok(Next::Restart(exe.clone()))
        }
        InstallKind::MacApp => {
            std::process::Command::new("open").arg(package).spawn().map_err(|e| format!("open: {e}"))?;
            Ok(Next::Opened)
        }
        InstallKind::Deb | InstallKind::Other => Err("this installation is not updated from the app".into()),
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Unpack a `.zip` with the system's own tool (PowerShell on Windows).
fn unzip(zip: &Path, into: &Path) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(into);
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg(format!(
            "Expand-Archive -LiteralPath {} -DestinationPath {} -Force",
            ps_quote(&zip.to_string_lossy()),
            ps_quote(&into.to_string_lossy())
        ))
        .status()
        .map_err(|e| format!("powershell: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("could not unpack {}", zip.display())) }
}

/// `name` in `dir` or a folder below it, at most `depth` levels down.
fn find_file(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut folders = Vec::new();
    for entry in entries.flatten().take(1000) {
        let path = entry.path();
        if path.is_dir() {
            folders.push(path);
        } else if path.file_name().is_some_and(|n| n.eq_ignore_ascii_case(name)) {
            return Some(path);
        }
    }
    if depth == 0 {
        return None;
    }
    folders.into_iter().find_map(|f| find_file(&f, name, depth - 1))
}

/// Replace the (possibly running) executable `current` with `new`: `current`
/// is renamed to `<name>.old` first, and put back if the new one can't take
/// its place.
pub fn swap_executable(current: &Path, new: &Path) -> Result<(), String> {
    let old = with_suffix(current, ".old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(current, &old).map_err(|e| format!("{}: {e}", current.display()))?;
    if let Err(e) = std::fs::rename(new, current).or_else(|_| std::fs::copy(new, current).map(|_| ())) {
        let _ = std::fs::rename(&old, current);
        return Err(format!("{}: {e}", current.display()));
    }
    Ok(())
}

/// Remove the `.old` executables a previous update left beside `exe`.
pub fn remove_old_executables(exe: &Path) {
    let Some(dir) = exe.parent() else { return };
    let _ = std::fs::remove_file(with_suffix(exe, ".old"));
    let _ = std::fs::remove_file(dir.join("ccad.exe.old"));
}

/// A PowerShell single-quoted string.
fn ps_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// The command that runs the installer `msi`, waits for it and then starts
/// `exe` (the installed program). If the user cancels the installer, the old
/// version is still there to start.
pub fn installer_command(msi: &Path, exe: &Path) -> (String, Vec<String>) {
    // msiexec splits its own command line, so the path needs double quotes inside the argument.
    let msi_arg = format!("\"{}\"", msi.to_string_lossy());
    let script = format!(
        "Start-Process -FilePath 'msiexec.exe' -ArgumentList '/i',{},'/passive' -Wait; Start-Process -FilePath {}",
        ps_quote(&msi_arg),
        ps_quote(&exe.to_string_lossy())
    );
    ("powershell".into(), vec!["-NoProfile".into(), "-WindowStyle".into(), "Hidden".into(), "-Command".into(), script])
}

/// Start what an update left to do, as the app closes.
pub fn finish(next: &Next) {
    let started = match next {
        Next::Restart(program) => std::process::Command::new(program).spawn().map(|_| ()),
        Next::Installer(msi) => {
            let exe = std::env::current_exe().unwrap_or_default();
            let (program, args) = installer_command(msi, &exe);
            std::process::Command::new(program).args(args).spawn().map(|_| ())
        }
        Next::Opened => Ok(()),
    };
    if let Err(e) = started {
        log::warn!("could not finish the update: {e}");
    }
}

/// A running update: its progress, and the result when it is done.
pub struct Running {
    pub progress: Arc<Progress>,
    result: std::sync::mpsc::Receiver<Result<Next, String>>,
}

impl std::fmt::Debug for Running {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Running").field("progress", &self.progress).finish()
    }
}

impl Running {
    /// Start installing `package` on a background thread.
    pub fn start(ctx: &egui::Context, kind: InstallKind, package: Asset, sums_url: String) -> Result<Running, String> {
        let progress = Arc::new(Progress::default());
        let (tx, rx) = std::sync::mpsc::channel();
        let shared = progress.clone();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("update-install".into())
            .spawn(move || {
                let result = install(&kind, &package, &sums_url, &shared);
                if let Err(e) = &result {
                    log::warn!("update failed: {e}");
                }
                let _ = tx.send(result);
                ctx.request_repaint();
            })
            .map_err(|e| e.to_string())?;
        Ok(Running { progress, result: rx })
    }

    /// The result, once the thread is done.
    pub fn poll(&self) -> Option<Result<Next, String>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("the update stopped".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn assets(names: &[&str]) -> Vec<Asset> {
        names.iter().map(|n| Asset { name: n.to_string(), url: format!("{DOWNLOAD_PREFIX}v0.8.0/{n}") }).collect()
    }

    const RELEASE: [&str; 9] = [
        "constructcraft-v0.8.0-x86_64-pc-windows-msvc.zip",
        "constructcraft-v0.8.0-x86_64.msi",
        "constructcraft-v0.8.0-x86_64.AppImage",
        "constructcraft-v0.8.0-aarch64.AppImage",
        "constructcraft_0.8.0_amd64.deb",
        "constructcraft_0.8.0_arm64.deb",
        "constructcraft-v0.8.0-aarch64-apple-darwin.dmg",
        "constructcraft-v0.8.0-x86_64-unknown-linux-gnu.tar.gz",
        "SHA256SUMS.txt",
    ];

    /// Only plain names linked under this repository's releases are kept.
    #[test]
    fn assets_come_from_our_releases_only() {
        let link = |name: &str| format!("{DOWNLOAD_PREFIX}v0.8.0/{name}");
        let release = json!({"assets": [
            {"name": "ok.msi", "browser_download_url": link("ok.msi")},
            {"name": "evil.msi", "browser_download_url": "https://example.com/evil.msi"},
            {"name": "other.msi", "browser_download_url": "https://github.com/someone/constructcraft/releases/download/v1/other.msi"},
            {"name": "mismatch.msi", "browser_download_url": link("different.msi")},
            {"name": "../up.msi", "browser_download_url": link("../up.msi")},
            {"name": "sub/dir.msi", "browser_download_url": link("sub/dir.msi")},
            {"name": "x.msi", "browser_download_url": format!("{DOWNLOAD_PREFIX}../x.msi")},
            {"name": 5, "browser_download_url": link("5")},
        ]});
        let kept: Vec<String> = assets_of(&release).into_iter().map(|a| a.name).collect();
        assert_eq!(kept, ["ok.msi"]);
        // Links under the repository's former name are still ours.
        let former = json!({"assets": [
            {"name": "a.msi", "browser_download_url": format!("{FORMER_DOWNLOAD_PREFIX}v0.8.0/a.msi")},
        ]});
        assert_eq!(assets_of(&former).len(), 1);
        assert!(assets_of(&json!({})).is_empty());
    }

    #[test]
    fn installations_are_told_apart() {
        let none = |_: &str| None;
        let appimage = |n: &str| (n == "APPIMAGE").then(|| "/home/a/ConstructCraft.AppImage".to_string());
        assert_eq!(
            detect("linux", Some(Path::new("/tmp/.mount_x/usr/bin/constructcraft")), appimage),
            InstallKind::AppImage(PathBuf::from("/home/a/ConstructCraft.AppImage"))
        );
        assert_eq!(detect("linux", Some(Path::new("/usr/bin/constructcraft")), none), InstallKind::Deb);
        assert_eq!(detect("linux", Some(Path::new("/home/a/cc/constructcraft")), none), InstallKind::Other);
        let program_files = |n: &str| (n == "ProgramFiles").then(|| "C:\\Program Files".to_string());
        let msi = Path::new("C:\\Program Files\\ConstructCraft\\constructcraft.exe");
        assert_eq!(detect("windows", Some(msi), program_files), InstallKind::Msi);
        assert_eq!(
            detect("macos", Some(Path::new("/Applications/ConstructCraft.app/Contents/MacOS/constructcraft")), none),
            InstallKind::MacApp
        );
        assert_eq!(detect("macos", Some(Path::new("/Users/a/bin/constructcraft")), none), InstallKind::Other);
        assert_eq!(detect("freebsd", None, none), InstallKind::Other);

        // The Windows .zip: ccad.exe beside the app, outside Program Files.
        let dir = std::env::temp_dir().join(format!("ccad-detect-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("constructcraft.exe");
        assert_eq!(detect("windows", Some(&exe), program_files), InstallKind::Other, "no ccad.exe");
        std::fs::write(dir.join("ccad.exe"), b"").unwrap();
        assert_eq!(detect("windows", Some(&exe), program_files), InstallKind::Portable(exe.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_package_fits_the_installation() {
        let list = assets(&RELEASE);
        let pick = |kind: InstallKind, arch: &str| package_for(&kind, arch, &list).map(|a| a.name.clone());
        let image = InstallKind::AppImage(PathBuf::from("/a.AppImage"));
        assert_eq!(pick(image.clone(), "x86_64").as_deref(), Some(RELEASE[2]));
        assert_eq!(pick(image, "aarch64").as_deref(), Some(RELEASE[3]));
        assert_eq!(pick(InstallKind::Msi, "x86_64").as_deref(), Some(RELEASE[1]));
        assert_eq!(pick(InstallKind::Portable(PathBuf::new()), "x86_64").as_deref(), Some(RELEASE[0]));
        assert_eq!(pick(InstallKind::Deb, "aarch64").as_deref(), Some(RELEASE[5]));
        assert_eq!(pick(InstallKind::MacApp, "aarch64").as_deref(), Some(RELEASE[6]));
        assert_eq!(pick(InstallKind::MacApp, "x86_64"), None, "no Intel disk image in this release");
        assert_eq!(pick(InstallKind::Msi, "aarch64"), None);
        assert_eq!(pick(InstallKind::Other, "x86_64"), None);
        assert_eq!(deb_command(&list[4]), "sudo apt install ./constructcraft_0.8.0_amd64.deb");
    }

    #[test]
    fn checksums_are_read_from_the_list() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let sums = format!("{a}  one.msi\r\n{b} *two.zip\nnot-hex  three\n{}  short\n", "c".repeat(10));
        assert_eq!(expected_sha256(&sums, "one.msi"), Some(a));
        assert_eq!(expected_sha256(&sums, "two.zip"), Some("b".repeat(64)));
        assert_eq!(expected_sha256(&sums, "three"), None);
        assert_eq!(expected_sha256(&sums, "short"), None);
        assert_eq!(expected_sha256(&sums, "missing"), None);
        assert_eq!(hex(&Sha256::digest(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    /// A running executable is renamed aside, the new one takes its name, and
    /// a failed swap puts the old one back.
    #[test]
    fn executables_are_swapped_and_restored() {
        let dir = std::env::temp_dir().join(format!("ccad-swap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("constructcraft.exe");
        let new = dir.join("new.exe");
        std::fs::write(&exe, b"old").unwrap();
        std::fs::write(&new, b"new").unwrap();
        swap_executable(&exe, &new).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert_eq!(std::fs::read(dir.join("constructcraft.exe.old")).unwrap(), b"old");
        remove_old_executables(&exe);
        assert!(!dir.join("constructcraft.exe.old").exists());
        // The new file is missing: the old executable comes back.
        assert!(swap_executable(&exe, &dir.join("missing.exe")).is_err());
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_installer_command_quotes_paths() {
        let (program, args) = installer_command(
            Path::new("C:\\Temp\\it's here\\a.msi"),
            Path::new("C:\\Program Files\\ConstructCraft\\constructcraft.exe"),
        );
        assert_eq!(program, "powershell");
        let script = args.last().unwrap();
        assert!(script.contains("'\"C:\\Temp\\it''s here\\a.msi\"'"), "{script}");
        assert!(script.contains("'/passive' -Wait"), "{script}");
        assert!(
            script.ends_with("Start-Process -FilePath 'C:\\Program Files\\ConstructCraft\\constructcraft.exe'"),
            "{script}"
        );
    }

    #[test]
    fn files_are_found_in_unpacked_folders() {
        let dir = std::env::temp_dir().join(format!("ccad-find-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let inner = dir.join("constructcraft-v0.8.0-x86_64-pc-windows-msvc");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("ConstructCraft.exe"), b"").unwrap();
        assert_eq!(find_file(&dir, "constructcraft.exe", 3), Some(inner.join("ConstructCraft.exe")));
        assert_eq!(find_file(&dir, "constructcraft.exe", 0), None, "only one level is searched");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The whole Windows `.zip` path against a published release: download,
    /// SHA-256 against its list, unpack and swap into a scratch folder with
    /// stand-in executables. Reaches the network, so it runs on request:
    /// `cargo test -p cc_app live_portable_update -- --ignored`.
    #[test]
    #[ignore]
    fn live_portable_update() {
        if std::env::consts::OS != "windows" {
            return;
        }
        let tag = "v0.7.0";
        let name = format!("constructcraft-{tag}-x86_64-pc-windows-msvc.zip");
        let dir = std::env::temp_dir().join(format!("ccad-live-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("constructcraft.exe");
        std::fs::write(&exe, b"stand-in").unwrap();
        std::fs::write(dir.join("ccad.exe"), b"stand-in").unwrap();
        let package = Asset { url: format!("{DOWNLOAD_PREFIX}{tag}/{name}"), name };
        let sums = format!("{DOWNLOAD_PREFIX}{tag}/{SUMS}");
        let progress = Progress::default();
        let next = install(&InstallKind::Portable(exe.clone()), &package, &sums, &progress).unwrap();
        assert_eq!(next, Next::Restart(exe.clone()));
        assert!(std::fs::metadata(&exe).unwrap().len() > 1_000_000, "the real executable is in place");
        assert!(std::fs::metadata(dir.join("ccad.exe")).unwrap().len() > 1_000_000);
        assert_eq!(std::fs::read(dir.join("constructcraft.exe.old")).unwrap(), b"stand-in");
        assert!(progress.done.load(Ordering::Relaxed) > 1_000_000);
        // A wrong checksum list: nothing is installed and the download is removed.
        let bad = Asset { name: "SHA256SUMS.txt".into(), url: package.url.clone() };
        assert!(install(&InstallKind::Portable(exe.clone()), &bad, &sums, &Progress::default()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
