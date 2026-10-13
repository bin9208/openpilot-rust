use super::{Error, Frame, Samples};
use crate::h264::Control;
use std::{
    collections::VecDeque,
    io,
    sync::{mpsc::SyncSender, Arc, Condvar, Mutex, MutexGuard},
};
#[derive(Default)]
pub(super) struct State {
    pub pending: Option<Frame>,
    pub controls: VecDeque<(Control, SyncSender<io::Result<()>>)>,
    pub closing: bool,
    pub finished: bool,
    pub error: Option<Arc<io::Error>>,
    pub samples: Samples,
}
impl State {
    pub fn check(&self) -> Result<(), Error> {
        match &self.error {
            Some(error) => Err(Error::Failed(Arc::clone(error))),
            None => Ok(()),
        }
    }
}
pub(super) struct Shared {
    pub state: Mutex<State>,
    pub ready: Condvar,
}
impl Shared {
    pub fn lock(&self) -> Result<MutexGuard<'_, State>, Error> {
        self.state.lock().map_err(|_| Error::Poisoned)
    }
    pub fn failure(&self, error: io::Error) {
        if let Ok(mut state) = self.state.lock() {
            state.error = Some(Arc::new(error));
        }
        self.ready.notify_all();
    }
    pub fn finish(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.finished = true;
            let failure = state.error.clone();
            for (_, reply) in state.controls.drain(..) {
                let error = failure.clone().map_or_else(
                    || io::Error::other("JPEG worker has closed"),
                    io::Error::other,
                );
                let _ = reply.send(Err(error));
            }
        }
        self.ready.notify_all();
    }
}
