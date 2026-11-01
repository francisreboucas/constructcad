//! Repair of projects read from files, which may be damaged or crafted:
//! invalid numbers and dangling references are removed or reset so the rest
//! of the program can rely on a consistent model.

use crate::{Element, ElementKind, Project, TypeData, ViewKind, WallTop};
use glam::DVec2;

fn finite(values: &[f64]) -> bool {
    values.iter().all(|v| v.is_finite())
}

fn finite_points(points: &[DVec2]) -> bool {
    points.iter().all(|p| p.is_finite())
}

fn top_ok(top: WallTop) -> bool {
    match top {
        WallTop::Level { offset, .. } => offset.is_finite(),
        WallTop::Unconnected => true,
    }
}

/// Whether every number of the element is finite and its outlines have at least three points.
fn element_is_valid(e: &Element) -> bool {
    match &e.kind {
        ElementKind::Wall(w) => finite(&[w.base_offset, w.height]) && finite_points(&[w.start, w.end]) && top_ok(w.top),
        ElementKind::Door(h) | ElementKind::Window(h) => finite(&[h.offset, h.sill_height]),
        ElementKind::Floor(f) => f.height_offset.is_finite() && f.boundary.len() >= 3 && finite_points(&f.boundary),
        ElementKind::Ceiling(c) => c.height_offset.is_finite() && c.boundary.len() >= 3 && finite_points(&c.boundary),
        ElementKind::Roof(r) => {
            let shape = match r.shape {
                crate::RoofShape::Flat => true,
                crate::RoofShape::Shed { slope, direction_deg } => finite(&[slope, direction_deg]),
                crate::RoofShape::Gable { slope, ridge_deg } => finite(&[slope, ridge_deg]),
                crate::RoofShape::Hip { slope } => slope.is_finite(),
            };
            shape && finite(&[r.base_offset, r.overhang]) && r.footprint.len() >= 3 && finite_points(&r.footprint)
        }
        ElementKind::Room(r) => r.height.is_finite() && r.position.is_finite(),
        ElementKind::Grid(g) => finite_points(&[g.start, g.end]),
        ElementKind::Dimension(d) => {
            let kind = match d.kind {
                crate::DimensionKind::Linear { dir } | crate::DimensionKind::Ordinate { dir } => {
                    dir.is_finite() && dir.length() > 1e-9
                }
                crate::DimensionKind::Angular { vertex, .. } => vertex.is_finite(),
                _ => true,
            };
            kind && d.offset.is_finite() && finite_points(&[d.a, d.b])
        }
        ElementKind::Text(t) => {
            let leaders = t.leaders.iter().all(|l| l.end.is_finite() && l.elbow.is_none_or(|e| e.is_finite()));
            let width = t.width.is_none_or(|w| w.is_finite() && w > 0.0);
            leaders && width && t.rotation_deg.is_finite() && t.position.is_finite()
        }
        ElementKind::Column(c) => {
            finite(&[c.rotation_deg, c.base_offset, c.height]) && c.position.is_finite() && top_ok(c.top)
        }
        ElementKind::Beam(b) => {
            b.top_offset.is_finite() && b.end_offset.is_none_or(f64::is_finite) && finite_points(&[b.start, b.end])
        }
        ElementKind::Rebar(r) => {
            let layout = match r.layout {
                crate::RebarLayout::Longitudinal { count, .. } => (1..=200).contains(&count),
                crate::RebarLayout::Stirrups { spacing } => spacing.is_finite() && spacing >= 0.02,
            };
            layout && r.cover.is_finite() && r.cover >= 0.0
        }
        ElementKind::Foundation(f) => {
            let shape = match &f.shape {
                crate::FoundationShape::Pad { position, rotation_deg } => {
                    position.is_finite() && rotation_deg.is_finite()
                }
                crate::FoundationShape::Strip { start, end } => finite_points(&[*start, *end]),
                crate::FoundationShape::Slab { boundary } => boundary.len() >= 3 && finite_points(boundary),
            };
            shape && f.top_offset.is_finite()
        }
        ElementKind::Stair(s) => {
            finite(&[s.direction_deg, s.base_offset, s.height]) && s.start.is_finite() && top_ok(s.top)
        }
        ElementKind::Ramp(r) => {
            finite(&[r.direction_deg, r.base_offset, r.height]) && r.start.is_finite() && top_ok(r.top)
        }
        ElementKind::Railing(r) => r.base_offset.is_finite() && r.path.len() >= 2 && finite_points(&r.path),
        ElementKind::Component(c) => finite(&[c.rotation_deg, c.base_offset]) && c.position.is_finite(),
        ElementKind::Pipe(s) | ElementKind::Conduit(s) => {
            finite(&[s.start_offset, s.end_offset])
                && finite_points(&[s.start, s.end])
                && (s.plan_length() >= 1e-3 || s.rise().abs() >= 1e-3)
        }
        ElementKind::CadImport(c) => c.is_valid(),
        ElementKind::Mass(m) => m.is_valid(),
        ElementKind::Sweep(s) => s.is_valid(),
    }
}

