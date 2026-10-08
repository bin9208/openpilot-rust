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
struct Fixture {
    calls: Calls,
    kind: Vec<u8>,
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
        unreachable!()
    }
    fn file_read(&mut self, _: &Path, _: Option<usize>) -> Result<Vec<u8>, Fault> {
        unreachable!()
    }
    fn log(&mut self, _: Level, _: String) -> Result<(), Fault> {
        Ok(())
    }
    fn sleep(&mut self, _: f64) -> Result<(), Fault> {
        unreachable!()
    }
    fn monotonic(&mut self) -> Result<f64, Fault> {
        unreachable!()
    }
    fn dfu_list(&mut self) -> Result<Vec<Option<String>>, Fault> {
        unreachable!()
    }
    fn dfu_recover(&mut self, _: Option<&str>) -> Result<(), Fault> {
        unreachable!()
    }
}

#[test]
fn connect_preserves_checks_reset_and_all_three_can_speeds() {
    let calls = Calls::default();
    let client = Client::open(
        Fixture {
            calls: calls.clone(),
            kind: vec![9],
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
        },
        "serial".into(),
    );
    assert!(result.is_err());
    assert_eq!(*calls.borrow(), [(0xc0, 0xc1, 0, 0), (0xc0, 0xc1, 0, 0)]);
}
