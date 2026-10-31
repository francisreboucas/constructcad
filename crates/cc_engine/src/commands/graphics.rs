//! View graphics: category visibility and overrides, element overrides and
//! hiding in a view, view filters, view templates and temporary hiding and
//! isolation.

use super::query::snake;
use super::{edit, patch_value};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{
    Category, CategoryGraphics, EditCtx, ElementId, FilterId, FilterOp, FilterParam, FilterRule, FilterUse,
    GraphicOverride, MepDiscipline, Project, ScheduleField, TemplateId, TemporaryVisibility, ViewFilter, ViewGraphics,
    ViewId, ViewTemplate,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Longest name or rule value accepted.
const MAX_TEXT: usize = 200;
/// Most rules in one filter.
const MAX_RULES: usize = 50;

/// Where graphics are edited: a view's or a view template's.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Target {
    View(ViewId),
    Template(TemplateId),
}

/// What a view's template controls (empty when it follows none).
fn controlled(p: &Project, view: ViewId) -> Option<(&ViewTemplate, cc_core::TemplateIncludes)> {
    let t = p.template(p.view(view)?.template?)?;
    Some((t, t.include))
}

fn target(p: &Project, view: Option<&Ref>, template: Option<&Ref>) -> Result<Target> {
    match (view, template) {
        (Some(v), None) => Ok(Target::View(refs::view(p, v)?)),
        (None, Some(t)) => Ok(Target::Template(refs::template(p, t)?)),
        _ => Err(EngineError::invalid("give either a view or a template")),
    }
}

/// Refuse to edit graphics of a view that its template controls.
fn check_free(p: &Project, target: Target, filters: bool) -> Result<()> {
    if let Target::View(v) = target {
        if !p.view(v).is_some_and(|v| ViewTemplate::applies_to(&v.kind)) {
            return Err(EngineError::invalid("this view has no visibility/graphics"));
        }
        if let Some((t, inc)) = controlled(p, v)
            && (if filters { inc.filters } else { inc.categories })
        {
            return Err(EngineError::rejected(t!("msg-controlled-by-template", "template" => t.name.clone())));
        }
    }
    Ok(())
}

fn graphics_of(p: &Project, target: Target) -> Result<ViewGraphics> {
    match target {
        Target::View(v) => p.view(v).map(|v| v.graphics.clone()).ok_or_else(|| EngineError::not_found("view")),
        Target::Template(t) => {
            p.template(t).map(|t| t.graphics.clone()).ok_or_else(|| EngineError::not_found("view template"))
        }
    }
}

fn store(e: &mut EditCtx, target: Target, graphics: ViewGraphics) -> Result<()> {
    match target {
        Target::View(v) => e.view_mut(v).ok_or_else(|| EngineError::not_found("view"))?.graphics = graphics,
        Target::Template(t) => {
            e.templates_mut().get_mut(&t).ok_or_else(|| EngineError::not_found("view template"))?.graphics = graphics
        }
    }
    Ok(())
}

/// Apply a merge patch (`null` resets an attribute) to overrides.
pub(crate) fn patched(current: &GraphicOverride, patch: Option<&Value>, reset: bool) -> Result<GraphicOverride> {
    let base = if reset { GraphicOverride::default() } else { *current };
    let mut out = match patch {
        Some(p) => patch_value(&base, p)?,
        None => base,
    };
    if out.repair() > 0 {
        return Err(EngineError::invalid("weights must be between 0 and 10 mm and transparency between 0 and 1"));
    }
    if out.surface_fill.pattern.is_some_and(|p| !cc_core::Pattern::ALL.contains(&p)) {
        out.surface_fill.pattern = None;
    }
    Ok(out)
}

pub(crate) fn overrides_json(o: &GraphicOverride) -> Value {
    serde_json::to_value(o).unwrap_or(json!({}))
}

fn filter_use_json(p: &Project, f: &FilterUse) -> Value {
    json!({
        "filter": f.filter.0,
        "name": p.filter(f.filter).map(|x| x.name.as_str()),
        "visible": f.visible,
        "enabled": f.enabled,
        "overrides": overrides_json(&f.overrides),
    })
}

fn graphics_json(p: &Project, g: &ViewGraphics) -> Value {
    json!({
        "categories": Category::ALL.iter().map(|c| {
            let cg = g.category(*c);
            json!({"category": refs::category_name(*c), "visible": cg.visible, "overrides": overrides_json(&cg.overrides)})
        }).collect::<Vec<_>>(),
        "filters": g.filters.iter().map(|f| filter_use_json(p, f)).collect::<Vec<_>>(),
        "elements": g.elements.iter().map(|(id, o)| json!({"id": id.0, "overrides": overrides_json(o)})).collect::<Vec<_>>(),
        "hidden": g.hidden.iter().map(|id| id.0).collect::<Vec<_>>(),
    })
}

fn includes_json(inc: &cc_core::TemplateIncludes) -> Value {
    serde_json::to_value(inc).unwrap_or(json!({}))
}

