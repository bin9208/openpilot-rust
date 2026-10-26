//! Serial native build groups bound to retained source and immutable outputs.
//! A prepared build is engineering evidence; it is not a sealed/active runtime.
use crate::{build, objects, regular_file, source_store, Recipe, Report, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
    time::Instant,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: u32,
    pub groups: BTreeMap<String, Recipe>,
    pub expected_source_growth_bytes: u64,
    pub remaining_object_growth_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Prepared {
    pub schema: u32,
    pub plan: Plan,
    pub source: source_store::Snapshot,
    pub reports: BTreeMap<String, Report>,
    pub binaries: BTreeMap<String, objects::Object>,
    pub stored_output_bytes: u64,
    pub elapsed_ms: u128,
}

impl Plan {
    pub fn read(path: &Path) -> Result<Self> {
        let file = regular_file(path)?;
        if file.metadata()?.len() > 4 * 1024 * 1024 {
            return Err("native build plan exceeds 4 MiB".into());
        }
        let plan: Self = serde_json::from_reader(file.take(4 * 1024 * 1024 + 1))?;
        plan.validate()?;
        Ok(plan)
    }

    fn validate(&self) -> Result<()> {
        if self.schema != 1 || self.groups.is_empty() || self.groups.len() > 128 {
            return Err("invalid native build plan".into());
        }
        let mut binaries = BTreeSet::new();
        let mut target = None;
        for (name, recipe) in &self.groups {
            recipe.validate()?;
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
            {
                return Err("invalid native build group name".into());
            }
            if target.is_some_and(|value| value != &recipe.target) {
                return Err("native build groups must use one actual host target".into());
            }
            target = Some(&recipe.target);
            for binary in &recipe.binaries {
                if !binaries.insert(binary) {
                    return Err("native build plan assigns a binary to multiple groups".into());
                }
            }
        }
        Ok(())
    }
}

fn name(path: &Path) -> Result<&str> {
    path.file_name()
        .and_then(|p| p.to_str())
        .ok_or_else(|| "invalid Cargo binary path".into())
}

impl Prepared {
    /// Verify saved evidence after mutable build outputs or source are removed.
    /// AGNOS ABI/provider/generated completeness is a separate sealing gate.
    pub fn verify(&self, source_store: &Path, object_store: &Path) -> Result<()> {
        self.plan.validate()?;
        if self.schema != 1 || self.reports.is_empty() || self.reports.len() > 128 {
            return Err("invalid prepared native build".into());
        }
        self.source.verify(source_store)?;
        if self.reports.keys().ne(self.plan.groups.keys()) {
            return Err("prepared build groups do not match the recorded plan".into());
        }
        let first = self
            .reports
            .values()
            .next()
            .ok_or("prepared build has no groups")?;
        let mut binaries = BTreeSet::new();
        for (group, report) in &self.reports {
            let recipe = &self.plan.groups[group];
            let expected_recipe = format!("{:x}", Sha256::digest(serde_json::to_vec(recipe)?));
            if report.schema != 2
                || report.source != self.source.source
                || report.artifacts.is_empty()
                || report.recipe_sha256 != expected_recipe
                || report.rustc_sha256 != first.rustc_sha256
                || report.cargo_sha256 != first.cargo_sha256
                || report.native_tools != first.native_tools
            {
                return Err("mixed or incomplete source, recipe or native tool reports".into());
            }
            let actual: BTreeSet<_> = report
                .artifacts
                .iter()
                .map(|a| name(&a.executable))
                .collect::<Result<_>>()?;
            let expected: BTreeSet<_> = recipe.binaries.iter().map(String::as_str).collect();
            if actual != expected || actual.len() != report.artifacts.len() {
                return Err("prepared build report differs from its planned binary roster".into());
            }
            for artifact in &report.artifacts {
                let name = name(&artifact.executable)?;
                let object = self
                    .binaries
                    .get(name)
                    .ok_or("native build output missing from prepared objects")?;
                if !binaries.insert(name)
                    || object.sha256 != artifact.sha256
                    || object.bytes != artifact.bytes
                    || object.mode != 0o555
                {
                    return Err("prepared binary does not match its actual build report".into());
                }
                object.verify(object_store)?;
            }
        }
        if binaries.len() != self.binaries.len() {
            return Err("prepared build contains unreported binaries".into());
        }
        Ok(())
    }
}

/// Build from the fixed mutable input path so Cargo can reuse fingerprints across
/// commits. Retain each source separately for execution/recovery, then copy only
/// newly changed outputs. Failures publish no prepared result or active pointer.
pub fn run(
    root: &Path,
    cache: &Path,
    toolchain: &Path,
    source_store: &Path,
    object_store: &Path,
    plan: &Plan,
) -> Result<Prepared> {
    let started = Instant::now();
    plan.validate()?;
    let source = source_store::retain(root, source_store, plan.expected_source_growth_bytes)?;
    let mut reports = BTreeMap::new();
    let mut binaries = BTreeMap::new();
    let mut stored_output_bytes = 0_u64;
    for (group, recipe) in &plan.groups {
        let report = build(root, cache, toolchain, recipe)?;
        if report.source != source.source {
            return Err("native build source changed between groups".into());
        }
        let actual: BTreeSet<_> = report
            .artifacts
            .iter()
            .map(|a| name(&a.executable))
            .collect::<Result<_>>()?;
        let expected: BTreeSet<_> = recipe.binaries.iter().map(String::as_str).collect();
        if actual != expected || actual.len() != report.artifacts.len() {
            return Err("native build produced an unexpected binary roster".into());
        }
        for artifact in &report.artifacts {
            let imported = objects::import(
                object_store,
                &artifact.executable,
                plan.remaining_object_growth_bytes,
            )?;
            if imported.object.sha256 != artifact.sha256
                || imported.object.bytes != artifact.bytes
                || imported.object.mode != 0o555
            {
                return Err(
                    "native output changed after its build report; previous generation retained"
                        .into(),
                );
            }
            stored_output_bytes = stored_output_bytes
                .checked_add(imported.stored_bytes)
                .ok_or("native output accounting overflow")?;
            binaries.insert(name(&artifact.executable)?.to_owned(), imported.object);
        }
        reports.insert(group.clone(), report);
    }
    let prepared = Prepared {
        schema: 1,
        plan: plan.clone(),
        source,
        reports,
        binaries,
        stored_output_bytes,
        elapsed_ms: started.elapsed().as_millis(),
    };
    prepared.verify(source_store, object_store)?;
    Ok(prepared)
}
