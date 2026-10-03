use super::Error;
use crate::vehicle_params::bytes_repr;
use openpilot_cereal::car_capnp::car_params;
use openpilot_control_policy::{flux::Flux, numerics::Numerics, similarity};
use openpilot_params::Params;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct Common {
    pub use_nnff: bool,
    pub use_nnff_lite: bool,
    pub model_path: Option<PathBuf>,
    model: Option<(Flux, Numerics)>,
}
impl Common {
    pub fn new(
        cp: car_params::Reader<'_>,
        settings: &Params,
        assets: &Path,
        numerics: &Path,
    ) -> Result<Self, Error> {
        settings.put("LongitudinalPersonalityMax", b"3")?;
        let candidate = cp.get_car_fingerprint()?.to_str()?;
        let mut firmware = String::new();
        for fw in cp.get_car_fw()? {
            if fw.get_ecu()? == car_params::Ecu::Eps {
                firmware = bytes_repr(fw.get_fw_version()?);
                break;
            }
        }
        let comma: serde_json::Value =
            serde_json::from_slice(&fs::read(assets.join("neural_ff_weights.json"))?)?;
        let directory = assets.join("lat_models");
        println!("########get_nn_model_path : {candidate} {firmware}");
        let files = fs::read_dir(&directory)?
            .map(|entry| {
                let entry = entry?;
                entry
                    .file_name()
                    .into_string()
                    .map_err(|_| std::io::Error::other("feedforward filename is not Unicode"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !files.iter().any(|file| file.ends_with(".json")) {
            return Err(Error::EmptyModels);
        }
        let model_path =
            similarity::select(&files, candidate, &firmware).map(|file| directory.join(file));
        let model = if let Some(path) = &model_path {
            let bytes = fs::read(path)?;
            let raw: serde_json::Value = serde_json::from_slice(&bytes)?;
            if raw.get("output_size").is_none() {
                return Err(Error::ModelOutputSize);
            }
            let kernel = Numerics::load(numerics)?;
            Some((Flux::decode(&bytes, &kernel)?, kernel))
        } else {
            None
        };
        let use_nnff =
            comma.get(candidate).is_none() && model.is_some() && settings.get_bool("NNFF")?;
        let use_nnff_lite = !use_nnff && settings.get_bool("NNFFLite")?;
        Ok(Self {
            use_nnff,
            use_nnff_lite,
            model_path,
            model,
        })
    }
    pub fn evaluate(&self, input: &[f64]) -> Result<Option<f64>, Error> {
        self.model
            .as_ref()
            .map(|(model, kernel)| model.evaluate(input, kernel))
            .transpose()
            .map_err(Into::into)
    }
    pub fn friction_override(&self) -> Option<bool> {
        self.model
            .as_ref()
            .map(|(model, _)| model.friction_override)
    }
}
