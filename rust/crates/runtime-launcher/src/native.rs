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
    sync::atomic::AtomicBool,
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
        let selected = selection::resolve(&self.config.stores).map_err(error)?;
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
