//! Installing plugins from the index: version choice, checked archives,
//! install, update and removal.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_plugins::store::{self, Index, IndexPlugin, IndexVersion};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// A zip of stored (uncompressed) files; `links` are marked as symbolic links.
fn zip(files: &[(&str, &str)], links: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, text) in files {
        let data = text.as_bytes();
        let crc = cc_skp::zip::crc32(data);
        let offset = out.len() as u32;
        let mut local = vec![0x50, 0x4b, 0x03, 0x04, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        local.extend(crc.to_le_bytes());
        local.extend((data.len() as u32).to_le_bytes());
        local.extend((data.len() as u32).to_le_bytes());
        local.extend((name.len() as u16).to_le_bytes());
        local.extend(0u16.to_le_bytes());
        local.extend(name.as_bytes());
        out.extend(local);
        out.extend(data);
        let attr: u32 = if links.contains(name) { 0o120777 << 16 } else { 0o100644 << 16 };
        let mut c = vec![0x50, 0x4b, 0x01, 0x02, 20, 3, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        c.extend(crc.to_le_bytes());
        c.extend((data.len() as u32).to_le_bytes());
        c.extend((data.len() as u32).to_le_bytes());
        c.extend((name.len() as u16).to_le_bytes());
        c.extend([0, 0, 0, 0, 0, 0, 0, 0]);
        c.extend(attr.to_le_bytes());
        c.extend(offset.to_le_bytes());
        c.extend(name.as_bytes());
        central.extend(c);
    }
    let cd_offset = out.len() as u32;
    out.extend(&central);
    let mut end = vec![0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0];
    end.extend((files.len() as u16).to_le_bytes());
    end.extend((files.len() as u16).to_le_bytes());
    end.extend((central.len() as u32).to_le_bytes());
    end.extend(cd_offset.to_le_bytes());
    end.extend([0, 0]);
    out.extend(end);
    out
}

fn sha(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

fn manifest(version: &str, extra: &str) -> String {
    format!("id = \"rooms\"\nname = \"Rooms\"\nversion = \"{version}\"\n{extra}")
}

const MAIN: &str = "ccad.command { name = 'hello', kind = 'query', run = function() return 'hi' end }";

fn entry(versions: Vec<IndexVersion>) -> IndexPlugin {
    IndexPlugin {
        id: "rooms".into(),
        name: "Rooms".into(),
        description: String::new(),
        authors: vec![],
        license: "MIT".into(),
        homepage: None,
        repository: None,
        tags: vec![],
        versions,
    }
}

fn version(v: &str, archive: &[u8]) -> IndexVersion {
    IndexVersion {
        version: v.into(),
        api: 1,
        url: format!("https://example.org/rooms-{v}.zip"),
        sha256: sha(archive),
        permissions: vec![],
        path: None,
        min_app: None,
        notes: None,
    }
}

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ccad-store-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn the_index_is_read_and_filtered() {
    let json = br#"{"format": 1, "plugins": [
      {"id": "b_tool", "name": "B tool", "versions": [{"version": "1.0.0", "api": 1, "url": "https://x/b.zip", "sha256": "00"}]},
      {"id": "Bad Id", "name": "Bad", "versions": [{"version": "1.0.0", "api": 1, "url": "https://x/c.zip", "sha256": "00"}]},
      {"id": "plain_http", "name": "Plain", "versions": [{"version": "1.0.0", "api": 1, "url": "http://x/c.zip", "sha256": "00"}]},
      {"id": "a_tool", "name": "A tool", "extra": true, "versions": [{"version": "1.0.0", "api": 1, "url": "https://x/a.zip", "sha256": "00"}]}
    ]}"#;
    let index = Index::parse(json).unwrap();
    let ids: Vec<&str> = index.plugins.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["a_tool", "b_tool"]);
    assert!(Index::parse(br#"{"format": 2, "plugins": []}"#).is_err());
    assert!(Index::parse(b"not json").is_err());
}

#[test]
fn the_newest_runnable_version_is_offered() {
    let a = zip(&[], &[]);
    let mut v1 = version("1.0.0", &a);
    let mut v2 = version("1.10.0", &a);
    let mut v3 = version("2.0.0", &a);
    v3.api = 99;
    v2.min_app = Some("0.12.0".into());
    v1.min_app = Some("0.10.0".into());
    let p = entry(vec![v1, v2, v3]);
    assert_eq!(p.best((0, 12, 0)).unwrap().version, "1.10.0");
    assert_eq!(p.best((0, 11, 3)).unwrap().version, "1.0.0");
    assert!(p.best((0, 9, 0)).is_none());
}

#[test]
fn install_update_and_remove() {
    let dir = folder("cycle");
    let a1 = zip(&[("rooms-1/plugin.toml", &manifest("1.0.0", "")), ("rooms-1/main.lua", MAIN)], &[]);
    let v1 = version("1.0.0", &a1);
    let p = entry(vec![v1.clone()]);
    let installed = store::install(&a1, &p, &v1, &dir).unwrap();
    assert_eq!(installed, dir.join("rooms"));
    let found = store::installed(&dir);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].manifest.as_ref().unwrap().version, "1.0.0");
    // An update replaces it, leaving nothing else behind.
    let a2 = zip(&[("plugin.toml", &manifest("1.1.0", "")), ("main.lua", MAIN), ("lib/x.lua", "return 1")], &[]);
    let v2 = version("1.1.0", &a2);
    store::install(&a2, &p, &v2, &dir).unwrap();
    assert_eq!(store::installed(&dir)[0].manifest.as_ref().unwrap().version, "1.1.0");
    assert!(dir.join("rooms/lib/x.lua").is_file());
    let names: Vec<String> =
        std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["rooms"]);
    store::uninstall(&dir, "rooms").unwrap();
    assert!(store::installed(&dir).is_empty());
    assert!(store::uninstall(&dir, "rooms").is_err());
}

