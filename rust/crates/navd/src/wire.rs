use crate::{
    geometry::Coordinate,
    instructions::Direction,
    route::{Instruction, SpeedLimitSign},
    Error,
};
use num_traits::ToPrimitive;
use openpilot_cereal::log_capnp::{event, nav_instruction};

fn float(value: f64) -> Result<f32, Error> {
    value.to_f32().ok_or(Error::Field("Float32"))
}

fn count(value: usize) -> Result<u32, Error> {
    u32::try_from(value).map_err(|_| Error::GeometrySize)
}

pub fn instruction(message: &Instruction, timestamp: u64) -> Result<Vec<u8>, Error> {
    let mut output = capnp::message::Builder::new_default();
    let mut event = output.init_root::<event::Builder>();
    event.set_log_mono_time(timestamp);
    event.set_valid(message.valid);
    let mut instruction = event.init_nav_instruction();
    if message.valid {
        instruction.set_maneuver_distance(float(message.maneuver_distance)?);
        if let Some(banner) = &message.banner {
            instruction.set_show_full(banner.show_full);
            if let Some(value) = &banner.maneuver_primary_text {
                instruction.set_maneuver_primary_text(value.as_str());
            }
            if let Some(value) = &banner.maneuver_secondary_text {
                instruction.set_maneuver_secondary_text(value.as_str());
            }
            if let Some(value) = &banner.maneuver_type {
                instruction.set_maneuver_type(value.as_str());
            }
            if let Some(value) = &banner.maneuver_modifier {
                instruction.set_maneuver_modifier(value.as_str());
            }
            if let Some(lanes) = &banner.lanes {
                let mut list = instruction.reborrow().init_lanes(count(lanes.len())?);
                for (index, value) in lanes.iter().enumerate() {
                    let mut lane = list.reborrow().get(count(index)?);
                    lane.set_active(value.active);
                    if let Some(value) = value.active_direction {
                        lane.set_active_direction(direction(value));
                    }
                    let mut directions = lane.init_directions(count(value.directions.len())?);
                    for (index, value) in value.directions.iter().enumerate() {
                        directions.set(count(index)?, direction(*value));
                    }
                }
            }
        }
        let mut maneuvers = instruction
            .reborrow()
            .init_all_maneuvers(count(message.maneuvers.len())?);
        for (index, value) in message.maneuvers.iter().enumerate() {
            let mut maneuver = maneuvers.reborrow().get(count(index)?);
            maneuver.set_distance(float(value.distance)?);
            if let Some(value) = &value.kind {
                maneuver.set_type(value.as_str());
            }
            if let Some(value) = &value.modifier {
                maneuver.set_modifier(value.as_str());
            }
        }
        instruction.set_distance_remaining(float(message.distance_remaining)?);
        instruction.set_time_remaining(float(message.time_remaining)?);
        instruction.set_time_remaining_typical(float(message.time_remaining_typical)?);
        if let Some(value) = message.speed_limit {
            instruction.set_speed_limit(float(value)?);
        }
        if let Some(value) = message.speed_limit_sign {
            instruction.set_speed_limit_sign(match value {
                SpeedLimitSign::Mutcd => nav_instruction::SpeedLimitSign::Mutcd,
                SpeedLimitSign::Vienna => nav_instruction::SpeedLimitSign::Vienna,
            });
        }
    }
    Ok(capnp::serialize::write_message_to_words(&output))
}

pub fn route(coordinates: &[Coordinate], timestamp: u64) -> Result<Vec<u8>, Error> {
    let mut output = capnp::message::Builder::new_default();
    let mut event = output.init_root::<event::Builder>();
    event.set_log_mono_time(timestamp);
    event.set_valid(true);
    let mut points = event
        .init_nav_route_navd()
        .init_coordinates(count(coordinates.len())?);
    for (index, coordinate) in coordinates.iter().enumerate() {
        let mut point = points.reborrow().get(count(index)?);
        point.set_latitude(float(coordinate.latitude)?);
        point.set_longitude(float(coordinate.longitude)?);
    }
    Ok(capnp::serialize::write_message_to_words(&output))
}

fn direction(value: Direction) -> nav_instruction::Direction {
    match value {
        Direction::None => nav_instruction::Direction::None,
        Direction::Left => nav_instruction::Direction::Left,
        Direction::Right => nav_instruction::Direction::Right,
        Direction::Straight => nav_instruction::Direction::Straight,
        Direction::SlightLeft => nav_instruction::Direction::SlightLeft,
        Direction::SlightRight => nav_instruction::Direction::SlightRight,
    }
}
