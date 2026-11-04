//! Installing plugins: the community index (`index.json` of
//! [constructcad-plugins](https://github.com/francisreboucas/constructcad-plugins)),
//! the version to offer, and checked installs from a downloaded archive.
//!
//! Nothing here reaches the network: callers download the index and the
//! archives (the app does it in a background thread) and pass the bytes.
//! Archives are hostile input: the checksum must match the index, the zip is
//! read in memory with bounded sizes, entries with unsafe paths or links are
//! refused, and the plugin must load (in the sandbox) before it replaces an
//! installed one.

use crate::api::API_VERSION;
use crate::plugin::{MANIFEST, Manifest, Plugin};
use crate::sandbox::Limits;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Where the app reads the index.
pub const INDEX_URL: &str = "https://raw.githubusercontent.com/francisreboucas/constructcad-plugins/main/index.json";
/// The index format this build reads.
pub const INDEX_FORMAT: u32 = 1;
/// Largest index, archive, unpacked plugin and number of files accepted.
pub const MAX_INDEX: usize = 4 * 1024 * 1024;
pub const MAX_ARCHIVE: usize = 20 * 1024 * 1024;
const MAX_UNPACKED: u64 = 50 * 1024 * 1024;
const MAX_FILES: usize = 2000;
const MAX_PLUGINS: usize = 10_000;

/// The plugin index.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Index {
    pub format: u32,
    #[serde(default)]
    pub plugins: Vec<IndexPlugin>,
}

/// A plugin in the index.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct IndexPlugin {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub versions: Vec<IndexVersion>,
}

/// One downloadable version of a plugin.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct IndexVersion {
    pub version: String,
    pub api: u32,
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    /// The folder of `plugin.toml` inside the archive.
    #[serde(default)]
    pub path: Option<String>,
    /// The oldest ConstructCAD version it works with.
    #[serde(default)]
    pub min_app: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// A version `major.minor.patch`, ignoring any pre-release or build suffix.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.trim().split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let v = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(v)
}

impl Index {
    /// Read an index; plugins with bad ids or no usable version are left out.
    pub fn parse(json: &[u8]) -> Result<Index, String> {
        if json.len() > MAX_INDEX {
            return Err(format!("the index is larger than {MAX_INDEX} bytes"));
        }
        let mut index: Index = serde_json::from_slice(json).map_err(|e| format!("the index cannot be read: {e}"))?;
        if index.format != INDEX_FORMAT {
            return Err(format!("index format {} (this version reads {INDEX_FORMAT})", index.format));
        }
        index.plugins.truncate(MAX_PLUGINS);
        index
            .plugins
            .retain(|p| crate::plugin::valid_id(&p.id) && p.versions.iter().any(|v| v.url.starts_with("https://")));
        index.plugins.sort_by_key(|p| p.name.to_lowercase());
        Ok(index)
    }
}

impl IndexPlugin {
    /// The newest version this build can run: plugin API at most
    /// [`API_VERSION`], `min_app` at most `app`, an https archive.
    pub fn best(&self, app: (u64, u64, u64)) -> Option<&IndexVersion> {
        self.versions
            .iter()
            .filter(|v| v.api >= 1 && v.api <= API_VERSION && v.url.starts_with("https://"))
            .filter(|v| v.min_app.as_deref().and_then(parse_version).is_none_or(|min| min <= app))
            .filter_map(|v| parse_version(&v.version).map(|n| (n, v)))
            .max_by_key(|(n, _)| *n)
            .map(|(_, v)| v)
    }
}

/// A plugin found in a plugins folder (loaded or not).
#[derive(Debug, Clone, PartialEq)]
pub struct Installed {
    pub dir: PathBuf,
    /// Its manifest, or why it cannot be read.
    pub manifest: Result<Manifest, String>,
}

/// The plugins of `folder`: each subfolder with a manifest, in name order.
pub fn installed(folder: &Path) -> Vec<Installed> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join(MANIFEST).is_file())
        .filter(|p| !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
        .take(1000)
        .collect();
    dirs.sort();
    dirs.into_iter()
        .map(|dir| {
            let manifest = std::fs::read_to_string(dir.join(MANIFEST))
                .map_err(|e| e.to_string())
                .and_then(|text| Manifest::parse(&text));
            Installed { dir, manifest }
        })
        .collect()
}

