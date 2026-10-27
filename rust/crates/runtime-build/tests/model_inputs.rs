#[path = "../../runtime-installer/tests/support/model_inputs.rs"]
mod support;
use openpilot_runtime_build::model_inputs;

#[test]
fn complete_model_mapping_is_required_and_cannot_redirect_a_camera() {
    let f = support::Fixture::new();
    let verified = model_inputs::verify(&f.source, &f.provider).unwrap();
    let mut mapping = verified
        .keys()
        .map(|p| (std::path::Path::new("rust/bin").join(p), p.clone()))
        .collect();
    model_inputs::verify_mapped(&f.source, &f.provider, &mapping).unwrap();
    let key = std::path::PathBuf::from("rust/bin/driver-models/current.json");
    mapping.insert(key.clone(), "driving-models/current.json".into());
    assert!(model_inputs::verify_mapped(&f.source, &f.provider, &mapping).is_err());
    mapping = verified
        .keys()
        .map(|p| (std::path::Path::new("rust/bin").join(p), p.clone()))
        .collect();
    mapping.insert(
        "rust/bin/driver-models/unexpected".into(),
        "other-provider-file".into(),
    );
    assert!(model_inputs::verify_mapped(&f.source, &f.provider, &mapping).is_err());
    mapping.remove(&key);
    assert!(model_inputs::verify_mapped(&f.source, &f.provider, &mapping).is_err());
    assert!(f.temporary.path().exists());
}

#[test]
fn real_model_cli_verifies_inputs_and_missing_models_stop_before_any_build() {
    use openpilot_runtime_build::{prepare, Recipe};
    use std::{collections::BTreeMap, process::Command};
    let f = support::Fixture::new();
    let output = Command::new(env!("CARGO_BIN_EXE_openpilot-runtime-inputs"))
        .args(["--verify-models", "--root"])
        .arg(&f.source)
        .arg("--store")
        .arg(&f.provider)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["gpu_executed"], false);
    assert_eq!(value["runtime_activated"], false);
    let base = f.temporary.path();
    let plan = prepare::Plan {
        schema: 1,
        groups: BTreeMap::from([(
            "owned".into(),
            Recipe {
                schema: 1,
                target: "aarch64-unknown-linux-gnu".into(),
                packages: vec!["owned".into()],
                binaries: vec!["openpilot-driving-modeld".into()],
                features: vec![],
                inputs: BTreeMap::new(),
                input_trees: BTreeMap::new(),
                environment: BTreeMap::new(),
                expected_growth_bytes: 1024 * 1024,
            },
        )]),
        expected_source_growth_bytes: 64 * 1024 * 1024,
        remaining_object_growth_bytes: 1024 * 1024,
        runtime_inputs: Default::default(),
    };
    let cache = base.join("must-not-build");
    let result = prepare::run(
        &f.source,
        &cache,
        &base.join("absent-toolchain"),
        &base.join("sources"),
        &base.join("objects"),
        &plan,
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("source-bound model runtime input"));
    assert!(!cache.exists());
}
