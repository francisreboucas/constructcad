//! Commands added at run time: listed with the catalog, one undo step each,
//! nothing left behind when they fail.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_engine::{CommandInfo, CommandKind, ErrorCode, Extension, Session};
use serde_json::{Value, json};

/// `test__walls` creates `n` walls, then fails if asked to.
struct Walls;

impl Extension for Walls {
    fn commands(&self) -> Vec<CommandInfo> {
        vec![CommandInfo {
            name: "test__walls".into(),
            kind: CommandKind::Edit,
            group: "Test".into(),
            description: "Walls".into(),
            input_schema: json!({"type": "object", "properties": {}}),
        }]
    }

    fn run(&mut self, session: &mut Session, name: &str, args: Value) -> Option<cc_engine::Result<Value>> {
        if name != "test__walls" {
            return None;
        }
        let n = args["n"].as_u64().unwrap_or(1);
        for i in 0..n {
            let x = i as f64 * 2.0;
            if let Err(e) = session.execute("wall_create", json!({"start": [x, 0], "end": [x + 1.0, 0]})) {
                return Some(Err(e));
            }
        }
        // An extension's command cannot run another one: the extension is out.
        if let Err(e) = session.execute("test__walls", json!({})) {
            assert_eq!(e.code, ErrorCode::UnknownCommand);
        }
        if args["fail"] == json!(true) {
            return Some(Err(cc_engine::EngineError::rejected("asked to fail")));
        }
        Some(Ok(json!({"created": n})))
    }
}

fn walls(s: &mut Session) -> usize {
    s.execute("elements_list", json!({"category": "wall"})).unwrap().as_array().unwrap().len()
}

#[test]
fn extension_commands_are_one_undo_step() {
    let mut s = Session::default();
    assert_eq!(s.execute("test__walls", json!({})).unwrap_err().code, ErrorCode::UnknownCommand);
    s.set_extension(Some(Box::new(Walls)));
    assert!(s.commands().iter().any(|c| c.name == "test__walls"));
    assert_eq!(s.kind_of("test__walls"), Some(CommandKind::Edit));
    assert_eq!(s.kind_of("wall_create"), Some(CommandKind::Edit));
    let depth = s.doc.undo_depth();
    assert_eq!(s.execute("test__walls", json!({"n": 3})).unwrap(), json!({"created": 3}));
    assert_eq!((walls(&mut s), s.doc.undo_depth()), (3, depth + 1));
    s.execute("undo", json!({})).unwrap();
    assert_eq!(walls(&mut s), 0);
    s.execute("redo", json!({})).unwrap();
    assert_eq!(walls(&mut s), 3);
}

#[test]
fn failed_extension_commands_leave_nothing() {
    let mut s = Session::default();
    s.set_extension(Some(Box::new(Walls)));
    let depth = s.doc.undo_depth();
    let e = s.execute("test__walls", json!({"n": 2, "fail": true})).unwrap_err();
    assert_eq!(e.code, ErrorCode::Rejected);
    assert_eq!((walls(&mut s), s.doc.undo_depth()), (0, depth));
    // The extension is back for the next command, and survives a new project.
    s.execute("project_new", json!({})).unwrap();
    assert!(s.execute("test__walls", json!({})).is_ok());
}

/// Regression: opening a project replaced the whole session, losing the
/// extension and the count of running commands (an overflow panic).
#[test]
fn opening_a_project_keeps_the_extension() {
    let path = std::env::temp_dir().join(format!("ccad-ext-{}.ccad", std::process::id()));
    let mut s = Session::default();
    s.execute("project_save", json!({"path": path})).unwrap();
    s.set_extension(Some(Box::new(Walls)));
    s.execute("project_open", json!({"path": path})).unwrap();
    assert!(s.execute("test__walls", json!({})).is_ok());
    assert_eq!(s.doc.path.as_deref(), Some(path.as_path()));
    assert!(!s.is_nested());
}
