use openpilot_runtime_build::{
    assets::{prepare, Asset, Download, Plan},
    Result,
};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    collections::BTreeMap,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
};

struct Fixture {
    temp: tempfile::TempDir,
    calls: Cell<usize>,
}
impl Fixture {
    fn new() -> Self {
        Self {
            temp: tempfile::tempdir().unwrap(),
            calls: Cell::new(0),
        }
    }
    fn cache(&self) -> std::path::PathBuf {
        self.temp.path().join("assets")
    }
}
impl Download for Fixture {
    fn fetch(&self, asset: &Asset, destination: &Path) -> Result<u64> {
        self.calls.set(self.calls.get() + 1);
        let bytes = asset.url.strip_prefix("https://fixture.invalid/").unwrap();
        fs::write(destination, bytes)?;
        Ok(bytes.len() as u64)
    }
}
fn asset(value: &str) -> Asset {
    Asset {
        url: format!("https://fixture.invalid/{value}"),
        sha256: format!("{:x}", Sha256::digest(value.as_bytes())),
        bytes: value.len() as u64,
    }
}
fn plan() -> Plan {
    Plan {
        schema: 1,
        reserve_bytes: 0,
        assets: BTreeMap::from([
            ("model".into(), asset("model-v1")),
            ("provider".into(), asset("provider-v1")),
            ("toolchain".into(), asset("toolchain-v1")),
        ]),
    }
}

#[test]
fn unchanged_inputs_transfer_zero_and_only_a_changed_provider_is_prepared() {
    let fixture = Fixture::new();
    let mut plan = plan();
    let first = prepare(&fixture.cache(), &plan, &fixture).unwrap();
    assert_eq!(fixture.calls.get(), 3);
    assert_eq!(first.values().map(|r| r.transferred_bytes).sum::<u64>(), 31);
    let repeated = prepare(&fixture.cache(), &plan, &fixture).unwrap();
    assert_eq!(fixture.calls.get(), 3);
    assert!(repeated
        .values()
        .all(|r| r.transferred_bytes == 0 && r.stored_bytes == 0));
    plan.assets.insert("provider".into(), asset("provider-v2"));
    let changed = prepare(&fixture.cache(), &plan, &fixture).unwrap();
    assert_eq!(fixture.calls.get(), 4);
    assert_eq!(changed["model"].transferred_bytes, 0);
    assert_eq!(changed["toolchain"].transferred_bytes, 0);
    assert_eq!(changed["provider"].transferred_bytes, 11);
    assert_eq!(fs::read(&first["provider"].path).unwrap(), b"provider-v1");
    eprintln!("asset evidence: initial_body_bytes=31 repeated_body_bytes=0 changed_provider_body_bytes=11 model_body_bytes=0 toolchain_body_bytes=0");
}

#[test]
fn corrupt_hit_is_rejected_without_downloading_or_replacing_other_inputs() {
    let fixture = Fixture::new();
    let plan = plan();
    let first = prepare(&fixture.cache(), &plan, &fixture).unwrap();
    let path = &first["model"].path;
    fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(path, b"tampered").unwrap();
    assert!(prepare(&fixture.cache(), &plan, &fixture).is_err());
    assert_eq!(fixture.calls.get(), 3);
    assert_eq!(fs::read(&first["provider"].path).unwrap(), b"provider-v1");
}

#[test]
fn bad_download_failure_and_resource_shortage_do_not_publish_an_object() {
    let fixture = Fixture::new();
    let mut plan = plan();
    plan.assets.clear();
    let mut bad = asset("expected");
    bad.url = "https://fixture.invalid/replaced".into();
    plan.assets.insert("provider".into(), bad.clone());
    assert!(prepare(&fixture.cache(), &plan, &fixture).is_err());
    let object = fixture.cache().join("sha256").join(&bad.sha256);
    assert!(!object.join("blob").exists());
    assert_eq!(fs::read_dir(&object).unwrap().count(), 1); // Lock only; pending input removed.

    struct Interrupted;
    impl Download for Interrupted {
        fn fetch(&self, _: &Asset, destination: &Path) -> Result<u64> {
            fs::write(destination, b"partial")?;
            Err("interrupted input transfer".into())
        }
    }
    assert!(prepare(&fixture.cache(), &plan, &Interrupted).is_err());
    assert!(!object.join("blob").exists());
    assert_eq!(fs::read_dir(&object).unwrap().count(), 1);
    plan.reserve_bytes = u64::MAX - 1024;
    assert!(prepare(&fixture.cache(), &plan, &fixture).is_err());
    assert_eq!(fixture.calls.get(), 1);
    assert!(!object.join("blob").exists());
}

#[test]
fn symlinked_cache_or_object_and_concurrent_owner_are_rejected() {
    let fixture = Fixture::new();
    let plan = plan();
    let first = prepare(&fixture.cache(), &plan, &fixture).unwrap();
    let object = first["model"].path.parent().unwrap();
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(object.join("lock"))
        .unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive).unwrap();
    assert!(prepare(&fixture.cache(), &plan, &fixture).is_err());
    drop(lock);
    fs::remove_file(&first["model"].path).unwrap();
    symlink(&first["provider"].path, &first["model"].path).unwrap();
    assert!(prepare(&fixture.cache(), &plan, &fixture).is_err());
    let alias = fixture.temp.path().join("alias");
    symlink(fixture.cache(), &alias).unwrap();
    assert!(prepare(&alias, &plan, &fixture).is_err());
    assert_eq!(fixture.calls.get(), 3);
}

#[test]
fn invalid_asset_identities_are_rejected_before_io() {
    let fixture = Fixture::new();
    for url in [
        "file:///tmp/input",
        "http://fixture.invalid/input",
        "https://user:secret@fixture.invalid/input",
        "https:///input",
    ] {
        let mut plan = plan();
        plan.assets.get_mut("model").unwrap().url = url.into();
        assert!(prepare(&fixture.cache(), &plan, &fixture).is_err());
    }
    let mut plan = plan();
    plan.assets.get_mut("model").unwrap().sha256 = "../escape".into();
    assert!(prepare(&fixture.cache(), &plan, &fixture).is_err());
    assert!(!fixture.cache().exists());
    assert_eq!(fixture.calls.get(), 0);
}
