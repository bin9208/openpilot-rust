use super::{float, model::Model, parameters::tuning, Error};
use crate::vehicle_params::{self, TorqueOptions};
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;

pub(super) fn configure(
    model: &Model,
    cp: &mut car_params::Builder<'_>,
    settings: &Params,
    ev: bool,
    pedal: bool,
    flags: &mut u32,
) -> Result<(), Error> {
    let torque = |cp: &mut car_params::Builder<'_>| {
        vehicle_params::configure_torque(
            &model.name,
            cp.reborrow().get_lateral_tuning(),
            TorqueOptions::default(),
        )
    };
    match model.name.as_str() {
        "CHEVROLET_VOLT" | "CADILLAC_CT6_ACC" => {
            cp.set_steer_actuator_delay(if model.volt() && ev { 0.45 } else { 0.3 });
            tuning(
                cp,
                &[0.],
                &[1.],
                &[0.],
                &[if model.volt() { 0.35 } else { 0.3 }],
            )?;
            cp.reborrow().get_longitudinal_tuning()?.set_kf(1.);
            cp.set_stopping_decel_rate(if model.volt() && ev { 1. } else { 0.2 });
            cp.set_stop_accel(-0.5);
            cp.set_starting_state(true);
            cp.set_start_accel(if model.volt() {
                if ev {
                    0.6
                } else {
                    1.9
                }
            } else {
                1.5
            });
            if model.volt() {
                cp.set_v_ego_stopping(0.25);
                cp.set_v_ego_starting(0.15);
            }
            if settings.get_bool("LateralTorqueCustom")? {
                torque(cp)?;
            } else {
                let mut t = cp.reborrow().get_lateral_tuning().init_pid();
                let mut bp = t.reborrow().init_kp_b_p(2);
                bp.set(0, 0.);
                bp.set(1, 40.);
                let mut kp = t.reborrow().init_kp_v(2);
                kp.set(0, 0.);
                kp.set(1, 0.17);
                t.reborrow().init_ki_b_p(1).set(0, 0.);
                t.reborrow().init_ki_v(1).set(0, 0.);
                t.set_kf(1.);
            }
        }
        "GMC_ACADIA"
        | "CADILLAC_XT4"
        | "CHEVROLET_VOLT_2019"
        | "CHEVROLET_TRAVERSE"
        | "BUICK_BABYENCLAVE" => {
            cp.set_min_enable_speed(-1.);
            cp.set_steer_actuator_delay(0.2);
            torque(cp)?;
            if model.name == "CADILLAC_XT4" {
                cp.set_min_steer_speed(float(30. * (1.609344 * (1. / 3.6)))?);
            }
        }
        "CHEVROLET_MALIBU"
        | "CHEVROLET_MALIBU_CC"
        | "CHEVROLET_BOLT_EUV"
        | "CHEVROLET_BOLT_CC"
        | "CHEVROLET_TRAILBLAZER"
        | "CHEVROLET_TRAILBLAZER_CC"
        | "GMC_YUKON_CC"
        | "CADILLAC_XT5_CC" => {
            cp.set_steer_actuator_delay(0.2);
            torque(cp)?;
            if pedal
                && matches!(
                    model.name.as_str(),
                    "CHEVROLET_BOLT_EUV" | "CHEVROLET_BOLT_CC"
                )
            {
                *flags |= 1;
            }
        }
        "BUICK_LACROSSE" | "CHEVROLET_EQUINOX" | "CHEVROLET_EQUINOX_CC" | "CADILLAC_CT6_CC" => {
            torque(cp)?;
        }
        "CADILLAC_ESCALADE" | "CADILLAC_ESCALADE_ESV_2019" => {
            cp.set_min_enable_speed(-1.);
            if model.name == "CADILLAC_ESCALADE_ESV_2019" {
                cp.set_steer_actuator_delay(0.2);
            }
            torque(cp)?;
        }
        "CADILLAC_ESCALADE_ESV" => {
            cp.set_min_enable_speed(-1.);
            let mut t = cp.reborrow().get_lateral_tuning().init_pid();
            let mut bp = t.reborrow().init_kp_b_p(2);
            bp.set(0, 10.);
            bp.set(1, 41.);
            let mut bp = t.reborrow().init_ki_b_p(2);
            bp.set(0, 10.);
            bp.set(1, 41.);
            let mut kp = t.reborrow().init_kp_v(2);
            kp.set(0, 0.13);
            kp.set(1, 0.24);
            let mut ki = t.reborrow().init_ki_v(2);
            ki.set(0, 0.01);
            ki.set(1, 0.02);
            t.set_kf(0.000045);
        }
        "CHEVROLET_SILVERADO" => {
            if cp.reborrow_as_reader().get_openpilot_longitudinal_control() {
                cp.set_min_enable_speed(-1.);
            }
            torque(cp)?;
        }
        "CHEVROLET_SUBURBAN" | "CHEVROLET_SUBURBAN_CC" => {
            cp.set_steer_actuator_delay(0.075);
            torque(cp)?;
        }
        "CHEVROLET_TRAX" => {
            torque(cp)?;
            cp.set_stopping_decel_rate(0.3);
            cp.set_min_enable_speed(-1.);
            cp.set_stop_accel(-0.5);
            cp.set_starting_state(true);
            cp.set_start_accel(1.);
        }
        "GMC_YUKON" => {
            cp.set_steer_actuator_delay(0.5);
            torque(cp)?;
            cp.set_dashcam_only(true);
        }
        "HOLDEN_ASTRA" | "CADILLAC_ATS" | "BUICK_REGAL" | "CHEVROLET_VOLT_CC" => {}
        other => return Err(Error::Platform(other.into())),
    }
    Ok(())
}
