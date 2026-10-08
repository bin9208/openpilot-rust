use openpilot_pandad::supervisor_runtime::{self, Config};
use std::{path::PathBuf, process::ExitCode};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let base = std::env::var_os("BASEDIR")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let executable = std::env::current_exe()?;
    let mut config = Config {
        root: "/".into(),
        firmware: base.join("panda/board/obj"),
        basedir: base,
        child: executable.with_file_name("openpilot-pandad"),
        launcher: executable.with_file_name("openpilot-process-child"),
        cycles: None,
    };
    let mut explicit_firmware = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(option) = args.next() {
        match option.to_str() {
            Some("--help") => {
                println!("openpilot-pandad-supervisor [--root PATH] [--basedir PATH] [--firmware PATH] [--child PATH] [--launcher PATH] [--cycles N]\n\nPrepares Panda firmware and supervises the native Panda runtime. Alternate paths and bounded cycles support owned host fixtures.");
                return Ok(());
            }
            Some("--root") => config.root = args.next().ok_or("--root needs a path")?.into(),
            Some("--basedir") => {
                config.basedir = args.next().ok_or("--basedir needs a path")?.into()
            }
            Some("--firmware") => {
                config.firmware = args.next().ok_or("--firmware needs a path")?.into();
                explicit_firmware = true;
            }
            Some("--child") => config.child = args.next().ok_or("--child needs a path")?.into(),
            Some("--launcher") => {
                config.launcher = args.next().ok_or("--launcher needs a path")?.into()
            }
            Some("--cycles") if config.cycles.is_none() => {
                config.cycles = args
                    .next()
                    .and_then(|value| value.to_str().and_then(|text| text.parse().ok()))
                    .filter(|value| *value > 0);
                if config.cycles.is_none() {
                    return Err("--cycles must be positive".into());
                }
            }
            _ => return Err("unknown or duplicate Panda supervisor option".into()),
        }
    }
    if !explicit_firmware {
        config.firmware = config.basedir.join("panda/board/obj");
    }
    supervisor_runtime::run(config)?;
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pandad-supervisor: {error}");
            ExitCode::FAILURE
        }
    }
}
