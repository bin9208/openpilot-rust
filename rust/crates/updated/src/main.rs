use openpilot_hardware_info::{HardwareInfo, HardwarePaths, Pc, Tici};
use openpilot_logging::producer::Factory;
use openpilot_timed::clock::SystemClock;
use openpilot_updated::{
    agnos::Native,
    paths::Paths,
    process::NativeCommands,
    runtime,
    signals::{Signals, Wake},
    updater::{Context, Updater},
    Error, Params,
};
use std::{path::PathBuf, process::ExitCode, sync::Arc};
fn run() -> Result<(), Error> {
    let mut base = std::env::current_dir()?;
    let mut system_root = PathBuf::from("/");
    let mut launcher = std::env::current_exe()?.with_file_name("openpilot-process-child");
    let mut cycles = None;
    let mut agnos_config = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--basedir" => {
                base = args
                    .next()
                    .ok_or(Error::Contract("--basedir needs path"))?
                    .into()
            }
            "--system-root" => {
                system_root = args
                    .next()
                    .ok_or(Error::Contract("--system-root needs path"))?
                    .into()
            }
            "--launcher" => {
                launcher = args
                    .next()
                    .ok_or(Error::Contract("--launcher needs path"))?
                    .into()
            }
            "--cycles" => {
                cycles = Some(
                    args.next()
                        .and_then(|v| v.parse::<u64>().ok())
                        .filter(|v| *v > 0)
                        .ok_or(Error::Contract("cycles must be positive"))?,
                )
            }
            "--agnos-config" => {
                let path = args
                    .next()
                    .ok_or(Error::Contract("--agnos-config needs path"))?;
                agnos_config = Some(serde_json::from_slice::<openpilot_agnos::cli::Config>(
                    &std::fs::read(path)?,
                )?);
            }
            "--help" => {
                println!("openpilot-updated [--basedir PATH] [--system-root PATH] [--launcher PATH] [--cycles N] [--agnos-config PATH]\nContinuous native staged updater. Uses UPDATER_LOCK_FILE and UPDATER_STAGING_ROOT. Alternate system roots restrict validation paths and sync operations; they do not sandbox commands. AGNOS config overrides support owned fixtures; defaults address device partitions.");
                return Ok(());
            }
            _ => return Err(Error::Contract("unknown argument")),
        }
    }
    let paths = Paths::for_runtime(base, system_root);
    let params = Params::open(&paths.system_root)?;
    let factory = Factory::for_runtime()?;
    let mut logger = factory.logger();
    let mut agnos = Native::new(
        agnos_config.unwrap_or_else(|| openpilot_agnos::cli::Config {
            launcher: launcher.clone(),
            ..Default::default()
        }),
        factory.logger(),
    );
    let wake = Arc::new(Wake::default());
    let _signals = Signals::install(Arc::clone(&wake), factory)?;
    let mut commands = NativeCommands {
        launcher,
        wake: Arc::clone(&wake),
    };
    let hardware: Box<dyn HardwareInfo> = if paths.system("/TICI").is_file() {
        Box::new(Tici::with_paths(HardwarePaths::under(&paths.system_root)))
    } else {
        Box::new(Pc)
    };
    let mut updater = Updater::new(
        params,
        Context {
            paths: &paths,
            commands: &mut commands,
            hardware: hardware.as_ref(),
            agnos: &mut agnos,
            clock: &SystemClock,
            logger: &mut logger,
            is_agnos: paths.system("/AGNOS").is_file(),
        },
    );
    runtime::run(&mut updater, &wake, cycles)
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("updated: {error}");
            ExitCode::FAILURE
        }
    }
}
