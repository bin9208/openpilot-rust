use crate::{
    model_delivery::{assets, catalog::Catalog, known_manifest, packaged, sha256, Error},
    worker::{Info, Metadata},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct Artifact {
    pub assets: PathBuf,
    pub descriptor: Vec<u8>,
    pub warp: Vec<u8>,
    pub info: Info,
    pub manifest_sha256: String,
}

pub struct Binding {
    root: PathBuf,
    manifest_sha256: String,
}
impl Binding {
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
    #[must_use]
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
}

struct Identity {
    digest: String,
    bytes: u64,
    package: assets::Package,
}

fn package(model: &Path, root: &Path) -> Result<Identity, Error> {
    let directory = model
        .parent()
        .ok_or_else(|| Error::Invalid("installed model has no directory".into()))?;
    let bytes = match fs::read(directory.join("installed.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let package = packaged::worker_package(model, root)?;
            let identity = packaged::identity(model)?;
            let digest = identity["model_sha256"]
                .as_str()
                .ok_or_else(|| Error::Invalid("native model identity missing".into()))?;
            let pinned = known_manifest(digest)
                .ok_or_else(|| Error::Invalid("native model identity unknown".into()))?;
            return Ok(Identity {
                digest: pinned.sha256,
                bytes: pinned.size,
                package,
            });
        }
        Err(error) => return Err(error.into()),
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let digest = value["pickle"]["sha256"]
        .as_str()
        .ok_or_else(|| Error::Invalid("installed model hash missing".into()))?;
    let url = value["catalog_url"]
        .as_str()
        .ok_or_else(|| Error::Invalid("installed catalog URL missing".into()))?;
    let catalog = Catalog::parse(&bytes, digest, url)?;
    if !catalog.generic {
        return Err(Error::Invalid(
            "native comma-run-model companion/queue adapter unavailable".into(),
        ));
    }
    let root = root.canonicalize()?;
    let package = assets::validate(&root, &catalog.pickle.sha256, catalog.pickle.size)?;
    if catalog.value["model_checkpoint"] != package.checkpoint {
        return Err(Error::Invalid("precompiled checkpoint mismatch".into()));
    }
    Ok(Identity {
        digest: catalog.pickle.sha256,
        bytes: catalog.pickle.size,
        package,
    })
}

pub fn bind(model: &Path, root: &Path) -> Result<Binding, Error> {
    if crate::local_artifact::is_local(model) {
        let package = crate::local_artifact::Package::open(model, root, None)?;
        return Ok(Binding {
            root: package.assets,
            manifest_sha256: package.manifest_sha256,
        });
    }
    let Identity { package, .. } = package(model, root)?;
    Ok(Binding {
        root: package.root,
        manifest_sha256: package.manifest_sha256,
    })
}

/// Hash the selected PKL and verify all native companions before opening a GPU.
///
/// # Errors
/// Missing/corrupt assets, a changed PKL or inconsistent catalog/metadata reject
/// the load. Adjacent unverified descriptors/firmware never override this package.
pub fn prepare(
    model: &Path,
    root: &Path,
    camera: [u32; 2],
    expected: Option<&str>,
) -> Result<Artifact, Error> {
    let Identity {
        digest,
        bytes,
        package,
    } = package(model, root)?;
    if expected.is_some_and(|expected| expected != package.manifest_sha256) {
        return Err(Error::Invalid(
            "native asset manifest differs from validated binding".into(),
        ));
    }
    if model.metadata()?.len() != bytes || sha256(model)? != digest {
        return Err(Error::Invalid("precompiled PKL checksum mismatch".into()));
    }
    let metadata: Metadata = serde_json::from_slice(&fs::read(&package.metadata)?)?;
    let info = Info::new(metadata, camera)?;
    Ok(Artifact {
        warp: fs::read(
            package
                .root
                .join(format!("warp-gfx1200-{}x{}.json", camera[0], camera[1])),
        )?,
        assets: package.root,
        descriptor: fs::read(package.descriptor)?,
        info,
        manifest_sha256: package.manifest_sha256,
    })
}
