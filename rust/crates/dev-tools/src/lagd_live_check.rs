//! Original synthetic IPC/Params checks with source-derived loop expectations.
//! Only native IPC transport is delegated to the unchanged lag_ipc_peer executable.
use crate::{
    control_tools_policy_check::json, lagd_check, lagd_frames as frames, lagd_loop_reference,
    lagd_numeric_cases::compact, lagd_packet_reference as packet, lagd_reference, python_fs,
    qa_process::Captured, seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
mod native;
pub use native::NativeRuntime;
#[derive(Clone, Copy)]
pub enum Kind {
    Daemon,
    ParamsIo,
}
#[derive(Clone, Copy)]
pub enum Scenario {
    Learning,
    Corrupt,
    Cadence,
    WaitSignal,
    ParamsCar,
    ParamsDelay,
}
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub peer: Option<PathBuf>,
    pub output: PathBuf,
}
pub struct Spawn {
    pub argv: Vec<OsString>,
    pub env: BTreeMap<OsString, OsString>,
    pub stdin_pipe: bool,
    pub stdout: Option<PathBuf>,
    pub stderr: Option<PathBuf>,
    /// Original with-open descriptors, retained across spawn and error paths.
    pub stdout_file: Option<std::sync::Arc<fs::File>>,
    pub stderr_file: Option<std::sync::Arc<fs::File>>,
    pub new_session: bool,
}
/// OS boundaries only. No runtime estimator or candidate result is an oracle.
pub trait Runtime {
    fn environment(&self) -> BTreeMap<OsString, OsString>;
    fn spawn(&mut self, spec: &Spawn) -> Result<usize>;
    fn read_line(&mut self, id: usize, timeout: Option<Duration>) -> Result<Vec<u8>>;
    fn write_flush(&mut self, id: usize, bytes: &[u8]) -> Result<()>;
    fn poll(&mut self, id: usize) -> Result<Option<i32>>;
    fn signal(&mut self, id: usize, signal: i32) -> Result<()>;
    fn wait(&mut self, id: usize, timeout: Duration) -> Result<i32>;
    fn communicate(&mut self, id: usize, timeout: Duration) -> Result<Captured>;
    fn executable_is(&mut self, id: usize, path: &Path) -> Result<bool>;
    fn close(&mut self, id: usize) -> Result<()>;
    fn sleep(&mut self, time: Duration) -> Result<()>;
    fn monotonic(&mut self) -> Result<f64>;
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.to_owned().into())
    }
}
fn node(value: impl std::fmt::Display) -> Result<JsonValue> {
    Ok(JsonValue::parse(&value.to_string())?)
}
fn field(value: &JsonValue, key: &str) -> Result<JsonValue> {
    frames::field(value, key)
}
fn array(value: &JsonValue) -> Result<Vec<JsonValue>> {
    if let JsonView::Array(value) = value.view() {
        Ok(value)
    } else {
        Err("expected source array".into())
    }
}
fn write_json(path: &Path, value: &JsonValue) -> Result<()> {
    fs::write(path, format!("{}\n", json::pretty(value, 0)?))?;
    Ok(())
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
struct Namespace {
    prefix: String,
    path: PathBuf,
    logger: PathBuf,
    remove_logger: bool,
}
impl Namespace {
    fn new(stem: &str, remove_logger: bool) -> Result<Self> {
        // The suffix is allocated by the OS, then reserved with create_dir. We never
        // delete a namespace we did not successfully create.
        let unique = tempfile::Builder::new().prefix(stem).tempdir()?;
        let suffix = unique
            .path()
            .file_name()
            .ok_or("missing unique namespace")?
            .to_str()
            .ok_or("invalid namespace")?;
        let prefix = suffix.replace('.', "_");
        let path = PathBuf::from(format!("/dev/shm/msgq_{prefix}"));
        fs::create_dir(&path)?;
        Ok(Self {
            logger: PathBuf::from(format!("/tmp/logmessage{prefix}")),
            prefix,
            path,
            remove_logger,
        })
    }
}
impl Drop for Namespace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
        if self.remove_logger {
            let _ = fs::remove_file(&self.logger);
        }
    }
}
fn environment(
    runtime: &impl Runtime,
    root: &Path,
    namespace: &Namespace,
    mode: Option<&str>,
) -> BTreeMap<OsString, OsString> {
    let mut env = runtime.environment();
    env.insert("PARAMS_ROOT".into(), root.as_os_str().to_owned());
    env.insert("OPENPILOT_PREFIX".into(), namespace.prefix.clone().into());
    if let Some(mode) = mode {
        env.remove(std::ffi::OsStr::new("ZMQ"));
        env.remove(std::ffi::OsStr::new("CEREAL_FAKE"));
        env.insert("DEBUG".into(), "1".into());
        env.insert(
            "SIMULATION".into(),
            if mode == "cadence" { "0" } else { "1" }.into(),
        );
    }
    env
}
fn process(options: &Options, env: BTreeMap<OsString, OsString>, frames: bool) -> Spawn {
    let mut argv = vec![crate::timed_fixtures::lexical(&options.binary).into_os_string()];
    if frames {
        argv.extend(["--frames".into(), "1".into()]);
    }
    Spawn {
        argv,
        env,
        stdin_pipe: false,
        stdout: None,
        stderr: None,
        stdout_file: None,
        stderr_file: None,
        new_session: false,
    }
}
fn params_case(options: &Options, key: &str, runtime: &mut impl Runtime) -> Result<JsonValue> {
    let namespace = Namespace::new("lagdio_", true)?;
    let temporary = tempfile::Builder::new().prefix("lagd-io-").tempdir()?;
    let params = temporary.path().join(&namespace.prefix);
    fs::create_dir(&params)?;
    fs::create_dir(params.join(key))?;
    if key == "LiveDelay" {
        fs::write(
            params.join("CarParams"),
            frames::car_params("SYNTHETIC-LAGD", 0.2)?,
        )?;
    }
    let spec = process(
        options,
        environment(runtime, temporary.path(), &namespace, None),
        true,
    );
    let id = runtime.spawn(&spec)?;
    let result = (|| -> Result<JsonValue> {
        if key == "CarParams" {
            runtime.sleep(Duration::from_millis(200))?;
            if runtime.poll(id)?.is_none() {
                runtime.signal(id, 15)?;
            }
        }
        let captured = runtime.communicate(id, Duration::from_secs(3))?;
        let stdout = String::from_utf8(captured.stdout)?;
        let stderr = String::from_utf8(captured.stderr)?;
        let retained = params.join(key).is_dir();
        packet::object(&[
            ("key", JsonValue::text(key)),
            ("returncode", node(captured.code)?),
            ("directory_retained", node(retained)?),
            ("stdout", JsonValue::text(&stdout)),
            ("stderr", JsonValue::text(&stderr)),
            ("pass", node(captured.code == 0 && retained)?),
        ])
    })();
    let cleanup = runtime.close(id);
    let value = result?;
    cleanup?;
    Ok(value)
}
fn wait_signal(
    options: &Options,
    runtime: &mut impl Runtime,
    stdout: &mut dyn Write,
) -> Result<JsonValue> {
    let temporary = tempfile::Builder::new().prefix("lagd-stop-").tempdir()?;
    let namespace = Namespace::new("lagd_", false)?;
    let spec = process(
        options,
        environment(runtime, temporary.path(), &namespace, None),
        false,
    );
    let id = runtime.spawn(&spec)?;
    let result = (|| -> Result<JsonValue> {
        runtime.sleep(Duration::from_millis(150))?;
        require(
            runtime.poll(id)?.is_none(),
            "daemon exited before CarParams signal",
        )?;
        runtime.signal(id, 15)?;
        let captured = runtime.communicate(id, Duration::from_secs(3))?;
        require(
            captured.code == 0,
            "daemon failed while waiting for CarParams",
        )?;
        let row = packet::object(&[
            ("returncode", node(captured.code)?),
            (
                "stdout",
                JsonValue::text(&String::from_utf8(captured.stdout)?),
            ),
            (
                "stderr",
                JsonValue::text(&String::from_utf8(captured.stderr)?),
            ),
            ("pass", node(true)?),
        ])?;
        fs::write(&options.output, format!("{}\n", compact(&row)?))?;
        Ok(JsonValue::parse("null")?)
    })();
    let cleanup = runtime.close(id);
    let value = result?;
    cleanup?;
    writeln!(stdout, "PASS: signal while waiting for CarParams")?;
    stdout.flush()?;
    Ok(value)
}
fn daemon_scenario(
    options: &Options,
    mode: &str,
    runtime: &mut impl Runtime,
    stdout: &mut dyn Write,
) -> Result<JsonValue> {
    fs::create_dir_all(&options.output)?;
    let namespace = Namespace::new("lagd_", true)?;
    let temporary = tempfile::Builder::new().prefix("lagd-native-").tempdir()?;
    let params = temporary.path().join(&namespace.prefix);
    fs::create_dir(&params)?;
    let cp = frames::car_params("SYNTHETIC-LAGD", 0.2)?;
    let seed = if mode == "corrupt" {
        b"corrupt".to_vec()
    } else {
        frames::cached(0.4, 5, &JsonValue::text("estimated"))?
    };
    fs::write(params.join("LiveDelay"), &seed)?;
    fs::write(params.join("CarParamsPrevRoute"), &cp)?;
    let env = environment(runtime, temporary.path(), &namespace, Some(mode));
    let count = if mode == "learning" {
        1201
    } else if mode == "cadence" {
        11
    } else {
        6
    };
    let publications = (count - 1) / 5 + 1;
    let daemon_log = options.output.join("daemon.log");
    let peer_log = options.output.join("peer.log");
    // Preserve the original nested with-open ordering, including empty daemon.log
    // when the peer cannot be spawned or does not announce readiness.
    let daemon_file = std::sync::Arc::new(fs::File::create(&daemon_log)?);
    let peer_file = std::sync::Arc::new(fs::File::create(&peer_log)?);
    let peer_spec = Spawn {
        argv: vec![crate::timed_fixtures::lexical(
            options.peer.as_deref().ok_or("missing IPC peer")?,
        )
        .into_os_string()],
        env: env.clone(),
        stdin_pipe: true,
        stdout: None,
        stderr: Some(peer_log),
        stdout_file: None,
        stderr_file: Some(peer_file),
        new_session: true,
    };
    let peer = runtime.spawn(&peer_spec)?;
    let mut daemon = None;
    let result = (|| -> Result<JsonValue> {
        let ready = text(runtime.read_line(peer, None)?)?;
        require(
            ready.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
                == "ready",
            "peer did not announce readiness",
        )?;
        let mut spec = process(options, env, false);
        spec.argv
            .extend(["--frames".into(), publications.to_string().into()]);
        spec.stdout = Some(daemon_log.clone());
        spec.stderr = Some(daemon_log.clone());
        spec.stdout_file = Some(std::sync::Arc::clone(&daemon_file));
        spec.stderr_file = Some(std::sync::Arc::clone(&daemon_file));
        spec.new_session = true;
        let id = runtime.spawn(&spec)?;
        daemon = Some(id);
        runtime.sleep(Duration::from_millis(150))?;
        require(
            runtime.poll(id)?.is_none(),
            "daemon exited before CarParams",
        )?;
        require(
            !params.join("CarParams").exists(),
            "CarParams appeared before publication",
        )?;
        fs::write(params.join("CarParams"), &cp)?;
        runtime.sleep(Duration::from_millis(150))?;
        require(
            runtime.executable_is(id, &options.binary)?,
            "daemon executable identity changed",
        )?;
        let mut frames_input = Vec::new();
        let mut observed = Vec::new();
        let mut times = Vec::new();
        for index in 0..count {
            let mut raw = frames::messages(
                index as i64,
                if mode == "learning" {
                    "recovery"
                } else {
                    "normal"
                },
            )?;
            if mode == "cadence" && index % 5 != 0 {
                raw.remove(1);
            }
            raw.rotate_left(1);
            let wires = frames::array(
                &raw.iter()
                    .map(|wire| frames::json_bytes(wire))
                    .collect::<Result<Vec<_>>>()?,
            )?;
            frames_input.push(packet::object(&[("messages", wires.clone())])?);
            let request =
                packet::object(&[("messages", wires), ("receive", node(index % 5 == 0)?)])?;
            runtime.write_flush(peer, format!("{}\n", compact(&request)?).as_bytes())?;
            let row = json::parse_text(&text(
                runtime.read_line(peer, Some(Duration::from_secs(6)))?,
            )?)?;
            if index % 5 == 0 {
                let value = field(&row, "packet")?;
                require(
                    !matches!(value.view(), JsonView::Null),
                    "peer returned no publication",
                )?;
                let bytes = frames::bytes(&value)?;
                let value = packet::project_event(&bytes)?;
                let age =
                    runtime.monotonic()? - frames::number(&field(&value, "logMonoTime")?)? / 1e9;
                require(
                    (0.0..2.0).contains(&age),
                    "publication timestamp age is outside unchanged bound",
                )?;
                times.push(runtime.monotonic()?);
                observed.push(value);
                if [0, 5, 1200].contains(&index) {
                    fs::write(options.output.join(format!("frame-{index}.capnp")), bytes)?;
                }
            }
            if mode == "cadence" {
                runtime.sleep(Duration::from_millis(50))?;
            }
        }
        let returncode = runtime.wait(id, Duration::from_secs(3))?;
        require(returncode == 0, "daemon returned nonzero")?;
        let saved = packet::project_event(&fs::read(params.join("LiveDelay"))?)?;
        let persisted = observed
            .get(if mode == "learning" {
                observed.len() - 1
            } else {
                0
            })
            .ok_or("missing observations")?;
        require(
            json::equal(&saved, persisted),
            "persisted publication differs",
        )?;
        if mode == "cadence" {
            require(
                !lagd_check::truth(&field(&observed[0], "valid")?)
                    && observed[1..]
                        .iter()
                        .all(|row| field(row, "valid").is_ok_and(|v| lagd_check::truth(&v))),
                "cadence validity transition differs",
            )?;
            require(
                times
                    .windows(2)
                    .all(|pair| (0.20..=0.4).contains(&(pair[1] - pair[0]))),
                "publication cadence outside unchanged bounds",
            )?;
        } else {
            let config = packet::object(&[
                ("car", frames::json_bytes(&cp)?),
                ("saved", frames::json_bytes(&seed)?),
                ("previous", frames::json_bytes(&cp)?),
            ])?;
            let expected = array(&lagd_loop_reference::run(
                &frames::array(&frames_input)?,
                &config,
            )?)?
            .into_iter()
            .filter_map(|row| match field(&row, "publish") {
                Ok(v) if lagd_check::truth(&v) => Some(field(&row, "packet")),
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<Vec<_>>>()?;
            require(
                expected.len() == observed.len(),
                "publication cardinality differs",
            )?;
            let (absolute, relative) = lagd_check::cereal_budget(&options.root)?;
            for (index, (wire, value)) in expected.iter().zip(&observed).enumerate() {
                let expected = packet::project_event(&frames::bytes(wire)?)?;
                let JsonView::Object(entries) = value.view() else {
                    return Err("publication is not object".into());
                };
                let normalized = packet::object(
                    &entries
                        .into_iter()
                        .map(|(key, value)| {
                            let key = JsonValue::codepoints(key.to_vec())
                                .ok_or("invalid field key")?
                                .to_utf8()
                                .ok_or("surrogate field key")?;
                            let value = if key == "logMonoTime" {
                                node(123456789)?
                            } else {
                                value
                            };
                            Ok((key, value))
                        })
                        .collect::<Result<Vec<_>>>()?
                        .iter()
                        .map(|(key, value)| (key.as_str(), value.clone()))
                        .collect::<Vec<_>>(),
                )?;
                lagd_check::compare_values(
                    &expected,
                    &normalized,
                    absolute,
                    relative,
                    &format!("{mode}-publication-{index}"),
                )?;
            }
            if mode == "corrupt" {
                require(
                    fs::read_to_string(&daemon_log)?.contains("Failed to retrieve initial lag"),
                    "missing corrupt-cache log",
                )?;
            } else {
                let live = field(observed.last().ok_or("no final observation")?, "liveDelay")?;
                require(
                    frames::integer(&field(&live, "validBlocks")?)? > 5,
                    "learning did not add blocks",
                )?;
                require(
                    !lagd_check::truth(&field(observed.last().unwrap(), "valid")?),
                    "learning final validity differs",
                )?;
            }
        }
        write_json(
            &options.output.join("publications.json"),
            &frames::array(&observed)?,
        )?;
        let intervals = times
            .windows(2)
            .map(|v| node(format!("{:?}", v[1] - v[0])))
            .collect::<Result<Vec<_>>>()?;
        write_json(
            &options.output.join("result.json"),
            &packet::object(&[
                ("mode", JsonValue::text(mode)),
                ("input_frames", node(count)?),
                ("publications", node(observed.len())?),
                ("returncode", node(returncode)?),
                (
                    "native_executable",
                    python_fs::json_value(
                        crate::path_resolve::resolve_non_strict(&options.binary)?.as_os_str(),
                    ),
                ),
                ("carparams_wait", node(true)?),
                (
                    "persisted_frame",
                    node(if mode == "learning" { 1200 } else { 0 })?,
                ),
                ("intervals", frames::array(&intervals)?),
                ("pass", node(true)?),
            ])?,
        )?;
        Ok(JsonValue::parse("null")?)
    })();
    // Close in the source's daemon,peer order on success and every failure; the
    // native backend keeps exited leaders reserved until their group is cleared.
    let daemon_cleanup = daemon.map(|id| runtime.close(id)).transpose();
    let peer_cleanup = runtime.close(peer);
    let value = result?;
    daemon_cleanup?;
    peer_cleanup?;
    writeln!(stdout, "PASS: native {mode}")?;
    stdout.flush()?;
    Ok(value)
}
pub fn scenario_with(
    scenario: Scenario,
    options: &Options,
    runtime: &mut impl Runtime,
    stdout: &mut dyn Write,
) -> Result<JsonValue> {
    match scenario {
        Scenario::Learning => daemon_scenario(options, "learning", runtime, stdout),
        Scenario::Corrupt => daemon_scenario(options, "corrupt", runtime, stdout),
        Scenario::Cadence => daemon_scenario(options, "cadence", runtime, stdout),
        Scenario::WaitSignal => wait_signal(options, runtime, stdout),
        Scenario::ParamsCar => params_case(options, "CarParams", runtime),
        Scenario::ParamsDelay => params_case(options, "LiveDelay", runtime),
    }
}
pub fn run(kind: Kind, options: &Options, stdout: &mut dyn Write) -> Result<()> {
    lagd_reference::sources(&options.root)?;
    let mut runtime = NativeRuntime::new()?;
    match kind {
        Kind::Daemon => {
            fs::create_dir_all(&options.output)?;
            for (scenario, name) in [
                (Scenario::Learning, "learning"),
                (Scenario::Corrupt, "corrupt"),
                (Scenario::Cadence, "cadence"),
                (Scenario::WaitSignal, "wait-signal.json"),
            ] {
                let child = Options {
                    root: options.root.clone(),
                    binary: options.binary.clone(),
                    peer: options.peer.clone(),
                    output: options.output.join(name),
                };
                scenario_with(scenario, &child, &mut runtime, stdout)?;
            }
        }
        Kind::ParamsIo => {
            let mut rows = Vec::new();
            for scenario in [Scenario::ParamsCar, Scenario::ParamsDelay] {
                rows.push(scenario_with(scenario, options, &mut runtime, stdout)?);
            }
            write_json(&options.output, &frames::array(&rows)?)?;
            require(
                rows.iter()
                    .all(|row| field(row, "pass").is_ok_and(|v| lagd_check::truth(&v))),
                "Params directory behavior differs",
            )?;
            writeln!(stdout,"PASS: unreadable CarParams waits for signal; unreadable LiveDelay remains absent to source policy")?;
        }
    }
    Ok(())
}
pub fn cli(kind: Kind) -> i32 {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let mut stdout = std::io::stdout();
    let result = (|| -> Result<()> {
        let (name, description, names) = match kind {
            Kind::Daemon => (
                "daemon",
                "Native IPC/Params process checks using only synthetic private namespaces.",
                vec!["binary", "peer", "output"],
            ),
            Kind::ParamsIo => (
                "params_io",
                "Source read_file directory-as-empty behavior at native lagd Params boundaries.",
                vec!["binary", "output"],
            ),
        };
        let Some(values) = lagd_check::arguments(&args, name, description, &names, &mut stdout)?
        else {
            return Ok(());
        };
        let options = Options {
            root,
            binary: values[0].clone().into(),
            peer: matches!(kind, Kind::Daemon).then(|| values[1].clone().into()),
            output: values.last().ok_or("missing output")?.clone().into(),
        };
        run(kind, &options, &mut stdout)
    })();
    if let Err(error) = result {
        eprintln!("{error}");
        if error.is::<native::Interrupted>() {
            // run() has dropped its runtime/namespace owners before restoring
            // the original KeyboardInterrupt termination class.
            let _ = signal_hook::low_level::emulate_default_handler(signal_hook::consts::SIGINT);
            return 130;
        }
        if error.is::<crate::argparse_scan::ArgumentError>() {
            2
        } else {
            1
        }
    } else {
        0
    }
}
