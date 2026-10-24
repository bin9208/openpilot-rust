//! Real IPC observations; the source gate reproduces subscriber invalidation,
//! rather than substituting a preselected publication result.
use super::{require, validate_observed, Bound, Result, CASES, MARKER};
use crate::stats_daemon_check::process::Process;
use openpilot_cereal::log_capnp::event;
use openpilot_msgq::Subscriber;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
const BIG: usize = 10 * 1024 * 1024;
fn require_alive(process: &Process<'_>, deadline: Instant) -> Result<()> {
    require(
        process.exited()?.is_none(),
        "collector exited during startup",
    )?;
    require(
        Instant::now() < deadline,
        "collector startup/readiness exceeded20s",
    )
}
fn channel(listener: &UnixListener, process: &Process<'_>, end: Instant) -> Result<UnixStream> {
    listener.set_nonblocking(true)?;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_read_timeout(Some(Duration::from_millis(10)))?;
                return Ok(stream);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => (),
            Err(error) => return Err(error.into()),
        }
        require_alive(process, end)?;
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn byte(stream: &mut UnixStream, process: &Process<'_>, end: Instant, expected: u8) -> Result<()> {
    let mut bytes = [0];
    loop {
        match stream.read(&mut bytes) {
            Ok(0) => return Err("source initialization gate closed".into()),
            Ok(_) => {
                return require(
                    bytes[0] == expected,
                    "source initialization gate acknowledgement differs",
                )
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error.into()),
        }
        require_alive(process, end)?;
    }
}
fn decode(packet: &[u8], error: bool) -> Result<String> {
    let message = capnp::serialize::read_message(Cursor::new(packet), Default::default())?;
    let event = message.get_root::<event::Reader<'_>>()?;
    require(event.get_valid(), "startup event invalid")?;
    let text = match (error, event.which()?) {
        (false, event::LogMessage(text)) | (true, event::ErrorLogMessage(text)) => text?,
        _ => return Err("startup event has wrong union".into()),
    };
    Ok(text.to_str()?.to_owned())
}
fn synchronize(
    process: &Process<'_>,
    socket: &zmq::Socket,
    log: &mut Subscriber,
    error: &mut Subscriber,
    prefix: &str,
    output: &Path,
) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(20);
    let mut sequence = 0_u64;
    while Instant::now() < end {
        require_alive(process, end)?;
        sequence += 1;
        let marker = format!("{{\"msg\": \"ready-{prefix}-{sequence}\"}}");
        socket.send_multipart([&[10_u8][..], marker.as_bytes()], 0)?;
        let retry = Instant::now() + Duration::from_millis(50);
        while Instant::now() < retry {
            if let Some(packet) = log.receive(Duration::ZERO)? {
                OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(output.join("readiness.bin"))?
                    .write_all(&packet)?;
                if decode(&packet, false)? != marker {
                    continue;
                }
                // receive refreshes a reader invalidated by publisher creation.
                require(
                    error.receive(Duration::ZERO)?.is_none(),
                    "unexpected readiness error publication",
                )?;
                fs::write(
                    output.join("readiness.json"),
                    serde_json::to_vec(&json!({"acknowledged_marker":marker,"attempts":sequence}))?,
                )?;
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    Err("collector did not acknowledge readiness within20s".into())
}
fn invocation(
    argv: &[OsString],
    home: &Path,
    prefix: &str,
    hash: Option<&str>,
    path: &Path,
) -> Result<()> {
    let argv = argv
        .iter()
        .map(|v| crate::python_fs::json_string(v))
        .collect::<Result<Vec<_>>>()?
        .join(",");
    fs::write(
        path,
        format!(
            "{{\"argv\":[{argv}],\"HOME\":{},\"OPENPILOT_PREFIX\":{},\"binary_sha256\":{}}}",
            crate::python_fs::json_string(home.as_os_str())?,
            serde_json::to_string(prefix)?,
            serde_json::to_string(&hash)?
        ),
    )?;
    Ok(())
}
pub(super) fn source(args: &[OsString]) -> Result<()> {
    require(
        args.len() == 3,
        "source collector requires endpoint, log root and gate",
    )?;
    crate::managed_entry_collector_source::run_gated(
        args[0].to_str().ok_or("endpoint encoding")?,
        Path::new(&args[1]),
        Path::new(&args[2]),
    )
}
pub(super) fn run(args: &[OsString]) -> Result<()> {
    require(
        args.len() == 4,
        "startup worker requires case, collector, output and gate home",
    )?;
    let case = args[0].to_str().ok_or("startup case encoding")?;
    require(CASES.contains(&case), "unknown startup case")?;
    let original = case != "ready-native";
    let ready = case != "legacy-source";
    let output = Path::new(&args[2]);
    let home = output.join("collector/home");
    let prefix = std::env::var("OPENPILOT_PREFIX")?;
    let endpoint = format!("ipc:///tmp/logmessage{prefix}");
    let root = home.join(format!(".comma{prefix}/log"));
    fs::create_dir_all(&root)?;
    let gate_path = Path::new(&args[3]).join("publisher");
    let listener = if original {
        Some(UnixListener::bind(&gate_path)?)
    } else {
        None
    };
    let current = std::env::current_exe()?;
    let argv = if original {
        vec![
            current.into_os_string(),
            "--startup-source".into(),
            endpoint.clone().into(),
            root.clone().into_os_string(),
            gate_path.into_os_string(),
        ]
    } else {
        vec![args[1].clone()]
    };
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .env("HOME", &home)
        .env_remove("STARTUP_ENDPOINT_OWNER")
        .stdout(Stdio::from(File::create(
            output.join("collector/stdout.log"),
        )?))
        .stderr(Stdio::from(File::create(
            output.join("collector/stderr.log"),
        )?));
    // Admission occurs at the source spawn phase, after log files exist. The
    // source cases do not require the unused candidate path to exist.
    let artifact = if original {
        None
    } else {
        Some(Bound::open(Path::new(&args[1]))?)
    };
    let cancel = AtomicBool::new(false);
    let process = Process::spawn_logged(&mut command, &cancel, false)?;
    let observation = (|| -> Result<Value> {
        let start = Instant::now() + Duration::from_secs(20);
        let mut gate = listener
            .as_ref()
            .map(|v| channel(v, &process, start))
            .transpose()?;
        if let Some(stream) = &mut gate {
            byte(stream, &process, start, b'R')?;
        }
        let shm = PathBuf::from(format!("/dev/shm/msgq_{prefix}"));
        while !["logMessage", "errorLogMessage"]
            .iter()
            .all(|name| shm.join(name).exists())
        {
            require_alive(&process, start)?;
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut capacities = BTreeMap::new();
        for name in ["logMessage", "errorLogMessage"] {
            let size = fs::metadata(shm.join(name))?.len();
            require(size > BIG as u64, "collector queue capacity reduced")?;
            capacities.insert(name, size);
        }
        let mut log = Subscriber::for_runtime("logMessage", false, BIG)?;
        let mut error = Subscriber::for_runtime("errorLogMessage", false, BIG)?;
        let context = zmq::Context::new();
        let socket = context.socket(zmq::PUSH)?;
        socket.set_linger(0)?;
        socket.set_sndtimeo(10000)?;
        socket.connect(&endpoint)?;
        let auth = std::env::var_os("STARTUP_ENDPOINT_OWNER")
            .ok_or("endpoint ownership channel absent")?;
        crate::qa_endpoint::acknowledge(
            Path::new(endpoint.strip_prefix("ipc://").ok_or("endpoint URI")?),
            Path::new(&auth),
            &prefix,
            &cancel,
        )?;
        invocation(
            &argv,
            &home,
            &prefix,
            artifact.as_ref().map(|v| v.hash.as_str()),
            &output.join("collector/invocation.json"),
        )?;
        fs::write(
            output.join("process-ready.json"),
            serde_json::to_vec(
                &json!({"worker":std::process::id(),"collector":process.pid().as_raw_pid(),"prefix":prefix}),
            )?,
        )?;
        fs::remove_file(output.join("publisher.gate"))?;
        if let Some(stream) = &mut gate {
            stream.set_write_timeout(Some(Duration::from_secs(10)))?;
            stream.write_all(b"1")?;
            byte(stream, &process, start, b'1')?;
        }
        drop(gate);
        drop(listener);
        if ready {
            synchronize(
                &process,
                &socket,
                &mut log,
                &mut error,
                &prefix,
                &output.join("collector"),
            )?;
        }
        socket.send_multipart([&[40_u8][..], MARKER.as_bytes()], 0)?;
        std::thread::sleep(Duration::from_millis(200));
        let mut observed = json!({"logMessage":[],"errorLogMessage":[]});
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            for (name, subscriber, is_error) in [
                ("logMessage", &mut log, false),
                ("errorLogMessage", &mut error, true),
            ] {
                if let Some(packet) = subscriber.receive(Duration::ZERO)? {
                    OpenOptions::new()
                        .append(true)
                        .create(true)
                        .open(output.join(format!("{name}.bin")))?
                        .write_all(&packet)?;
                    observed[name]
                        .as_array_mut()
                        .ok_or("observation array")?
                        .push(json!(decode(&packet, is_error)?));
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        fs::write(
            output.join("observed.json"),
            serde_json::to_vec_pretty(&observed)?,
        )?;
        let mut disk = vec![];
        for file in fs::read_dir(&root)? {
            let file = file?;
            if file.file_name().as_encoded_bytes().starts_with(b"swaglog.") {
                for line in fs::read_to_string(file.path())?.lines() {
                    disk.push(serde_json::from_str::<Value>(line)?);
                }
            }
        }
        let counts = validate_observed(case, &observed, &disk)?;
        Ok(
            json!({"counts":counts,"queue_file_bytes":capacities,"collector_sha256":artifact.as_ref().map(|v|&v.hash)}),
        )
    })();
    let stop = Instant::now();
    let shutdown = (|| -> Result<i32> {
        if process.exited()?.is_none() {
            process.signal(rustix::process::Signal::INT)?;
        }
        process.wait(Some(Duration::from_secs(10)))
    })();
    if let Ok(code) = &shutdown {
        fs::write(
            output.join("collector/exit.json"),
            serde_json::to_vec(
                &json!({"code":code,"shutdown_seconds":stop.elapsed().as_secs_f64(),"signal":2}),
            )?,
        )?;
    }
    if let Some(artifact) = &artifact {
        artifact.verify()?;
    }
    drop(process);
    shutdown?;
    println!("{}", serde_json::to_string(&observation?)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wire_validity_and_exact_union_are_required_before_text_comparison() {
        let mut message = capnp::message::Builder::new_default();
        {
            let mut value = message.init_root::<event::Builder<'_>>();
            value.set_valid(true);
            value.set_log_message("marker");
        }
        let packet = capnp::serialize::write_message_to_words(&message);
        assert_eq!(decode(&packet, false).unwrap(), "marker");
        assert!(decode(&packet, true).is_err());
        {
            let mut value = message.get_root::<event::Builder<'_>>().unwrap();
            value.set_valid(false);
        }
        assert!(decode(&capnp::serialize::write_message_to_words(&message), false).is_err());
        assert!(decode(b"not a capnp packet", false).is_err());
    }
}
