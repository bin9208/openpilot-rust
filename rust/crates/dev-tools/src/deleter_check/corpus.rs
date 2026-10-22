//! Immutable independent source observations; no native daemon oracle.
use super::{require, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::Path,
};
pub const ORIGINAL_SHA256: &str =
    "b4e652323459d74d54acbd3cf05eb8ae0a1f543033686d039928f63c530f6a64";
pub const SOURCE_PINS: &[(&str, &str)] = &[
    (
        "rust/tools/check_deleter_reference.py",
        "60b2ca4e6d4e7260bb275f1aacad0108b029fecf1d7b4ed836dcfa5ed72f0a19",
    ),
    (
        "rust/tools/check_deleter_daemon.py",
        "ec9b90f63f80d53db05543cdd56556435030f5e84bf1032cad1022771dcc6ec5",
    ),
    (
        "rust/tools/deleter_fixtures.py",
        "73610dca2ad77b7fe95a3364b7adf86ed7d49069ed991f916dc6dd92022559fd",
    ),
    (
        "rust/tools/tests/test_deleter_observer.py",
        "32078173a58f11e81af9927fae8fb33c1f3d533a45b61f7d025ca773081ba87d",
    ),
    (
        "openpilot/system/loggerd/deleter.py",
        "99b0d67d04405b63037f84b9eeade4e14a10ff690eb43ec29ecd6c5c587abc2b",
    ),
    (
        "openpilot/system/loggerd/uploader.py",
        "8af80c2411a3303d9f3e9e2255dd0732177d9af280abab6e7f568c7576e3a02e",
    ),
    (
        "openpilot/system/loggerd/xattr_cache.py",
        "35b6460ce0d1ea99066d86d6e81ef10edcd989a2f0967f07a8bae3957720e631",
    ),
    (
        "openpilot/system/loggerd/config.py",
        "ec950971dbdad607ba19e1144a4ab2455d744560ed962eaa7363420beafb36e8",
    ),
];
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Corpus {
    pub version: u32,
    pub source_pins: BTreeMap<String, String>,
    pub reference: Vec<Scenario>,
    pub low_space_original: Value,
    pub cli_contract: Value,
    pub expected_native_outputs_used: bool,
    pub original_source_execution_completed: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    pub directories_hex: Vec<String>,
    pub steps: Vec<Vec<Value>>,
    pub rows: Vec<Row>,
    pub expected_origin: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub input: Value,
    pub original: Value,
    pub expected: Value,
    pub original_snapshot: Value,
    pub expected_snapshot: Value,
    pub intentional_correction: Option<String>,
}
pub fn bytes(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    require(bytes.len() as u64 <= limit, "deleter input exceeds bound")?;
    Ok(bytes)
}
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn sources(root: &Path) -> Result<()> {
    for &(name, pin) in SOURCE_PINS {
        require(
            hash(&bytes(&root.join(name), 8 << 20)?) == pin,
            &format!("deleter retained original source changed: {name}"),
        )?;
    }
    Ok(())
}
pub fn load(root: &Path, path: &Path, sha: &str) -> Result<Corpus> {
    let raw = bytes(path, 8 << 20)?;
    require(hash(&raw) == sha, "original deleter fixture differs")?;
    require(
        sha == ORIGINAL_SHA256,
        "unaccepted original deleter fixture SHA",
    )?;
    let corpus: Corpus = serde_json::from_slice(&raw)?;
    require(
        corpus.version == 1
            && !corpus.expected_native_outputs_used
            && corpus.original_source_execution_completed,
        "deleter source observation origin differs",
    )?;
    let pins = SOURCE_PINS
        .iter()
        .map(|(p, s)| (p.to_string(), s.to_string()))
        .collect::<BTreeMap<_, _>>();
    require(
        corpus.source_pins == pins,
        "deleter exact original source roster differs",
    )?;
    sources(root)?;
    let mut names = [
        "threshold-order",
        "locks-and-preserve",
        "late-bookmark",
        "removed-bookmark",
        "five-marked-segments",
        "invalid-marked-counts",
        "attribute-value",
        "symlink-and-io",
        "nan-idle",
    ]
    .map(str::to_owned)
    .to_vec();
    names.extend((0..19).map(|i| format!("numeric-name-{i}")));
    require(
        corpus.reference.len() == 28
            && corpus
                .reference
                .iter()
                .map(|s| s.name.as_str())
                .collect::<BTreeSet<_>>()
                == names.iter().map(String::as_str).collect(),
        "deleter complete scenario roster differs",
    )?;
    require(
        corpus.reference.iter().map(|s| s.rows.len()).sum::<usize>() == 78,
        "deleter complete observed row roster differs",
    )?;
    for case in &corpus.reference {
        require(
            case.directories_hex.len() <= 128 && case.steps.len() <= 256 && !case.rows.is_empty(),
            "deleter fixture bound",
        )?;
        let correction = ["late-bookmark", "removed-bookmark"].contains(&case.name.as_str());
        require(
            case.rows.iter().all(|r| {
                r.intentional_correction.as_deref()
                    == if correction {
                        Some("#298: refresh mutable preserve attributes")
                    } else {
                        None
                    }
            }),
            "deleter original/corrected row role differs",
        )?;
        if correction {
            require(
                case.rows.iter().any(|r| r.original != r.expected),
                "deleter inherited #298 behavior lost",
            )?;
        } else {
            require(
                case.rows.iter().all(|r| {
                    r.original == r.expected && r.original_snapshot == r.expected_snapshot
                }),
                "unapproved original deleter divergence",
            )?;
        }
    }
    require(
        corpus.low_space_original["native_execution"] == false
            && corpus.low_space_original["cycles"] == 3
            && corpus.low_space_original["source_waits"] == serde_json::json!([0.1, 0.1, 0.1]),
        "original low-space loop receipt differs",
    )?;
    require(
        corpus.cli_contract
            == serde_json::json!({"signals":[2,15],"signal_exit":0,"shutdown_seconds_below":0.3,"invalid_argv":[["--cycles","0"],["--cycles","-1"],["--unknown"]],"invalid_exit":1,"one_cycle_argv":["--cycles","1"],"one_cycle_exit":0}),
        "historical deleter CLI assertions differ",
    )?;
    Ok(corpus)
}
