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
fn changed_cxx_tool_rebuilds_its_real_generated_input_without_relabelling_previous_output() {
    use std::os::unix::fs::PermissionsExt;
    let mut fixture = Fixture::new();
    let wrapper = fixture._temp.path().join("native-cxx");
    let compiler = fixture._temp.path().join("driver-body");
    // Driver aliases such as clang++ dispatch by argv[0]. Hash their actual
    // target while preserving the invocation spelling, including symlinks.
    std::os::unix::fs::symlink("driver-body", &wrapper).unwrap();
    let install = |value| {
        fs::write(&compiler, format!("#!/bin/sh\ntest \"${{0##*/}}\" = native-cxx || exit 42\nexec /usr/bin/g++ \"$@\" -DNATIVE_VALUE={value}\n")).unwrap();
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700)).unwrap();
    };
    install(42);
    fixture
        .recipe
        .environment
        .insert("CXX".into(), wrapper.to_string_lossy().into_owned());
    fs::write(
        fixture.root.join("rust/provider.cc"),
        "#include <iostream>\nint main(){ std::cout << NATIVE_VALUE << std::endl; }\n",
    )
    .unwrap();
    fs::write(fixture.root.join("rust/build.rs"), r#"
use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=provider.cc");
    println!("cargo:rerun-if-env-changed=CXX");
    let provider = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("provider");
    assert!(Command::new(env::var_os("CXX").unwrap()).args(["-std=c++17", "provider.cc", "-o"])
        .arg(&provider).status().unwrap().success());
    let output = Command::new(provider).output().unwrap();
    assert!(output.status.success());
    println!("cargo:rustc-env=NATIVE_PROVIDER_VALUE={}", String::from_utf8(output.stdout).unwrap().trim());
}
"#).unwrap();
    fs::write(
        fixture.root.join("rust/src/main.rs"),
        "fn main(){ println!(\"{}\", env!(\"NATIVE_PROVIDER_VALUE\")); }\n",
    )
    .unwrap();
    commit(&fixture.root);
    let first = fixture.run();
    assert_eq!(
        first.native_tools[&compiler],
        digest_file(&compiler).unwrap()
    );
    assert_eq!(
        Command::new(&first.artifacts[0].executable)
            .output()
            .unwrap()
            .stdout,
        b"42\n"
    );
    assert!(fixture.run().artifacts[0].fresh);
    // Same path, recipe, source and gcc version; the actual C++ tool changed.
    install(43);
    let changed = fixture.run();
    assert_eq!(changed.source, first.source);
    assert_eq!(changed.recipe_sha256, first.recipe_sha256);
    assert_ne!(changed.cache_key, first.cache_key);
    assert_ne!(
        changed.native_tools[&compiler],
        first.native_tools[&compiler]
    );
    assert!(!changed.artifacts[0].fresh);
    assert_ne!(changed.artifacts[0].sha256, first.artifacts[0].sha256);
    assert_eq!(
        Command::new(&changed.artifacts[0].executable)
            .output()
            .unwrap()
            .stdout,
        b"43\n"
    );
    assert_eq!(
        Command::new(&first.artifacts[0].executable)
            .output()
            .unwrap()
            .stdout,
        b"42\n"
    );
}

