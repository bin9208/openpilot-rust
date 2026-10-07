use crate::{scalar, Error};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct Object {
    pub object_id: i64,
    #[serde(
        serialize_with = "scalar::serialize_float",
        deserialize_with = "scalar::deserialize_float"
    )]
    pub x: f64,
    #[serde(
        serialize_with = "scalar::serialize_float",
        deserialize_with = "scalar::deserialize_float"
    )]
    pub y: f64,
    #[serde(
        serialize_with = "scalar::serialize_float",
        deserialize_with = "scalar::deserialize_float"
    )]
    pub v: f64,
    #[serde(
        serialize_with = "scalar::serialize_float",
        deserialize_with = "scalar::deserialize_float"
    )]
    pub length: f64,
}

impl Object {
    pub fn distance(&self) -> f64 {
        scalar::maximum(0., self.x - self.length * 0.5 - 0.1)
    }
}

pub struct TrackIds {
    pub next_id: u64,
    pub previous: IndexMap<i64, (u64, Object)>,
}

impl Default for TrackIds {
    fn default() -> Self {
        Self {
            next_id: 1_000_000,
            previous: IndexMap::new(),
        }
    }
}

impl TrackIds {
    pub fn update(&mut self, objects: &BTreeMap<u32, Object>) -> Result<BTreeMap<u32, u64>, Error> {
        let mut counts = BTreeMap::new();
        for object in objects.values() {
            *counts.entry(object.object_id).or_insert(0) += 1;
        }
        let mut assignments = BTreeMap::new();
        let mut current = IndexMap::new();
        for (slot, object) in objects {
            if !(0 < object.object_id
                && object.object_id < 128
                && counts[&object.object_id] == 1
                && 0. <= object.x
                && object.x < 204.7
                && [object.x, object.y, object.v, object.length]
                    .iter()
                    .all(|value| value.is_finite()))
            {
                continue;
            }
            let continuous = self
                .previous
                .get(&object.object_id)
                .filter(|(_, previous)| {
                    (object.x - (previous.x + previous.v * 0.05)).abs() <= 8.
                        && (object.y - previous.y).abs() <= 3.
                        && (object.v - previous.v).abs() <= 4.
                });
            let id = match continuous {
                Some((id, _)) => *id,
                None => {
                    let id = self.next_id;
                    self.next_id = self.next_id.checked_add(1).ok_or(Error::IntegerOverflow)?;
                    id
                }
            };
            assignments.insert(*slot, id);
            current.insert(object.object_id, (id, *object));
        }
        self.previous = current;
        Ok(assignments)
    }
}
