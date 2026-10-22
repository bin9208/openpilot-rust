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
    sync::{atomic::Ordering, mpsc, Arc, Condvar, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};
// Independent fixtures must not compete for each other's 50 ms frame budget under ASan.
static FIXTURE_LOCK: Mutex<()> = Mutex::new(());

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
    identity: Identity,
    gate: Arc<(Mutex<bool>, Condvar)>,
    started: mpsc::Sender<u32>,
}
impl Backend for Peer {
    fn connect(&mut self) -> Result<Identity, Error> {
        Ok(self.identity.clone())
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
    validation: Option<Vec<u8>>,
    controls: ControlState,
    // Last field: retain isolation until both real worker owners have been dropped.
    _serial: MutexGuard<'static, ()>,
}
impl Fixture {
    fn new(open: bool) -> Self {
        let fixture = Self::with_identity(open, identity());
        assert_eq!(fixture.runtime.status.decision.phase, Phase::Ready);
        fixture
    }
    fn with_identity(open: bool, identity: Identity) -> Self {
        let serial = FIXTURE_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rpc.sock");
        let (tx, started) = mpsc::channel();
        let gate = Arc::new((Mutex::new(open), Condvar::new()));
        let server = Server::spawn(
            Peer {
                identity,
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
            validation: None,
            controls: ControlState {
                standstill: false,
                cruise_enabled: true,
                lateral_active: true,
                enabled: true,
            },
            _serial: serial,
        };
        while fixture.runtime.status.decision.phase != Phase::Ready
            && fixture.runtime.detail.is_empty()
        {
            assert!(Instant::now() < end);
            fixture.begin(Mode::ActiveRequest, 1, true);
            thread::yield_now();
        }
        fixture.controls = ControlState {
            standstill: true,
            cruise_enabled: false,
            lateral_active: false,
            enabled: false,
        };
        fixture
    }
    fn begin(&mut self, mode: Mode, frame: u32, prepare_only: bool) {
        self.runtime.begin(
            FrameInput {
                mode,
                controls: self.controls,
                frame,
                source_sof: 0,
                prepare_only,
                camera_ready: true,
                validation: self.validation.as_deref(),
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
fn moving_selection_keeps_native_without_waiting_for_external_worker() {
    // Given a real external worker held at the hardware seam.
    let mut f = Fixture::new(false);
    f.controls.standstill = false;
    f.begin(Mode::ActiveRequest, 42, false);
    assert_eq!(f.started.recv_timeout(Duration::from_secs(1)).unwrap(), 42);
    // When native inference completes while USB is still held.
    let start = Instant::now();
    let output = f.runtime.finish(Some(native())).unwrap();
    // Then source selection returns native immediately without releasing the external worker.
    assert_eq!(output.plan[0][0], 7.0);
    assert!(start.elapsed() < Duration::from_millis(30));
    assert_eq!(f.runtime.status.decision.source, Source::Native);
    assert!(!f._dir.path().join("carrot-jetlink-fault").exists());
}
#[test]
fn explicit_selection_uses_current_external_result_without_historical_acceptance() {
    // Given an explicit stopped selection, live contract, and no acceptance record.
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
    assert!(!f.runtime.status.validated);
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
    let fault: f64 = std::fs::read_to_string(f._dir.path().join("carrot-jetlink-fault"))
        .unwrap()
        .parse()
        .unwrap();
    let age = openpilot_jetlink::now_ns() as f64 * 1e-9 - fault;
    assert!((0. ..5.).contains(&age));
    assert!(!f.runtime.valid_at_publish());
    f.release();
    f.begin(Mode::ActiveRequest, 43, true);
    assert!(f.runtime.status.decision.loss_latched);
    assert_eq!(f.runtime.status.decision.source, Source::Native);
}
#[test]
fn historical_acceptance_mismatch_does_not_block_explicit_live_selection() {
    // Given a connected model and an unrelated historical acceptance record.
    let mut f = Fixture::new(true);
    let mut record: serde_json::Value = serde_json::from_slice(&validation()).unwrap();
    record["identity"]["device_model"] = "other".into();
    f.validation = Some(serde_json::to_vec(&record).unwrap());
    // Live selection is independent of recorded device acceptance.
    f.begin(Mode::ActiveRequest, 42, false);
    assert!(!f.runtime.status.validated);
    assert_eq!(f.runtime.finish(Some(native())).unwrap().plan[0][0], 2.0);
    assert_eq!(f.runtime.status.decision.source, Source::Jetlink);
}

#[test]
fn matching_historical_acceptance_remains_visible_as_evidence() {
    let mut f = Fixture::new(true);
    f.validation = Some(validation());
    f.begin(Mode::ActiveRequest, 42, false);
    assert!(f.runtime.status.validated);
    assert_eq!(f.runtime.finish(Some(native())).unwrap().plan[0][0], 2.0);
}

#[test]
fn mismatched_or_missing_live_artifact_identity_cannot_select_external() {
    for artifact in [Some("b".repeat(64)), None] {
        let mut identity = identity();
        if let Some(artifact) = artifact {
            identity.insert("artifact_sha256".into(), artifact);
        } else {
            identity.remove("artifact_sha256");
        }
        let mut f = Fixture::with_identity(true, identity);
        assert!(f.runtime.detail.contains("model identity"));
        f.begin(Mode::ActiveRequest, 42, false);
        assert_eq!(f.runtime.finish(Some(native())).unwrap().plan[0][0], 7.0);
        assert_eq!(f.runtime.status.decision.source, Source::Native);
        assert!(!f.runtime.status.validated);
        assert!(f.started.try_recv().is_err());
    }
}

#[test]
fn off_does_not_warp_or_select_external_output() {
    let mut f = Fixture::new(true);
    f.runtime.begin(
        FrameInput {
            mode: Mode::Off,
            controls: f.controls,
            frame: 42,
            source_sof: 0,
            prepare_only: false,
            camera_ready: true,
            validation: None,
            desire: [0.0; 8],
            traffic: [1.0, 0.0],
            action: [0.3, 0.8],
        },
        || panic!("Off must not warp for Jetlink"),
    );
    assert_eq!(f.runtime.finish(Some(native())).unwrap().plan[0][0], 7.0);
    assert_eq!(f.runtime.status.decision.phase, Phase::Off);
}
