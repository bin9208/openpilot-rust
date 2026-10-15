//! Original road/wide stream selection and poll cadence, using native `VisionIPC`.
mod connection;
mod selection;
use super::Error;
use openpilot_msgq::{VisionClient, VisionFrame, VisionMetadata, VisionStream};

const CONNECTION_RETRY_SECONDS: f64 = 0.5;
const FRAME_POLL_SECONDS: f64 = 0.1;
const FRAME_STALE_SECONDS: f64 = 1.2;
const STREAM_DISCOVERY_SECONDS: f64 = 1.0;

pub struct Frame {
    pub metadata: VisionMetadata,
    pub bytes: Vec<u8>,
}
struct Target {
    stream: VisionStream,
    client: VisionClient,
}
pub struct Input {
    name: String,
    stream: VisionStream,
    client: Option<VisionClient>,
    target: Option<Target>,
    available: Vec<VisionStream>,
    last_discovery: f64,
    last_attempt: f64,
    connected_at: f64,
    last_poll: f64,
    last_frame_at: f64,
    waiting_logged: bool,
    frame: Option<Frame>,
    revision: u64,
    generation: u64,
    copy: bool,
}
impl Input {
    /// # Errors
    /// Returns a native `VisionIPC` queue error.
    pub fn new() -> Result<Self, Error> {
        Self::for_server("camerad")
    }
    /// # Errors
    /// Returns a native `VisionIPC` queue error for this owned server.
    pub fn for_server(name: &str) -> Result<Self, Error> {
        Ok(Self {
            name: name.to_owned(),
            stream: VisionStream::Road,
            client: Some(VisionClient::new(name, VisionStream::Road, true)?),
            target: None,
            available: Vec::new(),
            last_discovery: 0.0,
            last_attempt: 0.0,
            connected_at: 0.0,
            last_poll: 0.0,
            last_frame_at: 0.0,
            waiting_logged: false,
            frame: None,
            revision: 0,
            generation: 0,
            copy: true,
        })
    }
    #[must_use]
    pub const fn is_wide(&self) -> bool {
        matches!(self.stream, VisionStream::WideRoad)
    }
    #[must_use]
    pub const fn stream(&self) -> VisionStream {
        self.stream
    }
    #[must_use]
    pub fn available_streams(&self) -> &[VisionStream] {
        &self.available
    }
    #[must_use]
    pub const fn frame(&self) -> Option<&Frame> {
        self.frame.as_ref()
    }
    /// Source display identity is valid only while the retained slot still holds this image.
    #[must_use]
    pub fn display_frame(&self) -> Option<(u32, f64, usize, usize, bool)> {
        let frame = self.frame.as_ref()?;
        let retained = self.retained_buffer().ok()?;
        if retained.frame_id().ok()? != u64::from(frame.metadata.frame_id) {
            return None;
        }
        #[allow(clippy::cast_precision_loss)]
        let eof = frame.metadata.timestamp_eof as f64 / 1e9;
        Some((
            frame.metadata.frame_id,
            eof,
            frame.metadata.width,
            frame.metadata.height,
            self.is_wide(),
        ))
    }
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    pub(super) fn external(&mut self) {
        self.copy = false;
    }
    pub(super) fn retained_buffer(&self) -> Result<openpilot_msgq::RetainedBuffer<'_>, Error> {
        let index = self
            .frame
            .as_ref()
            .ok_or(Error::Contract("camera frame missing"))?
            .metadata
            .index;
        Ok(self
            .client
            .as_ref()
            .ok_or(Error::Contract("camera client closed"))?
            .retained_buffer(index)?)
    }
    pub(super) fn enable_copy(&mut self) -> Result<(), Error> {
        self.copy = true;
        if let Some(frame) = &mut self.frame {
            frame.bytes.resize(frame.metadata.len, 0);
            self.client
                .as_ref()
                .ok_or(Error::Contract("camera client closed"))?
                .retained_buffer(frame.metadata.index)?
                .copy_into(&mut frame.bytes)?;
            self.revision = self.revision.wrapping_add(1);
        }
        Ok(())
    }
    pub fn close(&mut self) {
        self.frame = None;
        self.connected_at = 0.0;
        self.last_frame_at = 0.0;
        self.client = None;
        self.target = None;
        self.available.clear();
        self.generation = self.generation.wrapping_add(1);
    }
}

fn copy_frame(
    frame: &VisionFrame<'_>,
    previous: Option<Frame>,
    copy: bool,
) -> Result<Frame, Error> {
    let metadata = *frame.metadata();
    let mut bytes = previous.map_or_else(Vec::new, |previous| previous.bytes);
    if copy {
        bytes.resize(metadata.len, 0);
        frame.copy_into(&mut bytes)?;
    }
    Ok(Frame { metadata, bytes })
}
