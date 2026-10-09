use super::Error;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Read;

#[derive(Clone, Deserialize, Serialize)]
pub struct Artifact {
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

pub struct Catalog {
    pub value: Value,
    pub pickle: Artifact,
    pub runtime: Artifact,
    pub generic: bool,
}

use crate::model::authority;

fn cameras(value: &Value) -> bool {
    let Some(rows) = value.as_array() else {
        return false;
    };
    rows.len() == 2
        && rows
            .iter()
            .zip([[1928.0, 1208.0], [1344.0, 760.0]])
            .all(|(row, expected)| {
                row.as_array().is_some_and(|row| {
                    row.len() == 2
                        && row
                            .iter()
                            .zip(expected)
                            .all(|(number, expected)| number.as_f64() == Some(expected))
                })
            })
}

impl Catalog {
    pub fn parse(bytes: &[u8], model_sha: &str, catalog_url: &str) -> Result<Self, Error> {
        if bytes.len() > 65536 {
            return Err(Error::Invalid("precompiled catalog too large".into()));
        }
        let mut value: Value = serde_json::from_slice(bytes)?;
        if !value.is_object() {
            return Err(Error::Invalid(
                "precompiled catalog must be a JSON object".into(),
            ));
        }
        let generic = value["format"] == "comma-generic-onnx";
        let protocol = value["protocol"] == true || value["protocol"].as_f64() == Some(1.0);
        if !protocol
            || value[if generic {
                "model_sha256"
            } else {
                "onnx_sha256"
            }] != model_sha
        {
            return Err(Error::Invalid(
                "precompiled model does not match the selected ONNX/protocol".into(),
            ));
        }
        if !matches!(
            value["format"].as_str(),
            Some("comma-generic-onnx" | "comma-run-model")
        ) || value["gpu_arch"] != "gfx1200"
        {
            return Err(Error::Invalid(
                "unsupported precompiled model format/GPU".into(),
            ));
        }
        if value["frame_skip"].as_f64() != Some(4.0) || !cameras(&value["camera_resolutions"]) {
            return Err(Error::Invalid(
                "incompatible precompiled model inputs".into(),
            ));
        }
        let origin =
            url::Url::parse(catalog_url).map_err(|error| Error::Invalid(error.to_string()))?;
        let raw_origin = authority(catalog_url)
            .ok_or_else(|| Error::Invalid("catalog has no HTTPS origin".into()))?;
        for (key, maximum) in [("pickle", 4u64 << 30), ("runtime", 128 << 20)] {
            if !value[key].is_object() {
                return Err(Error::Invalid("artifact must be a JSON object".into()));
            }
            let mut artifact: Artifact = serde_json::from_value(value[key].clone())?;
            if !(1..=maximum).contains(&artifact.size)
                || artifact.sha256.len() != 64
                || !artifact
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(Error::Invalid("invalid artifact hash/size".into()));
            }
            let resolved = origin
                .join(&artifact.url)
                .map_err(|error| Error::Invalid(error.to_string()))?;
            let raw_artifact = authority(&artifact.url).unwrap_or(raw_origin);
            if resolved.scheme() != "https" || raw_artifact != raw_origin {
                return Err(Error::Invalid(
                    "artifact must use the model server HTTPS origin".into(),
                ));
            }
            artifact.url = format!(
                "{}://{}{}",
                resolved.scheme(),
                raw_artifact,
                &resolved[url::Position::BeforePath..]
            );
            value[key]["url"] = serde_json::json!(artifact.url);
        }
        let pickle: Artifact = serde_json::from_value(value["pickle"].clone())?;
        let runtime = serde_json::from_value(value["runtime"].clone())?;
        if generic && pickle.sha256 != model_sha {
            return Err(Error::Invalid(
                "generic artifact must match selected model hash".into(),
            ));
        }
        Ok(Self {
            value,
            pickle,
            runtime,
            generic,
        })
    }

    pub fn fetch(agent: &ureq::Agent, model_sha: &str, url: &str) -> Result<Self, Error> {
        let response = agent
            .get(url)
            .header("Accept-Encoding", "identity")
            .header("User-Agent", "carrot-precompiled/1")
            .call()?;
        if response.status().as_u16() >= 400 {
            return Err(Error::Invalid(format!(
                "catalog HTTP {}",
                response.status()
            )));
        }
        let mut data = Vec::new();
        response
            .into_body()
            .into_reader()
            .take(65537)
            .read_to_end(&mut data)?;
        Self::parse(&data, model_sha, url)
    }
}
