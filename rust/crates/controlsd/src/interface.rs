use crate::{
    config::{Config, Torque, Tuning},
    parameters::Parameters,
    Error,
};
use openpilot_control_policy::{
    flux::Flux,
    identity::{Identity, Interface},
    math::{divide, interp},
    nano::Nano,
    numerics::Numerics,
    similarity,
};
use std::{collections::BTreeMap, fs, path::Path};

pub struct ControlInterface {
    pub identity: Identity,
    pub flux: Option<Flux>,
    pub nano: Option<Nano>,
    pub use_nnff: bool,
    pub use_nnff_lite: bool,
    pub numerics: Numerics,
}
impl ControlInterface {
    pub fn new(
        config: &Config,
        params: &mut impl Parameters,
        assets: &Path,
        numerics: Numerics,
    ) -> Result<Self, Error> {
        let identity = Identity::lookup(&config.fingerprint)?;
        constructor_parameters(config, identity.interface, params)?;
        params.put_integer("LongitudinalPersonalityMax", 3)?;
        let neural: BTreeMap<String, serde_json::Value> =
            serde_json::from_slice(&fs::read(assets.join("neural_ff_weights.json"))?)?;
        println!(
            "########get_nn_model_path : {} {}",
            config.fingerprint, config.firmware
        );
        let files = fs::read_dir(assets.join("lat_models"))?
            .map(|entry| {
                entry?
                    .file_name()
                    .into_string()
                    .map_err(|_| Error::Contract("non-UTF8 model name"))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        if !files.iter().any(|file| file.ends_with(".json")) {
            return Err(Error::Contract("empty torque model directory"));
        }
        let flux = similarity::select(&files, &config.fingerprint, &config.firmware)
            .map(|file| {
                Ok::<_, Error>(Flux::decode(
                    &fs::read(assets.join("lat_models").join(file))?,
                    &numerics,
                )?)
            })
            .transpose()?;
        let use_nnff = !neural.contains_key(&config.fingerprint)
            && flux.is_some()
            && params.boolean("NNFF")?;
        let use_nnff_lite = !use_nnff && params.boolean("NNFFLite")?;
        let nano = if identity.interface == Interface::Gm
            && !config.angle
            && matches!(&config.tuning, Tuning::Torque(_))
        {
            neural
                .get(&config.fingerprint)
                .cloned()
                .map(serde_json::from_value)
                .transpose()?
        } else {
            None
        };
        Ok(Self {
            identity,
            flux,
            nano,
            use_nnff,
            use_nnff_lite,
            numerics,
        })
    }
    pub fn neural(&self, inputs: &[f64]) -> Result<f64, Error> {
        Ok(self
            .flux
            .as_ref()
            .ok_or(Error::Contract("missing selected Flux model"))?
            .evaluate(inputs, &self.numerics)?)
    }
    pub fn torque(
        &self,
        mut inputs: [f64; 4],
        tuning: &Torque,
        error: f64,
        deadzone: f64,
        compensate: bool,
        gravity: bool,
    ) -> Result<f64, Error> {
        let error = if error > -deadzone && error < deadzone {
            0.
        } else {
            error
        };
        let friction = interp(
            error,
            &[-0.3, 0.3],
            &[-f64::from(tuning.friction), f64::from(tuning.friction)],
        )?;
        let friction = if compensate { friction } else { 0. };
        let output = if let Some(nano) = &self.nano {
            if gravity {
                inputs[0] += inputs[1];
            }
            nano.predict(&inputs, &self.numerics)?
        } else if self.identity.interface == Interface::Gm && self.identity.siglin.is_some() {
            let [a, b, c, _] = self
                .identity
                .siglin
                .ok_or(Error::Contract("GM sigmoid parameters"))?;
            let value = inputs[0] * a;
            let sigmoid = if value >= 0. {
                1. / (1. + (-value).exp()) - 0.5
            } else {
                let z = value.exp();
                z / (1. + z) - 0.5
            };
            sigmoid * b + inputs[0] * c
        } else {
            divide(inputs[0], f64::from(tuning.factor))?
        };
        Ok(output + friction)
    }
}
fn constructor_parameters(
    config: &Config,
    interface: Interface,
    params: &mut impl Parameters,
) -> Result<(), Error> {
    match interface {
        Interface::Psa => {
            return Err(Error::Contract(
                "PSA startup unsupported: missing source DBC (#156)",
            ));
        }
        Interface::Hyundai => {
            params.integer("HyundaiCameraSCC")?;
            params.put_boolean("HyundaiCameraSccHint", false)?;
            params.integer("AutoEngage")?;
            params.integer("VehicleSpeedCameraDistanceTime")?;
            params.integer("VehicleNaviCanControl")?;
            params.boolean("VehicleNaviSchoolZoneControl")?;
            let fingerprints = params
                .string("FingerPrints")?
                .ok_or(Error::Contract("missing FingerPrints"))?;
            crate::fingerprints::validate(&fingerprints, config, params)?;
            if config.flags & 1 != 0 && config.flags & 8192 != 0 {
                params.integer("HyundaiCameraSCC")?;
            }
            if config.flags & 1 != 0 {
                params.integer("HyundaiCameraSCC")?;
            }
            params.integer("HyundaiCameraSCC")?;
            params.boolean("IsLdwsCar")?;
            params.integer("PaddleMode")?;
        }
        Interface::Tesla => {
            params.boolean("DisableMinSteerSpeed")?;
        }
        Interface::Gm => {
            params.integer("AutoEngage")?;
            params.boolean("DisengageOnAccelerator")?;
            params.boolean("SoftHoldOnCancel")?;
            params.integer("AutoEngage")?;
            params.integer("UseLaneLineSpeed")?;
        }
        _ => {}
    }
    Ok(())
}
