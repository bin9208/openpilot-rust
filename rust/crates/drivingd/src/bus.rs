use crate::Error;
use openpilot_cereal::{
    car_capnp::{car_control, car_state},
    custom_capnp::carrot_man,
    log_capnp::{
        device_state, driver_monitoring_state, event, frame_data, init_data, live_calibration_data,
        live_delay_data, radar_state,
    },
};
use openpilot_messaging::state::State;
use openpilot_modeld::calibration::{CalibrationUpdate, DrivingCalibration};

pub const TOPICS: &[&str] = &[
    "deviceState",
    "carState",
    "roadCameraState",
    "liveCalibration",
    "driverMonitoringState",
    "carControl",
    "liveDelay",
    "carrotMan",
    "radarState",
];
pub const OUTPUTS: &[&str] = &["modelV2", "drivingModelData", "cameraOdometry"];

pub struct Inputs<'a> {
    pub device: device_state::Reader<'a>,
    pub car: car_state::Reader<'a>,
    pub road: frame_data::Reader<'a>,
    pub calibration: live_calibration_data::Reader<'a>,
    pub monitoring: driver_monitoring_state::Reader<'a>,
    pub control: car_control::Reader<'a>,
    pub delay: live_delay_data::Reader<'a>,
    pub navigation: carrot_man::Reader<'a>,
    pub radar: radar_state::Reader<'a>,
}

macro_rules! topic {
    ($state:expr, $name:literal, $kind:ident) => {{
        let event::$kind(value) = $state.topic($name)?.event()?.which()? else {
            return Err(Error::Contract(concat!("expected ", $name)));
        };
        value?
    }};
}

impl<'a> Inputs<'a> {
    pub fn read(state: &'a State) -> Result<Self, Error> {
        Ok(Self {
            device: topic!(state, "deviceState", DeviceState),
            car: topic!(state, "carState", CarState),
            road: topic!(state, "roadCameraState", RoadCameraState),
            calibration: topic!(state, "liveCalibration", LiveCalibration),
            monitoring: topic!(state, "driverMonitoringState", DriverMonitoringState),
            control: topic!(state, "carControl", CarControl),
            delay: topic!(state, "liveDelay", LiveDelay),
            navigation: topic!(state, "carrotMan", CarrotMan),
            radar: topic!(state, "radarState", RadarState),
        })
    }

    pub fn update_calibration(
        &self,
        state: &State,
        target: &mut DrivingCalibration,
        yaw_trim: f64,
        main_wide: bool,
        use_extra: bool,
    ) -> Result<(), Error> {
        if !state.topic("liveCalibration")?.updated
            || !state.topic("roadCameraState")?.seen
            || !state.topic("deviceState")?.seen
        {
            return Ok(());
        }
        let rpy = self.calibration.get_rpy_calib()?;
        if rpy.len() != 3 {
            return Err(Error::Contract(
                "driving calibration must have three angles",
            ));
        }
        let device = match self.device.get_device_type()? {
            init_data::DeviceType::Unknown => "unknown",
            init_data::DeviceType::Neo => "neo",
            init_data::DeviceType::ChffrAndroid => "chffrAndroid",
            init_data::DeviceType::ChffrIos => "chffrIos",
            init_data::DeviceType::Tici => "tici",
            init_data::DeviceType::Pc => "pc",
            init_data::DeviceType::Tizi => "tizi",
            init_data::DeviceType::Mici => "mici",
        };
        let sensor = match self.road.get_sensor()? {
            frame_data::ImageSensor::Unknown => "unknown",
            frame_data::ImageSensor::Ar0231 => "ar0231",
            frame_data::ImageSensor::Ox03c10 => "ox03c10",
            frame_data::ImageSensor::Os04c10 => "os04c10",
        };
        target.update(CalibrationUpdate {
            updated: true,
            road_seen: true,
            device_seen: true,
            rpy: [rpy.get(0), rpy.get(1), rpy.get(2)],
            calibrated: self.calibration.get_cal_status()?
                == live_calibration_data::Status::Calibrated,
            yaw_trim_degrees: yaw_trim,
            device,
            sensor,
            main_wide,
            use_extra,
        })?;
        Ok(())
    }
}
