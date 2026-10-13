use openpilot_cluster::autorun::{gpu::wait_for_usbgpu_startup, params::Reader};
use openpilot_params::Params;
use std::time::{Duration, Instant};

#[test]
fn startup_probe_failure_without_remembered_gpu_releases_display() {
    let temporary = tempfile::tempdir().unwrap();
    let reader = Reader::new(Params::open(temporary.path(), "fixture").unwrap());
    let mut calls = 0;
    wait_for_usbgpu_startup(&reader, || {
        calls += 1;
        Err(openpilot_cluster::Error::Contract("owned probe failure"))
    })
    .unwrap();
    assert_eq!(calls, 1);
}

#[test]
fn loading_to_failed_startup_releases_display_without_stabilization() {
    let temporary = tempfile::tempdir().unwrap();
    let params = Params::open(temporary.path(), "fixture").unwrap();
    params.put_bool("UsbGpuLoading", true).unwrap();
    let reader = Reader::new(params.clone());
    let updater = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(250));
        params.put_bool("UsbGpuLoading", false).unwrap();
        params.put_bool("UsbGpuStartupFailed", true).unwrap();
    });
    let started = Instant::now();
    wait_for_usbgpu_startup(&reader, || Ok(true)).unwrap();
    let elapsed = started.elapsed();
    updater.join().unwrap();
    assert!(elapsed >= Duration::from_millis(200));
    assert!(elapsed < Duration::from_secs(5));
}
