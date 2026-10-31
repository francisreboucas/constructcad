//! Doors and windows as components: the built-in definitions that the
//! template's door and window types use and that older files' door and
//! window types become.
//!
//! A door or window component ([`ComponentKind::Door`] or `Window`, with a
//! [`HostOpening`]) is placed in a wall: its frame has `x` along the wall
//! from the opening's center, `y` across the wall from its location line
//! (positive on the side a door opens to) and `z` up from the sill; its
//! expressions may use [`WALL_THICKNESS`], the host wall's thickness.

use crate::component::{
    ComponentDef, ComponentItem, ComponentKind, ComponentParam, ComponentShape, HostOpening, ParamKind, WALL_THICKNESS,
};
use crate::{Project, TypeData};

/// Width of frame members (jambs, head, sash rails), meters.
const FRAME: f64 = 0.05;

/// Names of the built-in parameters and pieces, in the project's language.
#[derive(Debug, Clone)]
pub struct OpeningNames {
    pub door: String,
    pub window: String,
    pub width: String,
    pub height: String,
    pub sill: String,
    pub frame: String,
    pub leaf: String,
    pub sash: String,
    pub glass: String,
    pub sill_board: String,
}

impl Default for OpeningNames {
    /// The Portuguese names (the template's defaults).
    fn default() -> Self {
        Self::new(&|key| crate::project::template_default(key).to_string())
    }
}

impl OpeningNames {
    /// Names from `n(key)` (template keys `tpl-*`); parameter names that are
    /// not valid in expressions fall back to the Portuguese ones.
    pub fn new(n: &dyn Fn(&str) -> String) -> Self {
        let param = |key: &str| {
            let name = n(key);
            if crate::expr::is_valid_name(&name) && name != WALL_THICKNESS {
                name
            } else {
                crate::project::template_default(key).to_string()
            }
        };
        Self {
            door: n("tpl-comp-door"),
            window: n("tpl-comp-window"),
            width: param("tpl-param-width"),
            height: param("tpl-param-height"),
            sill: param("tpl-param-sill"),
            frame: n("tpl-piece-frame"),
            leaf: n("tpl-piece-leaf"),
            sash: n("tpl-piece-sash"),
            glass: n("tpl-piece-glass"),
            sill_board: n("tpl-piece-sill-board"),
        }
    }
}

fn length(name: &str, value: f64) -> ComponentParam {
    ComponentParam { name: name.to_string(), value, kind: ParamKind::Length, instance: false }
}

/// A box piece from its corner and sizes (expressions).
fn piece(name: &str, material: &str, [x, y, z, width, depth, height]: [String; 6]) -> ComponentItem {
    ComponentItem {
        name: name.to_string(),
        shape: ComponentShape::Box { x, y, z, width, depth, height },
        material: material.to_string(),
        in_plan: false,
        void: false,
        plan_only: false,
        detail: Default::default(),
    }
}

/// A swing door: two jambs and a head as deep as the wall, and a leaf of
/// `material` in the middle of the wall.
pub fn door_def(names: &OpeningNames, width: f64, height: f64, material: &str) -> ComponentDef {
    let (w, h, t) = (&names.width, &names.height, WALL_THICKNESS);
    let s = |text: &str| text.replace("{W}", w).replace("{H}", h).replace("{T}", t).replace("{F}", &FRAME.to_string());
    let frame = &names.frame;
    let items = vec![
        piece(frame, "", [s("-{W} / 2"), s("-{T} / 2"), s("0"), s("{F}"), s("{T}"), s("{H}")]),
        piece(frame, "", [s("{W} / 2 - {F}"), s("-{T} / 2"), s("0"), s("{F}"), s("{T}"), s("{H}")]),
        piece(frame, "", [s("-{W} / 2 + {F}"), s("-{T} / 2"), s("{H} - {F}"), s("{W} - 2 * {F}"), s("{T}"), s("{F}")]),
        piece(
            &names.leaf,
            material,
            [s("-{W} / 2 + {F}"), s("-0.02"), s("0"), s("{W} - 2 * {F}"), s("0.04"), s("{H} - {F}")],
        ),
    ];
    ComponentDef {
        kind: ComponentKind::Door,
        params: vec![length(w, width), length(h, height)],
        items,
        opening: Some(Box::new(HostOpening {
            width: w.clone(),
            height: h.clone(),
            sill: String::new(),
            own_symbol: false,
        })),
        ..Default::default()
    }
}

