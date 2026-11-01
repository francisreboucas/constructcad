//! View graphics: what a view shows and how it draws it.
//!
//! Each view holds a [`ViewGraphics`]: per-category visibility and graphic
//! overrides, the view filters applied to it (in order) and per-element
//! overrides and hidden elements. The project holds the named
//! [`ViewFilter`]s (categories plus rules on parameters) and the
//! [`ViewTemplate`]s (sets of view properties a view can follow).
//!
//! Overrides combine attribute by attribute: an element's own override
//! beats the filters, the first matching filter beats later ones, and the
//! filters beat the element's phase status overrides (see [`crate::phase`]),
//! which beat the category. An element is hidden when it is hidden in the
//! view, its category is hidden, or any matching filter hides it.

use crate::{Category, ElementId, FilterId, MepDiscipline, Pattern, ScheduleField, TemplateId, View, ViewKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Dash pattern of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum LinePattern {
    #[default]
    Solid,
    Dash,
    /// Short dashes (hidden lines).
    ShortDash,
    Dot,
    DashDot,
}

impl LinePattern {
    pub const ALL: [LinePattern; 5] =
        [LinePattern::Solid, LinePattern::Dash, LinePattern::ShortDash, LinePattern::Dot, LinePattern::DashDot];

    pub fn key(self) -> &'static str {
        match self {
            LinePattern::Solid => "linepattern-solid",
            LinePattern::Dash => "linepattern-dash",
            LinePattern::ShortDash => "linepattern-short-dash",
            LinePattern::Dot => "linepattern-dot",
            LinePattern::DashDot => "linepattern-dash-dot",
        }
    }

    /// Dash and gap lengths in paper millimeters (empty: continuous).
    pub fn dashes_mm(self) -> &'static [f64] {
        match self {
            LinePattern::Solid => &[],
            LinePattern::Dash => &[3.0, 1.5],
            LinePattern::ShortDash => &[1.5, 1.0],
            LinePattern::Dot => &[0.3, 0.9],
            LinePattern::DashDot => &[6.0, 1.0, 1.0, 1.0],
        }
    }
}

/// Overrides of a kind of line; `None` keeps what the view draws.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LineGraphics {
    /// RGB color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
    /// Pen width on paper, millimeters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<LinePattern>,
}

impl LineGraphics {
    pub fn is_empty(&self) -> bool {
        self.color.is_none() && self.weight.is_none() && self.pattern.is_none()
    }

    /// Attribute by attribute, these values or else `lower`'s.
    pub fn or(&self, lower: &Self) -> Self {
        Self {
            color: self.color.or(lower.color),
            weight: self.weight.or(lower.weight),
            pattern: self.pattern.or(lower.pattern),
        }
    }
}

/// Overrides of a kind of fill; `None` keeps what the view draws.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FillGraphics {
    /// RGB color (of the pattern lines, or of a solid fill).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
    /// `None` hides the fill, `Solid` fills it with the color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<Pattern>,
}

impl FillGraphics {
    pub fn is_empty(&self) -> bool {
        self.color.is_none() && self.pattern.is_none()
    }

    pub fn or(&self, lower: &Self) -> Self {
        Self { color: self.color.or(lower.color), pattern: self.pattern.or(lower.pattern) }
    }
}

/// Graphic overrides of a category, filter or element in a view. Cut
/// graphics apply where the view plane cuts the element (plans, sections),
/// projection and surface graphics to what is seen beyond it.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphicOverride {
    #[serde(skip_serializing_if = "LineGraphics::is_empty")]
    pub projection_lines: LineGraphics,
    #[serde(skip_serializing_if = "LineGraphics::is_empty")]
    pub cut_lines: LineGraphics,
    #[serde(skip_serializing_if = "FillGraphics::is_empty")]
    pub surface_fill: FillGraphics,
    #[serde(skip_serializing_if = "FillGraphics::is_empty")]
    pub cut_fill: FillGraphics,
    /// Draw faint (lines and fills mixed with white).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub halftone: Option<bool>,
    /// Surface transparency, 0 (opaque) to 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transparency: Option<f64>,
}