/// Fit a profile's thicknesses into its box; returns how many changed (0 or 1).
fn fit_shape(shape: &mut crate::SectionShape, width: f64, depth: f64) -> usize {
    let fitted = shape.fitted(width, depth);
    let changed = fitted != *shape;
    *shape = fitted;
    usize::from(changed)
}

/// Replace a non-finite or non-positive value by `fallback`; returns whether it changed.
fn fix_positive(value: &mut f64, fallback: f64) -> bool {
    if value.is_finite() && *value > 0.0 {
        return false;
    }
    *value = fallback;
    true
}

impl Project {
    /// Remove or reset invalid data; returns the number of repairs made.
    pub fn repair(&mut self) -> usize {
        let mut repairs = usize::from(self.units.repair());
        repairs += self.materials.iter_mut().map(|m| m.repair()).sum::<usize>();
        for level in &mut self.levels {
            if !level.elevation.is_finite() {
                level.elevation = 0.0;
                repairs += 1;
            }
        }
        self.sort_levels();

        for ty in self.types.values_mut() {
            match &mut ty.data {
                TypeData::Wall { layers, .. } | TypeData::Floor { layers } => {
                    for layer in layers.iter_mut() {
                        repairs += usize::from(fix_positive(&mut layer.thickness, 0.01));
                    }
                }
                TypeData::Roof { layers, framing, .. } => {
                    for layer in layers.iter_mut() {
                        repairs += usize::from(fix_positive(&mut layer.thickness, 0.01));
                    }
                    repairs += framing.repair();
                }
                TypeData::Door { width, height, .. } => {
                    repairs += usize::from(fix_positive(width, 0.9)) + usize::from(fix_positive(height, 2.1));
                }
                TypeData::Window { width, height, default_sill, .. } => {
                    repairs += usize::from(fix_positive(width, 1.2)) + usize::from(fix_positive(height, 1.2));
                    if !default_sill.is_finite() {
                        *default_sill = 1.0;
                        repairs += 1;
                    }
                }
                TypeData::Ceiling { thickness, .. } => repairs += usize::from(fix_positive(thickness, 0.02)),
                TypeData::Grid { bubble } => repairs += usize::from(fix_positive(bubble, 8.0)),
                TypeData::Dimension { text_size } => repairs += usize::from(fix_positive(text_size, 2.5)),
                TypeData::Text { size, .. } => repairs += usize::from(fix_positive(size, 2.5)),
                TypeData::Room {} | TypeData::CadImport {} | TypeData::Mass { .. } => {}
                TypeData::Column { section, .. } => match section {
                    crate::ColumnSection::Rectangular { width, depth } => {
                        repairs += usize::from(fix_positive(width, 0.2)) + usize::from(fix_positive(depth, 0.2));
                    }
                    crate::ColumnSection::Round { diameter } => repairs += usize::from(fix_positive(diameter, 0.3)),
                    crate::ColumnSection::Profile { width, depth, shape } => {
                        repairs += usize::from(fix_positive(width, 0.2)) + usize::from(fix_positive(depth, 0.2));
                        repairs += fit_shape(shape, *width, *depth);
                    }
                    crate::ColumnSection::Pipe { diameter, wall } => {
                        repairs += usize::from(fix_positive(diameter, 0.2));
                        let fitted =
                            if wall.is_finite() { wall.clamp(0.001, *diameter * 0.5) } else { *diameter * 0.5 };
                        if fitted != *wall {
                            *wall = fitted;
                            repairs += 1;
                        }
                    }
                },
                TypeData::Beam { width, height, shape, .. } => {
                    repairs += usize::from(fix_positive(width, 0.15)) + usize::from(fix_positive(height, 0.4));
                    repairs += fit_shape(shape, *width, *height);
                }
                TypeData::Rebar { diameter, .. } => repairs += usize::from(fix_positive(diameter, 0.01)),
                TypeData::Pipe { diameter, .. } | TypeData::Conduit { diameter, .. } => {
                    repairs += usize::from(fix_positive(diameter, 0.025));
                }
                TypeData::Foundation { width, length, thickness, .. } => {
                    repairs += usize::from(fix_positive(width, 1.0))
                        + usize::from(fix_positive(length, 1.0))
                        + usize::from(fix_positive(thickness, 0.4));
                }
                TypeData::Stair { width, max_riser, tread, build, rules, .. } => {
                    repairs += usize::from(fix_positive(width, 1.0))
                        + usize::from(fix_positive(max_riser, 0.18))
                        + usize::from(fix_positive(tread, 0.28));
                    repairs += build.repair() + rules.repair();
                }
                TypeData::Ramp { width, thickness, rules, .. } => {
                    repairs += usize::from(fix_positive(width, 1.2)) + usize::from(fix_positive(thickness, 0.15));
                    repairs += rules.repair();
                }
                TypeData::Sweep { profile, .. } => {
                    if crate::profile_sweep::check_profile(profile).is_err() {
                        *profile = crate::profile_sweep::baseboard();
                        repairs += 1;
                    }
                }
                TypeData::Railing { height, post_spacing, .. } => {
                    repairs += usize::from(fix_positive(height, 1.1)) + usize::from(fix_positive(post_spacing, 1.2));
                }
                TypeData::Component { def, .. } => {
                    if let Some(ifc) = &mut def.ifc {
                        repairs += crate::properties::repair_sets(&mut ifc.properties);
                    }
                    for p in &mut def.params {
                        if !p.value.is_finite() {
                            p.value = 0.0;
                            repairs += 1;
                        }
                    }
                    let fix_nonnegative = |v: &mut f64| {
                        let bad = !(v.is_finite() && *v >= 0.0);
                        if bad {
                            *v = 0.0;
                        }
                        usize::from(bad)
                    };
                    match &mut def.mep {
                        Some(crate::MepData::Fixture { drain_units, supply_weight, .. }) => {
                            repairs += fix_nonnegative(drain_units) + fix_nonnegative(supply_weight);
                        }
                        Some(crate::MepData::Device { power, .. }) => repairs += fix_nonnegative(power),
                        None => {}
                    }
                }
            }
        }

        // Imported drawings: invalid primitives and layer references go (copied
        // only when something is wrong, since the drawing is shared).
        for e in self.elements.values_mut() {
            if let ElementKind::CadImport(c) = &mut e.kind
                && c.drawing.needs_repair()
            {
                repairs += std::sync::Arc::make_mut(&mut c.drawing).repair();
            }
        }

        // Sketches that are invalid or no longer make their element's outline.
        repairs += self.elements.values_mut().map(|e| usize::from(e.drop_stale_sketch())).sum::<usize>();

        // Mass floors on levels that are gone.
        let levels: std::collections::BTreeSet<_> = self.levels.iter().map(|l| l.id).collect();
        for e in self.elements.values_mut() {
            if let ElementKind::Mass(m) = &mut e.kind {
                let before = m.floors.len();
                m.floors.retain(|l| levels.contains(l));
                m.floors.dedup();
                repairs += before - m.floors.len();
            }
        }

        // Elements: repeat until stable, since removing a wall orphans what it hosts.
        loop {
            let invalid: Vec<_> = self
                .elements
                .values()
                .filter(|e| {
                    let type_ok = self.element_type(e.type_id).is_some_and(|t| t.category() == e.category());
                    let level_ok = self.level(e.level).is_some();
                    let host_ok = match &e.kind {
                        ElementKind::Door(h) | ElementKind::Window(h) => {
                            self.element(h.host).is_some_and(|w| w.as_wall().is_some())
                        }
                        ElementKind::Rebar(r) => self
                            .element(r.host)
                            .is_some_and(|h| matches!(h.kind, ElementKind::Beam(_) | ElementKind::Column(_))),
                        ElementKind::Dimension(d) => self.view(d.view).is_some(),
                        ElementKind::Text(t) => self.view(t.view).is_some(),
                        ElementKind::CadImport(c) => self.view(c.view).is_some(),
                        _ => true,
                    };
                    !(type_ok && level_ok && host_ok && element_is_valid(e))
                })
                .map(|e| e.id)
                .collect();
            if invalid.is_empty() {
                break;
            }
            repairs += invalid.len();
            for id in invalid {
                self.elements.remove(&id);
            }
        }
        // Dimension ends referencing missing elements or bad numbers become free.
        let ids: std::collections::BTreeSet<_> = self.elements.keys().copied().collect();
        let level_ids: Vec<_> = self.levels.iter().map(|l| l.id).collect();
        let exists = |t: crate::RefTarget| match t {
            crate::RefTarget::Element(id) => ids.contains(&id),
            crate::RefTarget::Level(id) => level_ids.contains(&id),
        };
        for e in self.elements.values_mut() {
            let ElementKind::Dimension(d) = &mut e.kind else { continue };
            for r in d.all_refs_mut() {
                if r.is_some_and(|r| !exists(r.target) || !r.is_finite()) {
                    *r = None;
                    repairs += 1;
                }
            }
        }
        // Stair and ramp layouts get valid sizes; railings of missing ones keep their path.
        let stairs: std::collections::BTreeSet<_> = self
            .elements
            .values()
            .filter(|e| matches!(e.kind, ElementKind::Stair(_) | ElementKind::Ramp(_)))
            .map(|e| e.id)
            .collect();
        for e in self.elements.values_mut() {
            match &mut e.kind {
                ElementKind::Stair(s) => repairs += s.layout.repair(),
                ElementKind::Ramp(r) => repairs += r.layout.repair(),
                ElementKind::Railing(r) => {
                    if r.stair.is_some_and(|h| !stairs.contains(&h.host)) {
                        r.stair = None;
                        repairs += 1;
                    }
                    if let Some(h) = &mut r.stair
                        && !(h.inset.is_finite() && h.inset.abs() < 1.0)
                    {
                        h.inset = 0.05;
                        repairs += 1;
                    }
                }
                _ => {}
            }
        }
        // Pins of missing elements go.
        let before = self.pinned.len();
        let elements = &self.elements;
        self.pinned.retain(|id| elements.contains_key(id));
        repairs += before - self.pinned.len();
        // Joins need two distinct elements with a body, joined once.
        let before = self.joins.len();
        let mut seen = std::collections::BTreeSet::new();
        let joinable = |id: crate::ElementId| elements.get(&id).is_some_and(|e| crate::can_join_geometry(e.category()));
        self.joins.retain(|&(a, b)| a != b && joinable(a) && joinable(b) && seen.insert((a.min(b), a.max(b))));
        repairs += before - self.joins.len();
        // Groups keep existing elements, each in one group, and are not empty.
        let mut grouped = std::collections::BTreeSet::new();
        for group in self.groups.values_mut() {
            let before = group.members.len();
            group.members.retain(|m| elements.contains_key(m) && grouped.insert(*m));
            repairs += before - group.members.len();
        }
        let before = self.groups.len();
        self.groups.retain(|_, g| !g.members.is_empty());
        repairs += before - self.groups.len();
        // Component overrides and imported properties must be finite.
        for e in self.elements.values_mut() {
            if let ElementKind::Component(c) = &mut e.kind {
                let before = c.overrides.len();
                c.overrides.retain(|_, v| v.is_finite());
                repairs += before - c.overrides.len();
                repairs += crate::properties::repair_sets(&mut c.properties);
            }
        }
        // Floor openings need three finite points.
        for e in self.elements.values_mut() {
            let ElementKind::Floor(f) = &mut e.kind else { continue };
            let before = f.openings.len();
            f.openings.retain(|o| o.len() >= 3 && finite_points(o));
            repairs += before - f.openings.len();
        }
        // Roof openings need three finite points.
        for e in self.elements.values_mut() {
            let ElementKind::Roof(r) = &mut e.kind else { continue };
            let before = r.holes.len();
            r.holes.retain(|h| h.len() >= 3 && finite_points(h));
            repairs += before - r.holes.len();
            // Edge slopes: one per footprint edge, finite and not negative.
            let n = r.footprint.len();
            if r.edge_slopes.len() > n {
                r.edge_slopes.truncate(n);
                repairs += 1;
            }
            for s in &mut r.edge_slopes {
                if s.is_some_and(|v| !(v.is_finite() && v >= 0.0)) {
                    *s = None;
                    repairs += 1;
                }
            }
            while r.edge_slopes.last().is_some_and(Option::is_none) {
                r.edge_slopes.pop();
            }
        }
        // Arc walls: a bulge that is not a number makes the wall straight, one
        // past a nearly full circle is clamped, a negligible one is dropped.
        for e in self.elements.values_mut() {
            let ElementKind::Wall(w) = &mut e.kind else { continue };
            let fixed = if !w.bulge.is_finite() || w.bulge.abs() < 1e-6 {
                0.0
            } else {
                w.bulge.clamp(-crate::arc::MAX_BULGE, crate::arc::MAX_BULGE)
            };
            if fixed.to_bits() != w.bulge.to_bits() {
                w.bulge = fixed;
                repairs += 1;
            }
        }
        // Systems: circuits fed by a missing board lose it, voltages are positive;
        // elements drop references to missing systems (a pipe keeps one system).
        for system in self.systems.values_mut() {
            if system.panel.is_some_and(|p| !ids.contains(&p)) {
                system.panel = None;
                repairs += 1;
            }
            repairs += usize::from(fix_positive(&mut system.voltage, 127.0));
        }
        let systems = &self.systems;
        for e in self.elements.values_mut() {
            let max = if matches!(e.kind, ElementKind::Pipe(_)) { 1 } else { usize::MAX };
            match &mut e.kind {
                ElementKind::Pipe(s) | ElementKind::Conduit(s) => {
                    let before = s.systems.len();
                    let mut seen = std::collections::BTreeSet::new();
                    s.systems.retain(|id| systems.contains_key(id) && seen.insert(*id));
                    s.systems.truncate(max);
                    repairs += before - s.systems.len();
                }
                ElementKind::Component(c) if c.system.is_some_and(|id| !systems.contains_key(&id)) => {
                    c.system = None;
                    repairs += 1;
                }
                _ => {}
            }
        }
        // Walls attached to a missing level keep their height, unconnected.
        let levels: Vec<_> = self.levels.iter().map(|l| l.id).collect();
        for e in self.elements.values_mut() {
            let top = match &mut e.kind {
                ElementKind::Wall(w) => &mut w.top,
                ElementKind::Column(c) => &mut c.top,
                ElementKind::Stair(s) => &mut s.top,
                ElementKind::Ramp(r) => &mut r.top,
                _ => continue,
            };
            if let WallTop::Level { level, .. } = *top
                && !levels.contains(&level)
            {
                *top = WallTop::Unconnected;
                repairs += 1;
            }
        }

        // Views on missing levels (or boards) go; sheets and legends drop missing references.
        let elements = &self.elements;
        let dangling: Vec<_> = self
            .views
            .values()
            .filter(|v| {
                v.level().is_some_and(|l| !levels.contains(&l))
                    || matches!(v.kind, ViewKind::Diagram { panel } if !elements.contains_key(&panel))
            })
            .map(|v| v.id)
            .collect();
        repairs += dangling.len();
        for id in dangling {
            self.views.remove(&id);
        }
        let view_ids: Vec<_> = self.views.keys().copied().collect();
        let type_ids: Vec<_> = self.types.keys().copied().collect();
        for view in self.views.values_mut() {
            match &mut view.kind {
                ViewKind::Sheet { viewports, .. } => {
                    let before = viewports.len();
                    viewports.retain(|vp| view_ids.contains(&vp.view) && vp.center.is_finite());
                    repairs += before - viewports.len();
                }
                ViewKind::Legend { types } => {
                    let before = types.len();
                    types.retain(|t| type_ids.contains(t));
                    repairs += before - types.len();
                }
                _ => {}
            }
            if view.scale == 0 {
                view.scale = 100;
                repairs += 1;
            }
        }

        // View graphics: filters, templates and the elements they refer to.
        for f in self.filters.values_mut() {
            let before = f.rules.len();
            f.rules.retain(|r| r.value.len() <= 1000);
            repairs += before - f.rules.len();
        }
        repairs += self.repair_phases();
        for t in self.templates.values_mut() {
            repairs += t.repair(&self.filters, &self.phase_filters);
        }
        let elements = &self.elements;
        let exists = |id: crate::ElementId| elements.contains_key(&id);
        for view in self.views.values_mut() {
            repairs += view.graphics.repair(&self.filters, Some(&exists));
            let tags = view.tag_offsets.len();
            view.tag_offsets.retain(|id, o| exists(*id) && o.is_finite() && o.length() < 1e4);
            repairs += tags - view.tag_offsets.len();
            match view.template.map(|t| self.templates.get(&t)) {
                Some(None) => {
                    view.template = None;
                    repairs += 1;
                }
                Some(Some(_)) if !crate::ViewTemplate::applies_to(&view.kind) => {
                    view.template = None;
                    repairs += 1;
                }
                Some(Some(t)) => {
                    let before = view.clone();
                    t.apply(view);
                    repairs += usize::from(*view != before);
                }
                None => {}
            }
        }
        repairs
    }
}

