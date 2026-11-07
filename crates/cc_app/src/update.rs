//! Update notice: at startup (at most once a day) and on request, the app
//! asks GitHub for the latest release of ConstructCraft in a background thread
//! and, when it is newer than this build, shows a notice with a link to the
//! release page. Checks only notify; *Update Now* (see [`crate::update_install`])
//! downloads, verifies and installs a release, and only when the user asks.
//!
//! The one request is an HTTPS GET of [`RELEASES_API`] with a short timeout
//! and a bounded response; every failure is silent (logged) and reported as
//! "could not check" only when the user asked. Automatic checks can be turned
//! off in the About dialog, and the environment variable [`NO_CHECK_ENV`]
//! turns every check off (CI and tests). Version comparison and the response
//! parsing are pure functions, tested without a network.

use crate::t;
use crate::theme::{self, PALETTE};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

/// The GitHub API endpoint of the latest (non-draft, non-prerelease) release.
pub const RELEASES_API: &str = "https://api.github.com/repos/francisreboucas/constructcraft/releases/latest";
/// Release pages; the tag is appended.
const RELEASE_PAGE: &str = "https://github.com/francisreboucas/constructcraft/releases/tag/";
/// Set (to anything but `0` or nothing) to turn every update check off.
pub const NO_CHECK_ENV: &str = "CONSTRUCTCAD_NO_UPDATE_CHECK";
/// Time between automatic checks.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// The whole request (connect, send, receive) gives up after this.
const TIMEOUT: Duration = Duration::from_secs(10);
/// Larger responses are refused (the release JSON is a few kilobytes plus notes).
const MAX_RESPONSE: u64 = 512 * 1024;

/// A release version `major.minor.patch` (build metadata ignored).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    /// Parse `1.2.3` or `v1.2.3` (build metadata `+...` ignored). Pre-releases
    /// (`1.2.3-beta`) and anything else give `None`.
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.trim();
        let text = text.strip_prefix(['v', 'V']).unwrap_or(text);
        let core = match text.split_once('+') {
            Some((core, build)) => {
                let valid =
                    !build.is_empty() && build.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
                if !valid {
                    return None;
                }
                core
            }
            None => text,
        };
        let mut parts = core.split('.');
        let mut number = || -> Option<u64> {
            let part = parts.next()?;
            // Digits only, without leading zeros (as semantic versioning requires).
            let valid =
                !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()) && (part == "0" || !part.starts_with('0'));
            if valid { part.parse().ok() } else { None }
        };
        let version = Version { major: number()?, minor: number()?, patch: number()? };
        if parts.next().is_some() {
            return None;
        }
        Some(version)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// A published release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    /// Its page on GitHub (built from the tag, never taken from the response).
    pub url: String,
    /// Its packages and checksum list (only links to this repository's releases).
    pub assets: Vec<crate::update_install::Asset>,
}

/// The result of a check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    UpToDate,
    Available(Release),
    /// No answer, or one that could not be read.
    Failed,
    /// Checks are turned off by [`NO_CHECK_ENV`].
    Disabled,
}

/// The version of this build.
pub fn current() -> Option<Version> {
    Version::parse(env!("CARGO_PKG_VERSION"))
}

/// Compare the latest-release JSON of the GitHub API with `current`.
/// Drafts and pre-releases are ignored (reported as up to date).
pub fn evaluate(current: Version, json: &str) -> Outcome {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else { return Outcome::Failed };
    let flag = |key: &str| value.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
    if flag("draft") || flag("prerelease") {
        return Outcome::UpToDate;
    }
    let Some(tag) = value.get("tag_name").and_then(|v| v.as_str()) else { return Outcome::Failed };
    if tag.contains('-') && Version::parse(tag).is_none() {
        // A pre-release tag published as a regular release.
        return Outcome::UpToDate;
    }
    let Some(latest) = Version::parse(tag) else { return Outcome::Failed };
    if latest <= current {
        return Outcome::UpToDate;
    }
    // `parse` accepted the tag, so it only holds `v`, digits, `.`, `+`, `-` and letters.
    let assets = crate::update_install::assets_of(&value);
    Outcome::Available(Release { version: latest, url: format!("{RELEASE_PAGE}{}", tag.trim()), assets })
}

