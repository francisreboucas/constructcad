//! Reference planes, reference lines and labeled dimensions of a
//! component: the skeleton that parameters drive and pieces follow.
//!
//! A reference plane is a named plane square to one axis of the component's
//! frame, at a position given by an [`crate::expr`] expression. Its name can
//! be used in any expression of the component (a piece's coordinates, other
//! planes, connectors), so a piece "locked" to a plane simply reads it:
//! `x = Right`. Three origin planes are always there, at zero:
//! [`CENTER_X`] (the plane x = 0, center left/right), [`CENTER_Y`] (y = 0,
//! center front/back) and [`BASE`] (z = 0).
//!
//! A dimension between two parallel planes shows their distance in one of
//! the component's views. Labeling it with a parameter writes the second
//! plane's position as the first plus (or minus) the parameter, so changing
//! the parameter moves the plane and whatever reads it. Everything stays an
//! expression; there is no constraint solver.

use crate::component::{ComponentDef, SketchPlane};
use crate::expr::{self, ExprError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The origin plane x = 0 (center left/right).
pub const CENTER_X: &str = "CenterX";
/// The origin plane y = 0 (center front/back).
pub const CENTER_Y: &str = "CenterY";
/// The origin plane z = 0 (the base).
pub const BASE: &str = "Base";

/// The origin planes: name and axis.
pub const ORIGIN_PLANES: [(&str, PlaneAxis); 3] =
    [(CENTER_X, PlaneAxis::X), (CENTER_Y, PlaneAxis::Y), (BASE, PlaneAxis::Z)];

/// Most reference planes, lines and dimensions of a component.
pub const MAX_REFS: usize = 512;

/// The axis a reference plane is square to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PlaneAxis {
    /// A plane x = constant (left/right): seen edge-on in the plan and front views.
    #[default]
    X,
    /// A plane y = constant (front/back): seen edge-on in the plan and side views.
    Y,
    /// A plane z = constant (a height): seen edge-on in the front and side views.
    Z,
}

impl PlaneAxis {
    pub const ALL: [PlaneAxis; 3] = [PlaneAxis::X, PlaneAxis::Y, PlaneAxis::Z];

    /// Where a plane square to this axis is seen edge-on in `view`: `Some(true)`
    /// as a line of constant u (vertical), `Some(false)` as a line of constant
    /// v (horizontal), `None` when the view looks along the plane's face.
    pub fn in_view(self, view: SketchPlane) -> Option<bool> {
        match (view, self) {
            (SketchPlane::Plan, PlaneAxis::X) | (SketchPlane::Front, PlaneAxis::X) => Some(true),
            (SketchPlane::Side, PlaneAxis::Y) => Some(true),
            (SketchPlane::Plan, PlaneAxis::Y) => Some(false),
            (SketchPlane::Front | SketchPlane::Side, PlaneAxis::Z) => Some(false),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            PlaneAxis::X => "comp-axis-x",
            PlaneAxis::Y => "comp-axis-y",
            PlaneAxis::Z => "comp-axis-z",
        }
    }
}

/// A named plane square to `axis` at the position `at` (an expression).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefPlane {
    pub name: String,
    pub axis: PlaneAxis,
    pub at: String,
}

/// A named line drawn on one of the sketch planes: the plane stands at
/// `offset` along its normal axis, and the ends are (u, v) expressions in
/// it (see [`SketchPlane`]). Revolution axes and angled references use it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefLine {
    pub name: String,
    pub plane: SketchPlane,
    #[serde(default = "zero")]
    pub offset: String,
    pub start: (String, String),
    pub end: (String, String),
}

fn zero() -> String {
    "0".into()
}

/// A dimension between the parallel reference planes `from` and `to`, shown
/// in `view`; `label` is the parameter that drives it, if any. `place` is
/// where its line sits across the planes (meters, in the view's other
/// coordinate).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefDimension {
    pub view: SketchPlane,
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub place: f64,
}

/// An equality (EQ): the plane `middle` stays halfway between the parallel
/// planes `a` and `b`, shown in `view` as two equal dimensions at `place`.
/// Its position is written `(a + b) / 2` when the equality is made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefEqual {
    pub view: SketchPlane,
    pub a: String,
    pub middle: String,
    pub b: String,
    #[serde(default)]
    pub place: f64,
}

