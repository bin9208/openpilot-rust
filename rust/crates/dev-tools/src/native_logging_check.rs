//! Live comparison with unchanged C++ swaglog through its independent builder.
//! Only validated callsite, realtime and Rust provenance fields are projected;
//! all other JSON bytes, their ordering and console bytes compare exactly.
use crate::{
    argparse_scan::{scan_os_required_first, OptionSpec},
    qa_process::CommandSpec,
    seal::{digest, Result},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use regex::Regex;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, ChildStderr, ChildStdin, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
mod lifecycle;
pub use lifecycle::Interrupted;
mod retained;
mod socket;
mod trace;
mod transport;
const STRIPPED: [&str; 7] = [
    "DONGLE_ID",
    "GIT_ORIGIN",
    "GIT_BRANCH",
    "GIT_COMMIT",
    "MANAGER_DAEMON",
    "CLEAN",
    "LOGPRINT",
];
fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn realtime() -> Result<f64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64())
}
fn config() -> BTreeMap<OsString, OsString> {
    std::env::vars_os()
        .filter(|(key, _)| !STRIPPED.iter().any(|v| key == std::ffi::OsStr::new(v)))
        .collect()
}
fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let mut spec = CommandSpec::new_os(
        std::iter::once(OsString::from("git"))
            .chain(args.iter().map(OsString::from))
            .collect(),
        Duration::from_secs(10),
    );
    spec.cwd = Some(root.into());
    let captured = lifecycle::run(&spec)?;
    require(captured.code == 0, "native logging Git observation failed")?;
    Ok(captured.stdout)
}
struct Receiver {
    socket: zmq::Socket,
    _context: zmq::Context,
    _root: lifecycle::Directory,
    endpoint: String,
}
impl Receiver {
    fn new() -> Result<Self> {
        let root = tempfile::Builder::new()
            .prefix("native-log-")
            .tempdir_in("/tmp")?;
        let endpoint = format!("ipc://{}", root.path().join("socket").display());
        let context = zmq::Context::new();
        let socket = context.socket(zmq::PULL)?;
        socket.set_linger(0)?;
        socket.bind(&endpoint)?;
        Ok(Self {
            socket,
            _context: context,
            _root: lifecycle::Directory::new(root),
            endpoint,
        })
    }
    fn empty(&self) -> Result<()> {
        require(
            self.socket.poll(zmq::POLLIN, 50)? == 0,
            "unexpected native log packet",
        )
    }
}
struct Starting(Option<Child>);
impl Drop for Starting {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = lifecycle::cleanup(&mut child);
        }
    }
}
struct Peer {
    child: Option<Child>,
    input: Option<ChildStdin>,
    stderr: ChildStderr,
    pending: Vec<u8>,
    output: PathBuf,
    root: PathBuf,
    original: bool,
    started: f64,
    commit: String,
    tree: String,
    responses: Vec<Value>,
    packets: Vec<Value>,
}
impl Drop for Peer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = lifecycle::cleanup(&mut child);
        }
    }
}
impl Peer {
    fn start(
        binary: &Path,
        root: &Path,
        output: &Path,
        endpoint: &str,
        env: &BTreeMap<OsString, OsString>,
        original: bool,
        full: bool,
    ) -> Result<Self> {
        lifecycle::check()?;
        fs::create_dir_all(output)?;
        let commit = String::from_utf8(git(root, &["rev-parse", "HEAD"])?)?
            .trim()
            .to_owned();
        let tree = if git(root, &["status", "--porcelain", "--untracked-files=normal"])?.is_empty()
        {
            "clean"
        } else {
            "dirty"
        }
        .into();
        let stdout = File::create(if full {
            Path::new("/dev/full").to_path_buf()
        } else {
            output.join("stdout.txt")
        })?;
        let mut command = Command::new(binary);
        if !original {
            command.arg(endpoint);
        }
        let started = realtime()?;
        let child = command
            .current_dir(root)
            .process_group(0)
            .env_clear()
            .envs(env)
            .env("NATIVE_LOG_ENDPOINT", endpoint)
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .stdout(stdout)
            .spawn()?;
        let mut starting = Starting(Some(child));
        let child = starting.0.as_mut().ok_or("producer owner missing")?;
        let input = child.stdin.take().ok_or("missing producer stdin")?;
        let stderr = child.stderr.take().ok_or("missing producer stderr")?;
        let peer = Self {
            child: starting.0.take(),
            input: Some(input),
            stderr,
            pending: Vec::new(),
            output: output.into(),
            root: root.into(),
            original,
            started,
            commit,
            tree,
            responses: Vec::new(),
            packets: Vec::new(),
        };
        let flags = rustix::fs::fcntl_getfl(&peer.stderr)?;
        rustix::fs::fcntl_setfl(&peer.stderr, flags | rustix::fs::OFlags::NONBLOCK)?;
        fs::write(
            output.join("invocation.json"),
            serde_json::to_vec_pretty(
                &json!({"binary":binary,"binary_sha256":digest(binary)?,"endpoint":endpoint,"original":original,"cwd":root,"source_commit":peer.commit,"source_tree":peer.tree,"started":started,"pid":peer.child.as_ref().map(Child::id)}),
            )?,
        )?;
        lifecycle::hold(
            endpoint,
            peer.child.as_ref().ok_or("producer owner missing")?.id(),
            output,
        )?;
        Ok(peer)
    }
    fn write(&mut self, value: &Value) -> Result<Value> {
        lifecycle::check()?;
        let mut value = value.clone();
        if self.original && value["op"] == "rate" {
            value["timestamp"] = json!(value["timestamp"]
                .as_u64()
                .ok_or("rate timestamp missing")?
                .to_string());
        }
        let input = self.input.as_mut().ok_or("producer input closed")?;
        serde_json::to_writer(&mut *input, &value)?;
        input.write_all(b"\n")?;
        input.flush()?;
        Ok(value)
    }
    fn command(&mut self, value: Value) -> Result<Value> {
        let command = self.write(&value)?;
        let started = Instant::now();
        loop {
            lifecycle::check()?;
            if let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
                let line: Vec<_> = self.pending.drain(..=end).collect();
                let response: Value = serde_json::from_slice(&line)?;
                let row = json!({"command":command,"response":response});
                let mut log = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(self.output.join("responses.jsonl"))?;
                serde_json::to_writer(&mut log, &row)?;
                log.write_all(b"\n")?;
                self.responses.push(row);
                return Ok(response);
            }
            require(
                !self.read_stderr()?,
                "producer closed stderr before command response",
            )?;
            require(
                started.elapsed() < Duration::from_secs(5),
                "producer command timed out",
            )?;
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn read_stderr(&mut self) -> Result<bool> {
        let mut bytes = [0; 8192];
        match self.stderr.read(&mut bytes) {
            Ok(0) => Ok(true),
            Ok(count) => {
                require(
                    self.pending.len() + count <= 1 << 20,
                    "producer diagnostics exceed bound",
                )?;
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(self.output.join("stderr.raw"))?
                    .write_all(&bytes[..count])?;
                self.pending.extend_from_slice(&bytes[..count]);
                Ok(false)
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                Ok(false)
            }
            Err(e) => Err(e.into()),
        }
    }
    fn receive(&mut self, pull: &zmq::Socket) -> Result<Vec<u8>> {
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            lifecycle::check()?;
            if pull.poll(zmq::POLLIN, 10)? > 0 {
                break;
            }
            require(Instant::now() < until, "no native packet")?;
        }
        let packet = pull.recv_bytes(0)?;
        fs::write(
            self.output
                .join(format!("packet-{:04}.bin", self.packets.len())),
            &packet,
        )?;
        let record = validate_packet(
            &packet,
            self.started,
            realtime()?,
            &self.root,
            self.original,
            &self.commit,
            &self.tree,
        )?;
        self.packets
            .push(json!({"raw_base64":STANDARD.encode(&packet),"record":record}));
        Ok(packet)
    }
    fn wait(&mut self, timeout: Duration) -> Result<i32> {
        let child = self.child.as_mut().ok_or("producer already reaped")?;
        let code = lifecycle::wait(child, timeout)?;
        self.child = None;
        lifecycle::reaped()?;
        Ok(code)
    }
    fn finish(&mut self) -> Result<()> {
        self.input.take();
        require(
            self.wait(Duration::from_secs(5))? == 0,
            "producer exit was not zero",
        )?;
        let start = Instant::now();
        while !self.read_stderr()? {
            lifecycle::check()?;
            require(
                start.elapsed() < Duration::from_secs(1),
                "producer stderr failed to close",
            )?;
            std::thread::sleep(Duration::from_millis(1));
        }
        require(
            self.pending.is_empty(),
            "producer left trailing diagnostics",
        )?;
        fs::write(
            self.output.join("responses.json"),
            serde_json::to_vec_pretty(&self.responses)?,
        )?;
        fs::write(
            self.output.join("packets.json"),
            serde_json::to_vec_pretty(&self.packets)?,
        )?;
        Ok(())
    }
}
fn validate_packet(
    packet: &[u8],
    started: f64,
    now: f64,
    root: &Path,
    original: bool,
    commit: &str,
    tree: &str,
) -> Result<Value> {
    require(
        !packet.is_empty() && packet.len() <= 1 << 20,
        "native log packet size invalid",
    )?;
    let record: Value = serde_json::from_slice(&packet[1..])?;
    let object = record
        .as_object()
        .ok_or("native log record is not an object")?;
    let names = [
        "created", "ctx", "filename", "funcname", "levelnum", "lineno", "msg",
    ];
    require(
        object.len() == names.len() && names.iter().all(|n| object.contains_key(*n)),
        "native log fields differ",
    )?;
    require(
        record["levelnum"].as_u64() == Some(u64::from(packet[0])),
        "native log severity differs",
    )?;
    let created = record["created"]
        .as_f64()
        .ok_or("native log timestamp missing")?;
    require(
        started <= created && created <= now,
        "native log timestamp outside lifetime",
    )?;
    let source = Path::new(
        record["filename"]
            .as_str()
            .ok_or("native callsite missing")?,
    );
    let source = if source.is_absolute() {
        source.into()
    } else {
        root.join("rust").join(source)
    };
    let number = usize::try_from(record["lineno"].as_u64().ok_or("native log line missing")?)?;
    let contents = fs::read_to_string(source)?;
    let line = number
        .checked_sub(1)
        .and_then(|i| contents.lines().nth(i))
        .ok_or("native log line outside source")?;
    require(
        line.contains(if original { "cloudlog" } else { "logger.emit" }),
        "native log callsite source differs",
    )?;
    let function = record["funcname"]
        .as_str()
        .ok_or("native function missing")?;
    require(
        if original {
            ["emit", "rate"].contains(&function)
        } else {
            function == "native_logging_probe::emit"
        },
        "native log function differs",
    )?;
    if !original {
        require(
            record["ctx"]["runtime_language"] == "rust"
                && record["ctx"]["source_commit"] == commit
                && record["ctx"]["source_tree"] == tree,
            "native log provenance differs",
        )?;
    }
    Ok(record)
}
/// Matches the original comparator's declared fields without reserializing JSON.
pub fn normalize(packet: &[u8]) -> Result<Vec<u8>> {
    let (level, bytes) = packet.split_first().ok_or("empty native packet")?;
    let mut text = std::str::from_utf8(bytes)?.to_owned();
    text = Regex::new(r#""created": [^,}]+"#)?
        .replace_all(&text, "\"created\": 0")
        .into_owned();
    for key in ["filename", "funcname"] {
        text = Regex::new(&format!(r#""{key}": "(?:[^"\\]|\\.)*""#))?
            .replace_all(&text, format!("\"{key}\": \"metadata\""))
            .into_owned();
    }
    text = Regex::new(r#""lineno": \d+"#)?
        .replace_all(&text, "\"lineno\": 0")
        .into_owned();
    for key in ["runtime_language", "source_commit", "source_tree"] {
        text = Regex::new(&format!(r#""{key}": "(?:[^"\\]|\\.)*", "#))?
            .replace_all(&text, "")
            .into_owned();
    }
    Ok(std::iter::once(*level).chain(text.into_bytes()).collect())
}
fn console(peer: &Peer) -> Result<Vec<u8>> {
    let filename = peer
        .packets
        .first()
        .and_then(|v| v["record"]["filename"].as_str())
        .ok_or("console callsite missing")?;
    // Filesystem source paths are UTF-8 here, as required by the original reader.
    let text = String::from_utf8(fs::read(peer.output.join("stdout.txt"))?)?;
    Ok(text
        .replace(&format!("{filename}: "), "FILE: ")
        .into_bytes())
}
fn check_response(response: &Value, original: bool) -> Result<()> {
    require(
        response["sent"] == 1 && response["dropped"] == 0,
        "native log was not delivered",
    )?;
    if original {
        require(
            response["linger"] == 100 && response["errors"] == 0,
            "original linger/error differs",
        )?;
    }
    Ok(())
}
fn console_modes() -> [Option<&'static str>; 7] {
    [
        None,
        Some(""),
        Some("debug"),
        Some("info"),
        Some("warning"),
        Some("error"),
        Some("DEBUG"),
    ]
}
fn console_environment(
    env: &BTreeMap<OsString, OsString>,
    i: usize,
    mode: Option<&str>,
) -> BTreeMap<OsString, OsString> {
    let mut env = env.clone();
    if let Some(mode) = mode {
        env.insert("LOGPRINT".into(), mode.into());
    }
    if i % 2 == 1 {
        for (key, value) in [
            ("DONGLE_ID", ""),
            ("GIT_ORIGIN", "fixture-origin"),
            ("GIT_BRANCH", "한글/branch"),
            ("GIT_COMMIT", "fixture-commit"),
            ("MANAGER_DAEMON", "fixture-daemon"),
            ("CLEAN", ""),
        ] {
            env.insert(key.into(), value.into());
        }
    }
    if i == 2 {
        env.insert("CLEAN".into(), "0".into());
    }
    env
}
fn console_case(
    binary: &Path,
    root: &Path,
    output: &Path,
    env: &BTreeMap<OsString, OsString>,
    original: bool,
) -> Result<(Peer, Vec<Vec<u8>>)> {
    let receiver = Receiver::new()?;
    let mut peer = Peer::start(
        binary,
        root,
        output,
        &receiver.endpoint,
        env,
        original,
        false,
    )?;
    let mut packets = Vec::new();
    for level in [0, 10, 20, 30, 40, 50] {
        for text in [
            "plain",
            "한글😀é\u{2028}\u{2029}\"\\\u{8}\u{c}\n\r\t\u{1}",
            "\0tail",
            "prefix\0tail",
        ] {
            check_response(
                &peer.command(json!({"op":"emit","level":level,"text":text}))?,
                original,
            )?;
            packets.push(normalize(&peer.receive(&receiver.socket)?)?);
        }
    }
    require(
        peer.command(json!({"op":"emit","level":40,"text":""}))?["sent"] == 0,
        "empty message emitted",
    )?;
    receiver.empty()?;
    peer.finish()?;
    Ok((peer, packets))
}
fn rate_timestamps() -> Vec<u64> {
    let mut timestamps: Vec<u64> = vec![
        0,
        0,
        1,
        100_000_001,
        100_000_002,
        200_000_002,
        300_000_002,
        300_000_003,
    ];
    timestamps.extend((0..500).map(|i| 300_000_004 + i));
    timestamps.extend([
        500_000_000,
        500_000_001,
        600_000_001,
        600_000_002,
        u64::MAX - 1,
        u64::MAX,
        0,
        1,
    ]);
    timestamps
}
fn rate_case(
    binary: &Path,
    root: &Path,
    output: &Path,
    env: &BTreeMap<OsString, OsString>,
    original: bool,
) -> Result<(Peer, Vec<Vec<u8>>)> {
    let receiver = Receiver::new()?;
    let mut peer = Peer::start(
        binary,
        root,
        output,
        &receiver.endpoint,
        env,
        original,
        false,
    )?;
    let mut packets = Vec::new();
    for timestamp in &rate_timestamps() {
        let response = peer.command(
            json!({"op":"rate","timestamp":timestamp,"text":format!("rate-{timestamp}")}),
        )?;
        require(response["dropped"] == 0, "rate message dropped")?;
        let count = response["sent"]
            .as_u64()
            .ok_or("rate delivery count missing")?;
        require(count <= 2, "rate emitted unexpected packet count")?;
        for _ in 0..count {
            packets.push(normalize(&peer.receive(&receiver.socket)?)?);
        }
    }
    receiver.empty()?;
    peer.finish()?;
    Ok((peer, packets))
}
fn compare(source: &Path, native: &Path, root: &Path, output: &Path) -> Result<Value> {
    let env = config();
    let modes = console_modes();
    let mut total = 0;
    for (i, mode) in modes.iter().enumerate() {
        let env = console_environment(&env, i, *mode);
        let mut records = Vec::new();
        let mut consoles = Vec::new();
        for (original, binary) in [(true, source), (false, native)] {
            let (peer, packets) = console_case(
                binary,
                root,
                &output.join(format!(
                    "console-{i}/{}",
                    if original { "source" } else { "rust" }
                )),
                &env,
                original,
            )?;
            consoles.push(console(&peer)?);
            records.push(packets);
        }
        require(records[0] == records[1], "native logging wire bytes differ")?;
        require(
            consoles[0] == consoles[1],
            "native logging console bytes differ",
        )?;
        total += records[0].len();
    }
    let mut rates = Vec::new();
    for (original, binary) in [(true, source), (false, native)] {
        let (_, packets) = rate_case(
            binary,
            root,
            &output.join(format!("rate/{}", if original { "source" } else { "rust" })),
            &env,
            original,
        )?;
        rates.push(packets);
    }
    require(rates[0] == rates[1], "rate-limit wire bytes differ")?;
    Ok(
        json!({"wire_records":total,"logprint_cases":modes.len(),"rate_inputs":rate_timestamps().len(),"rate_records":rates[0].len()}),
    )
}
struct HeldBinary {
    path: PathBuf,
    file: File,
}
impl HeldBinary {
    fn open(path: &Path) -> Result<Self> {
        let file = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC
                | rustix::fs::OFlags::NONBLOCK,
            rustix::fs::Mode::empty(),
        )?);
        require(
            file.metadata()?.is_file(),
            "logging provider is not a regular file",
        )?;
        use std::os::unix::fs::FileExt;
        let mut header = [0_u8; 64];
        file.read_exact_at(&mut header, 0)?;
        let machine = match std::env::consts::ARCH {
            "x86_64" => 62,
            "aarch64" => 183,
            _ => return Err("unsupported logging QA host architecture".into()),
        };
        require(
            &header[..6] == b"\x7fELF\x02\x01"
                && u16::from_le_bytes([header[18], header[19]]) == machine,
            "logging provider is not a host ELF64 executable",
        )?;
        Ok(Self {
            path: path.into(),
            file,
        })
    }
    fn snapshot(&self) -> Result<Value> {
        use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
        let held = self.file.metadata()?;
        let current = fs::symlink_metadata(&self.path)?;
        require(
            (held.dev(), held.ino()) == (current.dev(), current.ino()),
            "logging provider pathname changed",
        )?;
        let bytes_path = PathBuf::from(format!("/proc/self/fd/{}", self.file.as_raw_fd()));
        Ok(
            json!({"path":self.path,"dev":held.dev(),"ino":held.ino(),"size":held.len(),"mode":held.mode(),"mtime":held.mtime(),"mtime_nsec":held.mtime_nsec(),"ctime":held.ctime(),"ctime_nsec":held.ctime_nsec(),"sha256":digest(&bytes_path)?}),
        )
    }
}
pub fn cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if trace::selected(&args) {
        return trace::cli_selected(&args);
    }
    if retained::prepare::selected(&args) {
        return retained::prepare::cli(&args[1..]);
    }
    if retained::selected(&args) {
        return retained::cli(&args[1..]);
    }
    let parsed = scan_os_required_first(
        &args,
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--builder", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--root", false),
            OptionSpec::value("--reference", false),
            OptionSpec::value("--source-policy", false),
        ],
    )?;
    if parsed.help {
        println!(
            "usage: check-native-logging --binary BINARY --builder BUILDER --output OUTPUT [--root ROOT] [--reference PREBUILT_CPP] [--source-policy present|absent]"
        );
        return Ok(());
    }
    let _cancel = lifecycle::Cancel::new()?;
    // A signal may interrupt poll/read/Git/build before a loop next checks the
    // latch. Normalize it only after all peers/receivers have unwound.
    let result = run_cli(parsed);
    lifecycle::reaped()?;
    lifecycle::check()?;
    result
}
fn run_cli(parsed: crate::argparse_scan::ParsedOs) -> Result<()> {
    let requested_root = parsed.values.get("--root").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        PathBuf::from,
    );
    let root = requested_root.canonicalize()?;
    require(root.is_dir(), "logging source root is not a directory")?;
    let binary = PathBuf::from(&parsed.values["--binary"]).canonicalize()?;
    let builder = PathBuf::from(&parsed.values["--builder"]).canonicalize()?;
    let output = std::path::absolute(Path::new(&parsed.values["--output"]))?;
    // Reject all occupied leaves, including dangling aliases, before building
    // or opening fixed report/provider names. create_dir below is the atomic
    // admission check if a foreign node arrives after this early rejection.
    match fs::symlink_metadata(&output) {
        Ok(_) => return Err("logging evidence output already exists".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let policy = retained::SourcePolicy::parse(
        parsed
            .values
            .get("--source-policy")
            .map(OsString::as_os_str),
    )?;
    let source_guard = retained::SourceGuard::open(&requested_root, policy)?;
    require(
        source_guard.root() == root,
        "logging source parent boundary invalid",
    )?;
    let actual = (|| -> Result<()> {
        let native_held = HeldBinary::open(&binary)?;
        let _builder_held = HeldBinary::open(&builder)?;
        let reference_held = parsed
            .values
            .get("--reference")
            .map(|path| HeldBinary::open(&PathBuf::from(path).canonicalize()?))
            .transpose()?;
        source_guard.verify()?;
        fs::create_dir_all(
            output
                .parent()
                .ok_or("logging evidence output has no parent")?,
        )?;
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(&output)?;
        source_guard.verify()?;
        let source_held = if let Some(reference) = reference_held {
            reference
        } else {
            let mut spec = CommandSpec::new_os(
                vec![
                    builder.into_os_string(),
                    "--root".into(),
                    root.clone().into_os_string(),
                    "--output".into(),
                    output.join("reference").into_os_string(),
                ],
                Duration::from_secs(300),
            );
            spec.cwd = Some(root.clone());
            let built = lifecycle::run(&spec)?;
            fs::write(output.join("reference-builder.stdout"), &built.stdout)?;
            fs::write(output.join("reference-builder.stderr"), &built.stderr)?;
            require(built.code == 0, "native logging original builder failed")?;
            HeldBinary::open(&PathBuf::from(std::str::from_utf8(&built.stdout)?.trim()))?
        };
        let source = source_held.path.clone();
        // The C++ provider is created by the independently held native builder.
        // Hold both actual ELF inodes continuously through every live protocol case.
        let held = [source_held, native_held];
        let before = held
            .iter()
            .map(HeldBinary::snapshot)
            .collect::<Result<Vec<_>>>()?;
        fs::write(
            output.join("providers.before.json"),
            serde_json::to_vec_pretty(&before)?,
        )?;
        source_guard.verify()?;
        let actual = (|| -> Result<Value> {
            let mut report = compare(&source, &binary, &root, &output)?;
            report["transport"] =
                transport::check(&source, &binary, &root, &output.join("transport"))?;
            Ok(report)
        })();
        let final_checks = (|| -> Result<()> {
            let after = held
                .iter()
                .map(HeldBinary::snapshot)
                .collect::<Result<Vec<_>>>()?;
            fs::write(
                output.join("providers.after.json"),
                serde_json::to_vec_pretty(&after)?,
            )?;
            require(
                before == after,
                "logging ELF provider changed during live comparison",
            )?;
            Ok(())
        })();
        let mut report = retained::checked_result(actual, final_checks)?;
        report["result"] = json!("pass");
        report["source_binary_sha256"] = json!(digest(&source)?);
        report["native_binary_sha256"] = json!(digest(&binary)?);
        source_guard.verify()?;
        report["source_policy"] = json!(policy.as_str());
        fs::write(
            output.join("report.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        println!("{report}");
        Ok(())
    })();
    match actual {
        Ok(()) => Ok(()),
        Err(error) => source_guard.complete(Err(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet(root: &Path, original: bool) -> Vec<u8> {
        fs::create_dir_all(root.join("rust")).unwrap();
        fs::write(
            root.join("rust/site.rs"),
            if original {
                "cloudlog(20, text)\n"
            } else {
                "logger.emit(site, text)\n"
            },
        )
        .unwrap();
        let ctx = if original {
            json!({"arbitrary":"preserve"})
        } else {
            json!({"arbitrary":"preserve","runtime_language":"rust","source_commit":"abc","source_tree":"clean"})
        };
        let value = json!({"created":2.0,"ctx":ctx,"filename":"site.rs","funcname":if original {"emit"} else {"native_logging_probe::emit"},"levelnum":20,"lineno":1,"msg":"escaped \"created\": 8 must survive"});
        std::iter::once(20)
            .chain(serde_json::to_vec(&value).unwrap())
            .collect()
    }
    #[test]
    fn saved_created_number_preserves_actual_wire_float_bits() {
        let record: Value = serde_json::from_slice(br#"{"created": 1792684466.2917035}"#).unwrap();
        assert_eq!(
            record["created"].as_f64().unwrap().to_bits(),
            1792684466.2917035_f64.to_bits()
        );
        let saved = serde_json::to_vec(&record).unwrap();
        let replay: Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(
            replay["created"].as_f64().unwrap().to_bits(),
            1792684466.2917035_f64.to_bits()
        );
    }
    #[test]
    fn validation_checks_level_lifetime_callsite_and_provenance_before_projection() {
        let root = tempfile::tempdir().unwrap();
        let data = packet(root.path(), false);
        assert!(validate_packet(&data, 1.0, 3.0, root.path(), false, "abc", "clean").is_ok());
        assert!(validate_packet(&data, 2.1, 3.0, root.path(), false, "abc", "clean").is_err());
        assert!(validate_packet(&data, 1.0, 1.9, root.path(), false, "abc", "clean").is_err());
        assert!(validate_packet(&data, 1.0, 3.0, root.path(), false, "def", "clean").is_err());
        assert!(validate_packet(&data, 1.0, 3.0, root.path(), false, "abc", "dirty").is_err());
        let mut wrong = data.clone();
        wrong[0] = 30;
        assert!(validate_packet(&wrong, 1.0, 3.0, root.path(), false, "abc", "clean").is_err());
        fs::write(root.path().join("rust/site.rs"), "not a callsite\n").unwrap();
        assert!(validate_packet(&data, 1.0, 3.0, root.path(), false, "abc", "clean").is_err());
    }
    #[test]
    fn normalization_is_byte_preserving_outside_declared_callsite_and_provenance() {
        let source=b"\x14{\"created\": 1.1, \"ctx\": {\"keep\": \"value\"}, \"filename\": \"a\\\"b.cc\", \"funcname\": \"emit\", \"levelnum\": 20, \"lineno\": 3, \"msg\": \"literal \\\"created\\\": 8\"}";
        let native=b"\x14{\"created\": 2.2, \"ctx\": {\"runtime_language\": \"rust\", \"source_commit\": \"abc\", \"source_tree\": \"clean\", \"keep\": \"value\"}, \"filename\": \"site.rs\", \"funcname\": \"native_logging_probe::emit\", \"levelnum\": 20, \"lineno\": 99, \"msg\": \"literal \\\"created\\\": 8\"}";
        assert_eq!(normalize(source).unwrap(), normalize(native).unwrap());
        let changed = String::from_utf8(native[1..].to_vec())
            .unwrap()
            .replace("value", "other");
        let changed = std::iter::once(20)
            .chain(changed.bytes())
            .collect::<Vec<_>>();
        assert_ne!(normalize(source).unwrap(), normalize(&changed).unwrap());
        assert!(normalize(b"").is_err());
    }
}
