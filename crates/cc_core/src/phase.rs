//! Construction phases (renovation work).
//!
//! The project holds an ordered list of [`Phase`]s (the template has
//! "Existing" and "New Construction"). Every model element is created in a
//! phase and may be demolished in the same or a later one. Each model view
//! shows the model as of one phase, through a [`PhaseFilter`]: relative to
//! the view's phase an element is
//!
//! * **Existing** — created in an earlier phase and not demolished yet;
//! * **Demolished** — created earlier and demolished in this phase;
//! * **New** — created in this phase (and not demolished in it);
//! * **Temporary** — created and demolished in this phase.
//!
//! Elements created in a later phase or demolished in an earlier one are
//! not in the view at all. The filter maps each status to not displayed,
//! displayed by category, or displayed with the project's
//! [`PhaseGraphics`] for that status (which combine with view graphics
//! below element overrides and view filters, above category overrides).
//!
//! Hosted elements follow their hosts (see [`Project::phase_fixes`]): a door
//! or window (or a rebar set) is never created before its host nor survives
//! its host's demolition; it can be demolished earlier on its own.
//! Annotation categories (dimensions, text, grids) have no phase.

use crate::{Category, Element, ElementId, ElementKind, FillGraphics, GraphicOverride, LineGraphics, Pattern, Project};
use crate::{PhaseFilterId, PhaseId};
use serde::{Deserialize, Serialize};

/// A construction phase. The project keeps them in time order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Phase {
    pub id: PhaseId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// An element's status relative to a view's phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PhaseStatus {
    Existing,
    Demolished,
    New,
    Temporary,
}

impl PhaseStatus {
    pub const ALL: [PhaseStatus; 4] =
        [PhaseStatus::Existing, PhaseStatus::Demolished, PhaseStatus::New, PhaseStatus::Temporary];

    /// Translation key of the status name.
    pub fn key(self) -> &'static str {
        match self {
            PhaseStatus::Existing => "phase-status-existing",
            PhaseStatus::Demolished => "phase-status-demolished",
            PhaseStatus::New => "phase-status-new",
            PhaseStatus::Temporary => "phase-status-temporary",
        }
    }

    /// The IFC4 `Status` enumeration value (`Pset_*Common`).
    pub fn ifc(self) -> &'static str {
        match self {
            PhaseStatus::Existing => "EXISTING",
            PhaseStatus::Demolished => "DEMOLISH",
            PhaseStatus::New => "NEW",
            PhaseStatus::Temporary => "TEMPORARY",
        }
    }

    /// Read an IFC `Status` value (case-insensitive).
    pub fn from_ifc(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "EXISTING" => Some(PhaseStatus::Existing),
            "DEMOLISH" | "DEMOLISHED" => Some(PhaseStatus::Demolished),
            "NEW" => Some(PhaseStatus::New),
            "TEMPORARY" => Some(PhaseStatus::Temporary),
            _ => None,
        }
    }
}

/// Where an element stands in a view's phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseState {
    /// Annotations: shown in every phase, never overridden.
    Unphased,
    /// Created later or demolished earlier: not part of the view.
    Absent,
    Status(PhaseStatus),
}

/// How a phase filter shows elements of one status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PhaseDisplay {
    NotDisplayed,
    /// With the view's graphics only.
    #[default]
    ByCategory,
    /// With the project's graphics for the status.
    Overridden,
}

impl PhaseDisplay {
    pub const ALL: [PhaseDisplay; 3] = [PhaseDisplay::NotDisplayed, PhaseDisplay::ByCategory, PhaseDisplay::Overridden];

    pub fn key(self) -> &'static str {
        match self {
            PhaseDisplay::NotDisplayed => "phase-display-not-displayed",
            PhaseDisplay::ByCategory => "phase-display-by-category",
            PhaseDisplay::Overridden => "phase-display-overridden",
        }
    }
}

/// A named rule for which statuses a view shows, and how.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaseFilter {
    pub id: PhaseFilterId,
    pub name: String,
    #[serde(default)]
    pub existing: PhaseDisplay,
    #[serde(default)]
    pub demolished: PhaseDisplay,
    #[serde(default)]
    pub new: PhaseDisplay,
    #[serde(default)]
    pub temporary: PhaseDisplay,
}

