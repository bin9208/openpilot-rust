//! A current-host catalog is separate from independently captured model data.
use super::{read_regular, real_parent_chain, sha, CatalogPin, InputPin};
use crate::seal::Result;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
const MAX_RECEIPT: u64 = 4_000_000;
const CACHED_SOURCES: [&str; 9] = [
    "rust/crates/model-runtime/src/cached_code.rs",
    "rust/crates/model-runtime/src/bin/offline-cached-code.rs",
    "rust/crates/model-runtime/src/cached_elf.rs",
    "rust/crates/model-runtime/src/cpu.rs",
    "rust/crates/model-runtime/src/graph.rs",
    "rust/crates/model-runtime/src/error.rs",
    "rust/crates/model-runtime/src/entrypoint.rs",
    "rust/crates/model-runtime/src/assets.rs",
    "rust/crates/model-runtime/src/buffer.rs",
];
fn source_roster(root: &Path, guarded_add: bool, cached: bool) -> Result<BTreeSet<String>> {
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
    if guarded_add {
        for name in [
            "rust/crates/model-runtime/src/guarded_add.rs",
            "rust/crates/model-runtime/src/bin/offline-llvm-guarded-add.rs",
            "rust/crates/model-runtime/src/owned_outputs.rs",
            "rust/crates/model-runtime/src/lib.rs",
            "rust/crates/model-runtime/Cargo.toml",
            "rust/Cargo.toml",
            "rust/Cargo.lock",
            "rust/rust-toolchain.toml",
        ] {
            visit(root, &root.join(name), &mut names)?;
        }
    }
    if cached {
        for name in CACHED_SOURCES {
            visit(root, &root.join(name), &mut names)?;
        }
    }
    Ok(names)
}
fn helper_matches(binding: &Value) -> Result<()> {
    let path = std::env::var_os("MODEL_LLVM_GUARDED_ADD_BINARY");
    let hash = std::env::var_os("MODEL_LLVM_GUARDED_ADD_SHA256");
    if binding.is_null() {
        if path.is_some() || hash.is_some() {
            return Err("unrecorded guarded add helper configured".into());
        }
        return Ok(());
    }
    if binding["enabled"] != true
        || path.as_deref().and_then(|p| p.to_str()) != binding["path"].as_str()
        || hash.as_deref().and_then(|p| p.to_str()) != binding["sha256"].as_str()
    {
        return Err("guarded add helper configuration differs from catalog".into());
    }
    Ok(())
}
fn cached_canonical_sha(value: &Value) -> Result<String> {
    // Cached binding fields use the same recursive key ordering as Python's
    // _canonical; serde_json/preserve_order can be enabled by another feature.
    // Sort a clone only. Do not rewrite receipt values or legacy receipt hashes.
    let mut canonical = value.clone();
    canonical.sort_all_objects();
    Ok(sha(&serde_json::to_vec(&canonical)?))
}

