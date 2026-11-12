use crate::Error;
use openpilot_cereal::{
    car_capnp::car_state,
    custom_capnp::carrot_man,
    log_capnp::{self, model_data_v2, radar_state},
};
use openpilot_desire::{
    helper::DesireHelper,
    types::{Car, Indicator, Input, Lead, Model, Navigation, State},
};

fn floats(values: capnp::primitive_list::Reader<'_, f32>) -> Vec<f64> {
    values.iter().map(f64::from).collect()
}

fn lanes_y(
    lanes: capnp::struct_list::Reader<'_, log_capnp::x_y_z_t_data::Owned>,
) -> Result<Vec<Vec<f64>>, Error> {
    (0..4).map(|i| Ok(floats(lanes.get(i).get_y()?))).collect()
}

fn lead(value: radar_state::lead_data::Reader<'_>) -> Lead {
    Lead {
        status: value.get_status(),
        d_rel: value.get_d_rel().into(),
        v_rel: Some(value.get_v_rel().into()),
        v_lead: Some(value.get_v_lead().into()),
        radar_track_id: value.get_radar_track_id().into(),
    }
}

pub fn desire_input(
    car: car_state::Reader<'_>,
    model: model_data_v2::Reader<'_>,
    navigation: carrot_man::Reader<'_>,
    radar: radar_state::Reader<'_>,
    lateral_active: bool,
    indicator: Indicator,
) -> Result<Input, Error> {
    let lanes = model.get_lane_lines()?;
    let edges = model.get_road_edges()?;
    let probs = model.get_lane_line_probs()?;
    let desires = model.get_meta()?.get_desire_state()?;
    if lanes.len() != 4 || edges.len() != 2 || probs.len() != 4 || desires.len() != 8 {
        return Err(Error::Contract("invalid model geometry for desire"));
    }
    let lane_x = (0..4)
        .map(|i| Ok(floats(lanes.get(i).get_x()?)))
        .collect::<Result<Vec<_>, Error>>()?;
    let lanes = lanes_y(lanes)?;
    let edges = (0..2)
        .map(|i| Ok(floats(edges.get(i).get_y()?)))
        .collect::<Result<Vec<_>, Error>>()?;
    let position = model.get_position()?;
    Ok(Input {
        car: Car {
            can_valid: car.get_can_valid(),
            left_blinker: car.get_left_blinker(),
            right_blinker: car.get_right_blinker(),
            v_ego: car.get_v_ego().into(),
            a_ego: car.get_a_ego().into(),
            trailer_connected: car.get_trailer_connected(),
            steering_torque: car.get_steering_torque().into(),
            steering_pressed: car.get_steering_pressed(),
            steering_angle_deg: car.get_steering_angle_deg().into(),
            left_lane_line: car.get_left_lane_line().into(),
            right_lane_line: car.get_right_lane_line().into(),
            left_blindspot: car.get_left_blindspot(),
            right_blindspot: car.get_right_blindspot(),
            blinker_lever: car.get_blinker_lever(),
            blinker_lamp: car.get_blinker_lamp(),
            stalk_counts: [
                car.get_left_blinker_stalk_count(),
                car.get_right_blinker_stalk_count(),
            ],
            drive: car.get_gear_shifter() == Ok(car_state::GearShifter::Drive),
            steer_fault_temporary: car.get_steer_fault_temporary(),
            steer_fault_permanent: car.get_steer_fault_permanent(),
            lane_line_info_mono_time: car.get_lane_line_info_mono_time(),
            cruise_cancel_state: car.get_cruise_cancel_state(),
            turn_signal_driver_valid: car.get_turn_signal_driver_valid(),
            turn_signal_driver_direction: car.get_turn_signal_driver_direction(),
            auto_turn_signal_ready_request_id: car.get_auto_turn_signal_ready_request_id(),
            vehicle_sensors_invalid: car.get_vehicle_sensors_invalid(),
            standstill: car.get_standstill(),
        },
        model: Model {
            lane_lines: lanes
                .try_into()
                .map_err(|_| Error::Contract("lane count"))?,
            road_edges: edges
                .try_into()
                .map_err(|_| Error::Contract("edge count"))?,
            lane_line_probs: std::array::from_fn(|i| f64::from(probs.get(i as u32))),
            desire_state: std::array::from_fn(|i| f64::from(desires.get(i as u32))),
            orientation_rate_z: floats(model.get_orientation_rate()?.get_z()?),
            lane_line_x: lane_x
                .try_into()
                .map_err(|_| Error::Contract("lane count"))?,
            position_t: floats(position.get_t()?),
            position_x: floats(position.get_x()?),
            position_y: floats(position.get_y()?),
            frame_id: model.get_frame_id(),
            desire_prediction: floats(model.get_meta()?.get_desire_prediction()?),
        },
        navigation: Navigation {
            atc_type: navigation.get_atc_type()?.to_str()?.to_owned(),
            command_index: navigation.get_carrot_cmd_index().into(),
            command: navigation.get_carrot_cmd()?.to_str()?.to_owned(),
            argument: navigation.get_carrot_arg()?.to_str()?.to_owned(),
            x_turn_info: navigation.get_x_turn_info(),
            x_dist_to_turn: f64::from(navigation.get_x_dist_to_turn()),
        },
        leads: [lead(radar.get_lead_left()?), lead(radar.get_lead_right()?)],
        objects: [
            radar.get_leads_left()?.iter().map(lead).collect(),
            radar.get_leads_right()?.iter().map(lead).collect(),
        ],
        lateral_active,
        lane_change_prob: f64::from(desires.get(3)) + f64::from(desires.get(4)),
        indicator,
    })
}

