//! The pinned seed prepares and builds the current tools inside the same resource
//! and cancellation boundary as a full generation. No runtime is selected here.
use crate::{
    assets, cargo_inputs,
    generation::{self, Stores},
    memory_cgroup,
    prepare::{self, Prepared},
    resources::{self, Policy},
    source, supervision, system_sdk, toolchain, Recipe, Result,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Read,
    os::unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Component, Path, PathBuf},
    process::Command,
};

const TOOLS: [&str; 12] = [
    "openpilot-runtime-build",
    "openpilot-runtime-assets",
    "openpilot-runtime-inputs",
    "openpilot-runtime-source",
    "openpilot-runtime-prepare",
    "openpilot-runtime-native",
    "openpilot-native-build-scope",
    "openpilot-runtime-plan",
    "build-runtime-ffmpeg",
    "stage-startup-ui-raylib",
    "build-runtime-opencv",
    "runtime-elf-audit",
];
const ATTEMPT: &str = "OPENPILOT_NATIVE_BOOTSTRAP_ATTEMPT";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: u32,
    pub stores: Stores,
    pub cache: PathBuf,
    /// Independently authenticated factory snapshot; never capture the live `/`.
    pub system_sdk_receipt: PathBuf,
    pub system_sdk_sha256: String,
    pub expected_source_growth_bytes: u64,
    pub expected_compile_growth_bytes: u64,
    pub remaining_object_growth_bytes: u64,
    pub resources: Policy,
}

fn absolute(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
}

impl Config {
    pub fn read(path: &Path) -> Result<Self> {
        Ok(Self::read_pinned(path)?.0)
    }

    fn read_pinned(path: &Path) -> Result<(Self, String)> {
        const LIMIT: u64 = 4 * 1024 * 1024;
        let file = crate::regular_file(path)?;
        if file.metadata()?.len() > LIMIT {
            return Err("bootstrap configuration exceeds 4 MiB".into());
        }
        let mut bytes = Vec::new();
        file.take(LIMIT + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > LIMIT {
            return Err("bootstrap configuration exceeds 4 MiB".into());
        }
        let config: Self = serde_json::from_slice(&bytes)?;
        config.resources.validate()?;
        let paths = [
            &config.cache,
            &config.stores.sources,
            &config.stores.objects,
            &config.stores.generations,
            &config.stores.params,
        ];
        if config.schema != 1
            || !absolute(&config.system_sdk_receipt)
            || paths.iter().any(|p| !absolute(p))
            || paths.iter().enumerate().any(|(i, a)| {
                paths
                    .iter()
                    .skip(i + 1)
                    .any(|b| a.starts_with(b) || b.starts_with(a))
            })
            || config.system_sdk_sha256.len() != 64
            || !config
                .system_sdk_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(
                "bootstrap needs separate absolute stores and an independent SDK SHA pin".into(),
            );
        }
        Ok((config, format!("{:x}", Sha256::digest(&bytes))))
    }

