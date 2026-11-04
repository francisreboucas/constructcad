//! Command palette (Ctrl+Shift+P): search every interface command, the
//! project's views and the engine's command catalog by name, description
//! or shortcut, and run the chosen one. Engine commands open a small form
//! with their JSON arguments, the same ones scripts and the MCP server use.

use crate::commands::UiCommand;
use crate::icons::{self, Badge, Icon, Tones};
use crate::state::AppState;
use crate::t;
use crate::theme;
use cc_core::{Category, LevelId, ViewCategory, ViewId};
use egui::{Key, RichText, Sense, Stroke, vec2};

/// The open palette.
#[derive(Default)]
pub struct Palette {
    query: String,
    /// Index of the highlighted match.
    selected: usize,
    /// Engine commands that do not replace the document, read once on opening.
    catalog: Vec<EngineCommand>,
    /// The engine command whose arguments are being written.
    form: Option<Form>,
    /// Focus the search field on the next frame.
    focus: bool,
    /// Not shown yet: the click that opened it (a button) must not close it.
    fresh: bool,
}

struct EngineCommand {
    name: String,
    description: String,
    schema: serde_json::Value,
}

struct Form {
    name: String,
    description: String,
    args: String,
    result: Option<Result<String, String>>,
}

impl Palette {
    pub fn new() -> Palette {
        // File commands (open, save, import, export) replace the document or
        // need file dialogs; their interface commands cover them.
        let catalog = cc_engine::catalog()
            .into_iter()
            .filter(|c| c.kind != cc_engine::CommandKind::File)
            .map(|c| EngineCommand { name: c.name, description: c.description, schema: c.input_schema })
            .collect();
        Palette { catalog, focus: true, fresh: true, ..Default::default() }
    }
}

/// What an entry does when chosen.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Action {
    Command(UiCommand),
    Plan {
        level: LevelId,
        ceiling: bool,
    },
    Schedule(Category),
    OpenView(ViewId),
    /// Index into the palette's catalog.
    Engine(usize),
}

struct Entry {
    action: Action,
    label: String,
    detail: String,
    shortcut: Option<String>,
    icon: Icon,
    badge: Option<Badge>,
    /// Why it cannot run now.
    why: Option<String>,
}

/// Lowercase without accents, so "niveis" finds "Níveis".
fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            '_' => ' ',
            c => c,
        })
        .collect()
}

/// How well `entry` matches the query words: `None` when a word is missing.
/// Words found at the start of the label, or the exact shortcut, rank first.
fn score(entry: &Entry, words: &[String]) -> Option<u32> {
    let label = fold(&entry.label);
    let rest = fold(&format!("{} {}", entry.detail, entry.shortcut.as_deref().unwrap_or_default()));
    let mut total = 0;
    for word in words {
        if entry.shortcut.as_deref().is_some_and(|s| fold(s) == *word) {
            total += 8;
        } else if label.starts_with(word.as_str()) {
            total += 4;
        } else if label.split([' ', ':', '(']).any(|w| w.starts_with(word.as_str())) {
            total += 3;
        } else if label.contains(word.as_str()) {
            total += 2;
        } else if rest.contains(word.as_str()) {
            total += 1;
        } else {
            return None;
        }
    }
    Some(total)
}

fn entries(state: &AppState, palette: &Palette) -> Vec<Entry> {
    let mut list = Vec::new();
    let command_entry = |command: UiCommand| Entry {
        action: Action::Command(command),
        label: command.label(state),
        detail: command.description(),
        shortcut: command.shortcut(),
        icon: command.icon(),
        badge: command.badge(),
        why: command.unavailable(state),
    };
    list.extend(UiCommand::all().into_iter().map(command_entry));
    if state.project_open && !state.show_home {
        let project = state.project();
        for level in &project.levels {
            for (ceiling, command) in [(false, UiCommand::FloorPlan), (true, UiCommand::CeilingPlan)] {
                list.push(Entry {
                    action: Action::Plan { level: level.id, ceiling },
                    label: format!("{}: {}", command.label(state), level.name),
                    detail: command.description(),
                    shortcut: None,
                    icon: command.icon(),
                    badge: command.badge(),
                    why: None,
                });
            }
        }
        for category in Category::SCHEDULABLE {
            list.push(Entry {
                action: Action::Schedule(category),
                label: format!("{}: {}", UiCommand::Schedule.label(state), crate::actions::t_category(category)),
                detail: UiCommand::Schedule.description(),
                shortcut: None,
                icon: Icon::Schedule,
                badge: Some(Badge::New),
                why: None,
            });
        }
        for view in project.views.values() {
            let category = view.category();
            let icon = match category {
                ViewCategory::FloorPlan => Icon::FloorPlan,
                ViewCategory::CeilingPlan => Icon::CeilingPlan,
                ViewCategory::Elevation => Icon::Elevation,
                ViewCategory::Section => Icon::Section,
                ViewCategory::ThreeD => Icon::View3D,
                ViewCategory::Legend => Icon::Legend,
                ViewCategory::Schedule => Icon::Schedule,
                ViewCategory::Diagram => Icon::Systems,
                ViewCategory::Sheet => Icon::Sheet,
            };
            list.push(Entry {
                action: Action::OpenView(view.id),
                label: t!("palette-open-view", "view" => state.view_title(view.id)),
                detail: crate::i18n::tr(category.key()),
                shortcut: None,
                icon,
                badge: None,
                why: None,
            });
        }
        for (i, command) in palette.catalog.iter().enumerate() {
            list.push(Entry {
                action: Action::Engine(i),
                label: command.name.to_string(),
                detail: command.description.clone(),
                shortcut: None,
                icon: Icon::MacroRun,
                badge: None,
                why: None,
            });
        }
    }
    list
}

