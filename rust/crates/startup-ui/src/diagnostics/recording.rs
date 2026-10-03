use crate::Error;
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender, TrySendError},
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub struct Encoding {
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub speed: u32,
    pub quality: u8,
    pub bitrate: String,
    pub preset: &'static str,
    pub capacity: usize,
}
impl Encoding {
    pub fn arguments(&self, output: &Path) -> Result<Vec<String>, Error> {
        if self.width <= 0 || self.height <= 0 || self.fps <= 0 {
            return Err(Error::Contract(
                "recording dimensions and fps must be positive",
            ));
        }
        let output_fps = u32::try_from(self.fps)
            .map_err(|_| Error::Contract("invalid fps"))?
            .checked_mul(self.speed)
            .ok_or(Error::Contract("recording fps overflow"))?;
        let mut args = vec![
            "-v".into(),
            "warning".into(),
            "-nostats".into(),
            "-f".into(),
            "rawvideo".into(),
            "-pix_fmt".into(),
            "rgba".into(),
            "-s".into(),
            format!("{}x{}", self.width, self.height),
            "-r".into(),
            self.fps.to_string(),
            "-i".into(),
            "pipe:0".into(),
            "-vf".into(),
            "vflip,format=yuv420p".into(),
            "-r".into(),
            output_fps.to_string(),
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            self.preset.into(),
            "-crf".into(),
            self.quality.to_string(),
        ];
        if !self.bitrate.is_empty() {
            for key in ["-b:v", "-maxrate", "-bufsize"] {
                args.push(key.into());
                args.push(self.bitrate.clone());
            }
        }
        args.extend([
            "-y".into(),
            "-f".into(),
            "mp4".into(),
            output
                .to_str()
                .ok_or(Error::Contract("recording path is not UTF-8"))?
                .into(),
        ]);
        Ok(args)
    }
}
pub struct Recorder {
    child: Child,
    sender: Option<SyncSender<Vec<u8>>>,
    done: mpsc::Receiver<Result<(), std::io::Error>>,
    worker: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    expected_bytes: usize,
    pub output: PathBuf,
    pub accepted: u64,
    pub dropped: u64,
}
impl Recorder {
    pub fn child_pid(&self) -> u32 {
        self.child.id()
    }
    pub fn start(encoding: &Encoding, output: &Path) -> Result<Self, Error> {
        let args = encoding.arguments(output)?;
        let mut child = Command::new("ffmpeg")
            .args(args)
            .stdin(Stdio::piped())
            .spawn()?;
        let mut input = child
            .stdin
            .take()
            .ok_or(Error::Contract("ffmpeg stdin missing"))?;
        let (sender, receiver) = mpsc::sync_channel::<Vec<u8>>(encoding.capacity);
        let (done_sender, done) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = std::thread::Builder::new()
            .name("ui-ffmpeg".into())
            .spawn(move || {
                let result = (|| {
                    loop {
                        match receiver.recv_timeout(Duration::from_secs(1)) {
                            Ok(bytes) => input.write_all(&bytes)?,
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                if worker_stop.load(Ordering::Relaxed) {
                                    break;
                                }
                            }
                        }
                    }
                    input.flush()
                })();
                drop(input);
                if done_sender.send(result).is_err() {
                    eprintln!("UI recording completion receiver closed");
                }
            });
        let worker = match worker {
            Ok(worker) => worker,
            Err(error) => {
                if let Err(kill) = child.kill() {
                    eprintln!("UI recording child cleanup: {kill}");
                }
                if let Err(wait) = child.wait() {
                    eprintln!("UI recording child wait: {wait}");
                }
                return Err(error.into());
            }
        };
        let expected_bytes = usize::try_from(encoding.width)
            .ok()
            .and_then(|width| {
                usize::try_from(encoding.height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(Error::Contract("recording image size overflow"))?;
        Ok(Self {
            child,
            sender: Some(sender),
            done,
            worker: Some(worker),
            stop,
            expected_bytes,
            output: output.into(),
            accepted: 0,
            dropped: 0,
        })
    }
    pub fn submit(&mut self, bytes: Vec<u8>) -> Result<(), Error> {
        if bytes.len() != self.expected_bytes {
            return Err(Error::Contract("recording frame byte length mismatch"));
        }
        let sender = self
            .sender
            .as_ref()
            .ok_or(Error::Contract("recording already closed"))?;
        match sender.try_send(bytes) {
            Ok(()) => self.accepted = self.accepted.saturating_add(1),
            Err(TrySendError::Full(_)) => self.dropped = self.dropped.saturating_add(1),
            Err(TrySendError::Disconnected(_)) => {
                return Err(Error::Contract("ffmpeg writer disconnected"));
            }
        }
        Ok(())
    }
    pub fn close(&mut self) -> Result<(), Error> {
        if self.worker.is_none() {
            return Ok(());
        }
        self.stop.store(true, Ordering::Relaxed);
        self.sender.take();
        let writer = self.done.recv_timeout(Duration::from_secs(30));
        if matches!(writer, Err(mpsc::RecvTimeoutError::Timeout)) {
            self.child.kill()?;
        }
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| Error::Contract("recording writer panicked"))?;
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = self.child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                self.child.kill()?;
                break self.child.wait()?;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        match writer {
            Ok(result) => result?,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(Error::Contract("ffmpeg writer timed out"));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(Error::Contract("ffmpeg completion lost"));
            }
        }
        if !status.success() {
            return Err(Error::Contract("ffmpeg encoding failed"));
        }
        Ok(())
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("UI recording close: {error}");
        }
    }
}
