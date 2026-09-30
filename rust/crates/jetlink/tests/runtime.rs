use openpilot_jetlink::{
    adapter::Adapter,
    contract::{self, Identity},
    owner::{Backend, Server},
    runtime::{FrameInput, Runtime},
    transition::{ControlState, Mode, Phase, Source},
    Deadline, Error,
};
use openpilot_modeld::prediction::DrivingPrediction;
use std::{
    sync::{atomic::Ordering, mpsc, Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};
fn identity() -> Identity {
    [
        ("device_model", "fixture"),
        ("android_api", "35"),
        ("backend_requested", "cpu"),
        ("runtime_version", "1.22.0"),
        ("app_version", "test"),
        ("artifact_sha256", contract::SHA256),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect()
}
fn validation() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"approved":true,"device_test":true,"numerical_parity":true,"model_sha256":contract::SHA256,"identity":identity(),"warp_contract":"native-c3x-512x256-v1","duration_seconds":1800,"end_to_end_max_ms":50,"deadline_misses":0,"validation_id":"a".repeat(64)})).unwrap()
}
fn native() -> DrivingPrediction {
    let mut values = vec![0.0; contract::OUTPUT_FLOATS];
    values[917] = 7.0;
    Adapter::new()
        .unwrap()
        .parse(&values, Deadline::after(Duration::from_secs(1)).unwrap())
        .unwrap()
}
struct Peer {
    gate: Arc<(Mutex<bool>, Condvar)>,
    started: mpsc::Sender<u32>,
}
impl Backend for Peer {
    fn connect(&mut self) -> Result<Identity, Error> {
        Ok(identity())
    }
    fn infer(
        &mut self,
        frame: u32,
        _: &[u8],
        _: &[f32],
        _: Deadline,
        _: bool,
    ) -> Result<Vec<f32>, Error> {
        self.started.send(frame).unwrap();
        let (lock, cv) = &*self.gate;
        let mut open = lock.lock().unwrap();
        while !*open {
            open = cv.wait(open).unwrap();
        }
        let mut values = vec![0.0; contract::OUTPUT_FLOATS];
        values[917] = 2.0;
        Ok(values)
    }
    fn dead(&self) -> bool {
        false
    }
    fn close(&mut self) {}
}
struct Fixture {
    _dir: tempfile::TempDir,
    server: Server,
    runtime: Runtime,
    gate: Arc<(Mutex<bool>, Condvar)>,
    started: mpsc::Receiver<u32>,
    validation: Vec<u8>,
}
impl Fixture {
    fn new(open: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rpc.sock");
        let (tx, started) = mpsc::channel();
        let gate = Arc::new((Mutex::new(open), Condvar::new()));
        let server = Server::spawn(
            Peer {
                gate: Arc::clone(&gate),
                started: tx,
            },
            &path,
        )
        .unwrap();
        let end = Instant::now() + Duration::from_secs(2);
        while !server.ready.load(Ordering::Acquire) {
            assert!(Instant::now() < end);
            thread::yield_now();
        }
        let runtime = Runtime::new(&path, false).unwrap();
        let mut fixture = Self {
            _dir: dir,
            server,
            runtime,
            gate,
            started,
            validation: validation(),
        };
        while fixture.runtime.status.decision.phase != Phase::Ready {
            assert!(Instant::now() < end);
            fixture.begin(Mode::Shadow, 1, true);
            thread::yield_now();
        }
        fixture
    }
    fn begin(&mut self, mode: Mode, frame: u32, prepare_only: bool) {
        self.runtime.begin(
            FrameInput {
                mode,
                controls: ControlState {
                    standstill: true,
                    cruise_enabled: false,
                    lateral_active: false,
                    enabled: false,
                },
                frame,
                prepare_only,
                camera_ready: true,
                validation: Some(&self.validation),
                desire: [0.0; 8],
                traffic: [1.0, 0.0],
                action: [0.3, 0.8],
            },
            || Ok(vec![0; contract::WARPED_BYTES]),
        );
    }
    fn release(&self) {
        let (lock, cv) = &*self.gate;
        *lock.lock().unwrap() = true;
        cv.notify_all();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.release();
        if let Err(error) = self.server.stop(Duration::from_secs(1)) {
            panic!("server shutdown: {error}");
        }
    }
}
#[test]
fn shadow_never_waits_for_external_worker() {
    // Given a real external worker held at the hardware seam.
    let mut f = Fixture::new(false);
    f.begin(Mode::Shadow, 42, false);
    assert_eq!(f.started.recv_timeout(Duration::from_secs(1)).unwrap(), 42);
    // When native inference completes while USB is still held.
    let start = Instant::now();
    let output = f.runtime.finish(Some(native())).unwrap();
    // Then source selection returns native immediately without releasing the external worker.
    assert_eq!(output.plan[0][0], 7.0);
    assert!(start.elapsed() < Duration::from_millis(30));
    assert_eq!(f.runtime.status.decision.source, Source::Native);
}
#[test]
fn active_uses_matching_external_result_while_native_remains_executed() {
    // Given validated stopped activation with a real local worker.
    let mut f = Fixture::new(true);
    f.begin(Mode::ActiveRequest, 42, false);
    // When the caller executes its native model and then finishes the same frame.
    let native_result = native();
    assert_eq!(native_result.plan[0][0], 7.0);
    let output = f.runtime.finish(Some(native_result)).unwrap();
    // Then that frame selects external output and records its identity/activation reset.
    assert_eq!(output.plan[0][0], 2.0);
    assert_eq!(f.runtime.status.frame, 42);
    assert_eq!(f.runtime.status.decision.source, Source::Jetlink);
    assert!(f.runtime.status.decision.reset_required);
    assert!(f.runtime.valid_at_publish());
}
#[test]
fn active_timeout_latches_loss_and_never_relabels_old_native_as_fresh() {
    // Given active inference blocked at the hardware seam.
    let mut f = Fixture::new(false);
    f.begin(Mode::ActiveRequest, 42, false);
    assert_eq!(f.started.recv_timeout(Duration::from_secs(1)).unwrap(), 42);
    // When the complete frame budget expires, the native result is already available.
    let start = Instant::now();
    let output = f.runtime.finish(Some(native())).unwrap();
    // Then fallback is bounded and stale, and later native success cannot clear the latch.
    assert_eq!(output.plan[0][0], 7.0);
    assert!(start.elapsed() < Duration::from_millis(200));
    assert!(f.runtime.status.decision.loss_latched);
    assert!(!f.runtime.valid_at_publish());
    f.release();
    f.begin(Mode::ActiveRequest, 43, true);
    assert!(f.runtime.status.decision.loss_latched);
    assert_eq!(f.runtime.status.decision.source, Source::Native);
}
#[test]
fn ready_without_exact_acceptance_record_cannot_activate() {
    // Given a connected model with a mutated acceptance identity.
    let mut f = Fixture::new(true);
    let mut record: serde_json::Value = serde_json::from_slice(&f.validation).unwrap();
    record["identity"]["device_model"] = "other".into();
    f.validation = serde_json::to_vec(&record).unwrap();
    // When an activation edge is requested, then native remains the source.
    f.begin(Mode::ActiveRequest, 42, false);
    assert!(!f.runtime.status.validated);
    assert_eq!(f.runtime.finish(Some(native())).unwrap().plan[0][0], 7.0);
    assert_eq!(f.runtime.status.decision.source, Source::Native);
}
