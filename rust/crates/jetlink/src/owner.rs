use crate::{
    contract::{self, Identity},
    rpc, Deadline, Error,
};
use openpilot_logging::{
    log_site, producer::Factory, runtime::RuntimeDiagnostics, Fields, Number, Value,
};
use std::{
    fs,
    io::Read,
    net::Shutdown,
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// carrot's jetlink client FRAME_TIMEOUT: past it the far end is gone, not slow.
const FRAME_TIMEOUT: Duration = Duration::from_secs(3);

pub trait Backend: Send + 'static {
    fn after_reply(&mut self) -> Result<(), Error> {
        Ok(())
    }
    fn idle(&mut self) -> Result<(), Error> {
        Ok(())
    }
    /// Called once per accepted modeld connection, before any frame.
    fn session_start(&mut self) -> Result<(), Error> {
        Ok(())
    }
    fn source_sof(&mut self, _source_sof: u64) {}
    /// The peer's own report for the last answered frame, in ms: inference,
    /// time queued before it, and its total.
    fn last_timings(&self) -> Option<[f64; 3]> {
        None
    }
    fn session_contract(&self) -> &crate::session_contract::SessionContract {
        crate::session_contract::SessionContract::native_default()
    }
    fn connect(&mut self) -> Result<Identity, Error>;
    fn infer(
        &mut self,
        frame: u32,
        warped: &[u8],
        packed: &[f32],
        deadline: Deadline,
        reset: bool,
    ) -> Result<Vec<f32>, Error>;
    fn dead(&self) -> bool;
    fn close(&mut self);
}
#[derive(Default)]
pub struct GadgetOwner {
    enabled: bool,
}
impl GadgetOwner {
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn enable(
        &mut self,
        egpu_present: bool,
        setup: impl FnOnce() -> Result<(), Error>,
    ) -> Result<(), Error> {
        if self.enabled {
            return Ok(());
        }
        if egpu_present {
            return Err(Error::Contract("eGPU owns USB"));
        }
        setup()?;
        self.enabled = true;
        Ok(())
    }
    pub fn disable(&mut self, teardown: impl FnOnce() -> Result<(), Error>) -> Result<(), Error> {
        if !self.enabled {
            return Ok(());
        }
        teardown()?;
        self.enabled = false;
        Ok(())
    }
}

pub fn infer_request(
    backend: &mut impl Backend,
    generation: [u8; 16],
    payload: &[u8],
) -> Result<(Vec<u8>, Deadline, Duration), Error> {
    let packed_count = backend.session_contract().packed_count();
    let base = 1 + rpc::REQUEST_BYTES + contract::WARPED_BYTES + packed_count * 4;
    if ![base, base + 8].contains(&payload.len()) || payload[0] != b'I' {
        return Err(Error::Contract("inference request"));
    }
    if payload[1..17] != generation || payload[29] > 1 {
        return Err(Error::Contract("request identity/reset"));
    }
    let frame = u32::from_le_bytes(
        payload[17..21]
            .try_into()
            .map_err(|_| Error::Contract("frame"))?,
    );
    let deadline = Deadline(u64::from_le_bytes(
        payload[21..29]
            .try_into()
            .map_err(|_| Error::Contract("deadline"))?,
    ));
    if deadline.remaining()? > Duration::from_nanos(crate::runtime::FRAME_BUDGET_NS) {
        return Err(Error::Contract("frame budget exceeds the modeld limit"));
    }
    let offset = 1 + rpc::REQUEST_BYTES;
    let warped = &payload[offset..offset + contract::WARPED_BYTES];
    let packed = rpc::decode_floats(&payload[offset + contract::WARPED_BYTES..base])?;
    let source_sof = if payload.len() == base + 8 {
        u64::from_le_bytes(
            payload[base..]
                .try_into()
                .map_err(|_| Error::Contract("source SOF"))?,
        )
    } else {
        0
    };
    backend.source_sof(source_sof);
    backend.session_contract().check_input(warped, &packed)?;
    // The peer gets carrot's FRAME_TIMEOUT, not modeld's frame budget. A frame
    // that misses the budget is one dropped frame for modeld, which leaves and
    // joins again; only a stall this long means the far end is gone. Using the
    // 50 ms budget here closed the link, and the phone saw the cable drop and
    // come back every 20-30 s.
    let link = Deadline::after(FRAME_TIMEOUT)?;
    let exchange_started = Instant::now();
    let output = backend.infer(frame, warped, &packed, link, payload[29] == 1)?;
    let exchange = exchange_started.elapsed();
    backend.session_contract().check_output(&output)?;
    deadline.remaining()?;
    let mut response = Vec::with_capacity(21 + output.len() * 4);
    response.push(b'R');
    response.extend(generation);
    response.extend(frame.to_le_bytes());
    for value in output {
        response.extend(value.to_le_bytes());
    }
    Ok((response, deadline, exchange))
}

