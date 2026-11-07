//! `.ccomp` component files: a component definition with all its types and
//! the materials it uses, to share components between projects.

use crate::component::ComponentDef;
use crate::{ElementType, ElementTypeId, Material, Project, TypeData, TypeInfo, expr};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Current `.ccomp` format version.
pub const COMPONENT_FORMAT_VERSION: u32 = 1;

/// Limits that keep a hostile file from exhausting memory or time.
const MAX_PARAMS: usize = 256;
const MAX_ITEMS: usize = 1024;
const MAX_POINTS: usize = 1024;
const MAX_TYPES: usize = 256;
const MAX_MESH_VERTICES: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentFile {
    pub format_version: u32,
    #[serde(default)]
    pub generator: String,
    pub component: String,
    /// The definition, with the first type's parameter values.
    pub def: ComponentDef,
    pub types: Vec<ComponentFileType>,
    #[serde(default)]
    pub materials: Vec<Material>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentFileType {
    pub name: String,
    /// Parameter values of this type.
    #[serde(default)]
    pub values: BTreeMap<String, f64>,
    #[serde(default)]
    pub info: TypeInfo,
}

#[derive(Debug, thiserror::Error)]
pub enum ComponentFileError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid file: {0}")]
    Parse(#[from] ron::error::SpannedError),
    #[error("serialization error: {0}")]
    Serialize(#[from] ron::Error),
    #[error("unsupported format version {0} (maximum {COMPONENT_FORMAT_VERSION})")]
    Version(u32),
    #[error("invalid component: {0}")]
    Invalid(String),
    #[error("the project has no component named {0}")]
    NotFound(String),
}

/// Collect the component named `component` from `project`.
pub fn extract(project: &Project, component: &str) -> Result<ComponentFile, ComponentFileError> {
    let types: Vec<(&ElementType, &ComponentDef)> = project
        .component_types()
        .filter_map(|t| match &t.data {
            TypeData::Component { component: c, def } if c == component => Some((t, def)),
            _ => None,
        })
        .collect();
    let Some((_, first)) = types.first() else { return Err(ComponentFileError::NotFound(component.to_string())) };
    let used: Vec<&str> = types
        .iter()
        .flat_map(|(_, def)| def.items.iter().map(|i| i.material.as_str()))
        .filter(|m| !m.is_empty())
        .collect();
    Ok(ComponentFile {
        format_version: COMPONENT_FORMAT_VERSION,
        generator: format!("ConstructCAD {}", env!("CARGO_PKG_VERSION")),
        component: component.to_string(),
        def: (*first).clone(),
        types: types
            .iter()
            .map(|(t, def)| ComponentFileType {
                name: t.name.clone(),
                values: def.params.iter().map(|p| (p.name.clone(), p.value)).collect(),
                info: t.info.clone(),
            })
            .collect(),
        materials: project.materials.iter().filter(|m| used.contains(&m.name.as_str())).cloned().collect(),
    })
}

pub fn to_ron(file: &ComponentFile) -> Result<String, ComponentFileError> {
    let config = ron::ser::PrettyConfig::new().depth_limit(8).indentor("  ".to_string());
    Ok(ron::ser::to_string_pretty(file, config)?)
}

/// Parse and validate a component file.
pub fn from_ron(text: &str) -> Result<ComponentFile, ComponentFileError> {
    let file: ComponentFile = ron::from_str(text)?;
    if file.format_version > COMPONENT_FORMAT_VERSION {
        return Err(ComponentFileError::Version(file.format_version));
    }
    validate(&file)?;
    Ok(file)
}

fn validate(file: &ComponentFile) -> Result<(), ComponentFileError> {
    let bad = |m: &str| Err(ComponentFileError::Invalid(m.to_string()));
    let def = &file.def;
    if file.component.trim().is_empty() {
        return bad("empty component name");
    }
    if def.params.len() > MAX_PARAMS || def.items.len() > MAX_ITEMS || file.types.len() > MAX_TYPES {
        return bad("too many parameters, pieces or types");
    }
    // A title block holds only 2D sheet pieces (possibly none), within their limits.
    let title_block = def.kind == crate::ComponentKind::TitleBlock;
    if (def.items.is_empty() && !title_block) || file.types.is_empty() {
        return bad("no pieces or no types");
    }
    if title_block && crate::title_block::repair_def(&mut def.clone()) > 0 {
        return bad("invalid or too many sheet pieces");
    }
    if !title_block && !def.sheet.is_empty() {
        return bad("sheet pieces in a component that is not a title block");
    }
    for (i, p) in def.params.iter().enumerate() {
        if !expr::is_valid_name(&p.name) || def.params[..i].iter().any(|q| q.name == p.name) {
            return bad(&format!("invalid or repeated parameter name '{}'", p.name));
        }
        if !p.value.is_finite() {
            return bad(&format!("parameter '{}' is not a finite number", p.name));
        }
    }
    for item in &def.items {
        if item.shape.point_count() > MAX_POINTS {
            return bad("too many profile points");
        }
        if let crate::ComponentShape::Mesh { vertices, faces } = &item.shape
            && (vertices.len() > MAX_MESH_VERTICES || faces.iter().map(Vec::len).sum::<usize>() > 4 * MAX_MESH_VERTICES)
        {
            return bad("mesh too large");
        }
    }
    for t in &file.types {
        if t.name.trim().is_empty() || t.values.values().any(|v| !v.is_finite()) {
            return bad("invalid type");
        }
    }
    if def.ref_planes.len() + def.ref_lines.len() + def.dimensions.len() + def.equals.len()
        > crate::component_ref::MAX_REFS
    {
        return bad("too many reference planes, lines and dimensions");
    }
    let names = def.ref_planes.iter().map(|p| &p.name).chain(def.ref_lines.iter().map(|l| &l.name));
    let mut seen: Vec<&str> = def.params.iter().map(|p| p.name.as_str()).collect();
    for name in names {
        if !expr::is_valid_name(name)
            || crate::component_ref::is_reserved_for(def.kind, name)
            || seen.contains(&name.as_str())
        {
            return bad(&format!("invalid or repeated reference name '{name}'"));
        }
        seen.push(name);
    }
    Ok(())
}

pub fn save(file: &ComponentFile, path: &Path) -> Result<(), ComponentFileError> {
    let text = to_ron(file)?;
    let tmp = path.with_extension("ccomp.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn load(path: &Path) -> Result<ComponentFile, ComponentFileError> {
    from_ron(&std::fs::read_to_string(path)?)
}

/// Most component files a library scan reads.
pub const MAX_LIBRARY_FILES: usize = 5000;
/// Deepest subfolder a library scan enters.
const MAX_LIBRARY_DEPTH: usize = 8;

/// The component files (`.ccomp`) in `folders` and their subfolders, sorted
/// by path: the ones that load, and the paths of those that do not, with
/// why. Bounded in depth and number of files; links to folders are not followed.
#[allow(clippy::type_complexity)]
pub fn scan(
    folders: &[std::path::PathBuf],
) -> (Vec<(std::path::PathBuf, ComponentFile)>, Vec<(std::path::PathBuf, String)>) {
    let mut paths = Vec::new();
    let mut pending: Vec<(std::path::PathBuf, usize)> = folders.iter().map(|f| (f.clone(), 0)).collect();
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() && depth < MAX_LIBRARY_DEPTH {
                pending.push((path, depth + 1));
            } else if kind.is_file()
                && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ccomp"))
                && paths.len() < MAX_LIBRARY_FILES
            {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths.dedup();
    let mut loaded = Vec::new();
    let mut failed = Vec::new();
    for path in paths {
        match load(&path) {
            Ok(file) => loaded.push((path, file)),
            Err(e) => failed.push((path, e.to_string())),
        }
    }
    (loaded, failed)
}

/// Types to add to `project` for `file`: the component and type names are
/// made unique and missing materials are listed. Ids are allocated from
/// `alloc`, so callers can add them inside a transaction.
pub fn types_for(
    project: &Project,
    file: &ComponentFile,
    mut alloc: impl FnMut() -> u64,
) -> (Vec<ElementType>, Vec<Material>) {
    let components: Vec<&str> = project
        .component_types()
        .filter_map(|t| match &t.data {
            TypeData::Component { component, .. } => Some(component.as_str()),
            _ => None,
        })
        .collect();
    let unique = |base: &str, taken: &dyn Fn(&str) -> bool| {
        let mut name = base.to_string();
        let mut n = 2;
        while taken(&name) {
            name = format!("{base} ({n})");
            n += 1;
        }
        name
    };
    let component = unique(&file.component, &|n| components.contains(&n));
    let mut names: Vec<String> = project.component_types().map(|t| t.name.clone()).collect();
    let mut types = Vec::new();
    for t in &file.types {
        let name = unique(&t.name, &|n| names.iter().any(|x| x == n));
        names.push(name.clone());
        let mut def = file.def.clone();
        for p in &mut def.params {
            if let Some(v) = t.values.get(&p.name) {
                p.value = *v;
            }
        }
        let id = ElementTypeId(alloc());
        types.push(ElementType {
            id,
            name,
            data: TypeData::Component { component: component.clone(), def },
            info: t.info.clone(),
            uid: Default::default(),
        });
    }
    let materials =
        file.materials.iter().filter(|m| !project.materials.iter().any(|x| x.name == m.name)).cloned().collect();
    (types, materials)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Category;

    fn table_component(p: &Project) -> String {
        p.types_of(Category::Component)
            .find_map(|t| match &t.data {
                TypeData::Component { component, def } if def.items.len() == 5 => Some(component.clone()),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn round_trips_into_another_project() {
        let mut source = Project::from_template();
        let name = table_component(&source);
        // A custom material on the top, and a second, wider type.
        let table = source.types_of(Category::Component).find(|t| t.name == "Mesa 120 x 80").unwrap().id;
        if let Some(TypeData::Component { def, .. }) = source.types.get_mut(&table).map(|t| &mut t.data) {
            def.items[0].material = "Nogueira".into();
        }
        source.materials.push(Material::new("Nogueira", [90, 60, 40]));
        let mut wide = source.types[&table].clone();
        wide.id = ElementTypeId(source.alloc_id());
        wide.name = "Mesa 200 x 90".into();
        if let TypeData::Component { def, .. } = &mut wide.data {
            def.params[0].value = 2.0;
        }
        source.types.insert(wide.id, wide);
        let file = extract(&source, &name).unwrap();
        assert_eq!(file.types.len(), 2);
        let text = to_ron(&file).unwrap();
        let back = from_ron(&text).unwrap();
        assert_eq!(back, file);

        // Loading into a project that already has the component renames it.
        let target = Project::from_template();
        let mut next = 1000;
        let (types, materials) = types_for(&target, &back, || {
            next += 1;
            next
        });
        assert_eq!(types.len(), 2);
        assert!(matches!(&types[0].data, TypeData::Component { component, .. } if *component == format!("{name} (2)")));
        assert_eq!(types[0].name, "Mesa 120 x 80 (2)");
        let widths: Vec<f64> = types
            .iter()
            .filter_map(|t| match &t.data {
                TypeData::Component { def, .. } => Some(def.params[0].value),
                _ => None,
            })
            .collect();
        assert_eq!(widths, vec![1.2, 2.0]);
        // Only the material the target lacks is added.
        assert_eq!(materials, vec![Material::new("Nogueira", [90, 60, 40])]);
        assert!(matches!(extract(&target, "Nope"), Err(ComponentFileError::NotFound(_))));
    }

    #[test]
    fn hostile_files_are_rejected() {
        let p = Project::from_template();
        let file = extract(&p, &table_component(&p)).unwrap();
        let text = to_ron(&file).unwrap();
        let future = text.replacen("format_version: 1", "format_version: 99", 1);
        assert!(matches!(from_ron(&future), Err(ComponentFileError::Version(99))));
        let bad_name = text.replacen("name: \"Largura\"", "name: \"1 bad\"", 1);
        assert!(matches!(from_ron(&bad_name), Err(ComponentFileError::Invalid(_))));
        let mut big = file.clone();
        big.def.items = vec![big.def.items[0].clone(); MAX_ITEMS + 1];
        assert!(matches!(from_ron(&to_ron(&big).unwrap()), Err(ComponentFileError::Invalid(_))));
        assert!(from_ron("not ron").is_err());
    }

    #[test]
    fn title_blocks_hold_only_sheet_pieces() {
        let p = Project::from_template();
        let tb = p.title_blocks()[0];
        let name = p.types[&tb].name.clone();
        let file = extract(&p, &name).unwrap();
        // No 3D pieces, and an empty title block, are fine.
        assert!(from_ron(&to_ron(&file).unwrap()).is_ok());
        let mut empty = file.clone();
        empty.def.sheet.clear();
        assert!(from_ron(&to_ron(&empty).unwrap()).is_ok());
        // Sheet pieces in another kind, or too many of them, are not.
        let mut table = extract(&p, &table_component(&p)).unwrap();
        table.def.sheet = file.def.sheet.clone();
        assert!(matches!(from_ron(&to_ron(&table).unwrap()), Err(ComponentFileError::Invalid(_))));
        let mut big = file.clone();
        big.def.sheet = vec![file.def.sheet[0].clone(); crate::title_block::MAX_SHEET_PIECES + 1];
        assert!(matches!(from_ron(&to_ron(&big).unwrap()), Err(ComponentFileError::Invalid(_))));
    }
}
