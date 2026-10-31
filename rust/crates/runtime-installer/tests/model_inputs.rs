mod support {
    pub mod model_inputs;
}
use openpilot_runtime_installer::model_inputs;
use std::fs;
use support::model_inputs::{commit, file, Fixture};

#[test]
fn model_receipts_bind_current_source_but_preserve_original_doc_provenance() {
    let f = Fixture::new();
    let before = model_inputs::verify(&f.source, &f.provider).unwrap();
    file(&f.source.join("docs/owned.md"), b"owned docs change");
    file(
        &f.source.join("rust/crates/loggerd/src/owned.rs"),
        b"// unrelated runtime change\n",
    );
    commit(&f.source);
    assert_eq!(
        before,
        model_inputs::verify(&f.source, &f.provider).unwrap()
    );
    file(
        &f.source.join("tinygrad_repo/tinygrad/added.py"),
        b"# new producer input\n",
    );
    commit(&f.source);
    assert!(model_inputs::verify(&f.source, &f.provider).is_err());
    assert!(f.temporary.path().exists());
}

#[test]
fn verifier_uses_each_retained_version_one_scope_instead_of_updater_scope_bytes() {
    let original = Fixture::new();
    let mut scope: serde_json::Value =
        serde_json::from_slice(include_bytes!("../../../native-model-source-scope.json")).unwrap();
    scope["python_files"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!(
            "rust/tools/additional-version-one-input.py"
        ));
    let other = Fixture::with_scope(&serde_json::to_vec(&scope).unwrap());
    model_inputs::verify(&original.source, &original.provider).unwrap();
    model_inputs::verify(&other.source, &other.provider).unwrap();
    assert!(other.temporary.path().exists());
}

#[test]
fn model_compiler_or_model_git_change_rejects_stale_outputs() {
    for path in [
        "rust/crates/model-runtime/src/owned.py",
        "openpilot/selfdrive/modeld/models/dmonitoring_model.onnx",
        "openpilot/selfdrive/modeld/models/driving_supercombo.onnx",
        "rust/native-model-wheels.json",
    ] {
        let f = Fixture::new();
        model_inputs::verify(&f.source, &f.provider).unwrap();
        file(&f.source.join(path), b"changed bound input\n");
        commit(&f.source);
        assert!(
            model_inputs::verify(&f.source, &f.provider).is_err(),
            "{path}"
        );
    }
}

#[test]
fn model_receipt_payload_pointer_and_extra_files_fail_closed() {
    for case in ["receipt", "weights", "pointer", "extra", "symlink"] {
        let f = Fixture::new();
        let files = model_inputs::verify(&f.source, &f.provider).unwrap();
        let weights = files.keys().find(|p| p.ends_with("weights.bin")).unwrap();
        match case {
            "receipt" => file(
                &f.provider.join("model-inputs/driver-1928x1208.json"),
                b"{}",
            ),
            "weights" => file(&f.provider.join(weights), b"wrong owned data"),
            "pointer" => file(
                &f.provider.join("driver-models/current.json"),
                b"{\"version\":1,\"generation\":\"../escape\"}",
            ),
            "extra" => file(&f.provider.join("driver-models/unexpected"), b"extra"),
            _ => {
                let path = f.provider.join(weights);
                fs::remove_file(&path).unwrap();
                std::os::unix::fs::symlink("/etc/passwd", path).unwrap();
            }
        }
        assert!(
            model_inputs::verify(&f.source, &f.provider).is_err(),
            "{case}"
        );
    }
}
