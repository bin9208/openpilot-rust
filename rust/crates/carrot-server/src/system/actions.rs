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
                let mut child = std::process::Command::new("sudo")
                    .arg("reboot")
                    .spawn()
                    .map_err(|error| crate::state::io_error(error, std::path::Path::new("sudo")))?;
                std::thread::spawn(move || {
                    if let Err(error) = child.wait() {
                        eprintln!("reboot child: {error}");
                    }
                });
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
