//! App preferences (not model data): what they hold, how they are kept in
//! eframe's storage (every key lives here), how the interface follows them,
//! and the Preferences window (File tab, Ctrl+,, home screen).
//!
//! Stored values are hostile input: numbers are parsed and clamped, lists
//! are bounded, and folders that no longer exist are kept but skipped where
//! they are used. Missing values take defaults that match the behavior
//! before the window existed.

use crate::commands::UiCommand;
use crate::dialog::Dialog;
use crate::i18n::Language;
use crate::icons::{self, Icon};
use crate::state::AppState;
use crate::t;
use crate::table;
use crate::theme::{self, PALETTE, ThemeChoice};
use egui::{Align, Context, Layout, RichText, Ui};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// eframe storage key of the UI language.
const LANGUAGE_KEY: &str = "language";
/// eframe storage key of the recent projects (one path per line).
const RECENT_KEY: &str = "recent_files";
/// eframe storage key of the component library folders (one path per line).
const LIBRARY_KEY: &str = "component_library_folders";
/// eframe storage key of the interface theme ("system", "light" or "dark").
const THEME_KEY: &str = "theme";
/// eframe storage key of the start-collapsed ribbon preference ("1" when collapsed).
const RIBBON_KEY: &str = "ribbon_collapsed";
/// eframe storage key of the automatic update check preference ("0" when off).
const UPDATE_AUTO_KEY: &str = "update_check_auto";
/// eframe storage key of the time of the last update check (seconds since the Unix epoch).
const UPDATE_LAST_KEY: &str = "update_check_last";
/// eframe storage key of the version whose update notice the user skipped.
const UPDATE_SKIPPED_KEY: &str = "update_skipped_version";
/// eframe storage key of the interface scale (percent).
const UI_SCALE_KEY: &str = "ui_scale";
/// eframe storage key of the font size (percent of the normal size).
const FONT_SIZE_KEY: &str = "font_size";
/// eframe storage key of the detailed tooltips preference ("0" when off).
const TOOLTIPS_KEY: &str = "detailed_tooltips";
/// eframe storage key of the 3D left-drag preference ("1" when it selects).
const SELECT_3D_KEY: &str = "select_drag_3d";
/// eframe storage key of the custom two-letter shortcuts (`CommandId=KEYS` per line).
const SHORTCUTS_KEY: &str = "shortcut_keys";
/// eframe storage key of the macros folder.
const MACRO_FOLDER_KEY: &str = "macro_folder";
/// eframe storage key of the default projects folder.
const PROJECTS_FOLDER_KEY: &str = "projects_folder";
/// eframe storage key of the autosave interval (minutes, 0 = off).
const AUTOSAVE_KEY: &str = "autosave_minutes";
/// eframe storage key of the number of recent projects kept.
const RECENT_COUNT_KEY: &str = "recent_count";
/// eframe storage key of the plugins folder (empty: the default one).
const PLUGIN_FOLDER_KEY: &str = "plugin_folder";
/// eframe storage key of the plugins turned off (one id per line).
const PLUGINS_OFF_KEY: &str = "plugins_off";
/// eframe storage key of the plugins allowed to work with files (one id per line).
const PLUGINS_FILES_KEY: &str = "plugins_files";
/// Most plugin ids kept in each of those lists.
const MAX_PLUGIN_IDS: usize = 1000;

/// Interface scales offered (percent).
pub const UI_SCALES: [u32; 8] = [75, 90, 100, 110, 125, 150, 175, 200];
/// Font sizes offered (percent of the normal size) and their names.
const FONT_SIZES: [(u32, &str); 4] =
    [(90, "pref-font-small"), (100, "pref-font-normal"), (115, "pref-font-large"), (130, "pref-font-larger")];
/// Autosave intervals offered (minutes; 0 turns autosave off).
pub const AUTOSAVE_MINUTES: [u32; 6] = [0, 1, 2, 5, 10, 15];
/// Numbers of recent projects offered.
pub const RECENT_COUNTS: [usize; 6] = [5, 8, 12, 16, 20, 30];
/// At most this many library folders are kept.
const MAX_LIBRARY_FOLDERS: usize = 64;
/// At most this many recent projects are kept.
const MAX_RECENT: usize = 50;
/// The macro menu lists at most this many files.
pub const MAX_MACROS: usize = 200;
/// Directory entries looked at when listing macros (a huge folder is not read to the end).
const MAX_MACRO_SCAN: usize = 10_000;
/// At most this many custom shortcuts are read.
const MAX_SHORTCUTS: usize = 500;

/// The app preferences. Language and the update check's bookkeeping are kept
/// elsewhere (`i18n`, `Updates`), but stored and restored here too.
#[derive(Debug, Clone, PartialEq)]
pub struct Preferences {
    pub theme: ThemeChoice,
    /// Check for a newer release at startup (at most once a day).
    pub check_updates: bool,
    /// Interface scale in percent (the egui zoom factor), within [`UI_SCALES`]'s range.
    pub ui_scale: u32,
    /// Text size in percent of the normal size, independent of the scale.
    pub font_size: u32,
    /// The ribbon starts collapsed (only its tabs shown).
    pub ribbon_collapsed: bool,
    /// Tooltips of commands show what they do and the engine command, not only the name.
    pub detailed_tooltips: bool,
    /// A left drag in 3D views draws a selection box (Shift+drag orbits);
    /// otherwise it orbits and Shift+drag selects.
    pub select_drag_3d: bool,
    /// Custom two-letter keys by command id (`UiCommand::id`); empty keys
    /// take a command's keys away.
    pub shortcut_keys: BTreeMap<String, String>,
    /// Folders of the component library.
    pub library_folders: Vec<PathBuf>,
    /// Where macros are recorded to and listed from.
    pub macro_folder: Option<PathBuf>,
    /// Where the project file dialogs start.
    pub projects_folder: Option<PathBuf>,
    /// Minutes between autosave copies of unsaved changes (0: off).
    pub autosave_minutes: u32,
    /// How many recent projects the home screen lists.
    pub recent_count: usize,
    /// Where plugins are installed and loaded from (`None`: the default
    /// folder, see [`Preferences::plugins_dir`]).
    pub plugin_folder: Option<PathBuf>,
    /// Plugins turned off (by id): not loaded.
    pub plugins_off: BTreeSet<String>,
    /// Plugins the user allowed to work with files (the `files` permission).
    pub plugins_files: BTreeSet<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::System,
            check_updates: true,
            ui_scale: 100,
            font_size: 100,
            ribbon_collapsed: false,
            detailed_tooltips: true,
            select_drag_3d: false,
            shortcut_keys: BTreeMap::new(),
            library_folders: Vec::new(),
            macro_folder: None,
            projects_folder: None,
            autosave_minutes: 1,
            recent_count: 12,
            plugin_folder: None,
            plugins_off: BTreeSet::new(),
            plugins_files: BTreeSet::new(),
        }
    }
}

