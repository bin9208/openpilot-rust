use crate::{
    tools::runner::{Command, Runner},
    Error,
};
use openpilot_dashcam_upload::metadata;
use std::{collections::BTreeMap, path::Path, time::Duration};

#[derive(Clone)]
pub(crate) struct Metadata {
    pub car: String,
    pub dongle: String,
    pub serial: String,
    pub branch: String,
    pub commit: String,
    pub date: String,
}
async fn git(repository: &Path, runner: &Runner, args: &[&str]) -> String {
    let argv: Vec<_> = std::iter::once("git")
        .chain(args.iter().copied())
        .map(str::to_owned)
        .collect();
    let output = runner
        .raw_capture(Command {
            argv: &argv,
            cwd: Some(repository),
            timeout: Some(Duration::from_secs(4)),
        })
        .await;
    let read = || -> Result<String, Error> {
        let output = output.map_err(|error| Error::Source(error.to_string()))?;
        let out = crate::request_text::decode(&output.stdout, "utf-8")?;
        crate::request_text::decode(&output.stderr, "utf-8")?;
        if !output.status.success() {
            return Ok("unknown".into());
        }
        let out = out.trim_matches(crate::tools::text::whitespace);
        Ok(if out.is_empty() { "unknown" } else { out }.into())
    };
    read().unwrap_or_else(|_| "unknown".into())
}
pub(crate) async fn collect(
    repository: &Path,
    params: Option<openpilot_params::Params>,
    runner: &Runner,
) -> Result<Metadata, Error> {
    let basic = tokio::task::spawn_blocking(move || -> Result<_, Error> {
        let mut logger = openpilot_logging::producer::Logger::for_runtime_retained()
            .map_err(|error| Error::Source(error.to_string()))?;
        let environment: BTreeMap<_, _> = ["CARROT_DEVICE_SERIAL", "DEVICE_SERIAL", "SERIAL"]
            .into_iter()
            .filter_map(|name| std::env::var(name).ok().map(|value| (name.into(), value)))
            .collect();
        Ok((
            metadata::param_text(params.as_ref(), "CarName", "none", &mut logger),
            metadata::param_text(params.as_ref(), "DongleId", "unknown", &mut logger),
            metadata::device_serial(
                params.as_ref(),
                &environment,
                &metadata::hardware_serial(),
                &mut logger,
            ),
        ))
    })
    .await
    .map_err(|error| Error::Source(error.to_string()))??;
    Ok(Metadata {
        car: basic.0,
        dongle: basic.1,
        serial: basic.2,
        branch: git(repository, runner, &["branch", "--show-current"]).await,
        commit: git(repository, runner, &["rev-parse", "--short", "HEAD"]).await,
        date: git(
            repository,
            runner,
            &[
                "show",
                "-s",
                "--date=format:%Y-%m-%d %H:%M:%S",
                "--format=%cd",
                "HEAD",
            ],
        )
        .await,
    })
}
