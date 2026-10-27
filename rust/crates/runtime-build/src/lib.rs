//! Persistent Cargo builds for #598. This command does not install or activate a runtime.
//!
//! Cargo tracks source dependencies. A commit SHA is deliberately absent from the
//! target-directory key: documentation changes must not discard compiled crates.
//! Explicit external inputs and toolchain identities partition incompatible caches.
#![forbid(unsafe_code)]

#[cfg(any(
    feature = "prepare-rust",
    feature = "prepare-capnp",
    feature = "prepare-cargo",
    feature = "prepare-source-assets",
    feature = "prepare-python"
))]
mod archive_tree;
pub mod assets;
#[cfg(all(feature = "prepare-rust", feature = "prepare-cargo"))]
pub mod bootstrap_tools;
#[cfg(feature = "prepare-capnp")]
pub mod capnp;
#[cfg(feature = "prepare-cargo")]
pub mod cargo_inputs;
pub mod catalog;
pub mod catalog_bindings;
mod catalog_features;
pub mod dbc_inputs;
pub mod generation;
pub mod inputs;
pub mod memory_cgroup;
pub mod model_inputs;
pub mod mpc_inputs;
mod native_tools;
pub mod objects;
pub mod panda_inputs;
pub mod pending;
pub mod prepare;
#[cfg(feature = "prepare-python")]
pub mod python;
pub mod resources;
pub mod runtime_layout;
pub mod selection;
mod source;
#[cfg(feature = "prepare-source-assets")]
pub mod source_assets;
pub mod source_lfs;
pub mod source_store;
pub mod supervision;
pub mod system_sdk;
#[cfg(feature = "prepare-rust")]
pub mod toolchain;
pub mod usbgpu_inputs;
pub use source::Source;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub schema: u32,
    pub target: String,
    pub packages: Vec<String>,
    pub binaries: Vec<String>,
    pub features: Vec<String>,
    /// Files outside Cargo's tracked source graph, including provider receipts/headers.
    pub inputs: BTreeMap<PathBuf, String>,
    /// Unpacked tree -> receipt file. Each receipt must also be SHA-bound in inputs.
    #[serde(default)]
    pub input_trees: BTreeMap<PathBuf, PathBuf>,
    /// Required build environment, included verbatim in the cache identity.
    pub environment: BTreeMap<String, String>,
    pub expected_growth_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema: u32,
    pub cache_key: String,
    pub target_directory: PathBuf,
    pub artifacts: Vec<Artifact>,
    pub source: Source,
    pub recipe_sha256: String,
    pub rustc_sha256: String,
    pub cargo_sha256: String,
    pub toolchain_directory: PathBuf,
    pub native_tools: BTreeMap<PathBuf, String>,
    pub verified_input_trees: BTreeMap<PathBuf, String>,
    pub elapsed_ms: u128,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub executable: PathBuf,
    pub fresh: bool,
    pub sha256: String,
    pub bytes: u64,
}

fn regular_file(path: &Path) -> Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(format!("not a regular build input: {}", path.display()).into());
    }
    Ok(file)
}

