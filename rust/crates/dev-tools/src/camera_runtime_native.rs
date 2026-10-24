//! Safe native Msgq/VisionIPC/ZMQ and owned live-child boundary.
use super::{control, verify, Options, Runtime, SERVICES};
use crate::{
    argparse_scan::{scan_os, ArgumentError, OptionSpec},
    camera_lifecycle_check::values::*,
    camera_sensor_check::lexical,
    camera_state_check::{NativeSchemaDecoder, SchemaDecoder},
    qa_process::{CommandSpec, NativeRunner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use openpilot_msgq::{Subscriber, VisionClient, VisionStream};
use rustix::process::{
    kill_process, kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions,
};
use std::{
    ffi::OsString,
    fs,
    os::unix::process::{CommandExt, ExitStatusExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct OwnedChild {
    child: Child,
    pid: Pid,
    reaped: bool,
}
impl OwnedChild {
    fn poll(&self) -> Result<Option<i32>> {
        loop {
            match waitid(
                WaitId::Pid(self.pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(status) => {
                    return Ok(status.map(|status| {
                        status
                            .exit_status()
                            .unwrap_or_else(|| -status.terminating_signal().unwrap_or(1))
                    }))
                }
                Err(rustix::io::Errno::INTR) => continue,
                Err(cause) => return Err(cause.into()),
            }
        }
    }
    fn kill(&mut self) -> Result<()> {
        // The direct child remains unreaped, reserving this process-group ID.
        match kill_process_group(self.pid, Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(cause) => return Err(cause.into()),
        }
        Ok(())
    }
    fn wait(&mut self) -> Result<i32> {
        // Observe exit without releasing the leader's numeric process-group ID.
        // Descendants must be stopped before wait() permits that ID to be reused.
        while self.poll()?.is_none() {
            std::thread::sleep(Duration::from_millis(2));
        }
        self.kill()?;
        let status = self.child.wait()?;
        self.reaped = true;
        Ok(status
            .code()
            .unwrap_or_else(|| -status.signal().unwrap_or(1)))
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.kill();
            let _ = self.child.kill();
            let _ = self.child.wait();
            self.reaped = true;
        }
    }
}
/// CLI-owned backend. Prefix initialization occurs before opening IPC or ZMQ;
/// tests exercise it in separate processes, never concurrent environment mutation.
pub struct NativeRuntime {
    epoch: Instant,
    namespace: Option<PathBuf>,
    endpoint: Option<PathBuf>,
    subscribers: Vec<Subscriber>,
    vision: [Option<VisionClient>; 3],
    pixels: [Vec<u8>; 3],
    context: Option<zmq::Context>,
    logger: Option<zmq::Socket>,
    child: Option<OwnedChild>,
    schema: Option<tempfile::TempDir>,
}
impl NativeRuntime {
    pub fn new() -> Self {
        Self {
            epoch: Instant::now(),
            namespace: None,
            endpoint: None,
            subscribers: vec![],
            vision: std::array::from_fn(|_| None),
            pixels: std::array::from_fn(|_| vec![]),
            context: None,
            logger: None,
            child: None,
            schema: None,
        }
    }
    fn logger(&self) -> Result<&zmq::Socket> {
        self.logger
            .as_ref()
            .ok_or_else(|| "logger is not initialized".into())
    }
    fn child(&mut self) -> Result<&mut OwnedChild> {
        let child = self
            .child
            .as_mut()
            .ok_or("runtime child is not initialized")?;
        if child.reaped {
            return Err("runtime child already reaped".into());
        }
        Ok(child)
    }
}
impl Default for NativeRuntime {
    fn default() -> Self {
        Self::new()
    }
}
impl Drop for NativeRuntime {
    fn drop(&mut self) {
        // Retain the unreaped leader until its group is terminated. Drop its
        // descendants before releasing the IPC namespace they might still use.
        self.child.take();
        self.logger.take();
        self.context.take();
        self.vision = std::array::from_fn(|_| None);
        self.subscribers.clear();
        if let Some(path) = self.namespace.take() {
            let _ = fs::remove_dir_all(path);
        }
        if let Some(path) = self.endpoint.take() {
            let _ = fs::remove_file(path);
        }
    }
}
impl Runtime for NativeRuntime {
    fn pid(&self) -> u32 {
        std::process::id()
    }
    fn inherited_environment(&self) -> Vec<(OsString, OsString)> {
        std::env::vars_os().collect()
    }
    fn create_namespace(&mut self, path: &Path) -> Result<()> {
        let prefix = path
            .file_name()
            .and_then(|v| v.to_str())
            .and_then(|v| v.strip_prefix("msgq_"))
            .ok_or("invalid owned Msgq namespace")?;
        if path.parent() != Some(Path::new("/dev/shm"))
            || prefix.is_empty()
            || !prefix
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
        {
            return Err("invalid owned Msgq namespace".into());
        }
        // This CLI initializes its namespace before spawning threads/children.
        std::env::set_var("OPENPILOT_PREFIX", prefix);
        fs::create_dir(path)?;
        self.namespace = Some(path.to_owned());
        Ok(())
    }
    fn prepare_schema(&mut self, decoder: &mut dyn SchemaDecoder) -> Result<()> {
        let staged = stage_schema()?;
        self.schema = Some(staged);
        decoder.load(&self.schema.as_ref().unwrap().path().join("log.capnp"))
    }
    fn subscribe(&mut self, service: &str, conflate: bool, segment_size: usize) -> Result<()> {
        if self.subscribers.len() >= SERVICES.len() {
            return Err("too many runtime subscriptions".into());
        }
        self.subscribers
            .push(Subscriber::for_runtime(service, conflate, segment_size)?);
        Ok(())
    }
    fn vision_new(&mut self, name: &str, port: usize, conflate: bool) -> Result<()> {
        let stream = match port {
            0 => VisionStream::WideRoad,
            1 => VisionStream::Road,
            2 => VisionStream::Driver,
            _ => return Err("invalid camera port".into()),
        };
        self.vision[port] = Some(VisionClient::new(name, stream, conflate)?);
        Ok(())
    }
    fn logger_socket(&mut self) -> Result<()> {
        let context = zmq::Context::new();
        self.logger = Some(context.socket(zmq::PULL)?);
        self.context = Some(context);
        Ok(())
    }
    fn logger_linger_zero(&mut self) -> Result<()> {
        self.logger()?.set_linger(0)?;
        Ok(())
    }
    fn logger_bind(&mut self, endpoint: &str) -> Result<()> {
        self.logger()?.bind(endpoint)?;
        self.endpoint = Some(PathBuf::from(
            endpoint
                .strip_prefix("ipc://")
                .ok_or("logger must use IPC")?,
        ));
        Ok(())
    }
    fn spawn(&mut self, spec: &CommandSpec, stdout: &Path, stderr: &Path) -> Result<()> {
        if self.child.is_some() {
            return Err("runtime already owns a child".into());
        }
        let argv = spec.effective_argv();
        let mut command = Command::new(argv.first().ok_or("missing runtime executable")?);
        command
            .args(&argv[1..])
            .env_clear()
            .envs(spec.effective_env())
            .process_group(0)
            .stdout(Stdio::from(
                fs::OpenOptions::new().write(true).open(stdout)?,
            ))
            .stderr(Stdio::from(
                fs::OpenOptions::new().write(true).open(stderr)?,
            ))
            .stdin(Stdio::inherit());
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        for key in &spec.env_remove {
            command.env_remove(key);
        }
        let child = command.spawn()?;
        let pid = Pid::from_child(&child);
        self.child = Some(OwnedChild {
            child,
            pid,
            reaped: false,
        });
        Ok(())
    }
    fn poll(&mut self) -> Result<Option<i32>> {
        self.child()?.poll()
    }
    fn signal(&mut self, signal: i32) -> Result<()> {
        let signal = Signal::from_named_raw(signal).ok_or("unsupported camera stop signal")?;
        kill_process(self.child()?.pid, signal)?;
        Ok(())
    }
    fn kill(&mut self) -> Result<()> {
        self.child()?.kill()
    }
    fn wait(&mut self) -> Result<i32> {
        self.child()?.wait()
    }
    fn monotonic(&self) -> Duration {
        self.epoch.elapsed()
    }
    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration)
    }
    fn receive_message(&mut self, port: usize) -> Result<Option<Vec<u8>>> {
        Ok(self
            .subscribers
            .get_mut(port)
            .ok_or("uninitialized subscriber")?
            .receive(Duration::ZERO)?)
    }
    fn vision_connected(&self, port: usize) -> bool {
        self.vision
            .get(port)
            .and_then(Option::as_ref)
            .is_some_and(VisionClient::is_connected)
    }
    fn vision_connect(&mut self, port: usize) -> Result<bool> {
        Ok(self
            .vision
            .get_mut(port)
            .and_then(Option::as_mut)
            .ok_or("uninitialized camera client")?
            .connect()?)
    }
    fn receive_image(&mut self, port: usize) -> Result<Option<JsonValue>> {
        let client = self
            .vision
            .get_mut(port)
            .and_then(Option::as_mut)
            .ok_or("uninitialized camera client")?;
        let Some(frame) = client.receive(Duration::ZERO)? else {
            return Ok(None);
        };
        let metadata = *frame.metadata();
        let buffer_id = frame.descriptor()?.buffer_frame_id;
        let pixels = &mut self.pixels[port];
        pixels.resize(metadata.len, 0);
        frame.copy_into(pixels)?;
        let samples = [128, 1024, 8192]
            .iter()
            .map(|&at| {
                pixels
                    .get(at)
                    .copied()
                    .map(number)
                    .ok_or_else(|| error("IndexError", "index out of bounds on dimension 1"))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(object(&[
            ("frame_id".into(), number(metadata.frame_id)),
            ("buffer_frame_id".into(), number(buffer_id)),
            ("sof".into(), number(metadata.timestamp_sof)),
            ("eof".into(), number(metadata.timestamp_eof)),
            ("slot".into(), number(metadata.index)),
            ("width".into(), number(metadata.width)),
            ("height".into(), number(metadata.height)),
            ("stride".into(), number(metadata.stride)),
            ("length".into(), number(metadata.len)),
            ("samples".into(), array(&samples)?),
        ])?))
    }
    fn receive_log(&mut self) -> Result<Option<Vec<u8>>> {
        if self.logger()?.poll(zmq::POLLIN, 0)? == 0 {
            return Ok(None);
        }
        Ok(Some(self.logger()?.recv_bytes(0)?))
    }
    fn logger_close(&mut self) -> Result<()> {
        self.logger.take();
        Ok(())
    }
    fn context_term(&mut self) -> Result<()> {
        self.context.take();
        Ok(())
    }
    fn remove_namespace(&mut self, path: &Path) -> Result<()> {
        if self.namespace.as_deref() == Some(path) {
            fs::remove_dir_all(path)?;
            self.namespace = None;
        }
        Ok(())
    }
    fn unlink_endpoint(&mut self, path: &Path) -> Result<()> {
        if self.endpoint.as_deref() == Some(path) {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            self.endpoint = None;
        }
        Ok(())
    }
}
fn source_root() -> PathBuf {
    std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."))
}
fn stage_schema() -> Result<tempfile::TempDir> {
    let root = source_root();
    let stage = tempfile::tempdir()?;
    fs::create_dir(stage.path().join("include"))?;
    for name in ["log", "custom", "deprecated"] {
        fs::copy(
            root.join("openpilot/cereal").join(format!("{name}.capnp")),
            stage.path().join(format!("{name}.capnp")),
        )?;
    }
    fs::copy(
        root.join("opendbc_repo/opendbc/car/car.capnp"),
        stage.path().join("car.capnp"),
    )?;
    fs::copy(
        root.join("openpilot/cereal/include/c++.capnp"),
        stage.path().join("include/c++.capnp"),
    )?;
    Ok(stage)
}
pub fn cli() -> Result<()> {
    let parsed = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--fixture", true),
            OptionSpec::value("--oracle", true),
            OptionSpec::value("--output", true),
            OptionSpec::append("--case"),
            OptionSpec::flag("--verify-only", None),
            OptionSpec::value("--qemu", false),
            OptionSpec::value("--sysroot", false),
        ],
    )?;
    let cases = parsed.append_values.get("--case");
    for case in cases.into_iter().flatten() {
        if case.to_str().is_none_or(|v| super::ports(v).is_err()) {
            return Err(ArgumentError("invalid runtime case".into()).into());
        }
    }
    if parsed.help {
        println!("Check real camera runtime IPC, signal handling and cleanup");
        return Ok(());
    }
    let case = cases
        .and_then(|v| v.last())
        .and_then(|v| v.to_str())
        .ok_or_else(|| ArgumentError("--case required".into()))?
        .to_owned();
    let path = |key| lexical(Path::new(parsed.values.get(key).unwrap()));
    let options = Options {
        binary: path("--binary"),
        fixture: path("--fixture"),
        oracle: path("--oracle"),
        output: path("--output"),
        case,
        qemu: parsed.values.get("--qemu").map(|v| lexical(Path::new(v))),
        sysroot: parsed
            .values
            .get("--sysroot")
            .map(|v| lexical(Path::new(v))),
    };
    let program = std::env::var_os("OPENPILOT_CAPNP").unwrap_or_else(|| "capnp".into());
    let mut decoder = NativeSchemaDecoder::new(program.into());
    if parsed.values.contains_key("--verify-only") {
        let staged = stage_schema()?;
        decoder.load(&staged.path().join("log.capnp"))?;
        verify(
            &options,
            &mut NativeRunner,
            &mut decoder,
            &mut std::io::stdout().lock(),
        )
    } else {
        let mut runtime = NativeRuntime::new();
        control(&options, &mut runtime, &mut decoder)?;
        verify(
            &options,
            &mut NativeRunner,
            &mut decoder,
            &mut std::io::stdout().lock(),
        )
    }
}
