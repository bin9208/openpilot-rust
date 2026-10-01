use openpilot_ui_application::context::Context;

use serde::Deserialize;
#[derive(Deserialize)]
pub struct Driver {
    pub rhd: bool,
    pub detected: bool,
    pub orientation: [f32; 3],
    pub deviation: f32,
    pub eyes: [f32; 2],
    pub glasses: f32,
}
pub fn messages(
    context: &Context,
    options: &Driver,
    index: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut driver = capnp::message::Builder::new_default();
    let mut data = driver
        .init_root::<openpilot_cereal::log_capnp::event::Builder<'_>>()
        .init_driver_state_v2();
    for right in [false, true] {
        let mut data = if right {
            data.reborrow().init_right_driver_data()
        } else {
            data.reborrow().init_left_driver_data()
        };
        let mut orientation = data.reborrow().init_face_orientation(3);
        for (i, value) in options.orientation.iter().enumerate() {
            orientation.set(u32::try_from(i)?, *value);
        }
        let mut deviation = data.reborrow().init_face_orientation_std(3);
        deviation.set(0, options.deviation);
        deviation.set(1, options.deviation);
        deviation.set(2, 0.1);
        let mut position = data.reborrow().init_face_position(2);
        position.set(0, if right { 0.18 } else { -0.18 });
        position.set(1, 0.05);
        data.set_left_eye_prob(options.eyes[0]);
        data.set_right_eye_prob(options.eyes[1]);
        data.set_sunglasses_prob(options.glasses);
    }
    let mut monitoring = capnp::message::Builder::new_default();
    let mut data = monitoring
        .init_root::<openpilot_cereal::log_capnp::event::Builder<'_>>()
        .init_driver_monitoring_state();
    data.set_is_r_h_d(options.rhd);
    data.set_active_policy(
        openpilot_cereal::log_capnp::driver_monitoring_state::MonitoringPolicy::Vision,
    );
    let mut vision = data.init_vision_policy_state();
    vision.set_face_detected(options.detected);
    vision.set_awareness_percent(83);
    context.messages.borrow_mut().state.update(
        f64::from(index) / 20.0,
        &[
            capnp::serialize::write_message_to_words(&driver),
            capnp::serialize::write_message_to_words(&monitoring),
        ],
    )?;
    Ok(())
}

pub fn scroll(
    cards: &mut openpilot_ui_application::mici::layouts::cards::Cards,
    index: usize,
) -> Result<(), openpilot_ui_framework::Error> {
    let rect = cards
        .scroller
        .item(index)
        .ok_or(openpilot_ui_framework::Error::Contract("card index"))?
        .state()
        .rect;
    cards.scroller.scroll_to(
        f64::from(rect.x + rect.width / 2.0) - 268.0,
        false,
        false,
        false,
    )
}
pub fn synchronize(index: u32) -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::path::PathBuf::from(std::env::var("UI_CAMERA_SYNC")?);
    std::fs::write(directory.join(format!("{index}.ready")), b"ready")?;
    let start = std::time::Instant::now();
    while !directory.join(format!("{index}.allow")).exists() {
        if start.elapsed() > std::time::Duration::from_secs(10) {
            return Err("owned camera coordinator timeout".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    Ok(())
}
