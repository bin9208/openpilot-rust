//! Owned Git pointers and files exercise content identity, reuse and failure
//! preservation. Real HTTPS objects and the ARM CLI are checked by Actions.
#![cfg(feature = "prepare-source-assets")]
use openpilot_runtime_build::{
    assets::{Asset, Download},
    digest_file,
    inputs::Tree,
    objects::Object,
    source_assets::{self, Plan, Prepared},
    source_lfs, Result,
};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

const FIRST: &str = "openpilot/selfdrive/carrot/web/assets/alert_camera.svg";
const SECOND: &str = "openpilot/selfdrive/carrot/web/assets/alert_police.svg";
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Owned source LFS fixture",
            "-c",
            "user.email=lfs@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim_end().into()
}
fn commit(root: &Path) {
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "Owned input revision"]);
}
fn pointer(root: &Path, path: &str, bytes: &[u8]) {
    fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
    fs::write(
        root.join(path),
        format!(
            "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize {}\n",
            sha(bytes),
            bytes.len()
        ),
    )
    .unwrap();
}
#[derive(Default)]
struct Fetch {
    data: RefCell<BTreeMap<String, Vec<u8>>>,
    calls: RefCell<Vec<String>>,
}
impl Download for Fetch {
    fn fetch(&self, asset: &Asset, output: &Path) -> Result<u64> {
        self.calls.borrow_mut().push(asset.sha256.clone());
        let bytes = self
            .data
            .borrow()
            .get(&asset.sha256)
            .cloned()
            .ok_or("missing owned object")?;
        fs::write(output, &bytes)?;
        Ok(bytes.len() as u64)
    }
}
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    cache: PathBuf,
    store: PathBuf,
    objects: PathBuf,
    plan: Plan,
    fetch: Fetch,
}
impl Fixture {
    fn new(same: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let root = base.join("source");
        fs::create_dir(&root).unwrap();
        git(&root, &["init", "--quiet"]);
        let first = b"<svg>first owned source</svg>".to_vec();
        let second = if same {
            first.clone()
        } else {
            b"<svg>second owned source</svg>".to_vec()
        };
        pointer(&root, FIRST, &first);
        pointer(&root, SECOND, &second);
        let font = root.join("openpilot/selfdrive/assets/fonts/KaiGenGothicKR-Bold.ttf");
        fs::create_dir_all(font.parent().unwrap()).unwrap();
        fs::write(font, b"owned ordinary Git bytes, not an actual font").unwrap();
        commit(&root);
        let origin_commit = git(&root, &["rev-parse", "HEAD"]);
        let assets = [(FIRST, first.clone()), (SECOND, second.clone())].into_iter().map(|(path, bytes)| {
            (PathBuf::from(path), Asset { url: format!("https://media.githubusercontent.com/media/bin9208/openpilot-rust/{origin_commit}/{path}"), sha256: sha(&bytes), bytes: bytes.len() as u64 })
        }).collect();
        let fetch = Fetch::default();
        fetch.data.borrow_mut().insert(sha(&first), first);
        fetch.data.borrow_mut().insert(sha(&second), second);
        Self {
            _temp: temp,
            root,
            cache: base.join("cache"),
            store: base.join("prefixes"),
            objects: base.join("objects"),
            plan: Plan {
                schema: 1,
                origin_commit,
                assets,
            },
            fetch,
        }
    }
    fn prepare(&self) -> Result<Prepared> {
        source_assets::prepare(
            &self.plan,
            &self.root,
            &self.cache,
            &self.store,
            &self.objects,
            &self.fetch,
        )
    }
    fn change(&mut self) -> Vec<u8> {
        let bytes = b"<svg>a new owned object</svg>".to_vec();
        pointer(&self.root, FIRST, &bytes);
        commit(&self.root);
        self.plan.origin_commit = git(&self.root, &["rev-parse", "HEAD"]);
        let row = self.plan.assets.get_mut(Path::new(FIRST)).unwrap();
        row.sha256 = sha(&bytes);
        row.bytes = bytes.len() as u64;
        for (path, row) in &mut self.plan.assets {
            row.url = format!(
                "https://media.githubusercontent.com/media/bin9208/openpilot-rust/{}/{}",
                self.plan.origin_commit,
                path.display()
            );
        }
        self.fetch
            .data
            .borrow_mut()
            .insert(sha(&bytes), bytes.clone());
        bytes
    }
    fn published(&self) -> usize {
        fs::read_dir(&self.store)
            .unwrap()
            .filter_map(|p| p.ok())
            .filter(|p| p.path().join("generation").exists())
            .count()
    }
}