    fn verify_sdk(&self, root: &Path) -> Result<()> {
        const LIMIT: u64 = 16 * 1024 * 1024;
        let file = crate::regular_file(&self.system_sdk_receipt)?;
        if file.metadata()?.len() > LIMIT {
            return Err("bootstrap SDK receipt exceeds 16 MiB".into());
        }
        let mut bytes = Vec::new();
        file.take(LIMIT + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > LIMIT
            || format!("{:x}", Sha256::digest(&bytes)) != self.system_sdk_sha256
        {
            return Err("bootstrap system SDK receipt differs from its independent pin".into());
        }
        let sdk: system_sdk::Snapshot = serde_json::from_slice(&bytes)?;
        if sdk.plan != system_sdk::Plan::read(&root.join("rust/native-system-sdk.json"))? {
            return Err("bootstrap SDK profile differs from the requested source".into());
        }
        sdk.verify(Path::new("/"))
    }

    fn directories(&self, root: &Path) -> Result<()> {
        for path in [
            &self.cache,
            &self.stores.sources,
            &self.stores.objects,
            &self.stores.generations,
        ] {
            if path.starts_with(root) || root.starts_with(path) {
                return Err(
                    "bootstrap writable stores must be outside the requested checkout".into(),
                );
            }
            generation::directory(path)?;
            if path.canonicalize()? != *path {
                return Err("bootstrap writable store is redirected".into());
            }
        }
        Ok(())
    }

    fn plan(&self, rust: &toolchain::Prepared, cargo: &cargo_inputs::Prepared) -> prepare::Plan {
        let inputs = BTreeMap::from([
            (
                self.system_sdk_receipt.clone(),
                self.system_sdk_sha256.clone(),
            ),
            (rust.tree_receipt.clone(), rust.tree_sha256.clone()),
            (cargo.tree_receipt.clone(), cargo.tree_sha256.clone()),
        ]);
        let trees = BTreeMap::from([
            (PathBuf::from("/"), self.system_sdk_receipt.clone()),
            (rust.root.clone(), rust.tree_receipt.clone()),
            (cargo.root.clone(), cargo.tree_receipt.clone()),
        ]);
        prepare::Plan {
            schema: 1,
            groups: BTreeMap::from([(
                "bootstrap-tools".into(),
                Recipe {
                    schema: 1,
                    target: "aarch64-unknown-linux-gnu".into(),
                    packages: vec![
                        "openpilot-runtime-build".into(),
                        "openpilot-dev-tools".into(),
                    ],
                    binaries: TOOLS.iter().map(|s| (*s).into()).collect(),
                    features: [
                        "prepare-rust",
                        "prepare-capnp",
                        "prepare-cargo",
                        "prepare-source-assets",
                        "prepare-python",
                    ]
                    .iter()
                    .map(|s| format!("openpilot-runtime-build/{s}"))
                    .collect(),
                    inputs,
                    input_trees: trees,
                    environment: BTreeMap::from([
                        (
                            "OPENPILOT_CARGO_VENDOR".into(),
                            cargo.vendor.to_string_lossy().into_owned(),
                        ),
                        ("OPENPILOT_NATIVE_INCREMENTAL".into(), "1".into()),
                        (
                            "TMPDIR".into(),
                            self.cache.join("tmp").to_string_lossy().into_owned(),
                        ),
                    ]),
                    expected_growth_bytes: self.expected_compile_growth_bytes,
                },
            )]),
            expected_source_growth_bytes: self.expected_source_growth_bytes,
            remaining_object_growth_bytes: self.remaining_object_growth_bytes,
            runtime_inputs: prepare::RuntimeInputs::default(),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WorkerResult {
    schema: u32,
    configuration_sha256: String,
    seed_helper_sha256: String,
    preparations: serde_json::Value,
    prepared: Prepared,
}

fn worker(
    config: &Config,
    config_path: &Path,
    root: &Path,
    configuration_sha256: &str,
) -> Result<()> {
    let _guard = supervision::WorkerGuard::start(&config.stores.params)?;
    memory_cgroup::enter(
        &config.resources.memory_cgroup_parent,
        config.resources.maximum_memory_bytes,
    )?;
    generation::agnos_identity()?;
    config.verify_sdk(root)?;
    let source_before = source::inspect(root)?;
    let attempt = PathBuf::from(std::env::var_os(ATTEMPT).ok_or("bootstrap attempt missing")?);
    if attempt.canonicalize()? != attempt
        || attempt.parent()
            != Some(
                config
                    .stores
                    .generations
                    .join("bootstrap-attempts")
                    .as_path(),
            )
        || !attempt
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with("attempt-"))
    {
        return Err("bootstrap worker attempt is outside its configuration".into());
    }
    let rust = toolchain::prepare(
        &toolchain::Plan::read(&root.join("rust/native-rust-toolchain.json"))?,
        &config.cache.join("rust-assets"),
        &config.cache.join("rust-prefixes"),
        &assets::Curl,
    )?;
    let cargo = cargo_inputs::prepare(
        &cargo_inputs::Plan::read(&root.join("rust/native-cargo-inputs.json"))?,
        &root.join("rust/Cargo.lock"),
        &config.cache.join("cargo-assets"),
        &config.cache.join("cargo-prefixes"),
        &assets::Curl,
    )?;
    let plan = config.plan(&rust, &cargo);
    system_sdk::generation_profiles(root, &plan)?;
    let prepared = prepare::run(
        root,
        &config.cache.join("cargo-build"),
        &rust.root.join("bin"),
        &config.stores.sources,
        &config.stores.objects,
        &plan,
    )?;
    if source::inspect(root)? != source_before {
        return Err("bootstrap source changed during tool preparation".into());
    }
    config.verify_sdk(root)?;
    if crate::digest_file(config_path)? != configuration_sha256 {
        return Err("bootstrap configuration changed during worker execution".into());
    }
    generation::write(
        &attempt.join("worker-result.json"),
        &WorkerResult {
            schema: 1,
            configuration_sha256: configuration_sha256.into(),
            seed_helper_sha256: crate::digest_file(&std::env::current_exe()?)?,
            preparations: serde_json::json!({"rust":rust,"cargo":cargo}),
            prepared,
        },
        false,
    )
}

fn bundle_key(prepared: &Prepared) -> Result<String> {
    let identities: Vec<_> = prepared
        .reports
        .values()
        .map(|r| {
            (
                &r.recipe_sha256,
                &r.rustc_sha256,
                &r.cargo_sha256,
                &r.native_tools,
                &r.verified_input_trees,
            )
        })
        .collect();
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            "openpilot-native-bootstrap-tools-v1",
            &prepared.binaries,
            identities
        ))?)
    ))
}

