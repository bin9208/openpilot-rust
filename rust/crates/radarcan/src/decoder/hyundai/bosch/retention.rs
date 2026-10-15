//! Bounded mature-family morphology memory; certificates never confer identity.
use super::{
    group_config::Config,
    group_pairs::Pair,
    physical::{self, Object, VisionCue},
    tracking::Track,
};
use serde::{
    ser::{SerializeMap, SerializeTuple},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::collections::{BTreeMap, BTreeSet};
mod family_map {
    use super::*;
    pub fn serialize<S: Serializer, V: Serialize>(
        map: &BTreeMap<Vec<u32>, V>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_map(Some(map.len()))?;
        for (key, value) in map {
            let key = format!(
                "({})",
                key.iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            out.serialize_entry(&key, value)?;
        }
        out.end()
    }
    pub fn deserialize<'de, D: Deserializer<'de>, V: Deserialize<'de>>(
        d: D,
    ) -> Result<BTreeMap<Vec<u32>, V>, D::Error> {
        let map = BTreeMap::<String, V>::deserialize(d)?;
        let mut out = BTreeMap::new();
        for (key, value) in map {
            let ids = key
                .trim_matches(['(', ')'])
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().parse().map_err(serde::de::Error::custom))
                .collect::<Result<Vec<u32>, D::Error>>()?;
            out.insert(ids, value);
        }
        Ok(out)
    }
}
#[derive(Clone)]
pub struct History {
    pub ns: i64,
    pub offsets: BTreeMap<Pair, (f64, f64)>,
}
impl Serialize for History {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let offsets: BTreeMap<_, _> = self
            .offsets
            .iter()
            .map(|(&(a, b), v)| (format!("({a}, {b})"), v))
            .collect();
        let mut out = s.serialize_tuple(2)?;
        out.serialize_element(&self.ns)?;
        out.serialize_element(&offsets)?;
        out.end()
    }
}
impl<'de> Deserialize<'de> for History {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let (ns, offsets) = <(i64, BTreeMap<String, (f64, f64)>)>::deserialize(d)?;
        let mut map = BTreeMap::new();
        for (key, value) in offsets {
            let ids: Vec<u32> = key
                .trim_matches(['(', ')'])
                .split(',')
                .map(|s| s.trim().parse().map_err(serde::de::Error::custom))
                .collect::<Result<_, D::Error>>()?;
            if ids.len() != 2 {
                return Err(serde::de::Error::custom("expected raw pair"));
            }
            map.insert((ids[0], ids[1]), value);
        }
        Ok(Self { ns, offsets: map })
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Certificate {
    pub start_ns: i64,
    pub last_strict_ns: i64,
    pub ages: Vec<usize>,
    pub history: Vec<History>,
    pub ready: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Context(pub Option<f64>, pub Option<usize>, pub Vec<VisionCue>);
#[derive(Clone, Serialize, Deserialize)]
pub struct Event {
    pub ns: i64,
    pub family: Vec<u32>,
    pub added: Vec<Pair>,
    pub lease_ms: f64,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Retention {
    #[serde(with = "family_map")]
    pub families: BTreeMap<Vec<u32>, Certificate>,
    #[serde(with = "family_map")]
    pub suspended: BTreeMap<Vec<u32>, Certificate>,
    pub last_raw_ages: BTreeMap<u32, usize>,
    pub now_ns: Option<i64>,
    pub stats: BTreeMap<String, usize>,
    pub events: Vec<Event>,
    pub peak: usize,
    pub archive_peak: usize,
    pub context: Context,
    pub path: Vec<(f64, f64)>,
    pub path_ns: Option<i64>,
}
pub struct Frame<'a> {
    pub config: &'a Config,
    pub timestamp: i64,
    pub raw: &'a [Track],
    pub previous: &'a [&'a Object],
    pub index: &'a BTreeMap<u32, usize>,
    pub ego: f64,
}
fn pairs(family: &[u32]) -> Vec<Pair> {
    let mut out = vec![];
    for (i, &a) in family.iter().enumerate() {
        for &b in &family[i + 1..] {
            out.push((a, b));
        }
    }
    out
}
impl Retention {
    fn count(&mut self, key: &str) {
        *self.stats.entry(key.into()).or_default() += 1;
    }
    pub fn ingest_path(&mut self, path: &[(f64, f64)], timestamp: i64) {
        if path.len() >= 2 {
            self.path = path.to_vec();
            self.path_ns = Some(timestamp);
        }
    }
    fn surface_guard(
        &self,
        frame: &Frame<'_>,
        family: &[u32],
        live: &BTreeMap<u32, &Track>,
    ) -> Option<&'static str> {
        let prior = frame.previous.iter().find(|object| {
            let mut ids: Vec<_> = object.members.iter().map(|m| m.raw_track_id).collect();
            ids.sort_unstable();
            ids.as_slice() == family
        });
        let Some(prior) = prior else {
            return Some("guard_prior_missing_veto");
        };
        let supported: Vec<_> = frame
            .raw
            .iter()
            .map(|raw| super::group_config::vision_support(raw, &self.context.2))
            .collect();
        let dt = (frame.timestamp - prior.timestamp_ns) as f64 / 1e9;
        let yaw = self.context.0.filter(|v| *v != 0.).unwrap_or(0.);
        let angle = -yaw * dt;
        let dx = prior.d_rel + prior.v_rel * dt;
        let (ca, sa) = (angle.cos(), angle.sin());
        let candidates: Vec<_> = family.iter().map(|id| (*live[id]).clone()).collect();
        let rep = physical::representative(
            &candidates,
            frame.index,
            &supported,
            self.context.1,
            Some((
                prior,
                (dx * ca - prior.y_rel * sa, dx * sa + prior.y_rel * ca),
            )),
        )
        .expect("nonempty observed family");
        let mut surface = rep;
        // Final source OEM-nearer publication mode is ACTIVE.
        if let Some(slot) = self.context.1 {
            if let Some(owned) = candidates.iter().find(|m| m.detection.slot == slot) {
                if owned.detection.d_rel < rep.detection.d_rel {
                    surface = owned;
                }
            }
        }
        let nearest = candidates
            .iter()
            .map(|m| m.detection.d_rel)
            .reduce(f64::min)
            .unwrap();
        if surface.detection.d_rel - nearest > 0.25 {
            return Some("guard_surface_not_nearest_veto");
        }
        if self.path.len() < 2
            || self
                .path_ns
                .is_none_or(|ns| frame.timestamp < ns || frame.timestamp - ns > 500_000_000)
        {
            return Some("guard_path_missing_veto");
        }
        let surface_path =
            path_offset(surface.detection.d_rel, surface.detection.y_rel, &self.path).abs();
        for member in &candidates {
            if std::ptr::eq(member, surface) {
                continue;
            }
            if surface.detection.v_rel > member.detection.v_rel + 0.25 {
                return Some("guard_cover_speed_veto");
            }
            let member_path =
                path_offset(member.detection.d_rel, member.detection.y_rel, &self.path).abs();
            if surface_path > member_path.max(0.5) {
                return Some("guard_cover_lateral_veto");
            }
        }
        None
    }
    fn veto(
        certificate: &Certificate,
        pairs: &[Pair],
        offsets: &BTreeMap<Pair, (f64, f64)>,
        live: &BTreeMap<u32, &Track>,
        config: &Config,
        ego: f64,
    ) -> Option<&'static str> {
        for &(a, b) in pairs {
            let (ra, rb) = (&live[&a].detection, &live[&b].detection);
            let (dd, dy) = offsets[&(a, b)];
            let (dd, dy) = (dd.abs(), dy.abs());
            if dd > config.distance_diameter_m + 0.25 || dy > config.lateral_diameter_m + 0.03125 {
                return Some("extent_veto");
            }
            let (sa, sb) = ((ra.v_rel + ego).abs(), (rb.v_rel + ego).abs());
            if (ra.v_rel - rb.v_rel).abs() > 0.5
                || (ego.is_finite()
                    && sa.min(sb) <= config.stationary_speed_mps
                    && sa.max(sb) >= config.moving_speed_mps)
            {
                return Some("motion_veto");
            }
            if !certificate.history.is_empty() {
                let historical: Vec<_> = certificate
                    .history
                    .iter()
                    .map(|h| h.offsets[&(a, b)])
                    .collect();
                let cutoff = certificate.last_strict_ns
                    - (config.evidence_window_s * 1e9).round_ties_even() as i64;
                let recent: Vec<_> = certificate
                    .history
                    .iter()
                    .filter(|h| h.ns >= cutoff)
                    .map(|h| h.offsets[&(a, b)])
                    .collect();
                let min_d = historical
                    .iter()
                    .map(|p| p.0.abs())
                    .reduce(f64::min)
                    .unwrap();
                let min_y = historical
                    .iter()
                    .map(|p| p.1.abs())
                    .reduce(f64::min)
                    .unwrap();
                let unstable = dd - min_d > config.max_relative_distance_growth_m + 0.25
                    || dy - min_y > config.max_relative_lateral_growth_m + 0.1875;
                if recent.is_empty() {
                    return Some("separation_reference_missing_veto");
                }
                let largest = recent
                    .iter()
                    .map(|p| crate::scalar::norm(&[p.0, p.1]))
                    .reduce(f64::max)
                    .unwrap();
                if unstable && crate::scalar::norm(&[dd, dy]) > largest + 0.25 {
                    return Some("shape_growth_veto");
                }
            }
        }
        None
    }
    pub fn before(
        &mut self,
        frame: Frame<'_>,
        compatible: &mut [u32],
        pair_cost: &mut [f64],
    ) -> Result<(), &'static str> {
        let timestamp = frame.timestamp;
        let config = frame.config;
        let live: BTreeMap<_, _> = frame.raw.iter().map(|r| (r.raw_track_id, r)).collect();
        let raw_ids: BTreeSet<_> = live.keys().copied().collect();
        let previous_groups: Vec<BTreeSet<_>> = frame
            .previous
            .iter()
            .map(|o| o.members.iter().map(|m| m.raw_track_id).collect())
            .collect();
        self.events.clear();
        if self.now_ns.is_some_and(|now| {
            timestamp - now > (config.pair_max_gap_s * 1e9).round_ties_even() as i64
        }) {
            self.suspended.clear();
            self.last_raw_ages.clear();
            self.families.clear();
            self.count("gap_resets");
        }
        self.now_ns = Some(timestamp);
        let broken: BTreeSet<_> = live
            .iter()
            .filter(|(rid, raw)| {
                self.last_raw_ages
                    .get(*rid)
                    .is_some_and(|age| raw.age_scans <= *age)
            })
            .map(|(&rid, _)| rid)
            .collect();
        let pruned: Vec<_> = self
            .suspended
            .iter()
            .filter(|(family, cert)| {
                !family.iter().all(|id| raw_ids.contains(id))
                    || family.iter().any(|id| broken.contains(id))
                    || timestamp - cert.last_strict_ns > 3_000_000_000
                    || previous_groups.iter().any(|g| {
                        g.iter().any(|id| family.contains(id))
                            && !g.iter().all(|id| family.contains(id))
                    })
            })
            .map(|(f, _)| f.clone())
            .collect();
        for family in pruned {
            self.suspended.remove(&family);
            self.count("suspended_certificate_pruned");
        }
        let previous: BTreeSet<Vec<u32>> = previous_groups
            .iter()
            .filter(|g| g.len() >= 2)
            .map(|g| g.iter().copied().collect())
            .collect();
        for family in &previous {
            if !self.families.contains_key(family) {
                if let Some(certificate) = self.suspended.get(family) {
                    self.families.insert(family.clone(), certificate.clone());
                    self.count("natural_rejoin_certificate_reused");
                }
            }
        }
        self.families.retain(|family, _| previous.contains(family));
        for family in &previous {
            if !family.iter().all(|id| live.contains_key(id)) {
                self.families.remove(family);
                self.count("missing_member_veto");
                continue;
            }
            let pairs = pairs(family);
            let indices: Vec<_> = family.iter().map(|id| frame.index[id]).collect();
            let strict = pairs
                .iter()
                .all(|(a, b)| compatible[frame.index[a]] & (1 << frame.index[b]) != 0);
            let ages: Vec<_> = family.iter().map(|id| live[id].age_scans).collect();
            let mut certificate = self.families.get(family).cloned();
            if certificate
                .as_ref()
                .is_some_and(|c| ages.iter().zip(&c.ages).any(|(a, b)| a <= b))
            {
                certificate = None;
                self.families.remove(family);
                self.count("age_rollback_veto");
            }
            let offsets: BTreeMap<_, _> = pairs
                .iter()
                .map(|&(a, b)| {
                    (
                        (a, b),
                        (
                            live[&a].detection.d_rel - live[&b].detection.d_rel,
                            live[&a].detection.y_rel - live[&b].detection.y_rel,
                        ),
                    )
                })
                .collect();
            if strict {
                let mut certificate = certificate.unwrap_or(Certificate {
                    start_ns: timestamp,
                    last_strict_ns: timestamp,
                    ages: ages.clone(),
                    history: vec![],
                    ready: false,
                });
                certificate.last_strict_ns = timestamp;
                certificate.ready |= timestamp - certificate.start_ns >= 2_000_000_000;
                certificate.ages = ages;
                certificate.history.push(History {
                    ns: timestamp,
                    offsets,
                });
                if certificate.history.len() > 21 {
                    certificate.history.drain(..certificate.history.len() - 21);
                }
                self.families.insert(family.clone(), certificate);
                continue;
            }
            let Some(mut certificate) = certificate.filter(|c| c.ready) else {
                self.count("maturity_veto");
                self.families.remove(family);
                continue;
            };
            certificate.ages = ages;
            let elapsed = timestamp - certificate.last_strict_ns;
            self.families.insert(family.clone(), certificate.clone());
            if elapsed > 2_000_000_000 {
                self.count("lease_expiry_veto");
                self.families.remove(family);
                continue;
            }
            let mut reason = Self::veto(&certificate, &pairs, &offsets, &live, config, frame.ego);
            if reason.is_none() {
                for outsider in frame.raw {
                    if family.contains(&outsider.raw_track_id) {
                        continue;
                    }
                    let oi = frame.index[&outsider.raw_track_id];
                    if indices.iter().any(|&i| compatible[i] & (1 << oi) != 0) {
                        reason = Some("strict_competitor_veto");
                        break;
                    }
                    if family.iter().any(|id| {
                        let m = &live[id].detection;
                        let o = &outsider.detection;
                        (o.d_rel - m.d_rel).abs() <= 5.
                            && (o.y_rel - m.y_rel).abs() <= 2.
                            && (o.v_rel - m.v_rel).abs() <= 1.
                    }) {
                        reason = Some("near_competitor_veto");
                        break;
                    }
                }
            }
            if reason.is_none() {
                reason = self.surface_guard(&frame, family, &live);
            }
            if let Some(reason) = reason {
                self.count(reason);
                self.families.remove(family);
                continue;
            }
            let mut added = vec![];
            let n = frame.raw.len();
            for (a, b) in pairs {
                let (i, j) = (frame.index[&a], frame.index[&b]);
                if compatible[i] & (1 << j) == 0 {
                    compatible[i] |= 1 << j;
                    compatible[j] |= 1 << i;
                    let (a_track, b_track) = (&live[&a].detection, &live[&b].detection);
                    let cost = (a_track.d_rel - b_track.d_rel).abs() / config.distance_diameter_m
                        + (a_track.y_rel - b_track.y_rel).abs() / config.lateral_diameter_m
                        + (a_track.v_rel - b_track.v_rel).abs() / config.velocity_diameter_mps;
                    pair_cost[i * n + j] = cost;
                    pair_cost[j * n + i] = cost;
                    added.push((a, b));
                }
            }
            self.events.push(Event {
                ns: timestamp,
                family: family.clone(),
                added: added.clone(),
                lease_ms: elapsed as f64 / 1e6,
            });
            self.count("retained_group_scans");
            *self.stats.entry("retained_edges".into()).or_default() += added.len();
        }
        self.peak = self.peak.max(self.families.len());
        if self.families.len() > 16 {
            return Err("nonexclusive family certificate");
        }
        for (family, certificate) in &self.families {
            if certificate.ready {
                self.suspended.insert(family.clone(), certificate.clone());
            }
        }
        if self.suspended.len() > 64 {
            let mut archive: Vec<_> = self
                .suspended
                .iter()
                .map(|(f, c)| (f.clone(), c.clone()))
                .collect();
            archive.sort_by(|(a, x), (b, y)| {
                y.last_strict_ns
                    .cmp(&x.last_strict_ns)
                    .then_with(|| a.cmp(b))
            });
            self.suspended = archive.into_iter().take(64).collect();
        }
        self.last_raw_ages = live.iter().map(|(&id, r)| (id, r.age_scans)).collect();
        self.archive_peak = self.archive_peak.max(self.suspended.len());
        self.peak = self.peak.max(self.families.len() + self.suspended.len());
        Ok(())
    }
}
pub fn path_offset(distance: f64, lateral: f64, path: &[(f64, f64)]) -> f64 {
    let mut before: Option<(f64, f64)> = None;
    for &(x, y) in path {
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        if x >= distance {
            let py = match before {
                None => y,
                Some((bx, _)) if x == bx => y,
                Some((bx, by)) => by + (distance - bx) / (x - bx) * (y - by),
            };
            return lateral + py;
        }
        before = Some((x, y));
    }
    before.map_or(lateral, |(_, y)| lateral + y)
}