#[test]
fn docs_reuse_and_one_object_change_preserve_previous_data_without_unrelated_download_or_copy() {
    let mut f = Fixture::new(false);
    let first = f.prepare().unwrap();
    assert_eq!(f.fetch.calls.borrow().len(), 2);
    assert!(first.asset_cache_consulted && first.linked_bytes > 0);
    let original = fs::read(first.root.join(FIRST)).unwrap();
    fs::write(f.root.join("README.md"), "owned docs revision\n").unwrap();
    commit(&f.root);
    let docs = f.prepare().unwrap();
    assert_ne!(docs.source.commit, first.source.commit);
    assert_eq!(docs.origin_commit, first.origin_commit);
    assert_eq!(
        (docs.identity, &docs.root, docs.tree_sha256),
        (
            first.identity.clone(),
            &first.root,
            first.tree_sha256.clone()
        )
    );
    assert!(!docs.asset_cache_consulted);
    assert_eq!(
        (
            docs.copied_bytes,
            docs.linked_bytes,
            docs.expected_additional_bytes
        ),
        (0, 0, 0)
    );
    assert_eq!(f.fetch.calls.borrow().len(), 2);
    let changed_bytes = f.change();
    let changed = f.prepare().unwrap();
    assert_eq!(f.fetch.calls.borrow().len(), 3);
    assert_eq!(
        changed.assets[Path::new(FIRST)].transferred_bytes,
        changed_bytes.len() as u64
    );
    assert_eq!(changed.assets[Path::new(SECOND)].transferred_bytes, 0);
    let metadata = fs::metadata(changed.root.join("provenance/committed-source-lfs.json"))
        .unwrap()
        .len();
    assert_eq!(changed.copied_bytes, changed_bytes.len() as u64 + metadata);
    assert_eq!(
        fs::metadata(first.root.join(SECOND)).unwrap().ino(),
        fs::metadata(changed.root.join(SECOND)).unwrap().ino()
    );
    assert_eq!(fs::read(first.root.join(FIRST)).unwrap(), original);
    Tree::read(&first.tree_receipt)
        .unwrap()
        .verify(&first.root)
        .unwrap();
    assert!(git(&f.root, &["status", "--porcelain"]).is_empty());
}

#[test]
fn missing_wrong_extra_dirty_and_non_lfs_declarations_fail_before_cache_or_download() {
    let f = Fixture::new(false);
    let mut bad = Vec::new();
    let mut plan = f.plan.clone();
    plan.assets.remove(Path::new(FIRST));
    bad.push(plan);
    let mut plan = f.plan.clone();
    plan.assets.get_mut(Path::new(FIRST)).unwrap().bytes += 1;
    bad.push(plan);
    let mut plan = f.plan.clone();
    plan.assets.get_mut(Path::new(FIRST)).unwrap().url = "https://example.invalid/object".into();
    bad.push(plan);
    for path in [
        "openpilot/selfdrive/assets/fonts/KaiGenGothicKR-Bold.ttf",
        "panda/board/obj/panda.bin.signed",
    ] {
        let mut plan = f.plan.clone();
        plan.assets.insert(
            path.into(),
            Asset {
                url: format!(
                    "https://media.githubusercontent.com/media/bin9208/openpilot-rust/{}/{path}",
                    plan.origin_commit
                ),
                sha256: sha(b"ordinary or absent"),
                bytes: 18,
            },
        );
        bad.push(plan);
    }
    for plan in bad {
        assert!(
            source_assets::prepare(&plan, &f.root, &f.cache, &f.store, &f.objects, &f.fetch)
                .is_err()
        );
    }
    fs::write(f.root.join("README.md"), "uncommitted owned source\n").unwrap();
    assert!(f.prepare().is_err());
    assert!(f.fetch.calls.borrow().is_empty());
    assert!(!f.cache.exists() && !f.store.exists() && !f.objects.exists());
}

