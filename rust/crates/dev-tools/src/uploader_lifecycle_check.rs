//! Native uploader sibling controllers checked against sealed original captures.
//! Original helpers remain while sibling and maintained caller closure proceeds.
mod corpus;
mod interface;
mod records;
mod timeouts;
use crate::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_msgq::Publisher;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub metered: bool,
    pub force: bool,
    pub none: bool,
    pub fake: bool,
    pub sleep: bool,
    pub sigint: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    root: PathBuf,
    binary: PathBuf,
    output: PathBuf,
    private: PathBuf,
    prefix: String,
    settings: Settings,
    environment: BTreeMap<String, String>,
}
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub worker: PathBuf,
    pub output: PathBuf,
}
pub use interface::cli;
pub fn run_timeouts(
    root: &Path,
    trace: &Path,
    output: &Path,
    expected: &[Value],
    late_only: bool,
) -> Result<Value> {
    timeouts::run(root, trace, output, expected, late_only)
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
struct Owned(Option<Child>);
impl Owned {
    fn alive(&mut self) -> Result<()> {
        let child = self.0.as_mut().ok_or("daemon no longer owned")?;
        if child.try_wait()?.is_some() {
            self.0 = None;
            return Err("uploader daemon exited unexpectedly".into());
        }
        Ok(())
    }
    fn stop(&mut self, signal: rustix::process::Signal) -> Result<(i32, f64)> {
        self.alive()?;
        let child = self.0.as_ref().ok_or("daemon absent")?;
        let start = Instant::now();
        rustix::process::kill_process(rustix::process::Pid::from_child(child), signal)?;
        loop {
            let status = self.0.as_mut().ok_or("daemon absent")?.try_wait()?;
            if let Some(status) = status {
                self.0 = None;
                return Ok((
                    status
                        .code()
                        .unwrap_or_else(|| -status.signal().unwrap_or(1)),
                    start.elapsed().as_secs_f64(),
                ));
            }
            require(
                start.elapsed() < Duration::from_secs(2),
                "uploader daemon signal timeout",
            )?;
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
use crate::uploader_logging_check::ipc::ownership::Namespace;
fn marked(path: &Path) -> bool {
    let mut bytes = [0; 32];
    matches!(
        rustix::fs::getxattr(path, "user.upload", &mut bytes[..]),
        Ok(1)
    ) && bytes[0] == b'1'
}
fn input(settings: &Settings) -> Result<Vec<u8>> {
    let mut message = capnp::message::Builder::new_default();
    {
        let mut event = message.init_root::<openpilot_cereal::log_capnp::event::Builder>();
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        event.set_log_mono_time(
            u64::try_from(now.tv_sec)? * 1_000_000_000 + u64::try_from(now.tv_nsec)?,
        );
        event.set_valid(false);
        let mut device = event.init_device_state();
        device.set_network_type(u16::from(!settings.none).try_into()?);
        device.set_network_metered(settings.metered);
    }
    Ok(capnp::serialize::write_message_to_words(&message))
}
fn ready(publisher: &mut Publisher, daemon: &mut Owned, seconds: u64) -> Result<()> {
    let start = Instant::now();
    while !publisher.readers_caught_up() {
        daemon.alive()?;
        require(
            start.elapsed() < Duration::from_secs(seconds),
            "uploader deviceState reader timeout",
        )?;
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}
fn wait_mark(
    publisher: &mut Publisher,
    daemon: &mut Owned,
    packet: &[u8],
    path: &Path,
) -> Result<()> {
    let start = Instant::now();
    while !marked(path) {
        publisher.send(packet)?;
        daemon.alive()?;
        require(
            start.elapsed() < Duration::from_secs(5),
            "uploader failed to mark input within original five-second bound",
        )?;
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}
fn file(path: &Path, size: usize) -> Result<()> {
    fs::create_dir_all(path.parent().ok_or("fixture parent absent")?)?;
    fs::write(
        path,
        (0..size)
            .map(|i| ((i * 23 + 17) % 256) as u8)
            .collect::<Vec<_>>(),
    )?;
    Ok(())
}
fn worker(path: &Path) -> Result<Value> {
    require(
        fs::metadata(path)?.len() < 1 << 20,
        "lifecycle worker request oversized",
    )?;
    let request: Request = serde_json::from_slice(&fs::read(path)?)?;
    require(
        request.prefix.starts_with("rust-probe-uploader-life-")
            && request.prefix.len() < 90
            && request
                .prefix
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "lifecycle worker prefix is not an owned basename",
    )?;
    require(
        std::env::var("OPENPILOT_PREFIX").ok().as_deref() == Some(&request.prefix),
        "uploader worker prefix differs",
    )?;
    let namespace = Namespace::worker(&request.prefix)?;
    let mut publisher = Publisher::for_runtime("deviceState", 250 * 1024)?;
    let context = zmq::Context::new();
    let collector = context.socket(zmq::PULL)?;
    collector.set_linger(0)?;
    collector.bind(&format!("ipc://{}", namespace.endpoint.display()))?;
    let temp = &request.private;
    fs::create_dir_all(temp)?;
    let logs = temp.join("logs");
    fs::create_dir(&logs)?;
    file(&logs.join("route--0/qlog.lock"), 0)?;
    let home = temp.join("home");
    let persist = home.join(format!(".comma{}/persist", request.prefix));
    crate::uploader_reference_check::http::write_ecdsa(&persist)?;
    let params = temp.join("params").join(&request.prefix);
    fs::create_dir_all(&params)?;
    fs::write(params.join("DongleId"), b"0000000000000000")?;
    fs::write(params.join("IsOffroad"), b"1")?;
    let server = crate::uploader_reference_check::http::Server::start(&json!({}))?;
    let mut command = Command::new(&request.binary);
    command
        .current_dir(&request.root)
        .env("ROUTE_UPLOAD_QA_IPC_ROOT", &namespace.ipc_root)
        .env("HOME", &home)
        .env("LOG_ROOT", &logs)
        .env(
            "PARAMS_ROOT",
            params.parent().ok_or("Params parent absent")?,
        )
        .env("API_HOST", &server.url)
        .env(
            "UPLOADER_SLEEP",
            if request.settings.sleep { "1" } else { "0" },
        )
        .env_remove("FORCEWIFI")
        .env_remove("FAKEUPLOAD")
        .env_remove("LOGPRINT")
        .env_remove("OPENPILOT_LOG_CONTEXT")
        .env_remove("MANAGER_DAEMON");
    if request.settings.force {
        command.env("FORCEWIFI", "");
    }
    if request.settings.fake {
        command.env("FAKEUPLOAD", "");
    }
    command
        .envs(&request.environment)
        .stdout(Stdio::from(File::create(
            request.output.join("daemon.stdout"),
        )?))
        .stderr(Stdio::from(File::create(
            request.output.join("daemon.stderr"),
        )?));
    let mut daemon = Owned(Some(command.spawn()?));
    let pid = daemon.0.as_ref().ok_or("daemon absent")?.id();
    ready(&mut publisher, &mut daemon, 5)?;
    daemon.alive()?;
    require(
        !logs.join("route--0/qlog.lock").exists(),
        "uploader failed to clear startup lock",
    )?;
    let packet = input(&request.settings)?;
    fs::write(request.output.join("device-state.capnp"), &packet)?;
    publisher.send(&packet)?;
    if request.settings.sleep {
        std::thread::sleep(Duration::from_millis(150));
        require(
            !marked(&logs.join("route--0/qlog")),
            "idle uploader marked missing input",
        )?;
    } else {
        ready(&mut publisher, &mut daemon, 3)?;
        let lock = logs.join("route--0/new.lock");
        file(&lock, 0)?;
        file(&logs.join("route--0/qlog"), 2048)?;
        file(&logs.join("route--0/qcamera.ts"), 1031)?;
        fs::remove_file(lock)?;
        wait_mark(
            &mut publisher,
            &mut daemon,
            &packet,
            &logs.join("route--0/qlog"),
        )?;
        if request.settings.metered {
            std::thread::sleep(Duration::from_millis(50));
            require(
                !marked(&logs.join("route--0/qcamera.ts")),
                "metered uploader ignored requested-route gate",
            )?;
            fs::write(
                params.join("AthenadRecentlyViewedRoutes"),
                b"synthetic-dongle|route",
            )?;
        }
        wait_mark(
            &mut publisher,
            &mut daemon,
            &packet,
            &logs.join("route--0/qcamera.ts"),
        )?;
    }
    let mut records = Vec::new();
    let mut packets = Vec::new();
    while collector.poll(zmq::POLLIN, 50)? != 0 {
        let bytes = collector.recv_bytes(0)?;
        require(
            bytes.len() > 1 && bytes.len() < 2 << 20,
            "uploader daemon raw log packet outside bound",
        )?;
        let record: Value = serde_json::from_slice(&bytes[1..])?;
        require(
            record["levelnum"] == u64::from(bytes[0]),
            "uploader log packet level differs",
        )?;
        fs::write(
            request
                .output
                .join(format!("log-{:03}.packet", records.len())),
            &bytes,
        )?;
        records.push(record);
        packets.push(bytes);
    }
    fs::write(
        request.output.join("records.json"),
        serde_json::to_vec_pretty(&records)?,
    )?;
    let signal = if request.settings.sigint {
        rustix::process::Signal::INT
    } else {
        rustix::process::Signal::TERM
    };
    let (code, elapsed) = daemon.stop(signal)?;
    require(
        code == 0 && elapsed < 1.0,
        "uploader normal signal contract changed",
    )?;
    let captures = serde_json::to_value(server.finish()?)?;
    fs::write(
        request.output.join("http.json"),
        serde_json::to_vec_pretty(&captures)?,
    )?;
    let uploads = captures
        .as_array()
        .ok_or("HTTP capture absent")?
        .iter()
        .filter(|r| r["method"] == "PUT")
        .collect::<Vec<_>>();
    if request.settings.sleep {
        require(
            captures.as_array().is_some_and(Vec::is_empty),
            "idle uploader unexpectedly issued HTTP",
        )?;
    } else {
        require(
            uploads.len() == if request.settings.fake { 0 } else { 2 },
            "uploader duplicated or lost HTTP uploads",
        )?;
        let successes = records
            .iter()
            .filter(|r| r["msg"]["event"] == "upload_success")
            .collect::<Vec<_>>();
        require(successes.len() == 2, "uploader lost successful records")?;
        for row in successes {
            require(
                row["process"] == pid
                    && row["ctx"]["runtime_language"] == "rust"
                    && row["msg"]["network_type"]
                        == u64::from(!(request.settings.force && request.settings.none))
                    && row["msg"]["metered"] == request.settings.metered,
                "uploader success context/network semantics changed",
            )?;
        }
    }
    let result = json!({"passed":true,"exit_code":code,"signal_seconds":elapsed,"requests":captures,"log_packets":records.len(),"queue_bytes":fs::metadata(namespace.shm.join("deviceState"))?.len(),"cleared_startup_lock":true,"qlog_marked":marked(&logs.join("route--0/qlog")),"qcamera_marked":marked(&logs.join("route--0/qcamera.ts")),"packet_count":packets.len()});
    drop(collector);
    drop(context);
    drop(publisher);
    drop(namespace);
    Ok(result)
}
/// Dispatch this before ordinary CLI parsing in the eventual registered wrapper.
pub fn worker_cli(args: &[String]) -> Result<bool> {
    if args
        .first()
        .is_none_or(|s| s != "--uploader-lifecycle-worker")
    {
        return Ok(false);
    }
    require(args.len() == 2, "lifecycle worker arguments invalid")?;
    println!("{}", worker(Path::new(&args[1]))?);
    Ok(true)
}
/// Six actual daemon/IPC/HTTP/shutdown scenarios from the original controller.
/// Historical native observations are never used as expected uploader outputs.
pub fn run_daemon(options: &Options) -> Result<Value> {
    fs::create_dir_all(&options.output)?;
    let cases = [
        ("wifi", Settings::default()),
        (
            "metered-requested",
            Settings {
                metered: true,
                ..Default::default()
            },
        ),
        (
            "force-none-metered",
            Settings {
                force: true,
                none: true,
                metered: true,
                ..Default::default()
            },
        ),
        (
            "fake",
            Settings {
                fake: true,
                ..Default::default()
            },
        ),
        (
            "no-network-sigterm",
            Settings {
                sleep: true,
                ..Default::default()
            },
        ),
        (
            "no-network-sigint",
            Settings {
                sleep: true,
                sigint: true,
                ..Default::default()
            },
        ),
    ];
    let mut rows = Vec::new();
    for (name, settings) in cases {
        rows.push(scenario(options, name, settings, BTreeMap::new())?);
    }
    let startup = startup_failures(options)?;
    Ok(json!({"passed":true,"scenarios":rows,"startup_errors":startup}))
}

fn scenario(
    options: &Options,
    name: &str,
    settings: Settings,
    environment: BTreeMap<String, String>,
) -> Result<Value> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let private_token = tempfile::tempdir()?;
    let token = private_token
        .path()
        .file_name()
        .ok_or("private token absent")?
        .to_string_lossy()
        .replace('.', "-");
    let output = options.output.join(name);
    fs::create_dir_all(&output)?;
    let prefix = format!(
        "rust-probe-uploader-life-{}-{token}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let namespace = Namespace::new(&prefix)?;
    let request = Request {
        root: options.root.clone(),
        binary: options.binary.clone(),
        output: output.clone(),
        private: private_token.path().join("private"),
        prefix: prefix.clone(),
        settings,
        environment,
    };
    let path = output.join("request.json");
    fs::write(&path, serde_json::to_vec(&request)?)?;
    let mut command = CommandSpec::new_os(
        vec![
            options.worker.as_os_str().to_owned(),
            "--uploader-lifecycle-worker".into(),
            path.into_os_string(),
        ],
        Duration::from_secs(30),
    );
    for (key, value) in namespace.environment()? {
        command.env.insert(key, value);
    }
    command.env.insert("OPENPILOT_PREFIX".into(), prefix);
    command.env_remove = vec!["ZMQ".into(), "CEREAL_FAKE".into()];
    let captured = NativeRunner.run(&command)?;
    fs::write(output.join("worker.stdout"), &captured.stdout)?;
    fs::write(output.join("worker.stderr"), &captured.stderr)?;
    require(captured.code == 0, "uploader lifecycle worker failed")?;
    let row: Value = serde_json::from_slice(&captured.stdout)?;
    fs::write(output.join("report.json"), serde_json::to_vec_pretty(&row)?)?;
    Ok(row)
}

fn startup_failures(options: &Options) -> Result<Value> {
    let temp = tempfile::tempdir()?;
    let logs = temp.path().join("logs");
    fs::create_dir(&logs)?;
    let params = temp.path().join("params/d");
    fs::create_dir_all(&params)?;
    let mut rows = Vec::new();
    for name in ["missing-id", "invalid-sleep", "missing-root"] {
        let mut command = CommandSpec::new_os(
            vec![
                options.binary.as_os_str().to_owned(),
                "--cycles".into(),
                "1".into(),
            ],
            Duration::from_secs(3),
        );
        command.cwd = Some(options.root.clone());
        for (key, path) in [
            ("HOME", temp.path()),
            (
                "PARAMS_ROOT",
                params.parent().ok_or("startup params parent absent")?,
            ),
            ("LOG_ROOT", logs.as_path()),
        ] {
            command.env.insert(
                key.into(),
                path.to_str().ok_or("nonUTF8 fixture path")?.into(),
            );
        }
        command.env.insert(
            "UPLOADER_SLEEP".into(),
            if name == "invalid-sleep" {
                "invalid"
            } else {
                "0"
            }
            .into(),
        );
        if name == "missing-root" {
            command.env.insert(
                "LOG_ROOT".into(),
                temp.path()
                    .join("absent")
                    .to_str()
                    .ok_or("nonUTF8 fixture path")?
                    .into(),
            );
        }
        command.env_remove = vec![
            "OPENPILOT_PREFIX".into(),
            "OPENPILOT_LOG_CONTEXT".into(),
            "MANAGER_DAEMON".into(),
        ];
        let captured = NativeRunner.run(&command)?;
        fs::write(
            options.output.join(format!("{name}.stdout")),
            &captured.stdout,
        )?;
        fs::write(
            options.output.join(format!("{name}.stderr")),
            &captured.stderr,
        )?;
        require(
            captured.code != 0,
            "uploader accepted invalid startup fixture",
        )?;
        rows.push(json!({"name":name,"exit_code":captured.code}));
    }
    Ok(json!(rows))
}

/// The same actual daemon scenario, with the original send-boundary provider.
pub fn run_interrupted_send(options: &Options) -> Result<Value> {
    let trace = options.output.join("send.tsv");
    let environment = BTreeMap::from([
        ("ZMQ_TEST_SEND_MATCH".into(), "upload_success".into()),
        ("ZMQ_TEST_SEND_ERRORS".into(), "4,4,4,0".into()),
        (
            "ZMQ_TEST_SEND_TRACE".into(),
            trace.to_str().ok_or("nonUTF8 trace path")?.into(),
        ),
    ]);
    let result = scenario(
        options,
        "force-none-metered",
        Settings {
            force: true,
            none: true,
            metered: true,
            ..Default::default()
        },
        environment,
    )?;
    let raw = fs::read(&trace)?;
    require(raw.len() < 1 << 20, "interrupted-send trace exceeds bound")?;
    let rows = std::str::from_utf8(&raw)?
        .lines()
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .collect::<Vec<_>>();
    require(
        rows.len() == 5 && rows.iter().all(|row| row.len() == 9),
        "interrupted uploader send attempt roster differs",
    )?;
    let integers = |column: usize| -> Result<Vec<i64>> {
        rows.iter().map(|row| Ok(row[column].parse()?)).collect()
    };
    require(
        integers(5)? == [4, 4, 4, 0, 0]
            && integers(3)? == [1, 1, 1, 0, 0]
            && integers(6)? == [1, 1, 1, 1, 1],
        "EINTR retries/flags differ from original source controller",
    )?;
    require(
        rows[..4].iter().all(|row| row[8] == rows[0][8]),
        "interrupted send retries changed packet",
    )?;
    let delivered = fs::read_dir(options.output.join("force-none-metered"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|s| s == "packet"))
        .map(fs::read)
        .collect::<std::io::Result<Vec<_>>>()?;
    for row in &rows {
        if row[4].parse::<i64>()? >= 0 {
            let packet = hex(row[8])?;
            require(
                delivered.iter().filter(|raw| **raw == packet).count() == 1,
                "accepted uploader send was lost or duplicated",
            )?;
        }
    }
    Ok(
        json!({"passed":true,"interrupted_attempts":3,"accepted_success_records":2,"qlog_and_qcamera_marked":true,"scenario":result}),
    )
}

fn hex(text: &str) -> Result<Vec<u8>> {
    require(
        text.len().is_multiple_of(2) && text.len() < 1 << 20,
        "invalid send packet hex length",
    )?;
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

/// Reuse the original compiled interrupted_read.c interposer; no launcher runs.
pub fn run_interrupted_http(options: &Options, provider: &Path) -> Result<Value> {
    let trace = options.output.join("read.trace");
    let environment = BTreeMap::from([
        (
            "LD_PRELOAD".into(),
            provider
                .to_str()
                .ok_or("nonUTF8 HTTP provider path")?
                .into(),
        ),
        (
            "HTTP_EINTR_TRACE".into(),
            trace.to_str().ok_or("nonUTF8 trace path")?.into(),
        ),
    ]);
    let result = scenario(
        options,
        "interrupted-put",
        Settings {
            metered: true,
            ..Default::default()
        },
        environment,
    )?;
    let raw = fs::read(&trace)?;
    require(
        raw == b"PUT recv EINTR\nPUT recv EINTR\n",
        "interrupted PUT read roster differs",
    )?;
    let requests = result["requests"]
        .as_array()
        .ok_or("missing actual HTTP requests")?;
    require(
        requests.iter().map(|r| r["method"].as_str()).eq([
            Some("GET"),
            Some("PUT"),
            Some("GET"),
            Some("PUT"),
        ]),
        "interrupted socket read repeated HTTP requests",
    )?;
    Ok(
        json!({"passed":true,"interrupted_put_reads":2,"repeated_http_requests":0,"scenario":result}),
    )
}
