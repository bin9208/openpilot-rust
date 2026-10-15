//! Bounded raw/group lineage for final camera-companion publication only.
use super::{group_pairs::Pair, physical::Object};
use serde::{ser::SerializeMap, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
const MAX_AGE_NS: i64 = 40_000_000_000;
const RAW_PAIR_MAX: usize = 32 * 31 / 2;
pub(crate) mod pair_map {
    use super::*;
    pub fn serialize<S: Serializer, V: Serialize>(
        map: &BTreeMap<Pair, V>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_map(Some(map.len()))?;
        for (&(a, b), v) in map {
            out.serialize_entry(&format!("({a}, {b})"), v)?;
        }
        out.end()
    }
    pub fn deserialize<'de, D: Deserializer<'de>, V: Deserialize<'de>>(
        d: D,
    ) -> Result<BTreeMap<Pair, V>, D::Error> {
        let input = BTreeMap::<String, V>::deserialize(d)?;
        let mut out = BTreeMap::new();
        for (k, v) in input {
            let ids = k
                .trim_matches(['(', ')'])
                .split(',')
                .map(|s| s.trim().parse::<u32>().map_err(serde::de::Error::custom))
                .collect::<Result<Vec<_>, D::Error>>()?;
            if ids.len() != 2 {
                return Err(serde::de::Error::custom("expected raw ID pair"));
            }
            out.insert((ids[0], ids[1]), v);
        }
        Ok(out)
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub timestamp_ns: i64,
    pub physical_track_id: u32,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Ancestry {
    #[serde(with = "pair_map")]
    pub common_group_ancestry: BTreeMap<Pair, Evidence>,
    #[serde(deserialize_with = "owners_from_strings")]
    pub common_representative_owners: BTreeMap<u32, BTreeMap<u32, i64>>,
}
impl Ancestry {
    pub fn reset(&mut self) {
        self.common_group_ancestry.clear();
        self.common_representative_owners.clear();
    }
    pub fn update(
        &mut self,
        timestamp: i64,
        states: &BTreeMap<u32, BTreeSet<u32>>,
        objects: &[Object],
    ) {
        let active: BTreeSet<_> = states.values().flat_map(|s| s.iter().copied()).collect();
        let fresh = |ns: i64| (0..=MAX_AGE_NS).contains(&(timestamp - ns));
        self.common_group_ancestry.retain(|&(a, b), e| {
            fresh(e.timestamp_ns) && active.contains(&a) && active.contains(&b)
        });
        self.common_representative_owners.retain(|raw, owners| {
            if !active.contains(raw) {
                return false;
            }
            owners.retain(|pid, ns| states.contains_key(pid) && fresh(*ns));
            !owners.is_empty()
        });
        for obj in objects {
            for (i, a) in obj.members.iter().enumerate() {
                for b in &obj.members[i + 1..] {
                    let key = if a.raw_track_id < b.raw_track_id {
                        (a.raw_track_id, b.raw_track_id)
                    } else {
                        (b.raw_track_id, a.raw_track_id)
                    };
                    self.common_group_ancestry.entry(key).or_insert(Evidence {
                        timestamp_ns: timestamp,
                        physical_track_id: obj.physical_track_id,
                    });
                }
            }
            self.common_representative_owners
                .entry(obj.representative_raw_track_id)
                .or_default()
                .insert(obj.physical_track_id, timestamp);
        }
        if self.common_group_ancestry.len() > RAW_PAIR_MAX {
            let mut ranked: Vec<_> = self
                .common_group_ancestry
                .iter()
                .map(|(k, v)| (*k, v.timestamp_ns))
                .collect();
            ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            let keep: BTreeSet<_> = ranked
                .into_iter()
                .take(RAW_PAIR_MAX)
                .map(|(k, _)| k)
                .collect();
            self.common_group_ancestry.retain(|k, _| keep.contains(k));
        }
    }
    pub fn evidence(
        &self,
        first: &Object,
        second: &Object,
        shared_start: i64,
        timestamp: i64,
    ) -> (bool, bool) {
        let fresh = |ns: i64| (0..=MAX_AGE_NS).contains(&(timestamp - ns));
        let split = first.members.iter().any(|a| {
            second.members.iter().any(|b| {
                if a.raw_track_id == b.raw_track_id {
                    return false;
                }
                let key = if a.raw_track_id < b.raw_track_id {
                    (a.raw_track_id, b.raw_track_id)
                } else {
                    (b.raw_track_id, a.raw_track_id)
                };
                self.common_group_ancestry
                    .get(&key)
                    .is_some_and(|e| e.timestamp_ns < shared_start && fresh(e.timestamp_ns))
            })
        });
        let handoff = self.common_representative_owners.values().any(|owners| {
            [first.physical_track_id, second.physical_track_id]
                .iter()
                .all(|pid| owners.get(pid).is_some_and(|ns| fresh(*ns)))
        });
        (split, handoff)
    }
}

fn owners_from_strings<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<u32, BTreeMap<u32, i64>>, D::Error> {
    let input = BTreeMap::<String, BTreeMap<String, i64>>::deserialize(d)?;
    input
        .into_iter()
        .map(|(raw, owners)| {
            let raw = raw.parse().map_err(serde::de::Error::custom)?;
            let owners = owners
                .into_iter()
                .map(|(pid, ns)| {
                    pid.parse()
                        .map(|pid| (pid, ns))
                        .map_err(serde::de::Error::custom)
                })
                .collect::<Result<_, D::Error>>()?;
            Ok((raw, owners))
        })
        .collect()
}
