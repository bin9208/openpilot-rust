use crate::{
    model,
    types::{diagonal, Event, Input, Pose, Sensor, KINDS},
    Error,
};
use capnp::{message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::{event, live_pose, sensor_event_data};

fn triple(values: capnp::primitive_list::Reader<'_, f32>) -> Result<[f64; 3], Error> {
    if values.len() != 3 {
        return Err(Error::Contract("three vector components required"));
    }
    Ok([
        f64::from(values.get(0)),
        f64::from(values.get(1)),
        f64::from(values.get(2)),
    ])
}
pub fn decode(bytes: &[u8]) -> Result<Event, Error> {
    let message = serialize::read_message(bytes, ReaderOptions::new())?;
    decode_event(message.get_root()?)
}
pub fn decode_event(root: event::Reader<'_>) -> Result<Event, Error> {
    if !root.get_valid() {
        let service = match root.which()? {
            event::Accelerometer(_) => "accelerometer",
            event::Gyroscope(_) => "gyroscope",
            event::CarState(_) => "carState",
            event::LiveCalibration(_) => "liveCalibration",
            event::CameraOdometry(_) => "cameraOdometry",
            _ => return Err(Error::Contract("unexpected locationd service")),
        };
        return Ok(Event {
            log_time_ns: root.get_log_mono_time(),
            valid: false,
            service,
            input: Input::Ignored,
        });
    }
    let (service, input) = match root.which()? {
        event::Accelerometer(sensor) => (
            "accelerometer",
            sensor_input(sensor?, Sensor::Acceleration)?,
        ),
        event::Gyroscope(sensor) => ("gyroscope", sensor_input(sensor?, Sensor::Gyroscope)?),
        event::CarState(car) => ("carState", Input::Speed(f64::from(car?.get_v_ego()))),
        event::LiveCalibration(calibration) => {
            let values = calibration?.get_rpy_calib()?;
            (
                "liveCalibration",
                Input::Calibration(if values.is_empty() {
                    None
                } else {
                    Some(triple(values)?)
                }),
            )
        }
        event::CameraOdometry(camera) => {
            let camera = camera?;
            (
                "cameraOdometry",
                Input::Camera {
                    time: camera.get_timestamp_eof() as f64 * 1e-9 - 0.1,
                    rotation: triple(camera.get_rot()?)?,
                    translation: triple(camera.get_trans()?)?,
                    rotation_std: triple(camera.get_rot_std()?)?,
                    translation_std: triple(camera.get_trans_std()?)?,
                },
            )
        }
        _ => return Err(Error::Contract("unexpected locationd service")),
    };
    Ok(Event {
        log_time_ns: root.get_log_mono_time(),
        valid: root.get_valid(),
        service,
        input,
    })
}
fn sensor_input(sensor: sensor_event_data::Reader<'_>, kind: Sensor) -> Result<Input, Error> {
    let values = match (kind, sensor.which()?) {
        (Sensor::Acceleration, sensor_event_data::Acceleration(vector))
        | (Sensor::Gyroscope, sensor_event_data::GyroUncalibrated(vector)) => {
            let values = vector?.get_v()?;
            if values.len() < 3 {
                return Err(Error::Contract("sensor vector prefix"));
            }
            [
                f64::from(values.get(0)),
                f64::from(values.get(1)),
                f64::from(values.get(2)),
            ]
        }
        _ => return Ok(Input::Ignored),
    };
    Ok(Input::Sensor {
        kind,
        time: sensor.get_timestamp() as f64 * 1e-9,
        secondary: matches!(
            sensor.get_source(),
            Ok(sensor_event_data::SensorSource::Bmx055)
        ),
        values,
    })
}
pub struct Seed {
    pub x: [f64; 18],
    pub covariance: [f64; 324],
}
pub fn seed(bytes: &[u8]) -> Result<Seed, Error> {
    let message = serialize::read_message(bytes, ReaderOptions::new())?;
    let root = message.get_root::<event::Reader<'_>>()?;
    let event::LivePose(pose) = root.which()? else {
        return Err(Error::Contract("initial state must be livePose"));
    };
    let state = pose?.get_debug_filter_state()?;
    let values = state.get_value()?;
    let std = state.get_std()?;
    let mut x = model::INITIAL_X;
    let mut p = model::INITIAL_P;
    if !values.is_empty() {
        if values.len() != 18 {
            return Err(Error::Contract("initial state size"));
        }
        for (i, value) in values.iter().enumerate() {
            x[i] = value;
        }
    }
    if !std.is_empty() {
        if std.len() != 18 {
            return Err(Error::Contract("initial covariance size"));
        }
        for (i, value) in std.iter().enumerate() {
            p[i] = value;
        }
    }
    Ok(Seed {
        x,
        covariance: diagonal(p),
    })
}
fn xyz(mut out: live_pose::x_y_z_measurement::Builder<'_>, x: &[f64], std: &[f64], valid: bool) {
    out.set_x(x[0] as f32);
    out.set_y(x[1] as f32);
    out.set_z(x[2] as f32);
    out.set_x_std(std[0] as f32);
    out.set_y_std(std[1] as f32);
    out.set_z_std(std[2] as f32);
    out.set_valid(valid);
}
pub fn encode(pose: &Pose, publication_ns: u64) -> Result<Vec<u8>, Error> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_valid(pose.filter_valid);
    event.set_log_mono_time(publication_ns);
    let mut out = event.init_live_pose();
    xyz(
        out.reborrow().init_orientation_n_e_d(),
        &pose.x[..3],
        &pose.std[..3],
        pose.filter_valid,
    );
    xyz(
        out.reborrow().init_velocity_device(),
        &pose.x[3..6],
        &pose.std[3..6],
        pose.filter_valid,
    );
    xyz(
        out.reborrow().init_angular_velocity_device(),
        &pose.x[6..9],
        &pose.std[6..9],
        pose.filter_valid,
    );
    xyz(
        out.reborrow().init_acceleration_device(),
        &pose.x[12..15],
        &pose.std[12..15],
        pose.filter_valid,
    );
    out.set_inputs_o_k(pose.inputs_valid);
    out.set_sensors_o_k(pose.sensors_valid);
    out.set_posenet_o_k(pose.posenet_valid);
    out.set_timestamp(pose.timestamp);
    if pose.debug {
        let mut debug = out.init_debug_filter_state();
        debug.set_valid(pose.filter_valid);
        let mut values = debug.reborrow().init_value(18);
        for (i, value) in pose.x.iter().enumerate() {
            values.set(i as u32, *value);
        }
        let mut std = debug.reborrow().init_std(18);
        for (i, value) in pose.std.iter().enumerate() {
            std.set(i as u32, *value);
        }
        let mut observations = debug.init_observations(4);
        for (i, kind) in KINDS.into_iter().enumerate() {
            let mut observation = observations.reborrow().get(i as u32);
            observation.set_kind(kind);
            let mut values = observation.reborrow().init_value(3);
            for j in 0..3 {
                values.set(j as u32, pose.observations[i][j] as f32);
            }
            let mut errors = observation.init_error(3);
            for j in 0..3 {
                errors.set(j as u32, pose.errors[i][j] as f32);
            }
        }
    }
    Ok(serialize::write_message_to_words(&message))
}