fn clamp_scale(percent: u32) -> u32 {
    percent.clamp(UI_SCALES[0], UI_SCALES[UI_SCALES.len() - 1])
}

fn clamp_font(percent: u32) -> u32 {
    let (lo, hi) = (*theme::FONT_SCALE_RANGE.start(), *theme::FONT_SCALE_RANGE.end());
    percent.clamp((lo * 100.0).round() as u32, (hi * 100.0).round() as u32)
}

/// A zoom factor as a valid interface scale (percent).
fn scale_of_zoom(zoom: f32) -> u32 {
    if zoom.is_finite() && zoom > 0.0 { clamp_scale((zoom * 100.0).round().min(1000.0) as u32) } else { 100 }
}

/// Paths stored one per line: empty lines dropped, duplicates removed, at most `max`.
fn path_lines(text: &str, max: usize) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for line in text.lines().map(|l| l.trim_end_matches('\r')).filter(|l| !l.trim().is_empty()) {
        let path = PathBuf::from(line);
        if !paths.contains(&path) {
            paths.push(path);
        }
        if paths.len() >= max {
            break;
        }
    }
    paths
}

fn join_paths(paths: &[PathBuf]) -> String {
    paths.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join("\n")
}

fn folder(text: Option<String>) -> Option<PathBuf> {
    text.map(|t| t.trim_end_matches(['\r', '\n']).to_string()).filter(|t| !t.trim().is_empty()).map(PathBuf::from)
}

/// Custom shortcuts stored as `CommandId=KEYS` lines: only commands that
/// exist and may change keys, with two capital letters or none.
fn shortcut_lines(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .take(MAX_SHORTCUTS)
        .filter_map(|line| {
            let (id, keys) = line.trim_end_matches('\r').split_once('=')?;
            let command = crate::commands::UiCommand::from_id(id.trim())?;
            let keys = keys.trim().to_ascii_uppercase();
            let valid = keys.is_empty() || crate::commands::is_letter_keys(&keys);
            (valid && command.customizable()).then(|| (command.id(), keys))
        })
        .collect()
}

/// Plugin ids, one per line (invalid ones dropped, bounded).
fn id_lines(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|id| cc_lua::plugin::valid_id(id))
        .take(MAX_PLUGIN_IDS)
        .map(str::to_string)
        .collect()
}

fn flag(on: bool) -> String {
    if on { "1" } else { "0" }.to_string()
}

impl Preferences {
    /// Read the preferences with `get` (a storage lookup). `zoom` is the
    /// interface zoom eframe restored, used when no scale was stored yet
    /// (it kept the Ctrl+Plus/Minus zoom before this preference existed).
    pub fn read(get: impl Fn(&str) -> Option<String>, zoom: f32) -> Self {
        let d = Self::default();
        let number = |key| get(key).and_then(|v| v.trim().parse::<u64>().ok());
        let on = |key, default: bool| get(key).map_or(default, |v| if default { v != "0" } else { v == "1" });
        Self {
            theme: get(THEME_KEY).and_then(|v| ThemeChoice::from_code(&v)).unwrap_or(d.theme),
            check_updates: on(UPDATE_AUTO_KEY, d.check_updates),
            ui_scale: number(UI_SCALE_KEY)
                .map_or_else(|| scale_of_zoom(zoom), |v| clamp_scale(v.min(u32::MAX as u64) as u32)),
            font_size: number(FONT_SIZE_KEY).map_or(d.font_size, |v| clamp_font(v.min(u32::MAX as u64) as u32)),
            ribbon_collapsed: on(RIBBON_KEY, d.ribbon_collapsed),
            detailed_tooltips: on(TOOLTIPS_KEY, d.detailed_tooltips),
            select_drag_3d: on(SELECT_3D_KEY, d.select_drag_3d),
            shortcut_keys: get(SHORTCUTS_KEY).map(|v| shortcut_lines(&v)).unwrap_or_default(),
            library_folders: get(LIBRARY_KEY).map(|v| path_lines(&v, MAX_LIBRARY_FOLDERS)).unwrap_or_default(),
            macro_folder: folder(get(MACRO_FOLDER_KEY)),
            projects_folder: folder(get(PROJECTS_FOLDER_KEY)),
            autosave_minutes: number(AUTOSAVE_KEY).map_or(d.autosave_minutes, |v| v.min(60) as u32),
            recent_count: number(RECENT_COUNT_KEY).map_or(d.recent_count, |v| v.clamp(1, MAX_RECENT as u64) as usize),
            plugin_folder: folder(get(PLUGIN_FOLDER_KEY)),
            plugins_off: get(PLUGINS_OFF_KEY).map(|v| id_lines(&v)).unwrap_or_default(),
            plugins_files: get(PLUGINS_FILES_KEY).map(|v| id_lines(&v)).unwrap_or_default(),
        }
    }

    /// Write the preferences with `set` (a storage write).
    pub fn write(&self, mut set: impl FnMut(&str, String)) {
        set(THEME_KEY, self.theme.code().to_string());
        set(UPDATE_AUTO_KEY, flag(self.check_updates));
        set(UI_SCALE_KEY, self.ui_scale.to_string());
        set(FONT_SIZE_KEY, self.font_size.to_string());
        set(RIBBON_KEY, flag(self.ribbon_collapsed));
        set(TOOLTIPS_KEY, flag(self.detailed_tooltips));
        set(SELECT_3D_KEY, flag(self.select_drag_3d));
        let keys = self.shortcut_keys.iter().map(|(id, keys)| format!("{id}={keys}")).collect::<Vec<_>>();
        set(SHORTCUTS_KEY, keys.join("\n"));
        set(LIBRARY_KEY, join_paths(&self.library_folders));
        let text = |p: &Option<PathBuf>| p.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        set(MACRO_FOLDER_KEY, text(&self.macro_folder));
        set(PROJECTS_FOLDER_KEY, text(&self.projects_folder));
        set(AUTOSAVE_KEY, self.autosave_minutes.to_string());
        set(RECENT_COUNT_KEY, self.recent_count.to_string());
        set(PLUGIN_FOLDER_KEY, text(&self.plugin_folder));
        let ids = |set: &BTreeSet<String>| set.iter().cloned().collect::<Vec<_>>().join("\n");
        set(PLUGINS_OFF_KEY, ids(&self.plugins_off));
        set(PLUGINS_FILES_KEY, ids(&self.plugins_files));
    }

