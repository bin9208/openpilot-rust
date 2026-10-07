use crate::point::{Identity, Point};
use indexmap::IndexMap;
use openpilot_plannerd::lead_dynamics::{AccelerationSample, LeadAccelTau};
use serde::Serialize;

#[derive(Default)]
pub struct Dynamics {
    states: IndexMap<Identity, LeadAccelTau>,
}
impl Dynamics {
    pub fn reset(&mut self) {
        self.states.clear();
    }
    pub fn update(&mut self, points: &[Point], time: f64) {
        let mut active = std::collections::HashSet::new();
        for point in points {
            let identity = point.identity();
            active.insert(identity.clone());
            self.states
                .entry(identity)
                .or_insert_with(|| LeadAccelTau::new(1.5))
                .update(AccelerationSample {
                    acceleration: point.a_lead,
                    jerk: point.j_lead,
                    time,
                    measured: point.measured,
                });
        }
        self.states.retain(|identity, _| active.contains(identity));
    }
    pub fn tau(&self, point: &Point) -> f64 {
        let identity = point
            .kinematics_source
            .as_ref()
            .zip(point.kinematics_track_id)
            .map_or_else(
                || point.identity(),
                |(source, id)| Identity(source.clone(), id),
            );
        self.states
            .get(&identity)
            .map_or(1.5, |state| state.snapshot().tau)
    }
}
impl Serialize for Dynamics {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Snapshot {
            #[serde(serialize_with = "crate::primary::entries")]
            _a_lead_tau:
                IndexMap<Identity, openpilot_plannerd::lead_dynamics::LeadAccelTauSnapshot>,
        }
        Snapshot {
            _a_lead_tau: self
                .states
                .iter()
                .map(|(identity, state)| (identity.clone(), state.snapshot()))
                .collect(),
        }
        .serialize(serializer)
    }
}
