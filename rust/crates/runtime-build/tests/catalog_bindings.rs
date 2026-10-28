//! Assignment and rejection checks; the genuine AGNOS job supplies real pinned
//! compiler Trees to the same CLI. These fixtures do not claim a runtime build.
use openpilot_runtime_build::{
    catalog::{Bindings, Catalog},
    catalog_bindings::{Budgets, Kind, NativeInputs, Pin},
    prepare::RuntimeInputs,
};
use std::{collections::BTreeMap, path::PathBuf, process::Command};

fn fixture() -> (Catalog, NativeInputs, Budgets) {
    let catalog: Catalog =
        serde_json::from_str(include_str!("../../../native-runtime-catalog.json")).unwrap();
    let inputs = NativeInputs {
        schema: 1,
        inputs: [
            Kind::Rust,
            Kind::Cargo,
            Kind::SystemSdk,
            Kind::Capnp,
            Kind::Ffmpeg,
            Kind::Libyuv,
            Kind::Raylib,
            Kind::Opencv,
            Kind::Eigen,
        ]
        .into_iter()
        .map(|kind| {
            (
                kind,
                Pin {
                    root: if kind == Kind::SystemSdk {
                        PathBuf::from("/")
                    } else {
                        PathBuf::from(format!("/owned/{kind:?}"))
                    },
                    receipt: PathBuf::from(format!("/owned/receipts/{kind:?}.json")),
                    sha256: "1".repeat(64),
                },
            )
        })
        .collect(),
        runtime_inputs: RuntimeInputs::default(),
    };
    let budgets = Budgets {
        schema: 1,
        group_growth_bytes: catalog
            .groups
            .keys()
            .map(|key| (key.clone(), 1024))
            .collect(),
        expected_source_growth_bytes: 1024,
        remaining_object_growth_bytes: 1024,
        estimate_basis: "Owned assignment fixture only; not a measured build estimate".into(),
    };
    (catalog, inputs, budgets)
}

fn identity(bindings: &Bindings, group: &str) -> serde_json::Value {
    let sets: BTreeMap<_, _> = bindings
        .common
        .iter()
        .chain(&bindings.groups[group])
        .map(|name| (name, &bindings.input_sets[name]))
        .collect();
    serde_json::to_value(sets).unwrap()
}

#[test]
fn each_provider_changes_only_consuming_groups_and_models_change_no_compiler_group() {
    let (catalog, inputs, budgets) = fixture();
    let before = inputs.bindings(&catalog, &budgets).unwrap();
    assert_eq!(before.groups.len(), 13);
    for (role, affected) in [
        (Kind::Raylib, vec!["00", "01", "02"]),
        (Kind::Libyuv, vec!["00", "01"]),
        (Kind::Opencv, vec!["03", "04"]),
        (Kind::Eigen, vec!["04", "06"]),
        (Kind::Ffmpeg, vec!["00", "01", "03", "07", "08", "09", "10"]),
    ] {
        let mut changed = inputs.clone();
        changed.inputs.get_mut(&role).unwrap().sha256 = "2".repeat(64);
        let changed = changed.bindings(&catalog, &budgets).unwrap();
        for name in catalog.groups.keys() {
            assert_eq!(
                identity(&before, name) != identity(&changed, name),
                affected.contains(&name.as_str()),
                "{role:?}:{name}"
            );
        }
    }
    let mut changed = inputs.clone();
    changed
        .runtime_inputs
        .files
        .insert("/owned/model".into(), "2".repeat(64));
    let changed = changed.bindings(&catalog, &budgets).unwrap();
    for name in catalog.groups.keys() {
        assert_eq!(identity(&before, name), identity(&changed, name));
    }
    let env = &before.input_sets["environment-03"].environment;
    assert_eq!(env["LIBCLANG_PATH"], "/usr/lib/llvm-18/lib");
    assert_eq!(env["OPENPILOT_WEBCAM_OPENCV_ROOT"], "/owned/Opencv");
    for (group, key, expected) in [
        ("00", "FFMPEG_DIR", "/owned/Ffmpeg"),
        ("02", "STARTUP_UI_RAYLIB_ROOT", "/owned/Raylib"),
        ("04", "OPENPILOT_OPENCV_ROOT", "/owned/Opencv"),
        ("04", "LOCATIOND_EIGEN_INCLUDE", "/owned/Eigen"),
        ("06", "PARAMSD_EIGEN_INCLUDE", "/owned/Eigen"),
    ] {
        assert_eq!(
            before.input_sets[&format!("environment-{group}")].environment[key],
            expected,
            "{group}:{key}"
        );
    }
    assert!(env["LD_LIBRARY_PATH"].contains("/owned/Opencv/install/lib"));
    assert!(!env["LD_LIBRARY_PATH"].contains("/owned/Raylib"));
    assert!(!env.contains_key("OPENPILOT_CARGO_VENDOR"));
    assert_eq!(
        before.input_sets["cargo"].environment["OPENPILOT_CARGO_VENDOR"],
        "/owned/Cargo/vendor"
    );
}

