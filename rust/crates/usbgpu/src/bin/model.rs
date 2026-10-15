use openpilot_usbgpu::{
    hardware,
    model::{self, Paths},
    model_delivery::{self, boot, smoke},
    Error,
};
use std::{
    collections::VecDeque,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
    sync::{atomic::AtomicBool, Arc},
};

fn emit(value: impl std::fmt::Display) -> io::Result<()> {
    writeln!(io::stdout().lock(), "{value}")
}

fn argument(args: &mut VecDeque<String>) -> Result<String, Error> {
    args.pop_front()
        .ok_or(Error::Contract("missing model command argument"))
}
fn paths(root: PathBuf, assets: PathBuf) -> Result<Paths, Error> {
    Paths::for_runtime(&root, assets)
}
fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1).collect::<VecDeque<_>>();
    let mode = argument(&mut args)?;
    if mode == "--present" {
        let mut devices = PathBuf::from(hardware::SYSFS);
        while let Some(flag) = args.pop_front() {
            if flag != "--devices" {
                return Err(Error::Contract("unknown presence argument").into());
            }
            devices = argument(&mut args)?.into();
        }
        return Ok(if model_delivery::presence::present(&devices) {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }
    let mut root = std::env::current_dir()?;
    let mut assets = std::env::var_os("USBGPU_ASSETS_ROOT")
        .map(PathBuf::from)
        .map_or_else(
            || std::env::current_exe().map(|path| path.with_file_name("usbgpu-assets")),
            Ok,
        )?;
    let mut worker = std::env::current_exe()?.with_file_name("openpilot-usbgpu-worker");
    let mut devices = PathBuf::from(hardware::SYSFS);
    let mut identity_root = PathBuf::from("/");
    let mut expected_assets = None;
    let mut manifest_url = None;
    let mut network_wait_seconds = 0.0;
    let mut retry_network = false;
    let model_path = if mode == "--smoke" {
        Some(PathBuf::from(argument(&mut args)?))
    } else {
        None
    };
    let mut cameras = Vec::new();
    while let Some(flag) = args.pop_front() {
        match flag.as_str() {
            "--root" => root = argument(&mut args)?.into(),
            "--assets" => assets = argument(&mut args)?.into(),
            "--worker" => worker = argument(&mut args)?.into(),
            "--devices" => devices = argument(&mut args)?.into(),
            "--identity-root" => identity_root = argument(&mut args)?.into(),
            "--expect-assets-manifest" => expected_assets = Some(argument(&mut args)?),
            "--manifest-url" => manifest_url = Some(argument(&mut args)?),
            "--retry-network" => retry_network = true,
            "--network-wait-seconds" => {
                network_wait_seconds = argument(&mut args)?.parse::<f64>()?
            }
            "--camera" => cameras.push(match argument(&mut args)?.as_str() {
                "1928x1208" => [1928, 1208],
                "1344x760" => [1344, 760],
                _ => return Err(Error::Contract("unsupported smoke camera").into()),
            }),
            _ => return Err(Error::Contract("unknown model command argument").into()),
        }
    }
    if network_wait_seconds < 0.0 {
        writeln!(
            io::stderr().lock(),
            "network wait seconds must be non-negative"
        )?;
        return Ok(ExitCode::from(2));
    }
    let manifest_url = match manifest_url {
        Some(url) => url,
        None if matches!(mode.as_str(), "--ensure-if-egpu" | "--prepare-for-startup") => {
            match std::env::var("CARROT_BIG_MODEL_MANIFEST") {
                Ok(value) => value,
                Err(std::env::VarError::NotPresent) => model_delivery::DEFAULT_MANIFEST_URL.into(),
                Err(_) => return Err(Error::Contract("invalid model manifest URL encoding").into()),
            }
        }
        None => String::new(),
    };
    if mode == "--boot"
        && std::env::var_os("CARROT_BIG_MODEL_STARTUP_FAILED").is_some_and(|value| value == "1")
    {
        emit(serde_json::json!({"prepared":false}))?;
        return Ok(ExitCode::SUCCESS);
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    if mode != "--ensure-if-egpu" || retry_network {
        signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&cancelled))?;
        signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&cancelled))?;
    }
    let paths = paths(root, assets)?;
    match mode.as_str() {
        "--smoke" => {
            let model = model_path.ok_or(Error::Contract("missing smoke model"))?;
            if cameras.is_empty() {cameras = model_delivery::validation::cameras("");}
            let binding = openpilot_usbgpu::worker_artifact::bind(&model, &paths.assets)?;
            if expected_assets
                .as_deref()
                .is_some_and(|expected| expected != binding.manifest_sha256())
            {
                return Err(Error::Contract("native asset binding changed before smoke").into());
            }
            let reports = smoke::run_with_assets(&worker, &model, &cameras, &cancelled, &binding)?;
            emit(serde_json::to_string_pretty(&reports)?)?;
        }
        "--boot" => {
            let prepared = boot::prepare(&boot::Config {paths, worker, runner: std::env::current_exe()?, devices, identity_root}, &cancelled)?;
            emit(serde_json::json!({"prepared":prepared}))?;
        }
        "--prepare-for-startup" => {
            let delivered = model_delivery::background::prepare_for_startup(&paths, &manifest_url, retry_network, &cancelled)?;
            return Ok(if delivered { ExitCode::SUCCESS } else { ExitCode::FAILURE });
        }
        "--ensure-if-egpu" => {
            let params = openpilot_params::Params::for_runtime()?;
            model_delivery::background::run_retrying(&model_delivery::background::Config {
                paths: &paths,
                params: &params,
                devices: &devices,
                manifest_url: &manifest_url,
                network_wait_seconds,
            }, retry_network, &cancelled)?;
        }
        "--ready" => return Ok(if model::compiled_path(&paths).is_some() {ExitCode::SUCCESS} else {ExitCode::FAILURE}),
        "--active-sha" => emit(model::selected_manifest(&paths).map_or_else(String::new, |model| model.sha256))?,
        "--active-path" => emit(model::selected_manifest(&paths).map_or_else(String::new, |model| paths.cache.join(model.cache_filename()).display().to_string()))?,
        "--help" => emit("openpilot-usbgpu-model --boot|--present|--ready|--active-sha|--active-path|--ensure-if-egpu|--prepare-for-startup|--smoke MODEL.pkl [--camera WIDTHxHEIGHT]")?,
        _ => return Err(Error::Contract("unknown model command").into()),
    }
    Ok(ExitCode::SUCCESS)
}
fn main() -> ExitCode {
    match run() {
        Ok(exit) => exit,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
