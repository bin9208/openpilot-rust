use openpilot_driving_modeld::recovery_task::{Candidate, Health, Task};
use std::{
    sync::{atomic::Ordering, mpsc, Arc},
    thread,
    time::{Duration, Instant},
};
struct Owned(mpsc::Sender<thread::ThreadId>);
impl Candidate for Owned {
    fn alive(&mut self) -> bool {
        true
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        self.0.send(thread::current().id()).unwrap();
    }
}

#[test]
fn unclaimed_candidate_cleanup_never_runs_on_frame_thread() {
    let health = Arc::new(Health::default());
    health.observe(true);
    let (drop_tx, drop_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let task = Task::start_with_schedule(
        health,
        move |_| {
            ready_tx.send(()).unwrap();
            Ok(Owned(drop_tx))
        },
        || Ok(()),
    );
    ready_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let started = Instant::now();
    drop(task);
    assert!(started.elapsed() < Duration::from_millis(20));
    assert_ne!(
        drop_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        thread::current().id()
    );
}

#[test]
fn stalled_publication_cancels_preparation_and_reports_source_reason() {
    let health = Arc::new(Health::default());
    health.observe(true);
    let task = Task::<Owned>::start_with_schedule(
        health,
        move |cancel| {
            while !cancel.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(2));
            }
            Err("cancelled".into())
        },
        || Ok(()),
    );
    let started = Instant::now();
    while !task.finished() && started.elapsed() < Duration::from_secs(1) {
        thread::sleep(Duration::from_millis(2));
    }
    assert!(task.finished());
    assert!(task.cancelled.load(Ordering::Acquire));
    assert_eq!(
        task.error().as_deref(),
        Some("internal model publication delayed")
    );
    assert!(task.claim().is_none());
}

#[test]
fn healthy_candidate_transfers_ownership_once_without_thread_join() {
    let health = Arc::new(Health::default());
    health.observe(true);
    let (tx, rx) = mpsc::channel();
    let task = Task::start_with_schedule(Arc::clone(&health), move |_| Ok(Owned(tx)), || Ok(()));
    let started = Instant::now();
    let candidate = loop {
        health.observe(true);
        if let Some(candidate) = task.claim() {
            break candidate;
        }
        assert!(started.elapsed() < Duration::from_secs(1));
        thread::sleep(Duration::from_millis(2));
    };
    assert!(task.claim().is_none());
    let cancel = Arc::clone(&task.cancelled);
    drop(task);
    assert!(!cancel.load(Ordering::Acquire));
    assert!(rx.try_recv().is_err());
    drop(candidate);
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        thread::current().id()
    );
}

#[test]
fn preparation_memory_floor_and_proc_units_match_source() {
    use openpilot_driving_modeld::recovery_prepare::{mem_available, memory_floor};
    assert_eq!(memory_floor(799_942_179), 2 << 30);
    assert_eq!(memory_floor(2 << 30), (4 << 30) + (512 << 20));
    assert_eq!(memory_floor(u64::MAX), u64::MAX);
    assert_eq!(
        mem_available("MemTotal: 100 kB\nMemAvailable: 2048 kB\n"),
        Some(2 << 20)
    );
    for value in [
        "MemAvailable: -1 kB",
        "MemAvailable: 12 bytes",
        "MemAvailable: 1 kB extra",
        "MemAvailable: 1 kB\nMemAvailable: 2 kB",
        "MemTotal: 1 kB",
    ] {
        assert!(mem_available(value).is_none(), "{value}");
    }
}

#[test]
fn fresh_join_gate_preserves_source_stopped_or_driver_override_policy() {
    use openpilot_driving_modeld::recovery::{may_join, retryable, retryable_preparation};
    assert!(may_join(10., [true; 3], [9.9; 3], true, 0.009, false));
    assert!(!may_join(10., [true; 3], [9.9; 3], true, 0.01, false));
    assert!(may_join(10., [true; 3], [9.9; 3], false, 15., true));
    for time in [9.75, 10.01, f64::NAN] {
        assert!(!may_join(10., [true; 3], [time, 9.9, 9.9], true, 0., true));
    }
    assert!(!may_join(
        10.,
        [true, false, true],
        [9.9; 3],
        true,
        0.,
        true
    ));
    let runtime = "90970e01f2ba70d553294c0f9d5c6fe1678827f750705cab679a7b19b7bcb0b2";
    assert!(retryable(
        "libusb_bulk_transfer: Input/Output Error",
        runtime
    ));
    assert!(!retryable("precompiled eGPU worker exited", runtime));
    assert!(retryable_preparation(
        "RuntimeError: PCIe link not up (LTSSM=0xab)",
        runtime
    ));
    for value in [
        "PCIe link not up (LTSSM=0x)",
        "prefix PCIe link not up (LTSSM=0x1)",
        "PCIe link not up (LTSSM=0x1) suffix",
        "invalid precompiled model output",
    ] {
        assert!(!retryable_preparation(value, runtime), "{value}");
    }
}

#[test]
fn persisted_budget_and_shared_status_match_source_without_launching_candidate() {
    use openpilot_driving_modeld::{
        recovery::Recovery, recovery_policy::State, recovery_prepare::Preparation,
    };
    use std::fs;
    let owned = tempfile::tempdir().unwrap();
    let model = owned.path().join("model.pkl");
    let prepare = Preparation {
        model,
        worker: owned.path().join("never-run"),
        assets: owned.path().to_owned(),
        camera: [1344, 760],
        identity_file: owned.path().join("installed.json"),
        identity: serde_json::json!({"format":"comma-generic-onnx"}),
        asset_manifest_sha256: "a".repeat(64),
    };
    let status = owned.path().join("status.json");
    let budget = owned.path().join("budget.json");
    let mut recovery = Recovery::at(prepare.clone(), &status, &budget).unwrap();
    recovery
        .failed(10., "libusb_bulk_transfer: Input/Output Error")
        .unwrap();
    assert_eq!(recovery.policy.state, State::Waiting);
    assert!(recovery.tick(11., false, false).unwrap().is_none());
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&status).unwrap()).unwrap();
    assert_eq!(value["state"], "waiting");
    assert_eq!(value["attempts"], 0);
    assert_eq!(value["limit"], 3);
    assert_eq!(value["updated"], 11.);
    assert!(!budget.exists());
    fs::write(&budget, b"corrupt budget").unwrap();
    let mut recovery = Recovery::at(prepare, &status, &budget).unwrap();
    recovery
        .failed(12., "libusb_bulk_transfer: Input/Output Error")
        .unwrap();
    assert_eq!(recovery.policy.state, State::Stopped);
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&budget).unwrap()).unwrap();
    assert_eq!(value, serde_json::json!({"attempts":3,"stopped":true}));
}
