//! Autosave and crash recovery of the open project (the files are written by
//! `cc_core::recovery`): a copy of a project with unsaved changes is written
//! in the background an interval (a preference: a minute by default, or off)
//! after its last copy; a clean save, close or
//! exit removes it; a panic writes a final copy and appends to a crash log;
//! at start-up, copies left by earlier sessions are offered for restoring.

use crate::dialog::Dialog;
use crate::state::AppState;
use crate::t;
use cc_core::recovery::{self, Autosave, AutosaveAction, RecoveryEntry, RecoveryMeta};
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long unsaved changes wait before a copy is written, until the preference is read.
const INTERVAL: Duration = Duration::from_secs(60);
/// The crash log is started over once it grows past this many bytes.
const CRASH_LOG_LIMIT: u64 = 1 << 20;

/// The app's data folder (where eframe keeps its preferences).
pub fn data_dir() -> Option<PathBuf> {
    eframe::storage_dir("ConstructCAD")
}

/// The folder of the recovery copies.
pub fn folder() -> Option<PathBuf> {
    data_dir().map(|d| d.join("recovery"))
}

/// The crash log: one entry per panic, with its message, place and backtrace,
/// and per failed graphics start-up or lost GPU device.
pub fn crash_log() -> Option<PathBuf> {
    data_dir().map(|d| d.join("crash.log"))
}

/// Recovery state of the running session.
pub struct Recovery {
    /// Where copies go (`None`: no data folder, recovery is off).
    dir: Option<PathBuf>,
    /// This session's entry id.
    id: String,
    autosave: Autosave,
    /// Autosave copies are written (the preference is not "off").
    enabled: bool,
    /// A copy being written in the background: its revision and the thread.
    writing: Option<(u64, JoinHandle<bool>)>,
    /// The document generation the copy belongs to.
    generation: u64,
    /// Entries of earlier sessions, offered once at start-up, and the one picked.
    offered: Vec<RecoveryEntry>,
    picked: usize,
}

impl Recovery {
    /// Recovery in `dir`; the entries already there are offered.
    pub fn new(dir: Option<PathBuf>) -> Self {
        let id = format!("session-{}-{}", std::process::id(), recovery::now_seconds());
        let offered = dir.as_deref().map(recovery::list).unwrap_or_default();
        let autosave = Autosave::new(INTERVAL);
        Self { dir, id, autosave, enabled: true, writing: None, generation: 0, offered, picked: 0 }
    }

    /// Follow the autosave preference: copies every `interval`, or none (`None`).
    pub fn set_interval(&mut self, interval: Option<Duration>) {
        self.enabled = interval.is_some();
        if let Some(interval) = interval {
            self.autosave.interval = interval;
        }
    }

    fn meta(state: &AppState) -> RecoveryMeta {
        let doc = &state.session.doc;
        RecoveryMeta {
            name: doc.project.info.name.clone(),
            original: doc.path.clone(),
            saved_at: recovery::now_seconds(),
        }
    }

    /// Called once a frame: write a copy in the background when it is due,
    /// remove it once the document has no unsaved changes.
    pub fn tick(&mut self, state: &AppState) {
        let Some(dir) = self.dir.clone() else { return };
        if let Some((revision, handle)) = self.writing.take() {
            if !handle.is_finished() {
                self.writing = Some((revision, handle));
                return;
            }
            if handle.join().unwrap_or(false) {
                self.autosave.written(revision);
            } else {
                self.autosave.retry_later(Instant::now());
            }
        }
        // Another document replaced the one copied (after its changes were
        // saved or discarded): its copy goes.
        if state.generation != self.generation {
            self.generation = state.generation;
            if self.autosave.has_copy() {
                recovery::remove(&dir, &self.id);
            }
            self.autosave.removed();
        }
        let doc = &state.session.doc;
        let dirty = state.project_open && doc.is_dirty();
        match self.autosave.poll(doc.revision(), dirty, Instant::now()) {
            AutosaveAction::Nothing => {}
            // Autosave off: no new copies (one already written still goes once the changes are saved).
            AutosaveAction::Write if !self.enabled => {}
            AutosaveAction::Remove => recovery::remove(&dir, &self.id),
            AutosaveAction::Write => {
                // The copy is cheap next to serializing and writing it, which
                // happen off the interface thread.
                let project = doc.project.clone();
                let (id, meta) = (self.id.clone(), Self::meta(state));
                let spawned =
                    std::thread::Builder::new().name("autosave".into()).spawn(move || {
                        match recovery::write(&dir, &id, &project, &meta) {
                            Ok(_) => true,
                            Err(e) => {
                                log::warn!("autosave failed: {e}");
                                false
                            }
                        }
                    });
                match spawned {
                    Ok(handle) => self.writing = Some((doc.revision(), handle)),
                    Err(_) => self.autosave.retry_later(Instant::now()),
                }
            }
        }
    }