pub fn digest_file(path: &Path) -> Result<String> {
    let mut file = regular_file(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

impl Recipe {
    pub fn read(path: &Path) -> Result<Self> {
        let file = regular_file(path)?;
        if file.metadata()?.len() > 1024 * 1024 {
            return Err("build recipe exceeds 1 MiB".into());
        }
        let recipe: Self = serde_json::from_reader(file)?;
        recipe.validate()?;
        Ok(recipe)
    }

    fn validate(&self) -> Result<()> {
        fn name(value: &str) -> bool {
            !value.is_empty()
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        }
        if self.schema != 1
            || !name(&self.target)
            || self.packages.is_empty()
            || self.binaries.is_empty()
            || !self.packages.iter().all(|p| name(p))
            || !self.binaries.iter().all(|p| name(p))
            || !self
                .features
                .iter()
                .all(|p| !p.is_empty() && p.split('/').all(name))
        {
            return Err("invalid runtime build recipe".into());
        }
        for (key, value) in &self.environment {
            // Cache/driver ownership cannot be overridden by a recipe.
            if key.is_empty()
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                || matches!(key.as_str(), "PATH" | "HOME" | "RUSTC" | "RUSTDOC")
                || key.starts_with("CARGO_")
                || (key == "RUSTFLAGS" && value.contains("linker"))
                || value.contains('\0')
            {
                return Err(format!("unsupported build environment key: {key}").into());
            }
        }
        if self
            .input_trees
            .values()
            .any(|receipt| !self.inputs.contains_key(receipt))
        {
            return Err("each unpacked input receipt must have a pinned SHA-256 in inputs".into());
        }
        Ok(())
    }

    fn verify_inputs(&self, root: &Path) -> Result<()> {
        for (path, expected) in &self.inputs {
            if expected.len() != 64
                || !expected
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || digest_file(&root.join(path))? != *expected
            {
                return Err(format!("build input identity mismatch: {}", path.display()).into());
            }
        }
        for (tree, receipt) in &self.input_trees {
            inputs::Receipt::read(&root.join(receipt))?.verify(&root.join(tree))?;
        }
        if let Some(value) = self.environment.get("OPENPILOT_CARGO_VENDOR") {
            let vendor = Path::new(value);
            if !vendor.is_absolute()
                || vendor.file_name() != Some(std::ffi::OsStr::new("vendor"))
                || !fs::symlink_metadata(vendor)?.is_dir()
                || vendor.canonicalize()? != vendor
            {
                return Err("Cargo vendor source must be its canonical prepared directory".into());
            }
            let prefix = vendor.parent().ok_or("missing Cargo vendor prefix")?;
            if !self.input_trees.keys().any(|p| root.join(p) == prefix) {
                return Err(
                    "Cargo vendor prefix requires its complete SHA-pinned input tree".into(),
                );
            }
            let metadata = regular_file(&prefix.join("provenance/native-cargo.json"))?;
            if metadata.metadata()?.len() > 4 * 1024 * 1024 {
                return Err("Cargo input provenance exceeds 4 MiB".into());
            }
            let metadata: serde_json::Value =
                serde_json::from_reader(metadata.take(4 * 1024 * 1024 + 1))?;
            if metadata["schema"] != 1
                || metadata["format"] != "openpilot-native-cargo-v1"
                || metadata["cargo_lock_sha256"] != digest_file(&root.join("rust/Cargo.lock"))?
            {
                return Err("prepared Cargo sources differ from the current lock file".into());
            }
        }
        Ok(())
    }
}

// Opt-in diagnostics stay on stderr and do not change saved report identities.
struct PhaseTimer<'a> {
    enabled: bool,
    marked: Instant,
    binaries: &'a [String],
}

impl PhaseTimer<'_> {
    fn mark(&mut self, phase: &str) {
        if self.enabled {
            eprintln!(
                "native-build-phase {}",
                serde_json::json!({
                    "schema":1,"phase":phase,"binaries":self.binaries,
                    "elapsed_ms":self.marked.elapsed().as_millis()
                })
            );
            self.marked = Instant::now();
        }
    }
}

