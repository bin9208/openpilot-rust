//! External-I/O fixture: real Params, original daemon code, recorded hardware/clock calls.
use openpilot_beepd::{integer, runtime::Ratekeeper, Clock, Commands, Driver, Error, Stdout};
use openpilot_params::Params;
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, BufRead, Write},
    os::unix::process::ExitStatusExt,
    path::PathBuf,
    process::ExitStatus,
    sync::{Arc, Mutex},
};

#[derive(Deserialize)]
struct Config {
    mode: String,
    root: PathBuf,
    trace: PathBuf,
    #[serde(default)]
    bytes: Vec<u8>,
    #[serde(default)]
    actions: Vec<Action>,
    #[serde(default)]
    command_fail: Vec<usize>,
    #[serde(default)]
    status: i32,
    #[serde(default)]
    sleep_fail: Vec<usize>,
    #[serde(default)]
    mutations: BTreeMap<usize, Vec<u8>>,
    #[serde(default)]
    times: Vec<f64>,
    #[serde(default)]
    count: u64,
    #[serde(default)]
    timespecs: Vec<(i64, i64)>,
    #[serde(default)]
    sleep_bits: Vec<u64>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Action {
    Alert { value: u16 },
    Volume { bytes: Option<Vec<u8>> },
}
#[derive(Default)]
struct State {
    commands: usize,
    sleeps: usize,
    time_index: usize,
}
struct Fixture {
    config: Config,
    state: Mutex<State>,
    file: Mutex<File>,
    params: Arc<Params>,
}
impl Fixture {
    fn record(&self, value: serde_json::Value) -> io::Result<()> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| io::Error::other("trace lock poisoned"))?;
        writeln!(file, "{value}")?;
        file.flush()
    }
}
impl Commands for Fixture {
    fn run(&self, command: &str) -> io::Result<ExitStatus> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("state lock poisoned"))?;
        let failure = self.config.command_fail.contains(&state.commands);
        state.commands += 1;
        self.record(json!({"kind":"command","command":command,"failed":failure,"status":self.config.status}))?;
        if failure {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "injected command spawn failure",
            ))
        } else {
            Ok(ExitStatus::from_raw(self.config.status << 8))
        }
    }
}
impl Clock for Fixture {
    fn monotonic(&self) -> Result<f64, Error> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("fixture state lock poisoned"))?;
        let value = *self
            .config
            .times
            .get(state.time_index)
            .ok_or(Error::Contract("fixture clock exhausted"))?;
        state.time_index += 1;
        Ok(value)
    }
    fn sleep(&self, seconds: f64) -> Result<(), Error> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("state lock poisoned"))?;
        let index = state.sleeps;
        state.sleeps += 1;
        self.record(json!({"kind":"sleep","seconds":seconds}))?;
        if let Some(bytes) = self.config.mutations.get(&index) {
            self.params.put("SoundVolumeAdjust", bytes)?;
        }
        if self.config.sleep_fail.contains(&index) {
            return Err(io::Error::other("injected clock sleep failure").into());
        }
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = io::stdin()
        .lock()
        .lines()
        .next()
        .ok_or("missing config")??;
    let config: Config = serde_json::from_str(&input)?;
    if config.mode == "sleep" {
        for bits in config.sleep_bits {
            let result = match openpilot_beepd::sleep_duration(f64::from_bits(bits)) {
                Ok(duration) => json!({"nanoseconds":duration.as_nanos().to_string()}),
                Err(error) => json!({"error":error.to_string()}),
            };
            println!("{result}");
        }
        return Ok(());
    }
    if config.mode == "clock" {
        for (tv_sec, tv_nsec) in config.timespecs {
            println!(
                "{}",
                openpilot_beepd::monotonic_seconds(rustix::time::Timespec { tv_sec, tv_nsec })?
                    .to_bits()
            );
        }
        return Ok(());
    }
    if config.mode == "integer" {
        let result = match integer(&config.bytes) {
            Ok(value) => json!({"value":value}),
            Err(error) => json!({"error":format!("{error:?}")}),
        };
        println!("{result}");
        return Ok(());
    }
    let root = fs::canonicalize(&config.root)?;
    if !root.starts_with(std::env::temp_dir())
        || std::env::var_os("PARAMS_ROOT").map(PathBuf::from) != Some(root)
    {
        return Err("fixture Params root must be the isolated temporary PARAMS_ROOT".into());
    }
    if config.mode == "rate" && u64::try_from(config.times.len())? != config.count * 2 + 2 {
        return Err("invalid fixture clock read count".into());
    }
    let params = Arc::new(Params::for_runtime()?);
    let file = File::create(&config.trace)?;
    let fixture = Arc::new(Fixture {
        config,
        state: Mutex::new(State::default()),
        file: Mutex::new(file),
        params: Arc::clone(&params),
    });
    if fixture.config.mode == "rate" {
        let mut ratekeeper = Ratekeeper::default();
        for _ in 0..fixture.config.count {
            ratekeeper.keep_time(fixture.as_ref(), &Stdout, "beep")?;
            fixture.record(
                json!({"kind":"rate","frame":ratekeeper.frame,"remaining":ratekeeper.remaining}),
            )?;
        }
        return Ok(());
    }
    let mut driver = Driver::new(params, fixture.clone(), fixture.clone(), Arc::new(Stdout));
    driver.startup()?;
    for action in &fixture.config.actions {
        match action {
            Action::Alert { value } => {
                driver.update_alert(*value)?;
                driver.wait_workers();
            }
            Action::Volume { bytes: Some(bytes) } => {
                fixture.params.put("SoundVolumeAdjust", bytes)?
            }
            Action::Volume { bytes: None } => fixture.params.remove("SoundVolumeAdjust")?,
        }
    }
    Ok(())
}
