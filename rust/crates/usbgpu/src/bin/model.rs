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
    let cache = match std::env::var_os("CARROT_BIG_MODEL_DIR") {
        Some(path) => path.into(),
        None if std::path::Path::new("/TICI").is_file() => "/data/media/0/carrot/models".into(),
        None => PathBuf::from(
            std::env::var_os("HOME").ok_or(Error::Contract("missing home directory"))?,
        )
        .join(".comma/models"),
    };
    Ok(Paths {
        models: root.join("openpilot/selfdrive/modeld/models"),
        cache,
        assets,
    })
}
fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1).collect::<VecDeque<_>>();
    let mode = argument(&mut args)?;
    let mut root = std::env::current_dir()?;
    let mut assets = std::env::current_exe()?.with_file_name("usbgpu-assets");
    let mut worker = std::env::current_exe()?.with_file_name("openpilot-usbgpu-worker");
    let mut devices = PathBuf::from(hardware::SYSFS);
    let mut identity_root = PathBuf::from("/");
    let mut expected_assets = None;
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
            "--camera" => cameras.push(match argument(&mut args)?.as_str() {
                "1928x1208" => [1928, 1208],
                "1344x760" => [1344, 760],
                _ => return Err(Error::Contract("unsupported smoke camera").into()),
            }),
            _ => return Err(Error::Contract("unknown model command argument").into()),
        }
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&cancelled))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&cancelled))?;
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
        "--ready" => return Ok(if model::active_compiled_path(&paths).is_some() {ExitCode::SUCCESS} else {ExitCode::FAILURE}),
        "--active-sha" => emit(model::active_manifest(&paths).map_or_else(String::new, |model| model.sha256))?,
        "--active-path" => emit(model::active_manifest(&paths).map_or_else(String::new, |model| paths.cache.join(model.cache_filename()).display().to_string()))?,
        "--help" => emit("openpilot-usbgpu-model --boot|--ready|--active-sha|--active-path|--smoke MODEL.pkl [--camera WIDTHxHEIGHT]")?,
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
