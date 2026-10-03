use super::{
    controller::Controller,
    controller_control::Command,
    flags as f,
    legacy_acc::{self, LegacyAccInput, LegacySccMessages},
    legacy_steering::{self, LkasInput},
    state::State,
    wire::Values,
    Error,
};
use openpilot_can::Frame;
use openpilot_cereal::car_capnp::{car_control, car_state};
use std::collections::BTreeMap;

pub fn messages(
    controller: &mut Controller,
    input: (car_control::Reader<'_>, &mut State),
    context: (&Command, &BTreeMap<&str, Values>),
) -> Result<Vec<Frame>, Error> {
    let (cc, state) = input;
    let (command, captures) = context;
    let c = controller;
    let config = &state.config;
    let cs = state.out.get_root_as_reader::<car_state::Reader<'_>>()?;
    let hud = cc.get_hud_control()?;
    let mut result = Vec::new();
    if let Some(lkas) = captures.get("lkas11") {
        if c.lkas_active {
            result.push(legacy_steering::lkas(
                &mut c.writer,
                &LkasInput {
                    candidate: &config.candidate,
                    flags: config.flags,
                    frame: c.frame,
                    torque: command.torque,
                    steer_req: command.request,
                    torque_fault: cc.get_lat_active() && !command.request,
                    sys_warning: command.warning,
                    sys_state: command.sys_state,
                    enabled: cc.get_enabled(),
                    left_lane: hud.get_left_lane_visible(),
                    right_lane: hud.get_right_lane_visible(),
                    left_depart: command.lane_warning[0],
                    right_depart: command.lane_warning[1],
                    ldws_car: c.settings.ldws,
                },
                lkas,
            )?);
        }
        c.lkas_active = true;
    }
    if !config.longitudinal {
        result.extend(c.buttons.messages(
            &mut c.writer,
            (cc, state),
            (&c.settings, c.frame, captures),
        )?);
    }
    if matches!(
        config.candidate.as_str(),
        "GENESIS_G90" | "GENESIS_G90_2019" | "KIA_K9" | "KIA_K7" | "KIA_K7_PE"
    ) {
        if let Some(mdps) = captures.get("mdps12") {
            let mut mdps = mdps.clone();
            result.push(legacy_steering::mdps(&mut c.writer, &mut mdps, c.frame)?);
        }
    }
    if c.frame.is_multiple_of(2) && config.longitudinal {
        c.jerk((cc, state), command)?;
        let i = LegacyAccInput {
            enabled: cc.get_enabled(),
            accel: command.accel,
            index: c.frame / 2,
            gap: f64::from(hud.get_lead_distance_bars()),
            lead_visible: hud.get_lead_visible(),
            lead_distance: f64::from(hud.get_lead_distance()),
            lead_speed: f64::from(hud.get_lead_rel_speed()),
            set_speed: command.speed,
            stopping: command.stopping,
            long_override: cc.get_cruise_control()?.get_override(),
            available: cs.get_cruise_state()?.get_available(),
            brake_hold: cs.get_brake_hold_active(),
            brake_pressed: cs.get_brake_pressed(),
            paddle: state.paddle,
            soft_hold: u8::try_from(state.soft_hold).map_err(|_| Error::Numeric)?,
            soft_hold_mode: c.settings.soft_hold_mode,
            carrot_cruise: c.jerk.carrot_cruise,
            carrot_accel: c.jerk.carrot_accel,
            band_upper: c.jerk.band_upper,
            band_lower: c.jerk.band_lower,
            jerk_u: c.jerk.jerk_u,
            jerk_l: c.jerk.jerk_l,
            use_fca: config.flags & f::USE_FCA != 0,
            flags: config.flags,
            casper_fca: config.candidate == "HYUNDAI_CASPER_EV",
        };
        let source = if config.flags & f::CAMERA_SCC != 0 {
            Some(LegacySccMessages {
                scc11: captures.get("scc11"),
                scc12: captures.get("scc12"),
                scc14: captures.get("scc14"),
                fca11: captures.get("fca11"),
            })
        } else {
            None
        };
        result.extend(legacy_acc::commands(&mut c.writer, &i, source)?);
    }
    if c.frame.is_multiple_of(5) && config.flags & f::SEND_LFA != 0 {
        result.push(legacy_steering::lfa_mfc(
            &mut c.writer,
            [cc.get_lat_active(), cc.get_enabled(), c.steering.blink],
            hud.get_active_carrot(),
        )?);
    }
    if c.frame.is_multiple_of(20) && config.longitudinal && config.flags & f::CAMERA_SCC == 0 {
        result.extend(legacy_acc::options(&mut c.writer, config.flags)?);
    }
    if c.frame.is_multiple_of(50) && config.longitudinal && config.flags & f::CAMERA_SCC == 0 {
        result.push(legacy_acc::radar_option(&mut c.writer)?);
    }
    Ok(result)
}
