//! Real isolated CPU neural/VisionIPC gate; never vehicle timing evidence.
use openpilot_msgq::{VisionLayout, VisionMetadata, VisionServer, VisionStream};
use openpilot_plannerd::signal::transport::Mode;
use openpilot_signal_observer::{
    entry::Entry,
    runtime::{self, Config, Error},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{atomic::AtomicBool, Arc},
    thread,
    time::{Duration, Instant},
};
fn command() -> Result<Command, Error> {
    let binary = env::current_exe()?;
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
struct Guard(Child);
impl Drop for Guard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
fn records(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}
fn wait_for(mut condition: impl FnMut() -> bool) -> Result<(), Error> {
    let deadline = Instant::now() + Duration::from_secs(600);
    while !condition() {
        if Instant::now() >= deadline {
            return Err("signal runtime fixture timed out".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}
fn worker(root: &Path, lock_only: bool) -> Result<(), Error> {
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, stop.clone())?;
    let config = Config {
        directory: root.join("worker"),
        start: root.join("start.json"),
        prepare: root.join("prepare.json"),
        display: root.join("display.json"),
        frames: None,
        duration: Some(Duration::from_secs(600)),
    };
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join(if lock_only {
            "lock-events.jsonl"
        } else {
            "events.jsonl"
        }))?;
    let provider =
        PathBuf::from(env::var_os("SIGNAL_TRACE_ORT_ROOT").ok_or("missing provider root")?);
    let params = openpilot_params::Params::for_runtime_at(&root.join("params"))?;
    Entry {
        config: &config,
        stop: &stop,
        params: &params,
        provider: Some(&provider),
        child: None,
    }
    .run(
        &mut || Mode::Observe,
        &mut |name, fields| {
            writeln!(log, "{}", json!({"event":name,"fields":fields}))?;
            Ok(())
        },
        &mut |_| Err("explicit CPU entry must not spawn an automatic child".into()),
    )
}
fn same(actual: &Value, expected: &Value) -> bool {
    match expected {
        Value::Number(number) if number.is_f64() => {
            actual.as_f64().map(f64::to_bits) == number.as_f64().map(f64::to_bits)
        }
        Value::Array(values) => actual.as_array().is_some_and(|a| {
            a.len() == values.len() && a.iter().zip(values).all(|(a, e)| same(a, e))
        }),
        Value::Object(values) => actual.as_object().is_some_and(|a| {
            a.len() == values.len()
                && values
                    .iter()
                    .all(|(key, e)| a.get(key).is_some_and(|a| same(a, e)))
        }),
        _ => actual == expected,
    }
}
fn controller(root: &Path) -> Result<(), Error> {
    openpilot_opencv_runtime::initialize(1)?;
    let worker_root = root.join("worker");
    fs::create_dir(&worker_root)?;
    for name in [
        "enabled",
        "neural_enabled",
        "start_guard_enabled",
        "display_enabled",
    ] {
        fs::write(worker_root.join(name), b"1")?;
    }
    fs::write(worker_root.join("neural_backend"), b"cpu")?;
    let model_root = PathBuf::from(env::var_os("SIGNAL_TRACE_MODELS").ok_or("missing models")?);
    fs::create_dir(worker_root.join("neural"))?;
    for name in ["detector.onnx", "classifier.onnx"] {
        std::os::unix::fs::symlink(
            fs::canonicalize(model_root.join(name))?,
            worker_root.join("neural").join(name),
        )?;
    }
    let expected: Value = serde_json::from_slice(&fs::read(
        env::var_os("SIGNAL_TRACE_EXPECTED").ok_or("missing expected source")?,
    )?)?;
    let server = VisionServer::new("camerad")?;
    let layout = VisionLayout {
        width: 1344,
        height: 760,
        stride: 1408,
        uv_offset: 1408 * 760 + 128,
        len: 1408 * 760 + 128 + 1408 * 380,
    };
    let images = server.create_stream(VisionStream::Road, 2, layout)?;
    let packed = fs::read(env::var_os("SIGNAL_TRACE_NV12").ok_or("missing packed NV12")?)?;
    if packed.len() != 1344 * 760 * 3 / 2 {
        return Err("fixture NV12 extent".into());
    }
    let mut bytes = vec![0; layout.len];
    for y in 0..760 {
        bytes[y * layout.stride..y * layout.stride + 1344]
            .copy_from_slice(&packed[y * 1344..(y + 1) * 1344]);
    }
    for y in 0..380 {
        let offset = layout.uv_offset + y * layout.stride;
        let source = 1344 * 760 + y * 1344;
        bytes[offset..offset + 1344].copy_from_slice(&packed[source..source + 1344]);
    }
    let rgb = openpilot_signal_observer::worker::copy_nv12_rgb(
        &bytes,
        1344,
        760,
        layout.stride,
        layout.uv_offset,
    )?;
    if format!("{:x}", Sha256::digest(&packed))
        != expected["nv12_sha256"]
            .as_str()
            .ok_or("missing NV12 identity")?
        || format!("{:x}", Sha256::digest(&rgb.data))
            != expected["rgb_sha256"]
                .as_str()
                .ok_or("missing RGB identity")?
    {
        return Err("source frozen pixel identity differs".into());
    }
    for image in &images {
        image.write(0, &bytes)?;
    }
    server.start_listener()?;
    let mut worker = Guard(command()?.arg("worker").arg(root).spawn()?);
    let events = root.join("events.jsonl");
    wait_for(|| {
        records(&events)
            .iter()
            .any(|v| v["event"] == "signalNeuralShadowLoaded")
    })?;
    let status = command()?.arg("lock-only").arg(root).status()?;
    if !status.success() || !records(&root.join("lock-events.jsonl")).is_empty() {
        return Err("exclusive worker lock admitted a second observer".into());
    }
    let publish = |id: u32, age: u64| -> Result<(), Error> {
        let now = runtime::monotonic_ns();
        images[0].publish(VisionMetadata {
            width: layout.width,
            height: layout.height,
            stride: layout.stride,
            uv_offset: layout.uv_offset,
            len: layout.len,
            frame_id: id,
            timestamp_sof: now.saturating_sub(age),
            timestamp_eof: now.saturating_sub(age),
            valid: true,
            received: true,
            index: 0,
            fd: 0,
        })?;
        Ok(())
    };
    // Consumption is proved by the first valid record's gap from stale ID1.
    for _ in 0..50 {
        publish(1, 151_000_000)?;
        thread::sleep(Duration::from_millis(20));
    }
    if records(&events)
        .iter()
        .any(|v| v["event"] == "signalNeuralShadow")
    {
        return Err("stale frame reached neural inference".into());
    }
    let deadline = Instant::now() + Duration::from_secs(600);
    while !records(&events)
        .iter()
        .any(|v| v["event"] == "signalNeuralShadow")
    {
        publish(10, 0)?;
        thread::sleep(Duration::from_millis(20));
        if Instant::now() > deadline {
            return Err("first native frame was not processed".into());
        }
    }
    let first = records(&events)
        .into_iter()
        .find(|v| v["event"] == "signalNeuralShadow")
        .ok_or("missing first record")?;
    for _ in 0..25 {
        publish(10, 0)?;
        thread::sleep(Duration::from_millis(20));
    }
    if records(&events)
        .iter()
        .filter(|v| v["event"] == "signalNeuralShadow")
        .count()
        != 1
    {
        return Err("duplicate capture produced new observation".into());
    }
    let deadline = Instant::now() + Duration::from_secs(600);
    while records(&events)
        .iter()
        .filter(|v| v["event"] == "signalNeuralShadow")
        .count()
        < 2
    {
        publish(9, 0)?;
        thread::sleep(Duration::from_millis(20));
        if Instant::now() > deadline {
            return Err("rollback frame not processed".into());
        }
    }
    fs::write(worker_root.join("enabled"), b"0")?;
    while worker.0.try_wait()?.is_none() {
        thread::sleep(Duration::from_millis(20));
        if Instant::now() > deadline {
            return Err("disabled worker did not exit".into());
        }
    }
    if !worker.0.wait()?.success() {
        return Err("native observer child failed".into());
    }
    let all = records(&events);
    let result = all
        .iter()
        .filter(|v| v["event"] == "signalNeuralShadow")
        .collect::<Vec<_>>();
    if result.len() != 2
        || first["fields"]["frame_gap"] != 9
        || !same(&first["fields"]["tracks"], &expected["tracks"])
        || !same(&result[1]["fields"]["tracks"], &expected["tracks"])
        || result[1]["fields"]["frame_gap"] != -1
        || result[1]["fields"]["session"] == first["fields"]["session"]
    {
        return Err("rollback did not reset tracker/session".into());
    }
    for (name, engine) in [("start.json", false), ("display.json", true)] {
        let payload: Value = serde_json::from_slice(&fs::read(root.join(name))?)?;
        if payload["frame_id"] != 9
            || payload["stream"] != "road"
            || payload["size"] != json!([1344, 760])
            || payload["session"] != result[1]["fields"]["session"]
            || payload.get("control_permission").is_some()
            || engine && payload["engine"] != "neural_cpu"
        {
            return Err("worker publication envelope mismatch".into());
        }
    }
    if root.join("prepare.json").exists() {
        return Err("neural worker wrote preparation transport".into());
    }
    for name in ["start", "display"] {
        let payload: Value = serde_json::from_slice(&fs::read(root.join(format!("{name}.json")))?)?;
        if !same(&payload["tracks"], &expected["payloads"][name]["tracks"]) {
            return Err("actual-source neural transport tracks differ".into());
        }
    }
    println!(
        "{}",
        json!({"frames":2,"stale_input":true,"duplicate_rejected":true,"rollback_reset":true,"exclusive_lock":true,"disable_exit":true,"publications":2,"active_source_tracks":true,"control_permission":false})
    );
    Ok(())
}
fn run() -> Result<(), Error> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("worker") => worker(
            &PathBuf::from(args.next().ok_or("missing worker directory")?),
            false,
        ),
        Some("lock-only") => worker(
            &PathBuf::from(args.next().ok_or("missing worker directory")?),
            true,
        ),
        Some("controller") => controller(&PathBuf::from(
            args.next().ok_or("missing controller directory")?,
        )),
        None => {
            let prefix = format!("rust-probe-signal-neural-live-{}", std::process::id());
            let namespace = PathBuf::from("/dev/shm").join(format!("msgq_{prefix}"));
            let root = env::temp_dir().join(&prefix);
            fs::create_dir(&namespace)?;
            fs::create_dir(&root)?;
            let result = command()?
                .arg("controller")
                .arg(&root)
                .env("OPENPILOT_PREFIX", prefix)
                .env_remove("CEREAL_FAKE")
                .status();
            let _ = fs::remove_dir_all(&root);
            let _ = fs::remove_dir_all(&namespace);
            if !result?.success() {
                return Err("signal runtime controller failed".into());
            }
            Ok(())
        }
        _ => Err("unknown fixture stage".into()),
    }
}
fn main() -> Result<(), Error> {
    run()
}