impl PhaseFilter {
    pub fn display(&self, status: PhaseStatus) -> PhaseDisplay {
        match status {
            PhaseStatus::Existing => self.existing,
            PhaseStatus::Demolished => self.demolished,
            PhaseStatus::New => self.new,
            PhaseStatus::Temporary => self.temporary,
        }
    }

    pub fn set_display(&mut self, status: PhaseStatus, display: PhaseDisplay) {
        match status {
            PhaseStatus::Existing => self.existing = display,
            PhaseStatus::Demolished => self.demolished = display,
            PhaseStatus::New => self.new = display,
            PhaseStatus::Temporary => self.temporary = display,
        }
    }

    /// Whether the filter shows everything, overriding all but new elements
    /// (the default filter of new views).
    pub fn is_show_all(&self) -> bool {
        use PhaseDisplay::*;
        (self.existing, self.demolished, self.new, self.temporary) == (Overridden, Overridden, ByCategory, Overridden)
    }
}

/// Graphic overrides of each status (used where a filter says "Overridden").
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PhaseGraphics {
    pub existing: GraphicOverride,
    pub demolished: GraphicOverride,
    pub new: GraphicOverride,
    pub temporary: GraphicOverride,
}

impl Default for PhaseGraphics {
    /// Existing: faint; demolished: dashed, without cut pattern; new: as
    /// drawn; temporary: faint and dashed.
    fn default() -> Self {
        let dashed = LineGraphics { pattern: Some(crate::LinePattern::Dash), ..Default::default() };
        let short = LineGraphics { pattern: Some(crate::LinePattern::ShortDash), ..Default::default() };
        let blank = FillGraphics { color: None, pattern: Some(Pattern::None) };
        Self {
            existing: GraphicOverride { halftone: Some(true), ..Default::default() },
            demolished: GraphicOverride {
                projection_lines: dashed,
                cut_lines: dashed,
                cut_fill: blank,
                ..Default::default()
            },
            new: GraphicOverride::default(),
            temporary: GraphicOverride {
                projection_lines: short,
                cut_lines: short,
                cut_fill: blank,
                halftone: Some(true),
                ..Default::default()
            },
        }
    }
}

impl PhaseGraphics {
    pub fn get(&self, status: PhaseStatus) -> &GraphicOverride {
        match status {
            PhaseStatus::Existing => &self.existing,
            PhaseStatus::Demolished => &self.demolished,
            PhaseStatus::New => &self.new,
            PhaseStatus::Temporary => &self.temporary,
        }
    }

    pub fn get_mut(&mut self, status: PhaseStatus) -> &mut GraphicOverride {
        match status {
            PhaseStatus::Existing => &mut self.existing,
            PhaseStatus::Demolished => &mut self.demolished,
            PhaseStatus::New => &mut self.new,
            PhaseStatus::Temporary => &mut self.temporary,
        }
    }

    fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Reset invalid values; returns how many.
    pub fn repair(&mut self) -> usize {
        PhaseStatus::ALL.iter().map(|s| self.get_mut(*s).repair()).sum()
    }
}

pub(crate) fn phase_graphics_is_default(g: &PhaseGraphics) -> bool {
    g.is_default()
}

/// The template's phase filters: name key and displays of existing,
/// demolished, new and temporary elements.
pub const TEMPLATE_FILTERS: [(&str, [PhaseDisplay; 4]); 7] = {
    use PhaseDisplay::{ByCategory as C, NotDisplayed as N, Overridden as O};
    [
        ("tpl-phase-filter-all", [O, O, C, O]),
        ("tpl-phase-filter-complete", [C, N, C, N]),
        ("tpl-phase-filter-new", [N, N, C, N]),
        ("tpl-phase-filter-previous-demo", [O, O, N, N]),
        ("tpl-phase-filter-previous-new", [O, N, C, N]),
        ("tpl-phase-filter-previous", [O, N, N, N]),
        ("tpl-phase-filter-demo-new", [N, O, C, O]),
    ]
};