impl Project {
    /// Phases, phase filters and the phases of elements and views; returns
    /// how many repairs were made.
    fn repair_phases(&mut self) -> usize {
        let mut repairs = 0;
        let mut seen = std::collections::BTreeSet::new();
        let before = self.phases.len();
        self.phases.retain(|p| p.id.0 != 0 && seen.insert(p.id));
        repairs += before - self.phases.len();
        for p in &mut self.phases {
            if p.name.trim().is_empty() {
                p.name = p.id.to_string();
                repairs += 1;
            }
        }
        for (id, f) in &mut self.phase_filters {
            if f.id != *id {
                f.id = *id;
                repairs += 1;
            }
        }
        repairs += self.phase_graphics.repair();
        let fixes = if self.phases.is_empty() {
            // No phases: nothing may refer to one.
            self.elements
                .values()
                .filter(|e| e.phase_created.0 != 0 || e.phase_demolished.is_some())
                .map(|e| (e.id, crate::PhaseId(0), None))
                .collect()
        } else {
            self.phase_fixes()
        };
        repairs += fixes.len();
        for (id, created, demolished) in fixes {
            if let Some(e) = self.elements.get_mut(&id) {
                e.phase_created = created;
                e.phase_demolished = demolished;
            }
        }
        let last = self.last_phase().unwrap_or_default();
        for v in self.views.values_mut() {
            if v.phase.0 != 0 && !self.phases.iter().any(|p| p.id == v.phase) {
                v.phase = last;
                repairs += 1;
            }
            if v.phase_filter.is_some_and(|f| !self.phase_filters.contains_key(&f)) {
                v.phase_filter = None;
                repairs += 1;
            }
        }
        repairs
    }
}

