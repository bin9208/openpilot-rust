//! Single background RPC worker; native execution stays on the frame thread.
use crate::{
    adapter::Adapter,
    contract,
    rpc::ProxyClient,
    transition::{ControlState, Decision, Mode, Outcome, Phase, Source, Transition},
    Deadline, Error,
};
use openpilot_modeld::prediction::DrivingPrediction;
use std::{
    net::Shutdown,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

struct Model {
    client: ProxyClient,
    adapter: Adapter,
}
enum Work {
    Connect(PathBuf),
    Infer {
        model: Model,
        warped: Vec<u8>,
        packed: [f32; 12],
        frame: u32,
        deadline: Deadline,
        started: Instant,
    },
}
enum Complete {
    Connected(Model),
    Inferred {
        model: Model,
        output: Box<DrivingPrediction>,
        frame: u32,
        elapsed: Duration,
    },
}
#[derive(Clone, Debug)]
pub struct Status {
    pub decision: Decision,
    pub generation: String,
    pub frame: u32,
    pub execution_ms: f64,
    pub validated: bool,
}
pub struct FrameInput<'a> {
    pub mode: Mode,
    pub controls: ControlState,
    pub frame: u32,
    pub prepare_only: bool,
    pub camera_ready: bool,
    pub validation: Option<&'a [u8]>,
    pub desire: [f32; 8],
    pub traffic: [f32; 2],
    pub action: [f32; 2],
}
pub struct Runtime {
    work: Option<SyncSender<Work>>,
    results: Receiver<Result<Complete, Error>>,
    worker: Option<JoinHandle<()>>,
    identity: Option<contract::Identity>,
    abandoned: bool,
    path: PathBuf,
    transition: Transition,
    model: Option<Model>,
    pending: bool,
    connecting: bool,
    cancel: Option<UnixStream>,
    retry_at: Instant,
    mode: Mode,
    controls: ControlState,
    deadline: Deadline,
    started: Instant,
    frame: u32,
    pub status: Status,
    pub detail: String,
}
impl Runtime {
    /// Construct before assigning the realtime frame thread's scheduler/affinity.
    pub fn new(path: &Path, previously_active: bool) -> Result<Self, Error> {
        let (work_tx, work_rx) = mpsc::sync_channel::<Work>(1);
        let (result_tx, results) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("jetlink-rpc".into())
            .spawn(move || {
                crate::platform::background();
                while let Ok(work) = work_rx.recv() {
                    let result = execute(work);
                    if result_tx.send(result).is_err() {
                        break;
                    }
                }
            })?;
        let now = Instant::now();
        Ok(Self {
            work: Some(work_tx),
            results,
            worker: Some(worker),
            identity: None,
            abandoned: false,
            path: path.to_owned(),
            transition: Transition::new(previously_active),
            model: None,
            pending: false,
            connecting: false,
            cancel: None,
            retry_at: now,
            mode: Mode::Off,
            controls: ControlState {
                standstill: false,
                cruise_enabled: false,
                lateral_active: false,
                enabled: false,
            },
            deadline: Deadline(0),
            started: now,
            frame: 0,
            status: Status {
                decision: Decision {
                    source: Source::Native,
                    phase: Phase::Off,
                    loss_latched: previously_active,
                    reset_required: false,
                },
                generation: String::new(),
                frame: 0,
                execution_ms: 0.0,
                validated: false,
            },
            detail: String::new(),
        })
    }
    fn send(&self, work: Work) -> Result<(), Error> {
        self.work
            .as_ref()
            .ok_or(Error::Closed)?
            .try_send(work)
            .map_err(|_| Error::Contract("RPC worker busy or stopped"))
    }
    fn cancel(&mut self) {
        if let Some(stream) = self.cancel.take() {
            if let Err(error) = stream.shutdown(Shutdown::Both) {
                if error.kind() != std::io::ErrorKind::NotConnected {
                    eprintln!("jetlink cancellation: {error}");
                }
            }
        }
    }
    fn lost(&mut self, error: impl std::fmt::Display) {
        self.detail = error.to_string().chars().take(240).collect();
        self.status.decision = self.transition.update(
            self.mode,
            false,
            self.status.validated,
            self.controls,
            Outcome::Lost,
        );
        self.cancel();
        self.model = None;
        self.identity = None;
        self.abandoned = true;
        self.retry_at = Instant::now() + Duration::from_secs(5);
        // A cancelled worker is drained before any new work is submitted.
    }
    fn collect(&mut self, result: Result<Complete, Error>) -> Option<DrivingPrediction> {
        let was_connecting = self.connecting;
        self.pending = false;
        self.connecting = false;
        if self.abandoned {
            self.abandoned = false;
            return None;
        }
        match result {
            Ok(Complete::Connected(model)) => {
                self.status.generation = crate::rpc::encode_generation(model.client.generation);
                self.identity = Some(model.client.identity.clone());
                match model.client.cancel_handle() {
                    Ok(handle) => self.cancel = Some(handle),
                    Err(error) => {
                        self.lost(error);
                        return None;
                    }
                }
                self.model = Some(model);
                self.detail.clear();
                None
            }
            Ok(Complete::Inferred {
                model,
                output,
                frame,
                elapsed,
            }) => {
                self.status.execution_ms = elapsed.as_secs_f64() * 1000.0;
                self.status.frame = frame;
                self.model = Some(model);
                Some(*output)
            }
            Err(error) => {
                if was_connecting {
                    self.detail = error.to_string().chars().take(240).collect();
                    self.retry_at = Instant::now() + Duration::from_secs(5);
                } else {
                    self.lost(error);
                }
                None
            }
        }
    }
    pub fn begin(&mut self, input: FrameInput<'_>, warp: impl FnOnce() -> Result<Vec<u8>, Error>) {
        let FrameInput {
            mode,
            controls,
            frame,
            prepare_only,
            camera_ready,
            validation,
            desire,
            traffic,
            action,
        } = input;
        self.started = Instant::now();
        self.deadline = Deadline(crate::now_ns().saturating_add(50_000_000));
        self.frame = frame;
        self.controls = controls;
        self.mode = mode;
        if self.pending || self.connecting {
            match self.results.try_recv() {
                Ok(result) => {
                    self.collect(result);
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.lost(Error::Closed),
            }
        }
        if mode == Mode::Off {
            self.cancel();
            self.model = None;
            self.identity = None;
            if self.pending || self.connecting {
                self.abandoned = true;
            }
        } else if self.model.is_none()
            && !self.pending
            && !self.connecting
            && Instant::now() >= self.retry_at
        {
            self.abandoned = false;
            match self.send(Work::Connect(self.path.clone())) {
                Ok(()) => self.connecting = true,
                Err(error) => self.lost(error),
            }
        }
        let ready = self
            .model
            .as_ref()
            .is_some_and(|model| !model.client.dead())
            && camera_ready
            && !self.pending;
        self.status.validated = self
            .identity
            .as_ref()
            .is_some_and(|identity| contract::validation_matches(validation, identity));
        self.status.decision =
            self.transition
                .update(mode, ready, self.status.validated, controls, Outcome::None);
        if !ready || mode == Mode::Off {
            return;
        }
        let Some(mut model) = self.model.take() else {
            return;
        };
        if self.status.decision.reset_required {
            model.adapter.reset();
        }
        if prepare_only {
            model.adapter.reset();
            self.model = Some(model);
            return;
        }
        let prepared = warp().and_then(|warped| {
            model
                .adapter
                .prepare(&warped, desire, traffic, action, false)
                .map(|packed| (warped, packed))
        });
        match prepared {
            Ok((warped, Some(packed))) => {
                match self.send(Work::Infer {
                    model,
                    warped,
                    packed,
                    frame,
                    deadline: self.deadline,
                    started: self.started,
                }) {
                    Ok(()) => self.pending = true,
                    Err(error) => self.lost(error),
                }
            }
            Ok((_, None)) => self.model = Some(model),
            Err(error) => self.lost(error),
        }
    }
    pub fn finish(&mut self, native: Option<DrivingPrediction>) -> Option<DrivingPrediction> {
        if native.is_none() || self.status.decision.source != Source::Jetlink {
            return native;
        }
        if !self.pending {
            self.lost("No external frame available");
            return native;
        }
        let result = self.deadline.remaining().and_then(|remaining| {
            self.results
                .recv_timeout(remaining)
                .map_err(|_| Error::Deadline)
        });
        match result {
            Ok(result) => {
                let output = self.collect(result);
                if output.is_none() {
                    return native;
                }
                if self.status.frame != self.frame || crate::now_ns() > self.deadline.0 {
                    self.lost(Error::Deadline);
                    native
                } else {
                    output
                }
            }
            Err(error) => {
                self.lost(error);
                native
            }
        }
    }
    pub fn valid_at_publish(&mut self) -> bool {
        if self.status.decision.source == Source::Jetlink && crate::now_ns() > self.deadline.0 {
            self.lost("Jetlink publication exceeded 50ms");
            return false;
        }
        !(self.status.decision.loss_latched && crate::now_ns() > self.deadline.0)
    }
}
fn execute(work: Work) -> Result<Complete, Error> {
    match work {
        Work::Connect(path) => Ok(Complete::Connected(Model {
            client: ProxyClient::connect(&path)?,
            adapter: Adapter::new()?,
        })),
        Work::Infer {
            mut model,
            warped,
            packed,
            frame,
            deadline,
            started,
        } => {
            let values = model.client.infer(
                frame,
                &warped,
                &packed,
                deadline,
                model.adapter.reset_next(),
            )?;
            let output = Box::new(model.adapter.parse(&values, deadline)?);
            Ok(Complete::Inferred {
                model,
                output,
                frame,
                elapsed: started.elapsed(),
            })
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.cancel();
        self.work.take();
        // Connect is bounded at 500ms and inference by its original 50ms deadline.
        let until = Instant::now() + Duration::from_millis(600);
        while self
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
            && Instant::now() < until
        {
            match self.results.recv_timeout(Duration::from_millis(5)) {
                Ok(_) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        if let Some(worker) = self.worker.take() {
            if worker.is_finished() {
                if worker.join().is_err() {
                    eprintln!("jetlink RPC worker panicked");
                }
            } else {
                eprintln!("jetlink RPC worker did not stop within 600ms");
            }
        }
    }
}
