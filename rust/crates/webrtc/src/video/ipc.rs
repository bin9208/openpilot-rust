use super::FrameClock;
use crate::{cereal, Error};
use capnp::dynamic_value::Reader;
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::services;
use openpilot_msgq::Subscriber;
use serde::Serialize;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Camera {
    Road,
    WideRoad,
    Driver,
}

impl Camera {
    /// Parses one of the three original camera names.
    ///
    /// # Errors
    /// Rejects unknown camera names.
    pub fn parse(name: &str) -> Result<Self, Error> {
        match name {
            "road" => Ok(Self::Road),
            "wideRoad" => Ok(Self::WideRoad),
            "driver" => Ok(Self::Driver),
            _ => Err(Error::Contract("invalid camera")),
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Road => "road",
            Self::WideRoad => "wideRoad",
            Self::Driver => "driver",
        }
    }

    #[must_use]
    pub const fn endpoint(self) -> &'static str {
        match self {
            Self::Road => "livestreamRoadEncodeData",
            Self::WideRoad => "livestreamWideRoadEncodeData",
            Self::Driver => "livestreamDriverEncodeData",
        }
    }
}

#[derive(Serialize)]
pub struct EncodedFrame {
    pub frame_id: u32,
    pub pts: i64,
    pub data: Vec<u8>,
}

pub struct CameraTrack {
    socket: Subscriber,
    clock: FrameClock,
}

fn content(reader: Reader<'_>) -> Result<capnp::dynamic_struct::Reader<'_>, Error> {
    match reader {
        Reader::Struct(value) => Ok(value),
        _ => Err(Error::Contract("encoded frame must be a struct")),
    }
}

fn data(reader: Reader<'_>) -> Result<&[u8], Error> {
    match reader {
        Reader::Data(value) => Ok(value),
        _ => Err(Error::Contract("encoded bitstream must be bytes")),
    }
}

impl CameraTrack {
    /// Opens the original conflated encoded-camera runtime queue.
    ///
    /// # Errors
    /// Returns queue or service configuration errors.
    pub fn for_runtime(camera: Camera, carrot_vision: bool) -> Result<Self, Error> {
        let service = services::lookup(camera.endpoint())
            .ok_or_else(|| Error::Service(camera.endpoint().to_owned()))?;
        Ok(Self {
            socket: Subscriber::for_runtime(service.name, true, service.queue_size)?,
            clock: FrameClock::new(camera == Camera::Road && carrot_vision),
        })
    }

    /// Reads one available encoded frame without blocking the peer driver.
    ///
    /// # Errors
    /// Returns IPC errors or malformed encoded-frame errors.
    pub fn receive(&mut self) -> Result<Option<EncodedFrame>, Error> {
        let Some(bytes) = self.socket.receive(Duration::ZERO)? else {
            return Ok(None);
        };
        let message = cereal::read(&bytes)?;
        let root = content(message.get_root::<event::Reader<'_>>()?.into())?;
        let field = root
            .which()?
            .ok_or(Error::Contract("missing frame service"))?;
        let value = content(root.get(field)?)?;
        let idx = content(value.get_named("idx")?)?;
        let Reader::UInt32(frame_id) = idx.get_named("frameId")? else {
            return Err(Error::Contract("invalid source frame ID"));
        };
        let mut bitstream = data(value.get_named("header")?)?.to_vec();
        bitstream.extend_from_slice(data(value.get_named("data")?)?);
        Ok(Some(EncodedFrame {
            frame_id,
            pts: self.clock.next(frame_id)?,
            data: bitstream,
        }))
    }
}
