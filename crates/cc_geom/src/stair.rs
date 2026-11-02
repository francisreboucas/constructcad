//! Stair geometry: the treads, landings and winders of a stair element laid
//! out from its shape, the solids of its construction (solid, cast in place,
//! assembled) and the paths its railings follow.
//!
//! Shapes are built in a local frame `(a, b)`: `a` along the first flight
//! from the foot of the first riser, `b` across it towards the side the stair
//! turns to; a stair turning right is the mirror image.

use crate::Solid;
use crate::polygon;
use cc_core::{
    Project, StairBuild, StairCheck, StairConstruction, StairLayout, StairRules, StairShape, StairSide, Stringers,
    TypeData,
};
use glam::{DVec2, DVec3};
use std::f64::consts::{FRAC_PI_2, TAU};

/// What a stair step is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    Tread,
    /// A tapered tread turning a corner (winders, spiral treads).
    Winder,
    Landing,
}

/// One walking surface of a stair: a tread, winder or landing.
#[derive(Debug, Clone, PartialEq)]
pub struct StairStep {
    /// Plan outline (world).
    pub outline: Vec<DVec2>,
    /// Position of each outline vertex from the front of the step (0) to its
    /// back (1), along the way up: the pitch of soffits and stringers.
    pub along: Vec<f64>,
    /// Front edge (the riser line), in plan.
    pub front: [DVec2; 2],
    /// Elevation of the walking surface.
    pub z: f64,
    pub kind: StepKind,
}

/// A straight flight: its treads go up along `dir` from `origin` (the
/// middle of its first riser).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flight {
    pub origin: DVec2,
    pub dir: DVec2,
    /// Unit vector to the left of `dir`.
    pub left: DVec2,
    pub treads: usize,
    /// Elevation of the first tread and of the floor under its riser.
    pub z0: f64,
    pub floor: f64,
    /// Depth of the landing its last riser rises to (0: it ends on the floor above).
    pub landing_ahead: f64,
}

/// A spiral stair's circle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spiral {
    pub center: DVec2,
    pub inner: f64,
    pub outer: f64,
}

/// Layout of a stair element.
#[derive(Debug, Clone, PartialEq)]
pub struct StairGeom {
    pub start: DVec2,
    /// Direction of the first flight and the perpendicular to its left.
    pub u: DVec2,
    pub v: DVec2,
    pub width: f64,
    pub risers: usize,
    pub riser: f64,
    /// Tread depth (on the walking line).
    pub tread: f64,
    pub base: f64,
    pub top: f64,
    pub layout: StairLayout,
    pub steps: Vec<StairStep>,
    pub flights: Vec<Flight>,
    pub spiral: Option<Spiral>,
    /// Walking line from the foot of the stair to its top, with elevations.
    pub walkline: Vec<DVec3>,
    /// Plan outlines of the flights and landings (for snapping).
    pub parts: Vec<Vec<DVec2>>,
    pub check: StairCheck,
    /// Local plan: what the railing paths are rebuilt from.
    plan: Plan,
}

/// The local layout, kept for railing paths.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Plan {
    /// Treads of the first flight, slots in the corners (a landing or
    /// winders; the two landings of a three-flight stair), treads of the
    /// middle flight of a three-flight stair and of the last flight.
    first: usize,
    corner: usize,
    middle: usize,
    second: usize,
    /// Landing depth (straight and U-shaped stairs).
    landing: f64,
}

/// Closed solids, each a list of planar polygons wound outward.
pub type Pieces = Vec<Vec<Vec<DVec3>>>;

/// Least rise of a spiral stair in one full turn: headroom over the treads below.
const SPIRAL_TURN_RISE: f64 = 2.2;

/// Segments per radian on arcs.
const ARC_STEPS: f64 = 8.0;

impl StairGeom {
    /// Plan length of the walking line.
    pub fn run(&self) -> f64 {
        self.walkline.windows(2).map(|w| w[0].truncate().distance(w[1].truncate())).sum()
    }

    fn sign(&self) -> f64 {
        self.layout.turn.sign()
    }

    /// World point of local `(a, b)`.
    fn world(&self, p: DVec2) -> DVec2 {
        self.start + self.u * p.x + self.v * (p.y * self.sign())
    }

    /// Elevation of the walking surface of slot `k` (0: the first tread).
    fn slot_z(&self, k: usize) -> f64 {
        self.base + (k + 1) as f64 * self.riser
    }

    /// Path of a railing along `side`, `inset` in from the edge, at the
    /// height of the nosings (from the first nosing to the top).
    pub fn railing_path(&self, side: StairSide, inset: f64) -> Vec<DVec3> {
        let inset = inset.clamp(-0.5, self.width / 2.0 - 0.01);
        // Local +b is the side the stair turns to: the left for a left turn.
        let turn_side = (side == StairSide::Left) == (self.sign() > 0.0);
        let local = match self.layout.shape {
            StairShape::Spiral => self.spiral_rail(turn_side, inset),
            _ => self.flight_rail(turn_side, inset),
        };
        let mut out: Vec<DVec3> = Vec::with_capacity(local.len());
        for (p, z) in local {
            let w = self.world(p);
            let q = DVec3::new(w.x, w.y, z);
            if out.last().is_none_or(|l: &DVec3| l.distance(q) > 1e-6) {
                out.push(q);
            }
        }
        out
    }

    fn flight_rail(&self, turn_side: bool, e: f64) -> Vec<(DVec2, f64)> {
        let (g, w) = (self.tread, self.width);
        let hw = w / 2.0;
        let o = hw - e;
        let Plan { first, corner, middle, second, landing } = self.plan;
        let mut pts: Vec<(DVec2, f64)> = Vec::new();
        let side = if turn_side { 1.0 } else { -1.0 };
        // Nosings of a flight's treads, from slot `slot`.
        let flight = |pts: &mut Vec<(DVec2, f64)>, origin: DVec2, dir: DVec2, count: usize, slot: usize, off: f64| {
            for j in 0..count {
                pts.push((origin + dir * (j as f64 * g) + dir.perp() * off, self.slot_z(slot + j)));
            }
        };
        let a0 = first as f64 * g;
        match self.layout.shape {
            StairShape::Straight | StairShape::Spiral => {
                flight(&mut pts, DVec2::ZERO, DVec2::X, first, 0, side * o);
                pts.push((DVec2::new(a0, side * o), self.top));
            }
            StairShape::StraightLanding => {
                let zl = self.slot_z(first);
                flight(&mut pts, DVec2::ZERO, DVec2::X, first, 0, side * o);
                pts.push((DVec2::new(a0, side * o), zl));
                if landing > g {
                    pts.push((DVec2::new(a0 + landing - g, side * o), zl));
                }
                let o2 = DVec2::new(a0 + landing, 0.0);
                flight(&mut pts, o2, DVec2::X, second, first + 1, side * o);
                pts.push((o2 + DVec2::new(second as f64 * g, side * o), self.top));
            }
            StairShape::LShaped => {
                let after = first + corner;
                flight(&mut pts, DVec2::ZERO, DVec2::X, first, 0, side * o);
                if turn_side {
                    // The inner corner.
                    let z = (self.slot_z(first) + self.slot_z(after.saturating_sub(1))) / 2.0;
                    pts.push((DVec2::new(a0 + e, o), z));
                } else if corner <= 1 {
                    let zl = self.slot_z(first);
                    pts.push((DVec2::new(a0, -o), zl));
                    pts.push((DVec2::new(a0 + w - e, -o), zl));
                    if w > g {
                        pts.push((DVec2::new(a0 + w - e, hw - g), zl));
                    }
                } else {
                    // Winder fronts on the outer edges, the corner between.
                    let reach = w - e;
                    let corner_pt = DVec2::new(a0 + reach, hw - reach);
                    for k in 0..corner {
                        let phi = -FRAC_PI_2 + FRAC_PI_2 * k as f64 / corner as f64;
                        let next = -FRAC_PI_2 + FRAC_PI_2 * (k + 1) as f64 / corner as f64;
                        let z = self.slot_z(first + k);
                        pts.push((winder_hit(a0, hw, reach, phi), z));
                        if phi < -FRAC_PI_4 && next > -FRAC_PI_4 + 1e-9 {
                            let t = (-FRAC_PI_4 - phi) / (next - phi);
                            pts.push((corner_pt, z + t * self.riser));
                        }
                    }
                }
                // The second flight goes along +b, centered at a0 + hw.
                let o2 = DVec2::new(a0 + hw, hw);
                flight(&mut pts, o2, DVec2::Y, second, after, side * o);
                pts.push((o2 + DVec2::new(-side * o, second as f64 * g), self.top));
            }
            StairShape::UShaped => {
                let zl = self.slot_z(first);
                flight(&mut pts, DVec2::ZERO, DVec2::X, first, 0, side * o);
                let gap = self.layout.gap;
                let b2 = w + gap;
                if turn_side {
                    // Around the end of the well between the flights.
                    pts.push((DVec2::new(a0, o), zl));
                    pts.push((DVec2::new(a0 + e, o), zl));
                    pts.push((DVec2::new(a0 + e, b2 - o), zl));
                    pts.push((DVec2::new(a0, b2 - o), zl));
                } else {
                    pts.push((DVec2::new(a0, -o), zl));
                    pts.push((DVec2::new(a0 + landing - e, -o), zl));
                    pts.push((DVec2::new(a0 + landing - e, b2 + o), zl));
                    if landing - e > g {
                        pts.push((DVec2::new(a0 + g, b2 + o), zl));
                    }
                }
                // The second flight comes back along -a; its left is -b.
                let o2 = DVec2::new(a0, b2);
                let off = if turn_side { o } else { -o };
                flight(&mut pts, o2, -DVec2::X, second, first + 1, off);
                pts.push((o2 + DVec2::new(-(second as f64) * g, -off), self.top));
            }
            StairShape::ThreeFlight => {
                // Up along +a, a corner landing, along +b, a second corner
                // landing, back along -a; the inner side runs round the well.
                let b1 = hw + middle as f64 * g;
                let (z1, z2) = (self.slot_z(first), self.slot_z(first + 1 + middle));
                flight(&mut pts, DVec2::ZERO, DVec2::X, first, 0, side * o);
                if turn_side {
                    pts.push((DVec2::new(a0 + e, o), z1));
                } else {
                    pts.push((DVec2::new(a0, -o), z1));
                    pts.push((DVec2::new(a0 + w - e, -o), z1));
                    if w - e > g {
                        pts.push((DVec2::new(a0 + w - e, hw - g), z1));
                    }
                }
                let o2 = DVec2::new(a0 + hw, hw);
                flight(&mut pts, o2, DVec2::Y, middle, first + 1, side * o);
                if turn_side {
                    pts.push((DVec2::new(a0 + e, b1 + e), z2));
                } else {
                    pts.push((DVec2::new(a0 + w - e, b1), z2));
                    pts.push((DVec2::new(a0 + w - e, b1 + w - e), z2));
                    if w - e > g {
                        pts.push((DVec2::new(a0 + g, b1 + w - e), z2));
                    }
                }
                let o3 = DVec2::new(a0, b1 + hw);
                flight(&mut pts, o3, -DVec2::X, second, first + 2 + middle, side * o);
                pts.push((o3 - DVec2::X * (second as f64 * g) + (-DVec2::X).perp() * (side * o), self.top));
            }
        }
        pts
    }