    /// Time between autosave copies, or `None` when autosave is off.
    pub fn autosave_interval(&self) -> Option<Duration> {
        (self.autosave_minutes > 0).then(|| Duration::from_secs(u64::from(self.autosave_minutes) * 60))
    }

    /// The macros folder, if set and present.
    pub fn macro_dir(&self) -> Option<&Path> {
        self.macro_folder.as_deref().filter(|p| p.is_dir())
    }

    /// The default projects folder, if set and present.
    pub fn projects_dir(&self) -> Option<&Path> {
        self.projects_folder.as_deref().filter(|p| p.is_dir())
    }

    /// The plugins folder: the one set, else `plugins` in the app's data
    /// folder (`None` when there is none).
    pub fn plugins_dir(&self) -> Option<PathBuf> {
        // Tests never load the user's own plugins.
        let default = if cfg!(test) { None } else { crate::recovery::data_dir().map(|d| d.join("plugins")) };
        self.plugin_folder.clone().or(default)
    }

    /// The interface zoom factor.
    pub fn zoom(&self) -> f32 {
        clamp_scale(self.ui_scale) as f32 / 100.0
    }

    /// The font size multiplier.
    pub fn font_scale(&self) -> f32 {
        clamp_font(self.font_size) as f32 / 100.0
    }
}

/// Set the UI language at startup: `language` (from the command line), else
/// the stored one, else the operating system's.
pub fn load_language(get: impl Fn(&str) -> Option<String>, language: Option<Language>) {
    let saved = get(LANGUAGE_KEY).and_then(|c| Language::from_code(&c));
    crate::i18n::set_language(language.or(saved).unwrap_or_else(Language::from_system));
}

/// Restore every other stored preference into `state` at startup (`zoom`: see [`Preferences::read`]).
pub fn load(state: &mut AppState, get: impl Fn(&str) -> Option<String>, zoom: f32) {
    state.prefs = Preferences::read(&get, zoom);
    state.ribbon_collapsed = state.prefs.ribbon_collapsed;
    state.recent = get(RECENT_KEY).map(|v| path_lines(&v, state.prefs.recent_count)).unwrap_or_default();
    state.updates.last_check = get(UPDATE_LAST_KEY).and_then(|v| v.trim().parse().ok());
    state.updates.skipped = get(UPDATE_SKIPPED_KEY).and_then(|v| crate::update::Version::parse(&v));
}

/// Store every preference of `state` (and the language).
pub fn save(state: &AppState, mut set: impl FnMut(&str, String)) {
    set(LANGUAGE_KEY, crate::i18n::language().code().to_string());
    state.prefs.write(&mut set);
    let recent: Vec<PathBuf> = state.recent.iter().take(state.prefs.recent_count).cloned().collect();
    set(RECENT_KEY, join_paths(&recent));
    let updates = &state.updates;
    set(UPDATE_LAST_KEY, updates.last_check.map(|t| t.to_string()).unwrap_or_default());
    set(UPDATE_SKIPPED_KEY, updates.skipped.map(|v| v.to_string()).unwrap_or_default());
}

/// What the interface last applied of the preferences (theme, zoom, font size).
#[derive(Debug, Clone, Copy)]
pub struct Look {
    zoom: f32,
    font: f32,
    theme: Option<ThemeChoice>,
}

impl Look {
    /// The look `ctx` shows now.
    pub fn current(ctx: &Context) -> Self {
        Self { zoom: ctx.zoom_factor(), font: theme::font_scale(), theme: None }
    }
}

/// Make the interface follow the preferences; call it once at the start of
/// every frame. A zoom changed with Ctrl+Plus/Minus since the last frame
/// becomes the scale preference.
pub fn sync_look(ctx: &Context, prefs: &mut Preferences, look: &mut Look) {
    crate::commands::set_key_overrides(&prefs.shortcut_keys);
    let zoom = ctx.zoom_factor();
    if (zoom - look.zoom).abs() > 1e-3 {
        prefs.ui_scale = scale_of_zoom(zoom);
        look.zoom = zoom;
    }
    if (prefs.zoom() - look.zoom).abs() > 1e-3 {
        look.zoom = prefs.zoom();
        ctx.set_zoom_factor(look.zoom);
    }
    if (prefs.font_scale() - look.font).abs() > 1e-3 {
        look.font = prefs.font_scale();
        theme::set_font_scale(ctx, look.font);
    }
    if look.theme != Some(prefs.theme) {
        look.theme = Some(prefs.theme);
        theme::set_theme(ctx, prefs.theme);
    }
}

/// Keep at most `prefs.recent_count` recent projects.
pub fn trim_recent(state: &mut AppState) {
    let count = state.prefs.recent_count.clamp(1, MAX_RECENT);
    state.recent.truncate(count);
}

/// The macro files (`.ccmacro`) of `folder`, sorted by name: not recursive,
/// at most [`MAX_MACROS`]; a missing or unreadable folder has none.
pub fn macro_files(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries
        .take(MAX_MACRO_SCAN)
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ccmacro")) && p.is_file())
        .collect();
    files.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    files.truncate(MAX_MACROS);
    files
}

// --- The Preferences window ---

/// A page of the Preferences window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    General,
    Interface,
    FileLocations,
    Saving,
    Shortcuts,
}

impl Section {
    pub const ALL: [Section; 5] =
        [Section::General, Section::Interface, Section::FileLocations, Section::Saving, Section::Shortcuts];

    pub fn label(self) -> String {
        match self {
            Section::General => t!("pref-general"),
            Section::Interface => t!("pref-interface"),
            Section::FileLocations => t!("pref-file-locations"),
            Section::Saving => t!("pref-saving"),
            Section::Shortcuts => t!("pref-shortcuts"),
        }
    }

    fn icon(self) -> Icon {
        match self {
            Section::General => Icon::Settings,
            Section::Interface => Icon::Interface,
            Section::FileLocations => Icon::Open,
            Section::Saving => Icon::Save,
            Section::Shortcuts => Icon::Keyboard,
        }
    }
}