#[test]
fn serial_package_groups_reuse_their_real_shared_dependency() {
    let mut fixture = Fixture::new();
    fs::create_dir_all(fixture.root.join("rust/shared/src")).unwrap();
    fs::create_dir_all(fixture.root.join("rust/other/src")).unwrap();
    fs::write(fixture.root.join("rust/Cargo.toml"),
        "[package]\nname='runtime-fixture'\nversion='0.1.0'\nedition='2021'\n[workspace]\nmembers=['shared','other']\n[dependencies]\nruntime-shared={path='shared'}\n").unwrap();
    fs::write(fixture.root.join("rust/other/Cargo.toml"),
        "[package]\nname='runtime-other'\nversion='0.1.0'\nedition='2021'\n[dependencies]\nruntime-shared={path='../shared'}\n").unwrap();
    fs::write(
        fixture.root.join("rust/shared/Cargo.toml"),
        "[package]\nname='runtime-shared'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();
    let shared = fixture.root.join("rust/shared/src/lib.rs");
    fs::write(&shared, "pub fn value() -> u32 { 42 }\n").unwrap();
    fs::write(
        fixture.root.join("rust/src/main.rs"),
        "fn main() { println!(\"main {}\", runtime_shared::value()); }\n",
    )
    .unwrap();
    fs::write(
        fixture.root.join("rust/other/src/main.rs"),
        "fn main() { println!(\"other {}\", runtime_shared::value()); }\n",
    )
    .unwrap();
    fs::write(
        fixture.root.join("rust/shared/build.rs"),
        r#"
use std::{fs::OpenOptions, io::Write};
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/lib.rs");
    let mut trace = OpenOptions::new().create(true).append(true)
        .open(std::env::var_os("SHARED_BUILD_TRACE").unwrap()).unwrap();
    writeln!(trace, "compiled").unwrap();
}

"#,
    )
    .unwrap();
    assert!(Command::new(fixture.toolchain.join("cargo"))
        .args(["generate-lockfile", "--offline", "--manifest-path"])
        .arg(fixture.root.join("rust/Cargo.toml"))
        .status()
        .unwrap()
        .success());
    commit(&fixture.root);
    let trace = fixture._temp.path().join("shared-build-trace");
    fixture.recipe.environment.insert(
        "SHARED_BUILD_TRACE".into(),
        trace.to_string_lossy().into_owned(),
    );
    let mut other = fixture.recipe.clone();
    other.packages = vec!["runtime-other".into()];
    other.binaries = vec!["runtime-other".into()];
    let first = fixture.run();
    let libraries: Vec<_> = fs::read_dir(
        first
            .target_directory
            .join(&fixture.recipe.target)
            .join("release/deps"),
    )
    .unwrap()
    .map(|entry| entry.unwrap().path())
    .filter(|path| {
        path.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("libruntime_shared-")
            && path
                .extension()
                .is_some_and(|extension| extension == "rlib")
    })
    .collect();
    assert_eq!(libraries.len(), 1);
    let library = &libraries[0];
    let library_before = (
        fs::metadata(library).unwrap().modified().unwrap(),
        digest_file(library).unwrap(),
    );
    let second = build(&fixture.root, &fixture.cache, &fixture.toolchain, &other).unwrap();
    assert_eq!(first.cache_key, second.cache_key);
    assert_eq!(first.target_directory, second.target_directory);
    assert_ne!(first.recipe_sha256, second.recipe_sha256);
    assert_eq!(fs::read_to_string(&trace).unwrap(), "compiled\n");
    assert_eq!(
        library_before,
        (
            fs::metadata(library).unwrap().modified().unwrap(),
            digest_file(library).unwrap()
        )
    );
    assert!(!first.artifacts[0].fresh && !second.artifacts[0].fresh);
    assert!(fixture.run().artifacts[0].fresh);
    assert!(
        build(&fixture.root, &fixture.cache, &fixture.toolchain, &other)
            .unwrap()
            .artifacts[0]
            .fresh
    );
    // A real shared source change must still rebuild both consumers, once for
    // the dependency, with actual changed executable results.
    fs::write(&shared, "pub fn value() -> u32 { 43 }\n").unwrap();
    commit(&fixture.root);
    let changed_main = fixture.run();
    let changed_other = build(&fixture.root, &fixture.cache, &fixture.toolchain, &other).unwrap();
    assert_eq!(fs::read_to_string(&trace).unwrap(), "compiled\ncompiled\n");
    for (before, after, expected) in [
        (&first, &changed_main, "main 43\n"),
        (&second, &changed_other, "other 43\n"),
    ] {
        assert!(!after.artifacts[0].fresh);
        assert_ne!(before.artifacts[0].sha256, after.artifacts[0].sha256);
        let output = Command::new(&after.artifacts[0].executable)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, expected.as_bytes());
    }
    println!("serial_groups=2 shared_build_script_initial_runs=1 shared_library_hash_and_mtime_retained=true changed_shared_build_script_runs=1 actual_consumers_rebuilt=true");
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
    // An auxiliary schema tool belongs only to the group that consumes it.
    // This owned executable tests identity bookkeeping; genuine capnp is
    // exercised separately against the complete schemas in AGNOS.
    let schema_tool = fixture._temp.path().join("owned-schema-tool");
    fs::write(&schema_tool, "#!/bin/sh\nprintf 'owned schema tool\\n'\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&schema_tool, fs::Permissions::from_mode(0o555)).unwrap();
    }
    other.environment.insert(
        "OPENPILOT_CAPNP".into(),
        schema_tool.to_str().unwrap().into(),
    );
    other
        .inputs
        .insert(schema_tool.clone(), digest_file(&schema_tool).unwrap());
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
    assert!(!first.reports["main"]
        .native_tools
        .contains_key(&schema_tool));
    assert_eq!(
        first.reports["other"].native_tools[&schema_tool],
        digest_file(&schema_tool).unwrap()
    );
    first.verify(&source_store, &object_store).unwrap();
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
    let shared_tool = first.reports["main"]
        .native_tools
        .keys()
        .next()
        .unwrap()
        .clone();
    let mut conflicting = first.clone();
    conflicting
        .reports
        .get_mut("other")
        .unwrap()
        .native_tools
        .insert(shared_tool, "0".repeat(64));
    assert!(conflicting.verify(&source_store, &object_store).is_err());
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

