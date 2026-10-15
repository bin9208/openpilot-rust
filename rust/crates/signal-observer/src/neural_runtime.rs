//! Selected native neural owner; classical/CPU/QCOM selection never silently falls back.
#[cfg(feature = "neural-ort")]
use crate::neural;
use crate::{
    backend::Request,
    neural_sequence::Sequence,
    runtime::{self, Config, DIRECTORY, Error},
    scheduling, transport,
    worker::{self, Capture, CapturePolicy},
};
use openpilot_msgq::{VisionClient, VisionStream};
#[cfg(feature = "neural-ort")]
use openpilot_onnx_runtime::Provider;
use openpilot_plannerd::signal::transport::Mode;
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};
fn now() -> f64 {
    runtime::monotonic_ns() as f64 / 1e9
}
fn cpu() -> f64 {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    t.tv_sec as f64 + t.tv_nsec as f64 / 1e9
}
fn session() -> String {
    format!("neural:{}:{}", std::process::id(), runtime::monotonic_ns())
}
fn flag(config: &Config, name: &str) -> Result<bool, Error> {
    crate::auto_runtime::flag(config, name)
}
fn requested(config: &Config, mode: Mode) -> Result<bool, Error> {
    if config.directory == Path::new(DIRECTORY) {
        Ok(mode.enabled())
    } else {
        flag(config, "enabled")
    }
}
fn observation_flag(config: &Config, name: &str, mode: Mode) -> Result<bool, Error> {
    if config.directory == Path::new(DIRECTORY) {
        Ok(mode.enabled())
    } else {
        flag(config, name)
    }
}
fn selection(config: &Config) -> Result<Request, Error> {
    match fs::read_to_string(config.directory.join("neural_backend")) {
        Ok(value) => Ok(Request::parse(Some(&value))?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Request::Cpu),
        Err(e) => Err(e.into()),
    }
}
fn latest(config: &Config, record: &Value) -> Result<(), Error> {
    let temp = config.directory.join("neural_latest.tmp");
    fs::write(&temp, serde_json::to_vec(record)?)?;
    fs::rename(temp, config.directory.join("neural_latest.json"))?;
    Ok(())
}
/// The caller must explicitly select CPU. A missing or invalid provider fails;
/// this function never substitutes a CPU model for an unavailable QCOM owner.
#[cfg(feature = "neural-ort")]
pub fn run_cpu(
    config: &Config,
    provider_root: &Path,
    stop: &AtomicBool,
    mode: &mut impl FnMut() -> Mode,
    event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
) -> Result<(), Error> {
    run_cpu_inner(config, provider_root, stop, mode, event, false)
}
/// Entry has already performed the exact requested/neural/backend prefix.
#[cfg(feature = "neural-ort")]
pub(crate) fn run_cpu_selected(
    config: &Config,
    provider_root: &Path,
    stop: &AtomicBool,
    mode: &mut impl FnMut() -> Mode,
    event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
) -> Result<(), Error> {
    run_cpu_inner(config, provider_root, stop, mode, event, true)
}
#[cfg(feature = "neural-ort")]
fn run_cpu_inner(
    config: &Config,
    provider_root: &Path,
    stop: &AtomicBool,
    mode: &mut impl FnMut() -> Mode,
    event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
    selected_by_entry: bool,
) -> Result<(), Error> {
    if !selected_by_entry {
        if !requested(config, mode())? || !flag(config, "neural_enabled")? {
            return Ok(());
        }
        if selection(config)? != Request::Cpu {
            return Err("CPU owner requires explicit CPU backend selection".into());
        }
    }
    run_loaded(config, stop, mode, event, Request::Cpu, "cpu", || {
        let provider = Provider::verified(provider_root)?;
        let mut network = neural::cpu(&provider, &config.directory.join("neural"))?;
        network.warm_up()?;
        Ok(network)
    })
}

#[cfg(feature = "neural-qcom")]
pub(crate) fn run_qcom_selected(
    config: &Config,
    stop: &AtomicBool,
    mode: &mut impl FnMut() -> Mode,
    event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
) -> Result<(), Error> {
    run_qcom_leaf(config, stop, mode, event, || {
        let root = std::env::var_os("CARROT_RUNTIME_ROOT")
            .ok_or("QCOM requires a verified runtime package root")?;
        let digest = std::env::var("CARROT_RUNTIME_MANIFEST_SHA256")
            .map_err(|_| "QCOM requires the verified launcher manifest identity")?;
        // SAFETY: normal launcher verification supplies and overwrites this
        // inherited identity; settings and adjacent model metadata do not.
        #[allow(unsafe_code)]
        let mut network = unsafe { crate::qcom_provider::open(Path::new(&root), &digest) }?;
        network.warm_up()?;
        Ok(network)
    })
}
/// Explicit injected provider boundary for non-computing lifecycle validation.
/// Production entry always uses the native factory above, with no fallback.
#[cfg(feature = "neural-qcom")]
pub fn run_qcom_with_factory<F: crate::qcom_provider::Factory>(
    config: &Config,
    root: &Path,
    digest: &str,
    factory: &mut F,
    stop: &AtomicBool,
    mode: &mut impl FnMut() -> Mode,
    event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
) -> Result<(), Error> {
    crate::qcom::require_native_cached(root)?;
    run_qcom_leaf(config, stop, mode, event, || {
        let mut network = crate::qcom_provider::open_with(root, digest, factory)?;
        network.warm_up()?;
        Ok(network)
    })
}
#[cfg(feature = "neural-qcom")]
fn run_qcom_leaf<N: crate::neural_sequence::Network>(
    config: &Config,
    stop: &AtomicBool,
    mode: &mut impl FnMut() -> Mode,
    event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
    load: impl FnOnce() -> Result<N, Error>,
) -> Result<(), Error> {
    if !requested(config, mode())? || !flag(config, "neural_enabled")? {
        return Ok(());
    }
    let selected = selection(config)?;
    if !matches!(selected, Request::Auto | Request::Qcom) {
        return Err("QCOM child requires automatic/QCOM selection".into());
    }
    run_loaded(config, stop, mode, event, selected, "qcom", load)
}

