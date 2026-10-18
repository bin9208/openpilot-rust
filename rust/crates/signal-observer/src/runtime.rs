//! Isolated classical producer. The optional neural owner is selected separately.
use crate::{
    process::Process,
    scheduling, transport,
    worker::{self, Capture, CapturePolicy},
};
use openpilot_msgq::{VisionClient, VisionStream};
use openpilot_plannerd::signal::transport::Mode;
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};
pub type Error = Box<dyn std::error::Error>;
pub const DIRECTORY: &str = "/data/signal-color-shadow";
#[derive(Debug)]
pub struct Config {
    pub directory: PathBuf,
    pub start: PathBuf,
    pub prepare: PathBuf,
    pub display: PathBuf,
    pub frames: Option<u64>,
    pub duration: Option<Duration>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            directory: DIRECTORY.into(),
            start: transport::START_PATH.into(),
            prepare: transport::PREPARE_PATH.into(),
            display: transport::DISPLAY_PATH.into(),
            frames: None,
            duration: None,
        }
    }
}
/// Raw SignalAssistMode acquisition; shared by cached entry and fresh auto policy.
pub fn parameter_value(bytes: &[u8]) -> i64 {
    openpilot_beepd::integer(bytes).map(i64::from).unwrap_or(0)
}

pub fn monotonic_ns() -> u64 {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}
fn cpu() -> f64 {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    t.tv_sec as f64 + t.tv_nsec as f64 / 1e9
}
fn now() -> f64 {
    monotonic_ns() as f64 / 1e9
}
fn session() -> String {
    format!("{}:{}", std::process::id(), monotonic_ns())
}
fn flag(config: &Config, name: &str) -> Result<bool, Error> {
    crate::auto_runtime::flag(config, name)
}
/// Runtime entry only permits the production classical selection. Neural owners
/// must be executed through their verified provider boundary, never substituted.
pub fn run_classical(
    config: &Config,
    stop: &AtomicBool,
    mode: &mut impl FnMut() -> Mode,
    event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
) -> Result<(), Error> {
    run_classical_selected(config, stop, mode, event)
}
/// Source automatic tracking child forces comparison even while neural_enabled
/// remains set. It must never enter CPU neural inference as a fallback.
pub fn run_classical_comparison(
    config: &Config,
    stop: &AtomicBool,
    mode: &mut impl FnMut() -> Mode,
    event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
) -> Result<(), Error> {
    run_classical_selected(config, stop, mode, event)
}
fn run_classical_selected(
    config: &Config,
    stop: &AtomicBool,
    mode: &mut impl FnMut() -> Mode,
    event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
) -> Result<(), Error> {
    if !mode().enabled() || !mode().enabled() {
        return Ok(());
    }
    fs::create_dir_all(&config.directory)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(config.directory.join("worker.lock"))?;
    match rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => {}
        Err(rustix::io::Errno::WOULDBLOCK) => return Ok(()),
        Err(e) => return Err(e.into()),
    }
    scheduling::configure()?;
    openpilot_opencv_runtime::initialize(1)?;
    scheduling::configure()?;
    let fixture: Value =
        serde_json::from_str(include_str!("../../../upstream/signal-source.json"))?;
    let algorithm = fixture["files"]["tools/signal_analysis/signal_tracker.py"]["sha256"]
        .as_str()
        .ok_or("missing signal source identity")?;
    let comparison_selection = flag(config, "daytime_comparison_enabled")?;
    let mut last_file = 0.;
    let mut tracker = Process::default();
    let mut brake_observer = crate::lead_brake::runtime::Runtime::default();
    let mut identity = session();
    let mut client = VisionClient::new("camerad", VisionStream::Road, true)?;
    let mut capture = CapturePolicy::default();
    let start = now();
    event(
        "signalTrackingShadowLoaded",
        json!({"mode":"tracking_observation","algorithm_sha256":algorithm,"runtime":"4.13.0","pid":std::process::id(),"max_hz":20,"target_cpu_duty":0.5,"stream":"road","reference_width":1344,"reference_height":760,"control_permission":false,"assist_transport":false,"daytime_comparison":true,"revalidated":true,"single_tracker":true,"verified_lamps":true,"native_observer":true}),
    )?;
    let mut count = 0;
    while !stop.load(Ordering::Relaxed)
        && mode().enabled()
        && mode().enabled()
        && flag(config, "daytime_comparison_enabled")? == comparison_selection
        && config
            .duration
            .is_none_or(|limit| now() - start < limit.as_secs_f64())
        && config.frames.is_none_or(|limit| count < limit)
    {
        if !client.is_connected() && !client.connect()? {
            tracker = Process::default();
            identity = session();
            thread::sleep(Duration::from_millis(200));
            continue;
        }
        let Some(frame) = client.receive(Duration::from_millis(100))? else {
            tracker = Process::default();
            identity = session();
            continue;
        };
        let meta = *frame.metadata();
        let work_start = now();
        let cpu_start = cpu();
        let input_now = monotonic_ns();
        let (gap, reset) = match capture.receive(meta.frame_id, meta.timestamp_eof, input_now) {
            Capture::Duplicate => {
                thread::sleep(Duration::from_millis(5));
                continue;
            }
            Capture::Stale => {
                tracker = Process::default();
                identity = session();
                event(
                    "signalTrackingShadowSkipped",
                    json!({"frame_id":meta.frame_id,"timestamp_eof":meta.timestamp_eof,"reason":"stale_input","input_age_ms":(i128::from(input_now)-i128::from(meta.timestamp_eof)) as f64/1e6}),
                )?;
                thread::sleep(Duration::from_millis(50));
                continue;
            }
            Capture::Process { gap, reset } => (gap, reset),
        };
        if reset {
            tracker = Process::default();
            identity = session();
        }
        let mut bytes = vec![0; meta.len];
        frame.copy_into(&mut bytes)?;
        let rgb = worker::copy_nv12_rgb(
            &bytes,
            meta.width.try_into()?,
            meta.height.try_into()?,
            meta.stride,
            meta.uv_offset,
        )?;
        if frame.descriptor()?.buffer_frame_id != u64::from(meta.frame_id) {
            tracker = Process::default();
            identity = session();
            event(
                "signalTrackingShadowSkipped",
                json!({"frame_id":meta.frame_id,"timestamp_eof":meta.timestamp_eof,"reason":"buffer_reused_during_copy"}),
            )?;
            thread::sleep(Duration::from_millis(50));
            continue;
        }
        let result = tracker.process(&rgb, meta.timestamp_eof as f64 / 1e9, true)?;
        let result_now = monotonic_ns();
        let fresh = worker::result_fresh(meta.timestamp_eof, result_now);
        let recorded_monotonic_ns = monotonic_ns();
        let work_ms = (now() - work_start) * 1000.;
        let cpu_ms = (cpu() - cpu_start) * 1000.;
        let start_guard_transport = observation_requested(config, "start_guard_enabled", mode)?;
        let record = json!({"mode":"tracking_observation","algorithm_sha256":algorithm,"stream":"road","frame_id":meta.frame_id,"frame_gap":gap,"timestamp_eof":meta.timestamp_eof,"recorded_monotonic_ns":recorded_monotonic_ns,"reference_width":1344,"reference_height":760,"input_age_ms":(i128::from(input_now)-i128::from(meta.timestamp_eof)) as f64/1e6,"result_age_ms":(i128::from(result_now)-i128::from(meta.timestamp_eof)) as f64/1e6,"work_ms":work_ms,"cpu_ms":cpu_ms,"tracks":result["tracks"],"daytime_comparison":true,"start_guard_transport":start_guard_transport,"assist_transport":false,"revalidated":true,"single_tracker":true,"verified_lamps":true,"session":identity,"prediction":if fresh{result["state"].clone()}else{json!("unknown")},"fresh":fresh,"reason":if fresh{result["reason"].clone()}else{json!("stale_result")},"control_permission":false});
        event("signalTrackingShadow", record.clone())?;
        publish_classical(config, &record, mode, event)?;
        if now() - last_file >= 1. {
            let temp = config.directory.join("tracking_latest.tmp");
            fs::write(&temp, serde_json::to_vec(&record)?)?;
            fs::rename(temp, config.directory.join("tracking_latest.json"))?;
            last_file = now();
        }
        brake_observer.update(
            crate::lead_brake::runtime::Frame {
                mode_enabled: mode().enabled(),
                rgb: &rgb,
                frame_id: meta.frame_id,
                stamp: meta.timestamp_eof as f64 / 1e9,
                session: &identity,
                work_start,
            },
            event,
        );
        count += 1;
        thread::sleep(Duration::from_secs_f64(worker::next_delay(
            now() - work_start,
            cpu() - cpu_start,
        )));
    }
    Ok(())
}