fn temporary_json(t: &TemporaryVisibility) -> Value {
    json!({
        "mode": if t.isolate { "isolate" } else { "hide" },
        "elements": t.elements.iter().map(|id| id.0).collect::<Vec<_>>(),
        "categories": t.categories.iter().map(|c| refs::category_name(*c)).collect::<Vec<_>>(),
    })
}

/// The visibility/graphics of a view (or of a view template): every
/// category's visibility and overrides, the applied filters in order, the
/// element overrides and hidden elements, the view's template and what it
/// controls, and what is temporarily hidden or isolated. Overrides are
/// objects like `{"cut_lines": {"color": [255, 0, 0], "weight": 0.5,
/// "pattern": "Dash"}, "projection_lines": {..}, "cut_fill": {"color": [..],
/// "pattern": "Solid"}, "surface_fill": {..}, "halftone": true,
/// "transparency": 0.5}` (absent attributes are not overridden).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ViewGraphicsGet {
    /// View id or name.
    #[serde(default)]
    pub view: Option<Ref>,
    /// Or a view template id or name.
    #[serde(default)]
    pub template: Option<Ref>,
}

impl Command for ViewGraphicsGet {
    const NAME: &'static str = "view_graphics_get";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let target = target(p, self.view.as_ref(), self.template.as_ref())?;
        let mut out = graphics_json(p, &graphics_of(p, target)?);
        if let Target::View(v) = target {
            out["view"] = json!(v.0);
            out["template"] = match controlled(p, v) {
                Some((t, inc)) => json!({"id": t.id.0, "name": t.name, "controls": includes_json(&inc)}),
                None => Value::Null,
            };
            out["temporary"] = s.temporary(v).map_or(Value::Null, temporary_json);
        } else if let Target::Template(t) = target {
            out["template"] = json!(t.0);
        }
        Ok(out)
    }
}

/// Show or hide a category in a view (or view template) and change its
/// graphic overrides: `overrides` is merged into the current ones (`null`
/// resets one attribute; see `view_graphics_get`), `reset` clears them
/// first. Refused when the view's template controls its categories.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ViewGraphicsSet {
    /// View id or name.
    #[serde(default)]
    pub view: Option<Ref>,
    /// Or a view template id or name.
    #[serde(default)]
    pub template: Option<Ref>,
    /// Category (`wall`, `door`, `room`, `dimension`…).
    pub category: String,
    /// Show (`true`) or hide (`false`) the category.
    #[serde(default)]
    pub visible: Option<bool>,
    /// Overrides to merge.
    #[serde(default)]
    pub overrides: Option<Value>,
    /// Clear the category's overrides before merging.
    #[serde(default)]
    pub reset: bool,
}

impl Command for ViewGraphicsSet {
    const NAME: &'static str = "view_graphics_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let target = target(p, self.view.as_ref(), self.template.as_ref())?;
        check_free(p, target, false)?;
        let category = refs::category(&self.category)?;
        let mut g = graphics_of(p, target)?;
        let mut cg = g.category(category);
        cg.overrides = patched(&cg.overrides, self.overrides.as_ref(), self.reset)?;
        if let Some(v) = self.visible {
            cg.visible = v;
        }
        g.set_category(category, CategoryGraphics { ..cg });
        edit(s, t!("tx-view-graphics"), |e| store(e, target, g))?;
        let p = s.project();
        Ok(graphics_json(p, &graphics_of(p, target)?))
    }
}

fn element_ids(p: &Project, ids: &[u64]) -> Result<Vec<ElementId>> {
    if ids.is_empty() {
        return Err(EngineError::invalid("no elements given"));
    }
    refs::elements(p, ids)
}

fn graphical_view(p: &Project, r: &Ref) -> Result<ViewId> {
    let v = refs::view(p, r)?;
    if !p.view(v).is_some_and(|v| ViewTemplate::applies_to(&v.kind)) {
        return Err(EngineError::invalid("this view has no visibility/graphics"));
    }
    Ok(v)
}

/// Override the graphics of elements in one view (they keep their
/// category's and filters' graphics for what is not overridden):
/// `overrides` is merged into each element's (`null` resets an attribute),
/// `reset` clears them first, so `reset` alone removes the overrides.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ElementOverrideSet {
    /// View id or name.
    pub view: Ref,
    /// Element ids.
    pub ids: Vec<u64>,
    /// Overrides to merge (see `view_graphics_get`).
    #[serde(default)]
    pub overrides: Option<Value>,
    /// Clear the elements' overrides before merging.
    #[serde(default)]
    pub reset: bool,
}

impl Command for ElementOverrideSet {
    const NAME: &'static str = "element_override_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let view = graphical_view(p, &self.view)?;
        let ids = element_ids(p, &self.ids)?;
        let current = graphics_of(p, Target::View(view))?;
        let mut changes = Vec::new();
        for id in &ids {
            let old = current.elements.get(id).copied().unwrap_or_default();
            changes.push((*id, patched(&old, self.overrides.as_ref(), self.reset)?));
        }
        edit(s, t!("tx-override-graphics"), |e| {
            let v = e.view_mut(view).ok_or_else(|| EngineError::not_found("view"))?;
            for (id, o) in changes {
                if o.is_empty() {
                    v.graphics.elements.remove(&id);
                } else {
                    v.graphics.elements.insert(id, o);
                }
            }
            Ok(())
        })?;
        let p = s.project();
        Ok(graphics_json(p, &graphics_of(p, Target::View(view))?))
    }
}

