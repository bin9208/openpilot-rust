//! Workers own all referenced data, including when the source one-second stop deadline expires.
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
#[derive(Default)]
pub struct Gate {
    started: AtomicBool,
    awake: AtomicBool,
}
impl Gate {
    pub fn update(&self, started: bool, awake: bool) {
        self.started.store(started, Ordering::Relaxed);
        self.awake.store(awake, Ordering::Relaxed);
    }
    pub fn enabled(&self) -> bool {
        !self.started.load(Ordering::Relaxed) && self.awake.load(Ordering::Relaxed)
    }
}
#[derive(Default)]
struct Stop {
    flag: Mutex<bool>,
    changed: Condvar,
}
pub struct Poller {
    stop: Arc<Stop>,
    join: Option<JoinHandle<()>>,
}
impl Poller {
    pub fn start(
        gate: Arc<Gate>,
        interval: Duration,
        mut fetch: impl FnMut() + Send + 'static,
    ) -> Result<Self, std::io::Error> {
        let stop = Arc::new(Stop::default());
        let task_stop = stop.clone();
        let join = thread::Builder::new()
            .name("ui-api-poll".into())
            .spawn(move || loop {
                if *task_stop
                    .flag
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                {
                    break;
                }
                if gate.enabled() {
                    fetch();
                }
                let guard = task_stop
                    .flag
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                let (guard, _) = task_stop
                    .changed
                    .wait_timeout_while(guard, interval, |stop| !*stop)
                    .unwrap_or_else(|error| error.into_inner());
                if *guard {
                    break;
                }
            })?;
        Ok(Self {
            stop,
            join: Some(join),
        })
    }
    pub fn stop(&mut self) {
        *self
            .stop
            .flag
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = true;
        self.stop.changed.notify_all();
        let deadline = Instant::now() + Duration::from_secs(1);
        if let Some(join) = self.join.take() {
            while !join.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            if join.is_finished() && join.join().is_err() {
                openpilot_startup_ui::logging::emit(
                    openpilot_logging::record::Level::Error,
                    "UI API worker panicked".into(),
                );
            }
        }
    }
}
impl Drop for Poller {
    fn drop(&mut self) {
        self.stop();
    }
}
