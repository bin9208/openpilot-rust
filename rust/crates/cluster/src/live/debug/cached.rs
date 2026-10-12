use super::Settings;
use crate::models::LiveDebugInfo;

pub(super) fn read(params: &Settings) -> Option<LiveDebugInfo> {
    let delay = params.event("LiveDelay", "liveDelay", |data| {
        Some((
            data.field("calPerc").float(),
            data.field("lateralDelay").float(),
        ))
    });
    let torque = params.event("LiveTorqueParameters", "liveTorqueParameters", |data| {
        Some((
            data.field("calPerc").float(),
            data.field("liveValid").boolean(),
            data.field("latAccelFactorFiltered").float(),
            data.field("frictionCoefficientFiltered").float(),
        ))
    });
    let ratio = params
        .event("LiveParametersV2", "liveParameters", |data| {
            data.field("steerRatio").float()
        })
        .or_else(|| {
            let value = crate::live::json::read(&params.persistent, "LiveParameters")?;
            crate::live::json::finite(&value.get("steerRatio")?)
        });
    let info = LiveDebugInfo {
        live_delay_calibration_percent: delay.and_then(|data| data.0),
        live_delay_lateral_s: delay.and_then(|data| data.1),
        live_torque_calibration_percent: torque.and_then(|data| data.0),
        live_torque_valid: torque.and_then(|data| data.1),
        live_torque_lat_accel_factor: torque.and_then(|data| data.2),
        live_torque_friction: torque.and_then(|data| data.3),
        live_steer_ratio: ratio,
        ..LiveDebugInfo::default()
    };
    present(&info).then_some(info)
}
pub(super) fn present(info: &LiveDebugInfo) -> bool {
    info.live_delay_calibration_percent.is_some()
        || info.live_delay_lateral_s.is_some()
        || info.live_torque_calibration_percent.is_some()
        || info.live_torque_valid.is_some()
        || info.live_torque_lat_accel_factor.is_some()
        || info.live_torque_friction.is_some()
        || info.live_steer_ratio.is_some()
        || info.custom_steer_ratio.is_some()
        || info.steer_actuator_delay_s.is_some()
}
