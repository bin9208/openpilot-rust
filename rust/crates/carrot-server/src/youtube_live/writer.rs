use super::{
    rtmp::Client,
    writer_state::{Frame, Shared, State, Stats, BYTES, FRAMES},
};
use crate::Error;
use std::{
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};
use tokio::sync::{oneshot, watch};

pub(super) struct Codec {
    pub header: Vec<u8>,
    pub fps: u32,
}
pub(super) struct Writer {
    pub client: Arc<Client>,
    shared: Arc<Shared>,
    initialized: Option<oneshot::Receiver<Result<(), String>>>,
    thread: Option<JoinHandle<()>>,
}
impl Writer {
    pub fn spawn(client: Arc<Client>, codec: Codec) -> Result<Self, Error> {
        let (done, _) = watch::channel(false);
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            ready: Condvar::new(),
            done,
            sink: Arc::new(Mutex::new(super::sink::Snapshot::default())),
        });
        let (ready, initialized) = oneshot::channel();
        let owned = Arc::clone(&shared);
        let transport = Arc::clone(&client);
        let thread = std::thread::Builder::new()
            .name("youtube-rtmp".into())
            .spawn(move || {
                let _completion = super::writer_state::Completion(Arc::clone(&owned));
                super::writer_worker::run(transport, codec, &owned, ready);
            })?;
        Ok(Self {
            client,
            shared,
            initialized: Some(initialized),
            thread: Some(thread),
        })
    }
    pub fn initialization(&mut self) -> Result<oneshot::Receiver<Result<(), String>>, Error> {
        self.initialized
            .take()
            .ok_or_else(|| Error::Source("RTMP writer already initialized".into()))
    }
    pub fn enqueue(&self, frame: Frame) -> Result<bool, Error> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| Error::Source("RTMP writer poisoned".into()))?;
        state.stats.rejection.clear();
        let rejection = if !state.stats.error.is_empty() {
            Some(state.stats.error.clone())
        } else if !state.running || state.stopping {
            Some("RTMP writer is not running".into())
        } else if frame
            .payload
            .len()
            .saturating_add(state.stats.pending_bytes)
            > BYTES
        {
            Some(format!("RTMP frame backlog reached {BYTES} bytes"))
        } else if state.queue.len() >= FRAMES {
            Some(format!("RTMP frame backlog reached {FRAMES} frames"))
        } else {
            None
        };
        if let Some(rejection) = rejection {
            state.stats.rejection = rejection;
            return Ok(false);
        }
        state.stats.pending_bytes += frame.payload.len();
        state.queue.push_back(frame);
        state.stats.pending_frames = state.queue.len();
        state.stats.high_frames = state.stats.high_frames.max(state.queue.len());
        state.stats.high_bytes = state.stats.high_bytes.max(state.stats.pending_bytes);
        self.shared.ready.notify_one();
        Ok(true)
    }
    pub fn snapshot(&self) -> Result<Stats, Error> {
        self.shared
            .state
            .lock()
            .map(|state| state.stats.clone())
            .map_err(|_| Error::Source("RTMP writer poisoned".into()))
    }
    pub fn sink_snapshot(&self) -> Result<super::sink::Snapshot, Error> {
        self.shared
            .sink
            .lock()
            .map(|snapshot| snapshot.clone())
            .map_err(|_| Error::Source("RTMP sink snapshot poisoned".into()))
    }
    pub fn request_stop(&self) -> usize {
        match self.shared.state.lock() {
            Ok(mut state) => {
                state.stopping = true;
                let discarded = state.queue.len();
                state.queue.clear();
                state.stats.pending_bytes = 0;
                state.stats.pending_frames = 0;
                self.shared.ready.notify_one();
                discarded
            }
            Err(_) => 0,
        }
    }
    pub async fn stop(&mut self) -> Result<usize, Error> {
        let mut done = self.shared.done.subscribe();
        let discarded = self.request_stop();
        while !*done.borrow_and_update() {
            if done.changed().await.is_err() {
                break;
            }
        }
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| Error::Source("RTMP writer panicked".into()))?;
        }
        Ok(discarded)
    }
    pub fn force(&self) {
        self.client.wake_shutdown();
        self.request_stop();
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        self.force();
        if let Some(thread) = self.thread.take() {
            let _finished = thread.join();
        }
    }
}
