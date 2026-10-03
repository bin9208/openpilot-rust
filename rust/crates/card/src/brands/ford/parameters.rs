use super::Error;
use crate::{
    core::{Message, VehicleLog},
    firmware::{Ecu, Firmware},
    query::DiagnosticLevel,
    vehicle_params::{self, FinishOptions},
};
use openpilot_cereal::car_capnp::car_params::{
    self, SafetyModel, SteerControlType, TransmissionType,
};
use openpilot_params::Params;

pub struct ParamsInput<'a> {
    pub candidate: &'a str,
    pub fingerprints: &'a [(u8, Vec<(u32, usize)>)],
    pub firmware: &'a [Firmware],
    pub alpha_long: bool,
    pub settings: &'a Params,
}

fn offset(input: &ParamsInput<'_>) -> u8 {
    input
        .fingerprints
        .iter()
        .filter(|(_, rows)| !rows.is_empty())
        .map(|(bus, _)| *bus / 4 * 4)
        .max()
        .unwrap_or(0)
}

pub fn parameter_logs(input: &ParamsInput<'_>) -> Result<Vec<VehicleLog>, Error> {
    let platform = vehicle_params::platform(input.candidate)?;
    let warning = if platform.flags & 1 != 0 {
        let camera = input
            .fingerprints
            .iter()
            .find(|(bus, _)| *bus == offset(input) + 2);
        camera
            .filter(|(_, rows)| !rows.is_empty())
            .and_then(|(_, rows)| {
                let size = |address| rows.iter().find(|(a, _)| *a == address).map(|(_, n)| *n);
                (size(0x3d6) != Some(8) || size(0x186) != Some(8))
                    .then_some("dashcamOnly: SecOC is unsupported")
            })
    } else {
        input
            .firmware
            .iter()
            .find(|fw| fw.ecu == Ecu::Eps && fw.request.iter().any(|r| r == b"\x22\xde\x01"))
            .and_then(|fw| {
                if fw.fw_version.len() != 24 {
                    Some("dashcamOnly: Invalid EPS FW version")
                } else if fw.fw_version[7] != 0xff || fw.fw_version[8] != 0xff {
                    Some("dashcamOnly: Car lacks required lateral control APIs")
                } else {
                    None
                }
            })
    };
    Ok(warning
        .into_iter()
        .map(|message| VehicleLog {
            level: DiagnosticLevel::Error,
            message: message.into(),
        })
        .collect())
}

fn prepare(input: &ParamsInput<'_>) -> Result<Message, Error> {
    let platform = vehicle_params::platform(input.candidate)?;
    if platform.brand != "ford" {
        return Err(Error::Platform(input.candidate.into()));
    }
    let mut message = vehicle_params::baseline(input.candidate)?;
    let mut cp = message.get_root::<car_params::Builder>()?;
    cp.set_brand("ford");
    let radar_unavailable = platform.dbc_radar.is_none();
    cp.set_radar_unavailable(radar_unavailable);
    cp.set_steer_control_type(SteerControlType::Angle);
    cp.set_steer_actuator_delay(0.2);
    cp.set_steer_limit_timer(1.);
    cp.set_steer_at_standstill(true);
    let mut tuning = cp.reborrow().get_longitudinal_tuning()?;
    tuning.reborrow().init_ki_b_p(1).set(0, 0.);
    tuning.init_ki_v(1).set(0, 0.5);
    if platform.dbc_radar.as_deref() == Some("FORD_CADS") {
        cp.set_radar_delay(0.06);
    }
    let main = offset(input);
    let count = if main >= 4 { 2 } else { 1 };
    let mut safeties = cp.reborrow().init_safety_configs(count);
    if count == 2 {
        safeties
            .reborrow()
            .get(0)
            .set_safety_model(SafetyModel::NoOutput);
    }
    let mut safety = safeties.get(count - 1);
    safety.set_safety_model(SafetyModel::Ford);
    let longitudinal = input.alpha_long || !radar_unavailable;
    safety.set_safety_param(u16::from(longitudinal) | if platform.flags & 1 != 0 { 2 } else { 0 });
    cp.set_alpha_longitudinal_available(radar_unavailable);
    if longitudinal {
        cp.set_openpilot_longitudinal_control(true);
    }
    if !parameter_logs(input)?.is_empty() {
        cp.set_dashcam_only(true);
    }
    let has = |address| {
        input
            .fingerprints
            .iter()
            .any(|(bus, rows)| *bus == main && rows.iter().any(|(a, _)| *a == address))
    };
    if input.firmware.iter().any(|fw| fw.ecu == Ecu::ShiftByWire) || has(0x5a) {
        cp.set_transmission_type(TransmissionType::Automatic);
    } else {
        cp.set_transmission_type(TransmissionType::Manual);
        cp.set_min_enable_speed(super::state::float(20. * (1.609344 * (1. / 3.6)))?);
    }
    cp.set_enable_bsm(has(0x3a6) && has(0x3a7));
    cp.set_min_steer_speed(0.);
    cp.set_auto_resume_sng(cp.reborrow_as_reader().get_min_enable_speed() == -1.);
    cp.set_center_to_front(super::state::float(
        f64::from(cp.reborrow_as_reader().get_wheelbase()) * 0.44,
    )?);
    Ok(message)
}

pub fn parameters(input: ParamsInput<'_>) -> Result<Message, Error> {
    let mut message = prepare(&input)?;
    vehicle_params::finish(
        message.get_root::<car_params::Builder>()?,
        input.settings,
        FinishOptions {
            firmware: input.firmware,
        },
    )?;
    Ok(message)
}

pub fn parameters_logged(
    input: ParamsInput<'_>,
    mut emit: impl FnMut(&VehicleLog) -> Result<(), crate::core::Error>,
) -> Result<Message, crate::core::Error> {
    let mut message = prepare(&input)?;
    for log in parameter_logs(&input)? {
        emit(&log)?;
    }
    vehicle_params::finish(
        message.get_root::<car_params::Builder>()?,
        input.settings,
        FinishOptions {
            firmware: input.firmware,
        },
    )?;
    Ok(message)
}
