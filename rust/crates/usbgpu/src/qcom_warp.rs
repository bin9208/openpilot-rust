use crate::{warp_validation::IMAGE_BYTES, Error};
use openpilot_model_runtime::qcom::QcomModel;
use std::path::Path;

pub struct LocalWarp(QcomModel);
impl LocalWarp {
    /// # Safety
    /// The directory must contain trusted immutable source warp kernels matching the native argument and memory contract. Checksums do not authenticate code.
    pub unsafe fn load(directory: &Path) -> Result<Self, Error> {
        // SAFETY: the caller guarantees trusted immutable kernels and QcomBundle validates their argument and buffer layout.
        Ok(Self(unsafe { QcomModel::load(directory, 8) }?))
    }
    pub fn prepare(&mut self, frames: &[u8], transforms: &[u8]) -> Result<Vec<u8>, Error> {
        self.0.write_input("frames", frames)?;
        self.0.write_input("transforms", transforms)?;
        self.0.run()?;
        let output = self.0.read_output("new_img")?;
        if output.len() != IMAGE_BYTES {
            return Err(Error::Contract("QCOM warp output layout mismatch"));
        }
        Ok(output.to_vec())
    }
}
