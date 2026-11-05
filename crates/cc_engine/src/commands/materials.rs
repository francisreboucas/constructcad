//! Materials: create, duplicate, rename, change, delete.

use super::{edit, patch_value};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{Material, Project};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// `base`, or `base 2`, `base 3`… when taken.
fn unique_name(p: &Project, base: &str) -> String {
    let base = base.trim();
    let mut name = base.to_string();
    let mut n = 2;
    while p.material(&name).is_some() {
        name = format!("{base} {n}");
        n += 1;
    }
    name
}

fn material_out(s: &Session, name: &str) -> Value {
    s.project().material(name).and_then(|m| serde_json::to_value(m).ok()).unwrap_or(json!({ "name": name }))
}

/// Create a material (default name "Material"; a number is added when taken).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaterialCreate {
    /// Name (default: "Material"; a number is added when taken).
    #[serde(default)]
    pub name: Option<String>,
    /// Shading color `[r, g, b]`, 0–255 (default light gray).
    #[serde(default)]
    pub color: Option<[u8; 3]>,
}

impl Command for MaterialCreate {
    const NAME: &'static str = "material_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let name = unique_name(s.project(), &self.name.unwrap_or_else(|| t!("mat-new-name")));
        let material = Material::new(name.clone(), self.color.unwrap_or([190, 190, 190]));
        edit(s, t!("tx-modify-material"), |e| {
            e.materials_mut().push(material);
            Ok(())
        })?;
        Ok(material_out(s, &name))
    }
}

/// Copy a material under a new name (default "<name> (copy)").
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaterialDuplicate {
    pub material: Ref,
    /// Name of the copy (default: "<name> (copy)").
    #[serde(default)]
    pub name: Option<String>,
}

impl Command for MaterialDuplicate {
    const NAME: &'static str = "material_duplicate";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let src = p
            .material(&refs::material(p, &self.material)?)
            .cloned()
            .ok_or_else(|| EngineError::not_found("material"))?;
        let name = unique_name(p, &self.name.unwrap_or_else(|| t!("copy-of", "name" => src.name)));
        let copy = Material { name: name.clone(), ..src };
        edit(s, t!("tx-modify-material"), |e| {
            e.materials_mut().push(copy);
            Ok(())
        })?;
        Ok(material_out(s, &name))
    }
}

/// Rename a material; every type using it follows.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaterialRename {
    pub material: Ref,
    /// New name.
    pub name: String,
}

impl Command for MaterialRename {
    const NAME: &'static str = "material_rename";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let old = refs::material(s.project(), &self.material)?;
        let new = self.name.trim().to_string();
        if new.is_empty() {
            return Err(EngineError::invalid("the name is empty"));
        }
        if new != old {
            edit(s, t!("tx-rename-material"), |e| {
                if e.rename_material(&old, &new) {
                    Ok(())
                } else {
                    Err(EngineError::rejected("the name is already used"))
                }
            })?;
        }
        Ok(material_out(s, &new))
    }
}

/// Change a material's fields (merged into what `material_get` shows:
/// description, class, colors, patterns, transparency, finish, physical
/// values…). Use `material_rename` to rename it.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaterialSet {
    pub material: Ref,
    /// Fields to change, shaped like the result of `material_get`.
    pub values: Value,
}

impl Command for MaterialSet {
    const NAME: &'static str = "material_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let name = refs::material(p, &self.material)?;
        let current = p.material(&name).ok_or_else(|| EngineError::not_found("material"))?;
        let mut changed = patch_value(current, &self.values)?;
        if changed.name != name {
            return Err(EngineError::invalid("use material_rename to rename a material"));
        }
        if changed.repair() > 0 {
            return Err(EngineError::invalid("invalid material values"));
        }
        edit(s, t!("tx-modify-material"), |e| {
            if let Some(m) = e.materials_mut().iter_mut().find(|m| m.name == name) {
                *m = changed;
            }
            Ok(())
        })?;
        Ok(material_out(s, &name))
    }
}

/// Delete a material no type uses.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaterialDelete {
    pub material: Ref,
}

impl Command for MaterialDelete {
    const NAME: &'static str = "material_delete";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let name = refs::material(p, &self.material)?;
        let users = p.material_users(&name).len();
        if users > 0 {
            return Err(EngineError::rejected(format!("{users} types use this material")));
        }
        let elements = p.material_instance_users(&name).len();
        if elements > 0 {
            return Err(EngineError::rejected(format!("{elements} components use this material")));
        }
        edit(s, t!("tx-modify-material"), |e| {
            e.materials_mut().retain(|m| m.name != name);
            Ok(())
        })?;
        Ok(json!({ "deleted": name }))
    }
}
