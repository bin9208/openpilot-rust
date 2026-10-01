use openpilot_ui_application::{context::Context, mici::onroad::driver_camera::Preview};
use openpilot_ui_framework::widget::WidgetHandle;
use serde::Deserialize;
#[derive(Deserialize)]
pub struct Options {
    pub setup: bool,
    pub rhd: bool,
    pub detected: bool,
    pub orientation: [f32; 3],
    pub deviation: f32,
    pub eyes: [f32; 2],
    pub glasses: f32,
}
pub fn before(
    context: &Context,
    widget: &WidgetHandle,
    options: &Options,
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
    if options.setup {
        widget.get_mut::<Preview>()?.driver_orientation()?;
    }
    Ok(())
}
pub fn snapshot(
    context: &Context,
    widget: &WidgetHandle,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    use openpilot_ui_application::params::Read;
    let widget = widget.get::<Preview>()?;
    Ok(
        serde_json::json!({"frame":widget.has_frame(),"rhd":widget.is_rhd(),"enabled":context.params.boolean("IsDriverViewEnabled")?,"distracted":context.params.bytes("DriverTooDistracted")?.is_some(),"timeout":context.device.borrow().override_interactive_timeout}),
    )
}