#[test]
fn bad_archives_are_refused_and_keep_the_installed_version() {
    let dir = folder("bad");
    let good = zip(&[("plugin.toml", &manifest("1.0.0", "")), ("main.lua", MAIN)], &[]);
    let v = version("1.0.0", &good);
    store::install(&good, &entry(vec![v.clone()]), &v, &dir).unwrap();
    let cases: Vec<(Vec<u8>, &str, &str)> = vec![
        (zip(&[("main.lua", MAIN)], &[]), "1.0.0", "no plugin.toml"),
        (
            zip(&[("plugin.toml", &manifest("1.0.0", "")), ("main.lua", MAIN), ("../evil.lua", "")], &[]),
            "1.0.0",
            "unsafe path",
        ),
        (
            zip(&[("plugin.toml", &manifest("1.0.0", "")), ("main.lua", MAIN), ("link", "/etc")], &["link"]),
            "1.0.0",
            "links",
        ),
        (zip(&[("plugin.toml", &manifest("2.0.0", "")), ("main.lua", MAIN)], &[]), "1.0.0", "version 2.0.0"),
        (
            zip(&[("plugin.toml", &manifest("1.0.0", "permissions = [\"files\"]\n")), ("main.lua", MAIN)], &[]),
            "1.0.0",
            "permissions",
        ),
        (zip(&[("plugin.toml", &manifest("1.0.0", "")), ("main.lua", "syntax error here")], &[]), "1.0.0", "main.lua"),
        (
            zip(&[("plugin.toml", &manifest("1.0.0", "").replace("rooms", "other")), ("main.lua", MAIN)], &[]),
            "1.0.0",
            "\"other\"",
        ),
        (b"not a zip at all, but long enough to look".to_vec(), "1.0.0", "not a zip"),
    ];
    for (archive, ver, expected) in cases {
        let v = version(ver, &archive);
        let e = store::install(&archive, &entry(vec![v.clone()]), &v, &dir).unwrap_err();
        assert!(e.contains(expected), "{expected}: {e}");
        assert_eq!(store::installed(&dir)[0].manifest.as_ref().unwrap().version, "1.0.0");
    }
    // The checksum must match.
    let mut v = version("1.0.0", &good);
    v.sha256 = "0".repeat(64);
    let e = store::install(&good, &entry(vec![v.clone()]), &v, &dir).unwrap_err();
    assert!(e.contains("checksum"), "{e}");
}

#[test]
fn a_local_archive_installs_without_the_index() {
    let dir = folder("local");
    let a = zip(&[("pkg/plugin.toml", &manifest("0.2.0", "")), ("pkg/main.lua", MAIN)], &[]);
    let path = store::install_archive(&a, &dir).unwrap();
    assert_eq!(path, dir.join("rooms"));
}
