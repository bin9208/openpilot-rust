//! Stable central-red support and separated side-green identities.
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug)]
pub struct Candidate {
    pub id: i64,
    pub distance: [f64; 2],
    pub bounds: [f64; 4],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Priority {
    pub kind: &'static str,
    pub support: Vec<i64>,
    pub green_ids: Vec<i64>,
}
fn overlap(a: [f64; 4], b: [f64; 4]) -> bool {
    a[2].min(b[2]) > a[0].max(b[0]) && a[3].min(b[3]) > a[1].max(b[1])
}
pub fn central(
    reds: &[Candidate],
    greens: &[Candidate],
    mut alignment: impl FnMut(&Candidate) -> Option<[f64; 2]>,
) -> Option<Priority> {
    let mut extents = BTreeMap::new();
    for red in reds {
        if let Some(bounds) =
            alignment(red).filter(|b| b.iter().all(|v| v.is_finite()) && b[0] <= b[1])
        {
            extents.insert(red.id, bounds[0].abs().max(bounds[1].abs()));
        }
    }
    let mut support: Vec<_> = extents
        .iter()
        .filter(|(_, extent)| **extent <= 0.75)
        .map(|(id, _)| *id)
        .collect();
    let kind = if support.is_empty() {
        support = extents
            .iter()
            .filter(|(_, extent)| **extent <= 1.25)
            .map(|(id, _)| *id)
            .collect();
        if support.len() < 2 {
            return None;
        }
        "central_red_pair"
    } else {
        "central_red"
    };
    let extent = support
        .iter()
        .map(|id| extents[id])
        .fold(f64::NEG_INFINITY, f64::max);
    let mut green_ids = Vec::new();
    for green in greens {
        let bounds = alignment(green)?;
        if !bounds.iter().all(|v| v.is_finite())
            || bounds[0] > bounds[1]
            || (bounds[0] <= 0. && 0. <= bounds[1])
        {
            return None;
        }
        let nearest = bounds[0].abs().min(bounds[1].abs());
        if nearest < 1.75
            || nearest - extent < 0.5
            || reds.iter().any(|red| overlap(green.bounds, red.bounds))
        {
            return None;
        }
        green_ids.push(green.id);
    }
    let unique: BTreeSet<_> = green_ids.iter().copied().collect();
    if unique.len() != green_ids.len() || reds.iter().any(|red| unique.contains(&red.id)) {
        return None;
    }
    green_ids.sort_unstable();
    Some(Priority {
        kind,
        support,
        green_ids,
    })
}
