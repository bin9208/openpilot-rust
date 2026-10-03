use super::{
    flags as f,
    parser_inputs::Channel,
    state::State,
    state_fields::{float32, gear, int16},
    Error,
};
use openpilot_cereal::car_capnp::car_state::{self, GearShifter};

fn pt(state: &mut State, input: (&str, &str), now: u64) -> Result<f64, Error> {
    state.inputs.signal(Channel::Pt, input.0, input.1, now)
}

pub fn update(state: &mut State, mut ret: car_state::Builder<'_>, now: u64) -> Result<(), Error> {
    let flags = state.config.flags;
    if flags & (f::HYBRID | f::EV | f::FCEV) != 0 {
        let gas = if flags & f::FCEV != 0 {
            pt(state, ("FCEV_ACCELERATOR", "ACCELERATOR_PEDAL"), now)? / 254.
        } else if flags & f::HYBRID != 0 {
            pt(state, ("E_EMS11", "CR_Vcu_AccPedDep_Pos"), now)? / 254.
        } else {
            pt(state, ("E_EMS11", "Accel_Pedal_Pos"), now)? / 254.
        };
        ret.set_gas(float32(gas)?);
        ret.set_gas_pressed(ret.reborrow_as_reader().get_gas() > 0.);
    } else {
        ret.set_gas(float32(pt(state, ("EMS12", "PV_AV_CAN"), now)? / 100.)?);
        ret.set_gas_pressed(pt(state, ("EMS16", "CF_Ems_AclAct"), now)? != 0.);
    }
    let gear_value = if flags & (f::HYBRID | f::EV) != 0 {
        let value = pt(state, ("ELECT_GEAR", "Elect_Gear_Shifter"), now)?;
        let step = pt(state, ("ELECT_GEAR", "Elect_Gear_Step"), now)?;
        ret.set_gear_step(if state.config.candidate == "HYUNDAI_CASPER_EV" {
            0
        } else {
            int16(step)?
        });
        value
    } else if flags & f::FCEV != 0 {
        pt(state, ("EMS20", "HYDROGEN_GEAR_SHIFTER"), now)?
    } else if flags & f::CLUSTER_GEARS != 0 {
        let value = pt(state, ("CLU15", "CF_Clu_Gear"), now)?;
        if state.config.candidate == "KIA_K7" {
            ret.set_gear_step(int16(pt(state, ("LVR11", "CF_Lvr_GearInf"), now)?)?);
        }
        value
    } else if flags & f::TCU_GEARS != 0 {
        pt(state, ("TCU12", "CUR_GR"), now)?
    } else {
        let value = pt(state, ("LVR12", "CF_Lvr_Gear"), now)?;
        ret.set_gear_step(int16(pt(state, ("LVR11", "CF_Lvr_GearInf"), now)?)?);
        value
    };
    if state.config.candidate != "HYUNDAI_NEXO" {
        ret.set_gear_shifter(gear(state, gear_value)?);
    } else {
        let value = pt(state, ("ELECT_GEAR", "Elect_Gear_Shifter"), now)?;
        let decoded = if value == 1546. {
            GearShifter::Drive
        } else if value == 2314. {
            GearShifter::Neutral
        } else if value == 2569. {
            GearShifter::Park
        } else if value == 2566. {
            GearShifter::Reverse
        } else {
            GearShifter::Unknown
        };
        if decoded != GearShifter::Unknown && decoded != state.gear_shifter {
            state.gear_shifter = decoded;
        }
        ret.set_gear_shifter(state.gear_shifter);
    }
    Ok(())
}
