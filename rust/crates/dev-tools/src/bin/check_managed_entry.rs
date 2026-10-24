//! The parent-owned outer checker also dispatches the isolated source worker.
use openpilot_dev_tools::{managed_entry_check, managed_entry_source, seal::Result};
use std::{ffi::OsStr, fs};
fn run() -> Result<i32> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args
        .first()
        .is_some_and(|arg| arg == OsStr::new("--managed-collector-gated"))
    {
        if args.len() != 4 {
            return Err("--managed-collector-gated requires endpoint, root and gate path".into());
        }
        openpilot_dev_tools::managed_entry_collector_source::run_gated(
            args[1].to_str().ok_or("collector endpoint must be UTF-8")?,
            std::path::Path::new(&args[2]),
            std::path::Path::new(&args[3]),
        )?;
        return Ok(0);
    }
    if args
        .first()
        .is_some_and(|arg| arg == OsStr::new("--managed-collector"))
    {
        if args.len() != 3 {
            return Err("--managed-collector requires endpoint and root".into());
        }
        openpilot_dev_tools::managed_entry_collector_source::run(
            args[1].to_str().ok_or("collector endpoint must be UTF-8")?,
            std::path::Path::new(&args[2]),
        )?;
        return Ok(0);
    }
    if args
        .first()
        .is_some_and(|arg| arg == OsStr::new("--managed-source"))
    {
        if args.len() != 2 {
            return Err("--managed-source requires exactly one request file".into());
        }
        let request = serde_json::from_slice(&fs::read(&args[1])?)?;
        let environment = std::env::vars_os().collect();
        return managed_entry_source::run(
            &request,
            &environment,
            &mut std::io::stdin().lock(),
            &mut std::io::stdout().lock(),
        );
    }
    if args
        .first()
        .is_some_and(|arg| arg == OsStr::new("--managed-worker"))
    {
        if args.len() != 2 {
            return Err("--managed-worker requires exactly one request file".into());
        }
        println!(
            "{}",
            managed_entry_check::run_worker(std::path::Path::new(&args[1]))?
        );
        return Ok(0);
    }
    let readiness = args
        .first()
        .is_some_and(|arg| arg == OsStr::new("--readiness"));
    let arguments = if readiness { &args[1..] } else { &args[..] };
    let environment = std::env::vars_os().collect();
    let parse = if readiness {
        managed_entry_check::parse_readiness
    } else {
        managed_entry_check::parse
    };
    match parse(arguments, &std::env::current_dir()?, &environment)? {
        managed_entry_check::Command::Help => {
            if readiness {
                println!("usage: check-managed-entry --readiness binary collector binding output");
            } else {
                println!("usage: check-managed-entry binary collector binding output [--case CASE] [--runner ...]");
            }
        }
        managed_entry_check::Command::Run(options) => {
            let repository = std::env::var_os("OPENPILOT_RUST_ROOT")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
                });
            let execute = if readiness {
                managed_entry_check::run_readiness
            } else {
                managed_entry_check::run
            };
            let report = execute(&repository, &options, &std::env::current_exe()?)?;
            println!("{}", report.to_json()?);
        }
    }
    Ok(0)
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            if let Some(admission) = error.downcast_ref::<managed_entry_check::InvalidCase>() {
                if let Err(cleanup) = fs::remove_file(admission.output.join("manifest.json")) {
                    if cleanup.kind() != std::io::ErrorKind::NotFound {
                        eprintln!("could not invalidate prior managed-entry result: {cleanup}");
                    }
                }
            }
            if let Some(openpilot_dev_tools::version_reference_check::Interrupted(signal)) =
                error.downcast_ref()
            {
                let _ = signal_hook::low_level::emulate_default_handler(*signal);
            }
            eprintln!("{error}");
            std::process::exit(1)
        }
    }
}
