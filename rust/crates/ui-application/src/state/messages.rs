//! Typed cereal extraction from the original 28-service SubMaster boundary.
use super::{ControlState, Input, Panda};
use crate::Error;
use openpilot_messaging::state::State;
pub const SERVICES: [&str; 28] = [
    "modelV2",
    "controlsState",
    "onroadEvents",
    "liveCalibration",
    "radarState",
    "deviceState",
    "pandaStates",
    "carParams",
    "driverMonitoringState",
    "carState",
    "driverStateV2",
    "roadCameraState",
    "wideRoadCameraState",
    "managerState",
    "selfdriveState",
    "longitudinalPlan",
    "gpsLocationExternal",
    "carOutput",
    "carControl",
    "liveParameters",
    "rawAudioData",
    "carrotMan",
    "carrotNavi",
    "peripheralState",
    "liveDelay",
    "liveTorqueParameters",
    "lateralPlan",
    "customReservedRawData0",
];
#[derive(Clone, Copy)]
pub struct Clock {
    pub now: f64,
    pub fps: i32,
}
impl Input {
    pub fn from_messages(sm: &State, clock: Clock) -> Result<Self, Error> {
        let pandas = sm.topic("pandaStates")?;
        let wide = sm.topic("wideRoadCameraState")?;
        let selfdrive = sm.topic("selfdriveState")?;
        let ss = selfdrive_state(sm)?;
        let state = ss.get_state()?;
        use openpilot_cereal::log_capnp::selfdrive_state::OpenpilotState;
        Ok(Self {
            frame: sm.frame(),
            now: clock.now,
            fps: clock.fps,
            panda_updated: pandas.updated,
            panda_receive_frame: pandas.receive_frame,
            pandas: panda_states(sm)?
                .iter()
                .map(|p| {
                    Ok(Panda {
                        panda_type: u16::from(p.get_panda_type()?),
                        ignition_line: p.get_ignition_line(),
                        ignition_can: p.get_ignition_can(),
                    })
                })
                .collect::<Result<_, Error>>()?,
            wide_updated: wide.updated,
            wide_alive: wide.alive,
            wide_valid: wide.valid,
            exposure_percent: f64::from(wide_road_camera_state(sm)?.get_exposure_val_percent()),
            device_started: device_state(sm)?.get_started(),
            selfdrive_updated: selfdrive.updated,
            enabled: ss.get_enabled(),
            control_state: match state {
                OpenpilotState::Disabled => ControlState::Disabled,
                OpenpilotState::PreEnabled => ControlState::PreEnabled,
                OpenpilotState::Enabled => ControlState::Enabled,
                OpenpilotState::SoftDisabling => ControlState::SoftDisabling,
                OpenpilotState::Overriding => ControlState::Overriding,
            },
            lat_active: car_control(sm)?.get_lat_active(),
        })
    }
}

macro_rules! reader {
    ($name:ident, $service:literal, $variant:ident, $result:ty) => {
        pub fn $name(sm: &State) -> Result<$result, Error> {
            match sm.topic($service)?.event()?.which()? {
                openpilot_cereal::log_capnp::event::Which::$variant(value) => Ok(value?),
                _ => Err(Error::Contract(concat!("expected ", $service, " event"))),
            }
        }
    };
}
reader!(
    selfdrive_state,
    "selfdriveState",
    SelfdriveState,
    openpilot_cereal::log_capnp::selfdrive_state::Reader<'_>
);
reader!(
    manager_state,
    "managerState",
    ManagerState,
    openpilot_cereal::log_capnp::manager_state::Reader<'_>
);
reader!(
    device_state,
    "deviceState",
    DeviceState,
    openpilot_cereal::log_capnp::device_state::Reader<'_>
);
reader!(
    car_control,
    "carControl",
    CarControl,
    openpilot_cereal::car_capnp::car_control::Reader<'_>
);
reader!(
    wide_road_camera_state,
    "wideRoadCameraState",
    WideRoadCameraState,
    openpilot_cereal::log_capnp::frame_data::Reader<'_>
);
reader!(
    panda_states,
    "pandaStates",
    PandaStates,
    capnp::struct_list::Reader<'_, openpilot_cereal::log_capnp::panda_state::Owned>
);

reader!(
    driver_state,
    "driverStateV2",
    DriverStateV2,
    openpilot_cereal::log_capnp::driver_state_v2::Reader<'_>
);
reader!(
    driver_monitoring_state,
    "driverMonitoringState",
    DriverMonitoringState,
    openpilot_cereal::log_capnp::driver_monitoring_state::Reader<'_>
);

reader!(
    car_state,
    "carState",
    CarState,
    openpilot_cereal::car_capnp::car_state::Reader<'_>
);
reader!(
    model,
    "modelV2",
    ModelV2,
    openpilot_cereal::log_capnp::model_data_v2::Reader<'_>
);
reader!(
    longitudinal_plan,
    "longitudinalPlan",
    LongitudinalPlan,
    openpilot_cereal::log_capnp::longitudinal_plan::Reader<'_>
);
reader!(
    controls_state,
    "controlsState",
    ControlsState,
    openpilot_cereal::log_capnp::controls_state::Reader<'_>
);
reader!(
    car_output,
    "carOutput",
    CarOutput,
    openpilot_cereal::car_capnp::car_output::Reader<'_>
);
reader!(
    live_parameters,
    "liveParameters",
    LiveParameters,
    openpilot_cereal::log_capnp::live_parameters_data::Reader<'_>
);
reader!(
    vision_data,
    "customReservedRawData0",
    CustomReservedRawData0,
    capnp::data::Reader<'_>
);
reader!(
    radar_state,
    "radarState",
    RadarState,
    openpilot_cereal::log_capnp::radar_state::Reader<'_>
);
reader!(
    onroad_events,
    "onroadEvents",
    OnroadEvents,
    capnp::struct_list::Reader<'_, openpilot_cereal::log_capnp::onroad_event::Owned>
);
reader!(
    carrot_man,
    "carrotMan",
    CarrotMan,
    openpilot_cereal::custom_capnp::carrot_man::Reader<'_>
);
reader!(
    carrot_navi,
    "carrotNavi",
    CarrotNavi,
    openpilot_cereal::custom_capnp::carrot_navi_state::Reader<'_>
);
reader!(
    peripheral_state,
    "peripheralState",
    PeripheralState,
    openpilot_cereal::log_capnp::peripheral_state::Reader<'_>
);
reader!(
    gps_location,
    "gpsLocationExternal",
    GpsLocationExternal,
    openpilot_cereal::log_capnp::gps_location_data::Reader<'_>
);
reader!(
    live_delay,
    "liveDelay",
    LiveDelay,
    openpilot_cereal::log_capnp::live_delay_data::Reader<'_>
);
reader!(
    live_torque,
    "liveTorqueParameters",
    LiveTorqueParameters,
    openpilot_cereal::log_capnp::live_torque_parameters_data::Reader<'_>
);
reader!(
    lateral_plan,
    "lateralPlan",
    LateralPlan,
    openpilot_cereal::log_capnp::lateral_plan::Reader<'_>
);
