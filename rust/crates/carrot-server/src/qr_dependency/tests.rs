use super::{
    install,
    manifest::{target, LIBRARIES},
    Provider,
};
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::symlink};

#[test]
fn preserves_current_generation_when_staged_libraries_cannot_load() {
    // Given: an owned current generation and a target/hash-valid but unloadable candidate.
    let root = tempfile::tempdir().expect("owned fixture");
    let bundle = root.path().join("bundle");
    let active = root.path().join("active");
    let previous = active.join("generation-previous");
    fs::create_dir_all(&bundle).expect("bundle");
    fs::create_dir_all(&previous).expect("previous generation");
    fs::write(previous.join("marker"), b"previous bytes").expect("previous content");
    symlink("generation-previous", active.join("current")).expect("active pointer");
    let mut bytes = vec![0; 64];
    bytes[..6].copy_from_slice(b"\x7fELF\x02\x01");
    bytes[16..18].copy_from_slice(&3_u16.to_le_bytes());
    let machine: u16 = if cfg!(target_arch = "aarch64") {
        183
    } else {
        62
    };
    bytes[18..20].copy_from_slice(&machine.to_le_bytes());
    for name in LIBRARIES {
        fs::write(bundle.join(name), &bytes).expect("candidate file");
    }
    let files: std::collections::BTreeMap<_, _> = LIBRARIES
        .into_iter()
        .map(|name| (name, format!("{:x}", Sha256::digest(&bytes))))
        .collect();
    let manifest = serde_json::json!({"abi_version": 1, "target": target().expect("supported target"), "brotli_version": 16781312, "files": files});
    fs::write(bundle.join("manifest.json"), manifest.to_string()).expect("manifest");
    let provider = Provider::new(bundle, active.clone(), false);
    // When: the verified staged candidate reaches the actual native loader and fails.
    let result = install::install(&provider);
    // Then: the prior pointer/content survive and incomplete staging is removed.
    assert!(result.is_err());
    assert_eq!(
        fs::read_link(active.join("current")).expect("current"),
        std::path::Path::new("generation-previous")
    );
    assert_eq!(
        fs::read(previous.join("marker")).expect("previous bytes"),
        b"previous bytes"
    );
    assert_eq!(fs::read_dir(active).expect("active directory").count(), 2);
}