impl GraphicOverride {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Attribute by attribute, these values or else `lower`'s.
    pub fn or(&self, lower: &Self) -> Self {
        Self {
            projection_lines: self.projection_lines.or(&lower.projection_lines),
            cut_lines: self.cut_lines.or(&lower.cut_lines),
            surface_fill: self.surface_fill.or(&lower.surface_fill),
            cut_fill: self.cut_fill.or(&lower.cut_fill),
            halftone: self.halftone.or(lower.halftone),
            transparency: self.transparency.or(lower.transparency),
        }
    }

    /// Reset values that are not valid numbers or out of range; returns how many.
    pub fn repair(&mut self) -> usize {
        let mut n = 0;
        for lines in [&mut self.projection_lines, &mut self.cut_lines] {
            if lines.weight.is_some_and(|w| !(w.is_finite() && w > 0.0 && w <= 10.0)) {
                lines.weight = None;
                n += 1;
            }
        }
        if self.transparency.is_some_and(|t| !(0.0..=1.0).contains(&t)) {
            self.transparency = None;
            n += 1;
        }
        n
    }
}

fn yes() -> bool {
    true
}

fn is_true(v: &bool) -> bool {
    *v
}

/// Visibility and overrides of a category in a view.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CategoryGraphics {
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub visible: bool,
    #[serde(default, skip_serializing_if = "GraphicOverride::is_empty")]
    pub overrides: GraphicOverride,
}

impl Default for CategoryGraphics {
    fn default() -> Self {
        Self { visible: true, overrides: GraphicOverride::default() }
    }
}

impl CategoryGraphics {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A filter applied to a view: elements it matches are hidden or drawn
/// with its overrides.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FilterUse {
    pub filter: FilterId,
    /// Matching elements are shown (`false` hides them).
    #[serde(default = "yes")]
    pub visible: bool,
    /// The filter is applied (`false` keeps it listed without effect).
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "GraphicOverride::is_empty")]
    pub overrides: GraphicOverride,
}

impl FilterUse {
    pub fn new(filter: FilterId) -> Self {
        Self { filter, visible: true, enabled: true, overrides: GraphicOverride::default() }
    }
}

/// What a view shows and how: category visibility and overrides, applied
/// filters, and element overrides and hidden elements.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewGraphics {
    /// Categories with a non-default visibility or overrides.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub categories: BTreeMap<Category, CategoryGraphics>,
    /// Filters in priority order (the first matching filter wins per attribute).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<FilterUse>,
    /// Overrides of single elements in this view.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub elements: BTreeMap<ElementId, GraphicOverride>,
    /// Elements hidden in this view.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub hidden: BTreeSet<ElementId>,
}

impl ViewGraphics {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn category(&self, c: Category) -> CategoryGraphics {
        self.categories.get(&c).copied().unwrap_or_default()
    }

    /// Set a category's graphics (dropping the entry when it is the default).
    pub fn set_category(&mut self, c: Category, g: CategoryGraphics) {
        if g.is_default() {
            self.categories.remove(&c);
        } else {
            self.categories.insert(c, g);
        }
    }

    /// The graphics of an element of `category`, or `None` when it is
    /// hidden; `matches` says whether a filter matches the element.
    pub fn resolve(
        &self,
        id: ElementId,
        category: Category,
        matches: &mut dyn FnMut(FilterId) -> bool,
    ) -> Option<GraphicOverride> {
        self.resolve_phased(id, category, matches, &GraphicOverride::default())
    }

    /// [`Self::resolve`] with the overrides of the element's phase status,
    /// which rank below the filters and above the category.
    pub fn resolve_phased(
        &self,
        id: ElementId,
        category: Category,
        matches: &mut dyn FnMut(FilterId) -> bool,
        phase: &GraphicOverride,
    ) -> Option<GraphicOverride> {
        if self.hidden.contains(&id) {
            return None;
        }
        let cat = self.category(category);
        if !cat.visible {
            return None;
        }
        let mut out = self.elements.get(&id).copied().unwrap_or_default();
        for f in self.filters.iter().filter(|f| f.enabled) {
            if matches(f.filter) {
                if !f.visible {
                    return None;
                }
                out = out.or(&f.overrides);
            }
        }
        Some(out.or(phase).or(&cat.overrides))
    }