/// Hide elements in one view (`hidden: false` shows them again). Hidden
/// elements stay in the model and in other views.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ElementHideInView {
    /// View id or name.
    pub view: Ref,
    /// Element ids.
    pub ids: Vec<u64>,
    /// Hide (default) or unhide.
    #[serde(default = "yes")]
    pub hidden: bool,
}

fn yes() -> bool {
    true
}

impl Command for ElementHideInView {
    const NAME: &'static str = "element_hide_in_view";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let view = graphical_view(p, &self.view)?;
        let ids = element_ids(p, &self.ids)?;
        let tx = if self.hidden { t!("tx-hide-in-view") } else { t!("tx-unhide-in-view") };
        edit(s, tx, |e| {
            let v = e.view_mut(view).ok_or_else(|| EngineError::not_found("view"))?;
            for id in ids {
                if self.hidden {
                    v.graphics.hidden.insert(id);
                } else {
                    v.graphics.hidden.remove(&id);
                }
            }
            Ok(())
        })?;
        let p = s.project();
        Ok(
            json!({ "view": view.0, "hidden": p.view(view).map(|v| v.graphics.hidden.iter().map(|i| i.0).collect::<Vec<_>>()) }),
        )
    }
}

// ---------------------------------------------------------------- filters

/// A rule as commands take it.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleArg {
    /// Parameter: `elevation` (of the element's level) or a schedule field —
    /// `type`, `mark`, `comments`, `level`, `material`, `system`,
    /// `structural` (yes/no), `function`, `width`, `height`, `thickness`,
    /// `length`, `area`, `volume`, `diameter`, `sill`, `type_mark`,
    /// `description`, `manufacturer`, `model`, `fire_rating`, `name`,
    /// `number`, `component_name`…
    pub param: String,
    /// `equals`, `not_equals`, `contains`, `not_contains`, `begins_with`,
    /// `not_begins_with`, `greater`, `greater_or_equal`, `less`,
    /// `less_or_equal`, `has_value` or `has_no_value`.
    pub op: String,
    /// Text, or a number in SI units (meters) for quantities.
    #[serde(default)]
    pub value: Option<Value>,
}

fn pascal(snake_name: &str) -> String {
    snake_name
        .trim()
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_ascii_uppercase().to_string() + &c.as_str().to_ascii_lowercase()).unwrap_or_default()
        })
        .collect()
}

pub(crate) fn param(name: &str) -> Result<FilterParam> {
    if name.trim().eq_ignore_ascii_case("elevation") {
        return Ok(FilterParam::Elevation);
    }
    serde_json::from_value::<ScheduleField>(Value::String(pascal(name)))
        .map(FilterParam::Field)
        .map_err(|_| EngineError::invalid(format!("unknown filter parameter '{name}'")))
}

pub fn param_name(p: FilterParam) -> String {
    match p {
        FilterParam::Elevation => "elevation".into(),
        FilterParam::Field(f) => snake(&format!("{f:?}")),
    }
}

fn op(name: &str) -> Result<FilterOp> {
    serde_json::from_value(Value::String(name.trim().to_ascii_lowercase()))
        .map_err(|_| EngineError::invalid(format!("unknown filter operator '{name}'")))
}

fn rule(r: &RuleArg) -> Result<FilterRule> {
    let value = match &r.value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => if *b { "yes" } else { "no" }.to_string(),
        Some(_) => return Err(EngineError::invalid("a rule value is text or a number")),
    };
    if value.len() > MAX_TEXT {
        return Err(EngineError::invalid("the rule value is too long"));
    }
    Ok(FilterRule { param: param(&r.param)?, op: op(&r.op)?, value })
}

fn rules(rs: &[RuleArg]) -> Result<Vec<FilterRule>> {
    if rs.len() > MAX_RULES {
        return Err(EngineError::invalid(format!("at most {MAX_RULES} rules")));
    }
    rs.iter().map(rule).collect()
}

/// Refuse a rule that could never pass: its parameter applies to none of the
/// filter's categories (`width` on walls).
fn check_rules(f: &ViewFilter) -> Result<()> {
    let Some(p) = f.inapplicable_param() else { return Ok(()) };
    let names = |cats: &mut dyn Iterator<Item = Category>| cats.map(refs::category_name).collect::<Vec<_>>().join(", ");
    let chosen = names(&mut f.categories.iter().copied());
    let applies = names(&mut Category::ALL.into_iter().filter(|c| p.applies_to(*c)));
    Err(EngineError::invalid(format!(
        "the parameter '{}' does not apply to {chosen} (it applies to {applies})",
        param_name(p)
    )))
}

fn categories(names: &[String]) -> Result<BTreeSet<Category>> {
    names.iter().map(|n| refs::category(n)).collect()
}

