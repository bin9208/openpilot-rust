//! The native source-build route stages a verified generation, without invoking
//! the legacy release package downloader or replacing a live source checkout.
use crate::{paths::Paths, process::Commands, Error};
use openpilot_runtime_build::{
    generation::{self, Config},
    machine, pending, selection, supervision,
};
use std::{env, fs::File, path::PathBuf};

fn error(value: impl std::fmt::Display) -> Error {
    Error::Native(value.to_string())
}

pub struct Context {
    path: PathBuf,
    config: Config,
    configuration_sha256: String,
    running: String,
    machine_path: Option<PathBuf>,
}

pub fn context() -> Result<Option<Context>, Error> {
    let Some(path) = env::var_os("OPENPILOT_NATIVE_CONFIG") else {
        return Ok(None);
    };
    if let Some(reason) = env::var_os("OPENPILOT_NATIVE_UPDATE_ERROR") {
        return Err(error(reason.to_string_lossy()));
    }
    let path = PathBuf::from(path).canonicalize()?;
    let (config, configuration_sha256) = Config::read_pinned(&path).map_err(error)?;
    let running = env::var("OPENPILOT_RUNNING_GENERATION").map_err(error)?;
    Ok(Some(Context {
        path,
        config,
        configuration_sha256,
        running,
        machine_path: env::var_os(machine::ENV).map(PathBuf::from),
    }))
}

impl Context {
    pub fn validate(&self, paths: &Paths) -> Result<(), Error> {
        let preparation = self.preparation()?;
        self.validate_preparation(paths, preparation.as_ref().map(|p| &p.1))
    }

    fn preparation(&self) -> Result<Option<(PathBuf, machine::Config, String)>, Error> {
        self.machine_path
            .as_ref()
            .map(|path| {
                let (config, pin) = machine::Config::read_pinned(path).map_err(error)?;
                config.verify_stores(&self.config.stores).map_err(error)?;
                Ok((path.clone(), config, pin))
            })
            .transpose()
    }

    fn validate_preparation(
        &self,
        paths: &Paths,
        preparation: Option<&machine::Config>,
    ) -> Result<(), Error> {
        Config::verify_pin(&self.path, &self.configuration_sha256).map_err(error)?;
        let selected = selection::resolve(&self.config.stores).map_err(error)?;
        let runtime =
            env::var_os("OPENPILOT_ROOT").ok_or_else(|| error("actual runtime root is missing"))?;
        if selected.id != self.running
            || selected.source_root != PathBuf::from(runtime).canonicalize()?
            || self.config.stores.params != paths.system("/data/params")
            || paths.staging
                != preparation
                    .map_or_else(
                        || self.config.stores.generations.join("updater"),
                        |machine| machine.updater_staging.clone(),
                    )
                    .join(&self.running)
            || paths.lock != paths.staging.join("updater.lock")
        {
            return Err(error(
                "native updater context differs from the selected source, Params or staging",
            ));
        }
        let sealed = generation::load(&self.config.stores, &self.running).map_err(error)?;
        let metadata: serde_json::Value =
            serde_json::from_reader(File::open(sealed.runtime.root.join("build.json"))?)?;
        let branch = metadata["channel"]
            .as_str()
            .ok_or_else(|| error("native channel missing"))?;
        let base = sealed
            .prepared
            .source
            .update_checkout(
                &self.config.stores.sources,
                branch,
                sealed.layout.expected_source_growth_bytes,
            )
            .map_err(error)?;
        if paths.base != base {
            return Err(error("native updater base is not its owned Git clone"));
        }
        Config::verify_pin(&self.path, &self.configuration_sha256).map_err(error)?;
        Ok(())
    }

    pub fn ready(&self) -> Result<bool, Error> {
        Ok(
            pending::ready_pinned(&self.path, &self.configuration_sha256)
                .map_err(error)?
                .is_some(),
        )
    }

    pub fn candidate_root(&self) -> Result<Option<PathBuf>, Error> {
        let Some(pending) =
            pending::ready_pinned(&self.path, &self.configuration_sha256).map_err(error)?
        else {
            return Ok(None);
        };
        let root = generation::load(&self.config.stores, &pending.candidate)
            .map_err(error)?
            .runtime
            .root;
        Config::verify_pin(&self.path, &self.configuration_sha256).map_err(error)?;
        Ok(Some(root))
    }

