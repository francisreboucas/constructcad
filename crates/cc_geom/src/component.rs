//! Component pieces as solids: the definition's pieces resolved with their
//! parameter values, and the void pieces cut out of the solid pieces they
//! overlap. Script pieces run through the process's shared script cache
//! ([`cc_script::shared`]).

use crate::{Solid, polygon};
use cc_core::component::{ComponentPart, PartError, ResolveCtx};
use cc_core::component_script::ScriptOwner;
use cc_core::{Category, ComponentDef, ElementId, ElementType};
use glam::{DVec2, DVec3};
use std::collections::BTreeMap;

/// The solid pieces of `def` with `overrides`, in the component's frame,
/// voids cut out. A piece a void reaches becomes faces; past the boolean
/// budget it stays whole. Pieces whose expressions fail are left out.
pub fn resolve_component(def: &ComponentDef, overrides: &BTreeMap<String, f64>) -> Vec<ComponentPart> {
    resolve_component_in(def, overrides, &[])
}

/// [`resolve_component`] with `extra` names defined (a door's wall thickness).
pub fn resolve_component_in(
    def: &ComponentDef,
    overrides: &BTreeMap<String, f64>,
    extra: &[(&str, f64)],
) -> Vec<ComponentPart> {
    let ctx = ResolveCtx { overrides, extra, scripts: cc_script::shared(), ..ResolveCtx::default() };
    cut_voids(def.resolve_with(&ctx).0)
}

/// Every piece of `def` and the errors of those that fail (script pieces
/// run), without cutting voids: for editors and checks.
pub fn resolve_pieces(
    def: &ComponentDef,
    overrides: &BTreeMap<String, f64>,
    extra: &[(&str, f64)],
) -> (Vec<ComponentPart>, Vec<(usize, PartError)>) {
    def.resolve_with(&ResolveCtx { overrides, extra, scripts: cc_script::shared(), ..ResolveCtx::default() })
}

/// The solid pieces of a placed component of type `ty` (its instance
/// values, text values and moved hotspots), voids cut out. A failing script
/// shows the last pieces that worked for this element.
pub fn resolve_instance(
    ty: &ElementType,
    def: &ComponentDef,
    element: &cc_core::Element,
    component: Option<&cc_core::Component>,
    extra: &[(&str, f64)],
) -> Vec<ComponentPart> {
    instance_pieces(ty, element, component, extra, &|ctx| cut_voids(def.resolve_with(ctx).0))
}

/// The errors of a placed component's pieces (a failing script's among them).
pub fn instance_errors(
    ty: &ElementType,
    def: &ComponentDef,
    element: &cc_core::Element,
    component: Option<&cc_core::Component>,
) -> Vec<(usize, PartError)> {
    instance_pieces(ty, element, component, &[], &|ctx| def.resolve_with(ctx).1)
}

fn instance_pieces<T>(
    ty: &ElementType,
    element: &cc_core::Element,
    component: Option<&cc_core::Component>,
    extra: &[(&str, f64)],
    f: &dyn Fn(&ResolveCtx) -> T,
) -> T {
    let empty = cc_core::Component::default();
    let c = component.unwrap_or(&empty);
    let ctx = ResolveCtx {
        overrides: &c.overrides,
        texts: &c.text_overrides,
        hotspots: &c.hotspots,
        extra,
        scripts: cc_script::shared(),
        owner: ScriptOwner { type_uid: ty.uid, element: Some(element.uid), piece: 0 },
    };
    f(&ctx)
}

/// `parts` with the void pieces cut out of the solid pieces.
fn cut_voids(parts: Vec<ComponentPart>) -> Vec<ComponentPart> {
    // Plan symbols take no part in the solids.
    let (symbols, parts): (Vec<ComponentPart>, Vec<ComponentPart>) = parts.into_iter().partition(|p| p.plan_only);
    let (voids, mut solids): (Vec<ComponentPart>, Vec<ComponentPart>) = parts.into_iter().partition(|p| p.void);
    if voids.is_empty() {
        solids.extend(symbols);
        return solids;
    }
    let cutters: Vec<Vec<Vec<DVec3>>> = voids.iter().map(|v| triangulated(&faces_of(v))).collect();
    for part in &mut solids {
        let mut body = faces_of(part);
        let mut cut = false;
        for cutter in &cutters {
            if !crate::csg::bounds_overlap(&body, cutter) {
                continue;
            }
            if let Some(rest) = crate::csg::subtract(&triangulated(&body), cutter) {
                body = rest;
                cut = true;
            }
        }
        if !cut {
            continue;
        }
        let all: Vec<DVec3> = body.iter().flatten().copied().collect();
        if all.is_empty() {
            // Cut away entirely.
            part.footprint.clear();
            part.faces.clear();
            continue;
        }
        let (z0, z1) = all.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.z), hi.max(p.z)));
        part.footprint = cc_core::component::convex_hull(&all.iter().map(|p| p.truncate()).collect::<Vec<_>>());
        part.z0 = z0;
        part.z1 = z1;
        part.faces = body;
    }
    solids.retain(|p| p.footprint.len() >= 3);
    solids.extend(symbols);
    solids
}

