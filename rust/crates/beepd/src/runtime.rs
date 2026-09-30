use crate::{Clock, Driver, Error, Output};
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::{runtime::SubMaster, state::Options};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

#[derive(Default)]
pub struct Ratekeeper {
    next: Option<f64>,
    last_monitor: f64,
    pub frame: u64,
    pub remaining: f64,
}
impl Ratekeeper {
    pub fn keep_time(
        &mut self,
        clock: &dyn Clock,
        output: &dyn Output,
        name: &str,
    ) -> Result<(), Error> {
        let next = match self.next {
            Some(next) => next,
            None => {
                let next = clock.monotonic()? + 0.05;
                self.last_monitor = clock.monotonic()?;
                next
            }
        };
        self.last_monitor = clock.monotonic()?;
        self.remaining = next - clock.monotonic()?;
        self.next = Some(next + 0.05);
        if self.remaining < 0.0 {
            output.line(&format!(
                "{name} lagging by {:.2} ms",
                -self.remaining * 1000.0
            ))?;
        }
        self.frame += 1;
        if self.remaining > 0.0 {
            clock.sleep(self.remaining)?;
        }
        Ok(())
    }
}

pub fn run(
    mut driver: Driver,
    stop: &AtomicBool,
    cycles: Option<u64>,
    name: &str,
) -> Result<(), Error> {
    driver.startup()?;
    let mut subscriber = SubMaster::for_runtime(&["selfdriveState"], Options::default())?;
    let mut ratekeeper = Ratekeeper::default();
    while !stop.load(Ordering::Relaxed) {
        subscriber.update(Duration::ZERO)?;
        let topic = subscriber.state.topic("selfdriveState")?;
        if topic.updated {
            let message = match topic
                .event()?
                .which()
                .map_err(|_| Error::Contract("invalid selfdriveState union"))?
            {
                event::Which::SelfdriveState(message) => message?,
                _ => return Err(Error::Contract("unexpected selfdriveState event")),
            };
            let alert = match message.get_alert_sound() {
                Ok(alert) => u16::from(alert),
                Err(capnp::NotInSchema(raw)) => raw,
            };
            driver.update_alert(alert)?;
        }
        ratekeeper.keep_time(driver.shared.clock.as_ref(), driver.output.as_ref(), name)?;
        if cycles.is_some_and(|limit| ratekeeper.frame >= limit) {
            break;
        }
    }
    // Alert workers are daemon threads: do not join, cancel, or synthesize an off pulse.
    Ok(())
}

pub fn supervise(
    driver: Driver,
    stop: Arc<AtomicBool>,
    cycles: Option<u64>,
    name: String,
) -> Result<(), Error> {
    let (sender, receiver) = mpsc::channel();
    let worker_stop = Arc::clone(&stop);
    let _worker = thread::Builder::new()
        .name("beep-main".into())
        .spawn(move || {
            if sender
                .send(run(driver, &worker_stop, cycles, &name))
                .is_err()
                && !worker_stop.load(Ordering::Relaxed)
            {
                eprintln!("beepd: runtime receiver closed unexpectedly");
            }
        })?;
    loop {
        if stop.load(Ordering::Relaxed) {
            // Main process exit terminates all daemon threads, including a blocked
            // startup/command wait, matching the manager's bounded signal stop.
            return Ok(());
        }
        match receiver.recv_timeout(Duration::from_millis(20)) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(Error::Contract("beep runtime worker terminated"))
            }
        }
    }
}
