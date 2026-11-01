//! SketchUp (.skp) files: reader (current and legacy formats, scenes and
//! per-definition instanced scenes) and writer (`create`, `edit`).
//!
//! Derived from the MIT-licensed OpenSKP toolkit (see `LICENSE`); no external
//! dependencies. Lengths in the parsed model are inches, as in the file.
//!
//! ```no_run
//! use cc_skp::{ParseOptions, SkpFile};
//!
//! let model = SkpFile::open("building.skp")?.parse(&ParseOptions::default())?;
//! println!("{} {} definitions", model.version, model.definitions.len());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod create;
pub mod edit;
pub mod errors;
pub mod instanced_scene;
pub mod model;
pub mod observability;
pub mod parser;
pub mod scene;

pub mod triangulator;

/// Raw DEFLATE / zlib compressor (no external crates).
pub mod deflate;
/// Raw DEFLATE / zlib decompressor (miniz `tinfl` / `mz_uncompress` replacement).
pub mod inflate;
/// In-memory ZIP reader (miniz `mz_zip_reader_*` replacement).
pub mod zip;

#[doc(hidden)]
pub mod curves;
#[doc(hidden)]
pub mod earcut;
#[doc(hidden)]
pub mod face_groups;
#[doc(hidden)]
pub mod instanced_scene_build;
#[doc(hidden)]
pub mod internal;
#[doc(hidden)]
pub mod legacy;
#[doc(hidden)]
pub mod scene_build;

pub use edit::{OpenExistingError, OpenExistingResult, open_existing};
pub use errors::SkpParseError;
pub use instanced_scene::{
    InstancedCurveResource, InstancedMeshResource, InstancedNode, InstancedScene, LocalCurve, LocalPrimitive,
    SceneBounds,
};
pub use model::{
    ArcCurve, AttributeKind, ByteBuffer, CoEdge, Color3, Color4, ConstructionLine, ConstructionPoint, Definition,
    Dimension, Edge, EntityId, Face, Instance, Layer, LooseEdgeRun, METRES_TO_INCHES, Material, Page, ParsedAttrDict,
    ParsedAttrDictionaries, ParsedAttribute, SectionPlane, SkpModel, StringDictionaries, Style, StyleItem,
    StyleWatermark, TextEntity, Texture, Vec3, Vertex, ViewCamera, loose_edge_runs, stringify_attr_dict,
    stringify_attr_dictionaries,
};
pub use observability::{LogLevel, PROGRESS_INTERVAL, ParseOptions, ParseProgress, ParseStage, stage_name};
pub use parser::{SkpFile, build_instanced_scene, build_scene, parse_skp};
pub use scene::{
    CurveArc, CurveSetMetadata, GlbPrimitive, GltfMaterial, InstanceNode, MeshMetadata, PbrMetallicRoughness, Scene,
    SceneTexture,
};
pub use triangulator::{compute_face_normal, triangulate_face_3d};

pub use create::{
    ArcOptions, AttributeDict, AttributeValue, CircleOptions, ComponentDefinitionBuilder, DefinitionHandle,
    DefinitionOptions, FaceOptions, GroupInstanceOptions, GroupOptions, ImageOptions, InstanceOptions, LayerOptions,
    PolylineOptions, SkpBuilder, SkpWriteError, create,
};
