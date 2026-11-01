//! Face normals and planar-face triangulation (`triangulator.hpp` /
//! `triangulator.cpp`).

use std::collections::BTreeMap;

use crate::earcut::{EarPoint, earcut_2d};
use crate::model::{EntityId, Vec3, Vertex};

/// Newell's-method unit normal of a polygon, or `None` for fewer than three
/// points or a degenerate (zero-area) polygon.
pub fn compute_face_normal(points: &[Vec3]) -> Option<Vec3> {
    if points.len() < 3 {
        return None;
    }
    let mut normal = [0.0; 3];
    for i in 0..points.len() {
        let current = &points[i];
        let next = &points[(i + 1) % points.len()];
        normal[0] += (current[1] - next[1]) * (current[2] + next[2]);
        normal[1] += (current[2] - next[2]) * (current[0] + next[0]);
        normal[2] += (current[0] - next[0]) * (current[1] + next[1]);
    }
    let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if length < 1e-12 {
        return None;
    }
    for value in &mut normal {
        *value /= length;
    }
    Some(normal)
}

/// Triangulates a planar face given as vertex-id loops (`loops[0]` the outer
/// boundary, the rest holes) in the plane of `normal`. Returns vertex-id
/// triples; empty when a general-path loop references a vertex missing from
/// `vertices`.
pub fn triangulate_face_3d(
    vertices: &BTreeMap<EntityId, Vertex>,
    loops: &[Vec<EntityId>],
    normal: Vec3,
) -> Vec<[EntityId; 3]> {
    triangulate_with(|id| vertices.get(&id).map(|q| [q.x, q.y, q.z]), loops, normal)
}

/// [`triangulate_face_3d`] over any vertex lookup (the scene builders read the
/// builder's own `id -> position` map instead of copying it into `Vertex`es).
pub(crate) fn triangulate_with(
    lookup: impl Fn(EntityId) -> Option<Vec3>,
    loops: &[Vec<EntityId>],
    mut n: Vec3,
) -> Vec<[EntityId; 3]> {
    if loops.is_empty() {
        return Vec::new();
    }
    if loops.len() == 1 && loops[0].len() == 3 {
        return vec![[loops[0][0], loops[0][1], loops[0][2]]];
    }
    if loops.len() == 1 && loops[0].len() == 4 {
        return vec![[loops[0][0], loops[0][1], loops[0][2]], [loops[0][0], loops[0][2], loops[0][3]]];
    }
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len < 1e-6 {
        n = [0.0, 0.0, 1.0];
    } else {
        for x in &mut n {
            *x /= len;
        }
    }
    let seed: Vec3 = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let mut u = [n[1] * seed[2] - n[2] * seed[1], n[2] * seed[0] - n[0] * seed[2], n[0] * seed[1] - n[1] * seed[0]];
    let len = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
    if len < 1e-12 {
        u = [1.0, 0.0, 0.0];
    } else {
        for x in &mut u {
            *x /= len;
        }
    }
    let v = [n[1] * u[2] - n[2] * u[1], n[2] * u[0] - n[0] * u[2], n[0] * u[1] - n[1] * u[0]];
    let mut projected = Vec::with_capacity(loops.len());
    for lp in loops {
        let mut p = Vec::with_capacity(lp.len());
        for &id in lp {
            let Some(q) = lookup(id) else { return Vec::new() };
            p.push(EarPoint {
                x: q[0] * u[0] + q[1] * u[1] + q[2] * u[2],
                y: q[0] * v[0] + q[1] * v[1] + q[2] * v[2],
                id,
            });
        }
        projected.push(p);
    }
    earcut_2d(projected)
}
