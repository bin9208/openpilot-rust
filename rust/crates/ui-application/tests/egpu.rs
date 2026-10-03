use openpilot_ui_application::{
    services::egpu::{native::Native, Backend, Check},
    state::SlowParams,
    Error,
};
use openpilot_usbgpu::{check::Options, model::Paths};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

struct Pending(AtomicUsize);
impl Backend for Pending {
    fn status(&self, _: &SlowParams) -> Result<String, Error> {
        Ok(String::new())
    }
    fn link(&self) -> Result<String, Error> {
        Ok(String::new())
    }
    fn check(&self, cancelled: &AtomicBool) -> Result<Option<String>, Error> {
        self.0.fetch_add(1, Ordering::Relaxed);
        while !cancelled.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(1));
        }
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(None)
    }
    fn remove_compiled_manifest(&self) -> Result<(), Error> {
        Ok(())
    }
}

#[test]
fn pending_check_deduplicates_and_drop_cancels_then_joins() -> Result<(), Error> {
    let backend = Arc::new(Pending(AtomicUsize::new(0)));
    let mut check = Check::new(backend.clone());
    check.start()?;
    check.start()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while backend.0.load(Ordering::Relaxed) == 0 {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(check.running());
    assert!(!check.poll()?);
    drop(check);
    assert_eq!(backend.0.load(Ordering::Relaxed), 2);
    Ok(())
}

#[test]
fn native_facade_reads_owned_sysfs_and_runs_owned_probe() -> Result<(), Box<dyn std::error::Error>>
{
    let temp = tempfile::tempdir()?;
    let devices = temp.path().join("devices");
    let device = devices.join("owned-device");
    fs::create_dir_all(&device)?;
    for (file, text) in [
        ("idVendor", "add1"),
        ("idProduct", "0001"),
        ("speed", "5000"),
        ("product", "custom ed4e39b7-CLEAN"),
    ] {
        fs::write(device.join(file), text)?;
    }
    let probe = temp.path().join("probe");
    fs::write(&probe, "#!/bin/sh\nexit 0\n")?;
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o700))?;
    let native = Arc::new(Native {
        options: Options {
            devices,
            probe,
            timeout: Duration::from_secs(2),
            require_clean_link: true,
        },
        models: Paths {
            models: temp.path().join("models"),
            cache: temp.path().join("cache"),
        },
    });
    assert_eq!(native.link()?, "5 Gbps");
    let state = SlowParams {
        usbgpu_active: true,
        usbgpu_startup_failed: true,
        ..Default::default()
    };
    assert_eq!(native.status(&state)?, "startup failed");
    let mut check = Check::new(native);
    check.start()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while !check.poll()? {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!check.running());
    assert_eq!(check.result, None);
    Ok(())
}
