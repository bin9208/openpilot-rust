use openpilot_pandad::{
    firmware::{
        client::{Client, Connection, Environment, Handle, Level},
        Request, Transport,
    },
    supervisor::Fault,
};
use std::{cell::RefCell, path::Path, rc::Rc};

type Calls = Rc<RefCell<Vec<(u8, u8, u16, u16)>>>;
struct Device {
    calls: Calls,
    kind: Vec<u8>,
}
impl Transport for Device {
    type Error = Fault;
    fn control_read(&mut self, request: Request, _: usize) -> Result<Vec<u8>, Fault> {
        self.calls
            .borrow_mut()
            .push((request.kind, request.request, request.value, request.index));
        Ok(if request.request == 0xc1 {
            self.kind.clone()
        } else if matches!(request.request, 0xd3 | 0xd4) {
            vec![0; 64]
        } else {
            vec![0; 3]
        })
    }
    fn control_write(&mut self, request: Request, _: &[u8]) -> Result<(), Fault> {
        self.calls
            .borrow_mut()
            .push((request.kind, request.request, request.value, request.index));
        Ok(())
    }
    fn bulk_write(&mut self, _: u8, _: &[u8], _: u32) -> Result<(), Fault> {
        unreachable!()
    }
}
impl Handle for Device {
    fn close(&mut self) -> Result<(), Fault> {
        Ok(())
    }
}
#[derive(Default)]
struct Fixture {
    calls: Calls,
    kind: Vec<u8>,
    files: Rc<RefCell<Vec<String>>>,
    missing: bool,
}
impl Environment for Fixture {
    type Device = Device;
    fn usb_connect(
        &mut self,
        serial: &str,
        _: bool,
        _: bool,
    ) -> Result<Option<Connection<Device>>, Fault> {
        Ok(Some(Connection {
            handle: Device {
                calls: self.calls.clone(),
                kind: self.kind.clone(),
            },
            serial: serial.into(),
            bootstub: false,
            bcd: None,
            spi: false,
        }))
    }
    fn spi_connect(&mut self, _: &str) -> Result<Option<Connection<Device>>, Fault> {
        unreachable!()
    }
    fn firmware_dir(&self) -> &Path {
        Path::new("/firmware")
    }
    fn file_exists(&mut self, _: &Path) -> Result<bool, Fault> {
        Ok(!self.missing)
    }
    fn file_read(&mut self, path: &Path, _: Option<usize>) -> Result<Vec<u8>, Fault> {
        self.files.borrow_mut().push(path.display().to_string());
        if path
            .file_name()
            .is_some_and(|name| name == "panda_h7_tesla_wake.bin.signed")
        {
            Ok(vec![0; 128])
        } else {
            Err(Fault::Other("wrong firmware variant".into()))
        }
    }
    fn log(&mut self, _: Level, _: String) -> Result<(), Fault> {
        Ok(())
    }
    fn sleep(&mut self, _: f64) -> Result<(), Fault> {
        unreachable!()
    }
    fn monotonic(&mut self) -> Result<f64, Fault> {
        Ok(0.0)
    }
    fn dfu_list(&mut self) -> Result<Vec<Option<String>>, Fault> {
        Ok(vec![openpilot_pandad::firmware::dfu_serial(
            "010002000300040005000600",
            openpilot_pandad::firmware::Mcu::H7,
        )
        .unwrap()])
    }
    fn dfu_recover(&mut self, _: Option<&str>) -> Result<(), Fault> {
        Ok(())
    }
}

#[test]
fn connect_preserves_checks_reset_and_all_three_can_speeds() {
    let calls = Calls::default();
    let client = Client::open(
        Fixture {
            calls: calls.clone(),
            kind: vec![9],
            ..Fixture::default()
        },
        "serial".into(),
    )
    .unwrap();
    assert_eq!(
        *calls.borrow(),
        [
            (0xc0, 0xc1, 0, 0),
            (0xc0, 0xc1, 0, 0),
            (0xc0, 0xdd, 0, 0),
            (0x40, 0xf8, 0, 0),
            (0x40, 0xe7, 0, 0),
            (0x40, 0xc0, 0, 0),
            (0x40, 0xde, 0, 5000),
            (0x40, 0xde, 1, 5000),
            (0x40, 0xde, 2, 5000)
        ]
    );
    assert!(client.connected());
}

#[test]
fn unknown_hardware_fails_before_disabling_checks() {
    let calls = Calls::default();
    let result = Client::open(
        Fixture {
            calls: calls.clone(),
            kind: vec![4],
            ..Fixture::default()
        },
        "serial".into(),
    );
    assert!(result.is_err());
    assert_eq!(*calls.borrow(), [(0xc0, 0xc1, 0, 0), (0xc0, 0xc1, 0, 0)]);
}

#[test]
fn selected_wake_identity_survives_dfu_recovery_and_reconnect() {
    let files = Rc::new(RefCell::new(Vec::new()));
    let mut client = Client::open(
        Fixture {
            kind: vec![9],
            files: files.clone(),
            ..Fixture::default()
        },
        "010002000300040005000600".into(),
    )
    .unwrap();
    client.select_tesla_wake(true);
    assert!(client.recover_with_timeout(Some(1.0), false).unwrap());
    assert_eq!(
        *files.borrow(),
        ["/firmware/panda_h7_tesla_wake.bin.signed"]
    );
}

#[test]
fn missing_wake_image_fails_before_signature_read_instead_of_using_stock() {
    let files = Rc::new(RefCell::new(Vec::new()));
    let mut client = Client::open(
        Fixture {
            kind: vec![9],
            files: files.clone(),
            missing: true,
            ..Fixture::default()
        },
        "serial".into(),
    )
    .unwrap();
    client.select_tesla_wake(true);
    assert!(client
        .expected_signature()
        .unwrap_err()
        .to_string()
        .contains("Tesla wake firmware is missing"));
    assert!(files.borrow().is_empty());
}
