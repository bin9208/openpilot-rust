//! CLI-private actual IPC/child owner and independent source loop. Environment
//! prefixes change only after every prior child and mapped queue has closed.
use openpilot_cereal::log_capnp::event;
use openpilot_dev_tools::{
    argparse_scan::{scan_os, OptionSpec},
    feedbackd_check::{input_packet, iteration_contract, validate_bookmark, validate_source},
    path_resolve::resolve_non_strict,
    seal::{digest, Result},
};
use openpilot_msgq::{MultiSubscriber, Publisher, Subscriber, Subscription};
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs::{self, File},
    io::Cursor,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
const CAPACITY: usize = 256_000;
const INPUTS: [&str; 3] = ["rawAudioData", "bookmarkButton", "carState"];
const PASS: &str =
    "PASS source/native feedback IPC, validity, logging, disabled recording and shutdown";
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
fn pretty(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(value)? + "\n")?;
    Ok(())
}
fn monotonic() -> u64 {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}
fn exit_code(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| -status.signal().unwrap_or(1))
}
fn wait(child: &mut Child, deadline: Duration) -> Result<Option<ExitStatus>> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if start.elapsed() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
/// Source Python SubMaster conflates inputs and updates all three once per poll.
/// The literal False audio gate means neither Params nor car/audio values can
/// enable recording. Only an updated bookmark (even valid=false) sends output.
fn reference(output: &Path) -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, stop.clone())?;
    // SIGINT is the original KeyboardInterrupt boundary. SIGTERM retains its
    // ordinary default disposition; it is not silently turned into success.
    let prefix = std::env::var_os("OPENPILOT_PREFIX").unwrap_or_default();
    let params = output.join("params").join(prefix);
    fs::create_dir_all(&params)?;
    fs::write(params.join("RecordAudioFeedback"), b"1")?;
    let mut bookmark = Publisher::for_runtime("userBookmark", CAPACITY)?;
    let _audio = Publisher::for_runtime("audioFeedback", CAPACITY)?;
    let specs = INPUTS.map(|endpoint| Subscription {
        endpoint,
        capacity: CAPACITY,
        polled: true,
    });
    let mut subscriber = MultiSubscriber::for_runtime(&specs)?;
    let context = zmq::Context::new();
    let mut logger = None;
    while !stop.load(Ordering::Relaxed) {
        let ready = subscriber.poll_ready(Duration::from_millis(100))?;
        let mut updated_bookmark = false;
        for index in ready {
            if let Some(raw) = subscriber.receive_one(index)? {
                let message = capnp::serialize::read_message(
                    &mut Cursor::new(raw),
                    capnp::message::ReaderOptions::new(),
                )?;
                let e = message.get_root::<event::Reader>()?;
                // Decode all original subscribed inputs without borrowing any
                // candidate feedback/messaging state or decision policy.
                match (index, e.which()?) {
                    (0, event::RawAudioData(payload)) => {
                        payload?;
                    }
                    (1, event::BookmarkButton(payload)) => {
                        payload?;
                        updated_bookmark = true;
                    }
                    (2, event::CarState(payload)) => {
                        payload?;
                    }
                    _ => return Err("feedback subscribed input union mismatch".into()),
                }
            }
        }
        if updated_bookmark {
            // Original stream handler runs before its lazy PUSH connection.
            eprintln!("Bookmark button pressed!");
            if logger.is_none() {
                let socket = context.socket(zmq::PUSH)?;
                socket.set_linger(10)?;
                socket.connect(&format!("ipc://{}", output.join("log").display()))?;
                logger = Some(socket);
            }
            let record = json!({"msg":"Bookmark button pressed!","levelnum":20,"level":"INFO"});
            let mut payload = vec![20];
            payload.extend_from_slice(record.to_string().as_bytes());
            match logger.as_ref().unwrap().send(&payload, zmq::DONTWAIT) {
                Ok(()) | Err(zmq::Error::EAGAIN) => {}
                Err(error) => return Err(error.into()),
            }
            let mut message = capnp::message::Builder::new_default();
            let mut e = message.init_root::<event::Builder>();
            e.set_log_mono_time(monotonic());
            e.set_valid(true);
            e.init_user_bookmark();
            bookmark.send(&capnp::serialize::write_message_to_words(&message))?;
        }
    }
    Ok(())
}
struct Owner {
    namespace: Option<tempfile::TempDir>,
    namespace_path: PathBuf,
    previous_prefix: Option<Option<OsString>>,
    child: Option<Child>,
    status: Option<ExitStatus>,
    publishers: Vec<Publisher>,
    subscribers: Vec<Subscriber>,
    phase: &'static str,
    timed_out: bool,
}
impl Owner {
    fn new() -> Result<Self> {
        let namespace = tempfile::Builder::new()
            .prefix("msgq_feedback-")
            .tempdir_in("/dev/shm")?;
        let namespace_path = namespace.path().to_path_buf();
        let prefix = namespace_path
            .file_name()
            .and_then(|v| v.to_str())
            .and_then(|v| v.strip_prefix("msgq_"))
            .ok_or("invalid owned feedback namespace")?;
        let previous_prefix = std::env::var_os("OPENPILOT_PREFIX");
        std::env::set_var("OPENPILOT_PREFIX", prefix);
        Ok(Self {
            namespace: Some(namespace),
            namespace_path,
            previous_prefix: Some(previous_prefix),
            child: None,
            status: None,
            publishers: Vec::new(),
            subscribers: Vec::new(),
            phase: "prepare",
            timed_out: false,
        })
    }
    fn poll(&mut self) -> Result<Option<ExitStatus>> {
        if self.status.is_none() {
            if let Some(child) = &mut self.child {
                self.status = child.try_wait()?;
            }
        }
        Ok(self.status)
    }
    fn child(&mut self) -> Result<&mut Child> {
        self.child
            .as_mut()
            .ok_or_else(|| "feedback child not spawned".into())
    }
    fn close(&mut self) -> Result<()> {
        let cleanup = (|| -> Result<()> {
            // A reaped child is never signalled. An unreaped Child reserves its
            // PID until killed/waited, so there is no recycled-PID target.
            if self.poll()?.is_none() && self.child.is_some() {
                self.child()?.kill()?;
                self.status = wait(self.child()?, Duration::from_secs(3))?;
                require(
                    self.status.is_some(),
                    "feedback killed child did not reap within three seconds",
                )?;
            }
            self.publishers.clear();
            self.subscribers.clear();
            if let Some(namespace) = self.namespace.take() {
                namespace.close()?;
            }
            Ok(())
        })();
        if let Some(previous) = self.previous_prefix.take() {
            match previous {
                Some(value) => std::env::set_var("OPENPILOT_PREFIX", value),
                None => std::env::remove_var("OPENPILOT_PREFIX"),
            }
        }
        cleanup
    }
    fn receipt(&self) -> Value {
        json!({"phase":self.phase,"exit_code":self.status.map(exit_code),"timed_out":self.timed_out,
            "child_reaped":self.child.is_some() && self.status.is_some(),
            "namespace":self.namespace_path,"namespace_removed":!self.namespace_path.exists()})
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        // Normal paths call close explicitly and retain any error. Drop is the
        // final safety net for an earlier I/O/spawn/protocol failure.
        if self.namespace.is_some() {
            let _ = self.close();
        }
    }
}
fn stderr(output: &Path) -> String {
    fs::read_to_string(output.join("stderr.log")).unwrap_or_default()
}
fn scenario(command: &mut Command, output: &Path) -> Result<Vec<Value>> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)?; // Existing even-empty outputs are not silently reused.
    let mut owner = Owner::new()?;
    let result = (|| -> Result<Vec<Value>> {
        for topic in INPUTS {
            owner
                .publishers
                .push(Publisher::for_runtime(topic, CAPACITY)?);
        }
        let prefix = std::env::var_os("OPENPILOT_PREFIX").ok_or("owned feedback prefix missing")?;
        let params = output.join("params").join(prefix);
        fs::create_dir_all(&params)?;
        fs::write(params.join("RecordAudioFeedback"), b"1")?;
        let stdout = File::create(output.join("stdout.log"))?;
        let err = File::create(output.join("stderr.log"))?;
        command
            .env("PARAMS_ROOT", output.join("params"))
            .env("LOGPRINT", "info")
            .stdin(Stdio::inherit())
            .stdout(stdout)
            .stderr(err);
        owner.phase = "spawn";
        owner.child = Some(command.spawn()?);
        owner.phase = "readiness";
        let start = Instant::now();
        loop {
            owner.publishers[0].send(&input_packet("rawAudioData", true, monotonic())?)?;
            owner.publishers[2].send(&input_packet("carState", true, monotonic())?)?;
            std::thread::sleep(Duration::from_millis(20));
            require(
                owner.poll()?.is_none(),
                format!("feedback child exited during readiness: {}", stderr(output)),
            )?;
            if owner.publishers[0].readers_caught_up() && owner.publishers[2].readers_caught_up() {
                break;
            }
            if start.elapsed() >= Duration::from_secs(8) {
                owner.timed_out = true;
                return Err("feedback subscriptions not ready".into());
            }
        }
        for topic in ["userBookmark", "audioFeedback"] {
            let mut subscriber = Subscriber::for_runtime(topic, false, CAPACITY)?;
            require(
                subscriber.receive(Duration::ZERO)?.is_none(),
                "feedback output present before test input",
            )?;
            owner.subscribers.push(subscriber);
        }
        owner.phase = "iterations";
        let mut rows = Vec::new();
        for i in 0..8 {
            let contract = iteration_contract(i)?;
            let started = monotonic();
            owner.publishers[0].send(&input_packet("rawAudioData", true, monotonic())?)?;
            owner.publishers[2].send(&input_packet(
                "carState",
                contract.car_valid,
                monotonic(),
            )?)?;
            if let Some(valid) = contract.bookmark_valid {
                owner.publishers[1].send(&input_packet("bookmarkButton", valid, monotonic())?)?;
                let raw = owner.subscribers[0]
                    .receive(Duration::from_millis(200))?
                    .ok_or_else(|| format!("feedback bookmark missing: {}", stderr(output)))?;
                validate_bookmark(&raw, started, monotonic())?;
                fs::write(output.join(format!("bookmark-{i}.capnp")), raw)?;
            } else {
                require(
                    owner.subscribers[0]
                        .receive(Duration::from_millis(200))?
                        .is_none(),
                    "unexpected feedback bookmark",
                )?;
            }
            require(
                owner.subscribers[1].receive(Duration::ZERO)?.is_none(),
                "disabled feedback audio was published",
            )?;
            require(
                owner.subscribers[0].receive(Duration::ZERO)?.is_none(),
                "duplicate feedback bookmark",
            )?;
            rows.push(json!({"iteration":i,"bookmarks":contract.bookmarks}));
        }
        owner.phase = "shutdown";
        let pid = rustix::process::Pid::from_raw(i32::try_from(owner.child()?.id())?)
            .ok_or("invalid feedback child PID")?;
        // Still owned and unreaped when signalled, including an exit race.
        rustix::process::kill_process(pid, rustix::process::Signal::INT)?;
        owner.status = wait(owner.child()?, Duration::from_secs(3))?;
        if owner.status.is_none() {
            owner.timed_out = true;
            return Err("feedback shutdown exceeded three seconds".into());
        }
        require(
            owner.status.map(exit_code) == Some(0),
            format!("feedback shutdown failed: {}", stderr(output)),
        )?;
        owner.phase = "logging";
        let text = fs::read_to_string(output.join("stdout.log"))?
            + &fs::read_to_string(output.join("stderr.log"))?;
        require(
            text.matches("Bookmark button pressed!").count() == 4,
            format!("feedback bookmark log count mismatch: {text}"),
        )?;
        owner.phase = "complete";
        Ok(rows)
    })();
    let cleanup = owner.close();
    let receipt = pretty(&output.join("lifecycle.json"), &owner.receipt());
    // Preserve the primary source/child error while still attempting all owned
    // cleanup and writing lifecycle evidence, including spawn failures (#362).
    let rows = result?;
    cleanup?;
    receipt?;
    Ok(rows)
}
pub fn cli() -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", false),
            OptionSpec::value("--output", true),
            OptionSpec::flag("--reference", None),
        ],
    )?;
    if args.help {
        println!("usage: check-feedbackd [--binary BINARY] --output OUTPUT [--reference]");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let root = resolve_non_strict(&root)?;
    let pins = validate_source(&root)?;
    let output = resolve_non_strict(Path::new(&args.values["--output"]))?;
    if args.values.contains_key("--reference") {
        return reference(&output);
    }
    let binary = args.values.get("--binary").map(PathBuf::from);
    require(
        binary.as_ref().is_some_and(|p| p.is_file()),
        "native feedback executable missing",
    )?;
    let binary = resolve_non_strict(&binary.unwrap())?;
    let checker = std::env::current_exe()?;
    let source = scenario(
        Command::new(&checker)
            .arg("--reference")
            .arg("--output")
            .arg(output.join("source")),
        &output.join("source"),
    )?;
    let native = scenario(&mut Command::new(&binary), &output.join("native"))?;
    require(source == native, "feedback source/native rows differ")?;
    pretty(
        &output.join("comparison.json"),
        &json!({"source_sha256":pins["openpilot/selfdrive/ui/feedback/feedbackd.py"],
        "rows":native,"audio_recording_disabled":true,"source_native_equal":true}),
    )?;
    let mut native_sources = serde_json::Map::new();
    for name in [
        "rust/crates/dev-tools/src/feedbackd_check.rs",
        "rust/crates/dev-tools/src/feedbackd_native.rs",
        "rust/crates/dev-tools/src/bin/check_feedbackd.rs",
    ] {
        native_sources.insert(name.into(), json!(digest(&root.join(name))?));
    }
    pretty(
        &output.join("inputs.json"),
        &json!({"sources":pins,"native_sources":native_sources,
        "checker_sha256":digest(&checker)?,"candidate_sha256":digest(&binary)?}),
    )?;
    println!("{PASS}");
    Ok(())
}