fn bundle(config: &Config, current: &WorkerResult) -> Result<(PathBuf, WorkerResult)> {
    let key = bundle_key(&current.prepared)?;
    let parent = config.cache.join("tools");
    generation::directory(&parent)?;
    let destination = parent.join(key);
    if !destination.try_exists()? {
        let stage = tempfile::Builder::new()
            .prefix(".pending-tools-")
            .tempdir_in(&parent)?;
        for (name, object) in &current.prepared.binaries {
            object.materialize(&config.stores.objects, &stage.path().join(name))?;
        }
        generation::write(&stage.path().join("manifest.json"), current, false)?;
        fs::set_permissions(
            stage.path().join("manifest.json"),
            fs::Permissions::from_mode(0o444),
        )?;
        std::fs::File::open(stage.path().join("manifest.json"))?.sync_all()?;
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            stage.path(),
            rustix::fs::CWD,
            &destination,
            rustix::fs::RenameFlags::NOREPLACE,
        )?;
        std::fs::File::open(&parent)?.sync_all()?;
    }
    if destination.canonicalize()? != destination || !fs::symlink_metadata(&destination)?.is_dir() {
        return Err("bootstrap tool bundle is redirected".into());
    }
    let origin: WorkerResult = generation::read(&destination.join("manifest.json"))?;
    origin
        .prepared
        .verify(&config.stores.sources, &config.stores.objects)?;
    if origin.schema != 1
        || origin.prepared.binaries != current.prepared.binaries
        || destination.file_name() != Some(std::ffi::OsStr::new(&bundle_key(&origin.prepared)?))
        || crate::regular_file(&destination.join("manifest.json"))?
            .metadata()?
            .permissions()
            .mode()
            & 0o7777
            != 0o444
    {
        return Err("bootstrap bundle differs from the verified build outputs".into());
    }
    let expected: BTreeSet<_> = TOOLS
        .iter()
        .map(|name| OsString::from(*name))
        .chain(["manifest.json".into()])
        .collect();
    let actual = fs::read_dir(&destination)?
        .map(|entry| entry.map(|e| e.file_name()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    if actual != expected {
        return Err("bootstrap tool bundle inventory differs".into());
    }
    for (name, object) in &origin.prepared.binaries {
        let path = destination.join(name);
        let metadata = crate::regular_file(&path)?.metadata()?;
        if metadata.len() != object.bytes
            || metadata.permissions().mode() & 0o7777 != 0o555
            || crate::digest_file(&path)? != object.sha256
        {
            return Err("bootstrap materialized tool differs from its immutable object".into());
        }
    }
    Ok((destination, origin))
}

fn frontend(
    config: &Config,
    config_path: &Path,
    root: &Path,
    configuration_sha256: &str,
) -> Result<()> {
    memory_cgroup::verify_control(&config.resources.memory_cgroup_parent)?;
    if !rustix::process::getgroups()?.is_empty() {
        return Err("bootstrap frontend retained supplementary groups".into());
    }
    generation::offroad(&config.stores.params)?;
    let before = source::inspect(root)?;
    config.directories(root)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(config.cache.join("bootstrap-tools.lock"))?;
    if !lock.metadata()?.is_file() {
        return Err("bootstrap lock is not regular".into());
    }
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
    let attempts = config.stores.generations.join("bootstrap-attempts");
    generation::directory(&attempts)?;
    let attempt = tempfile::Builder::new()
        .prefix("attempt-")
        .tempdir_in(&attempts)?
        .keep();
    let temporary = config.cache.join("tmp");
    generation::directory(&temporary)?;
    let mut monitor = resources::Monitor::start(
        &config.resources,
        &[
            &config.cache,
            &config.stores.sources,
            &config.stores.objects,
            &config.stores.generations,
            &temporary,
        ],
        &attempt.join("resources.jsonl"),
        serde_json::json!({"phase":"native-bootstrap-tools","requested_source":before,"configuration_sha256":configuration_sha256,
            "frontend_uid":rustix::process::getuid().as_raw(),"supplementary_groups_cleared":true}),
    )?;
    let mut command = Command::new(std::env::current_exe()?);
    let output_path = attempt.join("build.log");
    let output = supervision::retain_output(&mut command, &output_path)?;
    command
        .env(memory_cgroup::ENV, &monitor.group.path)
        .env("TMPDIR", &temporary)
        .env(ATTEMPT, &attempt)
        .arg("--bootstrap-tools-worker")
        .arg("--config")
        .arg(config_path)
        .arg("--configuration-sha256")
        .arg(configuration_sha256)
        .arg("--root")
        .arg(root);
    let result = supervision::run_observed(command, || monitor.observe());
    let success = result.as_ref().is_ok_and(|s| s.success());
    let reason = result.as_ref().err().map(|e| e.to_string()).or_else(|| {
        result
            .as_ref()
            .ok()
            .filter(|s| !s.success())
            .map(|s| s.to_string())
    });
    let finished = monitor.finish(success, reason);
    output.sync_all()?;
    let status = result?;
    finished?;
    if !status.success() {
        eprintln!(
            "{}",
            String::from_utf8_lossy(&supervision::output_tail(&output_path)?)
        );
        return Err(format!(
            "native bootstrap worker failed: {status}; evidence {}",
            attempt.display()
        )
        .into());
    }
    let current: WorkerResult = generation::read(&attempt.join("worker-result.json"))?;
    if current.schema != 1
        || current.configuration_sha256 != configuration_sha256
        || crate::digest_file(config_path)? != configuration_sha256
        || current.seed_helper_sha256 != crate::digest_file(&std::env::current_exe()?)?
        || current.prepared.source.source != before
        || source::inspect(root)? != before
    {
        return Err("bootstrap result differs from its source, seed or configuration".into());
    }
    current
        .prepared
        .verify(&config.stores.sources, &config.stores.objects)?;
    supervision::check_request_owner()?;
    generation::offroad(&config.stores.params)?;
    let (path, origin) = bundle(config, &current)?;
    if crate::digest_file(config_path)? != configuration_sha256 {
        return Err("bootstrap configuration changed during bundle publication".into());
    }
    println!(
        "{}",
        serde_json::json!({"schema":1,"tools":path,"resource_evidence":attempt,"configuration_sha256":configuration_sha256,
        "requested_source":before,"bundle_original_verified_source":origin.prepared.source.source,
        "build":current.prepared,"preparations":current.preparations,"seed_helper_sha256":current.seed_helper_sha256,
        "runtime_activated":false,"runtime_started":false,"runtime_release_downloaded":false})
    );
    Ok(())
}

pub fn run(arguments: Vec<OsString>) -> Result<()> {
    let mut args = arguments.into_iter();
    let mode = args.next().ok_or("bootstrap mode missing")?;
    let (mut config_path, mut root, mut owner, mut pin) = (None, None, None, None);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--config") if config_path.is_none() => {
                config_path = Some(PathBuf::from(
                    args.next().ok_or("bootstrap configuration missing")?,
                ))
            }
            Some("--root") if root.is_none() => {
                root = Some(PathBuf::from(
                    args.next().ok_or("bootstrap source missing")?,
                ))
            }
            Some("--request-owner") if owner.is_none() => {
                owner = Some(args.next().ok_or("bootstrap request owner missing")?)
            }
            Some("--configuration-sha256") if pin.is_none() => {
                pin = Some(
                    args.next()
                        .ok_or("bootstrap configuration pin missing")?
                        .into_string()
                        .map_err(|_| "invalid bootstrap configuration pin")?,
                );
            }
            _ => return Err("unknown or duplicate bootstrap argument".into()),
        }
    }
    let config_path = config_path
        .ok_or("bootstrap --config is required")?
        .canonicalize()?;
    let (config, configuration_sha256) = Config::read_pinned(&config_path)?;
    if mode == "--bootstrap-tools-worker" && pin.is_none() {
        return Err("bootstrap worker requires its frontend configuration pin".into());
    }
    if pin.is_some_and(|pin| pin != configuration_sha256) {
        return Err("bootstrap configuration changed before the requested phase".into());
    }
    let root = root.ok_or("bootstrap --root is required")?.canonicalize()?;
    match mode.to_str() {
        Some("--bootstrap-tools-scope") => {
            let owner = owner.ok_or("bootstrap scope requires its original request owner")?;
            let uid = std::env::var("SUDO_UID")?.parse::<u32>()?;
            let gid = std::env::var("SUDO_GID")?.parse::<u32>()?;
            let control =
                memory_cgroup::prepare_scope(&config.resources.memory_cgroup_parent, uid, gid)?;
            memory_cgroup::enter_control(&control)?;
            let parent = rustix::process::getppid().ok_or("bootstrap scope parent missing")?;
            let error = Command::new(std::env::current_exe()?)
                .uid(uid)
                .gid(gid)
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .env("HOME", &config.cache)
                .env(
                    "OPENPILOT_NATIVE_SCOPE_PARENT",
                    parent.as_raw_nonzero().get().to_string(),
                )
                .env(supervision::REQUEST_OWNER, owner)
                .arg("--bootstrap-tools")
                .arg("--config")
                .arg(config_path)
                .arg("--configuration-sha256")
                .arg(&configuration_sha256)
                .arg("--root")
                .arg(root)
                .exec();
            Err(error.into())
        }
        Some("--bootstrap-tools") if owner.is_none() => {
            frontend(&config, &config_path, &root, &configuration_sha256)
        }
        Some("--bootstrap-tools-worker") if owner.is_none() => {
            worker(&config, &config_path, &root, &configuration_sha256)
        }
        _ => Err("unknown or mixed bootstrap mode".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(root: &Path) -> Config {
        Config {
            schema: 1,
            stores: Stores {
                sources: root.join("sources"),
                objects: root.join("objects"),
                generations: root.join("generations"),
                params: root.join("params"),
            },
            cache: root.join("cache"),
            system_sdk_receipt: root.join("factory-sdk.json"),
            system_sdk_sha256: "a".repeat(64),
            expected_source_growth_bytes: 1,
            expected_compile_growth_bytes: 1,
            remaining_object_growth_bytes: 1,
            resources: Policy {
                schema: 1,
                memory_cgroup_parent: root.join("ordinary-not-controller"),
                maximum_memory_bytes: 1,
                minimum_available_memory_bytes: 1,
                filesystems: BTreeMap::from([(
                    root.to_owned(),
                    resources::DiskBudget {
                        maximum_growth_bytes: 1,
                        minimum_free_bytes: 1,
                    },
                )]),
            },
        }
    }

    #[test]
    fn overlapping_stores_and_unpinned_sdk_cannot_enter_bootstrap() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json");
        let mut config = fixture(temp.path());
        generation::write(&path, &config, true).unwrap();
        Config::read(&path).unwrap();
        config.stores.objects = config.cache.join("objects");
        generation::write(&path, &config, true).unwrap();
        assert!(Config::read(&path).is_err());
        config = fixture(temp.path());
        config.system_sdk_sha256.clear();
        generation::write(&path, &config, true).unwrap();
        assert!(Config::read(&path).is_err());
    }

    #[test]
    fn fake_controller_never_falls_back_to_unbounded_first_tool_build() {
        let temp = tempfile::tempdir().unwrap();
        let config = fixture(temp.path());
        fs::create_dir(&config.resources.memory_cgroup_parent).unwrap();
        fs::write(temp.path().join("previous-runtime"), b"preserved runtime").unwrap();
        fs::write(temp.path().join("previous-log"), b"preserved log").unwrap();
        let path = temp.path().join("config.json");
        generation::write(&path, &config, false).unwrap();
        assert!(frontend(
            &config,
            &path,
            temp.path(),
            &crate::digest_file(&path).unwrap()
        )
        .is_err());
        assert!(!config.cache.exists());
        assert!(!config.stores.generations.exists());
        assert_eq!(
            fs::read(temp.path().join("previous-runtime")).unwrap(),
            b"preserved runtime"
        );
        assert_eq!(
            fs::read(temp.path().join("previous-log")).unwrap(),
            b"preserved log"
        );
    }

    #[test]
    fn factory_sdk_digest_is_checked_before_parsing_or_compiler_execution() {
        let temp = tempfile::tempdir().unwrap();
        let config = fixture(temp.path());
        fs::write(&config.system_sdk_receipt, b"corrupt untrusted receipt").unwrap();
        let error = config.verify_sdk(temp.path()).unwrap_err().to_string();
        assert!(error.contains("independent pin"));
        assert!(!config.cache.exists());
    }

    #[test]
    fn changed_configuration_is_rejected_before_entering_the_next_phase() {
        let temp = tempfile::tempdir().unwrap();
        let mut config = fixture(temp.path());
        let path = temp.path().join("config.json");
        generation::write(&path, &config, false).unwrap();
        let (_, pin) = Config::read_pinned(&path).unwrap();
        config.expected_compile_growth_bytes += 1;
        generation::write(&path, &config, true).unwrap();
        let error = run(vec![
            "--bootstrap-tools".into(),
            "--config".into(),
            path.into_os_string(),
            "--configuration-sha256".into(),
            pin.into(),
            "--root".into(),
            temp.path().as_os_str().to_owned(),
        ])
        .unwrap_err()
        .to_string();
        assert!(error.contains("configuration changed"));
        assert!(!config.cache.exists());
        assert!(!config.stores.generations.exists());
    }
}
