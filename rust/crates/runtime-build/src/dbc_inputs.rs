//! Reuse unchanged original generated DBC data only for its exact source inputs.
//! The producer, raw DBC/templates, generated outputs and MIT notice remain bound.
//! No radar parsing, selection, tolerance or timing behavior changes here.
use crate::{digest_file, regular_file, source, Result, Source};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Component, Path, PathBuf},
    process::Command,
};

pub const PATH: &str = "rust/native-dbc-source-contract.json";
pub const ARTIFACT_CONTRACT: &str = "rust/bin/radarcan-dbc/source-contract.json";

const ROOTS: &[&str] = &[
    "opendbc_repo/LICENSE",
    "opendbc_repo/opendbc/__init__.py",
    "opendbc_repo/opendbc/dbc",
    "rust/tools/radarcan_prepare_assets.py",
];
const PREFIX: &str = "rust/bin/radarcan-dbc";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub schema: u32,
    pub kind: String,
    pub original_source_commit: String,
    pub roots: Vec<PathBuf>,
    pub entries: usize,
    pub entries_sha256: String,
    pub license_text: String,
    pub outputs: BTreeMap<PathBuf, String>,
}

fn hex(value: &str, size: usize) -> bool {
    value.len() == size
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|c| matches!(c, Component::Normal(name) if name != ".git"))
}

impl Contract {
    pub fn read(path: &Path) -> Result<Self> {
        let file = regular_file(path)?;
        if file.metadata()?.len() > 1024 * 1024 {
            return Err("RadarCAN DBC source contract exceeds 1 MiB".into());
        }
        let value: Self = serde_json::from_reader(file.take(1024 * 1024 + 1))?;
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<()> {
        let paths = |values: &[&str]| values.iter().map(PathBuf::from).collect::<BTreeSet<_>>();
        if self.schema != 1
            || self.kind != "openpilot-radarcan-dbc-source-v1"
            || !hex(&self.original_source_commit, 40)
            || self.entries == 0
            || self.entries > 100_000
            || !hex(&self.entries_sha256, 64)
            || self.roots.len() != ROOTS.len()
            || self.roots.iter().cloned().collect::<BTreeSet<_>>() != paths(ROOTS)
            || self.outputs.len() != 104
            || !self
                .outputs
                .contains_key(&Path::new(PREFIX).join("radarcan-assets.json"))
            || self.outputs.keys().any(|p| {
                !relative(p)
                    || p.parent() != Some(Path::new(PREFIX))
                    || (p.extension().is_none_or(|e| e != "dbc")
                        && p.file_name().is_none_or(|n| n != "radarcan-assets.json"))
            })
            || self.license_text.is_empty()
            || self.license_text.len() > 16384
            || self.outputs.values().any(|sha| !hex(sha, 64))
        {
            return Err("invalid or incomplete RadarCAN DBC source/output contract".into());
        }
        Ok(())
    }
    fn verify_entries(&self, root: &Path) -> Result<()> {
        self.validate()?;
        let output = Command::new("/usr/bin/git")
            .arg("-C")
            .arg(root)
            .args(["ls-tree", "-r", "-z", "HEAD", "--"])
            .args(&self.roots)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()?;
        if !output.status.success() || output.stdout.len() > 8 * 1024 * 1024 {
            return Err(
                "cannot inspect the bounded RadarCAN DBC generation source namespace".into(),
            );
        }
        let mut entries = BTreeMap::new();
        for row in output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|row| !row.is_empty())
        {
            let text = std::str::from_utf8(row)?;
            let (metadata, name) = text
                .split_once('\t')
                .ok_or("invalid RadarCAN DBC source Git entry")?;
            let path = Path::new(name);
            if !relative(path) || name.contains(['\n', '\r', '\t']) {
                return Err("unsafe RadarCAN DBC generation source path".into());
            }
            let fields: Vec<_> = metadata.split(' ').collect();
            if fields.len() != 3
                || !hex(fields[2], 40)
                || !matches!(
                    (fields[0], fields[1]),
                    ("100644" | "100755" | "120000", "blob") | ("160000", "commit")
                )
            {
                return Err("unsupported RadarCAN DBC generation source Git entry".into());
            }
            if entries.insert(name, row).is_some() {
                return Err("duplicate RadarCAN DBC generation input".into());
            }
        }
        let mut digest = Sha256::new();
        for row in entries.values() {
            digest.update(row);
            digest.update([0]);
        }
        if entries.len() != self.entries
            || format!("{:x}", digest.finalize()) != self.entries_sha256
        {
            return Err(
                "RadarCAN DBC generation inputs differ; prepare newly verified DBC data for this source"
                    .into(),
            );
        }
        Ok(())
    }
    pub fn verify_source(&self, root: &Path) -> Result<Source> {
        let root = root.canonicalize()?;
        let before = source::inspect(&root)?;
        self.verify_entries(&root)?;
        if std::fs::read_to_string(root.join("opendbc_repo/LICENSE"))? != self.license_text {
            return Err("RadarCAN DBC license differs from its source".into());
        }
        if source::inspect(&root)? != before {
            return Err("RadarCAN DBC source changed during contract verification".into());
        }
        Ok(before)
    }
}

