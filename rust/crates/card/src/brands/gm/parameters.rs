use super::{float, model::Model, parameters_model, Error};
use crate::{
    brands::hyundai::settings_float,
    core::Message,
    firmware::Firmware,
    vehicle_params::{self, FinishOptions},
};
use openpilot_cereal::car_capnp::car_params::{
    self, NetworkLocation, SafetyModel, TransmissionType,
};
use openpilot_params::Params;

pub struct ParamsInput<'a> {
    pub candidate: &'a str,
    pub fingerprints: &'a [(u8, Vec<(u32, usize)>)],
    pub firmware: &'a [Firmware],
    pub alpha_long: bool,
    pub settings: &'a Params,
}
pub(super) fn tuning(
    cp: &mut car_params::Builder<'_>,
    kp_bp: &[f32],
    kp_v: &[f32],
    ki_bp: &[f32],
    ki_v: &[f32],
) -> Result<(), Error> {
    let mut t = cp.reborrow().get_longitudinal_tuning()?;
    let mut set = t
        .reborrow()
        .init_kp_b_p(u32::try_from(kp_bp.len()).map_err(|_| Error::Numeric)?);
    for (i, v) in kp_bp.iter().enumerate() {
        set.set(u32::try_from(i).map_err(|_| Error::Numeric)?, *v);
    }
    let mut set = t
        .reborrow()
        .init_kp_v(u32::try_from(kp_v.len()).map_err(|_| Error::Numeric)?);
    for (i, v) in kp_v.iter().enumerate() {
        set.set(u32::try_from(i).map_err(|_| Error::Numeric)?, *v);
    }
    let mut set = t
        .reborrow()
        .init_ki_b_p(u32::try_from(ki_bp.len()).map_err(|_| Error::Numeric)?);
    for (i, v) in ki_bp.iter().enumerate() {
        set.set(u32::try_from(i).map_err(|_| Error::Numeric)?, *v);
    }
    let mut set = t
        .reborrow()
        .init_ki_v(u32::try_from(ki_v.len()).map_err(|_| Error::Numeric)?);
    for (i, v) in ki_v.iter().enumerate() {
        set.set(u32::try_from(i).map_err(|_| Error::Numeric)?, *v);
    }
    Ok(())
}
pub fn parameters(input: ParamsInput<'_>) -> Result<Message, Error> {
    let model = Model::new(input.candidate)?;
    let mut message = vehicle_params::baseline(input.candidate)?;
    let mut cp = message.get_root::<car_params::Builder>()?;
    cp.set_brand("gm");
    cp.reborrow()
        .init_safety_configs(1)
        .get(0)
        .set_safety_model(SafetyModel::Gm);
    cp.set_auto_resume_sng(false);
    let has = |bus, address| {
        input
            .fingerprints
            .iter()
            .any(|(b, rows)| *b == bus && rows.iter().any(|(a, _)| *a == address))
    };
    cp.set_enable_bsm(has(0, 0x142) || has(2, 0x142));
    cp.set_start_accel(1.);
    cp.set_radar_time_step(0.067);
    cp.set_alternative_experience(0);
    let ev_tables = input.settings.get_bool("EVTable")?;
    let pedal = has(0, 0x201);
    cp.set_enable_gas_interceptor_d_e_p_r_e_c_a_t_e_d(pedal);
    let mut safety: u16 = if pedal { 128 } else { 0 };
    let mut flags = cp.reborrow_as_reader().get_flags();
    cp.set_transmission_type(if model.ev {
        TransmissionType::Direct
    } else {
        TransmissionType::Automatic
    });
    if model.camera || model.sdgm {
        cp.set_alpha_longitudinal_available(!model.sdgm);
        cp.set_network_location(NetworkLocation::FwdCamera);
        cp.set_radar_unavailable(true);
        cp.set_pcm_cruise(true);
        safety |= 1;
        cp.set_min_enable_speed(float(-1. / 3.6)?);
        cp.set_min_steer_speed(float(10. * (1. / 3.6))?);
        let kp = cp
            .reborrow_as_reader()
            .get_longitudinal_tuning()?
            .get_kp_v()?
            .iter()
            .collect::<Vec<_>>();
        tuning(&mut cp, &[0.], &kp, &[0.], &[1.7])?;
        cp.set_stopping_decel_rate(2.);
        cp.set_v_ego_stopping(0.5);
        cp.set_v_ego_starting(0.4);
        cp.set_stop_accel(-0.4);
        cp.set_starting_state(true);
        if input.alpha_long {
            cp.set_pcm_cruise(false);
            cp.set_openpilot_longitudinal_control(true);
            safety |= 2;
        }
    } else {
        cp.set_openpilot_longitudinal_control(true);
        cp.set_network_location(NetworkLocation::Gateway);
        cp.set_radar_unavailable(false);
        cp.set_pcm_cruise(false);
        cp.set_min_enable_speed(float(-(1.609344 * (1. / 3.6)))?);
        cp.set_min_steer_speed(float(
            (if ev_tables { 6.7 } else { 7. }) * (1.609344 * (1. / 3.6)),
        )?);
        tuning(&mut cp, &[0.], &[1.], &[0.], &[0.3])?;
        if pedal {
            safety |= 16;
        }
    }
    cp.set_steer_actuator_delay(0.28);
    cp.set_steer_limit_timer(0.4);
    cp.set_longitudinal_actuator_delay(float(
        settings_float::read(input.settings, "LongActuatorDelay")? * 0.01,
    )?);
    parameters_model::configure(
        &model,
        &mut cp,
        input.settings,
        ev_tables,
        pedal,
        &mut flags,
    )?;
    if pedal {
        cp.set_network_location(NetworkLocation::FwdCamera);
        safety |= 1;
        cp.set_min_enable_speed(-1.);
        cp.set_pcm_cruise(false);
        cp.set_openpilot_longitudinal_control(true);
        cp.set_auto_resume_sng(true);
        if model.cc {
            flags |= 1;
            safety |= 64;
            tuning(
                &mut cp,
                &[0., 3., 6., 35.],
                &[0.08, 0.175, 0.225, 0.33],
                &[0., 35.],
                &[0.07, 0.07],
            )?;
            cp.reborrow().get_longitudinal_tuning()?.set_kf(0.25);
            cp.set_stopping_decel_rate(0.8);
        } else {
            safety |= 2;
            cp.set_starting_state(true);
            cp.set_v_ego_stopping(0.25);
            cp.set_v_ego_starting(0.25);
        }
    } else if model.cc {
        flags |= 2;
        safety |= 4;
        if input.alpha_long {
            cp.set_openpilot_longitudinal_control(true);
        }
        cp.set_radar_unavailable(true);
        cp.set_alpha_longitudinal_available(true);
        cp.set_min_enable_speed(float(24. * (1.609344 * (1. / 3.6)))?);
        cp.set_pcm_cruise(true);
        cp.set_stopping_decel_rate(11.18);
        let t = cp.reborrow_as_reader().get_longitudinal_tuning()?;
        let kp_bp = t.get_kp_b_p()?.iter().collect::<Vec<_>>();
        let kp = t.get_kp_v()?.iter().collect::<Vec<_>>();
        tuning(&mut cp, &kp_bp, &kp, &[10.7, 10.8, 28.], &[0., 20., 20.])?;
    }
    if model.cc {
        safety |= 32;
    }
    if (cp.reborrow_as_reader().get_network_location()? == NetworkLocation::FwdCamera || model.cc)
        && !has(2, 0x320)
        && !model.sdgm
    {
        flags |= 4;
        safety |= 8;
    }
    if !has(0, 0xbe) {
        flags |= 8;
    }
    if has(0, 608) {
        flags |= 16;
    }
    cp.set_flags(flags);
    cp.reborrow()
        .get_safety_configs()?
        .get(0)
        .set_safety_param(safety);
    vehicle_params::finish(
        cp,
        input.settings,
        FinishOptions {
            firmware: input.firmware,
        },
    )?;
    Ok(message)
}
