//! The `ccad` binary: one-off commands, scripts and an MCP session.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

fn ccad(args: &[&str], stdin: &str) -> (i32, Value, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ccad"))
        .args(args)
        .env("CCAD_LANG", "en")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let value = serde_json::from_str(&stdout).unwrap_or(Value::String(stdout));
    (out.status.code().unwrap_or(-1), value, String::from_utf8_lossy(&out.stderr).into_owned())
}

fn scratch(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("ccad-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name).to_string_lossy().into_owned()
}

#[test]
fn run_saves_and_reopens() {
    let file = scratch("one.ccad");
    let (code, wall, err) =
        ccad(&["--save-as", &file, "run", "wall_create", "start=[0,0]", "end=[5,0]", "height=2.5"], "");
    assert_eq!(code, 0, "{err}");
    assert_eq!(wall["derived"]["length"], 5.0);
    // JSON arguments, on the saved project.
    let (code, got, err) = ccad(&["-p", &file, "run", "element_get", &format!("{{\"id\": {}}}", wall["id"])], "");
    assert_eq!(code, 0, "{err}");
    assert_eq!(got["params"]["height"], 2.5);
    // Errors go to stderr as JSON with exit code 1; usage errors exit with 2.
    let (code, _, err) = ccad(&["run", "element_get", "id=424242"], "");
    assert_eq!(code, 1);
    assert!(err.contains("not_found"), "{err}");
    assert_eq!(ccad(&["run", "wall_create", "oops"], "").0, 2);
    assert_eq!(ccad(&["frobnicate"], "").0, 2);
}

#[test]
fn scripts_run_in_order_and_stop_at_errors() {
    let script = r#"
{"command": "level_create", "args": {"name": "Upper", "elevation": 3}}
{"command": "wall_create", "args": {"start": [0, 0], "end": [4, 0], "top_level": "Upper"}}
{"command": "elements_list", "args": {"category": "wall"}}
"#;
    let (code, out, err) = ccad(&["script", "-"], script);
    assert_eq!(code, 0, "{err}");
    let steps = out.as_array().unwrap();
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[1]["result"]["derived"]["height"], 3.0);
    let failing = r#"[{"command": "undo"}, {"command": "project_info"}]"#;
    let (code, out, err) = ccad(&["script", "-"], failing);
    assert_eq!(code, 1);
    assert_eq!(out, json!([]));
    assert!(err.contains("step 1"), "{err}");
}

#[test]
fn catalog_and_schemas() {
    let (code, list, _) = ccad(&["commands"], "");
    assert_eq!(code, 0);
    assert!(list.as_array().unwrap().iter().any(|c| c["name"] == "wall_create"));
    let (code, schema, _) = ccad(&["schema", "door_create"], "");
    assert_eq!(code, 0);
    assert!(schema["input_schema"]["properties"]["wall"].is_object());
}

#[test]
fn mcp_session() {
    let messages = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "wall_create", "arguments": {"start": [0, 0], "end": [3, 0]}}}),
        json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "elements_list", "arguments": {}}}),
        json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "element_get", "arguments": {"id": 999999}}}),
        json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": {"name": "nope"}}),
        json!({"jsonrpc": "2.0", "id": 7, "method": "resources/list"}),
        json!({"jsonrpc": "2.0", "id": 8, "method": "ping"}),
    ];
    let input: String = messages.iter().map(|m| format!("{m}\n")).collect::<String>() + "not json\n";
    let mut child = Command::new(env!("CARGO_BIN_EXE_ccad"))
        .args(["mcp"])
        .env("CCAD_LANG", "en")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    let replies: Vec<Value> =
        String::from_utf8_lossy(&out.stdout).lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    // No reply to the notification.
    assert_eq!(replies.len(), 9);
    let by_id = |id: i64| replies.iter().find(|r| r["id"] == id).unwrap();
    assert_eq!(by_id(1)["result"]["protocolVersion"], "2025-06-18");
    assert!(by_id(1)["result"]["capabilities"]["tools"].is_object());
    let tools = by_id(2)["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), cc_count());
    let list = tools.iter().find(|t| t["name"] == "elements_list").unwrap();
    assert_eq!(list["annotations"]["readOnlyHint"], true);
    assert_eq!(by_id(3)["result"]["isError"], false);
    assert_eq!(by_id(3)["result"]["structuredContent"]["category"], "wall");
    // The wall stays in the session for the next call.
    assert_eq!(by_id(4)["result"]["structuredContent"]["result"].as_array().unwrap().len(), 1);
    assert_eq!(by_id(5)["result"]["isError"], true);
    assert_eq!(by_id(6)["error"]["code"], -32602);
    assert_eq!(by_id(7)["error"]["code"], -32601);
    assert_eq!(by_id(8)["result"], json!({}));
    assert_eq!(replies.last().unwrap()["error"]["code"], -32700);
}