    fn spiral_rail(&self, turn_side: bool, e: f64) -> Vec<(DVec2, f64)> {
        let Some(sp) = self.local_spiral() else { return Vec::new() };
        let r = if turn_side { sp.inner + e } else { sp.outer - e };
        if r < 0.02 {
            return Vec::new();
        }
        let delta = self.tread / (sp.inner + self.width / 2.0);
        let treads = self.risers.saturating_sub(1);
        let sweep = delta * treads as f64;
        let n = ((sweep * ARC_STEPS).ceil() as usize).clamp(1, 4000);
        let rise = self.top - self.slot_z(0);
        (0..=n)
            .map(|k| {
                let t = k as f64 / n as f64;
                let phi = -FRAC_PI_2 + sweep * t;
                (sp.center + DVec2::from_angle(phi) * r, self.slot_z(0) + rise * t)
            })
            .collect()
    }

    /// The spiral circle in the local frame.
    fn local_spiral(&self) -> Option<Spiral> {
        let inner = self.layout.inner_radius;
        (self.layout.shape == StairShape::Spiral).then(|| Spiral {
            center: DVec2::new(0.0, inner + self.width / 2.0),
            inner,
            outer: inner + self.width,
        })
    }

    /// Add the stair's solids, built as `build` says.
    pub fn add_to(&self, build: &StairBuild, solid: &mut Solid) {
        let first = solid.faces.len();
        for piece in self.pieces(build) {
            solid.add_polygons(piece);
        }
        // A spiral's curved sides and helical soffit read as smooth surfaces:
        // no seams between their facets, only silhouettes.
        if let Some(sp) = self.spiral {
            for face in solid.faces.iter_mut().skip(first) {
                let n = face.normal;
                let radial = (face.centroid().truncate() - sp.center).normalize_or_zero();
                let side = n.z.abs() < 1e-6 && n.truncate().dot(radial).abs() > 0.97;
                let soffit = n.z < -1e-3 && n.z > -0.9999;
                face.smooth |= side || soffit;
            }
        }
    }

    /// The stair's solids as closed pieces (treads, landings, stringers...), built as `build` says.
    pub fn pieces(&self, build: &StairBuild) -> Pieces {
        let mut pieces = Pieces::new();
        self.add_pieces(build, &mut pieces);
        pieces
    }

    fn add_pieces(&self, build: &StairBuild, s: &mut Pieces) {
        let h = self.riser;
        let k = if self.tread > 1e-6 { h / self.tread } else { 0.0 };
        let slope = (1.0 + k * k).sqrt();
        // A spiral filled down to the floor would be a column: it is cast.
        let construction = match build.construction {
            StairConstruction::Solid if self.spiral.is_some() => StairConstruction::CastInPlace,
            c => c,
        };
        match construction {
            StairConstruction::Solid => {
                // Each flight one block from its profile; landings and winders filled down.
                for f in &self.flights {
                    s.push(self.flight_solid(f, None));
                }
                for st in self.steps.iter().filter(|st| st.kind != StepKind::Tread) {
                    let n = st.outline.len();
                    add_wedge(s, &st.outline, &vec![self.base; n], &vec![st.z; n]);
                }
            }
            StairConstruction::CastInPlace => {
                // Treads on a waist slab measured square to the pitch from
                // the inner corners of the steps; landings are slabs. Each
                // straight flight is one block from its profile.
                let waist = build.waist * slope;
                for f in &self.flights {
                    s.push(self.flight_solid(f, Some(build.waist)));
                }
                for st in self.steps.iter().filter(|st| st.kind != StepKind::Tread) {
                    let bottom: Vec<f64> = match st.kind {
                        StepKind::Landing => vec![(st.z - build.waist).max(self.base); st.outline.len()],
                        _ => st.along.iter().map(|t| (st.z - h - waist + t * h).max(self.base)).collect(),
                    };
                    let top = vec![st.z; st.outline.len()];
                    add_wedge(s, &st.outline, &bottom, &top);
                }
            }
            StairConstruction::Assembled => self.add_assembled(build, slope, s),
        }
    }