/// The matches of the query, best first (all entries in their order when it is empty).
fn matches(entries: Vec<Entry>, query: &str) -> Vec<Entry> {
    let words: Vec<String> = fold(query).split_whitespace().map(String::from).collect();
    let mut scored: Vec<(u32, usize, Entry)> =
        entries.into_iter().enumerate().filter_map(|(i, e)| score(&e, &words).map(|s| (s, i, e))).collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, e)| e).collect()
}

/// JSON arguments with every required field, as a starting point.
fn skeleton(schema: &serde_json::Value) -> String {
    use serde_json::{Map, Value, json};
    let mut args = Map::new();
    let required = schema["required"].as_array().cloned().unwrap_or_default();
    for name in required.iter().filter_map(Value::as_str) {
        let field = &schema["properties"][name];
        let kind = match &field["type"] {
            Value::Array(types) => types.iter().filter_map(Value::as_str).find(|t| *t != "null").map(String::from),
            Value::String(t) => Some(t.clone()),
            _ => None,
        };
        let value = match kind.as_deref() {
            Some("string") => json!(""),
            Some("integer") | Some("number") => json!(0),
            Some("boolean") => json!(false),
            Some("array") => json!([]),
            Some("object") => json!({}),
            _ => Value::Null,
        };
        args.insert(name.to_string(), value);
    }
    serde_json::to_string_pretty(&Value::Object(args)).unwrap_or_else(|_| "{}".to_string())
}

/// Open or close the palette.
pub fn toggle(state: &mut AppState) {
    state.palette = match state.palette {
        Some(_) => None,
        None => Some(Palette::new()),
    };
}

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut palette) = state.palette.take() else { return };
    let mut keep = true;
    let screen = ctx.content_rect();
    let width = 600.0_f32.min(screen.width() - 32.0).max(200.0);
    let p = theme::palette(ctx);
    let frame = egui::Frame::new()
        .fill(p.panel)
        .stroke(Stroke::new(1.0, p.border_strong))
        .corner_radius(8)
        .shadow(ctx.global_style().visuals.window_shadow)
        .inner_margin(egui::Margin::same(8));
    let area = egui::Area::new(egui::Id::new("command-palette"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(screen.center().x - width / 2.0, screen.top() + 72.0))
        .show(ctx, |ui| {
            frame.show(ui, |ui| {
                ui.set_width(width);
                match palette.form.take() {
                    Some(form) => palette.form = engine_form(ui, state, form),
                    None => search(ui, state, &mut palette, &mut keep),
                }
            });
        });
    // Esc closes (a form goes back to the search first); so does a click elsewhere.
    let fresh = std::mem::take(&mut palette.fresh);
    let (escape, outside) = ctx.input(|i| {
        let pos = i.pointer.interact_pos();
        let click = !fresh && i.pointer.any_click();
        (i.key_pressed(Key::Escape), click && pos.is_some_and(|p| !area.response.rect.contains(p)))
    });
    if escape && palette.form.is_some() {
        palette.form = None;
        palette.focus = true;
    } else if escape || outside {
        keep = false;
    }
    if keep && state.palette.is_none() {
        state.palette = Some(palette);
    }
}

