use super::{
    config::Bus, parameters_tuning, Error, BOSCH, BOSCH_ALT_BRAKE, BOSCH_EXT_HUD, BOSCH_RADARLESS,
    NIDEC_ALT_SCM_MESSAGES,
};
use crate::{
    core::Message,
    firmware::{Ecu, Firmware},
    vehicle_params::{self, FinishOptions},
};
use num_traits::ToPrimitive;
use openpilot_cereal::car_capnp::car_params::{self, SafetyModel, TransmissionType};
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
    if platform.brand != "honda" {
        return Err(Error::Platform(input.candidate.to_owned()));
    }
    let mut message = vehicle_params::baseline(input.candidate)?;
    let mut cp = message.get_root::<car_params::Builder>()?;
    let mut flags = platform.flags;
    let bosch = flags & BOSCH != 0;
    let radarless = flags & BOSCH_RADARLESS != 0;
    let offset = input
        .fingerprints
        .iter()
        .filter(|(_, rows)| !rows.is_empty())
        .map(|(bus, _)| *bus / 4 * 4)
        .max()
        .unwrap_or(0);
    let bus = Bus::new(offset, flags, false);
    let detected = |bus, address| {
        input
            .fingerprints
            .iter()
            .any(|(b, rows)| *b == bus && rows.iter().any(|(a, _)| *a == address))
    };
    cp.set_brand("honda");
    cp.set_openpilot_longitudinal_control(!bosch || input.alpha_long);
    if bosch {
        cp.set_radar_unavailable(true);
        cp.set_alpha_longitudinal_available(true);
        cp.set_pcm_cruise(!input.alpha_long);
        cp.set_longitudinal_actuator_delay(0.5);
        if radarless {
            cp.set_stop_accel(-3.5);
        }
    } else {
        cp.set_pcm_cruise(true);
        let mut tuning = cp.reborrow().get_longitudinal_tuning()?;
        let mut bp = tuning.reborrow().init_ki_b_p(3);
        for (index, value) in [0., 5., 35.].into_iter().enumerate() {
            bp.set(u32::try_from(index).map_err(|_| Error::Numeric)?, value);
        }
        let mut values = tuning.init_ki_v(3);
        for (index, value) in [1.2, 0.8, 0.5].into_iter().enumerate() {
            values.set(u32::try_from(index).map_err(|_| Error::Numeric)?, value);
        }
    }
    if input.candidate == "HONDA_CRV_5G" {
        cp.set_enable_bsm(detected(bus.radar, 0x12f8bfa7));
    }
    if input
        .fingerprints
        .iter()
        .any(|(_, rows)| rows.iter().any(|(a, _)| *a == 0x33da))
    {
        flags |= BOSCH_EXT_HUD;
    }
    if input.candidate == "HONDA_ACCORD" && detected(bus.pt, 0x191) {
        cp.set_transmission_type(TransmissionType::Cvt);
    } else if input.candidate == "HONDA_CIVIC_2022" {
        if !detected(bus.pt, 0x191) && !detected(bus.pt, 0x1a3) {
            cp.set_transmission_type(TransmissionType::Manual);
        } else if detected(bus.pt, 0x1a3) {
            cp.set_transmission_type(TransmissionType::Cvt);
        }
    }
    let modified = input
        .firmware
        .iter()
        .any(|fw| fw.ecu == Ecu::Eps && fw.fw_version.contains(&b','));
    parameters_tuning::tuning(&mut cp, input.candidate, modified)?;
    if detected(bus.pt, 0x1be) && matches!(input.candidate, "HONDA_ACCORD" | "HONDA_HRV_3G") {
        flags |= BOSCH_ALT_BRAKE;
    }
    cp.set_flags(flags);
    let mut safety = cp.reborrow().init_safety_configs(1).get(0);
    safety.set_safety_model(if bosch {
        SafetyModel::HondaBosch
    } else {
        SafetyModel::HondaNidec
    });
    safety.set_safety_param(
        u16::from(flags & BOSCH_ALT_BRAKE != 0)
            | if platform.flags & NIDEC_ALT_SCM_MESSAGES != 0 {
                4
            } else {
                0
            }
            | if bosch && input.alpha_long { 2 } else { 0 }
            | if radarless { 8 } else { 0 },
    );
    let resume = bosch || input.candidate == "HONDA_CIVIC";
    cp.set_auto_resume_sng(resume);
    cp.set_min_enable_speed(if resume {
        -1.
    } else {
        (25.51_f64 * (1.609344 * (1. / 3.6)))
            .to_f32()
            .ok_or(Error::Numeric)?
    });
    cp.set_steer_actuator_delay(0.1);
    cp.set_steer_limit_timer(0.8);
    cp.set_radar_delay(0.1);
    vehicle_params::finish(
        cp,
        input.settings,
        FinishOptions {
            firmware: input.firmware,
        },
    )?;
    Ok(message)
}
