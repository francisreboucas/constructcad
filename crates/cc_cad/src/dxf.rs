//! DXF files: sections, tables, blocks and entities, read into raw
//! entities (their group pairs) that [`crate::flatten`] turns into
//! primitives. Model space entities are flattened as they are read, so the
//! largest section is never held whole.

use crate::flatten::{Block, Flattener};
use crate::tokens::{Pairs, Val};
use crate::{CadError, CadRead, Limits};
use cc_core::cad::{CadDrawing, CadLayer, CadUnit, aci_color};
use glam::DVec3;
use std::collections::HashMap;

/// An entity as read: its type, its groups in order, and the entities that
/// belong to it (a polyline's vertices, an insert's attributes).
#[derive(Debug, Clone, Default)]
pub struct Entity {
    pub name: String,
    pub groups: Vec<(i32, Val)>,
    pub children: Vec<Entity>,
}

impl Entity {
    pub fn get(&self, code: i32) -> Option<&Val> {
        self.groups.iter().find(|(c, _)| *c == code).map(|(_, v)| v)
    }

    pub fn real(&self, code: i32, default: f64) -> f64 {
        self.get(code).map(Val::real).filter(|v| v.is_finite()).unwrap_or(default)
    }

    pub fn int(&self, code: i32, default: i64) -> i64 {
        self.get(code).map(Val::int).unwrap_or(default)
    }

    pub fn str(&self, code: i32) -> &str {
        self.get(code).map(Val::str).unwrap_or("")
    }

    pub fn has(&self, code: i32) -> bool {
        self.get(code).is_some()
    }

    /// The point of groups `code`, `code + 10`, `code + 20` (first occurrence).
    pub fn point(&self, code: i32) -> DVec3 {
        DVec3::new(self.real(code, 0.0), self.real(code + 10, 0.0), self.real(code + 20, 0.0))
    }

    /// The extrusion direction (210/220/230), +Z by default.
    pub fn normal(&self) -> DVec3 {
        if self.has(210) || self.has(220) || self.has(230) {
            let n = DVec3::new(self.real(210, 0.0), self.real(220, 0.0), self.real(230, 1.0));
            n.try_normalize().unwrap_or(DVec3::Z)
        } else {
            DVec3::Z
        }
    }

    /// Every point of groups `code`/`code + 10`/`code + 20`, in order.
    pub fn points(&self, code: i32) -> Vec<DVec3> {
        let mut out: Vec<DVec3> = Vec::new();
        for (c, v) in &self.groups {
            let v = v.real();
            let v = if v.is_finite() { v } else { 0.0 };
            if *c == code {
                out.push(DVec3::new(v, 0.0, 0.0));
            } else if *c == code + 10 {
                if let Some(p) = out.last_mut() {
                    p.y = v;
                }
            } else if *c == code + 20
                && let Some(p) = out.last_mut()
            {
                p.z = v;
            }
        }
        out
    }

    /// Every value of `code`, in order.
    pub fn reals(&self, code: i32) -> Vec<f64> {
        self.groups.iter().filter(|(c, _)| *c == code).map(|(_, v)| v.real()).collect()
    }
}

/// Entity types the reader draws (and those it ignores on purpose).
fn supported(name: &str) -> bool {
    matches!(
        name,
        "LINE"
            | "LWPOLYLINE"
            | "POLYLINE"
            | "CIRCLE"
            | "ARC"
            | "ELLIPSE"
            | "SPLINE"
            | "POINT"
            | "TEXT"
            | "MTEXT"
            | "INSERT"
            | "DIMENSION"
            | "ARC_DIMENSION"
            | "LARGE_RADIAL_DIMENSION"
            | "HATCH"
            | "SOLID"
            | "TRACE"
            | "3DFACE"
            | "LEADER"
            | "ATTRIB"
            // Not drawn on purpose: attribute templates and paper space viewports.
            | "ATTDEF"
            | "VIEWPORT"
    )
}