    /// Side profile of a straight flight in `(s, z)`, `s` along it from its
    /// first riser, counter-clockwise: the steps on top; below, the floor
    /// (`waist: None`) or a waist slab `waist` thick square to the pitch,
    /// flat at the foot where it meets the floor (or the bottom of the
    /// landing it starts from) and with a haunch under the landing it rises
    /// to, down to the landing's underside.
    fn flight_profile(&self, f: &Flight, waist: Option<f64>) -> Vec<(f64, f64)> {
        let (g, h) = (self.tread, self.riser);
        let n = f.treads;
        let len = n as f64 * g;
        let last = f.z0 + (n.saturating_sub(1)) as f64 * h;
        let mut top: Vec<(f64, f64)> = vec![(0.0, f.floor)];
        for i in 0..n {
            let z = f.z0 + i as f64 * h;
            top.push((i as f64 * g, z));
            top.push(((i + 1) as f64 * g, z));
        }
        let mut bottom: Vec<(f64, f64)> = Vec::new();
        match waist {
            None => {
                bottom.push((len, self.base));
                bottom.push((0.0, self.base));
            }
            Some(w) => {
                let k = if g > 1e-6 { h / g } else { 0.0 };
                let w_pitch = w * (1.0 + k * k).sqrt();
                // The line through the inner corners of the steps, `w_pitch` lower.
                let soffit = |s: f64| f.z0 - h + k * s - w_pitch;
                let starts_on_landing = f.floor > self.base + 1e-9;
                let low = if starts_on_landing { (f.floor - w).max(self.base) } else { self.base };
                let mut end = len;
                if f.landing_ahead > 1e-9 && k > 1e-9 {
                    // Haunch: on along the soffit until it meets the landing's underside.
                    let under = last + h - w;
                    let reach = ((under - soffit(len)) / k).clamp(0.0, f.landing_ahead);
                    if reach > 1e-6 {
                        end = len + reach;
                        top.push((len, under));
                        top.push((end, under));
                    }
                }
                let at = |s: f64| soffit(s).max(low);
                bottom.push((end, at(end)));
                if k > 1e-9 {
                    let foot = (low - soffit(0.0)) / k;
                    if foot > 1e-6 && foot < end - 1e-6 {
                        bottom.push((foot, low));
                    }
                }
                bottom.push((0.0, low));
            }
        }
        let mut profile = top;
        profile.extend(bottom);
        profile.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9);
        if profile.len() > 2
            && let (Some(a), Some(b)) = (profile.first(), profile.last())
            && (a.0 - b.0).abs() < 1e-9
            && (a.1 - b.1).abs() < 1e-9
        {
            profile.pop();
        }
        profile
    }

    /// A straight flight as one closed solid: its profile across the width.
    fn flight_solid(&self, f: &Flight, waist: Option<f64>) -> Vec<Vec<DVec3>> {
        let hw = self.width / 2.0;
        extrude_profile(f, self.flight_profile(f, waist), hw, -hw)
    }

    fn add_assembled(&self, build: &StairBuild, slope: f64, s: &mut Pieces) {
        let h = self.riser;
        let tt = build.tread_thickness.min(h * 0.9);
        let depth = build.stringer_depth * slope;
        for st in &self.steps {
            let n = st.outline.len();
            match st.kind {
                StepKind::Landing => {
                    // A framed platform as deep as the stringers.
                    let bottom = (st.z - build.stringer_depth).max(self.base);
                    add_wedge(s, &st.outline, &vec![bottom; n], &vec![st.z; n]);
                }
                StepKind::Winder if self.spiral.is_none() => {
                    // Winders sit on a carriage following their pitch.
                    let bottom: Vec<f64> = st.along.iter().map(|t| (st.z - h - depth + t * h).max(self.base)).collect();
                    add_wedge(s, &st.outline, &bottom, &vec![st.z; n]);
                }
                _ => add_wedge(s, &st.outline, &vec![st.z - tt; n], &vec![st.z; n]),
            }
            if build.closed_risers {
                let [a, b] = st.front;
                let centroid = st.outline.iter().copied().sum::<DVec2>() / n.max(1) as f64;
                let mut inward = (b - a).perp().normalize_or_zero();
                if inward.dot(centroid - a) < 0.0 {
                    inward = -inward;
                }
                let board = [a, b, b + inward * 0.02, a + inward * 0.02];
                let lo = (st.z - h).max(self.base);
                add_wedge(s, &board, &[lo; 4], &[st.z - tt; 4]);
            }
        }
        let sw = build.stringer_width.min(self.width / 2.0);
        if let Some(sp) = self.spiral {
            match build.stringers {
                Stringers::Center => {
                    let r = sp.inner.max(sw / 2.0);
                    let circle: Vec<DVec2> =
                        (0..24).map(|i| sp.center + DVec2::from_angle(TAU * i as f64 / 24.0) * r).collect();
                    add_wedge(s, &circle, &[self.base; 24], &[self.top; 24]);
                }
                Stringers::Sides => {
                    // Helical stringers under the outer (and inner) edge.
                    let mut rings = vec![(sp.outer - sw, sp.outer)];
                    if sp.inner >= sw {
                        rings.push((sp.inner, sp.inner + sw));
                    }
                    for st in self.steps.iter().filter(|st| st.kind == StepKind::Winder) {
                        for &(r0, r1) in &rings {
                            if let Some((outline, along)) = self.ring_piece(st, sp, r0, r1) {
                                let top: Vec<f64> = along.iter().map(|t| st.z + t * h + 0.05).collect();
                                let bottom: Vec<f64> = top.iter().map(|z| (z - depth).max(self.base)).collect();
                                add_wedge(s, &outline, &bottom, &top);
                            }
                        }
                    }
                }
            }
            return;
        }
        for f in self.flights.iter().filter(|f| f.treads > 0) {
            let strips: Vec<(f64, f64, f64)> = match build.stringers {
                // Housed stringers whose top runs 5 cm above the nosings.
                Stringers::Sides => {
                    let hw = self.width / 2.0;
                    vec![(hw - sw, hw, 0.05), (-hw, -hw + sw, 0.05)]
                }
                // Under the treads, touching the back of each.
                Stringers::Center => vec![(-sw / 2.0, sw / 2.0, -tt - h)],
            };
            for (v0, v1, lift) in strips {
                for profile in self.stringer_profiles(f, lift, depth, build.stringer_depth) {
                    s.push(extrude_profile(f, profile, v1, v0));
                }
            }
        }
    }

    /// Side profiles, in `(s, z)` along flight `f`, of a stringer whose top
    /// runs `lift` above the nosings, `depth` deep measured plumb: a band
    /// parallel to the pitch, cut level where it stands (the floor, or the
    /// underside of the landing the flight leaves, `landing_depth` below
    /// its top) and, where the flight rises to a landing, carried on under
    /// it up to the landing's underside, so it bears on the landing instead
    /// of touching its edge.
    pub(crate) fn stringer_profiles(
        &self,
        f: &Flight,
        lift: f64,
        depth: f64,
        landing_depth: f64,
    ) -> Vec<Vec<(f64, f64)>> {
        let (g, h) = (self.tread, self.riser);
        let k = if g > 1e-6 { h / g } else { 0.0 };
        let len = f.treads as f64 * g;
        let top = |s: f64| f.z0 + lift + k * s;
        let low = if f.floor > self.base + 1e-9 { (f.floor - landing_depth).max(self.base) } else { self.base };
        let band = |s0: f64, s1: f64| vec![(s0, top(s0) - depth), (s1, top(s1) - depth), (s1, top(s1)), (s0, top(s0))];
        let mut out = Vec::new();
        let along = clip_level(&band(0.0, len), low, true);
        if along.len() >= 3 {
            out.push(along);
        }
        if f.landing_ahead > 1e-9 {
            // Under the landing it rises to: up to the landing's underside.
            let underside = f.z0 + f.treads as f64 * h - landing_depth;
            let under = clip_level(&clip_level(&band(len, len + f.landing_ahead), low, true), underside, false);
            if under.len() >= 3 {
                out.push(under);
            }
        }
        out
    }

    /// The part of spiral tread `st` between radii `r0` and `r1`.
    fn ring_piece(&self, st: &StairStep, sp: Spiral, r0: f64, r1: f64) -> Option<(Vec<DVec2>, Vec<f64>)> {
        let angle = |p: DVec2| (p - sp.center).to_angle();
        let [outer, _] = st.front;
        let a0 = angle(outer);
        let delta = self.tread / (sp.inner + self.width / 2.0) * self.sign();
        let n = ((delta.abs() * ARC_STEPS).ceil() as usize).clamp(1, 64);
        let arc = |r: f64| -> Vec<(DVec2, f64)> {
            (0..=n)
                .map(|i| {
                    let t = i as f64 / n as f64;
                    (sp.center + DVec2::from_angle(a0 + delta * t) * r, t)
                })
                .collect()
        };
        let mut pts = arc(r1);
        pts.extend(arc(r0).into_iter().rev());
        (r1 - r0 > 1e-4).then(|| pts.into_iter().unzip())
    }
}

const FRAC_PI_4: f64 = std::f64::consts::FRAC_PI_4;

/// Where a winder's front ray from the inner corner `(a0, hw)` at angle `phi`
/// (from -90° to 0°) meets the outer edges of the corner square, `reach` away.
fn winder_hit(a0: f64, hw: f64, reach: f64, phi: f64) -> DVec2 {
    if phi <= -FRAC_PI_4 {
        DVec2::new(a0 + reach * phi.cos() / (-phi.sin()).max(1e-9), hw - reach)
    } else {
        DVec2::new(a0 + reach, hw + reach * phi.tan())
    }
}

/// A side profile `(s, z)` of flight `f` extruded across it, from `left` to
/// `right` (offsets to the flight's left), as a closed solid.
fn extrude_profile(f: &Flight, profile: Vec<(f64, f64)>, left: f64, right: f64) -> Vec<Vec<DVec3>> {
    let at = |(s, z): (f64, f64), v: f64| {
        let p = f.origin + f.dir * s + f.left * v;
        DVec3::new(p.x, p.y, z)
    };
    let n = profile.len();
    // Counter-clockwise in (s, z) seen from the left side (+v): that face
    // takes the order, the right one its reverse, the sides each edge.
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (profile[i], profile[(i + 1) % n]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum();
    let pts: Vec<(f64, f64)> = if area >= 0.0 { profile } else { profile.into_iter().rev().collect() };
    let mut faces: Vec<Vec<DVec3>> = Vec::with_capacity(n + 2);
    faces.push(pts.iter().rev().map(|&p| at(p, left)).collect());
    faces.push(pts.iter().map(|&p| at(p, right)).collect());
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        faces.push(vec![at(a, left), at(b, left), at(b, right), at(a, right)]);
    }
    faces
}

