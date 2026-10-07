use openpilot_can::Frame;
use openpilot_card::{
    ecu::{scan, EcuAddress, ScanConfig},
    isotp::Error,
    query::QueryIo,
    vin::{decode, valid},
};
use std::collections::VecDeque;

#[derive(Default)]
struct Io {
    batches: VecDeque<Vec<Vec<Frame>>>,
    sent: Vec<Frame>,
    now: f64,
}
impl QueryIo for Io {
    fn receive(&mut self, _: bool) -> Result<Vec<Vec<Frame>>, Error> {
        Ok(self.batches.pop_front().unwrap_or_default())
    }
    fn send(&mut self, frames: &[Frame]) -> Result<(), Error> {
        self.sent.extend_from_slice(frames);
        Ok(())
    }
    fn sleep(&mut self, _: f64) -> Result<(), Error> {
        Ok(())
    }
    fn now(&mut self) -> f64 {
        self.now += 0.001;
        self.now
    }
}

#[test]
fn vin_decoding_preserves_honda_length_and_ford_padding() {
    let vin = "1HGCM82633A004352";
    assert_eq!(
        decode(b"\0\xff 1hgcm82633a004352\x1c\0\xff"),
        Some(vin.to_owned())
    );
    assert_eq!(
        decode(b"\x111HGCM82633A004352trailing"),
        Some(vin.to_owned())
    );
    assert!(valid(vin));
    assert!(!valid("1HGCM82633A00435I"));
    assert_eq!(decode(b"1HGCM82633A00435\xff"), None);
}

#[test]
fn ecu_scan_filters_bus_subaddress_and_protocol_response() {
    let target = EcuAddress(0x700, Some(0x10), 1);
    let expected = EcuAddress(0x708, Some(0x10), 1);
    let mut io = Io {
        batches: VecDeque::from([
            vec![],
            vec![vec![
                Frame {
                    address: 0x708,
                    data: vec![0x10, 2, 0x7e, 0],
                    bus: 0,
                },
                Frame {
                    address: 0x708,
                    data: vec![0x20, 2, 0x7e, 0],
                    bus: 1,
                },
                Frame {
                    address: 0x708,
                    data: vec![0x10, 3, 0x7f, 0x3e, 0x12],
                    bus: 1,
                },
            ]],
        ]),
        ..Io::default()
    };
    let observed = scan(
        ScanConfig {
            queries: &[target],
            responses: &[expected],
            timeout: 0.004,
        },
        &mut io,
    );
    assert_eq!(observed, vec![expected]);
    assert_eq!(io.sent[0].address, 0x700);
    assert_eq!(io.sent[0].data, [0x10, 2, 0x3e, 0, 0, 0, 0, 0]);
    assert_eq!(io.sent[0].bus, 1);
}
