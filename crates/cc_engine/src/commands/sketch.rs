//! Sketched outlines: the curves of floors, ceilings, roofs and masses
//! (see [`cc_core::sketch`]), given to the commands that create them as
//! `curves`, read with `sketch_get` and drawn again with `sketch_set`.

use super::create::{check_openings, check_roof_holes};
use super::edit;
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, finite, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::ElementKind;
use cc_core::sketch::{Profile, Sketch, SketchCurve, SketchError};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A curve of a sketch. Rectangles and polygons become their sides (lines).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CurveArg {
    /// A straight line.
    Line {
        /// Start point `[x, y]`.
        start: Point,
        /// End point `[x, y]`.
        end: Point,
    },
    /// A circular arc between two points: by its `bulge` (tan of a quarter of
    /// the turn, positive counter-clockwise, 1 for a half circle) or a point
    /// it passes `through`.
    Arc {
        /// Start point `[x, y]`.
        start: Point,
        /// End point `[x, y]`.
        end: Point,
        /// Bulge: tan(sweep / 4), positive turning counter-clockwise.
        #[serde(default)]
        bulge: Option<f64>,
        /// A point on the arc between its ends.
        #[serde(default)]
        through: Option<Point>,
    },
    /// A whole circle.
    Circle {
        /// Center `[x, y]`.
        center: Point,
        /// Radius, meters.
        radius: f64,
    },
    /// An ellipse, or a part of it from `start` turning by `sweep` degrees
    /// (of its parameter angle).
    Ellipse {
        /// Center `[x, y]`.
        center: Point,
        /// Semi-axes `[along its x, along its y]`, meters.
        radii: Point,
        /// Angle of its x axis from the plan's, degrees (default 0).
        #[serde(default)]
        rotation: f64,
        /// Parameter angle where a partial ellipse starts, degrees (default 0).
        #[serde(default)]
        start: f64,
        /// Turn of a partial ellipse, degrees (default 360: the whole ellipse).
        #[serde(default)]
        sweep: Option<f64>,
    },
    /// A rectangle from a corner to the opposite one, turned `rotation`
    /// degrees about the first corner: four lines.
    Rectangle {
        /// First corner `[x, y]`.
        corner: Point,
        /// Opposite corner `[x, y]`, before the rotation.
        opposite: Point,
        /// Rotation about the first corner, degrees (default 0).
        #[serde(default)]
        rotation: f64,
    },
    /// A regular polygon: its sides as lines.
    Polygon {
        /// Center `[x, y]`.
        center: Point,
        /// Distance from the center to the corners (`inscribed`, the default)
        /// or to the middle of the sides.
        radius: f64,
        /// Number of sides, 3 to 64.
        sides: usize,
        /// Whether `radius` reaches the corners (true) or the sides (false).
        #[serde(default = "yes")]
        inscribed: bool,
        /// Angle of the first corner from the plan's x axis, degrees (default 0).
        #[serde(default)]
        rotation: f64,
    },
}

fn yes() -> bool {
    true
}

