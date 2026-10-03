use crate::{
    brands::{
        body, chrysler, ford, gm, honda, hyundai, mazda, mock, nissan, psa, rivian, subaru, tesla,
        toyota, volkswagen,
    },
    core::{Error, Message},
    identification::Identification,
    vehicle_params,
};
use openpilot_params::Params;
use std::path::Path;

pub struct ParamsInput<'a> {
    pub identification: &'a Identification,
    pub settings: &'a Params,
    pub alpha_long: bool,
}

pub fn parameters_logged(
    input: ParamsInput<'_>,
    io: &mut impl crate::core::StepIo,
) -> Result<Message, Error> {
    if vehicle_params::platform(&input.identification.candidate)?.brand == "ford" {
        ford::parameters_logged(
            ford::ParamsInput {
                candidate: &input.identification.candidate,
                fingerprints: &input.identification.observed,
                firmware: &input.identification.firmware,
                alpha_long: input.alpha_long,
                settings: input.settings,
            },
            |log| io.vehicle_log(log),
        )
    } else {
        parameters(input.identification, input.settings, input.alpha_long)
    }
}

pub fn parameter_diagnostics(
    input: &ParamsInput<'_>,
    params: &Message,
    assets: &Path,
) -> Result<Vec<String>, Error> {
    let ParamsInput {
        identification,
        settings,
        alpha_long,
    } = input;
    let platform = vehicle_params::platform(&identification.candidate)?;
    let mut lines = if platform.brand == "hyundai" {
        hyundai::Hyundai::parameter_diagnostics(
            &hyundai::ParamsInput {
                candidate: &identification.candidate,
                fingerprints: &identification.observed,
                firmware: &identification.firmware,
                alpha_long: *alpha_long,
                is_release: true,
                settings,
            },
            params,
        )?
    } else {
        Vec::new()
    };
    lines.extend(vehicle_params::parameter_diagnostics(
        vehicle_params::DiagnosticInput {
            params: params.get_root_as_reader()?,
            firmware: &identification.firmware,
            settings,
            assets,
        },
    )?);
    Ok(lines)
}

pub fn parameters(
    identification: &Identification,
    settings: &Params,
    alpha_long: bool,
) -> Result<Message, Error> {
    let candidate = &identification.candidate;
    let platform = vehicle_params::platform(candidate)?;
    match platform.brand.as_str() {
        "body" => body::parameters(candidate, &identification.firmware, settings),
        "mock" => mock::parameters(candidate, &identification.firmware, settings),
        "hyundai" => Ok(hyundai::Hyundai::parameters(hyundai::ParamsInput {
            candidate,
            fingerprints: &identification.observed,
            firmware: &identification.firmware,
            alpha_long,
            is_release: true,
            settings,
        })?),
        "tesla" => Ok(tesla::parameters(tesla::ParamsInput {
            candidate,
            fingerprints: &identification.observed,
            firmware: &identification.firmware,
            alpha_long,
            settings,
        })?),
        "mazda" => Ok(mazda::parameters(mazda::ParamsInput {
            candidate,
            fingerprints: &identification.observed,
            firmware: &identification.firmware,
            alpha_long,
            settings,
        })?),
        "nissan" => Ok(nissan::parameters(nissan::ParamsInput {
            candidate,
            fingerprints: &identification.observed,
            firmware: &identification.firmware,
            alpha_long,
            settings,
        })?),
        "chrysler" => Ok(chrysler::parameters(chrysler::ParamsInput {
            candidate,
            fingerprints: &identification.observed,
            firmware: &identification.firmware,
            alpha_long,
            settings,
        })?),
        "rivian" => Ok(rivian::parameters(rivian::ParamsInput {
            candidate,
            fingerprints: &identification.observed,
            firmware: &identification.firmware,
            alpha_long,
            settings,
        })?),
        "ford" => Ok(ford::parameters(ford::ParamsInput {
            candidate,
            fingerprints: &identification.observed,
            firmware: &identification.firmware,
            alpha_long,
            settings,
        })?),
        "subaru" => Ok(subaru::parameters(subaru::ParamsInput {
            candidate,
            fingerprints: &identification.observed,
            firmware: &identification.firmware,
            alpha_long,
            settings,
        })?),
        "toyota" => Ok(toyota::parameters(toyota::ParamsInput {
            candidate,
            fingerprints: &identification.observed,
            firmware: &identification.firmware,
            alpha_long,
            settings,
        })?),
        "gm" => Ok(gm::parameters(gm::ParamsInput {
            candidate,
            fingerprints: &identification.observed,
            firmware: &identification.firmware,
            alpha_long,
            settings,
        })?),
        "psa" => Ok(psa::parameters(psa::ParamsInput {
            candidate,
            fingerprints: &identification.observed,
            firmware: &identification.firmware,
            alpha_long,
            settings,
        })?),
        "honda" => Ok(honda::parameters(honda::ParamsInput {
            candidate,
            fingerprints: &identification.observed,
            firmware: &identification.firmware,
            alpha_long,
            settings,
        })?),
        "volkswagen" => Ok(volkswagen::parameters(volkswagen::ParamsInput {
            candidate,
            fingerprints: &identification.observed,
            firmware: &identification.firmware,
            alpha_long,
            settings,
        })?),
        _ => Err(Error::UnsupportedVehicle {
            brand: platform.brand,
            candidate: candidate.clone(),
        }),
    }
}
