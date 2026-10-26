//! Source-built runtime generations. Sealing and active selection are separate.
//! The existing manager sealer and ELF audit remain mandatory, source-built tools.
use crate::{
    digest_file, objects, prepare::Prepared, regular_file, source, source_store::Snapshot, Result,
};
use openpilot_runtime_installer::{
    artifact_path_allowed, source_asset_allowed, verify_source_build,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

const LIMIT: u64 = 4 * 1024 * 1024;
const AGNOS: &str = "19.8-carrot-bt1";

pub(crate) fn require_tools<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<()> {
    let names: BTreeSet<_> = names.into_iter().collect();
    for name in openpilot_runtime_installer::required_runtime_executables()
        .iter()
        .copied()
        .chain([
            "seal-runtime-package",
            "runtime-elf-audit",
            "openpilot-runtime-native",
            "openpilot-native-build-scope",
        ])
    {
        if !names.contains(name) {
            return Err(format!(
                "native build plan is missing required runtime/sealing executable: {name}"
            )
            .into());
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Stores {
    pub sources: PathBuf,
    pub objects: PathBuf,
    pub generations: PathBuf,
    pub params: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: u32,
    pub stores: Stores,
    pub cache: PathBuf,
    pub toolchain: PathBuf,
    pub build_plan: PathBuf,
    pub layout: PathBuf,
    pub resources: crate::resources::Policy,
}

impl Config {
    pub fn read(path: &Path) -> Result<Self> {
        let config: Self = read(path)?;
        config.resources.validate()?;
        if config.schema != 1
            || [
                &config.stores.sources,
                &config.stores.objects,
                &config.stores.generations,
                &config.stores.params,
                &config.cache,
                &config.toolchain,
                &config.build_plan,
                &config.layout,
            ]
            .iter()
            .any(|p| {
                !p.is_absolute()
                    || p.components()
                        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
            })
        {
            return Err("native configuration requires absolute, contained path spellings".into());
        }
        Ok(config)
    }

    /// Full native build/seal entry. Activation remains a separate offroad step.
    pub fn build(&self, root: &Path) -> Result<String> {
        offroad(&self.stores.params)?;
        agnos_identity()?;
        let layout: Layout = read(&self.layout)?;
        let plan = crate::prepare::Plan::read(&self.build_plan)?;
        if plan.groups.values().any(|recipe| {
            recipe
                .environment
                .get("OPENPILOT_NATIVE_INCREMENTAL")
                .map(String::as_str)
                != Some("1")
        }) {
            return Err("native build plan must explicitly bind incremental source-stamp mode in every group".into());
        }
        require_tools(
            plan.groups
                .values()
                .flat_map(|r| r.binaries.iter().map(String::as_str)),
        )?;
        let source = crate::source_store::retain(
            root,
            &self.stores.sources,
            plan.expected_source_growth_bytes,
        )?;
        let workspace =
            source.workspace(&self.stores.sources, layout.expected_source_growth_bytes)?;
        crate::catalog::verify_source_plan(&workspace.root, &plan)?;
        crate::system_sdk::generation_profiles(&workspace.root, &plan)?;
        let prepared = crate::prepare::from_retained(
            &workspace.root,
            &self.cache,
            &self.toolchain,
            &self.stores.sources,
            &self.stores.objects,
            &plan,
            source,
        )?;
        seal(root, &self.stores, prepared, layout)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub schema: u32,
    pub toolchain_root: PathBuf,
    pub provider_root: PathBuf,
    pub provider_receipt: PathBuf,
    /// Runtime destination -> provider-relative file (not directory).
    pub files: BTreeMap<PathBuf, PathBuf>,
    /// Ordered external AGNOS library directories. rust/bin/lib precedes these.
    pub libraries: Vec<PathBuf>,
    pub expected_source_growth_bytes: u64,
    pub remaining_object_growth_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SystemFile {
    pub target: PathBuf,
    pub sha256: String,
}

/// Source identity passed to the existing sealer before build.json is generated.
/// Only allowlisted LFS assets with their committed OID may differ from Git blobs.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceForSeal {
    pub schema: u32,
    pub source: Snapshot,
    pub source_store: PathBuf,
    pub assets: BTreeMap<PathBuf, String>,
}

impl SourceForSeal {
    pub fn read_and_verify(path: &Path, root: &Path) -> Result<Self> {
        let input: Self = read(path)?;
        if input.schema != 1 || input.source.root != root {
            return Err("native sealer source root mismatch".into());
        }
        input
            .source
            .verify_runtime(&input.source_store, None, &input.assets)?;
        Ok(input)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Sealed {
    pub schema: u32,
    pub origin: String,
    pub architecture: String,
    pub startup_update_protocol: u32,
    pub agnos_version: String,
    pub prepared: Prepared,
    pub runtime: Snapshot,
    pub layout: Layout,
    pub metadata_sha256: String,
    pub manifest_sha256: String,
    pub abi_report: objects::Object,
    pub files: BTreeMap<PathBuf, objects::Object>,
    pub system: BTreeMap<PathBuf, SystemFile>,
    pub agnos_identity: BTreeMap<PathBuf, String>,
    pub stored_output_bytes: u64,
}

fn relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|c| matches!(c, Component::Normal(name) if name != ".git"))
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(crate) fn directory(path: &Path) -> Result<()> {
    match fs::create_dir(path) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err("native generation store requires real directories".into());
    }
    Ok(())
}

pub(crate) fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let file = regular_file(path)?;
    if file.metadata()?.len() > LIMIT {
        return Err("native generation JSON exceeds 4 MiB".into());
    }
    Ok(serde_json::from_reader(file.take(LIMIT + 1))?)
}

pub(crate) fn write<T: Serialize>(path: &Path, value: &T, replace: bool) -> Result<()> {
    let parent = path
        .parent()
        .ok_or("native generation file needs a parent")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    if file.as_file().metadata()?.len() > LIMIT {
        return Err("native generation JSON exceeds 4 MiB".into());
    }
    file.as_file().sync_all()?;
    if replace {
        file.persist(path)?;
    } else {
        file.persist_noclobber(path)?;
    }
    File::open(parent)?.sync_all()?;
    Ok(())
}

pub(crate) fn lock(store: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(store.join("generation.lock"))?;
    if !file.metadata()?.is_file() {
        return Err("native generation lock is not regular".into());
    }
    rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
    Ok(file)
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err("cannot capture source-build metadata".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim_end().into())
}

fn metadata(root: &Path, prepared: &Prepared) -> Result<Value> {
    if source::inspect(root)? != prepared.source.source {
        return Err("requested source differs from prepared source".into());
    }
    let origin = git(root, &["remote", "get-url", "origin"])?;
    if ![
        "https://github.com/bin9208/openpilot-rust.git",
        "https://github.com/bin9208/openpilot-rust",
        "git@github.com:bin9208/openpilot-rust.git",
        "ssh://git@github.com/bin9208/openpilot-rust.git",
    ]
    .contains(&origin.as_str())
    {
        return Err("native runtime requires the openpilot-rust source origin".into());
    }
    let header = fs::read_to_string(root.join("openpilot/common/version.h"))?;
    let version = header
        .split('"')
        .nth(1)
        .ok_or("source version header is invalid")?;
    let notes = fs::read_to_string(root.join("RELEASES.md"))?;
    Ok(
        json!({"channel":git(root,&["rev-parse","--abbrev-ref","HEAD"])? ,"openpilot":{
        "version":version,"release_notes":notes.split_once("\n\n").map_or(notes.as_str(), |(first,_)| first),
        "git_commit":prepared.source.source.commit,"git_origin":origin,
        "git_commit_date":git(root,&["show","--no-patch","--format='%ct %ci'","HEAD"])? ,"build_style":"source-build","is_dirty":false}}),
    )
}

struct Pins {
    files: BTreeMap<PathBuf, String>,
    plain_trees: BTreeSet<PathBuf>,
}

fn absolute(prepared: &Prepared, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        prepared.source.root.join(path)
    }
}

impl Pins {
    /// Verify each complete tree once for this sealing boundary, even when many
    /// groups and ELF dependencies use it. Never replace this with metadata-only
    /// freshness checks. The final boundary repeats this after inspection tools.
    fn capture(prepared: &Prepared) -> Result<Self> {
        let mut files = BTreeMap::new();
        let mut trees = BTreeMap::new();
        let mut plain_trees = BTreeSet::new();
        let runtime = &prepared.plan.runtime_inputs;
        let inputs = prepared
            .plan
            .groups
            .iter()
            .map(|(group, recipe)| {
                (
                    &recipe.inputs,
                    &recipe.input_trees,
                    &prepared.reports[group].verified_input_trees,
                )
            })
            .chain(std::iter::once((
                &runtime.files,
                &runtime.trees,
                &prepared.verified_runtime_trees,
            )));
        for (input_files, input_trees, verified_trees) in inputs {
            for (path, expected) in input_files {
                let path = absolute(prepared, path).canonicalize()?;
                if !files.contains_key(&path) && digest_file(&path)? != *expected {
                    return Err("native input changed after compilation".into());
                }
                if files
                    .insert(path, expected.clone())
                    .is_some_and(|previous| previous != *expected)
                {
                    return Err("conflicting native file input identities".into());
                }
            }
            for (tree_key, receipt_key) in input_trees {
                let tree = absolute(prepared, tree_key).canonicalize()?;
                let receipt = absolute(prepared, receipt_key).canonicalize()?;
                let expected = input_files
                    .get(receipt_key)
                    .ok_or("native tree receipt is unpinned")?;
                if files.get(&receipt) != Some(expected)
                    || verified_trees.get(tree_key) != Some(expected)
                {
                    return Err(
                        "native tree verification is not bound to its compiler report".into(),
                    );
                }
                if let Some(previous) = trees.get(&tree) {
                    if previous != expected {
                        return Err("conflicting complete native tree identities".into());
                    }
                    continue;
                }
                let receipt = crate::inputs::Receipt::read(&receipt)?;
                receipt.verify(&tree)?;
                if matches!(&receipt, crate::inputs::Receipt::Tree(_)) {
                    plain_trees.insert(tree.clone());
                }
                for (path, sha256) in receipt.files(&tree)? {
                    if files
                        .insert(path, sha256.clone())
                        .is_some_and(|previous| previous != sha256)
                    {
                        return Err("conflicting native tree member identities".into());
                    }
                }
                trees.insert(tree, expected.clone());
            }
        }
        Ok(Self { files, plain_trees })
    }

    fn contains(&self, path: &Path, sha256: &str) -> bool {
        self.files
            .get(path)
            .is_some_and(|expected| expected == sha256)
    }
}

impl Layout {
    fn verify(&self, prepared: &Prepared) -> Result<Pins> {
        if self.schema != 1
            || self.files.is_empty()
            || self.files.len() > 10000
            || self.libraries.is_empty()
            || self.libraries.len() > 64
        {
            return Err("invalid native generation layout".into());
        }
        let provider = self.provider_root.canonicalize()?;
        let receipt = self.provider_receipt.canonicalize()?;
        let receipt_sha = digest_file(&receipt)?;
        let pins = Pins::capture(prepared)?;
        let mut provider_bound = false;
        for (group, recipe) in &prepared.plan.groups {
            if recipe.target != "aarch64-unknown-linux-gnu" {
                return Err("runtime sealing requires native AArch64 build groups".into());
            }
            let report = &prepared.reports[group];
            let toolchain = self.toolchain_root.canonicalize()?;
            if report.toolchain_directory.parent() != Some(toolchain.as_path())
                || !pins.contains(
                    &report.toolchain_directory.join("rustc"),
                    &report.rustc_sha256,
                )
                || !pins.contains(
                    &report.toolchain_directory.join("cargo"),
                    &report.cargo_sha256,
                )
            {
                return Err(
                    "native toolchain tree differs from the actual compiler location or identity"
                        .into(),
                );
            }
            let mut toolchain_bound = false;
            for (tree, receipt_path) in &recipe.input_trees {
                let absolute = |p: &PathBuf| {
                    if p.is_absolute() {
                        p.clone()
                    } else {
                        prepared.source.root.join(p)
                    }
                };
                let root = absolute(tree).canonicalize()?;
                let receipt_path_absolute = absolute(receipt_path).canonicalize()?;
                if root == toolchain || root == provider {
                    if !pins.plain_trees.contains(&root) {
                        return Err("a system SDK cannot replace an unpacked Rust toolchain or runtime provider tree".into());
                    }
                    if report.verified_input_trees.get(tree) != recipe.inputs.get(receipt_path) {
                        return Err("native tree verification missing from build report".into());
                    }
                    toolchain_bound |= root == toolchain;
                    provider_bound |= root == provider
                        && receipt_path_absolute == receipt
                        && recipe.inputs.get(receipt_path) == Some(&receipt_sha);
                }
            }
            if !toolchain_bound {
                return Err(
                    "complete native toolchain tree was not bound to each build group".into(),
                );
            }
            for (path, sha) in &report.native_tools {
                if !pins.contains(path, sha) {
                    return Err("native linker/compiler tool is not bound to build inputs".into());
                }
            }
        }
        for (tree, receipt_key) in &prepared.plan.runtime_inputs.trees {
            provider_bound |= absolute(prepared, tree).canonicalize()? == provider
                && absolute(prepared, receipt_key).canonicalize()? == receipt
                && prepared.plan.runtime_inputs.files.get(receipt_key) == Some(&receipt_sha)
                && prepared.verified_runtime_trees.get(tree) == Some(&receipt_sha);
        }
        if !provider_bound {
            return Err("runtime provider tree was not bound to native build inputs".into());
        }
        for (target, source) in &self.files {
            if !artifact_path_allowed(target)
                || !relative(source)
                || !self
                    .provider_root
                    .join(source)
                    .canonicalize()?
                    .starts_with(&provider)
            {
                return Err("unsafe native provider mapping".into());
            }
        }
        Ok(pins)
    }
}

fn parent(root: &Path, relative: &Path) -> Result<PathBuf> {
    if !relative
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err("unsafe candidate parent".into());
    }
    let mut path = root.to_path_buf();
    for component in relative.components() {
        path.push(component);
        directory(&path)?;
    }
    Ok(path)
}

fn tool(root: &Path, name: &str, log: &Path) -> Result<Command> {
    let mut command = Command::new("/usr/bin/timeout");
    command
        .args(["--signal=TERM", "--kill-after=30s", "1800"])
        .arg(root.join("rust/bin").join(name))
        .current_dir(root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LC_ALL", "C")
        .env("LD_LIBRARY_PATH", root.join("rust/bin/lib"))
        .stdout(Stdio::from(File::create(log)?))
        .stderr(Stdio::from(File::create(log.with_extension("stderr"))?));
    Ok(command)
}

/// All paths are owned and have already been checked. Flush source data and Git
/// worktree metadata before a durable descriptor can refer to this execution root.
fn sync_source(snapshot: &Snapshot) -> Result<()> {
    fn tree(root: &Path) -> Result<()> {
        let mut pending = vec![root.to_owned()];
        let mut directories = Vec::new();
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(&directory)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    pending.push(entry.path());
                } else if entry.file_type()?.is_file() {
                    regular_file(&entry.path())?.sync_all()?;
                }
            }
            directories.push(directory);
        }
        for directory in directories.iter().rev() {
            File::open(directory)?.sync_all()?;
        }
        Ok(())
    }
    tree(&snapshot.root)?;
    for module in
        std::iter::once(Path::new("")).chain(snapshot.modules.keys().map(PathBuf::as_path))
    {
        let root = snapshot.root.join(module);
        let gitdir = PathBuf::from(git(&root, &["rev-parse", "--absolute-git-dir"])?);
        tree(&gitdir)?;
        File::open(gitdir.parent().ok_or("missing Git worktrees directory")?)?.sync_all()?;
    }
    File::open(snapshot.root.parent().ok_or("missing source store")?)?.sync_all()?;
    Ok(())
}

