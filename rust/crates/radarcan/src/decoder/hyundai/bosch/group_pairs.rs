//! Temporal pair evidence and provisional geometry before complete-link merging.
use super::{group_config::Config, tracking::Track};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
pub type Pair = (u32, u32);
#[derive(Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub samples: VecDeque<(i64, f64, f64)>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub raw_track_ids: Pair,
    pub timestamp_ns: i64,
    pub last_ages: (usize, usize),
    pub representative_raw_track_id: u32,
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_rel: f64,
}
pub struct Context {
    pub timestamp: i64,
    pub yaw: Option<f64>,
    pub ego: f64,
    pub oem: Option<usize>,
}
pub struct Result {
    pub compatible: Vec<u32>,
    pub pair_cost: Vec<f64>,
    pub provisional_matches: BTreeMap<Pair, (usize, usize, [f64; 5])>,
    pub provisional_handoffs: BTreeSet<Pair>,
    pub pair_candidates: usize,
    pub lateral_rejections: usize,
    pub velocity_rejections: usize,
    pub motion_rejections: usize,
}
pub fn update(
    context: &Context,
    raw: &[Track],
    config: &Config,
    pairs: &mut BTreeMap<Pair, Evidence>,
    bundles: &BTreeMap<Pair, Bundle>,
) -> Result {
    let n = raw.len();
    let timestamp = context.timestamp;
    let index: BTreeMap<_, _> = raw
        .iter()
        .enumerate()
        .map(|(i, r)| (r.raw_track_id, i))
        .collect();
    pairs.retain(|key, evidence| {
        let old = (timestamp - evidence.samples.back().expect("nonempty pair evidence").0) as f64
            > config.evidence_window_s * 1e9;
        let separated = match (index.get(&key.0), index.get(&key.1)) {
            (Some(&i), Some(&j)) => {
                (raw[i].detection.d_rel - raw[j].detection.d_rel).abs() > config.distance_diameter_m
            }
            _ => false,
        };
        !old && !separated
    });
    let mut out = Result {
        compatible: vec![0; n],
        pair_cost: vec![0.; n * n],
        provisional_matches: BTreeMap::new(),
        provisional_handoffs: BTreeSet::new(),
        pair_candidates: 0,
        lateral_rejections: 0,
        velocity_rejections: 0,
        motion_rejections: 0,
    };
    let mut order: Vec<_> = (0..n).collect();
    order.sort_by(|&a, &b| {
        raw[a]
            .detection
            .d_rel
            .partial_cmp(&raw[b].detection.d_rel)
            .expect("finite raw geometry")
    });
    for (position, &a) in order.iter().enumerate() {
        for &b in &order[position + 1..] {
            if raw[b].detection.d_rel - raw[a].detection.d_rel > config.distance_diameter_m {
                break;
            }
            out.pair_candidates += 1;
            let (i, j) = if a < b { (a, b) } else { (b, a) };
            let (left, right) = (&raw[i], &raw[j]);
            let (p, q) = (&left.detection, &right.detection);
            let key = (left.raw_track_id, right.raw_track_id);
            let (dd, dy, dv) = (
                p.d_rel - q.d_rel,
                p.y_rel - q.y_rel,
                (p.v_rel - q.v_rel).abs(),
            );
            if config.provisional_enabled && context.ego.is_finite() {
                let prior = bundles.get(&key);
                let fresh = left.age_scans == 1 && right.age_scans == 1;
                if fresh || prior.is_some() {
                    let yaw = context.yaw.filter(|y| *y != 0.).unwrap_or(0.);
                    let wi = p.v_rel + context.ego - yaw * p.y_rel;
                    let wj = q.v_rel + context.ego - yaw * q.y_rel;
                    let bi = p.y_rel.atan2(p.d_rel.max(0.5)).to_degrees();
                    let bj = q.y_rel.atan2(q.d_rel.max(0.5)).to_degrees();
                    let metrics = [dd.abs(), dy.abs(), (bi - bj).abs(), dv, (wi - wj).abs()];
                    let gap = prior.is_none_or(|v| {
                        (timestamp - v.timestamp_ns) as f64 <= config.pair_max_gap_s * 1e9
                    });
                    let ages = prior.is_none_or(|v| {
                        left.age_scans > v.last_ages.0 && right.age_scans > v.last_ages.1
                    });
                    let supported = context.oem.is_some_and(|o| o == p.slot || o == q.slot);
                    if gap
                        && ages
                        && !supported
                        && config.provisional_min_distance_m <= metrics[0]
                        && metrics[0] <= config.provisional_max_distance_m
                        && metrics[1] <= config.provisional_max_lateral_m
                        && metrics[2] <= config.provisional_max_bearing_deg
                        && metrics[3] <= config.provisional_max_velocity_mps
                        && metrics[4] <= config.provisional_max_world_speed_mps
                        && p.d_rel.min(q.d_rel) >= config.provisional_min_range_m
                        && wi.abs().min(wj.abs()) >= config.provisional_min_world_speed_mps
                        && p.slot.abs_diff(q.slot) == 1
                    {
                        out.provisional_matches.insert(key, (i, j, metrics));
                    }
                }
            }
            if dy.abs() > config.lateral_diameter_m {
                out.lateral_rejections += 1;
                pairs.remove(&key);
                continue;
            }
            if dv > config.velocity_diameter_mps {
                out.velocity_rejections += 1;
                pairs.remove(&key);
                continue;
            }
            if context.ego.is_finite() {
                let (a, b) = ((p.v_rel + context.ego).abs(), (q.v_rel + context.ego).abs());
                if a.min(b) <= config.stationary_speed_mps && a.max(b) >= config.moving_speed_mps {
                    out.motion_rejections += 1;
                    pairs.remove(&key);
                    continue;
                }
            }
            let evidence = pairs.entry(key).or_insert_with(|| Evidence {
                samples: VecDeque::new(),
            });
            let samples = &mut evidence.samples;
            if samples
                .back()
                .is_some_and(|s| (timestamp - s.0) as f64 > config.pair_max_gap_s * 1e9)
            {
                samples.clear();
            }
            let (ad, ay) = (dd.abs(), dy.abs());
            samples.push_back((timestamp, ad, ay));
            while (timestamp - samples.front().unwrap().0) as f64 > config.evidence_window_s * 1e9 {
                samples.pop_front();
            }
            let min_d = samples.iter().map(|s| s.1).reduce(f64::min).unwrap();
            let min_y = samples.iter().map(|s| s.2).reduce(f64::min).unwrap();
            let stable = ad - min_d <= config.max_relative_distance_growth_m
                && ay - min_y <= config.max_relative_lateral_growth_m;
            let mature = samples.len() >= config.min_pair_observations
                && (timestamp - samples.front().unwrap().0) as f64 / 1e9 + 1e-6
                    >= config.min_pair_span_s;
            if stable && mature {
                if bundles.contains_key(&key) {
                    out.provisional_matches.remove(&key);
                    out.provisional_handoffs.insert(key);
                }
                out.compatible[i] |= 1 << j;
                out.compatible[j] |= 1 << i;
                let cost = ad / config.distance_diameter_m
                    + ay / config.lateral_diameter_m
                    + dv / config.velocity_diameter_mps;
                out.pair_cost[i * n + j] = cost;
                out.pair_cost[j * n + i] = cost;
            }
        }
    }
    out
}
