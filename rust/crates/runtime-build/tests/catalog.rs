//! Actual workspace metadata and actual Cargo executions. The two small test
//! packages exercise input assignment, not the complete AGNOS provider closure.
use openpilot_runtime_build::{
    build,
    catalog::{Bindings, Catalog, Group, InputSet},
    digest_file,
    prepare::RuntimeInputs,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn git(root: &Path, args: &[&str]) {
    let result = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Native plan fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn toolchain() -> PathBuf {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = Command::new(rustc)
        .args(["--print", "sysroot"])
        .output()
        .unwrap();
    assert!(output.status.success());
    PathBuf::from(String::from_utf8(output.stdout).unwrap().trim()).join("bin")
}

#[test]
fn actual_workspace_roster_and_package_features_are_audited_by_the_real_cli() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_openpilot-runtime-plan"))
        .arg("--root")
        .arg(root)
        .arg("--cargo")
        .arg(toolchain().join("cargo"))
        .arg("--check")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let audit: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(audit["groups"], 13);
    assert_eq!(audit["binaries"], 99);
    assert_eq!(audit["target"], "aarch64-unknown-linux-gnu");
    assert_eq!(audit["compiled"], false);
    assert_eq!(audit["activated"], false);
    assert!(audit["plan_sha256"].is_null());
    let metadata = Command::new(toolchain().join("cargo"))
        .current_dir(root)
        .args([
            "metadata",
            "--manifest-path",
            "rust/Cargo.toml",
            "--locked",
            "--offline",
            "--no-deps",
            "--format-version",
            "1",
        ])
        .output()
        .unwrap();
    assert!(metadata.status.success());
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let catalog: Catalog =
        serde_json::from_str(include_str!("../../../native-runtime-catalog.json")).unwrap();
    catalog.verify(&metadata).unwrap();
    let mut wrong = catalog.clone();
    wrong
        .groups
        .get_mut("00")
        .unwrap()
        .binaries
        .push("absent-workspace-binary".into());
    assert!(wrong
        .verify(&metadata)
        .unwrap_err()
        .to_string()
        .contains("owner"));
    let mut wrong = catalog.clone();
    wrong
        .groups
        .get_mut("00")
        .unwrap()
        .features
        .push("openpilot-cluster/absent-feature".into());
    assert!(wrong
        .verify(&metadata)
        .unwrap_err()
        .to_string()
        .contains("feature"));
    let mut wrong = catalog.clone();
    wrong.groups.get_mut("01").unwrap().features.retain(|name| {
        name != "openpilot-encoderd/native" && name != "openpilot-encoderd/visionipc-ion"
    });
    assert!(wrong
        .verify(&metadata)
        .unwrap_err()
        .to_string()
        .contains("required feature"));
    let mut wrong = catalog;
    wrong
        .groups
        .get_mut("00")
        .unwrap()
        .binaries
        .push("openpilot-loggerd".into());
    assert!(wrong
        .verify(&metadata)
        .unwrap_err()
        .to_string()
        .contains("multiple groups"));
}

#[test]
fn provider_and_model_changes_partition_actual_compiler_work_without_changing_other_groups() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source");
    fs::create_dir_all(root.join("rust/first/src")).unwrap();
    fs::create_dir_all(root.join("rust/second/src")).unwrap();
    fs::write(
        root.join("rust/Cargo.toml"),
        "[workspace]\nmembers=['first','second']\nresolver='2'\n",
    )
    .unwrap();
    fs::write(root.join("rust/Cargo.lock"), "version = 4\n\n[[package]]\nname = \"first\"\nversion = \"0.1.0\"\n\n[[package]]\nname = \"second\"\nversion = \"0.1.0\"\n").unwrap();
    for name in ["first", "second"] {
        fs::write(
            root.join(format!("rust/{name}/Cargo.toml")),
            format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n"),
        )
        .unwrap();
    }
    fs::write(
        root.join("rust/first/src/main.rs"),
        "fn main(){println!(\"{}\", env!(\"PROVIDER_VALUE\"));}\n",
    )
    .unwrap();
    fs::write(root.join("rust/first/build.rs"), "fn main(){let p=std::env::var(\"PROVIDER_FILE\").unwrap();let v=std::fs::read_to_string(&p).unwrap();println!(\"cargo:rustc-env=PROVIDER_VALUE={}\",v.trim());println!(\"cargo:rerun-if-changed={}\",p);println!(\"cargo:rerun-if-env-changed=PROVIDER_FILE\");}\n").unwrap();
    fs::write(
        root.join("rust/second/src/main.rs"),
        "fn main(){println!(\"42\");}\n",
    )
    .unwrap();
    git(&root, &["init", "--quiet"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "--quiet", "-m", "owned fixture"]);
    let tools = toolchain();
    let version = Command::new(tools.join("rustc"))
        .arg("-vV")
        .output()
        .unwrap();
    assert!(version.status.success());
    let version = String::from_utf8(version.stdout).unwrap();
    let target = version
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .unwrap()
        .to_owned();
    let catalog = Catalog {
        schema: 1,
        target,
        roster_provenance: "Owned two-package fixture, not a runtime catalog".into(),
        groups: ["first", "second"]
            .into_iter()
            .map(|name| {
                (
                    name.into(),
                    Group {
                        packages: vec![name.into()],
                        features: vec![],
                        binaries: vec![name.into()],
                    },
                )
            })
            .collect(),
    };
    let sdk_marker = temp.path().join("compiler-input-marker");
    let provider = temp.path().join("provider-value");
    let model = temp.path().join("runtime-model");
    fs::write(
        &sdk_marker,
        "Owned compiler marker; not an authenticated SDK",
    )
    .unwrap();
    fs::write(&provider, "42\n").unwrap();
    fs::write(&model, "model one").unwrap();
    let mut bindings = Bindings {
        schema: 1,
        input_sets: BTreeMap::from([
            (
                "compiler".into(),
                InputSet {
                    files: BTreeMap::from([(
                        sdk_marker.clone(),
                        digest_file(&sdk_marker).unwrap(),
                    )]),
                    ..InputSet::default()
                },
            ),
            (
                "provider".into(),
                InputSet {
                    files: BTreeMap::from([(provider.clone(), digest_file(&provider).unwrap())]),
                    environment: BTreeMap::from([(
                        "PROVIDER_FILE".into(),
                        provider.to_str().unwrap().into(),
                    )]),
                    ..InputSet::default()
                },
            ),
        ]),
        common: vec!["compiler".into()],
        groups: BTreeMap::from([
            ("first".into(), vec!["provider".into()]),
            ("second".into(), vec![]),
        ]),
        group_growth_bytes: BTreeMap::from([
            ("first".into(), 32 * 1024 * 1024),
            ("second".into(), 32 * 1024 * 1024),
        ]),
        expected_source_growth_bytes: 1024 * 1024,
        remaining_object_growth_bytes: 1024 * 1024,
        runtime_inputs: RuntimeInputs {
            files: BTreeMap::from([(model.clone(), digest_file(&model).unwrap())]),
            trees: BTreeMap::new(),
        },
    };
    let cache = temp.path().join("cache");
    let initial = catalog.compose(&bindings, &root).unwrap();
    let first = build(&root, &cache, &tools, &initial.groups["first"]).unwrap();
    let second = build(&root, &cache, &tools, &initial.groups["second"]).unwrap();
    assert!(first
        .artifacts
        .iter()
        .chain(&second.artifacts)
        .all(|a| !a.fresh));
    fs::write(&provider, "43\n").unwrap();
    assert!(catalog.compose(&bindings, &root).is_err());
    bindings
        .input_sets
        .get_mut("provider")
        .unwrap()
        .files
        .insert(provider.clone(), digest_file(&provider).unwrap());
    let changed = catalog.compose(&bindings, &root).unwrap();
    assert_eq!(
        serde_json::to_value(&initial.groups["second"]).unwrap(),
        serde_json::to_value(&changed.groups["second"]).unwrap()
    );
    let new_first = build(&root, &cache, &tools, &changed.groups["first"]).unwrap();
    let new_second = build(&root, &cache, &tools, &changed.groups["second"]).unwrap();
    assert_ne!(new_first.cache_key, first.cache_key);
    assert_ne!(new_first.artifacts[0].sha256, first.artifacts[0].sha256);
    assert!(!new_first.artifacts[0].fresh);
    assert_eq!(new_second.cache_key, second.cache_key);
    assert_eq!(new_second.artifacts[0].sha256, second.artifacts[0].sha256);
    assert!(new_second.artifacts[0].fresh);
    for (artifact, expected) in [
        (&first.artifacts[0], "42\n"),
        (&new_first.artifacts[0], "43\n"),
        (&new_second.artifacts[0], "42\n"),
    ] {
        let result = Command::new(&artifact.executable).output().unwrap();
        assert!(result.status.success());
        assert_eq!(result.stdout, expected.as_bytes());
    }
    fs::write(&model, "model two").unwrap();
    assert!(catalog.compose(&bindings, &root).is_err());
    bindings
        .runtime_inputs
        .files
        .insert(model.clone(), digest_file(&model).unwrap());
    let model_only = catalog.compose(&bindings, &root).unwrap();
    for name in ["first", "second"] {
        assert_eq!(
            serde_json::to_value(&changed.groups[name]).unwrap(),
            serde_json::to_value(&model_only.groups[name]).unwrap()
        );
        assert!(build(&root, &cache, &tools, &model_only.groups[name])
            .unwrap()
            .artifacts
            .iter()
            .all(|a| a.fresh));
    }
    let mut bad = bindings.clone();
    bad.groups.remove("second");
    assert!(catalog.compose(&bad, &root).is_err());
    let mut bad = bindings.clone();
    bad.input_sets
        .get_mut("provider")
        .unwrap()
        .environment
        .insert("OPENPILOT_NATIVE_INCREMENTAL".into(), "0".into());
    assert!(catalog.compose(&bad, &root).is_err());
    let mut bad = bindings.clone();
    bad.groups
        .get_mut("second")
        .unwrap()
        .push("unknown-set".into());
    assert!(catalog.compose(&bad, &root).is_err());
    let mut bad = bindings;
    bad.groups
        .get_mut("second")
        .unwrap()
        .push("compiler".into());
    assert!(catalog.compose(&bad, &root).is_err());
}

