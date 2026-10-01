use crate::{
    message::{Packet, Status},
    motion::Input,
    pose::{Measurement, Pose},
    Error,
};
use capnp::{dynamic_value, message::ReaderOptions};
use num_traits::ToPrimitive;
use openpilot_cereal::{
    car_capnp::car_params,
    log_capnp::{event, live_delay_data, live_pose},
};
use std::io::Cursor;
#[derive(Debug, serde::Serialize)]
pub struct Car {
    pub fingerprint: String,
    pub actuator_delay: f64,
}
pub fn car(bytes: &[u8]) -> Result<Car, Error> {
    let message = capnp::serialize::read_message(Cursor::new(bytes), ReaderOptions::new())?;
    let value = message.get_root::<car_params::Reader>()?;
    Ok(Car {
        fingerprint: value.get_car_fingerprint()?.to_str()?.into(),
        actuator_delay: f64::from(value.get_steer_actuator_delay()),
    })
}
pub fn saved(bytes: &[u8], previous: &[u8], current: &Car) -> Result<(f64, i32), Error> {
    let message = capnp::serialize::read_message(Cursor::new(bytes), ReaderOptions::new())?;
    let event = message.get_root::<event::Reader>()?;
    let event::LiveDelay(value) = event.which()? else {
        return Err(Error::Contract("cached Event is not liveDelay"));
    };
    let value = value?;
    if car(previous)?.fingerprint != current.fingerprint {
        return Err(Error::Contract("Car model mismatch"));
    }
    let count = value.get_valid_blocks();
    if count > 50 {
        return Err(Error::Contract("Invalid number of valid blocks"));
    }
    if value.get_status() == Ok(live_delay_data::Status::Invalid) {
        return Err(Error::Contract("Lag estimate is invalid"));
    }
    Ok((f64::from(value.get_lateral_delay_estimate()), count))
}
fn measurement(value: live_pose::x_y_z_measurement::Reader<'_>) -> Measurement {
    Measurement {
        xyz: [value.get_x(), value.get_y(), value.get_z()].map(f64::from),
        std: [value.get_x_std(), value.get_y_std(), value.get_z_std()].map(f64::from),
    }
}
pub fn input(value: event::Reader<'_>) -> Result<(f64, Input), Error> {
    let time = value
        .get_log_mono_time()
        .to_f64()
        .ok_or(Error::Contract("event timestamp conversion"))?
        * 1e-9;
    let input = match value.which()? {
        event::CarControl(value) => Input::Control {
            active: value?.get_lat_active(),
        },
        event::CarState(value) => {
            let value = value?;
            Input::State {
                speed: f64::from(value.get_v_ego()),
                pressed: value.get_steering_pressed(),
            }
        }
        event::ControlsState(value) => {
            let value = value?;
            let dynamic_value::Reader::Struct(union) = value.get_lateral_control_state().into()
            else {
                return Err(Error::Contract("lateral control union"));
            };
            let field = union
                .which()?
                .ok_or(Error::Contract("missing lateral control state"))?;
            let dynamic_value::Reader::Struct(state) = union.get(field)? else {
                return Err(Error::Contract("lateral control payload"));
            };
            let dynamic_value::Reader::Bool(saturated) = state.get_named("saturated")? else {
                return Err(Error::Contract("lateral saturation field"));
            };
            Input::Controls {
                saturated,
                curvature: f64::from(value.get_desired_curvature()),
            }
        }
        event::LiveCalibration(value) => {
            let value = value?;
            let rpy: Vec<_> = value.get_rpy_calib()?.iter().map(f64::from).collect();
            Input::Calibration {
                rpy: rpy
                    .try_into()
                    .map_err(|_| Error::Contract("calibration RPY must contain three entries"))?,
                valid: value.get_cal_status()
                    == Ok(openpilot_cereal::log_capnp::live_calibration_data::Status::Calibrated),
            }
        }
        event::LivePose(value) => {
            let value = value?;
            let angular = value.get_angular_velocity_device()?;
            Input::Pose {
                pose: Pose {
                    orientation: measurement(value.get_orientation_n_e_d()?),
                    velocity: measurement(value.get_velocity_device()?),
                    acceleration: measurement(value.get_acceleration_device()?),
                    angular_velocity: measurement(angular),
                },
                valid: angular.get_valid() && value.get_posenet_o_k() && value.get_inputs_o_k(),
            }
        }
        _ => return Err(Error::Contract("unexpected lag estimator service")),
    };
    Ok((time, input))
}
fn float(value: f64) -> Result<f32, Error> {
    value.to_f32().ok_or(Error::Contract("Float32 conversion"))
}
pub fn encode(packet: &Packet, timestamp: u64, valid: bool) -> Result<Vec<u8>, Error> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder>();
    event.set_log_mono_time(timestamp);
    event.set_valid(valid);
    let mut target = event.init_live_delay();
    target.set_lateral_delay(float(packet.delay)?);
    target.set_lateral_delay_estimate(float(packet.estimate)?);
    target.set_lateral_delay_estimate_std(float(packet.std)?);
    target.set_valid_blocks(packet.valid_blocks);
    target.set_cal_perc(packet.percent);
    target.set_status(match packet.status {
        Status::Unestimated => live_delay_data::Status::Unestimated,
        Status::Estimated => live_delay_data::Status::Estimated,
        Status::Invalid => live_delay_data::Status::Invalid,
    });
    if let Some(points) = &packet.points {
        let points: Vec<_> = points
            .iter()
            .copied()
            .map(float)
            .collect::<Result<_, _>>()?;
        target.set_points(points.as_slice())?;
    }
    Ok(capnp::serialize::write_message_to_words(&message))
}
