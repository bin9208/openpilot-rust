mod descriptor;

use crate::{assets::read_verified, qcom::QcomGraph, Error, Graph};
use descriptor::Contract;
pub use descriptor::{Descriptor, Dtype, Metadata, Nv12, TensorSpec};
use serde::Deserialize;
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Driving,
    Driver,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pointer {
    version: u32,
    generation: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    version: u32,
    bundles: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    kind: Kind,
    camera: [u32; 2],
    directory: String,
    graph_sha256: String,
    pipeline_sha256: String,
}

#[derive(Deserialize)]
struct Backend {
    backend: String,
}

pub struct Bundle {
    pub directory: PathBuf,
    pub descriptor: Descriptor,
    pub backend: String,
}

pub struct Catalog {
    bundles: Vec<Bundle>,
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl Catalog {
    pub fn load(root: &Path) -> Result<Self, Error> {
        let mut bytes = Vec::new();
        fs::File::open(root.join("current.json"))?
            .take(4097)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 4096 {
            return Err(Error::Limit("pipeline pointer bytes"));
        }
        let pointer: Pointer = serde_json::from_slice(&bytes)?;
        if pointer.version != 1 || !digest(&pointer.generation) {
            return Err(Error::Contract("pipeline generation"));
        }
        let generation = root.join(&pointer.generation);
        let bytes = read_verified(
            &generation.join("index.json"),
            &pointer.generation,
            1024 * 1024,
        )?;
        let index: Index = serde_json::from_slice(&bytes)?;
        if index.version != 1 || index.bundles.is_empty() || index.bundles.len() > 16 {
            return Err(Error::Contract("pipeline index version/count"));
        }
        let mut keys = HashSet::new();
        let mut bundles = Vec::with_capacity(index.bundles.len());
        for entry in index.bundles {
            if !keys.insert((entry.kind, entry.camera))
                || entry.directory.is_empty()
                || entry.directory.len() > 128
                || !entry
                    .directory
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                || !digest(&entry.graph_sha256)
                || !digest(&entry.pipeline_sha256)
            {
                return Err(Error::Contract("pipeline index entry"));
            }
            let directory = generation.join(&entry.directory);
            let graph_bytes = read_verified(
                &directory.join("graph.json"),
                &entry.graph_sha256,
                64 * 1024 * 1024,
            )?;
            let pipeline = read_verified(
                &directory.join("pipeline.json"),
                &entry.pipeline_sha256,
                1024 * 1024,
            )?;
            let descriptor: Descriptor = serde_json::from_slice(&pipeline)?;
            if descriptor.kind != entry.kind || descriptor.camera != entry.camera {
                return Err(Error::Contract("pipeline index/descriptor identity"));
            }
            let backend: Backend = serde_json::from_slice(&graph_bytes)?;
            match backend.backend.as_str() {
                "cpu-clang" | "cpu-llvm" => {
                    let graph = Graph::parse(&graph_bytes)?.graph;
                    descriptor.validate(&Contract {
                        inputs: &graph.inputs,
                        outputs: &graph.outputs,
                        views: &graph.views,
                        entries: &graph.entrypoints,
                    })?;
                }
                "qcom-cl" => {
                    let graph = QcomGraph::parse(&graph_bytes)?;
                    descriptor.validate(&Contract {
                        inputs: &graph.inputs,
                        outputs: &graph.outputs,
                        views: &graph.views,
                        entries: &graph.entrypoints,
                    })?;
                }
                _ => return Err(Error::Contract("pipeline backend")),
            }
            bundles.push(Bundle {
                directory,
                descriptor,
                backend: backend.backend,
            });
        }
        Ok(Self { bundles })
    }

    pub fn bundles(&self) -> &[Bundle] {
        &self.bundles
    }

    pub fn select(&self, kind: Kind, camera: [u32; 2]) -> Result<&Bundle, Error> {
        self.bundles
            .iter()
            .find(|bundle| bundle.descriptor.kind == kind && bundle.descriptor.camera == camera)
            .ok_or(Error::Contract("pipeline kind/resolution unavailable"))
    }
}
