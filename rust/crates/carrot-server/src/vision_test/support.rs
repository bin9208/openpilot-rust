//! Shared native Carrot Vision diagnostic boundaries.
use crate::{Error, Value};
use openpilot_msgq::{VisionClient, VisionStream};
use std::{ffi::OsString, path::PathBuf};
#[derive(Clone)]
pub struct CommandSpec {
    pub path: PathBuf,
    pub args: Vec<OsString>,
    pub pattern: String,
}
pub(crate) fn streams() -> Vec<i32> {
    let mut streams: Vec<_> = VisionClient::available_streams("camerad")
        .unwrap_or_default()
        .into_iter()
        .map(|stream| match stream {
            VisionStream::Road => 0,
            VisionStream::Driver => 1,
            VisionStream::WideRoad => 2,
            VisionStream::Map => 3,
        })
        .collect();
    streams.sort_unstable();
    streams
}
pub(crate) fn snapshot_active(
    params: &openpilot_params::Params,
    repository: &std::path::Path,
    active: bool,
    component: &str,
) -> Result<(), Error> {
    params.put_bool("IsTakingSnapshot", active)?;
    let alert = "Offroad_IsTakingSnapshot";
    if active {
        let saved = (|| -> Result<(), Error> {
            let mut value = crate::state::read(
                &repository.join("openpilot/selfdrive/selfdrived/alerts_offroad.json"),
            )
            .get(alert)
            .clone();
            if !matches!(value, Value::Object(_)) {
                return Ok(());
            }
            crate::json_fields::set(&mut value, "extra", Value::text(""))?;
            params.put(alert, value.encode()?.as_bytes())?;
            Ok(())
        })();
        if let Err(error) = saved {
            eprintln!("[{component}] snapshot alert: {error}");
        }
    } else if let Err(error) = params.remove(alert) {
        eprintln!("[{component}] snapshot alert: {error}");
    }
    Ok(())
}