#[test]
fn actual_full_plan_assembly_binds_its_receipt_and_never_replaces_an_existing_output() {
    // This marker only exercises assembly admission. The resulting plan is not
    // a complete SDK and cannot pass the mandatory generation toolchain gate.
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("owned-input-marker");
    fs::write(&marker, "owned input").unwrap();
    let catalog: Catalog =
        serde_json::from_str(include_str!("../../../native-runtime-catalog.json")).unwrap();
    let bindings = Bindings {
        schema: 1,
        input_sets: BTreeMap::from([(
            "owned-input".into(),
            InputSet {
                files: BTreeMap::from([(marker.clone(), digest_file(&marker).unwrap())]),
                ..InputSet::default()
            },
        )]),
        common: vec!["owned-input".into()],
        groups: catalog
            .groups
            .keys()
            .map(|key| (key.clone(), vec![]))
            .collect(),
        group_growth_bytes: catalog
            .groups
            .keys()
            .map(|key| (key.clone(), 32 * 1024 * 1024))
            .collect(),
        expected_source_growth_bytes: 1024 * 1024,
        remaining_object_growth_bytes: 1024 * 1024,
        runtime_inputs: RuntimeInputs::default(),
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let bindings_path = temp.path().join("bindings.json");
    fs::write(&bindings_path, serde_json::to_vec(&bindings).unwrap()).unwrap();
    let output = temp.path().join("new-plan");
    let invoke = |output: &Path| {
        Command::new(env!("CARGO_BIN_EXE_openpilot-runtime-plan"))
            .arg("--root")
            .arg(root)
            .arg("--cargo")
            .arg(toolchain().join("cargo"))
            .arg("--bindings")
            .arg(&bindings_path)
            .arg("--output")
            .arg(output)
            .output()
            .unwrap()
    };
    let result = invoke(&output);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(
        receipt["bindings_sha256"],
        digest_file(&bindings_path).unwrap()
    );
    let plan_hash = digest_file(&output.join("build-plan.json")).unwrap();
    assert_eq!(receipt["plan_sha256"], plan_hash);
    let plan =
        openpilot_runtime_build::prepare::Plan::read(&output.join("build-plan.json")).unwrap();
    assert_eq!(plan.groups.len(), 13);
    assert!(plan
        .groups
        .values()
        .all(|g| g.environment["OPENPILOT_NATIVE_INCREMENTAL"] == "1"));
    assert!(!invoke(&output).status.success());
    assert_eq!(
        digest_file(&output.join("build-plan.json")).unwrap(),
        plan_hash
    );
    let linked = temp.path().join("output-symlink");
    std::os::unix::fs::symlink(&output, &linked).unwrap();
    assert!(!invoke(&linked).status.success());
    assert!(linked.is_symlink());
    assert_eq!(
        digest_file(&output.join("build-plan.json")).unwrap(),
        plan_hash
    );
    fs::write(&marker, "tampered input").unwrap();
    let rejected = temp.path().join("rejected-plan");
    assert!(!invoke(&rejected).status.success());
    assert!(!rejected.exists());
}
