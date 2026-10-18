//! Missing-metadata collector QA, with independent source policy and isolated IPC.
//! Library run leaves caller environment/signals unchanged; Msgq state lives only
//! in version-runtime-worker. The CLI alone owns its executable-root SIGINT guard.
use crate::{
    path_resolve::resolve_non_strict,
    qa_process::{CaptureError, CommandSpec, NativeRunner},
    seal::{digest, Result},
    version_source,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{FileTypeExt, MetadataExt},
    },
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};
pub(crate) mod process;
mod worker;

pub struct Options {
    pub probe: PathBuf,
    pub collector: PathBuf,
    pub output: PathBuf,
    pub environment: BTreeMap<OsString, OsString>,
}
pub fn parse(
    args: &[OsString],
    cwd: &Path,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<Options> {
    if args.len() != 3 {
        return Err("expected PROBE COLLECTOR OUTPUT (exactly three positional arguments)".into());
    }
    let path = |arg: &OsString| resolve_non_strict(&cwd.join(arg));
    Ok(Options {
        probe: path(&args[0])?,
        collector: path(&args[1])?,
        output: path(&args[2])?,
        environment: environment.clone(),
    })
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn pretty(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(value)? + "\n")?;
    Ok(())
}
fn invalidate_success(output: &Path) -> Result<()> {
    match fs::remove_file(output.join("manifest.json")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
#[derive(Serialize, Deserialize)]
struct Request {
    repository: Vec<u8>,
    probe: Vec<u8>,
    collector: Vec<u8>,
    output: Vec<u8>,
    prefix: String,
    nonce: String,
    worker_sha256: String,
    controller_sha256: String,
    sources: BTreeMap<String, String>,
}
fn path(bytes: Vec<u8>) -> PathBuf {
    OsString::from_vec(bytes).into()
}
fn sources(repository: &Path) -> Result<BTreeMap<String, String>> {
    let mut sources = version_source::sources(repository)?;
    for (name, expected) in [
        (
            "rust/tools/logmessaged_native.py",
            "3366e64eb36b0bad9b10023c46217f61c66aae0fe823120ae23d597d2456c872",
        ),
        (
            "rust/tools/logmessaged_reference.py",
            "ce6d6764911daaab1e9e5ce70152d9f5efc9b5ea006482064bb8192d36f735e3",
        ),
        (
            "openpilot/cereal/services.py",
            "a0745e2ef00036f0eef08b453a4798b4281549ea9ac6dc91c8b348ef76131edb",
        ),
        (
            "openpilot/cereal/log.capnp",
            "fdecc98871c0ebc11417a558a600de6ff96cc51c2ce620122d39b2cee55f1620",
        ),
    ] {
        let actual = digest(&repository.join(name))?;
        require(
            actual == expected,
            &format!("changed Version collector source: {name}"),
        )?;
        sources.insert(name.into(), actual);
    }
    Ok(sources)
}
struct Namespace {
    _directory: tempfile::TempDir,
    prefix: String,
    endpoint: PathBuf,
    endpoint_identity: Option<(u64, u64)>,
}
impl Namespace {
    fn new() -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("msgq_version-collector-")
            .tempdir_in("/dev/shm")?;
        let prefix = directory
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("msgq_"))
            .ok_or("invalid private Version namespace")?
            .to_owned();
        let endpoint = Path::new("/tmp").join(format!("logmessage{prefix}"));
        match fs::symlink_metadata(&endpoint) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => {
                return Err("Version collector endpoint already exists or cannot be checked".into())
            }
        }
        Ok(Self {
            _directory: directory,
            prefix,
            endpoint,
            endpoint_identity: None,
        })
    }
    fn observe_endpoint(&mut self) {
        match fs::symlink_metadata(&self.endpoint) {
            Ok(metadata)
                if self.endpoint_identity.is_none() && metadata.file_type().is_socket() =>
            {
                self.endpoint_identity = Some((metadata.dev(), metadata.ino()));
            }
            _ => {}
        }
    }
}
impl Drop for Namespace {
    fn drop(&mut self) {
        // The worker's group owner is always dropped/reaped BEFORE this guard.
        // Do not unlink a replacement inode or an endpoint present before launch.
        match (self.endpoint_identity, fs::symlink_metadata(&self.endpoint)) {
            (Some(identity), Ok(metadata))
                if metadata.file_type().is_socket()
                    && identity == (metadata.dev(), metadata.ino()) =>
            {
                let _ = fs::remove_file(&self.endpoint);
            }
            _ => {}
        }
    }
}
pub fn run(repository: &Path, options: &Options, worker: &Path) -> Result<Value> {
    run_controlled(repository, options, worker, &AtomicBool::new(false))
}
fn run_controlled(
    repository: &Path,
    options: &Options,
    worker: &Path,
    stop: &AtomicBool,
) -> Result<Value> {
    // A new attempt invalidates previous success even if source/worker admission
    // or the exclusive collector directory check fails before worker startup.
    invalidate_success(&options.output)?;
    let source_hashes = sources(repository)?;
    let worker_hash = digest(worker)?;
    fs::create_dir_all(&options.output)?;
    // Preserve the original exclusive Peer directory boundary, including reuse.
    fs::create_dir(options.output.join("collector"))?;
    let request_directory = tempfile::Builder::new()
        .prefix("version-collector-request-")
        .tempdir()?;
    let mut namespace = Namespace::new()?;
    let nonce = request_directory
        .path()
        .file_name()
        .ok_or("missing request nonce")?
        .to_str()
        .ok_or("private Version request nonce is not Unicode")?
        .to_owned();
    let request = Request {
        repository: repository.as_os_str().as_bytes().to_vec(),
        probe: options.probe.as_os_str().as_bytes().to_vec(),
        collector: options.collector.as_os_str().as_bytes().to_vec(),
        output: options.output.as_os_str().as_bytes().to_vec(),
        prefix: namespace.prefix.clone(),
        nonce: nonce.clone(),
        worker_sha256: worker_hash,
        controller_sha256: digest(&std::env::current_exe()?)?,
        sources: source_hashes,
    };
    let request_path = request_directory.path().join("request.json");
    fs::write(&request_path, serde_json::to_vec(&request)?)?;
    let mut command = CommandSpec::new_os(
        vec![
            worker.as_os_str().to_owned(),
            "--request".into(),
            request_path.into_os_string(),
        ],
        Duration::from_secs(90),
    );
    let mut environment = options.environment.clone();
    environment.insert("OPENPILOT_PREFIX".into(), namespace.prefix.clone().into());
    command.env_remove = std::env::vars_os()
        .map(|(key, _)| key)
        .filter(|key| !environment.contains_key(key))
        .collect();
    command.set_os_env(environment);
    command.inherit_stdin = true;
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    // The runner retains its leader through group cleanup, including cancellation.
    // IPC remains owned until that cleanup has returned.
    let result = NativeRunner.run_controlled(&command, Instant::now() + command.timeout, stop);
    namespace.observe_endpoint();
    let captured = match result {
        Ok(captured) => captured,
        Err(error) => {
            if let Some(CaptureError::Timeout { stdout, stderr, .. }) =
                error.downcast_ref::<CaptureError>()
            {
                fs::write(options.output.join("worker.stdout"), stdout)?;
                fs::write(options.output.join("worker.stderr"), stderr)?;
            }
            return Err(error);
        }
    };
    fs::write(options.output.join("worker.stdout"), &captured.stdout)?;
    fs::write(options.output.join("worker.stderr"), &captured.stderr)?;
    require(
        captured.code == 0,
        &format!(
            "Version collector worker exited {}: {}",
            captured.code,
            String::from_utf8_lossy(&captured.stderr)
        ),
    )?;
    let reply: Value = serde_json::from_slice(&captured.stdout)?;
    require(
        reply["nonce"] == nonce,
        "Version collector reply nonce mismatch",
    )?;
    let report = reply
        .get("report")
        .ok_or("missing live Version collector report")?
        .clone();
    require(
        report["result"] == "PASS",
        "Version collector worker did not pass",
    )?;
    process::interrupted(stop)?;
    let publication = pretty(&options.output.join("manifest.json"), &report);
    if publication.is_err() || stop.load(std::sync::atomic::Ordering::Relaxed) {
        invalidate_success(&options.output)?;
    }
    publication?;
    process::interrupted(stop)?;
    // Keep the namespace guard live through reply validation and close it only
    // after worker pipes, collector maps and the owned group have all finished.
    Ok(report)
}
/// Executable-root API for check-version-collector. No help-option extension:
/// the original treats every argument as a positional filesystem path.
pub fn cli() -> Result<()> {
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let options = parse(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &std::env::current_dir()?,
        &std::env::vars_os().collect(),
    )?;
    let worker = std::env::current_exe()?
        .parent()
        .ok_or("checker has no executable directory")?
        .join("version-runtime-worker");
    let stop = Arc::new(AtomicBool::new(false));
    let registration = signal_hook::flag::register(signal_hook::consts::SIGINT, stop.clone())?;
    let result = run_controlled(&root, &options, &worker, &stop);
    signal_hook::low_level::unregister(registration);
    if stop.load(std::sync::atomic::Ordering::Relaxed) {
        invalidate_success(&options.output)?;
        signal_hook::low_level::emulate_default_handler(signal_hook::consts::SIGINT)?;
    }
    println!("{}", serde_json::to_string_pretty(&result?)?);
    Ok(())
}
/// Private native worker entrypoint. Call only from version-runtime-worker.
pub fn worker_main() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args == [OsString::from("--source-once")] {
        return worker::source_once();
    }
    require(
        args.len() == 2 && args[0] == "--request",
        "expected private --request FILE",
    )?;
    let request: Request = serde_json::from_slice(&fs::read(&args[1])?)?;
    require(
        digest(&std::env::current_exe()?)? == request.worker_sha256,
        "Version worker executable identity changed",
    )?;
    let repository = path(request.repository.clone());
    require(
        sources(&repository)? == request.sources,
        "Version source changed between controller and worker",
    )?;
    require(
        std::env::var("OPENPILOT_PREFIX")? == request.prefix,
        "Version worker namespace mismatch",
    )?;
    let stop = Arc::new(AtomicBool::new(false));
    let registration = signal_hook::flag::register(signal_hook::consts::SIGINT, stop.clone())?;
    let result = worker::run(&request, &stop);
    signal_hook::low_level::unregister(registration);
    let report = result?;
    println!(
        "{}",
        serde_json::to_string(&json!({"nonce":request.nonce,"report":report}))?
    );
    Ok(())
}
