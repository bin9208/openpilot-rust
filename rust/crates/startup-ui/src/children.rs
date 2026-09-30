//! Owning counterparts of common/spinner.py and common/text_window.py.
use crate::Error;
use std::time::Duration;
#[derive(Clone, Copy, Debug)]
pub enum Window<'a> {
    Spinner,
    Text(&'a str),
}
#[derive(Debug, thiserror::Error)]
pub enum StopError {
    #[error("child wait timed out")]
    Timeout,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
pub trait Child {
    fn status(&mut self) -> Result<Option<i32>, Error>;
    fn send(&mut self, payload: &[u8]) -> std::io::Result<bool>;
    fn kill(&mut self) -> std::io::Result<()>;
    fn terminate(&mut self) -> Result<(), Error>;
    fn communicate(&mut self, timeout: Duration) -> Result<(), StopError>;
}
pub trait Launch {
    type Child: Child;
    fn spawn(&mut self, window: Window<'_>) -> Result<Self::Child, Error>;
    fn warning(&mut self, message: &str) {
        eprintln!("{message}");
    }
}
pub struct Spinner<L: Launch> {
    pub launcher: L,
    child: Option<L::Child>,
    attempts: u8,
}
impl<L: Launch> Spinner<L> {
    pub fn new(launcher: L) -> Self {
        let mut spinner = Self {
            launcher,
            child: None,
            attempts: 0,
        };
        spinner.start();
        spinner
    }
    fn start(&mut self) -> bool {
        if self.attempts >= 2 {
            return false;
        }
        self.attempts += 1;
        match self.launcher.spawn(Window::Spinner) {
            Ok(child) => {
                self.child = Some(child);
                true
            }
            Err(error) => {
                self.launcher
                    .warning(&format!("WARNING: failed to start build spinner: {error}"));
                self.child = None;
                false
            }
        }
    }
    fn ensure_running(&mut self) -> Result<bool, Error> {
        if let Some(child) = &mut self.child {
            match child.status()? {
                None => return Ok(true),
                Some(code) => {
                    self.launcher.warning(&format!(
                        "WARNING: build spinner exited with code {code}; restarting"
                    ));
                    self.close();
                }
            }
        }
        Ok(self.start())
    }
    fn send(&mut self, payload: &[u8]) -> bool {
        match &mut self.child {
            None => false,
            Some(child) => match child.send(payload) {
                Ok(sent) => sent,
                Err(error) => {
                    self.launcher.warning(&format!(
                        "WARNING: build spinner stopped accepting updates: {error}"
                    ));
                    false
                }
            },
        }
    }
    pub fn update(&mut self, text: &str) -> Result<bool, Error> {
        let mut payload = text.as_bytes().to_vec();
        payload.push(b'\n');
        if !self.ensure_running()? {
            return Ok(false);
        }
        if self.send(&payload) {
            return Ok(true);
        }
        self.close();
        if !self.start() {
            return Ok(false);
        }
        let sent = self.send(&payload);
        if !sent {
            self.close();
        }
        Ok(sent)
    }
    pub fn update_progress(&mut self, current: f64, total: f64) -> Result<bool, Error> {
        if total == 0.0 {
            return Err(Error::Contract("progress division by zero"));
        }
        let value = (100.0 * current / total).round_ties_even();
        if !value.is_finite() {
            return Err(Error::Contract("nonfinite rounded progress"));
        }
        self.update(&format!("{value:.0}"))
    }
    pub fn close(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let result = (|| {
            if child
                .status()
                .map_err(|error| StopError::Io(std::io::Error::other(error.to_string())))?
                .is_none()
            {
                child.kill()?;
            }
            child.communicate(Duration::from_secs(2))
        })();
        match result {
            Ok(()) | Err(StopError::Io(_)) => {}
            Err(StopError::Timeout) => self
                .launcher
                .warning("WARNING: failed to kill build spinner"),
        }
    }
    pub fn attempts(&self) -> u8 {
        self.attempts
    }
}
impl<L: Launch> Drop for Spinner<L> {
    fn drop(&mut self) {
        self.close();
    }
}
pub struct TextWindow<L: Launch> {
    pub launcher: L,
    child: Option<L::Child>,
}
impl<L: Launch> TextWindow<L> {
    pub fn new(mut launcher: L, text: &str) -> Self {
        let child = launcher.spawn(Window::Text(text)).ok();
        Self { launcher, child }
    }
    pub fn status(&mut self) -> Result<Option<i32>, Error> {
        match &mut self.child {
            Some(child) => child.status(),
            None => Ok(None),
        }
    }
    /// Source contract waits specifically for exit code 1; ordinary code 0 does not finish this wait.
    pub fn wait_for_exit(&mut self) -> Result<(), Error> {
        if self.child.is_some() {
            loop {
                if self.status()? == Some(1) {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        Ok(())
    }
    pub fn close(&mut self) -> Result<(), Error> {
        if let Some(child) = &mut self.child {
            child.terminate()?;
            self.child = None;
        }
        Ok(())
    }
}
impl<L: Launch> Drop for TextWindow<L> {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            self.launcher.warning(&format!("TextWindow close: {error}"));
        }
    }
}