fn search(ui: &mut egui::Ui, state: &mut AppState, palette: &mut Palette, keep: &mut bool) {
    let p = theme::palette(ui.ctx());
    let field = egui::TextEdit::singleline(&mut palette.query)
        .hint_text(t!("palette-hint"))
        .desired_width(f32::INFINITY)
        .margin(egui::Margin::symmetric(8, 6))
        .font(egui::FontId::proportional(15.0));
    let response = ui.add(field);
    if palette.focus {
        response.request_focus();
        palette.focus = false;
    }
    if response.changed() {
        palette.selected = 0;
    }
    let found = matches(entries(state, palette), &palette.query);
    let (up, down, enter) =
        ui.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::Enter)));
    let last = found.len().saturating_sub(1);
    if down {
        palette.selected = (palette.selected + 1).min(last);
    }
    if up {
        palette.selected = palette.selected.saturating_sub(1);
    }
    palette.selected = palette.selected.min(last);
    ui.add_space(6.0);
    let mut chosen = enter.then_some(palette.selected);
    if found.is_empty() {
        ui.label(theme::muted(t!("palette-none")));
    }
    let row_height = 34.0;
    egui::ScrollArea::vertical().max_height(row_height * 10.0).auto_shrink([false, true]).show_rows(
        ui,
        row_height,
        found.len(),
        |ui, rows| {
            for i in rows {
                let Some(entry) = found.get(i) else { continue };
                let selected = i == palette.selected;
                let r = entry_row(ui, entry, selected, row_height);
                if selected && (up || down) {
                    r.scroll_to_me(None);
                }
                if r.clicked() {
                    chosen = Some(i);
                }
                if r.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                    palette.selected = i;
                }
            }
        },
    );
    ui.horizontal(|ui| {
        let hint = RichText::new(t!("palette-keys")).size(11.0).color(p.text_faint);
        ui.label(hint);
    });
    let Some(entry) = chosen.and_then(|i| found.get(i)) else { return };
    if let Some(why) = &entry.why {
        state.message = why.clone();
        return;
    }
    let ctx = ui.ctx().clone();
    match entry.action {
        Action::Command(command) => {
            *keep = false;
            crate::commands::run(state, &ctx, command);
        }
        Action::Plan { level, ceiling } => {
            *keep = false;
            crate::actions::new_plan(state, level, ceiling);
        }
        Action::Schedule(category) => {
            *keep = false;
            crate::actions::new_schedule(state, category);
        }
        Action::OpenView(view) => {
            *keep = false;
            state.open_view(view);
        }
        Action::Engine(i) => {
            if let Some(command) = palette.catalog.get(i) {
                palette.form = Some(Form {
                    name: command.name.clone(),
                    description: command.description.clone(),
                    args: skeleton(&command.schema),
                    result: None,
                });
            }
        }
    }
}

fn entry_row(ui: &mut egui::Ui, entry: &Entry, selected: bool, height: f32) -> egui::Response {
    let p = theme::palette(ui.ctx());
    let enabled = entry.why.is_none();
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, theme::RADIUS as f32, p.accent_soft);
    } else if response.hovered() {
        painter.rect_filled(rect, theme::RADIUS as f32, p.hover);
    }
    let icon_rect = egui::Rect::from_center_size(rect.left_center() + vec2(18.0, 0.0), vec2(18.0, 18.0));
    icons::paint_toned(painter, icon_rect, entry.icon, Tones::of(p, entry.badge, enabled, selected), entry.badge);
    let text_color = if enabled { p.text } else { p.text_faint };
    let engine = matches!(entry.action, Action::Engine(_));
    let font = if engine { egui::FontId::monospace(13.0) } else { egui::FontId::proportional(13.5) };
    let left = rect.left() + 38.0;
    let label = painter.layout_no_wrap(entry.label.clone(), font, text_color);
    let label_w = label.size().x;
    painter.galley(egui::pos2(left, rect.center().y - label.size().y / 2.0), label, text_color);
    // The detail (or why it is unavailable) fills the rest of the row, cut to fit.
    let mut right = rect.right() - 8.0;
    if let Some(keys) = entry.shortcut.as_deref() {
        let galley = painter.layout_no_wrap(keys.to_string(), egui::FontId::proportional(11.5), p.text_muted);
        let size = galley.size() + vec2(10.0, 4.0);
        let cap = egui::Rect::from_min_size(egui::pos2(right - size.x, rect.center().y - size.y / 2.0), size);
        painter.rect(cap, 3.0, p.surface, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
        painter.galley(cap.min + vec2(5.0, 2.0), galley, p.text_muted);
        right = cap.left() - 8.0;
    }
    let detail_left = left + label_w + 12.0;
    if right - detail_left > 40.0 {
        let (detail, color) = match &entry.why {
            Some(why) => (why.clone(), p.warning),
            None => (entry.detail.clone(), p.text_faint),
        };
        let mut job = egui::text::LayoutJob::simple_singleline(detail, egui::FontId::proportional(12.0), color);
        job.wrap = egui::text::TextWrapping::truncate_at_width(right - detail_left);
        let galley = painter.layout_job(job);
        painter.galley(egui::pos2(detail_left, rect.center().y - galley.size().y / 2.0), galley, color);
    }
    let label = entry.label.clone();
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, &label));
    response
}