impl RefEqual {
    /// The expression that keeps `middle` halfway.
    pub fn expression(&self) -> String {
        format!("({} + {}) / 2", self.a, self.b)
    }
}

/// Why a reference plane, line or dimension does not evaluate.
#[derive(Debug, Clone, PartialEq)]
pub enum RefError {
    /// Its expression fails (an unknown name also covers planes that
    /// depend on each other in a cycle).
    Expr { name: String, error: ExprError },
    /// A dimension names a missing plane, or planes that are not parallel.
    Dimension { index: usize },
    /// An equality names a missing plane, planes that are not parallel, or
    /// its middle plane is no longer halfway.
    Equal { index: usize },
}

impl std::fmt::Display for RefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefError::Expr { name, error } => write!(f, "{name}: {error}"),
            RefError::Dimension { index } => {
                write!(f, "dimension {}: its planes are missing or not parallel", index + 1)
            }
            RefError::Equal { index } => {
                write!(f, "equality {}: its planes are missing, not parallel or not kept halfway", index + 1)
            }
        }
    }
}

/// Every name that expressions of the component read, with its value: the
/// parameters (with overrides), `extra` names, the origin planes and the
/// reference planes that evaluate, plus the planes that do not.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scope {
    pub values: BTreeMap<String, f64>,
    pub errors: Vec<RefError>,
}

impl Scope {
    pub fn get(&self, name: &str) -> Option<f64> {
        self.values.get(name).copied()
    }
}

/// Whether `name` is one of the reserved names (the origin planes, a door's
/// wall thickness).
pub fn is_reserved(name: &str) -> bool {
    ORIGIN_PLANES.iter().any(|(n, _)| *n == name) || name == crate::component::WALL_THICKNESS
}

impl ComponentDef {
    /// The values every expression of the component reads (see [`Scope`]).
    /// Planes may read parameters and other planes in any order; a plane
    /// left unresolved (a missing name or a cycle) is reported once.
    pub fn scope(&self, overrides: &BTreeMap<String, f64>, extra: &[(&str, f64)]) -> Scope {
        let mut values = BTreeMap::new();
        for (name, v) in extra {
            values.insert((*name).to_string(), *v);
        }
        for (name, _) in ORIGIN_PLANES {
            values.insert(name.to_string(), 0.0);
        }
        // Parameters win over extra names of the same name.
        values.extend(self.values(overrides));
        // Bounded: a crafted file may hold any number of planes.
        let mut pending: Vec<&RefPlane> = self.ref_planes.iter().take(MAX_REFS).collect();
        // Each pass resolves at least one plane, or stops.
        while !pending.is_empty() {
            let before = pending.len();
            pending.retain(|p| {
                let lookup = |n: &str| values.get(n).copied();
                match expr::eval(&p.at, &lookup) {
                    Ok(v) => {
                        values.insert(p.name.clone(), v);
                        false
                    }
                    Err(_) => true,
                }
            });
            if pending.len() == before {
                break;
            }
        }
        let errors = pending
            .iter()
            .map(|p| {
                let lookup = |n: &str| values.get(n).copied();
                let error = expr::eval(&p.at, &lookup).err().unwrap_or(ExprError::NotFinite);
                RefError::Expr { name: p.name.clone(), error }
            })
            .collect();
        Scope { values, errors }
    }

    /// A reference plane by name, origin planes included (as `at: "0"`).
    pub fn ref_plane(&self, name: &str) -> Option<RefPlane> {
        if let Some((n, axis)) = ORIGIN_PLANES.iter().find(|(n, _)| *n == name) {
            return Some(RefPlane { name: (*n).to_string(), axis: *axis, at: "0".into() });
        }
        self.ref_planes.iter().find(|p| p.name == name).cloned()
    }

    /// Every plane, the origin ones first.
    pub fn all_ref_planes(&self) -> Vec<RefPlane> {
        ORIGIN_PLANES
            .iter()
            .map(|(n, axis)| RefPlane { name: (*n).to_string(), axis: *axis, at: "0".into() })
            .chain(self.ref_planes.iter().cloned())
            .collect()
    }

