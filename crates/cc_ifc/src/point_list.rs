//! Point lists as text, for the property sets that carry a terrain's points
//! and boundary (`export::TOPOGRAPHY_PSET`): coordinates separated by
//! spaces, points by `;`.

/// Points as text: coordinates separated by spaces, points by `;`.
pub(crate) fn encode(points: impl Iterator<Item = Vec<f64>>) -> String {
    let parts: Vec<String> = points.map(|p| p.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ")).collect();
    parts.join(";")
}

/// Points read from [`encode`] text, `dims` numbers each: at most `max` of
/// them; malformed or non-finite entries are skipped.
pub(crate) fn decode(text: &str, dims: usize, max: usize) -> Vec<Vec<f64>> {
    text.split(';')
        .take(max)
        .filter_map(|p| {
            let v: Vec<f64> = p.split_whitespace().map_while(|s| s.parse::<f64>().ok()).collect();
            (v.len() == dims && v.iter().all(|x| x.is_finite())).then_some(v)
        })
        .collect()
}