/// A convex profile `(s, z)` cut by the level `z = level`, keeping what lies
/// above it (`above`) or below it.
fn clip_level(poly: &[(f64, f64)], level: f64, above: bool) -> Vec<(f64, f64)> {
    let keep = |p: (f64, f64)| if above { p.1 >= level - 1e-12 } else { p.1 <= level + 1e-12 };
    let n = poly.len();
    let mut out = Vec::with_capacity(n + 2);
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if keep(a) {
            out.push(a);
        }
        if keep(a) != keep(b) && (b.1 - a.1).abs() > 1e-15 {
            let t = (level - a.1) / (b.1 - a.1);
            out.push((a.0 + (b.0 - a.0) * t, level));
        }
    }
    out.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-12 && (a.1 - b.1).abs() < 1e-12);
    if out.len() > 1
        && out.first().zip(out.last()).is_some_and(|(a, b)| (a.0 - b.0).abs() < 1e-12 && (a.1 - b.1).abs() < 1e-12)
    {
        out.pop();
    }
    out
}

/// A prism over `outline` with an elevation per vertex at the bottom and the
/// top; uneven faces are split into triangles.
pub(crate) fn add_wedge(s: &mut Pieces, outline: &[DVec2], bottom: &[f64], top: &[f64]) {
    let n = outline.len();
    if n < 3 || bottom.len() != n || top.len() != n {
        return;
    }
    let (mut pts, mut lo, mut hi) = (outline.to_vec(), bottom.to_vec(), top.to_vec());
    if polygon::signed_area(&pts) < 0.0 {
        pts.reverse();
        lo.reverse();
        hi.reverse();
    }
    let lift = |i: usize, z: &[f64]| DVec3::new(pts[i].x, pts[i].y, z[i]);
    let flat = |z: &[f64]| z.iter().all(|v| (v - z[0]).abs() < 1e-9);
    let tris = polygon::triangulate(&pts);
    let mut faces: Vec<Vec<DVec3>> = Vec::new();
    if flat(&hi) {
        faces.push((0..n).map(|i| lift(i, &hi)).collect());
    } else {
        faces.extend(tris.iter().map(|t| t.iter().map(|&i| lift(i, &hi)).collect()));
    }
    if flat(&lo) {
        faces.push((0..n).rev().map(|i| lift(i, &lo)).collect());
    } else {
        faces.extend(tris.iter().map(|t| t.iter().rev().map(|&i| lift(i, &lo)).collect()));
    }
    for i in 0..n {
        let j = (i + 1) % n;
        faces.push(vec![lift(i, &lo), lift(j, &lo), lift(j, &hi), lift(i, &hi)]);
    }
    s.push(faces);
}

/// Layout of a stair element from its type, constraints and shape.
pub fn stair_geom(project: &Project, element: &cc_core::Element, stair: &cc_core::Stair) -> Option<StairGeom> {
    let TypeData::Stair { width, max_riser, tread, rules, .. } = project.element_type(element.type_id)?.data else {
        return None;
    };
    let (base, top) = project.stair_span(element, stair);
    let u = DVec2::from_angle(stair.direction_deg.to_radians());
    Some(layout_stair(stair.start, u, width, base, top, max_riser, tread, &rules, stair.layout))
}

/// Lay out a stair from its foot `start`, first-flight direction `u`, width
/// and elevations.
#[allow(clippy::too_many_arguments)]
pub fn layout_stair(
    start: DVec2,
    u: DVec2,
    width: f64,
    base: f64,
    top: f64,
    max_riser: f64,
    tread: f64,
    rules: &StairRules,
    mut layout: StairLayout,
) -> StairGeom {
    let width = if width.is_finite() { width.clamp(0.1, 20.0) } else { 1.0 };
    let (risers, riser, g) = layout.steps(top - base, max_riser, tread, rules);
    // A spiral turns no faster than leaves headroom over the turn below:
    // its tread on the walking line grows when the type's would.
    let g = match layout.shape {
        StairShape::Spiral => {
            let walk = (layout.inner_radius.max(0.0) + width / 2.0).max(1e-6);
            (g / walk).max(TAU * riser / SPIRAL_TURN_RISE) * walk
        }
        _ => g,
    };
    let slots = risers.saturating_sub(1);
    let hw = width / 2.0;
    let landing = if layout.landing > 1e-3 { layout.landing } else { width };
    // Slots of the corners: one landing or the winders, two landings on a
    // three-flight stair (fewer if there are not enough treads).
    let corner = match layout.shape {
        StairShape::Straight | StairShape::Spiral => 0,
        StairShape::LShaped if layout.winders >= 2 => (layout.winders as usize).min(slots),
        StairShape::ThreeFlight => slots.min(2),
        _ => usize::from(slots >= 1),
    };
    let treads = slots - corner;
    let first = match layout.shape {
        StairShape::Straight | StairShape::Spiral => slots,
        _ if layout.split > 0 => (layout.split as usize).saturating_sub(1).min(treads),
        StairShape::ThreeFlight => treads / 3,
        _ => treads / 2,
    };
    let middle = match layout.shape {
        StairShape::ThreeFlight if layout.split2 > 0 => (layout.split2 as usize).saturating_sub(1).min(treads - first),
        StairShape::ThreeFlight => (treads - first) / 2,
        _ => 0,
    };
    let second = treads - first - middle;
    if layout.shape == StairShape::LShaped && corner < 2 {
        layout.winders = 0;
    }
    let plan = Plan { first, corner, middle, second, landing };
    let landing_check = match layout.shape {
        StairShape::StraightLanding | StairShape::UShaped if corner == 1 => Some((landing, width)),
        _ => None,
    };
    let mut geom = StairGeom {
        start,
        u,
        v: u.perp(),
        width,
        risers,
        riser,
        tread: g,
        base,
        top,
        layout,
        steps: Vec::new(),
        flights: Vec::new(),
        spiral: None,
        walkline: Vec::new(),
        parts: Vec::new(),
        check: StairCheck::new(risers, riser, g, max_riser, rules, landing_check),
        plan,
    };
    let mut b = Builder { geom: &geom, steps: Vec::new(), flights: Vec::new(), parts: Vec::new() };
    let a0 = first as f64 * g;
    let mut walk: Vec<(DVec2, f64)> = vec![(DVec2::ZERO, base)];
    match layout.shape {
        StairShape::Straight => {
            b.flight(DVec2::ZERO, DVec2::X, first, 0);
        }
        StairShape::StraightLanding => {
            b.flight(DVec2::ZERO, DVec2::X, first, 0);
            if corner == 1 {
                b.landing(rect(a0, a0 + landing, -hw, hw), [DVec2::new(a0, -hw), DVec2::new(a0, hw)], first);
            }
            let o2 = DVec2::new(a0 + landing * corner as f64, 0.0);
            b.flight(o2, DVec2::X, second, first + corner);
            walk.push((DVec2::new(a0, 0.0), b.z(first)));
            walk.push((o2, b.z(first)));
        }
        StairShape::LShaped => {
            b.flight(DVec2::ZERO, DVec2::X, first, 0);
            if corner == 1 {
                b.landing(rect(a0, a0 + width, -hw, hw), [DVec2::new(a0, -hw), DVec2::new(a0, hw)], first);
            } else if corner >= 2 {
                b.winders(a0, hw, width, corner, first);
            }
            let o2 = DVec2::new(a0 + hw, hw);
            b.flight(o2, DVec2::Y, second, first + corner);
            let zc = if corner > 0 { b.z(first + corner - 1) } else { b.z(first) };
            walk.push((DVec2::new(a0 + hw, 0.0), zc));
            walk.push((o2, zc));
        }
        StairShape::UShaped => {
            b.flight(DVec2::ZERO, DVec2::X, first, 0);
            let b2 = width + layout.gap;
            if corner == 1 {
                b.landing(rect(a0, a0 + landing, -hw, b2 + hw), [DVec2::new(a0, -hw), DVec2::new(a0, hw)], first);
            }
            let o2 = DVec2::new(a0, b2);
            b.flight(o2, -DVec2::X, second, first + corner);
            let zl = b.z(first);
            walk.push((DVec2::new(a0 + landing / 2.0, 0.0), zl));
            walk.push((DVec2::new(a0 + landing / 2.0, b2), zl));
            walk.push((o2, zl));
        }
        StairShape::ThreeFlight => {
            b.flight(DVec2::ZERO, DVec2::X, first, 0);
            if corner >= 1 {
                b.landing(rect(a0, a0 + width, -hw, hw), [DVec2::new(a0, -hw), DVec2::new(a0, hw)], first);
            }
            let o2 = DVec2::new(a0 + hw, hw);
            b.flight(o2, DVec2::Y, middle, first + 1);
            let b1 = hw + middle as f64 * g;
            if corner >= 2 {
                let front = [DVec2::new(a0 + width, b1), DVec2::new(a0, b1)];
                b.landing(rect(a0, a0 + width, b1, b1 + width), front, first + 1 + middle);
            }
            let o3 = DVec2::new(a0, b1 + hw);
            b.flight(o3, -DVec2::X, second, first + 2 + middle);
            let (z1, z2) = (b.z(first), b.z(first + 1 + middle));
            walk.push((DVec2::new(a0 + hw, 0.0), z1));
            walk.push((o2, z1));
            walk.push((DVec2::new(a0 + hw, b1 + hw), z2));
            walk.push((o3, z2));
        }
        StairShape::Spiral => {
            b.spiral(slots);
        }
    }
    let (steps, mut flights, parts) = (b.steps, b.flights, b.parts);
    // The first flight of a stair with a landing rises to it.
    let ahead = match layout.shape {
        StairShape::StraightLanding | StairShape::UShaped => landing,
        StairShape::LShaped | StairShape::ThreeFlight => width,
        StairShape::Straight | StairShape::Spiral => 0.0,
    };
    if corner == 1
        && first > 0
        && let Some(f) = flights.first_mut()
    {
        f.landing_ahead = ahead;
    }
    // The first two flights of a three-flight stair rise to a landing (still
    // in the local frame: they go along +a and +b).
    if layout.shape == StairShape::ThreeFlight && corner == 2 {
        for f in flights.iter_mut().filter(|f| f.dir.x > 0.5 || f.dir.y > 0.5) {
            f.landing_ahead = ahead;
        }
    }
    // The walking line ends at the back of the last tread.
    let end = match layout.shape {
        StairShape::Straight => Some(DVec2::new(a0, 0.0)),
        StairShape::StraightLanding => Some(DVec2::new(a0 + landing * corner as f64 + second as f64 * g, 0.0)),
        StairShape::LShaped => Some(DVec2::new(a0 + hw, hw + second as f64 * g)),
        StairShape::UShaped => Some(DVec2::new(a0 - second as f64 * g, width + layout.gap)),
        StairShape::ThreeFlight => Some(DVec2::new(a0 - second as f64 * g, width + middle as f64 * g)),
        StairShape::Spiral => None,
    };
    match end {
        Some(end) => walk.push((end, top)),
        None => {
            if let Some(sp) = geom.local_spiral() {
                let r = sp.inner + hw;
                let sweep = g / r.max(1e-6) * slots as f64;
                let n = ((sweep * ARC_STEPS).ceil() as usize).clamp(1, 4000);
                walk = (0..=n)
                    .map(|k| {
                        let t = k as f64 / n as f64;
                        (sp.center + DVec2::from_angle(-FRAC_PI_2 + sweep * t) * r, base + (top - base) * t)
                    })
                    .collect();
            }
        }
    }
    let (sign, v) = (geom.sign(), geom.v);
    let world = move |p: DVec2| start + u * p.x + v * (p.y * sign);
    let spiral = geom.local_spiral();
    geom.walkline = walk
        .iter()
        .map(|&(p, z)| {
            let w = world(p);
            DVec3::new(w.x, w.y, z)
        })
        .collect();
    geom.steps = steps
        .into_iter()
        .map(|st| StairStep {
            outline: st.outline.iter().map(|&p| world(p)).collect(),
            front: st.front.map(world),
            ..st
        })
        .collect();
    geom.flights = flights
        .into_iter()
        .map(|f| {
            let (o, d) = (world(f.origin), world(f.origin + f.dir) - world(f.origin));
            Flight { origin: o, dir: d, left: d.perp(), ..f }
        })
        .collect();
    geom.parts = parts.into_iter().map(|p| p.into_iter().map(world).collect()).collect();
    geom.spiral = spiral.map(|sp| Spiral { center: world(sp.center), ..sp });
    geom
}

