use super::{
    flv::Muxer,
    rtmp::Client,
    sink::Sink,
    writer::Codec,
    writer_state::{Frame, Shared},
};
use crate::Error;
use std::{sync::Arc, time::Instant};
use tokio::sync::oneshot;

pub(super) fn run(
    client: Arc<Client>,
    codec: Codec,
    shared: &Shared,
    ready: oneshot::Sender<Result<(), String>>,
) {
    let result = start(Arc::clone(&client), codec, Arc::clone(&shared.sink));
    match result {
        Ok(mut muxer) => {
            if let Ok(mut state) = shared.state.lock() {
                state.running = !state.stopping;
            }
            let _ready = ready.send(Ok(()));
            while let Some(frame) = next(shared) {
                let started = Instant::now();
                let result = muxer.mux(&frame.payload, frame.keyframe, None);
                let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                if let Ok(mut state) = shared.state.lock() {
                    match result {
                        Ok(()) => {
                            state.stats.last_write_ms = elapsed;
                            state.stats.max_write_ms = state.stats.max_write_ms.max(elapsed);
                            state.stats.frames_written =
                                state.stats.frames_written.saturating_add(1);
                        }
                        Err(error) => {
                            state.stats.error = error.to_string();
                            break;
                        }
                    }
                } else {
                    break;
                }
            }
            let _flushed = muxer.close();
        }
        Err(error) => {
            let message = error.to_string();
            if let Ok(mut state) = shared.state.lock() {
                state.stats.error = message.clone();
            }
            let _ready = ready.send(Err(message));
        }
    }
    client.close();
    if let Ok(mut state) = shared.state.lock() {
        state.running = false;
    }
}
fn start(
    client: Arc<Client>,
    codec: Codec,
    sink: Arc<std::sync::Mutex<super::sink::Snapshot>>,
) -> Result<Muxer<Sink>, Error> {
    client.connect()?;
    // AAC context/frame allocation and every codec call stay on this OS thread.
    Muxer::new(Sink::observed(client, sink), &codec.header, codec.fps)
}
fn next(shared: &Shared) -> Option<Frame> {
    let mut state = shared.state.lock().ok()?;
    loop {
        if state.stopping {
            return None;
        }
        if let Some(frame) = state.queue.pop_front() {
            state.stats.pending_bytes = state
                .stats
                .pending_bytes
                .saturating_sub(frame.payload.len());
            state.stats.pending_frames = state.queue.len();
            return Some(frame);
        }
        state = shared.ready.wait(state).ok()?;
    }
}
