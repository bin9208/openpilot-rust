//! Admission of captured Python observations, never native daemon expectations.
use super::{require, Result};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::{Component, Path},
};
pub const SHA: &str = "30f9e1f21c47cd9bcefcc161456e1df9984338cc2a2e940eabb8dce0a97a6176";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Corpus {
    pub version: u32,
    pub source_pins: BTreeMap<String, String>,
    pub origin: String,
    pub timeouts: Vec<Value>,
    pub fault: Vec<Value>,
    pub params: Vec<Value>,
}
pub fn bytes(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    require(bytes.len() as u64 <= limit, "sibling input exceeds bound")?;
    Ok(bytes)
}
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn load(root: &Path, path: &Path, sha: &str) -> Result<Corpus> {
    require(sha == SHA, "unaccepted sibling observation SHA")?;
    let data = bytes(path, 32 << 20)?;
    require(hash(&data) == sha, "sibling original observations changed")?;
    let corpus: Corpus = serde_json::from_slice(&data)?;
    require(
        corpus.version == 1 && corpus.source_pins.len() == 23 && corpus.origin.contains("original"),
        "sibling source provenance invalid",
    )?;
    for (name, pin) in &corpus.source_pins {
        require(
            Path::new(name)
                .components()
                .all(|p| matches!(p, Component::Normal(_))),
            "unsafe sibling source pin",
        )?;
        crate::uploader_source_admission::validate(root, name, pin)?;
    }
    let timeout_ids = corpus
        .timeouts
        .iter()
        .map(|row| (row["stage"].as_str(), row["mode"].as_str()))
        .collect::<BTreeSet<_>>();
    let mut required = BTreeSet::new();
    for stage in ["get-headers", "get-body", "put-headers", "put-body"] {
        for mode in ["late", "stall"] {
            required.insert((Some(stage), Some(mode)));
        }
    }
    required.insert((Some("get-body"), Some("progress")));
    required.insert((Some("put-body"), Some("progress")));
    require(
        corpus.timeouts.len() == 10
            && timeout_ids == required
            && corpus
                .timeouts
                .iter()
                .all(|v| v["implementation"] == "python"),
        "sibling timeout source roster invalid",
    )?;
    for (rows, ids, key, source_key) in [
        (
            &corpus.fault,
            &super::records::FAULT_IDS[..],
            "name",
            "original",
        ),
        (
            &corpus.params,
            &super::records::PARAM_IDS[..],
            "case",
            "source",
        ),
    ] {
        let actual = rows
            .iter()
            .map(|v| (v[key].as_str(), v["original_collector"].as_bool()))
            .collect::<BTreeSet<_>>();
        let expected = ids
            .iter()
            .flat_map(|id| [true, false].map(|collector| (Some(*id), Some(collector))))
            .collect::<BTreeSet<_>>();
        require(
            rows.len() == ids.len() * 2
                && actual == expected
                && rows
                    .iter()
                    .all(|v| v[source_key] == true && v["records"].is_array()),
            "sibling original record roster invalid",
        )?;
    }
    Ok(corpus)
}
