//! The native source-build route stages a verified generation, without invoking
//! the legacy release package downloader or replacing a live source checkout.
use crate::{paths::Paths, process::Commands, Error};
use openpilot_runtime_build::{
    generation::{self, Config},
    pending, selection, supervision,
};
use std::{env, fs::File, path::PathBuf};

fn error(value: impl std::fmt::Display) -> Error {
    Error::Native(value.to_string())
}

pub struct Context {
    path: PathBuf,
    config: Config,
    running: String,
}

pub fn context() -> Result<Option<Context>, Error> {
    let Some(path) = env::var_os("OPENPILOT_NATIVE_CONFIG") else {
        return Ok(None);
    };
    if let Some(reason) = env::var_os("OPENPILOT_NATIVE_UPDATE_ERROR") {
        return Err(error(reason.to_string_lossy()));
    }
    let path = PathBuf::from(path).canonicalize()?;
    let config = Config::read(&path).map_err(error)?;
    let running = env::var("OPENPILOT_RUNNING_GENERATION").map_err(error)?;
    Ok(Some(Context {
        path,
        config,
        running,
    }))
}

impl Context {
    pub fn validate(&self, paths: &Paths) -> Result<(), Error> {
        let selected = selection::resolve(&self.config.stores).map_err(error)?;
        let runtime =
            env::var_os("OPENPILOT_ROOT").ok_or_else(|| error("actual runtime root is missing"))?;
        if selected.id != self.running
            || selected.source_root != PathBuf::from(runtime).canonicalize()?
            || self.config.stores.params != paths.system("/data/params")
            || paths.staging
                != self
                    .config
                    .stores
                    .generations
                    .join("updater")
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
        Ok(())
    }

    pub fn ready(&self) -> Result<bool, Error> {
        Ok(pending::ready(&self.path).map_err(error)?.is_some())
    }

    pub fn candidate_root(&self) -> Result<Option<PathBuf>, Error> {
        let Some(pending) = pending::ready(&self.path).map_err(error)? else {
            return Ok(None);
        };
        Ok(Some(
            generation::load(&self.config.stores, &pending.candidate)
                .map_err(error)?
                .runtime
                .root,
        ))
    }

    pub fn stage(&self, paths: &Paths, commands: &mut dyn Commands) -> Result<(), Error> {
        self.validate(paths)?;
        let selected = selection::resolve(&self.config.stores).map_err(error)?;
        let binary = selected
            .source_root
            .join("rust/bin/openpilot-native-build-scope");
        let args = [
            "sudo".into(),
            "-n".into(),
            binary.to_string_lossy().into_owned(),
            "--config".into(),
            self.path.to_string_lossy().into_owned(),
            "--stage-update".into(),
            "--root".into(),
            paths.merged().to_string_lossy().into_owned(),
            "--previous".into(),
            self.running.clone(),
            "--request-owner".into(),
            supervision::process_identity(std::process::id()).map_err(error)?,
        ];
        commands.run(&args, Some(&selected.source_root))?;
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
