use openpilot_hardware_info::HardwareInfo;
use openpilot_logging::producer::Factory;
use openpilot_timed::{
    clock::{Clock, SystemClock},
    Error as ClockError,
};
use openpilot_updated::{
    agnos::Agnos,
    common, markdown,
    paths::Paths,
    process::{Commands, NativeCommands},
    runtime::LoopState,
    signals::{UserRequest, Wake},
    updater::{Context, Updater},
    Error, Params,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    cell::Cell,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
#[derive(Deserialize)]
struct Config {
    paths: Paths,
    launcher: PathBuf,
    now: f64,
    device: String,
    os_version: Option<String>,
    agnos: bool,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    request: Option<UserRequest>,
    #[serde(default)]
    params: std::collections::BTreeMap<String, String>,
    now: Option<f64>,
    #[serde(default)]
    has_internet: Option<bool>,
    #[serde(default)]
    report_failure: Option<u64>,
}
struct FixtureClock(Cell<u64>);
fn fixture_nanos(seconds: f64) -> Result<u64, Box<dyn std::error::Error>> {
    Ok(u64::try_from(
        openpilot_timed::clock::datetime(seconds)?
            .timestamp_nanos_opt()
            .ok_or("fixture clock range")?,
    )?)
}
impl Clock for FixtureClock {
    fn wall_nanos(&self) -> Result<u64, ClockError> {
        Ok(self.0.get())
    }
    fn monotonic(&self) -> Result<u64, ClockError> {
        Ok(self.0.get())
    }
    fn local(&self, epoch: f64) -> Result<chrono::NaiveDateTime, ClockError> {
        SystemClock.local(epoch)
    }
    fn sleep(&self, _duration: Duration, _stop: &AtomicBool) {}
}
struct Hardware {
    device: String,
    version: Option<String>,
}
impl HardwareInfo for Hardware {
    fn get_device_type(&self) -> Result<String, openpilot_hardware_info::Error> {
        Ok(self.device.clone())
    }
    fn get_os_version(&self) -> Result<Option<String>, openpilot_hardware_info::Error> {
        Ok(self.version.clone())
    }
}
struct RecordedCommands {
    inner: NativeCommands,
    calls: Vec<Value>,
}
impl Commands for RecordedCommands {
    fn run(&mut self, argv: &[String], cwd: Option<&Path>) -> Result<String, Error> {
        self.calls.push(json!({"argv":argv,"cwd":cwd}));
        self.inner.run(argv, cwd)
    }
}
struct Flash {
    calls: Vec<Value>,
}
impl Agnos for Flash {
    fn get_target_slot_number(&mut self) -> Result<u32, Error> {
        self.calls.push(json!({"kind":"slot","result":1}));
        Ok(1)
    }
    fn flash_agnos_update(&mut self, manifest: &Path, slot: u32) -> Result<(), Error> {
        self.calls
            .push(json!({"kind":"flash","manifest":manifest,"slot":slot}));
        if !manifest.is_file() || slot != 1 {
            return Err(Error::Contract("fixture AGNOS contract"));
        }
        Ok(())
    }
}
fn snapshot(updater: &Updater<'_>, wait: Option<u64>) -> Result<Value, Error> {
    let root = updater.context.paths.system_root.join("data/params/d");
    let mut values = serde_json::Map::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            values.insert(
                entry.file_name().to_string_lossy().into_owned(),
                json!(std::fs::read(entry.path())?
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()),
            );
        }
    }
    Ok(
        json!({"wait":wait,"params":values,"branches":updater.branches,"has_internet":updater.has_internet,"consistent":common::get_consistent_flag(&updater.context.paths.finalized())}),
    )
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let mut raw: Value = serde_json::from_str(&text)?;
    if raw.get("markdown").is_some() {
        println!(
            "{}",
            json!(markdown::parse(
                raw["markdown"].as_str().ok_or("markdown text")?,
                2
            )?)
        );
        return Ok(());
    }
    let config: Config = serde_json::from_value(raw.take())?;
    let params = Params::open(&config.paths.system_root)?;
    let wake = Arc::new(Wake::default());
    let mut commands = RecordedCommands {
        inner: NativeCommands {
            launcher: config.launcher,
            wake: Arc::clone(&wake),
        },
        calls: Vec::new(),
    };
    let mut logger = Factory::new(format!(
        "ipc://{}",
        config.paths.system_root.join("logs.sock").display()
    ))?
    .logger();
    let hardware = Hardware {
        device: config.device,
        version: config.os_version,
    };
    let mut flash = Flash { calls: Vec::new() };
    let clock = FixtureClock(Cell::new(fixture_nanos(config.now)?));
    let mut updater = Updater::new(
        params,
        Context {
            paths: &config.paths,
            commands: &mut commands,
            hardware: &hardware,
            agnos: &mut flash,
            clock: &clock,
            logger: &mut logger,
            is_agnos: config.agnos,
        },
    );
    if updater.params.date("InstallDate")?.is_none() {
        updater
            .params
            .put_date("InstallDate", updater.context.now()?)?;
    }
    common::set_consistent_flag(&config.paths, &config.paths.finalized(), false)?;
    updater.params.put("UpdaterState", b"idle")?;
    let mut state = LoopState::default();
    let mut results = Vec::new();
    for step in config.steps {
        if let Some(now) = step.now {
            clock.0.set(fixture_nanos(now)?);
        }
        for (key, value) in step.params {
            updater.params.put(&key, value.as_bytes())?;
        }
        if let Some(request) = step.request {
            wake.send(request)?;
        }
        if let Some(internet) = step.has_internet {
            updater.has_internet = internet;
        }
        let wait = if let Some(failures) = step.report_failure {
            updater.set_params(false, failures, Some("fixture failure"))?;
            None
        } else {
            Some(state.cycle(&mut updater, &wake)?.wait_seconds)
        };
        results.push(snapshot(&updater, wait)?);
    }
    drop(updater);
    println!(
        "{}",
        json!({"snapshots":results,"commands":commands.calls,"agnos":flash.calls})
    );
    Ok(())
}