impl Sealed {
    pub fn verify(&self, stores: &Stores) -> Result<()> {
        if self.schema != 1
            || self.origin != "source-build"
            || self.architecture != "aarch64"
            || self.startup_update_protocol != 1
            || self.agnos_version != AGNOS
            || !hex(&self.metadata_sha256, 64)
            || !hex(&self.manifest_sha256, 64)
        {
            return Err("invalid source-built runtime identity".into());
        }
        self.prepared.verify(&stores.sources, &stores.objects)?;
        crate::catalog::verify_source_plan(&self.prepared.source.root, &self.prepared.plan)?;
        // Prepared::verify checks each group's recorded input-tree identities.
        // Do not reopen compiler SDK caches during boot/fallback selection.
        crate::system_sdk::generation_receipts(&self.prepared.plan)?;
        require_tools(self.prepared.binaries.keys().map(String::as_str))?;
        if self.runtime.source != self.prepared.source.source
            || self.runtime.root == self.prepared.source.root
        {
            return Err("runtime candidate does not own an independent source root".into());
        }
        let assets = self
            .files
            .iter()
            .filter(|(p, _)| source_asset_allowed(p))
            .map(|(p, o)| (p.clone(), o.sha256.clone()))
            .collect();
        self.runtime
            .verify_runtime(&stores.sources, Some(&self.metadata_sha256), &assets)?;
        let root = &self.runtime.root;
        if digest_file(&root.join("rust/runtime-manifest.json"))? != self.manifest_sha256 {
            return Err("source-build manifest digest mismatch".into());
        }
        let manifest = verify_source_build(root, &self.runtime.source.commit)?;
        if manifest.files.len() != self.files.len() {
            return Err("source-build manifest/object roster mismatch".into());
        }
        for artifact in manifest.files {
            let object = self
                .files
                .get(&artifact.path)
                .ok_or("source-build artifact missing from objects")?;
            if artifact.sha256 != object.sha256
                || fs::metadata(root.join(&artifact.path))?.len() != object.bytes
            {
                return Err("source-build artifact differs from content object".into());
            }
            object.verify(&stores.objects)?;
        }
        for (name, object) in &self.prepared.binaries {
            if self.files.get(&PathBuf::from("rust/bin").join(name)) != Some(object) {
                return Err("source-build binary differs from its compiler result".into());
            }
            openpilot_runtime_installer::verify_native_elf(
                &root.join("rust/bin").join(name),
                true,
            )?;
        }
        for (path, expected) in &self.system {
            if path.canonicalize()? != expected.target
                || digest_file(&expected.target)? != expected.sha256
            {
                return Err("AGNOS runtime library identity changed".into());
            }
        }
        if self.agnos_identity.keys().cloned().collect::<BTreeSet<_>>()
            != BTreeSet::from([PathBuf::from("/AGNOS"), PathBuf::from("/VERSION")])
            || fs::read_to_string("/VERSION")?.trim() != AGNOS
        {
            return Err("source-built runtime requires its actual AGNOS version".into());
        }
        for (path, expected) in &self.agnos_identity {
            if digest_file(path)? != *expected {
                return Err("AGNOS identity changed after sealing".into());
            }
        }
        let report: Value = read(&self.abi_report.verify(&stores.objects)?)?;
        validate_abi(&report, root, &self.files)?;
        self.verify_abi_objects(&report)?;
        let metadata: Value = read(&root.join("build.json"))?;
        if metadata["openpilot"]["git_commit"] != self.runtime.source.commit
            || metadata["openpilot"]["build_style"] != "source-build"
        {
            return Err("source-build metadata identity mismatch".into());
        }
        Ok(())
    }

