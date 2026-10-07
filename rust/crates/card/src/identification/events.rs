use super::FingerprintSource;
use crate::ecu::EcuAddress;
use serde::Serialize;

#[derive(Serialize)]
#[serde(tag = "event")]
pub enum Event<'a> {
    #[serde(rename = "Malformed VIN")]
    MalformedVin { vin: &'a str },
    #[serde(rename = "fingerprinted")]
    Fingerprinted {
        car_fingerprint: &'a str,
        source: u16,
        fuzzy: bool,
        cached: bool,
        fw_count: usize,
        ecu_responses: &'a [EcuAddress],
        vin_rx_addr: i64,
        vin_rx_bus: i16,
        fingerprints: String,
        fw_query_time: f64,
    },
    #[serde(rename = "car doesn't match any fingerprints")]
    Unmatched { fingerprints: String },
}

impl FingerprintSource {
    pub const fn code(self) -> u16 {
        match self {
            Self::Can => 0,
            Self::Fw => 1,
            Self::Fixed => 2,
        }
    }
}
