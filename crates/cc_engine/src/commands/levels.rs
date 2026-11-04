//! Levels: create (with their plans), rename, change elevation, delete.

use super::edit;
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref, finite};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{ElementKind, Level, LevelId, View, ViewId, ViewKind, WallTop};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

fn level_out(s: &Session, id: LevelId) -> Value {
    s.project()
        .level(id)
        .map(|l| json!({"id": l.id.0, "name": l.name, "elevation": l.elevation}))
        .unwrap_or(json!({ "id": id.0 }))
}

/// Add a level (default: 3 m above the highest, named "Level n") with its
/// floor and ceiling plans.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct LevelCreate {
    /// Level name (default: "Level n").
    #[serde(default)]
    pub name: Option<String>,
    /// Elevation in meters.
    #[serde(default)]
    pub elevation: Option<f64>,
    /// Also create its floor and ceiling plans (default true).
    #[serde(default)]
    pub plans: Option<bool>,
}

impl Command for LevelCreate {
    const NAME: &'static str = "level_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let top = p.levels.iter().map(|l| l.elevation).fold(None, |m: Option<f64>, e| Some(m.map_or(e, |m| m.max(e))));
        let elevation = match self.elevation {
            Some(e) => finite(e, "elevation")?,
            None => top.map_or(0.0, |t| t + 3.0),
        };
        let name = self.name.unwrap_or_else(|| t!("default-level-name", "n" => p.levels.len()));
        let plans = self.plans.unwrap_or(true);
        let id = edit(s, t!("tx-add-level"), |e| {
            let id = LevelId(e.alloc_id());
            let levels = e.levels_mut();
            levels.push(Level { id, name: name.clone(), elevation });
            levels.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
            if plans {
                for kind in [ViewKind::floor_plan(id), ViewKind::ceiling_plan(id)] {
                    let vid = ViewId(e.alloc_id());
                    e.add_view(View::new(vid, name.clone(), 50, kind));
                }
            }
            Ok(id)
        })?;
        Ok(level_out(s, id))
    }
}

/// Rename a level or change its elevation (what is attached to it moves along).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LevelSet {
    pub level: Ref,
    /// New name.
    #[serde(default)]
    pub name: Option<String>,
    /// New elevation, meters.
    #[serde(default)]
    pub elevation: Option<f64>,
}

impl Command for LevelSet {
    const NAME: &'static str = "level_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::level(s.project(), &self.level)?;
        let elevation = self.elevation.map(|e| finite(e, "elevation")).transpose()?;
        if let Some(n) = &self.name
            && n.trim().is_empty()
        {
            return Err(EngineError::invalid("the name is empty"));
        }
        edit(s, t!("tx-modify-level"), |e| {
            let levels = e.levels_mut();
            if let Some(l) = levels.iter_mut().find(|l| l.id == id) {
                if let Some(n) = self.name {
                    l.name = n.trim().to_string();
                }
                if let Some(z) = elevation {
                    l.elevation = z;
                }
            }
            levels.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
            Ok(())
        })?;
        Ok(level_out(s, id))
    }
}

/// Delete a level no element is on, with its plan views; walls rising to it
/// keep their current height, unconnected. The last level is kept.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LevelDelete {
    pub level: Ref,
}

impl Command for LevelDelete {
    const NAME: &'static str = "level_delete";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let level = refs::level(p, &self.level)?;
        if p.elements.values().any(|e| e.level == level) {
            return Err(EngineError::rejected(t!("msg-level-in-use")));
        }
        if p.levels.len() <= 1 {
            return Err(EngineError::rejected("the last level cannot be deleted"));
        }
        let views: Vec<_> = p.views.values().filter(|v| v.level() == Some(level)).map(|v| v.id).collect();
        let masses: Vec<_> = p
            .elements
            .values()
            .filter(|e| matches!(&e.kind, ElementKind::Mass(m) if m.floors.contains(&level)))
            .map(|e| e.id)
            .collect();
        let attached: Vec<_> = p
            .elements
            .values()
            .filter_map(|e| match &e.kind {
                ElementKind::Wall(w) if matches!(w.top, WallTop::Level { level: l, .. } if l == level) => {
                    Some((e.id, p.wall_height(e, w)))
                }
                ElementKind::Column(c) if matches!(c.top, WallTop::Level { level: l, .. } if l == level) => {
                    Some((e.id, p.column_span(e, c).1 - p.column_span(e, c).0))
                }
                ElementKind::Stair(st) if matches!(st.top, WallTop::Level { level: l, .. } if l == level) => {
                    Some((e.id, p.stair_span(e, st).1 - p.stair_span(e, st).0))
                }
                ElementKind::Ramp(r) if matches!(r.top, WallTop::Level { level: l, .. } if l == level) => {
                    Some((e.id, p.ramp_span(e, r).1 - p.ramp_span(e, r).0))
                }
                _ => None,
            })
            .collect();
        edit(s, t!("tx-delete-level"), |e| {
            for v in views {
                e.remove_view(v);
            }
            for (id, height) in attached {
                match e.element_mut(id).map(|x| &mut x.kind) {
                    Some(ElementKind::Wall(w)) => (w.top, w.height) = (WallTop::Unconnected, height),
                    Some(ElementKind::Column(c)) => (c.top, c.height) = (WallTop::Unconnected, height),
                    Some(ElementKind::Stair(st)) => (st.top, st.height) = (WallTop::Unconnected, height),
                    Some(ElementKind::Ramp(r)) => (r.top, r.height) = (WallTop::Unconnected, height),
                    _ => {}
                }
            }
            // Mass floors at the level go with it.
            for id in masses {
                if let Some(ElementKind::Mass(m)) = e.element_mut(id).map(|x| &mut x.kind) {
                    m.floors.retain(|l| *l != level);
                }
            }
            // Underlays showing the level stop showing it.
            let shown: Vec<_> = e
                .project()
                .views
                .values()
                .filter(|v| v.kind.underlay().is_some_and(|u| u.clone().repair(|l| l != level) > 0))
                .map(|v| v.id)
                .collect();
            for v in shown {
                if let Some(u) = e.view_mut(v).and_then(|v| v.kind.underlay_mut()) {
                    u.repair(|l| l != level);
                }
            }
            e.levels_mut().retain(|l| l.id != level);
            Ok(())
        })?;
        Ok(json!({ "deleted": level.0 }))
    }
}