/// Whether [`NO_CHECK_ENV`] turns checks off.
pub fn disabled_by_env() -> bool {
    std::env::var_os(NO_CHECK_ENV).is_some_and(|v| !v.is_empty() && v != "0")
}

/// Whether an automatic check is due: turned on, and none in the last
/// [`CHECK_INTERVAL`] (or the clock went back). Times are seconds since the Unix epoch.
pub fn auto_check_due(enabled: bool, last_check: Option<u64>, now: u64) -> bool {
    enabled && last_check.is_none_or(|last| now < last || now - last >= CHECK_INTERVAL.as_secs())
}

/// Seconds since the Unix epoch (0 if the clock is before it).
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The one request: the latest release as JSON.
fn fetch() -> Result<String, ureq::Error> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .user_agent(format!("ConstructCraft/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut response = agent
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()?;
    response.body_mut().with_config().limit(MAX_RESPONSE).read_to_string()
}

/// Check in a background thread; the receiver gets one [`Outcome`] (or is
/// disconnected if the thread could not start), and `ctx` repaints then.
fn spawn(ctx: egui::Context) -> Receiver<Outcome> {
    let (tx, rx) = std::sync::mpsc::channel();
    let started = std::thread::Builder::new().name("update-check".into()).spawn(move || {
        let outcome = match (current(), fetch()) {
            (Some(current), Ok(body)) => evaluate(current, &body),
            (None, _) => Outcome::Failed,
            (_, Err(err)) => {
                log::info!("update check failed: {err}");
                Outcome::Failed
            }
        };
        let _ = tx.send(outcome);
        ctx.request_repaint();
    });
    if let Err(err) = started {
        log::warn!("could not start the update check: {err}");
    }
    rx
}

/// Update checks of the app: when the last ran, the check in progress and what to show.
#[derive(Debug, Default)]
pub struct Updates {
    /// When the last check started (seconds since the Unix epoch).
    pub last_check: Option<u64>,
    /// A version the user chose to skip: its notice is not shown again.
    pub skipped: Option<Version>,
    /// The check in progress, and whether the user asked for it.
    pending: Option<(Receiver<Outcome>, bool)>,
    /// A newer release whose notice is shown.
    pub available: Option<Release>,
    /// The result of a check the user asked for, shown in a dialog.
    pub report: Option<Outcome>,
    /// Update Now: its progress or result, shown in a dialog.
    pub install: Option<Install>,
    /// What an installed update starts when the app closes (the new version, or the installer).
    pub on_exit: Option<crate::update_install::Next>,
}

/// Update Now, as the dialog shows it.
#[derive(Debug)]
pub enum Install {
    /// Downloading and verifying `version` (its release page for a manual download).
    Running(Version, String, crate::update_install::Running),
    /// Done: what is left to do.
    Ready(Version, crate::update_install::Next),
    /// The package manager updates this copy: the command, and the release page.
    Command(String, String),
    /// Failed (why, and the release page for a manual download).
    Failed(String, String),
}

/// How Update Now would update to `release` here, if it can.
pub enum Plan {
    /// Download, verify and install `package` (with the release's checksum list).
    Install(crate::update_install::InstallKind, crate::update_install::Asset, String),
    /// Show the package manager's command.
    Command(String),
}

/// What Update Now does for `release` on this installation, or `None` (Download only).
pub fn plan(release: &Release) -> Option<Plan> {
    use crate::update_install::{InstallKind, SUMS, deb_command, installs_itself, package_for, this_install};
    let kind = this_install();
    let package = package_for(&kind, std::env::consts::ARCH, &release.assets)?.clone();
    if kind == InstallKind::Deb {
        return Some(Plan::Command(deb_command(&package)));
    }
    let sums = release.assets.iter().find(|a| a.name == SUMS)?.url.clone();
    installs_itself(&kind).then_some(Plan::Install(kind, package, sums))
}

impl Updates {
    /// Start an automatic check if one is due (`enabled`: the preference is on).
    pub fn start_if_due(&mut self, ctx: &egui::Context, enabled: bool) {
        if !disabled_by_env() && auto_check_due(enabled, self.last_check, now()) {
            self.start(ctx, false);
        }
    }

    /// Start a check (`manual`: the user asked, so the result is reported).
    pub fn start(&mut self, ctx: &egui::Context, manual: bool) {
        if let Some((_, asked)) = &mut self.pending {
            *asked |= manual;
            return;
        }
        if manual {
            self.report = None;
        }
        // Tests never reach the network.
        if disabled_by_env() || cfg!(test) {
            self.finish(Outcome::Disabled, manual);
            return;
        }
        self.last_check = Some(now());
        self.pending = Some((spawn(ctx.clone()), manual));
    }

    /// A check the user asked for is running.
    pub fn checking(&self) -> bool {
        self.pending.as_ref().is_some_and(|(_, manual)| *manual)
    }

    /// Stop reporting the check in progress (it still finishes quietly).
    pub fn cancel_report(&mut self) {
        if let Some((_, manual)) = &mut self.pending {
            *manual = false;
        }
        self.report = None;
    }

    /// Take the result of the check in progress, if it arrived.
    pub fn poll(&mut self) {
        self.poll_install();
        let Some((rx, manual)) = &self.pending else { return };
        let manual = *manual;
        let outcome = match rx.try_recv() {
            Ok(outcome) => outcome,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Outcome::Failed,
        };
        self.pending = None;
        self.finish(outcome, manual);
    }

    /// Show what a check found: a newer release's notice (unless skipped and
    /// the check was automatic) and, for a check the user asked for, the report.
    fn finish(&mut self, outcome: Outcome, manual: bool) {
        if let Outcome::Available(release) = &outcome
            && (manual || self.skipped != Some(release.version))
        {
            self.available = Some(release.clone());
        }
        if manual {
            self.report = Some(outcome);
        }
    }

    /// Update Now: install `release` here (or show how), in the update dialog.
    pub fn update_now(&mut self, ctx: &egui::Context, release: &Release) {
        self.available = None;
        self.report = None;
        self.install = Some(match plan(release) {
            Some(Plan::Install(kind, package, sums)) => {
                match crate::update_install::Running::start(ctx, kind, package, sums) {
                    Ok(running) => Install::Running(release.version, release.url.clone(), running),
                    Err(e) => Install::Failed(e, release.url.clone()),
                }
            }
            Some(Plan::Command(command)) => Install::Command(command, release.url.clone()),
            None => {
                ctx.open_url(egui::OpenUrl::new_tab(&release.url));
                return;
            }
        });
    }

    /// Take the result of Update Now, if it arrived.
    pub fn poll_install(&mut self) {
        let Some(Install::Running(version, url, running)) = &self.install else { return };
        let Some(result) = running.poll() else { return };
        let (version, url) = (*version, url.clone());
        self.install = Some(match result {
            Ok(next) => Install::Ready(version, next),
            // Cancel closes the dialog.
            Err(_) if running.progress.cancel.load(std::sync::atomic::Ordering::Relaxed) => {
                self.install = None;
                return;
            }
            Err(e) => Install::Failed(e, url),
        });
    }

    /// Hide the notice until the next check.
    pub fn dismiss(&mut self) {
        self.available = None;
    }

    /// Hide the notice and do not notify this version again.
    pub fn skip(&mut self) {
        if let Some(release) = self.available.take() {
            self.skipped = Some(release.version);
        }
    }
}

/// The notice of a newer release, in one row: message, Download, Skip, dismiss.
/// Shows nothing without one.
pub fn notice(ui: &mut egui::Ui, updates: &mut Updates) {
    let Some(release) = updates.available.clone() else { return };
    ui.horizontal(|ui| {
        crate::icons::paint(
            ui.painter(),
            egui::Rect::from_min_size(ui.cursor().min, egui::vec2(16.0, 16.0)).translate(egui::vec2(0.0, 1.0)),
            crate::icons::Icon::Update,
            PALETTE.accent,
        );
        ui.add_space(20.0);
        let text = t!("update-available", "version" => release.version.to_string());
        ui.label(egui::RichText::new(text).color(PALETTE.accent_text));
        if plan(&release).is_some() && ui.button(t!("update-now")).clicked() {
            updates.update_now(ui.ctx(), &release);
        }
        if ui.button(t!("update-download")).on_hover_text(&release.url).clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(&release.url));
            updates.dismiss();
        }
        if ui.button(t!("update-skip")).clicked() {
            updates.skip();
        }
        let close = ui.add(egui::Button::new(theme::muted("×")).frame(false)).on_hover_text(t!("update-dismiss"));
        close.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, t!("update-dismiss")));
        if close.clicked() {
            updates.dismiss();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Context::open_url` (Download, the repository link) only reaches the
    /// browser with eframe's `links` feature; without it the click is ignored.
    #[test]
    fn eframe_opens_links() {
        let manifest = include_str!("../../../Cargo.toml");
        let eframe = manifest.lines().find(|line| line.starts_with("eframe =")).unwrap_or_default();
        assert!(eframe.contains("\"links\""), "eframe needs the \"links\" feature: {eframe}");
    }

    fn v(major: u64, minor: u64, patch: u64) -> Version {
        Version { major, minor, patch }
    }

    #[test]
    fn versions_parse_and_order() {
        assert_eq!(Version::parse("0.3.0"), Some(v(0, 3, 0)));
        assert_eq!(Version::parse("v1.10.2"), Some(v(1, 10, 2)));
        assert_eq!(Version::parse(" v2.0.0+build.5 "), Some(v(2, 0, 0)));
        for bad in ["", "v", "1", "1.2", "1.2.3.4", "1.2.x", "01.2.3", "1.2.3-beta.1", "1.2.3+", "1.2.3+a/b", "-1.2.3"]
        {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
        assert!(v(0, 10, 0) > v(0, 9, 9));
        assert!(v(1, 0, 0) > v(0, 99, 99));
        assert!(v(0, 3, 1) > v(0, 3, 0));
        assert!(current().is_some(), "the crate version parses");
    }

    #[test]
    fn latest_release_against_this_version() {
        let json = |tag: &str| {
            format!(
                r#"{{"tag_name": "{tag}", "draft": false, "prerelease": false, "html_url": "https://example.com/x"}}"#
            )
        };
        let now = v(0, 3, 0);
        assert_eq!(
            evaluate(now, &json("v0.4.0")),
            Outcome::Available(Release {
                version: v(0, 4, 0),
                url: "https://github.com/francisreboucas/constructcraft/releases/tag/v0.4.0".into(),
                assets: Vec::new(),
            })
        );
        assert_eq!(evaluate(now, &json("v0.3.0")), Outcome::UpToDate);
        assert_eq!(evaluate(now, &json("v0.2.9")), Outcome::UpToDate);
        // Pre-releases are ignored, whether flagged or only tagged as such.
        assert_eq!(evaluate(now, &json("v0.5.0-rc.1")), Outcome::UpToDate);
        let flagged = r#"{"tag_name": "v9.0.0", "prerelease": true}"#;
        assert_eq!(evaluate(now, flagged), Outcome::UpToDate);
        assert_eq!(evaluate(now, r#"{"tag_name": "v9.0.0", "draft": true}"#), Outcome::UpToDate);
        // Anything unreadable is a failed check.
        for bad in ["", "not json", "[]", r#"{"message": "API rate limit exceeded"}"#, &json("latest"), &json("v1.2")] {
            assert_eq!(evaluate(now, bad), Outcome::Failed, "{bad}");
        }
    }

    #[test]
    fn automatic_checks_once_a_day() {
        let day = CHECK_INTERVAL.as_secs();
        assert!(auto_check_due(true, None, 1000));
        assert!(!auto_check_due(false, None, 1000));
        assert!(!auto_check_due(true, Some(1000), 1000 + day - 1));
        assert!(auto_check_due(true, Some(1000), 1000 + day));
        // A clock set back does not stop checks for good.
        assert!(auto_check_due(true, Some(1000 + 10 * day), 1000));
    }

    fn release(version: Version) -> Release {
        Release { version, url: String::new(), assets: Vec::new() }
    }

    #[test]
    fn skipped_versions_show_no_notice() {
        let mut updates = Updates::default();
        updates.finish(Outcome::Available(release(v(0, 4, 0))), false);
        assert_eq!(updates.available.as_ref().map(|r| r.version), Some(v(0, 4, 0)));
        assert_eq!(updates.report, None, "automatic checks report nothing");
        updates.skip();
        assert_eq!((updates.available.as_ref(), updates.skipped), (None, Some(v(0, 4, 0))));
        // Checked again automatically: still skipped; a newer one shows.
        updates.finish(Outcome::Available(release(v(0, 4, 0))), false);
        assert!(updates.available.is_none());
        updates.finish(Outcome::Available(release(v(0, 5, 0))), false);
        assert_eq!(updates.available.as_ref().map(|r| r.version), Some(v(0, 5, 0)));
        // Asked for, a skipped version is shown and reported.
        updates.dismiss();
        updates.skip();
        updates.skipped = Some(v(0, 5, 0));
        updates.finish(Outcome::Available(release(v(0, 5, 0))), true);
        assert!(updates.available.is_some());
        assert!(matches!(updates.report, Some(Outcome::Available(_))));
    }

    #[test]
    fn results_arrive_through_the_channel() {
        let mut updates = Updates::default();
        let (tx, rx) = std::sync::mpsc::channel();
        updates.pending = Some((rx, true));
        assert!(updates.checking());
        updates.poll();
        assert!(updates.checking(), "no answer yet");
        tx.send(Outcome::UpToDate).unwrap();
        updates.poll();
        assert!(!updates.checking());
        assert_eq!(updates.report, Some(Outcome::UpToDate));
        // A thread that ended without an answer is a failed check.
        let (tx, rx) = std::sync::mpsc::channel::<Outcome>();
        drop(tx);
        updates.pending = Some((rx, true));
        updates.poll();
        assert_eq!(updates.report, Some(Outcome::Failed));
    }

    #[test]
    fn tests_never_reach_the_network() {
        let ctx = egui::Context::default();
        let mut updates = Updates::default();
        updates.start(&ctx, true);
        assert!(!updates.checking());
        assert_eq!(updates.report, Some(Outcome::Disabled));
        assert_eq!(updates.last_check, None);
    }

    /// A newer release keeps its packages, but only links to this repository's releases.
    #[test]
    fn available_releases_carry_their_packages() {
        let prefix = crate::update_install::DOWNLOAD_PREFIX;
        let body = serde_json::json!({
            "tag_name": "v9.0.0", "draft": false, "prerelease": false,
            "assets": [
                {"name": "constructcraft-v9.0.0-x86_64.msi", "browser_download_url": format!("{prefix}v9.0.0/constructcraft-v9.0.0-x86_64.msi")},
                {"name": "SHA256SUMS.txt", "browser_download_url": format!("{prefix}v9.0.0/SHA256SUMS.txt")},
                {"name": "elsewhere.msi", "browser_download_url": "https://example.com/elsewhere.msi"},
            ]
        });
        let Outcome::Available(release) = evaluate(v(0, 1, 0), &body.to_string()) else { panic!("newer") };
        let names: Vec<&str> = release.assets.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["constructcraft-v9.0.0-x86_64.msi", "SHA256SUMS.txt"]);
    }
}