    /// Drop references to missing filters and elements and invalid values;
    /// returns how many.
    pub fn repair(
        &mut self,
        filters: &BTreeMap<FilterId, ViewFilter>,
        elements: Option<&dyn Fn(ElementId) -> bool>,
    ) -> usize {
        let mut n = 0;
        let before = self.filters.len();
        let mut seen = BTreeSet::new();
        self.filters.retain(|f| filters.contains_key(&f.filter) && seen.insert(f.filter));
        n += before - self.filters.len();
        if let Some(exists) = elements {
            let before = self.elements.len() + self.hidden.len();
            self.elements.retain(|id, _| exists(*id));
            self.hidden.retain(|id| exists(*id));
            n += before - self.elements.len() - self.hidden.len();
        }
        for c in self.categories.values_mut() {
            n += c.overrides.repair();
        }
        for f in &mut self.filters {
            n += f.overrides.repair();
        }
        for o in self.elements.values_mut() {
            n += o.repair();
        }
        let before = self.categories.len() + self.elements.len();
        self.categories.retain(|_, g| !g.is_default());
        self.elements.retain(|_, o| !o.is_empty());
        n += before - self.categories.len() - self.elements.len();
        n
    }
}

/// How a rule compares an element's parameter with its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    Equals,
    NotEquals,
    Contains,
    NotContains,
    BeginsWith,
    NotBeginsWith,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
    HasValue,
    HasNoValue,
}

impl FilterOp {
    pub const ALL: [FilterOp; 12] = [
        FilterOp::Equals,
        FilterOp::NotEquals,
        FilterOp::Contains,
        FilterOp::NotContains,
        FilterOp::BeginsWith,
        FilterOp::NotBeginsWith,
        FilterOp::Greater,
        FilterOp::GreaterOrEqual,
        FilterOp::Less,
        FilterOp::LessOrEqual,
        FilterOp::HasValue,
        FilterOp::HasNoValue,
    ];

    pub fn key(self) -> &'static str {
        match self {
            FilterOp::Equals => "filterop-equals",
            FilterOp::NotEquals => "filterop-not-equals",
            FilterOp::Contains => "filterop-contains",
            FilterOp::NotContains => "filterop-not-contains",
            FilterOp::BeginsWith => "filterop-begins-with",
            FilterOp::NotBeginsWith => "filterop-not-begins-with",
            FilterOp::Greater => "filterop-greater",
            FilterOp::GreaterOrEqual => "filterop-greater-or-equal",
            FilterOp::Less => "filterop-less",
            FilterOp::LessOrEqual => "filterop-less-or-equal",
            FilterOp::HasValue => "filterop-has-value",
            FilterOp::HasNoValue => "filterop-has-no-value",
        }
    }

    /// Whether the operator takes a value.
    pub fn needs_value(self) -> bool {
        !matches!(self, FilterOp::HasValue | FilterOp::HasNoValue)
    }

    /// Whether `actual` (the element's value as text and, for quantities,
    /// as a number in SI units; `None` when it has none) satisfies the
    /// rule `self value`. Text compares without case; numbers compare
    /// numerically (equality within a millionth) when both sides are numbers.
    pub fn test(self, actual: Option<(&str, Option<f64>)>, value: &str) -> bool {
        let value = value.trim();
        let number = value.replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite());
        let Some((text, num)) = actual else {
            // No value: only the negative tests pass.
            return matches!(
                self,
                FilterOp::HasNoValue | FilterOp::NotEquals | FilterOp::NotContains | FilterOp::NotBeginsWith
            );
        };
        let (t, v) = (text.trim().to_lowercase(), value.to_lowercase());
        let numeric = num.zip(number);
        let eq = match numeric {
            Some((a, b)) => (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0),
            None => t == v,
        };
        let cmp = |f: fn(f64, f64) -> bool| numeric.is_some_and(|(a, b)| f(a, b));
        match self {
            FilterOp::Equals => eq,
            FilterOp::NotEquals => !eq,
            FilterOp::Contains => t.contains(&v),
            FilterOp::NotContains => !t.contains(&v),
            FilterOp::BeginsWith => t.starts_with(&v),
            FilterOp::NotBeginsWith => !t.starts_with(&v),
            FilterOp::Greater => cmp(|a, b| a > b),
            FilterOp::GreaterOrEqual => cmp(|a, b| a >= b - 1e-9),
            FilterOp::Less => cmp(|a, b| a < b),
            FilterOp::LessOrEqual => cmp(|a, b| a <= b + 1e-9),
            FilterOp::HasValue => !t.is_empty(),
            FilterOp::HasNoValue => t.is_empty(),
        }
    }
}

