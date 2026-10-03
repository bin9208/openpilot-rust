use super::{
    flags as f,
    parser_inputs::Channel,
    state::State,
    state_fields::{float32, int16},
    Error,
};
use openpilot_cereal::car_capnp::car_state;
use openpilot_control_policy::math::clip;

pub fn update(state: &mut State, mut ret: car_state::Builder<'_>, now: u64) -> Result<(), Error> {
    let mut cruise = ret.reborrow().init_cruise_state();
    cruise.set_available(state.main_enabled && state.monitor.count >= 200);
    let avh = state
        .inputs
        .signal(Channel::Pt, "ESP_STATUS", "AVH_Sta", now)?;
    let lamp = state
        .inputs
        .signal(Channel::Pt, "ESP_STATUS", "AVH_LAMP", now)?;
    state.scc_hold = avh == 1.;
    if lamp == 2. {
        state.avh_latched = true;
        state.avh_grace = 50;
    } else if state.avh_latched && avh == 2. {
        state.avh_grace = 50;
    } else if state.avh_latched {
        state.avh_grace = state.avh_grace.saturating_sub(1);
        if state.avh_grace == 0 {
            state.avh_latched = false;
        }
    } else {
        state.avh_grace = 0;
    }
    if state.config.longitudinal {
        cruise.set_enabled(state.inputs.signal(Channel::Pt, "TCS", "ACC_REQ", now)? == 1.);
        cruise.set_standstill(false);
    } else {
        let channel = if state.config.flags & f::CAMERA_SCC != 0 {
            Channel::Cam
        } else {
            Channel::Pt
        };
        cruise.set_enabled([1., 2.].contains(&state.inputs.signal(
            channel,
            "SCC_CONTROL",
            "ACCMode",
            now,
        )?));
        if state
            .inputs
            .signal(channel, "SCC_CONTROL", "MainMode_ACC", now)?
            == 1.
        {
            state.main_enabled = true;
            cruise.set_available(true);
            ret.set_pcm_cruise_gap(int16(clip(
                state
                    .inputs
                    .signal(channel, "SCC_CONTROL", "DISTANCE_SETTING", now)?,
                1.,
                4.,
            ))?);
        }
        let mut cruise = ret.reborrow().get_cruise_state()?;
        cruise.set_standstill(
            state
                .inputs
                .signal(channel, "SCC_CONTROL", "InfoDisplay", now)?
                >= 4.,
        );
        cruise.set_speed(float32(
            state
                .inputs
                .signal(channel, "SCC_CONTROL", "VSetDis", now)?
                * if state.metric { 1. / 3.6 } else { 0.44704 },
        )?);
    }
    ret.set_brake_hold_active(state.avh_latched);
    Ok(())
}