/// Build only. The caller must seal, validate and activate the complete runtime separately.
/// No runtime release is downloaded, and no existing runtime or Cargo cache is removed.
pub fn build(root: &Path, cache: &Path, toolchain: &Path, recipe: &Recipe) -> Result<Report> {
    let started = Instant::now();
    let mut timing = PhaseTimer {
        enabled: std::env::var_os("OPENPILOT_NATIVE_BUILD_TIMINGS").as_deref()
            == Some(std::ffi::OsStr::new("1")),
        marked: started,
        binaries: &recipe.binaries,
    };
    recipe.validate()?;
    let root = root.canonicalize()?;
    let source = source::inspect(&root)?;
    timing.mark("source_before");
    let toolchain = toolchain.canonicalize()?;
    recipe.verify_inputs(&root)?;
    timing.mark("inputs_before");
    let rustc = toolchain.join("rustc");
    let cargo = toolchain.join("cargo");
    // The actual native compiler and Cargo are inputs, rather than unverified PATH aliases.
    let compiler_digest = digest_file(&rustc)?;
    let cargo_digest = digest_file(&cargo)?;
    let native = native_tools::inspect(recipe, &toolchain)?;
    let version = Command::new(&rustc)
        .arg("-vV")
        .env_clear()
        .envs(&recipe.environment)
        .envs(&native.environment)
        .env(
            "PATH",
            std::env::join_paths([
                toolchain.as_path(),
                Path::new("/usr/bin"),
                Path::new("/bin"),
            ])?,
        )
        .output()?;
    if !version.status.success()
        || !String::from_utf8_lossy(&version.stdout)
            .lines()
            .any(|line| line == "release: 1.94.0")
        || !String::from_utf8_lossy(&version.stdout)
            .lines()
            .any(|line| line == format!("host: {}", recipe.target))
    {
        return Err("runtime build needs a native Rust 1.94.0 toolchain for its target".into());
    }
    timing.mark("tools_before");
    // Selection is Cargo's job: package/bin choices do not change the native ABI
    // or compiler inputs. Share dependency artifacts between serial build groups
    // rather than storing a complete target tree for each selected binary set.
    // Cargo retains its package/source/feature fingerprints inside this cache.
    let mut cache_recipe = recipe.clone();
    cache_recipe.expected_growth_bytes = 0;
    cache_recipe.packages.clear();
    cache_recipe.binaries.clear();
    let identity = serde_json::to_vec(&(
        "openpilot-native-cargo-cache-v2",
        cache_recipe,
        &compiler_digest,
        &cargo_digest,
        version.stdout,
        &native.executables,
    ))?;
    let key = format!("{:x}", Sha256::digest(&identity));
    fs::create_dir_all(cache)?;
    let cache = cache.canonicalize()?;
    let directory = cache.join(&key);
    fs::create_dir_all(&directory)?;
    if fs::symlink_metadata(&directory)?.file_type().is_symlink() {
        return Err("build cache generation is a symlink".into());
    }
    use std::os::unix::fs::OpenOptionsExt;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(directory.join("build.lock"))?;
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
    let space = rustix::fs::statvfs(&directory)?;
    let available = space
        .f_bavail
        .checked_mul(space.f_frsize)
        .ok_or("space overflow")?;
    if available < recipe.expected_growth_bytes {
        return Err("insufficient space for this build's estimated growth; caches and user data were retained".into());
    }
    let target = directory.join("target");
    let cargo_home = cache.join("cargo-home");
    fs::create_dir_all(&cargo_home)?;
    let mut command = Command::new(&cargo);
    let linker_key = format!(
        "CARGO_TARGET_{}_LINKER",
        recipe.target.to_ascii_uppercase().replace('-', "_")
    );
    let mut flags: Vec<&str> = recipe
        .environment
        .get("RUSTFLAGS")
        .map(|v| v.split_whitespace().collect())
        .unwrap_or_default();
    let linker_flag = format!("linker={}", native.linker.display());
    flags.extend(["-C", &linker_flag]);
    command
        .current_dir(&root)
        .env_clear()
        .env(
            "PATH",
            std::env::join_paths([
                toolchain.as_path(),
                Path::new("/usr/bin"),
                Path::new("/bin"),
            ])?,
        )
        .env("HOME", &cache)
        .env("CARGO_HOME", &cargo_home)
        .env("CARGO_TARGET_DIR", &target)
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_INCREMENTAL", "0")
        .env("RUSTC", &rustc)
        .env("RUSTDOC", toolchain.join("rustdoc"))
        .envs(&recipe.environment)
        .envs(&native.environment)
        .env(linker_key, &native.linker)
        .env("CARGO_ENCODED_RUSTFLAGS", flags.join("\u{1f}"));
    if let Some(vendor) = recipe.environment.get("OPENPILOT_CARGO_VENDOR") {
        command.args([
            "--config",
            "source.crates-io.replace-with=\"openpilot-native\"",
        ]);
        command.args([
            "--config",
            &format!(
                "source.openpilot-native.directory={}",
                serde_json::to_string(vendor)?
            ),
        ]);
    }
    command.args([
        "build",
        "--manifest-path",
        "rust/Cargo.toml",
        "--locked",
        "--offline",
        "--release",
        "--message-format=json-render-diagnostics",
        "--target",
        &recipe.target,
    ]);
    for package in &recipe.packages {
        command.args(["--package", package]);
    }
    for binary in &recipe.binaries {
        command.args(["--bin", binary]);
    }
    if !recipe.features.is_empty() {
        command.args(["--features", &recipe.features.join(",")]);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::inherit());
    timing.mark("cache_and_command");
    let mut child = command.spawn()?;
    let result = (|| -> Result<Vec<Artifact>> {
        let output = child.stdout.take().ok_or("Cargo stdout missing")?;
        let mut artifacts = Vec::new();
        for line in BufReader::new(output).lines() {
            let line = line?;
            let value: serde_json::Value = serde_json::from_str(&line)?;
            if value["reason"] == "compiler-artifact" {
                if let Some(executable) = value["executable"].as_str() {
                    artifacts.push(Artifact {
                        executable: PathBuf::from(executable),
                        fresh: value["fresh"]
                            .as_bool()
                            .ok_or("Cargo fresh status missing")?,
                        sha256: String::new(),
                        bytes: 0,
                    });
                }
            } else if let Some(rendered) = value["message"]["rendered"].as_str() {
                eprint!("{rendered}");
            }
        }
        Ok(artifacts)
    })();
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    let mut artifacts = result?;
    if !status.success() || artifacts.is_empty() {
        return Err(format!(
            "native Cargo build failed: {status}; previous runtime was not changed"
        )
        .into());
    }
    timing.mark("cargo");
    recipe.verify_inputs(&root)?;
    timing.mark("inputs_after");
    if source::inspect(&root)? != source {
        return Err("runtime build source changed during compilation".into());
    }
    timing.mark("source_after");
    if digest_file(&rustc)? != compiler_digest || digest_file(&cargo)? != cargo_digest {
        return Err("Rust compiler or Cargo changed during build".into());
    }
    if native_tools::inspect(recipe, &toolchain)? != native {
        return Err(
            "native C/C++ compiler, linker or helper selection changed during build".into(),
        );
    }
    timing.mark("tools_after");
    for artifact in &mut artifacts {
        if !artifact.executable.canonicalize()?.starts_with(&target) {
            return Err("Cargo executable escapes the build target directory".into());
        }
        artifact.bytes = regular_file(&artifact.executable)?.metadata()?.len();
        artifact.sha256 = digest_file(&artifact.executable)?;
    }
    timing.mark("outputs");
    let report = Report {
        schema: 3,
        cache_key: key,
        target_directory: target,
        artifacts,
        source,
        recipe_sha256: format!("{:x}", Sha256::digest(serde_json::to_vec(recipe)?)),
        rustc_sha256: compiler_digest,
        cargo_sha256: cargo_digest,
        toolchain_directory: toolchain,
        native_tools: native.executables,
        verified_input_trees: recipe
            .input_trees
            .iter()
            .map(|(tree, receipt)| (tree.clone(), recipe.inputs[receipt].clone()))
            .collect(),
        elapsed_ms: started.elapsed().as_millis(),
    };
    let mut saved = tempfile::NamedTempFile::new_in(&directory)?;
    serde_json::to_writer_pretty(saved.as_file_mut(), &report)?;
    saved.as_file_mut().write_all(b"\n")?;
    saved.as_file_mut().sync_all()?;
    saved.persist(directory.join("last-successful-build.json"))?;
    timing.mark("report_persisted");
    Ok(report)
}