fn rect(a0: f64, a1: f64, b0: f64, b1: f64) -> Vec<DVec2> {
    vec![DVec2::new(a0, b0), DVec2::new(a1, b0), DVec2::new(a1, b1), DVec2::new(a0, b1)]
}

/// Collects steps in the local frame.
struct Builder<'a> {
    geom: &'a StairGeom,
    steps: Vec<StairStep>,
    flights: Vec<Flight>,
    parts: Vec<Vec<DVec2>>,
}

impl Builder<'_> {
    fn z(&self, slot: usize) -> f64 {
        self.geom.slot_z(slot)
    }

    /// `count` treads from `origin` along `dir`, the first at slot `slot`.
    fn flight(&mut self, origin: DVec2, dir: DVec2, count: usize, slot: usize) {
        let (g, hw) = (self.geom.tread, self.geom.width / 2.0);
        let left = dir.perp();
        for j in 0..count {
            let c = origin + dir * (j as f64 * g);
            let (r0, l0, r1, l1) = (c - left * hw, c + left * hw, c + dir * g - left * hw, c + dir * g + left * hw);
            self.steps.push(StairStep {
                outline: vec![r0, r1, l1, l0],
                along: vec![0.0, 1.0, 1.0, 0.0],
                front: [r0, l0],
                z: self.z(slot + j),
                kind: StepKind::Tread,
            });
        }
        if count > 0 {
            let len = count as f64 * g;
            self.parts.push(vec![
                origin - left * hw,
                origin + dir * len - left * hw,
                origin + dir * len + left * hw,
                origin + left * hw,
            ]);
            let z0 = self.z(slot);
            self.flights.push(Flight {
                origin,
                dir,
                left,
                treads: count,
                z0,
                floor: z0 - self.geom.riser,
                landing_ahead: 0.0,
            });
        }
    }

    fn landing(&mut self, outline: Vec<DVec2>, front: [DVec2; 2], slot: usize) {
        self.parts.push(outline.clone());
        let n = outline.len();
        self.steps.push(StairStep { outline, along: vec![0.5; n], front, z: self.z(slot), kind: StepKind::Landing });
    }

    /// `count` winders filling the corner square `[a0, a0 + w] × [-hw, hw]`,
    /// radiating from its inner corner `(a0, hw)`.
    fn winders(&mut self, a0: f64, hw: f64, w: f64, count: usize, slot: usize) {
        let inner = DVec2::new(a0, hw);
        let corner = DVec2::new(a0 + w, -hw);
        for k in 0..count {
            let phi0 = -FRAC_PI_2 + FRAC_PI_2 * k as f64 / count as f64;
            let phi1 = -FRAC_PI_2 + FRAC_PI_2 * (k + 1) as f64 / count as f64;
            let (p0, p1) = (winder_hit(a0, hw, w, phi0), winder_hit(a0, hw, w, phi1));
            let mut outline = vec![inner, p0];
            if phi0 < -FRAC_PI_4 - 1e-9 && phi1 > -FRAC_PI_4 + 1e-9 {
                outline.push(corner);
            }
            outline.push(p1);
            let along = outline
                .iter()
                .map(|&p| {
                    if p.distance(inner) < 1e-9 {
                        0.5
                    } else {
                        (((p - inner).to_angle() - phi0) / (phi1 - phi0)).clamp(0.0, 1.0)
                    }
                })
                .collect();
            self.steps.push(StairStep {
                outline,
                along,
                front: [p0, inner],
                z: self.z(slot + k),
                kind: StepKind::Winder,
            });
        }
        self.parts.push(rect(a0, a0 + w, -hw, hw));
    }

    /// `count` treads around the spiral's center.
    fn spiral(&mut self, count: usize) {
        let Some(sp) = self.geom.local_spiral() else { return };
        let delta = self.geom.tread / (sp.inner + self.geom.width / 2.0).max(1e-6);
        let n = ((delta * ARC_STEPS).ceil() as usize).clamp(1, 64);
        for i in 0..count {
            let phi0 = -FRAC_PI_2 + delta * i as f64;
            let at = |r: f64, t: f64| sp.center + DVec2::from_angle(phi0 + delta * t) * r;
            let mut outline = Vec::new();
            let mut along = Vec::new();
            for k in 0..=n {
                let t = k as f64 / n as f64;
                outline.push(at(sp.outer, t));
                along.push(t);
            }
            if sp.inner < 1e-3 {
                outline.push(sp.center);
                along.push(0.5);
            } else {
                for k in (0..=n).rev() {
                    let t = k as f64 / n as f64;
                    outline.push(at(sp.inner, t));
                    along.push(t);
                }
            }
            let inner_front = if sp.inner < 1e-3 { sp.center } else { at(sp.inner, 0.0) };
            self.steps.push(StairStep {
                outline,
                along,
                front: [at(sp.outer, 0.0), inner_front],
                z: self.z(i),
                kind: StepKind::Winder,
            });
        }
        // Outline of the whole sweep, up to one turn.
        let sweep = (delta * count as f64).min(TAU);
        let m = ((sweep * ARC_STEPS).ceil() as usize).clamp(1, 64);
        let mut part: Vec<DVec2> = (0..=m)
            .map(|k| sp.center + DVec2::from_angle(-FRAC_PI_2 + sweep * k as f64 / m as f64) * sp.outer)
            .collect();
        if sp.inner >= 1e-3 && sweep < TAU - 1e-6 {
            part.extend(
                (0..=m)
                    .rev()
                    .map(|k| sp.center + DVec2::from_angle(-FRAC_PI_2 + sweep * k as f64 / m as f64) * sp.inner),
            );
        } else if sweep < TAU - 1e-6 {
            part.push(sp.center);
        }
        self.parts.push(part);
    }
}

