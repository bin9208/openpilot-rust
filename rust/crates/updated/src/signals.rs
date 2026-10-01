use crate::Error;
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRequest {
    #[default]
    None,
    Check,
    Fetch,
}
#[derive(Default)]
struct State {
    ready: bool,
    stopped: bool,
    request: UserRequest,
}
#[derive(Default)]
pub struct Wake {
    state: Mutex<State>,
    changed: Condvar,
}
impl Wake {
    pub fn request(&self) -> Result<UserRequest, Error> {
        Ok(self
            .state
            .lock()
            .map_err(|_| Error::Contract("signal state poisoned"))?
            .request)
    }
    pub fn stopped(&self) -> bool {
        self.state.lock().map_or(true, |state| state.stopped)
    }
    pub fn clear_ready(&self) -> Result<(), Error> {
        self.state
            .lock()
            .map_err(|_| Error::Contract("signal state poisoned"))?
            .ready = false;
        Ok(())
    }
    pub fn clear_request(&self) -> Result<(), Error> {
        self.state
            .lock()
            .map_err(|_| Error::Contract("signal state poisoned"))?
            .request = UserRequest::None;
        Ok(())
    }
    pub fn send(&self, request: UserRequest) -> Result<(), Error> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::Contract("signal state poisoned"))?;
        state.request = request;
        state.ready = true;
        self.changed.notify_all();
        Ok(())
    }
    pub fn stop(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.stopped = true;
            self.changed.notify_all();
        }
    }
    pub fn sleep(&self, duration: Duration) -> Result<(), Error> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::Contract("signal state poisoned"))?;
        drop(
            self.changed
                .wait_timeout_while(state, duration, |s| !s.ready && !s.stopped)
                .map_err(|_| Error::Contract("signal state poisoned"))?,
        );
        Ok(())
    }
}
pub struct Signals {
    handle: signal_hook::iterator::Handle,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Signals {
    pub fn install(wake: Arc<Wake>, factory: Factory) -> Result<Self, Error> {
        use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM, SIGUSR1};
        let mut signals = signal_hook::iterator::Signals::new([SIGHUP, SIGUSR1, SIGINT, SIGTERM])?;
        let handle = signals.handle();
        let thread = std::thread::spawn(move || {
            let mut logger = factory.logger();
            for signal in signals.forever() {
                let request = match signal {
                    SIGHUP => Some((
                        UserRequest::Fetch,
                        "caught SIGHUP, attempting to downloading update",
                    )),
                    SIGUSR1 => Some((UserRequest::Check, "caught SIGUSR1, checking for updates")),
                    SIGINT | SIGTERM => {
                        wake.stop();
                        None
                    }
                    _ => None,
                };
                if let Some((request, message)) = request {
                    if let Err(error) =
                        logger.emit(log_site!(), Record::text(Level::Info, message.into()))
                    {
                        eprintln!("updated signal log: {error}");
                    }
                    if let Err(error) = wake.send(request) {
                        eprintln!("updated signal: {error}");
                        wake.stop();
                    }
                }
            }
        });
        Ok(Self {
            handle,
            thread: Some(thread),
        })
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        self.handle.close();
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                eprintln!("updated signal worker panicked");
            }
        }
    }
}
