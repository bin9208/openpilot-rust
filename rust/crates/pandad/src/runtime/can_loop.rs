use super::{platform, Error, Shared};
use crate::can_io::{send_is_current, CanIo, Outgoing};
use openpilot_cereal::log_capnp::event;
use openpilot_logging::record::Level;
use openpilot_messaging::{runtime::PubMaster, services};
use openpilot_msgq::Subscriber;
use std::time::Duration;

pub fn send(shared: &Shared) -> Result<(), Error> {
    platform::thread_name(c"pandad_can_send")?;
    if shared.hardware.board && platform::realtime(55).is_err() {
        shared.logs.write(
            Level::Error,
            "failed to raise Panda CAN send thread priority: -1",
        );
    }
    let capacity = services::lookup("sendcan")
        .ok_or(Error::Contract("missing sendcan service"))?
        .queue_size;
    let mut subscriber = Subscriber::for_runtime("sendcan", false, capacity)?;
    let senders = shared
        .pandas
        .iter()
        .map(|panda| CanIo::new(panda.bus_offset(), false))
        .collect::<Vec<_>>();
    while shared.connected() {
        let Some(bytes) = subscriber.receive(Duration::from_millis(100))? else {
            continue;
        };
        let message = capnp::serialize::read_message_from_flat_slice(
            &mut bytes.as_slice(),
            capnp::message::ReaderOptions::new(),
        )?;
        let event = message.get_root::<event::Reader<'_>>()?;
        let received_ns = platform::now_ns();
        if send_is_current(received_ns, event.get_log_mono_time(), shared.fake_send) {
            let event::Which::Sendcan(frames) = event.which().map_err(capnp::Error::from)? else {
                return Err(Error::Contract("sendcan queue carried another service"));
            };
            let frames = frames?;
            let mut outgoing = Vec::with_capacity(frames.len() as usize);
            for frame in frames.iter() {
                outgoing.push(Outgoing {
                    address: frame.get_address(),
                    src: frame.get_src(),
                    data: frame.get_dat()?,
                });
            }
            for (panda, sender) in shared.pandas.iter().zip(&senders) {
                shared.logs.trace(|| {
                    format!(
                        "sending sendcan to panda: {}",
                        String::from_utf8_lossy(panda.transport().serial())
                    )
                });
                sender.send(panda.transport(), &outgoing)?;
                shared.logs.trace(|| {
                    format!(
                        "sendcan sent to panda: {}",
                        String::from_utf8_lossy(panda.transport().serial())
                    )
                });
            }
        } else {
            shared.logs.write(
                Level::Error,
                format!(
                    "sendcan too old to send: {}, {}",
                    platform::now_ns(),
                    event.get_log_mono_time()
                ),
            );
        }
    }
    Ok(())
}

pub fn receive(shared: &Shared) -> Result<(), Error> {
    platform::thread_name(c"pandad_can_recv")?;
    if shared.hardware.board && platform::realtime(56).is_err() {
        shared.logs.write(
            Level::Error,
            "failed to raise Panda CAN receive thread priority: -1",
        );
    }
    let mut rate = platform::RateKeeper::new();
    let mut publisher = PubMaster::for_runtime(&["can"])?;
    let mut receivers = shared
        .pandas
        .iter()
        .map(|panda| CanIo::new(panda.bus_offset(), shared.maxout))
        .collect::<Vec<_>>();
    let mut frames = Vec::new();
    while shared.connected() {
        frames.clear();
        let mut healthy = true;
        for (panda, receiver) in shared.pandas.iter().zip(&mut receivers) {
            healthy &= receiver.receive(panda.transport(), &mut frames, || {
                shared.logs.write(Level::Error, "Panda CAN checksum failed")
            })?;
        }
        let mut message = capnp::message::Builder::new_default();
        let mut event = message.init_root::<event::Builder<'_>>();
        event.set_log_mono_time(platform::now_ns());
        event.set_valid(healthy);
        let count = u32::try_from(frames.len())
            .map_err(|_| Error::Contract("too many received CAN frames"))?;
        let mut output = event.init_can(count);
        for (index, frame) in (0..count).zip(&frames) {
            let mut row = output.reborrow().get(index);
            row.set_address(frame.address);
            row.set_src(frame.src as u8);
            row.set_dat(&frame.data);
        }
        publisher.send("can", &capnp::serialize::write_message_to_words(&message))?;
        rate.keep_time()?;
    }
    Ok(())
}
