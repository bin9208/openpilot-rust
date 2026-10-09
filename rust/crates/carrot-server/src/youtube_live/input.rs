use super::profiles;
use crate::Error;
use capnp::dynamic_value::Reader;
use openpilot_carrot_state::value::{data, field, integer};
use openpilot_msgq::Subscriber;
use std::time::Duration;

#[derive(Default)]
pub(super) struct Frame {
    pub header: Vec<u8>,
    pub data: Vec<u8>,
    pub id: Option<i128>,
    pub keyframe: bool,
    pub width: u32,
    pub height: u32,
}
#[derive(Default)]
pub(super) struct Input {
    socket: Option<Subscriber>,
    quality: u8,
}
impl Input {
    pub fn receive(&mut self, quality: u8) -> Result<Option<Frame>, Error> {
        if self.socket.is_none() || self.quality != quality {
            self.socket = None;
            let capacity = openpilot_messaging::services::lookup(profiles::SOURCE)
                .map_or(1024 * 1024, |service| service.queue_size);
            self.socket = Some(
                Subscriber::for_runtime(profiles::SOURCE, false, capacity).map_err(|error| {
                    Error::Source(format!("{} socket failed: {error}", profiles::SOURCE))
                })?,
            );
            self.quality = quality;
        }
        let Some(socket) = self.socket.as_mut() else {
            return Ok(None);
        };
        socket
            .receive(Duration::ZERO)
            .map_err(|error| Error::Source(error.to_string()))?
            .map(|bytes| parse(&bytes))
            .transpose()
    }
}
fn parse(bytes: &[u8]) -> Result<Frame, Error> {
    let message = openpilot_carrot_state::value::message(bytes)
        .map_err(|error| Error::Source(error.to_string()))?;
    let root: openpilot_cereal::log_capnp::event::Reader<'_> = message
        .get_root()
        .map_err(|error| Error::Source(error.to_string()))?;
    let Reader::Struct(root) = root.into() else {
        return Ok(Frame::default());
    };
    let Some(selected) = root
        .which()
        .map_err(|error| Error::Source(error.to_string()))?
    else {
        return Ok(Frame::default());
    };
    let frame = root
        .get(selected)
        .map_err(|error| Error::Source(error.to_string()))?;
    let header = data(field(frame, "header"));
    let idx = field(frame, "idx");
    let id = integer(field(idx, "frameId"));
    Ok(Frame {
        keyframe: !header.is_empty() || integer(field(idx, "flags")) & 8 != 0,
        header,
        data: data(field(frame, "data")),
        id: (id != 0).then_some(id),
        width: u32::try_from(integer(field(frame, "width")))
            .ok()
            .filter(|n| *n != 0)
            .unwrap_or(526),
        height: u32::try_from(integer(field(frame, "height")))
            .ok()
            .filter(|n| *n != 0)
            .unwrap_or(330),
    })
}
