//! Source constraints for the versioned, project-generated C MPC provider.
//! A current Rust build may reuse it only when its recorded source namespace
//! and every provider output still match. Original provenance is never rewritten.
use crate::{digest_file, regular_file, source, Result, Source};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Component, Path, PathBuf},
    process::Command,
};

pub const PATH: &str = "rust/native-mpc-source-contract.json";
pub const ARTIFACT_CONTRACT: &str = "plannerd-acados/source-contract.json";

const ROOTS: &[&str] = &[
    "openpilot",
    "opendbc",
    "opendbc_repo",
    "msgq",
    "msgq_repo",
    "rednose",
    "rednose_repo",
    "tinygrad",
    "tinygrad_repo",
    "teleoprtc",
    "teleoprtc_repo",
    "panda",
    "third_party",
    "pyproject.toml",
    "uv.lock",
    "rust/tools/build_plannerd_acados.py",
    "rust/tools/plannerd_generate_source.py",
    "rust/tools/plannerd_policy_source.py",
    "rust/crates/plannerd/licenses",
];
const EXCLUDES: &[&str] = &[
    "openpilot/selfdrive/carrot/web/generated",
    "openpilot/selfdrive/carrot/web/js",
    "openpilot/selfdrive/carrot/web/src",
    "openpilot/selfdrive/carrot/web/tests",
];
const OUTPUTS: &[&str] = &[
    "libacados.so",
    "libblasfeo.so",
    "libhpipm.so",
    "libqpOASES_e.so",
    "libqpOASES_e.so.3.1",
    "libacados_ocp_solver_lat.so",
    "libacados_ocp_solver_long.so",
    "manifest.json",
    "provenance.json",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub schema: u32,
    pub kind: String,
    pub original_source_commit: String,
    pub roots: Vec<PathBuf>,
    pub excludes: Vec<PathBuf>,
    pub entries: usize,
    pub entries_sha256: String,
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
            return Err("MPC source contract exceeds 1 MiB".into());
        }
        let value: Self = serde_json::from_reader(file.take(1024 * 1024 + 1))?;
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<()> {
        let paths = |values: &[&str]| values.iter().map(PathBuf::from).collect::<BTreeSet<_>>();
        let outputs: BTreeSet<_> = OUTPUTS
            .iter()
            .map(|name| Path::new("plannerd-acados").join(name))
            .collect();
        if self.schema != 1
            || self.kind != "openpilot-mpc-source-v1"
            || !hex(&self.original_source_commit, 40)
            || self.entries == 0
            || self.entries > 100_000
            || !hex(&self.entries_sha256, 64)
            || self.roots.len() != ROOTS.len()
            || self.roots.iter().cloned().collect::<BTreeSet<_>>() != paths(ROOTS)
            || self.excludes.len() != EXCLUDES.len()
            || self.excludes.iter().cloned().collect::<BTreeSet<_>>() != paths(EXCLUDES)
            || self.outputs.keys().cloned().collect::<BTreeSet<_>>() != outputs
            || self.outputs.values().any(|sha| !hex(sha, 64))
        {
            return Err("invalid or incomplete MPC source/output contract".into());
        }
        Ok(())
    }
    fn selected(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| path.starts_with(root))
            && (!self.excludes.iter().any(|root| path.starts_with(root))
                || path.extension().is_some_and(|extension| {
                    ["py", "pyi", "capnp"].iter().any(|e| extension == *e)
                }))
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
            return Err("cannot inspect the bounded MPC generation source namespace".into());
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
                .ok_or("invalid MPC source Git entry")?;
            let path = Path::new(name);
            if !relative(path) || name.contains(['\n', '\r', '\t']) {
                return Err("unsafe MPC generation source path".into());
            }
            let fields: Vec<_> = metadata.split(' ').collect();
            if fields.len() != 3
                || !hex(fields[2], 40)
                || !matches!(
                    (fields[0], fields[1]),
                    ("100644" | "100755" | "120000", "blob") | ("160000", "commit")
                )
            {
                return Err("unsupported MPC generation source Git entry".into());
            }
            if self.selected(path) && entries.insert(name, row).is_some() {
                return Err("duplicate MPC generation input".into());
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
                "MPC generation inputs differ; prepare a newly verified solver for this source"
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
            return Err("MPC source changed during contract verification".into());
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
    for (path, sha) in expected {
        let target = Path::new("rust/bin").join(path);
        let input = files
            .get(&target)
            .ok_or("native runtime is missing an MPC provider contract/output mapping")?;
        if !relative(input) {
            return Err("unsafe MPC provider mapping".into());
        }
        let input = provider.join(input).canonicalize()?;
        if !input.starts_with(&provider) || digest_file(&input)? != sha {
            return Err("selected MPC output differs from the source-bound provider".into());
        }
    }
    if digest_file(&contract_path)? != before {
        return Err("MPC source contract changed during mapping verification".into());
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
        return Err("runtime MPC source contract differs from its retained source".into());
    }
    for (path, sha) in &contract.outputs {
        if digest_file(&artifacts_root.join(path))? != *sha {
            return Err("runtime MPC output differs from its source-bound provider".into());
        }
    }
    if digest_file(&contract_path)? != before {
        return Err("MPC source contract changed during runtime verification".into());
    }
    Ok(())
}
