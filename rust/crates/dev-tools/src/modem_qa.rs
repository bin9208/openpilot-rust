//! Native PTY, command-fixture and original-source differential for the modem runtime.
use crate::{
    modem_capture::{self, Receipt},
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_process_supervision::pty::Pair;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, flock, FlockOperation, OFlags},
    io::{fcntl_getfd, fcntl_setfd, FdFlags},
    process::{kill_process, kill_process_group, Pid, Signal},
    termios::ttyname,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, Write},
    os::fd::AsFd,
    os::unix::{
        ffi::OsStrExt,
        fs::{symlink, MetadataExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const CASES: [&str; 13] = [
    "normal",
    "at_errors",
    "lpa_lock",
    "echo_retry",
    "identity_retry",
    "absent_sim",
    "roaming_blocked",
    "poll_errors",
    "retry_limit",
    "route_failure_retry",
    "route_failure",
    "invalid_route",
    "dns_failure",
];
const MAX_PTY_COMMANDS: usize = 2048;
const MAX_PTY_PENDING_BYTES: usize = 8192;
const MAX_PTY_RESPONSE_BYTES: usize = 16 * 1024;
const MAX_TRACE_OUTPUT_BYTES: u64 = 4 * 1024 * 1024;

fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into().into())
    }
}

#[derive(Clone)]
struct Case {
    name: &'static str,
    operations: Vec<Value>,
    responses: BTreeMap<String, Vec<String>>,
    flags: Vec<&'static str>,
    hold_lock: bool,
}

fn step(state: &str) -> Value {
    json!({"op":"step", "state":state})
}
fn command(name: &str) -> Value {
    json!({"op":"at", "command":name})
}
fn sleep(ms: u64) -> Value {
    json!({"op":"sleep", "ms":ms})
}
fn case(name: &'static str, operations: Vec<Value>) -> Case {
    Case {
        name,
        operations,
        responses: BTreeMap::new(),
        flags: Vec::new(),
        hold_lock: false,
    }
}

fn default_responses() -> BTreeMap<String, Vec<String>> {
    [
        ("AT+CGMI", vec!["Quectel"]),
        ("AT+CGSN", vec!["123456789012345"]),
        ("AT+QCCID", vec!["+QCCID: 8985235123456789012F"]),
        ("AT+CIMI", vec!["450081123456789"]),
        ("AT+GMR", vec!["EG25G_TEST"]),
        ("AT+CREG?", vec!["+CREG: 2,1"]),
        ("AT+CGREG?", vec!["+CGREG: 2,1"]),
        ("AT+CSQ", vec!["+CSQ: 23,99"]),
        ("AT+COPS?", vec!["+COPS: 0,0,\"Fixture\",7"]),
        (
            "AT+QNWINFO",
            vec!["+QNWINFO: \"FDD LTE\",\"45008\",\"LTE BAND 3\",1300"],
        ),
        (
            "AT+QENG=\"servingcell\"",
            vec!["+QENG: \"servingcell\",\"NOCONN\",\"LTE\""],
        ),
        ("AT+QTEMP", vec!["+QTEMP: 31,255,34"]),
        (
            "AT+CGCONTRDP=1",
            vec!["+CGCONTRDP: 1,5,\"apn\",\"10.0.0.2\",\"10.0.0.1\",\"1.1.1.1\",\"8.8.8.8\""],
        ),
        ("AT+BAD", vec!["+CME ERROR: 10"]),
        ("AT+PLAINERROR", vec!["ERROR"]),
        ("AT+BLANK", vec!["", "  ", "+TEST: 1"]),
    ]
    .into_iter()
    .map(|(command, lines)| {
        (
            command.to_owned(),
            lines.into_iter().map(str::to_owned).collect(),
        )
    })
    .collect()
}

/// The checker’s fixed inputs, translated directly from its unchanged scenario table.
fn scenarios() -> Vec<Case> {
    let init = || step("INITIALIZING");
    let mut cases = vec![
        case(
            "normal",
            vec![
                init(),
                step("SEARCHING"),
                step("CONNECTING"),
                sleep(150),
                step("CONNECTED"),
                json!({"op":"poll"}),
                step("DISCONNECTING"),
            ],
        ),
        case(
            "at_errors",
            ["AT+BAD", "AT+PLAINERROR", "AT+BLANK", "AT+TIMEOUT"]
                .into_iter()
                .map(command)
                .collect(),
        ),
        case("lpa_lock", vec![command("AT+CGMI"), init()]),
        case("echo_retry", vec![init()]),
        case("identity_retry", vec![init()]),
        case("absent_sim", vec![init(), step("SEARCHING")]),
        case("roaming_blocked", vec![init(), step("SEARCHING")]),
        case("poll_errors", vec![json!({"op":"poll"})]),
        {
            let mut operations = vec![init(), step("CONNECTING")];
            for _ in 0..3 {
                operations.push(json!({"op":"wait_exit"}));
                operations.push(step("CONNECTED"));
            }
            operations.push(step("DISCONNECTING"));
            case("retry_limit", operations)
        },
        case(
            "route_failure_retry",
            vec![
                init(),
                step("CONNECTING"),
                sleep(150),
                json!({"op":"poll"}),
                json!({"op":"wait_exit"}),
                step("CONNECTED"),
                sleep(500),
                json!({"op":"kill"}),
            ],
        ),
        case(
            "route_failure",
            vec![json!({"op":"routes", "ip":"10.0.0.2", "peer":"10.0.0.1"})],
        ),
        case(
            "invalid_route",
            vec![json!({"op":"routes", "ip":"bad", "peer":"10.0.0.1"})],
        ),
        case(
            "dns_failure",
            vec![json!({"op":"dns", "servers":["1.1.1.1"]})],
        ),
    ];
    cases[1]
        .responses
        .insert("AT+BAD".into(), vec!["+CME ERROR: 10".into()]);
    cases[1]
        .responses
        .insert("AT+PLAINERROR".into(), vec!["ERROR".into()]);
    cases[1].responses.insert(
        "AT+BLANK".into(),
        vec![String::new(), "  ".into(), "+TEST: 1".into()],
    );
    cases[2].hold_lock = true;
    cases[3]
        .responses
        .insert("AT+CGMI".into(), vec!["AT+CGMI".into(), "Quectel".into()]);
    cases[4]
        .responses
        .insert("AT+CGSN".into(), vec!["invalid".into()]);
    cases[5]
        .responses
        .insert("AT+QCCID".into(), vec!["ERROR".into()]);
    cases[5]
        .responses
        .insert("AT+CIMI".into(), vec!["ERROR".into()]);
    cases[6]
        .responses
        .insert("AT+QCCID".into(), vec!["+QCCID: 1234567890".into()]);
    cases[6]
        .responses
        .insert("AT+CREG?".into(), vec!["+CREG: 2,5".into()]);
    cases[7].responses.extend([
        ("AT+CSQ".into(), vec!["+CSQ: 99,99".into()]),
        ("AT+QTEMP".into(), vec!["+QTEMP: bad".into()]),
        (
            "AT+QNWINFO".into(),
            vec!["+QNWINFO: \"LTE\",\"45008\",\"LTE BAND 3\",bad".into()],
        ),
        ("AT+COPS?".into(), vec!["+COPS: 0,0,\"Fixture\",bad".into()]),
    ]);
    cases[7].flags.push("no-ip");
    cases[8].flags.push("fail-ppp");
    cases[9].flags.push("fail-route");
    cases[10].flags.push("fail-route");
    cases[12].flags.push("fail-dns");
    debug_assert_eq!(cases.iter().map(|c| c.name).collect::<Vec<_>>(), CASES);
    cases
}

