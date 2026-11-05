//! Lua scripts and plugins over a session: commands, errors, the sandbox's
//! limits, plugin loading and one undo step per plugin command.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::{CommandKind, ErrorCode, Session};
use cc_plugins::{Limits, Permissions, Plugin, PluginHost, ScriptOptions, run_script};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn script(s: &mut Session, src: &str) -> Result<Value, cc_engine::EngineError> {
    run_script(s, src, "test.lua", json!({}), ScriptOptions::default()).result
}

fn walls(s: &mut Session) -> usize {
    s.execute("elements_list", json!({"category": "wall"})).unwrap().as_array().unwrap().len()
}

/// A plugin folder with these files, in a fresh temporary folder.
fn plugin_dir(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ccad-lua-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, text) in files {
        let p = dir.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    dir
}

const MANIFEST: &str = r#"
id = "room_tools"
name = "Room tools"
version = "0.1.0"
api = 1
"#;

const MAIN: &str = r#"
local geometry = require("lib.geometry")
local run = ccad.run -- kept at load time: still works when a command runs

ccad.command {
  name = "box_walls",
  description = "Four walls around a w × h rectangle",
  params = { properties = { w = { type = "number" }, h = { type = "number" } }, required = { "w", "h" } },
  run = function(args)
    local ids = {}
    for _, seg in ipairs(geometry.rectangle(args.w, args.h)) do
      ids[#ids + 1] = run("wall_create", { start = seg[1], ["end"] = seg[2] }).id
    end
    return { walls = ids }
  end,
}

ccad.command {
  name = "half_done",
  run = function()
    ccad.run("wall_create", { start = { 0, 0 }, ["end"] = { 3, 0 } })
    error("stopped halfway")
  end,
}

ccad.command {
  name = "count",
  kind = "query",
  run = function() return #ccad.run("elements_list", { category = "wall" }) end,
}

ccad.command {
  name = "sneaky_query",
  kind = "query",
  run = function() return ccad.run("wall_create", { start = { 0, 0 }, ["end"] = { 3, 0 } }) end,
}

ccad.command {
  name = "save",
  run = function() return ccad.run("project_save", { path = "x.ccad" }) end,
}

ccad.command {
  name = "undo_it",
  run = function() return ccad.run("undo", {}) end,
}

ccad.command {
  name = "spin",
  kind = "query",
  run = function() while true do end end,
}

print("loaded " .. ccad.plugin.id)
"#;

const GEOMETRY: &str = r#"
local M = {}
function M.rectangle(w, h)
  local p = { { 0, 0 }, { w, 0 }, { w, h }, { 0, h } }
  local segs = {}
  for i = 1, 4 do segs[i] = { p[i], p[i % 4 + 1] } end
  return segs
end
return M
"#;

fn room_tools(name: &str) -> PathBuf {
    plugin_dir(name, &[("plugin.toml", MANIFEST), ("main.lua", MAIN), ("lib/geometry.lua", GEOMETRY)])
}

fn session_with(dir: &Path, permissions: Permissions) -> Session {
    let mut plugin = Plugin::load(dir, Limits::default()).unwrap();
    plugin.grant(permissions);
    let mut host = PluginHost::new();
    host.add(plugin).unwrap();
    let mut s = Session::default();
    s.set_extension(Some(Box::new(host)));
    s
}

#[test]
fn scripts_run_commands() {
    let mut s = Session::default();
    let src = r#"
      local n = 0
      for i = 0, 2 do
        ccad.run("wall_create", { start = { i * 4, 0 }, ["end"] = { i * 4 + 3, 0 } })
        n = n + 1
      end
      local list = ccad.run("elements_list", { category = "wall" })
      return { created = n, listed = #list, empty = ccad.array() }
    "#;
    assert_eq!(script(&mut s, src).unwrap(), json!({"created": 3, "listed": 3, "empty": []}));
    assert_eq!(walls(&mut s), 3);
    // Each command is its own undo step.
    s.execute("undo", json!({})).unwrap();
    assert_eq!(walls(&mut s), 2);
}

#[test]
fn script_arguments_output_and_json() {
    let mut s = Session::default();
    let out = run_script(
        &mut s,
        "print('a', 1, nil) ; return { sum = ccad.args.x + select(1, ...).y, j = ccad.json.encode(ccad.json.decode('[1,2]')) }",
        "t.lua",
        json!({"x": 2, "y": 3}),
        ScriptOptions::default(),
    );
    assert_eq!(out.result.unwrap(), json!({"sum": 5, "j": "[1,2]"}));
    assert_eq!(out.output, vec!["a\t1\tnil".to_string()]);
}

#[test]
fn command_errors_keep_their_code() {
    let mut s = Session::default();
    let e = script(&mut s, "ccad.run('element_get', { id = 999999 })").unwrap_err();
    assert_eq!(e.code, ErrorCode::NotFound);
    let caught = script(
        &mut s,
        "local r, err = ccad.try('element_get', { id = 999999 }); assert(r == nil); return { code = err.code, has_message = #err.message > 0 }",
    )
    .unwrap();
    assert_eq!(caught, json!({"code": "not_found", "has_message": true}));
    assert_eq!(script(&mut s, "ccad.run('no_such_command')").unwrap_err().code, ErrorCode::UnknownCommand);
    assert_eq!(script(&mut s, "local x = ").unwrap_err().code, ErrorCode::Script);
    assert_eq!(script(&mut s, "error('boom')").unwrap_err().code, ErrorCode::Script);
    assert!(script(&mut s, "return print").is_err(), "functions are not results");
}

#[test]
fn the_sandbox_has_no_files_or_code_loading() {
    let mut s = Session::default();
    let found = script(
        &mut s,
        "return { io = io ~= nil, os = os ~= nil, load = load ~= nil, dofile = dofile ~= nil, \
         loadfile = loadfile ~= nil, require = require ~= nil, debug = debug ~= nil, package = package ~= nil, \
         dump = string.dump ~= nil, gc = collectgarbage ~= nil }",
    )
    .unwrap();
    for (name, present) in found.as_object().unwrap() {
        assert_eq!(present, &json!(false), "{name} should not be available");
    }
    // The standard pieces scripts need are there.
    let ok = script(&mut s, "return string.format('%.1f', math.sqrt(2)) .. table.concat({'a','b'}) .. utf8.char(228)");
    assert_eq!(ok.unwrap(), json!("1.4abä"));
}

#[test]
fn runaway_scripts_stop() {
    let mut s = Session::default();
    let small =
        ScriptOptions { limits: Limits { instructions: 2_000_000, memory: 16 * 1024 * 1024 }, ..Default::default() };
    let run = |s: &mut Session, src: &str| run_script(s, src, "t.lua", json!({}), small).result.unwrap_err();
    for src in [
        "while true do end",
        // Catching the budget's error does not help.
        "while true do pcall(function() while true do end end) end",
        "while true do xpcall(function() while true do end end, function(e) return e end) end",
        "local co = coroutine.create(function() while true do end end); while true do coroutine.resume(co) end",
        "local function f() return f() end; return f()",
    ] {
        let e = run(&mut s, src);
        assert_eq!(e.code, ErrorCode::Script, "{src}");
        assert!(e.message.contains("instruction limit"), "{src}: {}", e.message);
    }
    let e = run(&mut s, "local t = {}; for i = 1, 1e9 do t[i] = string.rep('x', 1000) .. i end");
    assert_eq!(e.code, ErrorCode::Script);
    let e = run(&mut s, "return string.rep('x', 1e9)");
    assert_eq!(e.code, ErrorCode::Script);
    let e = run(&mut s, "local function f(n) return f(n + 1) + 1 end; return f(1)");
    assert_eq!(e.code, ErrorCode::Script);
    // The session still works.
    assert!(script(&mut s, "return 1").is_ok());
}

#[test]
fn plugin_commands_join_the_catalog() {
    let dir = room_tools("catalog");
    let s = session_with(&dir, Permissions::NONE);
    let names: Vec<String> = s.commands().into_iter().map(|c| c.name).collect();
    assert!(names.contains(&"wall_create".to_string()));
    assert!(names.contains(&"room_tools__box_walls".to_string()));
    let info = s.describe("room_tools__box_walls").unwrap();
    assert_eq!(info.kind, CommandKind::Edit);
    assert_eq!(info.description, "Four walls around a w × h rectangle");
    assert_eq!(info.input_schema["required"], json!(["w", "h"]));
    assert_eq!(s.kind_of("room_tools__count"), Some(CommandKind::Query));
}

#[test]
fn a_plugin_command_is_one_undo_step() {
    let dir = room_tools("undo");
    let mut s = session_with(&dir, Permissions::NONE);
    let before = s.doc.undo_depth();
    let out = s.execute("room_tools__box_walls", json!({"w": 6, "h": 4})).unwrap();
    assert_eq!(out["walls"].as_array().unwrap().len(), 4);
    assert_eq!(walls(&mut s), 4);
    assert_eq!(s.doc.undo_depth(), before + 1);
    assert_eq!(s.execute("room_tools__count", json!({})).unwrap(), json!(4));
    s.execute("undo", json!({})).unwrap();
    assert_eq!(walls(&mut s), 0);
    assert_eq!(s.take_extension_messages(), vec!["[room_tools] loaded room_tools".to_string()]);
}

#[test]
fn a_failed_plugin_command_leaves_nothing() {
    let dir = room_tools("failed");
    let mut s = session_with(&dir, Permissions::NONE);
    let before = s.doc.undo_depth();
    let e = s.execute("room_tools__half_done", json!({})).unwrap_err();
    assert_eq!(e.code, ErrorCode::Script);
    assert!(e.message.contains("stopped halfway"), "{}", e.message);
    assert_eq!((walls(&mut s), s.doc.undo_depth()), (0, before));
    let e = s.execute("room_tools__box_walls", json!({"w": 6})).unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidArguments);
    let e = s.execute("room_tools__spin", json!({})).unwrap_err();
    assert!(e.message.contains("instruction limit"));
    // The plugin works again after hitting its limit.
    assert!(s.execute("room_tools__box_walls", json!({"w": 2, "h": 2})).is_ok());
}

#[test]
fn plugins_only_do_what_they_may() {
    let dir = room_tools("access");
    let mut s = session_with(&dir, Permissions::NONE);
    let e = s.execute("room_tools__sneaky_query", json!({})).unwrap_err();
    assert_eq!(e.code, ErrorCode::Rejected);
    assert_eq!(walls(&mut s), 0);
    let e = s.execute("room_tools__save", json!({})).unwrap_err();
    assert_eq!(e.code, ErrorCode::Rejected);
    assert!(e.message.contains("files"), "{}", e.message);
    s.execute("wall_create", json!({"start": [0, 0], "end": [1, 0]})).unwrap();
    let e = s.execute("room_tools__undo_it", json!({})).unwrap_err();
    assert_eq!(e.code, ErrorCode::Rejected);
    assert_eq!(walls(&mut s), 1);
    assert_eq!(s.execute("room_tools__nothing", json!({})).unwrap_err().code, ErrorCode::UnknownCommand);
}

#[test]
fn granted_files_permission_needs_the_manifest() {
    let dir = room_tools("grant");
    let mut plugin = Plugin::load(&dir, Limits::default()).unwrap();
    plugin.grant(Permissions { files: true });
    assert_eq!(plugin.granted(), Permissions::NONE, "the manifest does not ask for files");
}

#[test]
fn only_the_top_command_is_recorded() {
    let dir = room_tools("record");
    let mut s = session_with(&dir, Permissions::NONE);
    s.start_recording();
    s.execute("room_tools__box_walls", json!({"w": 1, "h": 1})).unwrap();
    let steps = s.stop_recording();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].command, "room_tools__box_walls");
}