/// Verify the selected files before expensive compilation or sealing. The
/// caller separately binds the entire provider Tree to trusted input receipts.
pub fn verify_mapped(
    source_root: &Path,
    provider: &Path,
    files: &BTreeMap<PathBuf, PathBuf>,
) -> Result<()> {
    let contract_path = source_root.join(PATH);
    let before = digest_file(&contract_path)?;
    let contract = Contract::read(&contract_path)?;
    contract.verify_source(source_root)?;
    let provider = provider.canonicalize()?;
    let mut expected = contract.outputs.clone();
    expected.insert(ARTIFACT_CONTRACT.into(), before.clone());
    for path in files.keys().filter(|p| p.starts_with(PREFIX)) {
        if !expected.contains_key(path) {
            return Err("runtime layout adds an unverified RadarCAN DBC file".into());
        }
    }
    for (path, sha) in expected {
        let input = files
            .get(&path)
            .ok_or("native runtime is missing a RadarCAN DBC provider contract/output mapping")?;
        if !relative(input) {
            return Err("unsafe RadarCAN DBC provider mapping".into());
        }
        let input = provider.join(input).canonicalize()?;
        if !input.starts_with(&provider) || digest_file(&input)? != sha {
            return Err(
                "selected RadarCAN DBC output differs from the source-bound provider".into(),
            );
        }
    }
    if digest_file(&contract_path)? != before {
        return Err("RadarCAN DBC source contract changed during mapping verification".into());
    }
    Ok(())
}

/// Recheck the actual materialized candidate at seal/boot, without requiring
/// live compiler/provider caches. Retained source and immutable outputs remain.
pub fn verify_runtime(source_root: &Path, artifacts_root: &Path) -> Result<()> {
    let contract_path = source_root.join(PATH);
    let before = digest_file(&contract_path)?;
    let contract = Contract::read(&contract_path)?;
    contract.verify_source(source_root)?;
    if digest_file(&artifacts_root.join(ARTIFACT_CONTRACT))? != before {
        return Err("runtime RadarCAN DBC source contract differs from its retained source".into());
    }
    let mut expected: BTreeSet<_> = contract.outputs.keys().cloned().collect();
    expected.insert(ARTIFACT_CONTRACT.into());
    let actual = std::fs::read_dir(artifacts_root.join(PREFIX))?
        .map(|entry| {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                return Err("runtime RadarCAN DBC directory contains a link or non-file".into());
            }
            Ok(Path::new(PREFIX).join(entry.file_name()))
        })
        .collect::<Result<BTreeSet<_>>>()?;
    if actual != expected {
        return Err("runtime RadarCAN DBC file roster differs from its source contract".into());
    }
    for (path, sha) in &contract.outputs {
        if digest_file(&artifacts_root.join(path))? != *sha {
            return Err(
                "runtime RadarCAN DBC output differs from its source-bound provider".into(),
            );
        }
    }
    if digest_file(&contract_path)? != before {
        return Err("RadarCAN DBC source contract changed during runtime verification".into());
    }
    Ok(())
}
