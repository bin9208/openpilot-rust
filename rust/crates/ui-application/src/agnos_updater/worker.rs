use openpilot_process_supervision::{CapturedChild, CapturedCommand};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    process::{Pid, Signal, WaitId, WaitIdOptions, WaitIdStatus},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    Tici,
    Mici,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    pub layout: Layout,
    pub progress: u8,
    pub text: String,
    pub failure: Option<String>,
    pub running: bool,
    pub reboot_requested: bool,
    pub logs: VecDeque<String>,
}
impl State {
    pub fn new(layout: Layout) -> Self {
        Self {
            layout,
            progress: 0,
            text: if layout == Layout::Tici {
                "Loading..."
            } else {
                "loading"
            }
            .into(),
            failure: None,
            running: false,
            reboot_requested: false,
            logs: VecDeque::new(),
        }
    }
    pub fn line(&mut self, line: &str) {
        let text = line.trim_matches(openpilot_startup_ui::text::whitespace);
        if text.is_empty() {
            return;
        }
        println!("{text}");
        self.logs.push_back(
            text.chars()
                .rev()
                .take(240)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect(),
        );
        while self.logs.len() > 12 {
            self.logs.pop_front();
        }
        let Some((label, percent)) = text.rsplit_once(':') else {
            return;
        };
        let percent = percent.trim_start_matches(openpilot_startup_ui::text::whitespace);
        if label.is_empty() || !(1..=3).contains(&percent.chars().count()) {
            return;
        }
        let number = percent.chars().try_fold(0_u16, |value, ch| {
            openpilot_startup_ui::decimal_digit(ch).map(|digit| value * 10 + u16::from(digit))
        });
        let Some(number) = number else { return };
        self.progress = u8::try_from(number.min(100)).unwrap_or(100);
        self.text = match self.layout {
            Layout::Tici => label.into(),
            Layout::Mici => label.to_lowercase(),
        };
    }
    fn begin(&mut self) {
        self.progress = 0;
        self.text = if self.layout == Layout::Tici {
            "Starting update..."
        } else {
            "starting update"
        }
        .into();
        self.failure = None;
        self.running = true;
        self.reboot_requested = false;
        self.logs.clear();
    }
    fn complete(&mut self, code: i32) {
        self.running = false;
        if code == 0 {
            self.progress = 100;
            self.text = if self.layout == Layout::Tici {
                "Rebooting..."
            } else {
                "rebooting"
            }
            .into();
            self.reboot_requested = true;
        } else {
            let tail = self.logs.back().map_or("no error output", String::as_str);
            self.failure = Some(format!(
                "{} exited with code {code}: {tail}",
                if self.layout == Layout::Tici {
                    "Updater"
                } else {
                    "updater"
                }
            ));
            if self.layout == Layout::Tici {
                self.text = "Update failed".into();
            }
        }
    }
    fn fail_start(&mut self, error: &str) {
        self.running = false;
        self.failure = Some(format!(
            "{} to start updater: {error}",
            if self.layout == Layout::Tici {
                "Unable"
            } else {
                "unable"
            }
        ));
        if self.layout == Layout::Tici {
            self.text = "Update failed".into();
        }
    }
}

#[derive(Clone)]
pub struct Options {
    pub updater: PathBuf,
    pub manifest: PathBuf,
    pub launcher: PathBuf,
    pub cwd: PathBuf,
    pub layout: Layout,
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Process(#[from] openpilot_process_supervision::Error),
    #[error("updater worker state poisoned")]
    Poisoned,
    #[error("updater worker panicked")]
    Panicked,
    #[error("updater PID out of range")]
    Pid,
}

pub struct Worker {
    pub options: Options,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(), Error>>>,
}
impl Worker {
    pub fn new(options: Options) -> Self {
        Self {
            state: Arc::new(Mutex::new(State::new(options.layout))),
            options,
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
        }
    }
    pub fn snapshot(&self) -> Result<State, Error> {
        Ok(self.state.lock().map_err(|_| Error::Poisoned)?.clone())
    }
    pub fn take_reboot(&self) -> Result<bool, Error> {
        Ok(std::mem::take(
            &mut self
                .state
                .lock()
                .map_err(|_| Error::Poisoned)?
                .reboot_requested,
        ))
    }
    pub fn start(&mut self) -> Result<bool, Error> {
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
        {
            return Ok(false);
        }
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| Error::Panicked)??;
        }
        self.stop.store(false, Ordering::SeqCst);
        self.state.lock().map_err(|_| Error::Poisoned)?.begin();
        let options = self.options.clone();
        let state = self.state.clone();
        let stop = self.stop.clone();
        self.worker = Some(
            thread::Builder::new()
                .name("agnos-updater".into())
                .spawn(move || run(options, &state, &stop))?,
        );
        Ok(true)
    }
    pub fn close(&mut self) -> Result<(), Error> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| Error::Panicked)??;
        }
        Ok(())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("updater worker close: {error}");
        }
    }
}

