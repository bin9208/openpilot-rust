use super::{contract, create, fixture, require, save, Request, Result, Signals};
use openpilot_dev_tools::qa_process::LiveChild;
use openpilot_log_format::JsonValue;
use openpilot_messaging::{runtime::PubMaster, services};
use openpilot_msgq::Subscriber;
use rustix::fs::{fcntl_getfl, fcntl_setfl, OFlags};
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::{fd::OwnedFd, unix::net::UnixStream},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const DIAGNOSTIC_LIMIT: usize = 16 * 1024 * 1024;
struct Diagnostic {
    stream: File,
    log: File,
    count: usize,
}
impl Diagnostic {
    fn open(path: &Path) -> Result<(Self, Stdio)> {
        let (reader, writer) = UnixStream::pair()?;
        let stream = File::from(OwnedFd::from(reader));
        fcntl_setfl(&stream, fcntl_getfl(&stream)? | OFlags::NONBLOCK)?;
        Ok((
            Self {
                stream,
                log: File::options().write(true).create_new(true).open(path)?,
                count: 0,
            },
            Stdio::from(OwnedFd::from(writer)),
        ))
    }
    fn pump(&mut self) -> Result<()> {
        // Bound each turn so cancellation/deadlines remain observable under spam.
        for _ in 0..8 {
            let mut bytes = [0; 8192];
            match self.stream.read(&mut bytes) {
                Ok(0) => break,
                Ok(n) => {
                    let keep = n.min(DIAGNOSTIC_LIMIT.saturating_sub(self.count));
                    self.log.write_all(&bytes[..keep])?;
                    self.count += keep;
                    if keep != n {
                        self.log.flush()?;
                        return Err(
                            "daemon diagnostic exceeds retained 16 MiB prefix; excess omitted"
                                .into(),
                        );
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        self.log.flush()?;
        Ok(())
    }
}
fn pump(logs: &mut [Diagnostic; 2], signals: &Signals) -> Result<()> {
    signals.check()?;
    for log in logs {
        log.pump()?;
    }
    Ok(())
}
fn alive(child: &mut LiveChild) -> Result<()> {
    require(child.poll()?.is_none(), "controlsd exited before shutdown")
}
fn reader(shm: &Path, service: &str, pid: u32) -> Result<bool> {
    let mut header = [0; 984];
    File::open(shm.join(service))?.read_exact(&mut header)?;
    contract::reader_header(&header, pid)
}
fn writers(shm: &Path, pid: u32) -> Result<bool> {
    for service in ["controlsState", "carControl"] {
        let mut header = [0; 24];
        File::open(shm.join(service))?.read_exact(&mut header)?;
        if !contract::writer_header(&header, pid)? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn receive(
    subscriber: &mut Subscriber,
    child: &mut LiveChild,
    logs: &mut [Diagnostic; 2],
    signals: &Signals,
) -> Result<Vec<u8>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        pump(logs, signals)?;
        alive(child)?;
        require(Instant::now() < deadline, "native publication timeout")?;
        if let Some(packet) = subscriber.receive(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(10)),
        )? {
            require(packet.len() <= 1024 * 1024, "publication exceeds 1 MiB")?;
            return Ok(packet);
        }
    }
}
pub(super) fn fixture_row() -> Result<Value> {
    let row = fixture::generate(&JsonValue::parse(r#"{"op":"frame","index":30}"#)?)?;
    let request = format!(
        r#"{{"op":"change","row":{},"omit":["carrotMan"]}}"#,
        row.to_json()?
    );
    Ok(serde_json::from_str(
        &fixture::generate(&JsonValue::parse(&request)?)?.to_json()?,
    )?)
}
fn base_case(signal: i32) -> Result<Value> {
    let request = json!({"op":"case","name":format!("native-signal-{signal}"),"fingerprint":"HYUNDAI_PALISADE","mode":"torque","frames":[],"settings":{"NNFF":"1"}});
    let case = fixture::generate(&JsonValue::parse(&serde_json::to_string(&request)?)?)?;
    Ok(serde_json::from_str(&case.to_json()?)?)
}
pub(super) fn phase(request: &Request) -> Result<()> {
    require(
        rustix::process::getpgrp() == rustix::process::getpid(),
        "capture must run in an owned worker group",
    )?;
    require(
        request.signal == 2 || request.signal == 15,
        "unexpected phase signal",
    )?;
    if request.signal == 2 {
        require(
            request.saved.is_empty(),
            "first phase cannot have saved settings",
        )?;
    } else {
        contract::persisted(&request.saved)?;
    }
    let signals = Signals::new()?;
    let prefix = request
        .shm
        .file_name()
        .and_then(|x| x.to_str())
        .and_then(|x| x.strip_prefix("msgq_"))
        .ok_or("private msgq prefix")?;
    let params = request.params.join(prefix);
    fs::create_dir(&params)?;
    let mut case = base_case(request.signal)?;
    // Source constructs CarParams BEFORE merging saved values on restart.
    for (key, value) in &request.saved {
        case["params"][key] = json!(value.as_bytes());
    }
    for (key, bytes) in case["params"]
        .as_object()
        .ok_or("fixture Params required")?
    {
        if key != "CarParams" {
            create(
                &params.join(key),
                &serde_json::from_value::<Vec<u8>>(bytes.clone())?,
            )?;
        }
    }
    for (name, value) in [
        ("OPENPILOT_PREFIX", prefix.to_owned()),
        ("PARAMS_ROOT", request.params.to_string_lossy().into_owned()),
        ("SIMULATION", "1".into()),
        (
            "PATH",
            request
                .params
                .join("no-programs")
                .to_string_lossy()
                .into_owned(),
        ),
        ("OPENBLAS_NUM_THREADS", "1".into()),
    ] {
        std::env::set_var(name, value);
    }
    std::env::remove_var("ZMQ");
    std::env::remove_var("CEREAL_FAKE");
    let mut publisher = PubMaster::for_runtime(&fixture::SERVICES)?;
    let mut subscribers = Vec::new();
    for service in ["controlsState", "carControl"] {
        let mut subscriber = Subscriber::for_runtime(
            service,
            false,
            services::lookup(service)
                .ok_or("publication service")?
                .queue_size,
        )?;
        let _ = subscriber.receive(Duration::ZERO)?;
        subscribers.push(subscriber);
    }
    save(
        &request.output.join("peer-ready.json"),
        &json!({"ready":true,"native_ipc":"in-process PubMaster/Subscriber","pid":std::process::id()}),
    )?;
    let (stdout, out) = Diagnostic::open(&request.output.join("stdout.log"))?;
    let (stderr, err) = Diagnostic::open(&request.output.join("stderr.log"))?;
    let mut logs = [stdout, stderr];
    let mut command = Command::new(&request.daemon);
    command
        .args([
            std::ffi::OsStr::new("--numerics"),
            request.numerics.as_os_str(),
            std::ffi::OsStr::new("--assets"),
            request.assets.as_os_str(),
        ])
        .current_dir(&request.root)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    let mut daemon = LiveChild::spawn_in_owned_worker(&mut command)?;
    drop(command);
    let pid = daemon.pid()?;
    let wait = Instant::now() + Duration::from_millis(150);
    while Instant::now() < wait {
        pump(&mut logs, &signals)?;
        alive(&mut daemon)?;
        thread::sleep(Duration::from_millis(1));
    }
    require(
        params.join("LongitudinalPersonalityMax").exists() == (request.signal == 15),
        "Params initialized before CarParams",
    )?;
    require(
        !reader(&request.shm, "selfdriveState", pid)?,
        "native subscription opened before CarParams",
    )?;
    let car: Vec<u8> = serde_json::from_value(case["params"]["CarParams"].clone())?;
    create(&params.join("CarParams"), &car)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let readiness = loop {
        pump(&mut logs, &signals)?;
        alive(&mut daemon)?;
        let ready = contract::READERS
            .iter()
            .map(|name| Ok((name.to_string(), reader(&request.shm, name, pid)?)))
            .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
        if ready.values().all(|v| *v) && writers(&request.shm, pid)? {
            break ready;
        }
        require(
            Instant::now() < deadline,
            "native subscriptions/writers not ready within ten seconds",
        )?;
        thread::sleep(Duration::from_micros(500));
    };
    save(
        &request.output.join("reader-readiness.json"),
        &json!({"daemon_pid":pid,"readers":readiness,"writers_ready":true,"before_carparams":false}),
    )?;
    let row = fixture_row()?;
    for subscriber in &mut subscribers {
        let _ = subscriber.receive(Duration::ZERO)?;
    }
    for bytes in row["messages"]
        .as_array()
        .ok_or("fixture messages required")?
    {
        let packet: Vec<u8> = serde_json::from_value(bytes.clone())?;
        publisher.send(&fixture::service(&packet)?, &packet)?;
    }
    let executable = fs::read_link(format!("/proc/{pid}/exe"))?;
    require(
        executable == request.daemon,
        "native executable identity differs",
    )?;
    let maps = fs::read(format!("/proc/{pid}/maps"))?;
    require(maps.len() <= 4 * 1024 * 1024, "native maps exceed bound")?;
    let text = std::str::from_utf8(&maps)?;
    require(
        !["libpython", "numpy/_core", "cython"]
            .iter()
            .any(|x| text.contains(x))
            && text.contains("libscipy_openblas"),
        "native runtime provider mappings differ",
    )?;
    create(&request.output.join("maps.txt"), &maps)?;
    let cmdline = fs::read(format!("/proc/{pid}/cmdline"))?;
    require(cmdline.len() <= 1024 * 1024, "native cmdline exceeds bound")?;
    create(&request.output.join("cmdline.bin"), &cmdline)?;
    let mut packets = Vec::new();
    let mut frames = Vec::new();
    let mut received = false;
    let mut packet_bytes = 0_usize;
    let mut packet_log = File::options()
        .write(true)
        .create_new(true)
        .open(request.output.join("packets.jsonl"))?;
    let mut stamps = Vec::new();
    for _ in 0..140 {
        let mut raw = Vec::new();
        for subscriber in &mut subscribers {
            raw.push(receive(subscriber, &mut daemon, &mut logs, &signals)?);
        }
        let serialized = serde_json::to_vec(&raw)?;
        packet_bytes += serialized.len() + 1;
        require(
            packet_bytes <= 16 * 1024 * 1024,
            "retained packet log exceeds 16 MiB",
        )?;
        packet_log.write_all(&serialized)?;
        packet_log.write_all(b"\n")?;
        packet_log.flush()?;
        let value = contract::pair(&raw)?;
        frames.push(json!({"time":super::numeric::float(value.controls_time as f64 / 1e9),"messages":if value.has_input && !received { row["messages"].clone() } else { json!([]) }}));
        received |= value.has_input;
        stamps.push([value.controls_time, value.control_time]);
        packets.push(raw);
    }
    require(received, "native daemon did not consume the input")?;
    let persisted = ["LongitudinalPersonalityMax", "HyundaiCameraSccHint"]
        .iter()
        .map(|name| Ok((name.to_string(), fs::read_to_string(params.join(name))?)))
        .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
    contract::persisted(&persisted)?;
    let before = Instant::now();
    daemon.signal(if request.signal == 2 {
        rustix::process::Signal::INT
    } else {
        rustix::process::Signal::TERM
    })?;
    let code = loop {
        pump(&mut logs, &signals)?;
        if let Some(code) = daemon.poll()? {
            break code;
        }
        require(
            before.elapsed() < Duration::from_secs(5),
            "native shutdown exceeded five seconds",
        )?;
        thread::sleep(Duration::from_millis(1));
    };
    let shutdown = before.elapsed().as_secs_f64();
    require(
        code == 0 && shutdown <= 5.0,
        "native shutdown contract differs",
    )?;
    pump(&mut logs, &signals)?;
    let close = Instant::now();
    drop(subscribers);
    drop(publisher);
    let ipc_close = close.elapsed().as_secs_f64();
    require(
        ipc_close <= 5.0,
        "native IPC peer cleanup exceeded five seconds",
    )?;
    case["simulation"] = json!(true);
    case["frames"] = json!(frames);
    save(&request.output.join("input.json"), &case)?;
    save(&request.output.join("packets.json"), &json!(packets))?;
    save(&request.output.join("persisted.json"), &json!(persisted))?;
    save(
        &request.output.join("facts.json"),
        &json!({"capture_complete":true,"restart_from_persisted_params":request.signal==15,"frames":140,"publications":280,"startup_waits_for_carparams":true,"parameter_writes":persisted,"signal":request.signal,"exit":code,"shutdown_seconds":shutdown,"native_ipc_close_seconds":ipc_close,"native_executable":executable,"no_python_runtime":true,"publication_clocks":stamps}),
    )?;
    Ok(())
}
