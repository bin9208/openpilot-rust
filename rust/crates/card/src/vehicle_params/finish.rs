use super::{catalog, configure_torque, Error, TorqueOptions};
use crate::firmware::{Ecu, Firmware};
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;

#[derive(Default)]
pub struct FinishOptions<'a> {
    pub firmware: &'a [Firmware],
}

pub fn finish(
    mut cp: car_params::Builder<'_>,
    settings: &Params,
    options: FinishOptions<'_>,
) -> Result<(), Error> {
    let candidate = cp
        .reborrow_as_reader()
        .get_car_fingerprint()?
        .to_str()?
        .to_owned();
    if cp.reborrow_as_reader().get_steer_control_type()? != car_params::SteerControlType::Angle
        && settings.get_bool("NNFF")?
    {
        configure_torque(
            &candidate,
            cp.reborrow().get_lateral_tuning(),
            TorqueOptions::default(),
        )?;
        let firmware = options
            .firmware
            .iter()
            .find(|fw| fw.ecu == Ecu::Eps)
            .map(|fw| bytes_repr(&fw.fw_version))
            .unwrap_or_default();
        if openpilot_control_policy::similarity::select(
            &catalog::catalog()?.ff_files,
            &candidate,
            &firmware,
        )
        .is_some()
        {
            settings.put("NNFFModelName", candidate.replace('_', " ").as_bytes())?;
        }
    }
    if settings.get_bool("DisableMinSteerSpeed")? {
        cp.set_min_steer_speed(0.);
    }
    let reader = cp.reborrow_as_reader();
    let mass = reader.get_mass();
    if !reader.get_not_car() {
        cp.set_mass(mass + 136.);
    }
    let reader = cp.reborrow_as_reader();
    let mass = f64::from(reader.get_mass());
    let wheelbase = f64::from(reader.get_wheelbase());
    let front = f64::from(reader.get_center_to_front());
    let factor = f64::from(reader.get_tire_stiffness_factor());
    cp.set_rotational_inertia(
        (2500. * mass * wheelbase.powi(2) / (1462. * 2.7_f64.powi(2))) as f32,
    );
    cp.set_tire_stiffness_front(
        ((192150. * factor) * mass / 1462. * ((wheelbase - front) / wheelbase)
            / ((2.7 - 2.7 * 0.4) / 2.7)) as f32,
    );
    cp.set_tire_stiffness_rear(
        ((202500. * factor) * mass / 1462. * (front / wheelbase) / ((2.7 * 0.4) / 2.7)) as f32,
    );
    Ok(())
}

pub(crate) fn bytes_repr(bytes: &[u8]) -> String {
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') {
        '"'
    } else {
        '\''
    };
    let mut result = format!("b{quote}");
    for &byte in bytes {
        match byte {
            b'\\' => result.push_str("\\\\"),
            b'\t' => result.push_str("\\t"),
            b'\n' => result.push_str("\\n"),
            b'\r' => result.push_str("\\r"),
            byte if byte == quote as u8 => {
                result.push('\\');
                result.push(quote);
            }
            32..=126 => result.push(char::from(byte)),
            _ => result.push_str(&format!("\\x{byte:02x}")),
        }
    }
    result.push(quote);
    result
}
