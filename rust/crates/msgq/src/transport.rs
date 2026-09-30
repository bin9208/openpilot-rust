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
    pub fn for_runtime(endpoint: &str, capacity: usize) -> Result<Self, Error> {
        Ok(Self {
            queue: ffi::open_runtime_queue(endpoint, true, false, capacity)?,
            thread: PhantomData,
        })
    }

    pub fn new(endpoint: &str) -> Result<Self, Error> {
        Self::with_capacity(endpoint, 1024 * 1024)
    }

    pub fn with_capacity(endpoint: &str, capacity: usize) -> Result<Self, Error> {
        Ok(Self {
            queue: ffi::open_queue(endpoint, true, false, capacity)?,
            thread: PhantomData,
        })
    }

    pub fn send(&mut self, bytes: &[u8]) -> Result<(), Error> {
        Ok(self.queue.pin_mut().send(bytes)?)
    }

    pub fn readers_caught_up(&mut self) -> bool {
        self.queue.pin_mut().readers_caught_up()
    }
}

pub struct Subscriber {
    queue: cxx::UniquePtr<ffi::Queue>,
    thread: PhantomData<Rc<()>>,
}

impl Subscriber {
    pub fn for_runtime(endpoint: &str, conflate: bool, capacity: usize) -> Result<Self, Error> {
        Ok(Self {
            queue: ffi::open_runtime_queue(endpoint, false, conflate, capacity)?,
            thread: PhantomData,
        })
    }

    pub fn new(endpoint: &str, conflate: bool) -> Result<Self, Error> {
        Self::with_capacity(endpoint, conflate, 1024 * 1024)
    }

    pub fn with_capacity(endpoint: &str, conflate: bool, capacity: usize) -> Result<Self, Error> {
        Ok(Self {
            queue: ffi::open_queue(endpoint, false, conflate, capacity)?,
            thread: PhantomData,
        })
    }

    pub fn receive(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, Error> {
        let milliseconds = i32::try_from(timeout.as_millis()).map_err(|_| Error::TimeoutRange)?;
        let bytes = self.queue.pin_mut().receive(milliseconds)?;
        Ok((!bytes.is_empty()).then_some(bytes))
    }
}
