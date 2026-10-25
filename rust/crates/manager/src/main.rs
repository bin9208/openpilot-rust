mod application;
mod startup_api;
mod startup_display;
mod startup_failure;

use openpilot_manager::Error;
use openpilot_manager_catalog::binding::Installation;
use std::{path::PathBuf, process::ExitCode};

enum Mode {
    Inspect,
    Run,
    StdoutChild,
}

fn options() -> Result<Option<(Installation, Mode)>, Error> {
    let mut args = std::env::args_os().skip(1);
    let mut root = std::env::var_os("OPENPILOT_ROOT")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let mut binaries = None;
    let mut mode = Mode::Run;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help") => {
                println!("openpilot-manager --root REPOSITORY [--bin-dir DIRECTORY] [--inspect]\nRuns the installed native catalog, startup registration and manager IPC lifecycle.\nUses original Params/Paths environment; receives CARROT_BOOT_LOCK_SOCKET as an owned lock.");
                return Ok(None);
            }
            Some("--root") => root = args.next().ok_or(Error::Contract("missing root"))?.into(),
            Some("--bin-dir") => {
                binaries = Some(PathBuf::from(
                    args.next()
                        .ok_or(Error::Contract("missing bin directory"))?,
                ))
            }
            Some("--inspect") => mode = Mode::Inspect,
            Some("--stdout-child") => mode = Mode::StdoutChild,
            _ => return Err(Error::Contract("unknown manager option")),
        }
    }
    let root = root.canonicalize()?;
    Ok(Some((
        Installation {
            binaries: match binaries {
                Some(path) => std::env::current_dir()?.join(path),
                None => root.join("rust/bin"),
            },
            root,
        },
        mode,
    )))
}

fn main() -> ExitCode {
    let _runtime_logs = openpilot_logging::producer::RuntimeLogScope;
    use std::os::unix::process::ExitStatusExt;
    let result = options().and_then(|options| match options {
        None => Ok(None),
        Some((installation, Mode::Inspect)) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&openpilot_manager::installed::inspect(
                    &installation
                ))?
            );
            Ok(None)
        }
        Some((installation, Mode::StdoutChild)) => application::run(&installation).map(|()| None),
        Some((installation, Mode::Run)) => {
            let request = openpilot_process_supervision::CapturedCommand {
                launcher: installation.binaries.join("openpilot-process-child"),
                cwd: installation.root.clone(),
                argv: vec![
                    std::env::current_exe()?.into_os_string(),
                    "--stdout-child".into(),
                    "--root".into(),
                    installation.root.into_os_string(),
                    "--bin-dir".into(),
                    installation.binaries.into_os_string(),
                ],
            };
            openpilot_manager::output::run(&request).map(Some)
        }
    });
    match result {
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some(status)) => ExitCode::from(u8::try_from(status.into_raw() >> 8).unwrap_or(1)),
        Err(Error::Interrupted) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("Manager failed to start: {error}");
            ExitCode::FAILURE
        }
    }
}
