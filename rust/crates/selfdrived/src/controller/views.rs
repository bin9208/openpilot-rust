use super::Error;
use openpilot_cereal::{car_capnp, custom_capnp, log_capnp};
use openpilot_messaging::state::State;

pub struct Views<'a> {
    pub state: &'a State,
}
impl<'a> Views<'a> {
    pub fn device(&self) -> Result<log_capnp::device_state::Reader<'a>, Error> {
        match self.state.topic("deviceState")?.event()?.which()? {
            log_capnp::event::Which::DeviceState(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
    pub fn peripheral(&self) -> Result<log_capnp::peripheral_state::Reader<'a>, Error> {
        match self.state.topic("peripheralState")?.event()?.which()? {
            log_capnp::event::Which::PeripheralState(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
    pub fn model(&self) -> Result<log_capnp::model_data_v2::Reader<'a>, Error> {
        match self.state.topic("modelV2")?.event()?.which()? {
            log_capnp::event::Which::ModelV2(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
    pub fn calibration(&self) -> Result<log_capnp::live_calibration_data::Reader<'a>, Error> {
        match self.state.topic("liveCalibration")?.event()?.which()? {
            log_capnp::event::Which::LiveCalibration(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
    pub fn dm(&self) -> Result<log_capnp::driver_monitoring_state::Reader<'a>, Error> {
        match self
            .state
            .topic("driverMonitoringState")?
            .event()?
            .which()?
        {
            log_capnp::event::Which::DriverMonitoringState(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
    pub fn plan(&self) -> Result<log_capnp::longitudinal_plan::Reader<'a>, Error> {
        match self.state.topic("longitudinalPlan")?.event()?.which()? {
            log_capnp::event::Which::LongitudinalPlan(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
    pub fn pose(&self) -> Result<log_capnp::live_pose::Reader<'a>, Error> {
        match self.state.topic("livePose")?.event()?.which()? {
            log_capnp::event::Which::LivePose(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
    pub fn manager(&self) -> Result<log_capnp::manager_state::Reader<'a>, Error> {
        match self.state.topic("managerState")?.event()?.which()? {
            log_capnp::event::Which::ManagerState(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
    pub fn parameters(&self) -> Result<log_capnp::live_parameters_data::Reader<'a>, Error> {
        match self.state.topic("liveParameters")?.event()?.which()? {
            log_capnp::event::Which::LiveParameters(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
    pub fn radar(&self) -> Result<log_capnp::radar_state::Reader<'a>, Error> {
        match self.state.topic("radarState")?.event()?.which()? {
            log_capnp::event::Which::RadarState(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
    pub fn carrot(&self) -> Result<custom_capnp::carrot_man::Reader<'a>, Error> {
        match self.state.topic("carrotMan")?.event()?.which()? {
            log_capnp::event::Which::CarrotMan(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
    pub fn controls(&self) -> Result<log_capnp::controls_state::Reader<'a>, Error> {
        match self.state.topic("controlsState")?.event()?.which()? {
            log_capnp::event::Which::ControlsState(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
    pub fn control(&self) -> Result<car_capnp::car_control::Reader<'a>, Error> {
        match self.state.topic("carControl")?.event()?.which()? {
            log_capnp::event::Which::CarControl(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
    pub fn assistance(&self) -> Result<log_capnp::driver_assistance::Reader<'a>, Error> {
        match self.state.topic("driverAssistance")?.event()?.which()? {
            log_capnp::event::Which::DriverAssistance(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
    pub fn debug(&self) -> Result<log_capnp::debug_alert::Reader<'a>, Error> {
        match self.state.topic("alertDebug")?.event()?.which()? {
            log_capnp::event::Which::AlertDebug(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
    pub fn feedback(&self) -> Result<log_capnp::audio_feedback::Reader<'a>, Error> {
        match self.state.topic("audioFeedback")?.event()?.which()? {
            log_capnp::event::Which::AudioFeedback(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
    pub fn pandas(
        &self,
    ) -> Result<capnp::struct_list::Reader<'a, log_capnp::panda_state::Owned>, Error> {
        match self.state.topic("pandaStates")?.event()?.which()? {
            log_capnp::event::Which::PandaStates(value) => Ok(value?),
            _ => Err(Error::Contract("topic/event union mismatch")),
        }
    }
}
