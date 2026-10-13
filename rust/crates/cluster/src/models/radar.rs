pub const RADAR_ZERO_POSITION_EPS_M: f64 = 1e-3;
pub const RADAR_MOVING_VEHICLE_MIN_SPEED_KPH: f64 = 12.0;
pub const RADAR_CROSS_TRAFFIC_MIN_LATERAL_SPEED_MPS: f64 = 2.0;

#[must_use]
pub fn radar_position_is_zero(longitudinal_m: f64, lateral_m: f64) -> bool {
    longitudinal_m.abs() <= RADAR_ZERO_POSITION_EPS_M
        && lateral_m.abs() <= RADAR_ZERO_POSITION_EPS_M
}
