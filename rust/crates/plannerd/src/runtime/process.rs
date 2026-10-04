use super::{views::Views, Output, Planner};
use crate::{platform::Clock, radar_decode, types::PlannerMode, Error};
use openpilot_cereal::log_capnp::longitudinal_plan::PlanningTrigger;
use openpilot_logging::{Fields, Number, Value};
use openpilot_messaging::state::State;
mod lateral;
mod longitudinal;

pub struct Tick {
    pub trigger: PlanningTrigger,
    pub model_frame_id: u32,
    pub model_updated: bool,
    pub longitudinal_run: bool,
    pub timings: Vec<(String, Number)>,
}

impl Tick {
    pub fn context(&self) -> Fields {
        [
            (
                "planning_trigger".into(),
                Value::Text(
                    match self.trigger {
                        PlanningTrigger::ModelV2 => "modelV2",
                        PlanningTrigger::LiveTracks => "liveTracks",
                    }
                    .into(),
                ),
            ),
            (
                "model_frame_id".into(),
                Value::Integer(i128::from(self.model_frame_id)),
            ),
        ]
        .into_iter()
        .collect()
    }
}

impl Planner {
    pub fn process(
        &mut self,
        state: &State,
        parameters: &mut impl crate::parameters::Parameters,
        clock: &impl Clock,
        output: &mut impl Output,
    ) -> Result<Tick, Error> {
        let views = Views(state);
        let radar = radar_decode::radar(views.radar()?)?;
        let radar_topic = state.topic("radarState")?;
        if radar_topic.updated {
            self.fast.observe(
                &radar,
                radar_topic.log_mono_time,
                radar_topic.valid && radar_topic.alive,
            );
        }
        let live = state.topic("liveTracks")?;
        let recent = live.seen && clock.monotonic() - live.receive_time <= 0.10;
        let experimental = views.selfdrive()?.get_experimental_mode();
        let use_live = self.live_tracks
            && !experimental
            && self.carrot.mode == PlannerMode::Acc
            && recent
            && radar_topic.valid
            && radar_topic.alive
            && self.fast.lead_one_ready(&radar);
        let trigger_name = if use_live { "liveTracks" } else { "modelV2" };
        let trigger = if use_live {
            PlanningTrigger::LiveTracks
        } else {
            PlanningTrigger::ModelV2
        };
        let topic = state.topic(trigger_name)?;
        let interval_ok = self.last_longitudinal_ns == 0
            || i128::from(topic.log_mono_time) - i128::from(self.last_longitudinal_ns)
                >= 25_000_000;
        let model_topic = state.topic("modelV2")?;
        let run = topic.updated && interval_ok && model_topic.seen;
        let mut tick = Tick {
            trigger,
            model_frame_id: views.model()?.get_frame_id(),
            model_updated: model_topic.updated,
            longitudinal_run: run,
            timings: Vec::new(),
        };
        if run {
            self.last_longitudinal_ns = topic.log_mono_time;
            let elapsed =
                self.longitudinal_step(&views, &radar, trigger, parameters, clock, output)?;
            tick.timings
                .push(("longitudinal_ms".into(), Number::Float(elapsed)));
        }
        if model_topic.updated {
            let [age, elapsed] = self.lateral_step(&views, parameters, clock, output)?;
            tick.timings
                .push(("model_age_at_lateral_ms".into(), Number::Float(age)));
            tick.timings
                .push(("lateral_and_assistance_ms".into(), Number::Float(elapsed)));
        }
        Ok(tick)
    }
}
