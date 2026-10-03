use super::{parser_inputs::Channel, state::State, Error};
use num_traits::ToPrimitive;
use openpilot_cereal::car_capnp::car_state::{self, button_event::Type};
use std::collections::VecDeque;

pub fn float32(value: f64) -> Result<f32, Error> {
    value.to_f32().ok_or(Error::Numeric)
}
pub fn int16(value: f64) -> Result<i16, Error> {
    value.to_i16().ok_or(Error::Numeric)
}
pub fn byte(value: f64) -> Result<u8, Error> {
    value.to_u8().ok_or(Error::Numeric)
}

pub fn extend(queue: &mut VecDeque<u8>, values: impl IntoIterator<Item = u8>) {
    for value in values {
        if queue.len() == 8 {
            queue.pop_front();
        }
        queue.push_back(value);
    }
}

pub fn buttons(current: u8, previous: u8, mapping: &[(u8, Type)]) -> Vec<(bool, Type)> {
    if current == previous {
        return Vec::new();
    }
    [(false, previous), (true, current)]
        .into_iter()
        .filter(|(_, button)| *button != 0)
        .map(|(pressed, button)| {
            (
                pressed,
                mapping
                    .iter()
                    .find(|(known, _)| *known == button)
                    .map_or(Type::Unknown, |(_, kind)| *kind),
            )
        })
        .collect()
}

pub fn button_list(mut ret: car_state::Builder<'_>, events: &[(bool, Type)]) -> Result<(), Error> {
    let mut list = ret
        .reborrow()
        .init_button_events(u32::try_from(events.len()).map_err(|_| Error::Numeric)?);
    for (index, (pressed, kind)) in events.iter().enumerate() {
        let mut event = list
            .reborrow()
            .get(u32::try_from(index).map_err(|_| Error::Numeric)?);
        event.set_pressed(*pressed);
        event.set_type(*kind);
    }
    Ok(())
}

pub const CRUISE_BUTTONS: &[(u8, Type)] = &[
    (1, Type::AccelCruise),
    (2, Type::DecelCruise),
    (3, Type::GapAdjustCruise),
    (4, Type::Cancel),
    (5, Type::LfaButton),
];

pub fn wheels(
    state: &mut State,
    mut ret: car_state::Builder<'_>,
    signals: (&str, [&str; 4]),
    now: u64,
) -> Result<(), Error> {
    let mut values = [0f64; 4];
    for (index, name) in signals.1.iter().enumerate() {
        values[index] = state.inputs.signal(Channel::Pt, signals.0, name, now)?;
    }
    let values =
        crate::state_helpers::wheel_speeds(values, state.config.wheel_speed_factor, 1. / 3.6);
    let mut wheel = ret.reborrow().init_wheel_speeds();
    let values = [
        float32(values[0])?,
        float32(values[1])?,
        float32(values[2])?,
        float32(values[3])?,
    ];
    wheel.set_fl(values[0]);
    wheel.set_fr(values[1]);
    wheel.set_rl(values[2]);
    wheel.set_rr(values[3]);
    let raw = values.into_iter().map(f64::from).sum::<f64>() / 4.;
    ret.set_v_ego_raw(float32(raw)?);
    let [speed, accel] = state
        .speed_filter
        .update(f64::from(ret.reborrow_as_reader().get_v_ego_raw()));
    ret.set_v_ego(float32(speed)?);
    ret.set_a_ego(float32(accel)?);
    let threshold = 12. * 0.03125 / 3.6;
    ret.set_standstill(f64::from(values[0]) <= threshold && f64::from(values[3]) <= threshold);
    Ok(())
}

pub fn cluster_speed(
    state: &mut State,
    mut ret: car_state::Builder<'_>,
    speed: f64,
) -> Result<(), Error> {
    ret.set_v_ego_cluster(float32(speed)?);
    let [cluster, _] = state
        .cluster_filter
        .update(f64::from(ret.reborrow_as_reader().get_v_ego_cluster()));
    let ego = f64::from(ret.reborrow_as_reader().get_v_ego());
    ret.set_v_clu_ratio(float32(if cluster > 3. && ego > 3. {
        ego / cluster
    } else {
        1.
    })?);
    Ok(())
}

pub fn gear(state: &State, value: f64) -> Result<car_state::GearShifter, Error> {
    let key = value.to_i64().ok_or(Error::Numeric)?;
    Ok(crate::state_helpers::parse_gear(
        state.gear_values.get(&key).map(String::as_str),
    ))
}

pub fn timestamp(state: &State, channel: Channel, name: &str, signal: Option<&str>) -> u64 {
    let Ok(parser) = state.inputs.parser(channel) else {
        return 0;
    };
    let Some(address) = parser.dbc.names.get(name) else {
        return 0;
    };
    let Some(message) = parser.dbc.messages.get(address) else {
        return 0;
    };
    let Some(data) = parser.states.get(address) else {
        return 0;
    };
    if signal.is_some_and(|name| !message.signals.iter().any(|entry| entry.name == name)) {
        return 0;
    }
    data.timestamps.back().copied().unwrap_or(0)
}