#[cfg(test)]
mod tests {
    use crate::{Category, Element, ElementId, ElementKind, Hosted, Project, Wall};
    use glam::DVec2;

    #[test]
    fn invalid_elements_and_references_are_removed() {
        let mut p = Project::from_template();
        let level = p.levels[0].id;
        let wall_t = p.default_type(Category::Wall).unwrap();
        let door_t = p.default_type(Category::Door).unwrap();
        let add = |p: &mut Project, type_id, kind| {
            let id = ElementId(p.alloc_id());
            p.elements.insert(
                id,
                Element {
                    id,
                    level,
                    type_id,
                    mark: String::new(),
                    comments: String::new(),
                    kind,
                    uid: Default::default(),
                    phase_created: Default::default(),
                    phase_demolished: None,
                },
            );
            id
        };
        let good = add(&mut p, wall_t, ElementKind::Wall(Wall { end: DVec2::new(4.0, 0.0), ..Default::default() }));
        let nan = add(&mut p, wall_t, ElementKind::Wall(Wall { end: DVec2::new(f64::NAN, 0.0), ..Default::default() }));
        let door = |host| {
            ElementKind::Door(Hosted { host, offset: 1.0, sill_height: 0.0, flip_facing: false, flip_hand: false })
        };
        let on_good = add(&mut p, door_t, door(good));
        let on_nan = add(&mut p, door_t, door(nan));
        let missing_type = add(&mut p, crate::ElementTypeId(9999), ElementKind::Wall(Wall::default()));
        let wrong_type = add(&mut p, door_t, ElementKind::Wall(Wall::default()));

        let repairs = p.repair();
        assert_eq!(repairs, 4, "nan wall, its door, missing type, wrong category");
        assert!(p.element(good).is_some() && p.element(on_good).is_some());
        for id in [nan, on_nan, missing_type, wrong_type] {
            assert!(p.element(id).is_none());
        }
        assert_eq!(p.repair(), 0, "repair is idempotent");
    }

