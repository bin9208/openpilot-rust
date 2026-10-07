use super::{position_continuous, Candidate, Identity};
use crate::{math::maximum, Lead};
use serde::Serialize;

#[derive(Default, Serialize)]
pub struct StationaryShadow {
    _identity: Option<Identity>,
    _since_s: Option<f64>,
    _last_signal_s: Option<f64>,
    _last_candidate: Option<Candidate>,
    _last_time_s: Option<f64>,
}

fn stopped_equivalent(lead: &Lead) -> f64 {
    let speed = maximum(0., lead.v_lead);
    lead.d_rel + speed * speed / (2. * 2.5)
}

impl StationaryShadow {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn update(
        &mut self,
        now: f64,
        primary: Option<&Lead>,
        cutout_probability: f64,
        candidates: &[Candidate],
    ) -> Option<Candidate> {
        let moving = primary.is_some_and(|lead| lead.status && lead.v_lead > 4.);
        let signal = moving && cutout_probability >= 0.70;
        if signal {
            self._last_signal_s = Some(now);
        }
        let held = self._last_signal_s.is_some_and(|time| now - time <= 0.75);
        let eligible: Vec<_> = candidates
            .iter()
            .filter(|candidate| {
                let lead = &candidate.lead;
                lead.v_lead.abs() <= 1.5
                    && lead.d_path.abs() <= 0.75
                    && 0.8 < lead.d_rel
                    && lead.d_rel <= 80.
            })
            .collect();
        if eligible.is_empty() || !held {
            self.reset();
            return None;
        }
        let mut active = eligible.iter().copied().find(|candidate| {
            Some(candidate.identity()).as_ref() == self._identity.as_ref()
                && position_continuous(
                    self._last_candidate
                        .as_ref()
                        .map(|candidate| &candidate.lead)
                        .zip(self._last_time_s),
                    &candidate.lead,
                    now,
                    0.75,
                )
        });
        if active.is_none() && signal {
            if let Some(primary) = primary {
                let obstacle = stopped_equivalent(primary);
                active = eligible
                    .iter()
                    .copied()
                    .filter(|candidate| {
                        candidate.lead.d_rel >= primary.d_rel + 3.
                            && stopped_equivalent(&candidate.lead) < obstacle
                    })
                    .min_by(|left, right| left.lead.d_rel.total_cmp(&right.lead.d_rel));
            }
        }
        let Some(active) = active else {
            self.reset();
            return None;
        };
        let identity = active.identity();
        if self._identity.as_ref() != Some(&identity) {
            self._identity = Some(identity);
            self._since_s = Some(now);
        }
        self._last_candidate = Some(active.clone());
        self._last_time_s = Some(now);
        let mut result = active.clone();
        result.confirmed_stationary_shadow = self._since_s.is_some_and(|since| now - since >= 0.25);
        Some(result)
    }
}