fn filter_name(p: &Project, name: &str, except: Option<FilterId>) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_TEXT {
        return Err(EngineError::invalid("the name is empty or too long"));
    }
    if p.filters.values().any(|f| Some(f.id) != except && f.name.eq_ignore_ascii_case(name)) {
        return Err(EngineError::rejected(t!("msg-name-taken", "name" => name)));
    }
    Ok(name.to_string())
}

pub(crate) fn filter_json(p: &Project, f: &ViewFilter) -> Value {
    json!({
        "id": f.id.0,
        "name": f.name,
        "categories": f.categories.iter().map(|c| refs::category_name(*c)).collect::<Vec<_>>(),
        "any": f.any,
        "rules": f.rules.iter().map(|r| json!({
            "param": param_name(r.param),
            "op": serde_json::to_value(r.op).unwrap_or(Value::Null),
            "value": r.value,
        })).collect::<Vec<_>>(),
        "used_by_views": p.views.values().filter(|v| v.graphics.filters.iter().any(|u| u.filter == f.id)).count(),
    })
}

/// The project's view filters with their categories and rules.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct FiltersList {}

impl Command for FiltersList {
    const NAME: &'static str = "filters_list";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        Ok(p.filters.values().map(|f| filter_json(p, f)).collect())
    }
}

/// Create a view filter: the elements of `categories` (all when empty) that
/// pass its rules — all of them, or any with `any`; a filter without rules
/// matches every element of its categories. Apply it to views with
/// `view_filter_add`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct FilterCreate {
    /// Name (unique).
    pub name: String,
    /// Categories (`wall`, `door`…); empty for all.
    #[serde(default)]
    pub categories: Vec<String>,
    /// Rules on parameters.
    #[serde(default)]
    pub rules: Vec<RuleArg>,
    /// Match when any rule passes (default: all must pass).
    #[serde(default)]
    pub any: bool,
}

impl Command for FilterCreate {
    const NAME: &'static str = "filter_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let name = filter_name(p, &self.name, None)?;
        let filter = ViewFilter {
            id: FilterId(0),
            name,
            categories: categories(&self.categories)?,
            rules: rules(&self.rules)?,
            any: self.any,
        };
        check_rules(&filter)?;
        let id = edit(s, t!("tx-filter-create"), |e| {
            let id = FilterId(e.alloc_id());
            e.filters_mut().insert(id, ViewFilter { id, ..filter });
            Ok(id)
        })?;
        let p = s.project();
        p.filter(id).map(|f| filter_json(p, f)).ok_or_else(|| EngineError::not_found("filter"))
    }
}

/// Change a view filter: name, categories, rules (replaced as a whole) or
/// how rules combine. Views that apply it follow.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct FilterSet {
    /// Filter id or name.
    pub filter: Ref,
    /// New name.
    #[serde(default)]
    pub name: Option<String>,
    /// New categories (empty for all).
    #[serde(default)]
    pub categories: Option<Vec<String>>,
    /// New rules (replace the old ones).
    #[serde(default)]
    pub rules: Option<Vec<RuleArg>>,
    /// Match when any rule passes (`false`: all must pass).
    #[serde(default)]
    pub any: Option<bool>,
}

impl Command for FilterSet {
    const NAME: &'static str = "filter_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::filter(p, &self.filter)?;
        let mut f = p.filter(id).cloned().ok_or_else(|| EngineError::not_found("filter"))?;
        if let Some(n) = &self.name {
            f.name = filter_name(p, n, Some(id))?;
        }
        if let Some(c) = &self.categories {
            f.categories = categories(c)?;
        }
        if let Some(r) = &self.rules {
            f.rules = rules(r)?;
        }
        if let Some(a) = self.any {
            f.any = a;
        }
        check_rules(&f)?;
        edit(s, t!("tx-filter-modify"), |e| {
            e.filters_mut().insert(id, f);
            Ok(())
        })?;
        let p = s.project();
        p.filter(id).map(|f| filter_json(p, f)).ok_or_else(|| EngineError::not_found("filter"))
    }
}

/// Delete a view filter (views and templates stop applying it).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct FilterDelete {
    /// Filter id or name.
    pub filter: Ref,
}

impl Command for FilterDelete {
    const NAME: &'static str = "filter_delete";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::filter(s.project(), &self.filter)?;
        edit(s, t!("tx-filter-delete"), |e| {
            e.filters_mut().remove(&id);
            Ok(())
        })?;
        Ok(json!({ "deleted": id.0 }))
    }
}

/// The elements a view filter matches (only those drawn in `view` when given).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct FilterElements {
    /// Filter id or name.
    pub filter: Ref,
    /// Only elements drawn in this view.
    #[serde(default)]
    pub view: Option<Ref>,
}

