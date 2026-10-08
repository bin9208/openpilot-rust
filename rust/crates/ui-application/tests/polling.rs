use openpilot_ui_application::services::polling::{Gate, Poller};
use std::{
    sync::{mpsc, Arc},
    time::Duration,
};
#[test]
fn offroad_awake_gate_and_stop_release_owned_worker() -> Result<(), Box<dyn std::error::Error>> {
    let gate = Arc::new(Gate::default());
    gate.update(true, true);
    let (sender, receiver) = mpsc::channel();
    let mut worker = Poller::start(gate.clone(), Duration::from_millis(20), move || {
        let _ = sender.send(());
    })?;
    assert!(receiver.recv_timeout(Duration::from_millis(75)).is_err());
    gate.update(false, false);
    assert!(receiver.recv_timeout(Duration::from_millis(75)).is_err());
    gate.update(false, true);
    receiver.recv_timeout(Duration::from_secs(1))?;
    gate.update(true, true);
    std::thread::sleep(Duration::from_millis(30));
    while receiver.try_recv().is_ok() {}
    assert!(receiver.recv_timeout(Duration::from_millis(75)).is_err());
    worker.stop();
    assert_eq!(
        receiver.recv_timeout(Duration::from_millis(75)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    );
    worker.stop();
    Ok(())
}
