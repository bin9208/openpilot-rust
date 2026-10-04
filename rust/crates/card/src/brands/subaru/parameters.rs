use super::{Error, GLOBAL_GEN2, HYBRID, LKAS_ANGLE, PREGLOBAL, SEND_INFOTAINMENT};
use crate::{
    core::Message,
    firmware::Firmware,
    vehicle_params::{self, FinishOptions, TorqueOptions},
};
use openpilot_cereal::car_capnp::car_params::{self, SafetyModel, SteerControlType};
use openpilot_params::Params;

pub struct ParamsInput<'a> {
    pub candidate: &'a str,
    pub fingerprints: &'a [(u8, Vec<(u32, usize)>)],
    pub firmware: &'a [Firmware],
    pub alpha_long: bool,
    pub settings: &'a Params,
}
struct Pid<'a> {
    kf: f32,
    bp: &'a [f32],
    kp: &'a [f32],
    ki: &'a [f32],
}
fn pid(cp: &mut car_params::Builder<'_>, values: Pid<'_>) -> Result<(), Error> {
    let mut tuning = cp.reborrow().get_lateral_tuning().init_pid();
    tuning.set_kf(values.kf);
    let len = u32::try_from(values.bp.len()).map_err(|_| Error::Numeric)?;
    for (index, value) in values.bp.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| Error::Numeric)?;
        if index == 0 {
            tuning.reborrow().init_kp_b_p(len);
            tuning.reborrow().init_ki_b_p(len);
        }
        tuning.reborrow().get_kp_b_p()?.set(index, *value);
        tuning.reborrow().get_ki_b_p()?.set(index, *value);
    }
    let mut kp = tuning.reborrow().init_kp_v(len);
    for (index, value) in values.kp.iter().enumerate() {
        kp.set(u32::try_from(index).map_err(|_| Error::Numeric)?, *value);
    }
    let mut ki = tuning.reborrow().init_ki_v(len);
    for (index, value) in values.ki.iter().enumerate() {
        ki.set(u32::try_from(index).map_err(|_| Error::Numeric)?, *value);
    }
    Ok(())
}
pub fn parameters(input: ParamsInput<'_>) -> Result<Message, Error> {
    if vehicle_params::platform(input.candidate)?.brand != "subaru" {
        return Err(Error::Platform(input.candidate.to_owned()));
    }
    let mut message = vehicle_params::baseline(input.candidate)?;
    let mut cp = message.get_root::<car_params::Builder>()?;
    let mut flags = cp.reborrow_as_reader().get_flags();
    cp.set_brand("subaru");
    cp.set_radar_unavailable(true);
    cp.set_dashcam_only(flags & (PREGLOBAL | LKAS_ANGLE | HYBRID) != 0);
    cp.set_auto_resume_sng(false);
    let detected = |bus, address| {
        input
            .fingerprints
            .iter()
            .any(|(b, rows)| *b == bus && rows.iter().any(|(a, _)| *a == address))
    };
    if flags & PREGLOBAL == 0 && detected(2, 0x323) {
        flags |= SEND_INFOTAINMENT;
    }
    cp.set_flags(flags);
    cp.set_enable_bsm(detected(
        0,
        if flags & PREGLOBAL != 0 { 0x25c } else { 0x228 },
    ));
    let mut safety = cp.reborrow().init_safety_configs(1).get(0);
    safety.set_safety_model(if flags & PREGLOBAL != 0 {
        SafetyModel::SubaruPreglobal
    } else {
        SafetyModel::Subaru
    });
    let mut safety_param = u16::from(flags & GLOBAL_GEN2 != 0);
    cp.set_steer_limit_timer(0.4);
    cp.set_steer_actuator_delay(0.1);
    if flags & LKAS_ANGLE != 0 {
        cp.set_steer_control_type(SteerControlType::Angle);
    } else {
        vehicle_params::configure_torque(
            input.candidate,
            cp.reborrow().get_lateral_tuning(),
            TorqueOptions::default(),
        )?;
    }
    match input.candidate {
        "SUBARU_ASCENT" | "SUBARU_ASCENT_2023" => {
            cp.set_steer_actuator_delay(0.3);
            pid(
                &mut cp,
                Pid {
                    kf: 0.00003,
                    bp: &[0., 20.],
                    kp: &[0.0025, 0.1],
                    ki: &[0.00025, 0.01],
                },
            )?;
        }
        "SUBARU_IMPREZA" => {
            cp.set_steer_actuator_delay(0.4);
            pid(
                &mut cp,
                Pid {
                    kf: 0.00005,
                    bp: &[0., 20.],
                    kp: &[0.2, 0.3],
                    ki: &[0.02, 0.03],
                },
            )?;
        }
        "SUBARU_IMPREZA_2020" => pid(
            &mut cp,
            Pid {
                kf: 0.00005,
                bp: &[0., 14., 23.],
                kp: &[0.045, 0.042, 0.20],
                ki: &[0.04, 0.035, 0.045],
            },
        )?,
        "SUBARU_FORESTER" | "SUBARU_FORESTER_2022" | "SUBARU_FORESTER_HYBRID" => pid(
            &mut cp,
            Pid {
                kf: 0.000038,
                bp: &[0., 14., 23.],
                kp: &[0.01, 0.065, 0.2],
                ki: &[0.001, 0.015, 0.025],
            },
        )?,
        "SUBARU_FORESTER_PREGLOBAL" | "SUBARU_OUTBACK_PREGLOBAL_2018" => safety_param = 4,
        "SUBARU_LEGACY_PREGLOBAL" => cp.set_steer_actuator_delay(0.15),
        "SUBARU_CROSSTREK_HYBRID"
        | "SUBARU_OUTBACK"
        | "SUBARU_LEGACY"
        | "SUBARU_OUTBACK_2023"
        | "SUBARU_OUTBACK_PREGLOBAL" => {}
        unknown => return Err(Error::Platform(unknown.to_owned())),
    }
    let available = flags & (GLOBAL_GEN2 | PREGLOBAL | LKAS_ANGLE | HYBRID) == 0;
    let longitudinal = input.alpha_long && available;
    cp.set_alpha_longitudinal_available(available);
    cp.set_openpilot_longitudinal_control(longitudinal);
    if longitudinal {
        safety_param |= 2;
    }
    cp.reborrow()
        .get_safety_configs()?
        .get(0)
        .set_safety_param(safety_param);
    vehicle_params::finish(
        cp,
        input.settings,
        FinishOptions {
            firmware: input.firmware,
        },
    )?;
    Ok(message)
}
