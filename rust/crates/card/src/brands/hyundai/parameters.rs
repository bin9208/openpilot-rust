use super::{
    bus::Fingerprints,
    detection::{detect, DetectionInput},
    flags, Error,
};
use crate::{
    firmware::Firmware,
    vehicle_params::{self, FinishOptions, TorqueOptions},
};
use capnp::message::{Builder, HeapAllocator};
use num_traits::ToPrimitive;
use openpilot_cereal::car_capnp::car_params::{self, SafetyModel, SteerControlType};
use openpilot_params::Params;

#[derive(Clone, Copy)]
pub struct ParamsInput<'a> {
    pub candidate: &'a str,
    pub fingerprints: &'a Fingerprints,
    pub firmware: &'a [Firmware],
    pub alpha_long: bool,
    pub is_release: bool,
    pub settings: &'a Params,
}

pub fn setting_int(settings: &Params, key: &str) -> Result<i32, Error> {
    match settings.get(key)? {
        Some(bytes) if !bytes.is_empty() => {
            let start = bytes
                .iter()
                .position(|byte| !byte.is_ascii_whitespace())
                .unwrap_or(bytes.len());
            let raw = bytes.get(start..).ok_or(Error::Numeric)?;
            let sign = usize::from(raw.first().is_some_and(|byte| matches!(byte, b'+' | b'-')));
            let digits = raw
                .iter()
                .skip(sign)
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            // Not a number (or out of range) reads as 0 like a missing value;
            // see settings_float::read for why this must not fail card.
            Ok(
                std::str::from_utf8(raw.get(..sign + digits).unwrap_or_default())
                    .ok()
                    .and_then(|text| text.parse().ok())
                    .unwrap_or(0),
            )
        }
        Some(_) | None => Ok(0),
    }
}

