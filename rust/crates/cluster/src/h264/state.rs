use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Mutex, MutexGuard},
};

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub bytes_sent: usize,
    pub chunks_sent: usize,
    pub stderr_tail: VecDeque<String>,
    pub error: Option<String>,
    pub closing: bool,
    pub(super) child_closed: bool,
    pub(super) monitor: Option<super::observability::Monitor>,
}

pub(super) type Shared = Arc<Mutex<Snapshot>>;

pub(super) fn lock(state: &Shared) -> io::Result<MutexGuard<'_, Snapshot>> {
    state
        .lock()
        .map_err(|_| io::Error::other("H264 worker state poisoned"))
}

pub(super) fn failure(state: &Shared, error: &io::Error) {
    if let Ok(mut state) = lock(state) {
        if !state.closing && state.error.is_none() {
            state.error = Some(error.to_string());
        }
    }
}
