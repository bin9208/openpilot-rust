use super::{
    eps_scale, Error, ANGLE_CONTROL, DISABLE_RADAR, HYBRID, NO_DSU, NO_STOP_TIMER, RADAR_ACC,
    RAISED_ACCEL_LIMIT, SECOC, SNG_WITHOUT_DSU, TSS2, UNSUPPORTED_DSU,
};
use crate::{
    core::Message,
    firmware::{Ecu, Firmware},
    vehicle_params::{self, FinishOptions, TorqueOptions},
};
use num_traits::ToPrimitive;
use openpilot_cereal::car_capnp::car_params::{self, SafetyModel, SteerControlType};
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
    if platform.brand != "toyota" {
        return Err(Error::Platform(input.candidate.to_owned()));
    }
    let mut message = vehicle_params::baseline(input.candidate)?;
    let mut cp = message.get_root::<car_params::Builder>()?;
    let mut flags = platform.flags;
    let mut safety = eps_scale(input.candidate);
    cp.set_brand("toyota");
    if platform.dbc_pt.as_deref() == Some("toyota_new_mc_pt_generated") {
        safety |= 256;
    }
    if flags & SECOC != 0 {
        cp.set_sec_oc_required(true);
        safety |= 2048;
    }
    if flags & ANGLE_CONTROL != 0 {
        cp.set_steer_control_type(SteerControlType::Angle);
        safety |= 1024;
        cp.set_steer_actuator_delay(0.18);
        cp.set_steer_limit_timer(0.8);
    } else {
        vehicle_params::configure_torque(
            input.candidate,
            cp.reborrow().get_lateral_tuning(),
            TorqueOptions::default(),
        )?;
        cp.set_steer_actuator_delay(0.12);
        cp.set_steer_limit_timer(0.4);
    }
    let dsu = !input.firmware.is_empty()
        && !input.firmware.iter().any(|fw| fw.ecu == Ecu::Dsu)
        && flags & (NO_DSU | UNSUPPORTED_DSU) == 0;
    cp.set_enable_dsu(dsu);
    if input.firmware.iter().any(|fw| fw.ecu == Ecu::Hybrid) {
        flags |= HYBRID;
    }
    let mut stop_go = flags & TSS2 != 0;
    match input.candidate {
        "TOYOTA_PRIUS" => {
            stop_go = true;
            if input
                .firmware
                .iter()
                .any(|fw| fw.ecu == Ecu::Eps && fw.fw_version != b"8965B47060\0\0\0\0\0\0")
            {
                cp.set_steer_actuator_delay(0.25);
                vehicle_params::configure_torque(
                    input.candidate,
                    cp.reborrow().get_lateral_tuning(),
                    TorqueOptions {
                        deadzone_deg: 0.2,
                        use_steering_angle: true,
                    },
                )?;
            }
        }
        "LEXUS_RX" | "LEXUS_RX_TSS2" => {
            stop_go = true;
            cp.set_wheel_speed_factor(1.035);
        }
        "TOYOTA_AVALON" | "TOYOTA_AVALON_2019" | "TOYOTA_AVALON_TSS2" => {
            stop_go = input.candidate != "TOYOTA_AVALON"
        }
        "TOYOTA_RAV4_TSS2"
        | "TOYOTA_RAV4_TSS2_2022"
        | "TOYOTA_RAV4_TSS2_2023"
        | "TOYOTA_RAV4_PRIME"
        | "TOYOTA_SIENNA_4TH_GEN" => {
            let rack = input.firmware.iter().any(|fw| {
                fw.ecu == Ecu::Eps
                    && (fw.fw_version.starts_with(&[2])
                        || fw.fw_version == b"8965B42181\0\0\0\0\0\0")
            });
            let mut pid = cp.reborrow().get_lateral_tuning().init_pid();
            pid.reborrow().init_ki_b_p(1).set(0, 0.);
            pid.reborrow().init_kp_b_p(1).set(0, 0.);
            pid.reborrow()
                .init_kp_v(1)
                .set(0, if rack { 0.15 } else { 0.6 });
            pid.reborrow()
                .init_ki_v(1)
                .set(0, if rack { 0.05 } else { 0.1 });
            pid.set_kf(if rack { 0.00004 } else { 0.00007818594 });
        }
        "TOYOTA_CHR" | "TOYOTA_CAMRY" | "TOYOTA_SIENNA" | "LEXUS_CTH" | "LEXUS_NX" => {
            stop_go = true
        }
        _ => {}
    }
    if flags & SNG_WITHOUT_DSU != 0 {
        stop_go |= dsu;
    }
    let wheelbase = f64::from(cp.reborrow_as_reader().get_wheelbase());
    cp.set_center_to_front((wheelbase * 0.44).to_f32().ok_or(Error::Numeric)?);
    cp.set_enable_bsm(
        flags & TSS2 != 0
            && input
                .fingerprints
                .iter()
                .any(|(bus, rows)| *bus == 0 && rows.iter().any(|(address, _)| *address == 0x3f6)),
    );
    cp.set_radar_unavailable(
        platform.dbc_radar.is_none() || (flags & NO_DSU != 0 && flags & TSS2 == 0),
    );
    if flags & (RADAR_ACC | NO_DSU) != 0 {
        cp.set_alpha_longitudinal_available(flags & RADAR_ACC != 0);
        if input.alpha_long && flags & RADAR_ACC != 0 {
            flags |= DISABLE_RADAR;
        }
    }
    let long = flags & SECOC == 0
        && (dsu || (flags & TSS2 != 0 && flags & RADAR_ACC == 0) || flags & DISABLE_RADAR != 0);
    cp.set_openpilot_longitudinal_control(long);
    cp.set_auto_resume_sng(long && flags & NO_STOP_TIMER != 0);
    if !long {
        safety |= 512;
    }
    cp.set_min_enable_speed(if stop_go {
        -1.
    } else {
        (19_f64 * 0.44704).to_f32().ok_or(Error::Numeric)?
    });
    if flags & TSS2 != 0 {
        flags |= RAISED_ACCEL_LIMIT;
        cp.set_v_ego_stopping(0.25);
        cp.set_v_ego_starting(0.25);
        cp.set_stopping_decel_rate(0.3);
        if flags & HYBRID != 0 {
            cp.set_longitudinal_actuator_delay(0.05);
        }
    }
    cp.set_flags(flags);
    let mut config = cp.reborrow().init_safety_configs(1).get(0);
    config.set_safety_model(SafetyModel::Toyota);
    config.set_safety_param(safety);
    vehicle_params::finish(
        cp,
        input.settings,
        FinishOptions {
            firmware: input.firmware,
        },
    )?;
    Ok(message)
}