impl Command for FilterElements {
    const NAME: &'static str = "filter_elements";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::filter(p, &self.filter)?;
        let view = self.view.as_ref().map(|v| refs::view(p, v)).transpose()?;
        let drawn: Option<BTreeSet<ElementId>> = match view {
            Some(v) => {
                // As generated, before the view's own graphics hide anything.
                s.sync();
                let p = s.project();
                let mut plain = p.view(v).cloned().ok_or_else(|| EngineError::not_found("view"))?;
                plain.graphics = ViewGraphics::default();
                let d = cc_views::generate(p, s.geom(), &plain);
                Some(d.map(|d| d.prims.iter().filter_map(|x| x.element()).collect()).unwrap_or_default())
            }
            None => None,
        };
        let (p, geom) = s.project_and_geometry();
        let f = p.filter(id).ok_or_else(|| EngineError::not_found("filter"))?;
        let ids: Vec<u64> = cc_views::graphics::filter_elements(p, geom, f)
            .into_iter()
            .filter(|e| drawn.as_ref().is_none_or(|d| d.contains(e)))
            .map(|e| e.0)
            .collect();
        Ok(json!({ "filter": id.0, "count": ids.len(), "ids": ids }))
    }
}

/// Apply a filter to a view (or view template), at `index` in the list
/// (default last: the first matching filter wins for each attribute),
/// hiding what it matches (`visible: false`) or overriding its graphics.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ViewFilterAdd {
    /// View id or name.
    #[serde(default)]
    pub view: Option<Ref>,
    /// Or a view template id or name.
    #[serde(default)]
    pub template: Option<Ref>,
    /// Filter id or name.
    pub filter: Ref,
    /// Show (default) or hide the matching elements.
    #[serde(default)]
    pub visible: Option<bool>,
    /// Overrides of the matching elements (see `view_graphics_get`).
    #[serde(default)]
    pub overrides: Option<Value>,
    /// Position in the view's filter list (0 = first).
    #[serde(default)]
    pub index: Option<usize>,
}

impl Command for ViewFilterAdd {
    const NAME: &'static str = "view_filter_add";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let target = target(p, self.view.as_ref(), self.template.as_ref())?;
        check_free(p, target, true)?;
        let filter = refs::filter(p, &self.filter)?;
        let mut g = graphics_of(p, target)?;
        if g.filters.iter().any(|f| f.filter == filter) {
            return Err(EngineError::rejected(t!("msg-filter-already-applied")));
        }
        let mut u = FilterUse::new(filter);
        u.visible = self.visible.unwrap_or(true);
        u.overrides = patched(&u.overrides, self.overrides.as_ref(), false)?;
        let at = self.index.unwrap_or(g.filters.len()).min(g.filters.len());
        g.filters.insert(at, u);
        edit(s, t!("tx-view-graphics"), |e| store(e, target, g))?;
        let p = s.project();
        Ok(graphics_json(p, &graphics_of(p, target)?))
    }
}

/// Change how a view (or view template) applies a filter: visibility of the
/// matching elements, whether it is enabled, and its overrides (merged;
/// `reset` clears them first).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ViewFilterSet {
    /// View id or name.
    #[serde(default)]
    pub view: Option<Ref>,
    /// Or a view template id or name.
    #[serde(default)]
    pub template: Option<Ref>,
    /// Filter id or name.
    pub filter: Ref,
    /// Show (`true`) or hide (`false`) the matching elements.
    #[serde(default)]
    pub visible: Option<bool>,
    /// Apply the filter (`false` keeps it listed without effect).
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Overrides to merge (see `view_graphics_get`).
    #[serde(default)]
    pub overrides: Option<Value>,
    /// Clear the overrides before merging.
    #[serde(default)]
    pub reset: bool,
}

fn filter_index(g: &ViewGraphics, filter: FilterId) -> Result<usize> {
    g.filters
        .iter()
        .position(|f| f.filter == filter)
        .ok_or_else(|| EngineError::not_found(t!("msg-filter-not-applied")))
}

impl Command for ViewFilterSet {
    const NAME: &'static str = "view_filter_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let target = target(p, self.view.as_ref(), self.template.as_ref())?;
        check_free(p, target, true)?;
        let filter = refs::filter(p, &self.filter)?;
        let mut g = graphics_of(p, target)?;
        let i = filter_index(&g, filter)?;
        let u = &mut g.filters[i];
        u.overrides = patched(&u.overrides, self.overrides.as_ref(), self.reset)?;
        if let Some(v) = self.visible {
            u.visible = v;
        }
        if let Some(en) = self.enabled {
            u.enabled = en;
        }
        edit(s, t!("tx-view-graphics"), |e| store(e, target, g))?;
        let p = s.project();
        Ok(graphics_json(p, &graphics_of(p, target)?))
    }
}

/// Stop applying a filter in a view (or view template).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ViewFilterRemove {
    /// View id or name.
    #[serde(default)]
    pub view: Option<Ref>,
    /// Or a view template id or name.
    #[serde(default)]
    pub template: Option<Ref>,
    /// Filter id or name.
    pub filter: Ref,
}

impl Command for ViewFilterRemove {
    const NAME: &'static str = "view_filter_remove";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let target = target(p, self.view.as_ref(), self.template.as_ref())?;
        check_free(p, target, true)?;
        let filter = refs::filter(p, &self.filter)?;
        let mut g = graphics_of(p, target)?;
        let i = filter_index(&g, filter)?;
        g.filters.remove(i);
        edit(s, t!("tx-view-graphics"), |e| store(e, target, g))?;
        let p = s.project();
        Ok(graphics_json(p, &graphics_of(p, target)?))
    }
}

