//! Isolated real VisionIPC producer -> files -> message queues -> planner owner.
#[path = "../../examples/plannerd_owner_trace_support/snapshot.rs"]
mod snapshot;
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use openpilot_cereal::log_capnp::{self, event};
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::Options,
};
use openpilot_msgq::{VisionLayout, VisionMetadata, VisionServer, VisionStream};
use openpilot_params::Params;
use openpilot_plannerd::{
    config::Config,
    native_parameters,
    parameters::Parameters,
    platform::{Clock, SystemClock},
    runtime::{Output, Planner, SignalInputs, SERVICES},
    Error,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};
#[derive(Deserialize)]
struct Image {
    path: PathBuf,
    width: usize,
    height: usize,
}
#[derive(Deserialize)]
struct Request {
    source_commit: String,
    artifact: PathBuf,
    car_params: PathBuf,
    parameters: BTreeMap<String, String>,
    packets: BTreeMap<String, PathBuf>,
    moving_packets: BTreeMap<String, PathBuf>,
    images: Vec<Image>,
    #[serde(default)]
    faults: bool,
    #[serde(default = "default_moving_frames")]
    moving_frames: usize,
    #[serde(default)]
    prepare_faults: bool,
}
fn default_moving_frames() -> usize {
    6
}
struct Store {
    values: BTreeMap<String, String>,
    operations: Vec<[String; 2]>,
}
impl Parameters for Store {
    fn integer(&mut self, key: &'static str) -> Result<i32, Error> {
        self.operations.push(["get_int".into(), key.into()]);
        native_parameters::integer(self.values.get(key).map_or(&[], |v| v.as_bytes()))
    }
    fn float(&mut self, key: &'static str) -> Result<f64, Error> {
        self.operations.push(["get_float".into(), key.into()]);
        native_parameters::float(self.values.get(key).map_or(&[], |v| v.as_bytes()))
    }
}
#[derive(Default)]
struct RecordedClock {
    wall: RefCell<Vec<f64>>,
}
impl Clock for RecordedClock {
    fn monotonic(&self) -> f64 {
        SystemClock.monotonic()
    }
    fn wall(&self) -> f64 {
        let value = SystemClock.wall();
        self.wall.borrow_mut().push(value);
        value
    }
    fn thread_cpu(&self) -> f64 {
        SystemClock.thread_cpu()
    }
}
#[derive(Default)]
struct Capture {
    messages: Vec<(String, Vec<u8>)>,
    events: Vec<Value>,
}
impl Output for Capture {
    fn send(&mut self, name: &'static str, bytes: &[u8]) -> Result<(), Error> {
        self.messages.push((name.into(), bytes.to_vec()));
        Ok(())
    }
    fn warning(&mut self, text: String) -> Result<(), Error> {
        self.events.push(json!({"warning":text}));
        Ok(())
    }
    fn info(&mut self, text: String) -> Result<(), Error> {
        self.events.push(json!({"info":text}));
        Ok(())
    }
    fn event(&mut self, name: &str, fields: Value) -> Result<(), Error> {
        self.events.push(json!({"event":name,"fields":fields}));
        Ok(())
    }
}
struct Guard(Child);
impl Drop for Guard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
fn command(binary: &Path) -> Result<Command, Box<dyn std::error::Error>> {
    if cfg!(target_arch = "aarch64") {
        if let Ok(runner) = env::var("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER") {
            let mut words = runner.split_whitespace();
            let mut c = Command::new(words.next().ok_or("empty target runner")?);
            c.args(words).arg(binary);
            return Ok(c);
        }
    }
    Ok(Command::new(binary))
}
fn wait(mut ready: impl FnMut() -> bool) -> Result<(), Box<dyn std::error::Error>> {
    let until = Instant::now() + Duration::from_secs(45);
    while !ready() {
        if Instant::now() > until {
            return Err("live signal acquisition timeout".into());
        }
        thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}
fn records(root: &Path) -> Vec<Value> {
    fs::read_to_string(root.join("observer.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}
fn packet(
    name: &str,
    old: Option<&Path>,
    now: u64,
    index: u32,
    phase: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut message = Builder::new_default();
    if let Some(path) = old {
        let raw = fs::read(path)?;
        let reader = serialize::read_message(raw.as_slice(), ReaderOptions::new())?;
        message.set_root(reader.get_root::<event::Reader<'_>>()?)?;
    }
    let mut e = message.get_root::<event::Builder<'_>>()?;
    e.set_log_mono_time(now);
    e.set_valid(true);
    match name {
        "liveCalibration" => {
            let mut c = e.init_live_calibration();
            c.set_cal_status(if phase == "invalid_calibration" {
                log_capnp::live_calibration_data::Status::Uncalibrated
            } else {
                log_capnp::live_calibration_data::Status::Calibrated
            });
            let mut r = c.init_rpy_calib(3);
            for i in 0..3 {
                r.set(i, 0.);
            }
        }
        "roadCameraState" => {
            let mut c = e.init_road_camera_state();
            c.set_sensor(if phase == "invalid_camera" {
                log_capnp::frame_data::ImageSensor::Ar0231
            } else {
                log_capnp::frame_data::ImageSensor::Os04c10
            });
            c.set_frame_id(index);
        }
        "deviceState" => {
            e.init_device_state()
                .set_device_type(log_capnp::init_data::DeviceType::Tici);
        }
        "modelV2" => {
            if let event::ModelV2(value) = e.which()? {
                value?.set_frame_id(index);
            }
        }
        "livePose" => {
            let event::LivePose(value) = e.which()? else {
                return Err("pose fixture kind".into());
            };
            let mut p = value?;
            p.set_timestamp(if phase == "stale_pose" {
                now.saturating_sub(300_000_000)
            } else {
                now
            });
            p.set_inputs_o_k(phase != "invalid_pose");
            p.set_posenet_o_k(true);
            p.set_sensors_o_k(true);
            p.reborrow().get_orientation_n_e_d()?.set_valid(true);
            let mut a = p.get_angular_velocity_device()?;
            a.set_valid(true);
            a.set_x(0.);
            a.set_y(0.);
            a.set_z(if phase == "turning" {
                1.6_f32.to_radians()
            } else {
                0.
            });
            a.set_x_std(0.001);
            a.set_y_std(0.001);
            a.set_z_std(0.001);
        }
        "carrotNavi" => {
            let event::CarrotNavi(value) = e.which()? else {
                return Err("navigation fixture kind".into());
            };
            value?
                .get_lane_current()?
                .get_meta()?
                .set_received_mono_time_nanos(now);
        }
        "carState" if phase == "invalid_vehicle" => {
            let event::CarState(value) = e.which()? else {
                return Err("vehicle fixture kind".into());
            };
            value?.set_can_valid(false);
        }
        _ => {}
    }
    Ok(serialize::write_message_to_words(&message))
}
fn controller(root: &Path, input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let request: Request = serde_json::from_slice(&fs::read(input)?)?;
    if request.source_commit != "e8e152dbcfd9574a1791ed12026d1880e87689b3"
        || request.images.len() != 24
    {
        return Err("unexpected source/pixel fixture".into());
    }
    let params = Params::for_runtime_at(&root.join("params"))?;
    params.put("SignalAssistMode", b"2")?;
    let mut store = Store {
        values: request.parameters,
        operations: vec![],
    };
    store.values.insert("SignalAssistMode".into(), "2".into());
    fs::create_dir(root.join("observer"))?;
    for name in [
        "enabled",
        "start_guard_enabled",
        "stop_prepare_enabled",
        "display_enabled",
    ] {
        fs::write(root.join("observer").join(name), b"1")?;
    }
    let geometry_names = ["liveCalibration", "roadCameraState", "deviceState"];
    let mut names = SERVICES.to_vec();
    names.extend(geometry_names);
    let mut publisher = PubMaster::isolated(&names)?;
    let geometry = SubMaster::isolated(&geometry_names, Options::default())?;
    let config = Config::decode(&fs::read(request.car_params)?)?;
    let artifact = env::var_os("SIGNAL_LIVE_ARTIFACT")
        .map(PathBuf::from)
        .unwrap_or(request.artifact);
    let (planner, mut subscriber) = Planner::load_with(config, &artifact, &mut store, |options| {
        Ok(SubMaster::isolated(&SERVICES, options)?)
    })?;
    let mut planner = planner.with_signal_inputs(SignalInputs {
        params,
        start: root.join("start.json"),
        prepare: root.join("prepare.json"),
        geometry: Some(geometry),
    });
    let server = VisionServer::new("camerad")?;
    let w = request.images[0].width;
    let h = request.images[0].height;
    let layout = VisionLayout {
        width: w,
        height: h,
        stride: w,
        uv_offset: w * h,
        len: w * h * 3 / 2,
    };
    let images = server.create_stream(VisionStream::Road, 1, layout)?;
    server.start_listener()?;
    let binary =
        PathBuf::from(env::var_os("SIGNAL_LIVE_WORKER").ok_or("missing native worker binary")?);
    let mut worker = Guard(command(&binary)?.arg(root).spawn()?);
    wait(|| {
        records(root)
            .iter()
            .any(|r| r["event"] == "signalTrackingShadowLoaded")
    })?;
    let mut capture = Capture::default();
    let clock = RecordedClock::default();
    let mut trace = vec![];
    let mut sequence: Vec<_> = request
        .images
        .into_iter()
        .map(|image| (Some(image), "live"))
        .collect();
    if request.prepare_faults {
        for phase in [
            "invalid_vehicle",
            "invalid_camera",
            "invalid_calibration",
            "invalid_pose",
            "stale_pose",
            "turning",
            "stale_camera",
            "stale_vehicle",
            "malformed_json",
            "missing_endpoint",
            "wrong_version",
            "mode_disabled",
            "mode_observe",
            "mode_prepare",
            "old_after_mode",
        ] {
            sequence.extend((0..10).map(|_| (None, "red_reacquire")));
            sequence.push((None, phase));
        }
    } else if request.faults {
        sequence.extend(
            [
                "red_reacquire",
                "red_reacquire",
                "red_reacquire",
                "red_reacquire",
                "red_reacquire",
                "red_reacquire",
                "duplicate_green",
                "reordered_green",
                "stale_green",
                "future_green",
                "malformed_json",
                "oversized_json",
                "missing_endpoint",
                "wrong_version",
                "invalid_vehicle",
                "invalid_camera",
                "invalid_calibration",
                "invalid_pose",
                "stale_pose",
                "turning",
                "stale_camera",
                "stale_vehicle",
                "mode_disabled",
                "mode_observe",
                "mode_prepare",
                "old_after_mode",
                "red_reacquire",
                "red_reacquire",
                "red_reacquire",
                "red_reacquire",
                "red_reacquire",
                "red_reacquire",
                "fresh_green",
                "fresh_green",
                "fresh_green",
                "fresh_green",
                "fresh_green",
                "fresh_green",
            ]
            .into_iter()
            .map(|phase| (None, phase)),
        );
    }
    let mut red_template: Option<(Value, Value)> = None;
    let mut green_template: Option<(Value, Value)> = None;
    for (index, (image, phase)) in sequence.into_iter().enumerate() {
        let fid = u32::try_from(index + 1)?;
        if index == 24 {
            let params = Params::for_runtime_at(&root.join("params"))?;
            params.put("SignalAssistMode", b"0")?;
            wait(|| worker.0.try_wait().ok().flatten().is_some())?;
            if !worker.0.wait()?.success() {
                return Err("producer fault-stage shutdown failed".into());
            }
            params.put("SignalAssistMode", b"2")?;
            thread::sleep(Duration::from_millis(600));
        }
        if let Some(image) = image {
            images[0].write(0, &fs::read(image.path)?)?;
            let publish = || -> Result<(), Box<dyn std::error::Error>> {
                let now = clock.message_time()?;
                images[0].publish(VisionMetadata {
                    width: layout.width,
                    height: layout.height,
                    stride: layout.stride,
                    uv_offset: layout.uv_offset,
                    len: layout.len,
                    frame_id: fid,
                    timestamp_sof: now,
                    timestamp_eof: now,
                    valid: true,
                    received: true,
                    index: 0,
                    fd: 0,
                })?;
                Ok(())
            };
            publish()?;
            // Repeat the same identity only until connection/acquisition. The worker
            // must reject it after its first accepted result.
            wait(|| {
                let ready = ["start.json", "prepare.json"].iter().all(|p| {
                    fs::read(root.join(p))
                        .ok()
                        .and_then(|v| serde_json::from_slice::<Value>(&v).ok())
                        .is_some_and(|v| v["frame_id"] == fid)
                });
                if !ready {
                    let _ = publish();
                }
                ready
            })?;
        } else {
            if matches!(phase, "mode_disabled" | "mode_observe" | "mode_prepare") {
                let value = match phase {
                    "mode_disabled" => b"0",
                    "mode_observe" => b"1",
                    _ => b"2",
                };
                Params::for_runtime_at(&root.join("params"))?.put("SignalAssistMode", value)?;
                thread::sleep(Duration::from_millis(600));
            } else {
                thread::sleep(Duration::from_millis(
                    if phase.starts_with("stale_")
                        && matches!(phase, "stale_camera" | "stale_vehicle")
                    {
                        300
                    } else {
                        100
                    },
                ));
            }
            let green = phase.ends_with("green");
            let template = if green {
                green_template.as_ref()
            } else {
                red_template.as_ref()
            }
            .ok_or("missing real producer template")?;
            for (name, mut value) in [
                ("start", template.0.clone()),
                ("prepare", template.1.clone()),
            ] {
                let now = clock.monotonic();
                value["timestamp"] = json!(if phase == "stale_green" {
                    now - 0.8
                } else if phase == "future_green" {
                    now + 0.1
                } else if phase == "old_after_mode" {
                    0.
                } else {
                    now
                });
                value["frame_id"] = json!(if phase == "duplicate_green" {
                    fid - 1
                } else if phase == "reordered_green" {
                    1
                } else {
                    fid
                });
                if phase == "wrong_version" {
                    value["version"] = json!(0);
                }
                let path = root.join(format!("{name}.json"));
                match phase {
                    "missing_endpoint" => {
                        let _ = fs::remove_file(path);
                    }
                    "malformed_json" => fs::write(path, b"{")?,
                    "oversized_json" => fs::write(path, vec![b' '; 32769])?,
                    _ => {
                        let temp = path.with_extension("tmp");
                        fs::write(&temp, serde_json::to_vec(&value)?)?;
                        fs::rename(temp, path)?;
                    }
                }
            }
        }
        let now = clock.message_time()?;
        let mut sent = vec![];
        for name in &names {
            // liveTracks is intentionally absent: this fixture exercises the
            // model-trigger owner, whose fast-trigger behavior is gated elsewhere.
            if *name == "liveTracks"
                || (phase == "stale_camera" && *name == "roadCameraState")
                || (phase == "stale_vehicle" && *name == "carState")
            {
                continue;
            }
            let bytes = packet(
                name,
                if index < request.moving_frames || (index >= 24 && request.prepare_faults) {
                    request.moving_packets.get(*name)
                } else {
                    request.packets.get(*name)
                }
                .map(PathBuf::as_path),
                now,
                fid,
                phase,
            )?;
            publisher.send(name, &bytes)?;
            let path = output.join(format!("{fid}-{name}.bin"));
            fs::write(&path, &bytes)?;
            sent.push(path);
        }
        subscriber.update(Duration::from_millis(100))?;
        store.operations.clear();
        clock.wall.borrow_mut().clear();
        let tick = planner.process(&subscriber.state, &mut store, &clock, &mut capture)?;
        if !tick.longitudinal_run {
            return Err("fresh model queue did not run planner".into());
        }
        let mut publications = vec![];
        for (name, bytes) in capture.messages.drain(..) {
            let path = output.join(format!("{fid}-{name}.bin"));
            fs::write(&path, bytes)?;
            publications.push((name, path));
        }
        let file_value = |name: &str| {
            fs::read(root.join(format!("{name}.json")))
                .ok()
                .and_then(|v| serde_json::from_slice::<Value>(&v).ok())
                .unwrap_or(Value::Null)
        };
        let start = file_value("start");
        let prepare = file_value("prepare");
        if index == 11 {
            red_template = Some((start.clone(), prepare.clone()));
        }
        if index == 23 {
            green_template = Some((start.clone(), prepare.clone()));
        }
        if matches!(
            phase,
            "duplicate_green" | "reordered_green" | "stale_green" | "future_green"
        ) && planner
            .carrot
            .start_decision
            .as_ref()
            .is_some_and(|d| d.released)
        {
            return Err("invalid green released connected guard".into());
        }
        trace.push(json!({"frame_id":fid,"phase":phase,
            "mode_bytes":Params::for_runtime_at(&root.join("params"))?.get("SignalAssistMode")?,
            "endpoint_bytes":{"start":fs::read(root.join("start.json")).ok(),"prepare":fs::read(root.join("prepare.json")).ok()},"sent":sent,"publications":publications,"start":start,"prepare":prepare,
            "events":std::mem::take(&mut capture.events),"signal_state":planner.carrot.signal_snapshot(),"start_decision":planner.carrot.start_decision,"prepare_decision":planner.carrot.prepare_decision,
            "control_active":planner.carrot.prepare_control_active,"constraint":planner.longitudinal.signal_constraint_snapshot(),"color":planner.carrot.signal_observation_color,"green":planner.carrot.signal_green_active,
            "owner":snapshot::frame(&planner.longitudinal,&planner.lateral,&planner.carrot,None,false,&store.operations),
            "wall_calls":*clock.wall.borrow(),
            "services":subscriber.state.topics().iter().map(|t|json!({"name":t.service.name,"seen":t.seen,"valid":t.valid,"alive":t.alive,"receive_time":t.receive_time})).collect::<Vec<_>>() }));
    }
    let params = Params::for_runtime_at(&root.join("params"))?;
    params.put("SignalAssistMode", b"0")?;
    fs::write(root.join("observer/enabled"), b"0")?;
    wait(|| worker.0.try_wait().ok().flatten().is_some())?;
    if !worker.0.wait()?.success() {
        return Err("native producer did not exit successfully".into());
    }
    let observations = records(root);
    fs::write(
        output.join("observations.json"),
        serde_json::to_vec(&observations)?,
    )?;
    fs::write(output.join("trace.json"), serde_json::to_vec(&trace)?)?;
    println!(
        "{}",
        json!({"frames":trace.len(),"actual_visionipc_frames":24,"injected_consumer_actions":trace.len()-24,"actual_visionipc":true,"actual_observer":true,"actual_files":true,"actual_subscribers":true,"actual_planner_owner":true,"source_equivalence":false,"vehicle_timing":false})
    );
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    if args.next().as_deref() == Some(std::ffi::OsStr::new("controller")) {
        let root = PathBuf::from(args.next().ok_or("missing root")?);
        let input = PathBuf::from(args.next().ok_or("missing input")?);
        let output = PathBuf::from(args.next().ok_or("missing output")?);
        return controller(&root, &input, &output);
    }
    let input = PathBuf::from(env::args_os().nth(1).ok_or("missing input")?);
    let output = PathBuf::from(env::args_os().nth(2).ok_or("missing output")?);
    fs::create_dir(&output)?;
    let prefix = format!("rust-probe-signal-live-owner-{}", std::process::id());
    let namespace = PathBuf::from("/dev/shm").join(format!("msgq_{prefix}"));
    let root = env::temp_dir().join(&prefix);
    fs::create_dir(&namespace)?;
    fs::create_dir(&root)?;
    let status = command(&env::current_exe()?)?
        .arg("controller")
        .arg(&root)
        .arg(fs::canonicalize(input)?)
        .arg(fs::canonicalize(output)?)
        .env("OPENPILOT_PREFIX", prefix)
        .env_remove("CEREAL_FAKE")
        .status();
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(namespace);
    if !status?.success() {
        return Err("live owner child failed".into());
    }
    Ok(())
}
