use crate::Error;
use openpilot_cereal::{car_capnp as car, log_capnp as log};
use openpilot_messaging::state::State;

pub struct Input<'a> {
    pub messages: &'a State,
    pub model: log::model_data_v2::Reader<'a>,
    pub radar: log::radar_state::Reader<'a>,
    pub car: car::car_state::Reader<'a>,
    pub params: car::car_params::Reader<'a>,
    pub calibration: log::live_calibration_data::Reader<'a>,
    pub longitudinal: log::longitudinal_plan::Reader<'a>,
    pub lateral: log::lateral_plan::Reader<'a>,
    pub controls: log::controls_state::Reader<'a>,
    pub selfdrive: log::selfdrive_state::Reader<'a>,
    pub output: car::car_output::Reader<'a>,
}
macro_rules! reader {
    ($state:expr,$name:literal,$variant:ident) => {
        match $state.topic($name)?.event()?.which()? {
            log::event::Which::$variant(value) => value?,
            _ => return Err(Error::Contract(concat!("model renderer expected ", $name))),
        }
    };
}
impl<'a> Input<'a> {
    pub fn new(messages: &'a State) -> Result<Self, Error> {
        Ok(Self {
            messages,
            model: reader!(messages, "modelV2", ModelV2),
            radar: reader!(messages, "radarState", RadarState),
            car: reader!(messages, "carState", CarState),
            params: reader!(messages, "carParams", CarParams),
            calibration: reader!(messages, "liveCalibration", LiveCalibration),
            longitudinal: reader!(messages, "longitudinalPlan", LongitudinalPlan),
            lateral: reader!(messages, "lateralPlan", LateralPlan),
            controls: reader!(messages, "controlsState", ControlsState),
            selfdrive: reader!(messages, "selfdriveState", SelfdriveState),
            output: reader!(messages, "carOutput", CarOutput),
        })
    }
    pub fn valid(&self, name: &str) -> Result<bool, Error> {
        Ok(self.messages.topic(name)?.valid)
    }
    pub fn alive(&self, name: &str) -> Result<bool, Error> {
        Ok(self.messages.topic(name)?.alive)
    }
    pub fn updated(&self, name: &str) -> Result<bool, Error> {
        Ok(self.messages.topic(name)?.updated)
    }
    pub fn current(&self, started_frame: i64) -> Result<bool, Error> {
        Ok(
            self.messages.topic("modelV2")?.receive_frame >= started_frame
                && self.messages.topic("liveCalibration")?.receive_frame >= started_frame,
        )
    }
}
