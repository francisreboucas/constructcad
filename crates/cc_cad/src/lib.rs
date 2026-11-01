//! Reading DWG and DXF drawings into ConstructCAD's neutral
//! [`cc_core::cad::CadDrawing`] (an underlay placed in a 2D view).
//!
//! DXF (ASCII and binary) is read by an in-house reader: group-code pairs,
//! the HEADER (units, version, code page), the LAYER table, BLOCKS and
//! ENTITIES; blocks are inserted (nested, arrayed, cycle-safe) and 2D
//! entities taken from their object coordinate systems to the plan. DWG is
//! read by the `acadrust` crate, whose document is written to binary DXF in
//! memory and read by the same reader, so both formats share one path.
//! Everything is bounded (file size, primitives, nesting, work) and errors
//! are values.

mod dwg;
mod dxf;
mod flatten;
mod geom;
pub mod svg;
mod text;
mod tokens;

use cc_core::cad::CadDrawing;
use std::collections::BTreeMap;

/// Why a drawing could not be read.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CadError {
    #[error("the file is larger than {0} MB")]
    TooLarge(usize),
    #[error("not a DWG or DXF file")]
    UnknownFormat,
    #[error("invalid DXF data: {0}")]
    Format(String),
    #[error("the drawing is too complex: {0}")]
    TooComplex(String),
    #[error("the DWG file could not be read: {0}")]
    Dwg(String),
    #[error("{0}")]
    Io(String),
}

/// Bounds on what one read may do.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Largest DXF file, bytes.
    pub max_dxf_bytes: usize,
    /// Largest DWG file, bytes.
    pub max_dwg_bytes: usize,
    /// Most primitives produced (blocks inserted).
    pub max_prims: usize,
    /// Most entity visits while inserting blocks (arrays of nested blocks).
    pub max_work: usize,
    /// Deepest block nesting followed.
    pub max_depth: usize,
    /// Most entities kept in block definitions.
    pub max_block_entities: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_dxf_bytes: 256 << 20,
            max_dwg_bytes: 128 << 20,
            max_prims: cc_core::cad::MAX_PRIMS,
            max_work: 20_000_000,
            max_depth: 32,
            max_block_entities: 5_000_000,
        }
    }
}

/// File formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Dxf,
    Dwg,
    Svg,
}

/// A drawing read, with what the reader saw.
#[derive(Debug, Clone, Default)]
pub struct CadRead {
    pub drawing: CadDrawing,
    pub format: Format,
    /// `$ACADVER` (e.g. `AC1032`).
    pub version: String,
    /// A binary DXF file (or a DWG, read through binary DXF).
    pub binary: bool,
    /// Model space entities read.
    pub entities: usize,
    /// Block definitions read.
    pub blocks: usize,
    /// Entities not drawn, by type.
    pub skipped: BTreeMap<String, usize>,
    /// A limit stopped the reader; the drawing is incomplete.
    pub truncated: bool,
}

impl CadRead {
    pub fn skipped_total(&self) -> usize {
        self.skipped.values().sum()
    }
}

/// The format of a file's bytes: DWG files start with their version
/// (`AC1015`…), binary DXF with its sentinel, ASCII DXF with group pairs.
pub fn detect(data: &[u8]) -> Option<Format> {
    if data.len() >= 6 && data.starts_with(b"AC1") && data[3..6].iter().all(u8::is_ascii_digit) {
        return Some(Format::Dwg);
    }
    if data.len() >= 6 && data.starts_with(b"AC2.") {
        return Some(Format::Dwg);
    }
    if data.starts_with(tokens::BINARY_SENTINEL) {
        return Some(Format::Dxf);
    }
    // ASCII DXF: the first non-blank line is a group code.
    let head = std::str::from_utf8(data.get(..data.len().min(4096))?).ok().or_else(|| {
        // A non-UTF-8 code page further on: look at the ASCII prefix only.
        let end = data.iter().take(4096).position(|b| !b.is_ascii()).unwrap_or(0);
        std::str::from_utf8(data.get(..end)?).ok()
    })?;
    let first = head.trim_start_matches('\u{feff}').lines().map(str::trim).find(|l| !l.is_empty())?;
    first.parse::<i32>().is_ok().then_some(Format::Dxf)
}

/// Read a drawing from its bytes (DXF, DWG or SVG).
pub fn read(data: &[u8], limits: &Limits) -> Result<CadRead, CadError> {
    if detect(data).is_none() && svg::is_svg(data) {
        return svg::read(data, limits);
    }
    match detect(data).ok_or(CadError::UnknownFormat)? {
        Format::Dxf => {
            if data.len() > limits.max_dxf_bytes {
                return Err(CadError::TooLarge(limits.max_dxf_bytes >> 20));
            }
            dxf::read(data, limits)
        }
        Format::Dwg => {
            if data.len() > limits.max_dwg_bytes {
                return Err(CadError::TooLarge(limits.max_dwg_bytes >> 20));
            }
            dwg::read(data, limits)
        }
        Format::Svg => svg::read(data, limits),
    }
}

/// Read a drawing file.
pub fn read_file(path: &std::path::Path, limits: &Limits) -> Result<CadRead, CadError> {
    let max = limits.max_dxf_bytes.max(limits.max_dwg_bytes);
    let len = std::fs::metadata(path).map_err(|e| CadError::Io(e.to_string()))?.len();
    if len > max as u64 {
        return Err(CadError::TooLarge(max >> 20));
    }
    let data = std::fs::read(path).map_err(|e| CadError::Io(e.to_string()))?;
    read(&data, limits)
}

#[cfg(test)]
mod tests;
