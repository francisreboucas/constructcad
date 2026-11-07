//! A Model Context Protocol server over standard input/output: JSON-RPC 2.0
//! messages, one per line. Every engine command is an MCP tool with the
//! same name, description and input schema; one session (one open project)
//! lives as long as the server.

use cc_engine::{CommandKind, Session};
use serde_json::{Value, json};
use std::io::{BufRead, Write};

/// Protocol revisions this server speaks, newest first.
const VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "ConstructCraft BIM engine. One project is open per session: start with \
project_new, project_open or ifc_import; save with project_save. Lengths are meters, angles degrees, \
points [x, y]. Levels, types, views and materials accept an id or a name; elements take ids (see \
elements_list). element_get/type_get/view_get show the params that element_set/type_set/view_set change. \
Every edit is one undo step (undo, redo).";

/// Serve until standard input closes.
pub fn serve(mut session: Session, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let replies = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => batch.into_iter().filter_map(|m| handle(&mut session, m)).collect(),
            Ok(message) => handle(&mut session, message).into_iter().collect(),
            Err(e) => vec![error(Value::Null, -32700, &format!("parse error: {e}"))],
        };
        for reply in replies {
            writeln!(output, "{reply}")?;
        }
        output.flush()?;
    }
    Ok(())
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn reply(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// The reply to one message (none for notifications).
pub fn handle(session: &mut Session, message: Value) -> Option<Value> {
    let id = message.get("id").cloned();
    let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
    let params = message.get("params").cloned().unwrap_or(json!({}));
    // Notifications (no id) get no reply; responses to our requests do not occur.
    let id = id?;
    Some(match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
            let version = VERSIONS.iter().find(|v| **v == asked).copied().unwrap_or(VERSIONS[0]);
            reply(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "constructcad", "title": "ConstructCraft", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => reply(id, json!({})),
        "tools/list" => reply(id, json!({ "tools": tools(session) })),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(error(id, -32602, "tools/call needs a name"));
            };
            if session.kind_of(name).is_none() {
                return Some(error(id, -32602, &format!("unknown tool: {name}")));
            }
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            reply(id, call(session, name, args))
        }
        _ => error(id, -32601, &format!("method not found: {method}")),
    })
}

fn tools(session: &Session) -> Vec<Value> {
    session
        .commands()
        .into_iter()
        .map(|c| {
            json!({
                "name": c.name,
                "description": c.description,
                "inputSchema": c.input_schema,
                "annotations": {
                    "readOnlyHint": c.kind == CommandKind::Query,
                    "destructiveHint": c.kind != CommandKind::Query && (c.name.contains("delete") || c.kind == CommandKind::File),
                    "idempotentHint": c.kind == CommandKind::Query,
                    "openWorldHint": false,
                },
            })
        })
        .collect()
}

/// A tool result: the command's JSON as text and as structured content;
/// failures are tool errors the model can read and correct.
fn call(session: &mut Session, name: &str, args: Value) -> Value {
    let result = session.execute(name, args);
    // What plugins printed: standard error (standard output carries the protocol).
    for line in session.take_extension_messages() {
        eprintln!("{line}");
    }
    match result {
        Ok(out) => {
            let text = serde_json::to_string_pretty(&out).unwrap_or_default();
            // Structured content must be an object.
            let structured = if out.is_object() { out } else { json!({ "result": out }) };
            json!({ "content": [{ "type": "text", "text": text }], "structuredContent": structured, "isError": false })
        }
        Err(e) => {
            let text = serde_json::to_string_pretty(&json!({ "error": e })).unwrap_or_default();
            json!({ "content": [{ "type": "text", "text": text }], "isError": true })
        }
    }
}
