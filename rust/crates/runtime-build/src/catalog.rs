//! Turn the tracked full-runtime roster and independently pinned native inputs
//! into the existing serial build plan. No provider is downloaded or certified
//! by taking a new snapshot here; receipts must already have trusted hashes.
use crate::{digest_file, prepare, regular_file, source, Recipe, Result, Source};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

pub const PATH: &str = "rust/native-runtime-catalog.json";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub schema: u32,
    pub target: String,
    pub roster_provenance: String,
    pub groups: BTreeMap<String, Group>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub packages: Vec<String>,
    pub features: Vec<String>,
    pub binaries: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InputSet {
    pub files: BTreeMap<PathBuf, String>,
    pub trees: BTreeMap<PathBuf, PathBuf>,
    pub environment: BTreeMap<String, String>,
}

/// Group assignments are explicit: model/data preparation belongs in runtime
/// inputs, while only the groups consuming a compiler provider bind that set.
/// A matching tree proves content, not the origin of its expected digest.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bindings {
    pub schema: u32,
    pub input_sets: BTreeMap<String, InputSet>,
    pub common: Vec<String>,
    pub groups: BTreeMap<String, Vec<String>>,
    pub group_growth_bytes: BTreeMap<String, u64>,
    pub expected_source_growth_bytes: u64,
    pub remaining_object_growth_bytes: u64,
    pub runtime_inputs: prepare::RuntimeInputs,
}

#[derive(Debug, Serialize)]
pub struct Audit {
    pub schema: u32,
    pub source: Source,
    pub catalog_sha256: String,
    pub metadata_sha256: String,
    pub metadata_cargo_sha256: String,
    pub bindings_sha256: Option<String>,
    pub plan_sha256: Option<String>,
    pub groups: usize,
    pub binaries: usize,
    pub target: String,
    pub compiled: bool,
    pub activated: bool,
}

pub fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let file = regular_file(path)?;
    if file.metadata()?.len() > 4 * 1024 * 1024 {
        return Err("native plan input exceeds 4 MiB".into());
    }
    Ok(serde_json::from_reader(file.take(4 * 1024 * 1024 + 1))?)
}

fn merge<K: Ord + Clone, V: PartialEq + Clone>(
    target: &mut BTreeMap<K, V>,
    input: &BTreeMap<K, V>,
) -> Result<()> {
    for (key, value) in input {
        if target.get(key).is_some_and(|old| old != value) {
            return Err("conflicting native input or environment bindings".into());
        }
        target.insert(key.clone(), value.clone());
    }
    Ok(())
}

fn explicit(set: &InputSet) -> Result<()> {
    if set.files.is_empty() && set.trees.is_empty() {
        return Err("native input sets must bind actual files or trees".into());
    }
    for path in set
        .files
        .keys()
        .chain(set.trees.keys())
        .chain(set.trees.values())
    {
        if !path.is_absolute()
            || path.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
        {
            return Err("native input sets require absolute paths without dot components".into());
        }
    }
    Ok(())
}

impl Catalog {
    fn recipes(&self) -> prepare::Plan {
        prepare::Plan {
            schema: self.schema,
            groups: self
                .groups
                .iter()
                .map(|(name, group)| {
                    (
                        name.clone(),
                        Recipe {
                            schema: 1,
                            target: self.target.clone(),
                            packages: group.packages.clone(),
                            binaries: group.binaries.clone(),
                            features: group.features.clone(),
                            inputs: BTreeMap::new(),
                            input_trees: BTreeMap::new(),
                            environment: BTreeMap::from([(
                                "OPENPILOT_NATIVE_INCREMENTAL".into(),
                                "1".into(),
                            )]),
                            expected_growth_bytes: 0,
                        },
                    )
                })
                .collect(),
            expected_source_growth_bytes: 0,
            remaining_object_growth_bytes: 0,
            runtime_inputs: prepare::RuntimeInputs::default(),
        }
    }

    /// Full generations must retain the complete roster from their own source,
    /// even if the installer bootstrap minimum alone would otherwise be met.
    pub fn verify_plan(&self, plan: &prepare::Plan) -> Result<()> {
        self.recipes().validate()?;
        plan.validate()?;
        crate::generation::require_tools(
            self.groups
                .values()
                .flat_map(|g| g.binaries.iter().map(String::as_str)),
        )?;
        if self.target != "aarch64-unknown-linux-gnu" || plan.groups.keys().ne(self.groups.keys()) {
            return Err(
                "native generation plan must cover its source catalog's complete group roster"
                    .into(),
            );
        }
        for (name, group) in &self.groups {
            let recipe = &plan.groups[name];
            if recipe.target != self.target
                || recipe.packages != group.packages
                || recipe.features != group.features
                || recipe.binaries != group.binaries
                || recipe
                    .environment
                    .get("OPENPILOT_NATIVE_INCREMENTAL")
                    .map(String::as_str)
                    != Some("1")
            {
                return Err(format!(
                    "native generation plan differs from its source catalog: {name}"
                )
                .into());
            }
        }
        Ok(())
    }