fn cc_count() -> usize {
    ccad(&["commands"], "").1.as_array().unwrap().len()
}

#[test]
fn version() {
    for flag in ["--version", "-V"] {
        let (code, out, err) = ccad(&[flag], "");
        assert_eq!(code, 0, "{err}");
        assert_eq!(
            out,
            Value::String(format!(
                "ccad {}
",
                env!("CARGO_PKG_VERSION")
            ))
        );
    }
}

#[test]
fn lua_scripts() {
    let script = r#"
local n = ccad.args.n
for i = 1, n do ccad.run("wall_create", { start = { i * 2, 0 }, ["end"] = { i * 2 + 1, 0 } }) end
print("made " .. n)
return { walls = #ccad.run("elements_list", { category = "wall" }) }
"#;
    let (code, out, err) = ccad(&["lua", "-", "n=3"], script);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, Value::String("made 3\n{\n  \"walls\": 3\n}\n".into()));
    let (code, _, err) = ccad(&["lua", "-"], "error('nope')");
    assert_eq!(code, 1);
    assert!(err.contains("script") && err.contains("nope"), "{err}");
}

#[test]
fn plugins_add_commands() {
    let dir = std::env::temp_dir().join(format!("ccad-cli-{}-plugins", std::process::id()));
    let plugin = dir.join("tools");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::write(plugin.join("plugin.toml"), "id = \"tools\"\nname = \"Tools\"\nversion = \"1.0\"\n").unwrap();
    std::fs::write(
        plugin.join("main.lua"),
        r#"ccad.command { name = "pair", description = "Two walls",
             run = function(a)
               ccad.run("wall_create", { start = { 0, 0 }, ["end"] = { 1, 0 } })
               ccad.run("wall_create", { start = { 0, 1 }, ["end"] = { 1, 1 } })
               print("pair done")
               return { ok = true }
             end }"#,
    )
    .unwrap();
    let folder = dir.to_string_lossy().into_owned();
    let (code, list, err) = ccad(&["--plugins", &folder, "commands"], "");
    assert_eq!(code, 0, "{err}");
    let names: Vec<&str> = list.as_array().unwrap().iter().filter_map(|c| c["name"].as_str()).collect();
    assert!(names.contains(&"tools__pair"));
    assert_eq!(names.len(), cc_count() + 1);
    let (code, out, err) = ccad(&["--plugins", &folder, "run", "tools__pair"], "");
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, json!({"ok": true}));
    assert!(err.contains("[tools] pair done"), "{err}");
    let (code, summary, err) = ccad(&["--plugins", &folder, "plugins"], "");
    assert_eq!(code, 0, "{err}");
    assert_eq!(summary[0]["id"], "tools");
    assert_eq!(summary[0]["commands"][0]["name"], "tools__pair");
    // A plugin that fails to load is reported, the others still run.
    std::fs::create_dir_all(dir.join("broken")).unwrap();
    std::fs::write(dir.join("broken").join("plugin.toml"), "id = 7").unwrap();
    let (code, _, err) = ccad(&["--plugins", &folder, "run", "tools__pair"], "");
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("plugin not loaded"), "{err}");
    // Over MCP, the plugin's command is a tool.
    let messages = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "tools__pair", "arguments": {}}}),
    ];
    let input: String = messages.iter().map(|m| format!("{m}\n")).collect();
    let (_, out, _) = ccad(&["--plugins", &folder, "mcp"], &input);
    let replies: Vec<Value> = out.as_str().unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert!(replies[0]["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "tools__pair"));
    assert_eq!(replies[1]["result"]["isError"], false);
}

#[test]
fn plugins_install_and_remove() {
    let dir = std::env::temp_dir().join(format!("ccad-cli-{}-store", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let plugin = dir.join("tools");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::write(plugin.join("plugin.toml"), "id = \"tools\"\nname = \"Tools\"\nversion = \"1.0\"\n").unwrap();
    std::fs::write(plugin.join("main.lua"), "").unwrap();
    let folder = dir.to_string_lossy().into_owned();
    let not_zip = dir.join("not.zip");
    std::fs::write(&not_zip, "this is not a zip archive at all, honestly").unwrap();
    let (code, _, err) = ccad(&["plugins", "install", &not_zip.to_string_lossy(), &folder], "");
    assert_eq!(code, 1);
    assert!(err.contains("not a zip"), "{err}");
    let (code, out, err) = ccad(&["plugins", "remove", "tools", &folder], "");
    assert_eq!(code, 0, "{err}");
    assert_eq!(out["removed"], "tools");
    assert!(!plugin.exists());
    assert_eq!(ccad(&["plugins", "remove", "tools", &folder], "").0, 1);
    assert_eq!(ccad(&["plugins", "frobnicate"], "").0, 2);
}
