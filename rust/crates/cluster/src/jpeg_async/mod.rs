//! Source `cluster_usb_pipeline.py` latest-frame JPEG worker, with owned output.
pub mod codec;
mod output;
mod state;
mod worker;
use crate::h264::Control;
pub use output::DisplayOutput;
use std::{
    io,
    sync::{mpsc, Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
pub type Samples = Vec<(String, f64)>;
#[derive(Clone, Debug)]
pub struct Frame {
    pub rgba: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
}
/// Output and codec are created on, used on and dropped on the worker thread.
pub trait Output {
    fn dimensions(&self) -> Option<[u32; 2]> {
        None
    }
    fn clear_profile_samples(&mut self);
    /// # Errors
    /// Returns the selected codec's encoding failure.
    fn encode_jpeg(&mut self, frame: &Frame) -> io::Result<Vec<u8>>;
    /// # Errors
    /// Returns the existing USB upload/acknowledgement failure.
    fn send_jpeg(&mut self, jpeg: &[u8]) -> io::Result<()>;
    fn profile_samples(&self) -> Samples;
    /// # Errors
    /// Returns the existing brightness/orientation/FPS transport failure.
    fn control(&mut self, _control: Control) -> io::Result<()> {
        Err(io::Error::other(
            "JPEG output does not support display controls",
        ))
    }
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("asynchronous USB JPEG pipeline failed")]
    Failed(#[source] Arc<io::Error>),
    #[error("JPEG pipeline state lock poisoned")]
    Poisoned,
}
pub struct Pipeline {
    shared: Arc<state::Shared>,
    thread: Option<JoinHandle<()>>,
    dimensions: Option<[u32; 2]>,
    close_attempted: bool,
}
impl Pipeline {
    /// # Errors
    /// Returns thread creation or owned output initialization failure.
    pub fn start<S: Output + 'static>(
        factory: impl FnOnce() -> io::Result<S> + Send + 'static,
    ) -> Result<Self, Error> {
        let shared = Arc::new(state::Shared {
            state: Mutex::new(state::State::default()),
            ready: Condvar::new(),
        });
        let (ready, initialized) = mpsc::sync_channel(1);
        let state = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("cluster-usb-jpeg".into())
            .spawn(move || worker::run(factory, &state, &ready))?;
        let mut pipeline = Self {
            shared,
            thread: Some(thread),
            dimensions: None,
            close_attempted: false,
        };
        pipeline.dimensions = initialized.recv().map_err(io::Error::other)??;
        Ok(pipeline)
    }
    #[must_use]
    pub const fn dimensions(&self) -> Option<[u32; 2]> {
        self.dimensions
    }
    /// # Errors
    /// Returns the original worker failure or poisoned synchronization state.
    pub fn check_error(&self) -> Result<(), Error> {
        self.shared.lock()?.check()
    }
    /// Replaces the pending frame without copying immutable RGBA bytes.
    /// # Errors
    /// Returns an already-observed worker failure or poisoned state.
    pub fn submit_rgba(&self, frame: Frame) -> Result<(), Error> {
        self.check_error()?;
        self.shared.lock()?.pending = Some(frame);
        self.shared.ready.notify_one();
        Ok(())
    }
    /// # Errors
    /// Returns the worker's original failure or poisoned state.
    pub fn wait_for_capacity(&self, timeout: Option<Duration>) -> Result<bool, Error> {
        let started = Instant::now();
        let mut state = self.shared.lock()?;
        while state.pending.is_some() && !state.closing && state.error.is_none() {
            state = if let Some(timeout) = timeout {
                let remaining = timeout.saturating_sub(started.elapsed());
                if remaining.is_zero() {
                    return Ok(false);
                }
                self.shared
                    .ready
                    .wait_timeout(state, remaining)
                    .map_err(|_| Error::Poisoned)?
                    .0
            } else {
                self.shared.ready.wait(state).map_err(|_| Error::Poisoned)?
            };
        }
        state.check()?;
        Ok(state.pending.is_none())
    }
    /// Drains exactly the source pipeline's collected stage samples.
    /// # Errors
    /// Returns poisoned synchronization state.
    pub fn profile_samples(&self) -> Result<Samples, Error> {
        Ok(std::mem::take(&mut self.shared.lock()?.samples))
    }
    /// # Errors
    /// Returns worker failure or a closed control dispatch.
    pub fn control(&self, control: Control) -> Result<(), Error> {
        self.check_error()?;
        let (reply, result) = mpsc::sync_channel(1);
        {
            let mut state = self.shared.lock()?;
            if state.closing {
                return Err(io::Error::other("JPEG pipeline is closing").into());
            }
            state.controls.push_back((control, reply));
        }
        self.shared.ready.notify_one();
        result.recv().map_err(io::Error::other)??;
        Ok(())
    }
    /// Source's three-second join bound; false means the owned worker is still busy.
    /// A later close retries joining without losing its output-resource ownership.
    /// # Errors
    /// Returns poisoned state or an unexpected worker panic.
    pub fn close(&mut self) -> Result<bool, Error> {
        self.close_attempted = true;
        let mut state = self.shared.lock()?;
        state.closing = true;
        self.shared.ready.notify_all();
        let started = Instant::now();
        while !state.finished {
            let remaining = Duration::from_secs(3).saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Ok(false);
            }
            state = self
                .shared
                .ready
                .wait_timeout(state, remaining)
                .map_err(|_| Error::Poisoned)?
                .0;
        }
        drop(state);
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| io::Error::other("JPEG worker panicked"))?;
        }
        Ok(true)
    }
}
impl Drop for Pipeline {
    fn drop(&mut self) {
        if !self.close_attempted {
            if let Err(error) = self.close() {
                eprintln!("JPEG worker cleanup: {error}");
            }
        }
    }
}
