use super::{bytes_repr, catalog, Error};
use crate::firmware::{Ecu, Firmware};
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;
use std::path::Path;

pub struct DiagnosticInput<'a> {
    pub params: car_params::Reader<'a>,
    pub firmware: &'a [Firmware],
    pub settings: &'a Params,
    pub assets: &'a Path,
}

pub fn parameter_diagnostics(input: DiagnosticInput<'_>) -> Result<Vec<String>, Error> {
    if input.params.get_steer_control_type()? == car_params::SteerControlType::Angle
        || !input.settings.get_bool("NNFF")?
    {
        return Ok(Vec::new());
    }
    let candidate = input.params.get_car_fingerprint()?.to_str()?;
    let firmware = input
        .firmware
        .iter()
        .find(|fw| fw.ecu == Ecu::Eps)
        .map(|fw| bytes_repr(&fw.fw_version))
        .unwrap_or_default();
    let mut lines = vec![format!(
        "########get_nn_model_path : {candidate} {firmware}"
    )];
    if let Some(file) = openpilot_control_policy::similarity::select(
        &catalog::catalog()?.ff_files,
        candidate,
        &firmware,
    ) {
        lines.push(format!(
            "NNFF loaded... {}",
            input.assets.join("lat_models").join(file).display()
        ));
    }
    Ok(lines)
}
