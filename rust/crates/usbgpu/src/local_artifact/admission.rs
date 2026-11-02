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
    owner: [u32; 2],
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
        owner: [value.uid(), value.gid()],
    })
}
struct Verified {
    identity: Identity,
    files: Vec<(PathBuf, Stamp)>,
    ancestors: Vec<(PathBuf, Stamp)>,
}
impl Verified {
    fn unchanged(&self) -> bool {
        self.files
            .iter()
            .all(|(path, previous)| stamp(path).is_ok_and(|current| current == *previous))
            && self.ancestors.iter().all(|(path, previous)| {
                stamp(path).is_ok_and(|current| {
                    current.device == previous.device
                        && current.inode == previous.inode
                        && current.mode == previous.mode
                        && current.owner == previous.owner
                })
            })
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
        let mut ancestors = BTreeSet::new();
        for parent in parents {
            if parent.starts_with(&package.root) {
                // Package membership, including newly added unlisted files,
                // remains part of the full content snapshot.
                paths.insert(parent);
            } else if !paths.contains(&parent) {
                // Unrelated entries under /tmp, /data or / must not revoke a
                // model. Still reject redirected/replaced ancestor directories.
                ancestors.insert(parent);
            }
        }
        let files = paths
            .into_iter()
            .map(|path| stamp(&path).map(|value| (path, value)))
            .collect::<Result<Vec<_>, _>>()?;
        let ancestors = ancestors
            .into_iter()
            .map(|path| stamp(&path).map(|value| (path, value)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            identity,
            files,
            ancestors,
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, fs::FileTimes, time::UNIX_EPOCH};

    fn snapshot(parent: &Path) -> Verified {
        let root = parent.join("owned");
        let model = root.join("models/model.pkl");
        let package_root = root.join("assets/local/model/generation");
        fs::create_dir_all(model.parent().unwrap()).unwrap();
        fs::create_dir_all(package_root.join("kernels")).unwrap();
        fs::write(&model, b"model").unwrap();
        fs::write(package_root.join("kernels/code.bin"), b"code").unwrap();
        fs::write(package_root.join("manifest.json"), b"manifest").unwrap();
        fs::write(
            package_root.parent().unwrap().join("current.json"),
            b"pointer",
        )
        .unwrap();
        // The content verifier supplies Package; these tests exercise only its
        // immutable filesystem snapshot, without a model, firmware or GPU run.
        let package = Package {
            root: package_root,
            assets: root.join("assets"),
            manifest_sha256: "0".repeat(64),
            manifest: serde_json::from_value(serde_json::json!({
                "version":1,"format":"run-policy","model_sha256":"0".repeat(64),
                "pickle_sha256":"0".repeat(64),"pickle_bytes":5,"filename":"model.pkl",
                "checkpoint":"snapshot-only","scalar_inputs":[],"output_count":0,
                "output_slices":{},"cameras":[],
                "files":[{"path":"kernels/code.bin","bytes":4,"sha256":"0".repeat(64)}]
            }))
            .unwrap(),
        };
        Verified::capture(
            Identity {
                model,
                assets: package.assets.clone(),
                selected: "0".repeat(64),
            },
            &package,
        )
        .unwrap()
    }

    #[test]
    fn unrelated_ancestor_entries_do_not_revoke_a_verified_package() {
        let temp = tempfile::tempdir().unwrap();
        let verified = snapshot(temp.path());
        let old = &verified
            .ancestors
            .iter()
            .find(|(path, _)| path == temp.path())
            .unwrap()
            .1;
        fs::create_dir(temp.path().join("unrelated-job")).unwrap();
        fs::File::open(temp.path())
            .unwrap()
            .set_times(FileTimes::new().set_modified(UNIX_EPOCH))
            .unwrap();
        // The previous whole-ancestor stamp comparison rejects this input.
        assert!(stamp(temp.path()).unwrap() != *old);
        assert!(verified.unchanged());
    }

    #[test]
    fn model_rejection_package_membership_and_pointer_changes_still_invalidate() {
        for relative in [
            "models/model.pkl.native_rejected",
            "assets/local/model/generation/kernels/unlisted.bin",
            "assets/local/model/generation/kernels/code.bin",
            "assets/local/model/current.json",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let verified = snapshot(temp.path());
            assert!(verified.unchanged());
            fs::write(temp.path().join("owned").join(relative), b"changed input").unwrap();
            assert!(!verified.unchanged(), "{relative}");
        }
    }

    #[test]
    fn ancestor_redirection_still_invalidates_identical_owned_files() {
        let temp = tempfile::tempdir().unwrap();
        let verified = snapshot(temp.path());
        fs::rename(temp.path().join("owned"), temp.path().join("original")).unwrap();
        std::os::unix::fs::symlink(temp.path().join("original"), temp.path().join("owned"))
            .unwrap();
        assert!(!verified.unchanged());
    }
}