    pub fn stage(&self, paths: &Paths, commands: &mut dyn Commands) -> Result<(), Error> {
        let preparation = self.preparation()?;
        self.validate_preparation(paths, preparation.as_ref().map(|p| &p.1))?;
        let selected = selection::resolve(&self.config.stores).map_err(error)?;
        let owner = supervision::process_identity(std::process::id()).map_err(error)?;
        let args = if let Some((path, machine, pin)) = &preparation {
            machine
                .verify_source_paths(&paths.merged())
                .map_err(error)?;
            machine::Config::verify_pin(path, pin).map_err(error)?;
            vec![
                "/usr/bin/env".into(),
                "-u".into(),
                "LD_LIBRARY_PATH".into(),
                "-u".into(),
                "LD_PRELOAD".into(),
                "/usr/bin/python3".into(),
                "-I".into(),
                "-B".into(),
                "-S".into(),
                paths
                    .merged()
                    .join("tools/native/source_entry.py")
                    .to_string_lossy()
                    .into_owned(),
                "--source".into(),
                paths.merged().to_string_lossy().into_owned(),
                "--machine-config".into(),
                path.to_string_lossy().into_owned(),
                "--machine-configuration-sha256".into(),
                pin.clone(),
                "--running-config".into(),
                self.path.to_string_lossy().into_owned(),
                "--running-configuration-sha256".into(),
                self.configuration_sha256.clone(),
                "--previous".into(),
                self.running.clone(),
                "--request-owner".into(),
                owner,
            ]
        } else {
            let binary = selected
                .source_root
                .join("rust/bin/openpilot-native-build-scope");
            vec![
                "sudo".into(),
                "-n".into(),
                binary.to_string_lossy().into_owned(),
                "--config".into(),
                self.path.to_string_lossy().into_owned(),
                "--configuration-sha256".into(),
                self.configuration_sha256.clone(),
                "--stage-update".into(),
                "--root".into(),
                paths.merged().to_string_lossy().into_owned(),
                "--previous".into(),
                self.running.clone(),
                "--request-owner".into(),
                owner,
            ]
        };
        commands.run(&args, Some(&selected.source_root))?;
        if let Some((path, _, pin)) = preparation {
            machine::Config::verify_pin(&path, &pin).map_err(error)?;
        }
        if !self.ready()? {
            return Err(error(
                "native builder did not publish a verified pending generation",
            ));
        }
        Ok(())
    }
}

pub fn protocol() {
    println!(
        "{}",
        serde_json::json!({"source_generation_update_protocol":1})
    );
}

pub fn preparation_protocol() {
    println!(
        "{}",
        serde_json::json!({"source_preparation_update_protocol":1})
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpilot_runtime_build::{
        generation::Stores,
        resources::{DiskBudget, Policy},
    };
    use std::{collections::BTreeMap, fs};

    #[test]
    fn each_operation_keeps_its_own_pin_and_the_next_can_read_a_new_configuration() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let mut config = Config {
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
        let path = root.join("config.json");
        fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
        let (previous, pin) = Config::read_pinned(&path).unwrap();
        let previous = Context {
            path: path.clone(),
            config: previous,
            configuration_sha256: pin,
            running: "a".repeat(64),
            machine_path: None,
        };
        fs::write(root.join("previous.log"), b"previous normal runtime log").unwrap();
        config.cache = root.join("changed-cache");
        fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
        assert!(previous
            .ready()
            .unwrap_err()
            .to_string()
            .contains("configuration changed"));
        let (next, pin) = Config::read_pinned(&path).unwrap();
        let next = Context {
            path,
            config: next,
            configuration_sha256: pin,
            running: "a".repeat(64),
            machine_path: None,
        };
        assert!(!next.ready().unwrap());
        assert!(!config.cache.exists());
        assert!(!config.stores.generations.exists());
        assert_eq!(
            fs::read(root.join("previous.log")).unwrap(),
            b"previous normal runtime log"
        );
    }
}
