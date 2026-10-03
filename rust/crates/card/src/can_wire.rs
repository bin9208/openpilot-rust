use capnp::message::{Builder, ReaderOptions};
use openpilot_can::{Frame, Packet};
use openpilot_cereal::log_capnp::event;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Schema(#[from] capnp::Error),
    #[error(transparent)]
    Enum(#[from] capnp::NotInSchema),
    #[error(transparent)]
    Count(#[from] std::num::TryFromIntError),
    #[error("CAN subscription received a different cereal event")]
    Event,
}

pub fn decode(bytes: &[u8]) -> Result<Packet, Error> {
    let message =
        capnp::serialize::read_message(std::io::Cursor::new(bytes), ReaderOptions::new())?;
    let root = message.get_root::<event::Reader>()?;
    let event::Which::Can(frames) = root.which()? else {
        return Err(Error::Event);
    };
    let frames = frames?
        .iter()
        .map(|frame| {
            Ok(Frame {
                address: frame.get_address(),
                data: frame.get_dat()?.to_vec(),
                bus: frame.get_src(),
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(Packet {
        mono_time: root.get_log_mono_time(),
        frames,
    })
}

pub fn sendcan(frames: &[Frame], valid: bool, mono_time: u64) -> Result<Vec<u8>, Error> {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<event::Builder>();
    root.set_valid(valid);
    root.set_log_mono_time(mono_time);
    let mut can = root.init_sendcan(u32::try_from(frames.len())?);
    for (index, source) in frames.iter().enumerate() {
        let mut frame = can.reborrow().get(u32::try_from(index)?);
        frame.set_address(source.address);
        frame.set_dat(&source.data);
        frame.set_src(source.bus);
    }
    Ok(capnp::serialize::write_message_to_words(&message))
}
