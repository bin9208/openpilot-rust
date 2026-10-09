use super::debug_codec::{Codec, Encoder, Output, Work};
use crate::Error;
use std::{
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    thread::{self, JoinHandle},
    time::{SystemTime, UNIX_EPOCH},
};

struct Worker {
    input: Option<SyncSender<Work>>,
    output: Receiver<Result<Output, Error>>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    fn new(kind: Codec) -> Result<Self, Error> {
        let (input, requests) = mpsc::sync_channel(1);
        let (results, output) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("webrtc-debug-video".to_owned())
            .spawn(move || {
                let mut encoder = Encoder::new(kind);
                while let Ok(request) = requests.recv() {
                    if results.send(encoder.encode(request)).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            input: Some(input),
            output,
            thread: Some(thread),
        })
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.input = None;
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                eprintln!("WebRTC debug codec worker panicked");
            }
        }
    }
}

pub(crate) struct DebugTrack {
    worker: Option<Worker>,
    started: Option<f64>,
    timestamp: i64,
    pending: bool,
    keyframe: bool,
    bitrate: Option<u32>,
}

impl DebugTrack {
    pub(super) fn new() -> Result<Self, Error> {
        ffmpeg_next::init()?;
        Ok(Self {
            worker: None,
            started: None,
            timestamp: 0,
            pending: false,
            keyframe: false,
            bitrate: None,
        })
    }

    pub(super) fn receive(&mut self, mime: &str) -> Result<Option<Output>, Error> {
        if self.worker.is_none() {
            self.worker = Some(Worker::new(Codec::parse(mime)?)?);
        }
        let worker = self
            .worker
            .as_ref()
            .ok_or(Error::Contract("debug worker missing"))?;
        if self.pending {
            return match worker.output.try_recv() {
                Ok(output) => {
                    self.pending = false;
                    output.map(Some)
                }
                Err(TryRecvError::Empty) => Ok(None),
                Err(TryRecvError::Disconnected) => Err(Error::Contract("debug worker exited")),
            };
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Contract("debug wall clock predates epoch"))?
            .as_secs_f64();
        let start = *self.started.get_or_insert(now);
        let wait = start
            + num_traits::ToPrimitive::to_f64(&self.timestamp)
                .ok_or(Error::Contract("debug clock overflow"))?
                / 90_000.0
            - now;
        if wait > 0.0 {
            return Ok(None);
        }
        worker
            .input
            .as_ref()
            .ok_or(Error::Contract("debug worker closed"))?
            .send(Work {
                pts: self.timestamp,
                keyframe: self.keyframe,
                bitrate: self.bitrate,
            })
            .map_err(|_| Error::Contract("debug worker unavailable"))?;
        self.keyframe = false;
        self.timestamp = self
            .timestamp
            .checked_add(3000)
            .ok_or(Error::Contract("debug clock overflow"))?;
        self.pending = true;
        Ok(None)
    }

    pub(super) fn keyframe(&mut self) {
        self.keyframe = true;
    }

    pub(super) fn bitrate(&mut self, bitrate: u32) {
        if self.worker.is_some() {
            self.bitrate = Some(bitrate);
        }
    }
}