fn thread_cpu_ms() -> f64 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
    (now.tv_sec as f64 + now.tv_nsec as f64 * 1e-9) * 1000.0
}

/// One answered frame as carrot's daemon.py records it in `runtimeTiming`
/// (component `jetlinkd`), with the same metric names where the meaning is the
/// same. `ipc_receive_ms` includes the normal idle wait for modeld's request.
struct FrameTimes {
    frame: u32,
    receive_ms: f64,
    request_bytes: usize,
    exchange_ms: f64,
    reply_ms: f64,
    tail_ms: f64,
    loop_ms: f64,
    cpu_ms: f64,
    peer: Option<[f64; 3]>,
}
impl FrameTimes {
    fn metrics(&self) -> Vec<(String, Number)> {
        let mut metrics = vec![
            ("ipc_receive_ms", Number::Float(self.receive_ms)),
            (
                "ipc_request_bytes",
                Number::Integer(i64::try_from(self.request_bytes).unwrap_or(i64::MAX)),
            ),
            ("usb_exchange_ms", Number::Float(self.exchange_ms)),
            ("ipc_reply_ms", Number::Float(self.reply_ms)),
            ("reply_tail_ms", Number::Float(self.tail_ms)),
            ("loop_ms", Number::Float(self.loop_ms)),
            ("thread_cpu_ms", Number::Float(self.cpu_ms)),
        ];
        if let Some([gpu, queue, total]) = self.peer {
            metrics.extend([
                ("server_gpu_ms", Number::Float(gpu)),
                ("server_queue_ms", Number::Float(queue)),
                ("server_total_ms", Number::Float(total)),
            ]);
        }
        metrics
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect()
    }
    fn context(&self) -> Fields {
        [(
            "frame_id".to_owned(),
            Value::Integer(i128::from(self.frame)),
        )]
        .into_iter()
        .collect()
    }
}

