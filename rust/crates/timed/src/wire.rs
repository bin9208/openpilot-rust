use crate::Error;
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::state::State;

#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
pub struct Gps {
    pub updated: bool,
    pub log_mono_time: u64,
    pub has_fix: bool,
    pub longitude: f64,
    pub unix_timestamp_millis: i64,
}
impl Gps {
    pub fn usable(&self, now: u64) -> bool {
        // The source divides each nanosecond timestamp into float seconds before comparing.
        self.updated && self.has_fix && (now as f64 / 1e9 - self.log_mono_time as f64 / 1e9) <= 2.0
    }
    pub fn epoch(&self) -> f64 {
        // Matches the source's float conversion before division, including large i64 rounding.
        self.unix_timestamp_millis as f64 / 1000.0
    }
}
pub fn gps(state: &State, service: &str) -> Result<Gps, Error> {
    let topic = state.topic(service)?;
    let event = topic.event()?;
    let gps = match event
        .which()
        .map_err(|_| Error::Contract("invalid GPS event"))?
    {
        event::Which::GpsLocation(gps) | event::Which::GpsLocationExternal(gps) => gps?,
        _ => return Err(Error::Contract("unexpected GPS event")),
    };
    Ok(Gps {
        updated: topic.updated,
        log_mono_time: event.get_log_mono_time(),
        has_fix: gps.get_has_fix(),
        longitude: gps.get_longitude(),
        unix_timestamp_millis: gps.get_unix_timestamp_millis(),
    })
}
pub fn clocks(wall: u64, monotonic: u64, valid: bool) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder>();
    event.set_log_mono_time(monotonic);
    event.set_valid(valid);
    event.init_clocks().set_wall_time_nanos(wall);
    capnp::serialize::write_message_to_words(&message)
}
