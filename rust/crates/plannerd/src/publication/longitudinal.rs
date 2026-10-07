use super::values;
use crate::{
    carrot::{CarrotPlanner, TrafficState, XState},
    fast_radar::FastReason,
    longitudinal_planner::LongitudinalPlanner,
    number::float32,
    types::MpcSource,
    Error,
};
use capnp::{message::Builder, serialize};
use num_traits::ToPrimitive;
use openpilot_cereal::log_capnp::{
    event,
    longitudinal_plan::{FastLeadReason, LongitudinalPlanSource, PlanningTrigger},
    onroad_event::EventName,
};

#[derive(Clone, Copy)]
pub struct Metadata {
    pub now_ns: u64,
    pub valid: bool,
    pub model_ns: u64,
    pub radar_ns: u64,
    pub live_ns: u64,
    pub planner_seconds: f64,
    pub fast_seconds: f64,
    pub fast_mask: u8,
    pub fast_id: i32,
    pub trigger: PlanningTrigger,
    pub fast_reason: FastReason,
    pub lead_status: bool,
}

pub fn longitudinal(
    owner: &LongitudinalPlanner,
    carrot: &CarrotPlanner,
    meta: Metadata,
) -> Result<Vec<u8>, Error> {
    let mut message = Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_valid(meta.valid);
    event.set_log_mono_time(meta.now_ns);
    let mut plan = event.init_longitudinal_plan();
    plan.set_model_mono_time(meta.model_ns);
    plan.reborrow()
        .get_deprecated()
        .set_radar_state_mono_time(meta.radar_ns);
    let difference = (i128::from(meta.now_ns) - i128::from(meta.model_ns))
        .to_f64()
        .ok_or(Error::Contract("publication timestamp difference"))?;
    plan.set_processing_delay(float32(difference / 1e9)?);
    plan.set_solver_execution_time(float32(owner.mpc.times[0])?);
    plan.set_planner_execution_time(float32(meta.planner_seconds)?);
    plan.set_live_tracks_mono_time(meta.live_ns);
    plan.set_fast_lead_track_id(meta.fast_id);
    plan.set_fast_lead_mask(meta.fast_mask);
    plan.set_planning_trigger(meta.trigger);
    plan.set_fast_radar_execution_time(float32(meta.fast_seconds)?);
    plan.set_fast_lead_reason(reason(meta.fast_reason));
    values(plan.reborrow().init_speeds(17), &owner.speed_trajectory)?;
    values(plan.reborrow().init_accels(17), &owner.accel_trajectory)?;
    values(plan.reborrow().init_jerks(17), &owner.jerk_trajectory)?;
    plan.set_has_lead(meta.lead_status);
    plan.set_longitudinal_plan_source(source(owner.mpc.source));
    plan.set_fcw(owner.fcw);
    plan.set_a_target(float32(owner.target_acceleration)?);
    plan.set_a_target_base(float32(owner.base_acceleration)?);
    plan.set_lead_preview_seconds(float32(owner.lead_preview)?);
    plan.set_lead_preview_action_time(float32(owner.preview_action_time)?);
    plan.set_lead_preview_accel(float32(owner.preview_acceleration)?);
    plan.set_a_change_cost(float32(owner.mpc.change_cost)?);
    plan.set_traffic_stop_model_lead_offset(float32(carrot.traffic_stop_model_lead_offset)?);
    plan.set_v_target_now(float32(owner.target_speed)?);
    plan.set_j_target_now(float32(owner.target_jerk)?);
    plan.set_should_stop(owner.should_stop);
    plan.set_allow_brake(true);
    plan.set_allow_throttle(true);
    plan.set_x_state(match carrot.x_state {
        XState::Lead => 0,
        XState::Cruise => 1,
        XState::E2eCruise => 2,
        XState::E2eStop => 3,
        XState::E2ePrepare => 4,
        XState::E2eStopped => 5,
    });
    plan.set_traffic_state(match carrot.traffic_state {
        TrafficState::Off => 0,
        TrafficState::Red => 1,
        TrafficState::Green => 2,
    });
    plan.set_cruise_target(float32(owner.cruise_kph)?);
    plan.set_cruise_coasting_target(float32(owner.coasting_target)?);
    plan.set_cruise_coasting_percent(
        u8::try_from(owner.coasting_percent)
            .map_err(|_| Error::Contract("coasting percentage publication"))?,
    );
    plan.set_t_follow(float32(owner.mpc.following_time)?);
    plan.set_desired_distance(float32(owner.mpc.desired_distance)?);
    let mut events = plan.reborrow().init_events(
        u32::try_from(carrot.events.len()).map_err(|_| Error::Contract("planning event count"))?,
    );
    for (index, name) in carrot.events.iter().enumerate() {
        if !matches!(
            name,
            EventName::TrafficStopping
                | EventName::TrafficSignGreen
                | EventName::TrafficSignChanged
        ) {
            return Err(Error::Contract("unrecognized Carrot planner event"));
        }
        let mut event = events
            .reborrow()
            .get(u32::try_from(index).map_err(|_| Error::Contract("planning event index"))?);
        event.set_name(*name);
        event.set_warning(true);
    }
    plan.set_my_driving_mode(i32::from(carrot.driving_mode));
    Ok(serialize::write_message_to_words(&message))
}

fn source(source: MpcSource) -> LongitudinalPlanSource {
    match source {
        MpcSource::Lead0 => LongitudinalPlanSource::Lead0,
        MpcSource::Lead1 => LongitudinalPlanSource::Lead1,
        MpcSource::Cruise => LongitudinalPlanSource::Cruise,
        MpcSource::E2e => LongitudinalPlanSource::E2e,
    }
}

fn reason(reason: FastReason) -> FastLeadReason {
    match reason {
        FastReason::Inactive => FastLeadReason::Inactive,
        FastReason::NotRadarLead => FastLeadReason::NotRadarLead,
        FastReason::SelectionPending => FastLeadReason::SelectionPending,
        FastReason::SelectionUnstable => FastLeadReason::SelectionUnstable,
        FastReason::TrackMissing => FastLeadReason::TrackMissing,
        FastReason::TrackUnmeasured => FastLeadReason::TrackUnmeasured,
        FastReason::NonFinite => FastLeadReason::NonFinite,
        FastReason::InvalidDistance => FastLeadReason::InvalidDistance,
        FastReason::DistanceDiscontinuity => FastLeadReason::DistanceDiscontinuity,
        FastReason::VelocityDiscontinuity => FastLeadReason::VelocityDiscontinuity,
        FastReason::Active => FastLeadReason::Active,
        FastReason::RadarStateInvalid => FastLeadReason::RadarStateInvalid,
        FastReason::LiveTracksInvalid => FastLeadReason::LiveTracksInvalid,
        FastReason::SelectionStale => FastLeadReason::SelectionStale,
    }
}