#[test]
fn verified_prefix_needs_no_download_caches_and_forged_receipts_or_additions_cannot_repair_itself()
{
    let f = Fixture::new(false);
    let first = f.prepare().unwrap();
    fs::remove_dir_all(&f.cache).unwrap();
    fs::remove_dir_all(&f.objects).unwrap();
    let reused = f.prepare().unwrap();
    assert_eq!(reused.copied_bytes + reused.linked_bytes, 0);
    assert!(!f.cache.exists() && !f.objects.exists());
    assert_eq!(f.fetch.calls.borrow().len(), 2);
    let receipt = fs::read(&first.tree_receipt).unwrap();
    fs::set_permissions(&first.tree_receipt, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(&first.tree_receipt, b"{\"schema\":1,\"entries\":{}}").unwrap();
    assert!(f.prepare().is_err());
    assert_eq!(
        fs::read(&first.tree_receipt).unwrap(),
        b"{\"schema\":1,\"entries\":{}}"
    );
    fs::write(&first.tree_receipt, receipt).unwrap();
    fs::set_permissions(&first.tree_receipt, fs::Permissions::from_mode(0o444)).unwrap();
    fs::write(first.root.join("unexpected.bin"), "untrusted extra member").unwrap();
    assert!(f.prepare().is_err());
    assert!(first.root.join("unexpected.bin").exists());
    assert_eq!(f.fetch.calls.borrow().len(), 2);
}

#[test]
fn bad_download_and_source_change_during_download_leave_the_previous_generation_intact() {
    for mutate_source in [false, true] {
        let mut f = Fixture::new(false);
        let first = f.prepare().unwrap();
        drop(f.change());
        struct Interrupt<'a> {
            f: &'a Fixture,
            mutate: bool,
        }
        impl Download for Interrupt<'_> {
            fn fetch(&self, asset: &Asset, output: &Path) -> Result<u64> {
                if self.mutate {
                    self.f.fetch.fetch(asset, output)?;
                    fs::write(
                        self.f.root.join("README.md"),
                        "source changed during owned fetch\n",
                    )?;
                    commit(&self.f.root);
                } else {
                    fs::write(output, vec![b'x'; usize::try_from(asset.bytes)?])?;
                }
                Ok(asset.bytes)
            }
        }
        assert!(source_assets::prepare(
            &f.plan,
            &f.root,
            &f.cache,
            &f.store,
            &f.objects,
            &Interrupt {
                f: &f,
                mutate: mutate_source
            }
        )
        .is_err());
        assert_eq!(f.published(), 1);
        Tree::read(&first.tree_receipt)
            .unwrap()
            .verify(&first.root)
            .unwrap();
    }
}