/// The template's phases (name keys), in order.
pub const TEMPLATE_PHASES: [&str; 2] = ["tpl-phase-existing", "tpl-phase-new"];

impl Category {
    /// Whether elements of the category belong to phases (annotations do not).
    pub fn is_phased(self) -> bool {
        !matches!(
            self,
            Category::Dimension | Category::Text | Category::Grid | Category::CadImport | Category::TitleBlock
        )
    }
}

/// The element that decides an element's phases: the wall of a door or
/// window, the beam or column of a rebar set, the stair of a railing.
pub fn phase_host(e: &Element) -> Option<ElementId> {
    match &e.kind {
        ElementKind::Door(h) | ElementKind::Window(h) => Some(h.host),
        ElementKind::Rebar(r) => Some(r.host),
        ElementKind::Pad(p) => Some(p.host),
        ElementKind::Railing(r) => r.stair.map(|h| h.host),
        _ => None,
    }
}

impl Project {
    /// Position of a phase in time order.
    pub fn phase_index(&self, id: PhaseId) -> Option<usize> {
        self.phases.iter().position(|p| p.id == id)
    }

    pub fn phase(&self, id: PhaseId) -> Option<&Phase> {
        self.phases.iter().find(|p| p.id == id)
    }

    pub fn phase_name(&self, id: PhaseId) -> String {
        self.phase(id).map(|p| p.name.clone()).unwrap_or_default()
    }

    /// The latest phase (where new elements and views go by default).
    pub fn last_phase(&self) -> Option<PhaseId> {
        self.phases.last().map(|p| p.id)
    }

    pub fn phase_filter(&self, id: PhaseFilterId) -> Option<&PhaseFilter> {
        self.phase_filters.get(&id)
    }

    /// The filter new views get: the first that shows everything.
    pub fn default_phase_filter(&self) -> Option<PhaseFilterId> {
        self.phase_filters.values().find(|f| f.is_show_all()).map(|f| f.id)
    }

    /// Index of a phase reference; unassigned or unknown ids mean the last phase.
    fn resolved_index(&self, id: PhaseId) -> usize {
        self.phase_index(id).unwrap_or(self.phases.len().saturating_sub(1))
    }

    /// The phase an element is created in, as an index (see [`Self::resolved_index`]).
    pub fn created_index(&self, e: &Element) -> usize {
        self.resolved_index(e.phase_created)
    }

    /// The phase an element is demolished in, as an index (`None`: standing).
    pub fn demolished_index(&self, e: &Element) -> Option<usize> {
        e.phase_demolished.and_then(|d| self.phase_index(d))
    }

    /// The phase a view shows (its own, or the last one).
    pub fn view_phase(&self, view: &crate::View) -> Option<PhaseId> {
        self.phase(view.phase).map(|p| p.id).or_else(|| self.last_phase())
    }

    /// Status of `e` in phase number `phase` (an index into [`Project::phases`]).
    pub fn phase_state_at(&self, e: &Element, phase: usize) -> PhaseState {
        if !e.category().is_phased() {
            return PhaseState::Unphased;
        }
        let created = self.created_index(e);
        let demolished = self.demolished_index(e);
        if created > phase || demolished.is_some_and(|d| d < phase) {
            return PhaseState::Absent;
        }
        PhaseState::Status(match (created == phase, demolished == Some(phase)) {
            (true, true) => PhaseStatus::Temporary,
            (true, false) => PhaseStatus::New,
            (false, true) => PhaseStatus::Demolished,
            (false, false) => PhaseStatus::Existing,
        })
    }

    /// Status of `e` in phase `phase` (unknown phases mean the last one).
    pub fn phase_state(&self, e: &Element, phase: PhaseId) -> PhaseState {
        self.phase_state_at(e, self.resolved_index(phase))
    }

    /// Whether `e` stands at the end of phase number `phase`: created then
    /// or before and not demolished by then (annotations always).
    pub fn stands_after(&self, e: &Element, phase: usize) -> bool {
        !e.category().is_phased()
            || (self.created_index(e) <= phase && self.demolished_index(e).is_none_or(|d| d > phase))
    }