fn run_loaded<N: crate::neural_sequence::Network>(
    config: &Config,
    stop: &AtomicBool,
    mode: &mut impl FnMut() -> Mode,
    event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
    selected: Request,
    backend: &str,
    load: impl FnOnce() -> Result<N, Error>,
) -> Result<(), Error> {
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
    let mut network = load()?;
    scheduling::configure()?;
    let fixture: Value =
        serde_json::from_str(include_str!("../../../upstream/signal-source.json"))?;
    let algorithm = fixture["files"]["tools/signal_analysis/neural_signal.py"]["sha256"]
        .as_str()
        .ok_or("missing neural source identity")?;
    let assets: Value =
        serde_json::from_str(include_str!("../../../upstream/signal-runtime.json"))?;
    let mut sequence = Sequence::default();
    let mut identity = session();
    // Model construction and all three warmups finish before this subscription.
    let mut client = VisionClient::new("camerad", VisionStream::Road, true)?;
    let began = now();
    event(
        "signalNeuralShadowLoaded",
        json!({"backend":backend,"algorithm_sha256":algorithm,"detector_sha256":assets["models"]["detector"]["sha256"],"classifier_sha256":assets["models"]["classifier"]["sha256"],"pid":std::process::id(),"control_permission":false,"assist_transport":false}),
    )?;
    let selected_mode = if config.directory == Path::new(DIRECTORY) {
        Some(mode())
    } else {
        None
    };
    let mut capture = CapturePolicy::default();
    let mut count = 0;
    let mut last_file = 0.;
    while !stop.load(Ordering::Relaxed)
        && requested(config, mode())?
        && flag(config, "neural_enabled")?
        && selection(config)? == selected
        && selected_mode.is_none_or(|selected| mode() == selected)
        && config
            .duration
            .is_none_or(|limit| now() - began < limit.as_secs_f64())
        && config.frames.is_none_or(|limit| count < limit)
    {
        if !client.is_connected() && !client.connect()? {
            sequence = Sequence::default();
            identity = session();
            thread::sleep(Duration::from_millis(200));
            continue;
        }
        let Some(frame) = client.receive(Duration::from_millis(100))? else {
            sequence = Sequence::default();
            identity = session();
            continue;
        };
        let meta = *frame.metadata();
        let begin = now();
        let cpu_start = cpu();
        let input_now = runtime::monotonic_ns();
        let (gap, reset) = match capture.receive(meta.frame_id, meta.timestamp_eof, input_now) {
            Capture::Duplicate => {
                thread::sleep(Duration::from_millis(5));
                continue;
            }
            Capture::Stale => {
                sequence = Sequence::default();
                identity = session();
                thread::sleep(Duration::from_millis(50));
                continue;
            }
            Capture::Process { gap, reset } => (gap, reset),
        };
        if reset {
            sequence = Sequence::default();
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
            sequence = Sequence::default();
            identity = session();
            continue;
        }
        let result = sequence.process(&rgb, meta.timestamp_eof as f64 / 1e9, &mut network)?;
        let tracks = transport::neural_tracks(
            result["tracks"]
                .as_array()
                .ok_or("missing neural sequence tracks")?,
        )?;
        let record = json!({"mode":"neural_display_only","neural":true,"backend":backend,"algorithm_sha256":algorithm,"stream":"road","reference_width":1344,"reference_height":760,"frame_id":meta.frame_id,"frame_gap":gap,"timestamp_eof":meta.timestamp_eof,"session":identity,"detected":result["detected"],"input_age_ms":(i128::from(input_now)-i128::from(meta.timestamp_eof)) as f64/1e6,"result_age_ms":(i128::from(runtime::monotonic_ns())-i128::from(meta.timestamp_eof)) as f64/1e6,"recorded_monotonic_ns":runtime::monotonic_ns(),"work_ms":(now()-begin)*1000.,"cpu_ms":(cpu()-cpu_start)*1000.,"tracks":tracks,"start_guard_transport":observation_flag(config,"start_guard_enabled",mode())?,"control_permission":false,"assist_transport":false});
        event("signalNeuralShadow", record.clone())?;
        publish(
            config,
            &record,
            &mut |name| observation_flag(config, name, mode()),
            event,
        )?;
        if now() - last_file >= 1. {
            latest(config, &record)?;
            last_file = now();
        }
        count += 1;
        thread::sleep(Duration::from_secs_f64(worker::next_delay(
            now() - begin,
            cpu() - cpu_start,
        )));
    }
    Ok(())
}

/// Neural publication phases; display failures remain ordinary worker failures.
pub fn publish(
    config: &Config,
    record: &Value,
    flag: &mut impl FnMut(&str) -> Result<bool, Error>,
    event: &mut impl FnMut(&str, Value) -> Result<(), Error>,
) -> Result<(), Error> {
    let start = (|| {
        if flag("start_guard_enabled")? {
            transport::start(record)
                .and_then(|payload| transport::write(&config.start, &payload, 32768))?;
        }
        Ok::<_, Error>(())
    })();
    if let Err(error) = start {
        event(
            "signalStartObservationPublishFailed",
            json!({"error":error.to_string()}),
        )?;
    }
    if flag("display_enabled")? {
        transport::write(&config.display, &transport::display(record)?, 8192)?;
    }
    Ok(())
}
