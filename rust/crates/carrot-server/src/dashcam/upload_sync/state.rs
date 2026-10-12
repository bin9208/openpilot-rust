use crate::Error;
use tokio::sync::watch;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Running,
    Quiescing,
    Force,
}
#[derive(Clone, Copy)]
pub struct SyncState {
    pub(super) active: usize,
    pub(super) phase: Phase,
}
pub(crate) struct Admission(watch::Sender<SyncState>);
impl Admission {
    pub(super) fn begin(state: &watch::Sender<SyncState>) -> Result<Self, Error> {
        let mut accepted = false;
        state.send_if_modified(|state| {
            if state.phase != Phase::Running {
                return false;
            }
            let Some(active) = state.active.checked_add(1) else {
                return false;
            };
            state.active = active;
            accepted = true;
            true
        });
        if accepted {
            Ok(Self(state.clone()))
        } else {
            Err(Error::Source("dashcam sync upload service stopped".into()))
        }
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        self.0.send_modify(|state| {
            state.active = state.active.saturating_sub(1);
        });
    }
}
pub(super) fn channel() -> watch::Sender<SyncState> {
    watch::channel(SyncState {
        active: 0,
        phase: Phase::Running,
    })
    .0
}
pub(super) async fn forced(stopped: &mut watch::Receiver<SyncState>) {
    loop {
        let force = stopped.borrow().phase == Phase::Force;
        if force || stopped.changed().await.is_err() {
            return;
        }
    }
}