pub fn parameters(input: ParamsInput<'_>) -> Result<Builder<HeapAllocator>, Error> {
    let mut message = vehicle_params::baseline(input.candidate)?;
    let platform = vehicle_params::platform(input.candidate)?;
    let mut cp = message.get_root::<car_params::Builder<'_>>()?;
    let mut detection = detect(DetectionInput {
        candidate: input.candidate,
        fingerprints: input.fingerprints,
        flags: cp.reborrow_as_reader().get_flags(),
        has_radar_dbc: platform.dbc_radar.is_some(),
        camera_scc: setting_int(input.settings, "HyundaiCameraSCC")?,
        hda2: setting_int(input.settings, "CanfdHDA2")?,
        radar_tracks: setting_int(input.settings, "EnableRadarTracks")?,
        alpha_long: input.alpha_long,
    });
    if detection.flags & flags::CANFD != 0
        && detection.flags & flags::CAMERA_SCC != 0
        && input.settings.get_bool("HyundaiCanfdClusterDirectTx")?
    {
        detection.flags |= flags::CLUSTER_DIRECT_TX;
        detection.safety_param |= flags::safety::CLUSTER_DIRECT_TX;
    }
    // carrot interface.py (6af3073e3): select the Kick by the observed
    // protocol and harness, never the model name. Runtime CRC/counter/driver
    // checks still precede each new Kick request.
    let ecan_size = |address| super::bus::size(input.fingerprints, detection.bus.ecan, address);
    let kick = detection.flags & flags::CANFD != 0
        && super::spas::kick_compatible_with(
            ecan_size,
            detection.bus.ecan,
            detection.flags & flags::CAMERA_SCC != 0,
        )
        && input.settings.get_bool("CanfdBlinkerKick")?;
    if detection.flags & flags::CANFD != 0
        && detection.flags & flags::CAMERA_SCC != 0
        && super::bus::size(input.fingerprints, detection.bus.cam, 0x1ea) == Some(32)
    {
        detection.ext_flags |= flags::ext::CANFD_INDICATOR_HOLD;
    }
    if kick && input.settings.get_bool("CanfdBlinkerControl")? {
        // The Kick and the SPAS lease are exclusive; the Kick wins.
        input.settings.put_bool("CanfdBlinkerControl", false)?;
    }
    // carrot interface.py: opt-in SPAS indicators need both exact ECAN SPAS
    // messages and are exclusive with the HDA2 blinker path.
    if detection.flags & flags::CANFD != 0
        && input.settings.get_bool("CanfdBlinkerControl")?
        && {
            let size = |address| super::bus::size(input.fingerprints, detection.bus.ecan, address);
            super::spas::compatible_with(size) || super::spas::absent_with(size)
        }
        && detection.flags & flags::ENABLE_BLINKERS == 0
    {
        detection.flags |= flags::CANFD_SPAS_BLINKERS;
        detection.safety_param |= flags::safety::CANFD_SPAS_BLINKERS;
    }
    if kick {
        // panda admits the Kick ids only with this flag; read at fingerprint
        // time, so switching it on applies from the next onroad start.
        detection.safety_param |= flags::safety::CANFD_BLINKER_KICK;
    }
    cp.set_brand("hyundai");
    cp.set_flags(detection.flags);
    cp.set_ext_flags(detection.ext_flags);
    cp.set_alpha_longitudinal_available(true);
    cp.set_enable_bsm(detection.bsm);
    let extra_safety = detection.flags & flags::CANFD != 0 && detection.bus.ecan >= 4;
    let mut configs = cp
        .reborrow()
        .init_safety_configs(if extra_safety { 2 } else { 1 });
    if extra_safety {
        configs
            .reborrow()
            .get(0)
            .set_safety_model(SafetyModel::NoOutput);
    }
    let mut safety = configs.get(u32::from(extra_safety));
    safety.set_safety_model(detection.safety_model);
    safety.set_safety_param(detection.safety_param);
    let center = (f64::from(cp.reborrow_as_reader().get_wheelbase()) * 0.4)
        .to_f32()
        .ok_or(Error::Numeric)?;
    cp.set_center_to_front(center);
    cp.set_steer_actuator_delay(if input.candidate == "KIA_OPTIMA_G4_FL" {
        0.2
    } else {
        0.1
    });
    cp.set_steer_limit_timer(0.4);
    if detection.flags & flags::ANGLE_CONTROL != 0 {
        cp.set_steer_control_type(SteerControlType::Angle);
    } else {
        vehicle_params::configure_torque(
            input.candidate,
            cp.reborrow().get_lateral_tuning(),
            TorqueOptions {
                deadzone_deg: 0.,
                use_steering_angle: true,
            },
        )?;
    }
    cp.set_radar_unavailable(detection.radar_unavailable);
    cp.set_openpilot_longitudinal_control(detection.longitudinal);
    cp.set_radar_time_step(0.05);
    cp.set_pcm_cruise(!detection.longitudinal);
    cp.set_starting_state(false);
    cp.set_v_ego_starting(0.1);
    cp.set_start_accel(1.);
    cp.set_longitudinal_actuator_delay(0.5);
    let mut tuning = cp.reborrow().get_longitudinal_tuning()?;
    tuning.reborrow().init_kp_b_p(1).set(0, 0.);
    tuning.reborrow().init_kp_v(1).set(0, 1.);
    tuning.set_kf(1.);
    // carrot interface.py: only an explicitly enabled, ACC-less classic-CAN
    // adapter with the 0x201 sensor uses the Comma Pedal protocol.
    if let Some(profile) = super::pedal::profile(input.candidate) {
        let fp = input.fingerprints;
        let has = |bus: u8, address: u32| super::bus::size(fp, bus, address).is_some();
        if detection.flags & flags::CC_ONLY_CAR != 0
            && input.settings.get_bool("CommaPedalEnabled")?
            && detection.flags & (flags::CANFD | flags::LEGACY | flags::CAMERA_SCC) == 0
            && super::bus::size(fp, super::pedal::BUS, super::pedal::SENSOR_ADDR) == Some(6)
            && ![0, 2]
                .iter()
                .any(|bus| has(*bus, 0x420) || has(*bus, 0x421))
        {
            detection.flags |= flags::PEDAL_INTERCEPTOR;
            cp.set_flags(detection.flags);
            cp.set_enable_gas_interceptor_d_e_p_r_e_c_a_t_e_d(true);
            cp.set_alpha_longitudinal_available(true);
            cp.set_openpilot_longitudinal_control(true);
            cp.set_pcm_cruise(false);
            cp.set_radar_unavailable(true);
            cp.set_auto_resume_sng(false);
            cp.set_min_enable_speed(profile.min_enable_speed);
            let mut configs = cp.reborrow().get_safety_configs()?;
            let last = configs.len() - 1;
            let mut safety = configs.reborrow().get(last);
            safety.set_safety_param(
                safety.reborrow_as_reader().get_safety_param()
                    | flags::safety::LONG
                    | flags::safety::PEDAL_INTERCEPTOR
                    | profile.safety_profile,
            );
        }
    }
    // ClassicCgwBlinker (opt-in test switch): a classic-CAN car with the
    // BCM's 8-byte CGW1 on bus 0. Never with the pedal interceptor, whose
    // panda branch takes one exact safetyParam. Read at fingerprint time,
    // so switching it on applies from the next onroad start.
    if detection.flags & (flags::CANFD | flags::PEDAL_INTERCEPTOR) == 0
        && matches!(
            detection.safety_model,
            SafetyModel::Hyundai | SafetyModel::HyundaiLegacy
        )
        && super::bus::size(input.fingerprints, 0, super::cgw_kick::CGW1) == Some(8)
        && input.settings.get_bool("ClassicCgwBlinker")?
    {
        let mut configs = cp.reborrow().get_safety_configs()?;
        let last = configs.len() - 1;
        let mut safety = configs.reborrow().get(last);
        safety.set_safety_param(
            safety.reborrow_as_reader().get_safety_param() | flags::safety::CLASSIC_CGW_BLINKER,
        );
    }
    vehicle_params::finish(
        cp,
        input.settings,
        FinishOptions {
            firmware: input.firmware,
        },
    )?;
    Ok(message)
}

