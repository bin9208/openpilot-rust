use crate::isotp::{CanIo, Error};
use crate::query::QueryIo;
use openpilot_can::Frame;
use std::collections::BTreeMap;

pub(super) struct BufferedIo<'a, I> {
    pub io: &'a mut I,
    pub buffers: &'a mut BTreeMap<i64, Vec<Frame>>,
    pub address: i64,
    pub subaddress: Option<u8>,
}

impl<I: QueryIo> CanIo for BufferedIo<'_, I> {
    fn receive(&mut self) -> Result<Vec<Frame>, Error> {
        let frames = self.buffers.entry(self.address).or_default();
        match self.subaddress {
            None => Ok(std::mem::take(frames)),
            Some(subaddress) => {
                // Source rejects empty subaddressed frames before changing the buffer.
                if frames.iter().any(|frame| frame.data.is_empty()) {
                    return Err(Error::Truncated);
                }
                let (selected, retained) = std::mem::take(frames)
                    .into_iter()
                    .partition(|frame| frame.data[0] == subaddress);
                *frames = retained;
                Ok(selected)
            }
        }
    }

    fn send(&mut self, frame: Frame) -> Result<(), Error> {
        self.io.send(std::slice::from_ref(&frame))
    }

    fn sleep(&mut self, seconds: f64) -> Result<(), Error> {
        self.io.sleep(seconds)
    }

    fn now(&mut self) -> f64 {
        self.io.now()
    }
}
