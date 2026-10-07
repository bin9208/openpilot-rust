use crate::{receiver::Receiver, Error};
use std::sync::{Arc, Condvar, Mutex};
use tokio::sync::watch;

#[derive(Clone)]
pub struct Shared {
    state: Arc<(Mutex<Receiver>, Condvar)>,
    pub closing: watch::Sender<Close>,
    pub tasks: Arc<Mutex<tokio::task::JoinSet<()>>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Close {
    Running,
    MapChanged,
    Shutdown,
}

impl Shared {
    pub fn new(receiver: Receiver) -> Self {
        let (closing, _) = watch::channel(Close::Running);
        Self {
            state: Arc::new((Mutex::new(receiver), Condvar::new())),
            closing,
            tasks: Arc::new(Mutex::new(tokio::task::JoinSet::new())),
        }
    }
    pub fn with<T>(&self, action: impl FnOnce(&mut Receiver) -> T) -> Result<T, Error> {
        let mut receiver = self
            .state
            .0
            .lock()
            .map_err(|_| Error::typed("RuntimeError", "receiver lock poisoned".into()))?;
        let result = action(&mut receiver);
        self.state.1.notify_all();
        Ok(result)
    }
    pub fn wait(&self, seconds: f64) -> Result<(), Error> {
        let receiver = self
            .state
            .0
            .lock()
            .map_err(|_| Error::typed("RuntimeError", "receiver lock poisoned".into()))?;
        let (mut receiver, _) = self
            .state
            .1
            .wait_timeout_while(
                receiver,
                std::time::Duration::from_secs_f64(seconds.max(0.)),
                |receiver| !receiver.take_state_changed(),
            )
            .map_err(|_| Error::typed("RuntimeError", "receiver lock poisoned".into()))?;
        receiver.take_state_changed();
        Ok(())
    }
    pub fn wake(&self) {
        self.state.1.notify_all();
    }
    pub fn spawn(
        &self,
        future: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(), Error> {
        self.tasks
            .lock()
            .map_err(|_| Error::typed("RuntimeError", "task lock poisoned".into()))?
            .spawn(future);
        Ok(())
    }
}