/// A parameter a filter rule tests: any schedule field, or the element's
/// base elevation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterParam {
    /// A field as schedules show it (`Mark`, `Type`, `Width`…).
    Field(ScheduleField),
    /// Elevation of the element's level, meters.
    Elevation,
}

/// One test of a filter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterRule {
    pub param: FilterParam,
    pub op: FilterOp,
    /// Text, or a number in SI units (meters) for quantities.
    #[serde(default)]
    pub value: String,
}

/// A named, project-wide selection of elements by category and rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewFilter {
    pub id: FilterId,
    pub name: String,
    /// Categories the filter applies to (empty: all).
    #[serde(default)]
    pub categories: BTreeSet<Category>,
    #[serde(default)]
    pub rules: Vec<FilterRule>,
    /// Any rule is enough (`false`: all rules must pass).
    #[serde(default)]
    pub any: bool,
}

impl FilterParam {
    /// Whether elements of `category` can have a value for this parameter.
    pub fn applies_to(self, category: Category) -> bool {
        match self {
            FilterParam::Field(f) => f.applies_to(category),
            FilterParam::Elevation => true,
        }
    }
}

impl ViewFilter {
    /// The first rule parameter that applies to none of the filter's
    /// categories, so the rule never passes; `None` when every rule can pass
    /// or the filter covers all categories.
    pub fn inapplicable_param(&self) -> Option<FilterParam> {
        if self.categories.is_empty() {
            return None;
        }
        self.rules.iter().map(|r| r.param).find(|p| !self.categories.iter().any(|c| p.applies_to(*c)))
    }

    /// Whether the filter matches an element of `category`; `rule` tests
    /// one rule against the element.
    pub fn matches(&self, category: Category, rule: &mut dyn FnMut(&FilterRule) -> bool) -> bool {
        if !self.categories.is_empty() && !self.categories.contains(&category) {
            return false;
        }
        if self.rules.is_empty() {
            return true;
        }
        if self.any { self.rules.iter().any(rule) } else { self.rules.iter().all(rule) }
    }
}

/// The view properties a template controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplateIncludes {
    /// The view scale.
    pub scale: bool,
    /// Category visibility and overrides.
    pub categories: bool,
    /// The applied filters and their overrides.
    pub filters: bool,
    /// Plans: the cut plane height.
    pub cut_height: bool,
    /// Floor plans: the building services shown over a faint architecture.
    pub discipline: bool,
    /// Floor plans: structural (formwork) display and structure tags.
    pub structural: bool,
    /// The phase filter (not the phase: views of one template can show
    /// different phases).
    pub phase_filter: bool,
    /// The detail level.
    pub detail: bool,
}

impl Default for TemplateIncludes {
    fn default() -> Self {
        Self {
            scale: true,
            categories: true,
            filters: true,
            cut_height: true,
            discipline: true,
            structural: true,
            phase_filter: true,
            detail: true,
        }
    }
}

/// Named view properties views can copy or follow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewTemplate {
    pub id: TemplateId,
    pub name: String,
    #[serde(default)]
    pub include: TemplateIncludes,
    pub scale: u32,
    /// Category visibility and overrides, and filters (element overrides are not kept).
    #[serde(default)]
    pub graphics: ViewGraphics,
    #[serde(default = "default_cut")]
    pub cut_height: f64,
    #[serde(default)]
    pub discipline: Option<MepDiscipline>,
    #[serde(default)]
    pub structural: bool,
    #[serde(default)]
    pub structure_tags: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_filter: Option<crate::PhaseFilterId>,
    #[serde(default, skip_serializing_if = "crate::DetailLevel::is_medium")]
    pub detail: crate::DetailLevel,
}