#[test]
fn scripts_call_plugin_commands_but_plugins_do_not_nest() {
    let other = plugin_dir(
        "nest",
        &[
            ("plugin.toml", "id = \"nester\"\nname = \"N\"\nversion = \"1\"\n"),
            ("main.lua", "ccad.command { name = \"call\", run = function() return ccad.run('room_tools__count') end }"),
        ],
    );
    let dir = room_tools("nest-host");
    let mut host = PluginHost::new();
    host.add(Plugin::load(&dir, Limits::default()).unwrap()).unwrap();
    host.add(Plugin::load(&other, Limits::default()).unwrap()).unwrap();
    let mut s = Session::default();
    s.set_extension(Some(Box::new(host)));
    assert_eq!(script(&mut s, "return ccad.run('room_tools__count')").unwrap(), json!(0));
    let e = s.execute("nester__call", json!({})).unwrap_err();
    assert_eq!(e.code, ErrorCode::UnknownCommand);
    assert!(e.message.contains("inside another plugin command"), "{}", e.message);
}

/// A plugin's folder name, its files, and a part of the expected error.
type Case<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a str);

#[test]
fn bad_plugins_are_refused() {
    let cases: &[Case] = &[
        ("no-manifest", &[("main.lua", "")], "plugin.toml"),
        ("bad-id", &[("plugin.toml", "id = \"Bad-Id\"\nname = \"x\"\nversion = \"1\"\n"), ("main.lua", "")], "id"),
        ("double", &[("plugin.toml", "id = \"a__b\"\nname = \"x\"\nversion = \"1\"\n"), ("main.lua", "")], "id"),
        ("api", &[("plugin.toml", "id = \"a\"\nname = \"x\"\nversion = \"1\"\napi = 99\n"), ("main.lua", "")], "api"),
        ("main", &[("plugin.toml", "id = \"a\"\nname = \"x\"\nversion = \"1\"\nmain = \"../x.lua\"\n")], "main"),
        (
            "perm",
            &[
                ("plugin.toml", "id = \"a\"\nname = \"x\"\nversion = \"1\"\npermissions = [\"net\"]\n"),
                ("main.lua", ""),
            ],
            "permission",
        ),
        (
            "syntax",
            &[("plugin.toml", "id = \"a\"\nname = \"x\"\nversion = \"1\"\n"), ("main.lua", "local = 1")],
            "main.lua",
        ),
        (
            "cmd-name",
            &[
                ("plugin.toml", "id = \"a\"\nname = \"x\"\nversion = \"1\"\n"),
                ("main.lua", "ccad.command { name = 'Bad', run = print }"),
            ],
            "name",
        ),
        (
            "cmd-twice",
            &[
                ("plugin.toml", "id = \"a\"\nname = \"x\"\nversion = \"1\"\n"),
                ("main.lua", "for i = 1, 2 do ccad.command { name = 'c', run = print } end"),
            ],
            "twice",
        ),
        (
            "cmd-file",
            &[
                ("plugin.toml", "id = \"a\"\nname = \"x\"\nversion = \"1\"\n"),
                ("main.lua", "ccad.command { name = 'c', kind = 'file', run = print }"),
            ],
            "files",
        ),
        (
            "no-run",
            &[
                ("plugin.toml", "id = \"a\"\nname = \"x\"\nversion = \"1\"\n"),
                ("main.lua", "ccad.command { name = 'c' }"),
            ],
            "run",
        ),
        (
            "run-at-load",
            &[("plugin.toml", "id = \"a\"\nname = \"x\"\nversion = \"1\"\n"), ("main.lua", "ccad.run('levels_list')")],
            "only available",
        ),
        (
            "require-up",
            &[("plugin.toml", "id = \"a\"\nname = \"x\"\nversion = \"1\"\n"), ("main.lua", "require('..secret')")],
            "module name",
        ),
        (
            "require-self",
            &[
                ("plugin.toml", "id = \"a\"\nname = \"x\"\nversion = \"1\"\n"),
                ("main.lua", "require('m')"),
                ("m.lua", "require('m')"),
            ],
            "requires itself",
        ),
        (
            "loops",
            &[("plugin.toml", "id = \"a\"\nname = \"x\"\nversion = \"1\"\n"), ("main.lua", "while true do end")],
            "instruction limit",
        ),
    ];
    for (name, files, expected) in cases {
        let dir = plugin_dir(name, files);
        let e = Plugin::load(&dir, Limits { instructions: 1_000_000, memory: 16 * 1024 * 1024 }).unwrap_err();
        assert!(e.message.contains(expected), "{name}: {}", e.message);
    }
}