    /// Whether `name` is already used by a parameter, plane or line (or reserved).
    pub fn name_taken(&self, name: &str) -> bool {
        is_reserved(name)
            || self.params.iter().any(|p| p.name == name)
            || self.ref_planes.iter().any(|p| p.name == name)
            || self.ref_lines.iter().any(|l| l.name == name)
    }

    /// The value a dimension shows: the distance between its planes, or why not.
    pub fn dimension_value(&self, scope: &Scope, index: usize) -> Result<f64, RefError> {
        let bad = || RefError::Dimension { index };
        let d = self.dimensions.get(index).ok_or_else(bad)?;
        let (a, b) = (self.ref_plane(&d.from).ok_or_else(bad)?, self.ref_plane(&d.to).ok_or_else(bad)?);
        if a.axis != b.axis || a.axis.in_view(d.view).is_none() {
            return Err(bad());
        }
        Ok((scope.get(&d.to).ok_or_else(bad)? - scope.get(&d.from).ok_or_else(bad)?).abs())
    }

    /// Whether equality `index` holds: three parallel planes seen edge-on
    /// in its view, the middle one written halfway between the others.
    pub fn equal_holds(&self, index: usize) -> Result<(), RefError> {
        let bad = || RefError::Equal { index };
        let q = self.equals.get(index).ok_or_else(bad)?;
        let planes: Vec<RefPlane> =
            [&q.a, &q.middle, &q.b].iter().map(|n| self.ref_plane(n)).collect::<Option<_>>().ok_or_else(bad)?;
        let axis = planes.first().map(|p| p.axis).ok_or_else(bad)?;
        if planes.iter().any(|p| p.axis != axis) || axis.in_view(q.view).is_none() {
            return Err(bad());
        }
        let middle = planes.get(1).ok_or_else(bad)?;
        if middle.at.replace(' ', "") != q.expression().replace(' ', "") {
            return Err(bad());
        }
        Ok(())
    }

