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
    /// Temporary UI feeds yield ownership to a later daemon publisher. They must
    /// use `send_if_current` and stop on false rather than reclaim the endpoint.
    pub fn transient_for_runtime(endpoint: &str, capacity: usize) -> Result<Self, Error> {
        Ok(Self {
            queue: ffi::open_transient_runtime_publisher(endpoint, capacity)?,
            thread: PhantomData,
        })
    }

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

    pub fn send_if_current(&mut self, bytes: &[u8]) -> Result<bool, Error> {
        Ok(self.queue.pin_mut().send_if_current(bytes)?)
    }

    pub fn readers_caught_up(&mut self) -> bool {
        self.queue.pin_mut().readers_caught_up()
    }
}

pub struct Subscriber {
    queue: cxx::UniquePtr<ffi::Queue>,
    thread: PhantomData<Rc<()>>,
}

pub struct Subscription<'a> {
    pub endpoint: &'a str,
    pub capacity: usize,
    pub polled: bool,
}

pub use ffi::QueuedMessage;

pub struct MultiSubscriber {
    queues: cxx::UniquePtr<ffi::QueueBatch>,
    thread: PhantomData<Rc<()>>,
}

impl MultiSubscriber {
    /// Open each non-conflated queue only while its bridge has connected peers.
    pub fn lazy_for_runtime(specifications: &[Subscription<'_>]) -> Result<Self, Error> {
        let specifications: Vec<_> = specifications
            .iter()
            .map(|specification| ffi::QueueSpec {
                endpoint: specification.endpoint.to_owned(),
                capacity: specification.capacity,
                polled: specification.polled,
            })
            .collect();
        Ok(Self {
            queues: ffi::open_lazy_batch(&specifications)?,
            thread: PhantomData,
        })
    }

    pub fn set_active(&mut self, index: usize, active: bool) -> Result<(), Error> {
        Ok(self.queues.pin_mut().set_active(index, active)?)
    }

    pub fn new(specifications: &[Subscription<'_>]) -> Result<Self, Error> {
        Self::open(specifications, true)
    }

    pub fn for_runtime(specifications: &[Subscription<'_>]) -> Result<Self, Error> {
        Self::open(specifications, false)
    }

    /// Retain every queued packet for consumers such as route logging.
    pub fn queued_for_runtime(specifications: &[Subscription<'_>]) -> Result<Self, Error> {
        let specifications: Vec<_> = specifications
            .iter()
            .map(|specification| ffi::QueueSpec {
                endpoint: specification.endpoint.to_owned(),
                capacity: specification.capacity,
                polled: specification.polled,
            })
            .collect();
        Ok(Self {
            queues: ffi::open_queued_batch(&specifications)?,
            thread: PhantomData,
        })
    }

    /// Poll without consuming packets; callers control drain limits and stop checks.
    pub fn poll_ready(&mut self, timeout: Duration) -> Result<Vec<usize>, Error> {
        let milliseconds = i32::try_from(timeout.as_millis()).map_err(|_| Error::TimeoutRange)?;
        Ok(self.queues.pin_mut().poll_ready(milliseconds)?)
    }

    pub fn receive_one(&mut self, index: usize) -> Result<Option<Vec<u8>>, Error> {
        let bytes = self.queues.pin_mut().receive_one(index)?;
        Ok((!bytes.is_empty()).then_some(bytes))
    }

    fn open(specifications: &[Subscription<'_>], isolated: bool) -> Result<Self, Error> {
        let specifications: Vec<_> = specifications
            .iter()
            .map(|specification| ffi::QueueSpec {
                endpoint: specification.endpoint.to_owned(),
                capacity: specification.capacity,
                polled: specification.polled,
            })
            .collect();
        Ok(Self {
            queues: ffi::open_batch(&specifications, isolated)?,
            thread: PhantomData,
        })
    }

    pub fn receive(&mut self, timeout: Duration) -> Result<Vec<QueuedMessage>, Error> {
        let milliseconds = i32::try_from(timeout.as_millis()).map_err(|_| Error::TimeoutRange)?;
        Ok(self.queues.pin_mut().receive(milliseconds)?)
    }
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
