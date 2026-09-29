use super::{digest, Kind};
use crate::{
    entrypoint::Entrypoint,
    graph::{Binding, View},
    Error,
};
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    pub version: u32,
    pub kind: Kind,
    pub camera: [u32; 2],
    pub nv12: Nv12,
    pub metadata: Metadata,
    pub sources: BTreeMap<String, String>,
    pub inputs: Vec<TensorSpec>,
    pub outputs: Vec<TensorSpec>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Nv12 {
    pub stride: usize,
    pub y_height: usize,
    pub uv_height: usize,
    pub bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub model_checkpoint: String,
    pub input_shapes: BTreeMap<String, Vec<usize>>,
    pub output_shapes: BTreeMap<String, Vec<usize>>,
    pub output_slices: BTreeMap<String, [usize; 2]>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TensorSpec {
    pub name: String,
    pub shape: Vec<usize>,
    pub dtype: Dtype,
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub enum Dtype {
    #[serde(rename = "uint8")]
    Uint8,
    #[serde(rename = "float16")]
    Float16,
    #[serde(rename = "float32")]
    Float32,
}

impl Dtype {
    pub const fn item_size(self) -> usize {
        match self {
            Self::Uint8 => 1,
            Self::Float16 => 2,
            Self::Float32 => 4,
        }
    }
}

impl TensorSpec {
    pub fn bytes(&self) -> Result<usize, Error> {
        elements(&self.shape)?
            .checked_mul(self.dtype.item_size())
            .filter(|bytes| *bytes <= 4 * 1024 * 1024 * 1024)
            .ok_or(Error::Limit("pipeline tensor bytes"))
    }
}

pub(super) struct Contract<'a> {
    pub inputs: &'a [Binding],
    pub outputs: &'a [Binding],
    pub views: &'a [View],
    pub entries: &'a [Entrypoint],
}

fn elements(shape: &[usize]) -> Result<usize, Error> {
    if shape.is_empty() || shape.len() > 16 || shape.contains(&0) {
        return Err(Error::Contract("pipeline tensor shape"));
    }
    shape.iter().try_fold(1_usize, |total, size| {
        total
            .checked_mul(*size)
            .filter(|count| *count <= 1024 * 1024 * 1024)
            .ok_or(Error::Limit("pipeline tensor elements"))
    })
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 4096
}

impl Descriptor {
    pub(super) fn validate(&self, contract: &Contract<'_>) -> Result<(), Error> {
        let expected = match self.camera {
            [1928, 1208] => Nv12 {
                stride: 2048,
                y_height: 1216,
                uv_height: 608,
                bytes: 4804608,
            },
            [1344, 760] => Nv12 {
                stride: 1408,
                y_height: 768,
                uv_height: 384,
                bytes: 2428928,
            },
            _ => return Err(Error::Contract("pipeline camera resolution")),
        };
        if self.version != 1
            || self.nv12 != expected
            || self.sources.is_empty()
            || self.sources.len() > 16
            || self
                .sources
                .iter()
                .any(|(name, sha)| !valid_name(name) || !digest(sha))
        {
            return Err(Error::Contract(
                "pipeline descriptor version/layout/provenance",
            ));
        }
        let names = match self.kind {
            Kind::Driving => ["prepare", "policy"],
            Kind::Driver => ["prepare", "model"],
        };
        if !contract
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .eq(names)
        {
            return Err(Error::Contract("pipeline entrypoints"));
        }
        validate_tensors(&self.inputs, contract.inputs, contract.views)?;
        validate_tensors(&self.outputs, contract.outputs, contract.views)?;
        self.metadata.validate()?;
        let model = self
            .outputs
            .iter()
            .find(|tensor| tensor.name == "model")
            .ok_or(Error::Contract("pipeline model output"))?;
        if self.metadata.output_shapes.values().next() != Some(&model.shape) {
            return Err(Error::Contract("pipeline model output shape"));
        }
        Ok(())
    }
}

impl Metadata {
    fn validate(&self) -> Result<(), Error> {
        if !valid_name(&self.model_checkpoint)
            || self.output_shapes.len() != 1
            || self.output_slices.is_empty()
            || self.output_slices.len() > 1024
        {
            return Err(Error::Contract("pipeline metadata counts"));
        }
        for shapes in [&self.input_shapes, &self.output_shapes] {
            if shapes.is_empty() || shapes.len() > 1024 {
                return Err(Error::Contract("pipeline shape count"));
            }
            for (name, shape) in shapes {
                if !valid_name(name) {
                    return Err(Error::Contract("pipeline shape name"));
                }
                elements(shape)?;
            }
        }
        let size = elements(
            self.output_shapes
                .values()
                .next()
                .ok_or(Error::Contract("pipeline output shape"))?,
        )?;
        if self
            .output_slices
            .iter()
            .any(|(name, [start, end])| !valid_name(name) || start >= end || *end > size)
        {
            return Err(Error::Contract("pipeline output slice"));
        }
        Ok(())
    }
}

fn validate_tensors(
    specs: &[TensorSpec],
    bindings: &[Binding],
    views: &[View],
) -> Result<(), Error> {
    if specs.len() != bindings.len() || specs.len() > 1024 {
        return Err(Error::Contract("pipeline tensor count"));
    }
    let mut names = HashSet::new();
    for spec in specs {
        if !valid_name(&spec.name) || !names.insert(&spec.name) {
            return Err(Error::Contract("pipeline tensor names"));
        }
        let binding = bindings
            .iter()
            .find(|binding| binding.name == spec.name)
            .ok_or(Error::Contract("pipeline tensor binding"))?;
        if views[binding.view].bytes != spec.bytes()? {
            return Err(Error::Contract("pipeline tensor binding size"));
        }
    }
    Ok(())
}
