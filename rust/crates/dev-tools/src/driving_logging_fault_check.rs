//! Link-fault parity for the unchanged real driving model, collector and IPC.
//! The final-link wrapper is built by the cloud job; this checker never builds
//! or modifies the production executable and never runs a Python oracle.
use crate::{
    driving_daemon_check::{self, Options as DrivingOptions},
    driving_daemon_reference::Original,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::Duration,
};

const LIMIT: u64 = 16_000_000;

pub struct Options {
    pub fault_binary: PathBuf,
    pub collector: PathBuf,
    pub peer: PathBuf,
    pub catalog: PathBuf,
    pub original_dir: PathBuf,
    pub original_manifest_sha256: String,
    pub root: PathBuf,
    pub output: PathBuf,
}

fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn read_regular(path: &Path, max: u64) -> Result<Vec<u8>> {
    let descriptor = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    let mut file = File::from(descriptor);
    let metadata = file.metadata()?;
    require(
        metadata.is_file() && metadata.len() <= max,
        "invalid fault evidence file",
    )?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    require(
        bytes.len() as u64 == metadata.len(),
        "fault evidence file changed",
    )?;
    Ok(bytes)
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

struct Pin {
    path: PathBuf,
    file: File,
    max: u64,
    identity: (u64, u64, u64),
    digest: String,
}
impl Pin {
    fn open(path: PathBuf, max: u64) -> Result<Self> {
        let descriptor = rustix::fs::open(
            &path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(descriptor);
        let metadata = file.metadata()?;
        require(
            metadata.is_file() && metadata.len() <= max,
            "invalid fault input",
        )?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(max + 1)
            .read_to_end(&mut bytes)?;
        require(
            bytes.len() as u64 == metadata.len(),
            "fault input changed while binding",
        )?;
        Ok(Self {
            path,
            file,
            max,
            identity: (metadata.dev(), metadata.ino(), metadata.len()),
            digest: sha(&bytes),
        })
    }
    fn verify(&self) -> Result<()> {
        let held = self.file.metadata()?;
        let path = fs::symlink_metadata(&self.path)?;
        let identity = |value: &std::fs::Metadata| (value.dev(), value.ino(), value.len());
        require(
            path.is_file()
                && !path.file_type().is_symlink()
                && identity(&held) == self.identity
                && identity(&path) == self.identity
                && sha(&read_regular(&self.path, self.max)?) == self.digest,
            "fault input changed during live comparison",
        )
    }
}

struct Output {
    path: PathBuf,
    directory: File,
    identity: (u64, u64),
}
impl Output {
    fn create(path: &Path) -> Result<Self> {
        fs::create_dir(path)?;
        let directory = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let metadata = directory.metadata()?;
        Ok(Self {
            path: path.into(),
            directory,
            identity: (metadata.dev(), metadata.ino()),
        })
    }
    fn verify(&self) -> Result<()> {
        let held = self.directory.metadata()?;
        let path = fs::symlink_metadata(&self.path)?;
        require(
            path.is_dir()
                && !path.file_type().is_symlink()
                && (held.dev(), held.ino()) == self.identity
                && (path.dev(), path.ino()) == self.identity,
            "fault evidence output directory replaced",
        )
    }
    fn write_report(&self, report: &Value) -> Result<File> {
        self.verify()?;
        let mut file = File::from(rustix::fs::openat(
            &self.directory,
            "report.json",
            rustix::fs::OFlags::WRONLY
                | rustix::fs::OFlags::CREATE
                | rustix::fs::OFlags::EXCL
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )?);
        file.write_all(format!("{}\n", serde_json::to_string_pretty(report)?).as_bytes())?;
        file.sync_all()?;
        Ok(file)
    }
    fn remove_owned_report(&self, receipt: &File) {
        if self.verify().is_ok() {
            let path = self.path.join("report.json");
            if let (Ok(current), Ok(held)) = (fs::symlink_metadata(&path), receipt.metadata()) {
                if current.is_file()
                    && !current.file_type().is_symlink()
                    && (current.dev(), current.ino()) == (held.dev(), held.ino())
                {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }
}

fn source_failure(original: &Original) -> Result<Value> {
    let value: Value = serde_json::from_slice(&original.read("source-fault/source-failure.json")?)?;
    require(
        value["ordinary_errno"] == 88
            && value["runtime_timing"] == "exception suppressed after reset",
        "independent source-fault policy changed",
    )?;
    let recovered = value["recovered"]
        .as_array()
        .ok_or("source recovery missing")?;
    require(
        recovered.len() == 1
            && recovered[0]["frames"] == 1
            && recovered[0]["metrics"]["inference_ms"]["mean"] == 20.0
            && recovered[0]["metrics"]["inference_ms"]["count"] == 1,
        "independent source recovery changed",
    )?;
    Ok(value)
}

fn mode(options: &Options, mode: &str) -> Result<Value> {
    let output = options.output.join(mode);
    let audit = output.join("audit");
    let driving = DrivingOptions {
        binary: options.fault_binary.clone(),
        catalog: options.catalog.clone(),
        original_dir: options.original_dir.clone(),
        original_manifest_sha256: options.original_manifest_sha256.clone(),
        collector: Some(options.collector.clone()),
        output: output.clone(),
        root: options.root.clone(),
        peer: Some(options.peer.clone()),
        compiler: None,
    };
    let report = driving_daemon_check::run_link_fault_dual(&driving, mode, &audit)?;
    let run = report["runs"]
        .as_array()
        .filter(|rows| rows.len() == 1)
        .and_then(|rows| rows.first())
        .ok_or("fault run lacked one real dual scenario")?;
    require(
        run["resolution"] == json!([1344, 760])
            && run["streams"] == "dual"
            && run["messages"] == 33
            && run["frames"] == 12
            && run["logging"]["result"] == "pass",
        "fault run lost real model/collector parity",
    )?;
    let files = fs::read_dir(&audit)?.collect::<std::io::Result<Vec<_>>>()?;
    require(
        files.len() == 1,
        "link wrapper did not record exactly one fault",
    )?;
    let file = &files[0];
    let name = file.file_name();
    let name = name.to_str().ok_or("non-UTF8 fault packet filename")?;
    require(
        name.strip_suffix(".packet")
            .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|byte| byte.is_ascii_digit())),
        "fault packet filename changed",
    )?;
    let packet = read_regular(&file.path(), LIMIT)?;
    require(packet.first() == Some(&20), "fault packet priority changed")?;
    let failed: Value = serde_json::from_slice(&packet[1..])?;
    require(
        failed["msg"]["event"] == "runtimeTiming",
        "fault did not target runtimeTiming",
    )?;
    let logs: Vec<Value> = serde_json::from_slice(&read_regular(
        &output.join("1344-dual/logging/logMessage.json"),
        LIMIT,
    )?)?;
    let following = logs
        .iter()
        .filter(|record| {
            record["process"] == failed["process"]
                && record["msg"]["event"] == "runtimeTiming"
                && record["msg"]["mono_time"]
                    .as_f64()
                    .zip(failed["msg"]["mono_time"].as_f64())
                    .is_some_and(|(next, failed)| next > failed)
        })
        .map(|record| &record["msg"])
        .next()
        .ok_or("no timing publication after fault")?;
    let mut frames = vec![2_u64];
    for step in run["trace"].as_array().ok_or("fault trace absent")? {
        frames.push(
            step["frame_id"]
                .as_u64()
                .ok_or("fault trace frame ID absent")?,
        );
    }
    let failed_id = failed["msg"]["frame_id"]
        .as_u64()
        .ok_or("failed frame ID absent")?;
    let next_id = following["frame_id"]
        .as_u64()
        .ok_or("next timing frame ID absent")?;
    let expected = frames
        .iter()
        .filter(|&&frame| failed_id < frame && frame <= next_id)
        .count();
    require(
        expected > 0
            && following["frames"].as_u64() == Some(expected as u64)
            && following["metrics"].as_object().is_some_and(|metrics| {
                !metrics.is_empty()
                    && metrics
                        .values()
                        .all(|metric| metric["count"].as_u64() == Some(expected as u64))
            }),
        "timing accumulator did not recover exactly the next interval",
    )?;
    Ok(json!({"mode":mode, "fault_file":file.path(),
        "fault_sha256":sha(&packet), "failed_frames":failed["msg"]["frames"],
        "next_interval_frames":following["frames"], "expected_next_frames":expected,
        "model_messages":run["messages"], "compared_model_fields":run["compared_fields"]}))
}

fn ordinary(options: &Options) -> Result<i32> {
    let audit = options.output.join("ordinary-audit");
    fs::create_dir(&audit)?;
    let prefix = format!("model-log-fault-{}", std::process::id());
    let mut spec = CommandSpec::new_os(
        vec![
            options.fault_binary.as_os_str().to_owned(),
            "--trusted-catalog".into(),
            options.catalog.as_os_str().to_owned(),
        ],
        Duration::from_secs(5),
    );
    // The unchanged original ordinary ENOTSOCK capture was 51 stderr bytes.
    // Preserve the full stream while bounding a faulty child that writes forever.
    spec.stdout_limit = 1024 * 1024;
    spec.stderr_limit = 1024 * 1024;
    let mut environment = BTreeMap::<OsString, OsString>::new();
    environment.insert("OPENPILOT_PREFIX".into(), prefix.into());
    environment.insert("MODEL_LOG_FAULT".into(), "ordinary".into());
    environment.insert("MODEL_LOG_FAULT_AUDIT".into(), audit.as_os_str().to_owned());
    spec.set_os_env(environment);
    let result = NativeRunner.run(&spec)?;
    fs::write(options.output.join("ordinary.stdout"), &result.stdout)?;
    fs::write(options.output.join("ordinary.stderr"), &result.stderr)?;
    require(
        result.code == 1
            && result
                .stderr
                .windows(b"Socket operation on non-socket".len())
                .any(|window| window == b"Socket operation on non-socket"),
        "ordinary ENOTSOCK path changed",
    )?;
    let files = fs::read_dir(&audit)?.collect::<std::io::Result<Vec<_>>>()?;
    require(files.len() == 1, "ordinary link fault did not fire once")?;
    let packet = read_regular(&files[0].path(), LIMIT)?;
    require(
        packet.first() == Some(&30) || packet.first() == Some(&20),
        "ordinary fault packet has unexpected priority",
    )?;
    let message: Value = serde_json::from_slice(&packet[1..])?;
    require(
        message["msg"]["event"] != "runtimeTiming",
        "ordinary fault incorrectly targeted runtimeTiming",
    )?;
    Ok(result.code)
}

/// Real model, queue, VisionIPC and collector paths are exercised in both
/// timings. The original source-only exception policy is a separately sealed
/// observation; source Python remains in place until all callers migrate.
pub fn run(options: &Options) -> Result<Value> {
    let inputs = [
        Pin::open(options.fault_binary.clone(), 512_000_000)?,
        Pin::open(options.collector.clone(), 512_000_000)?,
        Pin::open(options.peer.clone(), 128_000_000)?,
        Pin::open(
            options.root.join("rust/tools/model_logging_fault.cc"),
            1_000_000,
        )?,
    ];
    let original = Original::open(&options.original_dir, &options.original_manifest_sha256)?;
    let source = source_failure(&original)?;
    let output = Output::create(&options.output)?;
    let first = mode(options, "timing-error")?;
    output.verify()?;
    let second = mode(options, "timing-full")?;
    output.verify()?;
    let code = ordinary(options)?;
    output.verify()?;
    original.verify()?;
    for input in &inputs {
        input.verify()?;
    }
    let report = json!({"result":"pass", "source":source, "native":[first,second],
        "ordinary_exit":code, "scope":"link-only ENOTSOCK/EAGAIN with unchanged model, VisionIPC, msgq and collector"});
    let receipt = output.write_report(&report)?;
    let finalization = (|| {
        output.verify()?;
        original.verify()?;
        for input in &inputs {
            input.verify()?;
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })();
    if let Err(error) = finalization {
        output.remove_owned_report(&receipt);
        return Err(error);
    }
    Ok(report)
}

pub fn cli() -> Result<()> {
    let mut fault_binary = None;
    let mut collector = None;
    let mut peer = None;
    let mut catalog = None;
    let mut original_dir = None;
    let mut original_manifest_sha256 = None;
    let mut root = None;
    let mut output = None;
    let mut arguments = std::env::args_os().skip(1);
    while let Some(key) = arguments.next() {
        let value = arguments
            .next()
            .ok_or("missing logging fault option value")?;
        let slot = match key.to_str() {
            Some("--fault-binary") => &mut fault_binary,
            Some("--collector") => &mut collector,
            Some("--peer") => &mut peer,
            Some("--catalog") => &mut catalog,
            Some("--original-dir") => &mut original_dir,
            Some("--original-manifest-sha256") => &mut original_manifest_sha256,
            Some("--root") => &mut root,
            Some("--output") => &mut output,
            _ => return Err("unknown logging fault option".into()),
        };
        let _ = slot.replace(value);
    }
    let required = |value: Option<OsString>| -> Result<PathBuf> {
        Ok(PathBuf::from(
            value.ok_or("required logging fault option missing")?,
        ))
    };
    let options = Options {
        fault_binary: required(fault_binary)?,
        collector: required(collector)?,
        peer: required(peer)?,
        catalog: required(catalog)?,
        original_dir: required(original_dir)?,
        original_manifest_sha256: original_manifest_sha256
            .ok_or("original manifest digest missing")?
            .into_string()
            .map_err(|_| "original manifest digest must be UTF-8")?,
        root: required(root)?,
        output: required(output)?,
    };
    println!("{}", serde_json::to_string(&run(&options)?)?);
    Ok(())
}
