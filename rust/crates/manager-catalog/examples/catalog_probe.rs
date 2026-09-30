use openpilot_manager_catalog::{
    catalog, Error, GpsPaths, ImportConfig, Parameters, Predicate, RustAvailability, SourceProcess,
    State,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::BTreeMap, path::PathBuf, rc::Rc};

#[derive(Deserialize)]
struct Config {
    pc: bool,
    tici: bool,
    webcam: bool,
    carrot_web_external: bool,
    darwin: bool,
    bodyteleop_available: bool,
}
impl From<Config> for ImportConfig {
    fn from(c: Config) -> Self {
        Self {
            pc: c.pc,
            tici: c.tici,
            webcam: c.webcam,
            carrot_web_external: c.carrot_web_external,
            darwin: c.darwin,
            bodyteleop_available: c.bodyteleop_available,
        }
    }
}
#[derive(Deserialize)]
struct Request {
    config: Option<Config>,
    #[serde(default)]
    bodyteleop_available: bool,
    #[serde(default)]
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    predicate: String,
    started: bool,
    not_car: bool,
    #[serde(default)]
    values: BTreeMap<String, Vec<u8>>,
    #[serde(default)]
    directories: Vec<String>,
    #[serde(default)]
    gps_paths: Vec<String>,
    exception_key: Option<String>,
}
struct Traced {
    params: openpilot_params::Params,
    trace: Rc<RefCell<Vec<Value>>>,
    exception_key: Option<String>,
}
impl Traced {
    fn access(&self, op: &str, key: &str) -> Result<(), Error> {
        self.trace.borrow_mut().push(json!([op, key]));
        if self.exception_key.as_deref() == Some(key) {
            return Err(openpilot_params::Error::UnknownKey("fixture exception".into()).into());
        }
        Ok(())
    }
}
impl Parameters for Traced {
    fn get_bool(&mut self, key: &str) -> Result<bool, Error> {
        self.access("bool", key)?;
        Parameters::get_bool(&mut self.params, key)
    }
    fn get_int(&mut self, key: &str) -> Result<i32, Error> {
        self.access("int", key)?;
        Parameters::get_int(&mut self.params, key)
    }
    fn get_bool_default(&mut self, key: &str) -> Result<bool, Error> {
        self.access("default", key)?;
        Parameters::get_bool_default(&mut self.params, key)
    }
    fn put_bool(&mut self, key: &str, value: bool) -> Result<(), Error> {
        self.trace.borrow_mut().push(json!(["put", key, value]));
        Parameters::put_bool(&mut self.params, key, value)
    }
}
struct Paths {
    trace: Rc<RefCell<Vec<Value>>>,
    paths: Vec<String>,
}
impl GpsPaths for Paths {
    fn exists(&mut self, path: &str) -> bool {
        self.trace.borrow_mut().push(json!(["exists", path]));
        self.paths.iter().any(|p| p == path)
    }
}
fn predicate(name: &str) -> Result<Predicate, Box<dyn std::error::Error>> {
    use Predicate::*;
    Ok(match name {
        "driverview" => DriverView,
        "notcar" => NotCar,
        "iscar" => IsCar,
        "logging" => Logging,
        "ublox" => Ublox,
        "joystick" => Joystick,
        "not_joystick" => NotJoystick,
        "long_maneuver" => LongManeuver,
        "lat_maneuver" => LatManeuver,
        "not_long_maneuver" => NotLongManeuver,
        "qcomgps" => QcomGps,
        "always_run" => Always,
        "only_onroad" => Onroad,
        "only_offroad" => Offroad,
        "enable_updated" => Updated,
        "enable_dm" => DriverMonitoring,
        "enable_xiaoge_data" => ShareData,
        "enable_webrtc" => WebRtc,
        "c3x_lite" => C3xLite,
        "enable_youtube_low_encoder" => YoutubeLow,
        "enable_youtube_medium_encoder" => YoutubeMedium,
        "enable_youtube_encoder" => Youtube,
        "enable_youtube_wide_encoder" => YoutubeWide,
        "enable_cluster_hud" => ClusterHud,
        _ => return Err(format!("unknown predicate {name}").into()),
    })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    let request: Request = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let output = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&output)?;
    let descriptors = catalog(
        request
            .config
            .map(Into::into)
            .unwrap_or_else(|| ImportConfig::from_environment(request.bodyteleop_available)),
    );
    let snapshots: Vec<_> = descriptors.iter().map(|d| {
        let (kind, module, cwd, argv, pid_key) = match d.source {
            SourceProcess::Python { module } => ("PythonProcess", Some(module), None, None, None),
            SourceProcess::Native { cwd, argv } => ("NativeProcess", None, Some(cwd), Some(argv), None),
            SourceProcess::Persistent { module, pid_key } => ("DaemonProcess", Some(module), None, None, Some(pid_key)),
        };
        json!({"name": d.name, "kind": kind, "module": module, "cwd": cwd, "argv": argv, "pid_key": pid_key,
            "enabled": d.enabled, "sigkill": d.sigkill, "restart_if_crash": d.restart_if_crash, "daemon": d.daemon})
    }).collect();
    let availability: Vec<_> = descriptors.iter().map(|d| match d.rust {
        RustAvailability::NotPorted => json!({"name": d.name, "status": "not_ported"}),
        RustAvailability::Candidate { package, binary, limitation } => json!({"name": d.name, "status": "candidate", "package": package, "binary": binary, "limitation": limitation}),
    }).collect();
    let mut results = Vec::new();
    for (index, case) in request.cases.into_iter().enumerate() {
        let root = output.join(format!("params-{index}"));
        let params = openpilot_params::Params::open(&root, "d")?;
        for (key, value) in case.values {
            openpilot_params::metadata(&key).ok_or("unknown fixture key")?;
            std::fs::write(root.join("d").join(&key), &value)?;
        }
        for key in case.directories {
            let path = root.join("d").join(key);
            if path.is_file() {
                std::fs::remove_file(&path)?;
            }
            std::fs::create_dir(path)?;
        }
        let trace = Rc::new(RefCell::new(Vec::new()));
        let mut params = Traced {
            params,
            trace: trace.clone(),
            exception_key: case.exception_key,
        };
        let mut paths = Paths {
            trace: trace.clone(),
            paths: case.gps_paths,
        };
        let callback = if let Some(name) = case.predicate.strip_prefix("process:") {
            descriptors
                .iter()
                .find(|d| d.name == name)
                .ok_or("unknown process")?
                .predicate
        } else {
            predicate(&case.predicate)?
        };
        let outcome = match callback.evaluate(
            State {
                started: case.started,
                not_car: case.not_car,
            },
            &mut params,
            &mut paths,
        ) {
            Ok(value) => json!({"value": value}),
            Err(Error::Integer { .. }) => json!({"error": "fatal_integer"}),
            Err(Error::Parameter(_)) => json!({"error": "parameter"}),
        };
        std::fs::write(
            output.join(format!("trace-{index}.json")),
            serde_json::to_vec(&*trace.borrow())?,
        )?;
        results.push(json!({"outcome": outcome, "trace": *trace.borrow(), "ublox": Parameters::get_bool(&mut params.params, "UbloxAvailable")?}));
    }
    std::fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"catalog": snapshots, "availability": availability, "results": results}),
        )?,
    )?;
    Ok(())
}
