//! Actual offline Cargo invocations, rather than mocked freshness decisions.
use openpilot_runtime_build::{build, digest_file, Recipe};
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};

struct Fixture {
    _temp: tempfile::TempDir,
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
            input_trees: BTreeMap::new(),
            inputs: BTreeMap::from([(
                PathBuf::from("provider.receipt"),
                digest_file(&root.join("provider.receipt")).unwrap(),
            )]),
            environment: BTreeMap::new(),
            expected_growth_bytes: 8 * 1024 * 1024,
        };
        let cache = temp.path().join("cache");
        git(&root, &["init", "--quiet"]);
        commit(&root);
        Self {
            _temp: temp,
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

fn git(root: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .args([
            "-c",
            "user.name=Runtime fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn commit(root: &std::path::Path) {
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "fixture input"]);
}

#[test]
fn prepared_groups_reuse_docs_and_rebuild_only_the_changed_binary_while_retaining_previous_execution(
) {
    use openpilot_runtime_build::prepare::{self, Plan};
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root.join("rust/src/bin")).unwrap();
    fs::write(
        fixture.root.join("rust/src/bin/other-fixture.rs"),
        "fn main() { println!(\"other\"); }\n",
    )
    .unwrap();
    commit(&fixture.root);
    let mut other = fixture.recipe.clone();
    other.binaries = vec!["other-fixture".into()];
    let mut plan = Plan {
        schema: 1,
        groups: BTreeMap::from([
            ("main".into(), fixture.recipe.clone()),
            ("other".into(), other),
        ]),
        expected_source_growth_bytes: 8 * 1024 * 1024,
        remaining_object_growth_bytes: 0,
        runtime_inputs: Default::default(),
    };
    let source_store = fixture._temp.path().join("sources");
    let object_store = fixture._temp.path().join("objects");
    let plan_path = fixture._temp.path().join("plan.json");
    let run = |plan: &Plan| {
        fs::write(&plan_path, serde_json::to_vec_pretty(plan).unwrap()).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_openpilot-runtime-prepare"))
            .arg("--root")
            .arg(&fixture.root)
            .arg("--plan")
            .arg(&plan_path)
            .arg("--cache")
            .arg(&fixture.cache)
            .arg("--toolchain")
            .arg(&fixture.toolchain)
            .arg("--source-store")
            .arg(&source_store)
            .arg("--object-store")
            .arg(&object_store)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice::<prepare::Prepared>(&result.stdout).unwrap()
    };
    let first = run(&plan);
    assert_eq!(first.binaries.len(), 2);
    assert!(first.stored_output_bytes > 0);
    let mut unbound = first.clone();
    unbound
        .plan
        .groups
        .get_mut("main")
        .unwrap()
        .features
        .push("invented-feature".into());
    assert!(unbound.verify(&source_store, &object_store).is_err());
    let mut mixed = first.clone();
    mixed.reports.get_mut("other").unwrap().cargo_sha256 = "0".repeat(64);
    assert!(mixed.verify(&source_store, &object_store).is_err());
    // A model/data update is independently pinned, not a compiler input for
    // these binaries. Changing it must not rebuild unrelated executable code.
    let model = fixture._temp.path().join("runtime-model");
    fs::create_dir(&model).unwrap();
    fs::write(model.join("model.onnx"), b"runtime model A").unwrap();
    let receipt = fixture._temp.path().join("runtime-model.json");
    fs::write(
        &receipt,
        serde_json::to_vec(&openpilot_runtime_build::inputs::Tree::capture(&model).unwrap())
            .unwrap(),
    )
    .unwrap();
    plan.runtime_inputs
        .trees
        .insert(model.clone(), receipt.clone());
    plan.runtime_inputs
        .files
        .insert(receipt.clone(), digest_file(&receipt).unwrap());
    let model_a = run(&plan);
    assert_eq!(model_a.stored_output_bytes, 0);
    fs::write(model.join("model.onnx"), b"runtime model B").unwrap();
    assert!(prepare::run(
        &fixture.root,
        &fixture.cache,
        &fixture.toolchain,
        &source_store,
        &object_store,
        &plan
    )
    .is_err());
    fs::write(
        &receipt,
        serde_json::to_vec(&openpilot_runtime_build::inputs::Tree::capture(&model).unwrap())
            .unwrap(),
    )
    .unwrap();
    plan.runtime_inputs
        .files
        .insert(receipt.clone(), digest_file(&receipt).unwrap());
    let model_b = run(&plan);
    assert_eq!(model_b.binaries, model_a.binaries);
    assert_eq!(model_b.stored_output_bytes, 0);
    assert_ne!(
        model_b.verified_runtime_trees,
        model_a.verified_runtime_trees
    );
    for group in ["main", "other"] {
        assert_eq!(
            model_b.reports[group].cache_key,
            model_a.reports[group].cache_key
        );
        assert!(model_b.reports[group].artifacts.iter().all(|a| a.fresh));
    }
    let previous = fixture._temp.path().join("running-previous");
    first.binaries["runtime-fixture"]
        .materialize(&object_store, &previous)
        .unwrap();
    fs::write(fixture.root.join("README.md"), "docs only").unwrap();
    commit(&fixture.root);
    let docs = run(&plan);
    assert_eq!(docs.stored_output_bytes, 0);
    assert!(docs
        .reports
        .values()
        .all(|r| r.artifacts.iter().all(|a| a.fresh)));
    assert_ne!(docs.source.source.commit, first.source.source.commit);
    fs::write(
        fixture.root.join("rust/src/main.rs"),
        "fn main() { println!(\"43\"); }\n",
    )
    .unwrap();
    commit(&fixture.root);
    let changed = run(&plan);
    assert!(!changed.reports["main"].artifacts[0].fresh);
    assert!(changed.reports["other"].artifacts[0].fresh);
    assert_ne!(
        changed.binaries["runtime-fixture"],
        first.binaries["runtime-fixture"]
    );
    assert_eq!(
        changed.binaries["other-fixture"],
        first.binaries["other-fixture"]
    );
    assert_eq!(
        changed.stored_output_bytes,
        changed.binaries["runtime-fixture"].bytes
    );
    // A rejected plan cannot replace either the running binary or its exact source.
    plan.groups.get_mut("other").unwrap().binaries = vec!["runtime-fixture".into()];
    assert!(prepare::run(
        &fixture.root,
        &fixture.cache,
        &fixture.toolchain,
        &source_store,
        &object_store,
        &plan
    )
    .is_err());
    fs::remove_dir_all(&fixture.root).unwrap();
    fs::remove_dir_all(&fixture.cache).unwrap();
    first.verify(&source_store, &object_store).unwrap();
    docs.verify(&source_store, &object_store).unwrap();
    changed.verify(&source_store, &object_store).unwrap();
    assert_eq!(Command::new(previous).output().unwrap().stdout, b"42\n");
}

