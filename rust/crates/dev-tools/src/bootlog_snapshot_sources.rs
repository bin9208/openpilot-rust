//! Shared source provenance for snapshot checks and independent reference.
use crate::{path_resolve::resolve_non_strict, seal::Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
const CORPUS: &[u8] = include_bytes!("../tests/fixtures/bootlog-snapshot/original.json");
const CORPUS_SHA: &str = "af18f39ad3e83c9c823a4beca7917a32c69452a172dfc465e91b6b91d50a6743";
const RETIRED: [&str; 2] = [
    "rust/tools/check_bootlog_snapshot.py",
    "rust/tools/bootlog_snapshot_source.py",
];
pub fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
pub fn root() -> Result<PathBuf> {
    resolve_non_strict(
        &std::env::var_os("OPENPILOT_RUST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")),
    )
}
pub fn sources(root: &Path) -> Result<Value> {
    require(
        sha(CORPUS) == CORPUS_SHA,
        "bootlog original capture changed",
    )?;
    let value: Value = serde_json::from_slice(CORPUS)?;
    for (name, pin) in value["provenance"]["sources"]
        .as_object()
        .ok_or("missing snapshot source pins")?
    {
        let bytes = match fs::read(root.join(name)) {
            Err(e)
                if RETIRED.contains(&name.as_str()) && e.kind() == std::io::ErrorKind::NotFound =>
            {
                continue
            }
            result => result?,
        };
        require(
            pin.as_str() == Some(&sha(&bytes)),
            format!("{name} changed; independently recapture snapshot source"),
        )?;
    }
    Ok(value)
}