/// Width of the window's body.
const WIDTH: f32 = 780.0;
/// Width of the list of pages.
const NAV_WIDTH: f32 = 180.0;
/// Height of a page: the same on every page, so the window does not jump;
/// longer pages scroll.
const PAGE_HEIGHT: f32 = 380.0;

/// What Cancel brings back: the preferences, language and ribbon state when the window opened.
#[derive(Debug, Clone)]
struct Snapshot {
    prefs: Preferences,
    language: Language,
    ribbon_collapsed: bool,
}

/// State of the open Preferences window. Changes show at once; Cancel
/// restores the values the window opened with.
#[derive(Debug, Clone)]
pub struct PreferencesDialog {
    pub section: Section,
    snapshot: Snapshot,
    shortcuts: ShortcutEditor,
}

/// The Shortcuts page: its search text and the keys being typed for a command.
#[derive(Debug, Clone, Default)]
struct ShortcutEditor {
    filter: String,
    /// The search field took the focus when the page first showed.
    searched: bool,
    /// The command whose keys are being typed, the letters so far, and why
    /// they were refused (keys of another command).
    editing: Option<(UiCommand, String, Option<String>)>,
}

/// Open the Preferences window on `section` (or show that section if it is open).
pub fn open(state: &mut AppState, section: Section) {
    if let Some(dialog) = &mut state.dialogs.preferences {
        dialog.section = section;
        return;
    }
    let snapshot = Snapshot {
        prefs: state.prefs.clone(),
        language: crate::i18n::language(),
        ribbon_collapsed: state.ribbon_collapsed,
    };
    state.dialogs.preferences = Some(PreferencesDialog { section, snapshot, shortcuts: ShortcutEditor::default() });
}

fn restore(state: &mut AppState, snapshot: Snapshot) {
    state.prefs = snapshot.prefs;
    state.ribbon_collapsed = snapshot.ribbon_collapsed;
    crate::actions::set_language(state, snapshot.language);
}

/// Restore Defaults: the default values of the page's preferences (the
/// language and the folders are left as they are).
fn restore_page(state: &mut AppState, section: Section) {
    let d = Preferences::default();
    let p = &mut state.prefs;
    match section {
        Section::General => {
            p.theme = d.theme;
            p.check_updates = d.check_updates;
        }
        Section::Interface => {
            p.ui_scale = d.ui_scale;
            p.font_size = d.font_size;
            p.ribbon_collapsed = d.ribbon_collapsed;
            p.detailed_tooltips = d.detailed_tooltips;
            p.select_drag_3d = d.select_drag_3d;
            state.ribbon_collapsed = d.ribbon_collapsed;
        }
        Section::FileLocations => {}
        Section::Saving => {
            p.autosave_minutes = d.autosave_minutes;
            p.recent_count = d.recent_count;
        }
        Section::Shortcuts => p.shortcut_keys = d.shortcut_keys,
    }
}

pub fn show(ctx: &Context, state: &mut AppState) {
    let Some(mut dialog) = state.dialogs.preferences.take() else { return };
    let before = state.prefs.clone();
    let (mut ok, mut defaults) = (0, None);
    let section = dialog.section;
    let response = Dialog::new("preferences", t!("dlg-preferences")).width(WIDTH).show(
        ctx,
        |ui| body(ui, state, &mut dialog),
        |f| {
            if section != Section::FileLocations {
                defaults = Some(f.extra(t!("pref-restore-defaults")));
            }
            ok = f.primary(t!("action-ok"));
            f.secondary(t!("action-cancel"));
        },
    );
    if response.clicked.is_some() && response.clicked == defaults {
        restore_page(state, section);
        dialog.shortcuts.editing = None;
    }
    if state.prefs != before {
        // The interface follows on the next frame (see `sync_look`).
        trim_recent(state);
        crate::commands::set_key_overrides(&state.prefs.shortcut_keys);
        ctx.request_repaint();
    }
    match response.clicked {
        Some(i) if i == ok => trim_recent(state),
        Some(i) if Some(i) == defaults => state.dialogs.preferences = Some(dialog),
        Some(_) => restore(state, dialog.snapshot),
        None if response.dismissed => restore(state, dialog.snapshot),
        None => state.dialogs.preferences = Some(dialog),
    }
}

fn body(ui: &mut Ui, state: &mut AppState, dialog: &mut PreferencesDialog) {
    crate::dialog::two_columns(ui, [NAV_WIDTH, WIDTH - NAV_WIDTH], |ui, column| {
        if column == 0 {
            ui.spacing_mut().item_spacing.y = 2.0;
            for section in Section::ALL {
                if nav_item(ui, section, dialog.section == section).clicked() {
                    dialog.section = section;
                }
            }
            return;
        }
        ui.set_min_height(PAGE_HEIGHT);
        ui.set_max_height(PAGE_HEIGHT);
        if dialog.section == Section::Shortcuts {
            // The list scrolls under its search field.
            shortcuts(ui, state, &mut dialog.shortcuts);
            return;
        }
        egui::ScrollArea::vertical().id_salt(("pref-page", dialog.section as u8)).auto_shrink([false, false]).show(
            ui,
            |ui| match dialog.section {
                Section::General => general(ui, state),
                Section::Interface => interface(ui, state),
                Section::FileLocations => file_locations(ui, state),
                Section::Saving => saving(ui, state),
                Section::Shortcuts => {}
            },
        );
    });
}

/// A page in the list on the left: its icon and name, the current one
/// marked with an accent bar.
fn nav_item(ui: &mut Ui, section: Section, selected: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(egui::vec2(NAV_WIDTH, 32.0), egui::Sense::click());
    let painter = ui.painter();
    let radius = theme::RADIUS as f32;
    if selected {
        painter.rect_filled(rect, radius, PALETTE.accent_soft);
        let bar =
            egui::Rect::from_min_size(rect.left_top() + egui::vec2(0.0, 7.0), egui::vec2(3.0, rect.height() - 14.0));
        painter.rect_filled(bar, 1.5, PALETTE.accent);
    } else if r.hovered() {
        painter.rect_filled(rect, radius, PALETTE.hover);
    }
    let color = if selected { PALETTE.accent_text } else { PALETTE.text };
    let icon = egui::Rect::from_center_size(egui::pos2(rect.left() + 22.0, rect.center().y), egui::vec2(18.0, 18.0));
    icons::paint(painter, icon, section.icon(), color);
    let font = if selected { egui::FontId::new(13.0, theme::semibold()) } else { egui::FontId::proportional(13.0) };
    painter.text(
        egui::pos2(rect.left() + 40.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        section.label(),
        font,
        color,
    );
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, section.label()));
    r
}