    /// Crafted arc walls: a NaN, an infinite, a huge and a negligible bulge.
    #[test]
    fn degenerate_arc_walls_are_repaired() {
        let mut p = Project::from_template();
        let level = p.levels[0].id;
        let wall_t = p.default_type(Category::Wall).unwrap();
        let mut ids = Vec::new();
        for bulge in [f64::NAN, f64::INFINITY, -1e6, 1e-9, 0.5] {
            let id = ElementId(p.alloc_id());
            let wall = Wall { end: DVec2::new(4.0, 0.0), bulge, ..Default::default() };
            let e = Element {
                id,
                level,
                type_id: wall_t,
                mark: String::new(),
                comments: String::new(),
                kind: ElementKind::Wall(wall),
                uid: Default::default(),
                phase_created: Default::default(),
                phase_demolished: None,
            };
            p.elements.insert(id, e);
            ids.push(id);
        }
        assert_eq!(p.repair(), 4);
        let bulges: Vec<f64> = ids.iter().map(|id| p.element(*id).unwrap().as_wall().unwrap().bulge).collect();
        assert_eq!(bulges, vec![0.0, 0.0, -crate::arc::MAX_BULGE, 0.0, 0.5]);
        assert_eq!(p.repair(), 0);
        // Collinear three points give no arc.
        assert!(crate::arc::Arc::bulge_through(DVec2::ZERO, DVec2::X, DVec2::new(2.0, 0.0)).is_none());
    }

