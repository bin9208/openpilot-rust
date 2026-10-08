use crate::Error;
use chrono::{Datelike, TimeZone, Utc};
use openpilot_params::Params;
use std::{fs, path::Path, process::Command};

pub fn set_time(epoch: i64, timezone: &str, params: &Params) -> Result<(), Error> {
    apply(
        epoch,
        timezone,
        params,
        Path::new("/data/etc/localtime"),
        Utc::now().timestamp_millis(),
        |name, args| Ok(Command::new(name).args(args).status()?.success()),
    )
}
pub fn apply(
    epoch: i64,
    timezone: &str,
    params: &Params,
    localtime: &Path,
    now_millis: i64,
    mut run: impl FnMut(&str, &[String]) -> std::io::Result<bool>,
) -> Result<(), Error> {
    let new_time = Utc
        .timestamp_opt(epoch, 0)
        .single()
        .filter(|t| (1..=9999).contains(&t.year()))
        .ok_or(Error::Contract("epoch datetime range"))?;
    let no_timezone = localtime
        .metadata()
        .map_or(true, |metadata| metadata.len() == 0);
    if now_millis.abs_diff(new_time.timestamp_millis()) < 10_000 && !no_timezone {
        return Ok(());
    }
    if fs::symlink_metadata(localtime).is_ok()
        && !run(
            "sudo",
            &[
                "rm".into(),
                "-f".into(),
                localtime.to_string_lossy().into_owned(),
            ],
        )?
    {
        return Ok(());
    }
    if run(
        "sudo",
        &[
            "ln".into(),
            "-s".into(),
            format!("/usr/share/zoneinfo/{timezone}"),
            localtime.to_string_lossy().into_owned(),
        ],
    )? {
        params.put("TimezoneName", timezone.as_bytes())?;
        params.put("TimezoneSource", b"app")?;
    }
    let _ = run(
        "sh",
        &[
            "-c".into(),
            format!("TZ=UTC date -s '{}'", new_time.format("%Y-%m-%d %H:%M:%S")),
        ],
    )?;
    Ok(())
}
