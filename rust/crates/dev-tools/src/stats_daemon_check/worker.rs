use super::{
    process::{check, Process},
    Result,
};
use openpilot_cereal::log_capnp::event;
use openpilot_msgq::{Publisher, Subscriber};
use rustix::process::Signal;
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{Cursor, Write},
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
const BIG: usize = 10 * 1024 * 1024;
fn require(value: bool, message: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(message.to_owned().into())
    }
}
fn append(path: &Path, value: &Value) -> Result<()> {
    writeln!(
        OpenOptions::new().create(true).append(true).open(path)?,
        "{}",
        serde_json::to_string(value)?
    )?;
    Ok(())
}
fn sorted(path: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(path)?
        .map(|v| v.map(|v| v.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    Ok(paths)
}
fn record(packet: &[u8], error: bool) -> Result<(Value, u64)> {
    let message = capnp::serialize::read_message(Cursor::new(packet), Default::default())?;
    let event = message.get_root::<event::Reader<'_>>()?;
    let text = match (error, event.which()?) {
        (false, event::LogMessage(v)) | (true, event::ErrorLogMessage(v)) => v?,
        _ => return Err("collector publication has wrong union".into()),
    };
    Ok((
        serde_json::from_str(text.to_str()?)?,
        event.get_log_mono_time(),
    ))
}
struct Collector<'a> {
    process: Process<'a>,
    log: Subscriber,
    error: Subscriber,
    root: PathBuf,
}
impl<'a> Collector<'a> {
    fn start(output: &Path, worker: &Path, cancel: &'a AtomicBool) -> Result<Self> {
        let prefix = std::env::var("OPENPILOT_PREFIX")?;
        let endpoint = format!("ipc:///tmp/logmessage{prefix}");
        let home = output.join("collector/home");
        let root = home.join(format!(".comma{prefix}/log"));
        fs::create_dir_all(&root)?;
        let mut command = Command::new(worker);
        command
            .arg("--stats-collector")
            .arg(&endpoint)
            .arg(&root)
            .env("HOME", &home)
            .stderr(Stdio::from(File::create(
                output.join("collector/stderr.log"),
            )?));
        let process = Process::spawn(&mut command, cancel, false)?;
        let shm = PathBuf::from(format!("/dev/shm/msgq_{prefix}"));
        let deadline = Instant::now() + Duration::from_secs(20);
        while !["logMessage", "errorLogMessage"]
            .iter()
            .all(|s| shm.join(s).exists())
        {
            check(cancel)?;
            require(
                process.exited()?.is_none(),
                "collector exited before queues",
            )?;
            require(Instant::now() < deadline, "collector startup timeout")?;
            std::thread::sleep(Duration::from_millis(10));
        }
        for name in ["logMessage", "errorLogMessage"] {
            require(
                fs::metadata(shm.join(name))?.len() > BIG as u64,
                "collector queue capacity reduced",
            )?;
        }
        let mut result = Self {
            process,
            log: Subscriber::for_runtime("logMessage", false, BIG)?,
            error: Subscriber::for_runtime("errorLogMessage", false, BIG)?,
            root,
        };
        // An actual debug ACK excludes the publisher-reset window. Debug is not written to disk.
        let context = zmq::Context::new();
        let socket = context.socket(zmq::PUSH)?;
        socket.set_linger(0)?;
        socket.set_sndtimeo(10000)?;
        socket.connect(&endpoint)?;
        let mut sequence = 0_u64;
        'ready: loop {
            check(cancel)?;
            require(Instant::now() < deadline, "collector readiness timeout")?;
            sequence += 1;
            let marker = json!({"msg":format!("stats-check-ready-{sequence}"),"levelnum":10});
            let text = serde_json::to_vec(&marker)?;
            socket.send_multipart([&[10_u8][..], text.as_slice()], 0)?;
            let retry = Instant::now() + Duration::from_millis(50);
            while Instant::now() < retry {
                if let Some(packet) = result
                    .log
                    .receive(retry.saturating_duration_since(Instant::now()))?
                {
                    if record(&packet, false)?.0 == marker {
                        break 'ready;
                    }
                }
            }
        }
        while result.log.receive(Duration::ZERO)?.is_some() {}
        require(
            result.error.receive(Duration::ZERO)?.is_none(),
            "unexpected readiness error",
        )?;
        let channel =
            std::env::var_os("STATS_ENDPOINT_OWNER").ok_or("collector endpoint owner missing")?;
        crate::qa_endpoint::acknowledge(
            Path::new(
                endpoint
                    .strip_prefix("ipc://")
                    .ok_or("collector endpoint URI")?,
            ),
            Path::new(&channel),
            &prefix,
            cancel,
        )?;
        Ok(result)
    }
    fn stop(&mut self) -> Result<Vec<Value>> {
        self.process.signal(Signal::INT)?;
        require(
            self.process.wait(Some(Duration::from_secs(10)))? == -2,
            "collector must terminate by SIGINT",
        )?;
        let mut result = vec![];
        for path in sorted(&self.root)? {
            if path
                .file_name()
                .and_then(|v| v.to_str())
                .is_some_and(|v| v.starts_with("swaglog."))
            {
                for line in fs::read_to_string(path)?.lines() {
                    let row: Value = serde_json::from_str(line)?;
                    result.push(json!({"msg":row.get("msg$s").or_else(||row.get("msg")).cloned().unwrap_or(Value::Null),"levelnum":row["levelnum"]}));
                }
            }
        }
        Ok(result)
    }
}
fn clock(process: &mut Process<'_>, output: &Path) -> Result<()> {
    let line = process.line(Some(Duration::from_secs(10)))?;
    append(&output.join("protocol.jsonl"), &json!({"received":line}))?;
    require(line == "clock", "daemon did not acknowledge clock")
}
fn tick(process: &mut Process<'_>, output: &Path, value: &str) -> Result<()> {
    process.send(&format!("{value}\n"))?;
    clock(process, output)
}
fn metric(producer: &mut Process<'_>, value: &str) -> Result<()> {
    producer.send(&format!("{value}\n"))?;
    require(
        producer.line(None)? == "Sent",
        "producer did not acknowledge Sent",
    )
}
fn device(publisher: &mut Publisher, started: bool) -> Result<()> {
    let mut message = capnp::message::Builder::new_default();
    {
        let mut event = message.init_root::<event::Builder<'_>>();
        event.set_valid(true);
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
        event.set_log_mono_time((now.tv_sec as u64) * 1_000_000_000 + now.tv_nsec as u64);
        event.init_device_state().set_started(started);
    }
    publisher.send(&capnp::serialize::write_message_to_words(&message))?;
    Ok(())
}
fn barrier(
    collector: &mut Collector<'_>,
    process: &mut Process<'_>,
    producer: &mut Process<'_>,
    output: &Path,
    label: &str,
    now: &str,
) -> Result<()> {
    let marker = format!("barrier-{label}");
    metric(producer, &marker)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        tick(process, output, now)?;
        while let Some(packet) = collector.log.receive(Duration::ZERO)? {
            let (row, observed) = record(&packet, false)?;
            if row["msg"] == json!({"event":"malformed metric","metric":marker}) {
                fs::write(output.join(format!("{marker}.bin")), packet)?;
                append(
                    &output.join("barriers.jsonl"),
                    &json!({"marker":marker,"clock":now.parse::<f64>()?,"log_mono_time":observed}),
                )?;
                return Ok(());
            }
        }
    }
    Err("metric-consumption barrier timed out".into())
}
fn full_error(collector: &mut Collector<'_>, output: Option<&Path>) -> Result<()> {
    let packet = collector
        .error
        .receive(Duration::from_secs(10))?
        .ok_or("missing stats directory full error")?;
    require(
        record(&packet, true)?.0["msg"] == "stats dir full",
        "wrong full-directory diagnostic",
    )?;
    if let Some(path) = output {
        fs::write(path, packet)?;
    }
    Ok(())
}
fn continuous(
    collector: &mut Collector<'_>,
    process: &mut Process<'_>,
    binaries: &Path,
    output: &Path,
    endpoint: &str,
    cancel: &AtomicBool,
) -> Result<Value> {
    let stats = output.join("stats");
    let mut publisher = Publisher::for_runtime("deviceState", 250 * 1024)?;
    let mut command = Command::new(binaries.join("examples/stats_producer"));
    command
        .arg(endpoint)
        .stderr(Stdio::from(File::create(output.join("producer.stderr"))?));
    let mut producer = Process::spawn(&mut command, cancel, false)?;
    for item in [
        "b:1|g",
        "a:3|g",
        "b:4|g",
        "s:1e16|sa",
        "s:1|sa",
        "s:-1e16|sa",
        "bad",
        "x:bad|unknown",
        "x:1|unknown",
        "colon:1:ignored|g|ignored",
        "unicode:１２_３.４|g",
        "zero:-0|g",
        "special:+Infinity|sa",
        "special:-INFINITY|sa",
        "special:nan|sa",
        "bad:1__2|g",
        "bad:_1|g",
        "bad:1_|g",
        "space:\u{a0}1.5\u{2003}|g",
        "bad:1\u{1c}|g",
    ] {
        metric(&mut producer, item)?;
    }
    let bytes = include_bytes!("../../tests/fixtures/stats-daemon/unicode-digits.json");
    use sha2::{Digest, Sha256};
    require(
        format!("{:x}", Sha256::digest(bytes))
            == "8566f6e647631df2ebc16d78aea56261d6c95adc43edbb7fe9784972c7f81a29",
        "original Unicode metric inputs changed",
    )?;
    let digits: Vec<String> = serde_json::from_slice(bytes)?;
    require(digits.len() == 680, "incomplete Unicode decimal corpus")?;
    for item in digits {
        metric(&mut producer, &item)?;
    }
    device(&mut publisher, false)?;
    barrier(collector, process, &mut producer, output, "initial", "100")?;
    tick(process, output, "160")?;
    require(sorted(&stats)?.is_empty(), "equal60s clock must not flush")?;
    tick(process, output, "160.001")?;
    require(
        sorted(&stats)?.is_empty(),
        "file published before last_flush clock",
    )?;
    tick(process, output, "161")?;
    let first = sorted(&stats)?;
    require(
        first.len() == 1
            && first[0]
                .file_name()
                .is_some_and(|v| v.as_encoded_bytes().ends_with(b"_0")),
        "first statistics file/index differs",
    )?;
    require(
        fs::metadata(&first[0])?.permissions().mode() & 0o777 == 0o600,
        "statistics file mode differs",
    )?;
    metric(&mut producer, "after:9|g")?;
    barrier(collector, process, &mut producer, output, "after", "161")?;
    device(&mut publisher, true)?;
    for _ in 0..6 {
        tick(process, output, "161")?;
        if sorted(&stats)?.len() == 2 {
            break;
        }
    }
    let files = sorted(&stats)?;
    require(
        files.len() == 2
            && files[1]
                .file_name()
                .is_some_and(|v| v.as_encoded_bytes().ends_with(b"_1")),
        "started transition did not flush second file",
    )?;
    require(
        fs::read_to_string(&files[1])?.contains("started=True"),
        "started tag differs",
    )?;
    for index in 0..9998 {
        File::create(stats.join(format!("cap-{index}")))?;
    }
    tick(process, output, "223")?;
    tick(process, output, "224")?;
    full_error(collector, Some(&output.join("full-error.bin")))?;
    metric(&mut producer, "discarded:8|g")?;
    barrier(
        collector,
        process,
        &mut producer,
        output,
        "discarded",
        "225",
    )?;
    tick(process, output, "286")?;
    tick(process, output, "287")?;
    full_error(collector, None)?;
    for path in sorted(&stats)? {
        if path
            .file_name()
            .is_some_and(|v| v.as_encoded_bytes().starts_with(b"cap-"))
        {
            fs::remove_file(path)?;
        }
    }
    tick(process, output, "348")?;
    tick(process, output, "349")?;
    require(
        sorted(&stats)?.len() == 2,
        "full-directory flush failed to discard pending metrics",
    )?;
    process.close_input();
    require(
        process.wait(Some(Duration::from_secs(10)))? == 0,
        "daemon EOF failed",
    )?;
    producer.close_input();
    require(
        producer.wait(Some(Duration::from_secs(10)))? == 0,
        "producer EOF failed",
    )?;
    let contents = files
        .iter()
        .map(fs::read_to_string)
        .collect::<std::io::Result<Vec<_>>>()?;
    let modes = files
        .iter()
        .map(|p| fs::metadata(p).map(|m| m.permissions().mode() & 0o777))
        .collect::<std::io::Result<Vec<_>>>()?;
    Ok(json!({"files":contents,"modes":modes,"logs":collector.stop()?}))
}
fn socket(endpoint: &str) -> Result<(zmq::Socket, zmq::Context)> {
    let context = zmq::Context::new();
    let socket = context.socket(zmq::PUSH)?;
    socket.set_linger(10)?;
    socket.set_sndtimeo(5000)?;
    socket.connect(endpoint)?;
    Ok((socket, context))
}
fn failures(
    collector: &mut Collector<'_>,
    process: &mut Process<'_>,
    case: &str,
    output: &Path,
    endpoint: &str,
) -> Result<Value> {
    let stats = output.join("stats");
    let (producer, _context) = socket(endpoint)?;
    producer.send(
        if case == "utf8" {
            &b"bad\xff"[..]
        } else {
            &b"x:1|g"[..]
        },
        0,
    )?;
    process.send("100\n")?;
    if case != "utf8" {
        clock(process, output)?;
        tick(process, output, "161")?;
        if case == "missing-directory" {
            fs::remove_dir(&stats)?;
        }
        if case == "permission" {
            fs::set_permissions(&stats, fs::Permissions::from_mode(0o500))?;
        }
        process.send("162\n")?;
        if matches!(case, "collision" | "dangling") {
            clock(process, output)?;
            let first = sorted(&stats)?
                .into_iter()
                .next()
                .ok_or("missing first stats file")?;
            let name = first
                .file_name()
                .and_then(|v| v.to_str())
                .ok_or("invalid generated statistics filename")?;
            let destination = first.with_file_name(format!("{}1", &name[..name.len() - 1]));
            if case == "collision" {
                fs::write(&destination, "existing-destination")?;
            } else {
                symlink(stats.join("absent-target"), &destination)?;
            }
            producer.send("next:2|g", 0)?;
            tick(process, output, "162")?;
            tick(process, output, "223")?;
            process.send("224\n")?;
        }
        if matches!(case, "missing-param" | "invalid-param" | "dangling") {
            clock(process, output)?;
            process.close_input();
        }
    }
    let code = process.wait(Some(Duration::from_secs(10)))?;
    require(
        (code == 0) == matches!(case, "missing-param" | "invalid-param" | "dangling"),
        "failure scenario exit classification differs",
    )?;
    drop(producer);
    drop(_context);
    let logs = collector.stop()?;
    let mut files = vec![];
    if stats.exists() {
        for path in sorted(&stats)? {
            let bytes = fs::read(&path)?;
            files.push(json!({"content":bytes.iter().map(|v|format!("{v:02x}")).collect::<String>(),"mode":fs::metadata(&path)?.permissions().mode()&0o777,"temporary":path.file_name().is_some_and(|v|v.as_encoded_bytes().starts_with(b"tmp"))}));
        }
    }
    if case == "missing-directory" {
        require(!stats.exists(), "removed directory was recreated")?;
    }
    if case == "dangling" {
        require(
            !sorted(&stats)?.iter().any(|p| p.is_symlink()),
            "dangling destination link not replaced",
        )?;
    }
    if case == "permission" {
        fs::set_permissions(&stats, fs::Permissions::from_mode(0o700))?;
    }
    Ok(json!({"files":files,"logs":logs}))
}
fn log_fault(
    collector: &mut Collector<'_>,
    process: &mut Process<'_>,
    output: &Path,
    endpoint: &str,
) -> Result<Value> {
    let (producer, _context) = socket(endpoint)?;
    producer.send("x:1|unknown", 0)?;
    tick(process, output, "100")?;
    let attempts: Value = serde_json::from_slice(&fs::read(output.join("attempts.json"))?)?;
    process.close_input();
    require(
        process.wait(Some(Duration::from_secs(5)))? == 0,
        "fault daemon EOF failed",
    )?;
    drop(producer);
    drop(_context);
    require(
        attempts == json!(["unknown metric type", "malformed metric"]),
        "one-shot log failure did not fall through",
    )?;
    let packet = collector
        .log
        .receive(Duration::from_secs(10))?
        .ok_or("missing fallback log publication")?;
    fs::write(output.join("logMessage.bin"), &packet)?;
    let row = record(&packet, false)?.0;
    let logs = collector.stop()?;
    require(logs.len() == 1, "fallback disk record count differs")?;
    Ok(
        json!({"attempts":attempts,"msg":row["msg"],"levelnum":row["levelnum"],"disk_records":logs.len()}),
    )
}
pub(super) fn run(
    case: &str,
    binaries: &Path,
    output: &Path,
    sockets: &Path,
    worker: &Path,
) -> Result<Value> {
    let cancel = AtomicBool::new(false);
    let mut collector = Collector::start(output, worker, &cancel)?;
    let prefix = std::env::var("OPENPILOT_PREFIX")?;
    let params = output.join("params").join(prefix);
    fs::create_dir_all(&params)?;
    if case == "daemon" {
        fs::write(params.join("DongleId"), "fixture한")?;
    } else if case == "invalid-param" {
        fs::write(params.join("DongleId"), b"invalid\xff'\n")?;
    }
    let metadata = output.join("metadata");
    fs::create_dir(&metadata)?;
    let content=match case{"daemon"=>serde_json::to_string(&json!({"channel":["test",true,null],"openpilot":{"version":{"v":0.00001},"git_origin":"https://github.com/test/openpilot.git"}}))?,"log-fault"=>"{\"openpilot\":{\"git_origin\":\"https://github.com/test/openpilot.git\"}}".into(),"surrogate"=>"{\"openpilot\":{\"version\":\"\\ud800\",\"git_origin\":\"git@github.com:test/openpilot.git\"}}".into(),_=>"{\"openpilot\":{\"version\":\"v\",\"git_origin\":\"git@github.com:test/openpilot.git\"}}".into()};
    fs::write(metadata.join("build.json"), content)?;
    let endpoint = format!("ipc://{}", sockets.join("stats").display());
    let mut command = Command::new(binaries.join("examples/stats_daemon"));
    command
        .arg(&endpoint)
        .arg(output.join("stats"))
        .arg(&metadata)
        .stderr(Stdio::from(File::create(output.join("daemon.stderr"))?));
    if case == "log-fault" {
        command.env(
            "STATS_ORACLE_LOG_FAILURE_TRACE",
            output.join("attempts.json"),
        );
    } else {
        command.env_remove("STATS_ORACLE_LOG_FAILURE_TRACE");
    }
    let mut process = Process::spawn(&mut command, &cancel, false)?;
    clock(&mut process, output)?;
    match case {
        "daemon" => continuous(
            &mut collector,
            &mut process,
            binaries,
            output,
            &endpoint,
            &cancel,
        ),
        "log-fault" => log_fault(&mut collector, &mut process, output, &endpoint),
        _ => failures(&mut collector, &mut process, case, output, &endpoint),
    }
}