    /// Write a copy now, on this thread (after a panic, before the app goes).
    pub fn write_now(&mut self, state: &AppState) {
        let Some(dir) = &self.dir else { return };
        if !state.project_open || !state.session.doc.is_dirty() {
            return;
        }
        // Wait for a copy being written, so the two do not race.
        if let Some((_, handle)) = self.writing.take() {
            let _ = handle.join();
        }
        let meta = Self::meta(state);
        let project = &state.session.doc.project;
        let written = std::panic::catch_unwind(|| recovery::write(dir, &self.id, project, &meta).is_ok());
        if written.unwrap_or(false) {
            self.autosave.written(state.session.doc.revision());
        }
    }

    /// A clean exit: the user saved or chose to discard the changes.
    pub fn exit(&mut self) {
        if let Some((_, handle)) = self.writing.take() {
            let _ = handle.join();
        }
        if let Some(dir) = &self.dir {
            recovery::remove(dir, &self.id);
        }
    }

    /// Restore `entry` as the open project: unsaved, saved to its original
    /// file (if it had one), and copied at once under this session's id.
    fn restore(&mut self, state: &mut AppState, entry: &RecoveryEntry) -> bool {
        let mut session = cc_engine::Session::default();
        let opened = session.run(cc_engine::commands::project::ProjectOpen { path: entry.project.clone() });
        if let Err(e) = opened {
            state.dialogs.error = Some(t!("msg-recovery-failed", "error" => e.message));
            return false;
        }
        let mut doc = session.doc;
        doc.path = entry.meta.original.clone();
        doc.mark_unsaved();
        state.replace_document(doc);
        state.project_open = true;
        state.show_home = false;
        state.message = t!("msg-recovered", "name" => display_name(entry));
        self.generation = state.generation;
        self.autosave.removed();
        self.write_now(state);
        true
    }

    /// The start-up dialog offering the copies of earlier sessions.
    pub fn dialog(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if self.offered.is_empty() {
            return;
        }
        self.picked = self.picked.min(self.offered.len() - 1);
        let now = recovery::now_seconds();
        let (mut restore, mut discard) = (0, 0);
        let entries = &self.offered;
        let picked = &mut self.picked;
        let response = Dialog::new("recovery", t!("dlg-recovery")).width(460.0).show(
            ctx,
            |ui| {
                ui.label(t!("dlg-recovery-text"));
                ui.add_space(4.0);
                for (i, entry) in entries.iter().enumerate() {
                    let place = match &entry.meta.original {
                        Some(path) => path.display().to_string(),
                        None => t!("recovery-never-saved"),
                    };
                    let text = format!("{}\n{place} · {}", display_name(entry), age(now, entry.meta.saved_at));
                    if ui.selectable_label(*picked == i, text).clicked() {
                        *picked = i;
                    }
                }
            },
            |f| {
                restore = f.primary(t!("action-restore"));
                discard = f.danger(t!("action-discard"));
                f.secondary(t!("action-cancel"));
            },
        );
        let Some(entry) = self.offered.get(self.picked).cloned() else { return };
        match response.clicked {
            Some(i) if i == restore => {
                if self.restore(state, &entry) {
                    // Restored and copied again under this session's id.
                    if let Some(dir) = &self.dir {
                        recovery::remove(dir, &entry.id);
                    }
                    // One project at a time: the others wait for the next start.
                    self.offered.clear();
                }
            }
            Some(i) if i == discard => {
                if let Some(dir) = &self.dir {
                    recovery::remove(dir, &entry.id);
                }
                self.offered.retain(|e| e.id != entry.id);
            }
            // Cancel keeps the copies for the next start.
            Some(_) => self.offered.clear(),
            None if response.dismissed => self.offered.clear(),
            None => {}
        }
    }
}

fn display_name(entry: &RecoveryEntry) -> String {
    if entry.meta.name.trim().is_empty() { entry.id.clone() } else { entry.meta.name.clone() }
}

/// How long ago `then` was, in words.
fn age(now: u64, then: u64) -> String {
    let minutes = now.saturating_sub(then) / 60;
    match minutes {
        0 => t!("recovery-age-now"),
        1..60 => t!("recovery-age-minutes", "count" => minutes),
        60..1440 => t!("recovery-age-hours", "count" => minutes / 60),
        _ => t!("recovery-age-days", "count" => minutes / 1440),
    }
}

/// Append a panic to the crash log (no recursion, no panics: errors are ignored).
pub fn log_panic(info: &std::panic::PanicHookInfo<'_>) {
    let Some(path) = crash_log() else { return };
    write_crash_entry(&path, &info.to_string(), true);
}

/// Append an event that is not a panic (a failed graphics start-up, a lost
/// GPU device) to the crash log, without a backtrace; errors are ignored.
pub fn log_event(what: &str) {
    let Some(path) = crash_log() else { return };
    write_crash_entry(&path, what, false);
}

