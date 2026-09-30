//! Deterministic clocks and paths around the actual continuous daemon.
use openpilot_statsd::{
    daemon::{emit_event, run, run_with_events, Clock, Configuration},
    Error,
};
use std::{
    io::{self, Write},
    path::PathBuf,
    sync::atomic::AtomicBool,
};
struct PipeClock;
impl Clock for PipeClock {
    fn monotonic(&mut self) -> Result<f64, Error> {
        println!("clock");
        io::stdout().flush()?;
        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            return Err(Error::Configuration("oracle EOF"));
        }
        line.trim()
            .parse()
            .map_err(|_| Error::Configuration("oracle clock"))
    }
    fn timestamp_ns(&mut self) -> Result<i128, Error> {
        Ok(1_723_456_789_123_456_000)
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let configuration = Configuration {
        endpoint: args.next().ok_or("endpoint")?,
        directory: PathBuf::from(args.next().ok_or("directory")?),
        source_root: PathBuf::from(args.next().ok_or("source root")?),
        device_type: Some("pc".into()),
    };
    let result = match std::env::var_os("STATS_ORACLE_LOG_FAILURE_TRACE") {
        None => run(&configuration, &mut PipeClock, &AtomicBool::new(false)),
        Some(path) => {
            let mut attempts = Vec::new();
            run_with_events(
                &configuration,
                &mut PipeClock,
                &AtomicBool::new(false),
                |logger, name, field| {
                    attempts.push(name.to_owned());
                    std::fs::write(
                        &path,
                        serde_json::to_vec(&attempts)
                            .map_err(|_| Error::Configuration("oracle trace"))?,
                    )?;
                    if attempts.len() == 1 {
                        return Err(Error::Zmq(zmq::Error::EINVAL));
                    }
                    emit_event(logger, name, field)
                },
            )
        }
    };
    match result {
        Err(Error::Configuration("oracle EOF")) => Ok(()),
        result => Ok(result?),
    }
}