impl CurveArg {
    /// The sketch curves this argument draws.
    pub fn curves(&self) -> Result<Vec<SketchCurve>> {
        let curves = match *self {
            CurveArg::Line { start, end } => vec![SketchCurve::Line { a: point(start)?, b: point(end)? }],
            CurveArg::Arc { start, end, bulge, through } => {
                let (a, b) = (point(start)?, point(end)?);
                let bulge = match (bulge, through) {
                    (Some(v), None) => finite(v, "bulge")?,
                    (None, Some(m)) => cc_core::arc::Arc::bulge_through(a, point(m)?, b)
                        .ok_or_else(|| EngineError::invalid("the arc's three points are in line"))?,
                    _ => return Err(EngineError::invalid("give an arc's bulge or a point it passes through")),
                };
                vec![SketchCurve::Arc { a, b, bulge }]
            }
            CurveArg::Circle { center, radius } => {
                vec![SketchCurve::Circle { center: point(center)?, radius: finite(radius, "radius")? }]
            }
            CurveArg::Ellipse { center, radii, rotation, start, sweep } => vec![SketchCurve::Ellipse {
                center: point(center)?,
                radii: point(radii)?,
                rotation_deg: finite(rotation, "rotation")?,
                start_deg: finite(start, "start")?,
                sweep_deg: finite(sweep.unwrap_or(360.0), "sweep")?.clamp(-360.0, 360.0),
            }],
            CurveArg::Rectangle { corner, opposite, rotation } => {
                let (a, c) = (point(corner)?, point(opposite)?);
                let turn = DVec2::from_angle(finite(rotation, "rotation")?.to_radians());
                let at = |p: DVec2| a + turn.rotate(p - a);
                let pts = [a, at(DVec2::new(c.x, a.y)), at(c), at(DVec2::new(a.x, c.y))];
                lines(&pts)
            }
            CurveArg::Polygon { center, radius, sides, inscribed, rotation } => {
                if !(3..=64).contains(&sides) {
                    return Err(EngineError::invalid("a polygon has 3 to 64 sides"));
                }
                let c = point(center)?;
                let step = std::f64::consts::TAU / sides as f64;
                // Circumscribed about a circle: its corners lie farther out.
                let r = finite(radius, "radius")? / if inscribed { 1.0 } else { (step / 2.0).cos() };
                let start = finite(rotation, "rotation")?.to_radians();
                let pts: Vec<DVec2> = (0..sides).map(|i| c + DVec2::from_angle(start + step * i as f64) * r).collect();
                lines(&pts)
            }
        };
        match curves.iter().position(|c| !c.is_valid()) {
            Some(_) => Err(EngineError::invalid("a curve has no size")),
            None => Ok(curves),
        }
    }

    /// The argument that gives back `curve`.
    pub fn of(curve: &SketchCurve) -> Self {
        let p = |v: DVec2| v.to_array();
        match *curve {
            SketchCurve::Line { a, b } => CurveArg::Line { start: p(a), end: p(b) },
            SketchCurve::Arc { a, b, bulge } => {
                CurveArg::Arc { start: p(a), end: p(b), bulge: Some(bulge), through: None }
            }
            SketchCurve::Circle { center, radius } => CurveArg::Circle { center: p(center), radius },
            SketchCurve::Ellipse { center, radii, rotation_deg, start_deg, sweep_deg } => CurveArg::Ellipse {
                center: p(center),
                radii: p(radii),
                rotation: rotation_deg,
                start: start_deg,
                sweep: Some(sweep_deg),
            },
        }
    }
}

/// The sides of a closed polygon as lines.
fn lines(pts: &[DVec2]) -> Vec<SketchCurve> {
    let n = pts.len();
    (0..n).map(|i| SketchCurve::Line { a: pts[i], b: pts[(i + 1) % n] }).collect()
}

/// The sketch the arguments draw.
pub fn sketch_of(args: &[CurveArg]) -> Result<Sketch> {
    let mut curves = Vec::new();
    for a in args {
        curves.extend(a.curves()?);
        if curves.len() > cc_core::sketch::MAX_CURVES {
            return Err(EngineError::invalid("the sketch has too many curves"));
        }
    }
    Ok(Sketch { curves })
}

/// A sketch's error as a command error.
pub fn sketch_error(e: SketchError) -> EngineError {
    EngineError::invalid(e.to_string())
}

/// The outline and openings of a sketch.
pub fn profile(sketch: &Sketch) -> Result<Profile> {
    sketch.profile().map_err(sketch_error)
}

/// The outline of a sketch that takes no openings.
pub fn outline(sketch: &Sketch) -> Result<Vec<DVec2>> {
    sketch.outline().map_err(sketch_error)
}

/// The curves an element's outline is drawn with: its sketch, or lines along
/// its outline and openings when it has none (floors, ceilings, roofs and masses).
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SketchGet {
    /// Element id.
    pub id: u64,
}

