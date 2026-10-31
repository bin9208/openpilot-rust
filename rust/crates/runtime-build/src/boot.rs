//! The independently pinned seed verifies generations before executing their
//! own launcher. Initial selection belongs to that launcher's normal boot lock.
use crate::{generation, machine, selection, Result};
use std::{
    ffi::OsString,
    os::unix::process::CommandExt,
    path::{Component, PathBuf},
    process::Command,
};

pub fn run(arguments: Vec<OsString>) -> Result<()> {
    let mut args = arguments.into_iter();
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--boot-runtime")) {
        return Err("native seed boot mode missing".into());
    }
    let (mut config, mut pin, mut root, mut initial, mut paths) = (None, None, None, None, None);
    let mut verification_only = false;
    while let Some(key) = args.next() {
        match key.to_str() {
            Some("--config") if config.is_none() => {
                config = Some(PathBuf::from(
                    args.next().ok_or("boot configuration missing")?,
                ))
            }
            Some("--configuration-sha256") if pin.is_none() => {
                pin = Some(
                    args.next()
                        .ok_or("boot configuration pin missing")?
                        .into_string()
                        .map_err(|_| "invalid boot configuration pin")?,
                )
            }
            Some("--root") if root.is_none() => {
                root = Some(PathBuf::from(
                    args.next().ok_or("requested boot checkout missing")?,
                ))
            }
            Some("--initial-generation") if initial.is_none() => {
                initial = Some(
                    args.next()
                        .ok_or("initial generation missing")?
                        .into_string()
                        .map_err(|_| "invalid initial generation")?,
                )
            }
            Some("--paths") if paths.is_none() => {
                paths = Some(PathBuf::from(args.next().ok_or("startup paths missing")?))
            }
            Some("--verify-package") if !verification_only => verification_only = true,
            _ => return Err("unknown or duplicate native seed boot argument".into()),
        }
    }
    let config_path = config
        .ok_or("native seed boot configuration required")?
        .canonicalize()?;
    let pin = pin.ok_or("native seed boot configuration pin required")?;
    let config = generation::Config::read_expected(&config_path, &pin)?;
    let root = root.ok_or("requested boot checkout required")?;
    if !root.is_absolute()
        || root
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err("requested boot checkout must be absolute".into());
    }
    if rustix::process::getuid().is_root() || rustix::process::geteuid().is_root() {
        return Err("native seed boot must run as the normal non-root runtime user".into());
    }
    generation::agnos_identity()?;
    let machine = match (
        std::env::var_os(machine::ENV),
        std::env::var_os(machine::LAUNCH_PIN),
    ) {
        (Some(path), Some(pin)) => {
            let path = PathBuf::from(path);
            let pin = pin
                .into_string()
                .map_err(|_| "invalid machine startup pin")?;
            let (policy, actual) = machine::Config::read_pinned(&path)?;
            if actual != pin {
                return Err("native machine configuration changed before boot".into());
            }
            policy.verify_stores(&config.stores)?;
            policy.verify_source_paths(&root)?;
            Some((path, pin))
        }
        (None, None) => None,
        _ => return Err("native machine startup path and pin must be supplied together".into()),
    };
    let (selected, sealed, initialize) =
        match std::fs::symlink_metadata(config.stores.generations.join("active.json")) {
            Ok(_) => {
                let (selected, sealed) = selection::resolve_verified(&config.stores)?;
                (selected, sealed, false)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let id =
                    initial.ok_or("no selected native generation or verified initial request")?;
                let sealed = generation::load(&config.stores, &id)?;
                let selected = selection::Selected {
                    id,
                    source_commit: sealed.runtime.source.commit.clone(),
                    source_root: sealed.runtime.root.clone(),
                    recovered_previous: false,
                    rejected: vec![],
                };
                (selected, sealed, true)
            }
            Err(e) => return Err(e.into()),
        };
    let mut libraries = vec![selected.source_root.join("rust/bin/lib")];
    libraries.extend(sealed.layout.libraries);
    eprintln!(
        "{}",
        serde_json::json!({"requested_checkout":root,"selected":selected,"initial_boot":initialize})
    );
    let mut command = Command::new(selected.source_root.join("rust/bin/openpilot-launch"));
    if let Some(paths) = paths {
        command.arg("--paths").arg(paths);
    }
    command
        .env("LD_LIBRARY_PATH", std::env::join_paths(libraries)?)
        .env("OPENPILOT_NATIVE_LAUNCH_CONFIG_SHA256", &pin)
        .env("OPENPILOT_NATIVE_CONFIG", &config_path)
        .env("OPENPILOT_RUNNING_GENERATION", &selected.id)
        .env("OPENPILOT_UPDATE_CHECKOUT", &root)
        .arg("--root")
        .arg(&selected.source_root)
        .arg("--native-config")
        .arg(&config_path)
        .arg("--update-checkout")
        .arg(root)
        .arg("--generation")
        .arg(&selected.id)
        .current_dir(&selected.source_root);
    if initialize {
        command.arg("--initialize-generation");
    }
    if verification_only {
        command.arg("--verify-package");
    }
    if let Some((path, pin)) = machine {
        machine::Config::verify_pin(&path, &pin)?;
    }
    generation::Config::verify_pin(&config_path, &pin)?;
    Err(command.exec().into())
}
