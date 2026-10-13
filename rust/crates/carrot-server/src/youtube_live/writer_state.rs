use std::{
    collections::VecDeque,
    sync::{Condvar, Mutex},
};
use tokio::sync::watch;

pub(super) const FRAMES: usize = 30;
pub(super) const BYTES: usize = 4 * 1024 * 1024;
pub(super) struct Frame {
    pub payload: Vec<u8>,
    pub keyframe: bool,
}
#[derive(Clone, Default)]
pub(super) struct Stats {
    pub pending_frames: usize,
    pub pending_bytes: usize,
    pub high_frames: usize,
    pub high_bytes: usize,
    pub frames_written: u64,
    pub last_write_ms: u64,
    pub max_write_ms: u64,
    pub error: String,
    pub rejection: String,
}
#[derive(Default)]
pub(super) struct State {
    pub queue: VecDeque<Frame>,
    pub stats: Stats,
    pub running: bool,
    pub stopping: bool,
}
pub(super) struct Shared {
    pub state: Mutex<State>,
    pub ready: Condvar,
    pub done: watch::Sender<bool>,
    pub sink: std::sync::Arc<Mutex<super::sink::Snapshot>>,
}
pub(super) struct Completion(pub std::sync::Arc<Shared>);
impl Drop for Completion {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.running = false;
            if std::thread::panicking() {
                state.stats.error = "RTMP writer panicked".into();
            }
        }
        self.0.done.send_replace(true);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panicking_worker_notifies_before_join() {
        let (done, mut completed) = watch::channel(false);
        let shared = std::sync::Arc::new(Shared {
            state: Mutex::new(State::default()),
            ready: Condvar::new(),
            done,
            sink: std::sync::Arc::new(Mutex::new(super::super::sink::Snapshot::default())),
        });
        let owned = std::sync::Arc::clone(&shared);
        let worker = std::thread::spawn(move || {
            let _completion = Completion(owned);
            panic!("owned writer cleanup control");
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !*completed.borrow_and_update() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(*completed.borrow());
        assert!(worker.join().is_err());
        assert_eq!(
            shared.state.lock().unwrap().stats.error,
            "RTMP writer panicked"
        );
    }
}
