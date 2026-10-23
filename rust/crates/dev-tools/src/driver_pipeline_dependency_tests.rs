//! These tests catch widened helper admission, unbound provenance and stale inputs.
//! They run only in the parent's exact-source Actions gate, never locally.
use super::*;
use std::{fs, os::unix::fs::symlink};

const SOURCE: &str = "9de275e61c04cce1d8b587d45ec0b5ac6cc9c9e4";
const NAME: &str = "rust/tools/check_pipeline_reference.py";
const HISTORICAL: &str = "63e8d880b79603215d7319011809dd3b5ec6699dae1e10c79c105b7dc5cc258b";
const MANIFEST: &str = "1b8e6cea511e9f212cd7737688b3e2ec0a165b9ff985b492f0e98c4639f18380";
const CURRENT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/check_pipeline_reference.py"
));
const ORIGINAL_MANIFEST: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/driver-pipeline-dependency/reference-manifest.json"
));

struct Fixture {
    root: tempfile::TempDir,
    original: OriginalCapture,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("rust/tools")).unwrap();
        fs::write(root.path().join(NAME), CURRENT).unwrap();
        let original_root = root.path().join("original");
        fs::create_dir(&original_root).unwrap();
        fs::write(
            original_root.join("reference-manifest.json"),
            ORIGINAL_MANIFEST,
        )
        .unwrap();
        let metadata = fs::metadata(&original_root).unwrap();
        // The admission unit reopens the real frozen manifest. Full packet
        // validation belongs to OriginalCapture::open, already required by run().
        let original = OriginalCapture {
            root: original_root,
            manifest: serde_json::from_slice(ORIGINAL_MANIFEST).unwrap(),
            manifest_sha256: MANIFEST.into(),
            root_identity: (metadata.dev(), metadata.ino()),
            collector_enabled: false,
        };
        Self { root, original }
    }
    fn context(&self) -> Context<'_> {
        Context {
            root: self.root.path(),
            original: &self.original,
            checkout: SOURCE,
            retired_current_catalog: true,
        }
    }
    fn pin(&self) -> InputPin {
        InputPin::open(&self.root.path().join(NAME), false).unwrap()
    }
}

