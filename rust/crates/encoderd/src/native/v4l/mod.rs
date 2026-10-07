#![allow(unsafe_code)]
mod abi;
mod io;
mod setup;

use super::{platform, publisher::VideoPublisher, Mapping};
use crate::{config::Settings, profile::EncoderInfo, Error};
use openpilot_logging::{log_site, record::Level};
use openpilot_msgq::VisionMetadata;
use std::{
    collections::VecDeque,
    os::fd::{AsRawFd, OwnedFd},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, MutexGuard,
    },
    thread::JoinHandle,
};

const INPUT_COUNT: usize = 7;
const CAPTURE_COUNT: usize = 6;
const CODEC_CONFIG: u32 = 0x0002_0000;
const EOS: u32 = 0x0200_0000;

fn warn(text: String) {
    platform::emit(log_site!(), Level::Warning, text);
}
fn failure(text: String) {
    platform::emit(log_site!(), Level::Error, text);
}

struct Queues {
    free: VecDeque<usize>,
    inputs: [Option<Arc<Mapping>>; INPUT_COUNT],
    extras: VecDeque<VisionMetadata>,
}
struct Shared {
    queues: Mutex<Queues>,
    available: Condvar,
    idle: AtomicBool,
}
impl Shared {
    fn lock(&self) -> Result<MutexGuard<'_, Queues>, Error> {
        self.queues
            .lock()
            .map_err(|_| Error::Contract("V4L queue poisoned"))
    }
    fn wait<'a>(&self, guard: MutexGuard<'a, Queues>) -> Result<MutexGuard<'a, Queues>, Error> {
        self.available
            .wait(guard)
            .map_err(|_| Error::Contract("V4L queue poisoned"))
    }
}
struct Resources {
    publisher: VideoPublisher,
    capture: Vec<io::IonBuffer>,
}

pub struct V4l {
    fd: Arc<OwnedFd>,
    shared: Arc<Shared>,
    resources: Option<Resources>,
    worker: Option<JoinHandle<Resources>>,
    service: &'static str,
    segment: i32,
    counter: i32,
    stopped: bool,
}
impl V4l {
    pub fn new(
        info: &EncoderInfo,
        input: (i32, i32),
        output: (i32, i32),
        settings: Settings,
        publisher: VideoPublisher,
    ) -> Result<Self, Error> {
        let fd = Arc::new(io::open(
            "/dev/v4l/by-path/platform-aa00000.qcom_vidc-video-index1",
        )?);
        let length = setup::configure(&fd, info, input, output, settings)?;
        io::request_buffers(
            &fd,
            abi::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
            CAPTURE_COUNT as u32,
        )?;
        io::request_buffers(
            &fd,
            abi::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
            INPUT_COUNT as u32,
        )?;
        io::stream(&fd, abi::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE, true)?;
        io::stream(&fd, abi::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE, true)?;
        let ion = Arc::new(io::open("/dev/ion")?);
        let mut encoder = Self {
            fd,
            shared: Arc::new(Shared {
                queues: Mutex::new(Queues {
                    free: (0..INPUT_COUNT).collect(),
                    inputs: std::array::from_fn(|_| None),
                    extras: VecDeque::new(),
                }),
                available: Condvar::new(),
                idle: AtomicBool::new(false),
            }),
            resources: Some(Resources {
                publisher,
                capture: Vec::with_capacity(CAPTURE_COUNT),
            }),
            worker: None,
            service: info.publish,
            segment: -1,
            counter: 0,
            stopped: false,
        };
        for index in 0..CAPTURE_COUNT {
            let buffer = io::IonBuffer::allocate(Arc::clone(&ion), length)?;
            let resources = encoder.resources.as_mut().expect("new encoder resources");
            resources.capture.push(buffer);
            let buffer = &resources.capture[index];
            io::queue(
                &encoder.fd,
                abi::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
                index as u32,
                buffer.mapping(),
                length,
                0,
            )?;
        }
        encoder.open()?;
        Ok(encoder)
    }
    fn open(&mut self) -> Result<(), Error> {
        if self.worker.is_some() {
            return Err(Error::Contract("V4L already open"));
        }
        let resources = self
            .resources
            .take()
            .ok_or(Error::Contract("V4L resources absent"))?;
        let fd = Arc::clone(&self.fd);
        let shared = Arc::clone(&self.shared);
        self.segment = self.segment.wrapping_add(1);
        let segment = self.segment;
        let service = self.service;
        self.worker = Some(std::thread::spawn(move || {
            match dequeue_worker(fd, shared, resources, service, segment) {
                Ok(resources) => resources,
                Err(error) => super::runtime::fatal(error),
            }
        }));
        self.counter = 0;
        Ok(())
    }
    pub fn idle(&self, value: bool) {
        self.shared.idle.store(value, Ordering::Relaxed);
    }
    pub fn encode(
        &mut self,
        mapping: Arc<Mapping>,
        length: usize,
        extra: &VisionMetadata,
    ) -> Result<i32, Error> {
        if self.worker.is_none() {
            return Err(Error::Contract("V4L encode while closed"));
        }
        let mut queues = self.shared.lock()?;
        while queues.free.is_empty() {
            queues = self.shared.wait(queues)?;
        }
        let index = queues.free.pop_front().expect("free slot after wait");
        if queues.inputs[index].is_some() {
            return Err(Error::Contract("V4L reused an active input slot"));
        }
        queues.inputs[index] = Some(mapping);
        queues.extras.push_back(*extra);
        self.shared.available.notify_all();
        if let Err(error) = io::queue(
            &self.fd,
            abi::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
            index as u32,
            queues.inputs[index].as_ref().expect("retained input"),
            length,
            extra.timestamp_eof / 1000,
        ) {
            super::runtime::fatal(error);
        }
        let count = self.counter;
        self.counter = self.counter.wrapping_add(1);
        Ok(count)
    }
    pub fn close(&mut self) -> Result<(), Error> {
        if self.worker.is_none() {
            return Ok(());
        }
        {
            let mut queues = self.shared.lock()?;
            while queues.free.len() != INPUT_COUNT {
                queues = self.shared.wait(queues)?;
            }
            // The source restores slot order after consuming all returned slots.
            queues.free = (0..INPUT_COUNT).collect();
        }
        let mut command = abi::v4l2_encoder_cmd {
            cmd: abi::V4L2_ENC_CMD_STOP,
            ..Default::default()
        };
        // SAFETY: [FFI boundary] STOP receives its initialized command structure;
        // every submitted input has returned, while capture buffers remain live.
        unsafe {
            io::ioctl(&self.fd, abi::ENCODER_VIDIOC_ENCODER_CMD, &mut command)?;
        }
        let worker = self.worker.take().expect("open worker");
        self.resources = Some(
            worker
                .join()
                .map_err(|_| Error::Contract("V4L dequeue thread panicked"))?,
        );
        if !self.shared.lock()?.extras.is_empty() {
            return Err(Error::Contract("source V4L drained metadata assertion"));
        }
        Ok(())
    }
    pub fn rotate(&mut self) -> Result<(), Error> {
        self.close()?;
        self.open()
    }
    fn shutdown(&mut self) -> Result<(), Error> {
        if self.stopped {
            return Ok(());
        }
        self.close()?;
        io::stream(&self.fd, abi::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE, false)?;
        io::request_buffers(&self.fd, abi::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE, 0)?;
        io::stream(&self.fd, abi::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE, false)?;
        io::request_buffers(&self.fd, abi::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE, 0)?;
        self.stopped = true;
        Ok(())
    }
}
impl Drop for V4l {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            super::runtime::fatal(error);
        }
    }
}

