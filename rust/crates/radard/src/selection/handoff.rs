use super::{position_continuous, Candidate, Identity};
use crate::{point::TrackId, Lead};
use serde::Serialize;

#[derive(Default, Serialize)]
pub struct PrimaryHandoff {
    _identity: Option<Identity>,
    _since_s: Option<f64>,
    _last_primary_s: Option<f64>,
    _last_primary_candidate: Option<Candidate>,
}

impl PrimaryHandoff {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn continuous(&self, now: f64, candidate: &Candidate) -> bool {
        position_continuous(
            self._last_primary_candidate
                .as_ref()
                .map(|candidate| &candidate.lead)
                .zip(self._last_primary_s),
            &candidate.lead,
            now,
            1.,
        )
    }

    pub fn update(
        &mut self,
        now: f64,
        primary: Option<&Lead>,
        candidates: &[Candidate],
        active: Option<&Identity>,
    ) -> Option<Candidate> {
        let values: Vec<_> = candidates
            .iter()
            .filter(|candidate| {
                let lead = &candidate.lead;
                candidate.source.starts_with("corner")
                    && lead.status
                    && lead.v_lead.abs() <= 4.
                    && lead.d_path.abs() <= 0.75
                    && 0.8 < lead.d_rel
                    && lead.d_rel <= 80.
            })
            .collect();
        let primary_id = TrackId(
            primary
                .filter(|lead| lead.status && lead.radar)
                .map_or(-1, |lead| i128::from(lead.radar_track_id)),
        );
        let supported: Vec<_> = values
            .iter()
            .copied()
            .filter(|candidate| candidate.lead.model_prob >= 0.40)
            .collect();
        let primary_candidate = supported
            .iter()
            .copied()
            .find(|candidate| candidate.track_id == primary_id)
            .or_else(|| {
                supported.iter().copied().min_by(|left, right| {
                    left.lead
                        .d_path
                        .abs()
                        .total_cmp(&right.lead.d_path.abs())
                        .then_with(|| (-left.lead.model_prob).total_cmp(&(-right.lead.model_prob)))
                        .then_with(|| left.lead.d_rel.total_cmp(&right.lead.d_rel))
                })
            });
        if let Some(candidate) = primary_candidate {
            let identity = candidate.identity();
            if self._identity.as_ref() != Some(&identity) || !self.continuous(now, candidate) {
                self._identity = Some(identity);
                self._since_s = Some(now);
            }
            self._last_primary_candidate = Some(candidate.clone());
            self._last_primary_s = Some(now);
        }
        let identity = self._identity.as_ref()?;
        let candidate = values
            .iter()
            .copied()
            .find(|candidate| &candidate.identity() == identity)?;
        if candidate.track_id == primary_id {
            return None;
        }
        if active == Some(identity) {
            return Some(candidate.clone());
        }
        if self._last_primary_s.is_none_or(|time| now - time > 1.)
            || !self.continuous(now, candidate)
        {
            self.reset();
            return None;
        }
        if self._since_s.is_none_or(|since| now - since < 0.25) {
            return None;
        }
        let primary = primary.filter(|lead| lead.status)?;
        if candidate.lead.d_rel + 1. >= primary.d_rel {
            return None;
        }
        let mut result = candidate.clone();
        result.confirmed_stationary_shadow = true;
        Some(result)
    }
}
