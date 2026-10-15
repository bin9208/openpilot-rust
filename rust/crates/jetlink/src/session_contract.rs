//! One immutable graph/wire contract per connected session.
use crate::{contract, model_spec::ModelSpec, wire::Version, Error};
use serde_json::Value;
#[derive(Clone, Debug)]
pub struct SessionContract {
    pub spec: ModelSpec,
    pub version: Version,
    raw: Value,
    packed: usize,
}
impl SessionContract {
    pub fn native_default() -> &'static Self {
        static NATIVE: std::sync::LazyLock<SessionContract> = std::sync::LazyLock::new(|| {
            SessionContract::native_legacy().expect("embedded native contract validated")
        });
        &NATIVE
    }
    pub fn native_legacy() -> Result<Self, Error> {
        Self::parse(contract::contract()?, Version::V2)
    }
    pub fn parse(raw: Value, version: Version) -> Result<Self, Error> {
        let spec = ModelSpec::parse(&serde_json::to_vec(&raw)?)?;
        // Only the existing, separately validated native contract retains its
        // historic stateful-v2 exception. Source Cinque v2 has another SHA.
        let packed = if version == Version::V2 && raw == contract::contract()? {
            contract::PACKED_FLOATS
        } else {
            spec.packed_count(version)?
        };
        Ok(Self {
            spec,
            version,
            raw,
            packed,
        })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn packed_count(&self) -> usize {
        self.packed
    }
    pub fn check_input(&self, warped: &[u8], packed: &[f32]) -> Result<(), Error> {
        if warped.len() != contract::WARPED_BYTES
            || packed.len() != self.packed
            || !packed.iter().all(|v| v.is_finite())
        {
            return Err(Error::Contract("session input shape/finite"));
        }
        Ok(())
    }
    pub fn check_output(&self, output: &[f32]) -> Result<(), Error> {
        if output.len() != self.spec.output_count()? || !output.iter().all(|v| v.is_finite()) {
            return Err(Error::Contract("session output shape/finite"));
        }
        Ok(())
    }
}
