use crate::{policy::Inputs, Error};
use openpilot_params::Params;
use openpilot_runtime_version::BuildMetadata;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub enum ParamsSource {
    Runtime,
    Isolated { root: PathBuf, prefix: String },
}
impl ParamsSource {
    pub fn open(&self) -> Result<Params, Error> {
        Ok(match self {
            Self::Runtime => Params::for_runtime()?,
            Self::Isolated { root, prefix } => Params::open(root, prefix)?,
        })
    }
}
pub struct RuntimeInputs {
    pub base: PathBuf,
    pub params: ParamsSource,
    pub pc: bool,
    pub device_override: Option<String>,
}
impl RuntimeInputs {
    pub fn new(base: PathBuf) -> Self {
        Self {
            base,
            params: ParamsSource::Runtime,
            pc: !Path::new("/TICI").is_file(),
            device_override: None,
        }
    }
}
impl Inputs for RuntimeInputs {
    fn build_metadata(&mut self) -> Result<BuildMetadata, Error> {
        Ok(openpilot_runtime_version::get_build_metadata(&self.base)?)
    }
    fn params(&mut self) -> Result<Params, Error> {
        self.params.open()
    }
    fn is_pc(&self) -> bool {
        self.pc
    }
    fn version(&mut self) -> Result<String, Error> {
        Ok(openpilot_runtime_version::get_version(&self.base)?)
    }
    fn device_type(&mut self) -> Result<String, Error> {
        if let Some(device) = &self.device_override {
            return Ok(device.clone());
        }
        if self.pc {
            return Ok("pc".into());
        }
        let text = String::from_utf8(std::fs::read("/sys/firmware/devicetree/base/model")?)?;
        Ok(text
            .trim_matches('\0')
            .rsplit("comma ")
            .next()
            .unwrap_or("")
            .into())
    }
}
