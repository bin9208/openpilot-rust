use super::{Error, AUTO_SPEED_LIMIT, FSD_14, HAS_VEHICLE_BUS, MISSING_DAS_SETTINGS};
use crate::{
    core::Message,
    firmware::{Ecu, Firmware},
    vehicle_params::{self, FinishOptions},
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
fn contains(fingerprints: &[(u8, Vec<(u32, usize)>)], bus: u8, address: u32) -> bool {
    fingerprints
        .iter()
        .any(|(b, frames)| *b == bus && frames.iter().any(|(a, _)| *a == address))
}
pub fn parameters(input: ParamsInput<'_>) -> Result<Message, Error> {
    let mut message = vehicle_params::baseline(input.candidate)?;
    let platform = vehicle_params::platform(input.candidate)?;
    let mut cp = message.get_root::<car_params::Builder>()?;
    cp.set_brand("tesla");
    cp.set_steer_limit_timer(0.4);
    cp.set_steer_actuator_delay(0.1);
    cp.set_steer_at_standstill(true);
    cp.set_steer_control_type(SteerControlType::Angle);
    let mut flags = cp.reborrow_as_reader().get_flags();
    let mut safety = 0;
    if !contains(input.fingerprints, 2, 0x293) {
        flags |= MISSING_DAS_SETTINGS;
    }
    cp.set_radar_unavailable(
        !contains(input.fingerprints, 1, 0x410) || platform.dbc_radar.is_none(),
    );
    let vehicle = contains(input.fingerprints, 1, 0x3df);
    if vehicle {
        flags |= HAS_VEHICLE_BUS;
    }
    cp.set_alpha_longitudinal_available(true);
    if input.alpha_long {
        cp.set_openpilot_longitudinal_control(true);
        safety |= 1;
        if vehicle {
            flags |= AUTO_SPEED_LIMIT;
            safety |= 4;
        }
        cp.set_v_ego_stopping(0.1);
        cp.set_v_ego_starting(0.1);
        cp.set_stopping_decel_rate(0.3);
    }
    let versions: &[&[u8]] = match input.candidate {
        "TESLA_MODEL_3" => &[
            b"TeMYG4_Main_0.0.0 (77),E4HP015.04.5",
            b"TeMYG4_Main_0.0.0 (78),E4HP015.05.0",
        ],
        "TESLA_MODEL_Y" => &[
            b"TeMYG4_Legacy3Y_0.0.0 (6),Y4003.04.0",
            b"TeMYG4_Main_0.0.0 (77),Y4003.05.4",
        ],
        _ => &[],
    };
    if input
        .firmware
        .iter()
        .any(|fw| fw.ecu == Ecu::Eps && versions.contains(&fw.fw_version.as_slice()))
    {
        flags |= FSD_14;
        safety |= 2;
    }
    cp.set_flags(flags);
    cp.set_dashcam_only(input.candidate == "TESLA_MODEL_X");
    let mut config = cp.reborrow().init_safety_configs(1).get(0);
    config.set_safety_model(SafetyModel::Tesla);
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
