use super::{
    flags as f,
    parser_inputs::Channel,
    state::State,
    state_canfd::pt,
    state_fields::{float32, gear, timestamp},
    wire::get,
    Error,
};
use openpilot_cereal::car_capnp::car_state;

pub fn update(state: &mut State, mut ret: car_state::Builder<'_>, now: u64) -> Result<bool, Error> {
    let flags = state.config.flags;
    if state.config.ext_flags & f::ext::EV_MODE_230 != 0 {
        let time = timestamp(
            state,
            Channel::Pt,
            "HCU_STATUS_230",
            Some("HYBRID_POWER_FLOW_MODE"),
        );
        let age = i128::from(state.inputs.pt.last_update) - i128::from(time);
        let valid = time > 0
            && state
                .inputs
                .pt
                .raw
                .get(&0x230)
                .is_some_and(|data| data.len() == 32)
            && !state.inputs.pt.bus_timeout()
            && (0..=500_000_000).contains(&age);
        ret.set_ev_mode_valid(valid);
        ret.set_ev_mode_active(
            valid
                && [1., 2., 6.].contains(
                    &pt(state, ("HCU_STATUS_230", "HYBRID_POWER_FLOW_MODE"), now)?.trunc(),
                ),
        );
    }
    state.metric = pt(state, ("CRUISE_BUTTONS_ALT", "DISTANCE_UNIT"), now)? != 1.;
    let use_accelerator = state.config.gear_message() == "ACCELERATOR";
    let accelerator = state.inputs.captured("accelerator")?;
    if flags & (f::EV | f::HYBRID) != 0 {
        let pedal = if !use_accelerator {
            pt(
                state,
                (state.config.accelerator_message(), "ACCELERATOR_PEDAL"),
                now,
            )?
        } else {
            match &accelerator {
                Some(data) => get(data, "ACCELERATOR_PEDAL")?,
                None => 0.,
            }
        };
        ret.set_gas(float32(
            pedal / if flags & f::EV != 0 { 255. } else { 1023. },
        )?);
        ret.set_gas_pressed(ret.reborrow_as_reader().get_gas() > 1e-5);
    } else {
        let pedal = if !use_accelerator {
            pt(
                state,
                (
                    state.config.accelerator_message(),
                    "ACCELERATOR_PEDAL_PRESSED",
                ),
                now,
            )? != 0.
        } else {
            match &accelerator {
                Some(data) => get(data, "ACCELERATOR_PEDAL_PRESSED")? != 0.,
                None => false,
            }
        };
        ret.set_gas_pressed(pedal);
    }
    let brake = pt(state, ("TCS", "DriverBraking"), now)? == 1.;
    ret.set_brake_pressed(brake);
    ret.set_parking_brake(pt(state, ("TCS", "ESC_PrkBrkActvSta"), now)? == 1.);
    if let Some(data) = state.inputs.captured("doors_seatbelts")? {
        ret.set_door_open(get(&data, "DRIVER_DOOR")? == 1.);
        ret.set_seatbelt_unlatched(get(&data, "DRIVER_SEATBELT")? == 0.);
    }
    let selected = if !use_accelerator {
        pt(state, (state.config.gear_message(), "GEAR"), now)?
    } else {
        match &accelerator {
            Some(data) => get(data, "GEAR")?,
            None => 0.,
        }
    };
    ret.set_gear_shifter(gear(state, selected)?);
    if state.capabilities.tpms {
        let unit = pt(state, ("TPMS", "UNIT"), now)?;
        let scale = if unit.trunc() > 0. { unit * 0.725 } else { 1. };
        let pressures = [
            pt(state, ("TPMS", "PRESSURE_FL"), now)?,
            pt(state, ("TPMS", "PRESSURE_FR"), now)?,
            pt(state, ("TPMS", "PRESSURE_RL"), now)?,
            pt(state, ("TPMS", "PRESSURE_RR"), now)?,
        ];
        let mut tpms = ret.reborrow().init_tpms();
        tpms.set_fl(float32(scale * pressures[0])?);
        tpms.set_fr(float32(scale * pressures[1])?);
        tpms.set_rl(float32(scale * pressures[2])?);
        tpms.set_rr(float32(scale * pressures[3])?);
    }
    Ok(brake)
}
