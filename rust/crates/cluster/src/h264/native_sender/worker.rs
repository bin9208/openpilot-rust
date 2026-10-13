mod start;
use super::{select_access_units, AccessUnit, Snapshot};
use crate::{
    h264::{Control, Output},
    usb::H264Chunk,
};
use std::{
    collections::VecDeque,
    io,
    sync::{mpsc, Arc, Condvar, Mutex, MutexGuard},
    thread::JoinHandle,
    time::Duration,
};
#[derive(Default)]
struct State {
    pending: VecDeque<AccessUnit>,
    controls: VecDeque<(Control, mpsc::SyncSender<io::Result<()>>)>,
    closing: bool,
    snapshot: Snapshot,
    monitor: Option<crate::h264::observability::Monitor>,
}
#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    ready: Condvar,
}
fn lock(shared: &Shared) -> io::Result<MutexGuard<'_, State>> {
    shared
        .state
        .lock()
        .map_err(|_| io::Error::other("native H264 sender state poisoned"))
}
pub struct Sender {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    finished: mpsc::Receiver<()>,
    pub chunk_size: usize,
}
impl Sender {
    /// # Errors
    /// Returns shared-state failure while attaching the source queue observations.
    pub fn set_monitor(&mut self, monitor: crate::h264::observability::Monitor) -> io::Result<()> {
        lock(&self.shared)?.monitor = Some(monitor);
        Ok(())
    }
    /// # Errors
    /// Preserves shared-state failure while applying the source three-unit retention policy.
    pub fn enqueue(&self, unit: AccessUnit) -> io::Result<()> {
        if unit.chunks.is_empty() {
            return Ok(());
        }
        let mut state = lock(&self.shared)?;
        if state.closing {
            return Ok(());
        }
        let selection = select_access_units(state.pending.drain(..).collect(), unit);
        state.pending = selection.kept.into();
        state.snapshot.dropped_units += selection.dropped_units;
        state.snapshot.dropped_chunks += selection.dropped_chunks;
        if let Some(monitor) = &state.monitor {
            monitor.queued(
                state.pending.len(),
                selection.dropped_units,
                selection.dropped_chunks,
            )?;
        }
        self.shared.ready.notify_one();
        Ok(())
    }
    /// # Errors
    /// Preserves output control or owned-worker termination failures.
    pub fn control(&mut self, control: Control) -> io::Result<()> {
        let (reply, result) = mpsc::sync_channel(1);
        let mut state = lock(&self.shared)?;
        if state.closing || state.snapshot.error.is_some() {
            return Err(io::Error::other("native H264 output is unavailable"));
        }
        state.controls.push_back((control, reply));
        self.shared.ready.notify_one();
        drop(state);
        result
            .recv_timeout(Duration::from_secs(3))
            .map_err(io::Error::other)?
    }
    /// # Errors
    /// Preserves poisoned state or a recorded native USB sender failure.
    pub fn snapshot(&self) -> io::Result<Snapshot> {
        Ok(lock(&self.shared)?.snapshot.clone())
    }
    /// # Errors
    /// Preserves a recorded sender failure before the next native frame is submitted.
    pub fn check_error(&self) -> io::Result<()> {
        self.snapshot()?
            .error
            .map_or(Ok(()), |error| Err(io::Error::other(error)))
    }
    /// # Errors
    /// Waits for the owned sender's source three-second drain before joining; timeout remains an error.
    pub fn close(&mut self) -> io::Result<()> {
        if self.thread.is_none() {
            return Ok(());
        }
        lock(&self.shared)?.closing = true;
        self.shared.ready.notify_all();
        match self.finished.recv_timeout(Duration::from_secs(3)) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {}
            Err(error) => return Err(io::Error::other(error)),
        }
        self.thread.take().map_or(Ok(()), |thread| {
            thread
                .join()
                .map_err(|_| io::Error::other("native H264 sender panicked"))
        })
    }
}
impl Drop for Sender {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
enum Work {
    Unit(AccessUnit),
    Control(Control, mpsc::SyncSender<io::Result<()>>),
}
fn run(
    shared: &Shared,
    output: &mut impl Output,
    wait_for_ack: bool,
    soft_ack: bool,
) -> io::Result<()> {
    loop {
        let work = {
            let mut state = lock(shared)?;
            while state.pending.is_empty() && state.controls.is_empty() && !state.closing {
                state = shared
                    .ready
                    .wait(state)
                    .map_err(|_| io::Error::other("native H264 sender wake poisoned"))?;
            }
            if let Some((control, reply)) = state.controls.pop_front() {
                Some(Work::Control(control, reply))
            } else {
                state.pending.pop_front().map(Work::Unit)
            }
        };
        match work {
            None => return Ok(()),
            Some(Work::Control(control, reply)) => {
                let _ = reply.send(output.control(control));
            }
            Some(Work::Unit(unit)) => {
                for bytes in unit.chunks {
                    let mut state = lock(shared)?;
                    state.snapshot.bytes_sent += bytes.len();
                    state.snapshot.chunks_sent += 1;
                    let monitor = state.monitor.clone();
                    drop(state);
                    if let Some(monitor) = &monitor {
                        monitor.sending(&bytes)?;
                    }
                    let started = std::time::Instant::now();
                    output.send(H264Chunk {
                        bytes: &bytes,
                        last: false,
                        wait_for_ack,
                        require_ack: !soft_ack,
                    })?;
                    if let Some(monitor) = monitor {
                        monitor.sent(&bytes, started.elapsed().as_secs_f64() * 1000.0)?;
                        monitor.samples(output.drain_profile_samples())?;
                    }
                }
            }
        }
    }
}
