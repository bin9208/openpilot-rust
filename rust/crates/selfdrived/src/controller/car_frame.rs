use super::Error;
use capnp::{
    message::{Builder, Reader, ReaderOptions},
    serialize::{self, OwnedSegments},
};
use openpilot_cereal::{car_capnp::car_state, log_capnp::event};
use std::io::Cursor;

pub struct CarFrame {
    pub bytes: Vec<u8>,
    message: Reader<OwnedSegments>,
}
impl CarFrame {
    pub fn initial() -> Result<Self, Error> {
        let mut message = Builder::new_default();
        message.init_root::<car_state::Builder<'_>>();
        Self::read(serialize::write_message_to_words(&message))
    }
    pub fn read(bytes: Vec<u8>) -> Result<Self, Error> {
        let message = serialize::read_message(
            Cursor::new(&bytes),
            ReaderOptions {
                traversal_limit_in_words: None,
                nesting_limit: 64,
            },
        )?;
        message.get_root::<car_state::Reader<'_>>()?;
        Ok(Self { bytes, message })
    }
    pub fn from_event(bytes: &[u8]) -> Result<Self, Error> {
        let event = serialize::read_message(Cursor::new(bytes), ReaderOptions::new())?;
        let event::Which::CarState(cs) = event.get_root::<event::Reader<'_>>()?.which()? else {
            return Err(Error::Contract("carState event union"));
        };
        let cs = cs?;
        let mut message = Builder::new_default();
        message.set_root(cs)?;
        Self::read(serialize::write_message_to_words(&message))
    }
    pub fn state(&self) -> Result<car_state::Reader<'_>, Error> {
        Ok(self.message.get_root()?)
    }
}
