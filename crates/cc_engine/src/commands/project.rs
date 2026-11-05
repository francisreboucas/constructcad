//! The session's project: new, open, save, information, undo, units, language.

use super::{edit, patch_value};
use crate::error::{EngineError, Result};
use crate::registry::{Command, CommandKind};
use crate::session::{Session, guard, new_project};
use crate::t;
use cc_core::Document;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Start a new project from the template (unsaved changes are discarded).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectNew {}

impl Command for ProjectNew {
    const NAME: &'static str = "project_new";
    const KIND: CommandKind = CommandKind::File;
    fn run(self, s: &mut Session) -> Result<Value> {
        s.replace(Document::new(new_project()));
        Ok(summary(s))
    }
}

/// Open a ConstructCAD project (.ccad), replacing the session's project.
/// Damaged files are repaired and the number of repairs reported.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectOpen {
    /// Path of the .ccad file.
    pub path: PathBuf,
}

impl Command for ProjectOpen {
    const NAME: &'static str = "project_open";
    const KIND: CommandKind = CommandKind::File;
    fn run(self, s: &mut Session) -> Result<Value> {
        let (opened, repairs) = Session::open(&self.path)?;
        // Replace the document only: plugins and the running command stay.
        s.replace(opened.doc);
        let mut out = summary(s);
        out["repairs"] = json!(repairs);
        Ok(out)
    }
}

/// Save the project to its file, or to `path` (which becomes its file).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectSave {
    /// Where to save; required when the project has no file yet.
    #[serde(default)]
    pub path: Option<PathBuf>,
}

impl Command for ProjectSave {
    const NAME: &'static str = "project_save";
    const KIND: CommandKind = CommandKind::File;
    fn run(self, s: &mut Session) -> Result<Value> {
        let path = self
            .path
            .or_else(|| s.doc.path.clone())
            .ok_or_else(|| EngineError::invalid("the project has no file yet: give a path"))?;
        guard(|| cc_core::save(s.project(), &path))??;
        s.doc.path = Some(path.clone());
        s.doc.mark_saved();
        Ok(json!({ "path": path }))
    }
}

/// Project information, file, unsaved state and counts.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectInfoGet {}

impl Command for ProjectInfoGet {
    const NAME: &'static str = "project_info";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        Ok(summary(s))
    }
}

fn summary(s: &Session) -> Value {
    let p = s.project();
    let i = &p.info;
    json!({
        "name": i.name,
        "number": i.number,
        "client": i.client,
        "address": i.address,
        "author": i.author,
        "path": s.doc.path,
        "unsaved_changes": s.doc.is_dirty(),
        "can_undo": s.doc.undo_name(),
        "can_redo": s.doc.redo_name(),
        "levels": p.levels.len(),
        "elements": p.elements.len(),
        "types": p.types.len(),
        "views": p.views.len(),
        "materials": p.materials.len(),
    })
}

/// Change the project information (title block fields).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectInfoSet {
    /// Project name.
    #[serde(default)]
    pub name: Option<String>,
    /// Project number.
    #[serde(default)]
    pub number: Option<String>,
    /// Client name.
    #[serde(default)]
    pub client: Option<String>,
    /// Project address.
    #[serde(default)]
    pub address: Option<String>,
    /// Author of the drawings.
    #[serde(default)]
    pub author: Option<String>,
}

impl Command for ProjectInfoSet {
    const NAME: &'static str = "project_info_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        edit(s, t!("tx-project-info"), |e| {
            let info = e.info_mut();
            for (field, value) in [
                (&mut info.name, self.name),
                (&mut info.number, self.number),
                (&mut info.client, self.client),
                (&mut info.address, self.address),
                (&mut info.author, self.author),
            ] {
                if let Some(v) = value {
                    *field = v;
                }
            }
            Ok(())
        })?;
        Ok(summary(s))
    }
}

/// Undo the last edit.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Undo {}

impl Command for Undo {
    const NAME: &'static str = "undo";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let name = s.doc.undo_name().map(str::to_string);
        if !s.doc.undo() {
            return Err(EngineError::rejected("nothing to undo"));
        }
        Ok(json!({ "undone": name }))
    }
}

/// Redo the last undone edit.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Redo {}

impl Command for Redo {
    const NAME: &'static str = "redo";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let name = s.doc.redo_name().map(str::to_string);
        if !s.doc.redo() {
            return Err(EngineError::rejected("nothing to redo"));
        }
        Ok(json!({ "redone": name }))
    }
}

/// The project units (length, area, volume, angle, slope, currency, number format).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnitsGet {}

impl Command for UnitsGet {
    const NAME: &'static str = "units_get";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        serde_json::to_value(&s.project().units).map_err(|e| EngineError::invalid(e.to_string()))
    }
}

/// Change the project units: `values` is merged into the object `units_get`
/// returns (only the fields given change), after `preset` when given.
/// Length units are `Millimeter`, `Centimeter`, `Meter`, `Foot`, `Inch`,
/// `FeetInches` (`12'-6 1/2"`) and `FractionalInch`; for the last two,
/// `decimals` is the precision, 1/2^decimals of an inch.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnitsSet {
    /// Fields to change, shaped like the result of `units_get`.
    #[serde(default)]
    pub values: Value,
    /// Start from `metric` (the defaults) or `imperial` (feet and inches to 1/16", square and cubic feet).
    #[serde(default)]
    pub preset: Option<String>,
}

impl Command for UnitsSet {
    const NAME: &'static str = "units_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let base = match self.preset.as_deref() {
            None => s.project().units.clone(),
            Some("metric") => cc_core::Units::default(),
            Some("imperial") => cc_core::Units::imperial(),
            Some(other) => return Err(EngineError::invalid(format!("unknown preset '{other}' (metric or imperial)"))),
        };
        let values = if self.values.is_null() { serde_json::json!({}) } else { self.values };
        let mut units = patch_value(&base, &values)?;
        if units.repair() {
            return Err(EngineError::invalid("invalid unit values"));
        }
        edit(s, t!("tx-units"), |e| {
            *e.units_mut() = units;
            Ok(())
        })?;
        UnitsGet {}.run(s)
    }
}

/// Language of default names, messages and sheet labels (`en`, `pt-BR`, `es`, `fr`, `de`, `it`, `lo`, `pl`, `uk-UA`, `zh-TW`, `zh-CN` or `ja`).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LanguageSet {
    /// Language code: `en`, `pt-BR`, `es`, `fr`, `de`, `it`, `lo`, `pl`, `uk-UA`, `zh-TW`, `zh-CN` or `ja`.
    pub language: String,
}

impl Command for LanguageSet {
    const NAME: &'static str = "language_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let lang = crate::i18n::Language::from_code(&self.language)
            .ok_or_else(|| EngineError::invalid(format!("unsupported language '{}'", self.language)))?;
        crate::i18n::set_language(lang);
        s.invalidate_drawings();
        Ok(json!({ "language": lang.code() }))
    }
}
