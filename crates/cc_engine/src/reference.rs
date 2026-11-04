//! The command reference (`docs/commands.md`), generated from the catalog
//! so it always matches the engine: `ccad reference > docs/commands.md`.

use crate::registry::{CommandKind, catalog};
use serde_json::Value;

/// A readable type for a parameter schema.
fn type_label(schema: &Value) -> String {
    if let Some(alternatives) = schema.get("anyOf").and_then(Value::as_array) {
        let labels: Vec<String> =
            alternatives.iter().filter(|a| a.get("type") != Some(&Value::from("null"))).map(type_label).collect();
        let is_ref =
            labels.len() == 2 && labels.contains(&"integer".to_string()) && labels.contains(&"string".to_string());
        return if is_ref { "id or name".into() } else { labels.join(" or ") };
    }
    let types: Vec<&str> = match schema.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).filter(|t| *t != "null").collect(),
        _ => Vec::new(),
    };
    match types.as_slice() {
        ["array"] => {
            let items = schema.get("items").cloned().unwrap_or(Value::Null);
            let pair =
                schema.get("minItems") == Some(&Value::from(2)) && schema.get("maxItems") == Some(&Value::from(2));
            let item = type_label(&items);
            match (pair, item.as_str()) {
                (true, "number") => "[x, y]".into(),
                (true, "[x, y]") => "[[x, y], [x, y]]".into(),
                (true, "string") => "[old, new]".into(),
                (_, "integer") => "[id, …]".into(),
                (_, "") => "list".into(),
                (_, item) => format!("[{item}, …]"),
            }
        }
        [one] => (*one).to_string(),
        [] => "JSON".into(),
        many => many.join(" or "),
    }
}

/// One line of text from a description (tables cannot hold line breaks).
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").replace('|', "\\|")
}

/// The Markdown reference of every command.
pub fn markdown() -> String {
    let mut out = String::from(
        "# Command reference\n\n\
         <!-- Generated from the engine's catalog: `cargo run -p cc_cli -- reference > docs/commands.md`. \
         Do not edit by hand. -->\n\n\
         Every command runs the same way from the desktop app, `ccad run` / `ccad script` and MCP \
         (see [the automation guide](automation.md)). Lengths are in meters, angles in degrees, points \
         `[x, y]` in plan (in a view's drawing coordinates for dimensions). *id or name* parameters accept \
         a numeric id or the name of a level, type, view or material. Kinds: **query** reads the model, \
         **edit** changes it (one undo step, refused if it would leave the model invalid), **file** works \
         with files or replaces the project.\n",
    );
    let commands = catalog();
    let mut groups: Vec<&str> = Vec::new();
    for c in &commands {
        if !groups.contains(&c.group.as_str()) {
            groups.push(&c.group);
        }
    }
    out.push_str("\n## Contents\n\n");
    for g in &groups {
        let names: Vec<String> =
            commands.iter().filter(|c| c.group == *g).map(|c| format!("[`{0}`](#{0})", c.name)).collect();
        out.push_str(&format!("- **{g}**: {}\n", names.join(", ")));
    }
    for g in &groups {
        out.push_str(&format!("\n## {g}\n"));
        for c in commands.iter().filter(|c| c.group == *g) {
            let kind = match c.kind {
                CommandKind::Query => "query",
                CommandKind::Edit => "edit",
                CommandKind::File => "file",
            };
            out.push_str(&format!("\n### `{}`\n\n*{kind}* — {}\n", c.name, c.description));
            let properties = c.input_schema.get("properties").and_then(Value::as_object);
            let required: Vec<&str> = c
                .input_schema
                .get("required")
                .and_then(Value::as_array)
                .map(|r| r.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            match properties.filter(|p| !p.is_empty()) {
                None => out.push_str("\nNo parameters.\n"),
                Some(props) => {
                    out.push_str("\n| Parameter | Type | | Description |\n|---|---|---|---|\n");
                    let mut names: Vec<&String> = props.keys().collect();
                    // Required parameters first, then in alphabetical order.
                    names.sort_by_key(|n| (!required.contains(&n.as_str()), n.to_string()));
                    for name in names {
                        let schema = &props[name];
                        let need = if required.contains(&name.as_str()) { "required" } else { "optional" };
                        let description = schema.get("description").and_then(Value::as_str).map(one_line);
                        out.push_str(&format!(
                            "| `{name}` | {} | {need} | {} |\n",
                            type_label(schema),
                            description.unwrap_or_default()
                        ));
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `docs/commands.md` is the generated reference of the current catalog.
    #[test]
    fn reference_is_current() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/commands.md");
        let written = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
        assert!(
            written == markdown(),
            "docs/commands.md is out of date: run `cargo run -p cc_cli -- reference > docs/commands.md`"
        );
    }

    #[test]
    fn every_parameter_is_described() {
        let mut missing = Vec::new();
        for c in catalog() {
            for (name, schema) in c.input_schema["properties"].as_object().into_iter().flatten() {
                if schema.get("description").is_none() {
                    missing.push(format!("{}.{name}", c.name));
                }
            }
        }
        assert!(missing.is_empty(), "parameters without a doc comment: {missing:?}");
    }
}
