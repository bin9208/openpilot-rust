use crate::Error;
use openpilot_params::Params;

pub(crate) fn reopen(params: Option<&Params>) -> Result<Option<Params>, Error> {
    let Some(params) = params else {
        return Ok(None);
    };
    if params.directory().exists() {
        return Ok(Some(params.clone()));
    }
    let directory = params.directory();
    let root = directory
        .parent()
        .ok_or_else(|| Error::Source("invalid Params path".into()))?;
    let prefix = directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Source("invalid Params prefix".into()))?;
    Params::open(root, prefix)
        .map(Some)
        .map_err(|error| match error {
            openpilot_params::Error::Io(error) => Error::Source(format!(
                "Failed to ensure params path, errno={}, path={}, param_prefix=/{prefix}",
                error.raw_os_error().unwrap_or(0),
                root.display(),
            )),
            error @ (openpilot_params::Error::InvalidPrefix
            | openpilot_params::Error::UnknownKey(_)) => error.into(),
        })
}
