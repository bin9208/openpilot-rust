use crate::{vision_bridge::ffi, Error, VisionMetadata};
use std::{marker::PhantomData, rc::Rc, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionStream {
    Road,
    Driver,
    WideRoad,
    Map,
}

impl VisionStream {
    fn native(self) -> i32 {
        match self {
            Self::Road => 0,
            Self::Driver => 1,
            Self::WideRoad => 2,
            Self::Map => 3,
        }
    }
}

/// Client for the original trusted local VisionIPC server protocol.
/// Native assertions and driver failures retain the original fail-stop behavior.
pub struct VisionClient {
    connection: cxx::UniquePtr<ffi::VisionConnection>,
    thread: PhantomData<Rc<()>>,
}

impl VisionClient {
    pub fn new(name: &str, stream: VisionStream, conflate: bool) -> Result<Self, Error> {
        Ok(Self {
            connection: ffi::open_vision(name, stream.native(), conflate)?,
            thread: PhantomData,
        })
    }

    /// Attempts discovery once; an absent server returns an empty list.
    pub fn available_streams(name: &str) -> Result<Vec<VisionStream>, Error> {
        let mask = ffi::vision_streams(name)?;
        Ok([
            VisionStream::Road,
            VisionStream::Driver,
            VisionStream::WideRoad,
            VisionStream::Map,
        ]
        .into_iter()
        .filter(|stream| mask & (1 << stream.native()) != 0)
        .collect())
    }

    /// Attempts connection once, without the native infinite retry loop.
    pub fn connect(&mut self) -> Result<bool, Error> {
        Ok(self.connection.pin_mut().connect()?)
    }

    pub fn is_connected(&self) -> bool {
        self.connection.connected()
    }

    /// A timeout or a changed server returns no frame; check `is_connected`
    /// to distinguish a reconnect requirement from an ordinary timeout.
    pub fn receive(&mut self, timeout: Duration) -> Result<Option<VisionFrame<'_>>, Error> {
        let milliseconds = i32::try_from(timeout.as_millis()).map_err(|_| Error::TimeoutRange)?;
        let metadata = self.connection.pin_mut().receive(milliseconds)?;
        Ok(metadata.received.then_some(VisionFrame {
            client: self,
            metadata,
        }))
    }
}

/// Keeps the imported mapping alive and excludes another receive/reconnect.
/// The producer may still recycle the buffer: this is not a producer lease.
pub struct VisionFrame<'a> {
    client: &'a mut VisionClient,
    metadata: VisionMetadata,
}

impl VisionFrame<'_> {
    pub fn metadata(&self) -> &VisionMetadata {
        &self.metadata
    }

    /// Copies the original mapped bytes into exactly `metadata().len` bytes.
    /// Like the source API, copying does not make producer updates atomic.
    pub fn copy_into(&self, destination: &mut [u8]) -> Result<(), Error> {
        Ok(self.client.connection.copy_frame(destination)?)
    }
}
