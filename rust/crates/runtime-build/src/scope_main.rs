//! Root prepares only a fixed per-user cgroup, then execs the same-source
//! frontend as the sudo caller. No project source/compiler runs as root.
use openpilot_runtime_build::{generation::Config, memory_cgroup, supervision, Result};
use std::{os::unix::process::CommandExt, path::PathBuf, process::Command};

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--help" {
        println!("openpilot-native-build-scope --config CONFIG (--build --root SOURCE | --stage-update --root SOURCE --previous ID --request-owner PID:START_TICKS | --verify)\nPrepare only the sudo caller's fixed memory scope, then run the same-source native frontend as that non-root user. No arbitrary root command is accepted.");
        return Ok(());
    }
    let mut config = None;
    let mut root = None;
    let mut previous = None;
    let mut request_owner = None;
    let mut mode = None;
    let mut arguments = args.into_iter();
    while let Some(arg) = arguments.next() {
        match arg.to_str() {
            Some("--config") if config.is_none() => {
                config = Some(PathBuf::from(
                    arguments.next().ok_or("missing configuration")?,
                ))
            }
            Some("--root") if root.is_none() => {
                root = Some(arguments.next().ok_or("missing source")?)
            }
            Some("--previous") if previous.is_none() => {
                previous = Some(arguments.next().ok_or("missing previous generation")?)
            }
            Some("--request-owner") if request_owner.is_none() => {
                request_owner = Some(arguments.next().ok_or("missing request owner")?);
            }
            Some(value @ ("--build" | "--stage-update" | "--verify")) if mode.is_none() => {
                mode = Some(value.to_owned())
            }
            _ => return Err("unknown or duplicate native scope argument".into()),
        }
    }
    let mode = mode.ok_or("native scope mode missing")?;
    if (mode == "--stage-update") != previous.is_some()
        || (mode == "--stage-update") != request_owner.is_some()
        || (mode == "--verify") == root.is_some()
    {
        return Err("mixed native scope arguments".into());
    }
    let path = config
        .ok_or("native scope configuration missing")?
        .canonicalize()?;
    let config = Config::read(&path)?;
    let uid = std::env::var("SUDO_UID")?.parse::<u32>()?;
    let gid = std::env::var("SUDO_GID")?.parse::<u32>()?;
    let control = memory_cgroup::prepare_scope(&config.resources.memory_cgroup_parent, uid, gid)?;
    memory_cgroup::enter_control(&control)?;
    let parent = rustix::process::getppid().ok_or("native scope parent missing")?;
    let mut command =
        Command::new(std::env::current_exe()?.with_file_name("openpilot-runtime-native"));
    // std's Unix exec applies uid/gid and clears supplementary groups before
    // replacing this single-threaded setup helper with the unprivileged tool.
    command
        .uid(uid)
        .gid(gid)
        .env_remove("SUDO_UID")
        .env_remove("SUDO_GID")
        .env_remove("SUDO_USER")
        .env_remove("OPENPILOT_NATIVE_BUILD_MEMORY_GROUP")
        .env_remove("OPENPILOT_NATIVE_BUILD_PARENT")
        .env_remove("OPENPILOT_NATIVE_BUILD_ATTEMPT")
        .env_remove(supervision::REQUEST_OWNER)
        // Build tools use the explicit cache, never sudo's root HOME/config.
        .env("HOME", &config.cache)
        .env(
            "OPENPILOT_NATIVE_SCOPE_PARENT",
            parent.as_raw_nonzero().get().to_string(),
        )
        .arg("--config")
        .arg(path)
        .arg(if mode == "--verify" {
            "--verify-build-scope"
        } else {
            &mode
        });
    if let Some(root) = root {
        command.arg("--root").arg(root);
    }
    if let Some(previous) = previous {
        command.arg("--previous").arg(previous);
    }
    if let Some(owner) = request_owner {
        command.env(supervision::REQUEST_OWNER, owner);
    }
    Err(command.exec().into())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("native build scope: {error}");
        std::process::exit(1);
    }
}
