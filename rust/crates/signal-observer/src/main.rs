use openpilot_logging::{
    Fields, Value, log_site,
    producer::Factory,
    record::{Level, Record},
};
use openpilot_plannerd::signal::transport::{Mode, ModeCache};
use openpilot_signal_observer::{
    backend::Backend,
    entry::Entry,
    runtime::{self, Config, Error},
};
use std::{
    env, io,
    process::ExitCode,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
fn run() -> Result<(), Error> {
    let mut config = Config::default();
    let mut child = None;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--help" => {
                println!(
                    "openpilot-signalcolord [--frames N] [--duration SECONDS]\nIsolated native road-camera signal observer."
                );
                return Ok(());
            }
            "--backend-child" => {
                if child.is_some() {
                    return Err("duplicate signal backend child selection".into());
                }
                child = Some(match arguments.next().as_deref() {
                    Some("tracking_cpu") => Backend::TrackingCpu,
                    Some("qcom") => Backend::Qcom,
                    _ => return Err("invalid signal backend child selection".into()),
                });
            }
            "--frames" => {
                let value = arguments
                    .next()
                    .ok_or("missing frame count")?
                    .parse::<u64>()?;
                if value == 0 || config.frames.is_some() {
                    return Err("positive unique frame count required".into());
                }
                config.frames = Some(value);
            }
            "--duration" => {
                let value = arguments.next().ok_or("missing duration")?.parse::<f64>()?;
                if !value.is_finite() || value <= 0. || config.duration.is_some() {
                    return Err("positive finite unique duration required".into());
                }
                config.duration = Some(Duration::from_secs_f64(value));
            }
            _ => return Err("unknown signal observer argument".into()),
        }
    }
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, stop.clone())?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, stop.clone())?;
    let params = openpilot_params::Params::for_runtime()?;
    let mut cache = ModeCache::default();
    let mut mode = || {
        cache.read(
            runtime::monotonic_ns() as f64 / 1e9,
            || -> Result<i64, openpilot_params::Error> {
                Ok(params
                    .get("SignalAssistMode")?
                    .map(|bytes| runtime::parameter_value(&bytes))
                    .unwrap_or(0))
            },
        )
    };
    if mode() == Mode::Off && child.is_none() {
        return Ok(());
    }
    let mut logger = Factory::for_runtime()?.logger();
    let mut event = |name: &str, value: serde_json::Value| -> Result<(), Error> {
        let object = value.as_object().ok_or("event fields must be object")?;
        let fields: Fields = object
            .iter()
            .map(|(k, v)| Ok((k.clone(), Value::from_json(v.clone())?)))
            .collect::<Result<_, openpilot_logging::Error>>()?;
        logger.emit(log_site!(), Record::event(name, vec![], fields)?)?;
        Ok(())
    };
    let provider = env::var_os("OPENPILOT_SIGNAL_ORT_ROOT").map(std::path::PathBuf::from);
    let outcome = Entry {
        config: &config,
        stop: &stop,
        params: &params,
        provider: provider.as_deref(),
        child,
    }
    .run(&mut mode, &mut event, &mut |backend| {
        let name = match backend {
            Backend::TrackingCpu => "tracking_cpu",
            Backend::Qcom => "qcom",
            Backend::Cpu => {
                return Err("automatic signal child cannot select CPU neural fallback".into());
            }
        };
        let mut command = std::process::Command::new(env::current_exe()?);
        command.arg("--backend-child").arg(name);
        Ok(command)
    });
    if child.is_some() || config.frames.is_some() || config.duration.is_some() {
        return outcome;
    }
    openpilot_signal_observer::lifecycle::finish(
        outcome,
        |error| {
            logger.emit(
                log_site!(),
                Record::text(
                    Level::Error,
                    openpilot_signal_observer::lifecycle::FAILURE_MESSAGE.into(),
                )
                .with_exception(error.to_string()),
            )?;
            Ok::<_, Error>(())
        },
        || mode().enabled(),
        &stop,
        std::thread::sleep,
    )
}
fn main() -> ExitCode {
    match openpilot_managed_entry::runtime(
        "openpilot.selfdrive.modeld.signal_color_shadow",
        "signalcolord",
        || {
            run().map_err(|e| {
                openpilot_managed_entry::StepError::raised(io::Error::other(e.to_string()))
            })
        },
    ) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("signalcolord: {error}");
            ExitCode::FAILURE
        }
    }
}
