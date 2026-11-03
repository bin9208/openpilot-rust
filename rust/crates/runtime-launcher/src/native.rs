//! Source generations use the existing locked manager handoff, with a separate
//! updater checkout. Legacy overlay activation never touches a sealed runtime.
use crate::{bootstrap::Paths, handoff, Error};
use openpilot_runtime_build::{
    generation::{self, Config},
    machine, pending, selection,
};
use std::{
    fs::File,
    path::{Component, Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

fn error(value: impl std::fmt::Display) -> Error {
    Error::Configuration(value.to_string())
}

#[derive(Debug)]
pub struct Context {
    pub config_path: PathBuf,
    pub requested_checkout: PathBuf,
    pub generation: String,
    config: Config,
    configuration_sha256: String,
    machine_path: Option<PathBuf>,
    machine_launch_pin: Option<std::ffi::OsString>,
    initialize: bool,
}

impl Context {
    pub fn from_arguments(
        config: Option<PathBuf>,
        requested: Option<PathBuf>,
        generation: Option<String>,
    ) -> Result<Option<Self>, Error> {
        Self::from_arguments_mode(config, requested, generation, false)
    }

    pub fn from_arguments_mode(
        config: Option<PathBuf>,
        requested: Option<PathBuf>,
        generation: Option<String>,
        initialize: bool,
    ) -> Result<Option<Self>, Error> {
        let expected = std::env::var_os("OPENPILOT_NATIVE_LAUNCH_CONFIG_SHA256");
        std::env::remove_var("OPENPILOT_NATIVE_LAUNCH_CONFIG_SHA256");
        let machine_launch_pin = std::env::var_os(machine::LAUNCH_PIN);
        std::env::remove_var(machine::LAUNCH_PIN);
        match (config, requested, generation) {
            (None, None, None) if !initialize => Ok(None),
            (Some(config), Some(requested_checkout), Some(generation)) => {
                if !requested_checkout.is_absolute()
                    || requested_checkout
                        .components()
                        .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
                {
                    return Err(error("native requested checkout must be an absolute path"));
                }
                let config_path = config.canonicalize()?;
                let (config, configuration_sha256) =
                    Config::read_pinned(&config_path).map_err(error)?;
                if expected
                    .as_ref()
                    .is_some_and(|pin| pin.to_str() != Some(configuration_sha256.as_str()))
                {
                    return Err(error("native launch configuration changed before handoff"));
                }
                Ok(Some(Self {
                    config_path,
                    requested_checkout,
                    generation,
                    config,
                    configuration_sha256,
                    machine_path: std::env::var_os(machine::ENV).map(PathBuf::from),
                    machine_launch_pin,
                    initialize,
                }))
            }
            _ => Err(error(
                "native startup needs configuration, requested checkout and generation together",
            )),
        }
    }

    pub fn verify(&self, paths: &Paths) -> Result<(), Error> {
        Config::verify_pin(&self.config_path, &self.configuration_sha256).map_err(error)?;
        let selected = if self.initialize {
            let sealed = generation::load(&self.config.stores, &self.generation).map_err(error)?;
            match std::fs::symlink_metadata(self.config.stores.generations.join("active.json")) {
                Ok(_) => {
                    if selection::resolve(&self.config.stores).map_err(error)?.id != self.generation
                    {
                        return Err(error(
                            "initial native request conflicts with the selected generation",
                        ));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
            selection::Selected {
                id: self.generation.clone(),
                source_commit: sealed.runtime.source.commit,
                source_root: sealed.runtime.root,
                recovered_previous: false,
                rejected: vec![],
            }
        } else {
            selection::resolve(&self.config.stores).map_err(error)?
        };
        if selected.id != self.generation
            || selected.source_root != paths.repository
            || self.config.stores.params != paths.data.join("params")
        {
            return Err(error(
                "native launch arguments differ from the actual selected source or Params root",
            ));
        }
        Config::verify_pin(&self.config_path, &self.configuration_sha256).map_err(error)?;
        Ok(())
    }

    /// The boot lock has already been acquired; activation cannot race a live
    /// manager. Failure keeps the journal and resolves the verified old runtime.
    pub fn boot(
        &self,
        paths: &Paths,
        lock: &File,
        stopped: &AtomicBool,
        signals: &mut signal_hook::iterator::Signals,
    ) -> Result<bool, Error> {
        if self.initialize {
            Config::verify_pin(&self.config_path, &self.configuration_sha256).map_err(error)?;
            selection::initialize(&self.config.stores, &self.generation).map_err(error)?;
        }
        if let Err(error) =
            pending::activate_at_boot_pinned(&self.config_path, &self.configuration_sha256)
        {
            eprintln!("native pending update retained without activation: {error}");
        }
        Config::verify_pin(&self.config_path, &self.configuration_sha256).map_err(error)?;
        let mut selected = selection::resolve(&self.config.stores).map_err(error)?;
        match self.prepare_checkout(&selected, stopped) {
            Ok(true) => {
                if let Err(error) =
                    pending::activate_at_boot_pinned(&self.config_path, &self.configuration_sha256)
                {
                    eprintln!("native checkout build retained without activation: {error}");
                }
                selected = selection::resolve(&self.config.stores).map_err(error)?;
            }
            Ok(false) => (),
            Err(error) => {
                eprintln!(
                    "native checkout build failed; keeping verified source {}: {error}",
                    selected.source_commit
                );
                std::env::set_var("OPENPILOT_NATIVE_UPDATE_ERROR", error.to_string());
            }
        }
        if selected.id == self.generation {
            return Ok(false);
        }
        eprintln!(
            "native boot selected actual source {} generation {}",
            selected.source_commit, selected.id
        );
        let mut next = paths.clone();
        next.repository = selected.source_root;
        let temporary = tempfile::NamedTempFile::new_in(&paths.temporary)?;
        serde_json::to_writer(temporary.as_file(), &next)?;
        let mut command = Command::new(next.repository.join("rust/bin/openpilot-launch"));
        if let Some(pin) = &self.machine_launch_pin {
            command.env(machine::LAUNCH_PIN, pin);
        }
        command
            .env(
                "OPENPILOT_NATIVE_LAUNCH_CONFIG_SHA256",
                &self.configuration_sha256,
            )
            .env(
                "LD_LIBRARY_PATH",
                selection::library_path(&self.config.stores, &selected.id).map_err(error)?,
            )
            .arg("--paths")
            .arg(temporary.path())
            .arg("--native-config")
            .arg(&self.config_path)
            .arg("--update-checkout")
            .arg(&self.requested_checkout)
            .arg("--generation")
            .arg(selected.id)
            .env_remove("AGNOS_VERSION")
            .current_dir(&next.repository);
        handoff::run_locked_with_signals(command, lock, stopped, signals)?;
        Ok(true)
    }

    // Git installation/branch changes must build the requested clean checkout
    // before manager startup, just like the original launch/build.py flow.
    // Reuse the updater's existing source-entry CLI and boot transaction.
    fn prepare_checkout(
        &self,
        selected: &selection::Selected,
        stopped: &AtomicBool,
    ) -> Result<bool, Error> {
        let Some(path) = &self.machine_path else {
            return Ok(false);
        };
        let requested = match openpilot_runtime_build::inspect_source(&self.requested_checkout) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("native boot keeps verified source; requested checkout is unavailable or dirty: {error}");
                return Ok(false);
            }
        };
        if requested.commit == selected.source_commit {
            return Ok(false);
        }
        let (policy, pin) = machine::Config::read_pinned(path).map_err(error)?;
        policy.verify_stores(&self.config.stores).map_err(error)?;
        policy
            .verify_source_paths(&self.requested_checkout)
            .map_err(error)?;
        if self
            .machine_launch_pin
            .as_ref()
            .is_some_and(|expected| expected.to_str() != Some(pin.as_str()))
        {
            return Err(error("native machine policy changed before checkout build"));
        }
        Config::verify_pin(&self.config_path, &self.configuration_sha256).map_err(error)?;
        generation::offroad(&self.config.stores.params).map_err(error)?;
        let owner = openpilot_runtime_build::supervision::process_identity(std::process::id())
            .map_err(error)?;
        let mut command = Command::new("/usr/bin/python3");
        command
            .env_remove("LD_LIBRARY_PATH")
            .env_remove("LD_PRELOAD")
            .env_remove("CARROT_BOOT_LOCK_SOCKET")
            .env_remove("CARROT_BOOT_LOCK_FD")
            .args(["-I", "-B", "-S"])
            .arg(self.requested_checkout.join("tools/native/source_entry.py"))
            .arg("--source")
            .arg(&self.requested_checkout)
            .arg("--machine-config")
            .arg(path)
            .args(["--machine-configuration-sha256", &pin])
            .arg("--running-config")
            .arg(&self.config_path)
            .args(["--running-configuration-sha256", &self.configuration_sha256])
            .args(["--previous", &selected.id, "--request-owner", &owner])
            .current_dir(&self.requested_checkout);
        let status = crate::watchdog::Server(command.spawn()?).finish(stopped)?;
        if !status.success() || stopped.load(Ordering::Relaxed) {
            return Err(error(format!(
                "requested checkout build did not complete: {status}"
            )));
        }
        machine::Config::verify_pin(path, &pin).map_err(error)?;
        if openpilot_runtime_build::inspect_source(&self.requested_checkout).map_err(error)?
            != requested
        {
            return Err(error("requested checkout changed during startup build"));
        }
        let pending = pending::ready_pinned(&self.config_path, &self.configuration_sha256)
            .map_err(error)?
            .ok_or_else(|| error("checkout build did not stage a verified result"))?;
        if pending.source_commit != requested.commit || pending.expected_previous != selected.id {
            return Err(error(
                "checkout build staged a different source or previous generation",
            ));
        }
        Ok(true)
    }

    pub fn prepare_updater(&self) -> Result<(), Error> {
        Config::verify_pin(&self.config_path, &self.configuration_sha256).map_err(error)?;
        let preparation = self
            .machine_path
            .as_ref()
            .map(|path| {
                let (policy, pin) = machine::Config::read_pinned(path).map_err(error)?;
                policy.verify_stores(&self.config.stores).map_err(error)?;
                policy
                    .verify_source_paths(&self.requested_checkout)
                    .map_err(error)?;
                if self
                    .machine_launch_pin
                    .as_ref()
                    .is_some_and(|p| p.to_str() != Some(pin.as_str()))
                {
                    return Err(error(
                        "native machine policy changed before startup handoff",
                    ));
                }
                Ok((path, policy, pin))
            })
            .transpose()?;
        if preparation.is_none() && self.machine_launch_pin.is_some() {
            return Err(error("native machine handoff pin has no policy path"));
        }
        let sealed = generation::load(&self.config.stores, &self.generation).map_err(error)?;
        if preparation.is_some() {
            verify_preparation_protocol(&sealed.runtime.root)?;
        }
        let metadata: serde_json::Value =
            serde_json::from_reader(File::open(sealed.runtime.root.join("build.json"))?)?;
        let branch = metadata["channel"]
            .as_str()
            .ok_or_else(|| error("native source channel missing"))?;
        let checkout = sealed
            .prepared
            .source
            .update_checkout(
                &self.config.stores.sources,
                branch,
                sealed.layout.expected_source_growth_bytes,
            )
            .map_err(error)?;
        let updater = preparation.as_ref().map_or_else(
            || self.config.stores.generations.join("updater"),
            |(_, machine, _)| machine.updater_staging.clone(),
        );
        for directory in [&updater, &updater.join(&self.generation)] {
            match std::fs::create_dir(directory) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e.into()),
            }
            if !std::fs::symlink_metadata(directory)?.is_dir() {
                return Err(error("native updater staging is redirected"));
            }
        }
        let staging = updater.join(&self.generation);
        if let Some((path, _, pin)) = preparation {
            machine::Config::verify_pin(path, &pin).map_err(error)?;
        }
        std::env::set_var("OPENPILOT_UPDATE_CHECKOUT", checkout);
        std::env::set_var("UPDATER_STAGING_ROOT", &staging);
        std::env::set_var("UPDATER_LOCK_FILE", staging.join("updater.lock"));
        std::env::remove_var("OPENPILOT_NATIVE_UPDATE_ERROR");
        Config::verify_pin(&self.config_path, &self.configuration_sha256).map_err(error)?;
        Ok(())
    }

    pub fn environment(&self) {
        std::env::set_var("OPENPILOT_NATIVE_CONFIG", &self.config_path);
        std::env::set_var("OPENPILOT_RUNNING_GENERATION", &self.generation);
        if let Some(path) = &self.machine_path {
            std::env::set_var(machine::ENV, path);
        }
        // The input pointers are materialized through pinned content assets by
        // the native preparation plan, rather than an unbounded Git LFS smudge.
        std::env::set_var("GIT_LFS_SKIP_SMUDGE", "1");
        if let Err(error) = self.prepare_updater() {
            eprintln!("native updater unavailable; normal runtime continues: {error}");
            std::env::set_var("OPENPILOT_NATIVE_UPDATE_ERROR", error.to_string());
            std::env::remove_var("OPENPILOT_UPDATE_CHECKOUT");
        }
    }
}

/// Capability negotiation checks the source-built updater too. Full startup and
/// update behavior are validated separately; this is not a device-boot probe.
pub fn protocol(root: &Path) -> Result<(), Error> {
    let output = Command::new(root.join("rust/bin/openpilot-updated"))
        .arg("--verify-native-generation-protocol")
        .current_dir(root)
        .output()?;
    if !output.status.success()
        || serde_json::from_slice::<serde_json::Value>(&output.stdout)?
            != serde_json::json!({"source_generation_update_protocol":1})
    {
        return Err(error(
            "source-built updater does not support native generation handoff",
        ));
    }
    println!(
        "{}",
        serde_json::json!({"source_generation_protocol":1,"startup":true,"updates":true})
    );
    Ok(())
}

pub fn verify_preparation_protocol(root: &Path) -> Result<(), Error> {
    let output = Command::new(root.join("rust/bin/openpilot-updated"))
        .arg("--verify-native-source-preparation-protocol")
        .current_dir(root)
        .output()?;
    if !output.status.success()
        || serde_json::from_slice::<serde_json::Value>(&output.stdout)?
            != serde_json::json!({"source_preparation_update_protocol":1})
    {
        return Err(error(
            "source-built updater does not support current-source input preparation",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod checkout_build_tests {
    use super::*;
    use std::{collections::BTreeMap, fs};

    fn git(root: &Path, args: &[&str]) -> String {
        let output = Command::new("/usr/bin/git")
            .args([
                "-c",
                "user.name=Owned boot build",
                "-c",
                "user.email=boot@example.invalid",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
                "-C",
            ])
            .arg(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().into()
    }

    #[test]
    fn real_checkout_cli_runs_only_for_clean_changes_and_cannot_publish_failure_or_unstaged_success(
    ) {
        use openpilot_runtime_build::{
            catalog_bindings::Budgets,
            generation::Stores,
            resources::{DiskBudget, Policy},
        };
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let source = root.join("source");
        fs::create_dir_all(source.join("tools/native")).unwrap();
        let marker = root.join("arguments.json");
        let script = source.join("tools/native/source_entry.py");
        fs::write(&script, format!("import json,sys\nfrom pathlib import Path\nPath({marker:?}).write_text(json.dumps(sys.argv[1:]))\nsys.exit(7)\n")).unwrap();
        fs::write(source.join("README"), "initial source").unwrap();
        git(&source, &["init", "-q"]);
        git(&source, &["add", "."]);
        git(&source, &["commit", "-qm", "initial"]);
        let before = git(&source, &["rev-parse", "HEAD"]);
        let config = Config {
            schema: 1,
            stores: Stores {
                sources: root.join("sources"),
                objects: root.join("objects"),
                generations: root.join("generations"),
                params: root.join("params"),
            },
            cache: root.join("cache"),
            toolchain: root.join("toolchain"),
            build_plan: root.join("plan.json"),
            layout: root.join("layout.json"),
            resources: Policy {
                schema: 1,
                memory_cgroup_parent: root.join("not-a-controller"),
                maximum_memory_bytes: 1,
                minimum_available_memory_bytes: 1,
                filesystems: BTreeMap::from([(
                    root.to_owned(),
                    DiskBudget {
                        maximum_growth_bytes: 1,
                        minimum_free_bytes: 1,
                    },
                )]),
            },
        };
        fs::create_dir_all(config.stores.params.join("d")).unwrap();
        fs::write(config.stores.params.join("d/IsOnroad"), b"0").unwrap();
        fs::create_dir_all(&config.stores.generations).unwrap();
        let active = config.stores.generations.join("active.json");
        fs::write(&active, b"previous selection sentinel").unwrap();
        let config_path = root.join("config.json");
        fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
        let (_, configuration_sha256) = Config::read_pinned(&config_path).unwrap();
        let policy = machine::Config {
            schema: 1,
            stores: config.stores.clone(),
            cache: config.cache.clone(),
            seed_cache: root.join("seed"),
            updater_staging: root.join("updates"),
            expected_source_growth_bytes: 1,
            expected_compile_growth_bytes: 1,
            remaining_object_growth_bytes: 1,
            compiler_growth_bytes: 1,
            resources: config.resources.clone(),
            budgets: Budgets {
                schema: 1,
                group_growth_bytes: BTreeMap::from([("00".into(), 1)]),
                expected_source_growth_bytes: 1,
                remaining_object_growth_bytes: 1,
                estimate_basis: "Owned failure-only CLI test; no native build/resource acceptance"
                    .into(),
            },
            boot_seed: None,
        };
        let machine_path = root.join("machine.json");
        fs::write(&machine_path, serde_json::to_vec(&policy).unwrap()).unwrap();
        let (_, pin) = machine::Config::read_pinned(&machine_path).unwrap();
        let context = Context {
            config_path,
            requested_checkout: source.clone(),
            generation: "a".repeat(64),
            config,
            configuration_sha256,
            machine_path: Some(machine_path),
            machine_launch_pin: Some(pin.clone().into()),
            initialize: false,
        };
        let selected = selection::Selected {
            id: context.generation.clone(),
            source_commit: before,
            source_root: root.join("previous"),
            recovered_previous: false,
            rejected: vec![],
        };
        let stopped = AtomicBool::new(false);
        assert!(!context.prepare_checkout(&selected, &stopped).unwrap());
        assert!(!marker.exists());
        fs::write(source.join("README"), "uncommitted change").unwrap();
        assert!(!context.prepare_checkout(&selected, &stopped).unwrap());
        assert!(!marker.exists());
        git(&source, &["add", "."]);
        git(&source, &["commit", "-qm", "changed checkout"]);
        let failure = context
            .prepare_checkout(&selected, &stopped)
            .unwrap_err()
            .to_string();
        assert!(failure.contains("build did not complete"), "{failure}");
        let args: Vec<String> = serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--previous", selected.id.as_str()]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--machine-configuration-sha256", pin.as_str()]));
        assert!(!args.iter().any(|arg| arg == "--launch"));
        // Exit 0 alone must not become activation authority.
        fs::write(&script, "import sys\nsys.exit(0)\n").unwrap();
        git(&source, &["add", "."]);
        git(&source, &["commit", "-qm", "exit without staged result"]);
        let failure = context
            .prepare_checkout(&selected, &stopped)
            .unwrap_err()
            .to_string();
        assert!(
            failure.contains("did not stage a verified result"),
            "{failure}"
        );
        assert_eq!(fs::read(active).unwrap(), b"previous selection sentinel");
        assert!(!context
            .config
            .stores
            .generations
            .join("pending.json")
            .exists());
    }
}
