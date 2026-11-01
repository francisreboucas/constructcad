//! Derived geometry for ConstructCAD.
//!
//! [`ModelGeometry::build`] turns a [`cc_core::Project`] into boundary
//! representation [`Solid`]s (planar polygon faces) that feed the 3D renderer,
//! elevations and sections, plus [`WallGeom`] and [`BeamGeom`] data used by plan views.

pub mod analytical;
mod beam;
mod build;
pub mod component;
pub mod csg;
mod cut;
pub mod mass;
pub mod mep;
pub mod mep_net;
mod mesh;
pub mod polygon;
pub mod profile_sweep;
pub mod ramp;
pub mod rebar;
pub mod roof_framing;
pub mod roofs;
mod rooms;
pub mod section;
pub mod sizing;
pub mod skeleton;
mod solid;
pub mod stair;
mod wall;

pub use beam::{BeamEnd, BeamGeom};
pub use build::{
    BoxPart, FRAME_COLOR, FoundationGeom, HostedPart, ModelGeometry, WorldConnector, column_outline,
    component_connectors, component_parts, hosted_parts, opening_parts,
};
pub use component::{resolve_component, resolve_component_in};
pub use cut::{clip_polygon, face_plane_segments};
pub use mep::{SegmentGeom, segment_geom};
pub use mesh::{MeshPlan, mesh_plan, plan_edges};
pub use ramp::{RampGeom, RampPiece, ramp_geom};
pub use rooms::{outer_boundary, room_boundary};
pub use solid::{Face, Ray, Solid};
pub use stair::{
    Flight, Pieces, StairGeom, StairStep, StepKind, add_railing, railing_length, railing_path, railing_pieces,
    stair_geom,
};
pub use wall::{Column, Opening, WallGeom};

pub use glam::{DVec2, DVec3};
