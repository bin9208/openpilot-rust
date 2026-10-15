use crate::Error;
use openpilot_cereal::{
    car_capnp::car_state::GearShifter,
    log_capnp::{driver_state_v2, event},
};
use openpilot_messaging::state::State;
use openpilot_monitoring::{DriverData, DriverState, Input};

macro_rules! topic {
    ($state:expr, $name:literal, $kind:ident) => {{
        let event::$kind(value) = $state.topic($name)?.event()?.which()? else {
            return Err(Error::Contract(concat!("expected ", $name)));
        };
        value?
    }};
}

pub fn driver(state: &State) -> Result<driver_state_v2::Reader<'_>, Error> {
    Ok(topic!(state, "driverStateV2", DriverStateV2))
}

fn floats(values: capnp::primitive_list::Reader<'_, f32>) -> Vec<f64> {
    values.iter().map(f64::from).collect()
}

pub fn data(value: driver_state_v2::driver_data::Reader<'_>) -> Result<DriverData, Error> {
    Ok(DriverData {
        face_orientation: Some(floats(value.get_face_orientation()?)),
        face_position: Some(floats(value.get_face_position()?)),
        face_orientation_std: Some(floats(value.get_face_orientation_std()?)),
        face_position_std: Some(floats(value.get_face_position_std()?)),
        face_prob: value.get_face_prob().into(),
        left_eye_prob: value.get_left_eye_prob().into(),
        right_eye_prob: value.get_right_eye_prob().into(),
        left_blink_prob: value.get_left_blink_prob().into(),
        right_blink_prob: value.get_right_blink_prob().into(),
        sunglasses_prob: value.get_sunglasses_prob().into(),
        phone_prob: value.get_phone_prob().into(),
        sleep_prob: value.get_sleep_prob().into(),
    })
}

pub fn input(
    state: &State,
    driver: driver_state_v2::Reader<'_>,
    demo: bool,
) -> Result<Input, Error> {
    let driver = DriverState {
        left: data(driver.get_left_driver_data()?)?,
        right: data(driver.get_right_driver_data()?)?,
        wheel_on_right_prob: driver.get_wheel_on_right_prob().into(),
    };
    if demo {
        return Ok(Input {
            driver,
            demo,
            ..Input::default()
        });
    }
    let car = topic!(state, "carState", CarState);
    let selfdrive = topic!(state, "selfdriveState", SelfdriveState);
    let calibration = topic!(state, "liveCalibration", LiveCalibration);
    let model = topic!(state, "modelV2", ModelV2);
    let brake = model
        .get_meta()?
        .get_disengage_predictions()?
        .get_brake_disengage_probs()?;
    if brake.is_empty() {
        return Err(Error::Contract("empty brakeDisengageProbs"));
    }
    Ok(Input {
        driver,
        car_speed: car.get_v_ego().into(),
        enabled: selfdrive.get_enabled(),
        wrong_gear: !matches!(
            car.get_gear_shifter(),
            Ok(GearShifter::Drive | GearShifter::Low)
        ),
        steering_pressed: car.get_steering_pressed(),
        gas_pressed: car.get_gas_pressed(),
        brake_disengage_prob: brake.get(0).into(),
        steering_angle_deg: car.get_steering_angle_deg().into(),
        calibration: floats(calibration.get_rpy_calib()?),
        demo,
    })
}

/// Reject malformed/NaN camera data before native policy arithmetic.
pub fn camera_usable(state: &State, rhd: bool, demo: bool, now: f64) -> Result<bool, Error> {
    let required = if demo {
        &["driverStateV2"][..]
    } else {
        &["driverStateV2", "liveCalibration", "modelV2"][..]
    };
    if !state.all_checks(required)?
        || (std::env::var_os("REPLAY").is_some()
            && !required.iter().all(|name| {
                state.topic(name).is_ok_and(|t| {
                    let age = now - t.log_mono_time as f64 / 1e9;
                    age >= 0. && age < 10. / t.service.frequency
                })
            }))
    {
        return Ok(false);
    }
    let d = driver(state)?;
    let value = data(if rhd {
        d.get_right_driver_data()?
    } else {
        d.get_left_driver_data()?
    })?;
    if ![
        value.face_prob,
        value.left_eye_prob,
        value.right_eye_prob,
        value.left_blink_prob,
        value.right_blink_prob,
        value.sleep_prob,
        value.phone_prob,
        value.sunglasses_prob,
    ]
    .iter()
    .all(|v| v.is_finite() && (0. ..=1.).contains(v))
    {
        return Ok(false);
    }
    if ![
        value.face_orientation,
        value.face_position,
        value.face_orientation_std,
        value.face_position_std,
    ]
    .iter()
    .all(|v| {
        v.as_ref()
            .is_some_and(|v| v.len() >= 2 && v.iter().all(|n| n.is_finite()))
    }) {
        return Ok(false);
    }
    if !demo {
        let cal = topic!(state, "liveCalibration", LiveCalibration);
        let model = topic!(state, "modelV2", ModelV2);
        let cal = cal.get_rpy_calib()?;
        let brake = model
            .get_meta()?
            .get_disengage_predictions()?
            .get_brake_disengage_probs()?;
        if cal.len() < 3
            || brake.is_empty()
            || !cal.iter().chain(brake.iter()).all(|v| v.is_finite())
        {
            return Ok(false);
        }
    }
    Ok((0. ..0.5).contains(&(now - state.topic("driverStateV2")?.log_mono_time as f64 / 1e9)))
}
