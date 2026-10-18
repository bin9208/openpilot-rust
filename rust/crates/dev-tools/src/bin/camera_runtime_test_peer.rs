//! Compiled IPC/process-boundary peer for the native camera QA tests.
use openpilot_dev_tools::{
    camera_runtime_check::{NativeRuntime, Runtime, SERVICES},
    qa_process::CommandSpec,
    seal::Result,
};
use openpilot_msgq::{Publisher, VisionLayout, VisionMetadata, VisionServer, VisionStream};
use std::{
    fs,
    io::BufRead,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn until(mut condition: impl FnMut() -> Result<bool>) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition()? {
        if Instant::now() >= deadline {
            return Err("native test peer deadline".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}
fn stopped(pid: u32) -> bool {
    match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
        Ok(text) => text
            .rsplit_once(')')
            .is_some_and(|(_, tail)| tail.trim_start().starts_with('Z')),
        _ => false,
    }
}
fn process_spec(root: &Path, mode: &str) -> Result<CommandSpec> {
    let mut spec = CommandSpec::new_os(
        vec![
            std::env::current_exe()?.into_os_string(),
            mode.into(),
            root.as_os_str().to_owned(),
        ],
        Duration::MAX,
    );
    spec.os_env = Some(std::env::vars_os().collect());
    spec.inherit_stdin = true;
    Ok(spec)
}
fn main_result() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let mode = args
        .first()
        .and_then(|v| v.to_str())
        .ok_or("missing peer mode")?;
    let root = Path::new(args.get(1).ok_or("missing peer directory")?);
    match mode {
        "worker" => loop {
            std::thread::sleep(Duration::from_secs(1));
        },
        "live-child" | "exited-child" => {
            let mut input = String::new();
            std::io::stdin().lock().read_line(&mut input)?;
            fs::write(root.join("input"), input)?;
            let worker = Command::new(std::env::current_exe()?)
                .arg("worker")
                .arg(root)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            fs::write(root.join("leader"), std::process::id().to_string())?;
            fs::write(root.join("descendant"), worker.id().to_string())?;
            if mode == "live-child" {
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        }
        "publish" => {
            let prefix = std::env::var("OPENPILOT_PREFIX")?;
            let mut publisher = Publisher::for_runtime(SERVICES[0], 250 * 1024)?;
            let server = VisionServer::new("camerad")?;
            let images = server.create_stream(
                VisionStream::WideRoad,
                1,
                VisionLayout {
                    width: 128,
                    height: 128,
                    stride: 128,
                    uv_offset: 128 * 128,
                    len: 128 * 128 * 3 / 2,
                },
            )?;
            images[0].write(0, &vec![64; 24576])?;
            server.start_listener()?;
            let context = zmq::Context::new();
            let logger = context.socket(zmq::PUSH)?;
            logger.set_linger(1000)?;
            logger.set_sndtimeo(1000)?;
            logger.connect(&format!("ipc:///tmp/logmessage{prefix}"))?;
            fs::write(root.join("ready"), "ready")?;
            until(|| Ok(root.join("go").exists()))?;
            publisher.send(&[17, 34, 51])?;
            images[0].publish(VisionMetadata {
                width: 128,
                height: 128,
                stride: 128,
                uv_offset: 16384,
                len: 24576,
                frame_id: 7,
                timestamp_sof: 100,
                timestamp_eof: 200,
                valid: true,
                received: true,
                index: 0,
                fd: -1,
            })?;
            logger.send(b"\x14{\"levelnum\":20}".as_slice(), 0)?;
            until(|| Ok(root.join("finish").exists()))?;
        }
        "backend-ipc" => {
            let prefix = format!("camera_runtime_{}", std::process::id());
            let namespace = Path::new("/dev/shm").join(format!("msgq_{prefix}"));
            let endpoint = Path::new("/tmp").join(format!("logmessage{prefix}"));
            let mut backend = NativeRuntime::new();
            backend.create_namespace(&namespace)?;
            backend.subscribe(SERVICES[0], false, 250 * 1024)?;
            backend.vision_new("camerad", 0, false)?;
            backend.logger_socket()?;
            backend.logger_linger_zero()?;
            backend.logger_bind(&format!("ipc://{}", endpoint.display()))?;
            fs::write(root.join("stdout"), "")?;
            fs::write(root.join("stderr"), "")?;
            backend.spawn(
                &process_spec(root, "publish")?,
                &root.join("stdout"),
                &root.join("stderr"),
            )?;
            until(|| Ok(root.join("ready").exists()))?;
            until(|| backend.vision_connect(0))?;
            // Join the actual publisher generation before its first message.
            assert!(backend.receive_message(0)?.is_none());
            fs::write(root.join("go"), "go")?;
            let mut message = None;
            let mut image = None;
            let mut log = None;
            until(|| {
                if message.is_none() {
                    message = backend.receive_message(0)?;
                }
                if image.is_none() {
                    image = backend.receive_image(0)?;
                }
                if log.is_none() {
                    log = backend.receive_log()?;
                }
                Ok(message.is_some() && image.is_some() && log.is_some())
            })?;
            fs::write(root.join("finish"), "finish")?;
            until(|| Ok(backend.poll()?.is_some()))?;
            backend.logger_close()?;
            backend.context_term()?;
            backend.remove_namespace(&namespace)?;
            backend.unlink_endpoint(&endpoint)?;
            drop(backend);
            println!(
                "{}",
                serde_json::json!({
                    "message":message.unwrap(),
                    "image":serde_json::from_str::<serde_json::Value>(&image.unwrap().to_json()?)?,
                    "log":log.unwrap(),"namespace_removed":!namespace.exists(),"endpoint_removed":!endpoint.exists(),
                })
            );
        }
        "backend-live-child"
        | "backend-exited-child"
        | "backend-live-child-wait"
        | "backend-exited-child-wait" => {
            let mut backend = NativeRuntime::new();
            fs::write(root.join("stdout"), "")?;
            fs::write(root.join("stderr"), "")?;
            let childmode = if mode.starts_with("backend-live-child") {
                "live-child"
            } else {
                "exited-child"
            };
            backend.spawn(
                &process_spec(root, childmode)?,
                &root.join("stdout"),
                &root.join("stderr"),
            )?;
            until(|| Ok(root.join("descendant").exists()))?;
            let child = fs::read_to_string(root.join("leader"))?.parse::<u32>()?;
            let descendant = fs::read_to_string(root.join("descendant"))?.parse::<u32>()?;
            if mode.starts_with("backend-live-child") {
                assert!(backend.poll()?.is_none());
            } else {
                until(|| Ok(backend.poll()? == Some(0)))?;
            }
            if mode.ends_with("-wait") {
                if mode.starts_with("backend-live-child") {
                    backend.kill()?;
                }
                backend.wait()?;
                // A reaped leader no longer reserves its numeric PID/group.
                assert!(backend.poll().is_err());
                assert!(backend.signal(15).is_err());
                assert!(backend.kill().is_err());
                assert!(backend.wait().is_err());
            }
            drop(backend);
            until(|| Ok(stopped(descendant)))?;
            println!(
                "{}",
                serde_json::json!({
                    "input":fs::read_to_string(root.join("input"))?,
                    "leader_reaped":!Path::new(&format!("/proc/{child}")).exists(),
                    "descendant_stopped":stopped(descendant),
                })
            );
        }
        "namespace-collision" => {
            let prefix = format!("camera_runtime_{}", std::process::id());
            let namespace = Path::new("/dev/shm").join(format!("msgq_{prefix}"));
            let endpoint = Path::new("/tmp").join(format!("logmessage{prefix}"));
            fs::create_dir(&namespace)?;
            fs::write(namespace.join("sentinel"), "unowned")?;
            fs::write(&endpoint, "unowned")?;
            let mut backend = NativeRuntime::new();
            assert!(backend.create_namespace(&namespace).is_err());
            drop(backend);
            let namespace_preserved = fs::read_to_string(namespace.join("sentinel"))? == "unowned";
            let endpoint_preserved = fs::read_to_string(&endpoint)? == "unowned";
            fs::remove_file(endpoint)?;
            fs::remove_dir_all(namespace)?;
            println!(
                "{}",
                serde_json::json!({"namespace_preserved":namespace_preserved,"endpoint_preserved":endpoint_preserved})
            );
        }
        _ => return Err("unknown native test peer mode".into()),
    }
    Ok(())
}
fn main() {
    if let Err(error) = main_result() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