/// The closed faces of a piece (a prism's are built from its footprint).
pub fn faces_of(part: &ComponentPart) -> Vec<Vec<DVec3>> {
    if !part.faces.is_empty() {
        return part.faces.clone();
    }
    let mut s = Solid::new(ElementId(0), Category::Component, [0; 3]);
    s.add_prism(&part.footprint, |_| part.z0, |_| part.z1);
    s.faces.into_iter().map(|f| f.verts).collect()
}

/// `faces` with every non-convex face split into triangles of the same
/// winding (the boolean operations clip convex polygons only).
fn triangulated(faces: &[Vec<DVec3>]) -> Vec<Vec<DVec3>> {
    let mut out = Vec::with_capacity(faces.len());
    for f in faces {
        let Some(normal) = newell(f) else { continue };
        if f.len() == 3 || is_convex(f, normal) {
            out.push(f.clone());
            continue;
        }
        // Drop the dominant axis of the normal and triangulate in that plane.
        let a = normal.abs();
        let flat: Vec<DVec2> = f
            .iter()
            .map(|p| {
                if a.z >= a.x && a.z >= a.y {
                    DVec2::new(p.x, p.y)
                } else if a.y >= a.x {
                    DVec2::new(p.z, p.x)
                } else {
                    DVec2::new(p.y, p.z)
                }
            })
            .collect();
        for [i, j, k] in polygon::triangulate(&flat) {
            let (Some(&p), Some(&q), Some(&r)) = (f.get(i), f.get(j), f.get(k)) else { continue };
            let tri = if (q - p).cross(r - p).dot(normal) >= 0.0 { vec![p, q, r] } else { vec![p, r, q] };
            out.push(tri);
        }
    }
    out
}

fn newell(f: &[DVec3]) -> Option<DVec3> {
    let n = f.len();
    (0..n)
        .map(|i| {
            let (a, b) = (f[i], f[(i + 1) % n]);
            DVec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y))
        })
        .sum::<DVec3>()
        .try_normalize()
}

