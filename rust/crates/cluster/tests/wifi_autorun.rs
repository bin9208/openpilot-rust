use openpilot_cluster::autorun::{
    controller::{self, Platform},
    entry,
    events::Events,
    params::Reader,
    RunRequest,
};
use openpilot_cluster_wifi::{save_config, Registration};
use openpilot_params::Params;
use std::{cell::Cell, path::PathBuf, rc::Rc, time::Duration};
struct Host {
    stop: Rc<Cell<bool>>,
    path: PathBuf,
    late: bool,
    wifi_calls: usize,
    scans: usize,
    discovery_failed: bool,
    startup_shared_usb: Vec<bool>,
}
impl Platform for Host {
    fn interrupted(&self) -> bool {
        self.stop.get()
    }
    fn sleep(&self, _: Duration) -> Result<(), controller::Error> {
        Err(controller::Error::Interrupted)
    }
    fn board(&self) -> bool {
        self.late
    }
    fn offroad(&mut self) -> Result<(), controller::Error> {
        Ok(())
    }
    fn startup(&mut self, _: &Reader) -> Result<(), controller::Error> {
        Ok(())
    }
    fn startup_transport(&mut self, _: &Reader, shared_usb: bool) -> Result<(), controller::Error> {
        self.startup_shared_usb.push(shared_usb);
        Ok(())
    }
    fn discover_wifi(&mut self) -> Result<(), controller::Error> {
        if self.discovery_failed {
            Err(std::io::Error::other("no authenticated S3").into())
        } else {
            Ok(())
        }
    }
    fn wifi_registered(&mut self) -> Result<bool, controller::Error> {
        self.wifi_calls += 1;
        if self.late && self.wifi_calls == 2 {
            pairing(&self.path);
        }
        Registration::load(&self.path)
            .map(|value| value.is_some())
            .map_err(|error| std::io::Error::other(error).into())
    }
    fn find(&mut self, _: u16) -> Option<u16> {
        self.scans += 1;
        None
    }
    fn dim(&mut self, _: u16, _: &str) -> bool {
        false
    }
    fn events(&mut self) -> Events {
        Events::default()
    }
}
struct Entry {
    stop: Rc<Cell<bool>>,
    requests: Vec<RunRequest>,
}
impl entry::Entrypoint for Entry {
    fn run(&mut self, request: RunRequest) -> Result<(), entry::Failure> {
        self.requests.push(request);
        self.stop.set(true);
        Ok(())
    }
}
fn pairing(path: &std::path::Path) {
    save_config(
        &serde_json::json!({"version":1,"device":"001122334455","key":"ab".repeat(32)}),
        path,
    )
    .unwrap();
}
fn parameters(path: &std::path::Path) -> Reader {
    let params = Params::open(path, "wifi-autorun").unwrap();
    for (key, value) in [
        ("ClusterHud", "1"),
        ("ClusterHudDebug", "1"),
        ("ClusterHudEncoder", "1"),
        ("ClusterHudOrientation", "0"),
    ] {
        params.put(key, value.as_bytes()).unwrap();
    }
    Reader::new(params)
}
#[test]
fn registered_wifi_bypasses_desktop_usb_fallback_and_late_registration_rechecks_wait() {
    for late in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pairing.json");
        if !late {
            pairing(&path);
        }
        let stop = Rc::new(Cell::new(false));
        let mut platform = Host {
            stop: stop.clone(),
            path,
            late,
            wifi_calls: 0,
            scans: 0,
            discovery_failed: false,
            startup_shared_usb: Vec::new(),
        };
        let mut entry = Entry {
            stop,
            requests: Vec::new(),
        };
        let result = controller::run(
            &parameters(&directory.path().join("params")),
            &mut platform,
            &mut entry,
        );
        assert!(matches!(result, Err(controller::Error::Interrupted)));
        assert_eq!(entry.requests.len(), 1);
        assert!(entry.requests[0].output.usb());
        assert_eq!(platform.scans, usize::from(late));
        assert_eq!(platform.wifi_calls, if late { 3 } else { 1 });
        assert_eq!(
            platform.startup_shared_usb,
            if late { vec![true, false] } else { vec![false] }
        );
    }
}
#[test]
fn invalid_registration_remains_fatal_before_any_usb_probe_or_child() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pairing.json");
    std::fs::write(&path, "invalid").unwrap();
    let stop = Rc::new(Cell::new(false));
    let mut platform = Host {
        stop: stop.clone(),
        path,
        late: false,
        wifi_calls: 0,
        scans: 0,
        discovery_failed: false,
        startup_shared_usb: Vec::new(),
    };
    let mut entry = Entry {
        stop,
        requests: Vec::new(),
    };
    assert!(matches!(
        controller::run(
            &parameters(&directory.path().join("params")),
            &mut platform,
            &mut entry
        ),
        Err(controller::Error::Io(_))
    ));
    assert_eq!(platform.scans, 0);
    assert!(entry.requests.is_empty());
}

#[test]
fn failed_wifi_discovery_retries_before_model_wait_or_graphics_entry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pairing.json");
    pairing(&path);
    let stop = Rc::new(Cell::new(false));
    let mut platform = Host {
        stop: stop.clone(),
        path,
        late: false,
        wifi_calls: 0,
        scans: 0,
        discovery_failed: true,
        startup_shared_usb: Vec::new(),
    };
    let mut entry = Entry {
        stop,
        requests: Vec::new(),
    };
    assert!(matches!(
        controller::run(
            &parameters(&dir.path().join("params")),
            &mut platform,
            &mut entry
        ),
        Err(controller::Error::Interrupted)
    ));
    assert!(platform.startup_shared_usb.is_empty());
    assert!(entry.requests.is_empty());
    assert_eq!(platform.scans, 0);
}
