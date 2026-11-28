//! Optional receive-only APP_trafficControl; never decodes legacy DAS_road
//! as APP. Port of carrot's `opendbc/car/tesla/traffic_control.py`.
use super::Error;
use openpilot_can::{dbc::Dbc, parser::Parser, Frame, Packet};
use openpilot_cereal::car_capnp::car_state::tesla_traffic_control::{self, Light};
use std::{path::Path, sync::Arc};

pub const MAX_AGE_NS: u64 = 500_000_000;
/// `CANBUS.autopilot_party`.
pub const BUS: u8 = 2;
pub const ADDRESS: u32 = 0x25d;

pub struct TrafficControl {
    parser: Parser,
    sample: u64,
    available: bool,
    valid: bool,
    distance: f64,
    light: i64,
    status: &'static str,
    payload: Option<Vec<u8>>,
}

impl TrafficControl {
    pub fn new(root: &Path, now: u64) -> Result<Self, Error> {
        let dbc = Arc::new(Dbc::load(&root.join("tesla_modely_hw4_perception.dbc"))?);
        let mut parser = Parser::new(dbc, BUS, now);
        parser.add("APP_trafficControl", Some(f64::NAN), false, now)?;
        Ok(Self {
            parser,
            sample: 0,
            available: false,
            valid: false,
            distance: 0.,
            light: 0,
            status: "waiting_for_app_traffic_control",
            payload: None,
        })
    }

    pub fn observe(&mut self, data: &[u8], source: u8, sample: u64, now: u64) -> Result<(), Error> {
        if source != BUS {
            return Ok(());
        }
        if !(0 < sample && sample <= now) || now - sample > MAX_AGE_NS || sample < self.sample {
            return Ok(());
        }
        if sample == self.sample {
            if self.payload.as_deref() != Some(data) {
                self.valid = false;
                self.status = "conflicting_timestamp";
            }
            return Ok(());
        }
        self.available = true;
        self.sample = sample;
        self.payload = Some(data.to_vec());
        self.valid = false;
        self.light = 0;
        if data.len() != 8 {
            self.status = "unsupported_dlc";
            return Ok(());
        }
        self.parser.update(&[Packet {
            mono_time: sample,
            frames: vec![Frame {
                address: ADDRESS,
                data: data.to_vec(),
                bus: source,
            }],
        }])?;
        let get = |p: &Parser, name: &str| p.signal("APP_trafficControl", name);
        self.distance = get(&self.parser, "APP_tcControlDistance")?;
        self.light = get(&self.parser, "APP_tcControlLightState")? as i64;
        self.valid = get(&self.parser, "APP_tcControlType")? == 3.
            && matches!(get(&self.parser, "APP_tcFeatureState")? as i64, 2 | 3)
            && matches!(get(&self.parser, "APP_tcControlSource")? as i64, 2 | 3)
            && get(&self.parser, "APP_tcVisionLight")? == 1.
            && get(&self.parser, "APP_tcUnavailableReason")? == 0.
            && get(&self.parser, "APP_tcWarningSuppressionReason")? == 0.
            && matches!(self.light, 1..=3)
            && (0. ..255.).contains(&self.distance);
        self.status = if self.valid {
            "app_traffic_control"
        } else {
            "invalid_app_traffic_control"
        };
        Ok(())
    }

    pub fn publish(&self, mut target: tesla_traffic_control::Builder<'_>, now: u64) {
        let fresh = 0 < self.sample && self.sample <= now && now - self.sample <= MAX_AGE_NS;
        let valid = self.valid && fresh;
        target.set_available(self.available);
        target.set_valid(valid);
        target.set_sample_mono_time(self.sample);
        target.set_stop_line_distance(self.distance as f32);
        target.set_light(match self.light {
            1 => Light::Red,
            2 => Light::Green,
            3 => Light::Yellow,
            _ => Light::Unknown,
        });
        target.set_status(if !self.valid || valid {
            self.status
        } else {
            "stale"
        });
    }
}
