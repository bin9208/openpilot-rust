//! Authenticated original observations and typed native-only controller contracts.
use super::Result;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Component, Path},
};
const PIN: &str = "db82220fc911ad6ee0d0b960a7c4f1bf7665c6e6d3404fc3c9135d80eb1495e9";
const DATA: &[u8] = include_bytes!("../../tests/fixtures/recovery-original/observations.json");
const SOURCE: &str = "73cb4466e9ee9a426911d4ba0c0040e68d988c5f";
#[derive(Deserialize)]
struct Member {
    bytes: u64,
    sha256: String,
    data_b64: String,
    origin_member: String,
    origin_member_sha256: String,
    json_pointer_projection: Option<String>,
    #[serde(default = "initial_capture")]
    capture: String,
    #[serde(default = "source_observation")]
    evidence_kind: String,
}
fn initial_capture() -> String {
    "initial".into()
}
fn source_observation() -> String {
    "source-observation".into()
}
#[derive(Deserialize)]
struct Manifest {
    version: u64,
    source_commit: String,
    artifact: u64,
    zip_sha256: String,
    complete: bool,
    files: BTreeMap<String, Member>,
    source_inputs: Vec<Value>,
    missing: Vec<String>,
    coverage: BTreeMap<String, String>,
    captures: Value,
    native_acceptance: bool,
    required_native_relationships: Vec<String>,
}
pub struct OriginalFixture {
    manifest: Manifest,
    files: BTreeMap<String, Vec<u8>>,
}
fn sha(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn safe(name: &str) -> bool {
    !name.is_empty()
        && !name
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
        && Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
impl OriginalFixture {
    /// Verify the entire immutable roster before exposing any observation.
    pub fn open() -> Result<Self> {
        Self::decode(DATA)
    }
    fn decode(data: &[u8]) -> Result<Self> {
        if data.len() > 8 * 1024 * 1024 || sha(data) != PIN {
            return Err("recovery original fixture pin changed".into());
        }
        let manifest: Manifest = serde_json::from_slice(data)?;
        if manifest.version != 1
            || manifest.source_commit != SOURCE
            || manifest.artifact != 12367729913
            || manifest.zip_sha256
                != "73dbb9fac31c64ad61e3e08c19ac965571834ed036c8222c1e2c936573c38eba"
            || !manifest.complete
            || manifest.native_acceptance
            || manifest.files.len() != 213
            || manifest.source_inputs.len() != 15
            || !manifest.missing.is_empty()
            || manifest.coverage.len() != 8
            || manifest.required_native_relationships.len() != 4
            || manifest.captures["actions"]["artifact"] != 12370668326u64
            || manifest.captures["actions"]["source_scenarios"] != 36
            || manifest.captures["actions"]["controller_exit"] != 0
            || manifest.captures["actions"]["native_owned_effects_accepted"] != false
        {
            return Err("recovery fixture provenance changed".into());
        }
        let mut files = BTreeMap::new();
        let mut total = 0usize;
        for (name, member) in &manifest.files {
            if !safe(name)
                || !safe(&member.origin_member)
                || member.origin_member_sha256.len() != 64
                || (!member.origin_member.starts_with("original/")
                    && !member.origin_member.contains("/source-evidence/")
                    && member.json_pointer_projection.is_none())
                || manifest.captures.get(&member.capture).is_none()
                || ![
                    "source-observation",
                    "controller-execution-receipt",
                    "native-only-controller-contract",
                ]
                .contains(&member.evidence_kind.as_str())
            {
                return Err("unsafe or non-source recovery observation".into());
            }
            let bytes = STANDARD.decode(&member.data_b64)?;
            total = total
                .checked_add(bytes.len())
                .ok_or("fixture size overflow")?;
            if total > 8 * 1024 * 1024
                || bytes.len() as u64 != member.bytes
                || sha(&bytes) != member.sha256
            {
                return Err("recovery observation size or hash changed".into());
            }
            let _: Value = serde_json::from_slice(&bytes)?;
            files.insert(name.clone(), bytes);
        }
        Ok(Self { manifest, files })
    }
    pub fn observation(&self, name: &str) -> Result<Value> {
        Ok(serde_json::from_slice(
            self.files
                .get(name)
                .ok_or("unsealed recovery original observation")?,
        )?)
    }
    /// Raw authenticated JSON preserves wire base64 and historical integer bytes.
    pub fn observation_bytes(&self, name: &str) -> Result<&[u8]> {
        Ok(self
            .files
            .get(name)
            .ok_or("unsealed recovery original observation")?)
    }
    /// Decode only explicitly wrapped opaque original bytes. These include
    /// shell argv, bounded PTY history and approved/automatic file effects.
    pub fn opaque_payload(&self, name: &str) -> Result<Vec<u8>> {
        let value = self.observation(name)?;
        if value["encoding"] != "base64" {
            return Err("original observation is not an opaque payload".into());
        }
        let bytes = STANDARD.decode(
            value["payload_b64"]
                .as_str()
                .ok_or("missing original payload bytes")?,
        )?;
        if value["payload_bytes"] != bytes.len() as u64 || value["payload_sha256"] != sha(&bytes) {
            return Err("original opaque payload seal changed".into());
        }
        Ok(bytes)
    }
    /// Typed coverage distinguishes source observations from live native-only
    /// lifecycle relationships; it is not a family-completion bypass.
    pub fn coverage(&self) -> &BTreeMap<String, String> {
        &self.manifest.coverage
    }
    /// Expose archive identity without importing mixed source/native pair files.
    pub fn provenance(&self, name: &str) -> Result<Value> {
        let member = self
            .manifest
            .files
            .get(name)
            .ok_or("unsealed recovery original observation")?;
        let capture = &self.manifest.captures[&member.capture];
        Ok(
            serde_json::json!({"artifact":capture["artifact"],"zip_sha256":capture["zip_sha256"],
            "source_commit":capture["source_commit"],"member":member.origin_member,"evidence_kind":member.evidence_kind,
            "member_sha256":member.origin_member_sha256,"projection":member.json_pointer_projection,
            "observation_sha256":member.sha256,"observation_bytes":member.bytes}),
        )
    }
    /// Require the caller's whole case roster, then return only sealed source
    /// rows. This does not authorize deleting a dynamic field or skipping a case.
    pub fn scenarios(
        &self,
        name: &str,
        expected_names: &[&str],
    ) -> Result<BTreeMap<String, Value>> {
        let value = self.observation(name)?;
        let rows = value
            .as_array()
            .ok_or("recovery scenario observation is not an array")?;
        let mut selected = BTreeMap::new();
        for row in rows {
            let scenario = row["scenario"]
                .as_str()
                .ok_or("recovery scenario name missing")?;
            if row.get("native").is_some()
                || row.get("native_requests").is_some()
                || selected.insert(scenario.to_owned(), row.clone()).is_some()
            {
                return Err("mixed or duplicate recovery scenario observation".into());
            }
        }
        let mut expected = expected_names.to_vec();
        expected.sort_unstable();
        if selected.keys().map(String::as_str).collect::<Vec<_>>() != expected {
            return Err("recovery source scenario roster changed".into());
        }
        Ok(selected)
    }
    pub fn original_input_sha256(&self, path: &str) -> Result<&str> {
        self.manifest
            .source_inputs
            .iter()
            .find(|row| row["path"] == path)
            .and_then(|row| row["sha256"].as_str())
            .ok_or_else(|| "unsealed original recovery input".into())
    }
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
    pub fn source_inputs(&self) -> &[Value] {
        &self.manifest.source_inputs
    }
    /// Data completeness only; live native contracts and acceptance remain separate.
    pub fn require_complete_family(&self) -> Result<()> {
        if self.manifest.complete && self.manifest.missing.is_empty() {
            Ok(())
        } else {
            Err("recovery controller evidence incomplete".into())
        }
    }
    pub fn required_native_relationships(&self) -> &[String] {
        &self.manifest.required_native_relationships
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn whole_roster_distinguishes_complete_data_from_native_acceptance() {
        let fixture = OriginalFixture::open().unwrap();
        assert_eq!(fixture.names().count(), 213);
        assert_eq!(fixture.coverage().len(), 8);
        assert!(!fixture
            .opaque_payload("terminal/source/shell-argv.log.json")
            .unwrap()
            .is_empty());
        assert!(fixture
            .opaque_payload("terminal/source/result.json")
            .is_err());
        fixture.require_complete_family().unwrap();
        assert!(!fixture.manifest.native_acceptance);
        assert_eq!(
            fixture
                .names()
                .filter(|n| n.starts_with("actions/") && n.ends_with("/comparison.json"))
                .count(),
            36
        );
        assert_eq!(
            fixture
                .provenance("actions/git_branches/source-evidence/reply.json")
                .unwrap()["artifact"],
            12370668326u64
        );
        assert_eq!(
            fixture
                .provenance("shutdown/source/observation.json")
                .unwrap()["evidence_kind"],
            "native-only-controller-contract"
        );
        assert!(fixture.observation("native/result.json").is_err());
        let upload = fixture
            .observation("projected-source/upload/comparisons.json")
            .unwrap();
        assert_eq!(upload.as_array().unwrap().len(), 16);
        for row in upload.as_array().unwrap() {
            assert!(row.get("native").is_none());
            assert!(row.get("native_requests").is_none());
            assert!(row["source"].is_object());
        }
        assert!(fixture
            .scenarios("projected-source/upload/comparisons.json", &["success"])
            .is_err());
        assert!(fixture
            .provenance("projected-source/upload/comparisons.json")
            .unwrap()["projection"]
            .is_string());
        assert_eq!(
            fixture
                .observation_bytes("projected-source/upload/comparisons.json")
                .unwrap()
                .len(),
            fixture
                .provenance("projected-source/upload/comparisons.json")
                .unwrap()["observation_bytes"]
                .as_u64()
                .unwrap() as usize
        );
    }
    #[test]
    fn rejects_changed_or_missing_data_and_unsafe_names() {
        let mut corrupt = DATA.to_vec();
        let index = corrupt.len() / 2;
        corrupt[index] ^= 1;
        assert!(OriginalFixture::decode(&corrupt).is_err());
        assert!(OriginalFixture::decode(&DATA[..DATA.len() - 1]).is_err());
        for name in ["../escape", "/escape", "a/./b", "a//b", ""] {
            assert!(!safe(name));
        }
    }
}
