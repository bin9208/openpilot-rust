use super::{
    parser_inputs::Channel,
    state::State,
    state_fields::{buttons, byte, extend, CRUISE_BUTTONS},
    Error,
};
use openpilot_cereal::car_capnp::car_state::button_event::Type;

pub fn update(state: &mut State, now: u64) -> Result<Vec<(bool, Type)>, Error> {
    let previous = state.cruise_buttons.back().copied().ok_or(Error::Numeric)?;
    let previous_main = state.main_buttons.back().copied().ok_or(Error::Numeric)?;
    let cruise = if (state.capabilities.alt_lfa_button
        && state
            .inputs
            .signal(Channel::Pt, "CRUISE_BUTTON_LFA", "CruiseSwLfa", now)?
            > 0.)
        || (state.capabilities.lfa_button
            && state
                .inputs
                .signal(Channel::Pt, "BCM_PO_11", "LFA_Pressed", now)?
                == 1.)
    {
        vec![5]
    } else if state.capabilities.alt_cruise_button {
        vec![byte(state.inputs.signal(
            Channel::Pt,
            "CRUISE_BUTTON_ALT",
            "CruiseSwState",
            now,
        )?)?]
    } else {
        state
            .inputs
            .all(Channel::Pt, "CLU11", "CF_Clu_CruiseSwState", now)?
            .into_iter()
            .map(byte)
            .collect::<Result<Vec<_>, _>>()?
    };
    extend(&mut state.cruise_buttons, cruise);
    let main = if state.capabilities.alt_cruise_button {
        state
            .inputs
            .all(Channel::Pt, "CRUISE_BUTTON_ALT", "CruiseSwMain", now)?
    } else {
        state
            .inputs
            .all(Channel::Pt, "CLU11", "CF_Clu_CruiseSwMain", now)?
    };
    extend(
        &mut state.main_buttons,
        main.into_iter().map(byte).collect::<Result<Vec<_>, _>>()?,
    );
    let current = state.cruise_buttons.back().copied().ok_or(Error::Numeric)?;
    let current_main = state.main_buttons.back().copied().ok_or(Error::Numeric)?;
    let mut events = buttons(current, previous, CRUISE_BUTTONS);
    events.extend(buttons(
        current_main,
        previous_main,
        &[(1, Type::MainCruise)],
    ));
    state
        .inputs
        .captures
        .insert("mdps12", (Channel::Pt, "MDPS12"));
    if previous_main == 0 && current_main != 0 {
        state.main_enabled = !state.main_enabled;
    }
    Ok(events)
}
