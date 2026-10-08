use openpilot_ui_application::{context::Context, state::CarConfig};
pub fn car_bytes(car: CarConfig) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut message = capnp::message::Builder::new_default();
    let mut cp = message.init_root::<openpilot_cereal::car_capnp::car_params::Builder>();
    cp.set_alpha_longitudinal_available(car.alpha_longitudinal_available);
    cp.set_openpilot_longitudinal_control(car.openpilot_longitudinal_control);
    cp.set_max_lateral_accel(openpilot_ui_framework::text_layout::float(
        car.max_lateral_accel,
    ));
    Ok(capnp::serialize::write_message_to_words(&message))
}
pub fn initialize(
    context: &Context,
    network: (u16, bool),
) -> Result<(), Box<dyn std::error::Error>> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<openpilot_cereal::log_capnp::event::Builder>();
    event.set_valid(true);
    let mut device = event.init_device_state();
    device.set_network_type(network.0.try_into()?);
    device.set_network_metered(network.1);
    context
        .messages
        .borrow_mut()
        .state
        .update(0.0, &[capnp::serialize::write_message_to_words(&message)])?;
    Ok(())
}
