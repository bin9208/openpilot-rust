use openpilot_can::Packet;
use openpilot_card::firmware::Firmware;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
pub struct Case {
    pub op: String,
    pub name: String,
    pub candidate: String,
    pub alpha_long: bool,
    pub fingerprints: Vec<(u8, Vec<(u32, usize)>)>,
    pub firmware: Vec<Firmware>,
    pub settings: BTreeMap<String, String>,
    pub now: u64,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub flags_or: u32,
    #[serde(default)]
    pub offset: bool,
}
#[derive(Deserialize)]
pub struct Step {
    pub now: u64,
    pub packets: Vec<Packet>,
    pub control: Vec<u8>,
    pub soft_hold: i16,
    pub commit: Commit,
    pub settings: BTreeMap<String, String>,
}
#[derive(Deserialize)]
pub struct Commit {
    #[serde(rename = "vCruise")]
    pub cruise: f32,
    #[serde(rename = "activateCruise")]
    pub activate: i16,
}
