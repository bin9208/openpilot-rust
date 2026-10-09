//! CVS1/CVB1 display fields from carrot/realtime/compact_state.py.
use crate::Error;
pub(super) use openpilot_carrot_state::batch;
pub use openpilot_carrot_state::services;

pub fn interval_ms(service: &str) -> u64 {
    match service {
        "carState" => 16,
        "controlsState" | "carControl" => 30,
        "liveTracks" | "carrotMan" => 100,
        "selfdriveState" => 200,
        "roadCameraState"
        | "liveCalibration"
        | "liveParameters"
        | "liveTorqueParameters"
        | "liveDelay" => 250,
        "carrotNavi" | "deviceState" | "peripheralState" | "gpsLocationExternal" => 500,
        _ => 50,
    }
}

pub fn encode(service: &str, bytes: &[u8], sequence: u16) -> Result<Vec<u8>, Error> {
    openpilot_carrot_state::encode(service, bytes, sequence)
        .map_err(|error| Error::Source(error.to_string()))
}
