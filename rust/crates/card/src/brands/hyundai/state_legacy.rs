use super::{
    flags as f,
    parser_inputs::Channel,
    state::State,
    state_fields::{self, float32, int16, timestamp},
    Error,
};
use openpilot_cereal::car_capnp::car_state;
use openpilot_control_policy::math::clip;

fn pt(state: &mut State, input: (&str, &str), now: u64) -> Result<f64, Error> {
    state.inputs.signal(Channel::Pt, input.0, input.1, now)
}

pub fn update(state: &mut State, mut ret: car_state::Builder<'_>, now: u64) -> Result<(), Error> {
    let flags = state.config.flags;
    let cruise_channel = if flags & f::CAMERA_SCC != 0 {
        Channel::Cam
    } else {
        Channel::Pt
    };
    state.metric = pt(state, ("CLU11", "CF_Clu_SPEED_UNIT"), now)? == 0.;
    let speed_unit = if state.metric { 1. / 3.6 } else { 0.44704 };
    let doors = [
        pt(state, ("CGW1", "CF_Gway_DrvDrSw"), now)?,
        pt(state, ("CGW1", "CF_Gway_AstDrSw"), now)?,
        pt(state, ("CGW2", "CF_Gway_RLDrSw"), now)?,
        pt(state, ("CGW2", "CF_Gway_RRDrSw"), now)?,
    ];
    ret.set_door_open(doors.into_iter().any(|v| v != 0.));
    ret.set_seatbelt_unlatched(pt(state, ("CGW1", "CF_Gway_DrvSeatBeltSw"), now)? == 0.);
    if timestamp(state, Channel::Pt, "EMS21", Some("SCR_UREA_LEVEL")) > 0 {
        ret.set_urea_gauge(float32(clip(
            pt(state, ("EMS21", "SCR_UREA_LEVEL"), now)? / 100.,
            0.,
            1.,
        ))?);
    }
    state_fields::wheels(
        state,
        ret.reborrow(),
        (
            "WHL_SPD11",
            ["WHL_SPD_FL", "WHL_SPD_FR", "WHL_SPD_RL", "WHL_SPD_RR"],
        ),
        now,
    )?;
    state.cluster_counter += 1;
    if state.cluster_counter > 20 {
        state.cluster_speed = pt(state, ("CLU15", "CF_Clu_VehicleSpeed"), now)?;
        state.cluster_counter = 0;
        if !state.metric && state.config.candidate != "KIA_SORENTO" {
            state.cluster_speed = (state.cluster_speed / 1.609344 + 1. / 1.609344).floor();
        }
    }
    ret.set_steering_angle_deg(float32(pt(state, ("SAS11", "SAS_Angle"), now)?)?);
    ret.set_steering_rate_deg(float32(pt(state, ("SAS11", "SAS_Speed"), now)?)?);
    ret.set_yaw_rate(float32(pt(state, ("ESP12", "YAW_RATE"), now)?)?);
    let lamps = [
        pt(state, ("CGW1", "CF_Gway_TurnSigLh"), now)? != 0.,
        pt(state, ("CGW1", "CF_Gway_TurnSigRh"), now)? != 0.,
    ];
    let blink = state.blinkers.lamp(50, lamps[0], lamps[1]);
    ret.set_left_blinker(blink[0]);
    ret.set_right_blinker(blink[1]);
    ret.set_steering_torque(float32(pt(state, ("MDPS12", "CR_Mdps_StrColTq"), now)?)?);
    ret.set_steering_torque_eps(float32(pt(state, ("MDPS12", "CR_Mdps_OutTq"), now)?)?);
    let pressed =
        f64::from(ret.reborrow_as_reader().get_steering_torque()).abs() > state.limits.threshold;
    ret.set_steering_pressed(state.steering_pressed.update(pressed, 5));
    ret.set_steer_fault_temporary(
        pt(state, ("MDPS12", "CF_Mdps_ToiUnavail"), now)? != 0.
            || pt(state, ("MDPS12", "CF_Mdps_ToiFlt"), now)? != 0.,
    );
    let mut cruise = ret.reborrow().init_cruise_state();
    if state.config.longitudinal {
        cruise.set_available(state.main_enabled && state.monitor.count >= 200);
        cruise.set_enabled(pt(state, ("TCS13", "ACC_REQ"), now)? == 1.);
        cruise.set_standstill(false);
        cruise.set_non_adaptive(false);
    } else if flags & f::CC_ONLY_CAR == 0 {
        state.main_enabled = state
            .inputs
            .signal(cruise_channel, "SCC11", "MainMode_ACC", now)?
            == 1.;
        cruise.set_available(state.main_enabled);
        cruise.set_enabled(
            state
                .inputs
                .signal(cruise_channel, "SCC12", "ACCMode", now)?
                != 0.,
        );
        cruise.set_standstill(
            state
                .inputs
                .signal(cruise_channel, "SCC11", "SCCInfoDisplay", now)?
                == 4.,
        );
        cruise.set_non_adaptive(
            state
                .inputs
                .signal(cruise_channel, "SCC11", "SCCInfoDisplay", now)?
                == 2.,
        );
        cruise.set_speed(float32(
            state
                .inputs
                .signal(cruise_channel, "SCC11", "VSetDis", now)?
                * speed_unit,
        )?);
        ret.set_pcm_cruise_gap(int16(state.inputs.signal(
            cruise_channel,
            "SCC11",
            "TauGapSet",
            now,
        )?)?);
    }
    ret.set_brake(0.);
    if flags & f::CC_ONLY_CAR == 0 {
        let brake = pt(state, ("TCS13", "DriverOverride"), now)? == 2.;
        ret.set_brake_pressed(brake);
        ret.set_brake_hold_active(pt(state, ("TCS15", "AVH_LAMP"), now)? == 2.);
        ret.set_parking_brake(pt(state, ("TCS13", "PBRAKE_ACT"), now)? == 1.);
        ret.set_esp_disabled(pt(state, ("TCS11", "TCS_PAS"), now)? == 1.);
        ret.set_esp_active(pt(state, ("TCS11", "ABS_ACT"), now)? == 1.);
        ret.set_acc_faulted(pt(state, ("TCS13", "ACCEnable"), now)? != 0.);
        ret.set_brake_lights(pt(state, ("TCS13", "BrakeLight"), now)? != 0. || brake);
    }
    super::legacy_powertrain::update(state, ret.reborrow(), now)?;
    if flags & f::CC_ONLY_CAR == 0 && (!state.config.longitudinal || flags & f::CAMERA_SCC != 0) {
        let (name, command) = if flags & f::USE_FCA != 0 {
            ("FCA11", "FCA_CmdAct")
        } else {
            ("SCC12", "AEB_CmdAct")
        };
        let mut warning = state
            .inputs
            .signal(cruise_channel, name, "CF_VSM_Warn", now)?
            != 0.;
        let scc_warning = state
            .inputs
            .signal(cruise_channel, "SCC12", "TakeOverReq", now)?
            == 1.;
        let mut braking = state
            .inputs
            .signal(cruise_channel, name, "CF_VSM_DecCmdAct", now)?
            != 0.
            || state.inputs.signal(cruise_channel, name, command, now)? != 0.;
        if state.config.candidate == "HYUNDAI_CASPER_EV" && name == "FCA11" {
            let fault = state
                .inputs
                .signal(cruise_channel, "FCA11", "FCA_Failinfo", now)?
                != 0.
                || state
                    .inputs
                    .signal(cruise_channel, "FCA11", "FCA_Status", now)?
                    == 3.;
            if fault {
                warning = false;
                braking = false;
            }
        }
        ret.set_stock_fcw((warning || scc_warning) && !braking);
        ret.set_stock_aeb(warning && braking);
    }
    if state.config.bsm {
        ret.set_left_blindspot(pt(state, ("LCA11", "CF_Lca_IndLeft"), now)? != 0.);
        ret.set_right_blindspot(pt(state, ("LCA11", "CF_Lca_IndRight"), now)? != 0.);
    }
    let events = super::legacy_buttons::update(state, now)?;
    state_fields::button_list(ret.reborrow(), &events)?;
    if flags & f::CC_ONLY_CAR == 0 {
        let unit = pt(state, ("TPMS11", "UNIT"), now)?;
        let multiplier = if unit.trunc() > 0. { unit * 0.725 } else { 1. };
        let pressures = [
            pt(state, ("TPMS11", "PRESSURE_FL"), now)?,
            pt(state, ("TPMS11", "PRESSURE_FR"), now)?,
            pt(state, ("TPMS11", "PRESSURE_RL"), now)?,
            pt(state, ("TPMS11", "PRESSURE_RR"), now)?,
        ];
        let mut tpms = ret.reborrow().init_tpms();
        tpms.set_fl(float32(multiplier * pressures[0])?);
        tpms.set_fr(float32(multiplier * pressures[1])?);
        tpms.set_rl(float32(multiplier * pressures[2])?);
        tpms.set_rr(float32(multiplier * pressures[3])?);
    }
    let mut speed = pt(state, ("CLU11", "CF_Clu_Vanz"), now)?;
    let decimal = pt(state, ("CLU11", "CF_Clu_VanzDecimal"), now)?;
    if decimal > 0. && decimal < 0.5 {
        speed += decimal;
    }
    state_fields::cluster_speed(state, ret.reborrow(), speed * speed_unit)?;
    let camera = if state.config.ext_flags & f::ext::NAVI_CLUSTER != 0 {
        let limit = pt(state, ("Navi_HU", "SpeedLim_Nav_Clu"), now)?;
        let camera = pt(state, ("Navi_HU", "SpeedLim_Nav_Cam"), now)? == 1.;
        ret.set_speed_limit(float32(if limit < 255. && camera { limit } else { 0. })?);
        camera
    } else {
        ret.set_speed_limit(0.);
        ret.set_speed_limit_distance(0.);
        false
    };
    super::state_navigation::update(state, ret, camera, false)
}