#[test]
fn missing_or_extra_roles_ambiguous_paths_and_missing_estimates_are_rejected() {
    let (catalog, inputs, budgets) = fixture();
    for kind in inputs.inputs.keys() {
        let mut bad = inputs.clone();
        bad.inputs.remove(kind);
        assert!(bad.bindings(&catalog, &budgets).is_err());
    }
    for root in [
        "/",
        "relative",
        "/owned/../foreign",
        "/owned/a b",
        "/owned/a:b",
        "/owned/a,b",
        "/owned/line\nfeed",
    ] {
        let mut bad = inputs.clone();
        bad.inputs.get_mut(&Kind::Rust).unwrap().root = root.into();
        assert!(bad.bindings(&catalog, &budgets).is_err(), "{root}");
    }
    let mut bad = inputs.clone();
    bad.inputs.get_mut(&Kind::SystemSdk).unwrap().root = "/owned/image".into();
    assert!(bad.bindings(&catalog, &budgets).is_err());
    let mut bad = catalog.clone();
    bad.groups.values_mut().for_each(|g| {
        g.compiler_inputs.remove("PARAMSD_EIGEN_INCLUDE");
        g.compiler_inputs.remove("LOCATIOND_EIGEN_INCLUDE");
    });
    assert!(inputs.bindings(&bad, &budgets).is_err());
    let mut bad = budgets.clone();
    bad.group_growth_bytes.remove("12");
    assert!(inputs.bindings(&catalog, &bad).is_err());
    let mut bad = budgets.clone();
    bad.group_growth_bytes.insert("12".into(), 0);
    assert!(inputs.bindings(&catalog, &bad).is_err());
    let mut bad = budgets.clone();
    bad.estimate_basis.clear();
    assert!(inputs.bindings(&catalog, &bad).is_err());
    let mut bad = inputs;
    bad.inputs.get_mut(&Kind::Cargo).unwrap().sha256 = "A".repeat(64);
    assert!(bad.bindings(&catalog, &budgets).is_err());
}

#[test]
fn output_cannot_mutate_source_or_any_pinned_input_tree() {
    let directory = tempfile::tempdir().unwrap();
    let base = directory.path().canonicalize().unwrap();
    for name in ["source", "rust", "models"] {
        std::fs::create_dir(base.join(name)).unwrap();
    }
    let (_, mut inputs, _) = fixture();
    inputs.inputs.get_mut(&Kind::Rust).unwrap().root = base.join("rust");
    inputs
        .runtime_inputs
        .trees
        .insert(base.join("models"), base.join("models.json"));
    for name in ["source", "rust", "models"] {
        assert!(inputs
            .check_output(&base.join("source"), &base.join(name).join("plan"))
            .is_err());
    }
    inputs
        .check_output(&base.join("source"), &base.join("plan"))
        .unwrap();
    assert!(inputs
        .check_output(&base.join("source"), &PathBuf::from("/usr/native-plan"))
        .is_err());
    let alias = base.join("model-alias");
    std::os::unix::fs::symlink(base.join("models"), &alias).unwrap();
    inputs.runtime_inputs.trees = BTreeMap::from([(alias, base.join("models.json"))]);
    assert!(inputs
        .check_output(&base.join("source"), &base.join("models/plan"))
        .is_err());
}

