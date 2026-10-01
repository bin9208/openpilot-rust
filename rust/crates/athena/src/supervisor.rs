use crate::{
    logging,
    state::{self, Shared, Stop},
    Error,
};
use openpilot_logging::record::{Level, Record};
use std::{
    os::unix::process::ExitStatusExt,
    process::{Child, Command},
    time::Duration,
};

struct ChildOwner(Child);
impl Drop for ChildOwner {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        let pid = i32::try_from(self.0.id())
            .ok()
            .and_then(rustix::process::Pid::from_raw);
        if let Some(pid) = pid {
            if let Err(error) = rustix::process::kill_process(pid, rustix::process::Signal::INT) {
                eprintln!("athena supervisor: signal: {error}");
            }
        }
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(35) {
            if matches!(self.0.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if let Err(error) = self.0.kill() {
            eprintln!("athena supervisor: kill: {error}");
        }
        if let Err(error) = self.0.wait() {
            eprintln!("athena supervisor: reap: {error}");
        }
    }
}
pub fn run(shared: &Shared, stop: &Stop) -> Result<(), Error> {
    logging::bind(shared)?;
    let mut logger = shared.factory.logger();
    let context = logger.context_snapshot()?.to_json()?;
    let result = (|| {
        while !stop.requested() {
            logger.emit(
                openpilot_logging::log_site!(),
                Record::text(Level::Info, "starting athena daemon".into()),
            )?;
            let mut child = ChildOwner(
                Command::new(std::env::current_exe()?.with_file_name("openpilot-athenad"))
                    .env("ATHENA_LOG_CONTEXT", &context)
                    .spawn()?,
            );
            loop {
                if let Some(status) = child.0.try_wait()? {
                    logging::event(
                        &mut logger,
                        "athenad exited",
                        serde_json::json!({"exitcode":status.code().unwrap_or_else(||-status.signal().unwrap_or(0))}),
                    )?;
                    break;
                }
                if stop.requested() {
                    break;
                }
                stop.wait(Duration::from_millis(20));
            }
            drop(child);
            stop.wait(Duration::from_secs(5));
        }
        Ok::<(), Error>(())
    })();
    if let Err(error) = result {
        logging::failure(&mut logger, "manage_athenad.exception", &error);
    }
    state::remove(&shared.params, "AthenadPid")?;
    Ok(())
}
