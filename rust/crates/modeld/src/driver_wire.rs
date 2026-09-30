use crate::prediction::{DriverData, DriverPrediction};
use openpilot_cereal::log_capnp::{driver_state_v2, event};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct DriverTiming {
    pub log_mono_time: u64,
    pub frame_id: u32,
    pub model_execution_time: f32,
    pub gpu_execution_time: f32,
}

/// Encodes the original dmonitoringmodeld driverStateV2 field mapping.
#[must_use]
pub fn encode(
    prediction: &DriverPrediction,
    timing: DriverTiming,
    raw_predictions: &[u8],
) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder>();
    event.set_log_mono_time(timing.log_mono_time);
    event.set_valid(true);
    let mut driver = event.init_driver_state_v2();
    driver.set_frame_id(timing.frame_id);
    driver.set_model_execution_time(timing.model_execution_time);
    driver.set_gpu_execution_time(timing.gpu_execution_time);
    driver.set_raw_predictions(raw_predictions);
    driver.set_wheel_on_right_prob(prediction.wheel_on_right);
    fill_driver_data(driver.reborrow().init_left_driver_data(), &prediction.left);
    fill_driver_data(driver.init_right_driver_data(), &prediction.right);
    capnp::serialize::write_message_to_words(&message)
}

fn fill_driver_data(mut message: driver_state_v2::driver_data::Builder<'_>, data: &DriverData) {
    let mut orientation = message.reborrow().init_face_orientation(3);
    for (i, value) in (0..3).zip(&data.face.mean[..3]) {
        orientation.set(i, *value);
    }
    let mut orientation_std = message.reborrow().init_face_orientation_std(3);
    for (i, value) in (0..3).zip(&data.face.std[..3]) {
        orientation_std.set(i, *value);
    }
    let mut position = message.reborrow().init_face_position(2);
    for (i, value) in (0..2).zip(&data.face.mean[3..5]) {
        position.set(i, *value);
    }
    let mut position_std = message.reborrow().init_face_position_std(2);
    for (i, value) in (0..2).zip(&data.face.std[3..5]) {
        position_std.set(i, *value);
    }
    let [face, left_eye, right_eye, left_blink, right_blink, sunglasses, phone, sleep] =
        data.probabilities;
    message.set_face_prob(face);
    message.set_left_eye_prob(left_eye);
    message.set_right_eye_prob(right_eye);
    message.set_left_blink_prob(left_blink);
    message.set_right_blink_prob(right_blink);
    message.set_sunglasses_prob(sunglasses);
    message.set_phone_prob(phone);
    message.set_sleep_prob(sleep);
}