    /// Crafted roof edge slopes: more than edges, a NaN and a negative one.
    #[test]
    fn roof_edge_slopes_are_repaired() {
        let mut p = Project::from_template();
        let level = p.levels[0].id;
        let roof_t = p.default_type(Category::Roof).unwrap();
        let square = vec![DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y];
        let mut roof = crate::Roof::new(square, 3.0, 0.0, crate::RoofShape::Hip { slope: 0.5 });
        roof.edge_slopes = vec![Some(f64::NAN), Some(0.8), Some(-1.0), None, Some(0.3)];
        let id = ElementId(p.alloc_id());
        let e = Element {
            id,
            level,
            type_id: roof_t,
            mark: String::new(),
            comments: String::new(),
            kind: ElementKind::Roof(roof),
            uid: Default::default(),
            phase_created: Default::default(),
            phase_demolished: None,
        };
        p.elements.insert(id, e);
        assert_eq!(p.repair(), 3, "one too many, a NaN, a negative slope");
        let Some(ElementKind::Roof(r)) = p.element(id).map(|e| &e.kind) else { panic!("the roof stays") };
        assert_eq!(r.edge_slopes, vec![None, Some(0.8)]);
        assert_eq!(p.repair(), 0);
    }

    /// Crafted building-services data: references to missing systems and
    /// boards, a pipe in two systems, an empty run and a NaN device power.
    #[test]
    fn building_services_are_repaired() {
        let mut p = Project::from_template();
        let level = p.levels[0].id;
        let pipe_t = p.default_type(Category::Pipe).unwrap();
        let system = *p.systems.keys().next().unwrap();
        let other = *p.systems.keys().nth(1).unwrap();
        let add = |p: &mut Project, kind| {
            let id = ElementId(p.alloc_id());
            let e = Element {
                id,
                level,
                type_id: pipe_t,
                mark: String::new(),
                comments: String::new(),
                kind,
                uid: Default::default(),
                phase_created: Default::default(),
                phase_demolished: None,
            };
            p.elements.insert(id, e);
            id
        };
        let mut run = crate::MepSegment::new(DVec2::ZERO, DVec2::new(2.0, 0.0), 0.5);
        run.systems = vec![system, crate::SystemId(9999), other];
        let pipe = add(&mut p, ElementKind::Pipe(run));
        let empty = add(&mut p, ElementKind::Pipe(crate::MepSegment::new(DVec2::ONE, DVec2::ONE, 0.5)));
        let circuit = crate::SystemId(p.alloc_id());
        p.systems.insert(
            circuit,
            crate::MepSystem {
                panel: Some(ElementId(424242)),
                voltage: f64::NAN,
                ..crate::MepSystem::new("1", "1", crate::SystemKind::Power)
            },
        );
        let outlet = p
            .types
            .values_mut()
            .find_map(|t| match &mut t.data {
                crate::TypeData::Component { def, .. } if matches!(def.mep, Some(crate::MepData::Device { .. })) => {
                    Some(def)
                }
                _ => None,
            })
            .unwrap();
        outlet.mep = Some(crate::MepData::Device { device: crate::DeviceKind::Outlet, power: f64::NAN });
        // The missing system, the second system, the empty run, the board, the voltage, the power.
        assert_eq!(p.repair(), 6);
        let Some(ElementKind::Pipe(s)) = p.element(pipe).map(|e| &e.kind) else { panic!("pipe removed") };
        assert_eq!(s.systems, vec![system]);
        assert!(p.element(empty).is_none());
        assert!(p.systems.values().all(|s| s.panel.is_none() && s.voltage == 127.0));
        assert_eq!(p.repair(), 0, "repair is idempotent");
    }

