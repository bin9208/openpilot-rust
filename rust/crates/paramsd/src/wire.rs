use crate::{
    types::{Car, Event, Input, Parameters, Pose},
    Error,
};
use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::{
    car_capnp::car_params,
    log_capnp::{event, live_pose},
};

pub fn car(bytes: &[u8]) -> Result<Car, Error> {
    let message = serialize::read_message(bytes, ReaderOptions::new())?;
    let cp = message.get_root::<car_params::Reader<'_>>()?;
    Ok(Car {
        fingerprint: cp
            .get_car_fingerprint()?
            .to_str()
            .map_err(|_| Error::Contract("car fingerprint utf8"))?
            .to_owned(),
        ratio: f64::from(cp.get_steer_ratio()),
        globals: [
            f64::from(cp.get_mass()),
            f64::from(cp.get_rotational_inertia()),
            f64::from(cp.get_center_to_front()),
            f64::from(cp.get_wheelbase()) - f64::from(cp.get_center_to_front()),
            f64::from(cp.get_tire_stiffness_front()),
            f64::from(cp.get_tire_stiffness_rear()),
        ],
    })
}
fn xyz(value: live_pose::x_y_z_measurement::Reader<'_>) -> ([f64; 3], [f64; 3]) {
    (
        [value.get_x(), value.get_y(), value.get_z()].map(f64::from),
        [value.get_x_std(), value.get_y_std(), value.get_z_std()].map(f64::from),
    )
}
pub fn decode(bytes: &[u8]) -> Result<Event, Error> {
    let message = serialize::read_message(bytes, ReaderOptions::new())?;
    decode_event(message.get_root()?)
}
pub fn decode_event(root: event::Reader<'_>) -> Result<Event, Error> {
    let input = match root.which()? {
        event::LivePose(pose) => {
            let pose = pose?;
            let angular = pose.get_angular_velocity_device()?;
            let (values, std) = xyz(angular);
            let orientation = pose.get_orientation_n_e_d()?;
            Input::Pose(Pose {
                time: pose.get_timestamp() as f64 * 1e-9,
                angular: values,
                angular_std: std,
                yaw_valid: angular.get_valid(),
                roll: f64::from(orientation.get_x()),
                roll_std: f64::from(orientation.get_x_std()),
                sensors_ok: pose.get_sensors_o_k(),
                posenet_ok: pose.get_posenet_o_k(),
            })
        }
        event::CarState(car) => {
            let car = car?;
            Input::Car {
                speed: f64::from(car.get_v_ego()),
                steering: f64::from(car.get_steering_angle_deg()),
            }
        }
        event::LiveCalibration(calibration) => {
            let rpy = calibration?.get_rpy_calib()?;
            if rpy.len() != 3 {
                return Err(Error::Contract("calibration requires three components"));
            }
            Input::Calibration([rpy.get(0), rpy.get(1), rpy.get(2)].map(f64::from))
        }
        event::GpsLocation(gps) | event::GpsLocationExternal(gps) => {
            let gps = gps?;
            Input::Gps {
                fix: gps.get_has_fix(),
                latitude: gps.get_latitude(),
                longitude: gps.get_longitude(),
                bearing: f64::from(gps.get_bearing_deg()),
            }
        }
        _ => return Err(Error::Contract("unexpected paramsd service")),
    };
    Ok(Event {
        time: root.get_log_mono_time() as f64 * 1e-9,
        input,
    })
}
pub fn encode(value: &Parameters, time: u64) -> Result<Vec<u8>, Error> {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_valid(value.valid);
    root.set_log_mono_time(time);
    let mut out = root.init_live_parameters();
    out.set_posenet_valid(true);
    out.set_sensor_valid(value.sensor_valid);
    out.set_steer_ratio(value.x[1] as f32);
    out.set_stiffness_factor(value.x[0] as f32);
    out.set_roll(value.roll as f32);
    out.set_angle_offset_average_deg(value.average as f32);
    out.set_angle_offset_deg(value.offset as f32);
    out.set_steer_ratio_valid(value.ratio_valid);
    out.set_stiffness_factor_valid(value.stiffness_valid);
    out.set_angle_offset_average_valid(value.average_valid);
    out.set_angle_offset_valid(value.offset_valid);
    out.set_valid(value.estimate_valid);
    out.set_steer_ratio_std(value.std[1] as f32);
    out.set_stiffness_factor_std(value.std[0] as f32);
    out.set_angle_offset_average_std(value.std[2] as f32);
    out.set_angle_offset_fast_std(value.std[3] as f32);
    if value.debug {
        let mut debug = out.init_debug_filter_state();
        let mut values = debug.reborrow().init_value(9);
        for (i, &value) in value.x.iter().enumerate() {
            values.set(i as u32, value);
        }
        let mut std = debug.init_std(9);
        for (i, &value) in value.std.iter().enumerate() {
            std.set(i as u32, value);
        }
    }
    Ok(serialize::write_message_to_words(&message))
}
