use super::{
    fold_commands,
    queue::{Latest, UpdateThrottle},
    wire::{Incoming, Outgoing, Packet, Pump},
    Error, Result, RouteReplayToolAction, ToolCommand, ToolsState, WindowConfig,
};
use openpilot_process_supervision::{CapturedChild, CapturedCommand};
use std::{
    os::unix::net::UnixListener,
    path::PathBuf,
    process::ExitStatus,
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
    time::{Duration, Instant},
};
struct Helper {
    child: CapturedChild,
}
impl Drop for Helper {
    fn drop(&mut self) {
        match self.child.process.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                if let Err(error) = self.child.process.kill() {
                    eprintln!("replay helper kill: {error}");
                }
                if let Err(error) = self.child.process.wait() {
                    eprintln!("replay helper reap: {error}");
                }
            }
            Err(error) => eprintln!("replay helper wait: {error}"),
        }
    }
}
pub struct RouteReplayToolsWindow {
    helper: Helper,
    pump: Pump,
    updates: Arc<Latest<Packet>>,
    commands: Receiver<ToolCommand>,
    throttle: UpdateThrottle,
    started: Instant,
    directory: tempfile::TempDir,
    exit_status: Option<ExitStatus>,
}
impl RouteReplayToolsWindow {
    /// # Errors
    /// Returns process launch, private IPC setup or helper handshake errors.
    pub fn new(config: &WindowConfig) -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("cluster-replay-")
            .tempdir_in("/tmp")?;
        let path = directory.path().join("tools.sock");
        let listener = UnixListener::bind(&path)?;
        listener.set_nonblocking(true)?;
        let command = CapturedCommand {
            launcher: config.launcher.clone(),
            cwd: config.cwd.clone(),
            argv: vec![
                config.program.as_os_str().into(),
                "--replay-tools-helper".into(),
                path.into_os_string(),
                serde_json::to_string(&config.gui)?.into(),
            ],
        };
        let mut helper = Helper {
            child: command.spawn_inherited()?,
        };
        let started = Instant::now();
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if helper.child.process.try_wait()?.is_some() {
                        return Err(Error::Contract(
                            "replay helper exited before IPC connection",
                        ));
                    }
                    if started.elapsed() > Duration::from_secs(5) {
                        return Err(Error::Contract("replay helper IPC connection timeout"));
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        };
        let updates = Arc::new(Latest::default());
        let (sender, commands) = mpsc::sync_channel(16);
        let pump = Pump::spawn(
            stream,
            Outgoing::Latest(Arc::clone(&updates)),
            Incoming::Commands(sender),
        )?;
        Ok(Self {
            helper,
            pump,
            updates,
            commands,
            throttle: UpdateThrottle::default(),
            started,
            directory,
            exit_status: None,
        })
    }
    /// # Errors
    /// Returns invalid replay times or helper wait errors.
    pub fn update(&mut self, state: ToolsState) -> Result<()> {
        if self.is_alive()?
            && self
                .throttle
                .accepts(self.started.elapsed().as_secs_f64() + 1.)
        {
            state.check_times()?;
            self.updates.replace(Packet::State {
                state: Box::new(state),
            });
        }
        Ok(())
    }
    /// # Errors
    /// Returns the child wait error.
    pub fn is_alive(&mut self) -> Result<bool> {
        if self.exit_status.is_none() {
            self.exit_status = self.helper.child.process.try_wait()?;
        }
        Ok(self.exit_status.is_none())
    }
    #[must_use]
    pub fn poll(&self) -> Option<RouteReplayToolAction> {
        fold_commands(self.commands.try_iter())
    }
    #[must_use]
    pub fn process_id(&self) -> u32 {
        self.helper.child.process.id()
    }
    #[must_use]
    pub fn exit_status(&self) -> Option<ExitStatus> {
        self.exit_status
    }
    #[must_use]
    pub fn socket_directory(&self) -> PathBuf {
        self.directory.path().into()
    }
    /// # Errors
    /// Returns signal, child wait or IPC shutdown errors after owned cleanup.
    pub fn close(&mut self) -> Result<()> {
        if self.is_alive()? {
            self.updates.replace(Packet::Stop);
            self.wait_for(Duration::from_secs(2))?;
        }
        if self.is_alive()? {
            let pid = rustix::process::Pid::from_raw(
                i32::try_from(self.process_id())
                    .map_err(|_| Error::Contract("helper PID invalid"))?,
            )
            .ok_or(Error::Contract("helper PID invalid"))?;
            match rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => return Err(std::io::Error::from(error).into()),
            }
            self.wait_for(Duration::from_secs(1))?;
        }
        if self.is_alive()? {
            self.helper.child.process.kill()?;
            self.exit_status = Some(self.helper.child.process.wait()?);
        }
        self.pump.close()?;
        Ok(())
    }
    fn wait_for(&mut self, duration: Duration) -> Result<()> {
        let start = Instant::now();
        while self.is_alive()? && start.elapsed() < duration {
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
}
impl Drop for RouteReplayToolsWindow {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("replay tools close: {error}");
        }
    }
}