impl Command for SketchGet {
    const NAME: &'static str = "sketch_get";
    const KIND: CommandKind = CommandKind::Query;
    fn run(self, s: &mut Session) -> Result<Value> {
        let p = s.project();
        let id = refs::element(p, self.id)?;
        let e = p.element(id).ok_or_else(|| EngineError::not_found(format!("no element {}", self.id)))?;
        let (outline, openings) =
            e.sketched_profile().ok_or_else(|| EngineError::invalid("the element has no sketched outline"))?;
        let (sketch, drawn) = match e.drawn_sketch() {
            Some(s) => (s.clone(), true),
            None => (Sketch::from_polygons(outline, openings), false),
        };
        let curves: Vec<CurveArg> = sketch.curves.iter().map(CurveArg::of).collect();
        Ok(json!({ "id": id.0, "curves": curves, "drawn": drawn }))
    }
}

/// Draw an element's outline again from curves (edit its boundary): floors
/// and roofs take the loops inside the outline as their openings; ceilings
/// and masses take one loop. The element keeps the curves, so `sketch_get`
/// gives them back.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SketchSet {
    /// Element id (a floor, ceiling, roof or mass).
    pub id: u64,
    /// The curves, closed into loops.
    pub curves: Vec<CurveArg>,
}

impl Command for SketchSet {
    const NAME: &'static str = "sketch_set";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let id = refs::element(s.project(), self.id)?;
        let sketch = sketch_of(&self.curves)?;
        let mut kind =
            s.project().element(id).map(|e| e.kind.clone()).ok_or_else(|| EngineError::not_found("element"))?;
        match &mut kind {
            ElementKind::Floor(f) => {
                let p = profile(&sketch)?;
                check_openings(&p.outline, &p.openings)?;
                (f.boundary, f.openings) = (p.outline, p.openings);
                f.sketch = Some(sketch);
            }
            ElementKind::Roof(r) => {
                let p = profile(&sketch)?;
                if p.outline.len() != r.footprint.len() {
                    r.edge_slopes.clear();
                }
                (r.footprint, r.holes) = (p.outline, p.openings);
                check_roof_holes(r)?;
                r.sketch = Some(sketch);
            }
            ElementKind::Ceiling(c) => {
                c.boundary = outline(&sketch)?;
                c.sketch = Some(sketch);
            }
            ElementKind::Mass(m) => {
                m.outline = outline(&sketch)?;
                m.sketch = Some(sketch);
            }
            _ => return Err(EngineError::invalid("only floors, ceilings, roofs and masses have a sketched outline")),
        }
        edit(s, t!("tx-modify"), |e| {
            let el = e.element_mut(id).ok_or_else(|| EngineError::not_found("element"))?;
            el.kind = kind;
            Ok(())
        })?;
        Ok(json!({ "id": id.0 }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_become_curves() {
        let rect = CurveArg::Rectangle { corner: [0.0, 0.0], opposite: [4.0, 2.0], rotation: 90.0 };
        let curves = rect.curves().unwrap();
        assert_eq!(curves.len(), 4);
        let p = Sketch { curves }.outline().unwrap();
        assert!(p.iter().any(|q| q.distance(DVec2::new(-2.0, 4.0)) < 1e-9), "{p:?}");
        let hex = CurveArg::Polygon { center: [0.0, 0.0], radius: 1.0, sides: 6, inscribed: false, rotation: 0.0 };
        let p = Sketch { curves: hex.curves().unwrap() }.outline().unwrap();
        // Circumscribed about the unit circle: its sides are 1 from the center.
        let mid = (p[0] + p[1]) / 2.0;
        assert!((mid.length() - 1.0).abs() < 1e-9);
        let arc = CurveArg::Arc { start: [1.0, 0.0], end: [-1.0, 0.0], bulge: None, through: Some([0.0, 1.0]) };
        let Ok(curves) = arc.curves() else { panic!("an arc") };
        assert!(matches!(curves[0], SketchCurve::Arc { bulge, .. } if (bulge - 1.0).abs() < 1e-9));
        assert!(
            CurveArg::Polygon { center: [0.0, 0.0], radius: 1.0, sides: 2, inscribed: true, rotation: 0.0 }
                .curves()
                .is_err()
        );
        assert!(CurveArg::Circle { center: [0.0, 0.0], radius: 0.0 }.curves().is_err());
    }
}
