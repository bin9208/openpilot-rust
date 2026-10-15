use super::{run::run, Error, HelperConfig, Result};
use crate::{
    autorun,
    runtime_args::{Output, ParseError, RuntimeArgs},
    runtime_preferences::Reader,
};
use openpilot_managed_entry::StepError;
use std::{
    env,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

fn root_arguments() -> Result<(PathBuf, Vec<String>)> {
    let mut root = env::var_os("OPENPILOT_BASEDIR")
        .map_or_else(env::current_dir, |path| Ok(PathBuf::from(path)))?;
    let mut args = env::args().skip(1);
    let mut forwarded = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "--root" {
            root = PathBuf::from(
                args.next()
                    .ok_or_else(|| Error::Invalid("--root requires a checkout path".into()))?,
            );
        } else if let Some(path) = arg.strip_prefix("--root=") {
            root = PathBuf::from(path);
        } else {
            forwarded.push(arg);
        }
    }
    Ok((root, forwarded))
}
fn helpers(root: &Path, autorun: bool) -> Result<HelperConfig> {
    let executable = env::current_exe()?;
    Ok(HelperConfig {
        program: if autorun {
            executable.with_file_name("openpilot-cluster")
        } else {
            executable.clone()
        },
        launcher: executable.with_file_name("openpilot-process-child"),
        cwd: root.to_owned(),
    })
}
fn dispatch_helper(args: &[String]) -> Result<bool> {
    if args
        .first()
        .is_some_and(|arg| arg == "--replay-tools-helper")
    {
        let [_, socket, config] = args else {
            return Err(Error::Invalid(
                "--replay-tools-helper requires socket and config".into(),
            ));
        };
        crate::replay_tools::run_helper(Path::new(socket), config)?;
        return Ok(true);
    }
    if args
        .first()
        .is_some_and(|arg| arg == "--route-preload-helper")
    {
        let [_, socket, corner] = args else {
            return Err(Error::Invalid(
                "--route-preload-helper requires socket and corner source".into(),
            ));
        };
        let source = match corner.as_str() {
            "live" => crate::runtime_args::CornerSource::Live,
            "raw" => crate::runtime_args::CornerSource::Raw,
            "stable" => crate::runtime_args::CornerSource::Stable,
            _ => return Err(Error::Invalid("invalid route helper corner source".into())),
        };
        crate::route_replay::preload::run_helper(Path::new(socket), source)?;
        return Ok(true);
    }
    Ok(false)
}
fn parsed(root: &Path, arguments: &[String]) -> Result<Option<RuntimeArgs>> {
    match RuntimeArgs::parse_from(&root.join("openpilot"), arguments) {
        Ok(args) => Ok(Some(args)),
        Err(ParseError::Help(help)) => {
            println!("{help}");
            Ok(None)
        }
        Err(error) => Err(Error::Invalid(error.to_string())),
    }
}
fn select_output(root: &Path, args: &mut RuntimeArgs) {
    if args.usb_output() && args.wifi_fps.is_none() && !Path::new("/TICI").is_file() {
        let mut scanner = autorun::usb::Usb::new(root, Path::new("/tmp/carrot_usbgpu_bus.lock"));
        let available = match args.cluster_hud_mode.and_then(autorun::product_id) {
            Some(product) => scanner.find(product).is_some(),
            None => scanner.find(0x0092).is_some() || scanner.find(0x0123).is_some(),
        };
        if !available {
            println!("TURZX USB cluster display not found on PC; rendering to window only.");
            args.output = Output::Window;
        }
    }
}
fn demo(
    root: &Path,
    arguments: &[String],
    helpers: &HelperConfig,
    interrupt: Option<&AtomicBool>,
) -> Result<()> {
    let Some(mut args) = parsed(root, arguments)? else {
        return Ok(());
    };
    let wifi = if args.usb_output() {
        openpilot_cluster_wifi::Registration::load(&openpilot_cluster_wifi::config_path())
            .map_err(|error| Error::Io(std::io::Error::other(error)))?
    } else {
        None
    };
    args.wifi_fps = wifi.as_ref().map(|config| config.fps);
    select_output(root, &mut args);
    let mut encoder_source = Reader::for_runtime().apply_encoder(&mut args);
    if let Some(config) = wifi {
        encoder_source = args.apply_wifi(&config);
    }
    if args.transport.usb_async && args.usb_codec != crate::runtime_args::Codec::Jpeg {
        return Err(Error::Invalid(
            "--usb-async only supports --usb-codec jpeg".into(),
        ));
    }
    println!(
        "Refreshing native raylib cluster UI input={} output={} usb_codec={} encoder_source={encoder_source}",
        format!("{:?}", args.input).to_ascii_lowercase(),
        format!("{:?}", args.output).to_ascii_lowercase(),
        format!("{:?}", args.usb_codec).to_ascii_lowercase()
    );
    if interrupt.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return Err(Error::Interrupted);
    }
    match run(root, &args, helpers) {
        Err(Error::Interrupted) => {
            if let Some(flag) = interrupt {
                flag.store(false, Ordering::Relaxed);
            }
            println!("\nStopped.");
        }
        result => {
            result?;
        }
    }
    Ok(())
}
struct Entrypoint {
    root: PathBuf,
    helpers: HelperConfig,
    interrupt: Option<Arc<AtomicBool>>,
}
impl autorun::entry::Entrypoint for Entrypoint {
    fn run(
        &mut self,
        request: autorun::RunRequest,
    ) -> std::result::Result<(), autorun::entry::Failure> {
        match demo(
            &self.root,
            &request.args(),
            &self.helpers,
            self.interrupt.as_deref(),
        ) {
            Ok(()) => Ok(()),
            Err(Error::H264Initialization(error)) => Err(
                autorun::entry::Failure::H264Initialization(crate::Error::Io(error)),
            ),
            Err(error) => Err(autorun::entry::Failure::Runtime(crate::Error::Io(
                std::io::Error::other(error),
            ))),
        }
    }
}
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error(transparent)]
    Runtime(#[from] Error),
    #[error(transparent)]
    Managed(#[from] openpilot_managed_entry::RuntimeError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
/// # Errors
/// Preserves helper, managed-entry, argument and full-runtime failures.
pub fn command_line(autorun_mode: bool) -> std::result::Result<(), CommandError> {
    crate::runtime_locale::configure()?;
    let (root, args) = root_arguments()?;
    if dispatch_helper(&args)? {
        return Ok(());
    }
    let helpers = helpers(&root, autorun_mode)?;
    openpilot_managed_entry::runtime("carrot_cluster", "carrot_cluster", || {
        let result = if autorun_mode {
            if args.iter().any(|arg| arg == "--help" || arg == "-h") {
                println!("openpilot-cluster-autorun [--root CHECKOUT]");
                return Ok(());
            }
            if !args.is_empty() {
                return Err(StepError::raised(Error::Invalid(
                    "autorun accepts only --root".into(),
                )));
            }
            (|| -> Result<()> {
                let params = autorun::params::Reader::new(
                    openpilot_params::Params::for_runtime().map_err(crate::Error::from)?,
                );
                let mut platform =
                    autorun::native::Native::new(&root).map_err(std::io::Error::other)?;
                let interrupt = if env::var_os("MANAGER_DAEMON").is_some() {
                    Some(super::stop::ManagedInterrupt::install()?)
                } else {
                    None
                };
                if let Some(interrupt) = &interrupt {
                    platform.set_interrupt(interrupt.flag());
                }
                let mut entry = Entrypoint {
                    root: root.clone(),
                    helpers,
                    interrupt: interrupt.as_ref().map(super::stop::ManagedInterrupt::flag),
                };
                autorun::controller::run(&params, &mut platform, &mut entry).map_err(|error| {
                    match error {
                        autorun::controller::Error::Interrupted
                        | autorun::controller::Error::Gpu(autorun::gpu::Error::Interrupted) => {
                            Error::Interrupted
                        }
                        error => Error::Io(std::io::Error::other(error)),
                    }
                })?;
                Ok(())
            })()
        } else {
            demo(&root, &args, &helpers, None)
        };
        match result {
            Err(Error::Interrupted) => Err(StepError::Interrupted),
            result => result.map_err(StepError::raised),
        }
    })?;
    Ok(())
}
