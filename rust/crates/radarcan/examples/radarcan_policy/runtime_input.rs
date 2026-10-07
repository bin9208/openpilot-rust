use openpilot_can::Packet;
use openpilot_cereal::log_capnp::event;
use openpilot_radarcan::{batch::Ego, wire, Error};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(untagged)]
pub enum Input<T> {
    Raw(Vec<u8>),
    Value(T),
}

pub fn packet(input: Input<Packet>) -> Result<Packet, Error> {
    match input {
        Input::Raw(bytes) => Ok(wire::can(&bytes)?),
        Input::Value(packet) => Ok(packet),
    }
}

pub fn ego(input: Input<Ego>) -> Result<Ego, Error> {
    match input {
        Input::Raw(bytes) => Ok(wire::ego(&bytes)?),
        Input::Value(input) => {
            let mut message = capnp::message::Builder::new_default();
            let mut root = message.init_root::<event::Builder>();
            root.set_valid(true);
            root.set_log_mono_time(input.receive_ns);
            let mut state = root.init_car_state();
            state.set_v_ego(input.v_ego as f32);
            state.set_a_ego(input.a_ego as f32);
            let mut batch = state.init_radar_input();
            batch.set_first_can_mono_time(input.first_can_ns);
            batch.set_last_can_mono_time(input.last_can_ns);
            batch.set_can_packet_count(input.packet_count);
            batch.set_receive_mono_time(input.receive_ns);
            Ok(wire::ego(&capnp::serialize::write_message_to_words(
                &message,
            ))?)
        }
    }
}