/// The files of a plugin archive, checked: `(path inside the plugin, bytes)`.
fn unpack(archive: &[u8], sub: Option<&str>) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    let zip = cc_skp::zip::ZipArchive::new(archive).map_err(|e| format!("not a zip archive ({e:?})"))?;
    if zip.len() > MAX_FILES {
        return Err(format!("more than {MAX_FILES} files"));
    }
    let mut entries = Vec::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let index = u32::try_from(i).map_err(|_| "too many files".to_string())?;
        let stat = zip.stat(index).map_err(|e| format!("bad entry ({e:?})"))?;
        let name = String::from_utf8(stat.filename.clone()).map_err(|_| "a file name that is not UTF-8".to_string())?;
        // Unix file type in the high bits: 0o12 is a symbolic link.
        if (stat.external_attr >> 16) & 0o170000 == 0o120000 {
            return Err(format!("{name}: links are not allowed"));
        }
        if stat.is_directory || name.ends_with('/') {
            continue;
        }
        let parts: Vec<&str> = name.split('/').collect();
        let safe = !name.starts_with('/')
            && !name.contains('\\')
            && !name.contains(':')
            && parts.iter().all(|p| !p.is_empty() && *p != "." && *p != "..");
        if !safe {
            return Err(format!("{name}: unsafe path"));
        }
        total = total.saturating_add(stat.uncomp_size);
        if total > MAX_UNPACKED {
            return Err(format!("more than {MAX_UNPACKED} bytes unpacked"));
        }
        entries.push((name, index));
    }
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    let root = match sub.map(|s| s.trim_matches('/')) {
        Some(s) if !s.is_empty() => format!("{s}/"),
        _ if names.contains(&MANIFEST) => String::new(),
        _ => {
            let mut tops = names.iter().filter_map(|n| n.split_once('/').map(|(t, _)| t));
            match tops.next() {
                Some(first) if names.iter().all(|n| n.starts_with(&format!("{first}/"))) => format!("{first}/"),
                _ => String::new(),
            }
        }
    };
    let mut files = Vec::new();
    for (name, index) in entries {
        let Some(rel) = name.strip_prefix(&root) else { continue };
        let bytes = zip.extract(index).map_err(|e| format!("{name}: cannot be unpacked ({e:?})"))?;
        files.push((PathBuf::from_iter(rel.split('/')), bytes));
    }
    if !files.iter().any(|(p, _)| p == Path::new(MANIFEST)) {
        return Err(format!("no {MANIFEST} in '{}' of the archive", if root.is_empty() { "/" } else { &root }));
    }
    Ok(files)
}

/// Install `version` of `plugin` from its downloaded `archive` into
/// `folder` (as `folder/<id>`, replacing an installed one). Returns the
/// plugin's folder.
pub fn install(archive: &[u8], plugin: &IndexPlugin, version: &IndexVersion, folder: &Path) -> Result<PathBuf, String> {
    if archive.len() > MAX_ARCHIVE {
        return Err(format!("the archive is larger than {MAX_ARCHIVE} bytes"));
    }
    let digest: String = Sha256::digest(archive).iter().map(|b| format!("{b:02x}")).collect();
    if !digest.eq_ignore_ascii_case(version.sha256.trim()) {
        return Err("the download does not match the index's checksum".into());
    }
    install_unchecked(archive, version.path.as_deref(), folder, |m| {
        if m.id != plugin.id {
            return Err(format!("the archive holds the plugin \"{}\", not \"{}\"", m.id, plugin.id));
        }
        if m.version != version.version {
            return Err(format!("the archive holds version {}, not {}", m.version, version.version));
        }
        let mut asked = m.permissions.clone();
        let mut listed = version.permissions.clone();
        asked.sort();
        listed.sort();
        if asked != listed {
            return Err("the archive asks for other permissions than the index lists".into());
        }
        Ok(())
    })
}

/// Install a plugin archive that is not in the index (a local `.zip`):
/// the same checks, except the checksum and the index's own fields.
pub fn install_archive(archive: &[u8], folder: &Path) -> Result<PathBuf, String> {
    if archive.len() > MAX_ARCHIVE {
        return Err(format!("the archive is larger than {MAX_ARCHIVE} bytes"));
    }
    install_unchecked(archive, None, folder, |_| Ok(()))
}

fn install_unchecked(
    archive: &[u8],
    sub: Option<&str>,
    folder: &Path,
    check: impl FnOnce(&Manifest) -> Result<(), String>,
) -> Result<PathBuf, String> {
    let files = unpack(archive, sub)?;
    let manifest_text = files
        .iter()
        .find(|(p, _)| p == Path::new(MANIFEST))
        .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default();
    let manifest = Manifest::parse(&manifest_text)?;
    check(&manifest)?;
    std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let staging = folder.join(format!(".{}.installing", manifest.id));
    let _ = std::fs::remove_dir_all(&staging);
    let written = (|| -> std::io::Result<()> {
        for (rel, bytes) in &files {
            let path = staging.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, bytes)?;
        }
        Ok(())
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!("{}: {e}", staging.display()));
    }
    // It must load before it replaces anything.
    if let Err(e) = Plugin::load(&staging, Limits::default()) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e.message);
    }
    let target = folder.join(&manifest.id);
    let old = folder.join(format!(".{}.old", manifest.id));
    let _ = std::fs::remove_dir_all(&old);
    if target.exists() {
        std::fs::rename(&target, &old).map_err(|e| format!("{}: {e}", target.display()))?;
    }
    if let Err(e) = std::fs::rename(&staging, &target) {
        // Put the previous version back.
        let _ = std::fs::rename(&old, &target);
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!("{}: {e}", target.display()));
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(target)
}

/// Remove the installed plugin `id` from `folder` (only a folder whose
/// manifest has that id).
pub fn uninstall(folder: &Path, id: &str) -> Result<(), String> {
    let dir = folder.join(id);
    let manifest = std::fs::read_to_string(dir.join(MANIFEST)).map_err(|e| format!("{}: {e}", dir.display()))?;
    let found = Manifest::parse(&manifest)?;
    if found.id != id {
        return Err(format!("{} holds the plugin \"{}\"", dir.display(), found.id));
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))
}
