use openpilot_can::Frame;
use openpilot_card::{
    firmware::{Brand, Catalog, Firmware},
    firmware_query::StartupIo,
    identification::{CachedParams, FingerprintSource, IdentifyOptions},
    isotp::Error,
    query::QueryIo,
};
use std::collections::VecDeque;

struct Io {
    batches: VecDeque<Vec<Vec<Frame>>>,
    sent: Vec<Frame>,
    obd: Vec<bool>,
    now: f64,
}
impl Io {
    fn passive() -> Self {
        Self {
            batches: VecDeque::from([vec![], vec![vec![]; 202]]),
            sent: vec![],
            obd: vec![],
            now: 0.,
        }
    }
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
fn manual_selection_skips_active_queries_and_overrides_environment_fingerprint() {
    let catalog = Catalog::load().unwrap();
    let (choice, candidate) = &catalog.selected[0];
    let options = IdentifyOptions {
        selected_car: Some(choice),
        fixed_fingerprint: "MOCK",
        ..IdentifyOptions::default()
    };
    let mut io = Io::passive();
    let result = catalog.identify(options, None, &mut io).unwrap();
    assert_eq!(result.candidate, *candidate);
    assert_eq!(result.source, FingerprintSource::Fixed);
    assert!(result.exact_match);
    assert!(io.sent.is_empty());
    assert_eq!(io.obd, [false]);
    assert_eq!(result.packets, 202);
}

#[test]
fn firmware_cache_preserves_candidate_and_sanitizes_malformed_vin() {
    let catalog = Catalog::load().unwrap();
    let model = catalog
        .models
        .iter()
        .find(|model| model.brand == Brand::Body)
        .unwrap();
    let firmware = model
        .firmware
        .iter()
        .map(|ecu| Firmware {
            ecu: ecu.ecu,
            address: ecu.address,
            sub_address: ecu.subaddress.unwrap_or(0),
            brand: "body".to_owned(),
            fw_version: ecu.versions[0].clone(),
            ..Firmware::default()
        })
        .collect();
    let cache = CachedParams {
        brand: "body".to_owned(),
        vin: "invalid".to_owned(),
        firmware,
    };
    let mut io = Io::passive();
    let result = catalog
        .identify(IdentifyOptions::default(), Some(&cache), &mut io)
        .unwrap();
    assert_eq!(result.candidate, model.name);
    assert_eq!(result.vin, "00000000000000000");
    assert_eq!(result.source, FingerprintSource::Fw);
    assert!(result.cached);
    assert!(io.sent.is_empty());
    assert_eq!(io.obd, [false]);
}
