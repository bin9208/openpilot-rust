//! The worker's view of carState, modelV2 and carrotMan, in the plain types
//! the observers and the TBT trigger use.
use super::{observed::Meta, tbt::TbtCar, Error};
use openpilot_cereal::{
    car_capnp::car_state,
    log_capnp::{model_data_v2, Desire, LaneChangeDirection, LaneChangeState},
};
use openpilot_desire::types::{Car, Model};

fn floats(values: capnp::primitive_list::Reader<'_, f32>) -> Vec<f64> {
    values.iter().map(f64::from).collect()
}

pub fn car(cs: car_state::Reader<'_>) -> (Car, TbtCar) {
    (
        Car {
            can_valid: cs.get_can_valid(),
            left_blinker: cs.get_left_blinker(),
            right_blinker: cs.get_right_blinker(),
            v_ego: cs.get_v_ego().into(),
            a_ego: cs.get_a_ego().into(),
            trailer_connected: cs.get_trailer_connected(),
            steering_torque: cs.get_steering_torque().into(),
            steering_pressed: cs.get_steering_pressed(),
            steering_angle_deg: cs.get_steering_angle_deg().into(),
            left_lane_line: cs.get_left_lane_line().into(),
            right_lane_line: cs.get_right_lane_line().into(),
            left_blindspot: cs.get_left_blindspot(),
            right_blindspot: cs.get_right_blindspot(),
            blinker_lever: cs.get_blinker_lever(),
            blinker_lamp: cs.get_blinker_lamp(),
            stalk_counts: [
                cs.get_left_blinker_stalk_count(),
                cs.get_right_blinker_stalk_count(),
            ],
            drive: cs.get_gear_shifter() == Ok(car_state::GearShifter::Drive),
            steer_fault_temporary: cs.get_steer_fault_temporary(),
            steer_fault_permanent: cs.get_steer_fault_permanent(),
            lane_line_info_mono_time: cs.get_lane_line_info_mono_time(),
            cruise_cancel_state: cs.get_cruise_cancel_state(),
            turn_signal_driver_valid: cs.get_turn_signal_driver_valid(),
            turn_signal_driver_direction: cs.get_turn_signal_driver_direction(),
            auto_turn_signal_ready_request_id: cs.get_auto_turn_signal_ready_request_id(),
            vehicle_sensors_invalid: cs.get_vehicle_sensors_invalid(),
            standstill: cs.get_standstill(),
        },
        TbtCar {
            rear_inputs_available: cs.get_tbt_rear_inputs_available(),
            right_rear_blocked: cs.get_tbt_right_rear_blocked(),
            right_blindspot: cs.get_right_blindspot(),
        },
    )
}

fn direction(value: Result<LaneChangeDirection, capnp::NotInSchema>) -> u8 {
    match value {
        Ok(LaneChangeDirection::Left) => 1,
        Ok(LaneChangeDirection::Right) => 2,
        _ => 0,
    }
}

fn direction_name(value: Result<LaneChangeDirection, capnp::NotInSchema>) -> &'static str {
    match direction(value) {
        1 => "left",
        2 => "right",
        _ => "none",
    }
}

pub fn model(md: model_data_v2::Reader<'_>) -> Result<(Model, Meta), Error> {
    let lanes = md.get_lane_lines()?;
    let edges = md.get_road_edges()?;
    let probs = md.get_lane_line_probs()?;
    let meta = md.get_meta()?;
    let desires = meta.get_desire_state()?;
    let lane = |i: u32, x: bool| -> Result<Vec<f64>, Error> {
        if i >= lanes.len() {
            return Ok(Vec::new());
        }
        let line = lanes.get(i);
        Ok(floats(if x { line.get_x()? } else { line.get_y()? }))
    };
    let edge = |i: u32| -> Result<Vec<f64>, Error> {
        if i >= edges.len() {
            return Ok(Vec::new());
        }
        Ok(floats(edges.get(i).get_y()?))
    };
    let position = md.get_position()?;
    let model = Model {
        lane_lines: [
            lane(0, false)?,
            lane(1, false)?,
            lane(2, false)?,
            lane(3, false)?,
        ],
        lane_line_x: [
            lane(0, true)?,
            lane(1, true)?,
            lane(2, true)?,
            lane(3, true)?,
        ],
        road_edges: [edge(0)?, edge(1)?],
        lane_line_probs: std::array::from_fn(|i| {
            if (i as u32) < probs.len() {
                f64::from(probs.get(i as u32))
            } else {
                f64::NAN
            }
        }),
        desire_state: std::array::from_fn(|i| {
            if (i as u32) < desires.len() {
                f64::from(desires.get(i as u32))
            } else {
                f64::NAN
            }
        }),
        orientation_rate_z: Vec::new(),
        position_t: floats(position.get_t()?),
        position_x: floats(position.get_x()?),
        position_y: floats(position.get_y()?),
        frame_id: md.get_frame_id(),
        road_edge_stds: {
            let stds = md.get_road_edge_stds()?;
            std::array::from_fn(|i| {
                if (i as u32) < stds.len() {
                    f64::from(stds.get(i as u32))
                } else {
                    f64::NAN
                }
            })
        },
        desire_prediction: floats(meta.get_desire_prediction()?),
    };
    let text = |r: capnp::Result<capnp::text::Reader<'_>>| -> String {
        r.ok()
            .and_then(|t| t.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    };
    let meta = Meta {
        observed_indicator: direction(meta.get_observed_indicator()),
        observed_mono_time: meta.get_observed_indicator_mono_time() as f64 / 1e9,
        observed_source: text(meta.get_observed_indicator_source()),
        isolation_mask: meta.get_indicator_isolation_mask(),
        journal_mono_time: meta.get_indicator_journal_mono_time() as f64 / 1e9,
        manual_lane_indicator: direction(meta.get_manual_lane_indicator()),
        manual_lane_mono_time: meta.get_manual_lane_indicator_mono_time() as f64 / 1e9,
        prepare_direction: direction(meta.get_indicator_prepare_direction()),
        prepare_source: text(meta.get_indicator_prepare_source()),
        lane_change_state: match meta.get_lane_change_state() {
            Ok(LaneChangeState::PreLaneChange) => "preLaneChange",
            Ok(LaneChangeState::LaneChangeStarting) => "laneChangeStarting",
            Ok(LaneChangeState::LaneChangeFinishing) => "laneChangeFinishing",
            _ => "off",
        },
        lane_change_direction: direction_name(meta.get_lane_change_direction()),
        desire: match meta.get_desire() {
            Ok(Desire::TurnLeft) => "turnLeft",
            Ok(Desire::TurnRight) => "turnRight",
            Ok(Desire::LaneChangeLeft) => "laneChangeLeft",
            Ok(Desire::LaneChangeRight) => "laneChangeRight",
            Ok(Desire::KeepLeft) => "keepLeft",
            Ok(Desire::KeepRight) => "keepRight",
            _ => "none",
        },
        lane_change_available_right: meta.get_lane_change_available_right(),
        lane_width_right: meta.get_lane_width_right().into(),
        assistant_signal_id: text(meta.get_assistant_lane_signal_id()),
        assistant_signal_route: text(meta.get_assistant_lane_signal_route()),
    };
    Ok((model, meta))
}