fn default_cut() -> f64 {
    1.2
}

impl ViewTemplate {
    /// A template holding the properties of `view`.
    pub fn from_view(id: TemplateId, name: String, view: &View) -> Self {
        let mut graphics = view.graphics.clone();
        graphics.elements.clear();
        graphics.hidden.clear();
        let mut t = Self {
            id,
            name,
            include: TemplateIncludes::default(),
            scale: view.scale,
            graphics,
            cut_height: 1.2,
            discipline: None,
            structural: false,
            structure_tags: false,
            phase_filter: view.phase_filter,
            detail: view.detail,
        };
        match &view.kind {
            ViewKind::FloorPlan { cut_height, structural, structure_tags, mep, .. } => {
                t.cut_height = *cut_height;
                t.structural = *structural;
                t.structure_tags = *structure_tags;
                t.discipline = *mep;
            }
            ViewKind::CeilingPlan { cut_height, .. } => t.cut_height = *cut_height,
            _ => {}
        }
        t
    }

    /// Whether views of `kind` can use templates.
    pub fn applies_to(kind: &ViewKind) -> bool {
        matches!(
            kind,
            ViewKind::FloorPlan { .. }
                | ViewKind::CeilingPlan { .. }
                | ViewKind::ThreeD { .. }
                | ViewKind::Elevation { .. }
                | ViewKind::Section { .. }
        )
    }

    /// Copy the included properties into `view` (those that apply to its kind).
    pub fn apply(&self, view: &mut View) {
        if !Self::applies_to(&view.kind) {
            return;
        }
        let inc = self.include;
        if inc.scale {
            view.scale = self.scale.max(1);
        }
        if inc.categories {
            view.graphics.categories = self.graphics.categories.clone();
        }
        if inc.filters {
            view.graphics.filters = self.graphics.filters.clone();
        }
        if inc.phase_filter {
            view.phase_filter = self.phase_filter;
        }
        if inc.detail {
            view.detail = self.detail;
        }
        match &mut view.kind {
            ViewKind::FloorPlan { cut_height, structural, structure_tags, mep, .. } => {
                if inc.cut_height {
                    *cut_height = self.cut_height;
                }
                if inc.discipline {
                    *mep = self.discipline;
                }
                if inc.structural {
                    *structural = self.structural;
                    *structure_tags = self.structure_tags;
                }
            }
            ViewKind::CeilingPlan { cut_height, .. } if inc.cut_height => *cut_height = self.cut_height,
            _ => {}
        }
    }

    /// Reset invalid values; returns how many.
    pub fn repair(
        &mut self,
        filters: &BTreeMap<FilterId, ViewFilter>,
        phase_filters: &BTreeMap<crate::PhaseFilterId, crate::PhaseFilter>,
    ) -> usize {
        let mut n = self.graphics.repair(filters, Some(&|_| false));
        if self.phase_filter.is_some_and(|f| !phase_filters.contains_key(&f)) {
            self.phase_filter = None;
            n += 1;
        }
        if self.scale == 0 {
            self.scale = 100;
            n += 1;
        }
        if !(self.cut_height.is_finite() && self.cut_height.abs() < 1e4) {
            self.cut_height = 1.2;
            n += 1;
        }
        n
    }
}

/// Elements hidden or isolated for a while in a view (not part of the
/// model: not saved and not undone).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TemporaryVisibility {
    /// `true`: only these elements and categories are shown; `false`: they are hidden.
    pub isolate: bool,
    pub elements: BTreeSet<ElementId>,
    pub categories: BTreeSet<Category>,
}