    /// Check names against the actual workspace metadata, including package
    /// ownership and local feature aliases required by selected binaries.
    pub fn verify(&self, metadata: &Value) -> Result<()> {
        if self.target != "aarch64-unknown-linux-gnu" || self.roster_provenance.is_empty() {
            return Err(
                "native runtime catalog requires its ARM target and roster provenance".into(),
            );
        }
        self.recipes().validate()?;
        crate::generation::require_tools(
            self.groups
                .values()
                .flat_map(|g| g.binaries.iter().map(String::as_str)),
        )?;
        let members: BTreeSet<_> = metadata["workspace_members"]
            .as_array()
            .ok_or("Cargo workspace members missing")?
            .iter()
            .filter_map(Value::as_str)
            .collect();
        let packages: BTreeMap<_, _> = metadata["packages"]
            .as_array()
            .ok_or("Cargo packages missing")?
            .iter()
            .filter(|p| p["id"].as_str().is_some_and(|id| members.contains(id)))
            .filter_map(|p| p["name"].as_str().map(|name| (name, p)))
            .collect();
        for group in self.groups.values() {
            let selected: BTreeSet<_> = group.packages.iter().map(String::as_str).collect();
            if selected.len() != group.packages.len()
                || selected.iter().any(|name| !packages.contains_key(name))
            {
                return Err(
                    "catalog package is absent from the actual workspace or duplicated".into(),
                );
            }
            let mut active: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
            let mut pending = Vec::new();
            for name in &selected {
                if packages[name]["features"].get("default").is_some() {
                    pending.push((*name, "default"));
                }
            }
            for feature in &group.features {
                let (package, feature) = feature
                    .split_once('/')
                    .ok_or("catalog features must name their selected package")?;
                if !selected.contains(package)
                    || packages[package]["features"].get(feature).is_none()
                {
                    return Err("catalog feature is absent from its selected package".into());
                }
                pending.push((package, feature));
            }
            while let Some((package, feature)) = pending.pop() {
                if !active.entry(package).or_default().insert(feature) {
                    continue;
                }
                if let Some(aliases) = packages[package]["features"][feature].as_array() {
                    for alias in aliases.iter().filter_map(Value::as_str) {
                        if packages[package]["features"].get(alias).is_some() {
                            pending.push((package, alias));
                        }
                    }
                }
            }
            for binary in &group.binaries {
                let owners: Vec<_> = selected
                    .iter()
                    .flat_map(|name| {
                        packages[name]["targets"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(move |target| {
                                target["name"].as_str() == Some(binary.as_str())
                                    && target["kind"]
                                        .as_array()
                                        .is_some_and(|k| k.iter().any(|v| v == "bin"))
                            })
                            .map(move |target| (*name, target))
                    })
                    .collect();
                if owners.len() != 1 {
                    return Err(format!(
                        "catalog binary has no unique selected workspace owner: {binary}"
                    )
                    .into());
                }
                let (package, target) = owners[0];
                for required in target["required-features"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    if !active
                        .get(package)
                        .is_some_and(|features| features.contains(required))
                    {
                        return Err(format!(
                            "catalog binary lacks required feature: {binary}: {required}"
                        )
                        .into());
                    }
                }
            }
        }
        Ok(())
    }

    pub fn compose(&self, bindings: &Bindings, root: &Path) -> Result<prepare::Plan> {
        if bindings.schema != 1
            || bindings.groups.keys().ne(self.groups.keys())
            || bindings.group_growth_bytes.keys().ne(self.groups.keys())
            || bindings.common.is_empty()
        {
            return Err(
                "native bindings must cover exactly the catalog groups and their growth estimates"
                    .into(),
            );
        }
        let mut used = BTreeSet::new();
        let mut plan = self.recipes();
        for (group, recipe) in &mut plan.groups {
            let mut selected = BTreeSet::new();
            for name in bindings.common.iter().chain(&bindings.groups[group]) {
                if !selected.insert(name) {
                    return Err("duplicate native input set assignment".into());
                }
                let set = bindings
                    .input_sets
                    .get(name)
                    .ok_or("unknown native input set assignment")?;
                if used.insert(name) {
                    explicit(set)?;
                    // Verify a shared toolchain/provider once during assembly;
                    // each actual build still performs its before/after checks.
                    let mut input = recipe.clone();
                    input.inputs = set.files.clone();
                    input.input_trees = set.trees.clone();
                    input.verify_inputs(root)?;
                }
                merge(&mut recipe.inputs, &set.files)?;
                merge(&mut recipe.input_trees, &set.trees)?;
                merge(&mut recipe.environment, &set.environment)?;
            }
            if recipe.inputs.is_empty() {
                return Err("every group requires pinned native compiler inputs".into());
            }
            recipe.expected_growth_bytes = bindings.group_growth_bytes[group];
        }
        if used.len() != bindings.input_sets.len() {
            return Err("unused native input set; group assignment may be incomplete".into());
        }
        plan.expected_source_growth_bytes = bindings.expected_source_growth_bytes;
        plan.remaining_object_growth_bytes = bindings.remaining_object_growth_bytes;
        plan.runtime_inputs = bindings.runtime_inputs.clone();
        plan.runtime_inputs.verify(root)?;
        plan.validate()?;
        Ok(plan)
    }
}

/// This is an actual Cargo metadata query, not a supplied list of pretend
/// targets. Metadata does not compile or prove the external provider closure.
pub fn audit(root: &Path, cargo: &Path) -> Result<(Catalog, Audit)> {
    let root = root.canonicalize()?;
    let before = source::inspect(&root)?;
    let catalog_path = root.join(PATH);
    let catalog_sha256 = digest_file(&catalog_path)?;
    let catalog: Catalog = read(&catalog_path)?;
    if !cargo.is_absolute() {
        return Err("metadata Cargo path must be absolute".into());
    }
    let cargo_sha256 = digest_file(cargo)?;
    let output = Command::new(cargo)
        .current_dir(&root)
        .args([
            "metadata",
            "--manifest-path",
            "rust/Cargo.toml",
            "--locked",
            "--offline",
            "--no-deps",
            "--format-version",
            "1",
        ])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "native catalog Cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    catalog.verify(&serde_json::from_slice(&output.stdout)?)?;
    if source::inspect(&root)? != before
        || digest_file(&catalog_path)? != catalog_sha256
        || digest_file(cargo)? != cargo_sha256
    {
        return Err("source, catalog or metadata Cargo changed during audit".into());
    }
    let audit = Audit {
        schema: 1,
        source: before,
        catalog_sha256,
        metadata_sha256: format!("{:x}", Sha256::digest(&output.stdout)),
        metadata_cargo_sha256: cargo_sha256,
        bindings_sha256: None,
        plan_sha256: None,
        groups: catalog.groups.len(),
        binaries: catalog.groups.values().map(|g| g.binaries.len()).sum(),
        target: catalog.target.clone(),
        compiled: false,
        activated: false,
    };
    Ok((catalog, audit))
}

pub fn assemble(root: &Path, cargo: &Path, bindings_path: &Path) -> Result<(prepare::Plan, Audit)> {
    let (catalog, mut audit) = audit(root, cargo)?;
    let bindings_sha256 = digest_file(bindings_path)?;
    let bindings: Bindings = read(bindings_path)?;
    let plan = catalog.compose(&bindings, root)?;
    if source::inspect(&root.canonicalize()?)? != audit.source
        || digest_file(&root.join(PATH))? != audit.catalog_sha256
        || digest_file(cargo)? != audit.metadata_cargo_sha256
        || digest_file(bindings_path)? != bindings_sha256
    {
        return Err("native plan assembly inputs changed during verification".into());
    }
    audit.bindings_sha256 = Some(bindings_sha256);
    let mut bytes = serde_json::to_vec_pretty(&plan)?;
    bytes.push(b'\n');
    audit.plan_sha256 = Some(format!("{:x}", Sha256::digest(&bytes)));
    Ok((plan, audit))
}

pub(crate) fn verify_source_plan(root: &Path, plan: &prepare::Plan) -> Result<()> {
    read::<Catalog>(&root.join(PATH))?.verify_plan(plan)
}

pub fn publish(output: &Path, plan: &prepare::Plan, audit: &Audit) -> Result<()> {
    let mut plan_bytes = serde_json::to_vec_pretty(plan)?;
    plan_bytes.push(b'\n');
    if plan_bytes.len() > 4 * 1024 * 1024 {
        return Err("native plan exceeds the existing plan reader's 4 MiB limit".into());
    }
    let expected_plan = format!("{:x}", Sha256::digest(&plan_bytes));
    if audit.compiled
        || audit.activated
        || audit.bindings_sha256.is_none()
        || audit.plan_sha256.as_deref() != Some(expected_plan.as_str())
    {
        return Err("native plan publication requires the matching assembly audit".into());
    }
    let parent = output
        .parent()
        .ok_or("native plan output needs a parent")?
        .canonicalize()?;
    let name = output
        .file_name()
        .ok_or("native plan output needs a directory name")?;
    let output = parent.join(name);
    let temporary = tempfile::Builder::new()
        .prefix(".native-plan-")
        .tempdir_in(&parent)?;
    for (name, bytes) in [
        ("build-plan.json", serde_json::to_vec_pretty(plan)?),
        ("catalog-audit.json", serde_json::to_vec_pretty(audit)?),
    ] {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temporary.path().join(name))?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    File::open(temporary.path())?.sync_all()?;
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        temporary.path(),
        rustix::fs::CWD,
        &output,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    File::open(parent)?.sync_all()?;
    Ok(())
}
