use crate::Error;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub path: PathBuf,
    pub sha256: String,
    pub executable: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub source_commit: String,
    pub agnos_version: String,
    pub files: Vec<Artifact>,
}

impl Manifest {
    pub fn verify_identity(&self, root: &Path) -> Result<(), Error> {
        if root.join(".git").exists()
            && openpilot_runtime_version::git::get_head(Some(root))? != self.source_commit
        {
            return Err(Error::Configuration(
                "runtime package source commit differs from Git HEAD".into(),
            ));
        }
        let metadata = openpilot_runtime_version::get_build_metadata(root)?;
        if !metadata.openpilot.git_commit.text_eq(&self.source_commit) {
            return Err(Error::Configuration(
                "runtime package source commit differs from checkout metadata".into(),
            ));
        }
        Ok(())
    }

    pub fn read(root: &Path) -> Result<Self, Error> {
        Ok(serde_json::from_reader(File::open(
            root.join("rust/runtime-manifest.json"),
        )?)?)
    }

    pub fn verify(&self, root: &Path, required_executables: &[&str]) -> Result<(), Error> {
        if self.schema != 1
            || self.source_commit.len() != 40
            || !self
                .source_commit
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || self.agnos_version != "19.8-carrot-bt1"
        {
            return Err(Error::Configuration(
                "invalid runtime manifest identity or required AGNOS version".into(),
            ));
        }
        let root = root.canonicalize()?;
        let mut entries = BTreeMap::new();
        for artifact in &self.files {
            if artifact.path.as_os_str().is_empty()
                || !artifact
                    .path
                    .components()
                    .all(|component| matches!(component, Component::Normal(_)))
                || entries.insert(&artifact.path, artifact).is_some()
            {
                return Err(Error::Configuration(format!(
                    "invalid or duplicate runtime artifact: {}",
                    artifact.path.display()
                )));
            }
            let path = root.join(&artifact.path).canonicalize()?;
            if !path.starts_with(&root) {
                return Err(Error::Configuration(format!(
                    "runtime artifact escapes checkout: {}",
                    artifact.path.display()
                )));
            }
            let metadata = fs::metadata(&path)?;
            if !metadata.is_file()
                || (artifact.executable && metadata.permissions().mode() & 0o111 == 0)
            {
                return Err(Error::Configuration(format!(
                    "runtime artifact has wrong file type or mode: {}",
                    artifact.path.display()
                )));
            }
            let mut file = File::open(&path)?;
            let mut digest = Sha256::new();
            let mut buffer = [0; 65536];
            loop {
                let count = file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                digest.update(&buffer[..count]);
            }
            if format!("{:x}", digest.finalize()) != artifact.sha256 {
                return Err(Error::Configuration(format!(
                    "runtime artifact digest mismatch: {}",
                    artifact.path.display()
                )));
            }
        }
        for required in required_executables {
            if !entries
                .get(&PathBuf::from(required))
                .is_some_and(|artifact| artifact.executable)
            {
                return Err(Error::Configuration(format!(
                    "required executable absent from runtime manifest: {required}"
                )));
            }
        }
        let binaries = root.join("rust/bin");
        if binaries.exists() {
            let mut directories = vec![binaries];
            while let Some(directory) = directories.pop() {
                for entry in fs::read_dir(directory)? {
                    let entry = entry?;
                    let path = entry.path();
                    if entry.file_type()?.is_dir() {
                        directories.push(path);
                    } else {
                        let relative = path.strip_prefix(&root).map_err(|error| {
                            Error::Configuration(format!("invalid runtime artifact path: {error}"))
                        })?;
                        if !entries.contains_key(&relative.to_path_buf()) {
                            return Err(Error::Configuration(format!(
                                "installed runtime artifact absent from manifest: {}",
                                relative.display()
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
