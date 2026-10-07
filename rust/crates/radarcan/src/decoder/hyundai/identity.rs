use crate::{point::Source, Error};
use indexmap::IndexMap;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
pub struct Candidate {
    pub slot: u64,
    pub object_id: i64,
    pub age: i64,
    pub quality: i64,
    pub distance: f64,
    pub lateral: f64,
    pub velocity: f64,
    pub lateral_velocity: f64,
    pub acceleration: f64,
}

#[derive(Clone, Copy)]
pub struct Previous {
    pub slot: u64,
    pub object_id: i64,
    pub age: i64,
    pub distance: f64,
    pub lateral: f64,
    pub cycle: u64,
}

pub struct TrackIds {
    pub next_id: u64,
    pub cycles: IndexMap<Source, u64>,
    pub states: IndexMap<(Source, u64), Previous>,
}

impl Default for TrackIds {
    fn default() -> Self {
        Self {
            next_id: 1000,
            cycles: IndexMap::new(),
            states: IndexMap::new(),
        }
    }
}

impl TrackIds {
    pub fn clear_source(&mut self, source: Source) {
        self.states
            .retain(|(state_source, _), _| *state_source != source);
        self.cycles.shift_remove(&source);
    }

    pub fn assign(
        &mut self,
        source: Source,
        candidates: &[Candidate],
    ) -> Result<BTreeMap<u64, u64>, Error> {
        let cycle = self.cycles.get(&source).copied().unwrap_or(0) + 1;
        self.cycles.insert(source, cycle);
        let previous = self
            .states
            .iter()
            .filter(|((state_source, _), state)| {
                *state_source == source && cycle - state.cycle <= 3
            })
            .map(|((_, id), state)| (*id, *state))
            .collect::<Vec<_>>();
        let mut used = BTreeSet::new();
        let mut assignments = BTreeMap::new();
        for candidate in candidates {
            let mut best: Option<(bool, f64, u64)> = None;
            for (id, state) in &previous {
                if used.contains(id)
                    || candidate.object_id != state.object_id
                    || candidate.age < state.age
                {
                    continue;
                }
                let distance = (candidate.distance - state.distance).abs();
                let lateral = (candidate.lateral - state.lateral).abs();
                if distance > 7. || lateral > 3.2 {
                    continue;
                }
                let value = (candidate.slot != state.slot, distance + lateral * 1.5, *id);
                if best.is_none_or(|current| prefer(value, current)) {
                    best = Some(value);
                }
            }
            let id = match best {
                Some((_, _, id)) => id,
                None => {
                    let id = self.next_id;
                    self.next_id = self.next_id.checked_add(1).ok_or(Error::IntegerOverflow)?;
                    id
                }
            };
            assignments.insert(candidate.slot, id);
            used.insert(id);
            self.states.insert(
                (source, id),
                Previous {
                    slot: candidate.slot,
                    object_id: candidate.object_id,
                    age: candidate.age,
                    distance: candidate.distance,
                    lateral: candidate.lateral,
                    cycle,
                },
            );
        }
        self.states
            .retain(|(state_source, _), state| *state_source != source || cycle - state.cycle <= 3);
        Ok(assignments)
    }
}

fn prefer(value: (bool, f64, u64), current: (bool, f64, u64)) -> bool {
    (!value.0 && current.0)
        || (value.0 == current.0
            && (value.1 < current.1 || (value.1 == current.1 && value.2 < current.2)))
}

pub fn deduplicate(candidates: &[Candidate]) -> Vec<Candidate> {
    let mut objects: Vec<Candidate> = Vec::new();
    for candidate in candidates {
        let duplicate = objects.iter().position(|previous| {
            candidate.object_id == previous.object_id
                && (candidate.distance - previous.distance).abs() <= 2.
                && (candidate.lateral - previous.lateral).abs() <= 1.
                && (candidate.velocity - previous.velocity).abs() <= 3.
        });
        if let Some(index) = duplicate {
            if (candidate.age, candidate.quality) > (objects[index].age, objects[index].quality) {
                objects[index] = *candidate;
            }
        } else {
            objects.push(*candidate);
        }
    }
    objects
}

pub fn position_valid(distance: f64, lateral: f64) -> bool {
    ((0.2 < distance && distance < 180.)
        || ((0. ..=0.2).contains(&distance) && (1.4..=4.5).contains(&lateral.abs())))
        && lateral.abs() < 40.
}