/// Reads a whole DXF file.
pub fn read(data: &[u8], limits: &Limits) -> Result<CadRead, CadError> {
    let mut pairs = Pairs::new(data);
    let mut layers: Vec<CadLayer> = Vec::new();
    let mut units: Option<i64> = None;
    let mut version = String::new();
    let mut blocks: HashMap<String, Block> = HashMap::new();
    let mut flattener: Option<Flattener> = None;
    let mut read = CadRead::default();

    while let Some((code, val)) = pairs.next_pair()? {
        if code == 999 {
            continue;
        }
        if code != 0 {
            // Stray pairs between sections are tolerated.
            continue;
        }
        match val.str() {
            "EOF" => break,
            "SECTION" => {}
            _ => continue,
        }
        let name = match pairs.next_pair()? {
            Some((2, v)) => v.str().to_ascii_uppercase(),
            _ => return Err(CadError::Format("a section has no name".into())),
        };
        match name.as_str() {
            "HEADER" => read_header(&mut pairs, &mut units, &mut version)?,
            "TABLES" => read_tables(&mut pairs, &mut layers)?,
            "BLOCKS" => read_blocks(&mut pairs, &mut blocks, &mut read, limits)?,
            "ENTITIES" => {
                let f = flattener.get_or_insert_with(|| Flattener::new(limits));
                f.set_layers(&layers);
                read_entities(&mut pairs, f, &blocks, &mut read)?;
            }
            _ => skip_section(&mut pairs)?,
        }
    }
    let mut f = flattener.unwrap_or_else(|| Flattener::new(limits));
    f.set_layers(&layers);
    let (prims, layers, truncated) = f.finish();
    let mut drawing = CadDrawing { layers, prims, units: units.and_then(CadUnit::from_insunits), ..Default::default() };
    drawing.repair();
    drawing.compute_extents();
    read.truncated |= truncated;
    read.version = version;
    read.binary = pairs.is_binary();
    read.drawing = drawing;
    Ok(read)
}

fn skip_section(pairs: &mut Pairs) -> Result<(), CadError> {
    while let Some((code, val)) = pairs.next_pair()? {
        if code == 0 && val.str() == "ENDSEC" {
            break;
        }
    }
    Ok(())
}

fn read_header(pairs: &mut Pairs, units: &mut Option<i64>, version: &mut String) -> Result<(), CadError> {
    let mut var = String::new();
    while let Some((code, val)) = pairs.next_pair()? {
        match code {
            0 if val.str() == "ENDSEC" => break,
            9 => var = val.str().to_ascii_uppercase(),
            70 if var == "$INSUNITS" => *units = Some(val.int()),
            3 if var == "$DWGCODEPAGE" => pairs.decoder.set_code_page(val.str()),
            1 if var == "$ACADVER" => *version = val.str().trim().to_string(),
            _ => {}
        }
    }
    Ok(())
}

/// Groups up to the next code 0.
fn read_groups(pairs: &mut Pairs) -> Result<Vec<(i32, Val)>, CadError> {
    let mut groups = Vec::new();
    loop {
        match pairs.peek()? {
            None => break,
            Some((0, _)) => break,
            Some(_) => {}
        }
        if let Some((c, v)) = pairs.next_pair()?
            && c != 999
            && v != Val::Bin
        {
            groups.push((c, v));
        }
    }
    Ok(groups)
}

fn read_tables(pairs: &mut Pairs, layers: &mut Vec<CadLayer>) -> Result<(), CadError> {
    while let Some((code, val)) = pairs.next_pair()? {
        if code != 0 {
            continue;
        }
        match val.str() {
            "ENDSEC" => break,
            "LAYER" => {
                let groups = read_groups(pairs)?;
                let e = Entity { name: "LAYER".into(), groups, children: Vec::new() };
                let name = e.str(2).to_string();
                if name.is_empty() || layers.iter().any(|l| l.name.eq_ignore_ascii_case(&name)) {
                    continue;
                }
                let aci = e.int(62, 7);
                let color = match e.get(420) {
                    Some(v) => rgb(v.int()),
                    None => aci_color(aci),
                };
                let flags = e.int(70, 0);
                // Off (negative color) or frozen (flag 1).
                let visible = aci >= 0 && flags & 1 == 0;
                layers.push(CadLayer { name, color, visible });
            }
            _ => {}
        }
    }
    Ok(())
}

