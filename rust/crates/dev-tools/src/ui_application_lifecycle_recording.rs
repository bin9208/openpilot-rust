//! Independent GuiApplication encoder arguments and shutdown policy.
//!
//! The owner supplies detached process/thread endpoints. This module preserves
//! the original exception scopes and deadlines, including the second wait's
//! swallowed timeout. It does not own application rendering or child transport.
use num_bigint::BigInt;
use std::{
    ffi::{OsStr, OsString},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseError {
    Timeout,
    Other,
}

/// The implementation must detach member references while retaining the
/// endpoints needed below. Presence queries refer to those retained endpoints.
pub trait ClosePeer {
    fn detach(&mut self);
    fn has_thread(&self) -> bool;
    fn has_event(&self) -> bool;
    fn has_queue(&self) -> bool;
    fn has_process(&self) -> bool;
    fn stdin_open(&self) -> bool;
    fn event_set(&mut self) -> Result<(), CloseError>;
    fn queue_sentinel(&mut self) -> Result<(), CloseError>;
    fn join(&mut self, timeout: Duration) -> Result<(), CloseError>;
    fn flush(&mut self) -> Result<(), CloseError>;
    fn close_stdin(&mut self) -> Result<(), CloseError>;
    fn wait(&mut self, timeout: Duration) -> Result<(), CloseError>;
    fn terminate(&mut self) -> Result<(), CloseError>;
    fn kill(&mut self) -> Result<(), CloseError>;
}

pub fn close(peer: &mut dyn ClosePeer) {
    peer.detach();
    // event.set() and thread.join() share the outer original try block.
    // A full queue, unlike an event error, does not skip the join.
    let event_ok = !peer.has_thread() || !peer.has_event() || peer.event_set().is_ok();
    if event_ok && peer.has_thread() && peer.has_queue() {
        let _ = peer.queue_sentinel();
        let _ = peer.join(Duration::from_secs(30));
    }
    if peer.has_process() {
        if peer.stdin_open() {
            let _ = peer.flush();
            let _ = peer.close_stdin();
        }
        match peer.wait(Duration::from_secs(10)) {
            Err(CloseError::Timeout) => {
                let _ = peer.terminate();
                let _ = peer.wait(Duration::from_secs(5));
            }
            Err(CloseError::Other) => {
                let _ = peer.kill();
            }
            Ok(()) => {}
        }
    }
}

pub struct GlobalOptions {
    pub quality: BigInt,
    pub speed: BigInt,
    pub bitrate: OsString,
}
fn command(
    size: (u32, u32),
    fps: i64,
    output_fps: String,
    preset: &str,
    quality: String,
    bitrate: &OsStr,
    output: &OsStr,
) -> Vec<OsString> {
    let (width, height) = size;
    let mut args = [
        "ffmpeg", "-v", "warning", "-nostats", "-f", "rawvideo", "-pix_fmt", "rgba", "-s",
    ]
    .map(OsString::from)
    .to_vec();
    args.extend([
        format!("{width}x{height}").into(),
        "-r".into(),
        fps.to_string().into(),
        "-i".into(),
        "pipe:0".into(),
        "-vf".into(),
        "vflip,format=yuv420p".into(),
        "-r".into(),
        output_fps.into(),
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        preset.into(),
        "-crf".into(),
        quality.into(),
    ]);
    if !bitrate.is_empty() {
        for flag in ["-b:v", "-maxrate", "-bufsize"] {
            args.extend([flag.into(), bitrate.to_owned()]);
        }
    }
    args.extend(["-y".into(), "-f".into(), "mp4".into(), output.to_owned()]);
    args
}
pub fn global_command(
    width: u32,
    height: u32,
    fps: i64,
    options: &GlobalOptions,
    output: &OsStr,
) -> Vec<OsString> {
    command(
        (width, height),
        fps,
        (BigInt::from(fps) * &options.speed).to_string(),
        "veryfast",
        options.quality.to_string(),
        &options.bitrate,
        output,
    )
}
pub fn dynamic_command(width: u32, height: u32, fps: i64, output: &OsStr) -> Vec<OsString> {
    let fps = if fps > 0 { fps } else { 20 };
    command(
        (width, height),
        fps,
        fps.to_string(),
        "ultrafast",
        "23".into(),
        OsStr::new(""),
        output,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriterError {
    Empty,
    Failure,
}
pub enum WriterRead {
    Frame(Vec<u8>),
    Sentinel,
}
pub trait WriterPeer {
    fn read(&mut self, timeout: Duration) -> Result<WriterRead, WriterError>;
    fn stopped(&mut self) -> Result<bool, WriterError>;
    fn write(&mut self, bytes: &[u8]) -> Result<usize, WriterError>;
}
pub fn write_frames(peer: &mut dyn WriterPeer) -> Result<(), WriterError> {
    loop {
        match peer.read(Duration::from_secs(1)) {
            Ok(WriterRead::Sentinel) => return Ok(()),
            Ok(WriterRead::Frame(bytes)) => {
                // Original BufferedWriter.write is invoked once; its return
                // count is ignored, including a successful partial return.
                if peer.write(&bytes).is_err() {
                    return Ok(());
                }
            }
            Err(WriterError::Empty) => {
                // Errors inside the original except queue.Empty handler escape.
                if peer.stopped()? {
                    return Ok(());
                }
            }
            Err(WriterError::Failure) => return Ok(()),
        }
    }
}
pub trait DynamicPeer {
    fn ensure_texture(&mut self) -> Result<bool, String>;
    fn new_path(&mut self) -> Result<std::path::PathBuf, String>;
    fn init(&mut self, path: &std::path::Path, enabled: bool) -> Result<(), String>;
    fn clock(&mut self, enabled: bool) -> Result<f64, String>;
    fn close(&mut self, enabled: bool) -> Result<(), String>;
}
pub struct Dynamic {
    pub enabled: bool,
    pub started: f64,
}
impl Dynamic {
    pub fn start(&mut self, peer: &mut dyn DynamicPeer) -> Result<(), String> {
        if self.enabled || !peer.ensure_texture()? {
            return Ok(());
        }
        let path = peer.new_path()?;
        peer.init(&path, self.enabled)?;
        self.enabled = true;
        self.started = peer.clock(self.enabled)?;
        println!("[REC] start -> {}", path.display());
        Ok(())
    }
    pub fn stop(&mut self, peer: &mut dyn DynamicPeer) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        self.enabled = false;
        peer.close(self.enabled)?;
        println!("[REC] stop");
        Ok(())
    }
}

use std::{
    io::{self, BufWriter, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitError {
    Full,
    Closed,
}
struct Shared {
    detached: AtomicBool,
    stopped: AtomicBool,
    frames: AtomicUsize,
    input: Mutex<Option<BufWriter<ChildStdin>>>,
    // The original application owns its Queue independently of the writer
    // thread; a broken pipe must not turn subsequent put_nowait into disconnect.
    receiver: Mutex<Receiver<Option<Vec<u8>>>>,
}
struct FrameWriter {
    shared: Arc<Shared>,
}
impl WriterPeer for FrameWriter {
    fn read(&mut self, timeout: Duration) -> Result<WriterRead, WriterError> {
        if self.shared.detached.load(Ordering::Acquire) {
            return Err(WriterError::Failure);
        }
        let receiver = self
            .shared
            .receiver
            .lock()
            .map_err(|_| WriterError::Failure)?;
        match receiver.recv_timeout(timeout) {
            Ok(Some(frame)) => Ok(WriterRead::Frame(frame)),
            Ok(None) => Ok(WriterRead::Sentinel),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(WriterError::Empty),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(WriterError::Failure),
        }
    }
    fn stopped(&mut self) -> Result<bool, WriterError> {
        if self.shared.detached.load(Ordering::Acquire) {
            return Err(WriterError::Failure);
        }
        Ok(self.shared.stopped.load(Ordering::Acquire))
    }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, WriterError> {
        if self.shared.detached.load(Ordering::Acquire) {
            return Err(WriterError::Failure);
        }
        let mut input = self.shared.input.lock().map_err(|_| WriterError::Failure)?;
        input
            .as_mut()
            .ok_or(WriterError::Failure)?
            .write_all(bytes)
            .map_err(|_| WriterError::Failure)?;
        self.shared.frames.fetch_add(1, Ordering::Release);
        Ok(bytes.len())
    }
}
struct OwnedEncoder {
    child: Child,
    sender: Option<SyncSender<Option<Vec<u8>>>>,
    writer: Option<JoinHandle<()>>,
    shared: Arc<Shared>,
}
impl ClosePeer for OwnedEncoder {
    fn detach(&mut self) {
        self.shared.detached.store(true, Ordering::Release);
    }
    fn has_thread(&self) -> bool {
        self.writer.is_some()
    }
    fn has_event(&self) -> bool {
        true
    }
    fn has_queue(&self) -> bool {
        self.sender.is_some()
    }
    fn has_process(&self) -> bool {
        true
    }
    fn stdin_open(&self) -> bool {
        self.shared.input.lock().is_ok_and(|v| v.is_some())
    }
    fn event_set(&mut self) -> Result<(), CloseError> {
        self.shared.stopped.store(true, Ordering::Release);
        Ok(())
    }
    fn queue_sentinel(&mut self) -> Result<(), CloseError> {
        self.sender
            .as_ref()
            .ok_or(CloseError::Other)?
            .try_send(None)
            .map_err(|_| CloseError::Other)
    }
    fn join(&mut self, timeout: Duration) -> Result<(), CloseError> {
        let start = Instant::now();
        while self.writer.as_ref().is_some_and(|v| !v.is_finished()) {
            if start.elapsed() >= timeout {
                return Err(CloseError::Timeout);
            }
            thread::sleep(Duration::from_millis(2));
        }
        if let Some(writer) = self.writer.take() {
            writer.join().map_err(|_| CloseError::Other)?;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), CloseError> {
        self.shared
            .input
            .lock()
            .map_err(|_| CloseError::Other)?
            .as_mut()
            .ok_or(CloseError::Other)?
            .flush()
            .map_err(|_| CloseError::Other)
    }
    fn close_stdin(&mut self) -> Result<(), CloseError> {
        let input = self
            .shared
            .input
            .lock()
            .map_err(|_| CloseError::Other)?
            .take();
        // Original BufferedWriter.close may retry buffered bytes after the
        // explicit flush. BufWriter's drop performs that same final buffer flush.
        drop(input);
        Ok(())
    }
    fn wait(&mut self, timeout: Duration) -> Result<(), CloseError> {
        let start = Instant::now();
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return Ok(()),
                Ok(None) if start.elapsed() < timeout => {
                    thread::sleep(Duration::from_millis(2));
                }
                Ok(None) => return Err(CloseError::Timeout),
                Err(_) => return Err(CloseError::Other),
            }
        }
    }
    fn terminate(&mut self) -> Result<(), CloseError> {
        let raw = i32::try_from(self.child.id()).map_err(|_| CloseError::Other)?;
        let pid = rustix::process::Pid::from_raw(raw).ok_or(CloseError::Other)?;
        rustix::process::kill_process(pid, rustix::process::Signal::TERM)
            .map_err(|_| CloseError::Other)
    }
    fn kill(&mut self) -> Result<(), CloseError> {
        self.child.kill().map_err(|_| CloseError::Other)
    }
}
/// The active recorder is taken before any shutdown callbacks, as in the
/// original's detached member references. Global/dynamic callers choose their
/// captured argv and queue capacities (60/8); transport has no mode policy.
pub struct Encoder {
    active: Option<OwnedEncoder>,
}
impl Encoder {
    pub fn spawn(command: &mut Command, capacity: usize) -> io::Result<Self> {
        let mut child = command.stdin(Stdio::piped()).spawn()?;
        let Some(input) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other("application encoder stdin missing"));
        };
        let (sender, receiver) = mpsc::sync_channel(capacity);
        // Python subprocess stdin is a buffered binary stream.
        let shared = Arc::new(Shared {
            detached: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            frames: AtomicUsize::new(0),
            input: Mutex::new(Some(BufWriter::new(input))),
            receiver: Mutex::new(receiver),
        });
        let mut peer = FrameWriter {
            shared: Arc::clone(&shared),
        };
        let writer = match thread::Builder::new()
            .name("application-encoder".into())
            .spawn(move || {
                let _ = write_frames(&mut peer);
            }) {
            Ok(writer) => writer,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        Ok(Self {
            active: Some(OwnedEncoder {
                child,
                sender: Some(sender),
                writer: Some(writer),
                shared,
            }),
        })
    }
    pub fn pid(&self) -> Option<u32> {
        self.active.as_ref().map(|v| v.child.id())
    }
    pub fn writer_finished(&self) -> bool {
        self.active
            .as_ref()
            .is_none_or(|v| v.writer.as_ref().is_none_or(JoinHandle::is_finished))
    }
    pub fn written_frames(&self) -> usize {
        self.active
            .as_ref()
            .map_or(0, |v| v.shared.frames.load(Ordering::Acquire))
    }
    pub fn submit(&self, frame: Vec<u8>) -> Result<(), SubmitError> {
        let active = self.active.as_ref().ok_or(SubmitError::Closed)?;
        active
            .sender
            .as_ref()
            .ok_or(SubmitError::Closed)?
            .try_send(Some(frame))
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => SubmitError::Full,
                mpsc::TrySendError::Disconnected(_) => SubmitError::Closed,
            })
    }
    pub fn close(&mut self) {
        let Some(mut active) = self.active.take() else {
            return;
        };
        close(&mut active);
        // The source deliberately does not kill after a second wait timeout.
        // Reap asynchronously if it is still alive, without changing signals.
        if matches!(active.child.try_wait(), Ok(None)) {
            thread::spawn(move || {
                let _ = active.child.wait();
            });
        }
    }
}
impl Drop for Encoder {
    fn drop(&mut self) {
        self.close();
    }
}
