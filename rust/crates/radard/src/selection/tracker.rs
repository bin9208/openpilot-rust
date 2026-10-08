use super::{position_continuous, selected_indices, Candidate, Exceptions, Identity, Selection};
use crate::Lead;
use serde::Serialize;

#[derive(Default, Serialize)]
pub struct LeadTwoTracker {
    pub active_identity: Option<Identity>,
    _active_stationary_shadow: bool,
    _last_lead: Option<Lead>,
    _last_time_s: Option<f64>,
}

impl LeadTwoTracker {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn update(
        &mut self,
        now: f64,
        primary: Option<&Lead>,
        candidates: &[Candidate],
    ) -> Selection {
        let active: Vec<_> = candidates
            .iter()
            .map(|candidate| {
                self.active_identity.as_ref().is_some_and(|identity| {
                    candidate.source == identity.0
                        && candidate.continuity_id == identity.2
                        && candidate.retainable
                        && position_continuous(
                            self._last_lead.as_ref().zip(self._last_time_s),
                            &candidate.lead,
                            now,
                            0.75,
                        )
                })
            })
            .collect();
        let mut eligible = Vec::new();
        let mut exceptions = Exceptions::default();
        let mut confirmed = Vec::new();
        let mut confirmed_exceptions = Exceptions::default();
        for (candidate, active) in candidates.iter().zip(&active) {
            if !(candidate.confirmed_cutin || candidate.confirmed_stationary_shadow || *active) {
                continue;
            }
            eligible.push(candidate);
            let id = candidate.lead.radar_track_id;
            if *active || candidate.confirmed_stationary_shadow || candidate.allow_low_speed {
                exceptions.stopped.insert(id);
            }
            if candidate.confirmed_stationary_shadow || (*active && self._active_stationary_shadow)
            {
                exceptions.farther.insert(id);
                exceptions.proximity.insert(id);
            }
            if candidate.confirmed_cutin {
                confirmed.push(candidate);
                if *active || candidate.allow_low_speed {
                    confirmed_exceptions.stopped.insert(id);
                }
            }
        }
        let indices = selected_indices(primary, &eligible, &exceptions);
        let selected = indices.first().map(|index| eligible[*index]);
        let confirmed_indices = selected_indices(primary, &confirmed, &confirmed_exceptions);
        let selection = Selection {
            cutins: confirmed_indices
                .iter()
                .map(|index| confirmed[*index].lead)
                .collect(),
            lead_two: selected.map(|candidate| candidate.lead),
        };
        if let Some(candidate) = selected {
            let identity = candidate.identity();
            self._active_stationary_shadow = candidate.confirmed_stationary_shadow
                || (self.active_identity.as_ref() == Some(&identity)
                    && self._active_stationary_shadow);
            self.active_identity = Some(identity);
            self._last_lead = Some(candidate.lead);
            self._last_time_s = Some(now);
        } else if let Some(identity) = &self.active_identity {
            if candidates
                .iter()
                .any(|candidate| candidate.source == identity.0 && candidate.track_id == identity.1)
            {
                self.reset();
            }
        }
        selection
    }
}