#[cfg(test)]
mod classic_cgw_tests {
    use super::*;

    /// The IONIQ PHEV's bus 0/1/2 ids (route 00000003 seg 0), sizes as logged.
    fn phev() -> Vec<(u8, Vec<(u32, usize)>)> {
        let bus0 = [
            0x7f, 0x130, 0x140, 0x153, 0x160, 0x220, 0x240, 0x251, 0x340, 0x371, 0x372, 0x381,
            0x386, 0x387, 0x389, 0x38d, 0x394, 0x410, 0x412, 0x420, 0x421, 0x483, 0x48c, 0x495,
            0x49f, 0x4a9, 0x4c9, 0x50a, 0x50b, 0x50c, 0x50e, 0x52a, 0x541, 0x544, 0x54b, 0x553,
            0x559, 0x563, 0x57f, 0x58b, 0x58c, 0x595, 0x596, 0x5a8, 0x5be, 0x5c4, 0x5ff,
        ]
        .map(|a| (a, 8usize))
        .to_vec();
        vec![(0, bus0.clone()), (1, vec![(0x201, 8)]), (2, bus0)]
    }

    fn param(candidate: &str, fingerprints: &[(u8, Vec<(u32, usize)>)], switch: bool) -> u16 {
        let temp = tempfile::tempdir().unwrap();
        let settings = Params::open(temp.path(), "d").unwrap();
        settings.put_bool("ClassicCgwBlinker", switch).unwrap();
        let message = parameters(ParamsInput {
            candidate,
            fingerprints,
            firmware: &[],
            alpha_long: false,
            is_release: false,
            settings: &settings,
        })
        .unwrap();
        let cp = message.get_root_as_reader::<car_params::Reader<'_>>().unwrap();
        let configs = cp.get_safety_configs().unwrap();
        configs.get(configs.len() - 1).get_safety_param()
    }

    #[test]
    fn only_the_switch_on_a_classic_car_with_cgw1_sets_the_bit() {
        let fp = phev();
        let bit = flags::safety::CLASSIC_CGW_BLINKER;
        let on = param("HYUNDAI_IONIQ_PHEV", &fp, true);
        let off = param("HYUNDAI_IONIQ_PHEV", &fp, false);
        assert_ne!(on & bit, 0);
        assert_eq!(off & bit, 0);
        assert_eq!(on & !bit, off, "nothing else changes");
        // No 8-byte CGW1 on bus 0: never.
        let without: Vec<_> = fp
            .iter()
            .map(|(bus, ids)| (*bus, ids.iter().copied().filter(|(a, _)| *a != 0x541).collect()))
            .collect();
        assert_eq!(param("HYUNDAI_IONIQ_PHEV", &without, true) & bit, 0);
    }

    #[test]
    fn never_on_a_canfd_car() {
        // CAN-FD ids plus an 8-byte 0x541 on bus 0: the switch is ignored.
        let fp = vec![(0, vec![(0x541, 8), (0x1ba, 32), (0x3c1, 8), (0x413, 8)]), (2, vec![])];
        assert_eq!(
            param("HYUNDAI_IONIQ_5_PE", &fp, true) & flags::safety::CLASSIC_CGW_BLINKER,
            0
        );
    }
}
