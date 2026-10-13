use super::{assets::relative, source, Package};
use crate::Error;
use parking_lot::Mutex;
use std::{
    collections::BTreeSet,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(PartialEq, Eq)]
struct Identity {
    model: PathBuf,
    assets: PathBuf,
    selected: String,
}
#[derive(PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    size: u64,
    modified: [i64; 2],
    changed: [i64; 2],
    mode: u32,
}
fn stamp(path: &Path) -> Result<Stamp, Error> {
    let value = path.symlink_metadata()?;
    if value.file_type().is_symlink() {
        return Err(Error::Contract("local admission path is a symlink"));
    }
    Ok(Stamp {
        device: value.dev(),
        inode: value.ino(),
        size: value.len(),
        modified: [value.mtime(), value.mtime_nsec()],
        changed: [value.ctime(), value.ctime_nsec()],
        mode: value.mode(),
    })
}
struct Verified {
    identity: Identity,
    files: Vec<(PathBuf, Stamp)>,
}
impl Verified {
    fn unchanged(&self) -> bool {
        self.files
            .iter()
            .all(|(path, previous)| stamp(path).is_ok_and(|current| current == *previous))
    }
    fn capture(identity: Identity, package: &Package) -> Result<Self, Error> {
        let mut paths = BTreeSet::new();
        paths.insert(
            identity
                .model
                .parent()
                .ok_or(Error::Contract("local model directory missing"))?
                .to_path_buf(),
        );
        paths.extend(source::paths(&identity.model)?);
        let marker = source::suffix(&identity.model, ".chunkmanifest");
        if marker.is_file() {
            paths.insert(marker);
        }
        let rejected = source::suffix(&identity.model, ".native_rejected");
        if rejected.is_file() {
            paths.insert(rejected);
        }
        paths.insert(package.root.join("manifest.json"));
        paths.insert(
            package
                .root
                .parent()
                .ok_or(Error::Contract("local package directory missing"))?
                .join("current.json"),
        );
        for file in &package.manifest.files {
            paths.insert(relative(&package.root, &file.path)?);
        }
        let parents = paths
            .iter()
            .flat_map(|path| path.ancestors().skip(1).map(Path::to_path_buf))
            .collect::<Vec<_>>();
        paths.extend(parents);
        let files = paths
            .into_iter()
            .map(|path| stamp(&path).map(|value| (path, value)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { identity, files })
    }
}
static VERIFIED: OnceLock<Mutex<Vec<Verified>>> = OnceLock::new();

pub fn ready(model: &Path, assets: &Path, selected: &str) -> Result<(), Error> {
    let identity = Identity {
        model: model
            .parent()
            .ok_or(Error::Contract("local model directory missing"))?
            .canonicalize()?
            .join(
                model
                    .file_name()
                    .ok_or(Error::Contract("local model name missing"))?,
            ),
        assets: assets.canonicalize()?,
        selected: selected.to_owned(),
    };
    let mut verified = VERIFIED.get_or_init(Mutex::default).lock();
    if verified
        .iter()
        .any(|entry| entry.identity == identity && entry.unchanged())
    {
        return Ok(());
    }
    verified.retain(|entry| entry.identity != identity);
    let package = Package::header(&identity.model, &identity.assets, Some(&identity.selected))?;
    let entry = Verified::capture(identity, &package)?;
    let checked = Package::open(
        &entry.identity.model,
        &entry.identity.assets,
        Some(&entry.identity.selected),
    )?;
    if checked.manifest_sha256 != package.manifest_sha256 || !entry.unchanged() {
        return Err(Error::Contract("local package changed during admission"));
    }
    if verified.len() == 2 {
        verified.remove(0);
    }
    verified.push(entry);
    Ok(())
}