fn cached_matches(binding: &Value) -> Result<()> {
    let names = [
        "MODEL_CACHED_CODE_BINARY",
        "MODEL_CACHED_CODE_SHA256",
        "MODEL_CACHED_CODE_REQUEST",
        "MODEL_CACHED_CODE_REQUEST_SHA256",
    ];
    let configured = names.map(std::env::var_os);
    if binding.is_null() {
        if configured.iter().any(Option::is_some) {
            return Err("unrecorded cached issuer configured".into());
        }
        return Ok(());
    }
    if binding["enabled"] != true
        || binding["complete_preflight"] != true
        || binding["generated_code_executed"] != false
        || configured[0].as_deref().and_then(|v| v.to_str()) != binding["helper"]["path"].as_str()
        || configured[1].as_deref().and_then(|v| v.to_str()) != binding["helper"]["sha256"].as_str()
        || configured[2].as_deref().and_then(|v| v.to_str()) != binding["request"]["path"].as_str()
        || configured[3].as_deref().and_then(|v| v.to_str())
            != binding["request"]["sha256"].as_str()
    {
        return Err("cached original/current native issuer configuration differs".into());
    }
    Ok(())
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
    /// Only exporter implementation files may use the separately authenticated
    /// current compilation closure instead of a frozen oracle source digest.
    pub(super) fn exporter_source(&self, name: &str) -> Result<Option<InputPin>> {
        if !name.starts_with("rust/tools/model_export/") || !name.ends_with(".py") {
            return Ok(None);
        }
        self.receipt.verify()?;
        let path = self.source_root.join(super::relative(name)?);
        real_parent_chain(&self.source_root, name)?;
        let seal = self
            .source_seals
            .iter()
            .find(|seal| seal.path == path)
            .ok_or("driver exporter source missing from authenticated current closure")?;
        let pin = InputPin::open(&path, false)?;
        if pin.identity != seal.identity || pin.digest != seal.digest {
            return Err("driver current exporter source binding changed".into());
        }
        Ok(Some(pin))
    }
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
        helper_matches(&value["guarded_add"])?;
        cached_matches(&value["cached_code"])?;
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
        if sources.keys().cloned().collect::<BTreeSet<_>>()
            != source_roster(
                root,
                !value["guarded_add"].is_null(),
                !value["cached_code"].is_null(),
            )?
        {
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
        if !value["guarded_add"].is_null() {
            let binding = &value["guarded_add"];
            let path = Path::new(
                binding["path"]
                    .as_str()
                    .ok_or("guarded add helper path missing")?,
            );
            if !path.is_absolute() || fs::canonicalize(path)? != path {
                return Err("guarded add helper path is not canonical".into());
            }
            let helper = InputPin::open(path, true)?;
            if helper.digest != binding["sha256"]
                || Some(helper.identity.2) != binding["bytes"].as_u64()
            {
                return Err("guarded add helper seal differs from compiler binding".into());
            }
            let m = helper.file.metadata()?;
            let mut header = [0_u8; 64];
            if !(64..=128 * 1024 * 1024).contains(&m.len())
                || m.mode() & 0o111 == 0
                || std::os::unix::fs::FileExt::read_at(&helper.file, &mut header, 0)?
                    != header.len()
                || &header[..7] != b"\x7fELF\x02\x01\x01"
                || !matches!(u16::from_le_bytes([header[16], header[17]]), 2 | 3)
                || u16::from_le_bytes([header[18], header[19]])
                    != if cfg!(target_arch = "aarch64") {
                        183
                    } else {
                        62
                    }
            {
                return Err("guarded add helper is not a bounded executable ELF".into());
            }
            if binding["identity"]
                != json!([
                    m.dev(),
                    m.ino(),
                    m.len(),
                    u64::from(m.mode()),
                    i128::from(m.mtime()) * 1_000_000_000 + i128::from(m.mtime_nsec()),
                    i128::from(m.ctime()) * 1_000_000_000 + i128::from(m.ctime_nsec())
                ])
            {
                return Err("guarded add helper metadata differs from compiler binding".into());
            }
            held.push(helper);
        }
        if !value["cached_code"].is_null() {
            let binding = &value["cached_code"];
            for (field, elf) in [("helper", true), ("request", false)] {
                let path = Path::new(binding[field]["path"].as_str().ok_or("cached input path")?);
                if !path.is_absolute() || fs::canonicalize(path)? != path {
                    return Err("cached input alias".into());
                }
                let input = InputPin::open(path, elf)?;
                if input.digest != binding[field]["sha256"]
                    || Some(input.identity.2) != binding[field]["bytes"].as_u64()
                {
                    return Err("cached original/current native input changed".into());
                }
                if elf {
                    let m = input.file.metadata()?;
                    let identity = json!([
                        m.dev(),
                        m.ino(),
                        m.len(),
                        u64::from(m.mode()),
                        i128::from(m.mtime()) * 1_000_000_000 + i128::from(m.mtime_nsec()),
                        i128::from(m.ctime()) * 1_000_000_000 + i128::from(m.ctime_nsec())
                    ]);
                    if m.mode() & 0o111 == 0
                        || m.len() > 128 * 1024 * 1024
                        || identity != binding[field]["identity"]
                    {
                        return Err("cached native issuer identity differs".into());
                    }
                } else {
                    let request: Value = serde_json::from_slice(&read_regular(path, MAX_RECEIPT)?)?;
                    let current = &request["admission"]["current"];
                    if current["exporter_roster_sha256"]
                        != cached_canonical_sha(&value["exporter_dependency_files"])?
                        || current["cached_helper_sha256"] != binding["helper"]["sha256"]
                        || current["cached_helper_binding_sha256"]
                            != cached_canonical_sha(&binding["helper"])?
                        || current["helper_binding_sha256"]
                            != cached_canonical_sha(&value["guarded_add"])?
                        || request["admission"]["original_model_sha256"]
                            != value["models"]["dmonitoring_model_tinygrad.pkl"]["sha256"]
                    {
                        return Err(
                            "cached current exporter/helper/original admission differs".into()
                        );
                    }
                    let mut found = 0;
                    for name in files
                        .keys()
                        .filter(|name| name.ends_with("/cached-admission.json"))
                    {
                        let candidate =
                            read_regular(&catalog.root.join(super::relative(name)?), MAX_RECEIPT)?;
                        let actual: Value = serde_json::from_slice(&candidate)?;
                        if actual != request["admission"]
                            || sha(&candidate) != binding["admission_sha256"]
                        {
                            return Err(
                                "cached sidecar differs from independent issuer admission".into()
                            );
                        }
                        found += 1;
                    }
                    if found != 2 {
                        return Err(
                            "cached complete driver admission sidecar roster differs".into()
                        );
                    }
                }
                held.push(input);
            }
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
        helper_matches(&self.value["guarded_add"])?;
        cached_matches(&self.value["cached_code"])?;
        if self.value["exporter_dependency_files"]
            .as_object()
            .ok_or("driver source roster missing")?
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
            != source_roster(
                &self.source_root,
                !self.value["guarded_add"].is_null(),
                !self.value["cached_code"].is_null(),
            )?
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
    pub(super) fn cached_admission(&self) -> Result<Option<(PathBuf, String)>> {
        self.verify()?;
        if self.value["cached_code"].is_null() {
            return Ok(None);
        }
        let generation = self.value["catalog_generation"]
            .as_str()
            .ok_or("cached generation absent")?;
        let path = self
            .catalog_root
            .join(generation)
            .join("driver-1344x760/cached-admission.json");
        let expected = self.value["cached_code"]["admission_sha256"]
            .as_str()
            .ok_or("cached admission SHA absent")?;
        let input = InputPin::open(&path, false)?;
        if input.digest != expected {
            return Err("cached expected admission changed".into());
        }
        Ok(Some((path, expected.to_owned())))
    }
    pub(super) fn report(&self) -> Value {
        json!({"receipt":self.receipt.receipt(),"catalog_generation":self.value["catalog_generation"],"host":self.value["host"],"source_commit":self.value["source_commit"],"stored_jit_executed":false,"metadata_excludes":["CPU frequency","uptime","process IDs"]})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Exercises source selection with real held files; this is not a fabricated
    // successful provider/catalog admission or a model numerical proof.
    fn exporter_binding(root: &Path) -> CurrentCatalog {
        let source = root.join("rust/tools/model_export/schema.py");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"current exporter source").unwrap();
        let pin = InputPin::open(&source, false).unwrap();
        let mut source_seals = vec![SourceSeal {
            path: source,
            identity: pin.identity,
            digest: pin.digest,
        }];
        let runtime = root.join("openpilot/selfdrive/modeld/dmonitoringmodeld.py");
        fs::create_dir_all(runtime.parent().unwrap()).unwrap();
        fs::write(&runtime, b"current runtime source").unwrap();
        let pin = InputPin::open(&runtime, false).unwrap();
        source_seals.push(SourceSeal {
            path: runtime,
            identity: pin.identity,
            digest: pin.digest,
        });
        let receipt = root.join("current-catalog-receipt.json");
        fs::write(&receipt, br#"{"guarded_add":"held binding"}"#).unwrap();
        CurrentCatalog {
            receipt: InputPin::open(&receipt, false).unwrap(),
            value: Value::Null,
            source_root: root.to_owned(),
            catalog_root: root.to_owned(),
            held: Vec::new(),
            source_seals,
            catalog_names: BTreeSet::new(),
        }
    }
    #[test]
    fn current_exporter_pin_rejects_mutation_alias_and_unsealed_source() {
        let root = tempfile::tempdir().unwrap();
        let binding = exporter_binding(root.path());
        let name = "rust/tools/model_export/schema.py";
        let original = binding.exporter_source(name).unwrap().unwrap();
        assert_eq!(original.digest, sha(b"current exporter source"));
        assert!(binding
            .exporter_source("rust/tools/model_export/unsealed.py")
            .is_err());
        for name in [
            "openpilot/selfdrive/modeld/dmonitoringmodeld.py",
            "rust/tools/check_pipeline_reference.py",
            "rust/tools/model_export/../model_output_reference.py",
        ] {
            assert!(!matches!(binding.exporter_source(name), Ok(Some(_))));
        }
        let path = root.path().join(name);
        fs::write(&path, b"mutated exporter source").unwrap();
        assert!(binding.exporter_source(name).is_err());
        fs::write(&path, b"current exporter source").unwrap();
        assert!(
            binding.exporter_source(name).is_err(),
            "stale identity admitted"
        );
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&binding.receipt.path, &path).unwrap();
        assert!(binding.exporter_source(name).is_err());
    }
    #[test]
    fn exporter_pin_rejects_stale_current_bridge_receipt() {
        let root = tempfile::tempdir().unwrap();
        let binding = exporter_binding(root.path());
        let name = "rust/tools/model_export/schema.py";
        assert!(binding.exporter_source(name).unwrap().is_some());
        let replacement = root.path().join("replaced-receipt.json");
        fs::copy(&binding.receipt.path, &replacement).unwrap();
        fs::rename(replacement, &binding.receipt.path).unwrap();
        assert!(
            binding.exporter_source(name).is_err(),
            "identical foreign binding admitted"
        );
    }
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
        let first = source_roster(root.path(), false, false).unwrap();
        assert!(source_roster(root.path(), true, false).is_err());
        let module = root.path().join("rust/tools/model_export/new.py");
        fs::write(&module, b"new exporter").unwrap();
        assert_ne!(source_roster(root.path(), false, false).unwrap(), first);
        fs::remove_file(module).unwrap();
        assert_eq!(source_roster(root.path(), false, false).unwrap(), first);
        let linked = root.path().join("rust/tools/model_export/linked.py");
        std::os::unix::fs::symlink(root.path().join("uv.lock"), &linked).unwrap();
        assert!(source_roster(root.path(), false, false).is_err());
        fs::remove_file(linked).unwrap();
        let linked = root.path().join("rust/tools/model_export/linked-directory");
        std::os::unix::fs::symlink(root.path().join("openpilot/common"), &linked).unwrap();
        assert!(source_roster(root.path(), false, false).is_err());
        fs::remove_file(linked).unwrap();
        fs::remove_file(root.path().join("uv.lock")).unwrap();
        assert!(source_roster(root.path(), false, false).is_err());
    }
}

#[cfg(test)]
mod cached_binding_canonical_tests {
    use super::*;

    #[test]
    fn nested_insertion_order_does_not_change_or_authorize_a_cached_binding() {
        let first: Value = serde_json::from_str(r#"{"😀":"한글","helper":{"path":"/검증/😀/worker","sha256":"abc","identity":[1,2,3]},"가":"🌌","sources":[{"path":"원본/🚙.rs","sha256":"123"}]}"#).unwrap();
        let reordered: Value = serde_json::from_str(r#"{"sources":[{"sha256":"123","path":"원본/🚙.rs"}],"가":"🌌","helper":{"identity":[1,2,3],"sha256":"abc","path":"/검증/😀/worker"},"😀":"한글"}"#).unwrap();
        // Literal UTF8 bytes and independently pinned SHA define the shared
        // Python/Rust contract, including non-ASCII keys and astral characters.
        const EXPECTED_CANONICAL: &str = r#"{"helper":{"identity":[1,2,3],"path":"/검증/😀/worker","sha256":"abc"},"sources":[{"path":"원본/🚙.rs","sha256":"123"}],"가":"🌌","😀":"한글"}"#;
        const EXPECTED_SHA256: &str =
            "a20da5d9e880d168f27d7f9889b6b639b24caad3f1f86836cf57c6784e3e7ce8";
        let mut canonical = first.clone();
        canonical.sort_all_objects();
        assert_eq!(
            serde_json::to_vec(&canonical).unwrap(),
            EXPECTED_CANONICAL.as_bytes()
        );
        assert_eq!(cached_canonical_sha(&first).unwrap(), EXPECTED_SHA256);
        let expected = EXPECTED_SHA256;
        let before = serde_json::to_vec(&reordered).unwrap();
        assert_eq!(cached_canonical_sha(&reordered).unwrap(), expected);
        // Canonicalizing the digest must not mutate the issued receipt values.
        assert_eq!(serde_json::to_vec(&reordered).unwrap(), before);
        let mut changed = reordered;
        changed["helper"]["identity"][2] = json!(4);
        assert_ne!(cached_canonical_sha(&changed).unwrap(), expected);
        changed["helper"]["identity"][2] = json!(3);
        changed["sources"][0]["sha256"] = json!("stale");
        assert_ne!(cached_canonical_sha(&changed).unwrap(), expected);
    }
}