fn validate_original_request_plans(cases: &[Case], receipt: &Receipt) -> Result<Value> {
    let plans = receipt.request_plans()?;
    let plans = plans
        .as_array()
        .ok_or("modem original request plans are not an array")?;
    require(
        plans.len() == cases.len() * 2,
        "modem original request plan count differs",
    )?;
    let expected_paths = [
        ("at_port", "<owned-fixture-root>/at"),
        ("ppp_port", "<owned-fixture-root>/absent-data-port"),
        ("state", "<owned-fixture-root>/state"),
        ("lock", "<owned-fixture-root>/lock"),
        ("params", "<owned-fixture-root>/params"),
        ("statistics", "<owned-fixture-root>/statistics"),
        ("modem_manager", "<owned-fixture-root>/manager.service"),
        ("sudo", "<owned-fixture-root>/sudo"),
        ("ip", "<owned-fixture-root>/ip"),
    ];
    for (index, case) in cases.iter().enumerate() {
        let source = &plans[index * 2];
        let native = &plans[index * 2 + 1];
        let mut source_as_native = source.clone();
        source_as_native["side"] = json!("native");
        require(
            source["side"] == "source"
                && native["side"] == "native"
                && source_as_native == native.clone(),
            format!("modem original paired request differs: {}", case.name),
        )?;
        let mut responses = default_responses();
        let mut changes = serde_json::Map::new();
        for (command, lines) in &case.responses {
            if responses.get(command) != Some(lines) {
                changes.insert(command.clone(), json!(lines));
            }
        }
        require(
            source["operations"] == Value::Array(case.operations.clone())
                && source["changes"] == Value::Object(changes)
                && source["flags"] == json!(case.flags)
                && source["held_lock"] == case.hold_lock,
            format!(
                "Rust modem scenario differs from original input: {}",
                case.name
            ),
        )?;
        responses.extend(case.responses.clone());
        require(
            source["initial_responses"] == json!(responses),
            format!(
                "Rust modem default responses differ from original input: {}",
                case.name
            ),
        )?;
        for (field, expected) in expected_paths {
            require(
                source["config"][field] == expected,
                format!("modem original fixture path differs: {}:{field}", case.name),
            )?;
        }
        require(
            source["config"]["serial_timeout_ms"] == 100
                && source["config"]["state_wait_ms"] == 30
                && source["config"]["iccid_interval"] == 0.1
                && source["config"]["launcher"].as_str().is_some_and(|path| {
                    path.ends_with("/rust/target/debug/openpilot-process-child")
                }),
            format!(
                "Rust modem scalar configuration differs from original: {}",
                case.name
            ),
        )?;
    }
    let original_operations = plans
        .iter()
        .step_by(2)
        .map(|plan| plan["operations"].as_array().map_or(0, Vec::len))
        .sum::<usize>();
    require(
        original_operations == 40,
        "modem original request operation count differs",
    )?;
    Ok(
        json!({"source_cases":cases.len(),"paired_request_plans":plans.len(),
        "original_source_operations":original_operations,"scenario_operations_match":true,
        "response_overrides_match":true,"default_responses_match":true,
        "scalar_configuration_match":true}),
    )
}

struct PtyServer {
    stop: Arc<AtomicBool>,
    commands: Arc<Mutex<Vec<String>>>,
    failure: Arc<Mutex<Option<String>>>,
    responses: Arc<Mutex<BTreeMap<String, Vec<String>>>>,
    thread: Option<JoinHandle<()>>,
    _slave: File,
    path: PathBuf,
}

