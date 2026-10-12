use openpilot_usbgpu::{
    bus_lock::BusLock,
    clock::WallClock,
    controller::Controller,
    pci::Config,
    transport::{BulkResult, Control, Description, Setup, Transfer, Transport},
    usb3::Usb3,
    Error,
};
#[derive(Default)]
struct Usb {
    requests: Vec<(Control, Vec<u8>)>,
    reads: usize,
}
impl Transport for Usb {
    fn describe(&self) -> Result<Description, Error> {
        Ok(Description {
            bus: 1,
            address: 1,
            product: b"custom fixture".to_vec(),
        })
    }
    fn setup(&mut self, _: Setup, _: i32, _: i32) -> Result<i32, Error> {
        Ok(0)
    }
    fn streams(&mut self, _: &[u8], _: u32) -> Result<i32, Error> {
        unreachable!()
    }
    fn control(&mut self, c: Control, data: &mut [u8]) -> Result<i32, Error> {
        self.requests.push((c, data.to_vec()));
        if c.request == 0xe4 {
            data[0] = 0x78;
        } else if c.kind == 0xc0 {
            let value = if self.reads == 0 {
                0x11223344u32
            } else {
                0x55667788
            };
            data[..4].copy_from_slice(&value.to_le_bytes());
            self.reads += 1;
        }
        Ok(data.len() as i32)
    }
    fn bulk(&mut self, _: u8, _: &mut [u8], _: u32) -> Result<BulkResult, Error> {
        unreachable!()
    }
    fn batch(&mut self, _: &mut [Transfer]) -> Result<(), Error> {
        unreachable!()
    }
    fn error_text(&self, _: i32) -> String {
        "fixture".into()
    }
}
#[test]
fn scalar64_and_config_requests_keep_original_wire_order() {
    let temp = tempfile::tempdir().unwrap();
    let usb = Usb3::new(
        Usb::default(),
        WallClock::default(),
        BusLock::open(&temp.path().join("lock")).unwrap(),
        false,
    )
    .unwrap();
    let mut controller = Controller::new(usb).unwrap();
    assert_eq!(
        controller.read_scalar(0x12340000, 8).unwrap(),
        0x1122334455667788
    );
    controller
        .write_scalar(0x12340000, 8, 0x8877665544332211)
        .unwrap();
    controller.write_config(4, 0x18, 4, 0x1234).unwrap();
    let Controller::Custom(custom) = controller else {
        panic!("wrong firmware controller");
    };
    let requests = custom.usb.transport.requests;
    let out = requests
        .iter()
        .filter(|(c, _)| c.request == 0xf0 && c.kind == 0x40)
        .collect::<Vec<_>>();
    let addresses = out
        .iter()
        .map(|(_, data)| u64::from_le_bytes(data[..8].try_into().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        addresses,
        [0x12340004, 0x12340000, 0x12340004, 0x12340000, 0x04000018]
    );
    assert_eq!(
        u32::from_le_bytes(out[2].1[8..].try_into().unwrap()),
        0x88776655
    );
    assert_eq!(
        u32::from_le_bytes(out[3].1[8..].try_into().unwrap()),
        0x44332211
    );
    assert_eq!(out[4].0.value, 0x0f45);
}
