use super::{Candidate, Error};
use crate::{
    core::Message,
    firmware::{Ecu, Firmware},
    vehicle_params::{self, FinishOptions, TorqueOptions},
};
use openpilot_cereal::car_capnp::car_params::{self, SafetyModel};
use openpilot_params::Params;

pub struct ParamsInput<'a> {
    pub candidate: &'a str,
    pub fingerprints: &'a [(u8, Vec<(u32, usize)>)],
    pub firmware: &'a [Firmware],
    pub alpha_long: bool,
    pub settings: &'a Params,
}
fn prefix(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.len().min(4)]
}
fn pid(cp: &mut car_params::Builder<'_>) {
    let mut tuning = cp.reborrow().get_lateral_tuning().init_pid();
    let mut kp_bp = tuning.reborrow().init_kp_b_p(2);
    kp_bp.set(0, 9.);
    kp_bp.set(1, 20.);
    let mut ki_bp = tuning.reborrow().init_ki_b_p(2);
    ki_bp.set(0, 9.);
    ki_bp.set(1, 20.);
    let mut kp = tuning.reborrow().init_kp_v(2);
    kp.set(0, 0.15);
    kp.set(1, 0.30);
    let mut ki = tuning.reborrow().init_ki_v(2);
    ki.set(0, 0.03);
    ki.set(1, 0.05);
    tuning.set_kf(0.00006);
}
pub fn parameters(input: ParamsInput<'_>) -> Result<Message, Error> {
    let candidate = Candidate::try_from(input.candidate)?;
    let mut message = vehicle_params::baseline(input.candidate)?;
    let mut cp = message.get_root::<car_params::Builder>()?;
    cp.set_brand("chrysler");
    cp.set_dashcam_only(matches!(candidate, Candidate::RamHd));
    cp.set_radar_unavailable(true);
    cp.set_steer_actuator_delay(0.1);
    cp.set_steer_limit_timer(0.4);
    let mut safety = cp.reborrow().init_safety_configs(1).get(0);
    safety.set_safety_model(SafetyModel::Chrysler);
    safety.set_safety_param(match candidate {
        Candidate::RamDt => 1,
        Candidate::RamHd => 2,
        Candidate::Pacifica2018Hybrid
        | Candidate::Pacifica2019Hybrid
        | Candidate::Pacifica2018
        | Candidate::Pacifica2020
        | Candidate::Durango
        | Candidate::GrandCherokee
        | Candidate::GrandCherokee2019 => 0,
    });
    vehicle_params::configure_torque(
        input.candidate,
        cp.reborrow().get_lateral_tuning(),
        TorqueOptions::default(),
    )?;
    let higher = !candidate.ram()
        && (candidate.higher_min()
            || input
                .firmware
                .iter()
                .any(|fw| fw.ecu == Ecu::Eps && prefix(&fw.fw_version) >= b"6841"));
    if higher {
        let flags = cp.reborrow_as_reader().get_flags();
        cp.set_flags(flags | 1);
    }
    match candidate {
        Candidate::Pacifica2018Hybrid
        | Candidate::Pacifica2019Hybrid
        | Candidate::Pacifica2018
        | Candidate::Pacifica2020
        | Candidate::Durango => pid(&mut cp),
        Candidate::GrandCherokee | Candidate::GrandCherokee2019 => {
            cp.set_steer_actuator_delay(0.2);
            pid(&mut cp);
        }
        Candidate::RamDt => {
            cp.set_steer_actuator_delay(0.2);
            cp.set_wheelbase(3.88);
            if input.firmware.iter().any(|fw| {
                fw.ecu == Ecu::Eps
                    && b"68".as_slice() < prefix(&fw.fw_version)
                    && prefix(&fw.fw_version) <= b"6831"
            }) {
                cp.set_min_steer_speed(0.);
            }
        }
        Candidate::RamHd => {
            cp.set_steer_actuator_delay(0.2);
            vehicle_params::configure_torque(
                input.candidate,
                cp.reborrow().get_lateral_tuning(),
                TorqueOptions {
                    deadzone_deg: 1.,
                    use_steering_angle: false,
                },
            )?;
        }
    }
    if cp.reborrow_as_reader().get_flags() & 1 != 0 {
        cp.set_min_steer_speed(17.5);
    }
    let wheelbase = f64::from(cp.reborrow_as_reader().get_wheelbase());
    cp.set_center_to_front((wheelbase * 0.44) as f32);
    cp.set_enable_bsm(
        input.fingerprints.iter().any(|(bus, messages)| {
            *bus == 0 && messages.iter().any(|(address, _)| *address == 720)
        }),
    );
    vehicle_params::finish(
        cp,
        input.settings,
        FinishOptions {
            firmware: input.firmware,
        },
    )?;
    Ok(message)
}
