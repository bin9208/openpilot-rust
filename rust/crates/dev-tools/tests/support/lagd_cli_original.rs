//! Capture-only CLI loader; numerical comparison belongs to lagd_expected.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
pub fn cli() -> Value {
    let manifest = include_bytes!("../fixtures/lagd-tools/boundary-manifest.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(manifest)),
        "56ff47d710bccb32fb0f32f80b66a9fd448ae523fdd90f3df6fea17945bd65f4"
    );
    let manifest: Value = serde_json::from_slice(manifest).unwrap();
    let expected = &manifest["files"]["cli-original.json.gz"];
    let raw = include_bytes!("../fixtures/lagd-tools/cli-original.json.gz");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        expected["sha256"].as_str().unwrap()
    );
    let mut decoded = Vec::new();
    flate2::read::GzDecoder::new(raw.as_slice())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&decoded)),
        expected["decoded_sha256"].as_str().unwrap()
    );
    serde_json::from_slice(&decoded).unwrap()
}
