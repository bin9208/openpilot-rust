//! Software H264 process/output ownership from `cluster_h264_pipeline.py` (MIT).
mod config;
pub mod diagnostics;
#[cfg(feature = "native-media")]
mod hardware;
pub mod native_sender;
pub mod observability;
mod output;
mod probe;
mod process;
mod software_start;
pub mod sps;
mod startup;
mod state;
mod stream;
#[cfg(feature = "native-media")]
pub mod vidc;
mod worker;
#[cfg(feature = "native-media")]
pub use hardware::{Hardware, InputStage};

pub use config::Config;
pub use output::{Control, Output};
pub use state::Snapshot;
use std::{
    io::{self, Write},
    process::{Child, ChildStdin},
    thread::JoinHandle,
};

pub struct Software {
    config: Config,
    child: Child,
    input: Option<ChildStdin>,
    output: worker::Worker,
    stderr: Option<JoinHandle<()>>,
    state: state::Shared,
    closed: bool,
}

impl Software {
    #[must_use]
    pub const fn config(&self) -> &Config {
        &self.config
    }
    #[must_use]
    pub const fn chunk_size(&self) -> usize {
        self.output.chunk_size
    }

    /// # Errors
    /// Returns worker-state failure while attaching the source diagnostic and profiling owner.
    pub fn set_monitor(&mut self, monitor: observability::Monitor) -> io::Result<()> {
        state::lock(&self.state)?.monitor = Some(monitor);
        Ok(())
    }
    /// # Errors
    /// Returns worker-state failure while draining measured source stages.
    pub fn profile_samples(&self) -> io::Result<Vec<(String, f64)>> {
        state::lock(&self.state)?
            .monitor
            .clone()
            .map_or(Ok(Vec::new()), |monitor| monitor.drain())
    }
    #[must_use]
    pub fn encoder_pid(&self) -> Option<u32> {
        (!self.closed).then(|| self.child.id())
    }

    /// # Errors
    /// Returns worker-state failure.
    pub fn snapshot(&self) -> io::Result<Snapshot> {
        Ok(state::lock(&self.state)?.clone())
    }

    /// # Errors
    /// Returns the first output failure or an unexpected encoder exit with stderr tail.
    pub fn check_error(&mut self) -> io::Result<()> {
        let state = state::lock(&self.state)?;
        let error = state.error.clone();
        let tail = state
            .stderr_tail
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        let closing = state.closing;
        drop(state);
        if let Some(error) = error {
            return Err(io::Error::other(format!(
                "H264 USB pipeline failed: {error}\n{tail}"
            )));
        }
        if !closing {
            if let Some(status) = self.child.try_wait()? {
                return Err(io::Error::other(format!(
                    "H264 ffmpeg encoder exited with {status}\n{tail}"
                )));
            }
        }
        Ok(())
    }

    /// # Errors
    /// Returns output-worker/control failure.
    pub fn control(&mut self, control: Control) -> io::Result<()> {
        self.check_error()?;
        if self.closed {
            return Err(io::Error::other("H264 USB pipeline is closing"));
        }
        self.output.control(control)
    }

    /// # Errors
    /// Returns size changes, short RGBA input, encoder/worker failure or pipe closure.
    pub fn submit_rgba(&mut self, bytes: &[u8], width: u32, height: u32) -> io::Result<()> {
        self.check_error()?;
        if (width, height) != (self.config.width, self.config.height) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "H264 encoder input size changed",
            ));
        }
        let bytes = bytes.get(..self.config.bytes()?).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "H264 source RGBA input is too short",
            )
        })?;
        let input = self
            .input
            .as_mut()
            .ok_or_else(|| io::Error::other("H264 USB pipeline is closing"))?;
        let started = std::time::Instant::now();
        if let Err(error) = input.write_all(bytes) {
            state::failure(&self.state, &error);
            return Err(error);
        }
        if let Some(monitor) = state::lock(&self.state)?.monitor.clone() {
            monitor.elapsed("usb_h264.write_rgba", started)?;
        }
        self.check_error()
    }

    /// # Errors
    /// Returns child termination, output drain or worker join failure after cleanup.
    pub fn close(&mut self) -> io::Result<()> {
        if self.closed {
            return Ok(());
        }
        state::lock(&self.state)?.closing = true;
        self.input = None;
        let child = process::close(&mut self.child);
        let output = self.output.close();
        state::lock(&self.state)?.child_closed = true;
        let stderr = self.stderr.take().map_or(Ok(()), |thread| {
            thread
                .join()
                .map_err(|_| io::Error::other("H264 stderr worker panicked"))
        });
        self.closed = true;
        let monitor = state::lock(&self.state)?
            .monitor
            .clone()
            .map_or(Ok(()), |monitor| monitor.finish());
        child.and(output).and(stderr).and(monitor)
    }
}

impl Drop for Software {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("H264 pipeline cleanup failed: {error}");
        }
    }
}
