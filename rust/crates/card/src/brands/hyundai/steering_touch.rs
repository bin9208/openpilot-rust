//! Original ECAN receive profile, separate from torque override and transmit data.
use serde::Serialize;
pub const ADDRESS: u32 = 0x2af;
pub const MESSAGE: &str = "STEER_TOUCH_2AF";
pub const TIMEOUT_NS: u64 = 250_000_000;
pub fn touch_checksum(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for byte in data {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x1d
            } else {
                crc << 1
            };
        }
    }
    crc ^ 0x32
}
#[derive(Default, Serialize)]
pub struct SteeringTouch {
    last_timestamp: u64,
    last_counter: Option<u8>,
    frame_valid: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    pub available: bool,
    pub valid: bool,
    pub touched: bool,
    pub sample_mono_time: u64,
    pub raw_status: u8,
    pub raw_touch1: u8,
    pub raw_touch2: u8,
}
impl SteeringTouch {
    pub fn update(
        &mut self,
        profile: bool,
        timestamp: u64,
        now: u64,
        bus_timeout: bool,
        data: &[u8],
    ) -> Option<Observation> {
        if !profile {
            return None;
        }
        let age = i128::from(now) - i128::from(timestamp);
        let fresh = timestamp > 0 && (0..=i128::from(TIMEOUT_NS)).contains(&age) && !bus_timeout;
        if timestamp != self.last_timestamp {
            let previous_timestamp = self.last_timestamp;
            let previous_counter = self.last_counter;
            self.last_timestamp = timestamp;
            let layout = data.len() == 8
                && data[1] & 0x0f == 0
                && data[1] >> 4 < 15
                && data[2] <= 4
                && data[3] == 1
                && data[6..] == [1, 0];
            let integrity = layout && data[0] == touch_checksum(&data[1..]);
            let counter = integrity.then(|| data[1] >> 4);
            let gap = i128::from(timestamp) - i128::from(previous_timestamp);
            self.frame_valid = fresh
                && integrity
                && previous_counter.is_some()
                && gap > 0
                && gap <= i128::from(TIMEOUT_NS)
                && counter == previous_counter.map(|v| (v + 1) % 15);
            self.last_counter = if fresh { counter } else { None };
        }
        if !fresh {
            self.frame_valid = false;
            self.last_counter = None;
        }
        let valid = fresh && self.frame_valid;
        Some(Observation {
            available: timestamp > 0,
            valid,
            touched: valid && data.get(2).is_some_and(|v| *v >= 1),
            sample_mono_time: timestamp,
            raw_status: if data.len() == 8 { data[2] } else { 0 },
            raw_touch1: if data.len() == 8 { data[4] } else { 0 },
            raw_touch2: if data.len() == 8 { data[5] } else { 0 },
        })
    }
}

impl SteeringTouch {
    pub fn update_parser(
        &mut self,
        parser: &mut openpilot_can::parser::Parser,
    ) -> Result<Option<Observation>, openpilot_can::Error> {
        let profile = parser
            .dbc
            .message(MESSAGE)
            .is_ok_and(|message| message.address == ADDRESS && message.size == 8);
        if !profile {
            return Ok(None);
        }
        if !parser.states.contains_key(&ADDRESS) && parser.seen_addresses.contains(&ADDRESS) {
            parser.add(MESSAGE, Some(f64::NAN), false, parser.last_update)?;
        }
        let timestamp = parser
            .states
            .get(&ADDRESS)
            .and_then(|state| state.timestamps.back().copied())
            .unwrap_or(0);
        let data = parser.raw.get(&ADDRESS).map_or(&[][..], Vec::as_slice);
        Ok(self.update(
            true,
            timestamp,
            parser.last_update,
            parser.bus_timeout(),
            data,
        ))
    }
}
