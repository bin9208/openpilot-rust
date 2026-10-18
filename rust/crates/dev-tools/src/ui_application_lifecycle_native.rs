//! Native mechanical endpoints for the independent application checker.
use super::{Backend, Options};
use crate::{
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
    ui_application_lifecycle_raw::Raw,
    ui_forms_input::{hex, Canvas},
};
use std::{
    ffi::OsStr,
    fs,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct OwnedChild {
    child: Child,
    status: Option<i32>,
}
fn code(status: std::process::ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| -status.signal().unwrap_or(1))
}
impl OwnedChild {
    fn poll(&mut self) -> Result<Option<i32>> {
        if self.status.is_none() {
            if let Some(status) = self.child.try_wait()? {
                self.status = Some(code(status));
            }
        }
        Ok(self.status)
    }
    fn kill(&mut self) -> Result<()> {
        if self.poll()?.is_none() {
            self.child.kill()?;
        }
        Ok(())
    }
    fn wait(&mut self, timeout: Option<Duration>) -> Result<Option<i32>> {
        if let Some(status) = self.poll()? {
            return Ok(Some(status));
        }
        let Some(timeout) = timeout else {
            self.status = Some(code(self.child.wait()?));
            return Ok(self.status);
        };
        let started = Instant::now();
        loop {
            if let Some(status) = self.poll()? {
                return Ok(Some(status));
            }
            if started.elapsed() >= timeout {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.status.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
pub struct NativeBackend {
    helper: PathBuf,
    library: PathBuf,
    raw: Option<Raw>,
    child: Option<OwnedChild>,
    socket: Option<zmq::Socket>,
    context: Option<zmq::Context>,
    epoch: Instant,
}
impl NativeBackend {
    pub fn new(helper: &Path, library: &Path) -> Self {
        Self {
            helper: helper.into(),
            library: library.into(),
            raw: None,
            child: None,
            socket: None,
            context: None,
            epoch: Instant::now(),
        }
    }
    fn raw(&mut self) -> Result<&mut Raw> {
        self.raw
            .as_mut()
            .ok_or("application X11 helper not open".into())
    }
    fn child(&mut self) -> Result<&mut OwnedChild> {
        self.child
            .as_mut()
            .ok_or("application child not started".into())
    }
}
impl Runner for NativeBackend {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
        NativeRunner.run(command)
    }
    fn run_to_file(&mut self, command: &CommandSpec, log: &Path) -> Result<Captured> {
        NativeRunner.run_to_file(command, log)
    }
}
impl Backend for NativeBackend {
    fn bind_logs(&mut self, address: &str) -> Result<()> {
        if self.socket.is_some() {
            return Err("application log receiver already bound".into());
        }
        let context = zmq::Context::new();
        let socket = context.socket(zmq::PULL)?;
        socket.bind(address)?;
        self.socket = Some(socket);
        self.context = Some(context);
        Ok(())
    }
    fn log_packet(&mut self) -> Result<Option<Vec<u8>>> {
        let socket = self
            .socket
            .as_ref()
            .ok_or("application log receiver not open")?;
        Ok(if socket.poll(zmq::POLLIN, 100)? != 0 {
            Some(socket.recv_bytes(0)?)
        } else {
            None
        })
    }
    fn close_logs(&mut self) -> Result<()> {
        drop(self.socket.take());
        drop(self.context.take());
        Ok(())
    }
    fn open_display(&mut self, display: &OsStr) -> Result<()> {
        if self.raw.is_some() {
            return Err("application display already open".into());
        }
        let mut raw = Raw::open(&self.helper, &self.library)?;
        raw.call(&format!(
            "x11-open {}",
            hex(display.to_str().ok_or("application display encoding")?)
        ))?;
        self.raw = Some(raw);
        Ok(())
    }
    fn window(&mut self, title: &str) -> Result<Option<u64>> {
        let id = self
            .raw()?
            .request(&format!("x11-window {}", hex(title)))?
            .parse()?;
        Ok((id != 0).then_some(id))
    }
    fn focus(&mut self, window: u64) -> Result<()> {
        self.raw()?.call(&format!("x11-focus {window}"))
    }
    fn sync(&mut self) -> Result<()> {
        self.raw()?.call("x11-sync")
    }
    fn key(&mut self, ch: char, down: bool) -> Result<()> {
        self.raw()?.call(&format!(
            "x11-key {} {}",
            hex(&ch.to_string()),
            u8::from(down)
        ))
    }
    fn motion(&mut self, window: u64, x: i32, y: i32) -> Result<()> {
        self.raw()?.call(&format!("x11-motion {window} {x} {y}"))
    }
    fn button(&mut self, down: bool) -> Result<()> {
        self.raw()?.call(&format!("x11-button {}", u8::from(down)))
    }
    fn close_display(&mut self) -> Result<()> {
        self.raw()?.call("x11-close")
    }
    fn protocol_errors(&mut self) -> Result<bool> {
        let errors = self.raw()?.request("x11-errors")?.parse::<u64>()?;
        self.raw()?.close()?;
        self.raw = None;
        Ok(errors != 0)
    }
    fn start_live(&mut self, spec: &CommandSpec, log: &Path) -> Result<u32> {
        if let Some(child) = &mut self.child {
            if child.poll()?.is_none() {
                return Err("application child still owned".into());
            }
        }
        if spec.input.is_some() {
            return Err("application live child requires inherited or null stdin".into());
        }
        let argv = spec.effective_argv();
        let (first, args) = argv
            .split_first()
            .ok_or("application child command empty")?;
        let file = fs::File::create(log)?;
        let mut command = Command::new(first);
        command
            .args(args)
            .envs(spec.effective_env())
            .stdin(if spec.inherit_stdin {
                Stdio::inherit()
            } else {
                Stdio::null()
            })
            .stdout(file.try_clone()?)
            .stderr(file);
        for key in &spec.env_remove {
            command.env_remove(key);
        }
        if let Some(path) = &spec.cwd {
            command.current_dir(path);
        }
        let child = command.spawn()?;
        let pid = child.id();
        self.child = Some(OwnedChild {
            child,
            status: None,
        });
        Ok(pid)
    }
    fn poll_live(&mut self) -> Result<Option<i32>> {
        self.child()?.poll()
    }
    fn wait_live(&mut self, timeout: Option<Duration>) -> Result<Option<i32>> {
        self.child()?.wait(timeout)
    }
    fn kill_live(&mut self) -> Result<()> {
        self.child()?.kill()
    }
    fn identity(&mut self, pid: u32) -> Result<(PathBuf, String)> {
        Ok((
            fs::canonicalize(format!("/proc/{pid}/exe"))?,
            fs::read_to_string(format!("/proc/{pid}/maps"))?,
        ))
    }
    fn now(&mut self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }
    fn sleep(&mut self, seconds: f64) {
        std::thread::sleep(Duration::from_secs_f64(seconds));
    }
}
impl Drop for NativeBackend {
    fn drop(&mut self) {
        drop(self.child.take());
        drop(self.raw.take());
        drop(self.socket.take());
        drop(self.context.take());
    }
}
pub fn cli() -> Result<()> {
    use crate::argparse_scan::{scan_os, OptionSpec};
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--display", true),
        ],
    )?;
    if args.help {
        println!("usage: check-ui-application --binary BINARY --output OUTPUT --display DISPLAY");
        return Ok(());
    }
    let root = crate::ui_diagnostic_reference::source_root()?;
    let current = std::env::current_exe()?;
    let parent = current
        .parent()
        .ok_or("application executable parent missing")?;
    let reference = std::env::var_os("UI_APPLICATION_REFERENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| parent.join("ui-application-reference"));
    let helper = std::env::var_os("UI_APPLICATION_RAW_HELPER")
        .map(PathBuf::from)
        .unwrap_or_else(|| parent.join("ui-application-raw"));
    let library = std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("STARTUP_UI_RAYLIB_ROOT")
                .map(|v| PathBuf::from(v).join("lib/libopenpilot-raylib.so"))
        })
        .unwrap_or_else(|| "libopenpilot-raylib.so".into());
    let options = Options {
        root,
        binary: PathBuf::from(&args.values["--binary"]),
        reference,
        output: PathBuf::from(&args.values["--output"]),
        display: args.values["--display"].clone(),
        pid: std::process::id(),
    };
    super::check(
        &options,
        &mut NativeBackend::new(&helper, &library),
        &mut std::io::stdout(),
    )
}
