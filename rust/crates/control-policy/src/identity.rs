use crate::{math::interp, Error};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Interface {
    Body,
    Chrysler,
    Ford,
    Gm,
    Honda,
    Hyundai,
    Mazda,
    Mock,
    Nissan,
    Psa,
    Rivian,
    Subaru,
    Tesla,
    Toyota,
    Volkswagen,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Accel {
    Base,
    Gm,
    Ford,
    Toyota,
    HondaBosch,
    HondaNidec,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Identity {
    pub fingerprint: String,
    pub interface: Interface,
    pub accel: Accel,
    pub volt_feedforward: bool,
    pub siglin: Option<[f64; 4]>,
}
impl Identity {
    pub fn lookup(fingerprint: &str) -> Result<Self, Error> {
        let entries: Vec<Self> = serde_json::from_str(include_str!("../data/registry.json"))?;
        entries
            .into_iter()
            .find(|entry| entry.fingerprint == fingerprint)
            .ok_or(Error::Contract("unsupported vehicle fingerprint"))
    }
    pub fn accel_limits(&self, flags: u32, speed: f64, cruise: f64) -> Result<[f64; 2], Error> {
        Ok(match self.accel {
            Accel::Base => [-4., 2.5],
            Accel::Gm => [-4., 2.],
            Accel::HondaBosch => [-3.5, 2.],
            Accel::Toyota => [-3.5, if flags & 1024 != 0 { 2. } else { 1.5 }],
            Accel::Ford => [
                -3.5,
                interp(speed, &[cruise - 2., cruise - 0.4], &[2., 0.2])?,
            ],
            Accel::HondaNidec => [
                -4.,
                interp(speed, &[cruise - 2., cruise - 0.2], &[1.6, 0.2])?,
            ],
        })
    }
    pub fn steer_feedforward(&self, angle: f64, speed: f64) -> f64 {
        if self.volt_feedforward {
            let angle = angle * 0.02904609;
            0.10006696 * (angle / (1. + angle.abs())) * (speed + 3.12485927)
        } else {
            angle * speed.powi(2)
        }
    }
}