impl PtyServer {
    fn new(responses: BTreeMap<String, Vec<String>>) -> Result<Self> {
        let pair = Pair::open(24, 80)?;
        let path = PathBuf::from(
            ttyname(&pair.slave, Vec::new())?
                .to_string_lossy()
                .into_owned(),
        );
        let flags = fcntl_getfl(&pair.master)?;
        fcntl_setfl(&pair.master, flags | OFlags::NONBLOCK)?;
        let stop = Arc::new(AtomicBool::new(false));
        let commands = Arc::new(Mutex::new(Vec::new()));
        let failure = Arc::new(Mutex::new(None));
        let responses = Arc::new(Mutex::new(responses));
        let thread_responses = Arc::clone(&responses);
        let thread_stop = Arc::clone(&stop);
        let thread_commands = Arc::clone(&commands);
        let thread_failure = Arc::clone(&failure);
        let master = pair.master;
        let thread = thread::spawn(move || {
            let mut pending = Vec::new();
            let mut bytes = [0u8; 4096];
            let poll_timeout = Timespec::try_from(Duration::from_millis(50)).ok();
            while !thread_stop.load(Ordering::Relaxed) {
                let mut fd = [PollFd::new(&master, PollFlags::IN)];
                if let Err(error) = poll(&mut fd, poll_timeout.as_ref()) {
                    if error == rustix::io::Errno::INTR {
                        continue;
                    }
                    if let Ok(mut failure) = thread_failure.lock() {
                        *failure = Some(format!("PTY poll failed: {error}"));
                    }
                    thread_stop.store(true, Ordering::Relaxed);
                    break;
                }
                match File::from(
                    master
                        .as_fd()
                        .try_clone_to_owned()
                        .expect("clone PTY master"),
                )
                .read(&mut bytes)
                {
                    Ok(0) => thread::sleep(Duration::from_millis(5)),
                    Ok(count) => {
                        pending.extend_from_slice(&bytes[..count]);
                        if pending.len() > MAX_PTY_PENDING_BYTES {
                            if let Ok(mut failure) = thread_failure.lock() {
                                *failure = Some("PTY input exceeded its bounded buffer".into());
                            }
                            thread_stop.store(true, Ordering::Relaxed);
                            break;
                        }
                        while let Some(end) = pending.iter().position(|byte| *byte == b'\r') {
                            let raw: Vec<_> = pending.drain(..=end).collect();
                            let text = String::from_utf8_lossy(&raw[..raw.len() - 1])
                                .trim()
                                .to_owned();
                            if text.is_empty() {
                                continue;
                            }
                            if let Ok(mut recorded) = thread_commands.lock() {
                                if recorded.len() >= MAX_PTY_COMMANDS {
                                    if let Ok(mut failure) = thread_failure.lock() {
                                        *failure =
                                            Some("PTY command count exceeded its bound".into());
                                    }
                                    thread_stop.store(true, Ordering::Relaxed);
                                    break;
                                }
                                recorded.push(text.clone());
                            }
                            if text == "AT+TIMEOUT" {
                                continue;
                            }
                            let lines = thread_responses
                                .lock()
                                .map(|map| map.get(&text).cloned().unwrap_or_default())
                                .unwrap_or_default();
                            let terminal_error = lines.last().is_some_and(|line| {
                                line == "ERROR" || line.starts_with("+CME ERROR")
                            });
                            let mut reply = String::from("\r\n");
                            if !lines.is_empty() {
                                reply.push_str(&lines.join("\r\n"));
                            }
                            if !terminal_error {
                                if !lines.is_empty() {
                                    reply.push_str("\r\n");
                                }
                                reply.push_str("OK");
                            }
                            reply.push_str("\r\n");
                            if reply.len() > MAX_PTY_RESPONSE_BYTES {
                                if let Ok(mut failure) = thread_failure.lock() {
                                    *failure = Some("PTY response exceeded its bound".into());
                                }
                                thread_stop.store(true, Ordering::Relaxed);
                                break;
                            }
                            let mut writer = File::from(
                                master
                                    .as_fd()
                                    .try_clone_to_owned()
                                    .expect("clone PTY master"),
                            );
                            if let Err(error) = writer.write_all(reply.as_bytes()) {
                                if let Ok(mut failure) = thread_failure.lock() {
                                    *failure = Some(format!("PTY response write failed: {error}"));
                                }
                                thread_stop.store(true, Ordering::Relaxed);
                                break;
                            }
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) => {}
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            stop,
            commands,
            failure,
            responses,
            thread: Some(thread),
            _slave: pair.slave,
            path,
        })
    }

    fn close(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.join().map_err(|_| "modem PTY server panicked")?;
        }
        Ok(())
    }

    fn commands(&self) -> Result<Vec<String>> {
        if let Some(error) = self
            .failure
            .lock()
            .map_err(|_| "modem PTY failure lock poisoned")?
            .as_ref()
        {
            return Err(error.clone().into());
        }
        Ok(self
            .commands
            .lock()
            .map_err(|_| "modem PTY command lock poisoned")?
            .clone())
    }

    fn set_response(&self, command: &str, lines: Vec<String>) -> Result<()> {
        self.responses
            .lock()
            .map_err(|_| "modem PTY response lock poisoned")?
            .insert(command.to_owned(), lines);
        Ok(())
    }
}
impl Drop for PtyServer {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

struct Fixture {
    directory: tempfile::TempDir,
    server: PtyServer,
    lock: Option<File>,
}

impl Fixture {
    fn new(case: &Case, executable: &Path, launcher: &Path) -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("modem-native-")
            .tempdir_in("/tmp")?;
        let root = directory.path();
        let mut responses = default_responses();
        responses.extend(case.responses.clone());
        let server = PtyServer::new(responses)?;
        symlink(&server.path, root.join("at"))?;
        for folder in ["params", "statistics"] {
            fs::create_dir(root.join(folder))?;
        }
        symlink(executable, root.join("sudo"))?;
        symlink(executable, root.join("ip"))?;
        symlink(launcher, root.join("launcher"))?;
        fs::write(root.join("statistics/tx_bytes"), b"123")?;
        fs::write(root.join("statistics/rx_bytes"), b"456")?;
        File::create(root.join("manager.service"))?;
        for flag in &case.flags {
            File::create(root.join(flag))?;
        }
        let lock = if case.hold_lock {
            let file = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(root.join("lock"))?;
            flock(&file, FlockOperation::NonBlockingLockExclusive)?;
            Some(file)
        } else {
            None
        };
        Ok(Self {
            directory,
            server,
            lock,
        })
    }

    fn config(&self) -> Value {
        let root = self.directory.path();
        json!({
          "at_port": root.join("at"),
          "ppp_port": root.join("absent-data-port"),
          "state": root.join("state"),
          "lock": root.join("lock"),
          "params": root.join("params"),
          "statistics": root.join("statistics"),
          "modem_manager": root.join("manager.service"),
          "sudo": root.join("sudo"),
          "ip": root.join("ip"),
          "launcher": root.join("launcher"),
          "serial_timeout_ms": 100,
          "state_wait_ms": 30,
          "iccid_interval": 0.1
        })
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn calls(&self) -> Result<Vec<Vec<String>>> {
        let path = self.directory.path().join("calls.jsonl");
        let Ok(text) = fs::read_to_string(path) else {
            return Ok(Vec::new());
        };
        text.lines()
            .map(|line| Ok(serde_json::from_str(line)?))
            .collect()
    }

    fn close(&mut self) -> Result<()> {
        self.server.close()?;
        self.lock.take();
        Ok(())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn helper_dispatch() -> Option<i32> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.is_empty() || std::env::var_os("MODEM_FIXTURE_ROOT").is_none() {
        return None;
    }
    let root = PathBuf::from(std::env::var_os("MODEM_FIXTURE_ROOT")?);
    let text = args
        .iter()
        .map(|item| item.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let result = fixture_command(&root, &text);
    Some(result.unwrap_or(1))
}

fn fixture_command(root: &Path, args: &[String]) -> Result<i32> {
    if let Some(log) = std::env::var_os("MODEM_BOUNDARY_LOG") {
        let marker =
            std::env::var_os("MODEM_BOUNDARY_MARKER").ok_or("MODEM_BOUNDARY_MARKER missing")?;
        let leaked = fs::read_dir("/proc/self/fd")?
            .filter_map(std::result::Result::ok)
            .any(|entry| {
                fs::read_link(entry.path())
                    .is_ok_and(|path| path.as_os_str().as_bytes() == marker.as_bytes())
            });
        let mut records = OpenOptions::new().create(true).append(true).open(log)?;
        serde_json::to_writer(&mut records, &json!({"argv":args,"leaked":leaked}))?;
        records.write_all(b"\n")?;
    } else {
        let mut calls = OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("calls.jsonl"))?;
        serde_json::to_writer(&mut calls, &args)?;
        calls.write_all(b"\n")?;
    }
    if std::env::var_os("MODEM_BOUNDARY_HELPER").is_some()
        && args.first().is_some_and(|arg| arg == "pppd")
    {
        return Ok(0);
    }
    if args.first().is_some_and(|arg| arg == "pppd") {
        fs::write(root.join("ppp.pid"), std::process::id().to_string())?;
        if root.join("fail-ppp").exists() {
            return Ok(1);
        }
        loop {
            thread::sleep(Duration::from_secs(60));
        }
    }
    if args.first().is_some_and(|arg| arg == "killall") {
        if let Ok(text) = fs::read_to_string(root.join("ppp.pid")) {
            if let Ok(pid) = text.parse::<i32>() {
                if let Some(pid) = Pid::from_raw(pid) {
                    let _ = kill_process(pid, Signal::KILL);
                }
            }
        }
    }
    if args.starts_with(&["ip".into(), "rule".into(), "del".into()]) {
        return Ok(1);
    }
    if args.starts_with(&["ip".into(), "route".into(), "add".into()])
        && root.join("fail-route").exists()
    {
        return Ok(1);
    }
    if args.starts_with(&["resolvectl".into(), "dns".into()]) && root.join("fail-dns").exists() {
        return Ok(1);
    }
    if args.first().is_some_and(|arg| arg == "-4") && !root.join("no-ip").exists() {
        println!("inet 10.0.0.2 peer 10.0.0.1/32 scope global ppp0");
    }
    Ok(0)
}

fn source_result(case: &Case, receipt: &Receipt) -> Result<Value> {
    let mut expected = receipt.expected(case.name)?;
    let rows = expected["rows"]
        .as_array_mut()
        .ok_or("modem original capture lacks rows")?;
    for row in rows {
        if let Some(snapshot) = row.get_mut("snapshot") {
            if let Some(object) = snapshot.as_object_mut() {
                object.remove("seconds_since_boot");
            }
        }
    }
    Ok(expected)
}

fn native_result(case: &Case, trace: &Path, launcher: &Path, executable: &Path) -> Result<Value> {
    let mut fixture = Fixture::new(case, executable, launcher)?;
    let input = json!({"config":fixture.config(), "operations":case.operations});
    let mut spec = CommandSpec::new_os(vec![trace.as_os_str().to_owned()], Duration::from_secs(60));
    spec.stdout_limit = MAX_TRACE_OUTPUT_BYTES as usize;
    spec.stderr_limit = MAX_TRACE_OUTPUT_BYTES as usize;
    spec.input = Some(serde_json::to_vec(&input)?);
    spec.set_os_env(BTreeMap::from([(
        OsString::from("MODEM_FIXTURE_ROOT"),
        fixture.directory.path().as_os_str().to_owned(),
    )]));
    let captured = NativeRunner.run(&spec)?;
    require_code(captured.code, &captured.stderr, case.name)?;
    let mut rows: Value = serde_json::from_slice(&captured.stdout)?;
    for row in rows
        .as_array_mut()
        .ok_or("native modem trace is not an array")?
    {
        if let Some(snapshot) = row.get_mut("snapshot") {
            if let Some(object) = snapshot.as_object_mut() {
                object.remove("seconds_since_boot");
            }
        }
    }
    let mut calls = fixture.calls()?;
    for call in &mut calls {
        if call.first().is_some_and(|arg| arg == "pppd") && call.len() > 1 {
            call[1] = "<owned-data-port>".into();
        }
    }
    let output = json!({"rows":rows, "commands":fixture.server.commands()?, "calls":calls});
    fixture.close()?;
    Ok(output)
}

fn require_code(code: i32, stderr: &[u8], name: &str) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!(
            "native modem trace {name} exited {code}: {}",
            String::from_utf8_lossy(stderr)
        )
        .into())
    }
}

pub struct TraceOptions {
    pub source_root: PathBuf,
    pub daemon: PathBuf,
    pub helper: PathBuf,
    pub trace: PathBuf,
    pub launcher: PathBuf,
    pub output: PathBuf,
    pub require_retired: bool,
}

/// Run all original-source trace scenarios and retain their separate raw outputs.
pub fn run_trace(options: &TraceOptions) -> Result<Value> {
    let receipt = modem_capture::validate(&options.source_root, options.require_retired)?;
    let executables = modem_capture::validate_executables(&[
        ("daemon", &options.daemon),
        ("checker helper", &options.helper),
        ("trace", &options.trace),
        ("process launcher", &options.launcher),
    ])?;
    let result = trace_admitted(options, &receipt)?;
    receipt.verify()?;
    executables.verify()?;
    let report = json!({"source_capture":receipt.report(),"executables":executables.report(),
      "scenarios":result["scenarios"],"passed":result["passed"]});
    finalize_report(
        &options.output.join("results.json"),
        &report,
        || {
            receipt.verify()?;
            executables.verify()
        },
        |_| Ok(()),
    )?;
    Ok(report)
}

struct OwnedDaemon {
    child: Option<Child>,
}

impl OwnedDaemon {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn child(&mut self) -> Result<&mut Child> {
        self.child
            .as_mut()
            .ok_or_else(|| "modem daemon was already reaped".into())
    }

