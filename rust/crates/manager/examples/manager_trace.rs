#[path = "trace/parameters.rs"]
mod parameters_fixture;
use parameters_fixture::TracedParams;
#[path = "trace/startup.rs"]
mod startup;
use openpilot_manager::{
    initialization::InitPaths,
    lifecycle::{Environment, ExitAction, Input, Runtime},
    parameters::Parameters,
    Error,
};
use openpilot_process_supervision::ProcessState;
use serde_json::{json, Value};
use std::{cell::RefCell, path::PathBuf, rc::Rc};
type Trace = Rc<RefCell<Vec<Value>>>;
struct Fixture {
    trace: Trace,
    scenario: String,
    params: Rc<TracedParams>,
    frame: usize,
}
impl Fixture {
    fn record(&self, value: Value) {
        self.trace.borrow_mut().push(value);
    }
}
impl Runtime for Fixture {
    fn start(&mut self) -> Result<(), Error> {
        if self.params.capture_logging {
            openpilot_manager::diagnostics::start(&mut self.params.logger.borrow_mut())?;
            self.record(json!(["lifecycle", "start"]));
        }
        Ok(())
    }
    fn cleanup_finished(&mut self) -> Result<(), Error> {
        if self.params.capture_logging {
            openpilot_manager::diagnostics::cleanup_finished(&mut self.params.logger.borrow_mut())?;
            self.record(json!(["lifecycle", "cleanup_finished"]));
        }
        Ok(())
    }

    fn poll(&mut self) -> Result<Input, Error> {
        self.record(json!(["poll", 1000]));
        if self.scenario == "poll_failure" {
            return Err(Error::Contract("fixture poll failure"));
        }
        if self.scenario == "interrupted" {
            return Err(Error::Interrupted);
        }
        self.frame += 1;
        if self.frame == 3 {
            for key in ["DoUninstall", "DoShutdown", "DoReboot"] {
                self.params.put_bool(key, true)?;
            }
        }
        Ok(Input {
            started: self.frame < 3,
            ignition: self.frame != 2,
            not_car: false,
            device_checks: true,
        })
    }
    fn initial_not_car(&self) -> Result<bool, Error> {
        Ok(false)
    }
    fn ensure_running(
        &mut self,
        started: bool,
        not_car: bool,
        ignore: &[String],
    ) -> Result<(), Error> {
        self.record(json!(["ensure", started, not_car, ignore]));
        Ok(())
    }
    fn states(&mut self) -> Result<Vec<ProcessState>, Error> {
        Ok(vec![])
    }
    fn publish(&mut self, _: &[ProcessState]) -> Result<(), Error> {
        self.record(json!(["publish", false]));
        Ok(())
    }
    fn watchdog(&mut self) -> Result<(), Error> {
        self.record(json!(["watchdog"]));
        Err(Error::Contract("ignored watchdog failure"))
    }
    fn timestamp(&mut self) -> String {
        "2026-10-01 12:00:00".into()
    }
    fn log_running(&mut self, _: &[ProcessState], _: bool) -> Result<(), Error> {
        Ok(())
    }
    fn warning(&mut self, message: &str) -> Result<(), Error> {
        self.record(json!(["warning", message]));
        Ok(())
    }
    fn stop(&mut self, block: bool) -> Result<(), Error> {
        self.record(json!(["stop", block]));
        Ok(())
    }
    fn capture_exception(&mut self, _: &Error) -> Result<(), Error> {
        self.record(json!(["capture"]));
        Ok(())
    }
    fn exit(&mut self, action: ExitAction) -> Result<(), Error> {
        self.record(json!(["exit", format!("{action:?}").to_lowercase()]));
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let directory = PathBuf::from(&args[1]);
    std::fs::create_dir_all(&directory)?;
    let trace = Rc::new(RefCell::new(Vec::new()));
    let params = Rc::new(TracedParams {
        inner: openpilot_params::Params::open(&directory.join("params"), "d")?,
        trace: trace.clone(),
        logger: RefCell::new(if args[2] == "logging" {
            openpilot_logging::producer::Factory::for_runtime()?.logger()
        } else {
            openpilot_logging::producer::Factory::new("inproc://manager-trace-isolated".into())?
                .logger()
        }),
        capture_logging: args[2] == "logging",
    });
    for (key, value) in [
        ("RecordFrontLock", b"1".as_slice()),
        ("UseWideCamera", b"0"),
        ("HardwareC3xLite", b"1"),
        ("RecordAudio", b"1"),
        ("CarParams", b"stale"),
        ("GitCommit", b"old"),
    ] {
        params.inner.put(key, value)?;
    }
    if args[2] == "logging" {
        params.inner.put("UptimeOnroad", b"1__2.5")?;
    }
    if args[2] == "default_edges" {
        for (key, value) in [
            ("CarrotYouTubeLive", "_1"),
            (
                "CarrotYouTubeQuality",
                "99999999999999999999999999999999999999999999999999999",
            ),
            ("UptimeOffroad", "1_2.5"),
            ("UptimeOnroad", "1__2.5"),
        ] {
            params.inner.put(key, value.as_bytes())?;
        }
    }
    let mut fixture = Fixture {
        trace: trace.clone(),
        scenario: args[2].clone(),
        params: params.clone(),
        frame: 0,
    };
    let paths = InitPaths {
        shm: &directory.join("shm"),
        params: &directory.join("params/d"),
    };
    let env = Environment {
        no_board: true,
        block: "ui,,custom".into(),
        prepare_only: args[2] == "prepare_only",
    };
    let main = openpilot_manager::main_loop::Main {
        params: params.as_ref(),
        paths,
        environment: env,
    };
    let outcome = if args[2] == "reset_defaults" {
        openpilot_manager::parameters::set_defaults(params.as_ref(), true)
    } else {
        main.run(&mut fixture, |startup| {
            Ok(Fixture {
                trace: startup.trace.clone(),
                scenario: startup.scenario.clone(),
                params: startup.params.clone(),
                frame: 0,
            })
        })
    };
    let status = match outcome {
        Ok(()) => "ok",
        Err(Error::Interrupted) => "interrupted",
        Err(_) => "error",
    };
    let mut values = serde_json::Map::new();
    for entry in std::fs::read_dir(directory.join("params/d"))? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            values.insert(
                entry.file_name().to_string_lossy().into_owned(),
                json!(std::fs::read(entry.path())?),
            );
        }
    }
    let environment: serde_json::Map<_, _> = [
        "DISABLE_WIDE_ROAD",
        "DONGLE_ID",
        "GIT_ORIGIN",
        "GIT_BRANCH",
        "GIT_COMMIT",
        "CLEAN",
    ]
    .into_iter()
    .filter_map(|key| std::env::var(key).ok().map(|v| (key.into(), json!(v))))
    .collect();
    std::fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"status":status,"trace":*trace.borrow(),"params":values,"environment":environment}),
        )?,
    )?;
    Ok(())
}
