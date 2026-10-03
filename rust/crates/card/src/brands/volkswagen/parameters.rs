use super::{
    Error, ALT_GEAR, MEB, MEB_GEN2, PQ, STOCK_EA_PRESENT, STOCK_HCA_PRESENT, STOCK_KLR_PRESENT,
};
use crate::{
    core::Message,
    firmware::Firmware,
    vehicle_params::{self, FinishOptions, TorqueOptions},
};
use openpilot_cereal::car_capnp::car_params::{
    self, NetworkLocation, SafetyModel, SteerControlType, TransmissionType,
};
use openpilot_params::Params;
pub struct ParamsInput<'a> {
    pub candidate: &'a str,
    pub fingerprints: &'a [(u8, Vec<(u32, usize)>)],
    pub firmware: &'a [Firmware],
    pub alpha_long: bool,
    pub settings: &'a Params,
}
pub fn parameters(input: ParamsInput<'_>) -> Result<Message, Error> {
    let platform = vehicle_params::platform(input.candidate)?;
    if platform.brand != "volkswagen" {
        return Err(Error::Platform(input.candidate.to_owned()));
    }
    let mut message = vehicle_params::baseline(input.candidate)?;
    let mut cp = message.get_root::<car_params::Builder>()?;
    let mut flags = platform.flags;
    let detected = |bus, address| {
        input
            .fingerprints
            .iter()
            .any(|(b, rows)| *b == bus && rows.iter().any(|(a, _)| *a == address))
    };
    cp.set_brand("volkswagen");
    cp.set_radar_unavailable(true);
    let (model, transmission, gateway) = if flags & PQ != 0 {
        cp.set_enable_bsm(detected(0, 0x3ba));
        cp.set_dashcam_only(true);
        (
            SafetyModel::VolkswagenPq,
            if detected(0, 0x440) {
                TransmissionType::Automatic
            } else {
                TransmissionType::Manual
            },
            [0x1a0, 0xc2].into_iter().any(|a| detected(1, a)),
        )
    } else if flags & MEB != 0 {
        cp.set_enable_bsm(detected(0, 0x24c));
        cp.set_steer_control_type(SteerControlType::Angle);
        cp.set_steer_at_standstill(true);
        if detected(0, 0x25d) {
            flags |= STOCK_KLR_PRESENT;
        }
        if detected(0, 0x3dc) {
            flags |= ALT_GEAR;
        }
        if detected(2, 0x1a4) && detected(2, 0x1f0) {
            flags |= STOCK_EA_PRESENT;
        }
        let gateway = [0x520, 0x86, 0xfd, 0x13d]
            .into_iter()
            .any(|a| detected(1, a));
        if gateway {
            cp.set_radar_unavailable(!detected(0, 0x24f));
        }
        (
            SafetyModel::VolkswagenMeb,
            TransmissionType::Direct,
            gateway,
        )
    } else {
        cp.set_enable_bsm(detected(0, 0x30f));
        if detected(2, 0x126) {
            flags |= STOCK_HCA_PRESENT;
        }
        (
            SafetyModel::Volkswagen,
            if detected(0, 0xad) {
                TransmissionType::Automatic
            } else if detected(0, 0x187) {
                TransmissionType::Direct
            } else {
                TransmissionType::Manual
            },
            [0x40, 0x86, 0xb2, 0xfd].into_iter().any(|a| detected(1, a)),
        )
    };
    cp.set_transmission_type(transmission);
    cp.set_network_location(if gateway {
        NetworkLocation::Gateway
    } else {
        NetworkLocation::FwdCamera
    });
    cp.set_steer_limit_timer(0.4);
    if flags & PQ != 0 {
        cp.set_steer_actuator_delay(0.2);
        vehicle_params::configure_torque(
            input.candidate,
            cp.reborrow().get_lateral_tuning(),
            TorqueOptions::default(),
        )?;
    } else if flags & MEB != 0 {
        cp.set_steer_actuator_delay(0.3);
    } else {
        cp.set_steer_actuator_delay(0.1);
        let mut pid = cp.reborrow().get_lateral_tuning().init_pid();
        pid.reborrow().init_kp_b_p(1).set(0, 0.);
        pid.reborrow().init_ki_b_p(1).set(0, 0.);
        pid.set_kf(0.00006);
        pid.reborrow().init_kp_v(1).set(0, 0.6);
        pid.init_ki_v(1).set(0, 0.2);
    }
    cp.set_alpha_longitudinal_available(gateway);
    let longitudinal = input.alpha_long && (gateway || flags & MEB == 0);
    cp.set_openpilot_longitudinal_control(longitudinal);
    if longitudinal && transmission == TransmissionType::Manual {
        cp.set_min_enable_speed(4.5);
    }
    cp.set_pcm_cruise(!longitudinal);
    cp.set_stop_accel(-0.55);
    cp.set_v_ego_starting(0.1);
    cp.set_v_ego_stopping(0.5);
    cp.set_auto_resume_sng(cp.reborrow_as_reader().get_min_enable_speed() == -1.);
    if flags & MEB != 0 {
        cp.set_starting_state(true);
        cp.set_start_accel(0.8);
        cp.set_v_ego_starting(0.5);
        cp.set_v_ego_stopping(0.1);
        cp.set_longitudinal_actuator_delay(0.5);
        cp.set_radar_delay(0.8);
        let mut tuning = cp.reborrow().get_longitudinal_tuning()?;
        let mut bp = tuning.reborrow().init_ki_b_p(2);
        bp.set(0, 0.);
        bp.set(1, 30.);
        let mut values = tuning.init_ki_v(2);
        values.set(0, 0.4);
        values.set(1, 0.);
    }
    cp.set_flags(flags);
    let mut safety = cp.reborrow().init_safety_configs(1).get(0);
    safety.set_safety_model(model);
    safety.set_safety_param(u16::from(longitudinal) | if flags & MEB_GEN2 != 0 { 2 } else { 0 });
    vehicle_params::finish(
        cp,
        input.settings,
        FinishOptions {
            firmware: input.firmware,
        },
    )?;
    Ok(message)
}
