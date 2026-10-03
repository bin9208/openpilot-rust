mod console;
mod events;
mod float_text;
mod printable_ranges;
use crate::{
    fingerprint,
    firmware::{Catalog, Firmware, MatchOptions},
    firmware_query::{self, StartupIo},
    isotp,
    vin::{self, VinConfig},
};
pub use events::Event;
use openpilot_can::Packet;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[repr(u16)]
#[serde(rename_all = "snake_case")]
pub enum FingerprintSource {
    Can = 0,
    Fw = 1,
    Fixed = 2,
}

pub struct IdentifyOptions<'a> {
    pub fixed_fingerprint: &'a str,
    pub selected_car: Option<&'a str>,
    pub skip_fw_query: bool,
    pub disable_fw_cache: bool,
    pub pandas: usize,
}
impl Default for IdentifyOptions<'_> {
    fn default() -> Self {
        Self {
            fixed_fingerprint: "",
            selected_car: None,
            skip_fw_query: false,
            disable_fw_cache: false,
            pandas: 1,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CachedParams {
    pub brand: String,
    pub vin: String,
    pub firmware: Vec<Firmware>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Identification {
    pub candidate: String,
    pub observed: Vec<(u8, Vec<(u32, usize)>)>,
    pub vin: String,
    pub firmware: Vec<Firmware>,
    pub source: FingerprintSource,
    pub exact_match: bool,
    pub cached: bool,
    pub vin_rx_address: Option<i64>,
    pub vin_rx_bus: Option<u8>,
    pub ecu_responses: Vec<crate::ecu::EcuAddress>,
    pub fw_query_time: f64,
    pub packets: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Transport(#[from] isotp::Error),
    #[error(transparent)]
    FirmwareQuery(#[from] firmware_query::Error),
    #[error(transparent)]
    Match(#[from] crate::firmware::Error),
    #[error(transparent)]
    Catalog(#[from] serde_json::Error),
}

impl Catalog {
    pub fn identify(
        &self,
        options: IdentifyOptions<'_>,
        cache: Option<&CachedParams>,
        io: &mut impl StartupIo,
    ) -> Result<Identification, Error> {
        let selected = options
            .selected_car
            .and_then(|name| self.selected_platform(name));
        let skip_query =
            options.skip_fw_query || !options.fixed_fingerprint.is_empty() || selected.is_some();
        let start = io.now();
        let mut vin = vin::UNKNOWN.to_owned();
        let mut firmware = Vec::new();
        let mut vin_rx_address = None;
        let mut vin_rx_bus = None;
        let mut ecu_responses = Vec::new();
        let mut cached = false;
        let mut firmware_candidates = std::collections::BTreeSet::new();
        let mut exact_fw_match = true;
        if !skip_query {
            let cache = cache.filter(|cache| {
                cache.brand != "mock" && !cache.firmware.is_empty() && !options.disable_fw_cache
            });
            match cache {
                Some(cache) => {
                    io.log(
                        crate::query::DiagnosticLevel::Warning,
                        "Using cached CarParams",
                    );
                    // A Cap'n Proto getter returns a distinct Python string even for VIN_UNKNOWN.
                    vin.clone_from(&cache.vin);
                    firmware.clone_from(&cache.firmware);
                    cached = true;
                }
                None => {
                    io.log(
                        crate::query::DiagnosticLevel::Warning,
                        "Getting VIN & FW versions",
                    );
                    io.set_obd_multiplexing(true)?;
                    let result = vin::query(
                        VinConfig {
                            buses: &[0, 1],
                            timeout: 0.1,
                            retry: 2,
                        },
                        io,
                    );
                    vin = result.vin;
                    vin_rx_address = result.address;
                    vin_rx_bus = result.bus;
                    ecu_responses = self.present_ecus(options.pandas, io)?;
                    firmware =
                        self.query_ordered(&ecu_responses, (&vin, options.pandas, 0.1), io)?;
                }
            }
            let result = self.match_car(
                &firmware,
                &vin,
                MatchOptions {
                    exact: true,
                    fuzzy: true,
                },
            )?;
            firmware_candidates = result.candidates;
            exact_fw_match = result.exact;
        } else {
            io.log(
                crate::query::DiagnosticLevel::Warning,
                "Skipping VIN & FW query",
            );
        }
        if !vin::valid(&vin) {
            io.identification_event(Event::MalformedVin { vin: &vin });
            vin = vin::UNKNOWN.to_owned();
        }
        io.log(
            crate::query::DiagnosticLevel::Warning,
            &format!("VIN {vin}"),
        );
        io.set_obd_multiplexing(false)?;
        let fw_query_time = io.now() - start;
        io.receive(false)?;
        let mut passive = fingerprint::Fingerprint::new(fingerprint::catalog()?);
        while !passive.done {
            let packets: Vec<_> = io
                .receive(true)?
                .into_iter()
                .map(|frames| Packet {
                    mono_time: 0,
                    frames,
                })
                .collect();
            passive.observe(&packets);
        }
        let mut candidate = passive.selected;
        let mut source = FingerprintSource::Can;
        let mut exact_match = true;
        if firmware_candidates.len() == 1 {
            candidate = firmware_candidates.into_iter().next();
            source = FingerprintSource::Fw;
            exact_match = exact_fw_match;
        }
        if !options.fixed_fingerprint.is_empty() {
            candidate = Some(options.fixed_fingerprint.to_owned());
            source = FingerprintSource::Fixed;
        }
        io.identification_event(Event::Fingerprinted {
            car_fingerprint: candidate.as_deref().unwrap_or("None"),
            source: source.code(),
            fuzzy: !exact_match,
            cached,
            fw_count: firmware.len(),
            ecu_responses: &ecu_responses,
            vin_rx_addr: vin_rx_address.unwrap_or(-1),
            vin_rx_bus: vin_rx_bus.map(i16::from).unwrap_or(-1),
            fingerprints: fingerprint::source_repr(&passive.observed),
            fw_query_time,
        });
        if let Some(selected) = selected {
            candidate = Some(selected.to_owned());
            source = FingerprintSource::Fixed;
            exact_match = true;
        }
        if candidate.is_none() {
            io.identification_event(Event::Unmatched {
                fingerprints: fingerprint::source_repr(&passive.observed),
            });
        }
        Ok(Identification {
            candidate: candidate.unwrap_or_else(|| "MOCK".to_owned()),
            observed: passive.observed,
            vin,
            firmware,
            source,
            exact_match,
            cached,
            vin_rx_address,
            vin_rx_bus,
            ecu_responses,
            fw_query_time,
            packets: passive.frames,
        })
    }
}
