use crate::{params::Backend, Error, Value};

pub enum Action {
    Reboot,
    Poweroff,
    Recalibrate,
}

#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error("Disengage first")]
    Engaged,
    #[error(transparent)]
    Boundary(#[from] Error),
}

pub fn run(action: Action, params: &mut Backend, engaged: bool) -> Result<Value, Failure> {
    if engaged {
        return Err(Failure::Engaged);
    }
    if !params.has_params() {
        match action {
            Action::Reboot => {
                spawn_reboot(0.0)?;
                return Ok(Value::object([("ok", Value::Bool(true))]));
            }
            Action::Poweroff | Action::Recalibrate => {
                return Err(Error::Source("params unavailable".into()).into())
            }
        }
    }
    let fresh = super::fresh::reopen(params.native_params())?
        .ok_or_else(|| Error::Source("params unavailable".into()))?;
    let mut params = Backend::native(fresh, std::path::PathBuf::new());
    match action {
        Action::Reboot => params.put("DoReboot", &Value::Bool(true), None)?,
        Action::Poweroff => params.put("DoShutdown", &Value::Bool(true), None)?,
        Action::Recalibrate => {
            for key in [
                "CalibrationParams",
                "LiveTorqueParameters",
                "LiveParameters",
                "LiveParametersV2",
                "LiveDelay",
            ] {
                match params.remove(key) {
                    Ok(()) | Err(_) => {}
                }
            }
            for key in ["OnroadCycleRequested", "DoReboot"] {
                match params.put(key, &Value::Bool(true), None) {
                    Ok(()) | Err(_) => {}
                }
            }
        }
    }
    Ok(Value::object([("ok", Value::Bool(true))]))
}

/// The shared helper checks device admission before a new session is created.
/// Its owned child completes the bounded chime/reboot sequence independently of
/// the request, while this reaper collects its exit status.
pub(crate) fn spawn_reboot(delay: f64) -> Result<(), Error> {
    let config = openpilot_hardware_control::reboot::Config::for_runtime()
        .map_err(|error| Error::Source(error.to_string()))?;
    let mut child = openpilot_hardware_control::reboot::spawn(&config, delay)
        .map_err(|error| Error::Source(error.to_string()))?;
    std::thread::spawn(move || {
        if let Err(error) = child.process.wait() {
            eprintln!("reboot child: {error}");
        }
    });
    Ok(())
}
