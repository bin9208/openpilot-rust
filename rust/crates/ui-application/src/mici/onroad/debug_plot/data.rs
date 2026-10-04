use crate::{state::messages, Error};
use openpilot_messaging::state::State;
pub const REQUIRED: [&str; 7] = [
    "carState",
    "longitudinalPlan",
    "carControl",
    "controlsState",
    "modelV2",
    "radarState",
    "liveParameters",
];
fn item(values: capnp::Result<capnp::primitive_list::Reader<'_, f32>>, index: u32) -> f64 {
    values
        .ok()
        .filter(|values| values.len() > index)
        .map_or(0.0, |values| f64::from(values.get(index)))
}
pub fn data(state: &State, mode: i32) -> Result<([f64; 3], &'static str), Error> {
    for service in REQUIRED {
        if !state.topic(service)?.alive {
            return Ok(([0.0; 3], "no data"));
        }
    }
    values(state, mode)
}
pub fn values(state: &State, mode: i32) -> Result<([f64; 3], &'static str), Error> {
    let car = messages::car_state(state)?;
    let plan = messages::longitudinal_plan(state)?;
    let control = messages::car_control(state)?.get_actuators()?;
    let acceleration = item(plan.get_accels(), 0);
    Ok(match mode {
        0|1 => ([f64::from(car.get_a_ego()),acceleration,f64::from(control.get_accel())],"1.Accel (Y:a_ego, G:a_target, O:a_out)"),
        2 => ([item(plan.get_speeds(),0),f64::from(car.get_v_ego()),f64::from(car.get_a_ego())],"2.Speed/Accel (Y:speed_0, G:v_ego, O:a_ego)"),
        3 => {
            let model = messages::model(state)?;
            let position = model.get_position().and_then(|value|value.get_x());
            ([item(position,32),item(model.get_velocity().and_then(|value|value.get_x()),32),item(model.get_velocity().and_then(|value|value.get_x()),0)],"3.Model (Y:pos_32, G:vel_32, O:vel_0)")
        }
        4 => {
            let lead = messages::radar_state(state)?.get_lead_one().ok();
            ([acceleration,lead.map_or(0.0,|lead|f64::from(lead.get_a_lead_k())),lead.map_or(0.0,|lead|f64::from(lead.get_v_rel()))],"4.Lead (Y:accel, G:a_leadK, O:v_rel)")
        }
        5 => {
            let lead = messages::radar_state(state)?.get_lead_one().ok();
            ([f64::from(car.get_a_ego()),lead.map_or(0.0,|lead|f64::from(lead.get_a_lead())),lead.map_or(0.0,|lead|f64::from(lead.get_j_lead()))],"5.Lead (Y:a_ego, G:a_lead, O:j_lead)")
        }
        6 => match messages::controls_state(state)?.get_lateral_control_state().which() {
            Ok(openpilot_cereal::log_capnp::controls_state::lateral_control_state::Which::TorqueState(Ok(torque))) =>
                ([f64::from(torque.get_actual_lateral_accel())*10.0,f64::from(torque.get_desired_lateral_accel())*10.0,f64::from(torque.get_output())*10.0],"6.Steer (Y:actual, G:desired, O:output) *10"),
            _ => ([0.0;3],"no data"),
        },
        7 => ([f64::from(car.get_steering_angle_deg()),f64::from(control.get_steering_angle_deg()),f64::from(messages::live_parameters(state)?.get_angle_offset_deg())*10.0],"7.SteerA (Y:Actual, G:Target, O:Offset*10)"),
        8 => ([f64::from(control.get_curvature())*10000.0;3],"8.Curvature (x10000)"),
        _ => ([0.0;3],"no data"),
    })
}
