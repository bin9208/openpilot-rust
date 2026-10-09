use crate::Error;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
#[derive(Default)]
struct State {
    active: usize,
    quiescing: bool,
}
pub(super) struct Counter {
    state: Mutex<State>,
    changed: watch::Sender<()>,
}
pub(super) struct Admission(Arc<Counter>);
impl Counter {
    pub fn new() -> Arc<Self> {
        let (changed, _) = watch::channel(());
        Arc::new(Self {
            state: Mutex::new(State::default()),
            changed,
        })
    }
    pub fn admit(self: &Arc<Self>) -> Result<Admission, Error> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::Source("Tools admission poisoned".into()))?;
        if state.quiescing {
            return Err(Error::Source("Tools service unavailable".into()));
        }
        state.active = state.active.saturating_add(1);
        Ok(Admission(Arc::clone(self)))
    }
    pub fn quiesce(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.quiescing = true;
        }
    }
    pub fn idle(&self) -> bool {
        self.state.lock().is_ok_and(|state| state.active == 0)
    }
    pub fn changed(&self) -> watch::Receiver<()> {
        self.changed.subscribe()
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.active = state.active.saturating_sub(1);
        }
        self.0.changed.send_replace(());
    }
}
