use super::{profiles::Profile, state::State};
use crate::Value;
use num_traits::ToPrimitive;

pub(super) fn warnings(state: &mut State, resource: &Value, profile: Profile, mono: f64) -> Value {
    let mut warnings = Vec::new();
    if !state.enabled(mono) {
        return Value::Array(warnings);
    }
    if resource.get("cluster").get("active").truth() {
        warnings.push(Value::text(
            "Cluster HUD is enabled; monitor overall load and temperature during simultaneous use.",
        ));
    }
    if resource.get("carrot_vision").get("active").truth() {
        warnings.push(Value::text("Carrot Vision is enabled; simultaneous streaming increases network and memory bandwidth use."));
    }
    if !resource.get("youtube_encoder").get("running").truth() {
        warnings.push(Value::text(
            "The selected YouTube encoder is waiting to start onroad.",
        ));
    }
    if state.last_frame.mono != 0.0
        && (state.width != u32::from(profile.width) || state.height != u32::from(profile.height))
    {
        warnings.push(Value::text(&format!(
            "YouTube encoder target is {}x{}, but current frames are {}x{}.",
            profile.width, profile.height, state.width, state.height
        )));
    }
    if state.connected && state.started.mono != 0.0 && mono - state.started.mono >= 6.0 {
        let elapsed = (mono - state.started.mono).max(1.0);
        let kbps = (state
            .bytes_sent
            .saturating_sub(state.session_start_bytes)
            .to_f64()
            .unwrap_or(0.0)
            * 8.0
            / 1000.0
            / elapsed)
            .to_u64()
            .unwrap_or(0);
        let required = (f64::from(profile.video_kbps) * 0.55).to_u64().unwrap_or(0);
        if kbps < required {
            warnings.push(Value::text(&format!("YouTube ingest may be starved: current bitrate is below {required} kbps for target {} kbps.", profile.video_kbps)));
        }
    }
    Value::Array(warnings)
}