/// A group of rows under a small heading.
fn group(ui: &mut Ui, title: String, first: bool) {
    if !first {
        ui.add_space(16.0);
    }
    ui.label(RichText::new(title).family(theme::semibold()).size(12.5).color(PALETTE.text));
    ui.add_space(4.0);
}

/// What the row above does, in small muted text under it.
fn hint(ui: &mut Ui, text: String) {
    ui.add_space(3.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.add(egui::Label::new(RichText::new(text).size(11.5).color(PALETTE.text_muted)).wrap());
    });
    ui.add_space(3.0);
}

/// A combo box in a table row choosing among `options`.
fn combo<T: PartialEq + Copy>(
    ui: &mut Ui,
    id: &str,
    value: &mut T,
    options: impl IntoIterator<Item = T>,
    label: impl Fn(T) -> String,
) {
    egui::ComboBox::from_id_salt(("pref", id)).selected_text(label(*value)).show_ui(ui, |ui| {
        for option in options {
            ui.selectable_value(value, option, label(option));
        }
    });
}

fn check(ui: &mut Ui, label: &str, value: &mut bool) {
    ui.add_space(4.0);
    // Unlike the flat cells around it, the box shows when unchecked too.
    ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::new(1.0, PALETTE.border_strong);
    let r = ui.checkbox(value, "");
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *value, label));
}

/// A checkbox row with its explanation under it.
fn check_row(ui: &mut Ui, label: &str, value: &mut bool, explanation: Option<String>) {
    table::row(ui, label, |ui| check(ui, label, value));
    if let Some(text) = explanation {
        hint(ui, text);
    }
}

/// A framed button that stands out in the flat table cells.
fn cell_button(ui: &mut Ui, enabled: bool, text: String) -> egui::Response {
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.bg_stroke = egui::Stroke::new(1.0, PALETTE.border_strong);
        w.inactive.weak_bg_fill = PALETTE.panel;
        w.hovered.weak_bg_fill = PALETTE.hover;
        ui.add_enabled(enabled, egui::Button::new(RichText::new(text).size(12.0)))
    })
    .inner
}

fn general(ui: &mut Ui, state: &mut AppState) {
    group(ui, t!("pref-group-language-theme"), true);
    let label = t!("settings-language");
    table::row(ui, &label, |ui| {
        let mut lang = crate::i18n::language();
        combo(ui, "language", &mut lang, Language::ALL, |l| l.native_name().to_string());
        crate::actions::set_language(state, lang);
    });
    table::row(ui, &t!("settings-theme"), |ui| {
        combo(ui, "theme", &mut state.prefs.theme, ThemeChoice::ALL, ThemeChoice::label);
    });
    group(ui, t!("pref-group-updates"), false);
    check_row(ui, &t!("update-auto"), &mut state.prefs.check_updates, Some(t!("update-auto-hint")));
}

fn interface(ui: &mut Ui, state: &mut AppState) {
    group(ui, t!("pref-group-size"), true);
    table::row(ui, &t!("pref-ui-scale"), |ui| {
        let mut scale = clamp_scale(state.prefs.ui_scale);
        let options = UI_SCALES.into_iter().chain((!UI_SCALES.contains(&scale)).then_some(scale));
        combo(ui, "scale", &mut scale, options, |s| format!("{s}%"));
        state.prefs.ui_scale = scale;
    });
    table::row(ui, &t!("pref-font-size"), |ui| {
        let name = |p: u32| {
            FONT_SIZES.iter().find(|(v, _)| *v == p).map_or_else(|| format!("{p}%"), |(_, key)| crate::i18n::tr(key))
        };
        let current = state.prefs.font_size;
        let options = FONT_SIZES
            .map(|(v, _)| v)
            .into_iter()
            .chain((!FONT_SIZES.iter().any(|(v, _)| *v == current)).then_some(current));
        combo(ui, "font", &mut state.prefs.font_size, options, name);
    });
    group(ui, t!("pref-group-ribbon"), false);
    let before = state.prefs.ribbon_collapsed;
    check_row(ui, &t!("pref-ribbon-collapsed"), &mut state.prefs.ribbon_collapsed, None);
    if state.prefs.ribbon_collapsed != before {
        state.ribbon_collapsed = state.prefs.ribbon_collapsed;
    }
    let label = t!("pref-detailed-tooltips");
    check_row(ui, &label, &mut state.prefs.detailed_tooltips, Some(t!("pref-detailed-tooltips-hint")));
    group(ui, t!("pref-group-3d"), false);
    let label = t!("pref-select-drag-3d");
    check_row(ui, &label, &mut state.prefs.select_drag_3d, Some(t!("pref-select-drag-3d-hint")));
}

/// `text` shortened in the middle to at most `max` characters.
fn middle_ellipsis(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max || max < 8 {
        return text.to_string();
    }
    let tail = (max - 1) * 3 / 5;
    let head = max - 1 - tail;
    let (start, end) = (chars.iter().take(head).collect::<String>(), chars.iter().skip(chars.len() - tail));
    format!("{start}…{}", end.collect::<String>())
}

/// A folder's icon and path (shortened in the middle, whole on hover), or
/// `empty` when there is none; a folder that is gone is marked.
fn folder_label(ui: &mut Ui, path: Option<&Path>, empty: &str, room: f32) {
    let missing = path.is_some_and(|p| !p.is_dir());
    let (icon, icon_r) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
    let tone = if path.is_some() { PALETTE.text_muted } else { PALETTE.text_faint };
    icons::paint(ui.painter(), icon, Icon::Open, if missing { PALETTE.warning } else { tone });
    let Some(path) = path else {
        ui.label(RichText::new(empty).color(PALETTE.text_faint));
        return;
    };
    let full = path.display().to_string();
    // About 6.5 points per character at the table's text size.
    let max = ((room - if missing { 110.0 } else { 0.0 }) / 6.5).max(8.0) as usize;
    let color = if missing { PALETTE.warning } else { PALETTE.text };
    ui.add(egui::Label::new(RichText::new(middle_ellipsis(&full, max)).color(color)).truncate()).on_hover_text(&full);
    if missing {
        ui.label(RichText::new(t!("pref-missing")).size(11.5).color(PALETTE.warning));
    }
    icon_r.on_hover_text(full);
}