/// Move a filter to `index` in a view's (or view template's) filter list
/// (0 = first, the highest priority).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ViewFilterMove {
    /// View id or name.
    #[serde(default)]
    pub view: Option<Ref>,
    /// Or a view template id or name.
    #[serde(default)]
    pub template: Option<Ref>,
    /// Filter id or name.
    pub filter: Ref,
    /// New position (0 = first).
    pub index: usize,
}

impl Command for ViewFilterMove {
    const NAME: &'static str = "view_filter_move";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let target = target(p, self.view.as_ref(), self.template.as_ref())?;
        check_free(p, target, true)?;
        let filter = refs::filter(p, &self.filter)?;
        let mut g = graphics_of(p, target)?;
        let i = filter_index(&g, filter)?;
        let u = g.filters.remove(i);
        let at = self.index.min(g.filters.len());
        g.filters.insert(at, u);
        edit(s, t!("tx-view-graphics"), |e| store(e, target, g))?;
        let p = s.project();
        Ok(graphics_json(p, &graphics_of(p, target)?))
    }
}

// ---------------------------------------------------------------- templates

fn discipline_name(d: Option<MepDiscipline>) -> &'static str {
    match d {
        None => "none",
        Some(MepDiscipline::Plumbing) => "plumbing",
        Some(MepDiscipline::Electrical) => "electrical",
    }
}

pub(crate) fn template_json(p: &Project, t: &ViewTemplate) -> Value {
    json!({
        "id": t.id.0,
        "name": t.name,
        "include": includes_json(&t.include),
        "scale": t.scale,
        "cut_height": t.cut_height,
        "discipline": discipline_name(t.discipline),
        "structural": t.structural,
        "structure_tags": t.structure_tags,
        "phase_filter": t.phase_filter.and_then(|f| p.phase_filter(f)).map(|f| json!({"id": f.id.0, "name": f.name})),
        "detail": t.detail.name(),
        "graphics": graphics_json(p, &t.graphics),
        "views": p.views.values().filter(|v| v.template == Some(t.id)).map(|v| v.id.0).collect::<Vec<_>>(),
    })
}

fn template_name(p: &Project, name: &str, except: Option<TemplateId>) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_TEXT {
        return Err(EngineError::invalid("the name is empty or too long"));
    }
    if p.templates.values().any(|t| Some(t.id) != except && t.name.eq_ignore_ascii_case(name)) {
        return Err(EngineError::rejected(t!("msg-name-taken", "name" => name)));
    }
    Ok(name.to_string())
}

/// The project's view templates: the properties each holds, which of them
/// it controls (`include`) and the views that follow it.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct TemplatesList {}

impl Command for TemplatesList {
    const NAME: &'static str = "templates_list";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        Ok(p.templates.values().map(|t| template_json(p, t)).collect())
    }
}

/// Create a view template, from the properties of `view` when given
/// (scale, category graphics, filters, plan cut height, building services
/// display, structural display), else with defaults. Every property is
/// controlled (`include`) until changed with `template_set`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct TemplateCreate {
    /// Name (unique).
    pub name: String,
    /// View to take the properties from.
    #[serde(default)]
    pub view: Option<Ref>,
}

impl Command for TemplateCreate {
    const NAME: &'static str = "template_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let name = template_name(p, &self.name, None)?;
        let source = match &self.view {
            Some(r) => {
                let v = refs::view(p, r)?;
                let view = p.view(v).cloned().ok_or_else(|| EngineError::not_found("view"))?;
                if !ViewTemplate::applies_to(&view.kind) {
                    return Err(EngineError::invalid(t!("msg-no-templates-for-view")));
                }
                Some(view)
            }
            None => None,
        };
        let id = edit(s, t!("tx-template-create"), |e| {
            let id = TemplateId(e.alloc_id());
            let t = match &source {
                Some(v) => ViewTemplate::from_view(id, name, v),
                None => ViewTemplate::from_view(
                    id,
                    name,
                    &cc_core::View::new(
                        ViewId(0),
                        "",
                        50,
                        cc_core::ViewKind::Elevation { direction: cc_core::Direction::South },
                    ),
                ),
            };
            e.templates_mut().insert(id, t);
            Ok(id)
        })?;
        let p = s.project();
        p.template(id).map(|t| template_json(p, t)).ok_or_else(|| EngineError::not_found("view template"))
    }
}

