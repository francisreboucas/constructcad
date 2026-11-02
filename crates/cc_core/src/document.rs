use crate::{
    Element, ElementId, ElementType, ElementTypeId, Level, Material, Project, ProjectInfo, View, ViewId, ViewKind,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Before/after state of one keyed object; `None` means absent.
type Change<T> = (Option<T>, Option<T>);

/// Project-wide settings tracked as a single unit.
#[derive(Debug, Clone, PartialEq)]
struct Settings {
    info: ProjectInfo,
    levels: Vec<Level>,
    materials: Vec<Material>,
    units: crate::Units,
    pinned: std::collections::BTreeSet<ElementId>,
    joins: std::collections::BTreeSet<(ElementId, ElementId)>,
    groups: BTreeMap<crate::GroupId, crate::Group>,
    systems: BTreeMap<crate::SystemId, crate::MepSystem>,
    filters: BTreeMap<crate::FilterId, crate::ViewFilter>,
    templates: BTreeMap<crate::TemplateId, crate::ViewTemplate>,
    phases: Vec<crate::Phase>,
    phase_filters: BTreeMap<crate::PhaseFilterId, crate::PhaseFilter>,
    phase_graphics: crate::PhaseGraphics,
}

impl Settings {
    fn of(p: &Project) -> Self {
        Self {
            info: p.info.clone(),
            levels: p.levels.clone(),
            materials: p.materials.clone(),
            units: p.units.clone(),
            pinned: p.pinned.clone(),
            joins: p.joins.clone(),
            groups: p.groups.clone(),
            systems: p.systems.clone(),
            filters: p.filters.clone(),
            templates: p.templates.clone(),
            phases: p.phases.clone(),
            phase_filters: p.phase_filters.clone(),
            phase_graphics: p.phase_graphics,
        }
    }

    fn restore(self, p: &mut Project) {
        p.info = self.info;
        p.levels = self.levels;
        p.materials = self.materials;
        p.units = self.units;
        p.pinned = self.pinned;
        p.joins = self.joins;
        p.groups = self.groups;
        p.systems = self.systems;
        p.filters = self.filters;
        p.templates = self.templates;
        p.phases = self.phases;
        p.phase_filters = self.phase_filters;
        p.phase_graphics = self.phase_graphics;
    }
}

/// A recorded, reversible set of changes.
#[derive(Debug, Clone, Default)]
pub struct Transaction {
    pub name: String,
    elements: BTreeMap<ElementId, Change<Element>>,
    views: BTreeMap<ViewId, Change<View>>,
    types: BTreeMap<ElementTypeId, Change<ElementType>>,
    settings: Option<Change<Settings>>,
}

fn put<K: Ord, V>(map: &mut BTreeMap<K, V>, key: K, value: Option<V>) {
    match value {
        Some(v) => {
            map.insert(key, v);
        }
        None => {
            map.remove(&key);
        }
    }
}

impl Transaction {
    /// Fold a newer transaction into this one, keeping the oldest "before" states.
    fn merge(&mut self, newer: Transaction) {
        fn fold<K: Ord, V>(into: &mut BTreeMap<K, Change<V>>, from: BTreeMap<K, Change<V>>) {
            for (k, (before, after)) in from {
                match into.get_mut(&k) {
                    Some(c) => c.1 = after,
                    None => {
                        into.insert(k, (before, after));
                    }
                }
            }
        }
        fold(&mut self.elements, newer.elements);
        fold(&mut self.views, newer.views);
        fold(&mut self.types, newer.types);
        if let Some((before, after)) = newer.settings {
            match &mut self.settings {
                Some(c) => c.1 = after,
                None => self.settings = Some((before, after)),
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty() && self.views.is_empty() && self.types.is_empty() && self.settings.is_none()
    }

    /// Element ids touched by this transaction.
    pub fn changed_elements(&self) -> impl Iterator<Item = ElementId> + '_ {
        self.elements.keys().copied()
    }

    fn apply(&self, p: &mut Project, forward: bool) {
        fn pick<T: Clone>(c: &Change<T>, forward: bool) -> Option<T> {
            if forward { c.1.clone() } else { c.0.clone() }
        }
        for (id, c) in &self.elements {
            put(&mut p.elements, *id, pick(c, forward));
        }
        for (id, c) in &self.views {
            put(&mut p.views, *id, pick(c, forward));
        }
        for (id, c) in &self.types {
            put(&mut p.types, *id, pick(c, forward));
        }
        if let Some(c) = &self.settings
            && let Some(s) = pick(c, forward)
        {
            s.restore(p);
        }
    }
}

/// Mutable access to a project inside [`Document::edit`]; records the
/// original state of everything it touches.
pub struct EditCtx<'a> {
    project: &'a mut Project,
    tx: Transaction,
    /// The phase new elements are built in (else the last one).
    creation_phase: Option<crate::PhaseId>,
}

impl<'a> EditCtx<'a> {
    pub fn project(&self) -> &Project {
        self.project
    }

    pub fn alloc_id(&mut self) -> u64 {
        self.project.alloc_id()
    }

    fn touch_element(&mut self, id: ElementId) {
        if !self.tx.elements.contains_key(&id) {
            let before = self.project.elements.get(&id).cloned();
            self.tx.elements.insert(id, (before, None));
        }
    }

    fn touch_view(&mut self, id: ViewId) {
        if !self.tx.views.contains_key(&id) {
            let before = self.project.views.get(&id).cloned();
            self.tx.views.insert(id, (before, None));
        }
    }

    fn touch_type(&mut self, id: ElementTypeId) {
        if !self.tx.types.contains_key(&id) {
            let before = self.project.types.get(&id).cloned();
            self.tx.types.insert(id, (before, None));
        }
    }

    /// Insert an element. Its `id` must come from [`EditCtx::alloc_id`].
    pub fn add_element(&mut self, mut element: Element) -> ElementId {
        let id = element.id;
        // Copies of an element get their own persistent id.
        if element.uid.is_nil() || self.project.elements.values().any(|e| e.uid == element.uid && e.id != id) {
            element.uid = crate::Uid::generate();
        }
        self.touch_element(id);
        self.project.elements.insert(id, element);
        id
    }

    pub fn element_mut(&mut self, id: ElementId) -> Option<&mut Element> {
        if !self.project.elements.contains_key(&id) {
            return None;
        }
        self.touch_element(id);
        self.project.elements.get_mut(&id)
    }

    /// Remove an element and everything it hosts. Iterative with a visited set,
    /// so cyclic hosting in a crafted file cannot recurse forever.
    pub fn remove_element(&mut self, id: ElementId) {
        let mut pending = vec![id];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            pending.extend(self.project.hosted_by(id).map(|e| e.id));
            pending.extend(self.project.railings_of(id).map(|e| e.id));
            pending.extend(self.project.rebar_of(id).map(|(e, _)| e.id));
            self.touch_element(id);
            self.project.elements.remove(&id);
            // A pinned element removed with its host or view takes its pin along.
            if self.project.pinned.contains(&id) {
                self.touch_settings();
                self.project.pinned.remove(&id);
            }
            if self.project.joined_with(id).next().is_some() {
                self.touch_settings();
                self.project.joins.retain(|&(a, b)| a != id && b != id);
            }
            // A board's diagrams go with it.
            let diagrams: Vec<ViewId> = self
                .project
                .views
                .values()
                .filter(|v| matches!(v.kind, ViewKind::Diagram { panel } if panel == id))
                .map(|v| v.id)
                .collect();
            for view in diagrams {
                self.remove_view(view);
            }
            if self.project.systems.values().any(|s| s.panel == Some(id)) {
                // Circuits fed by a removed board stay, unfed.
                for system in self.systems_mut().values_mut().filter(|s| s.panel == Some(id)) {
                    system.panel = None;
                }
            }
            if self.project.group_of(id).is_some() {
                self.touch_settings();
                for group in self.project.groups.values_mut() {
                    group.members.remove(&id);
                }
                self.project.groups.retain(|_, g| !g.members.is_empty());
            }
        }
    }

    pub fn add_view(&mut self, view: View) -> ViewId {
        let id = view.id;
        self.touch_view(id);
        self.project.views.insert(id, view);
        id
    }

    pub fn view_mut(&mut self, id: ViewId) -> Option<&mut View> {
        if !self.project.views.contains_key(&id) {
            return None;
        }
        self.touch_view(id);
        self.project.views.get_mut(&id)
    }

    /// Remove a view, the elements it owns (dimensions, text notes,
    /// imported drawings) and any sheet viewports showing it.
    pub fn remove_view(&mut self, id: ViewId) {
        let owned: Vec<ElementId> =
            self.project.elements.values().filter(|e| e.owner_view() == Some(id)).map(|e| e.id).collect();
        for e in owned {
            self.remove_element(e);
        }
        let sheets: Vec<_> = self
            .project
            .views
            .values()
            .filter(|v| match &v.kind {
                ViewKind::Sheet { viewports, .. } => viewports.iter().any(|vp| vp.view == id),
                _ => false,
            })
            .map(|v| v.id)
            .collect();
        for sheet in sheets {
            if let Some(ViewKind::Sheet { viewports, .. }) = self.view_mut(sheet).map(|v| &mut v.kind) {
                viewports.retain(|vp| vp.view != id);
            }
        }
        self.touch_view(id);
        self.project.views.remove(&id);
    }

    pub fn add_type(&mut self, mut element_type: ElementType) -> ElementTypeId {
        let id = element_type.id;
        if element_type.uid.is_nil() || self.project.types.values().any(|t| t.uid == element_type.uid && t.id != id) {
            element_type.uid = crate::Uid::generate();
        }
        self.touch_type(id);
        self.project.types.insert(id, element_type);
        id
    }

    pub fn type_mut(&mut self, id: ElementTypeId) -> Option<&mut ElementType> {
        if !self.project.types.contains_key(&id) {
            return None;
        }
        self.touch_type(id);
        self.project.types.get_mut(&id)
    }

    /// Remove a type (callers make sure no element uses it).
    pub fn remove_type(&mut self, id: ElementTypeId) {
        if self.project.types.contains_key(&id) {
            self.touch_type(id);
            self.project.types.remove(&id);
        }
    }

    fn touch_settings(&mut self) {
        if self.tx.settings.is_none() {
            self.tx.settings = Some((Some(Settings::of(self.project)), None));
        }
    }

    pub fn info_mut(&mut self) -> &mut ProjectInfo {
        self.touch_settings();
        &mut self.project.info
    }

    /// Levels; callers should keep them sorted by elevation.
    pub fn levels_mut(&mut self) -> &mut Vec<Level> {
        self.touch_settings();
        &mut self.project.levels
    }

    pub fn materials_mut(&mut self) -> &mut Vec<Material> {
        self.touch_settings();
        &mut self.project.materials
    }

    /// Rename a material and every type reference to it. Returns false
    /// (and changes nothing) when `new` is empty or already taken.
    pub fn rename_material(&mut self, old: &str, new: &str) -> bool {
        let new = new.trim();
        if new.is_empty() || old == new || self.project.material(new).is_some() || self.project.material(old).is_none()
        {
            return false;
        }
        for id in self.project.material_users(old) {
            if let Some(t) = self.type_mut(id) {
                t.data.rename_material(old, new);
            }
        }
        if let Some(m) = self.materials_mut().iter_mut().find(|m| m.name == old) {
            m.name = new.to_string();
        }
        true
    }

    /// Pin or unpin elements; returns how many changed.
    pub fn set_pinned(&mut self, ids: &[ElementId], pinned: bool) -> usize {
        let existing: Vec<ElementId> = ids
            .iter()
            .copied()
            .filter(|id| self.project.element(*id).is_some() && self.project.is_pinned(*id) != pinned)
            .collect();
        if existing.is_empty() {
            return 0;
        }
        self.touch_settings();
        for id in &existing {
            if pinned {
                self.project.pinned.insert(*id);
            } else {
                self.project.pinned.remove(id);
            }
        }
        existing.len()
    }

    /// Join the geometry of two elements, `cutting` keeping their common
    /// volume (an existing join between them is replaced), or unjoin them.
    /// Returns whether anything changed.
    pub fn set_joined(&mut self, cutting: ElementId, cut: ElementId, joined: bool) -> bool {
        let current = self.project.join_of(cutting, cut);
        let wanted = joined.then_some((cutting, cut));
        if current == wanted {
            return false;
        }
        self.touch_settings();
        if let Some(pair) = current {
            self.project.joins.remove(&pair);
        }
        if let Some(pair) = wanted {
            self.project.joins.insert(pair);
        }
        true
    }

    /// Group `members` (taking them out of any group they were in) under
    /// `name`; returns the new group's id.
    pub fn add_group(&mut self, name: String, members: std::collections::BTreeSet<ElementId>) -> crate::GroupId {
        self.touch_settings();
        let id = crate::GroupId(self.alloc_id());
        for group in self.project.groups.values_mut() {
            group.members.retain(|m| !members.contains(m));
        }
        self.project.groups.retain(|_, g| !g.members.is_empty());
        self.project.groups.insert(id, crate::Group { name, members });
        id
    }

    /// Dissolve a group (its elements stay); returns whether it existed.
    pub fn remove_group(&mut self, id: crate::GroupId) -> bool {
        if !self.project.groups.contains_key(&id) {
            return false;
        }
        self.touch_settings();
        self.project.groups.remove(&id);
        true
    }

    /// Plumbing systems and electrical circuits.
    pub fn systems_mut(&mut self) -> &mut BTreeMap<crate::SystemId, crate::MepSystem> {
        self.touch_settings();
        &mut self.project.systems
    }

    /// View filters.
    pub fn filters_mut(&mut self) -> &mut BTreeMap<crate::FilterId, crate::ViewFilter> {
        self.touch_settings();
        &mut self.project.filters
    }

    /// View templates (views that follow one are updated when the edit ends).
    pub fn templates_mut(&mut self) -> &mut BTreeMap<crate::TemplateId, crate::ViewTemplate> {
        self.touch_settings();
        &mut self.project.templates
    }

    /// Keep view graphics consistent with the model: drop overrides of
    /// missing elements and missing filters, unassign missing templates,
    /// and copy each template's controlled properties into its views.
    fn sync_views(&mut self) {
        // Templates drop filters that no longer exist.
        let filters = &self.project.filters;
        let stale = self
            .project
            .templates
            .values()
            .any(|t| t.graphics.filters.iter().any(|f| !filters.contains_key(&f.filter)));
        if stale {
            let filters = filters.clone();
            for t in self.templates_mut().values_mut() {
                t.graphics.filters.retain(|f| filters.contains_key(&f.filter));
            }
        }
        let p = &*self.project;
        let changed: Vec<(ViewId, View)> = p
            .views
            .values()
            .filter(|v| v.template.is_some() || !v.graphics.is_empty() || !v.tag_offsets.is_empty())
            .filter_map(|v| {
                let mut new = v.clone();
                match new.template.map(|t| p.templates.get(&t)) {
                    Some(Some(t)) => t.apply(&mut new),
                    Some(None) => new.template = None,
                    None => {}
                }
                new.graphics.repair(&p.filters, Some(&|id| p.elements.contains_key(&id)));
                // Tags of deleted elements go.
                new.tag_offsets.retain(|id, o| p.elements.contains_key(id) && o.is_finite());
                (new != *v).then_some((v.id, new))
            })
            .collect();
        for (id, view) in changed {
            if let Some(v) = self.view_mut(id) {
                *v = view;
            }
        }
    }

    /// Construction phases, in time order.
    pub fn phases_mut(&mut self) -> &mut Vec<crate::Phase> {
        self.touch_settings();
        &mut self.project.phases
    }

    /// Phase filters.
    pub fn phase_filters_mut(&mut self) -> &mut BTreeMap<crate::PhaseFilterId, crate::PhaseFilter> {
        self.touch_settings();
        &mut self.project.phase_filters
    }

    /// Graphics of each phase status.
    pub fn phase_graphics_mut(&mut self) -> &mut crate::PhaseGraphics {
        self.touch_settings();
        &mut self.project.phase_graphics
    }

    /// Walls attached to an element this edit deleted (or turned into
    /// something that is no roof, ceiling or floor) keep their own top.
    fn sync_attachments(&mut self) {
        let p = &*self.project;
        let slab = |id: ElementId| {
            p.elements.get(&id).is_some_and(|e| {
                matches!(
                    e.kind,
                    crate::ElementKind::Floor(_) | crate::ElementKind::Ceiling(_) | crate::ElementKind::Roof(_)
                )
            })
        };
        let stale: Vec<ElementId> = p
            .elements
            .values()
            .filter(|e| matches!(&e.kind, crate::ElementKind::Wall(w) if w.attach_top.is_some_and(|t| !slab(t))))
            .map(|e| e.id)
            .collect();
        for id in stale {
            if let Some(crate::ElementKind::Wall(w)) = self.element_mut(id).map(|e| &mut e.kind) {
                w.attach_top = None;
            }
        }
    }

    /// Floors, ceilings and roofs take the cuts of the shafts that pass
    /// through them (see [`crate::shaft`]), in the same undo step as the
    /// edit that moved a shaft or them; unchanged cuts are left alone.
    fn sync_shafts(&mut self) {
        let p = &*self.project;
        let shafts = p.elements.values().any(|e| matches!(e.kind, crate::ElementKind::Shaft(_)));
        let changed: Vec<(ElementId, Vec<Vec<glam::DVec2>>)> = p
            .elements
            .values()
            .filter(|e| {
                matches!(
                    e.kind,
                    crate::ElementKind::Floor(_) | crate::ElementKind::Ceiling(_) | crate::ElementKind::Roof(_)
                )
            })
            .filter_map(|e| {
                let want = if shafts { p.shaft_cuts(e) } else { Vec::new() };
                (want.as_slice() != e.shaft_cuts()).then_some((e.id, want))
            })
            .collect();
        for (id, cuts) in changed {
            if let Some(slot) = self.element_mut(id).and_then(|e| e.shaft_cuts_mut()) {
                *slot = cuts;
            }
        }
    }

    /// Elements whose outline this edit changed without their sketch drop it
    /// (see [`crate::sketch`]).
    fn sync_sketches(&mut self) {
        let ids: Vec<ElementId> = self.tx.elements.keys().copied().collect();
        for id in ids {
            if let Some(e) = self.project.elements.get_mut(&id) {
                e.drop_stale_sketch();
            }
        }
    }

    /// Keep phases consistent: elements and views added by this edit get
    /// the creation phase (views: the last phase and the default phase
    /// filter), and hosted elements follow their hosts
    /// ([`Project::phase_fixes`]).
    fn sync_phases(&mut self) {
        let p = &*self.project;
        let Some(last) = p.last_phase() else { return };
        let creation = self.creation_phase.filter(|c| p.phase(*c).is_some()).unwrap_or(last);
        let added: Vec<ElementId> = self
            .tx
            .elements
            .iter()
            .filter(|(id, c)| c.0.is_none() && p.elements.get(id).is_some_and(|e| p.phase(e.phase_created).is_none()))
            .map(|(id, _)| *id)
            .collect();
        for id in added {
            if let Some(e) = self.project.elements.get_mut(&id) {
                e.phase_created = creation;
            }
        }
        let p = &*self.project;
        let filter = p.default_phase_filter();
        let views: Vec<ViewId> = self
            .tx
            .views
            .iter()
            .filter(|(id, c)| c.0.is_none() && p.views.get(id).is_some_and(|v| p.phase(v.phase).is_none()))
            .map(|(id, _)| *id)
            .collect();
        for id in views {
            if let Some(v) = self.project.views.get_mut(&id) {
                v.phase = last;
                if v.phase_filter.is_none() {
                    v.phase_filter = filter;
                }
            }
        }
        for (id, created, demolished) in self.project.phase_fixes() {
            if let Some(e) = self.element_mut(id) {
                e.phase_created = created;
                e.phase_demolished = demolished;
            }
        }
    }

    pub fn units_mut(&mut self) -> &mut crate::Units {
        self.touch_settings();
        &mut self.project.units
    }

    /// Fill in "after" states and drop no-op entries.
    fn finish(mut self) -> Transaction {
        // Associative dimensions follow what this edit changed (elements or levels).
        self.sync_shafts();
        self.sync_attachments();
        self.sync_dimensions();
        self.sync_phases();
        self.sync_sketches();
        self.sync_views();
        let p = &*self.project;
        self.tx.elements.retain(|id, c| {
            c.1 = p.elements.get(id).cloned();
            c.0 != c.1
        });
        self.tx.views.retain(|id, c| {
            c.1 = p.views.get(id).cloned();
            c.0 != c.1
        });
        self.tx.types.retain(|id, c| {
            c.1 = p.types.get(id).cloned();
            c.0 != c.1
        });
        if let Some(c) = &mut self.tx.settings {
            c.1 = Some(Settings::of(p));
            if c.0 == c.1 {
                self.tx.settings = None;
            }
        }
        self.tx
    }
}

