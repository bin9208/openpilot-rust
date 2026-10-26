//! Actual offline Cargo invocations, rather than mocked freshness decisions.
use openpilot_runtime_build::{build, digest_file, Recipe};
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};

struct Fixture {
    temp: tempfile::TempDir,
    root: PathBuf,
    cache: PathBuf,
    toolchain: PathBuf,
    recipe: Recipe,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("source");
        fs::create_dir_all(root.join("rust/src")).unwrap();
        fs::write(
            root.join("rust/Cargo.toml"),
            "[package]\nname='runtime-fixture'\nversion='0.1.0'\nedition='2021'\n",
        )
        .unwrap();
        fs::write(
            root.join("rust/Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"runtime-fixture\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(
            root.join("rust/src/main.rs"),
            "fn main() { println!(\"42\"); }\n",
        )
        .unwrap();
        fs::write(root.join("provider.receipt"), b"provider version one").unwrap();
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let sysroot = Command::new(&rustc)
            .args(["--print", "sysroot"])
            .output()
            .unwrap();
        assert!(sysroot.status.success());
        let toolchain =
            PathBuf::from(String::from_utf8(sysroot.stdout).unwrap().trim()).join("bin");
        let version = Command::new(toolchain.join("rustc"))
            .arg("-vV")
            .output()
            .unwrap();
        assert!(version.status.success());
        let version = String::from_utf8(version.stdout).unwrap();
        let target = version
            .lines()
            .find_map(|l| l.strip_prefix("host: "))
            .unwrap()
            .to_owned();
        let recipe = Recipe {
            schema: 1,
            target,
            packages: vec!["runtime-fixture".into()],
            binaries: vec!["runtime-fixture".into()],
            features: vec![],
            inputs: BTreeMap::from([(
                PathBuf::from("provider.receipt"),
                digest_file(&root.join("provider.receipt")).unwrap(),
            )]),
            environment: BTreeMap::new(),
            expected_growth_bytes: 8 * 1024 * 1024,
        };
        let cache = temp.path().join("cache");
        Self {
            temp,
            root,
            cache,
            toolchain,
            recipe,
        }
    }

    fn run(&self) -> openpilot_runtime_build::Report {
        build(&self.root, &self.cache, &self.toolchain, &self.recipe).unwrap()
    }
}

#[test]
fn docs_change_reuses_real_binary_and_source_change_rebuilds_it_without_network() {
    let fixture = Fixture::new();
    let first = fixture.run();
    assert_eq!(first.artifacts.len(), 1);
    assert!(!first.artifacts[0].fresh);
    let before = digest_file(&first.artifacts[0].executable).unwrap();
    fs::write(fixture.root.join("README.md"), "documentation-only change").unwrap();
    let docs = fixture.run();
    assert_eq!(docs.cache_key, first.cache_key);
    assert!(docs.artifacts.iter().all(|a| a.fresh));
    assert_eq!(digest_file(&docs.artifacts[0].executable).unwrap(), before);
    fs::write(
        fixture.root.join("rust/src/main.rs"),
        "fn main() { println!(\"43\"); }\n",
    )
    .unwrap();
    let changed = fixture.run();
    assert_eq!(changed.cache_key, first.cache_key);
    assert!(!changed.artifacts[0].fresh);
    assert_ne!(
        digest_file(&changed.artifacts[0].executable).unwrap(),
        before
    );
    let result = Command::new(&changed.artifacts[0].executable)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, b"43\n");
}

#[test]
fn provider_tampering_is_rejected_and_changed_provider_partitions_the_build_cache() {
    let mut fixture = Fixture::new();
    let first = fixture.run();
    fs::write(
        fixture.root.join("provider.receipt"),
        b"provider version two",
    )
    .unwrap();
    assert!(build(
        &fixture.root,
        &fixture.cache,
        &fixture.toolchain,
        &fixture.recipe
    )
    .is_err());
    fixture.recipe.inputs.insert(
        PathBuf::from("provider.receipt"),
        digest_file(&fixture.root.join("provider.receipt")).unwrap(),
    );
    let updated = fixture.run();
    assert_ne!(first.cache_key, updated.cache_key);
    assert!(!updated.artifacts[0].fresh);
    assert!(first.artifacts[0].executable.is_file());
}

#[test]
fn compile_failure_preserves_previous_runtime_and_concurrent_cache_owner_is_rejected() {
    let fixture = Fixture::new();
    let previous = fixture.temp.path().join("active-runtime");
    fs::write(&previous, b"previous known good generation").unwrap();
    let first = fixture.run();
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(first.target_directory.parent().unwrap().join("build.lock"))
        .unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive).unwrap();
    assert!(build(
        &fixture.root,
        &fixture.cache,
        &fixture.toolchain,
        &fixture.recipe
    )
    .is_err());
    drop(lock);
    fs::write(
        fixture.root.join("rust/src/main.rs"),
        "compile_error!(\"intentional build failure\");",
    )
    .unwrap();
    assert!(build(
        &fixture.root,
        &fixture.cache,
        &fixture.toolchain,
        &fixture.recipe
    )
    .is_err());
    assert_eq!(
        fs::read(previous).unwrap(),
        b"previous known good generation"
    );
}
