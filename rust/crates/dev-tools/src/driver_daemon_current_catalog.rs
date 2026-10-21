//! A current-host catalog is separate from independently captured model data.
use super::{read_regular, real_parent_chain, sha, CatalogPin, InputPin};
use crate::seal::Result;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
const MAX_RECEIPT: u64 = 4_000_000;
fn source_roster(root: &Path) -> Result<BTreeSet<String>> {
    fn visit(root: &Path, path: &Path, names: &mut BTreeSet<String>) -> Result<()> {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            return Err("driver exporter source roster contains an unsealed symlink".into());
        }
        if metadata.is_dir() {
            if path.file_name().is_some_and(|name| name == "__pycache__") {
                return Ok(());
            }
            for entry in fs::read_dir(path)? {
                visit(root, &entry?.path(), names)?;
            }
        } else if metadata.is_file() {
            names.insert(
                path.strip_prefix(root)?
                    .to_str()
                    .ok_or("driver exporter path is not UTF-8")?
                    .to_owned(),
            );
            if names.len() > 5000 {
                return Err("driver exporter source roster exceeds bound".into());
            }
        } else {
            return Err("driver exporter source is a special file".into());
        }
        Ok(())
    }
    let mut names = BTreeSet::new();
    for name in [
        "tinygrad_repo",
        "openpilot/selfdrive/modeld",
        "openpilot/common",
        "openpilot/system/camerad",
        "rust/tools/model_export",
        "uv.lock",
    ] {
        visit(root, &root.join(name), &mut names)?;
    }
    Ok(names)
}
fn cpu_identity(text: &str) -> Result<Value> {
    let mut profiles = Vec::new();
    for section in text.split("\n\n") {
        let fields = section
            .lines()
            .filter_map(|line| line.split_once(':'))
            .map(|(k, v)| (k.trim(), v.trim()))
            .collect::<BTreeMap<_, _>>();
        if let (Some(model), Some(flags)) = (fields.get("model name"), fields.get("flags")) {
            let flags = flags.split_whitespace().collect::<BTreeSet<_>>();
            let profile = json!({"model_name":model,"flags":flags});
            if !profiles.contains(&profile) {
                profiles.push(profile);
            }
        }
    }
    if profiles.is_empty() {
        return Err("driver current CPU identity missing".into());
    }
    Ok(json!(profiles))
}
struct SourceSeal {
    path: PathBuf,
    identity: (u64, u64, u64, i64, i64, i64, i64),
    digest: String,
}
pub(super) struct CurrentCatalog {
    receipt: InputPin,
    value: Value,
    source_root: PathBuf,
    catalog_root: PathBuf,
    held: Vec<InputPin>,
    source_seals: Vec<SourceSeal>,
    catalog_names: BTreeSet<PathBuf>,
}
impl CurrentCatalog {
    pub(super) fn open(
        root: &Path,
        catalog: &CatalogPin,
        path: &Path,
        expected_sha: &str,
        models: &BTreeMap<String, String>,
        checkout: &str,
    ) -> Result<Self> {
        let receipt = InputPin::open(path, false)?;
        if receipt.digest != expected_sha {
            return Err("driver current catalog receipt hash changed".into());
        }
        let value: Value = serde_json::from_slice(&read_regular(path, MAX_RECEIPT)?)?;
        if value["version"] != 1
            || value["provider"] != "CPU:LLVM"
            || value["stored_jit_executed"] != false
            || value["source_commit"] != checkout
            || value["host"]["machine"] != std::env::consts::ARCH
        {
            return Err("driver current catalog provider/source policy changed".into());
        }
        let mut held = Vec::new();
        let mut source_seals = Vec::new();
        let recorded_models = value["models"]
            .as_object()
            .ok_or("driver current model roster missing")?;
        if recorded_models.len() != models.len()
            || !models
                .iter()
                .all(|(n, h)| recorded_models.get(n).is_some_and(|p| p["sha256"] == *h))
        {
            return Err(
                "driver current catalog model sources differ from independent original".into(),
            );
        }
        let sources = value["exporter_dependency_files"]
            .as_object()
            .ok_or("driver exporter dependencies missing")?;
        if sources.keys().cloned().collect::<BTreeSet<_>>() != source_roster(root)? {
            return Err("driver current exporter whole source roster changed".into());
        }
        if sources.is_empty() || sources.len() > 5000 {
            return Err("driver exporter dependency roster invalid".into());
        }
        for (name, seal) in sources {
            real_parent_chain(root, name)?;
            let pin = InputPin::open(&root.join(super::relative(name)?), false)?;
            if pin.digest != seal["sha256"]
                || pin.identity.2
                    != seal["bytes"]
                        .as_u64()
                        .ok_or("driver source length missing")?
            {
                return Err(format!("driver current exporter source changed: {name}").into());
            }
            source_seals.push(SourceSeal {
                path: pin.path.clone(),
                identity: pin.identity,
                digest: pin.digest.clone(),
            });
        }
        // The compiler implementation and lock are mandatory, not a user-selected
        // subset of otherwise valid source pins.
        for name in [
            "uv.lock",
            "rust/tools/model_export/native.py",
            "rust/tools/model_export/original.py",
            "rust/tools/model_export/compile.py",
            "tinygrad_repo/tinygrad/runtime/support/compiler_cpu.py",
            "tinygrad_repo/tinygrad/runtime/ops_cpu.py",
        ] {
            if !sources.contains_key(name) {
                return Err("driver current compiler dependency missing".into());
            }
        }
        let files = value["catalog_files"]
            .as_object()
            .ok_or("driver current catalog roster missing")?;
        let names = files.keys().map(PathBuf::from).collect::<BTreeSet<_>>();
        if names != catalog.names {
            return Err("driver current catalog whole roster changed".into());
        }
        for (name, seal) in files {
            let pin = InputPin::open(&catalog.root.join(super::relative(name)?), false)?;
            if pin.digest != seal["sha256"]
                || pin.identity.2
                    != seal["bytes"]
                        .as_u64()
                        .ok_or("driver catalog length missing")?
            {
                return Err("driver current catalog whole-file seal changed".into());
            }
            held.push(pin);
        }
        let generation = value["catalog_generation"]
            .as_str()
            .ok_or("driver current catalog generation missing")?;
        if generation.len() != 64
            || !generation.bytes().all(|b| b.is_ascii_hexdigit())
            || sha(&read_regular(
                &catalog.root.join(generation).join("index.json"),
                MAX_RECEIPT,
            )?) != generation
        {
            return Err("driver current catalog generation changed".into());
        }
        let library = value["host"]["llvm_path"]
            .as_str()
            .ok_or("driver current LLVM path missing")?;
        let configured_library =
            std::env::var_os("LLVM_PATH").ok_or("driver current LLVM_PATH missing")?;
        if fs::canonicalize(configured_library)? != Path::new(library) {
            return Err(
                "driver current LLVM provider path differs from configured compiler".into(),
            );
        }
        let library = InputPin::open(Path::new(library), false)?;
        if library.digest != value["host"]["llvm_sha256"] {
            return Err("driver current LLVM library changed".into());
        }
        held.push(library);
        let result = Self {
            receipt,
            value,
            source_root: root.to_owned(),
            catalog_root: catalog.root.clone(),
            held,
            source_seals,
            catalog_names: names,
        };
        result.verify()?;
        Ok(result)
    }
    pub(super) fn verify(&self) -> Result<()> {
        self.receipt.verify()?;
        if self.value["exporter_dependency_files"]
            .as_object()
            .ok_or("driver source roster missing")?
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
            != source_roster(&self.source_root)?
        {
            return Err("driver current exporter source reappearance or removal".into());
        }
        let configured = std::env::var_os("LLVM_PATH").ok_or("driver current LLVM_PATH missing")?;
        if fs::canonicalize(configured)?
            != Path::new(
                self.value["host"]["llvm_path"]
                    .as_str()
                    .ok_or("driver LLVM path missing")?,
            )
        {
            return Err("driver current LLVM provider changed".into());
        }
        if self.value["host"]["proc_cpu"] != cpu_identity(&fs::read_to_string("/proc/cpuinfo")?)? {
            return Err("driver current catalog CPU provider differs from this host".into());
        }
        for input in &self.held {
            input.verify()?;
        }
        for source in &self.source_seals {
            let current = InputPin::open(&source.path, false)?;
            if current.identity != source.identity || current.digest != source.digest {
                return Err("driver current exporter source identity changed".into());
            }
        }
        if CatalogPin::open(&self.catalog_root)?.names != self.catalog_names {
            return Err("driver current catalog member reappearance".into());
        }
        for name in self.value["exporter_dependency_files"]
            .as_object()
            .ok_or("driver exporter dependency roster missing")?
            .keys()
        {
            real_parent_chain(&self.source_root, name)?;
        }
        Ok(())
    }
    pub(super) fn report(&self) -> Value {
        json!({"receipt":self.receipt.receipt(),"catalog_generation":self.value["catalog_generation"],"host":self.value["host"],"source_commit":self.value["source_commit"],"stored_jit_executed":false,"metadata_excludes":["CPU frequency","uptime","process IDs"]})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_identity_excludes_volatile_fields_but_keeps_model_and_isa() {
        let original = "model name: CPU A\nflags: sse avx sse\ncpu MHz: 3000\n\n";
        assert_eq!(
            cpu_identity(original).unwrap(),
            cpu_identity("model name: CPU A\nflags: avx sse\ncpu MHz: 1000\n\n").unwrap()
        );
        assert_ne!(
            cpu_identity(original).unwrap(),
            cpu_identity("model name: CPU A\nflags: sse\n\n").unwrap()
        );
        assert_ne!(
            cpu_identity(original).unwrap(),
            cpu_identity("model name: CPU B\nflags: avx sse\n\n").unwrap()
        );
        assert!(cpu_identity("cpu MHz: 1000").is_err());
    }
    #[test]
    fn exporter_source_roster_detects_unsealed_addition_and_removal() {
        let root = tempfile::tempdir().unwrap();
        for name in [
            "tinygrad_repo",
            "openpilot/selfdrive/modeld",
            "openpilot/common",
            "openpilot/system/camerad",
            "rust/tools/model_export",
        ] {
            fs::create_dir_all(root.path().join(name)).unwrap();
        }
        fs::write(root.path().join("uv.lock"), b"locked").unwrap();
        let first = source_roster(root.path()).unwrap();
        let module = root.path().join("rust/tools/model_export/new.py");
        fs::write(&module, b"new exporter").unwrap();
        assert_ne!(source_roster(root.path()).unwrap(), first);
        fs::remove_file(module).unwrap();
        assert_eq!(source_roster(root.path()).unwrap(), first);
        let linked = root.path().join("rust/tools/model_export/linked.py");
        std::os::unix::fs::symlink(root.path().join("uv.lock"), &linked).unwrap();
        assert!(source_roster(root.path()).is_err());
        fs::remove_file(linked).unwrap();
        let linked = root.path().join("rust/tools/model_export/linked-directory");
        std::os::unix::fs::symlink(root.path().join("openpilot/common"), &linked).unwrap();
        assert!(source_roster(root.path()).is_err());
        fs::remove_file(linked).unwrap();
        fs::remove_file(root.path().join("uv.lock")).unwrap();
        assert!(source_roster(root.path()).is_err());
    }
}
