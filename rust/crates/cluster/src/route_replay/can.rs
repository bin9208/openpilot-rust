use super::{corner, Error, Parser};
use crate::{live::projection::Data, models::DetectedVehicle};
use capnp::dynamic_value::Reader;
use openpilot_can::dbc::Dbc;
use std::{collections::BTreeMap, sync::OnceLock};

const DBC_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../opendbc_repo/opendbc/dbc/generator/hyundai/hyundai_canfd.dbc"
));
static DBC: OnceLock<Result<Dbc, String>> = OnceLock::new();

impl Parser {
    pub(super) fn can(
        &mut self,
        messages: Data<'_>,
        time: f64,
        service: &str,
    ) -> Result<(), Error> {
        if self.projection.replay_front_only() {
            return Ok(());
        }
        for message in messages.items() {
            let Some(address) = message
                .field("address")
                .uint()
                .and_then(|value| u32::try_from(value).ok())
            else {
                continue;
            };
            let bus = message.field("src").int().unwrap_or(-1);
            if service == "can" && bus >= 0x80 {
                continue;
            }
            let Reader::Data(bytes) = message.field("dat").0 else {
                continue;
            };
            if service == "can" && bus == 1 {
                for object in corner::decode_raw(time, address, bytes) {
                    let key = (object.group.clone(), object.slot);
                    if object.valid() {
                        self.raw_tracker.update(&object)?;
                        self.raw_objects.insert(key, object);
                        self.projection.observe_replay_raw_corner();
                    } else {
                        self.raw_objects.remove(&key);
                    }
                }
            }
            if !matches!(address, 0x162 | 0x1ea)
                || service == "sendcan"
                || bus < 0
                || bus % 4 != 2
                || bytes.len() < 24
            {
                continue;
            }
            self.projection
                .observe_replay_corner(address, decode_corner(address, bytes)?, time);
        }
        Ok(())
    }
}

fn decode_corner(address: u32, bytes: &[u8]) -> Result<Vec<DetectedVehicle>, Error> {
    let dbc = DBC
        .get_or_init(|| Dbc::parse("hyundai_canfd", DBC_SOURCE).map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|error| Error::Invalid(error.clone()))?;
    let mut values = BTreeMap::new();
    if let Some(message) = dbc.messages.get(&address) {
        for signal in &message.signals {
            if !signal.name.contains("_DETECT") {
                continue;
            }
            let value = signal
                .raw(bytes)
                .and_then(|raw| signal.physical(&raw))
                .map_err(|error| Error::Invalid(error.to_string()))?;
            values.insert(signal.name.as_str(), value);
        }
    }
    let mut detections = Vec::new();
    for (label, side, forward) in [
        ("LF", -1.0, 1.0),
        ("RF", 1.0, 1.0),
        ("LR", -1.0, -1.0),
        ("RR", 1.0, -1.0),
    ] {
        let value = |suffix| {
            values
                .get(format!("{label}_{suffix}").as_str())
                .copied()
                .unwrap_or(0.0)
        };
        let distance = value("DETECT_DISTANCE");
        if value("DETECT") == 0.0 || distance <= 0.2 || distance >= 180.0 {
            continue;
        }
        let lateral = value("DETECT_LATERAL");
        detections.push(DetectedVehicle {
            label: label.into(),
            longitudinal_m: distance * forward,
            lateral_m: side
                * if (0.4..=6.0).contains(&lateral) {
                    lateral
                } else {
                    3.0
                },
            source: format!("CAN 0x{address:x}"),
            ..DetectedVehicle::default()
        });
    }
    Ok(detections)
}