pub fn apply_desire(
    mut model: model_data_v2::Builder<'_>,
    helper: &DesireHelper,
) -> Result<(), Error> {
    let mut meta = model.reborrow().get_meta()?;
    meta.set_lane_change_state(match helper.lane_change_state {
        State::Off => log_capnp::LaneChangeState::Off,
        State::PreLaneChange => log_capnp::LaneChangeState::PreLaneChange,
        State::Starting => log_capnp::LaneChangeState::LaneChangeStarting,
        State::Finishing => log_capnp::LaneChangeState::LaneChangeFinishing,
    });
    meta.set_lane_change_direction(log_capnp::LaneChangeDirection::try_from(u16::from(
        helper.lane_change_direction,
    ))?);
    meta.set_desire_log(&helper.desire_log);
    meta.set_lane_change_blinker_hold(log_capnp::LaneChangeDirection::try_from(u16::from(
        helper.blinker_hold_direction,
    ))?);
    meta.set_lever_turn(helper.lever_turn);
    meta.set_lane_width_left(helper.left.lane_width as f32);
    meta.set_lane_width_right(helper.right.lane_width as f32);
    meta.set_distance_to_road_edge_left(helper.left.dist_to_edge as f32);
    meta.set_distance_to_road_edge_right(helper.right.dist_to_edge as f32);
    meta.set_desire(log_capnp::Desire::try_from(u16::from(helper.desire))?);
    meta.set_lane_change_prob(helper.lane_change_ll_prob as f32);
    meta.set_model_turn_speed(200.0);
    meta.set_lane_change_available_left(helper.lane_change_available_left);
    meta.set_lane_change_available_right(helper.lane_change_available_right);
    let direction = |value: u8| log_capnp::LaneChangeDirection::try_from(u16::from(value));
    let nanos = |seconds: f64| (seconds * 1e9) as u64;
    meta.set_auto_turn_signal(direction(helper.auto_turn_signal)?);
    meta.set_auto_turn_signal_lane_change(helper.auto_turn_signal_lane_change);
    meta.set_auto_turn_signal_direction(direction(helper.tesla.signal_direction)?);
    meta.set_auto_turn_signal_request_id(helper.tesla.signal_request_id);
    let boundary = &helper.boundary_indicator;
    meta.set_manual_lane_indicator(direction(boundary.request_direction)?);
    meta.set_manual_lane_indicator_mono_time(nanos(boundary.request_time));
    let observation = helper.model_indicator.active.as_ref();
    meta.set_observed_indicator(direction(observation.map_or(0, |a| a.direction()))?);
    meta.set_observed_indicator_mono_time(nanos(helper.model_indicator.request_time));
    meta.set_observed_indicator_source(helper.model_indicator.reason);
    meta.set_indicator_isolation_mask(boundary.blocked);
    meta.set_indicator_journal_mono_time(nanos(helper.indicator_journal_time));
    let (prepare, source) = helper.indicator_preparation();
    meta.set_indicator_prepare_direction(direction(prepare)?);
    meta.set_indicator_prepare_source(source);
    Ok(())
}
