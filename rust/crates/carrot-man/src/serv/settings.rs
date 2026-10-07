use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Settings {
    pub bump_speed: f64,
    pub bump_time: f64,
    pub bump_end_distance: f64,
    pub control_end: f64,
    pub rear_hold_distance: f64,
    pub control_mode: i64,
    pub vehicle_can_control: i64,
    pub school_control: bool,
    pub camera_control_mode: i64,
    pub safety_factor: f64,
    pub deceleration: f64,
    pub countdown_mode: i64,
    pub turn_speed_mode: i64,
    pub map_turn_factor: f64,
    pub auto_turn_speed: f64,
    pub auto_turn_map_change: i64,
    pub auto_turn_control: i64,
    pub auto_turn_end: f64,
    pub curve_lower_limit: f64,
    pub is_metric: bool,
    pub road_limit_offset: f64,
    pub curve_factor: f64,
    pub language: String,
}

impl Settings {
    pub fn curve_factor(params: &openpilot_params::Params) -> Result<f64, crate::Error> {
        let raw = params.get("AutoCurveSpeedFactor")?.unwrap_or_default();
        let value = openpilot_beepd::integer(&raw)
            .map_err(|_| crate::Error::Contract("invalid curve factor"))?;
        Ok(f64::from(value) * 0.01)
    }
    pub fn read(params: &openpilot_params::Params) -> Result<Self, crate::Error> {
        let integer = |key: &str| -> Result<i64, crate::Error> {
            let bytes = params.get(key)?.unwrap_or_default();
            openpilot_beepd::integer(&bytes)
                .map(i64::from)
                .map_err(|_| crate::Error::Contract("invalid integer Params"))
        };
        let value = |key| -> Result<f64, crate::Error> {
            Ok(num_traits::ToPrimitive::to_f64(&integer(key)?)
                .ok_or(crate::Error::Contract("integer Params conversion"))?)
        };
        let map_turn = params.get("MapTurnSpeedFactor")?.unwrap_or_default();
        let map_turn = openpilot_calibrationd::parameters::parse_float(&map_turn)
            .map_err(|_| crate::Error::Contract("invalid float Params"))?;
        let language = match params.get("LanguageSetting") {
            Ok(Some(value)) if !value.is_empty() => Some(value),
            Ok(_) => params.get("lang").ok().flatten(),
            Err(_) => None,
        };
        let language = match language.as_deref() {
            Some(b"main_ko") => "ko",
            Some(b"main_zh-CHS") => "zh",
            _ => "en",
        };
        let curve_lower = params
            .get("AutoCurveSpeedLowerLimit")?
            .filter(|v| !v.is_empty())
            .ok_or(crate::Error::Contract("missing curve lower limit"))?;
        let curve_lower = std::str::from_utf8(&curve_lower)
            .map_err(|_| crate::Error::Contract("invalid curve lower limit"))?
            .trim()
            .parse::<i64>()
            .map_err(|_| crate::Error::Contract("invalid curve lower limit"))?;
        Ok(Self {
            bump_speed: value("AutoNaviSpeedBumpSpeed")?,
            bump_time: value("AutoNaviSpeedBumpTime")?,
            bump_end_distance: value("AutoNaviSpeedBumpEndDistance")?.clamp(0., 5000.) * 0.01,
            control_end: value("AutoNaviSpeedCtrlEnd")?,
            rear_hold_distance: value("AutoNaviRearCameraHoldDistance")?.clamp(0., 300.),
            control_mode: integer("AutoNaviSpeedCtrlMode")?,
            vehicle_can_control: integer("VehicleNaviCanControl")?.clamp(0, 3),
            school_control: params.get_bool("VehicleNaviSchoolZoneControl")?,
            camera_control_mode: integer("VehicleSpeedCameraControlMode")?.clamp(0, 3),
            safety_factor: value("AutoNaviSpeedSafetyFactor")? * 0.01,
            deceleration: value("AutoNaviSpeedDecelRate")? * 0.01,
            countdown_mode: integer("AutoNaviCountDownMode")?,
            turn_speed_mode: integer("TurnSpeedControlMode")?,
            map_turn_factor: map_turn * 0.01,
            auto_turn_speed: value("AutoTurnControlSpeedTurn")?,
            auto_turn_map_change: integer("AutoTurnMapChange")?,
            auto_turn_control: integer("AutoTurnControl")?,
            auto_turn_end: value("AutoTurnControlTurnEnd")?,
            curve_lower_limit: num_traits::ToPrimitive::to_f64(&curve_lower)
                .ok_or(crate::Error::Contract("curve lower limit conversion"))?,
            is_metric: params.get_bool("IsMetric")?,
            road_limit_offset: value("AutoRoadSpeedLimitOffset")?,
            curve_factor: value("AutoCurveSpeedFactor")? * 0.01,
            language: language.into(),
        })
    }
}
