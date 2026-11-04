//! Metadata-only owned fixtures; no fake byte is claimed as a functional model.
use openpilot_runtime_build::{
    digest_file,
    usbgpu_inputs::{self, Contract},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let result = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Owned eGPU input fixture",
            "-c",
            "user.email=egpu@example.invalid",
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
    result.stdout
}
fn commit(root: &Path) {
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "Owned source input"]);
}
struct Fixture {
    _temporary: tempfile::TempDir,
    source: PathBuf,
    provider: PathBuf,
    contract: Contract,
}
fn fixture() -> Fixture {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source");
    let provider = temporary.path().join("provider");
    fs::create_dir_all(source.join("rust/tools")).unwrap();
    fs::create_dir(&provider).unwrap();
    fs::write(
        source.join("rust/tools/import_usbgpu_hcq.py"),
        "# owned producer input\n",
    )
    .unwrap();
    git(&source, &["init", "--quiet"]);
    commit(&source);
    let mut contract: Contract =
        serde_json::from_str(include_str!("../../../native-usbgpu-source-contract.json")).unwrap();
    contract.original_source_commit = String::from_utf8(git(&source, &["rev-parse", "HEAD"]))
        .unwrap()
        .trim()
        .into();
    contract.entries = 1;
    contract.entries_sha256 = format!(
        "{:x}",
        Sha256::digest(git(
            &source,
            &["ls-tree", "-r", "-z", "HEAD", "--", "rust/tools"]
        ))
    );
    for (path, hash) in &mut contract.outputs {
        if path == Path::new(usbgpu_inputs::MODEL_PATH) {
            continue;
        }
        fs::create_dir_all(provider.join(path).parent().unwrap()).unwrap();
        fs::write(
            provider.join(path),
            format!("owned metadata {}", path.display()),
        )
        .unwrap();
        *hash = digest_file(&provider.join(path)).unwrap();
    }
    let bytes = serde_json::to_vec_pretty(&contract).unwrap();
    fs::write(source.join(usbgpu_inputs::PATH), &bytes).unwrap();
    fs::write(provider.join(usbgpu_inputs::ARTIFACT_CONTRACT), &bytes).unwrap();
    commit(&source);
    Fixture {
        _temporary: temporary,
        source,
        provider,
        contract,
    }
}

#[test]
fn independent_companion_reuse_is_source_bound_and_cannot_claim_a_missing_model() {
    let f = fixture();
    usbgpu_inputs::verify_companions(&f.source, &f.provider).unwrap();
    assert!(usbgpu_inputs::verify_runtime(&f.source, &f.provider).is_err());
    fs::write(
        f.source.join("README.md"),
        "owned docs-only source change\n",
    )
    .unwrap();
    commit(&f.source);
    usbgpu_inputs::verify_companions(&f.source, &f.provider).unwrap();
    let current = f.contract.verify_source(&f.source).unwrap();
    assert_ne!(current.commit, f.contract.original_source_commit);
    fs::write(
        f.source.join("rust/tools/import_usbgpu_hcq.py"),
        "# changed producer\n",
    )
    .unwrap();
    assert!(usbgpu_inputs::verify_companions(&f.source, &f.provider).is_err());
    commit(&f.source);
    assert!(f.contract.verify_source(&f.source).is_err());
}

#[test]
fn added_changed_and_redirected_data_cannot_enter_the_companion_prefix() {
    use std::os::unix::fs::symlink;
    let f = fixture();
    let added = f.provider.join("rust/bin/usbgpu-assets/extra");
    fs::write(&added, "extra").unwrap();
    assert!(usbgpu_inputs::verify_companions(&f.source, &f.provider).is_err());
    fs::remove_file(&added).unwrap();
    symlink("manifest.json", &added).unwrap();
    assert!(usbgpu_inputs::verify_companions(&f.source, &f.provider).is_err());
    fs::remove_file(&added).unwrap();
    usbgpu_inputs::verify_companions(&f.source, &f.provider).unwrap();
    fs::write(
        f.provider.join("rust/bin/usbgpu-assets/manifest.json"),
        "changed",
    )
    .unwrap();
    assert!(usbgpu_inputs::verify_companions(&f.source, &f.provider).is_err());
}

#[test]
fn source_scope_and_model_contract_cannot_be_narrowed_or_retagged() {
    let f = fixture();
    let path = f._temporary.path().join("bad.json");
    for field in ["roots", "outputs", "model"] {
        let mut value = serde_json::to_value(&f.contract).unwrap();
        match field {
            "roots" => {
                value[field].as_array_mut().unwrap().pop();
            }
            "outputs" => {
                value[field].as_object_mut().unwrap().clear();
            }
            _ => {
                value[field]["sha256"] = "0".repeat(64).into();
            }
        }
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(Contract::read(&path).is_err());
    }
}