    fn verify_abi_objects(&self, report: &Value) -> Result<()> {
        let root = &self.runtime.root;
        let rows = report["objects"]
            .as_array()
            .ok_or("ABI object list missing")?;
        if rows.is_empty() || rows.len() > 20000 {
            return Err("invalid ABI object count".into());
        }
        let mut paths = BTreeSet::new();
        let mut libraries = vec![root.join("rust/bin/lib")];
        libraries.extend(self.layout.libraries.iter().cloned());
        for row in rows {
            let path = PathBuf::from(row["path"].as_str().ok_or("ABI object path missing")?);
            let sha = row["sha256"].as_str().ok_or("ABI object digest missing")?;
            if !paths.insert(path.clone())
                || path.canonicalize()? != path
                || digest_file(&path)? != sha
            {
                return Err("ABI object identity changed".into());
            }
            if let Ok(relative) = path.strip_prefix(root) {
                if self
                    .files
                    .get(relative)
                    .is_none_or(|object| object.sha256 != sha)
                {
                    return Err("ABI object is not a sealed runtime artifact".into());
                }
            } else if self
                .system
                .get(&path)
                .is_none_or(|file| file.target != path || file.sha256 != sha)
            {
                return Err("ABI object is not a bound AGNOS library".into());
            }
            for needed in row["needed"].as_array().ok_or("ABI dependencies missing")? {
                let soname = needed.as_str().ok_or("invalid ABI soname")?;
                if !relative(Path::new(soname)) || Path::new(soname).components().count() != 1 {
                    return Err("unsafe ABI soname".into());
                }
                let alias = libraries
                    .iter()
                    .map(|p| p.join(soname))
                    .find(|p| p.is_file())
                    .ok_or("ABI dependency disappeared")?;
                let target = alias.canonicalize()?;
                if !target.starts_with(root)
                    && self
                        .system
                        .get(&alias)
                        .is_none_or(|file| file.target != target)
                {
                    return Err("ABI library alias differs from the verified selection".into());
                }
                if !rows
                    .iter()
                    .any(|object| object["path"].as_str() == target.to_str())
                {
                    return Err("ABI dependency was not inspected".into());
                }
            }
        }
        for row in report["roots"].as_array().ok_or("ABI roots missing")? {
            if !paths.contains(Path::new(
                row["root"].as_str().ok_or("ABI root path missing")?,
            )) {
                return Err("ABI root was not inspected".into());
            }
        }
        Ok(())
    }
}

