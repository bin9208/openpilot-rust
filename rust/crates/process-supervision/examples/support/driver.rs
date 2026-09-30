use crate::protocol::{Action, Config, Launch, Race};
use openpilot_cereal::log_capnp::manager_state::process_state;
use openpilot_logging::producer::Factory;
use openpilot_process_supervision::{
    ensure_running, Error, Execution, ManagedProcess, NativeCommand, ParamsSource,
    PersistentCommand, PersistentDaemonProcess, ProcessLog, ProcessPolicy, Signal, StopOptions,
};
use serde_json::{json, Value};
use std::{
    fs,
    time::{Duration, Instant},
};

pub fn processes(config: Config) -> Result<Vec<ManagedProcess>, Error> {
    let factory = Factory::new(config.endpoint)?;
    let logger = ProcessLog::new(factory.logger());
    config
        .processes
        .into_iter()
        .map(|spec| {
            let execution = match spec.launch {
                Launch::Native { cwd, argv } => Execution::Native(NativeCommand {
                    launcher: config.launcher.clone(),
                    basedir: config.basedir.clone(),
                    cwd,
                    argv: argv.into_iter().map(Into::into).collect(),
                }),
                Launch::Persistent {
                    argv,
                    identity,
                    param,
                } => Execution::Persistent(PersistentDaemonProcess::new(
                    PersistentCommand {
                        launcher: config.launcher.clone(),
                        argv: argv.into_iter().map(Into::into).collect(),
                        identity,
                        pid_param: param,
                    },
                    ParamsSource::Directory {
                        root: config.params_root.clone(),
                        prefix: config.prefix.clone(),
                    },
                )),
            };
            let mut process = ManagedProcess::new(spec.name, execution, logger.clone());
            process.policy = ProcessPolicy {
                enabled: spec.enabled,
                sigkill: spec.sigkill,
                restart_if_crash: spec.restart_if_crash,
            };
            Ok(process)
        })
        .collect()
}

fn signal(value: i32) -> Result<Signal, Error> {
    match value {
        2 => Ok(Signal::INT),
        9 => Ok(Signal::KILL),
        10 => Ok(Signal::USR1),
        15 => Ok(Signal::TERM),
        _ => Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "unsupported fixture signal",
        ))),
    }
}

fn selected<'a>(
    processes: &'a mut [ManagedProcess],
    name: &str,
) -> Result<&'a mut ManagedProcess, Error> {
    processes
        .iter_mut()
        .find(|p| p.name() == name)
        .ok_or_else(|| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "unknown fixture process",
            ))
        })
}

fn release(race: &Race) -> Result<(), Error> {
    let temporary = race.release.with_extension("tmp");
    fs::write(&temporary, "7")?;
    fs::rename(temporary, &race.release)?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let stat = fs::read_to_string(format!("/proc/{}/stat", race.pid))?;
        if stat
            .rsplit_once(") ")
            .is_some_and(|(_, rest)| rest.starts_with('Z'))
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "fixture did not exit during predicate",
            )));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub fn action(processes: &mut [ManagedProcess], action: Action) -> Result<Value, Error> {
    match action {
        Action::Start { name } => {
            selected(processes, &name)?.start()?;
            Ok(Value::Null)
        }
        Action::Restart { name } => {
            selected(processes, &name)?.restart()?;
            Ok(Value::Null)
        }
        Action::Prepare { name } => {
            selected(processes, &name)?.prepare();
            Ok(Value::Null)
        }
        Action::State { name } => Ok(serde_json::to_value(selected(processes, &name)?.state()?)?),
        Action::Stop {
            name,
            retry,
            block,
            signal: raw,
        } => Ok(json!(selected(processes, &name)?.stop(StopOptions {
            retry,
            block,
            signal: raw.map(signal).transpose()?
        })?)),
        Action::Signal { name, signal: raw } => {
            selected(processes, &name)?.signal(signal(raw)?)?;
            Ok(Value::Null)
        }
        Action::Ensure {
            allowed,
            not_run,
            race,
        } => {
            let mut predicates = Vec::new();
            let excluded: Vec<_> = not_run.iter().map(String::as_str).collect();
            let running = ensure_running(processes, &excluded, |name| {
                predicates.push(name.to_owned());
                if let Some(race) = &race {
                    if race.name == name {
                        release(race)?;
                    }
                }
                Ok(allowed.iter().any(|item| item == name))
            })?;
            Ok(
                json!({"running": running.into_iter().map(|i| processes[i].name()).collect::<Vec<_>>(), "predicates": predicates}),
            )
        }
        Action::Exit => Ok(Value::Null),
    }
}

pub fn snapshots(processes: &mut [ManagedProcess]) -> Result<Value, Error> {
    let mut values = Vec::new();
    for process in processes {
        let state = process.state()?;
        let mut message = capnp::message::Builder::new_default();
        state.write(message.init_root::<process_state::Builder<'_>>());
        let mut wire = Vec::new();
        capnp::serialize::write_message(&mut wire, &message).map_err(std::io::Error::other)?;
        values.push(json!({"state": state, "wire": wire, "has_process": process.has_process(), "shutting_down": process.shutting_down()}));
    }
    Ok(json!(values))
}

pub fn error(error: &Error) -> Value {
    let kind = match error {
        Error::PidRange => "OverflowError",
        Error::Io(error) if error.kind() == std::io::ErrorKind::NotFound => "FileNotFoundError",
        Error::Io(_) => "OSError",
        Error::Utf8(_) => "UnicodeDecodeError",
        Error::EmptyCommand => "IndexError",
        Error::PidKeyType(_) => "TypeError",
        Error::Params(openpilot_params::Error::UnknownKey(_)) => "UnknownKeyName",
        Error::Params(_)
        | Error::Logging(_)
        | Error::Json(_)
        | Error::Nul(_)
        | Error::MissingParams
        | Error::ReaperPoisoned
        | Error::LaunchProtocol(_) => "Error",
    };
    json!({"kind": kind, "message": error.to_string()})
}