#[test]
fn explicit_schema_compiler_changes_rebuild_actual_generated_input_and_reject_a_stale_pin() {
    use std::os::unix::fs::PermissionsExt;
    let mut fixture = Fixture::new();
    let compiler = fixture._temp.path().join("owned-capnp-input-tool");
    let install = |value| {
        fs::write(&compiler, format!("#!/bin/sh\nprintf '{value}\\n'\n")).unwrap();
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o555)).unwrap();
    };
    install(42);
    fixture
        .recipe
        .environment
        .insert("OPENPILOT_CAPNP".into(), compiler.to_str().unwrap().into());
    fixture
        .recipe
        .inputs
        .insert(compiler.clone(), digest_file(&compiler).unwrap());
    fs::write(fixture.root.join("rust/build.rs"), r#"
fn main() {
    let tool = std::env::var("OPENPILOT_CAPNP").unwrap();
    let output = std::process::Command::new(&tool).output().unwrap();
    assert!(output.status.success());
    println!("cargo:rerun-if-env-changed=OPENPILOT_CAPNP");
    println!("cargo:rerun-if-changed={tool}");
    println!("cargo:rustc-env=OWNED_GENERATED_VALUE={}", String::from_utf8(output.stdout).unwrap().trim());
}
"#).unwrap();
    fs::write(
        fixture.root.join("rust/src/main.rs"),
        "fn main(){println!(\"{}\",env!(\"OWNED_GENERATED_VALUE\"));}\n",
    )
    .unwrap();
    commit(&fixture.root);
    let first = fixture.run();
    assert_eq!(
        first.native_tools[&compiler],
        digest_file(&compiler).unwrap()
    );
    assert_eq!(
        Command::new(&first.artifacts[0].executable)
            .output()
            .unwrap()
            .stdout,
        b"42\n"
    );
    assert!(fixture.run().artifacts[0].fresh);
    // The helper is changed at the same path, size and original source commit.
    fs::set_permissions(&compiler, fs::Permissions::from_mode(0o755)).unwrap();
    install(43);
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
        .insert(compiler.clone(), digest_file(&compiler).unwrap());
    let changed = fixture.run();
    assert_eq!(changed.source, first.source);
    assert_ne!(changed.cache_key, first.cache_key);
    assert!(!changed.artifacts[0].fresh);
    assert_eq!(
        Command::new(&changed.artifacts[0].executable)
            .output()
            .unwrap()
            .stdout,
        b"43\n"
    );
    assert_eq!(
        Command::new(&first.artifacts[0].executable)
            .output()
            .unwrap()
            .stdout,
        b"42\n"
    );
}