/// The color of a 24-bit true color value.
pub fn rgb(v: i64) -> [u8; 3] {
    [((v >> 16) & 0xff) as u8, ((v >> 8) & 0xff) as u8, (v & 0xff) as u8]
}

/// One entity after its `0 NAME` pair, with its vertices or attributes.
fn read_entity(pairs: &mut Pairs, name: String) -> Result<Entity, CadError> {
    let groups = read_groups(pairs)?;
    let mut e = Entity { name, groups, children: Vec::new() };
    let follows = match e.name.as_str() {
        "POLYLINE" => true,
        "INSERT" => e.int(66, 0) == 1,
        _ => false,
    };
    if follows {
        // Vertices or attributes, up to SEQEND (or anything else, in damaged files).
        while let Some((0, v)) = pairs.peek()? {
            let child = v.str().to_string();
            if child == "SEQEND" {
                pairs.next_pair()?;
                read_groups(pairs)?;
                break;
            }
            if child != "VERTEX" && child != "ATTRIB" {
                break;
            }
            pairs.next_pair()?;
            let groups = read_groups(pairs)?;
            if e.children.len() < cc_core::cad::MAX_POLY_POINTS {
                e.children.push(Entity { name: child, groups, children: Vec::new() });
            }
        }
    }
    Ok(e)
}

fn count(read: &mut CadRead, name: &str) {
    if !supported(name) {
        *read.skipped.entry(name.to_string()).or_default() += 1;
    }
}

fn read_blocks(
    pairs: &mut Pairs,
    blocks: &mut HashMap<String, Block>,
    read: &mut CadRead,
    limits: &Limits,
) -> Result<(), CadError> {
    let mut stored = 0usize;
    while let Some((code, val)) = pairs.next_pair()? {
        if code != 0 {
            continue;
        }
        match val.str() {
            "ENDSEC" => break,
            "BLOCK" => {
                let head = Entity { name: "BLOCK".into(), groups: read_groups(pairs)?, children: Vec::new() };
                let name = head.str(2).to_ascii_uppercase();
                let mut block = Block { base: head.point(10), entities: Vec::new() };
                // Entities up to ENDBLK.
                while let Some((c, v)) = pairs.next_pair()? {
                    if c != 0 {
                        continue;
                    }
                    let kind = v.str().to_string();
                    if kind == "ENDBLK" {
                        read_groups(pairs)?;
                        break;
                    }
                    if kind == "ENDSEC" || kind == "EOF" {
                        // Damaged file: no ENDBLK.
                        blocks.insert(name, block);
                        return Ok(());
                    }
                    let e = read_entity(pairs, kind)?;
                    count(read, &e.name);
                    stored += 1 + e.children.len();
                    if stored > limits.max_block_entities {
                        return Err(CadError::TooComplex(format!(
                            "more than {} entities in block definitions",
                            limits.max_block_entities
                        )));
                    }
                    block.entities.push(e);
                }
                read.blocks += 1;
                if !name.is_empty() {
                    blocks.insert(name, block);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn read_entities(
    pairs: &mut Pairs,
    f: &mut Flattener,
    blocks: &HashMap<String, Block>,
    read: &mut CadRead,
) -> Result<(), CadError> {
    while let Some((code, val)) = pairs.next_pair()? {
        if code != 0 {
            continue;
        }
        let kind = val.str().to_string();
        if kind == "ENDSEC" || kind == "EOF" {
            break;
        }
        let e = read_entity(pairs, kind)?;
        read.entities += 1;
        count(read, &e.name);
        // Paper space entities are not part of the model drawing.
        if e.int(67, 0) == 1 {
            continue;
        }
        f.top_level(&e, blocks);
    }
    Ok(())
}