fn dequeue_worker(
    fd: Arc<OwnedFd>,
    shared: Arc<Shared>,
    mut resources: Resources,
    service: &'static str,
    segment: i32,
) -> Result<Resources, Error> {
    platform::name(&format!("dq-{service}"))?;
    let mut index = u32::MAX;
    let mut header = Vec::new();
    let mut exit = false;
    let debug = platform::debug_encoder();
    while !exit {
        let mut poll = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN | libc::POLLOUT,
            revents: 0,
        };
        // SAFETY: [FFI boundary] one initialized descriptor lives across poll.
        let result = unsafe { libc::poll(&mut poll, 1, 1000) };
        if result < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::Interrupted {
                failure(format!(
                    "poll failed ({result} - {})",
                    error.raw_os_error().unwrap_or(0)
                ));
            }
            continue;
        }
        if result == 0 {
            if !shared.idle.load(Ordering::Relaxed) {
                failure("encoder dequeue poll timeout".into());
            }
            continue;
        }
        if debug >= 2 {
            println!(
                "{service:>20} poll {:x} at {:.2} ms",
                poll.revents,
                super::publisher::nanos(rustix::time::ClockId::Boottime)? as f64 / 1e6
            );
        }
        if poll.revents & libc::POLLIN != 0 {
            let packet = io::dequeue(&fd, abi::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE)?;
            let buffer = resources
                .capture
                .get(packet.index)
                .ok_or(Error::Contract("V4L capture index out of range"))?;
            // Source ignores cache-sync failure; it is not a stream-fatal policy.
            let _ = buffer.sync_from_device();
            if packet.length > buffer.length() {
                return Err(Error::Contract("V4L packet exceeds capture allocation"));
            }
            let mut frame_id = -1;
            if packet.flags & EOS != 0 {
                exit = true;
            } else if packet.flags & CODEC_CONFIG != 0 {
                header = buffer.mapping().copy(packet.length)?;
            } else {
                let extra = {
                    let mut queues = shared.lock()?;
                    while queues.extras.is_empty() {
                        queues = shared.wait(queues)?;
                    }
                    queues.extras.pop_front().expect("metadata after wait")
                };
                if extra.timestamp_eof / 1000 != packet.timestamp_us {
                    return Err(Error::Contract(
                        "source V4L timestamp synchronization assertion",
                    ));
                }
                frame_id = extra.frame_id as i32;
                index = index.wrapping_add(1);
                resources.publisher.publish(
                    segment,
                    index,
                    &extra,
                    packet.flags,
                    &header,
                    &buffer.mapping().copy(packet.length)?,
                )?;
            }
            if debug != 0 {
                println!("{service:>20} got({}) {:6} bytes flags {:8x} idx {segment:3}/{:4} id {frame_id:8} ts {} lat {:.2} ms ({} frames free)",
                    packet.index, packet.length, packet.flags, index as i32, packet.timestamp_us,
                    super::publisher::nanos(rustix::time::ClockId::Boottime)? as f64 / 1e6 - packet.timestamp_us as f64 / 1000., shared.lock()?.free.len());
            }
            io::queue(
                &fd,
                abi::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
                packet.index as u32,
                buffer.mapping(),
                buffer.length(),
                0,
            )?;
        }
        if poll.revents & libc::POLLOUT != 0 {
            let returned = io::dequeue(&fd, abi::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE)?;
            let mut queues = shared.lock()?;
            let input = queues
                .inputs
                .get_mut(returned.index)
                .ok_or(Error::Contract("V4L returned input index out of range"))?;
            if input.take().is_none() {
                return Err(Error::Contract("V4L returned an unused input slot"));
            }
            queues.free.push_back(returned.index);
            shared.available.notify_all();
        }
    }
    Ok(resources)
}
