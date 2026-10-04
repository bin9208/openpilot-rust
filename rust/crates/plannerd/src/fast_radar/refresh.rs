use super::{FastRadarOverlay, FastReason};
use crate::{
    lead::Lead,
    lead_dynamics::{AccelerationSample, LeadAccelTau},
    number::wire_float,
    radar::RadarPoint,
    Error,
};
use openpilot_control_policy::math::maximum;

pub(super) struct Refresh<'a> {
    pub role: usize,
    pub point: Option<&'a RadarPoint>,
    pub ego_speed: f64,
    pub age: f64,
    pub sample_time: f64,
}

impl FastRadarOverlay {
    pub(super) fn refresh(
        &mut self,
        lead: &mut Lead,
        input: Refresh<'_>,
    ) -> Result<(bool, FastReason), Error> {
        if let Some(reason) = self.rejection(input.role, lead) {
            return Ok((false, reason));
        }
        let Some(point) = input.point else {
            return Ok((false, FastReason::TrackMissing));
        };
        if !point.measured {
            return Ok((false, FastReason::TrackUnmeasured));
        }
        if ![
            point.d_rel,
            point.v_rel,
            point.a_lead,
            point.j_lead,
            lead.d_rel,
            lead.v_rel,
        ]
        .iter()
        .all(|value| value.is_finite())
        {
            return Ok((false, FastReason::NonFinite));
        }
        let delay = if point.radar_source.corner() {
            0.05
        } else {
            self.front_delay
        };
        let distance = point.d_rel + point.v_rel * delay;
        if distance <= 0.2 {
            return Ok((false, FastReason::InvalidDistance));
        }
        let predicted = lead.d_rel + lead.v_rel * maximum(0., input.age);
        let gate = 1.5 + maximum(point.v_rel.abs(), lead.v_rel.abs()) * 0.20;
        if (distance - predicted).abs() > gate {
            return Ok((false, FastReason::DistanceDiscontinuity));
        }
        if (point.v_rel - lead.v_rel).abs() > 5. {
            return Ok((false, FastReason::VelocityDiscontinuity));
        }
        lead.d_rel = wire_float(distance)?;
        lead.v_rel = wire_float(point.v_rel)?;
        if point.a_rel.is_finite() {
            lead.a_rel = wire_float(point.a_rel)?;
        }
        lead.v_lead = wire_float(input.ego_speed + point.v_rel)?;
        lead.v_lead_k = lead.v_lead;
        lead.a_lead = wire_float(point.a_lead)?;
        lead.a_lead_k = lead.a_lead;
        lead.j_lead = wire_float(point.j_lead)?;
        let tau = self
            .acceleration_tau
            .entry((point.radar_source, lead.radar_track_id))
            .or_insert_with(|| LeadAccelTau::new(lead.a_lead_tau));
        lead.a_lead_tau = wire_float(tau.update(AccelerationSample {
            acceleration: point.a_lead,
            jerk: point.j_lead,
            time: input.sample_time,
            measured: true,
        }))?;
        Ok((true, FastReason::Active))
    }
}
