use openpilot_cereal::log_capnp::event;

#[derive(Clone, Copy)]
pub enum Topic {
    Log,
    Error,
}
impl Topic {
    pub fn name(self) -> &'static str {
        match self {
            Self::Log => "logMessage",
            Self::Error => "errorLogMessage",
        }
    }
}

pub fn packet(record: &str, topic: Topic, monotonic_ns: u64) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder>();
    event.set_valid(true);
    event.set_log_mono_time(monotonic_ns);
    match topic {
        Topic::Log => event.set_log_message(record),
        Topic::Error => event.set_error_log_message(record),
    }
    capnp::serialize::write_message_to_words(&message)
}