/// A folder in a table row: its path, Browse… and Clear.
fn folder_row(ui: &mut Ui, label: &str, value: &mut Option<PathBuf>) {
    table::row(ui, label, |ui| {
        let room = ui.available_width() - 150.0;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if cell_button(ui, value.is_some(), t!("pref-clear")).clicked() {
                *value = None;
            }
            if cell_button(ui, true, t!("pref-browse")).clicked() {
                let mut picker = rfd::FileDialog::new();
                if let Some(dir) = value.as_deref().filter(|p| p.is_dir()) {
                    picker = picker.set_directory(dir);
                }
                if let Some(folder) = picker.pick_folder() {
                    *value = Some(folder);
                }
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add_space(4.0);
                folder_label(ui, value.as_deref(), &t!("pref-not-set"), room);
            });
        });
    });
}

fn file_locations(ui: &mut Ui, state: &mut AppState) {
    group(ui, t!("pref-group-folders"), true);
    folder_row(ui, &t!("pref-macro-folder"), &mut state.prefs.macro_folder);
    folder_row(ui, &t!("pref-projects-folder"), &mut state.prefs.projects_folder);
    hint(ui, t!("pref-folders-hint"));
    folder_row(ui, &t!("pref-plugin-folder"), &mut state.prefs.plugin_folder);
    let default = crate::recovery::data_dir().map(|d| d.join("plugins").display().to_string()).unwrap_or_default();
    hint(ui, t!("pref-plugin-folder-hint", "path" => default));
    group(ui, t!("lib-folders"), false);
    let folders = &mut state.prefs.library_folders;
    if folders.is_empty() {
        hint(ui, t!("lib-empty"));
    }
    let mut remove = None;
    for (i, folder) in folders.iter().enumerate() {
        let name =
            folder.file_name().map_or_else(|| folder.display().to_string(), |n| n.to_string_lossy().into_owned());
        table::row(ui, &name, |ui| {
            let room = ui.available_width() - 80.0;
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let r = cell_button(ui, true, t!("action-remove"));
                r.widget_info(|| {
                    let label = format!("{} {}", t!("action-remove"), folder.display());
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label)
                });
                if r.clicked() {
                    remove = Some(i);
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add_space(4.0);
                    folder_label(ui, Some(folder), "", room);
                });
            });
        });
    }
    if let Some(i) = remove
        && i < folders.len()
    {
        folders.remove(i);
    }
    ui.add_space(8.0);
    let add = format!("+ {}", t!("lib-add-folder"));
    if cell_button(ui, folders.len() < MAX_LIBRARY_FOLDERS, add).clicked()
        && let Some(folder) = rfd::FileDialog::new().pick_folder()
        && !folders.contains(&folder)
    {
        folders.push(folder);
    }
}

fn saving(ui: &mut Ui, state: &mut AppState) {
    group(ui, t!("pref-group-recovery"), true);
    table::row(ui, &t!("pref-autosave"), |ui| {
        let current = state.prefs.autosave_minutes;
        let options = AUTOSAVE_MINUTES.into_iter().chain((!AUTOSAVE_MINUTES.contains(&current)).then_some(current));
        let label = |m: u32| if m == 0 { t!("pref-autosave-off") } else { t!("pref-autosave-every", "count" => m) };
        combo(ui, "autosave", &mut state.prefs.autosave_minutes, options, label);
    });
    hint(ui, t!("pref-autosave-hint"));
    group(ui, t!("pref-group-home"), false);
    table::row(ui, &t!("pref-recent-count"), |ui| {
        let current = state.prefs.recent_count;
        let options = RECENT_COUNTS.into_iter().chain((!RECENT_COUNTS.contains(&current)).then_some(current));
        combo(ui, "recent", &mut state.prefs.recent_count, options, |n| n.to_string());
    });
}

/// The keys `command` has with the custom ones in `custom`.
fn keys_of(custom: &BTreeMap<String, String>, command: UiCommand) -> Option<String> {
    let default = command.default_shortcut();
    if !command.customizable() {
        return default.map(str::to_string);
    }
    match custom.get(&command.id()) {
        Some(keys) if keys.is_empty() => None,
        Some(keys) => Some(keys.clone()),
        None => default.map(str::to_string),
    }
}

/// Give `command` the two-letter `keys` (empty: none), or tell which command
/// has them already.
fn set_keys(custom: &mut BTreeMap<String, String>, command: UiCommand, keys: &str) -> Result<(), UiCommand> {
    if !keys.is_empty()
        && let Some(other) =
            UiCommand::all().into_iter().find(|c| *c != command && keys_of(custom, *c).as_deref() == Some(keys))
    {
        return Err(other);
    }
    if command.default_shortcut().unwrap_or_default() == keys {
        custom.remove(&command.id());
    } else {
        custom.insert(command.id(), keys.to_string());
    }
    Ok(())
}

