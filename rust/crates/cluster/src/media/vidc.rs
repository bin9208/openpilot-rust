mod capture;
mod decode;

use openpilot_encoderd::{
    native::v4l::primitives::{Allocator, Buffer, Device, FormatKind, Queue},
    Error,
};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

pub const DEFAULT_DEVICE: &str = "/dev/v4l/by-path/platform-aa00000.qcom_vidc-video-index0";
static GENERATION: AtomicU64 = AtomicU64::new(1);
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug)]
pub struct Config {
    pub width: i32,
    pub height: i32,
    pub fps: u32,
    pub device_path: String,
    pub timeout: Duration,
    pub debug: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            fps: 30,
            device_path: DEFAULT_DEVICE.into(),
            timeout: Duration::from_millis(250),
            debug: false,
        }
    }
}
struct Input {
    buffer: Buffer,
    queued: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum CaptureState {
    Unallocated,
    Queued,
    Leased,
}
struct Capture {
    buffer: Buffer,
    state: CaptureState,
}
#[derive(Clone, Copy)]
struct ReadyFrame {
    index: usize,
    sequence: u64,
}
#[derive(Default)]
struct Streams {
    output: bool,
    capture: bool,
}

pub struct Decoder {
    config: Config,
    device: Option<Device>,
    allocator: Option<Allocator>,
    inputs: Vec<Input>,
    captures: Vec<Capture>,
    streams: Streams,
    opened: bool,
    pending: bool,
    coded: (u32, u32),
    layout: openpilot_startup_ui::egl::FrameLayout,
    capture_size: usize,
    submitted: VecDeque<u64>,
    ready: VecDeque<ReadyFrame>,
    generation: u64,
    release: mpsc::Sender<u32>,
    released: mpsc::Receiver<u32>,
    failures: mpsc::Sender<String>,
    failed: mpsc::Receiver<String>,
    disabled_reason: String,
}
impl Decoder {
    /// # Errors
    /// Rejects source-invalid dimensions, FPS, or an empty kernel device path.
    pub fn new(config: Config) -> Result<Self> {
        if config.width <= 0
            || config.height <= 0
            || config.width % 2 != 0
            || config.height % 2 != 0
        {
            return Err(Error::Contract(
                "cluster H264 decoder requires positive even dimensions",
            ));
        }
        if config.fps == 0 {
            return Err(Error::Contract("cluster H264 decoder fps must be positive"));
        }
        if config.device_path.is_empty() {
            return Err(Error::Contract(
                "cluster H264 decoder device path must not be empty",
            ));
        }
        let (release, released) = mpsc::channel();
        let (failures, failed) = mpsc::channel();
        let coded = (u32::try_from(config.width)?, u32::try_from(config.height)?);
        Ok(Self {
            config,
            device: None,
            allocator: None,
            inputs: Vec::new(),
            captures: Vec::new(),
            streams: Streams::default(),
            opened: false,
            pending: false,
            coded,
            layout: openpilot_startup_ui::egl::FrameLayout {
                width: 0,
                height: 0,
                stride: 0,
                uv_offset: 0,
            },
            capture_size: 0,
            submitted: VecDeque::new(),
            ready: VecDeque::new(),
            generation: GENERATION.fetch_add(1, Ordering::Relaxed),
            release,
            released,
            failures,
            failed,
            disabled_reason: String::new(),
        })
    }
    fn device(&self) -> Result<&Device> {
        self.device
            .as_ref()
            .ok_or(Error::Contract("cluster H264 decoder is not open"))
    }
    /// # Errors
    /// Preserves kernel/ION setup errors and closes partially initialized queues.
    pub fn open(&mut self) -> Result<()> {
        if self.opened {
            return Ok(());
        }
        self.device = Some(Device::open(&self.config.device_path)?);
        if let Err(error) = self.initialize() {
            self.close();
            return Err(error);
        }
        self.opened = true;
        Ok(())
    }
    fn initialize(&mut self) -> Result<()> {
        let capability = self.device()?.capability()?;
        if capability.driver != "msm_vidc_driver" || capability.card != "msm_vidc_vdec" {
            eprintln!(
                "cluster H264 decoder is {}/{}, expected msm_vidc_driver/msm_vidc_vdec",
                capability.driver, capability.card
            );
        }
        self.device()?.subscribe_decoder()?;
        let format = self
            .device()?
            .format(Queue::Output, self.coded, FormatKind::H264, false)?;
        if format.pixel_format != u32::from_le_bytes(*b"H264") {
            return Err(Error::Contract("V4L2 decoder rejected H264 input format"));
        }
        if format.size == 0 {
            return Err(Error::Contract(
                "V4L2 decoder returned zero H264 input sizeimage",
            ));
        }
        self.device()?.fps(Queue::Output, self.config.fps)?;
        self.device()?.decoder_dpb()?;
        let count = self.request(Queue::Output, 8)?;
        self.allocator = Some(Allocator::open()?);
        let allocator = self
            .allocator
            .as_ref()
            .ok_or(Error::Contract("decoder allocator missing"))?;
        for _ in 0..count {
            self.inputs.push(Input {
                buffer: allocator.allocate(usize::try_from(format.size)?)?,
                queued: false,
            });
        }
        self.device()?.stream(Queue::Output, true)?;
        self.streams.output = true;
        self.configure_capture()?;
        self.allocate_capture()?;
        Ok(())
    }
    fn request(&self, queue: Queue, count: u32) -> Result<u32> {
        let actual = self.device()?.buffers(queue, count)?;
        if count > 0 && actual == 0 {
            return Err(Error::Contract("V4L2 decoder allocated zero buffers"));
        }
        Ok(count.min(actual))
    }
    pub fn close(&mut self) {
        if let Some(device) = &self.device {
            if self.streams.capture {
                if let Err(error) = device.stream(Queue::Capture, false) {
                    eprintln!("cluster decoder capture cleanup: {error}");
                }
            }
            if self.streams.output {
                if let Err(error) = device.stream(Queue::Output, false) {
                    eprintln!("cluster decoder input cleanup: {error}");
                }
            }
            for queue in [Queue::Capture, Queue::Output] {
                if let Err(error) = device.buffers(queue, 0) {
                    eprintln!("cluster decoder queue cleanup: {error}");
                }
            }
        }
        self.streams = Streams::default();
        self.device = None;
        self.captures.clear();
        self.inputs.clear();
        self.allocator = None;
        self.submitted.clear();
        self.ready.clear();
        self.pending = false;
        self.opened = false;
        while self.released.try_recv().is_ok() {}
    }
}
impl Drop for Decoder {
    fn drop(&mut self) {
        self.close();
    }
}