#[test]
fn docs_change_reuses_real_binary_and_source_change_rebuilds_it_without_network() {
    let fixture = Fixture::new();
    let first = fixture.run();
    assert_eq!(first.artifacts.len(), 1);
    assert!(!first.artifacts[0].fresh);
    let before = digest_file(&first.artifacts[0].executable).unwrap();
    fs::write(fixture.root.join("README.md"), "documentation-only change").unwrap();
    commit(&fixture.root);
    let docs = fixture.run();
    assert_eq!(docs.cache_key, first.cache_key);
    assert!(docs.artifacts.iter().all(|a| a.fresh));
    assert_eq!(digest_file(&docs.artifacts[0].executable).unwrap(), before);
    assert_ne!(docs.source.commit, first.source.commit);
    assert_eq!(docs.artifacts[0].sha256, before);
    assert_eq!(
        docs.source.commit,
        git(&fixture.root, &["rev-parse", "HEAD"])
    );
    fs::write(
        fixture.root.join("rust/src/main.rs"),
        "fn main() { println!(\"43\"); }\n",
    )
    .unwrap();
    commit(&fixture.root);
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
    commit(&fixture.root);
    let updated = fixture.run();
    assert_ne!(first.cache_key, updated.cache_key);
    assert!(!updated.artifacts[0].fresh);
    assert!(first.artifacts[0].executable.is_file());
}

#[test]
fn compile_failure_preserves_previous_runtime_and_concurrent_cache_owner_is_rejected() {
    let fixture = Fixture::new();
    let first = fixture.run();
    let previous = &first.artifacts[0].executable;
    let previous_digest = digest_file(previous).unwrap();
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
    commit(&fixture.root);
    assert!(build(
        &fixture.root,
        &fixture.cache,
        &fixture.toolchain,
        &fixture.recipe
    )
    .is_err());
    assert_eq!(digest_file(previous).unwrap(), previous_digest);
    let result = Command::new(previous).output().unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, b"42\n");
}

