//! The recovery display owns one isolated action session and reaps it on exit.
use crate::{config::Config, Error};
use openpilot_process_supervision::{CapturedChild, CapturedCommand};
use openpilot_startup_recovery::state::{Progress, Snapshot, Update};
use rustix::process::{Pid, Signal, WaitId, WaitIdOptions};
use std::{
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};

pub fn initialize_process() -> Result<(), Error> {
    rustix::process::set_child_subreaper(Some(Pid::INIT)).map_err(std::io::Error::from)?;
    Ok(())
}
struct Action {
    child: Option<CapturedChild>,
    leader: Pid,
    _directory: tempfile::TempDir,
    state: PathBuf,
    started: Instant,
}
impl Action {
    fn finish(&mut self) -> Result<(), Error> {
        let Some(child) = &mut self.child else {
            return Ok(());
        };
        match rustix::process::kill_process_group(self.leader, Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(error) => return Err(std::io::Error::from(error).into()),
        }
        // Keep the leader unreaped while discovering/reaping its session.
        let cleanup = crate::terminal::session::cleanup(self.leader);
        let reaped = child.process.wait().map(|_| ()).map_err(Error::Io);
        self.child = None;
        cleanup.and(reaped)
    }
}
impl Drop for Action {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            eprintln!("recovery action cleanup: {error}");
        }
    }
}
pub struct Worker {
    config: Config,
    progress: Progress,
    action: Option<Action>,
    touch_log: Option<PathBuf>,
}
impl Worker {
    /// Call initialize_process once in the display/headless owner before use.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            progress: Update::default().progress(),
            action: None,
            touch_log: None,
        }
    }
    pub fn with_touch_log(mut self, path: PathBuf) -> Self {
        self.touch_log = Some(path);
        self
    }
    pub fn running(&self) -> bool {
        self.action.is_some()
    }
    pub fn progress(&self) -> &Progress {
        &self.progress
    }
    pub fn start(&mut self, automatic: bool, rebuild: bool) -> Result<bool, Error> {
        if automatic && rebuild {
            return Err(Error::Source("automatic repair is not allowed".into()));
        }
        if self.action.is_some() || self.progress.state.busy() {
            return Ok(false);
        }
        if std::env::var("CARROT_STARTUP_RECOVERY").as_deref() != Ok("1") {
            return Err(Error::Source(
                "Recovery actions are available only from the failed-startup launcher.".into(),
            ));
        }
        let directory = tempfile::tempdir()?;
        let state = directory.path().join("state.json");
        let helper = self
            .config
            .launcher
            .with_file_name("openpilot-carrot-recovery");
        let mut argv = vec![
            helper.into_os_string(),
            "__startup-action".into(),
            "--repo".into(),
            self.config.repository.clone().into_os_string(),
            "--state-file".into(),
            state.clone().into_os_string(),
        ];
        if automatic {
            argv.push("--automatic".into());
        }
        if rebuild {
            argv.push("--rebuild".into());
        }
        if let Some(path) = &self.touch_log {
            argv.extend(["--touch-log".into(), path.clone().into_os_string()]);
        }
        if let Some(pending) = &self.progress.pending_reboot {
            argv.extend(["--pending".into(), pending.into()]);
        }
        let child = CapturedCommand {
            launcher: self.config.launcher.clone(),
            cwd: self.config.repository.clone(),
            argv,
        }
        .spawn_session_redirected(Stdio::null(), Stdio::inherit())?;
        let leader = Pid::from_raw(
            i32::try_from(child.process.id())
                .map_err(|_| Error::Source("recovery PID out of range".into()))?,
        )
        .ok_or_else(|| Error::Source("invalid recovery PID".into()))?;
        self.action = Some(Action {
            child: Some(child),
            leader,
            _directory: directory,
            state,
            started: Instant::now(),
        });
        self.progress.state = Snapshot {
            status: if rebuild { "cleaning" } else { "updating" }.into(),
            detail: String::new(),
        };
        Ok(true)
    }
    pub fn poll(&mut self) -> Result<(), Error> {
        let Some(action) = &mut self.action else {
            return Ok(());
        };
        match super::startup_action::read(&action.state) {
            Ok(progress) => self.progress = progress,
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                action.finish()?;
                self.action = None;
                self.progress.state = Snapshot {
                    status: "failed".into(),
                    detail: error.to_string(),
                };
                return Ok(());
            }
        }
        let observed = match rustix::process::waitid(
            WaitId::Pid(action.leader),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(status) => status,
            Err(rustix::io::Errno::INTR) => None,
            Err(error) => return Err(std::io::Error::from(error).into()),
        };
        let timed_out = action.started.elapsed() > Duration::from_secs(1200);
        if observed.is_some() || timed_out {
            // A final atomic state write happens before a successful action exit.
            if let Ok(progress) = super::startup_action::read(&action.state) {
                self.progress = progress;
            }
            let incomplete = matches!(
                self.progress.state.status.as_str(),
                "idle" | "updating" | "cleaning"
            );
            action.finish()?;
            self.action = None;
            if incomplete
                || timed_out
                || observed.is_some_and(|status| status.exit_status() != Some(0))
            {
                self.progress.state = Snapshot {
                    status: "failed".into(),
                    detail: if timed_out {
                        "Recovery action timed out."
                    } else {
                        "Recovery action exited before reporting its result."
                    }
                    .into(),
                };
            }
        }
        Ok(())
    }
    pub fn stop(&mut self) -> Result<(), Error> {
        if let Some(mut action) = self.action.take() {
            action.finish()?;
        }
        Ok(())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("recovery worker stop: {error}");
        }
    }
}