struct Process {
    child: Option<CapturedChild>,
    pid: Pid,
}
impl Process {
    fn observe(&self) -> Result<Option<WaitIdStatus>, Error> {
        loop {
            match rustix::process::waitid(
                WaitId::Pid(self.pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(status) => return Ok(status),
                Err(rustix::io::Errno::INTR) => {}
                Err(error) => return Err(std::io::Error::from(error).into()),
            }
        }
    }
    fn signal(&self, signal: Signal) -> Result<(), Error> {
        match rustix::process::kill_process_group(self.pid, signal) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(std::io::Error::from(error).into()),
        }
    }
    fn terminate(&mut self) -> Result<(), Error> {
        self.signal(Signal::TERM)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.observe()?.is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        self.reap()
    }
    fn reap(&mut self) -> Result<(), Error> {
        if self.child.is_some() {
            self.signal(Signal::KILL)?;
        }
        if let Some(child) = &mut self.child {
            child.process.wait()?;
            self.child.take();
        }
        Ok(())
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if let Err(error) = self.reap() {
            eprintln!("updater child reap: {error}");
        }
    }
}

fn run(options: Options, state: &Mutex<State>, stop: &AtomicBool) -> Result<(), Error> {
    let request = CapturedCommand {
        launcher: options.launcher,
        cwd: options.cwd,
        argv: vec![
            options.updater.clone().into_os_string(),
            "--swap".into(),
            "--retry-network".into(),
            options.manifest.into_os_string(),
        ],
    };
    let launched = request.spawn_session_merged_with_env(&[]);
    let (child, pipe) = match launched {
        Ok(child) => child,
        Err(error) => {
            let detail = match error {
                openpilot_process_supervision::Error::Io(error) => format!(
                    "[Errno {}] {}: '{}'",
                    error.raw_os_error().unwrap_or(0),
                    error
                        .to_string()
                        .split(" (os error")
                        .next()
                        .unwrap_or("I/O error"),
                    options.updater.display()
                ),
                error => error.to_string(),
            };
            state
                .lock()
                .map_err(|_| Error::Poisoned)?
                .fail_start(&detail);
            return Ok(());
        }
    };
    let pid = Pid::from_raw(i32::try_from(child.process.id()).map_err(|_| Error::Pid)?)
        .ok_or(Error::Pid)?;
    let mut process = Process {
        child: Some(child),
        pid,
    };
    let flags = rustix::fs::fcntl_getfl(&pipe).map_err(std::io::Error::from)?;
    rustix::fs::fcntl_setfl(&pipe, flags | rustix::fs::OFlags::NONBLOCK)
        .map_err(std::io::Error::from)?;
    let mut pending = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        if stop.load(Ordering::SeqCst) {
            process.terminate()?;
            state.lock().map_err(|_| Error::Poisoned)?.running = false;
            return Ok(());
        }
        let mut descriptors = [PollFd::new(&pipe, PollFlags::IN)];
        match poll(
            &mut descriptors,
            Some(&Timespec {
                tv_sec: 0,
                tv_nsec: 20_000_000,
            }),
        ) {
            Ok(_) | Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(std::io::Error::from(error).into()),
        }
        match rustix::io::read(&pipe, &mut buffer) {
            Ok(0) => {
                if !pending.is_empty() {
                    state
                        .lock()
                        .map_err(|_| Error::Poisoned)?
                        .line(&String::from_utf8_lossy(&pending));
                }
                let status = loop {
                    if let Some(status) = process.observe()? {
                        break status;
                    }
                    if stop.load(Ordering::SeqCst) {
                        process.terminate()?;
                        state.lock().map_err(|_| Error::Poisoned)?.running = false;
                        return Ok(());
                    }
                    thread::sleep(Duration::from_millis(5));
                };
                let code = status
                    .exit_status()
                    .unwrap_or_else(|| -status.terminating_signal().unwrap_or(1));
                process.reap()?;
                state.lock().map_err(|_| Error::Poisoned)?.complete(code);
                return Ok(());
            }
            Ok(count) => {
                pending.extend_from_slice(&buffer[..count]);
                while let Some(end) = pending
                    .iter()
                    .position(|byte| matches!(byte, b'\n' | b'\r'))
                {
                    state
                        .lock()
                        .map_err(|_| Error::Poisoned)?
                        .line(&String::from_utf8_lossy(&pending[..end]));
                    pending.drain(..=end);
                }
            }
            Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => {}
            Err(error) => return Err(std::io::Error::from(error).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::OwnedFd;

    struct OwnedTestChild(std::process::Child);
    impl OwnedTestChild {
        fn sleeper() -> Self {
            Self(
                std::process::Command::new("sleep")
                    .arg("60")
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .unwrap(),
            )
        }
    }
    impl Drop for OwnedTestChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn process_fd(pid: u32) -> OwnedFd {
        rustix::process::pidfd_open(
            Pid::from_raw(i32::try_from(pid).unwrap()).unwrap(),
            rustix::process::PidfdFlags::empty(),
        )
        .unwrap()
    }

    fn exited_by(fd: &OwnedFd, deadline: Instant) -> bool {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let timeout = Timespec::try_from(remaining).unwrap();
            let mut descriptors = [PollFd::new(fd, PollFlags::IN)];
            match poll(&mut descriptors, Some(&timeout)) {
                Ok(_) => return descriptors[0].revents().contains(PollFlags::IN),
                Err(rustix::io::Errno::INTR) if Instant::now() < deadline => {}
                Err(error) => panic!("owned pidfd poll: {error}"),
            }
        }
    }

    #[test]
    fn pidfd_completion_rejects_live_process_and_survives_proc_disappearance() {
        let mut child = OwnedTestChild::sleeper();
        let fd = process_fd(child.0.id());
        assert!(!exited_by(&fd, Instant::now()));
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("owned-proc-entry");
        std::fs::create_dir(&path).unwrap();
        let existed = path.exists();
        assert!(existed);
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        std::fs::remove_dir(&path).unwrap();
        // Deterministically exit between the old exists/read-stat observations.
        let old_gone_or_zombie = !existed
            || std::fs::read_to_string(path.join("stat")).is_ok_and(|s| s.contains(") Z "));
        assert!(!old_gone_or_zombie);
        assert!(exited_by(&fd, Instant::now() + Duration::from_secs(1)));
    }

    #[test]
    fn unrelated_test_child_is_reaped_on_assertion_unwind() {
        let child = OwnedTestChild::sleeper();
        let fd = process_fd(child.0.id());
        let result = std::panic::catch_unwind(move || {
            let _owned = child;
            panic!("owned test cleanup control");
        });
        assert!(result.is_err());
        assert!(exited_by(&fd, Instant::now()));
    }
    #[test]
    fn progress_policy_preserves_source_valid_and_malformed_lines() {
        let mut state = State::new(Layout::Tici);
        state.line("waiting for internet: 17");
        assert_eq!(
            (state.progress, state.text.as_str()),
            (17, "waiting for internet")
        );
        state.line("ignored: -4");
        assert_eq!(state.progress, 17);
        state.line("label: part: 999");
        assert_eq!((state.progress, state.text.as_str()), (100, "label: part"));
        state.line("bad: 1000");
        assert_eq!(state.text, "label: part");
        let mut small = State::new(Layout::Mici);
        small.line("Downloading_SYSTEM: 31");
        assert_eq!(small.text, "downloading_system");
        for index in 0..14 {
            small.line(&format!("ordinary log {index}"));
        }
        assert_eq!(small.logs.len(), 12);
        assert_eq!(small.logs.back().unwrap(), "ordinary log 13");
        small.line(&"x".repeat(300));
        assert_eq!(small.logs.back().unwrap().chars().count(), 240);
    }
    fn fixture(layout: Layout, script: &str) -> (tempfile::TempDir, Worker) {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let updater = root.path().join("updater");
        std::fs::write(&updater, script).unwrap();
        std::fs::set_permissions(&updater, std::fs::Permissions::from_mode(0o755)).unwrap();
        let launcher = std::env::var_os("AGNOS_UPDATER_CHILD")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::current_exe()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("openpilot-process-child")
            });
        assert!(launcher.is_file(), "build openpilot-process-child first");
        let worker = Worker::new(Options {
            updater,
            manifest: root.path().join("manifest.json"),
            launcher,
            cwd: root.path().into(),
            layout,
        });
        (root, worker)
    }
    fn finished(worker: &Worker) -> State {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let state = worker.snapshot().unwrap();
            if !state.running {
                return state;
            }
            assert!(Instant::now() < deadline, "owned updater did not finish");
            thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn real_command_success_error_and_single_reboot() {
        for layout in [Layout::Tici, Layout::Mici] {
            let (root,mut worker)=fixture(layout,"#!/bin/sh\nprintf '%s\\n' \"$@\" > argv\nprintf 'Downloading_SYSTEM: 37\\nverified inactive slot: 100\\n'\nexit 0\n");
            assert!(worker.start().unwrap());
            let state = finished(&worker);
            assert_eq!(state.progress, 100);
            assert!(state.failure.is_none());
            assert!(worker.take_reboot().unwrap());
            assert!(!worker.take_reboot().unwrap());
            assert_eq!(
                std::fs::read_to_string(root.path().join("argv")).unwrap(),
                format!(
                    "--swap\n--retry-network\n{}\n",
                    root.path().join("manifest.json").display()
                )
            );
            worker.close().unwrap();
            let (_root, mut worker) = fixture(
                layout,
                "#!/bin/sh\nprintf 'partial: 17\\nfatal image hash mismatch\\n'\nexit 9\n",
            );
            worker.start().unwrap();
            let state = finished(&worker);
            assert_eq!(state.progress, 17);
            assert_eq!(
                state.failure.unwrap(),
                format!(
                    "{} exited with code 9: fatal image hash mismatch",
                    if layout == Layout::Tici {
                        "Updater"
                    } else {
                        "updater"
                    }
                )
            );
            assert!(!worker.take_reboot().unwrap());
            worker.close().unwrap();
            println!("owned real command {layout:?}: exact swap/retry argv, success reboot consumed once, fatal exit9 no reboot");
        }
    }
    #[test]
    fn duplicate_start_and_close_reap_owned_child_preserve_unrelated() {
        let (root,mut worker)=fixture(Layout::Mici,"#!/bin/sh\necho $$ > leader\nsleep 60 &\necho $! > descendant\nprintf 'waiting for internet: 0\\n'\nwait\n");
        let mut unrelated = OwnedTestChild::sleeper();
        worker.start().unwrap();
        assert!(!worker.start().unwrap());
        let deadline = Instant::now() + Duration::from_secs(2);
        while !root.path().join("descendant").is_file() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        let pid: i32 = std::fs::read_to_string(root.path().join("leader"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let child: i32 = std::fs::read_to_string(root.path().join("descendant"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let descendant = process_fd(u32::try_from(child).unwrap());
        assert!(!exited_by(&descendant, Instant::now()));
        let start = Instant::now();
        worker.close().unwrap();
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        assert!(
            exited_by(&descendant, start + Duration::from_secs(1)),
            "owned descendant {child} still alive after close"
        );
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(unrelated.0.try_wait().unwrap().is_none());
        println!("owned duplicate start=false; closed leader {pid} absent descendant {child} stopped; unrelated child remained alive");
    }
    #[test]
    fn missing_executable_is_visible_start_failure() {
        let (_root, mut worker) = fixture(Layout::Tici, "#!/bin/sh\nexit 0\n");
        std::fs::remove_file(&worker.options.updater).unwrap();
        worker.start().unwrap();
        let state = finished(&worker);
        assert_eq!(state.text, "Update failed");
        let failure = state.failure.unwrap();
        assert!(
            failure.starts_with("Unable to start updater: [Errno 2]"),
            "actual failure: {failure}"
        );
        assert!(!worker.take_reboot().unwrap());
        println!("owned missing executable: visible failure no reboot");
    }
}
