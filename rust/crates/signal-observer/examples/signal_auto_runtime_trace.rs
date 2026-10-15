//! Real child replacement/cleanup, with inert children and no GPU ownership.
use openpilot_signal_observer::{
    auto::{self, Error, Owner, Runtime},
    auto_process::ProcessOwner,
    backend::{Backend, Request},
};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, atomic::AtomicBool},
    thread,
    time::{Duration, Instant},
};
fn command() -> Result<Command, Error> {
    let binary = env::current_exe()?;
    if cfg!(target_arch = "aarch64") {
        if let Ok(runner) = env::var("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER") {
            let mut words = runner.split_whitespace();
            let mut command = Command::new(words.next().ok_or("empty runner")?);
            command.args(words).arg(binary);
            return Ok(command);
        }
    }
    Ok(Command::new(binary))
}
struct Io {
    root: PathBuf,
    phase: usize,
    fault: bool,
    read_error: bool,
    children: Vec<u32>,
    events: Vec<Value>,
}
impl Runtime for Io {
    type Child = ProcessOwner;
    fn selection(&mut self) -> Result<Request, Error> {
        Ok(Request::Auto)
    }
    fn requested(&mut self) -> Result<bool, Error> {
        Ok(self.phase < 3)
    }
    fn neural_requested(&mut self) -> Result<bool, Error> {
        Ok(true)
    }
    fn automatic_backend(&mut self) -> Result<Backend, Error> {
        if self.phase == 1 && self.read_error {
            return Err("source params read failed".into());
        }
        Ok(if self.phase == 1 && !self.fault {
            Backend::Qcom
        } else {
            Backend::TrackingCpu
        })
    }
    fn start(&mut self, backend: Backend) -> Result<ProcessOwner, Error> {
        // A backend label drives replacement only. These children neither open
        // GPU nodes nor load any network; QCOM inference is a separate gate.
        let ready = self.root.join(format!("ready-{}", self.children.len()));
        let mode = if self.fault {
            "exit"
        } else if self.children.is_empty() {
            "stubborn"
        } else {
            "normal"
        };
        let mut child = ProcessOwner::spawn(command()?.arg("child").arg(mode).arg(&ready))?;
        self.children.push(child.pid());
        let deadline = Instant::now() + Duration::from_secs(30);
        while !ready.exists() {
            if Instant::now() > deadline {
                child.kill()?;
                child.join(Duration::from_secs(2))?;
                return Err("inert child startup timed out".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
        if self.fault {
            while child.is_alive()? {
                if Instant::now() > deadline {
                    return Err("exit child did not complete".into());
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
        self.events.push(json!(["spawn", backend, child.pid()]));
        Ok(child)
    }
    fn event(&mut self, backend: Backend, pid: u32) -> Result<(), Error> {
        self.events.push(json!(["event", backend, pid]));
        Ok(())
    }
    fn now(&mut self) -> f64 {
        self.phase as f64 * 0.1
    }
    fn wait(&mut self, _: Duration) {
        self.phase += 1;
    }
    fn interrupted(&self) -> bool {
        false
    }
}
fn child(mode: &str, ready: &Path) -> Result<(), Error> {
    let stop = Arc::new(AtomicBool::new(false));
    if mode == "stubborn" {
        signal_hook::flag::register(signal_hook::consts::SIGTERM, stop)?;
    }
    fs::write(ready, b"ready")?;
    if mode == "exit" {
        std::process::exit(7);
    }
    loop {
        thread::sleep(Duration::from_millis(50));
    }
}
fn main() -> Result<(), Error> {
    let mut args = env::args().skip(1);
    if args.next().as_deref() == Some("child") {
        return child(
            &args.next().ok_or("missing child mode")?,
            &PathBuf::from(args.next().ok_or("missing ready path")?),
        );
    }
    let root = env::temp_dir().join(format!("signal-auto-process-{}", std::process::id()));
    fs::create_dir(&root)?;
    for (name, fault, read_error) in [
        ("replace", false, false),
        ("exit", true, false),
        ("params", false, true),
    ] {
        let directory = root.join(name);
        fs::create_dir(&directory)?;
        let mut io = Io {
            root: directory,
            phase: 0,
            fault,
            read_error,
            children: vec![],
            events: vec![],
        };
        let began = Instant::now();
        let outcome = auto::run(&mut io, None);
        if fault {
            assert_eq!(
                outcome.unwrap_err().to_string(),
                "signal backend tracking_cpu exited: 7"
            );
        } else if read_error {
            assert_eq!(
                outcome.unwrap_err().to_string(),
                "source params read failed"
            );
        } else {
            outcome?;
            assert_eq!(io.children.len(), 3);
        }
        if !fault && began.elapsed() < Duration::from_secs(2) {
            return Err("stubborn child was not joined through escalation".into());
        }
        for pid in io.children {
            let pid = rustix::process::Pid::from_raw(i32::try_from(pid)?).ok_or("invalid PID")?;
            if rustix::process::test_kill_process(pid).is_ok() {
                return Err("old child survived supervisor cleanup".into());
            }
        }
    }
    fs::remove_dir_all(root)?;
    println!(
        "{}",
        json!({"replacement":true,"kill_join":true,"unexpected_exit":true,"params_failure_cleanup":true,"gpu_access":false})
    );
    Ok(())
}