/// An open project plus its undo history and file state.
pub struct Document {
    pub project: Project,
    pub path: Option<PathBuf>,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    revision: u64,
    saved_revision: u64,
    /// A view state change (see [`Document::set_view_state`]) since saving.
    view_state_changed: bool,
    /// Merge key of the last recorded transaction, for coalescing.
    last_merge_key: Option<u64>,
    /// The phase new elements are built in (the active view's; `None`:
    /// the last phase). Display state: not saved, not undone.
    pub creation_phase: Option<crate::PhaseId>,
}

impl Document {
    pub fn new(mut project: Project) -> Self {
        project.ensure_uids();
        Self {
            project,
            path: None,
            undo: Vec::new(),
            redo: Vec::new(),
            revision: 1,
            saved_revision: 1,
            view_state_changed: false,
            last_merge_key: None,
            creation_phase: None,
        }
    }

    /// Apply a change that is saved but not undoable, and that no derived
    /// geometry depends on (a 3D view's camera). The revision stays, so caches
    /// keyed on it stay valid: callers drop what does depend on the change
    /// (see `Session::set_view_camera`).
    pub fn set_view_state(&mut self, f: impl FnOnce(&mut Project)) {
        f(&mut self.project);
        self.view_state_changed = true;
    }

    /// Monotonic counter bumped on every model change; use it to invalidate caches.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision || self.view_state_changed
    }

    pub fn mark_saved(&mut self) {
        self.saved_revision = self.revision;
        self.view_state_changed = false;
    }

    /// Treat the project as not saved (one restored from a recovery copy,
    /// whose changes never reached its file).
    pub fn mark_unsaved(&mut self) {
        self.saved_revision = 0;
    }

    /// Run `f` as one undoable transaction named `name`.
    pub fn edit<R>(&mut self, name: impl Into<String>, f: impl FnOnce(&mut EditCtx) -> R) -> R {
        self.edit_impl(name.into(), None, f)
    }

    /// Like [`Document::edit`], but consecutive edits with the same `merge_key`
    /// become a single undo step (e.g. while dragging a value).
    pub fn edit_merged<R>(&mut self, name: impl Into<String>, merge_key: u64, f: impl FnOnce(&mut EditCtx) -> R) -> R {
        self.edit_impl(name.into(), Some(merge_key), f)
    }

    /// End coalescing so the next merged edit starts a new undo step.
    /// Number of undoable transactions.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn break_merge(&mut self) {
        self.last_merge_key = None;
    }

    fn edit_impl<R>(&mut self, name: String, merge_key: Option<u64>, f: impl FnOnce(&mut EditCtx) -> R) -> R {
        let mut ctx = EditCtx {
            project: &mut self.project,
            tx: Transaction { name, ..Default::default() },
            creation_phase: self.creation_phase,
        };
        let result = f(&mut ctx);
        let tx = ctx.finish();
        if !tx.is_empty() {
            match self.undo.last_mut() {
                Some(last) if merge_key.is_some() && merge_key == self.last_merge_key && self.redo.is_empty() => {
                    last.merge(tx);
                }
                _ => self.undo.push(tx),
            }
            self.last_merge_key = merge_key;
            self.redo.clear();
            self.revision += 1;
        }
        result
    }

    /// Like [`Document::edit`], but the change is kept only if `f` succeeds
    /// and `check` accepts the resulting project; otherwise the project is
    /// restored and nothing is recorded (no undo step, redo kept).
    pub fn try_edit<R, E>(
        &mut self,
        name: impl Into<String>,
        f: impl FnOnce(&mut EditCtx) -> Result<R, E>,
        check: impl FnOnce(&Project) -> Result<(), E>,
    ) -> Result<R, E> {
        self.try_edit_merged(name, None, f, check)
    }

    /// [`Document::try_edit`] that merges into the previous step when
    /// `merge_key` matches its key (like [`Document::edit_merged`]).
    pub fn try_edit_merged<R, E>(
        &mut self,
        name: impl Into<String>,
        merge_key: Option<u64>,
        f: impl FnOnce(&mut EditCtx) -> Result<R, E>,
        check: impl FnOnce(&Project) -> Result<(), E>,
    ) -> Result<R, E> {
        let mut ctx = EditCtx {
            project: &mut self.project,
            tx: Transaction { name: name.into(), ..Default::default() },
            creation_phase: self.creation_phase,
        };
        let result = f(&mut ctx);
        let tx = ctx.finish();
        let outcome = result.and_then(|r| check(&self.project).map(|()| r));
        if outcome.is_err() {
            tx.apply(&mut self.project, false);
            return outcome;
        }
        if !tx.is_empty() {
            match self.undo.last_mut() {
                Some(last) if merge_key.is_some() && merge_key == self.last_merge_key && self.redo.is_empty() => {
                    last.merge(tx);
                }
                _ => self.undo.push(tx),
            }
            self.last_merge_key = merge_key;
            self.redo.clear();
            self.revision += 1;
        }
        outcome
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_name(&self) -> Option<&str> {
        self.undo.last().map(|t| t.name.as_str())
    }

    pub fn redo_name(&self) -> Option<&str> {
        self.redo.last().map(|t| t.name.as_str())
    }

    pub fn undo(&mut self) -> bool {
        let Some(tx) = self.undo.pop() else { return false };
        tx.apply(&mut self.project, false);
        self.redo.push(tx);
        self.last_merge_key = None;
        self.revision += 1;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(tx) = self.redo.pop() else { return false };
        tx.apply(&mut self.project, true);
        self.undo.push(tx);
        self.last_merge_key = None;
        self.revision += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Category, ElementKind, Hosted, Wall};
    use glam::DVec2;

    #[test]
    fn rejected_edits_leave_no_trace() {
        let mut doc = Document::new(Project::from_template());
        let before = doc.project.clone();
        let (depth, rev) = (doc.undo_depth(), doc.revision());
        let failed: Result<(), &str> = doc.try_edit(
            "t",
            |e| {
                e.info_mut().name = "changed".into();
                Err("no")
            },
            |_| Ok(()),
        );
        assert!(failed.is_err());
        let refused: Result<(), &str> = doc.try_edit(
            "t",
            |e| {
                e.info_mut().name = "changed".into();
                Ok(())
            },
            |p| if p.info.name == "changed" { Err("refused") } else { Ok(()) },
        );
        assert!(refused.is_err());
        assert_eq!(doc.project, before);
        assert_eq!((doc.undo_depth(), doc.revision()), (depth, rev));
        let ok: Result<(), &str> = doc.try_edit(
            "t",
            |e| {
                e.info_mut().name = "kept".into();
                Ok(())
            },
            |_| Ok(()),
        );
        assert!(ok.is_ok() && doc.can_undo() && doc.project.info.name == "kept");
    }

    fn add_wall(doc: &mut Document) -> ElementId {
        doc.edit("wall", |e| {
            let level = e.project().levels[0].id;
            let type_id = e.project().default_type(Category::Wall).unwrap();
            let id = ElementId(e.alloc_id());
            e.add_element(Element {
                id,
                level,
                type_id,
                mark: String::new(),
                comments: String::new(),
                uid: Default::default(),
                kind: ElementKind::Wall(Wall {
                    start: DVec2::ZERO,
                    end: DVec2::new(5.0, 0.0),
                    base_offset: 0.0,
                    height: 3.0,
                    ..Default::default()
                }),
                phase_created: Default::default(),
                phase_demolished: None,
            })
        })
    }

    #[test]
    fn undo_redo_create_and_modify() {
        let mut doc = Document::new(Project::from_template());
        let id = add_wall(&mut doc);
        doc.edit("move", |e| e.element_mut(id).unwrap().translate(DVec2::new(1.0, 0.0)));
        let wall = |d: &Document| d.project.element(id).and_then(|e| e.as_wall().cloned());
        assert_eq!(wall(&doc).unwrap().start, DVec2::new(1.0, 0.0));
        assert!(doc.undo());
        assert_eq!(wall(&doc).unwrap().start, DVec2::ZERO);
        assert!(doc.undo());
        assert!(wall(&doc).is_none());
        assert!(doc.redo());
        assert!(doc.redo());
        assert_eq!(wall(&doc).unwrap().start, DVec2::new(1.0, 0.0));
        assert!(!doc.redo());
    }

    #[test]
    fn deleting_wall_deletes_hosted_and_undo_restores() {
        let mut doc = Document::new(Project::from_template());
        let wall = add_wall(&mut doc);
        let door = doc.edit("door", |e| {
            let level = e.project().levels[0].id;
            let type_id = e.project().default_type(Category::Door).unwrap();
            let id = ElementId(e.alloc_id());
            e.add_element(Element {
                id,
                level,
                type_id,
                mark: "1".into(),
                comments: String::new(),
                uid: Default::default(),
                kind: ElementKind::Door(Hosted {
                    host: wall,
                    offset: 2.0,
                    sill_height: 0.0,
                    flip_facing: false,
                    flip_hand: false,
                }),
                phase_created: Default::default(),
                phase_demolished: None,
            })
        });
        doc.edit("delete", |e| e.remove_element(wall));
        assert!(doc.project.element(door).is_none());
        doc.undo();
        assert!(doc.project.element(door).is_some());
        assert!(doc.project.element(wall).is_some());
    }

    /// A crafted file can make doors host each other (or themselves); removing
    /// one must not recurse forever.
    #[test]
    fn removing_cyclic_hosted_elements_terminates() {
        let mut doc = Document::new(Project::from_template());
        let (a, b) = doc.edit("cycle", |e| {
            let level = e.project().levels[0].id;
            let type_id = e.project().default_type(Category::Door).unwrap();
            let (a, b) = (ElementId(e.alloc_id()), ElementId(e.alloc_id()));
            for (id, host) in [(a, b), (b, a)] {
                let kind = ElementKind::Door(Hosted {
                    host,
                    offset: 1.0,
                    sill_height: 0.0,
                    flip_facing: false,
                    flip_hand: false,
                });
                e.add_element(Element {
                    id,
                    level,
                    type_id,
                    mark: String::new(),
                    comments: String::new(),
                    kind,
                    uid: Default::default(),
                    phase_created: Default::default(),
                    phase_demolished: None,
                });
            }
            let selfie = ElementId(e.alloc_id());
            let kind = ElementKind::Door(Hosted {
                host: selfie,
                offset: 1.0,
                sill_height: 0.0,
                flip_facing: false,
                flip_hand: false,
            });
            e.add_element(Element {
                id: selfie,
                level,
                type_id,
                mark: String::new(),
                comments: String::new(),
                kind,
                uid: Default::default(),
                phase_created: Default::default(),
                phase_demolished: None,
            });
            e.remove_element(selfie);
            (a, b)
        });
        doc.edit("delete", |e| e.remove_element(a));
        assert!(doc.project.element(a).is_none() && doc.project.element(b).is_none());
    }

    #[test]
    fn merged_edits_undo_together() {
        let mut doc = Document::new(Project::from_template());
        let id = add_wall(&mut doc);
        for _ in 0..5 {
            doc.edit_merged("drag", 7, |e| e.element_mut(id).unwrap().translate(DVec2::X));
        }
        let start = |d: &Document| d.project.element(id).unwrap().as_wall().unwrap().start;
        assert_eq!(start(&doc), DVec2::new(5.0, 0.0));
        doc.undo();
        assert_eq!(start(&doc), DVec2::ZERO);
        assert!(doc.can_undo(), "wall creation is still undoable");
    }

    #[test]
    fn noop_edit_is_not_recorded() {
        let mut doc = Document::new(Project::from_template());
        let rev = doc.revision();
        doc.edit("nothing", |e| {
            let _ = e.info_mut();
        });
        assert_eq!(doc.revision(), rev);
        assert!(!doc.can_undo());
    }

    #[test]
    fn renaming_a_material_updates_types_in_one_step() {
        let mut doc = Document::new(Project::from_template());
        let old = "Madeira";
        let users = doc.project.material_users(old);
        assert!(!users.is_empty());
        assert!(doc.edit("rename", |e| e.rename_material(old, "Carvalho")));
        assert!(doc.project.material(old).is_none());
        assert_eq!(doc.project.material_users("Carvalho"), users);
        // Taken or empty names are refused.
        assert!(!doc.edit("rename", |e| e.rename_material("Carvalho", "Concreto")));
        assert!(!doc.edit("rename", |e| e.rename_material("Carvalho", "  ")));
        doc.undo();
        assert_eq!(doc.project.material_users(old), users);
        assert!(doc.project.material("Carvalho").is_none());
    }
}
