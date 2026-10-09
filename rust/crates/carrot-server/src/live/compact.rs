//! CVS1/CVB1 display fields from carrot/realtime/compact_state.py.
use super::{compact_schema::SERVICES, value};
use crate::Error;
pub fn services() -> impl Iterator<Item = &'static str> {
    SERVICES.iter().map(|(name, _, _)| *name)
}
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
    let (_, id, schema) = SERVICES
        .iter()
        .find(|(name, _, _)| *name == service)
        .ok_or_else(|| Error::Source("unknown compact service".into()))?;
    let message = value::message(bytes)?;
    let value = value::service(&message, service)?;
    let mut out = Vec::new();
    out.extend(b"CVS1");
    out.extend([*id, 0]);
    out.extend(sequence.to_le_bytes());
    super::compact_fields::fields(&mut out, value, schema)?;
    Ok(out)
}
pub(super) fn batch(frames: impl IntoIterator<Item = Vec<u8>>) -> Vec<u8> {
    let frames: Vec<_> = frames
        .into_iter()
        .filter(|frame| !frame.is_empty())
        .collect();
    let mut out = Vec::new();
    out.extend(b"CVB1");
    out.extend(u16::try_from(frames.len()).unwrap_or(0).to_le_bytes());
    for frame in frames {
        out.extend(u32::try_from(frame.len()).unwrap_or(0).to_le_bytes());
        out.extend(frame);
    }
    out
}