fn write_crash_entry(path: &Path, what: &str, with_backtrace: bool) {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let fresh = std::fs::metadata(path).is_ok_and(|m| m.len() > CRASH_LOG_LIMIT);
    let file = std::fs::OpenOptions::new().create(true).append(!fresh).write(true).truncate(fresh).open(path);
    let Ok(mut file) = file else { return };
    let backtrace = if with_backtrace { std::backtrace::Backtrace::force_capture().to_string() } else { String::new() };
    let _ = writeln!(
        file,
        "--- ConstructCraft {} ({} {}), {} s after the Unix epoch\n{what}\n{backtrace}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        recovery::now_seconds(),
    );
}

/// Install the panic hook: every panic is logged to the crash log, then
/// reported as before (on the console, in debug builds).
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log_panic(info);
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ccad-app-recovery-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn open_state() -> AppState {
        let mut state = AppState::new(cc_core::Document::new(cc_engine::new_project()));
        state.project_open = true;
        state.show_home = false;
        state
    }

    /// Wait for a background copy to finish (and be taken in by `tick`).
    fn settle(r: &mut Recovery, state: &AppState) {
        for _ in 0..500 {
            if r.writing.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
            r.tick(state);
        }
        panic!("the copy was never written");
    }

    #[test]
    fn unsaved_work_is_copied_restored_and_removed_after_a_save() {
        let dir = scratch("cycle");
        let mut r = Recovery::new(Some(dir.clone()));
        r.autosave.interval = Duration::ZERO;
        assert!(r.offered.is_empty());
        let mut state = open_state();
        // A clean project is not copied.
        r.tick(&state);
        assert!(r.writing.is_none() && recovery::list(&dir).is_empty());
        state.session.execute("wall_create", serde_json::json!({"start": [0, 0], "end": [5, 0]})).unwrap();
        r.tick(&state);
        settle(&mut r, &state);
        let entries = recovery::list(&dir);
        assert_eq!(entries.len(), 1);
        // The same revision is not written again.
        let written = std::fs::metadata(&entries[0].project).unwrap().modified().unwrap();
        r.tick(&state);
        assert!(r.writing.is_none());
        assert_eq!(std::fs::metadata(&entries[0].project).unwrap().modified().unwrap(), written);

        // The next start offers it; restoring brings the wall back, unsaved.
        let mut next = Recovery::new(Some(dir.clone()));
        assert_eq!(next.offered.len(), 1);
        let mut restored = AppState::new(cc_core::Document::new(cc_engine::new_project()));
        let entry = next.offered[0].clone();
        assert!(next.restore(&mut restored, &entry));
        assert!(restored.project_open && restored.session.doc.is_dirty());
        assert_eq!(restored.project().walls().count(), 1);
        assert_eq!(restored.session.doc.path, None);

        // Saving the first session's project removes its copy.
        let file = dir.join("saved.ccad");
        state.session.run(cc_engine::commands::project::ProjectSave { path: Some(file) }).unwrap();
        r.tick(&state);
        assert!(recovery::list(&dir).iter().all(|e| e.id != r.id));
        // A clean exit removes the restored session's copy.
        next.exit();
        assert!(recovery::list(&dir).iter().all(|e| e.id != next.id));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// With autosave off (a preference) unsaved changes are not copied.
    #[test]
    fn autosave_off_writes_no_copy() {
        let dir = scratch("off");
        let mut r = Recovery::new(Some(dir.clone()));
        r.set_interval(None);
        r.autosave.interval = Duration::ZERO;
        let mut state = open_state();
        state.session.execute("wall_create", serde_json::json!({"start": [0, 0], "end": [5, 0]})).unwrap();
        r.tick(&state);
        assert!(r.writing.is_none() && recovery::list(&dir).is_empty());
        // Turned on again with an interval: the copy is written.
        r.set_interval(Some(Duration::ZERO));
        r.tick(&state);
        settle(&mut r, &state);
        assert_eq!(recovery::list(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_panic_leaves_a_copy_and_a_crash_log() {
        let dir = scratch("panic");
        let mut r = Recovery::new(Some(dir.clone()));
        let mut state = open_state();
        state.session.execute("wall_create", serde_json::json!({"start": [0, 0], "end": [5, 0]})).unwrap();
        // The interval has not passed: only the final copy writes it.
        r.tick(&state);
        assert!(recovery::list(&dir).is_empty());
        r.write_now(&state);
        assert_eq!(recovery::list(&dir).len(), 1);
        let log = dir.join("crash.log");
        write_crash_entry(&log, "panicked at somewhere.rs:1:1", true);
        write_crash_entry(&log, "graphics start-up failed", false);
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.contains("somewhere.rs") && text.contains("graphics start-up failed"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
