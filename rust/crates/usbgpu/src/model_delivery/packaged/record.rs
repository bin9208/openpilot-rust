use super::super::{assets, known_manifest, Error};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) enum Kind {
    #[serde(rename = "native-generic-hcq")]
    Generic,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Pickle {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub version: u32,
    pub kind: Kind,
    pub model_sha256: String,
    pub model_checkpoint: String,
    pub pickle: Pickle,
    pub assets_manifest_sha256: String,
    pub runtime_archive_sha256: String,
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Record {
    pub fn read(root: &Path) -> Result<Option<Self>, Error> {
        let path = root.join("native-provider.json");
        let metadata = match path.symlink_metadata() {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_file() || metadata.len() > 4096 {
            return Err(Error::Invalid(
                "native provider record must be a bounded regular file".into(),
            ));
        }
        let record: Self = serde_json::from_slice(&fs::read(path)?)?;
        let pinned = known_manifest(&record.model_sha256)
            .ok_or_else(|| Error::Invalid("native provider has unknown source model".into()))?;
        if record.version != 1
            || record.model_sha256 != pinned.sha256
            || record.pickle.sha256 != pinned.sha256
            || record.pickle.size != pinned.size
            || record.pickle.path != format!("models/{}/model.pkl", pinned.sha256)
            || !digest(&record.assets_manifest_sha256)
            || !digest(&record.runtime_archive_sha256)
        {
            return Err(Error::Invalid(
                "native provider differs from the source-pinned generic model".into(),
            ));
        }
        Ok(Some(record))
    }

    pub fn write(&self, root: &Path) -> Result<(), Error> {
        let mut stage = tempfile::Builder::new()
            .prefix(".native-provider-")
            .tempfile_in(root)?;
        serde_json::to_writer_pretty(&mut stage, self)?;
        stage.write_all(b"\n")?;
        stage.flush()?;
        stage.as_file().sync_all()?;
        stage
            .persist(root.join("native-provider.json"))
            .map_err(|error| error.error)?;
        Ok(())
    }

    pub fn package(&self, root: &Path) -> Result<assets::Package, Error> {
        let package = assets::validate(root, &self.model_sha256, self.pickle.size)?;
        if package.manifest_sha256 != self.assets_manifest_sha256
            || package.checkpoint != self.model_checkpoint
        {
            return Err(Error::Invalid(
                "native provider asset generation/checkpoint mismatch".into(),
            ));
        }
        Ok(package)
    }

    pub fn model(&self, root: &Path) -> Result<PathBuf, Error> {
        let path = assets::regular(root, &self.pickle.path)?;
        if path.metadata()?.len() != self.pickle.size {
            return Err(Error::Invalid("native provider PKL size mismatch".into()));
        }
        Ok(path)
    }
}
