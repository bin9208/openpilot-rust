use openpilot_runtime_build::{
    generation::Config, memory_cgroup, pending, resources, selection, supervision, Result,
};
use std::{os::unix::process::CommandExt, path::PathBuf, process::Command};

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let (mut mode, mut root, mut config, mut id) = (None, None, None, None);
    let mut previous = None;
    let mut pin = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                println!("Usage: openpilot-runtime-native --config CONFIG_JSON [--configuration-sha256 SHA256] (--build --root CHECKOUT | --stage-update --root CHECKOUT --previous ID | --activate ID | --resolve | --launch --root CHECKOUT)\nBuild/seal a full native AGNOS runtime with source-built manager and ABI tools. Stage-update preserves the active runtime until a locked native boot. Activation requires verified offroad state. Launch resolves an actual preserved source generation and reports fallback explicitly. Requires prepared, pinned toolchain/dependencies/provider inputs. No exact-commit runtime release is fetched.");
                return Ok(());
            }
            Some(
                value @ ("--build"
                | "--build-worker"
                | "--stage-update"
                | "--stage-update-worker"
                | "--resolve"
                | "--verify-build-scope"
                | "--launch"),
            ) if mode.is_none() => mode = Some(value.to_owned()),
            Some("--activate") if mode.is_none() => {
                mode = Some("--activate".to_owned());
                id = Some(
                    args.next()
                        .ok_or("missing source generation ID")?
                        .into_string()
                        .map_err(|_| "invalid generation ID")?,
                );
            }
            Some("--root") if root.is_none() => {
                root = Some(PathBuf::from(args.next().ok_or("missing source root")?))
            }
            Some("--config") if config.is_none() => {
                config = Some(PathBuf::from(args.next().ok_or("missing configuration")?))
            }
            Some("--previous") if previous.is_none() => {
                previous = Some(
                    args.next()
                        .ok_or("missing previous generation")?
                        .into_string()
                        .map_err(|_| "invalid previous generation")?,
                );
            }
            Some("--configuration-sha256") if pin.is_none() => {
                pin = Some(
                    args.next()
                        .ok_or("missing configuration pin")?
                        .into_string()
                        .map_err(|_| "invalid configuration pin")?,
                );
            }
            _ => return Err("unknown, duplicate or mixed native runtime argument".into()),
        }
    }
    let config_path = config.ok_or("--config is required")?.canonicalize()?;
    let (config, configuration_sha256) = Config::read_pinned(&config_path)?;
    let mode = mode.as_deref().ok_or("native runtime mode is required")?;
    if matches!(mode, "--build-worker" | "--stage-update-worker") && pin.is_none() {
        return Err("native worker requires its frontend configuration pin".into());
    }
    if pin.is_some_and(|pin| pin != configuration_sha256) {
        return Err("native runtime configuration changed before the requested phase".into());
    }
    let staging = matches!(mode, "--stage-update" | "--stage-update-worker");
    if staging != previous.is_some() {
        return Err("--previous is required only for staging an update".into());
    }
    match mode {
        "--verify-build-scope" if root.is_none() => {
            memory_cgroup::verify_control(&config.resources.memory_cgroup_parent)?;
            let uid = rustix::process::getuid().as_raw();
            if !rustix::process::getgroups()?.is_empty() {
                return Err("native scope helper retained supplementary groups".into());
            }
            Config::verify_pin(&config_path, &configuration_sha256)?;
            println!(
                "{}",
                serde_json::json!({"scope_verified":true,"uid":uid,"gid":rustix::process::getgid().as_raw(),"supplementary_groups_cleared":true,"memory_parent":config.resources.memory_cgroup_parent})
            );
        }
        "--build" | "--stage-update" => {
            memory_cgroup::verify_control(&config.resources.memory_cgroup_parent)?;
            if staging && pending::ready_pinned(&config_path, &configuration_sha256)?.is_some() {
                return Err("a verified native update is already pending".into());
            }
            let root = root
                .ok_or("--root is required for building")?
                .canonicalize()?;
            let mut command = Command::new(std::env::current_exe()?);
            let (mut monitor, attempt, requested_source) =
                resources::attempt(&config, &config_path, &root, &configuration_sha256)?;
            eprintln!("native build resource evidence: {}", attempt.display());
            let output_path = attempt.join("build.log");
            let output = supervision::retain_output(&mut command, &output_path)?;
            eprintln!("native build worker output: {}", output_path.display());
            command
                .env(memory_cgroup::ENV, &monitor.group.path)
                .env(resources::ATTEMPT, &attempt)
                .arg(if staging {
                    "--stage-update-worker"
                } else {
                    "--build-worker"
                })
                .arg("--config")
                .arg(&config_path)
                .arg("--configuration-sha256")
                .arg(&configuration_sha256)
                .arg("--root")
                .arg(&root);
            if let Some(previous) = &previous {
                command.arg("--previous").arg(previous);
            }
            let result = supervision::run_observed(command, || monitor.observe());
            let success = result.as_ref().is_ok_and(|status| status.success());
            let reason = match &result {
                Ok(status) if !status.success() => Some(status.to_string()),
                Err(error) => Some(error.to_string()),
                _ => None,
            };
            let recorded = monitor.finish(success, reason);
            output.sync_all()?;
            let status = result?;
            recorded?;
            if !status.success() {
                eprintln!(
                    "{}",
                    String::from_utf8_lossy(&supervision::output_tail(&output_path)?)
                );
                return Err(format!("native build worker exited: {status}").into());
            }
            let id = resources::candidate(
                &config,
                &config_path,
                &root,
                &attempt,
                &configuration_sha256,
                &requested_source,
            )?;
            supervision::check_request_owner()?;
            if let Some(previous) = previous {
                pending::stage_pinned(&config_path, &configuration_sha256, &id, &previous)?;
            }
            println!(
                "{}",
                serde_json::json!({"generation_id":id,"activated":false,"staged_for_boot":staging,"resource_evidence":attempt,"configuration_sha256":configuration_sha256,"requested_source":requested_source})
            );
        }
        "--build-worker" | "--stage-update-worker" => {
            let _guard = supervision::WorkerGuard::start(&config.stores.params)?;
            memory_cgroup::enter(
                &config.resources.memory_cgroup_parent,
                config.resources.maximum_memory_bytes,
            )?;
            let root = root
                .ok_or("--root is required for building")?
                .canonicalize()?;
            let id = config.build(&root)?;
            resources::publish_candidate(&config, &config_path, &root, &id, &configuration_sha256)?;
        }
        "--activate" if root.is_none() => {
            Config::verify_pin(&config_path, &configuration_sha256)?;
            let selected =
                selection::activate(&config.stores, &id.ok_or("missing generation ID")?)?;
            println!("{}", serde_json::to_string_pretty(&selected)?);
        }
        "--resolve" if root.is_none() => {
            let selected = selection::resolve(&config.stores)?;
            Config::verify_pin(&config_path, &configuration_sha256)?;
            println!("{}", serde_json::to_string_pretty(&selected)?);
        }
        "--launch" => {
            let root = root.ok_or("--root is required for launch")?;
            if !root.is_absolute()
                || root.components().any(|c| {
                    matches!(
                        c,
                        std::path::Component::ParentDir | std::path::Component::CurDir
                    )
                })
            {
                return Err("requested launch checkout must be absolute".into());
            }
            let selected = selection::resolve(&config.stores)?;
            eprintln!(
                "{}",
                serde_json::json!({"requested_checkout":root,"selected":selected})
            );
            let mut command = Command::new(selected.source_root.join("rust/bin/openpilot-launch"));
            command
                .env(
                    "LD_LIBRARY_PATH",
                    selection::library_path(&config.stores, &selected.id)?,
                )
                .arg("--root")
                .arg(&selected.source_root)
                .arg("--native-config")
                .arg(&config_path)
                .arg("--update-checkout")
                .arg(&root)
                .arg("--generation")
                .arg(&selected.id)
                .current_dir(&selected.source_root)
                .env(
                    "OPENPILOT_NATIVE_LAUNCH_CONFIG_SHA256",
                    &configuration_sha256,
                )
                .env("OPENPILOT_NATIVE_CONFIG", &config_path)
                .env("OPENPILOT_UPDATE_CHECKOUT", root)
                .env("OPENPILOT_RUNNING_GENERATION", selected.id);
            Config::verify_pin(&config_path, &configuration_sha256)?;
            let error = command.exec();
            return Err(error.into());
        }
        _ => return Err("native runtime mode does not accept these paths".into()),
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("native runtime: {error}");
        std::process::exit(1);
    }
}
