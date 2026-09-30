use crate::{markdown, process::args, updater::Updater, Error};
use chrono::{Local, TimeZone};
use openpilot_logging::{
    log_site,
    producer::Logger,
    record::{Level, Record},
    Value,
};
use std::path::Path;

pub fn info(logger: &mut Logger, text: &str) -> Result<(), Error> {
    logger.emit(log_site!(), Record::text(Level::Info, text.into()))?;
    Ok(())
}
pub fn exception(logger: &mut Logger, text: &str, error: &Error) -> Result<(), Error> {
    logger.emit(
        log_site!(),
        Record::text(Level::Error, text.into()).with_exception(error.to_string()),
    )?;
    Ok(())
}
pub fn event(logger: &mut Logger, name: &str, fields: serde_json::Value) -> Result<(), Error> {
    let Value::Object(fields) = Value::from_json(fields)? else {
        return Err(Error::Contract("event fields must be object"));
    };
    logger.emit(log_site!(), Record::event(name, vec![], fields)?)?;
    Ok(())
}
impl Updater<'_> {
    pub fn get_description(&mut self, path: &Path) -> Result<String, Error> {
        if !path.exists() {
            return Ok(String::new());
        }
        let (mut version, mut branch, mut commit, mut date) =
            (String::new(), String::new(), String::new(), String::new());
        let result = (|| -> Result<(), Error> {
            branch = self.get_branch(path)?;
            commit = self.get_commit_hash(path)?.chars().take(7).collect();
            for name in ["common/version.h", "openpilot/common/version.h"] {
                let file = path.join(name);
                if file.exists() {
                    version = std::fs::read_to_string(file)?
                        .split('"')
                        .nth(1)
                        .ok_or(Error::Contract("version header has no quoted field"))?
                        .into();
                    break;
                }
            }
            let timestamp = self.context.commands.run(
                &args(&["git", "show", "-s", "--format=%ct", "HEAD"]),
                Some(path),
            )?;
            let timestamp = timestamp
                .trim_end()
                .parse::<i64>()
                .map_err(|_| Error::Contract("invalid commit timestamp"))?;
            date = Local
                .timestamp_opt(timestamp, 0)
                .single()
                .ok_or(Error::Contract("commit timestamp outside local range"))?
                .format("%b %d")
                .to_string();
            Ok(())
        })();
        if let Err(error) = result {
            if matches!(error, Error::Interrupted) {
                return Err(error);
            }
            exception(self.context.logger, "updater.get_description", &error)?;
        }
        Ok(format!("{version} / {branch} / {commit} / {date}"))
    }
    pub fn set_params(
        &mut self,
        success: bool,
        failed_count: u64,
        failure: Option<&str>,
    ) -> Result<(), Error> {
        self.params
            .put("UpdateFailedCount", failed_count.to_string().as_bytes())?;
        let target = self.target_branch()?;
        self.params.put("UpdaterTargetBranch", target.as_bytes())?;
        let available = self.update_available()?;
        self.params.put_bool("UpdaterFetchAvailable", available)?;
        if !self.branches.is_empty() {
            self.params.put(
                "UpdaterAvailableBranches",
                self.branches
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(",")
                    .as_bytes(),
            )?;
        }
        let mut last_uptime = self.params.number("UptimeOnroad")?;
        let mut last_routes = self.params.integer("RouteCount")?;
        if success {
            self.params
                .put_date("LastUpdateTime", self.context.now()?)?;
            self.params
                .put_number("LastUpdateUptimeOnroad", last_uptime)?;
            self.params
                .put("LastUpdateRouteCount", last_routes.to_string().as_bytes())?;
        } else {
            last_uptime = self.params.number("LastUpdateUptimeOnroad")?;
            last_routes = self.params.integer("LastUpdateRouteCount")?;
        }
        if let Some(failure) = failure {
            self.params.put("LastUpdateException", failure.as_bytes())?;
        } else {
            self.params.remove("LastUpdateException")?;
        }
        for (path, description_key, notes_key) in [
            (
                &self.context.paths.base.clone(),
                "UpdaterCurrentDescription",
                "UpdaterCurrentReleaseNotes",
            ),
            (
                &self.context.paths.finalized(),
                "UpdaterNewDescription",
                "UpdaterNewReleaseNotes",
            ),
        ] {
            let description = self.get_description(path)?;
            self.params.put(description_key, description.as_bytes())?;
            let notes = match markdown::release_notes(path) {
                Ok(notes) => notes,
                Err(error) => {
                    exception(self.context.logger, "failed to parse release notes", &error)?;
                    Vec::new()
                }
            };
            self.params.put(notes_key, &notes)?;
        }
        let ready = self.update_ready()?;
        self.params.put_bool("UpdateAvailable", ready)?;
        for alert in [
            "Offroad_UpdateFailed",
            "Offroad_ConnectivityNeeded",
            "Offroad_ConnectivityNeededPrompt",
        ] {
            self.params.alert(alert, false, None)?;
        }
        let hours = (self.params.number("UptimeOnroad")? - last_uptime) / 3600.;
        let routes = self
            .params
            .integer("RouteCount")?
            .checked_sub(last_routes)
            .ok_or(Error::Contract("route counter difference overflow"))?;
        let build = openpilot_runtime_version::get_build_metadata(&self.context.paths.base)?;
        if failed_count > 15 && failure.is_some() && self.has_internet {
            let text = if build.tested_channel() {
                "Ensure the software is correctly installed. Uninstall and re-install if this error persists."
            } else {
                failure.unwrap_or("")
            };
            self.params
                .alert("Offroad_UpdateFailed", true, Some(text))?;
        } else if failed_count > 0 {
            if hours > 2700. && routes > 8400 {
                self.params
                    .alert("Offroad_ConnectivityNeeded", true, None)?;
            } else if hours > 2300. && routes > 8000 {
                let difference = 2700. - hours;
                let remaining = difference.max(1.);
                let number = if difference >= 1. {
                    crate::params::python_float(difference)?
                } else {
                    "1".into()
                };
                let text = format!("{number} hour{}.", if remaining == 1. { "" } else { "s" });
                self.params
                    .alert("Offroad_ConnectivityNeededPrompt", true, Some(&text))?;
            }
        }
        Ok(())
    }
}