/// Change a view template: name, properties and which of them it controls
/// (`include`, merged: `{"scale": false}` lets views keep their own scale).
/// Views that follow the template change with it, in the same undo step.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct TemplateSet {
    /// Template id or name.
    pub template: Ref,
    /// New name.
    #[serde(default)]
    pub name: Option<String>,
    /// Scale denominator.
    #[serde(default)]
    pub scale: Option<u32>,
    /// Plans: cut plane height, meters.
    #[serde(default)]
    pub cut_height: Option<f64>,
    /// Floor plans: `none`, `plumbing` or `electrical` (services over a faint architecture).
    #[serde(default)]
    pub discipline: Option<String>,
    /// Floor plans: structural (formwork) display.
    #[serde(default)]
    pub structural: Option<bool>,
    /// Floor plans: tag beams and columns.
    #[serde(default)]
    pub structure_tags: Option<bool>,
    /// Phase filter (id or name), or `null` for none.
    #[serde(default, deserialize_with = "refs::nullable", skip_serializing_if = "Option::is_none")]
    pub phase_filter: Option<Option<Ref>>,
    /// Detail level: `coarse`, `medium` or `fine`.
    #[serde(default)]
    pub detail: Option<String>,
    /// Which properties the template controls: `scale`, `categories`,
    /// `filters`, `cut_height`, `discipline`, `structural`, `phase_filter`,
    /// `detail` (true/false).
    #[serde(default)]
    pub include: Option<Value>,
}

impl Command for TemplateSet {
    const NAME: &'static str = "template_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::template(p, &self.template)?;
        let mut t = p.template(id).cloned().ok_or_else(|| EngineError::not_found("view template"))?;
        if let Some(n) = &self.name {
            t.name = template_name(p, n, Some(id))?;
        }
        if let Some(sc) = self.scale {
            t.scale = sc.max(1);
        }
        if let Some(h) = self.cut_height {
            t.cut_height = refs::finite(h, "cut_height")?;
        }
        if let Some(d) = &self.discipline {
            t.discipline = match d.trim().to_ascii_lowercase().as_str() {
                "none" | "" => None,
                "plumbing" => Some(MepDiscipline::Plumbing),
                "electrical" => Some(MepDiscipline::Electrical),
                other => return Err(EngineError::invalid(format!("unknown discipline '{other}'"))),
            };
        }
        if let Some(v) = self.structural {
            t.structural = v;
        }
        if let Some(v) = self.structure_tags {
            t.structure_tags = v;
        }
        if let Some(d) = &self.detail {
            t.detail = refs::detail_level(d)?;
        }
        match &self.phase_filter {
            Some(Some(r)) => t.phase_filter = Some(refs::phase_filter(p, r)?),
            Some(None) => t.phase_filter = None,
            None => {}
        }
        if let Some(patch) = &self.include {
            t.include = patch_value(&t.include, patch)?;
        }
        edit(s, t!("tx-template-modify"), |e| {
            e.templates_mut().insert(id, t);
            Ok(())
        })?;
        let p = s.project();
        p.template(id).map(|t| template_json(p, t)).ok_or_else(|| EngineError::not_found("view template"))
    }
}

/// Delete a view template; views that follow it keep their current
/// properties and become free.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct TemplateDelete {
    /// Template id or name.
    pub template: Ref,
}

impl Command for TemplateDelete {
    const NAME: &'static str = "template_delete";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::template(s.project(), &self.template)?;
        edit(s, t!("tx-template-delete"), |e| {
            e.templates_mut().remove(&id);
            Ok(())
        })?;
        Ok(json!({ "deleted": id.0 }))
    }
}

fn template_view(p: &Project, r: &Ref) -> Result<ViewId> {
    let v = refs::view(p, r)?;
    if !p.view(v).is_some_and(|v| ViewTemplate::applies_to(&v.kind)) {
        return Err(EngineError::invalid(t!("msg-no-templates-for-view")));
    }
    Ok(v)
}

/// Copy a template's properties into views once (they stay free to
/// change). Views that follow another template are refused.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ViewTemplateApply {
    /// View ids or names.
    pub views: Vec<Ref>,
    /// Template id or name.
    pub template: Ref,
}

impl Command for ViewTemplateApply {
    const NAME: &'static str = "view_template_apply";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let t = p
            .template(refs::template(p, &self.template)?)
            .cloned()
            .ok_or_else(|| EngineError::not_found("view template"))?;
        let views: Vec<ViewId> = self.views.iter().map(|v| template_view(p, v)).collect::<Result<_>>()?;
        if let Some(v) = views.iter().find_map(|v| p.view(*v).filter(|v| v.template.is_some_and(|x| x != t.id))) {
            return Err(EngineError::rejected(t!("msg-view-follows-template", "view" => v.name.clone())));
        }
        edit(s, t!("tx-template-apply"), |e| {
            for v in &views {
                if let Some(view) = e.view_mut(*v) {
                    t.apply(view);
                }
            }
            Ok(())
        })?;
        Ok(json!({ "views": views.iter().map(|v| v.0).collect::<Vec<_>>(), "template": t.id.0 }))
    }
}

/// Make views follow a template (`template: null` frees them): the
/// properties it controls are copied now and whenever the template changes,
/// and cannot be changed in the view.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ViewTemplateAssign {
    /// View ids or names.
    pub views: Vec<Ref>,
    /// Template id or name, or null to unassign.
    #[serde(default)]
    pub template: Option<Ref>,
}

impl Command for ViewTemplateAssign {
    const NAME: &'static str = "view_template_assign";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let template = self.template.as_ref().map(|t| refs::template(p, t)).transpose()?;
        let views: Vec<ViewId> = self.views.iter().map(|v| template_view(p, v)).collect::<Result<_>>()?;
        edit(s, t!("tx-template-assign"), |e| {
            for v in &views {
                if let Some(view) = e.view_mut(*v) {
                    view.template = template;
                }
            }
            Ok(())
        })?;
        Ok(json!({ "views": views.iter().map(|v| v.0).collect::<Vec<_>>(), "template": template.map(|t| t.0) }))
    }
}

