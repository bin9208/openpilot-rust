//! Shared runtime package policy. The launcher compiles the same manifest source
//! against its existing Error type, preserving its public API and diagnostics.
#![forbid(unsafe_code)]
mod manifest;
pub use manifest::{Artifact, Manifest};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Component, Path, PathBuf},
};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Version(#[from] openpilot_runtime_version::Error),
    #[error("runtime launcher: {0}")]
    Configuration(String),
}
/// A snapshot bound to the manifest digest passed by the already verified
/// launcher/manager. This does not authenticate an arbitrary caller's digest.
pub struct VerifiedManifest {
    root: PathBuf,
    manifest: Manifest,
    digest: String,
}
impl VerifiedManifest {
    pub fn read_expected(root: &Path, expected: &str) -> Result<Self, Error> {
        if expected.len() != 64
            || !expected
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(Error::Configuration(
                "invalid expected runtime manifest digest".into(),
            ));
        }
        let root = root.canonicalize()?;
        let mut bytes = Vec::new();
        File::open(root.join("rust/runtime-manifest.json"))?
            .take((4 << 20) + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 4 << 20 || format!("{:x}", Sha256::digest(&bytes)) != expected {
            return Err(Error::Configuration(
                "runtime manifest digest differs from verified launcher identity".into(),
            ));
        }
        let manifest: Manifest = serde_json::from_slice(&bytes)?;
        // Schema, identity and file validation remain in the shared policy.
        manifest.verify_identity(&root)?;
        manifest.verify(&root, &[])?;
        Ok(Self {
            root,
            manifest,
            digest: expected.into(),
        })
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn source_commit(&self) -> &str {
        &self.manifest.source_commit
    }
    /// Read one bounded artifact snapshot against the already admitted hash.
    pub fn read_artifact(&self, relative: &Path, limit: u64) -> Result<Vec<u8>, Error> {
        let (path, expected) = self.artifact(relative)?;
        let mut bytes = Vec::new();
        File::open(path)?
            .take(
                limit
                    .checked_add(1)
                    .ok_or_else(|| Error::Configuration("artifact byte limit overflow".into()))?,
            )
            .read_to_end(&mut bytes)?;
        if u64::try_from(bytes.len()).map_or(true, |bytes| bytes > limit)
            || format!("{:x}", Sha256::digest(&bytes)) != expected
        {
            return Err(Error::Configuration(
                "artifact snapshot differs from verified package or exceeds limit".into(),
            ));
        }
        Ok(bytes)
    }
    pub fn artifact(&self, relative: &Path) -> Result<(PathBuf, &str), Error> {
        if relative.as_os_str().is_empty()
            || !relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err(Error::Configuration(
                "invalid admitted artifact path".into(),
            ));
        }
        let artifact = self
            .manifest
            .files
            .iter()
            .find(|artifact| artifact.path == relative)
            .ok_or_else(|| {
                Error::Configuration("artifact absent from verified runtime manifest".into())
            })?;
        let path = self.root.join(relative).canonicalize()?;
        if !path.starts_with(&self.root) {
            return Err(Error::Configuration(
                "admitted artifact escapes package".into(),
            ));
        }
        Ok((path, &artifact.sha256))
    }
}
