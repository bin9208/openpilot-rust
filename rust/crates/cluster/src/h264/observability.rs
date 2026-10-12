mod logs;
use super::diagnostics::Diagnostics;
use std::{
    io,
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

#[derive(Debug, Default)]
struct Counts {
    units: usize,
    encoder_packets: usize,
    encoder_bytes: usize,
    stdout_reads: usize,
    stdout_bytes: usize,
    unit_bytes: usize,
    sends: usize,
    send_bytes: usize,
    drops: usize,
    drop_chunks: usize,
    max_packet: usize,
    max_chunk: usize,
}
#[derive(Debug)]
struct State {
    diagnostics: Diagnostics,
    backend: &'static str,
    profile: bool,
    debug: bool,
    started: Instant,
    counts: Counts,
    samples: Vec<(String, f64)>,
    chunk_size: usize,
    ack: &'static str,
    finished: bool,
}
#[derive(Clone, Debug)]
pub struct Monitor {
    state: Arc<Mutex<State>>,
}
impl Monitor {
    #[must_use]
    pub fn new(backend: &'static str, interval: f64, profile: bool, debug: bool, now: f64) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                diagnostics: Diagnostics::new(interval, now),
                backend,
                profile,
                debug,
                started: Instant::now(),
                counts: Counts::default(),
                samples: Vec::new(),
                chunk_size: 1,
                ack: "off",
                finished: false,
            })),
        }
    }
    fn lock(&self) -> io::Result<MutexGuard<'_, State>> {
        self.state
            .lock()
            .map_err(|_| io::Error::other("H264 observation state poisoned"))
    }
    /// # Errors
    /// Returns poisoned state while recording the resolved source session configuration.
    pub fn session(
        &self,
        config: &super::Config,
        display: [i32; 2],
        requested: &str,
        chunk_size: usize,
    ) -> io::Result<()> {
        let mut state = self.lock()?;
        state.chunk_size = chunk_size;
        state.ack = if config.wait_for_ack && config.soft_ack {
            "soft"
        } else if config.wait_for_ack {
            "on"
        } else {
            "off"
        };
        logs::session(&state, config, display, requested);
        Ok(())
    }
    /// # Errors
    /// Returns poisoned state while preserving source stderr debug output.
    pub fn stderr(&self, text: &str) -> io::Result<()> {
        if self.lock()?.debug {
            println!("H264 encoder: {text}");
        }
        Ok(())
    }
    #[cfg(feature = "native-media")]
    /// # Errors
    /// Returns poisoned state while accounting raw callback bytes and logging the patched view.
    pub fn packet(
        &self,
        packet: &super::vidc::Packet,
        bytes: &[u8],
        patched: bool,
    ) -> io::Result<()> {
        let mut state = self.lock()?;
        if !patched {
            state.counts.encoder_packets += 1;
            state.counts.encoder_bytes += bytes.len();
        }
        state.counts.max_packet = state.counts.max_packet.max(bytes.len());
        logs::packet(&state, packet, bytes, patched);
        Ok(())
    }
    /// # Errors
    /// Returns poisoned observation state while preserving actual produced access-unit counts.
    pub fn unit(&self, packet: &[u8], chunks: &[usize], keyframe: bool) -> io::Result<()> {
        let mut state = self.lock()?;
        let source = state.backend;
        state
            .diagnostics
            .record_unit(source, packet, chunks, keyframe);
        state.counts.units += 1;
        state.counts.unit_bytes += packet.len();
        state.counts.max_packet = state.counts.max_packet.max(packet.len());
        logs::unit(&mut state, packet, chunks);
        Ok(())
    }
    /// # Errors
    /// Returns poisoned state while recording native queue retention and actual dropped units.
    pub fn queued(&self, depth: usize, drops: usize, chunks: usize) -> io::Result<()> {
        let mut state = self.lock()?;
        state.diagnostics.record_queue_depth(depth);
        state.diagnostics.record_drop(drops, chunks);
        state.counts.drops += drops;
        state.counts.drop_chunks += chunks;
        Ok(())
    }
    /// # Errors
    /// Returns poisoned state while preserving source attempted-transfer debug counters.
    pub fn sending(&self, bytes: &[u8]) -> io::Result<()> {
        let mut state = self.lock()?;
        state.counts.sends += 1;
        state.counts.send_bytes += bytes.len();
        state.counts.max_chunk = state.counts.max_chunk.max(bytes.len());
        logs::sent(&state, bytes);
        Ok(())
    }
    /// # Errors
    /// Returns clock or state failures while recording an actual successful USB transfer.
    pub fn sent(&self, bytes: &[u8], milliseconds: f64) -> io::Result<()> {
        let mut state = self.lock()?;
        let source = state.backend;
        state
            .diagnostics
            .record_send(source, bytes.len(), milliseconds);
        if state.profile {
            state
                .samples
                .push(("usb_h264.send_chunk".into(), milliseconds));
        }
        let now = crate::live::monotonic().map_err(io::Error::other)?;
        if let Some(line) = state.diagnostics.report(now, source, false) {
            println!("{line}");
        }
        Ok(())
    }
    /// # Errors
    /// Returns poisoned state while retaining measured runtime-stage samples.
    pub fn samples(&self, samples: impl IntoIterator<Item = (String, f64)>) -> io::Result<()> {
        let mut state = self.lock()?;
        if state.profile {
            state.samples.extend(samples);
        }
        Ok(())
    }
    /// # Errors
    /// Returns poisoned state while collecting measured source-policy samples.
    pub fn elapsed(&self, name: &str, started: Instant) -> io::Result<()> {
        self.samples([(name.into(), started.elapsed().as_secs_f64() * 1000.0)])
    }
    /// # Errors
    /// Returns poisoned state while draining the existing measured samples exactly once.
    pub fn drain(&self) -> io::Result<Vec<(String, f64)>> {
        Ok(std::mem::take(&mut self.lock()?.samples))
    }
    /// # Errors
    /// Returns clock or state failure while flushing the source diagnostic window and final counters.
    pub fn finish(&self) -> io::Result<()> {
        let mut state = self.lock()?;
        if state.finished {
            return Ok(());
        }
        let source = state.backend;
        if let Some(line) = state.diagnostics.report(
            crate::live::monotonic().map_err(io::Error::other)?,
            source,
            true,
        ) {
            println!("{line}");
        }
        logs::finish(&state);
        state.finished = true;
        Ok(())
    }
}