#[test]
fn cli_rejects_incomplete_or_conflicting_assembly_modes_before_metadata_execution() {
    for args in [
        vec!["--native-inputs", "pins.json"],
        vec!["--budgets", "budgets.json"],
        vec![
            "--check",
            "--native-inputs",
            "pins.json",
            "--budgets",
            "budgets.json",
        ],
        vec![
            "--bindings",
            "bindings.json",
            "--native-inputs",
            "pins.json",
            "--budgets",
            "budgets.json",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_openpilot-runtime-plan"))
            .args(["--root", "/nonexistent", "--cargo", "/nonexistent/cargo"])
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("choose --check"));
    }
}

#[test]
fn assembly_checks_current_vendor_lock_and_does_not_inherit_another_sets_environment() {
    use openpilot_runtime_build::{catalog::InputSet, digest_file, inputs::Tree};
    let directory = tempfile::tempdir().unwrap();
    let base = directory.path().canonicalize().unwrap();
    let source = base.join("source");
    let prefix = base.join("cargo");
    std::fs::create_dir_all(source.join("rust")).unwrap();
    std::fs::create_dir_all(prefix.join("vendor")).unwrap();
    std::fs::create_dir(prefix.join("provenance")).unwrap();
    std::fs::write(source.join("rust/Cargo.lock"), "owned lock fixture").unwrap();
    let provenance = prefix.join("provenance/native-cargo.json");
    let receipt = base.join("tree.json");
    let marker = base.join("sdk-marker");
    std::fs::write(&marker, "owned marker; not an SDK").unwrap();
    let (catalog, _, budgets) = fixture();
    for matching in [false, true] {
        std::fs::write(&provenance, serde_json::to_vec(&serde_json::json!({
            "schema":1, "format":"openpilot-native-cargo-v1",
            "cargo_lock_sha256":if matching { digest_file(&source.join("rust/Cargo.lock")).unwrap() } else { "0".repeat(64) }
        })).unwrap()).unwrap();
        std::fs::write(
            &receipt,
            serde_json::to_vec(&Tree::capture(&prefix).unwrap()).unwrap(),
        )
        .unwrap();
        let bindings = Bindings {
            schema: 1,
            input_sets: BTreeMap::from([
                (
                    "vendor".into(),
                    InputSet {
                        files: BTreeMap::from([(receipt.clone(), digest_file(&receipt).unwrap())]),
                        trees: BTreeMap::from([(prefix.clone(), receipt.clone())]),
                        environment: BTreeMap::from([(
                            "OPENPILOT_CARGO_VENDOR".into(),
                            prefix.join("vendor").display().to_string(),
                        )]),
                    },
                ),
                (
                    "system".into(),
                    InputSet {
                        files: BTreeMap::from([(marker.clone(), digest_file(&marker).unwrap())]),
                        ..InputSet::default()
                    },
                ),
            ]),
            common: vec!["vendor".into(), "system".into()],
            groups: catalog
                .groups
                .keys()
                .map(|key| (key.clone(), vec![]))
                .collect(),
            group_growth_bytes: budgets.group_growth_bytes.clone(),
            expected_source_growth_bytes: 1024,
            remaining_object_growth_bytes: 1024,
            runtime_inputs: RuntimeInputs::default(),
        };
        let result = catalog.compose(&bindings, &source);
        if matching {
            result.unwrap();
        } else {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("current lock file"));
        }
    }
}
