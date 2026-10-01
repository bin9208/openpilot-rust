use crate::{
    children::{Child, Launch, StopError, Window},
    Error,
};
use openpilot_process_supervision::{CapturedChild, CapturedCommand};
use std::{
    io::Write,
    os::unix::process::ExitStatusExt,
    path::PathBuf,
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct NativeLaunch {
    pub source_root: PathBuf,
    pub binaries: PathBuf,
    pub launcher: PathBuf,
}
pub struct NativeChild {
    child: Option<CapturedChild>,
}
impl Launch for NativeLaunch {
    type Child = NativeChild;
    fn spawn(&mut self, window: Window<'_>) -> Result<NativeChild, Error> {
        let (binary, text) = match window {
            Window::Spinner => ("openpilot-spinner", None),
            Window::Text(text) => ("openpilot-text-window", Some(text)),
        };
        let mut argv = vec![
            self.binaries.join(binary).into_os_string(),
            "--source-root".into(),
            self.source_root.clone().into_os_string(),
        ];
        if let Some(text) = text {
            argv.push("--text".into());
            argv.push(text.into());
        }
        Ok(NativeChild {
            child: Some(
                CapturedCommand {
                    launcher: self.launcher.clone(),
                    cwd: self.source_root.join("openpilot/system/ui"),
                    argv,
                }
                .spawn_piped_stdin()?,
            ),
        })
    }
}
impl Child for NativeChild {
    fn status(&mut self) -> Result<Option<i32>, Error> {
        let child = self
            .child
            .as_mut()
            .ok_or(Error::Contract("child already released"))?;
        Ok(child.process.try_wait()?.map(|status| {
            status
                .code()
                .unwrap_or_else(|| -status.signal().unwrap_or(0))
        }))
    }
    fn send(&mut self, payload: &[u8]) -> std::io::Result<bool> {
        let Some(stdin) = self
            .child
            .as_mut()
            .and_then(|child| child.process.stdin.as_mut())
        else {
            return Ok(false);
        };
        stdin.write_all(payload)?;
        stdin.flush()?;
        Ok(true)
    }
    fn kill(&mut self) -> std::io::Result<()> {
        match &mut self.child {
            Some(child) => child.process.kill(),
            None => Ok(()),
        }
    }
    fn terminate(&mut self) -> Result<(), Error> {
        if self.status()?.is_none() {
            let child = self
                .child
                .as_ref()
                .ok_or(Error::Contract("child missing"))?;
            let pid = i32::try_from(child.process.id())
                .ok()
                .and_then(rustix::process::Pid::from_raw)
                .ok_or(Error::Contract("PID out of range"))?;
            match rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => return Err(std::io::Error::from(error).into()),
            }
        }
        Ok(())
    }
    fn communicate(&mut self, timeout: Duration) -> Result<(), StopError> {
        let Some(child) = &mut self.child else {
            return Ok(());
        };
        drop(child.process.stdin.take());
        let start = Instant::now();
        loop {
            if child.process.try_wait()?.is_some() {
                self.child = None;
                return Ok(());
            }
            if start.elapsed() >= timeout {
                return Err(StopError::Timeout);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
impl Drop for NativeChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            drop(child.process.stdin.take());
            match child.process.try_wait() {
                Ok(Some(_)) => {}
                Ok(None) => match std::thread::Builder::new()
                    .name("startup-ui-reaper".into())
                    .spawn(move || {
                        if let Err(error) = child.process.wait() {
                            eprintln!("startup UI child reap: {error}");
                        }
                    }) {
                    Ok(handle) => drop(handle),
                    Err(error) => eprintln!("startup UI reaper: {error}"),
                },
                Err(error) => eprintln!("startup UI child status: {error}"),
            }
        }
    }
}
impl openpilot_registration::Spinner for crate::children::Spinner<NativeLaunch> {
    fn start(&mut self) -> Result<(), openpilot_registration::Error> {
        Ok(())
    }
    fn update(&mut self, text: &str) -> Result<(), openpilot_registration::Error> {
        crate::children::Spinner::update(self, text)
            .map(|_| ())
            .map_err(|error| openpilot_registration::Error::Spinner(error.to_string()))
    }
    fn close(&mut self) -> Result<(), openpilot_registration::Error> {
        crate::children::Spinner::close(self);
        Ok(())
    }
}
