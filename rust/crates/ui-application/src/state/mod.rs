//! ui_state.py state/transition ordering, separate from graphics and hardware effects.
mod input;
pub mod messages;
mod parameters;
use crate::{cache::TimedCache, params::Read, Error};
pub use input::{ControlState, Input, Panda};
pub use parameters::{CarConfig, ModelStatus, RealtimeParams, SlowParams};
use serde::Serialize;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Disengaged,
    Engaged,
    Override,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    Engaged,
    Offroad,
}
#[derive(Serialize)]
pub struct UiState {
    pub status: Status,
    pub lat_active: bool,
    pub started_frame: i64,
    pub started_time: f64,
    pub started: bool,
    pub ignition: bool,
    pub recording_audio: bool,
    pub panda_type: u16,
    pub light_sensor: f64,
    pub is_release: bool,
    pub realtime: TimedCache<RealtimeParams>,
    pub slow: SlowParams,
    pub param_update_time: f64,
    pub engaged: bool,
    engaged_previous: bool,
    started_previous: bool,
}
impl UiState {
    pub fn new(params: &impl Read, now: f64, models: ModelStatus) -> Result<Self, Error> {
        let mut realtime = TimedCache::new(RealtimeParams::default());
        realtime.refresh(now, || RealtimeParams::read(params));
        let is_release = params.boolean("IsReleaseBranch")?;
        let mut slow = SlowParams::default();
        slow.refresh(params, models)?;
        Ok(Self {
            status: Status::Disengaged,
            lat_active: false,
            started_frame: 0,
            started_time: 0.0,
            started: false,
            ignition: false,
            recording_audio: false,
            panda_type: 0,
            light_sensor: -1.0,
            is_release,
            realtime,
            slow,
            param_update_time: now,
            engaged: false,
            engaged_previous: false,
            started_previous: false,
        })
    }
    pub fn update(
        &mut self,
        input: &Input,
        params: &impl Read,
        models: ModelStatus,
    ) -> Result<Vec<Transition>, Error> {
        if input.panda_updated {
            if let Some(first) = input.pandas.first() {
                self.panda_type = first.panda_type;
                if self.panda_type != 0 {
                    self.ignition = input
                        .pandas
                        .iter()
                        .any(|p| p.ignition_line || p.ignition_can);
                }
            }
        } else if input.frame - input.panda_receive_frame > 5 * i64::from(input.fps) {
            self.panda_type = 0;
        }
        if input.wide_updated {
            self.light_sensor = (100.0 - input.exposure_percent).max(0.0);
        } else if !input.wide_alive || !input.wide_valid {
            self.light_sensor = -1.0;
        }
        self.started = input.device_started && self.ignition;
        self.realtime
            .refresh(input.now, || RealtimeParams::read(params));
        self.recording_audio = self.realtime.value.record_audio && self.started;
        if self.started && input.selfdrive_updated {
            self.status = match input.control_state {
                ControlState::PreEnabled | ControlState::Overriding => Status::Override,
                ControlState::Disabled | ControlState::Enabled | ControlState::SoftDisabling => {
                    if input.enabled {
                        Status::Engaged
                    } else {
                        Status::Disengaged
                    }
                }
            };
            self.lat_active = input.lat_active;
        }
        self.engaged = self.started && input.enabled;
        let mut transitions = Vec::new();
        if self.engaged != self.engaged_previous {
            transitions.push(Transition::Engaged);
            self.engaged_previous = self.engaged;
        }
        if self.started != self.started_previous || input.frame == 1 {
            if self.started {
                self.status = Status::Disengaged;
                self.started_frame = input.frame;
                self.started_time = input.now;
            }
            transitions.push(Transition::Offroad);
            self.started_previous = self.started;
        }
        if input.now - self.param_update_time > 5.0 {
            self.slow.refresh(params, models)?;
            self.param_update_time = input.now;
        }
        Ok(transitions)
    }
}
