//! Safe host oracle entrypoint: all external paths are supplied by a test harness.
use openpilot_logging::producer::Factory;
use openpilot_params::Params;
use openpilot_timed::{
    clock::{Clock, SystemClock},
    runtime::{self, Host},
    timezone::{self, Internet, Paths, Services},
    wire::Gps,
    Error, Sudo,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    cell::RefCell,
    io::{self, BufRead},
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

#[derive(Deserialize)]
struct Config {
    params_root: PathBuf,
    paths: FixturePaths,
    endpoint: String,
    wall: u64,
    monotonic: Option<u64>,
    #[serde(default)]
    live: bool,
    cycles: Option<u64>,
    #[serde(default)]
    actions: Vec<Action>,
}
#[derive(Deserialize)]
struct FixturePaths {
    localtime: PathBuf,
    zoneinfo: PathBuf,
    systemd: PathBuf,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Action {
    Apply { zone: String, source: String },
    Gps { longitude: f64 },
    Internet,
    Valid,
    Bounds,
    Step { gps: Gps, monotonic: u64 },
    SetTime { epoch: f64 },
    CloseLogger,
}
struct FixtureClock {
    wall: u64,
    monotonic: RefCell<Option<u64>>,
    sleeps: RefCell<Vec<f64>>,
    live: bool,
}
impl Clock for FixtureClock {
    fn wall_nanos(&self) -> Result<u64, Error> {
        Ok(self.wall)
    }
    fn monotonic(&self) -> Result<u64, Error> {
        self.monotonic
            .borrow()
            .map_or_else(|| SystemClock.monotonic(), Ok)
    }
    fn local(&self, epoch: f64) -> Result<chrono::NaiveDateTime, Error> {
        SystemClock.local(epoch)
    }
    fn sleep(&self, duration: Duration) {
        self.sleeps.borrow_mut().push(duration.as_secs_f64());
        if self.live {
            SystemClock.sleep(duration);
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = io::stdin()
        .lock()
        .lines()
        .next()
        .ok_or("missing fixture configuration")??;
    let config: Config = serde_json::from_str(&input)?;
    // Reject accidental production destinations before any side effect.
    let root = std::fs::canonicalize(&config.params_root)?;
    if !root.starts_with(std::env::temp_dir())
        && !root.starts_with(std::env::current_dir()?.join(".omo/evidence"))
    {
        return Err("fixture Params must reside under temporary/evidence directory".into());
    }
    let params = Params::for_runtime()?;
    let paths = Paths {
        localtime: config.paths.localtime,
        zoneinfo: config.paths.zoneinfo,
        systemd: config.paths.systemd,
    };
    if !paths.localtime.starts_with(&root)
        || !paths.zoneinfo.starts_with(&root)
        || !paths.systemd.starts_with(&root)
    {
        return Err("fixture paths must be nested under fixture Params root".into());
    }
    if !config.endpoint.starts_with("http://127.0.0.1:") {
        return Err("fixture HTTP must be loopback".into());
    }
    let internet = Internet {
        endpoint: config.endpoint,
        ..Internet::default()
    };
    let clock = FixtureClock {
        wall: config.wall,
        monotonic: RefCell::new(config.monotonic),
        sleeps: RefCell::new(Vec::new()),
        live: config.live,
    };
    let mut logger = Factory::for_runtime()?.logger();
    let mut commands = Sudo;
    let mut host = Host {
        clock: &clock,
        services: Services {
            params: &params,
            logger: &mut logger,
            commands: &mut commands,
            paths: &paths,
        },
        internet: &internet,
    };
    if config.live {
        let stop = Arc::new(AtomicBool::new(false));
        signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
        runtime::run(&mut host, config.cycles, &stop)?;
    } else {
        let mut state = runtime::State::default();
        for action in config.actions {
            let result: Result<serde_json::Value, Error> = match action {
                Action::Apply { zone, source } => {
                    timezone::apply(&zone, &source, &mut host.services).map(|v| json!(v))
                }
                Action::Gps { longitude } => timezone::from_gps(longitude).map(|v| json!(v)),
                Action::Internet => Ok(json!(internet.lookup(&paths))),
                Action::Valid => {
                    openpilot_timed::clock::valid(&clock, &paths.systemd).map(|v| json!(v))
                }
                Action::Bounds => openpilot_timed::clock::bounds(&clock, &paths.systemd)
                    .map(|(a, b)| json!([a.to_string(), b.to_string()])),
                Action::Step { gps, monotonic } => {
                    *clock.monotonic.borrow_mut() = Some(monotonic);
                    state.step(gps, &mut host).map(|v| json!(v))
                }
                Action::SetTime { epoch } => {
                    openpilot_timed::set_time(epoch, &clock, &mut host.services)
                        .map(|()| json!(null))
                }
                Action::CloseLogger => {
                    host.services.logger.close();
                    Ok(json!(null))
                }
            };
            let result = match result {
                Ok(value) => json!({"ok": value}),
                Err(error) => json!({"error":error.to_string()}),
            };
            println!(
                "{}",
                json!({"result":result,"last_attempt":state.last_timezone_attempt,"sleeps":clock.sleeps.borrow().clone()})
            );
        }
    }
    Ok(())
}