#[test]
fn changing_one_workspace_target_keeps_the_unrelated_target_fresh() {
    let mut fixture = Fixture::new();
    fs::write(fixture.root.join("rust/Cargo.toml"), "[package]\nname='runtime-fixture'\nversion='0.1.0'\nedition='2021'\n[workspace]\nmembers=['unrelated']\nresolver='2'\n").unwrap();
    fs::create_dir_all(fixture.root.join("rust/unrelated/src")).unwrap();
    fs::write(
        fixture.root.join("rust/unrelated/Cargo.toml"),
        "[package]\nname='runtime-unrelated'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();
    fs::write(
        fixture.root.join("rust/unrelated/src/main.rs"),
        "fn main() { println!(\"unchanged\"); }\n",
    )
    .unwrap();
    let mut lock = fs::read_to_string(fixture.root.join("rust/Cargo.lock")).unwrap();
    lock.push_str("\n[[package]]\nname = \"runtime-unrelated\"\nversion = \"0.1.0\"\n");
    fs::write(fixture.root.join("rust/Cargo.lock"), lock).unwrap();
    fixture.recipe.packages.push("runtime-unrelated".into());
    fixture.recipe.binaries.push("runtime-unrelated".into());
    commit(&fixture.root);
    let first = fixture.run();
    assert_eq!(first.artifacts.len(), 2);
    let unrelated = first
        .artifacts
        .iter()
        .find(|a| a.executable.file_name().unwrap() == "runtime-unrelated")
        .unwrap();
    let before = digest_file(&unrelated.executable).unwrap();
    fs::write(
        fixture.root.join("rust/src/main.rs"),
        "fn main() { println!(\"changed\"); }\n",
    )
    .unwrap();
    commit(&fixture.root);
    let changed = fixture.run();
    assert_eq!(changed.cache_key, first.cache_key);
    let unrelated = changed
        .artifacts
        .iter()
        .find(|a| a.executable.file_name().unwrap() == "runtime-unrelated")
        .unwrap();
    assert!(unrelated.fresh);
    assert_eq!(digest_file(&unrelated.executable).unwrap(), before);
    assert!(changed.artifacts.iter().any(|a| !a.fresh));
}

#[test]
fn dirty_source_and_source_mutation_during_build_cannot_publish_a_success_report() {
    let fixture = Fixture::new();
    let first = fixture.run();
    let report = first
        .target_directory
        .parent()
        .unwrap()
        .join("last-successful-build.json");
    let before = fs::read(&report).unwrap();
    fs::write(fixture.root.join("untracked.rs"), "unrecorded input").unwrap();
    assert!(build(
        &fixture.root,
        &fixture.cache,
        &fixture.toolchain,
        &fixture.recipe
    )
    .is_err());
    fs::remove_file(fixture.root.join("untracked.rs")).unwrap();
    fs::write(fixture.root.join("README.md"), "recorded source").unwrap();
    fs::write(fixture.root.join("rust/build.rs"), "fn main() { std::fs::write(\"../README.md\", \"source changed during build\").unwrap(); }\n").unwrap();
    commit(&fixture.root);
    let error = build(
        &fixture.root,
        &fixture.cache,
        &fixture.toolchain,
        &fixture.recipe,
    )
    .unwrap_err();
    assert!(error.to_string().contains("source"));
    assert_eq!(fs::read(report).unwrap(), before);
}

#[test]
fn external_provider_tree_mutation_requires_a_new_pinned_receipt_and_cache() {
    use openpilot_runtime_build::inputs::Tree;
    let mut fixture = Fixture::new();
    let provider = fixture.cache.parent().unwrap().join("provider");
    fs::create_dir(&provider).unwrap();
    fs::write(provider.join("header.h"), b"version one").unwrap();
    let receipt = fixture.root.join("provider-tree.json");
    fs::write(
        &receipt,
        serde_json::to_vec(&Tree::capture(&provider).unwrap()).unwrap(),
    )
    .unwrap();
    fixture
        .recipe
        .input_trees
        .insert(provider.clone(), receipt.clone());
    fixture
        .recipe
        .inputs
        .insert(receipt.clone(), digest_file(&receipt).unwrap());
    commit(&fixture.root);
    let first = fixture.run();
    assert_eq!(
        first.verified_input_trees[&provider],
        digest_file(&receipt).unwrap()
    );
    fs::write(provider.join("header.h"), b"version two").unwrap();
    assert!(build(
        &fixture.root,
        &fixture.cache,
        &fixture.toolchain,
        &fixture.recipe
    )
    .is_err());
    fs::write(
        &receipt,
        serde_json::to_vec(&Tree::capture(&provider).unwrap()).unwrap(),
    )
    .unwrap();
    commit(&fixture.root);
    // Regenerating a receipt alone does not make it trusted: its pinned digest is still old.
    assert!(build(
        &fixture.root,
        &fixture.cache,
        &fixture.toolchain,
        &fixture.recipe
    )
    .is_err());
    fixture
        .recipe
        .inputs
        .insert(receipt.clone(), digest_file(&receipt).unwrap());
    let changed = fixture.run();
    assert_ne!(changed.cache_key, first.cache_key);
    assert!(!changed.artifacts[0].fresh);
    assert!(first.artifacts[0].executable.is_file());
    fixture.recipe.inputs.remove(&receipt);
    assert!(build(
        &fixture.root,
        &fixture.cache,
        &fixture.toolchain,
        &fixture.recipe
    )
    .is_err());
}