impl TemporaryVisibility {
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty() && self.categories.is_empty()
    }

    /// Whether an element of `category` is shown.
    pub fn shows(&self, id: ElementId, category: Category) -> bool {
        if self.is_empty() {
            return true;
        }
        let listed = self.elements.contains(&id) || self.categories.contains(&category);
        listed == self.isolate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(color: [u8; 3]) -> LineGraphics {
        LineGraphics { color: Some(color), ..Default::default() }
    }

    #[test]
    fn element_beats_filters_beat_category() {
        let mut g = ViewGraphics::default();
        let cat = CategoryGraphics {
            visible: true,
            overrides: GraphicOverride {
                cut_lines: line([1, 1, 1]),
                projection_lines: line([1, 1, 1]),
                halftone: Some(true),
                ..Default::default()
            },
        };
        g.set_category(Category::Wall, cat);
        let mut f1 = FilterUse::new(FilterId(1));
        f1.overrides.cut_lines = line([2, 2, 2]);
        let mut f2 = FilterUse::new(FilterId(2));
        f2.overrides.cut_lines = line([3, 3, 3]);
        f2.overrides.projection_lines = line([3, 3, 3]);
        g.filters = vec![f1, f2];
        g.elements.insert(ElementId(7), GraphicOverride { cut_lines: line([4, 4, 4]), ..Default::default() });

        let r = g.resolve(ElementId(7), Category::Wall, &mut |_| true).unwrap();
        assert_eq!(r.cut_lines.color, Some([4, 4, 4]));
        assert_eq!(r.projection_lines.color, Some([3, 3, 3]));
        assert_eq!(r.halftone, Some(true));
        let r = g.resolve(ElementId(8), Category::Wall, &mut |_| true).unwrap();
        assert_eq!(r.cut_lines.color, Some([2, 2, 2]));
        let r = g.resolve(ElementId(8), Category::Wall, &mut |f| f == FilterId(2)).unwrap();
        assert_eq!(r.cut_lines.color, Some([3, 3, 3]));
        let r = g.resolve(ElementId(8), Category::Wall, &mut |_| false).unwrap();
        assert_eq!(r.cut_lines.color, Some([1, 1, 1]));

        // Hiding: a matching hiding filter, the element, the category.
        g.filters[1].visible = false;
        assert!(g.resolve(ElementId(8), Category::Wall, &mut |f| f == FilterId(2)).is_none());
        g.filters[1].enabled = false;
        assert!(g.resolve(ElementId(8), Category::Wall, &mut |f| f == FilterId(2)).is_some());
        g.hidden.insert(ElementId(8));
        assert!(g.resolve(ElementId(8), Category::Wall, &mut |_| false).is_none());
        g.set_category(Category::Door, CategoryGraphics { visible: false, ..Default::default() });
        assert!(g.resolve(ElementId(9), Category::Door, &mut |_| false).is_none());
    }

    #[test]
    fn rule_operators() {
        let n = |v: f64, s: &'static str| Some((s, Some(v)));
        assert!(FilterOp::Greater.test(n(0.2, "0.20"), "0.15"));
        assert!(!FilterOp::Greater.test(n(0.15, "0.15"), "0.15"));
        assert!(FilterOp::GreaterOrEqual.test(n(0.15, "0.15"), "0,15"));
        assert!(FilterOp::Equals.test(n(0.15, "0.15"), "0.150"));
        assert!(FilterOp::Less.test(n(0.1, "0.10"), "0.15"));
        assert!(FilterOp::Contains.test(Some(("Alvenaria 15", None)), "alven"));
        assert!(FilterOp::BeginsWith.test(Some(("Alvenaria 15", None)), "ALV"));
        assert!(FilterOp::NotBeginsWith.test(Some(("Alvenaria 15", None)), "Conc"));
        assert!(FilterOp::NotContains.test(None, "x"));
        assert!(FilterOp::HasNoValue.test(None, ""));
        assert!(!FilterOp::HasValue.test(None, ""));
        assert!(FilterOp::HasValue.test(Some(("P1", None)), ""));
        // Text against a number does not compare by size.
        assert!(!FilterOp::Greater.test(Some(("abc", None)), "1"));
    }

    #[test]
    fn temporary_hide_and_isolate() {
        let mut t = TemporaryVisibility::default();
        assert!(t.shows(ElementId(1), Category::Wall));
        t.elements.insert(ElementId(1));
        assert!(!t.shows(ElementId(1), Category::Wall));
        assert!(t.shows(ElementId(2), Category::Wall));
        t.isolate = true;
        assert!(t.shows(ElementId(1), Category::Wall));
        assert!(!t.shows(ElementId(2), Category::Wall));
        t.categories.insert(Category::Door);
        assert!(t.shows(ElementId(3), Category::Door));
    }
}
