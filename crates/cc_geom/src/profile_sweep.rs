//! The solid of a swept profile: its type's profile carried along its path
//! in plan, at the level's elevation plus its base offset, mitered at the
//! corners (and around a closed path).

use cc_core::component::SketchPlane;
use cc_core::profile_sweep::Sweep;
use cc_core::{Element, Project, TypeData};
use glam::{DVec2, DVec3};

/// The profile of a sweep's type, on the side the sweep puts it.
pub fn sweep_profile(project: &Project, element: &Element, sweep: &Sweep) -> Option<Vec<DVec2>> {
    let TypeData::Sweep { profile, .. } = &project.element_type(element.type_id)?.data else { return None };
    cc_core::profile_sweep::check_profile(profile).ok()?;
    let side = if sweep.flip { -1.0 } else { 1.0 };
    let placed: Vec<DVec2> = profile.iter().map(|p| DVec2::new(p.x * side, p.y)).collect();
    Some(cc_core::sweep::counter_clockwise(&placed))
}

/// The faces of a sweep (outward), or `None` when its path or profile cannot make one.
pub fn sweep_faces(project: &Project, element: &Element, sweep: &Sweep) -> Option<Vec<Vec<DVec3>>> {
    if !sweep.is_valid() {
        return None;
    }
    let profile = sweep_profile(project, element, sweep)?;
    let z = project.level_elevation(element.level) + sweep.base_offset;
    let faces = if sweep.closed {
        cc_core::sweep::sweep_closed(SketchPlane::Plan, &sweep.path, z, &profile)
    } else {
        let mut path = sweep.path.clone();
        path.dedup_by(|a, b| a.distance(*b) < 1e-9);
        cc_core::sweep::sweep(SketchPlane::Plan, &path, z, &profile)
    };
    faces.ok()
}
