use crate::Error;

pub(super) fn write_status(result: Result<(), openpilot_params::Error>) -> Result<(), Error> {
    match result {
        Ok(()) | Err(openpilot_params::Error::Io(_)) => Ok(()),
        Err(error) => Err(error.into()),
    }
}
