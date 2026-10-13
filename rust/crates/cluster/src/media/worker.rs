use super::{
    decoder::{Decoder, Factory},
    request::Request,
};
use crate::models::NaviMediaFrame;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{mpsc, Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

pub(crate) struct ResultFrame {
    pub epoch: u64,
    pub frame: Arc<NaviMediaFrame>,
}
#[derive(Default)]
struct State {
    pending: VecDeque<Request>,
    results: BTreeMap<String, ResultFrame>,
    waiting: BTreeSet<String>,
    stopped: bool,
    dropped: u64,
    error: String,
}
#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}
pub(crate) struct DecodeWorker {
    shared: Arc<Shared>,
    factory: Factory,
    thread: Option<JoinHandle<()>>,
    finished: Option<mpsc::Receiver<()>>,
}
impl DecodeWorker {
    pub fn new(factory: Factory) -> Self {
        Self {
            shared: Arc::new(Shared::default()),
            factory,
            thread: None,
            finished: None,
        }
    }
    pub fn submit(&mut self, mut request: Request) {
        if self
            .shared
            .state
            .lock()
            .expect("media worker state")
            .stopped
        {
            return;
        }
        if self.thread.is_none() {
            let shared = Arc::clone(&self.shared);
            let factory = Arc::clone(&self.factory);
            let (finished, receiver) = mpsc::channel();
            self.finished = Some(receiver);
            self.thread = Some(
                thread::Builder::new()
                    .name("cluster-navi-h264-decode".into())
                    .spawn(move || {
                        run(&shared, &factory);
                        let _ = finished.send(());
                    })
                    .expect("start cluster H264 decode worker"),
            );
        }
        let mut state = self.shared.state.lock().expect("media worker state");
        if state.stopped {
            return;
        }
        if request.keyframe {
            request.reset = state.waiting.remove(&request.key);
            let previous = state.pending.len();
            state.pending.retain(|item| item.key != request.key);
            state.dropped +=
                u64::try_from(previous - state.pending.len()).expect("bounded queue length");
        } else if state.waiting.contains(&request.key) {
            state.dropped += 1;
            return;
        } else if state
            .pending
            .iter()
            .filter(|item| item.key == request.key)
            .count()
            >= 60
        {
            let previous = state.pending.len();
            state.pending.retain(|item| item.key != request.key);
            state.dropped +=
                u64::try_from(previous - state.pending.len() + 1).expect("bounded queue length");
            state.waiting.insert(request.key);
            return;
        }
        state.pending.push_back(request);
        self.shared.wake.notify_one();
    }
    pub fn discard(&mut self, key: &str) {
        let mut state = self.shared.state.lock().expect("media worker state");
        state.pending.retain(|item| item.key != key);
        state.results.remove(key);
        state.waiting.remove(key);
    }
    pub fn reset(&mut self) {
        let mut state = self.shared.state.lock().expect("media worker state");
        state.pending.clear();
        state.results.clear();
        state.waiting.clear();
    }
    pub(crate) fn poll(&mut self) -> Vec<ResultFrame> {
        let mut state = self.shared.state.lock().expect("media worker state");
        std::mem::take(&mut state.results).into_values().collect()
    }
    pub fn dropped_requests(&self) -> u64 {
        self.shared
            .state
            .lock()
            .expect("media worker state")
            .dropped
    }
    pub fn close(&mut self) -> bool {
        {
            let mut state = self.shared.state.lock().expect("media worker state");
            state.stopped = true;
            state.pending.clear();
            self.shared.wake.notify_all();
        }
        let done = self.thread.is_none()
            || self
                .finished
                .as_ref()
                .is_none_or(|finished| finished.recv_timeout(Duration::from_secs(1)).is_ok());
        if done {
            self.finished = None;
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
        done
    }
}
impl Drop for DecodeWorker {
    fn drop(&mut self) {
        self.close();
    }
}
fn run(shared: &Shared, factory: &Factory) {
    let mut decoders: BTreeMap<String, Box<dyn Decoder>> = BTreeMap::new();
    let mut configs: BTreeMap<String, (Vec<u8>, i64, i64)> = BTreeMap::new();
    let mut epochs: BTreeMap<String, u64> = BTreeMap::new();
    loop {
        let request = {
            let mut state = shared.state.lock().expect("media worker state");
            while state.pending.is_empty() && !state.stopped {
                state = shared.wake.wait(state).expect("media worker wake");
            }
            if state.stopped {
                return;
            }
            state.pending.pop_front().expect("nonempty request queue")
        };
        let result = (|| {
            if !decoders.contains_key(&request.key) {
                decoders.insert(request.key.clone(), factory()?);
            }
            let decoder = decoders.get_mut(&request.key).expect("created decoder");
            let config = (request.config.clone(), request.width, request.height);
            if request.reset
                || configs.get(&request.key) != Some(&config)
                || epochs.get(&request.key) != Some(&request.epoch)
            {
                if epochs.contains_key(&request.key) {
                    decoder.reset()?;
                }
                configs.insert(request.key.clone(), config);
                epochs.insert(request.key.clone(), request.epoch);
            }
            let mut packet = request.config.clone();
            packet.extend_from_slice(&request.payload);
            let Some(output) =
                decoder.decode(&packet, request.sequence, request.width, request.height)?
            else {
                return Ok(None);
            };
            let mime = if output.hardware.is_some() {
                "video/nv12-dmabuf"
            } else {
                "image/yuv420p"
            };
            let frame = NaviMediaFrame {
                key: request.key.clone(),
                sequence: output.sequence.unwrap_or(request.sequence),
                present: true,
                mime: mime.into(),
                width: output.width,
                height: output.height,
                plane_data: output.planes,
                plane_strides: output.strides,
                hardware_buffer: output.hardware,
                ..NaviMediaFrame::default()
            };
            Ok::<_, String>(Some(ResultFrame {
                epoch: request.epoch,
                frame: Arc::new(frame),
            }))
        })();
        let mut state = shared.state.lock().expect("media worker state");
        match result {
            Ok(Some(frame)) => {
                state.results.insert(request.key, frame);
                state.error.clear();
            }
            Ok(None) => {}
            Err(error) => {
                if state.error != error {
                    eprintln!("Navi H264 decode worker failed: {error}");
                }
                state.error = error;
                state.waiting.insert(request.key);
            }
        }
    }
}

#[cfg(test)]
mod tests;
