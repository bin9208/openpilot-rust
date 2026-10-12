use crate::{
    worker::{Info, Input, Metadata},
    Error,
};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Pointer {
    pub version: u32,
    pub pickle_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    pub version: u32,
    pub format: String,
    pub model_sha256: String,
    pub pickle_sha256: String,
    pub pickle_bytes: u64,
    pub filename: String,
    pub checkpoint: String,
    pub scalar_inputs: Vec<Scalar>,
    pub output_count: usize,
    pub output_slices: BTreeMap<String, [usize; 2]>,
    pub cameras: Vec<Camera>,
    pub files: Vec<File>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Scalar {
    pub name: String,
    pub offset: usize,
    pub shape: Vec<usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Camera {
    pub camera: [u32; 2],
    pub graph: String,
    pub qcom_warp: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct File {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

pub(super) fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
}

impl Manifest {
    pub fn validate(&self, filename: &str) -> Result<(), Error> {
        if self.version != 1
            || self.format != "run-policy"
            || !digest(&self.model_sha256)
            || !digest(&self.pickle_sha256)
            || self.pickle_bytes == 0
            || self.pickle_bytes > 4 << 30
            || self.filename != filename
            || filename != format!("big_driving_{}_tinygrad.pkl", &self.model_sha256[..16])
            || self.files.is_empty()
            || self.files.len() > 16384
            || self.cameras.len() != 2
            || !self
                .cameras
                .iter()
                .map(|value| value.camera)
                .eq([[1928, 1208], [1344, 760]])
            || self.scalar_inputs.len() != 4
        {
            return Err(Error::Contract("local native package identity"));
        }
        let hidden = self
            .output_slices
            .get("hidden_state")
            .ok_or(Error::Contract("local hidden state missing"))?;
        let mut end = 0;
        for (input, name) in
            self.scalar_inputs
                .iter()
                .zip(["desire", "traffic_convention", "action_t", "prev_feat"])
        {
            if input.name != name
                || input.offset != end
                || input.shape.is_empty()
                || input.shape.len() > 16
                || input.shape.contains(&0)
            {
                return Err(Error::Contract("local packed scalar layout"));
            }
            let count = input
                .shape
                .iter()
                .try_fold(1usize, |total, size| total.checked_mul(*size))
                .filter(|count| *count <= 16 << 20)
                .ok_or(Error::Contract("local scalar shape limit"))?;
            if (name == "desire" && input.shape != [8])
                || (["traffic_convention", "action_t"].contains(&name) && input.shape != [1, 2])
                || (name == "prev_feat" && hidden[1].checked_sub(hidden[0]) != Some(count))
            {
                return Err(Error::Contract("local scalar/model metadata mismatch"));
            }
            end = end
                .checked_add(count * 4)
                .ok_or(Error::Contract("local packed bytes overflow"))?;
        }
        for camera in &self.cameras {
            self.info(camera.camera)?;
        }
        Ok(())
    }

    pub fn info(&self, camera: [u32; 2]) -> Result<Info, Error> {
        Info::local(
            Metadata {
                model_sha256: self.pickle_sha256.clone(),
                checkpoint: self.checkpoint.clone(),
                inputs: self
                    .scalar_inputs
                    .iter()
                    .take(3)
                    .map(|value| Input {
                        name: value.name.clone(),
                        shape: value.shape.clone(),
                    })
                    .collect(),
                output_count: self.output_count,
                output_slices: self
                    .output_slices
                    .iter()
                    .map(|(name, [start, end])| (name.clone(), [Some(*start), Some(*end), None]))
                    .collect(),
            },
            camera,
        )
    }

    pub fn packed_bytes(&self) -> Result<usize, Error> {
        let last = self
            .scalar_inputs
            .last()
            .ok_or(Error::Contract("local scalar inputs missing"))?;
        let count = last
            .shape
            .iter()
            .try_fold(1usize, |total, size| total.checked_mul(*size))
            .ok_or(Error::Contract("local packed bytes overflow"))?;
        last.offset
            .checked_add(count * 4)
            .ok_or(Error::Contract("local packed bytes overflow"))
    }
}