#[cfg(feature = "prepare-cargo")]
#[test]
fn prepared_registry_sources_build_offline_with_an_empty_cargo_home_and_reject_stale_pins() {
    use openpilot_runtime_build::{
        assets::{Asset, Download},
        cargo_inputs::{self, Package, Plan},
        Result,
    };
    use sha2::{Digest, Sha256};
    struct Fetch(Vec<u8>);
    impl Download for Fetch {
        fn fetch(&self, _: &Asset, path: &std::path::Path) -> Result<u64> {
            fs::write(path, &self.0)?;
            Ok(self.0.len() as u64)
        }
    }
    let mut fixture = Fixture::new();
    let temp = tempfile::tempdir().unwrap();
    let mut archive = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::fast(),
    ));
    let manifest = "[package]\nname='owned-depot-dep'\nversion='0.1.0'\nedition='2021'\n";
    let code = "pub fn answer()->u8{42}\n";
    for (path, bytes) in [
        ("Cargo.toml", manifest.as_bytes()),
        ("src/lib.rs", code.as_bytes()),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        archive
            .append_data(&mut header, format!("owned-depot-dep-0.1.0/{path}"), bytes)
            .unwrap();
    }
    let archive = archive.into_inner().unwrap().finish().unwrap();
    let sha = format!("{:x}", Sha256::digest(&archive));
    fs::write(fixture.root.join("rust/Cargo.toml"), "[package]\nname='runtime-fixture'\nversion='0.1.0'\nedition='2021'\n[dependencies]\nowned-depot-dep='=0.1.0'\n").unwrap();
    let lock = format!("version = 4\n\n[[package]]\nname='runtime-fixture'\nversion='0.1.0'\ndependencies=['owned-depot-dep']\n\n[[package]]\nname='owned-depot-dep'\nversion='0.1.0'\nsource='registry+https://github.com/rust-lang/crates.io-index'\nchecksum='{sha}'\n");
    let lock_path = fixture.root.join("rust/Cargo.lock");
    fs::write(&lock_path, lock).unwrap();
    fs::write(
        fixture.root.join("rust/src/main.rs"),
        "fn main(){println!(\"{}\",owned_depot_dep::answer());}\n",
    )
    .unwrap();
    commit(&fixture.root);
    let plan = Plan {
        schema: 1,
        cargo_lock_sha256: digest_file(&lock_path).unwrap(),
        reserve_bytes: 1024 * 1024,
        packages: vec![Package {
            name: "owned-depot-dep".into(),
            version: "0.1.0".into(),
            asset: Asset {
                url: "https://owned.invalid/dependency.crate".into(),
                sha256: sha,
                bytes: archive.len() as u64,
            },
            unpacked_bytes: (manifest.len() + code.len()) as u64,
            members: 2,
        }],
    };
    let inputs = cargo_inputs::prepare(
        &plan,
        &lock_path,
        &temp.path().join("assets"),
        &temp.path().join("depot"),
        &Fetch(archive),
    )
    .unwrap();
    fixture.recipe.environment.insert(
        "OPENPILOT_CARGO_VENDOR".into(),
        inputs.vendor.to_str().unwrap().into(),
    );
    // The source directory alone is insufficient: its complete independently
    // pinned parent tree and the exact Cargo.lock provenance are mandatory.
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
        .insert(inputs.tree_receipt.clone(), inputs.tree_sha256.clone());
    fixture
        .recipe
        .input_trees
        .insert(inputs.root.clone(), inputs.tree_receipt.clone());
    assert!(!fixture.cache.join("cargo-home/registry").exists());
    let first = fixture.run();
    assert!(!first.artifacts[0].fresh);
    assert_eq!(
        Command::new(&first.artifacts[0].executable)
            .output()
            .unwrap()
            .stdout,
        b"42\n"
    );
    let repeated = fixture.run();
    assert!(repeated.artifacts[0].fresh);
    assert_eq!(repeated.artifacts[0].sha256, first.artifacts[0].sha256);
    fs::write(
        fixture.root.join("README.md"),
        "Owned dependency docs-only change\n",
    )
    .unwrap();
    commit(&fixture.root);
    let docs = fixture.run();
    assert!(docs.artifacts[0].fresh);
    assert_eq!(docs.artifacts[0].sha256, first.artifacts[0].sha256);
    let locked = fs::read(&lock_path).unwrap();
    let mut changed_lock = locked.clone();
    changed_lock.extend_from_slice(b"\n# Owned lock identity change\n");
    fs::write(&lock_path, changed_lock).unwrap();
    commit(&fixture.root);
    assert!(build(
        &fixture.root,
        &fixture.cache,
        &fixture.toolchain,
        &fixture.recipe
    )
    .unwrap_err()
    .to_string()
    .contains("current lock file"));
    fs::write(&lock_path, locked).unwrap();
    commit(&fixture.root);
    // Same-size source damage is refused before Cargo can reuse its old target.
    let source = inputs.vendor.join("owned-depot-dep-0.1.0/src/lib.rs");
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&source, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(&source, code.replace("42", "43")).unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o444)).unwrap();
    assert!(build(
        &fixture.root,
        &fixture.cache,
        &fixture.toolchain,
        &fixture.recipe
    )
    .is_err());
    assert_eq!(
        Command::new(&first.artifacts[0].executable)
            .output()
            .unwrap()
            .stdout,
        b"42\n"
    );
}