/// Path of a railing with elevations: along its stair's or ramp's side when it has
/// one, else its own path at its base.
pub fn railing_path(project: &Project, element: &cc_core::Element, railing: &cc_core::Railing) -> Vec<DVec3> {
    if let Some(h) = railing.stair
        && let Some(host) = project.element(h.host)
    {
        match &host.kind {
            cc_core::ElementKind::Stair(st) => {
                if let Some(g) = stair_geom(project, host, st) {
                    return g.railing_path(h.side, h.inset);
                }
            }
            cc_core::ElementKind::Ramp(r) => {
                if let Some(g) = crate::ramp_geom(project, host, r) {
                    return g.railing_path(h.side, h.inset);
                }
            }
            _ => {}
        }
    }
    let base = project.level_elevation(element.level) + railing.base_offset;
    railing.path.iter().map(|p| DVec3::new(p.x, p.y, base)).collect()
}

/// The layout of the stair a railing follows, if it follows one.
fn hosted_stair(project: &Project, railing: &cc_core::Railing) -> Option<StairGeom> {
    let host = project.element(railing.stair?.host)?;
    let cc_core::ElementKind::Stair(st) = &host.kind else { return None };
    stair_geom(project, host, st)
}

/// Length of a railing along its (sloped) path.
pub fn railing_length(project: &Project, element: &cc_core::Element, railing: &cc_core::Railing) -> f64 {
    railing_path(project, element, railing).windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Add a railing's solids: a top rail its type's height above the path and
/// posts at the path's corners and every `post_spacing` along it.
pub fn add_railing(project: &Project, element: &cc_core::Element, railing: &cc_core::Railing, solid: &mut Solid) {
    for piece in railing_pieces(project, element, railing) {
        solid.add_polygons(piece);
    }
}

/// The rail and posts of a railing as closed pieces.
pub fn railing_pieces(project: &Project, element: &cc_core::Element, railing: &cc_core::Railing) -> Pieces {
    let mut pieces = Pieces::new();
    let Some(TypeData::Railing { height, post_spacing, .. }) = project.element_type(element.type_id).map(|t| &t.data)
    else {
        return pieces;
    };
    let s = &mut pieces;
    let path = railing_path(project, element, railing);
    let (rail, post) = (0.05, 0.04);
    // On a stair the path runs along the nosings, above the treads between
    // them: posts stand on the tread or landing under them.
    let steps: Vec<(Vec<DVec2>, f64)> = hosted_stair(project, railing)
        .map(|g| g.steps.into_iter().map(|st| (st.outline, st.z)).collect())
        .unwrap_or_default();
    let ground = |q: DVec2, u: DVec2, z: f64| {
        // A point on a riser line belongs to the step it rises to.
        let under = |p: DVec2| steps.iter().filter(|(o, _)| polygon::contains(o, p)).map(|(_, z)| *z).reduce(f64::max);
        under(q).or_else(|| under(q + u * 1e-4)).filter(|g| *g <= z + 1e-6).unwrap_or(z)
    };
    let post_at = |s: &mut Pieces, p: DVec3, u: DVec2| {
        let v = u.perp();
        let q = p.truncate();
        let r = post / 2.0;
        let outline = [q - u * r - v * r, q + u * r - v * r, q + u * r + v * r, q - u * r + v * r];
        let base = ground(q, u, p.z);
        add_wedge(s, &outline, &[base; 4], &[p.z + height - rail; 4]);
    };
    // Posts at the start, at corners (turns in plan or changes of pitch) and
    // at most `post_spacing` apart along the path between them.
    let spacing = post_spacing.max(0.1);
    let slope = |a: DVec3, b: DVec3| (b.z - a.z) / (b - a).truncate().length().max(1e-9);
    let mut since = f64::INFINITY;
    let mut posts = 0;
    for (i, w) in path.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let Some(u) = (b - a).truncate().try_normalize() else { continue };
        let len = (b - a).truncate().length();
        let v = u.perp() * (rail / 2.0);
        let (pa, pb) = (a.truncate(), b.truncate());
        let outline = [pa - v, pb - v, pb + v, pa + v];
        let top = [a.z + height, b.z + height, b.z + height, a.z + height];
        let bottom = top.map(|z| z - rail);
        add_wedge(s, &outline, &bottom, &top);
        let corner = i > 0
            && path.get(i - 1).is_some_and(|&p| {
                let before = (a - p).truncate().normalize_or_zero();
                before.dot(u) < 0.94 || (slope(p, a) - slope(a, b)).abs() > 0.1
            });
        if since.is_infinite() || (corner && since > post * 2.0) {
            post_at(s, a, u);
            since = 0.0;
        }
        let mut at = 0.0;
        while since + (len - at) >= spacing && posts < 1000 {
            at += spacing - since;
            post_at(s, a.lerp(b, at / len), u);
            since = 0.0;
            posts += 1;
        }
        since += len - at;
    }
    // Closing post at the end of the path.
    if let [.., a, b] = path.as_slice()
        && since > post * 2.0
        && let Some(u) = (*b - *a).truncate().try_normalize()
    {
        post_at(s, *b, u);
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::{StairBuild, Turn};

    fn geom(layout: StairLayout, height: f64) -> StairGeom {
        layout_stair(DVec2::ZERO, DVec2::X, 1.0, 0.0, height, 0.18, 0.28, &StairRules::default(), layout)
    }

    fn volume(g: &StairGeom, build: &StairBuild) -> f64 {
        let mut s = Solid::new(cc_core::ElementId(1), cc_core::Category::Stair, [0, 0, 0]);
        g.add_to(build, &mut s);
        s.volume()
    }

    #[test]
    fn straight_stair_keeps_its_layout_and_volume() {
        let g = geom(StairLayout::default(), 2.88);
        assert_eq!((g.risers, g.steps.len()), (16, 15));
        assert!((g.run() - 15.0 * 0.28).abs() < 1e-9);
        // Solid: treads filled to the base, sum of k × riser × tread × width.
        let expected: f64 = (1..=15).map(|k| k as f64 * 0.18 * 0.28).sum();
        assert!((volume(&g, &StairBuild::default()) - expected).abs() < 1e-9);
        let rail = g.railing_path(StairSide::Left, 0.05);
        assert_eq!(rail.len(), 16);
        assert!((rail[0] - DVec3::new(0.0, 0.45, 0.18)).length() < 1e-9);
        assert!((rail[15] - DVec3::new(4.2, 0.45, 2.88)).length() < 1e-9);
    }

    #[test]
    fn cast_in_place_stair_is_a_waist_slab_under_the_steps() {
        let g = geom(StairLayout::default(), 2.88);
        let build = StairBuild { construction: StairConstruction::CastInPlace, waist: 0.12, ..Default::default() };
        // Away from the foot, each step is its triangle plus a band `waist` thick
        // square to the pitch: (h·g/2 + waist·√(h²+g²)) per meter of width.
        let (h, t) = (0.18_f64, 0.28_f64);
        let band = 0.12 * (h * h + t * t).sqrt();
        let full = h * t / 2.0 + band;
        let v = volume(&g, &build);
        assert!(v < 15.0 * full + 1e-9 && v > 13.0 * full, "{v} vs {}", 15.0 * full);
        // Much less concrete than the solid block.
        assert!(v < 0.3 * volume(&g, &StairBuild::default()));
    }

    #[test]
    fn landings_split_the_flights() {
        let layout = StairLayout { shape: StairShape::StraightLanding, ..Default::default() };
        let g = geom(layout, 2.88);
        // 15 slots: 7 treads, a landing at the 8th riser, 7 treads.
        let kinds: Vec<_> = g.steps.iter().map(|s| s.kind).collect();
        assert_eq!(kinds.iter().filter(|k| **k == StepKind::Landing).count(), 1);
        assert_eq!(kinds[7], StepKind::Landing);
        assert!((g.steps[7].z - 8.0 * 0.18).abs() < 1e-9);
        assert!((g.run() - (14.0 * 0.28 + 1.0)).abs() < 1e-9);
        assert!(g.check.ok(), "{:?}", g.check.issues);
        // A short landing is reported.
        let short = geom(StairLayout { landing: 0.8, ..layout }, 2.88);
        assert!(short.check.issues.iter().any(|i| i.key() == "stair-issue-landing-short"));
    }

    #[test]
    fn l_shaped_stair_turns_to_its_side() {
        let layout = StairLayout { shape: StairShape::LShaped, split: 6, ..Default::default() };
        let left = geom(layout, 2.88);
        let right = geom(StairLayout { turn: Turn::Right, ..layout }, 2.88);
        // 5 treads, the landing, 9 treads going +y (left) or -y (right).
        let last = |g: &StairGeom| g.steps.last().map(|s| s.outline.iter().copied().sum::<DVec2>() / 4.0);
        let (l, r) = (last(&left).unwrap_or_default(), last(&right).unwrap_or_default());
        assert!((l.x - (5.0 * 0.28 + 0.5)).abs() < 1e-9 && (l.y - (0.5 + 8.5 * 0.28)).abs() < 1e-9, "{l}");
        assert!((r.y + l.y).abs() < 1e-9);
        // The outer railing goes round the landing; both end at the top.
        let outer = left.railing_path(StairSide::Right, 0.05);
        assert!(outer.iter().any(|p| (p.x - (1.4 + 0.95)).abs() < 1e-9 && (p.y + 0.45).abs() < 1e-9));
        assert!(outer.last().is_some_and(|p| (p.z - 2.88).abs() < 1e-9));
    }

    #[test]
    fn winders_fill_the_corner() {
        let layout = StairLayout { shape: StairShape::LShaped, winders: 3, ..Default::default() };
        let g = geom(layout, 2.88);
        let winders: Vec<_> = g.steps.iter().filter(|s| s.kind == StepKind::Winder).collect();
        assert_eq!(winders.len(), 3);
        let area: f64 = winders.iter().map(|s| polygon::area(&s.outline)).sum();
        assert!((area - 1.0).abs() < 1e-9, "{area}");
        // Every slot rises one riser.
        for (i, s) in g.steps.iter().enumerate() {
            assert!((s.z - (i + 1) as f64 * 0.18).abs() < 1e-9);
        }
    }

    #[test]
    fn u_shaped_stair_comes_back() {
        let layout = StairLayout { shape: StairShape::UShaped, gap: 0.2, ..Default::default() };
        let g = geom(layout, 2.88);
        let end = g.walkline.last().copied().unwrap_or_default();
        // 7 treads up, landing, 7 treads back: the top is over the start, beside it.
        assert!(end.x.abs() < 1e-9 && (end.y - 1.2).abs() < 1e-9 && (end.z - 2.88).abs() < 1e-9, "{end}");
    }

    #[test]
    fn three_flight_stair_turns_twice_at_landings() {
        let layout = StairLayout { shape: StairShape::ThreeFlight, ..Default::default() };
        let g = geom(layout, 2.88);
        // 15 slots: 4 treads, a landing, 4 treads, a landing, 5 treads.
        let kinds: Vec<_> = g.steps.iter().map(|s| s.kind).collect();
        assert_eq!(kinds.len(), 15);
        assert_eq!((kinds[4], kinds[9]), (StepKind::Landing, StepKind::Landing));
        assert_eq!(kinds.iter().filter(|k| **k == StepKind::Landing).count(), 2);
        for (i, s) in g.steps.iter().enumerate() {
            assert!((s.z - (i + 1) as f64 * 0.18).abs() < 1e-9);
        }
        // The landings are square and no step overlaps another.
        let area: f64 = g.steps.iter().map(|s| polygon::area(&s.outline)).sum();
        assert!((area - (13.0 * 0.28 + 2.0)).abs() < 1e-9, "{area}");
        // The third flight comes back beside the first: it ends one tread
        // behind the start, across the middle flight.
        let end = g.walkline.last().copied().unwrap_or_default();
        assert!((end - DVec3::new(-0.28, 1.0 + 4.0 * 0.28, 2.88)).length() < 1e-9, "{end}");
        let right = geom(StairLayout { turn: Turn::Right, ..layout }, 2.88);
        let r_end = right.walkline.last().copied().unwrap_or_default();
        assert!((r_end.y + end.y).abs() < 1e-9 && (r_end.x - end.x).abs() < 1e-9);
        // The inner railing turns at both inner corners; both railings reach the top.
        let a0 = 4.0 * 0.28;
        let inner = g.railing_path(StairSide::Left, 0.05);
        let has =
            |path: &[DVec3], x: f64, y: f64| path.iter().any(|p| (p.x - x).abs() < 1e-9 && (p.y - y).abs() < 1e-9);
        assert!(has(&inner, a0 + 0.05, 0.45) && has(&inner, a0 + 0.05, 0.5 + a0 + 0.05), "{inner:?}");
        let outer = g.railing_path(StairSide::Right, 0.05);
        assert!(has(&outer, a0 + 0.95, -0.45) && has(&outer, a0 + 0.95, 0.5 + a0 + 0.95), "{outer:?}");
        for path in [&inner, &outer] {
            assert!(path.last().is_some_and(|p| (p.z - 2.88).abs() < 1e-9));
        }
        // Cast in place: three flight blocks and two landing slabs, all closed.
        let build = StairBuild { construction: StairConstruction::CastInPlace, ..Default::default() };
        let pieces = g.pieces(&build);
        assert_eq!(pieces.len(), 5);
        assert_eq!(g.flights.iter().filter(|f| f.landing_ahead > 0.0).count(), 2);
        for piece in pieces {
            let mut s = Solid::new(cc_core::ElementId(1), cc_core::Category::Stair, [0, 0, 0]);
            s.add_polygons(piece);
            assert!(s.volume() > 0.0);
        }
        // The flights can be sized.
        let sized = geom(StairLayout { split: 3, split2: 7, ..layout }, 2.88);
        let treads = |g: &StairGeom| g.flights.iter().map(|f| f.treads).collect::<Vec<_>>();
        assert_eq!(treads(&sized), [2, 6, 5]);
    }

    #[test]
    fn a_stair_can_set_its_risers_and_tread() {
        // 18 risers of 16 cm and 30 cm treads instead of the type's 16 × 18 cm and 28 cm.
        let g = geom(StairLayout { risers: 18, tread: 0.30, ..Default::default() }, 2.88);
        assert_eq!((g.risers, g.steps.len()), (18, 17));
        assert!((g.riser - 0.16).abs() < 1e-12 && (g.tread - 0.30).abs() < 1e-12);
        assert!((g.run() - 17.0 * 0.30).abs() < 1e-9);
        // Too few risers break the type's maximum, and the check says so.
        let few = geom(StairLayout { risers: 12, ..Default::default() }, 2.88);
        assert!((few.riser - 0.24).abs() < 1e-12);
        assert!(few.check.issues.iter().any(|i| i.key() == "stair-issue-riser-high"));
    }

    /// The plumb thickness of a side profile at `s`: between the lowest and
    /// highest points where a vertical line crosses its edges.
    fn thickness_at(profile: &[(f64, f64)], s: f64) -> f64 {
        let n = profile.len();
        let zs: Vec<f64> = (0..n)
            .filter_map(|i| {
                let (a, b) = (profile[i], profile[(i + 1) % n]);
                let (lo, hi) = (a.0.min(b.0), a.0.max(b.0));
                (s >= lo && s <= hi && hi - lo > 1e-12).then(|| a.1 + (b.1 - a.1) * (s - a.0) / (b.0 - a.0))
            })
            .collect();
        let (lo, hi) = zs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), z| (lo.min(*z), hi.max(*z)));
        if hi > lo { hi - lo } else { 0.0 }
    }

    /// Issue #142: a central stringer is a band parallel to the pitch, cut
    /// level on the floor (not a wedge growing from nothing), and the
    /// stringers of flights between landings bear on them instead of
    /// touching their edges.
    #[test]
    fn center_stringers_are_bands_that_bear_on_the_floor_and_landings() {
        let build = StairBuild {
            construction: StairConstruction::Assembled,
            stringers: Stringers::Center,
            stringer_width: 0.06,
            stringer_depth: 0.25,
            tread_thickness: 0.04,
            ..Default::default()
        };
        let (h, g, tt): (f64, f64, f64) = (0.18, 0.28, 0.04);
        let k = h / g;
        let depth = 0.25 * (1.0 + k * k).sqrt();
        let lift = -tt - h;
        // A straight flight from the floor: full depth wherever the band clears the floor.
        let straight = geom(StairLayout::default(), 2.88);
        let f = straight.flights[0];
        let profiles = straight.stringer_profiles(&f, lift, depth, 0.25);
        assert_eq!(profiles.len(), 1);
        let p = &profiles[0];
        let len = f.treads as f64 * g;
        let clear = (depth - (f.z0 + lift)) / k; // where the band's underside reaches the floor
        for i in 1..20 {
            let s = clear + (len - clear) * i as f64 / 20.0;
            assert!((thickness_at(p, s) - depth).abs() < 1e-9, "at {s}: {}", thickness_at(p, s));
        }
        // The built stringer (the pieces within 3 cm of the axis) holds that
        // band: the wedge it used to be held about half of it.
        let mut stringer = 0.0;
        for piece in straight.pieces(&build) {
            if piece.iter().flatten().all(|q| q.y.abs() <= 0.03 + 1e-9) {
                let mut s = Solid::new(cc_core::ElementId(1), cc_core::Category::Stair, [0, 0, 0]);
                s.add_polygons(piece);
                stringer += s.volume();
            }
        }
        let n = p.len();
        let area = (0..n).map(|i| p[i].0 * p[(i + 1) % n].1 - p[(i + 1) % n].0 * p[i].1).sum::<f64>().abs() / 2.0;
        assert!((stringer - 0.06 * area).abs() < 1e-9, "{stringer} vs {}", 0.06 * area);
        assert!(area > 0.9 * depth * len, "{area} vs the full band {}", depth * len);
        // Cut level on the floor, nowhere below it.
        let low = p.iter().map(|q| q.1).fold(f64::INFINITY, f64::min);
        assert!(low.abs() < 1e-12, "{low}");
        assert!(p.iter().filter(|q| q.1.abs() < 1e-12).count() >= 2, "a level seat on the floor: {p:?}");

        // Three flights: the middle one leaves the first landing and rises to the second.
        let three = geom(StairLayout { shape: StairShape::ThreeFlight, ..Default::default() }, 2.88);
        let middle = three.flights[1];
        let pieces = three.stringer_profiles(&middle, lift, depth, 0.25);
        assert_eq!(pieces.len(), 2, "along the flight, and on under the landing");
        // Leaving the first landing: it bears on the landing's face from its underside up.
        let underside_1 = middle.floor - 0.25;
        let start = thickness_at(&pieces[0], 1e-9);
        let top_0 = middle.z0 + lift;
        assert!((start - (top_0 - underside_1)).abs() < 1e-9 && start > 0.15, "{start}");
        // Rising to the second: carried on under it up to its underside, along it.
        let underside_2 = middle.z0 + middle.treads as f64 * h - 0.25;
        let under = &pieces[1];
        let high = under.iter().map(|q| q.1).fold(f64::NEG_INFINITY, f64::max);
        assert!((high - underside_2).abs() < 1e-9, "{high} vs {underside_2}");
        let bearing = under.iter().filter(|q| (q.1 - underside_2).abs() < 1e-9).map(|q| q.0).collect::<Vec<_>>();
        let reach = bearing.iter().fold(f64::NEG_INFINITY, |a, b| a.max(*b))
            - bearing.iter().fold(f64::INFINITY, |a, b| a.min(*b));
        assert!(reach > 0.1, "it bears along the landing's underside for {reach} m");
        // Built, every piece is a closed solid.
        for piece in three.pieces(&build) {
            let mut s = Solid::new(cc_core::ElementId(1), cc_core::Category::Stair, [0, 0, 0]);
            s.add_polygons(piece);
            assert!(s.volume() > 0.0);
        }
    }

    #[test]
    fn spiral_treads_turn_around_the_center() {
        let layout = StairLayout { shape: StairShape::Spiral, inner_radius: 0.1, ..Default::default() };
        let g = geom(layout, 2.88);
        let sp = g.spiral.unwrap_or(Spiral { center: DVec2::ZERO, inner: 0.0, outer: 0.0 });
        assert!((sp.center - DVec2::new(0.0, 0.6)).length() < 1e-9);
        // 15 treads at r = 0.6: 28 cm would turn faster than headroom allows (2.20 m a turn),
        // so each takes 2π × 0.18 / 2.20 rad.
        let delta = TAU * 0.18 / 2.2;
        assert!((g.run() - 15.0 * delta * 0.6).abs() < 5e-3, "{}", g.run());
        assert!((g.tread - delta * 0.6).abs() < 1e-9);
        for st in &g.steps {
            for p in &st.outline {
                let r = p.distance(sp.center);
                assert!(r > 0.1 - 1e-9 && r < 1.1 + 1e-9);
            }
        }
        // An assembled spiral with a central column and a cast helical one are closed solids.
        for build in [
            StairBuild {
                construction: StairConstruction::Assembled,
                stringers: Stringers::Center,
                ..Default::default()
            },
            StairBuild { construction: StairConstruction::Assembled, ..Default::default() },
            StairBuild { construction: StairConstruction::CastInPlace, ..Default::default() },
        ] {
            assert!(volume(&g, &build) > 0.0);
        }
        assert!(!g.railing_path(StairSide::Right, 0.05).is_empty());
    }

    #[test]
    fn cast_flights_are_one_block_each_with_a_foot_and_a_haunch() {
        let layout = StairLayout { shape: StairShape::UShaped, gap: 0.0, ..Default::default() };
        let g = geom(layout, 2.88);
        let build = StairBuild { construction: StairConstruction::CastInPlace, waist: 0.12, ..Default::default() };
        // Two flights and the landing: three closed pieces.
        let pieces = g.pieces(&build);
        assert_eq!(pieces.len(), 3);
        for piece in &pieces {
            let mut s = Solid::new(cc_core::ElementId(1), cc_core::Category::Stair, [0, 0, 0]);
            s.add_polygons(piece.clone());
            assert!(s.volume() > 0.0);
        }
        let first = g.flights[0];
        let profile = g.flight_profile(&first, Some(0.12));
        // A flat foot on the floor, then the soffit; the haunch reaches under the landing.
        let len = first.treads as f64 * g.tread;
        assert!(profile.iter().any(|&(s, z)| s > 1e-6 && s < len && z.abs() < 1e-12), "{profile:?}");
        let landing_under = first.z0 + first.treads as f64 * g.riser - 0.12;
        assert!(profile.iter().any(|&(s, z)| s > len + 1e-6 && (z - landing_under).abs() < 1e-9), "{profile:?}");
        // The second flight starts from the landing's underside.
        let second = g.flight_profile(&g.flights[1], Some(0.12));
        assert!(second.iter().any(|&(_, z)| (z - (g.flights[1].floor - 0.12)).abs() < 1e-9));
    }

    #[test]
    fn a_solid_spiral_is_not_a_column() {
        let g = geom(StairLayout { shape: StairShape::Spiral, inner_radius: 0.1, ..Default::default() }, 2.88);
        let solid = volume(&g, &StairBuild::default());
        let cast = volume(&g, &StairBuild { construction: StairConstruction::CastInPlace, ..Default::default() });
        assert!((solid - cast).abs() < 1e-9);
        // Far less than the steps filled down to the floor.
        let filled: f64 = g.steps.iter().map(|s| polygon::area(&s.outline) * s.z).sum();
        assert!(solid < 0.3 * filled, "{solid} vs {filled}");
    }

    #[test]
    fn few_risers_do_not_break_shapes() {
        for shape in StairShape::ALL {
            for height in [0.0, 0.1, 0.3, 0.5] {
                let g = geom(StairLayout { shape, winders: 4, split: 50, ..Default::default() }, height);
                assert_eq!(g.steps.len(), g.risers.saturating_sub(1));
                let _ = g.railing_path(StairSide::Left, 0.05);
            }
        }
    }
}