/// The arguments of an engine command, a Run button and the result.
fn engine_form(ui: &mut egui::Ui, state: &mut AppState, mut form: Form) -> Option<Form> {
    let p = theme::palette(ui.ctx());
    ui.horizontal(|ui| {
        let (icon, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
        icons::paint(ui.painter(), icon, Icon::MacroRun, p.text);
        ui.label(RichText::new(form.name.as_str()).monospace().strong().color(p.text));
    });
    ui.label(theme::muted(&form.description));
    ui.add_space(4.0);
    ui.label(RichText::new(t!("palette-arguments")).size(12.0).color(p.text_muted));
    let editor = egui::TextEdit::multiline(&mut form.args)
        .code_editor()
        .desired_rows(6)
        .desired_width(f32::INFINITY)
        .lock_focus(true);
    let response = ui.add(editor);
    if !response.has_focus() && form.result.is_none() && !ui.ctx().memory(|m| m.focused().is_some()) {
        response.request_focus();
    }
    let run_keys = ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Enter));
    let mut run = run_keys;
    let mut back = false;
    ui.horizontal(|ui| {
        if crate::dialog::styled_button(ui, &t!("palette-run"), crate::dialog::Style::Primary).clicked() {
            run = true;
        }
        if ui.button(t!("palette-back")).clicked() {
            back = true;
        }
        ui.label(RichText::new("Ctrl+Enter").size(11.0).color(p.text_faint));
    });
    if run {
        form.result = Some(match serde_json::from_str::<serde_json::Value>(&form.args) {
            Ok(args) => match state.session.execute(&form.name, args) {
                Ok(out) => Ok(serde_json::to_string_pretty(&out).unwrap_or_default()),
                Err(e) => Err(e.message),
            },
            Err(e) => Err(t!("palette-bad-json", "error" => e)),
        });
    }
    match &form.result {
        Some(Ok(out)) => {
            ui.label(RichText::new(t!("palette-result")).size(12.0).color(p.success));
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                ui.label(RichText::new(out).monospace().size(12.0));
            });
        }
        Some(Err(message)) => {
            ui.label(RichText::new(message).color(p.danger));
        }
        None => {}
    }
    (!back).then_some(form)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(label: &str, detail: &str, shortcut: Option<&str>) -> Entry {
        Entry {
            action: Action::Command(UiCommand::Select),
            label: label.to_string(),
            detail: detail.to_string(),
            shortcut: shortcut.map(str::to_string),
            icon: Icon::Select,
            badge: None,
            why: None,
        }
    }

    #[test]
    fn search_ignores_case_and_accents_and_ranks_labels_first() {
        let list = [
            entry("Mover", "Move os elementos", Some("MV")),
            entry("Níveis", "Adiciona níveis", None),
            entry("Parede", "Desenha paredes", Some("WA")),
            entry("Planta de piso: Térreo", "Cria uma planta", None),
        ];
        let labels = |query: &str| -> Vec<String> {
            let list = list.iter().map(|e| entry(&e.label, &e.detail, e.shortcut.as_deref())).collect::<Vec<_>>();
            matches(list, query).into_iter().map(|e| e.label).collect()
        };
        assert_eq!(labels("NIVEIS"), ["Níveis"]);
        assert_eq!(labels("wa"), ["Parede"], "a shortcut finds its command");
        assert_eq!(labels("planta terreo"), ["Planta de piso: Térreo"], "every word must match");
        assert_eq!(labels("paredes"), ["Parede"], "descriptions match too");
        assert_eq!(labels("").len(), 4);
        assert!(labels("xyz").is_empty());
    }

    #[test]
    fn skeleton_lists_required_arguments() {
        let schema = serde_json::json!({
            "properties": {
                "view": {"type": ["integer", "string"]},
                "text": {"type": "string"},
                "position": {"type": "array"},
                "width": {"type": ["number", "null"]}
            },
            "required": ["view", "text", "position"]
        });
        let args: serde_json::Value = serde_json::from_str(&skeleton(&schema)).unwrap();
        assert_eq!(args, serde_json::json!({"view": 0, "text": "", "position": []}));
        // Every engine command offered by the palette gets valid JSON.
        for command in Palette::new().catalog {
            assert!(serde_json::from_str::<serde_json::Value>(&skeleton(&command.schema)).is_ok(), "{}", command.name);
        }
    }
}