#[test]
fn exact_dependency_pair_reports_both_full_hashes_and_original_provenance() {
    let f = Fixture::new();
    let value = attest(&f.context(), NAME, HISTORICAL, &f.pin())
        .unwrap()
        .receipt();
    assert_eq!(value["current_checkout_commit"], SOURCE);
    assert_eq!(value["review"]["original_manifest_sha256"], MANIFEST);
    assert_eq!(value["review"]["historical_helper_sha256"], HISTORICAL);
    assert_eq!(
        value["current_helper"]["sha256"],
        "10dfebf12b3d8fa5abdaff7764e80c11468043349ce44f3cc4e57a59b30312e5"
    );
    assert_eq!(
        value["review"]["imported_functions"]
            .as_object()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(value["review"]["source_retirement_credit"], 0);
}

#[test]
fn unchanged_functions_do_not_admit_an_unreviewed_whole_helper_edit() {
    let f = Fixture::new();
    let mut bytes = CURRENT.to_vec();
    bytes.extend_from_slice(b"\n# unreviewed change outside imported functions\n");
    fs::write(f.root.path().join(NAME), bytes).unwrap();
    assert!(attest(&f.context(), NAME, HISTORICAL, &f.pin()).is_err());
}

#[test]
fn changed_imported_function_is_rejected_before_replay() {
    let f = Fixture::new();
    let bytes = String::from_utf8(CURRENT.to_vec()).unwrap().replace(
        "stage.jit.captured(inputs, variables)",
        "stage.jit.captured([], variables)",
    );
    assert_ne!(bytes.as_bytes(), CURRENT);
    fs::write(f.root.path().join(NAME), bytes).unwrap();
    assert!(attest(&f.context(), NAME, HISTORICAL, &f.pin()).is_err());
}

#[test]
fn only_exact_path_historical_pin_and_retired_current_catalog_are_admitted() {
    let f = Fixture::new();
    let pin = f.pin();
    for name in [
        "rust/tools/model_export/check_pipeline_reference.py",
        "rust/tools/../tools/check_pipeline_reference.py",
    ] {
        assert!(attest(&f.context(), name, HISTORICAL, &pin).is_err());
    }
    assert!(attest(&f.context(), NAME, &"0".repeat(64), &pin).is_err());
    let mut context = f.context();
    context.retired_current_catalog = false;
    assert!(attest(&context, NAME, HISTORICAL, &pin).is_err());
    context = f.context();
    context.checkout = "not-a-source-commit";
    assert!(attest(&context, NAME, HISTORICAL, &pin).is_err());
}

#[test]
fn foreign_manifest_source_and_collector_corpus_cannot_borrow_the_attestation() {
    let mut f = Fixture::new();
    let pin = f.pin();
    f.original.manifest_sha256 = "0".repeat(64);
    assert!(attest(&f.context(), NAME, HISTORICAL, &pin).is_err());
    f.original.manifest_sha256 = MANIFEST.into();
    f.original.manifest["source_commit"] = json!(SOURCE);
    assert!(attest(&f.context(), NAME, HISTORICAL, &pin).is_err());
    f.original.manifest["source_commit"] = json!("481ae6fec39ecc68786c4c2813b1205fb4567141");
    f.original.collector_enabled = true;
    assert!(attest(&f.context(), NAME, HISTORICAL, &pin).is_err());
}

#[test]
fn changed_or_symlinked_original_manifest_is_rejected() {
    let f = Fixture::new();
    let path = f.original.root.join("reference-manifest.json");
    fs::write(&path, b"{}").unwrap();
    assert!(attest(&f.context(), NAME, HISTORICAL, &f.pin()).is_err());
    fs::remove_file(&path).unwrap();
    let target = f.root.path().join("manifest-target.json");
    fs::write(&target, ORIGINAL_MANIFEST).unwrap();
    symlink(target, &path).unwrap();
    assert!(attest(&f.context(), NAME, HISTORICAL, &f.pin()).is_err());
}

#[test]
fn stale_helper_pin_and_symlink_parent_are_rejected() {
    let f = Fixture::new();
    let path = f.root.path().join(NAME);
    let held = f.pin();
    fs::write(&path, b"changed").unwrap();
    assert!(attest(&f.context(), NAME, HISTORICAL, &held).is_err());
    let replacement = f.root.path().join("replacement-helper.txt");
    fs::write(&replacement, CURRENT).unwrap();
    fs::rename(replacement, &path).unwrap();
    assert!(attest(&f.context(), NAME, HISTORICAL, &held).is_err());
    let real_tools = f.root.path().join("real-tools");
    fs::rename(f.root.path().join("rust/tools"), &real_tools).unwrap();
    symlink(real_tools, f.root.path().join("rust/tools")).unwrap();
    assert!(attest(&f.context(), NAME, HISTORICAL, &f.pin()).is_err());
}

#[test]
fn accepted_dependency_retains_helper_and_manifest_identities_until_finalization() {
    let f = Fixture::new();
    let attestation = attest(&f.context(), NAME, HISTORICAL, &f.pin()).unwrap();
    attestation.verify().unwrap();
    let path = f.original.root.join("reference-manifest.json");
    let replacement = f.original.root.join("replacement.json");
    fs::write(&replacement, ORIGINAL_MANIFEST).unwrap();
    fs::rename(replacement, path).unwrap();
    assert!(attestation.verify().is_err());

    let f = Fixture::new();
    let attestation = attest(&f.context(), NAME, HISTORICAL, &f.pin()).unwrap();
    fs::write(f.root.path().join(NAME), b"changed after admission").unwrap();
    assert!(attestation.verify().is_err());
}

#[test]
fn accepted_dependency_rejects_leaf_alias_and_original_root_alias() {
    let f = Fixture::new();
    let held = f.pin();
    let path = f.root.path().join(NAME);
    let target = f.root.path().join("helper-target.txt");
    fs::rename(&path, &target).unwrap();
    symlink(target, &path).unwrap();
    assert!(attest(&f.context(), NAME, HISTORICAL, &held).is_err());

    let f = Fixture::new();
    let attestation = attest(&f.context(), NAME, HISTORICAL, &f.pin()).unwrap();
    let target = f.root.path().join("original-target");
    fs::rename(&f.original.root, &target).unwrap();
    symlink(target, &f.original.root).unwrap();
    assert!(attestation.verify().is_err());
}
