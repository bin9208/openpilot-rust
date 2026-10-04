use crate::car_state::CarState;
use openpilot_cereal::car_capnp::car_state;

pub fn state(value: car_state::Reader<'_>) -> CarState {
    CarState {
        v_ego: f64::from(value.get_v_ego()),
        a_ego: f64::from(value.get_a_ego()),
        v_ego_cluster: f64::from(value.get_v_ego_cluster()),
        v_clu_ratio: f64::from(value.get_v_clu_ratio()),
        gas_pressed: value.get_gas_pressed(),
        brake_pressed: value.get_brake_pressed(),
        standstill: value.get_standstill(),
        steering_angle_deg: f64::from(value.get_steering_angle_deg()),
        steering_torque: f64::from(value.get_steering_torque()),
        steering_pressed: value.get_steering_pressed(),
        left_blinker: value.get_left_blinker(),
        right_blinker: value.get_right_blinker(),
        left_blindspot: value.get_left_blindspot(),
        right_blindspot: value.get_right_blindspot(),
        soft_hold_active: i32::from(value.get_soft_hold_active()),
        carrot_cruise: i32::from(value.get_carrot_cruise()),
        use_lane_line_speed: f64::from(value.get_use_lane_line_speed()),
        v_cruise: f64::from(value.get_v_cruise()),
        v_cruise_cluster: f64::from(value.get_v_cruise_cluster()),
    }
}
