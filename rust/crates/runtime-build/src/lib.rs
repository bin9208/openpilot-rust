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
pub mod boot;
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
mod file_lock;
pub use file_lock::FileLock;
pub mod generation;
pub mod inputs;
pub mod machine;
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
pub mod user_build;
pub use source::{inspect as inspect_source, Source};

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

/// Actual Cargo messages for callers which need feature/profile provenance.
/// Kept separate from the persisted schema-3 runtime build report so existing
/// generation readers continue to accept that report unchanged.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CargoArtifactMetadata {
    pub executable: PathBuf,
    pub features: Vec<String>,
    pub profile: serde_json::Value,
    pub target: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct BuildWithMetadata {
    pub build: Report,
    pub cargo_artifacts: Vec<CargoArtifactMetadata>,
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

// serde_json does not buffer its reader. Keep file reads in blocks, especially
// when the native build runs under PRoot, while callers retain their byte limits.
fn read_json<T: serde::de::DeserializeOwned>(reader: impl Read) -> serde_json::Result<T> {
    serde_json::from_reader(BufReader::new(reader))
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
        let recipe: Self = crate::read_json(file)?;
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
        self.verify_vendor(root)
    }

    fn verify_vendor(&self, root: &Path) -> Result<()> {
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
            let metadata: serde_json::Value = crate::read_json(metadata.take(4 * 1024 * 1024 + 1))?;
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
    Ok(build_with_metadata(root, cache, toolchain, recipe)?.build)
}

pub fn build_with_metadata(
    root: &Path,
    cache: &Path,
    toolchain: &Path,
    recipe: &Recipe,
) -> Result<BuildWithMetadata> {
    build_inner(root, cache, toolchain, recipe, true)
}

// Only prepare::from_retained uses this, between its complete plan input checks.
// Standalone build callers still verify their complete inputs before and after Cargo.
pub(crate) fn build_with_plan_inputs(
    root: &Path,
    cache: &Path,
    toolchain: &Path,
    recipe: &Recipe,
) -> Result<Report> {
    Ok(build_inner(root, cache, toolchain, recipe, false)?.build)
}

fn build_inner(
    root: &Path,
    cache: &Path,
    toolchain: &Path,
    recipe: &Recipe,
    verify_inputs: bool,
) -> Result<BuildWithMetadata> {
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
    if verify_inputs {
        recipe.verify_inputs(&root)?;
    }
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
    // Cargo tracks package/bin/feature selection inside a target directory.
    // Keep compiler, input and environment changes in the ABI cache identity,
    // while allowing serial feature selections to reuse unchanged dependencies.
    let mut cache_recipe = recipe.clone();
    cache_recipe.expected_growth_bytes = 0;
    cache_recipe.packages.clear();
    cache_recipe.binaries.clear();
    cache_recipe.features.clear();
    // The source build path uses Cargo's ordinary optimized release settings.
    // Whole-program ThinLTO and one codegen unit make each of the 99 small
    // executables relink its dependency graph on the device. Keep opt-level=3
    // and the runtime checks/panic policy; final distribution builds retain the
    // workspace's separately selected release profile.
    let native_profile = if recipe
        .environment
        .get("OPENPILOT_NATIVE_INCREMENTAL")
        .map(String::as_str)
        == Some("1")
    {
        BTreeMap::from([
            ("CARGO_PROFILE_RELEASE_LTO", "false"),
            ("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "16"),
        ])
    } else {
        BTreeMap::new()
    };
    let identity = serde_json::to_vec(&(
        "openpilot-native-cargo-cache-v3",
        cache_recipe,
        &native_profile,
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
        .env("CARGO_INCREMENTAL", "0")
        // Bound concurrent compiler/linker processes independently of rustc's
        // CPU affinity. Device builds share limited memory with normal services.
        .env(
            "CARGO_BUILD_JOBS",
            if native_profile.is_empty() { "4" } else { "2" },
        )
        .env("RUSTC", &rustc)
        .env("RUSTDOC", toolchain.join("rustdoc"))
        .envs(&recipe.environment)
        .envs(&native_profile)
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
    let mut cargo_artifacts = Vec::new();
    let result = (|| -> Result<Vec<Artifact>> {
        let output = child.stdout.take().ok_or("Cargo stdout missing")?;
        let mut artifacts = Vec::new();
        for line in BufReader::new(output).lines() {
            let line = line?;
            let value: serde_json::Value = serde_json::from_str(&line)?;
            if value["reason"] == "compiler-artifact" {
                if let Some(executable) = value["executable"].as_str() {
                    cargo_artifacts.push(CargoArtifactMetadata {
                        executable: PathBuf::from(executable),
                        features: serde_json::from_value(value["features"].clone())?,
                        profile: value["profile"].clone(),
                        target: value["target"].clone(),
                    });
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
    if verify_inputs {
        recipe.verify_inputs(&root)?;
    }
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
    Ok(BuildWithMetadata {
        build: report,
        cargo_artifacts,
    })
}

#[cfg(test)]
mod json_read_tests {
    use super::read_json;
    use std::{cell::Cell, io::Read};

    struct Counted<'a> {
        bytes: &'a [u8],
        reads: &'a Cell<usize>,
    }

    impl Read for Counted<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            self.reads.set(self.reads.get() + 1);
            self.bytes.read(buffer)
        }
    }

    #[test]
    fn buffered_json_keeps_values_and_avoids_byte_reads() {
        // Match the observed SDK receipt size without a timing-sensitive gate.
        let bytes =
            serde_json::to_vec(&serde_json::json!({"receipt": "x".repeat(4_552_000)})).unwrap();
        let parse = |buffered: bool| {
            let reads = Cell::new(0);
            let input = Counted {
                bytes: &bytes,
                reads: &reads,
            };
            let value: serde_json::Value = if buffered {
                read_json(input).unwrap()
            } else {
                serde_json::from_reader(input).unwrap()
            };
            (value, reads.get())
        };
        let (legacy, legacy_reads) = parse(false);
        let (buffered, buffered_reads) = parse(true);
        assert_eq!(legacy, buffered);
        assert!(legacy_reads >= bytes.len());
        assert!(buffered_reads <= bytes.len().div_ceil(8192) + 2);
        eprintln!(
            "JSON bytes={} legacy_reads={legacy_reads} buffered_reads={buffered_reads}",
            bytes.len()
        );
    }

    #[test]
    fn buffered_json_keeps_limits_trailing_data_and_read_errors() {
        let valid = br#"{"schema":1}"#;
        assert!(read_json::<serde_json::Value>(valid.take(4)).is_err());
        for bytes in [br#"{"schema":1} {}"#.as_slice(), br#"{"schema":1"#, b"\xff"] {
            assert!(read_json::<serde_json::Value>(bytes).is_err());
        }
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("fixture read failure"))
            }
        }
        assert!(read_json::<serde_json::Value>(Broken).unwrap_err().is_io());
    }
}
