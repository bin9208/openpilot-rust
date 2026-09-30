use crate::{
    estimator::{Car, Estimator, Identity, Packet},
    history::Input,
    Error,
};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use openpilot_cereal::{car_capnp::car_params, log_capnp::event};
use openpilot_runtime_core::filters::FirstOrderFilter;
use std::io::Cursor;

fn car_reader(cp: car_params::Reader<'_>) -> Result<Car, Error> {
    use car_params::lateral_tuning::Which;
    let (tuning, torque) = match cp.get_lateral_tuning().which()? {
        Which::Pid(_) => (0, None),
        Which::IndiDEPRECATED(_) => (1, None),
        Which::LqrDEPRECATED(_) => (2, None),
        Which::Torque(value) => {
            let value = value?;
            (
                3,
                Some([
                    f64::from(value.get_friction()),
                    f64::from(value.get_lat_accel_factor()),
                ]),
            )
        }
    };
    Ok(Car {
        identity: Identity {
            fingerprint: cp.get_car_fingerprint()?.to_str()?.to_owned(),
            tuning,
            torque,
        },
        allowed_brand: ["toyota", "hyundai", "rivian", "honda", "volkswagen"]
            .contains(&cp.get_brand()?.to_str()?),
    })
}
pub fn car(bytes: &[u8]) -> Result<Car, Error> {
    let reader = serialize::read_message(Cursor::new(bytes), ReaderOptions::new())?;
    car_reader(reader.get_root::<car_params::Reader<'_>>()?)
}
pub fn restore(
    estimator: &mut Estimator,
    previous: Option<&[u8]>,
    saved: Option<&[u8]>,
) -> Result<Option<Error>, Error> {
    let mut values = std::array::from_fn(|i| estimator.filters[i].value());
    let result = (|| -> Result<(), Error> {
        if let (Some(previous), Some(saved)) = (previous, saved) {
            let reader = serialize::read_message(Cursor::new(saved), ReaderOptions::new())?;
            let event::LiveTorqueParameters(message) =
                reader.get_root::<event::Reader<'_>>()?.which()?
            else {
                return Err(Error::Contract("cached Event is not liveTorqueParameters"));
            };
            let message = message?;
            let old = car(previous)?;
            if old.identity == estimator.car.identity && message.get_version() == 1 {
                if message.get_live_valid() {
                    values = [
                        message.get_lat_accel_factor_filtered(),
                        message.get_lat_accel_offset_filtered(),
                        message.get_friction_coefficient_filtered(),
                    ]
                    .map(f64::from);
                }
                let points = message.get_points()?;
                estimator.decay = f64::from(message.get_decay());
                for point in points.iter() {
                    let point = point?;
                    if point.len() != 2 {
                        return Err(Error::Contract(
                            "cached torque point must contain two entries",
                        ));
                    }
                    estimator
                        .buckets
                        .add(f64::from(point.get(0)), f64::from(point.get(1)));
                }
            }
        }
        Ok(())
    })();
    if estimator.decay + 0.05 == 0. {
        return Err(Error::Contract("cached filter decay divides by zero"));
    }
    estimator.filters =
        values.map(|value| FirstOrderFilter::new(value, estimator.decay, 0.05, true));
    Ok(result.err())
}
pub fn input(event: event::Reader<'_>) -> Result<Input, Error> {
    let time = event.get_log_mono_time() as f64 * 1e-9;
    Ok(match event.which()? {
        event::CarControl(value) => Input::Control {
            time,
            active: value?.get_lat_active(),
        },
        event::CarOutput(value) => Input::Output {
            time,
            torque: f64::from(value?.get_actuators_output()?.get_torque()),
        },
        event::CarState(value) => {
            let value = value?;
            Input::State {
                time,
                speed: f64::from(value.get_v_ego()),
                pressed: value.get_steering_pressed(),
            }
        }
        event::LiveDelay(value) => Input::Delay(f64::from(value?.get_lateral_delay())),
        event::LiveCalibration(value) => {
            Input::Calibration(value?.get_rpy_calib()?.iter().map(f64::from).collect())
        }
        event::LivePose(value) => {
            let value = value?;
            let angular = value.get_angular_velocity_device()?;
            let orientation = value.get_orientation_n_e_d()?;
            Input::Pose {
                time: value.get_timestamp() as f64 * 1e-9,
                roll: f64::from(orientation.get_x()),
                angular: [angular.get_x(), angular.get_y(), angular.get_z()].map(f64::from),
                valid: angular.get_valid()
                    && orientation.get_valid()
                    && value.get_inputs_o_k()
                    && value.get_sensors_o_k()
                    && value.get_posenet_o_k(),
            }
        }
        _ => return Err(Error::Contract("unexpected torqued input service")),
    })
}
pub fn encode(packet: &Packet, timestamp: u64, valid: bool) -> Result<Vec<u8>, Error> {
    let mut builder = Builder::new_default();
    let mut event = builder.init_root::<event::Builder<'_>>();
    event.set_log_mono_time(timestamp);
    event.set_valid(valid);
    let mut message = event.init_live_torque_parameters();
    // Cereal Float32 assignments intentionally round f64 and preserve nonfinite values.
    message.set_version(1);
    message.set_use_params(packet.use_params);
    message.set_live_valid(packet.live_valid);
    message.set_lat_accel_factor_raw(packet.raw[0] as f32);
    message.set_lat_accel_offset_raw(packet.raw[1] as f32);
    message.set_friction_coefficient_raw(packet.raw[2] as f32);
    message.set_lat_accel_factor_filtered(packet.filtered[0] as f32);
    message.set_lat_accel_offset_filtered(packet.filtered[1] as f32);
    message.set_friction_coefficient_filtered(packet.filtered[2] as f32);
    message.set_total_bucket_points(packet.count as f32);
    message.set_cal_perc(packet.percent);
    message.set_decay(packet.decay as f32);
    message.set_max_resets(packet.resets as f32);
    if let Some(points) = &packet.points {
        let length =
            u32::try_from(points.len()).map_err(|_| Error::Contract("too many torque points"))?;
        let mut list = message.init_points(length);
        for (i, point) in (0..length).zip(points) {
            let mut row = list.reborrow().init(i, 2);
            row.set(0, point[0] as f32);
            row.set(1, point[2] as f32);
        }
    }
    Ok(serialize::write_message_to_words(&builder))
}
