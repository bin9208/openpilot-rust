use super::super::inputs::Decoded;
use super::super::{views::Views, Output, Planner};
use crate::{
    fast_radar::{FastInput, FastReason},
    publication::{self, Metadata},
    radar::Radar,
    radar_decode,
    stopping_lead::StopInput,
    types::PlannerMode,
};
use crate::{parameters::Parameters, platform::Clock, Error};
use openpilot_cereal::{
    car_capnp::car_control::actuators::LongControlState,
    log_capnp::longitudinal_plan::PlanningTrigger,
};

impl Planner {
    pub(super) fn longitudinal_step(
        &mut self,
        views: &Views<'_>,
        radar: &Radar,
        trigger: PlanningTrigger,
        parameters: &mut impl Parameters,
        clock: &impl Clock,
        output: &mut impl Output,
    ) -> Result<f64, Error> {
        let state = views.0;
        let radar_topic = state.topic("radarState")?;
        let model_topic = state.topic("modelV2")?;
        let live = state.topic("liveTracks")?;
        let use_live = trigger == PlanningTrigger::LiveTracks;
        let experimental = views.selfdrive()?.get_experimental_mode();
        let started = clock.monotonic();
        let fast_started = if use_live {
            Some(clock.monotonic())
        } else {
            None
        };
        let fast = if use_live {
            Some(self.fast.build(
                radar,
                &radar_decode::points(views.tracks()?)?,
                FastInput {
                    ego_speed: f64::from(views.car()?.get_v_ego()),
                    radar_mono_ns: radar_topic.log_mono_time,
                    live_mono_ns: live.log_mono_time,
                    radar_valid: radar_topic.valid && radar_topic.alive,
                    live_valid: live.valid && live.alive,
                },
            )?)
        } else {
            None
        };
        let fast_seconds = fast_started.map_or(0., |start| clock.monotonic() - start);
        let selected = fast.as_ref().map_or(radar, |fast| &fast.radar_state);
        let use_live_time = fast.as_ref().is_some_and(|fast| fast.lead_mask != 0);
        let stopping = self.stopping.update(
            selected,
            StopInput {
                stopping: self.config.longitudinal.longitudinal_control
                    && views.controls()?.get_long_control_state()? == LongControlState::Stopping
                    && !experimental
                    && self.carrot.mode == PlannerMode::Acc
                    && !views.car()?.get_gas_pressed(),
                speed: f64::from(views.car()?.get_v_ego()),
                mono_time_ns: if use_live_time {
                    live.log_mono_time
                } else {
                    radar_topic.log_mono_time
                },
                valid: views.available(&["radarState", "carState"])?
                    && (!use_live || (live.valid && live.alive)),
            },
        )?;
        let decoded = Decoded::read(views)?;
        let input = decoded.longitudinal(views, &stopping)?;
        let warning = self.longitudinal.update(
            &input,
            &mut self.carrot,
            parameters,
            &mut || clock.wall(),
            &mut || clock.monotonic(),
        )?;
        if let Some(status) = warning {
            output.warning(format!("Long mpc reset, solution_status: {status}"))?;
        }
        if self.longitudinal.fcw {
            output.info("FCW triggered".into())?;
        }
        let planner_seconds = clock.monotonic() - started;
        let bytes = publication::longitudinal(
            &self.longitudinal,
            &self.carrot,
            Metadata {
                now_ns: clock.message_time()?,
                valid: state.all_checks(&["carState", "controlsState", "selfdriveState"])?,
                model_ns: model_topic.log_mono_time,
                radar_ns: radar_topic.log_mono_time,
                live_ns: if live.seen { live.log_mono_time } else { 0 },
                planner_seconds,
                fast_seconds,
                fast_mask: fast.as_ref().map_or(0, |fast| fast.lead_mask),
                fast_id: fast.as_ref().map_or(-1, |fast| fast.lead_one_track_id),
                trigger,
                fast_reason: fast
                    .as_ref()
                    .map_or(FastReason::Inactive, |fast| fast.lead_one_reason),
                lead_status: stopping.lead_one.status,
            },
        )?;
        output.send("longitudinalPlan", &bytes)?;
        Ok((clock.monotonic() - started) * 1000.)
    }
}
