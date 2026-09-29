use crate::bridge::ffi;
use std::{marker::PhantomData, rc::Rc, time::Duration};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("native msgq: {0}")]
    Native(#[from] cxx::Exception),
    #[error("timeout exceeds the native millisecond range")]
    TimeoutRange,
}

pub struct Publisher {
    queue: cxx::UniquePtr<ffi::Queue>,
    thread: PhantomData<Rc<()>>,
}

impl Publisher {
    pub fn new(endpoint: &str) -> Result<Self, Error> {
        Ok(Self {
            queue: ffi::open_queue(endpoint, true, false)?,
            thread: PhantomData,
        })
    }

    pub fn send(&mut self, bytes: &[u8]) -> Result<(), Error> {
        Ok(self.queue.pin_mut().send(bytes)?)
    }
}

pub struct Subscriber {
    queue: cxx::UniquePtr<ffi::Queue>,
    thread: PhantomData<Rc<()>>,
}

impl Subscriber {
    pub fn new(endpoint: &str, conflate: bool) -> Result<Self, Error> {
        Ok(Self {
            queue: ffi::open_queue(endpoint, false, conflate)?,
            thread: PhantomData,
        })
    }

    pub fn receive(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, Error> {
        let milliseconds = i32::try_from(timeout.as_millis()).map_err(|_| Error::TimeoutRange)?;
        let bytes = self.queue.pin_mut().receive(milliseconds)?;
        Ok((!bytes.is_empty()).then_some(bytes))
    }
}