    fn stop(&mut self, signal: Signal) -> Result<i32> {
        let child = self.child()?;
        let group = Pid::from_child(child);
        match kill_process_group(group, signal) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(error) => return Err(error.into()),
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = child.try_wait()? {
                self.child.take();
                return Ok(status.code().unwrap_or(128));
            }
            if Instant::now() >= deadline {
                let _ = kill_process_group(group, Signal::KILL);
                let _ = child.kill();
                let _ = child.wait();
                self.child.take();
                return Err("modem daemon did not stop before its deadline".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for OwnedDaemon {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = kill_process_group(Pid::from_child(child), Signal::KILL);
            let _ = child.wait();
        }
    }
}

fn state(path: &Path) -> Value {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| json!({}))
}

fn wait_for_state(
    path: &Path,
    description: &str,
    mut predicate: impl FnMut(&Value) -> bool,
) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let value = state(path);
        if predicate(&value) {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err(format!("timeout waiting for modem {description}: {value}").into());
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn lifecycle(options: &TraceOptions) -> Result<Value> {
    fs::create_dir_all(&options.output)?;
    let test_case = case("lifecycle", Vec::new());
    let mut fixture = Fixture::new(&test_case, &options.helper, &options.launcher)?;
    let root = fixture.directory.path();
    let config_path = root.join("config.json");
    fs::write(&config_path, serde_json::to_vec(&fixture.config())?)?;
    let state_path = root.join("state");
    let log_path = options.output.join("daemon.stderr");
    let stderr = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&log_path)?;
    let child = Command::new(&options.daemon)
        .arg("--config")
        .arg(&config_path)
        .env("MODEM_FIXTURE_ROOT", root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr)
        .process_group(0)
        .spawn()?;
    let mut process = OwnedDaemon::new(child);
    let connected = |value: &Value| value["connected"] == true && value["state"] == "CONNECTED";
    let mut snapshots = vec![wait_for_state(
        &state_path,
        "initial PPP connection",
        connected,
    )?];
    let pid = process.child()?.id();
    let executable = fs::read_link(format!("/proc/{pid}/exe"))?.canonicalize()?;
    require_path_equal(
        &executable,
        &options.daemon.canonicalize()?,
        "running modem executable",
    )?;
    require(
        snapshots[0]["signal_strength"] == 23 && snapshots[0]["rx_bytes"] == 456,
        "modem initial state fields differ",
    )?;
    require(
        fs::metadata(&state_path)?.permissions().mode() & 0o777 == 0o644,
        "modem state file mode differs",
    )?;
    require(
        snapshots[0]["seconds_since_boot"]
            .as_f64()
            .is_some_and(|seconds| seconds > 0.0),
        "modem startup time is absent",
    )?;

    fs::write(root.join("params/GsmApn"), b"new.apn")?;
    wait_for_state(&state_path, "APN command", |_| {
        fixture
            .server
            .commands()
            .is_ok_and(|commands| commands.contains(&"AT+CGDCONT=1,\"IP\",\"new.apn\"".into()))
    })?;
    snapshots.push(wait_for_state(&state_path, "APN reconnect", connected)?);
    fixture
        .server
        .set_response("AT+QCCID", vec!["+QCCID: 8985235999999999999F".into()])?;
    wait_for_state(&state_path, "SIM identity refresh", |value| {
        value["iccid"] == "8985235999999999999"
    })?;
    snapshots.push(wait_for_state(&state_path, "SIM reconnect", connected)?);
    File::create(root.join("no-ip"))?;
    snapshots.push(wait_for_state(
        &state_path,
        "interface disappearance",
        |value| value["connected"] == false,
    )?);
    fs::remove_file(root.join("at"))?;
    snapshots.push(wait_for_state(
        &state_path,
        "serial port disappearance",
        |value| value["state"] == "INITIALIZING",
    )?);
    let exit_code = process.stop(Signal::TERM)?;
    require(
        exit_code == 0,
        format!("modem daemon exited with {exit_code}"),
    )?;
    require(
        !state_path.exists(),
        "modem state remains after clean shutdown",
    )?;
    let ppp_pid: i32 = fs::read_to_string(root.join("ppp.pid"))?.parse()?;
    require(
        !Path::new(&format!("/proc/{ppp_pid}")).exists(),
        "owned modem PPP helper remains after shutdown",
    )?;
    let calls = fixture.calls()?;
    require(
        calls.contains(
            &vec!["systemctl", "mask", "--runtime", "ModemManager"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        "modem daemon did not mask ModemManager",
    )?;
    require(
        calls.contains(
            &vec!["systemctl", "stop", "ModemManager"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        "modem daemon did not stop ModemManager",
    )?;
    require(
        calls.contains(
            &vec!["systemctl", "unmask", "--runtime", "ModemManager"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        "modem daemon did not unmask ModemManager",
    )?;
    require(
        calls.contains(
            &vec!["systemctl", "start", "ModemManager"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        "modem daemon did not restore ModemManager",
    )?;
    require(
        calls.contains(
            &vec!["resolvectl", "dns", "ppp0", "1.1.1.1", "8.8.8.8"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        "modem daemon did not configure cellular DNS",
    )?;
    fixture.close()?;
    let report = json!({"snapshots":snapshots,"at_commands":fixture.server.commands()?,
      "calls":calls,"native_executable":executable,"exit_code":exit_code,
      "state_removed":true,"child_reaped":true});
    write_new(
        &options.output.join("lifecycle.json"),
        &serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(
        json!({"scenario":"continuous_native_pty_lifecycle","pass":true,"artifact":"lifecycle.json"}),
    )
}

fn require_path_equal(actual: &Path, expected: &Path, label: &str) -> Result<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{label} differs: {} != {}",
            actual.display(),
            expected.display()
        )
        .into())
    }
}

fn direct_trace_with_deadline(
    trace: &Path,
    input: &[u8],
    root: &Path,
    stdout_path: &Path,
    stderr_path: &Path,
    boundary_log: Option<&Path>,
    marker: Option<&Path>,
) -> Result<i32> {
    let stdout = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stdout_path)?;
    let stderr = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stderr_path)?;
    let mut command = Command::new(trace);
    command
        .stdin(Stdio::piped())
        .stdout(stdout)
        .stderr(stderr)
        .env(OsString::from("MODEM_FIXTURE_ROOT"), root.as_os_str())
        .process_group(0);
    if let Some(log) = boundary_log {
        command.env("MODEM_BOUNDARY_LOG", log);
    }
    if let Some(marker) = marker {
        command.env("MODEM_BOUNDARY_MARKER", marker);
        command.env("MODEM_BOUNDARY_HELPER", "1");
    }
    let mut child = command.spawn()?;
    let input_write = child
        .stdin
        .take()
        .ok_or("modem trace stdin pipe missing")?
        .write_all(input);
    if let Err(error) = input_write {
        let group = Pid::from_child(&child);
        let _ = rustix::process::kill_process_group(group, Signal::KILL);
        let _ = child.kill();
        let _ = child.wait();
        return Err(error.into());
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.code().unwrap_or(128));
        }
        if fs::metadata(stdout_path)?.len() > MAX_TRACE_OUTPUT_BYTES
            || fs::metadata(stderr_path)?.len() > MAX_TRACE_OUTPUT_BYTES
        {
            let group = Pid::from_child(&child);
            let _ = rustix::process::kill_process_group(group, Signal::KILL);
            let _ = child.kill();
            let _ = child.wait();
            return Err("native modem process-boundary output exceeded its bound".into());
        }
        if Instant::now() >= deadline {
            let group = Pid::from_child(&child);
            let _ = rustix::process::kill_process_group(group, Signal::KILL);
            let _ = child.kill();
            let _ = child.wait();
            return Err("native modem process-boundary trace timed out".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn process_boundary(options: &TraceOptions, receipt: &Receipt) -> Result<Vec<Value>> {
    let original = receipt.original_process_startup()?;
    require(
        original["source_error"]
            .as_str()
            .is_some_and(|error| error.starts_with("[Errno 2]"))
            && original["external_commands"] == json!([]),
        "original modem process-boundary observation does not describe startup ordering",
    )?;
    let mut results = Vec::new();

    // A deliberately inherited nonstandard FD must be closed by every helper exec.
    let descriptor_case = case(
        "descriptor-boundary",
        vec![
            step("CONNECTING"),
            json!({"op":"wait_exit"}),
            json!({"op":"kill"}),
            json!({"op":"poll"}),
            json!({"op":"routes","ip":"10.0.0.2","peer":"10.0.0.1"}),
        ],
    );
    let fixture = Fixture::new(&descriptor_case, &options.helper, &options.launcher)?;
    let marker = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(fixture.root().join("inherited-marker"))?;
    let marker_path = fixture.root().join("inherited-marker").canonicalize()?;
    let marker_fd_flags = fcntl_getfd(&marker)?;
    fcntl_setfd(&marker, marker_fd_flags & !FdFlags::CLOEXEC)?;
    let descriptor_log = fixture.root().join("descriptor.jsonl");
    let request = json!({"config":fixture.config(),"operations":descriptor_case.operations});
    let descriptor_run = direct_trace_with_deadline(
        &options.trace,
        &serde_json::to_vec(&request)?,
        fixture.root(),
        &fixture.root().join("descriptor.stdout"),
        &fixture.root().join("descriptor.stderr"),
        Some(&descriptor_log),
        Some(&marker_path),
    );
    fcntl_setfd(&marker, marker_fd_flags)?;
    let code = descriptor_run?;
    require(code == 0, "modem descriptor-closure trace failed")?;
    let rows: Vec<Value> = fs::read_to_string(&descriptor_log)?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    require(
        rows.len() >= 8 && rows.iter().all(|row| row["leaked"] == false),
        "modem helper inherited an unrelated descriptor",
    )?;
    require(
        rows.iter().any(|row| row["argv"][0] == "pppd")
            && rows.iter().any(|row| row["argv"][0] == "-4"),
        "modem descriptor trace missed PPP or IP helper paths",
    )?;
    let descriptor_report = json!({"descriptor_closure":rows,"exit_code":code});
    write_new(
        &options.output.join("descriptor-boundary.json"),
        &serde_json::to_vec_pretty(&descriptor_report)?,
    )?;
    results.push(
        json!({"scenario":"descriptor_closure","pass":true,"artifact":"descriptor-boundary.json"}),
    );
    drop(marker);
    drop(fixture);

    // A text executable without a shebang must fail at execve; no shell may run it.
    let shell_case = case("shell-fallback", vec![json!({"op":"kill"})]);
    let shell_fixture = Fixture::new(&shell_case, &options.helper, &options.launcher)?;
    let bad = shell_fixture.root().join("no-shebang");
    let escaped = shell_fixture.root().join("shell-fallback-marker");
    fs::write(&bad, format!("touch {}\n", escaped.display()))?;
    let mut permissions = fs::metadata(&bad)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&bad, permissions)?;
    let mut config = shell_fixture.config();
    config["sudo"] = json!(bad);
    let request = json!({"config":config,"operations":shell_case.operations});
    let mut spec = CommandSpec::new_os(
        vec![options.trace.as_os_str().to_owned()],
        Duration::from_secs(10),
    );
    spec.stdout_limit = MAX_TRACE_OUTPUT_BYTES as usize;
    spec.stderr_limit = MAX_TRACE_OUTPUT_BYTES as usize;
    spec.input = Some(serde_json::to_vec(&request)?);
    spec.set_os_env(BTreeMap::from([(
        OsString::from("MODEM_FIXTURE_ROOT"),
        shell_fixture.root().as_os_str().to_owned(),
    )]));
    let captured = NativeRunner.run(&spec)?;
    require(
        captured.code != 0 && !escaped.exists(),
        "modem executable failure used a shell fallback",
    )?;
    let shell_report = json!({"exit_code":captured.code,"stderr":String::from_utf8_lossy(&captured.stderr),
      "shell_fallback_marker":escaped.exists()});
    write_new(
        &options.output.join("shell-fallback.json"),
        &serde_json::to_vec_pretty(&shell_report)?,
    )?;
    results.push(json!({"scenario":"exec_failure_without_shell_fallback","pass":true,"artifact":"shell-fallback.json"}));
    drop(shell_fixture);

    // The unchanged source fails while creating its initial state before helpers run.
    let startup_case = case("startup-order", Vec::new());
    let startup_fixture = Fixture::new(&startup_case, &options.helper, &options.launcher)?;
    let bad_state = startup_fixture.root().join("missing-parent/state");
    let mut config = startup_fixture.config();
    config["state"] = json!(bad_state);
    let config_path = startup_fixture.root().join("startup-config.json");
    fs::write(&config_path, serde_json::to_vec(&config)?)?;
    let mut spec = CommandSpec::new_os(
        vec![
            options.daemon.as_os_str().to_owned(),
            OsString::from("--config"),
            config_path.as_os_str().to_owned(),
        ],
        Duration::from_secs(10),
    );
    spec.set_os_env(BTreeMap::from([(
        OsString::from("MODEM_FIXTURE_ROOT"),
        startup_fixture.root().as_os_str().to_owned(),
    )]));
    let captured = NativeRunner.run(&spec)?;
    let original_commands = original["external_commands"].clone();
    require(
        captured.code != 0
            && String::from_utf8_lossy(&captured.stderr).contains("No such file or directory")
            && !startup_fixture.root().join("calls.jsonl").exists()
            && original_commands == json!([]),
        "modem startup failure ran external helpers before state publication",
    )?;
    let startup_report = json!({"source_startup_error":original["source_error"],
      "source_external_commands":original_commands,"native_exit_code":captured.code,
      "native_stderr":String::from_utf8_lossy(&captured.stderr),"native_external_commands":[]});
    write_new(
        &options.output.join("startup-order.json"),
        &serde_json::to_vec_pretty(&startup_report)?,
    )?;
    results.push(json!({"scenario":"startup_error_precedes_external_commands","pass":true,"artifact":"startup-order.json"}));
    drop(startup_fixture);
    Ok(results)
}

struct OwnedReport {
    path: PathBuf,
    file: File,
    dev: u64,
    ino: u64,
    mode: u32,
    modified: (i64, i64),
    changed: (i64, i64),
    committed: bool,
}

fn report_identity(metadata: &fs::Metadata) -> (u64, u64, u32, (i64, i64), (i64, i64)) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.mode(),
        (metadata.mtime(), metadata.mtime_nsec()),
        (metadata.ctime(), metadata.ctime_nsec()),
    )
}

impl OwnedReport {
    fn verify(&self, expected: &[u8]) -> Result<()> {
        let held = self.file.metadata()?;
        let path = fs::symlink_metadata(&self.path)?;
        let identity = (self.dev, self.ino, self.mode, self.modified, self.changed);
        require(
            path.is_file()
                && !path.file_type().is_symlink()
                && report_identity(&held) == identity
                && report_identity(&path) == identity
                && held.len() == expected.len() as u64,
            "modem report ownership changed",
        )?;
        let mut file = self.file.try_clone()?;
        file.seek(std::io::SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        file.take(expected.len() as u64 + 1)
            .read_to_end(&mut bytes)?;
        require(
            bytes == expected
                && report_identity(&self.file.metadata()?) == identity
                && fs::symlink_metadata(&self.path)
                    .is_ok_and(|metadata| report_identity(&metadata) == identity),
            "modem report bytes or identity changed",
        )
    }
}

impl Drop for OwnedReport {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let Ok(path) = fs::symlink_metadata(&self.path) {
            if path.is_file()
                && !path.file_type().is_symlink()
                && (path.dev(), path.ino()) == (self.dev, self.ino)
            {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

fn finalize_report(
    path: &Path,
    report: &Value,
    mut verify: impl FnMut() -> Result<()>,
    emit: impl FnOnce(&Value) -> Result<()>,
) -> Result<()> {
    verify()?;
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)?;
    let metadata = file.metadata()?;
    let mut publication = OwnedReport {
        path: path.to_path_buf(),
        dev: metadata.dev(),
        ino: metadata.ino(),
        mode: metadata.mode(),
        modified: (metadata.mtime(), metadata.mtime_nsec()),
        changed: (metadata.ctime(), metadata.ctime_nsec()),
        file,
        committed: false,
    };
    publication.file.write_all(&bytes)?;
    publication.file.sync_all()?;
    let metadata = publication.file.metadata()?;
    publication.mode = metadata.mode();
    publication.modified = (metadata.mtime(), metadata.mtime_nsec());
    publication.changed = (metadata.ctime(), metadata.ctime_nsec());
    publication.verify(&bytes)?;
    emit(report)?;
    verify()?;
    publication.verify(&bytes)?;
    publication.committed = true;
    Ok(())
}

/// Full modem evidence run with independent original-source expectations and live lifecycle.
pub fn run_with_emit(
    options: &TraceOptions,
    emit: impl FnOnce(&Value) -> Result<()>,
) -> Result<Value> {
    let receipt = modem_capture::validate(&options.source_root, options.require_retired)?;
    let executables = modem_capture::validate_executables(&[
        ("daemon", &options.daemon),
        ("checker helper", &options.helper),
        ("trace", &options.trace),
        ("process launcher", &options.launcher),
    ])?;
    let traces = trace_admitted(options, &receipt)?;
    let lifecycle = lifecycle(options)?;
    let boundaries = process_boundary(options, &receipt)?;
    receipt.verify()?;
    executables.verify()?;
    let report = json!({"source_capture":receipt.report(),"scenarios":traces["scenarios"],
      "executables":executables.report(),"lifecycle":lifecycle,"process_boundary":boundaries,"passed":17,
      "request_plan_validation":traces["request_plan_validation"]});
    finalize_report(
        &options.output.join("results.json"),
        &report,
        || {
            receipt.verify()?;
            executables.verify()
        },
        emit,
    )?;
    Ok(report)
}

pub fn run(options: &TraceOptions) -> Result<Value> {
    run_with_emit(options, |_| Ok(()))
}

fn trace_admitted(options: &TraceOptions, receipt: &Receipt) -> Result<Value> {
    let mut report = Vec::new();
    let cases = scenarios();
    let plan_validation = validate_original_request_plans(&cases, receipt)?;
    for case in cases {
        let expected = source_result(&case, &receipt)?;
        let actual = native_result(&case, &options.trace, &options.launcher, &options.helper)?;
        let pass = actual == expected;
        let artifact = options.output.join(format!("{}.json", case.name));
        let value = json!({"case":case.name, "pass":pass, "artifact":format!("{}.json", case.name),
              "original_source":expected, "native":actual});
        write_new(&artifact, &serde_json::to_vec_pretty(&value)?)?;
        if !pass {
            return Err(format!("modem original-source differential failed: {}", case.name).into());
        }
        report.push(
            json!({"scenario":case.name,"pass":true,"artifact":format!("{}.json", case.name)}),
        );
    }
    Ok(json!({"scenarios":report,"passed":report.len(),"request_plan_validation":plan_validation}))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Helper mode used when the checker executable is configured as fake sudo/ip.
pub fn dispatch_fixture_helper() -> Option<i32> {
    helper_dispatch()
}

/// Validate the fixed original-source inputs without starting a device or external service.
pub fn admit(root: &Path, require_retired: bool) -> Result<Receipt> {
    modem_capture::validate(root, require_retired)
}

#[cfg(test)]
mod plan_tests {
    use super::*;

    #[test]
    fn canonical_plan_has_all_thirteen_captured_cases() {
        let cases = scenarios();
        assert_eq!(
            cases.iter().map(|case| case.name).collect::<Vec<_>>(),
            CASES
        );
        assert_eq!(
            cases
                .iter()
                .map(|case| case.operations.len())
                .sum::<usize>(),
            40
        );
        assert_eq!(cases[8].operations.len(), 9);
        assert_eq!(cases[9].operations.len(), 8);
    }

    #[test]
    fn failed_stdout_and_late_input_mutation_remove_only_owned_pass() {
        let directory = tempfile::tempdir().unwrap();
        let report = directory.path().join("results.json");
        let value = json!({"passed":17});
        let stdout_failure = finalize_report(
            &report,
            &value,
            || Ok(()),
            |_| Err("simulated stdout failure".into()),
        );
        assert!(stdout_failure.is_err());
        assert!(!report.exists());

        let mut checks = 0;
        let late_mutation = finalize_report(
            &report,
            &value,
            || {
                checks += 1;
                if checks == 2 {
                    Err("simulated final input revalidation failure".into())
                } else {
                    Ok(())
                }
            },
            |_| Ok(()),
        );
        assert!(late_mutation.is_err());
        assert!(!report.exists());

        fs::write(&report, b"foreign report").unwrap();
        let foreign = finalize_report(&report, &value, || Ok(()), |_| Ok(()));
        assert!(foreign.is_err());
        assert_eq!(fs::read(report).unwrap(), b"foreign report");
    }

    #[test]
    fn stdout_replacement_is_preserved_when_publication_loses_ownership() {
        let directory = tempfile::tempdir().unwrap();
        let report = directory.path().join("results.json");
        let callback_path = report.clone();
        let result = finalize_report(
            &report,
            &json!({"passed":17}),
            || Ok(()),
            |_| {
                let replacement = callback_path.with_extension("replacement");
                fs::write(&replacement, b"foreign replacement")?;
                fs::rename(replacement, &callback_path)?;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read(report).unwrap(), b"foreign replacement");
    }
}