/// The Shortcuts page: every command with its keys; two-letter keys are
/// typed in place, Ctrl chords are fixed.
fn shortcuts(ui: &mut Ui, state: &mut AppState, editor: &mut ShortcutEditor) {
    ui.add(egui::Label::new(RichText::new(t!("pref-shortcuts-hint")).size(11.5).color(PALETTE.text_muted)).wrap());
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let (icon, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
        icons::paint(ui.painter(), icon, Icon::Search, PALETTE.text_muted);
        let search = egui::TextEdit::singleline(&mut editor.filter)
            .hint_text(t!("pref-shortcuts-search"))
            .desired_width(ui.available_width());
        let r = ui.add(search);
        r.widget_info(|| egui::WidgetInfo::text_edit(true, "", &editor.filter, t!("pref-shortcuts-search")));
        if !editor.searched {
            editor.searched = true;
            r.request_focus();
        }
    });
    ui.add_space(6.0);
    let filter = editor.filter.trim().to_lowercase();
    let custom = state.prefs.shortcut_keys.clone();
    let commands: Vec<(UiCommand, String)> = UiCommand::all()
        .into_iter()
        .filter(|c| {
            filter.is_empty()
                || c.label(state).to_lowercase().contains(&filter)
                || keys_of(&custom, *c).is_some_and(|k| k.to_lowercase().contains(&filter))
        })
        .map(|c| (c, c.label(state)))
        .collect();
    let mut change: Option<(UiCommand, String)> = None;
    egui::ScrollArea::vertical().id_salt("pref-shortcuts").auto_shrink([false, false]).show(ui, |ui| {
        if commands.is_empty() {
            hint(ui, t!("pref-shortcuts-none"));
        }
        for (command, label) in commands {
            let keys = keys_of(&custom, command);
            let changed = custom.contains_key(&command.id());
            table::row(ui, &label, |ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if command.customizable() && changed {
                        let reset = cell_button(ui, changed, t!("pref-shortcut-reset"));
                        reset.widget_info(|| {
                            let text = format!("{} {label}", t!("pref-shortcut-reset"));
                            egui::WidgetInfo::labeled(egui::WidgetType::Button, changed, text)
                        });
                        if reset.clicked() {
                            change = Some((command, command.default_shortcut().unwrap_or_default().to_string()));
                        }
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add_space(4.0);
                        if !command.customizable() {
                            crate::ribbon::key_cap(ui, &keys.unwrap_or_default())
                                .on_hover_text(t!("pref-shortcut-fixed"));
                            return;
                        }
                        let editing = editor.editing.as_mut().filter(|(c, _, _)| *c == command);
                        let mut text =
                            editing.as_ref().map_or_else(|| keys.clone().unwrap_or_default(), |e| e.1.clone());
                        let field = egui::TextEdit::singleline(&mut text)
                            .id(ui.id().with(("shortcut", command.id())))
                            .char_limit(2)
                            .desired_width(44.0)
                            .horizontal_align(Align::Center)
                            .font(egui::FontId::new(12.5, theme::semibold()))
                            .hint_text("—");
                        // A visible box in the flat table cell.
                        let v = ui.visuals_mut();
                        v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, PALETTE.border_strong);
                        v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, PALETTE.accent);
                        v.extreme_bg_color = PALETTE.panel;
                        let r = ui.add(field);
                        r.widget_info(|| {
                            egui::WidgetInfo::text_edit(true, keys.clone().unwrap_or_default(), &text, &label)
                        });
                        let text: String =
                            text.chars().filter(char::is_ascii_alphabetic).map(|c| c.to_ascii_uppercase()).collect();
                        if r.gained_focus() {
                            // Typed letters replace the keys.
                            if let Some(mut s) = egui::text_edit::TextEditState::load(ui.ctx(), r.id) {
                                let all = egui::text_selection::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(text.chars().count()),
                                );
                                s.cursor.set_char_range(Some(all));
                                s.store(ui.ctx(), r.id);
                            }
                        }
                        if r.gained_focus() || (r.has_focus() && editor.editing.as_ref().is_none_or(|e| e.0 != command))
                        {
                            editor.editing = Some((command, text.clone(), None));
                        } else if r.changed()
                            && let Some(e) = editor.editing.as_mut()
                        {
                            e.1 = text.clone();
                            e.2 = None;
                            if text.len() == 2 {
                                change = Some((command, text.clone()));
                            }
                        }
                        if r.lost_focus() {
                            // Leaving the field empty takes the keys away; one letter keeps the old ones.
                            if text.is_empty() && keys.is_some() {
                                change = Some((command, String::new()));
                            }
                            if editor.editing.as_ref().is_some_and(|e| e.0 == command && e.2.is_none()) {
                                editor.editing = None;
                            }
                        }
                        if let Some((_, _, Some(why))) = editor.editing.as_ref().filter(|e| e.0 == command) {
                            ui.label(RichText::new(why).size(11.5).color(PALETTE.danger));
                        } else if changed {
                            let default = command.default_shortcut().unwrap_or("—");
                            ui.label(
                                RichText::new(t!("pref-shortcut-default", "keys" => default))
                                    .size(11.5)
                                    .color(PALETTE.text_faint),
                            );
                        }
                    });
                });
            });
        }
    });
    if let Some((command, keys)) = change {
        match set_keys(&mut state.prefs.shortcut_keys, command, &keys) {
            Ok(()) => {
                if editor.editing.as_ref().is_some_and(|e| e.0 == command) {
                    editor.editing = Some((command, keys, None));
                }
            }
            Err(other) => {
                let why = t!("pref-shortcut-taken", "keys" => keys.clone(), "command" => other.label(state));
                editor.editing = Some((command, keys, Some(why)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn stored(prefs: &Preferences) -> HashMap<String, String> {
        let mut map = HashMap::new();
        prefs.write(|k, v| {
            map.insert(k.to_string(), v);
        });
        map
    }

    /// Nothing stored: today's behavior (system theme, update checks, a
    /// one-minute autosave, twelve recent projects, detailed tooltips).
    #[test]
    fn defaults_when_nothing_is_stored() {
        let prefs = Preferences::read(|_| None, 1.0);
        assert_eq!(prefs, Preferences::default());
        assert_eq!(prefs.autosave_interval(), Some(Duration::from_secs(60)));
        assert_eq!(prefs.recent_count, 12);
        // A zoom eframe kept from Ctrl+Plus becomes the scale.
        assert_eq!(Preferences::read(|_| None, 1.25).ui_scale, 125);
    }

    #[test]
    fn preferences_round_trip() {
        let prefs = Preferences {
            theme: ThemeChoice::Dark,
            check_updates: false,
            ui_scale: 150,
            font_size: 115,
            ribbon_collapsed: true,
            detailed_tooltips: false,
            select_drag_3d: true,
            shortcut_keys: BTreeMap::from([("Move".to_string(), "MO".to_string()), ("Pin".to_string(), String::new())]),
            library_folders: vec![PathBuf::from("/lib/a"), PathBuf::from("C:\\lib b")],
            macro_folder: Some(PathBuf::from("/macros")),
            projects_folder: Some(PathBuf::from("/projects")),
            autosave_minutes: 0,
            recent_count: 20,
            plugin_folder: Some(PathBuf::from("/plugins")),
            plugins_off: BTreeSet::from(["old_tool".to_string()]),
            plugins_files: BTreeSet::from(["exporter".to_string(), "importer".to_string()]),
        };
        let map = stored(&prefs);
        let back = Preferences::read(|k| map.get(k).cloned(), 1.0);
        assert_eq!(back, prefs);
        assert_eq!(back.autosave_interval(), None);
        // Cleared folders are stored empty and read back as unset.
        let map = stored(&Preferences::default());
        assert_eq!(Preferences::read(|k| map.get(k).cloned(), 1.0), Preferences::default());
    }

    /// Stored values are hostile input: they are clamped or ignored.
    #[test]
    fn invalid_values_are_clamped_or_ignored() {
        let lines: String = (0..500).map(|i| format!("/f{}\n\n/f{}\r\n", i % 100, i % 100)).collect();
        let map: HashMap<&str, String> = [
            (UI_SCALE_KEY, "100000".to_string()),
            (FONT_SIZE_KEY, "1".to_string()),
            (AUTOSAVE_KEY, "-3".to_string()),
            (RECENT_COUNT_KEY, "0".to_string()),
            (THEME_KEY, "sepia".to_string()),
            (LIBRARY_KEY, lines),
            (MACRO_FOLDER_KEY, "  ".to_string()),
            (UPDATE_AUTO_KEY, "maybe".to_string()),
            (PLUGINS_OFF_KEY, "good_id\nBad Id\n../x\n".to_string()),
        ]
        .into();
        let prefs = Preferences::read(|k| map.get(k).cloned(), f32::NAN);
        assert_eq!(prefs.ui_scale, 200);
        assert_eq!(prefs.font_size, 80);
        assert_eq!(prefs.autosave_minutes, 1, "unparsable: the default");
        assert_eq!(prefs.recent_count, 1);
        assert_eq!(prefs.theme, ThemeChoice::System);
        assert_eq!(prefs.plugins_off, BTreeSet::from(["good_id".to_string()]), "invalid plugin ids dropped");
        assert_eq!(prefs.library_folders.len(), MAX_LIBRARY_FOLDERS, "bounded and deduplicated");
        assert_eq!(prefs.library_folders[0], PathBuf::from("/f0"));
        assert_eq!(prefs.macro_folder, None);
        assert!(prefs.check_updates, "only \"0\" turns it off");
        assert_eq!(Preferences::read(|_| None, f32::INFINITY).ui_scale, 100);
        assert_eq!(Preferences::read(|_| None, 9.0).ui_scale, 200);
        // A folder that does not exist is kept but not used.
        let gone = Preferences { macro_folder: Some(PathBuf::from("/no/such/folder/ccad")), ..Default::default() };
        assert_eq!(gone.macro_dir(), None);
        assert!(macro_files(Path::new("/no/such/folder/ccad")).is_empty());
    }

    #[test]
    fn macro_files_are_listed_sorted_and_bounded() {
        let dir = std::env::temp_dir().join(format!("ccad-macros-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("nested.ccmacro")).unwrap();
        for name in ["b.ccmacro", "A.CCMACRO", "notes.txt", "c.ccmacro"] {
            std::fs::write(dir.join(name), "").unwrap();
        }
        let names: Vec<String> =
            macro_files(&dir).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["A.CCMACRO", "b.ccmacro", "c.ccmacro"]);
        for i in 0..MAX_MACROS + 5 {
            std::fs::write(dir.join(format!("m{i:03}.ccmacro")), "").unwrap();
        }
        assert_eq!(macro_files(&dir).len(), MAX_MACROS);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Startup reads the stored recent projects up to the preferred count, and saving writes them back.
    #[test]
    fn load_and_save_restore_the_app_state() {
        let recent: String = (0..30).map(|i| format!("p{i}.ccad\n")).collect();
        let map: HashMap<&str, String> = [
            (RECENT_KEY, recent),
            (RECENT_COUNT_KEY, "5".to_string()),
            (RIBBON_KEY, "1".to_string()),
            (UPDATE_LAST_KEY, "1234".to_string()),
            // The current language: other tests running at the same time read it.
            (LANGUAGE_KEY, crate::i18n::language().code().to_string()),
        ]
        .into();
        let mut state = AppState::new(cc_core::Document::new(cc_core::Project::from_template()));
        load_language(|k| map.get(k).cloned(), None);
        load(&mut state, |k| map.get(k).cloned(), 1.0);
        assert_eq!(state.recent.len(), 5);
        assert!(state.ribbon_collapsed, "the ribbon starts collapsed");
        assert_eq!(state.updates.last_check, Some(1234));
        let language = crate::i18n::language();
        let mut out = HashMap::new();
        save(&state, |k, v| {
            out.insert(k.to_string(), v);
        });
        assert_eq!(out[RECENT_KEY].lines().count(), 5);
        assert_eq!(out[LANGUAGE_KEY], language.code());
        assert_eq!(out[RIBBON_KEY], "1");
    }

    /// Stored shortcuts are hostile input: unknown commands, fixed Ctrl
    /// chords and keys that are not two letters are dropped.
    #[test]
    fn stored_shortcuts_are_checked() {
        let keys = shortcut_lines(
            "Move=mo\nPin=\nNoSuchCommand=XX\nSave=SS\nRotate=R1\nUndo=ZZ\r\nbroken line\nCopyElements= cp ",
        );
        let expected = [("CopyElements", "CP"), ("Move", "MO"), ("Pin", "")];
        assert_eq!(keys, expected.map(|(a, b)| (a.to_string(), b.to_string())).into_iter().collect());
        let many: String = (0..2000).map(|_| "Move=MO\n").collect();
        assert_eq!(shortcut_lines(&many).len(), 1);
    }

    /// Keys of another command are refused; the default keys clear the change.
    #[test]
    fn shortcut_changes_check_conflicts() {
        let mut custom = BTreeMap::new();
        assert_eq!(set_keys(&mut custom, UiCommand::Move, "WA"), Err(UiCommand::Place(cc_core::Category::Wall)));
        assert!(custom.is_empty());
        assert_eq!(set_keys(&mut custom, UiCommand::Move, "QQ"), Ok(()));
        assert_eq!(keys_of(&custom, UiCommand::Move).as_deref(), Some("QQ"));
        // Its old keys are free now, and another command may take them.
        assert_eq!(set_keys(&mut custom, UiCommand::Rotate, "MV"), Ok(()));
        assert_eq!(set_keys(&mut custom, UiCommand::Move, "MV"), Err(UiCommand::Rotate));
        assert_eq!(set_keys(&mut custom, UiCommand::Rotate, "RO"), Ok(()));
        assert_eq!(set_keys(&mut custom, UiCommand::Move, "MV"), Ok(()));
        assert!(custom.is_empty(), "back to the defaults");
        assert_eq!(set_keys(&mut custom, UiCommand::Pin, ""), Ok(()));
        assert_eq!(keys_of(&custom, UiCommand::Pin), None);
        assert_eq!(keys_of(&custom, UiCommand::Save).as_deref(), Some("Ctrl+S"), "fixed");
    }

    #[test]
    fn long_paths_are_shortened_in_the_middle() {
        assert_eq!(middle_ellipsis("/short", 20), "/short");
        let long = "/home/someone/Documents/ConstructCAD/library/doors";
        let short = middle_ellipsis(long, 24);
        assert_eq!(short.chars().count(), 24);
        assert!(short.starts_with("/home/so") && short.ends_with("library/doors") && short.contains('…'));
    }
}