    /// Whether `e` is part of phase number `phase` at all (standing at its
    /// start or built in it).
    pub fn present_at(&self, e: &Element, phase: usize) -> bool {
        !matches!(self.phase_state_at(e, phase), PhaseState::Absent)
    }

    /// Whether any element is absent from phase number `phase` (its views
    /// then need geometry of their own).
    pub fn phase_differs(&self, phase: usize) -> bool {
        self.elements.values().any(|e| !self.present_at(e, phase))
    }

    /// The phases that would make elements consistent: a known created
    /// phase (unassigned stays: it means the last), a demolition not before
    /// creation (else none), hosted elements not created before their host
    /// nor demolished after it, annotations without phases. Returns
    /// `(element, created, demolished)` for each element that needs a
    /// change; empty when the project has no phases.
    pub fn phase_fixes(&self) -> Vec<(ElementId, PhaseId, Option<PhaseId>)> {
        let Some(last) = self.last_phase() else { return Vec::new() };
        let index = |id: PhaseId| self.phase_index(id);
        let phase_at = |i: usize| self.phases.get(i).map(|p| p.id).unwrap_or(last);
        // An element's own phases as indices: created, demolished (not before).
        let own = |e: &Element| {
            let c = self.created_index(e);
            (c, e.phase_demolished.and_then(index).filter(|d| *d >= c))
        };
        let mut out = Vec::new();
        for e in self.elements.values() {
            let assigned = e.phase_created.0 == 0 || index(e.phase_created).is_some();
            let (mut c, mut d) = own(e);
            if !e.category().is_phased() {
                d = None;
            } else if let Some(host) = phase_host(e).and_then(|h| self.element(h)) {
                let (hc, hd) = own(host);
                c = c.max(hc);
                if let Some(hd) = hd {
                    d = Some(d.map_or(hd, |d| d.min(hd)));
                    c = c.min(hd);
                }
                d = d.map(|d| d.max(c));
            }
            let created = if assigned && c == self.created_index(e) { e.phase_created } else { phase_at(c) };
            let demolished = d.map(phase_at);
            if created != e.phase_created || demolished != e.phase_demolished {
                out.push((e.id, created, demolished));
            }
        }
        out
    }

