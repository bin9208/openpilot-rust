//! Exact reviewed original-helper provenance pairs; never an exporter or source skip.
use super::{real_parent_chain, InputPin, OriginalCapture};
use crate::seal::Result;
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

pub(super) const HELPER: &str = "rust/tools/check_pipeline_reference.py";

pub(super) struct Context<'a> {
    pub root: &'a Path,
    pub original: &'a OriginalCapture,
    pub checkout: &'a str,
    // Derived only after run() authenticates CurrentCatalog under retired-driver.
    pub retired_current_catalog: bool,
}

pub(super) struct Attestation {
    root: PathBuf,
    original_root: PathBuf,
    original_identity: (u64, u64),
    current: InputPin,
    manifest: InputPin,
    value: Value,
}
impl Attestation {
    pub(super) fn verify(&self) -> Result<()> {
        real_parent_chain(&self.root, HELPER)?;
        let root = fs::symlink_metadata(&self.original_root)?;
        if !root.is_dir()
            || root.file_type().is_symlink()
            || (root.dev(), root.ino()) != self.original_identity
        {
            return Err("driver dependency original root changed".into());
        }
        self.current.verify()?;
        self.manifest.verify()
    }
    pub(super) fn receipt(&self) -> Value {
        self.value.clone()
    }
}

pub(super) fn attest(
    context: &Context<'_>,
    name: &str,
    historical: &str,
    pin: &InputPin,
) -> Result<Attestation> {
    let review: Value =
        serde_json::from_str(include_str!("driver_pipeline_dependency_attestation.json"))?;
    let pair = [&review, &review["collector_pair"]]
        .into_iter()
        .find(|pair| {
            pair["collector_enabled"] == context.original.collector_enabled
                && pair["original_manifest_sha256"] == context.original.manifest_sha256
                && pair["original_source_commit"] == context.original.manifest["source_commit"]
        })
        .ok_or("driver original-helper dependency is outside the reviewed pair")?;
    if !context.retired_current_catalog
        || name != HELPER
        || review["historical_helper_sha256"] != historical
        || review["current_helper_sha256"] != pin.digest
        || pin.path != context.root.join(name)
        || context.checkout.len() != 40
        || !context
            .checkout
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || pair["original_source_commit"] == context.checkout
    {
        return Err("driver original-helper dependency is outside the reviewed pair".into());
    }
    real_parent_chain(context.root, name)?;
    pin.verify()?;
    let current = InputPin::open(&pin.path, false)?;
    let manifest = InputPin::open(
        &context.original.root.join("reference-manifest.json"),
        false,
    )?;
    if current.identity != pin.identity
        || current.digest != pin.digest
        || pair["original_manifest_sha256"] != manifest.digest
    {
        return Err("driver dependency helper or immutable manifest changed".into());
    }
    let value = json!({
        "review":review, "current_checkout_commit":context.checkout,
        "original_pair":{
            "collector_enabled":pair["collector_enabled"],
            "original_source_commit":pair["original_source_commit"],
            "original_manifest_sha256":pair["original_manifest_sha256"]
        },
        "current_helper":current.receipt(), "original_manifest":manifest.receipt(),
        "current_catalog_authenticated":true, "full_acceptance":false
    });
    let attestation = Attestation {
        root: context.root.into(),
        original_root: context.original.root.clone(),
        original_identity: context.original.root_identity,
        current,
        manifest,
        value,
    };
    attestation.verify()?;
    Ok(attestation)
}

#[cfg(test)]
#[path = "driver_pipeline_dependency_tests.rs"]
mod tests;