#[test]
fn a_folder_of_plugins_loads_each_once() {
    let root = std::env::temp_dir().join(format!("ccad-lua-{}-folder", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for sub in ["one", "two"] {
        let d = root.join(sub);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("plugin.toml"), "id = \"same\"\nname = \"x\"\nversion = \"1\"\n").unwrap();
        std::fs::write(d.join("main.lua"), "").unwrap();
    }
    std::fs::create_dir_all(root.join("not_a_plugin")).unwrap();
    let mut host = PluginHost::new();
    let errors = host.load_folder(&root, Limits::default(), |m| Some(m.requested()));
    assert_eq!(host.plugins().len(), 1);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("already loaded"));
}

fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

#[test]
fn the_example_script_builds_a_house() {
    let src = std::fs::read_to_string(examples().join("small_house.lua")).unwrap();
    let mut s = Session::default();
    let out = run_script(&mut s, &src, "small_house.lua", json!({"width": 10}), ScriptOptions::default());
    assert_eq!(out.result.unwrap(), json!({"rooms": 2}));
    assert_eq!(out.output, vec!["5 walls, 2 rooms".to_string()]);
    assert_eq!(walls(&mut s), 5);
}

#[test]
fn the_example_plugin_works() {
    let mut host = PluginHost::new();
    let errors = host.load_folder(&examples().join("plugins"), Limits::default(), |m| Some(m.requested()));
    assert!(errors.is_empty(), "{errors:?}");
    let mut s = Session::default();
    s.set_extension(Some(Box::new(host)));
    let room = s
        .execute(
            "quick_rooms__rectangle_room",
            json!({"x": 1, "width": 4, "depth": 3, "name": "Office", "door": "south"}),
        )
        .unwrap();
    assert!(room["door"].is_u64() && room["room"].is_u64() && room["floor"].is_u64());
    assert_eq!(walls(&mut s), 4);
    let totals = s.execute("quick_rooms__wall_totals", json!({})).unwrap();
    let (_, t) = totals.as_object().unwrap().iter().next().unwrap();
    assert_eq!(t["count"], 4);
    assert!((t["length"].as_f64().unwrap() - 14.0).abs() < 1e-9);
    let e = s.execute("quick_rooms__rectangle_room", json!({"width": -1, "depth": 3})).unwrap_err();
    assert_eq!(e.message, "width and depth must be positive");
    // One undo step removes the room with its walls, floor and door.
    s.execute("undo", json!({})).unwrap();
    assert_eq!(walls(&mut s), 0);
}

#[test]
fn plugins_left_out_run_no_code() {
    let dir = plugin_dir(
        "left-out",
        &[("plugin.toml", "id = \"off\"\nname = \"Off\"\nversion = \"1\"\n"), ("main.lua", "error('ran')")],
    );
    let mut host = PluginHost::new();
    assert!(host.load_folder(&dir, Limits::default(), |_| None).is_empty());
    assert!(host.plugins().is_empty());
    let errors = host.load_folder(&dir, Limits::default(), |m| Some(m.requested()));
    assert!(errors[0].message.contains("ran"), "{:?}", errors);
}
