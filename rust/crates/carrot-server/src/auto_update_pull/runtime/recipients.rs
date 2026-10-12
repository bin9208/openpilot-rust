use crate::{Error, Value};
use openpilot_params::Params;
use std::{fs, path::Path};

pub(super) fn alert(
    params: Option<&Params>,
    repository: &Path,
    show: bool,
    detail: &Value,
) -> Result<(), Error> {
    let params = params.ok_or_else(|| Error::Source("Params unavailable".into()))?;
    let name = "Offroad_CarrotAutoUpdateFailed";
    if show {
        let definitions = Value::parse(&fs::read_to_string(
            repository.join("openpilot/selfdrive/selfdrived/alerts_offroad.json"),
        )?)?;
        let mut alert = definitions.get(name).clone();
        crate::json_fields::set(&mut alert, "extra", detail.clone())?;
        param_result(params.put(name, alert.encode()?.as_bytes()))?;
    } else {
        param_result(params.remove(name))?;
    }
    Ok(())
}

pub(super) fn param_result(result: Result<(), openpilot_params::Error>) -> Result<(), Error> {
    match result {
        Ok(()) | Err(openpilot_params::Error::Io(_)) => Ok(()),
        Err(
            error @ (openpilot_params::Error::UnknownKey(_)
            | openpilot_params::Error::InvalidPrefix),
        ) => Err(error.into()),
    }
}