    /// Give a project without phases the template's (named by `name(key)`),
    /// its phase filters, and put every element and view in the last phase
    /// with the "show all" filter. Used for files made before phases.
    pub fn ensure_phases(&mut self, name: &dyn Fn(&str) -> String) {
        if self.phases.is_empty() {
            for key in TEMPLATE_PHASES {
                let id = PhaseId(self.alloc_id());
                self.phases.push(Phase { id, name: name(key), description: String::new() });
            }
        }
        if self.phase_filters.is_empty() {
            for (key, [existing, demolished, new, temporary]) in TEMPLATE_FILTERS {
                let id = PhaseFilterId(self.alloc_id());
                self.phase_filters
                    .insert(id, PhaseFilter { id, name: name(key), existing, demolished, new, temporary });
            }
        }
        let last = self.last_phase().unwrap_or(PhaseId(0));
        let filter = self.default_phase_filter();
        for e in self.elements.values_mut() {
            if !self.phases.iter().any(|p| p.id == e.phase_created) {
                e.phase_created = last;
            }
        }
        for v in self.views.values_mut() {
            if !self.phases.iter().any(|p| p.id == v.phase) {
                v.phase = last;
                if v.phase_filter.is_none() {
                    v.phase_filter = filter;
                }
            }
        }
        for t in self.templates.values_mut() {
            if t.phase_filter.is_none() {
                t.phase_filter = filter;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, ElementTypeId, LevelId, Wall};
    use glam::DVec2;

    fn wall(p: &mut Project, created: PhaseId, demolished: Option<PhaseId>) -> ElementId {
        let id = ElementId(p.alloc_id());
        let mut e = Element {
            id,
            level: LevelId(1),
            type_id: ElementTypeId(1),
            mark: String::new(),
            comments: String::new(),
            kind: ElementKind::Wall(Wall { start: DVec2::ZERO, end: DVec2::X, ..Default::default() }),
            uid: Default::default(),
            phase_created: created,
            phase_demolished: demolished,
        };
        e.uid = crate::Uid::generate();
        p.elements.insert(id, e);
        id
    }

    #[test]
    fn status_relative_to_the_view_phase() {
        let mut p = Project::empty();
        p.ensure_phases(&|k| k.to_string());
        let ids: Vec<PhaseId> = p.phases.iter().map(|x| x.id).collect();
        let third = PhaseId(p.alloc_id());
        p.phases.push(Phase { id: third, name: "3".into(), description: String::new() });
        let (a, b, c) = (ids[0], ids[1], third);
        let kept = wall(&mut p, a, None);
        let razed = wall(&mut p, a, Some(b));
        let temp = wall(&mut p, b, Some(b));
        let new = wall(&mut p, b, None);
        let st = |id: ElementId, ph: PhaseId| p.phase_state(p.element(id).unwrap(), ph);
        use PhaseState::{Absent, Status};
        use PhaseStatus::*;
        assert_eq!(st(kept, a), Status(New));
        assert_eq!(st(kept, b), Status(Existing));
        assert_eq!(st(razed, a), Status(New));
        assert_eq!(st(razed, b), Status(Demolished));
        assert_eq!(st(razed, c), Absent);
        assert_eq!(st(temp, a), Absent);
        assert_eq!(st(temp, b), Status(Temporary));
        assert_eq!(st(temp, c), Absent);
        assert_eq!(st(new, a), Absent);
        assert_eq!(st(new, c), Status(Existing));
        assert!(p.phase_differs(0) && p.phase_differs(2) && !p.phase_differs(1));
        assert!(p.stands_after(p.element(kept).unwrap(), 2));
        assert!(!p.stands_after(p.element(razed).unwrap(), 1));
    }

    #[test]
    fn template_filters_and_graphics() {
        let mut p = Project::empty();
        p.ensure_phases(&|k| k.to_string());
        assert_eq!(p.phases.len(), 2);
        assert_eq!(p.phase_filters.len(), 7);
        let all = p.default_phase_filter().and_then(|f| p.phase_filter(f)).unwrap();
        assert_eq!(all.name, "tpl-phase-filter-all");
        assert_eq!(all.display(PhaseStatus::New), PhaseDisplay::ByCategory);
        let g = PhaseGraphics::default();
        assert_eq!(g.existing.halftone, Some(true));
        assert_eq!(g.demolished.cut_lines.pattern, Some(crate::LinePattern::Dash));
        assert!(g.new.is_empty());
        let doc = Document::new(p);
        assert_eq!(doc.project.clone().repair(), 0);
    }

    #[test]
    fn hosted_follow_hosts() {
        let mut p = Project::empty();
        p.ensure_phases(&|k| k.to_string());
        let (a, b) = (p.phases[0].id, p.phases[1].id);
        let w = wall(&mut p, a, Some(b));
        let id = ElementId(p.alloc_id());
        let host = crate::Hosted { host: w, offset: 0.5, sill_height: 0.0, flip_hand: false, flip_facing: false };
        let mut door = p.element(w).unwrap().clone();
        door.id = id;
        door.kind = ElementKind::Door(host);
        door.phase_demolished = None;
        door.phase_created = a;
        p.elements.insert(id, door);
        // The wall goes in phase b: so does its door.
        assert_eq!(p.phase_fixes(), vec![(id, a, Some(b))]);
        // A door demolished earlier than the wall keeps its own demolition.
        p.elements.get_mut(&id).unwrap().phase_demolished = Some(a);
        assert!(p.phase_fixes().is_empty());
        // A door is not older than its wall.
        p.elements.get_mut(&w).unwrap().phase_created = b;
        p.elements.get_mut(&id).unwrap().phase_demolished = None;
        assert_eq!(p.phase_fixes(), vec![(id, b, Some(b))]);
    }
}