fn validate_abi(
    report: &Value,
    root: &Path,
    files: &BTreeMap<PathBuf, objects::Object>,
) -> Result<()> {
    if report["compatible"] != true {
        return Err("source-build ABI closure is incompatible".into());
    }
    let rows = report["roots"].as_array().ok_or("ABI roots missing")?;
    let mut actual = BTreeSet::new();
    for row in rows {
        if row["compatible"] != true
            || !row["failures"].as_array().is_some_and(Vec::is_empty)
            || !actual.insert(PathBuf::from(
                row["root"].as_str().ok_or("ABI root path missing")?,
            ))
        {
            return Err("invalid ABI root verification".into());
        }
    }
    let mut expected = BTreeSet::new();
    for path in files.keys() {
        let path = root.join(path);
        let mut prefix = [0; 4];
        if regular_file(&path)?.read(&mut prefix)? == 4 && prefix == *b"\x7fELF" {
            expected.insert(path.canonicalize()?);
        }
    }
    if actual != expected || actual.is_empty() {
        return Err("ABI report does not cover every runtime ELF".into());
    }
    Ok(())
}

/// Assemble and validate a full source-built runtime, then publish a descriptor.
/// There is no active-pointer mutation and no runtime release transport here.
pub fn seal(
    request_root: &Path,
    stores: &Stores,
    prepared: Prepared,
    layout: Layout,
) -> Result<String> {
    offroad(&stores.params)?;
    let agnos_identity = agnos_identity()?;
    prepared.verify(&stores.sources, &stores.objects)?;
    crate::catalog::verify_source_plan(&prepared.source.root, &prepared.plan)?;
    crate::system_sdk::generation_profiles(&prepared.source.root, &prepared.plan)?;
    require_tools(prepared.binaries.keys().map(String::as_str))?;
    let pins = layout.verify(&prepared)?;
    let metadata = metadata(request_root, &prepared)?;
    directory(&stores.generations)?;
    let _lock = lock(&stores.generations)?;
    let evidence = tempfile::Builder::new()
        .prefix("evidence-")
        .tempdir_in(&stores.generations)?
        .keep();
    let runtime = prepared
        .source
        .fork(&stores.sources, layout.expected_source_growth_bytes)?;
    parent(&runtime.root, Path::new("rust/bin"))?;
    let mut files = BTreeMap::new();
    let mut stored_output_bytes = 0_u64;
    for (name, object) in &prepared.binaries {
        let destination = PathBuf::from("rust/bin").join(name);
        object.materialize(&stores.objects, &runtime.root.join(&destination))?;
        openpilot_runtime_installer::verify_native_elf(&runtime.root.join(&destination), true)?;
        files.insert(destination, object.clone());
    }
    for (destination, source) in &layout.files {
        if files.contains_key(destination) {
            return Err("provider cannot replace a source-built runtime binary".into());
        }
        let input = layout.provider_root.join(source).canonicalize()?;
        let imported = objects::import(
            &stores.objects,
            &input,
            layout.remaining_object_growth_bytes,
        )?;
        if source_asset_allowed(destination) && imported.object.mode != 0o444 {
            return Err("source asset cannot be executable".into());
        }
        if destination.parent() == Some(Path::new("rust/bin"))
            && imported.object.mode == 0o555
            && !["ffmpeg", "ffprobe"].contains(
                &destination
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(""),
            )
        {
            return Err("provider cannot supply a project runtime executable".into());
        }
        parent(
            &runtime.root,
            destination
                .parent()
                .ok_or("candidate destination parent missing")?,
        )?;
        let target = runtime.root.join(destination);
        if target.try_exists()? {
            let lfs = if source_asset_allowed(destination) {
                source::lfs_identity(&runtime.root, destination)?
            } else {
                None
            };
            let matches_lfs = lfs.is_some_and(|(sha, bytes)| {
                sha == imported.object.sha256 && bytes == imported.object.bytes
            });
            if !source_asset_allowed(destination)
                || (!matches_lfs && digest_file(&target)? != imported.object.sha256)
            {
                return Err(
                    "provider would change tracked source or an existing candidate file".into(),
                );
            }
            fs::remove_file(&target)?;
        }
        imported.object.materialize(&stores.objects, &target)?;
        stored_output_bytes = stored_output_bytes
            .checked_add(imported.stored_bytes)
            .ok_or("generation accounting overflow")?;
        files.insert(destination.clone(), imported.object);
    }
    let source_assets = files
        .iter()
        .filter(|(p, _)| source_asset_allowed(p))
        .map(|(p, o)| (p.clone(), o.sha256.clone()))
        .collect();
    runtime.verify_runtime(&stores.sources, None, &source_assets)?;
    let source_input = evidence.join("source.json");
    write(
        &source_input,
        &SourceForSeal {
            schema: 1,
            source: runtime.clone(),
            source_store: stores.sources.clone(),
            assets: source_assets,
        },
        false,
    )?;
    for name in [
        "seal-runtime-package",
        "runtime-elf-audit",
        "openpilot-manager",
    ] {
        if !prepared.binaries.contains_key(name) {
            return Err("source-built sealing/inspection tool missing".into());
        }
    }
    let protocol = evidence.join("protocol.stdout");
    let status = tool(&runtime.root, "openpilot-launch", &protocol)?
        .arg("--verify-native-generation-protocol")
        .arg("--root")
        .arg(&runtime.root)
        .status()?;
    if !status.success()
        || read::<Value>(&protocol)?
            != json!({"source_generation_protocol":1,"startup":true,"updates":true})
    {
        return Err("source runtime has no verified native-generation startup/update handoff; candidate not published".into());
    }
    let status = tool(
        &runtime.root,
        "seal-runtime-package",
        &evidence.join("seal.stdout"),
    )?
    .arg("--root")
    .arg(&runtime.root)
    .arg("--manager")
    .arg(runtime.root.join("rust/bin/openpilot-manager"))
    .args(["--architecture", "aarch64"])
    .arg("--source-build")
    .arg(&source_input)
    .status()?;
    if !status.success() {
        return Err("manager inventory/runtime seal failed; previous generation retained".into());
    }
    let manifest = verify_source_build(&runtime.root, &runtime.source.commit)?;
    for artifact in &manifest.files {
        if !files.contains_key(&artifact.path) {
            if !source_asset_allowed(&artifact.path) {
                return Err("sealer found an unprepared runtime output".into());
            }
            let imported = objects::import(
                &stores.objects,
                &runtime.root.join(&artifact.path),
                layout.remaining_object_growth_bytes,
            )?;
            stored_output_bytes = stored_output_bytes
                .checked_add(imported.stored_bytes)
                .ok_or("generation accounting overflow")?;
            files.insert(artifact.path.clone(), imported.object);
        }
    }
    let mut libraries = vec![runtime.root.join("rust/bin/lib")];
    libraries.extend(layout.libraries.iter().cloned());
    let abi = evidence.join("abi.json");
    let mut audit = tool(
        &runtime.root,
        "runtime-elf-audit",
        &evidence.join("abi.stdout"),
    )?;
    for path in files.keys() {
        let path = runtime.root.join(path);
        let mut magic = [0; 4];
        if regular_file(&path)?.read(&mut magic)? == 4 && magic == *b"\x7fELF" {
            audit.arg("--root").arg(path);
        }
    }
    for library in &libraries {
        audit.arg("--library").arg(library);
    }
    if !audit.arg("--output").arg(&abi).status()?.success() {
        return Err("runtime ABI audit failed; previous generation retained".into());
    }
    let report: Value = read(&abi)?;
    validate_abi(&report, &runtime.root, &files)?;
    let mut system = BTreeMap::new();
    for object in report["objects"]
        .as_array()
        .ok_or("ABI object list missing")?
    {
        let path = PathBuf::from(object["path"].as_str().ok_or("ABI object path missing")?);
        let sha = object["sha256"]
            .as_str()
            .ok_or("ABI object digest missing")?;
        if !path.starts_with(&runtime.root) && !pins.contains(&path, sha) {
            return Err("runtime system library was not bound to native build inputs".into());
        }
        if !path.starts_with(&runtime.root) {
            system.insert(
                path.clone(),
                SystemFile {
                    target: path.clone(),
                    sha256: sha.into(),
                },
            );
        }
        for needed in object["needed"]
            .as_array()
            .ok_or("ABI dependency list missing")?
        {
            let needed = needed.as_str().ok_or("invalid ABI soname")?;
            if !relative(Path::new(needed)) || Path::new(needed).components().count() != 1 {
                return Err("unsafe ABI soname".into());
            }
            let alias = libraries
                .iter()
                .map(|p| p.join(needed))
                .find(|p| p.is_file())
                .ok_or("ABI provider missing after audit")?;
            if !alias.starts_with(&runtime.root) {
                let target = alias.canonicalize()?;
                system.insert(
                    alias,
                    SystemFile {
                        sha256: digest_file(&target)?,
                        target,
                    },
                );
            }
        }
    }
    write(&runtime.root.join("build.json"), &metadata, false)?;
    fs::set_permissions(
        runtime.root.join("build.json"),
        fs::Permissions::from_mode(0o444),
    )?;
    regular_file(&runtime.root.join("build.json"))?.sync_all()?;
    let imported = objects::import(&stores.objects, &abi, layout.remaining_object_growth_bytes)?;
    stored_output_bytes = stored_output_bytes
        .checked_add(imported.stored_bytes)
        .ok_or("generation accounting overflow")?;
    let sealed = Sealed {
        schema: 1,
        origin: "source-build".into(),
        architecture: "aarch64".into(),
        startup_update_protocol: 1,
        agnos_version: AGNOS.into(),
        metadata_sha256: digest_file(&runtime.root.join("build.json"))?,
        manifest_sha256: digest_file(&runtime.root.join("rust/runtime-manifest.json"))?,
        prepared,
        runtime,
        layout,
        abi_report: imported.object,
        files,
        system,
        agnos_identity,
        stored_output_bytes,
    };
    sealed.prepared.verify(&stores.sources, &stores.objects)?;
    sealed.layout.verify(&sealed.prepared)?;
    crate::system_sdk::generation_profiles(&sealed.prepared.source.root, &sealed.prepared.plan)?;
    sealed.verify(stores)?;
    if source::inspect(request_root)? != sealed.runtime.source {
        return Err("requested source changed during sealing; previous generation retained".into());
    }
    sync_source(&sealed.runtime)?;
    offroad(&stores.params)?;
    let id = format!("{:x}", Sha256::digest(serde_json::to_vec_pretty(&sealed)?));
    let destination = stores.generations.join(format!("{id}.json"));
    write(&destination, &sealed, false)?;
    Ok(id)
}

