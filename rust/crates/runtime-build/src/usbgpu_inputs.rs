//! Source/version-bound eGPU data. Large model bytes are cached independently
//! from generated shader/firmware companions. Original provenance stays intact.
use crate::{digest_file, regular_file, source, Result, Source};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Component, Path, PathBuf},
    process::Command,
};

pub const PATH: &str = "rust/native-usbgpu-source-contract.json";
pub const ARTIFACT_CONTRACT: &str = "rust/bin/licenses/usbgpu/source-contract.json";

pub const MODEL_SHA: &str = "e758b96df27858ea97122d18554930d04f9f8bda417417074edfb3a72b008d0b";
pub const MODEL_BYTES: u64 = 776_634_338;
pub const MODEL_PATH: &str = "rust/bin/usbgpu-assets/models/e758b96df27858ea97122d18554930d04f9f8bda417417074edfb3a72b008d0b/model.pkl";
const ROOTS: &[&str] = &[
    "rust/crates/usbgpu/assets/amd-metadata.json",
    "rust/crates/usbgpu/src/model.rs",
    "rust/crates/usbgpu/src/model_delivery/state.rs",
    "rust/tools/import_usbgpu_hcq.py",
    "rust/tools/usbgpu_catalog_package.py",
    "rust/tools/usbgpu_hcq_data.py",
    "rust/tools/usbgpu_hcq_dispatch.py",
    "rust/tools/usbgpu_worker_metadata.py",
];
const PREFIXES: &[&str] = &["rust/bin/usbgpu-assets", "rust/bin/licenses/usbgpu"];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub schema: u32,
    pub kind: String,
    pub original_source_commit: String,
    pub roots: Vec<PathBuf>,
    pub entries: usize,
    pub entries_sha256: String,
    pub model: crate::assets::Asset,
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
            return Err("eGPU data source contract exceeds 1 MiB".into());
        }
        let value: Self = serde_json::from_reader(file.take(1024 * 1024 + 1))?;
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<()> {
        let paths = |values: &[&str]| values.iter().map(PathBuf::from).collect::<BTreeSet<_>>();
        if self.schema != 1
            || self.kind != "openpilot-usbgpu-data-source-v1"
            || !hex(&self.original_source_commit, 40)
            || self.entries == 0
            || self.entries > 100_000
            || !hex(&self.entries_sha256, 64)
            || self.roots.len() != ROOTS.len()
            || self.roots.iter().cloned().collect::<BTreeSet<_>>() != paths(ROOTS)
            || self.outputs.len() != 48
            || self.model.sha256 != MODEL_SHA || self.model.bytes != MODEL_BYTES
            || self.model.url != "https://upload.shind0.synology.me/models/comma4-big-cinque-v3/big_driving_tinygrad.pkl"
            || self.outputs.get(Path::new(MODEL_PATH)).map(String::as_str) != Some(MODEL_SHA)
            || self.outputs.keys().any(|p| !relative(p) || !PREFIXES.iter().any(|prefix| p.starts_with(prefix)))
            || self.outputs.values().any(|sha| !hex(sha, 64))
        {
            return Err("invalid or incomplete eGPU data source/output contract".into());
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
            return Err("cannot inspect the bounded eGPU data generation source namespace".into());
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
                .ok_or("invalid eGPU data source Git entry")?;
            let path = Path::new(name);
            if !relative(path) || name.contains(['\n', '\r', '\t']) {
                return Err("unsafe eGPU data generation source path".into());
            }
            let fields: Vec<_> = metadata.split(' ').collect();
            if fields.len() != 3
                || !hex(fields[2], 40)
                || !matches!(
                    (fields[0], fields[1]),
                    ("100644" | "100755" | "120000", "blob") | ("160000", "commit")
                )
            {
                return Err("unsupported eGPU data generation source Git entry".into());
            }
            if entries.insert(name, row).is_some() {
                return Err("duplicate eGPU data generation input".into());
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
                "eGPU data generation inputs differ; prepare newly verified eGPU data for this source"
                    .into(),
            );
        }
        Ok(())
    }
    pub fn verify_source(&self, root: &Path) -> Result<Source> {
        let root = root.canonicalize()?;
        let before = source::inspect(&root)?;
        self.verify_entries(&root)?;
        if source::inspect(&root)? != before {
            return Err("eGPU data source changed during contract verification".into());
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
    for path in files
        .keys()
        .filter(|p| PREFIXES.iter().any(|prefix| p.starts_with(prefix)))
    {
        if !expected.contains_key(path) {
            return Err("runtime layout adds unverified eGPU data".into());
        }
    }
    for (path, sha) in expected {
        let input = files
            .get(&path)
            .ok_or("native runtime is missing a eGPU data provider contract/output mapping")?;
        if !relative(input) {
            return Err("unsafe eGPU data provider mapping".into());
        }
        let input = provider.join(input).canonicalize()?;
        if !input.starts_with(&provider) || digest_file(&input)? != sha {
            return Err("selected eGPU data output differs from the source-bound provider".into());
        }
    }
    if digest_file(&contract_path)? != before {
        return Err("eGPU data source contract changed during mapping verification".into());
    }
    Ok(())
}

/// Verify the independently cached shader/firmware/metadata component. The large
/// model has its own raw content cache and is admitted by the full layout gate.
pub fn verify_companions(source: &Path, root: &Path) -> Result<()> {
    verify_outputs(source, root, false)
}
/// Startup needs only retained source and actual sealed data, not input caches.
pub fn verify_runtime(source: &Path, root: &Path) -> Result<()> {
    verify_outputs(source, root, true)
}
fn verify_outputs(source_root: &Path, root: &Path, with_model: bool) -> Result<()> {
    let contract_path = source_root.join(PATH);
    let before = digest_file(&contract_path)?;
    let contract = Contract::read(&contract_path)?;
    contract.verify_source(source_root)?;
    let mut expected = contract.outputs.clone();
    if !with_model {
        expected.remove(Path::new(MODEL_PATH));
    }
    expected.insert(ARTIFACT_CONTRACT.into(), before.clone());
    let mut directories = BTreeSet::new();
    for path in expected.keys() {
        for parent in path.ancestors().skip(1) {
            if PREFIXES.iter().any(|prefix| parent.starts_with(prefix)) {
                directories.insert(parent.to_owned());
            }
        }
    }
    let mut pending: Vec<_> = PREFIXES.iter().map(PathBuf::from).collect();
    let mut actual = BTreeSet::new();
    while let Some(directory) = pending.pop() {
        if !std::fs::symlink_metadata(root.join(&directory))?.is_dir() {
            return Err("eGPU input directory is redirected".into());
        }
        for entry in std::fs::read_dir(root.join(&directory))? {
            let entry = entry?;
            let relative = directory.join(entry.file_name());
            if entry.file_type()?.is_dir() && directories.contains(&relative) {
                pending.push(relative);
            } else if entry.file_type()?.is_file() && expected.contains_key(&relative) {
                actual.insert(relative);
            } else {
                return Err("eGPU data contains an unexpected file, link or directory".into());
            }
        }
    }
    if actual != expected.keys().cloned().collect() {
        return Err("eGPU data file roster differs from source contract".into());
    }
    for (path, sha) in expected {
        if path == Path::new(MODEL_PATH)
            && regular_file(&root.join(&path))?.metadata()?.len() != MODEL_BYTES
        {
            return Err("eGPU model length differs from source pin".into());
        }
        if digest_file(&root.join(path))? != sha {
            return Err("eGPU output differs from source-bound data".into());
        }
    }
    if digest_file(&contract_path)? != before {
        return Err("eGPU source contract changed during verification".into());
    }
    Ok(())
}