#[test]
fn interrupted_object_import_is_retriable_and_prefix_lock_conflict_preserves_completed_data() {
    let f = Fixture::new(false);
    let asset = &f.plan.assets[Path::new(FIRST)];
    let interrupted = f.objects.join(format!("{}-444", asset.sha256));
    fs::create_dir_all(&interrupted).unwrap();
    File::create(interrupted.join("lock")).unwrap();
    let first = f.prepare().unwrap();
    assert!(interrupted.join("blob").is_file());
    let directory = first.root.parent().unwrap().parent().unwrap();
    let lock = OpenOptions::new()
        .write(true)
        .open(directory.join("lock"))
        .unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
    assert!(f.prepare().is_err());
    Tree::read(&first.tree_receipt)
        .unwrap()
        .verify(&first.root)
        .unwrap();
    drop(lock);
    assert_eq!(f.prepare().unwrap().copied_bytes, 0);
    let inside_source = f.root.join("cache");
    assert!(source_assets::prepare(
        &f.plan,
        &f.root,
        &inside_source,
        &f.store,
        &f.objects,
        &f.fetch
    )
    .is_err());
    assert!(!inside_source.exists());
}

#[test]
fn duplicate_oid_is_downloaded_and_imported_once_while_both_paths_are_verified() {
    let f = Fixture::new(true);
    let prepared = f.prepare().unwrap();
    let bytes = f.plan.assets[Path::new(FIRST)].bytes;
    assert_eq!(f.fetch.calls.borrow().len(), 1);
    assert_eq!(
        prepared
            .assets
            .values()
            .map(|a| a.transferred_bytes)
            .sum::<u64>(),
        bytes
    );
    let metadata = fs::metadata(prepared.root.join("provenance/committed-source-lfs.json"))
        .unwrap()
        .len();
    assert_eq!(prepared.copied_bytes, bytes + metadata);
    assert_eq!(prepared.linked_bytes, 2 * bytes);
    assert_eq!(
        fs::metadata(prepared.root.join(FIRST)).unwrap().ino(),
        fs::metadata(prepared.root.join(SECOND)).unwrap().ino()
    );
    assert_eq!(
        digest_file(&prepared.root.join(FIRST)).unwrap(),
        f.plan.assets[Path::new(FIRST)].sha256
    );
}

#[test]
fn generation_mappings_and_boot_roster_must_include_every_current_lfs_identity() {
    let f = Fixture::new(false);
    let prepared = f.prepare().unwrap();
    let mut mappings: BTreeMap<_, _> = f
        .plan
        .assets
        .keys()
        .map(|path| (path.clone(), path.clone()))
        .collect();
    let mut objects: BTreeMap<_, _> = f
        .plan
        .assets
        .iter()
        .map(|(path, asset)| {
            (
                path.clone(),
                Object {
                    sha256: asset.sha256.clone(),
                    bytes: asset.bytes,
                    mode: 0o444,
                },
            )
        })
        .collect();
    source_lfs::verify_layout(&f.root, &prepared.root, &mappings).unwrap();
    source_lfs::verify_recorded(&f.root, &objects).unwrap();
    let path = Path::new(FIRST);
    let original = mappings.remove(path).unwrap();
    assert!(source_lfs::verify_layout(&f.root, &prepared.root, &mappings).is_err());
    mappings.insert(path.into(), "../outside".into());
    assert!(source_lfs::verify_layout(&f.root, &prepared.root, &mappings).is_err());
    mappings.insert(path.into(), original);
    let object = objects.remove(path).unwrap();
    assert!(source_lfs::verify_recorded(&f.root, &objects).is_err());
    objects.insert(
        path.into(),
        Object {
            mode: 0o555,
            ..object.clone()
        },
    );
    assert!(source_lfs::verify_recorded(&f.root, &objects).is_err());
    objects.insert(
        path.into(),
        Object {
            bytes: object.bytes + 1,
            ..object.clone()
        },
    );
    assert!(source_lfs::verify_recorded(&f.root, &objects).is_err());
    objects.insert(path.into(), object);
    fs::remove_dir_all(&f.cache).unwrap();
    fs::remove_dir_all(&f.objects).unwrap();
    source_lfs::verify_recorded(&f.root, &objects).unwrap();
    source_lfs::verify_layout(&f.root, &prepared.root, &mappings).unwrap();
}