// ---------------------------------------------------------------- temporary

/// Temporarily hide or isolate elements or categories in a view, or reset.
/// This is display state, not a model change: it is not saved, not undone
/// and not printed (sheets and exports show the view as it is); the
/// application marks the view while it is active. `action`: `hide` and
/// `isolate` (the elements `ids`), `hide_category` and `isolate_category`
/// (`categories`, and the categories of `ids`), or `reset`. Hiding adds to
/// what is already hidden; isolating replaces it.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ViewTemporary {
    /// View id or name.
    pub view: Ref,
    /// `hide`, `isolate`, `hide_category`, `isolate_category` or `reset`.
    pub action: String,
    /// Element ids.
    #[serde(default)]
    pub ids: Vec<u64>,
    /// Category names.
    #[serde(default)]
    pub categories: Vec<String>,
}

impl Command for ViewTemporary {
    const NAME: &'static str = "view_temporary";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let view = graphical_view(p, &self.view)?;
        let ids = if self.ids.is_empty() { Vec::new() } else { refs::elements(p, &self.ids)? };
        let mut cats = categories(&self.categories)?;
        let action = self.action.trim().to_ascii_lowercase();
        let current = s.temporary(view).cloned().unwrap_or_default();
        let next = match action.as_str() {
            "reset" => TemporaryVisibility::default(),
            "hide" | "isolate" | "hide_category" | "isolate_category" => {
                let isolate = action.starts_with("isolate");
                if action.ends_with("category") {
                    cats.extend(ids.iter().filter_map(|id| p.element(*id)).map(|e| e.category()));
                    if cats.is_empty() {
                        return Err(EngineError::invalid("no categories given"));
                    }
                    if isolate {
                        TemporaryVisibility { isolate, elements: BTreeSet::new(), categories: cats }
                    } else {
                        let mut t = if current.isolate { TemporaryVisibility::default() } else { current };
                        t.categories.extend(cats);
                        t
                    }
                } else {
                    if ids.is_empty() {
                        return Err(EngineError::invalid("no elements given"));
                    }
                    if isolate {
                        TemporaryVisibility {
                            isolate,
                            elements: ids.into_iter().collect(),
                            categories: BTreeSet::new(),
                        }
                    } else {
                        let mut t = if current.isolate { TemporaryVisibility::default() } else { current };
                        t.elements.extend(ids);
                        t
                    }
                }
            }
            other => return Err(EngineError::invalid(format!("unknown action '{other}'"))),
        };
        s.set_temporary(view, Some(next));
        Ok(json!({ "view": view.0, "temporary": s.temporary(view).map_or(Value::Null, temporary_json) }))
    }
}

/// Make a view's temporary hiding permanent: hidden elements become hidden
/// in the view and hidden categories are turned off; when isolating, every
/// other element drawn in the view is hidden in it. The temporary state is
/// then reset. One undo step.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ViewTemporaryApply {
    /// View id or name.
    pub view: Ref,
}

impl Command for ViewTemporaryApply {
    const NAME: &'static str = "view_temporary_apply";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let view = graphical_view(s.project(), &self.view)?;
        let Some(temp) = s.temporary(view).cloned() else {
            return Err(EngineError::rejected(t!("msg-nothing-temporary")));
        };
        // Elements drawn in the view with everything shown.
        s.set_temporary(view, None);
        let drawn: BTreeSet<ElementId> =
            s.drawing(view).map(|d| d.prims.iter().filter_map(|x| x.element()).collect()).unwrap_or_default();
        let p = s.project();
        let hide: BTreeSet<ElementId> =
            drawn.iter().copied().filter(|id| p.element(*id).is_some_and(|e| !temp.shows(*id, e.category()))).collect();
        // Hiding a category permanently turns it off, unless the template controls categories.
        let off: Vec<Category> = if temp.isolate || controlled(p, view).is_some_and(|(_, inc)| inc.categories) {
            Vec::new()
        } else {
            temp.categories.iter().copied().collect()
        };
        // Elements of categories turned off need not be hidden one by one.
        let hide: Vec<ElementId> =
            hide.into_iter().filter(|id| p.element(*id).is_none_or(|e| !off.contains(&e.category()))).collect();
        let result = edit(s, t!("tx-make-permanent"), |e| {
            let v = e.view_mut(view).ok_or_else(|| EngineError::not_found("view"))?;
            for c in &off {
                let mut g = v.graphics.category(*c);
                g.visible = false;
                v.graphics.set_category(*c, g);
            }
            v.graphics.hidden.extend(hide.iter().copied());
            Ok(())
        });
        if result.is_err() {
            s.set_temporary(view, Some(temp));
        }
        result?;
        let p = s.project();
        Ok(json!({ "view": view.0, "hidden": p.view(view).map(|v| v.graphics.hidden.len()) }))
    }
}
