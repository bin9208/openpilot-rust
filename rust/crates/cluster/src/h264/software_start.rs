use super::{config::Config, process, state, worker, Output, Snapshot, Software};
use std::{
    io,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};

impl Software {
    /// # Errors
    /// Returns provider discovery, stream initialization, worker or encoder spawn failure.
    pub fn start<S: Output + 'static>(
        config: Config,
        factory: impl FnOnce() -> io::Result<S> + Send + 'static,
    ) -> io::Result<Self> {
        config.bytes()?;
        let (encoder, muxer) = config.resolve()?;
        let state = Arc::new(Mutex::new(Snapshot::default()));
        let (output, config) =
            worker::Worker::start(config, Arc::clone(&state), factory, |_, _| Ok(()))?;
        Self::from_output(config, output, state, &encoder, &muxer)
    }
    /// # Errors
    /// Returns worker-owned display opening, dimension configuration, stream or encoder failures.
    pub fn start_with<S: Output + 'static>(
        config: Config,
        factory: impl FnOnce() -> io::Result<S> + Send + 'static,
        configure: impl FnOnce(&mut Config, Option<[u32; 2]>) -> io::Result<()>,
    ) -> io::Result<Self> {
        let state = Arc::new(Mutex::new(Snapshot::default()));
        let mut resolved = None;
        let (output, config) =
            worker::Worker::start(config, Arc::clone(&state), factory, |config, dimensions| {
                configure(config, dimensions)?;
                config.bytes()?;
                resolved = Some(config.resolve()?);
                Ok(())
            })?;
        let (encoder, muxer) =
            resolved.ok_or_else(|| io::Error::other("H264 output configuration missing"))?;
        Self::from_output(config, output, state, &encoder, &muxer)
    }
    fn from_output(
        config: Config,
        mut output: worker::Worker,
        state: state::Shared,
        encoder: &str,
        muxer: &str,
    ) -> io::Result<Self> {
        let child = Command::new(&config.ffmpeg)
            .args(config.arguments(encoder, muxer))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(error) => {
                output.close()?;
                return Err(error);
            }
        };
        let input = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("H264 stdout missing"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("H264 stderr missing"))?;
        let mut pipeline = Self {
            config,
            child,
            input,
            output,
            stderr: None,
            state,
            closed: false,
        };
        pipeline.output.source(stdout)?;
        pipeline.stderr = Some(process::stderr(stderr, Arc::clone(&pipeline.state))?);
        Ok(pipeline)
    }
}