    /// The names an expression of the component reads, other than parameters
    /// and planes, that do not exist; and repeated or invalid names. Empty
    /// when the references are sound.
    pub fn ref_problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen: Vec<&str> = self.params.iter().map(|p| p.name.as_str()).collect();
        for name in self.ref_planes.iter().map(|p| &p.name).chain(self.ref_lines.iter().map(|l| &l.name)) {
            if !expr::is_valid_name(name) || is_reserved(name) || seen.contains(&name.as_str()) {
                out.push(format!("invalid or repeated name '{name}'"));
            }
            seen.push(name);
        }
        for i in 0..self.equals.len() {
            if let Err(e) = self.equal_holds(i) {
                out.push(e.to_string());
            }
        }
        if self.ref_planes.len() + self.ref_lines.len() + self.dimensions.len() + self.equals.len() > MAX_REFS {
            out.push("too many reference planes, lines and dimensions".into());
        }
        let scope = self.scope(&BTreeMap::new(), &[(crate::component::WALL_THICKNESS, 0.2)]);
        out.extend(scope.errors.iter().map(|e| e.to_string()));
        for (i, d) in self.dimensions.iter().enumerate() {
            if let Err(e) = self.dimension_value(&scope, i) {
                out.push(e.to_string());
            }
            if let Some(label) = &d.label
                && self.param(label).is_none()
            {
                out.push(format!("dimension {}: unknown parameter '{label}'", i + 1));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::{ComponentItem, ComponentParam, ComponentShape, ParamKind};

    fn param(name: &str, value: f64) -> ComponentParam {
        ComponentParam { name: name.into(), value, kind: ParamKind::Length, instance: false }
    }

    fn plane(name: &str, axis: PlaneAxis, at: &str) -> RefPlane {
        RefPlane { name: name.into(), axis, at: at.into() }
    }

    /// A box between the Left and Right planes and up to Top.
    fn boxed() -> ComponentDef {
        ComponentDef {
            params: vec![param("Width", 1.2), param("Height", 0.75)],
            // Out of order: Right reads Left, which comes after it.
            ref_planes: vec![
                plane("Right", PlaneAxis::X, "Left + Width"),
                plane("Left", PlaneAxis::X, "-Width/2"),
                plane("Top", PlaneAxis::Z, "Base + Height"),
            ],
            items: vec![ComponentItem {
                name: "Top".into(),
                shape: ComponentShape::Box {
                    x: "Left".into(),
                    y: "-0.4".into(),
                    z: "Top - 0.03".into(),
                    width: "Right - Left".into(),
                    depth: "0.8".into(),
                    height: "0.03".into(),
                },
                material: String::new(),
                in_plan: true,
                void: false,
                plan_only: false,
                detail: Default::default(),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn planes_resolve_in_any_order_and_pieces_read_them() {
        let def = boxed();
        let scope = def.scope(&BTreeMap::new(), &[]);
        assert!(scope.errors.is_empty(), "{:?}", scope.errors);
        assert_eq!(scope.get("Left"), Some(-0.6));
        assert_eq!(scope.get("Right"), Some(0.6));
        assert_eq!(scope.get(CENTER_X), Some(0.0));
        let (parts, errors) = def.resolve(&BTreeMap::new());
        assert!(errors.is_empty(), "{errors:?}");
        assert!((parts[0].z1 - 0.75).abs() < 1e-12);
        let xs: Vec<f64> = parts[0].footprint.iter().map(|p| p.x).collect();
        assert!(xs.iter().any(|x| (x + 0.6).abs() < 1e-12) && xs.iter().any(|x| (x - 0.6).abs() < 1e-12));
        assert!(def.ref_problems().is_empty(), "{:?}", def.ref_problems());
    }

    #[test]
    fn cycles_and_bad_names_are_reported() {
        let mut def = boxed();
        def.ref_planes[1].at = "Right - Width".into();
        let scope = def.scope(&BTreeMap::new(), &[]);
        assert_eq!(scope.errors.len(), 2, "{:?}", scope.errors);
        assert!(!def.ref_problems().is_empty());
        let mut def = boxed();
        def.ref_planes[0].name = "Width".into();
        assert!(def.ref_problems().iter().any(|p| p.contains("Width")));
        def.ref_planes[0].name = BASE.into();
        assert!(!def.ref_problems().is_empty());
        assert!(def.name_taken("Height") && def.name_taken(CENTER_Y) && !def.name_taken("Depth"));
    }

    #[test]
    fn equalities_keep_a_plane_halfway() {
        let mut def = boxed();
        def.ref_planes.push(plane("Mid", PlaneAxis::X, "(Left + Right) / 2"));
        def.equals.push(RefEqual {
            view: SketchPlane::Plan,
            a: "Left".into(),
            middle: "Mid".into(),
            b: "Right".into(),
            place: 0.0,
        });
        assert!(def.equal_holds(0).is_ok());
        assert!(def.ref_problems().is_empty(), "{:?}", def.ref_problems());
        // Renaming follows into the equality and its expression.
        def.rename("Left", "L");
        assert_eq!(def.equals[0].a, "L");
        assert!(def.equal_holds(0).is_ok(), "{:?}", def.ref_planes);
        // A middle moved by hand breaks it; so does a plane of another axis.
        def.ref_planes[3].at = "0.1".into();
        assert!(def.equal_holds(0).is_err());
        def.ref_planes[3].at = "(L + Right) / 2".into();
        def.equals[0].b = "Top".into();
        assert!(def.equal_holds(0).is_err());
    }

    #[test]
    fn dimensions_measure_parallel_planes_in_views_that_show_them() {
        let mut def = boxed();
        let dim = |view, from: &str, to: &str| RefDimension {
            view,
            from: from.into(),
            to: to.into(),
            label: Some("Width".into()),
            place: 0.0,
        };
        def.dimensions = vec![dim(SketchPlane::Plan, "Left", "Right"), dim(SketchPlane::Side, "Left", "Right")];
        def.dimensions.push(dim(SketchPlane::Front, BASE, "Top"));
        def.dimensions.push(dim(SketchPlane::Front, "Left", "Top"));
        let scope = def.scope(&BTreeMap::new(), &[]);
        assert!((def.dimension_value(&scope, 0).unwrap() - 1.2).abs() < 1e-12);
        // The side view looks along x: x planes are not seen edge-on there.
        assert!(def.dimension_value(&scope, 1).is_err());
        assert!((def.dimension_value(&scope, 2).unwrap() - 0.75).abs() < 1e-12);
        assert!(def.dimension_value(&scope, 3).is_err());
        assert_eq!(PlaneAxis::Z.in_view(SketchPlane::Plan), None);
    }
}
