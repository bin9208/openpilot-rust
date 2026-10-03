use crate::{analysis::Pressure, Error, SAMPLE_RATE};
use openpilot_cereal::log_capnp::event;
use rustfft::num_traits::ToPrimitive;

fn message() -> Result<capnp::message::Builder<capnp::message::HeapAllocator>, Error> {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    let time = u64::try_from(now.tv_sec)
        .ok()
        .and_then(|seconds| seconds.checked_mul(1_000_000_000))
        .and_then(|seconds| {
            u64::try_from(now.tv_nsec)
                .ok()
                .and_then(|nanos| seconds.checked_add(nanos))
        })
        .ok_or(Error::Contract("monotonic timestamp outside u64"))?;
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_log_mono_time(time);
    event.set_valid(true);
    Ok(message)
}
pub fn pressure(value: Pressure) -> Result<Vec<u8>, Error> {
    let mut message = message()?;
    let mut pressure = message
        .get_root::<event::Builder<'_>>()
        .map_err(|_| Error::Contract("new event"))?
        .init_sound_pressure();
    fn float32(value: f64) -> f32 {
        value.to_f32().unwrap_or(if value.is_sign_negative() {
            f32::NEG_INFINITY
        } else {
            f32::INFINITY
        })
    }
    pressure.set_sound_pressure(float32(value.unweighted));
    pressure.set_sound_pressure_weighted(float32(value.weighted));
    pressure.set_sound_pressure_weighted_db(float32(value.weighted_db));
    Ok(capnp::serialize::write_message_to_words(&message))
}
pub fn raw(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let mut message = message()?;
    let mut raw = message
        .get_root::<event::Builder<'_>>()
        .map_err(|_| Error::Contract("new event"))?
        .init_raw_audio_data();
    raw.set_data(bytes);
    raw.set_sample_rate(SAMPLE_RATE);
    Ok(capnp::serialize::write_message_to_words(&message))
}