/// A window: a sash frame in the middle of the wall around a pane of
/// `material`, and a sill board across the wall under it.
pub fn window_def(names: &OpeningNames, width: f64, height: f64, sill: f64, material: &str) -> ComponentDef {
    let (w, h, t) = (&names.width, &names.height, WALL_THICKNESS);
    let s = |text: &str| text.replace("{W}", w).replace("{H}", h).replace("{T}", t).replace("{F}", &FRAME.to_string());
    let sash = &names.sash;
    let items = vec![
        piece(sash, "", [s("-{W} / 2"), s("-0.04"), s("0"), s("{F}"), s("0.08"), s("{H}")]),
        piece(sash, "", [s("{W} / 2 - {F}"), s("-0.04"), s("0"), s("{F}"), s("0.08"), s("{H}")]),
        piece(sash, "", [s("-{W} / 2 + {F}"), s("-0.04"), s("0"), s("{W} - 2 * {F}"), s("0.08"), s("{F}")]),
        piece(sash, "", [s("-{W} / 2 + {F}"), s("-0.04"), s("{H} - {F}"), s("{W} - 2 * {F}"), s("0.08"), s("{F}")]),
        piece(
            &names.sill_board,
            "",
            [s("-{W} / 2"), s("-{T} / 2 - 0.02"), s("-0.02"), s("{W}"), s("{T} + 0.02"), s("0.02")],
        ),
        piece(
            &names.glass,
            material,
            [s("-{W} / 2 + {F}"), s("-0.006"), s("{F}"), s("{W} - 2 * {F}"), s("0.012"), s("{H} - 2 * {F}")],
        ),
    ];
    ComponentDef {
        kind: ComponentKind::Window,
        params: vec![length(w, width), length(h, height), length(&names.sill, sill)],
        items,
        opening: Some(Box::new(HostOpening {
            width: w.clone(),
            height: h.clone(),
            sill: names.sill.clone(),
            own_symbol: false,
        })),
        ..Default::default()
    }
}

impl Project {
    /// Types of parametric components, of any category (doors and windows too).
    pub fn component_types(&self) -> impl Iterator<Item = &crate::ElementType> {
        self.types.values().filter(|t| matches!(t.data, TypeData::Component { .. }))
    }

    /// Turn door and window types of the old kind (fixed width, height and
    /// material) into types of the built-in door and window components, with
    /// the same sizes; elements keep their types. Returns how many changed.
    pub fn convert_opening_types(&mut self, n: &dyn Fn(&str) -> String) -> usize {
        let names = OpeningNames::new(n);
        let mut changed = 0;
        for ty in self.types.values_mut() {
            let converted = match &ty.data {
                TypeData::Door { width, height, material } => Some(TypeData::Component {
                    component: names.door.clone(),
                    def: door_def(&names, *width, *height, material),
                }),
                TypeData::Window { width, height, default_sill, material } => Some(TypeData::Component {
                    component: names.window.clone(),
                    def: window_def(&names, *width, *height, *default_sill, material),
                }),
                _ => None,
            };
            if let Some(data) = converted {
                ty.data = data;
                changed += 1;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Category, ElementType, ElementTypeId};

    fn names() -> OpeningNames {
        OpeningNames::new(&|key| crate::project::template_default(key).to_string())
    }

    #[test]
    fn built_in_doors_and_windows_are_openings_of_their_category() {
        let names = names();
        let door = door_def(&names, 0.8, 2.1, "Madeira");
        assert_eq!(door.opening_size(), Some((0.8, 2.1)));
        assert_eq!(door.default_sill(), 0.0);
        let window = window_def(&names, 1.2, 1.0, 1.1, "Vidro");
        assert_eq!(window.opening_size(), Some((1.2, 1.0)));
        assert!((window.default_sill() - 1.1).abs() < 1e-12);
        let ty = |def: ComponentDef| ElementType {
            id: ElementTypeId(1),
            name: "t".into(),
            data: TypeData::Component { component: "c".into(), def },
            info: Default::default(),
            uid: Default::default(),
        };
        assert_eq!(ty(door.clone()).category(), Category::Door);
        assert_eq!(ty(window.clone()).category(), Category::Window);
        // Every piece resolves in a wall 15 cm thick; the frame is as deep as the wall.
        let (parts, errors) = door.resolve_in(&Default::default(), &[(WALL_THICKNESS, 0.15)]);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(parts.len(), 4);
        let (lo, hi) = parts[0].footprint.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.y), hi.max(p.y)));
        assert!((hi - lo - 0.15).abs() < 1e-12);
        let (_, errors) = window.resolve_in(&Default::default(), &[(WALL_THICKNESS, 0.15)]);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn old_door_and_window_types_become_components() {
        let mut p = Project::empty();
        let id = ElementTypeId(p.alloc_id());
        let data = TypeData::Window { width: 1.5, height: 1.0, default_sill: 1.1, material: "Vidro".into() };
        p.types
            .insert(id, ElementType { id, name: "J".into(), data, info: Default::default(), uid: Default::default() });
        assert_eq!(p.convert_opening_types(&|key| crate::project::template_default(key).to_string()), 1);
        let ty = p.element_type(id).unwrap();
        assert_eq!(ty.category(), Category::Window);
        assert_eq!(ty.opening_size(), Some((1.5, 1.0)));
        assert!((ty.default_sill() - 1.1).abs() < 1e-12);
        assert_eq!(p.convert_opening_types(&|key| crate::project::template_default(key).to_string()), 0);
    }
}
