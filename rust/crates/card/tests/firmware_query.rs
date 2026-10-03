use openpilot_can::Frame;
use openpilot_card::{
    firmware::{Brand, Catalog},
    firmware_query::{QueryOptions, StartupIo},
    isotp::Error,
    query::QueryIo,
};
use std::collections::VecDeque;

#[derive(Default)]
struct Io {
    batches: VecDeque<Vec<Vec<Frame>>>,
    sent: Vec<Frame>,
    obd: Vec<bool>,
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
impl StartupIo for Io {
    fn set_obd_multiplexing(&mut self, enabled: bool) -> Result<(), Error> {
        self.obd.push(enabled);
        Ok(())
    }
}

#[test]
fn unavailable_panda_buses_produce_no_queries_or_firmware() {
    let catalog = Catalog::load().unwrap();
    let mut io = Io::default();
    let versions = catalog
        .query_firmware(
            QueryOptions {
                brand: None,
                pandas: 0,
                timeout: 0.,
            },
            &mut io,
        )
        .unwrap();
    assert!(versions.is_empty());
    assert!(io.sent.is_empty());
    assert!(io.obd.is_empty());
}

#[test]
fn body_query_preserves_firmware_response_record() {
    let catalog = Catalog::load().unwrap();
    let config = catalog
        .brands
        .iter()
        .find(|config| config.brand == Brand::Body)
        .unwrap();
    let expected = &catalog
        .models
        .iter()
        .find(|model| model.brand == Brand::Body)
        .unwrap()
        .firmware[0];
    let request = &config.requests[0];
    let mut data = request.response[1].clone();
    data.push(0x55);
    let mut frame = vec![u8::try_from(data.len()).unwrap()];
    frame.extend_from_slice(&data);
    frame.resize(8, 0);
    let response_address = u32::try_from(i64::from(expected.address) + request.offset).unwrap();
    let mut io = Io {
        batches: VecDeque::from([
            vec![],
            vec![vec![Frame {
                address: response_address,
                data: vec![2, 0x7e, 0, 0, 0, 0, 0, 0],
                bus: request.bus,
            }]],
            vec![vec![Frame {
                address: response_address,
                data: frame,
                bus: request.bus,
            }]],
        ]),
        ..Io::default()
    };
    let versions = catalog
        .query_firmware(
            QueryOptions {
                brand: Some(Brand::Body),
                pandas: 1,
                timeout: 0.1,
            },
            &mut io,
        )
        .unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].fw_version, [0x55]);
    assert_eq!(versions[0].address, expected.address);
    assert_eq!(versions[0].brand, "body");
    assert_eq!(versions[0].request, request.request);
}
