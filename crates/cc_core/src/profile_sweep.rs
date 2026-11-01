//! Swept profiles: a closed profile, held by the element's type, carried
//! along a path drawn in plan at a height above the element's level. They
//! model what runs along walls, ceilings and roof edges: baseboards, crown
//! moldings, fascia boards, gutters, copings.
//!
//! The profile is drawn in its own plane, square to the path: `x` across,
//! positive to the left of the path walking along it (`flip` mirrors it to
//! the right), and `y` up from the path's height.

use glam::DVec2;
use serde::{Deserialize, Serialize};

/// Most points of a profile.
pub const MAX_PROFILE_POINTS: usize = 512;
/// Largest extent of a profile, meters.
pub const MAX_PROFILE_SIZE: f64 = 10.0;
/// Most points of a sweep's path.
pub const MAX_PATH_POINTS: usize = 4096;

/// A swept profile element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sweep {
    /// The path in plan, in order.
    pub path: Vec<DVec2>,
    /// The path runs back from its last point to its first (around a room).
    #[serde(default)]
    pub closed: bool,
    /// Height of the path above the level.
    pub base_offset: f64,
    /// The profile on the right of the path instead of the left.
    #[serde(default)]
    pub flip: bool,
}

impl Sweep {
    /// Plan length of the path.
    pub fn length(&self) -> f64 {
        let open: f64 = self.path.windows(2).map(|w| w[0].distance(w[1])).sum();
        let back = match (self.closed, self.path.first(), self.path.last()) {
            (true, Some(a), Some(b)) => a.distance(*b),
            _ => 0.0,
        };
        open + back
    }

    pub fn is_valid(&self) -> bool {
        let needed = if self.closed { 3 } else { 2 };
        self.base_offset.is_finite()
            && self.path.len() >= needed
            && self.path.len() <= MAX_PATH_POINTS
            && self.path.iter().all(|p| p.is_finite())
            && self.length() > 1e-6
    }
}

/// Why a profile is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileProblem {
    TooFewPoints,
    TooManyPoints,
    NotFinite,
    TooLarge,
    NoArea,
    Crossing,
}

impl ProfileProblem {
    pub fn message(self) -> &'static str {
        match self {
            ProfileProblem::TooFewPoints => "a profile needs at least 3 points",
            ProfileProblem::TooManyPoints => "a profile has at most 512 points",
            ProfileProblem::NotFinite => "a profile point is not a finite number",
            ProfileProblem::TooLarge => "a profile is at most 10 m across",
            ProfileProblem::NoArea => "a profile must enclose an area",
            ProfileProblem::Crossing => "a profile must not cross itself",
        }
    }
}

/// Check a profile: a simple closed outline of a sensible size.
pub fn check_profile(points: &[DVec2]) -> Result<(), ProfileProblem> {
    let n = points.len();
    if n < 3 {
        return Err(ProfileProblem::TooFewPoints);
    }
    if n > MAX_PROFILE_POINTS {
        return Err(ProfileProblem::TooManyPoints);
    }
    if points.iter().any(|p| !p.is_finite()) {
        return Err(ProfileProblem::NotFinite);
    }
    if points.iter().any(|p| p.abs().max_element() > MAX_PROFILE_SIZE) {
        return Err(ProfileProblem::TooLarge);
    }
    // Edges that are not neighbors must not meet.
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            let (c, d) = (points[j], points[(j + 1) % n]);
            if segments_meet(a, b, c, d) {
                return Err(ProfileProblem::Crossing);
            }
        }
    }
    let area: f64 = (0..n).map(|i| points[i].perp_dot(points[(i + 1) % n])).sum::<f64>() / 2.0;
    if area.abs() < 1e-8 {
        return Err(ProfileProblem::NoArea);
    }
    Ok(())
}

fn segments_meet(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let side = |p: DVec2, q: DVec2, r: DVec2| (q - p).perp_dot(r - p);
    let (d1, d2) = (side(c, d, a), side(c, d, b));
    let (d3, d4) = (side(a, b, c), side(a, b, d));
    let eps = 1e-12;
    if ((d1 > eps && d2 < -eps) || (d1 < -eps && d2 > eps)) && ((d3 > eps && d4 < -eps) || (d3 < -eps && d4 > eps)) {
        return true;
    }
    // Touching or overlapping.
    let on = |p: DVec2, q: DVec2, r: DVec2| {
        side(p, q, r).abs() <= eps
            && r.x >= p.x.min(q.x) - eps
            && r.x <= p.x.max(q.x) + eps
            && r.y >= p.y.min(q.y) - eps
            && r.y <= p.y.max(q.y) + eps
    };
    on(c, d, a) || on(c, d, b) || on(a, b, c) || on(a, b, d)
}

/// The profile of a new project's sweep type: a baseboard 1.5 cm thick and
/// 10 cm tall, on the left of the path.
pub fn baseboard() -> Vec<DVec2> {
    vec![DVec2::new(0.0, 0.0), DVec2::new(0.015, 0.0), DVec2::new(0.015, 0.1), DVec2::new(0.0, 0.1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_are_checked() {
        assert_eq!(check_profile(&baseboard()), Ok(()));
        assert_eq!(check_profile(&baseboard()[..2]), Err(ProfileProblem::TooFewPoints));
        let bow = [DVec2::ZERO, DVec2::new(1.0, 1.0), DVec2::new(1.0, 0.0), DVec2::new(0.0, 1.0)];
        assert_eq!(check_profile(&bow), Err(ProfileProblem::Crossing));
        let flat = [DVec2::ZERO, DVec2::X, DVec2::new(2.0, 0.0)];
        assert_eq!(check_profile(&flat), Err(ProfileProblem::NoArea));
        let mut nan = baseboard();
        nan[1].x = f64::NAN;
        assert_eq!(check_profile(&nan), Err(ProfileProblem::NotFinite));
        let huge = [DVec2::ZERO, DVec2::new(20.0, 0.0), DVec2::new(0.0, 1.0)];
        assert_eq!(check_profile(&huge), Err(ProfileProblem::TooLarge));
    }

    #[test]
    fn closed_paths_count_the_way_back() {
        let square = vec![DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y];
        let open = Sweep { path: square.clone(), closed: false, base_offset: 0.0, flip: false };
        assert!((open.length() - 3.0).abs() < 1e-12 && open.is_valid());
        let closed = Sweep { closed: true, ..open };
        assert!((closed.length() - 4.0).abs() < 1e-12 && closed.is_valid());
        let short = Sweep { path: square[..2].to_vec(), ..closed };
        assert!(!short.is_valid());
    }
}
