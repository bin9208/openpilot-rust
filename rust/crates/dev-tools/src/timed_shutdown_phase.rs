//! Original GPS shutdown phase observer, independent of the timed candidate.
use crate::seal::Result;
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    process::Child,
    time::{Duration, Instant},
};

pub fn wait_original_gps_sleep(process: &mut Child, output: &Path) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let proc = std::path::PathBuf::from("/proc").join(process.id().to_string());
    let mut observations: Vec<Value> = vec![];
    loop {
        if let Some(status) = process.try_wait()? {
            return Err(format!("source exited before GPS sleep: {status}").into());
        }
        let state = json!({"wchan":fs::read_to_string(proc.join("wchan"))?.trim(),
                           "syscall":fs::read_to_string(proc.join("syscall"))?.trim()});
        if observations.last().is_none_or(|row| row["state"] != state) {
            let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
            observations
                .push(json!({"at":now.tv_sec as f64 + now.tv_nsec as f64 / 1e9,"state":state}));
            fs::write(
                output.join("sleep-readiness.json"),
                format!("{}\n", serde_json::to_string_pretty(&observations)?),
            )?;
        }
        if state["wchan"] == "hrtimer_nanosleep" {
            return Ok(state);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "source did not enter GPS sleep: {}",
                observations.last().unwrap()
            )
            .into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