pub struct Server {
    stop: Arc<AtomicBool>,
    connection: Arc<Mutex<Option<UnixStream>>>,
    done: mpsc::Receiver<Result<(), Error>>,
    worker: Option<JoinHandle<()>>,
    pub ready: Arc<AtomicBool>,
    pub error: Arc<Mutex<String>>,
    pub generation: [u8; 16],
    path: PathBuf,
    stop_invoked: bool,
}
impl Server {
    /// Starts only the local worker. The caller owns gadget setup and the process lock.
    pub fn spawn(mut backend: impl Backend, path: &Path) -> Result<Self, Error> {
        let mut generation = [0; 16];
        fs::File::open("/dev/urandom")?.read_exact(&mut generation)?;
        let stop = Arc::new(AtomicBool::new(false));
        let ready = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(String::new()));
        let worker_error = Arc::clone(&error);
        let connection = Arc::new(Mutex::new(None));
        let (done_tx, done) = mpsc::channel();
        let worker_stop = Arc::clone(&stop);
        let worker_ready = Arc::clone(&ready);
        let worker_connection = Arc::clone(&connection);
        let worker_path = path.to_owned();
        let worker = thread::Builder::new()
            .name("jetlink-usb-owner".into())
            .spawn(move || {
                let result = serve(
                    &mut backend,
                    &worker_path,
                    generation,
                    &worker_stop,
                    &worker_ready,
                    &worker_connection,
                );
                if let Err(error) = &result {
                    if let Ok(mut slot) = worker_error.lock() {
                        *slot = error.to_string().chars().take(240).collect();
                    }
                }
                worker_ready.store(false, Ordering::Release);
                backend.close();
                if let Err(error) = done_tx.send(result) {
                    eprintln!("jetlink owner receiver closed: {error}");
                }
            })?;
        Ok(Self {
            stop,
            connection,
            done,
            worker: Some(worker),
            ready,
            error,
            generation,
            path: path.to_owned(),
            stop_invoked: false,
        })
    }
    /// A kernel-stuck USB worker is detached after the bound; it never blocks modeld shutdown.
    pub fn stop(&mut self, timeout: Duration) -> Result<(), Error> {
        self.stop_invoked = true;
        self.stop.store(true, Ordering::Release);
        if let Some(connection) = self
            .connection
            .lock()
            .map_err(|_| Error::Contract("connection mutex poisoned"))?
            .take()
        {
            if let Err(error) = connection.shutdown(Shutdown::Both) {
                if error.kind() != std::io::ErrorKind::NotConnected {
                    return Err(error.into());
                }
            }
        }
        if self.worker.is_none() {
            return Ok(());
        }
        let worker_result = self
            .done
            .recv_timeout(timeout)
            .map_err(|_| Error::Deadline)?;
        if let Err(error) = worker_result {
            eprintln!("jetlink owner ended: {error}");
        }
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| Error::Contract("owner worker panicked"))?;
        }
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if !self.stop_invoked {
            if let Err(error) = self.stop(Duration::from_secs(2)) {
                eprintln!("jetlink owner stop: {error}");
            }
        }
    }
}
fn serve(
    backend: &mut impl Backend,
    path: &Path,
    generation: [u8; 16],
    stop: &AtomicBool,
    ready: &AtomicBool,
    connection: &Mutex<Option<UnixStream>>,
) -> Result<(), Error> {
    let identity = backend.connect()?;
    if stop.load(Ordering::Acquire) {
        return Ok(());
    }
    // Never unlink a potentially live owner; the process-lock holder removes its stale socket before spawn.
    let listener = UnixListener::bind(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    ready.store(true, Ordering::Release);
    // rlog evidence of where a frame's time went. A missing log sink only
    // loses the evidence; it never stops the link.
    let mut logger = Factory::for_runtime().ok().map(|factory| factory.logger());
    let mut diagnostics =
        RuntimeDiagnostics::new("jetlinkd", 1.0).with_peak_metric("usb_exchange_ms");
    while !stop.load(Ordering::Acquire) {
        let (mut stream, _) = match listener.accept() {
            Ok(pair) => pair,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                backend.idle()?;
                thread::park_timeout(Duration::from_millis(5));
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        *connection
            .lock()
            .map_err(|_| Error::Contract("connection mutex poisoned"))? = Some(stream.try_clone()?);
        let hello = rpc::recv_packet(&mut stream, Deadline::after(Duration::from_millis(500))?);
        if matches!(hello.as_deref(), Ok(b"H")) {
            backend.session_start()?;
            let info = rpc::Hello {
                ready: true,
                generation: rpc::encode_generation(generation),
                spec: backend.session_contract().raw().clone(),
                protocol: backend.session_contract().version as u16,
                identity: identity.clone(),
                shared_input: true,
            };
            let mut reply = vec![b'J'];
            reply.extend(serde_json::to_vec(&info)?);
            if rpc::send_packet(
                &mut stream,
                &reply,
                Deadline::after(Duration::from_millis(500))?,
            )
            .is_ok()
            {
                let mut shared: Option<crate::shared_input::Reader> = None;
                let mut first_request = true;
                while !stop.load(Ordering::Acquire) {
                    let loop_started = Instant::now();
                    let cpu_started = thread_cpu_ms();
                    let payload = match rpc::recv_packet(
                        &mut stream,
                        Deadline::after(Duration::from_secs(1))?,
                    ) {
                        Ok(payload) => payload,
                        Err(_) => break,
                    };
                    // As carrot's daemon.py: the slot is offered only as the
                    // first request of a connection.
                    if first_request && payload == crate::shared_input::SETUP {
                        first_request = false;
                        match crate::shared_input::Reader::offer(
                            &mut stream,
                            backend.session_contract().packed_count(),
                            Deadline::after(Duration::from_millis(500))?,
                        ) {
                            Ok(reader) => {
                                shared = Some(reader);
                                continue;
                            }
                            Err(_) => break,
                        }
                    }
                    first_request = false;
                    let payload = if payload.first() == Some(&crate::shared_input::TOKEN_KIND) {
                        match shared.as_mut().map(|reader| reader.read(&payload)) {
                            Some(Ok(request)) => request,
                            _ => break,
                        }
                    } else {
                        payload
                    };
                    let received = Instant::now();
                    let (reply, deadline, exchange) =
                        match infer_request(backend, generation, &payload) {
                            Ok(result) => result,
                            Err(error) if backend.dead() => return Err(error),
                            Err(_) => break,
                        };
                    let answered = Instant::now();
                    if rpc::send_packet(&mut stream, &reply, deadline).is_err() {
                        break;
                    }
                    let replied = Instant::now();
                    backend.after_reply()?;
                    if let Some(logger) = &mut logger {
                        let ms = |from: Instant, to: Instant| {
                            to.saturating_duration_since(from).as_secs_f64() * 1000.0
                        };
                        let finished = Instant::now();
                        let times = FrameTimes {
                            frame: u32::from_le_bytes([reply[17], reply[18], reply[19], reply[20]]),
                            receive_ms: ms(loop_started, received),
                            request_bytes: payload.len(),
                            exchange_ms: exchange.as_secs_f64() * 1000.0,
                            reply_ms: ms(answered, replied),
                            tail_ms: ms(replied, finished),
                            loop_ms: ms(loop_started, finished),
                            cpu_ms: thread_cpu_ms() - cpu_started,
                            peer: backend.last_timings(),
                        };
                        // Observability must not interrupt the link.
                        let _ = diagnostics.record(
                            logger,
                            log_site!(),
                            times.metrics(),
                            times.context(),
                        );
                    }
                }
            }
        }
        *connection
            .lock()
            .map_err(|_| Error::Contract("connection mutex poisoned"))? = None;
    }
    Ok(())
}

#[cfg(test)]
mod frame_times_tests {
    use super::*;

    #[test]
    fn metrics_use_carrots_names_and_add_the_peer_report_only_when_known() {
        let mut times = FrameTimes {
            frame: 77,
            receive_ms: 12.0,
            request_bytes: 1000,
            exchange_ms: 41.5,
            reply_ms: 0.4,
            tail_ms: 0.1,
            loop_ms: 54.0,
            cpu_ms: 3.0,
            peer: None,
        };
        let names = |times: &FrameTimes| -> Vec<String> {
            times.metrics().into_iter().map(|(name, _)| name).collect()
        };
        assert_eq!(
            names(&times),
            [
                "ipc_receive_ms",
                "ipc_request_bytes",
                "usb_exchange_ms",
                "ipc_reply_ms",
                "reply_tail_ms",
                "loop_ms",
                "thread_cpu_ms"
            ]
        );
        times.peer = Some([29.6, 0.1, 29.8]);
        assert_eq!(
            names(&times)[7..],
            ["server_gpu_ms", "server_queue_ms", "server_total_ms"]
        );
        assert_eq!(times.context().len(), 1);
    }
}