/// Production enable override; isolated paths retain their source file flags.
pub fn observation_requested(
    config: &Config,
    name: &str,
    mode: &mut impl FnMut() -> Mode,
) -> Result<bool, Error> {
    if config.directory == Path::new(DIRECTORY) {
        Ok(mode().enabled())
    } else {
        flag(config, name)
    }
}
/// Preserve source record/start/prepare/display phase reads. Disabled payloads
/// are not rendered, and independent publication errors cannot enable control.
pub fn publish_classical(
    config: &Config,
    record: &Value,
    mode: &mut impl FnMut() -> Mode,
    event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
) -> Result<(), Error> {
    for (name, path, flag, render) in [
        (
            "signalStartObservationPublishFailed",
            &config.start,
            "start_guard_enabled",
            transport::start as fn(&Value) -> _,
        ),
        (
            "signalPrepareObservationPublishFailed",
            &config.prepare,
            "stop_prepare_enabled",
            transport::prepare as fn(&Value) -> _,
        ),
    ] {
        let outcome = (|| -> Result<(), Error> {
            if observation_requested(config, flag, mode)? {
                transport::write(path, &render(record)?, 32768)?;
            }
            Ok(())
        })();
        if let Err(error) = outcome {
            event(name, json!({"error":error.to_string()}))?;
        }
    }
    // Source evaluates display_requested outside its exception handler.
    if observation_requested(config, "display_enabled", mode)? {
        if let Err(error) = transport::display(record)
            .and_then(|payload| transport::write(&config.display, &payload, 8192))
        {
            event(
                "signalDisplayPublishFailed",
                json!({"error":error.to_string()}),
            )?;
        }
    }
    Ok(())
}