fn agnos_identity() -> Result<BTreeMap<PathBuf, String>> {
    if fs::read_to_string("/VERSION")?.trim() != AGNOS {
        return Err("native runtime work requires actual AGNOS 19.8-carrot-bt1 user space".into());
    }
    Ok(BTreeMap::from([
        (PathBuf::from("/AGNOS"), digest_file(Path::new("/AGNOS"))?),
        (
            PathBuf::from("/VERSION"),
            digest_file(Path::new("/VERSION"))?,
        ),
    ]))
}

pub fn offroad(params: &Path) -> Result<()> {
    let mut value = Vec::new();
    regular_file(&params.join("d/IsOnroad"))?
        .take(2)
        .read_to_end(&mut value)?;
    if value != b"0" {
        return Err("native generation work requires verified offroad state".into());
    }
    Ok(())
}

pub fn load(stores: &Stores, id: &str) -> Result<Sealed> {
    if !fs::symlink_metadata(&stores.generations)?.is_dir() {
        return Err("native generation store is redirected".into());
    }
    if !hex(id, 64) {
        return Err("invalid source generation ID".into());
    }
    let sealed: Sealed = read(&stores.generations.join(format!("{id}.json")))?;
    if format!("{:x}", Sha256::digest(serde_json::to_vec_pretty(&sealed)?)) != id {
        return Err("source generation descriptor digest mismatch".into());
    }
    sealed.verify(stores)?;
    Ok(sealed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatible_abi_report_must_cover_every_actual_runtime_elf() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("runtime");
        fs::create_dir_all(root.join("rust/bin")).unwrap();
        let store = temp.path().join("objects");
        let mut files = BTreeMap::new();
        for name in ["first", "second"] {
            let path = PathBuf::from("rust/bin").join(name);
            fs::copy("/bin/true", root.join(&path)).unwrap();
            files.insert(
                path.clone(),
                objects::import(&store, &root.join(&path), 0)
                    .unwrap()
                    .object,
            );
        }
        let row = |name: &str| json!({"root":root.join("rust/bin").join(name),"compatible":true,"failures":[]});
        assert!(validate_abi(
            &json!({"compatible":true,"roots":[row("first")]}),
            &root,
            &files
        )
        .is_err());
        assert!(validate_abi(
            &json!({"compatible":true,"roots":[row("first"),row("first")]}),
            &root,
            &files
        )
        .is_err());
        validate_abi(
            &json!({"compatible":true,"roots":[row("first"),row("second")]}),
            &root,
            &files,
        )
        .unwrap();
        assert!(validate_abi(
            &json!({"compatible":false,"roots":[row("first"),row("second")]}),
            &root,
            &files
        )
        .is_err());
    }

    #[test]
    fn missing_or_nonzero_offroad_state_is_not_an_activation_authorization() {
        let temp = tempfile::tempdir().unwrap();
        let params = temp.path().join("params");
        fs::create_dir_all(params.join("d")).unwrap();
        assert!(offroad(&params).is_err());
        for invalid in [b"".as_slice(), b"1", b"00", b"0\n"] {
            fs::write(params.join("d/IsOnroad"), invalid).unwrap();
            assert!(offroad(&params).is_err());
        }
        fs::write(params.join("d/IsOnroad"), b"0").unwrap();
        offroad(&params).unwrap();
    }
}