    #[test]
    fn components_are_repaired_and_old_projects_get_the_library() {
        let mut p = Project::from_template();
        let level = p.levels[0].id;
        let ty = p.default_type(Category::Component).unwrap();
        if let Some(crate::TypeData::Component { def, .. }) = p.types.get_mut(&ty).map(|t| &mut t.data) {
            def.params[0].value = f64::INFINITY;
        }
        let ok = ElementId(p.alloc_id());
        let bad = ElementId(p.alloc_id());
        for (id, rotation) in [(ok, 0.0), (bad, f64::NAN)] {
            let overrides = std::collections::BTreeMap::from([("x".to_string(), f64::NAN), ("y".to_string(), 1.0)]);
            let kind = ElementKind::Component(crate::Component {
                position: DVec2::ZERO,
                rotation_deg: rotation,
                base_offset: 0.0,
                overrides,
                properties: Vec::new(),
                system: None,
            });
            p.elements.insert(
                id,
                Element {
                    id,
                    level,
                    type_id: ty,
                    mark: String::new(),
                    comments: String::new(),
                    kind,
                    uid: Default::default(),
                    phase_created: Default::default(),
                    phase_demolished: None,
                },
            );
        }
        // Infinite parameter, invalid element, NaN override.
        assert_eq!(p.repair(), 3);
        assert!(p.element(bad).is_none());
        let Some(ElementKind::Component(c)) = p.element(ok).map(|e| &e.kind) else { panic!("component removed") };
        assert_eq!(c.overrides.len(), 1);

        let mut old = Project::from_template();
        old.types.retain(|_, t| t.category() != Category::Component);
        old.ensure_annotation_types();
        assert_eq!(old.types_of(Category::Component).count(), 16);
        assert!(old.materials.iter().any(|m| m.name == "Tecido"));
    }
}