fn is_convex(f: &[DVec3], normal: DVec3) -> bool {
    let n = f.len();
    (0..n).all(|i| {
        let (a, b, c) = (f[i], f[(i + 1) % n], f[(i + 2) % n]);
        (b - a).cross(c - b).dot(normal) >= -1e-12
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::{ComponentItem, ComponentShape, ProfilePoint, SketchPlane};

    fn item(name: &str, shape: ComponentShape, void: bool) -> ComponentItem {
        ComponentItem {
            name: name.into(),
            shape,
            material: String::new(),
            in_plan: true,
            void,
            plan_only: false,
            detail: Default::default(),
        }
    }

    fn volume(faces: &[Vec<DVec3>]) -> f64 {
        let mut s = Solid::new(ElementId(0), Category::Component, [0; 3]);
        s.add_polygons(faces.to_vec());
        s.volume()
    }

    #[test]
    fn voids_cut_the_pieces_they_reach() {
        let bx = |x: &str, w: &str, h: &str| ComponentShape::Box {
            x: x.into(),
            y: "0".into(),
            z: "0".into(),
            width: w.into(),
            depth: "1".into(),
            height: h.into(),
        };
        // A 1 m cube with a 0.5 m deep basin on top, an L-shaped (concave) block apart.
        let l_shape = ComponentShape::Profile {
            plane: SketchPlane::Front,
            points: vec![
                ProfilePoint::new("3", "0"),
                ProfilePoint::new("5", "0"),
                ProfilePoint::new("5", "1"),
                ProfilePoint::new("4", "1"),
                ProfilePoint::new("4", "2"),
                ProfilePoint::new("3", "2"),
            ],
            offset: "0".into(),
            depth: "1".into(),
        };
        // A round basin: a 0.3 m radius turned about the cube's vertical axis, from 0.5 m up.
        let basin = ComponentShape::Revolution {
            plane: SketchPlane::Front,
            points: vec![
                ProfilePoint::new("0.5", "0.5"),
                ProfilePoint::new("0.8", "0.5"),
                ProfilePoint::new("0.8", "1.5"),
                ProfilePoint::new("0.5", "1.5"),
            ],
            offset: "0.5".into(),
            axis: "0.5".into(),
            angle: "360".into(),
        };
        let def = ComponentDef {
            items: vec![
                item("cube", bx("0", "1", "1"), false),
                item("block", l_shape, false),
                item("basin", basin, true),
                // A slot through the L's arm, past its faces.
                item(
                    "slot",
                    ComponentShape::Box {
                        x: "3.4".into(),
                        y: "-0.1".into(),
                        z: "-0.1".into(),
                        width: "0.2".into(),
                        depth: "1.2".into(),
                        height: "3".into(),
                    },
                    true,
                ),
            ],
            ..Default::default()
        };
        let parts = resolve_component(&def, &BTreeMap::new());
        assert_eq!(parts.len(), 2, "voids are not pieces");
        // The basin takes a 48-sided prism of radius 0.3 and height 0.5 out of the cube.
        let cube = volume(&parts[0].faces);
        let basin = 0.5 * 48.0 * 0.09 * (std::f64::consts::TAU / 48.0).sin() * 0.5;
        assert!((1.0 - cube - basin).abs() < 1e-6, "{cube}");
        // The slot takes 0.2 x 2 m out of the L's 3 m² profile, across its 1 m depth.
        let block = volume(&parts[1].faces);
        assert!((block - 2.6).abs() < 1e-6, "{block}");
        // Pieces no void reaches stay prisms.
        let def = ComponentDef { items: vec![item("cube", bx("0", "1", "1"), false)], ..Default::default() };
        assert!(resolve_component(&def, &BTreeMap::new())[0].faces.is_empty());
    }

    #[test]
    fn placed_script_components_use_their_values_and_keep_the_last_pieces() {
        use cc_core::component_script::ParamChoice;
        let source = "param.length { 'width', default = 1, instance = true }\n\
                      param.choice { 'finish', { {'oak', 'Oak'}, {'pine', 'Pine'} }, instance = true }\n\
                      hotspot { 'top', 0, 0, 2 }\n\
                      function geometry(p)\n\
                        assert(p.width > 0, 'width must be positive')\n\
                        return { box { 0, 0, 0, p.width, 1, p.top.z, material = p.finish } }\n\
                      end";
        let decl = cc_script::declare(source).unwrap();
        let mut def = ComponentDef {
            items: vec![item("script", ComponentShape::Script { source: source.into() }, false)],
            ..Default::default()
        };
        cc_script::object::sync(&mut def.params, &mut def.hotspots, &decl).unwrap();
        assert_eq!(def.param("finish").unwrap().choices[1], ParamChoice { key: "pine".into(), label: "Pine".into() });
        let ty = ElementType {
            id: cc_core::ElementTypeId(1),
            name: "Scripted".into(),
            data: cc_core::TypeData::Component { component: "Scripted".into(), def: def.clone() },
            info: Default::default(),
            uid: cc_core::Uid(7),
        };
        let element = cc_core::Element {
            id: ElementId(2),
            level: cc_core::LevelId(0),
            type_id: ty.id,
            mark: String::new(),
            comments: String::new(),
            kind: cc_core::ElementKind::Component(Default::default()),
            uid: cc_core::Uid(8),
            phase_created: Default::default(),
            phase_demolished: None,
        };
        let mut c = cc_core::Component::default();
        c.overrides.insert("width".into(), 3.0);
        c.text_overrides.insert("finish".into(), "pine".into());
        c.hotspots.insert("top".into(), [0.0, 0.0, 4.0]);
        let parts = resolve_instance(&ty, &def, &element, Some(&c), &[]);
        assert_eq!(parts.len(), 1);
        assert_eq!((parts[0].material.as_str(), parts[0].z1), ("pine", 4.0));
        assert_eq!(parts[0].footprint[1].x, 3.0);
        // A failing run shows the last pieces that worked for this element and reports the error.
        c.overrides.insert("width".into(), -1.0);
        let again = resolve_instance(&ty, &def, &element, Some(&c), &[]);
        assert_eq!(again, parts);
        let errors = instance_errors(&ty, &def, &element, Some(&c));
        assert!(
            matches!(&errors[..], [(0, PartError::Script(m))] if m.contains("width must be positive")),
            "{errors:?}"
        );
    }
}
