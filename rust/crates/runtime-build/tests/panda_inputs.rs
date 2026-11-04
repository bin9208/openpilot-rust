//! Actual owned Git commits and output hashes; no firmware/compiler is mocked as
//! functional proof. Actions separately verifies the original four firmware bytes.
use openpilot_runtime_build::{
    digest_file,
    panda_inputs::{self, Contract},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Owned Panda firmware source fixture",
            "-c",
            "user.email=panda@example.invalid",
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
    output.stdout
}
fn commit(root: &Path) {
    git(root, &["add", "."]);
    git(
        root,
        &["commit", "--quiet", "-m", "Owned source input change"],
    );
}
struct Fixture {
    _temporary: tempfile::TempDir,
    source: PathBuf,
    provider: PathBuf,
    runtime: PathBuf,
    contract: Contract,
    files: BTreeMap<PathBuf, PathBuf>,
}
fn fixture() -> Fixture {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path().canonicalize().unwrap();
    let source = base.join("source");
    let provider = base.join("provider");
    let runtime = base.join("runtime");
    fs::create_dir_all(source.join("panda/board")).unwrap();
    fs::create_dir_all(provider.join("panda/board/obj")).unwrap();
    fs::create_dir_all(runtime.join("panda/board/obj")).unwrap();
    fs::write(source.join("panda/board/model.h"), "#define MODEL 1\n").unwrap();
    git(&source, &["init", "--quiet"]);
    commit(&source);
    let mut contract: Contract =
        serde_json::from_str(include_str!("../../../native-panda-source-contract.json")).unwrap();
    contract.original_source_commit = String::from_utf8(git(&source, &["rev-parse", "HEAD"]))
        .unwrap()
        .trim()
        .into();
    contract.entries = 1;
    contract.entries_sha256 = format!(
        "{:x}",
        Sha256::digest(git(
            &source,
            &["ls-tree", "-r", "-z", "HEAD", "--", "panda"]
        ))
    );
    let mut files = BTreeMap::new();
    for (path, sha) in &mut contract.outputs {
        let bytes = format!("owned metadata-only fixture {}", path.display());
        fs::create_dir_all(provider.join(path).parent().unwrap()).unwrap();
        fs::create_dir_all(runtime.join(path).parent().unwrap()).unwrap();
        fs::write(provider.join(path), &bytes).unwrap();
        fs::write(runtime.join(path), &bytes).unwrap();
        *sha = digest_file(&provider.join(path)).unwrap();
        files.insert(path.clone(), path.clone());
    }
    let mut bytes = serde_json::to_vec_pretty(&contract).unwrap();
    bytes.push(b'\n');
    fs::create_dir(source.join("rust")).unwrap();
    fs::write(source.join(panda_inputs::PATH), &bytes).unwrap();
    fs::write(provider.join(panda_inputs::ARTIFACT_CONTRACT), &bytes).unwrap();
    fs::write(runtime.join(panda_inputs::ARTIFACT_CONTRACT), &bytes).unwrap();
    files.insert(
        PathBuf::from(panda_inputs::ARTIFACT_CONTRACT),
        panda_inputs::ARTIFACT_CONTRACT.into(),
    );
    commit(&source);
    Fixture {
        _temporary: temporary,
        source,
        provider,
        runtime,
        contract,
        files,
    }
}

#[test]
fn docs_and_rust_changes_reuse_the_recorded_generation_inputs_without_rewriting_original_source() {
    let f = fixture();
    f.contract.verify_source(&f.source).unwrap();
    panda_inputs::verify_mapped(&f.source, &f.provider, &f.files).unwrap();
    for path in ["README.md", "rust/owned-runtime.rs", "docs/owned.md"] {
        let file = f.source.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, "owned independent change\n").unwrap();
        commit(&f.source);
        let current = f.contract.verify_source(&f.source).unwrap();
        assert_ne!(current.commit, f.contract.original_source_commit);
        panda_inputs::verify_runtime(&f.source, &f.runtime).unwrap();
    }
    let recorded = Contract::read(&f.runtime.join(panda_inputs::ARTIFACT_CONTRACT)).unwrap();
    assert_eq!(
        recorded.original_source_commit,
        f.contract.original_source_commit
    );
}

#[test]
fn changed_or_added_generation_sources_and_dirty_inputs_are_rejected() {
    for path in [
        "panda/board/model.h",
        "panda/board/added.h",
        "opendbc_repo/opendbc/safety/safety/added.h",
    ] {
        let f = fixture();
        let file = f.source.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "MODEL=2\n").unwrap();
        assert!(f.contract.verify_source(&f.source).is_err());
        commit(&f.source);
        assert!(f
            .contract
            .verify_source(&f.source)
            .unwrap_err()
            .to_string()
            .contains("generation inputs differ"));
        assert!(panda_inputs::verify_mapped(&f.source, &f.provider, &f.files).is_err());
        assert!(panda_inputs::verify_runtime(&f.source, &f.runtime).is_err());
    }
}

#[test]
fn missing_redirected_or_changed_outputs_cannot_be_sealed_or_selected_and_caches_are_not_needed_at_boot(
) {
    let mut f = fixture();
    let key = PathBuf::from("panda/board/obj/panda.bin.signed");
    let source = f.files.remove(&key).unwrap();
    assert!(panda_inputs::verify_mapped(&f.source, &f.provider, &f.files).is_err());
    f.files.insert(key.clone(), "../outside".into());
    assert!(panda_inputs::verify_mapped(&f.source, &f.provider, &f.files).is_err());
    f.files.insert(key, source.clone());
    let bytes = fs::read(f.provider.join(&source)).unwrap();
    fs::write(f.provider.join(&source), "wrong output").unwrap();
    assert!(panda_inputs::verify_mapped(&f.source, &f.provider, &f.files).is_err());
    fs::write(f.provider.join(&source), bytes).unwrap();
    panda_inputs::verify_mapped(&f.source, &f.provider, &f.files).unwrap();
    fs::remove_dir_all(&f.provider).unwrap();
    panda_inputs::verify_runtime(&f.source, &f.runtime).unwrap();
    fs::write(f.runtime.join(&source), "wrong cached output").unwrap();
    assert!(panda_inputs::verify_runtime(&f.source, &f.runtime).is_err());
}

#[test]
fn source_scope_and_output_roster_cannot_be_silently_narrowed() {
    let f = fixture();
    let path = f._temporary.path().join("bad.json");
    for field in ["roots", "outputs"] {
        let mut value = serde_json::to_value(&f.contract).unwrap();
        match field {
            "outputs" => {
                value[field].as_object_mut().unwrap().clear();
            }
            _ => {
                value[field].as_array_mut().unwrap().pop();
            }
        }
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(Contract::read(&path).is_err());
    }
}
