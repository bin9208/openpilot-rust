pub mod admission;
mod assets;
mod manifest;
mod source;
pub mod validation;

use crate::{model_graph::Bundle, worker::Info, Error};
use assets::relative;
use manifest::{digest, Manifest, Pointer};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct Package {
    root: PathBuf,
    pub assets: PathBuf,
    manifest: Manifest,
    pub manifest_sha256: String,
}
pub struct Artifact {
    pub assets: PathBuf,
    pub graph: Bundle,
    pub qcom_warp: Option<PathBuf>,
    pub info: Info,
    pub packed_bytes: usize,
    pub hidden: [usize; 2],
    pub scalar_offsets: [usize; 4],
}

pub fn is_local(model: &Path) -> bool {
    model
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.starts_with("big_driving_") && name.ends_with("_tinygrad.pkl"))
}

impl Package {
    pub(super) fn header(
        model: &Path,
        assets: &Path,
        selected: Option<&str>,
    ) -> Result<Self, Error> {
        let name = model
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|name| is_local(Path::new(name)))
            .ok_or(Error::Contract("local model filename missing"))?;
        let assets = assets.canonicalize()?;
        let model_root = relative(&assets, &format!("local/{name}"))?;
        let pointer = relative(&model_root, "current.json")?;
        if pointer.metadata()?.len() > 4096 {
            return Err(Error::Contract("local pointer size limit"));
        }
        let pointer: Pointer = serde_json::from_slice(&fs::read(pointer)?)?;
        if pointer.version != 1 || !digest(&pointer.pickle_sha256) {
            return Err(Error::Contract("local package generation"));
        }
        let root = relative(&model_root, &pointer.pickle_sha256)?;
        let path = relative(&root, "manifest.json")?;
        if path.metadata()?.len() > 4 << 20 {
            return Err(Error::Contract("local manifest size limit"));
        }
        let bytes = fs::read(path)?;
        let manifest: Manifest = serde_json::from_slice(&bytes)?;
        manifest.validate(name)?;
        if selected.is_some_and(|selected| selected != manifest.model_sha256) {
            return Err(Error::Contract("local selected model identity mismatch"));
        }
        if fs::read_to_string(source::suffix(model, ".native_rejected"))
            .is_ok_and(|digest| digest == manifest.pickle_sha256)
        {
            return Err(Error::Contract("local native artifact rejected"));
        }
        if pointer.pickle_sha256 != manifest.pickle_sha256 {
            return Err(Error::Contract("local package pointer identity mismatch"));
        }
        Ok(Self {
            root,
            assets,
            manifest,
            manifest_sha256: format!("{:x}", Sha256::digest(bytes)),
        })
    }

    pub fn open(model: &Path, assets: &Path, selected: Option<&str>) -> Result<Self, Error> {
        let package = Self::header(model, assets, selected)?;
        let Self { root, manifest, .. } = &package;
        if source::fingerprint(model)? != (manifest.pickle_bytes, manifest.pickle_sha256.clone()) {
            return Err(Error::Contract("local PKL checksum mismatch"));
        }
        let paths = assets::verify(root, &manifest.files)?;
        for camera in &manifest.cameras {
            let graph = Bundle::open(&relative(root, &camera.graph)?)?;
            let info = manifest.info(camera.camera)?;
            let expected = if camera.qcom_warp.is_some() {
                vec!["policy"]
            } else {
                vec!["prepare", "policy"]
            };
            if !graph.entrypoints().eq(expected)
                || graph.input_size("packed_npy_inputs")
                    != Some(
                        u64::try_from(manifest.packed_bytes()?)
                            .map_err(|_| Error::Contract("local packed size overflow"))?,
                    )
                || graph.output_size("model")
                    != Some(
                        u64::try_from(info.output_count * 4)
                            .map_err(|_| Error::Contract("local output size overflow"))?,
                    )
            {
                return Err(Error::Contract("local graph/model contract mismatch"));
            }
            if let Some(warp) = &camera.qcom_warp {
                verify_qcom(&relative(root, warp)?, &graph, &info)?;
            } else {
                for (input, size) in [
                    ("frame", info.frame_size),
                    ("big_frame", info.frame_size),
                    ("tfm", 36),
                    ("big_tfm", 36),
                ] {
                    if graph.input_size(input)
                        != Some(
                            u64::try_from(size)
                                .map_err(|_| Error::Contract("local frame size overflow"))?,
                        )
                    {
                        return Err(Error::Contract("local warp input layout mismatch"));
                    }
                }
            }
            for directory in [&camera.graph].into_iter().chain(camera.qcom_warp.as_ref()) {
                for item in fs::read_dir(relative(root, directory)?)? {
                    let item = item?;
                    let name = item
                        .file_name()
                        .into_string()
                        .map_err(|_| Error::Contract("local asset filename encoding"))?;
                    if !paths.contains(&format!("{directory}/{name}")) {
                        return Err(Error::Contract("local graph asset missing from manifest"));
                    }
                }
            }
        }
        Ok(package)
    }

    pub fn prepare(self, camera: [u32; 2]) -> Result<Artifact, Error> {
        let entry = self
            .manifest
            .cameras
            .iter()
            .find(|entry| entry.camera == camera)
            .ok_or(Error::Contract("local camera package missing"))?;
        Ok(Artifact {
            assets: self.root.clone(),
            graph: Bundle::open(&relative(&self.root, &entry.graph)?)?,
            qcom_warp: entry
                .qcom_warp
                .as_ref()
                .map(|path| relative(&self.root, path))
                .transpose()?,
            info: self.manifest.info(camera)?,
            packed_bytes: self.manifest.packed_bytes()?,
            hidden: *self
                .manifest
                .output_slices
                .get("hidden_state")
                .ok_or(Error::Contract("local hidden state missing"))?,
            scalar_offsets: std::array::from_fn(|index| self.manifest.scalar_inputs[index].offset),
        })
    }
}

#[cfg(feature = "native-skip-miri")]
fn verify_qcom(path: &Path, graph: &Bundle, info: &Info) -> Result<(), Error> {
    use openpilot_model_runtime::qcom::QcomBundle;
    let warp = QcomBundle::load(path)?;
    if !warp.entrypoints().eq(["prepare"]) {
        return Err(Error::Contract("local QCOM entrypoint mismatch"));
    }
    for (name, size) in [
        ("frame", info.frame_size),
        ("big_frame", info.frame_size),
        ("tfm", 36),
        ("big_tfm", 36),
    ] {
        if warp.input_size(name) != Some(size) {
            return Err(Error::Contract("local QCOM input size mismatch"));
        }
    }
    let output = warp
        .output_size("warped")
        .ok_or(Error::Contract("local QCOM output missing"))?;
    if graph.input_size("warped")
        != Some(
            u64::try_from(output)
                .map_err(|_| Error::Contract("local QCOM output size overflow"))?,
        )
    {
        return Err(Error::Contract("local QCOM/AMD bridge mismatch"));
    }
    Ok(())
}
#[cfg(not(feature = "native-skip-miri"))]
fn verify_qcom(_: &Path, _: &Bundle, _: &Info) -> Result<(), Error> {
    Err(Error::Contract("native QCOM feature unavailable"))
}
