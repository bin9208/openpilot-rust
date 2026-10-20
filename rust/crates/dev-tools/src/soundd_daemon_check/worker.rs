use super::{
    identity::{self, Artifact},
    input_packet, queue_capacity, require, validate_events, validate_samples, Event, Mode, Options,
    SampleStage,
};
use crate::{
    qa_process::{CommandSpec, NativeRunner},
    seal::Result,
};
use openpilot_msgq::Publisher;
use rustix::process::{waitid, Pid, Signal, WaitId, WaitIdOptions};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{self, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, PermissionsExt},
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
fn now() -> u64 {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}
fn interrupted(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::new(io::ErrorKind::Interrupted, "soundd checker interrupted").into())
    } else {
        Ok(())
    }
}
fn read_events(output: &Path) -> Result<Vec<Event>> {
    match fs::read_to_string(output.join("events.jsonl")) {
        Ok(s) => s
            .lines()
            .map(|s| serde_json::from_str(s).map_err(Into::into))
            .collect(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}
fn count(output: &Path, name: &str) -> Result<usize> {
    Ok(read_events(output)?
        .iter()
        .filter(|e| e.event == name)
        .count())
}
fn wait_for(mut predicate: impl FnMut() -> Result<bool>, seconds: u64) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(seconds);
    while !predicate()? {
        require(
            Instant::now() < end,
            "soundd predicate exceeded original deadline",
        )?;
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}
struct Namespace {
    path: PathBuf,
    identity: (u64, u64),
    prefix: OsString,
}
struct PrivateRoot {
    path: PathBuf,
    identity: (u64, u64),
}
impl PrivateRoot {
    fn new() -> Result<Self> {
        let temp = tempfile::Builder::new().prefix("sound-native-").tempdir()?;
        let path = temp.keep();
        let metadata = fs::symlink_metadata(&path)?;
        Ok(Self {
            path,
            identity: (metadata.dev(), metadata.ino()),
        })
    }
}
impl Drop for PrivateRoot {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path).is_ok_and(|m| {
            m.is_dir() && !m.file_type().is_symlink() && (m.dev(), m.ino()) == self.identity
        }) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
impl Namespace {
    fn new() -> Result<Self> {
        let temp = tempfile::Builder::new()
            .prefix("msgq_sound-")
            .tempdir_in("/dev/shm")?;
        let path = temp.keep();
        let m = fs::symlink_metadata(&path)?;
        let prefix = path
            .file_name()
            .and_then(OsStr::to_str)
            .and_then(|s| s.strip_prefix("msgq_"))
            .ok_or("bad sound namespace")?
            .into();
        Ok(Self {
            path,
            identity: (m.dev(), m.ino()),
            prefix,
        })
    }
}
impl Drop for Namespace {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path).is_ok_and(|m| {
            m.is_dir() && !m.file_type().is_symlink() && (m.dev(), m.ino()) == self.identity
        }) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
/// Direct daemon inherits the outer worker's owned group. No numeric signalling
/// is possible after wait() releases this leader identity.
struct Daemon {
    child: Option<Child>,
    pid: Pid,
}
impl Daemon {
    fn spawn(
        binary: &Path,
        base: &Path,
        library: &Path,
        env: &BTreeMap<OsString, OsString>,
        output: &Path,
        mode: Mode,
    ) -> Result<Self> {
        let stdout = File::create(output.join("stdout.log"))?;
        let stderr = File::create(output.join("stderr.log"))?;
        let mut command = Command::new(binary);
        command.args([
            OsStr::new("--assets"),
            base.as_os_str(),
            OsStr::new("--portaudio-library"),
            library.as_os_str(),
        ]);
        if matches!(mode, Mode::Bounded | Mode::Retry) {
            command.args(["--cycles", "5"]);
        }
        let child = command
            .env_clear()
            .envs(env)
            .stdout(stdout)
            .stderr(stderr)
            .spawn()?;
        let pid = Pid::from_child(&child);
        Ok(Self {
            child: Some(child),
            pid,
        })
    }
    fn exited(&self) -> Result<bool> {
        match waitid(
            WaitId::Pid(self.pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(s) => Ok(s.is_some()),
            Err(rustix::io::Errno::INTR) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
    fn signal(&self) -> Result<()> {
        Ok(rustix::process::kill_process(self.pid, Signal::TERM)?)
    }
    fn wait(&mut self, seconds: u64) -> Result<i32> {
        wait_for(|| self.exited(), seconds)?;
        let s = self.child.take().ok_or("daemon already reaped")?.wait()?;
        Ok(s.code().unwrap_or_else(|| -s.signal().unwrap_or(1)))
    }
}
impl Drop for Daemon {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = rustix::process::kill_process(self.pid, Signal::KILL);
            let _ = child.wait();
        }
    }
}
fn samples(output: &Path, stage: SampleStage, start: usize) -> Result<()> {
    let raw = fs::read(output.join("samples.f32"))?;
    require(
        (raw.len() / 4).is_multiple_of(4096),
        "sample capture does not reshape",
    )?;
    let offset = start
        .checked_mul(4096 * 4)
        .ok_or("sample offset overflow")?
        .min(raw.len() / 4 * 4);
    validate_samples(stage, &raw[offset..])
}
struct Feed {
    publishers: BTreeMap<&'static str, Publisher>,
    packets: Vec<Value>,
}
impl Feed {
    fn new() -> Result<Self> {
        let mut publishers = BTreeMap::new();
        for topic in ["selfdriveState", "carrotMan", "soundPressure", "carState"] {
            publishers.insert(
                topic,
                Publisher::for_runtime(topic, queue_capacity(topic)?)?,
            );
        }
        Ok(Self {
            publishers,
            packets: Vec::new(),
        })
    }
    fn send(&mut self, output: &Path, topic: &str, values: Value) -> Result<()> {
        let raw = input_packet(topic, &values, now())?;
        fs::write(
            output.join(format!("input-{}.capnp", self.packets.len())),
            &raw,
        )?;
        self.publishers
            .get_mut(topic)
            .ok_or("missing publisher")?
            .send(&raw)?;
        self.packets
            .push(json!({"topic":topic,"values":values,"time":now() as f64/1e9}));
        Ok(())
    }
    fn ready(&mut self) -> Result<()> {
        for topic in ["selfdriveState", "carrotMan", "soundPressure", "carState"] {
            let publisher = self.publishers.get_mut(topic).ok_or("missing publisher")?;
            // Original wait_for_readers uses one millisecond polling and five seconds per publisher.
            let end = Instant::now() + Duration::from_secs(5);
            while !publisher.readers_caught_up() {
                require(
                    Instant::now() < end,
                    "soundd publisher reader readiness timeout",
                )?;
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        Ok(())
    }
}
fn live(output: &Path, params: &Path, feed: &mut Feed, daemon: &mut Daemon) -> Result<i32> {
    feed.ready()?;
    wait_for(|| Ok(count(output, "callback")? >= 1), 5)?;
    feed.send(
        output,
        "soundPressure",
        json!({"soundPressureWeightedDb":54.0}),
    )?;
    wait_for(|| Ok(count(output, "poll")? >= 3), 5)?;
    feed.send(
        output,
        "selfdriveState",
        json!({"enabled":false,"alertSound":1}),
    )?;
    wait_for(|| Ok(count(output, "callback")? >= 5), 5)?;
    samples(output, SampleStage::Engage, 0)?;
    feed.send(
        output,
        "selfdriveState",
        json!({"enabled":false,"alertSound":7}),
    )?;
    let before = count(output, "callback")?;
    wait_for(|| Ok(count(output, "callback")? >= before + 3), 5)?;
    fs::write(params.join("SoundLanguageSetting"), b"ko")?;
    wait_for(
        || Ok(fs::read_to_string(output.join("stderr.log"))?.contains("language changed: ko")),
        3,
    )?;
    let before = count(output, "callback")?;
    wait_for(|| Ok(count(output, "callback")? >= before + 2), 5)?;
    samples(output, SampleStage::Language, 0)?;
    feed.send(
        output,
        "selfdriveState",
        json!({"enabled":false,"alertSound":0}),
    )?;
    feed.send(output, "carrotMan", json!({"leftSec":3}))?;
    let before = count(output, "callback")?;
    wait_for(|| Ok(count(output, "callback")? >= before + 2), 5)?;
    feed.send(output, "carrotMan", json!({"leftSec":-1}))?;
    feed.send(
        output,
        "selfdriveState",
        json!({"enabled":false,"alertSound":0}),
    )?;
    let before = count(output, "callback")?;
    wait_for(|| Ok(count(output, "callback")? >= before + 2), 5)?;
    let start = count(output, "callback")?;
    feed.send(
        output,
        "carState",
        json!({"buttonEvents":[{"type":"mainCruise","pressed":true}]}),
    )?;
    wait_for(|| Ok(count(output, "callback")? >= start + 27), 4)?;
    samples(output, SampleStage::MainHeld, start)?;
    feed.send(
        output,
        "carState",
        json!({"buttonEvents":[{"type":"mainCruise","pressed":false}]}),
    )?;
    feed.send(
        output,
        "selfdriveState",
        json!({"enabled":true,"alertSound":0}),
    )?;
    let start = count(output, "callback")?;
    wait_for(|| Ok(count(output, "callback")? >= start + 65), 7)?;
    samples(output, SampleStage::EnabledTimeout, start)?;
    daemon.signal()?;
    daemon.wait(2)
}
fn scenario(
    binary: &Path,
    library: &Path,
    output: &Path,
    mode: Mode,
    helper: &Path,
    prefix: &OsStr,
    temporary: &Path,
) -> Result<Value> {
    // Only this private single-threaded worker mutates msgq's process-global prefix.
    std::env::set_var("OPENPILOT_PREFIX", prefix);
    let mut spec = CommandSpec::new_os(
        vec![helper.into(), temporary.into(), "48000".into(), "1".into()],
        Duration::from_secs(30),
    );
    spec.inherit_stdin = true;
    let assets = NativeRunner.run_in_current_group(&spec)?;
    io::stdout().write_all(&assets.stdout)?;
    io::stdout().flush()?;
    io::stderr().write_all(&assets.stderr)?;
    io::stderr().flush()?;
    require(
        assets.code == 0,
        format!("fixture-assets child exited {}", assets.code),
    )?;
    let base = temporary.join("openpilot/selfdrive/assets");
    let params_root = temporary.join("params");
    let params = params_root.join(prefix);
    fs::create_dir_all(&params)?;
    for (key, value) in [
        ("SoundVolumeAdjustEngage", "37"),
        ("SoundVolumeAdjust", "73"),
        ("SoundLanguageSetting", "en"),
    ] {
        fs::write(params.join(key), value)?;
    }
    let mut env: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
    for (k, v) in [
        ("PARAMS_ROOT", params_root.into_os_string()),
        (
            "SOUND_FIXTURE_EVENTS",
            output.join("events.jsonl").into_os_string(),
        ),
        (
            "SOUND_FIXTURE_SAMPLES",
            output.join("samples.f32").into_os_string(),
        ),
        ("LOGPRINT", OsString::from("info")),
    ] {
        env.insert(k.into(), v);
    }
    match mode {
        Mode::Retry => {
            env.insert("SOUND_FIXTURE_FAIL_OPEN".into(), "2".into());
        }
        Mode::Exhausted | Mode::StopRetry => {
            env.insert("SOUND_FIXTURE_FAIL_OPEN".into(), "10".into());
        }
        Mode::Inactive => {
            env.insert("SOUND_FIXTURE_INACTIVE".into(), "1".into());
        }
        Mode::StartFailed => {
            env.insert("SOUND_FIXTURE_FAIL_START".into(), "1".into());
        }
        _ => {}
    }
    let mut feed = Feed::new()?;
    let mut daemon = Daemon::spawn(binary, &base, library, &env, output, mode)?;
    let pid = daemon.pid.as_raw_nonzero().get() as u32;
    let code = match mode {
        Mode::StopRetry => {
            wait_for(|| Ok(count(output, "open")? == 1), 5)?;
            let started = Instant::now();
            daemon.signal()?;
            let code = daemon.wait(2)?;
            require(
                code == 0 && started.elapsed() < Duration::from_secs(1),
                "stop-retry must stop successfully in less than one second",
            )?;
            code
        }
        Mode::Exhausted | Mode::Inactive | Mode::StartFailed => daemon.wait(35)?,
        Mode::Bounded | Mode::Retry => daemon.wait(10)?,
        Mode::Live => live(output, &params, &mut feed, &mut daemon)?,
    };
    // The original asserts process status before parsing the final event file.
    let expected = if matches!(mode, Mode::Exhausted | Mode::Inactive | Mode::StartFailed) {
        1
    } else {
        0
    };
    require(
        code == expected,
        format!("{}: daemon exit {code}", mode.as_str()),
    )?;
    let metrics = validate_events(mode, pid, code, &read_events(output)?)?;
    Ok(
        json!({"passed":true,"mode":mode.as_str(),"pid":pid,"exit":code,"opens":metrics.opens,"callbacks":metrics.callbacks,"median_period":metrics.median_period,"packets":feed.packets,"audio_hardware_opened":false}),
    )
}
pub(super) fn dispatch(args: &[OsString]) -> Result<()> {
    require(args.len() == 7, "invalid private soundd worker arguments")?;
    let mode = Mode::parse(args[0].to_str().ok_or("mode is not Unicode")?)?;
    let result = scenario(
        Path::new(&args[1]),
        Path::new(&args[2]),
        Path::new(&args[3]),
        mode,
        Path::new(&args[4]),
        &args[5],
        Path::new(&args[6]),
    )?;
    // Separate private protocol from fixture-helper stdout, which the original
    // exposes unchanged to its caller.
    json_file(&Path::new(&args[3]).join("worker.result.json"), &result)?;
    Ok(())
}
fn environment(spec: &mut CommandSpec, env: &BTreeMap<OsString, OsString>) {
    spec.set_os_env(env.clone());
    spec.env_remove = std::env::vars_os()
        .filter_map(|(k, _)| (!env.contains_key(&k)).then_some(k))
        .collect();
}
fn executable(path: &Path, env: &BTreeMap<OsString, OsString>) -> PathBuf {
    if path.components().count() > 1 || path.is_absolute() {
        return path.into();
    }
    let paths = env
        .get(OsStr::new("PATH"))
        .map_or(OsStr::new("/bin:/usr/bin"), OsString::as_os_str);
    std::env::split_paths(paths)
        .map(|p| p.join(path))
        .find(|p| fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
        .unwrap_or_else(|| path.into())
}
struct Publication {
    path: PathBuf,
    file: File,
}
impl Publication {
    fn remove_owned(&self) {
        // Holding the published file reserves its inode even if its pathname is
        // unlinked. A replacement cannot recycle that identity during cleanup.
        let Ok(owned) = self.file.metadata() else {
            return;
        };
        if fs::symlink_metadata(&self.path)
            .is_ok_and(|m| (m.dev(), m.ino()) == (owned.dev(), owned.ino()))
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}
fn json_file(path: &Path, value: &Value) -> Result<Publication> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().ok_or("summary parent missing")?)?;
    serde_json::to_writer_pretty(file.as_file_mut(), value)?;
    file.write_all(b"\n")?;
    file.flush()?;
    let file = file.persist(path).map_err(|e| e.error)?;
    Ok(Publication {
        path: path.into(),
        file,
    })
}
pub(super) struct Completed {
    pub(super) summary: Value,
    publication: Publication,
}
pub(super) fn finish_cli(result: &Completed, cancel: &AtomicBool) -> Result<()> {
    // Observe signals already latched before handler removal as well as those
    // observed by the runner. Never unlink a foreign replacement report.
    if let Err(error) = interrupted(cancel) {
        result.publication.remove_owned();
        return Err(error);
    }
    Ok(())
}
fn publish_after_stdout(
    stdout: &mut impl Write,
    results: Vec<Value>,
    output: &Path,
    verify: impl Fn() -> Result<()>,
) -> Result<Completed> {
    // This is the controller's actual fallible final stdout boundary. Identity
    // checks must follow it before and after publishing the owned receipt.
    writeln!(
        stdout,
        "{{\"passed\": true, \"scenarios\": {}}}",
        results.len()
    )?;
    stdout.flush()?;
    verify()?;
    let summary = Value::Array(results);
    let path = output.join("summary.json");
    let publication = json_file(&path, &summary)?;
    if let Err(error) = verify() {
        publication.remove_owned();
        return Err(error);
    }
    Ok(Completed {
        summary,
        publication,
    })
}
pub(super) fn run(options: &Options, worker: &Path, cancel: &AtomicBool) -> Result<Completed> {
    require(!options.modes.is_empty(), "empty soundd scenario roster")?;
    interrupted(cancel)?;
    let before = identity::validate_source(&options.root)?;
    // Invalid/nonregular executables still fail in their original child-spawn phase.
    let binary_path = executable(&options.binary, &options.environment);
    let binary = Artifact::open(&binary_path).ok();
    let helper = Artifact::open(&options.fixture_assets_binary).ok();
    let worker_identity = Artifact::open(worker)?;
    fs::create_dir_all(&options.output)?;
    let library = if let Some(path) = &options.portaudio_library {
        let library = fs::canonicalize(path)?;
        fs::write(
            options.output.join("compile.log"),
            b"reused prebuilt owned PortAudio ABI fixture\n",
        )?;
        library
    } else {
        let library = fs::canonicalize(&options.output)?.join("libowned-portaudio.so");
        let mut spec = CommandSpec::new_os(
            vec![
                "cc".into(),
                "-std=c11".into(),
                "-shared".into(),
                "-fPIC".into(),
                "-pthread".into(),
                "-Wall".into(),
                "-Wextra".into(),
                "-Werror".into(),
                options
                    .root
                    .join("rust/tools/fixtures/soundd/portaudio.c")
                    .into(),
                "-o".into(),
                library.clone().into(),
            ],
            Duration::from_secs(60),
        );
        environment(&mut spec, &options.environment);
        let result =
            NativeRunner.run_controlled(&spec, Instant::now() + Duration::from_secs(60), cancel)?;
        // subprocess.run(text=True) rejects undecodable compiler output and
        // applies universal-newline decoding before the original writes it.
        let text = |raw: Vec<u8>| -> Result<String> {
            Ok(String::from_utf8(raw)?
                .replace("\r\n", "\n")
                .replace('\r', "\n"))
        };
        let compiler_stdout = text(result.stdout)?;
        let compiler_stderr = text(result.stderr)?;
        require(
            result.code == 0,
            format!(
                "owned PortAudio compile exited {}: {compiler_stderr}",
                result.code
            ),
        )?;
        let log =
            format!("{compiler_stdout}{compiler_stderr}compiled owned PortAudio ABI fixture\n");
        fs::write(options.output.join("compile.log"), log)?;
        library
    };
    let library_identity = Artifact::open(&library).ok();
    let mut results = Vec::new();
    for &mode in &options.modes {
        interrupted(cancel)?;
        let output = options.output.join(mode.as_str());
        fs::create_dir_all(&output)?;
        // Outer owner survives a killed worker; cleanup never follows a replacement inode.
        let namespace = Namespace::new()?;
        let temporary = PrivateRoot::new()?;
        // Retain actual namespace ownership evidence even when the child fails.
        // This contains only an ASCII private prefix, never a global environment dump.
        json_file(
            &output.join("namespace.json"),
            &json!({"prefix":namespace.prefix.to_str().ok_or("invalid namespace prefix")?,"device":namespace.identity.0,"inode":namespace.identity.1,"temporary_path_bytes":temporary.path.as_os_str().as_bytes()}),
        )?;
        let mut spec = CommandSpec::new_os(
            vec![
                worker.into(),
                "--soundd-daemon-worker".into(),
                mode.as_str().into(),
                options.binary.clone().into(),
                library.clone().into(),
                output.clone().into(),
                options.fixture_assets_binary.clone().into(),
                namespace.prefix.clone(),
                temporary.path.clone().into(),
            ],
            Duration::from_secs(90),
        );
        environment(&mut spec, &options.environment);
        // The original fixture helper inherits the controller's caller stdin.
        // Both worker boundaries must preserve it.
        spec.inherit_stdin = true;
        let result =
            NativeRunner.run_controlled(&spec, Instant::now() + Duration::from_secs(90), cancel)?;
        // NativeRunner kills/reaps the owned worker group before Namespace drops.
        fs::write(output.join("worker.stderr.log"), &result.stderr)?;
        io::stdout().write_all(&result.stdout)?;
        io::stdout().flush()?;
        require(
            result.code == 0,
            format!(
                "{} worker exited {}: {}",
                mode.as_str(),
                result.code,
                String::from_utf8_lossy(&result.stderr)
            ),
        )?;
        let summary: Value = serde_json::from_slice(&fs::read(output.join("worker.result.json"))?)?;
        require(
            summary["passed"] == true && summary["mode"] == mode.as_str(),
            "invalid soundd worker result",
        )?;
        json_file(&output.join("summary.json"), &summary)?;
        results.push(summary);
    }
    let binary = binary.ok_or("successful daemon has no regular executable identity")?;
    let helper = helper.ok_or("successful fixture helper has no regular executable identity")?;
    let library_identity =
        library_identity.ok_or("successful PortAudio library has no regular identity")?;
    let verify = || -> Result<()> {
        interrupted(cancel)?;
        require(
            before == identity::validate_source(&options.root)?,
            "soundd original source presence changed during run",
        )?;
        binary.verify(&binary_path)?;
        helper.verify(&options.fixture_assets_binary)?;
        library_identity.verify(&library)?;
        worker_identity.verify(worker)?;
        interrupted(cancel)
    };
    verify()?;
    let provenance = json!({"source_sha256":before,"historical_source_sha256":{(identity::RETIRED):identity::ORIGINAL},"executables":{"daemon":binary.sha256,"fixture_assets":helper.sha256,"checker_worker":worker_identity.sha256,"owned_portaudio":library_identity.sha256},"limits":{"compiler_seconds":60,"assets_seconds":30,"worker_seconds":90},"physical_audio_tested":false});
    json_file(&options.output.join("provenance.json"), &provenance)?;
    verify()?;
    // Check fallible stdout BEFORE publishing a new PASS. Existing foreign reports
    // are untouched on admission/child/output failure; no eager invalidation.
    let mut stdout = io::stdout().lock();
    publish_after_stdout(&mut stdout, results, &options.output, verify)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct MutatingWriter {
        path: PathBuf,
        replace: bool,
        mutated: bool,
        written: Vec<u8>,
    }
    impl Write for MutatingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.mutated {
                if self.replace {
                    let replacement = self.path.with_extension("replacement");
                    fs::write(&replacement, b"changed private daemon\n")?;
                    fs::rename(replacement, &self.path)?;
                } else {
                    let mut original = fs::read(&self.path)?;
                    original.extend_from_slice(b"\n/* changed private source */\n");
                    fs::write(&self.path, original)?;
                }
                self.mutated = true;
            }
            self.written.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn final_stdout_mutation_rejects_private_source_and_daemon_before_receipt() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let manifest: Value = serde_json::from_slice(include_bytes!(
            "../../tests/fixtures/soundd-daemon/manifest.json"
        ))
        .unwrap();
        for replace in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("source");
            for name in manifest["source_sha256"].as_object().unwrap().keys() {
                let from = repository.join(name);
                if name == identity::RETIRED && !from.exists() {
                    continue;
                }
                let to = source.join(name);
                fs::create_dir_all(to.parent().unwrap()).unwrap();
                fs::copy(from, to).unwrap();
            }
            let before = identity::validate_source(&source).unwrap();
            let daemon = temp.path().join("soundd-rs");
            fs::write(&daemon, b"private daemon identity\n").unwrap();
            let held = Artifact::open(&daemon).unwrap();
            let output = temp.path().join("output");
            fs::create_dir(&output).unwrap();
            let receipt = output.join("summary.json");
            fs::write(&receipt, b"foreign receipt\n").unwrap();
            let mut writer = MutatingWriter {
                path: if replace {
                    daemon.clone()
                } else {
                    source.join("rust/tools/fixtures/soundd/portaudio.c")
                },
                replace,
                mutated: false,
                written: Vec::new(),
            };
            let verify = || -> Result<()> {
                require(
                    before == identity::validate_source(&source)?,
                    "soundd original source presence changed during run",
                )?;
                held.verify(&daemon)
            };
            verify().unwrap();
            assert!(publish_after_stdout(
                &mut writer,
                vec![json!({"passed":true})],
                &output,
                verify
            )
            .is_err());
            assert_eq!(
                writer.written.as_slice(),
                b"{\"passed\": true, \"scenarios\": 1}\n"
            );
            assert_eq!(fs::read(receipt).unwrap(), b"foreign receipt\n");
        }
    }
    #[test]
    fn namespace_and_temporary_root_cleanup_preserve_foreign_replacements() {
        let namespace = Namespace::new().unwrap();
        let old = namespace.path.with_extension("displaced-by-ownership-test");
        fs::rename(&namespace.path, &old).unwrap();
        fs::create_dir(&namespace.path).unwrap();
        fs::write(namespace.path.join("foreign"), b"keep").unwrap();
        let replacement = namespace.path.clone();
        drop(namespace);
        assert_eq!(fs::read(replacement.join("foreign")).unwrap(), b"keep");
        fs::remove_dir_all(replacement).unwrap();
        fs::remove_dir_all(old).unwrap();
        let root = PrivateRoot::new().unwrap();
        let old = root.path.with_extension("displaced-by-ownership-test");
        fs::rename(&root.path, &old).unwrap();
        fs::create_dir(&root.path).unwrap();
        fs::write(root.path.join("foreign"), b"keep").unwrap();
        let replacement = root.path.clone();
        drop(root);
        assert_eq!(fs::read(replacement.join("foreign")).unwrap(), b"keep");
        fs::remove_dir_all(replacement).unwrap();
        fs::remove_dir_all(old).unwrap();
    }
    #[test]
    fn latched_cancellation_removes_only_this_publication_inode() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("summary.json");
        let cancel = AtomicBool::new(true);
        let publication = json_file(&path, &json!({"passed":true})).unwrap();
        assert!(finish_cli(
            &Completed {
                summary: Value::Null,
                publication
            },
            &cancel
        )
        .is_err());
        assert!(!path.exists());
        let publication = json_file(&path, &json!({"passed":true})).unwrap();
        let identity = publication.file.metadata().unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(publication.file.metadata().unwrap().nlink(), 0);
        fs::write(&path, b"foreign receipt").unwrap();
        let replacement = fs::metadata(&path).unwrap();
        assert_ne!(
            (identity.dev(), identity.ino()),
            (replacement.dev(), replacement.ino())
        );
        assert!(finish_cli(
            &Completed {
                summary: Value::Null,
                publication
            },
            &cancel
        )
        .is_err());
        assert_eq!(fs::read(path).unwrap(), b"foreign receipt");
    }
}
