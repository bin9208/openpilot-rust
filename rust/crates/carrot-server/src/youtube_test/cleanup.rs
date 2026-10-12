use super::{config::Config, process, status, storage};
use crate::{Error, Value};
use std::time::Duration;

pub(super) fn snapshot(config: &Config, active: bool) -> Result<(), Error> {
    snapshot_active(&config.params, &config.repository, active, "youtube-test")
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
            let mut value = storage::json(
                &repository.join("openpilot/selfdrive/selfdrived/alerts_offroad.json"),
            )
            .get(alert)
            .clone();
            if !matches!(value, Value::Object(_)) {
                return Ok(());
            }
            storage::set(&mut value, "extra", Value::text(""));
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
pub(super) fn restore(config: &Config, state: &Value) -> Result<(), Error> {
    if state.get("forced_live").truth() && status::param_integer(config, "CarrotYouTubeLive") > 0 {
        let previous = storage::integer(state.get("previous_live"))?;
        config
            .params
            .put("CarrotYouTubeLive", previous.to_string().as_bytes())?;
    }
    Ok(())
}
pub(super) async fn children(config: &Config, state: &Value) -> Result<(), Error> {
    let quality = i32::try_from(storage::integer(state.get("quality"))?).unwrap_or(0);
    for (name, spec) in config.child_specs(quality).into_iter().rev() {
        let pid = i32::try_from(storage::integer(state.get("children").get(&name))?).unwrap_or(0);
        process::terminate(pid, &spec.pattern, Duration::from_secs(3)).await?;
    }
    Ok(())
}
