mod network;
use super::{
    ensure_fallible, fetch_manifest, presence, smoke,
    status::{Phase, Reporter, Values},
    Event,
};
use crate::model::{self, Manifest, Paths};
use openpilot_params::Params;
use std::{io::Write, path::Path, time::Instant};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Delivery(#[from] super::Error),
    #[error(transparent)]
    Params(#[from] openpilot_params::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Clock(#[from] crate::Error),
}

pub struct Config<'a> {
    pub paths: &'a Paths,
    pub params: &'a Params,
    pub devices: &'a Path,
    pub manifest_url: &'a str,
    pub network_wait_seconds: f64,
}

fn active(paths: &Paths) -> Result<Option<Manifest>, Error> {
    let Some(model) = super::State::read(&paths.cache)?.active else {
        return Ok(None);
    };
    let path = paths.cache.join(model.cache_filename());
    if path.is_file() && path.metadata()?.len() == model.size {
        Ok(Some(model))
    } else {
        Ok(None)
    }
}

fn remembered(params: &Params) -> Result<bool, openpilot_params::Error> {
    match params.get_bool("UsbGpuHardwareSeen") {
        Ok(value) => Ok(value),
        Err(openpilot_params::Error::Io(_)) => Ok(false),
        Err(error) => Err(error),
    }
}

fn remember(params: &Params) -> Result<(), openpilot_params::Error> {
    match params.put_bool("UsbGpuHardwareSeen", true) {
        Ok(()) | Err(openpilot_params::Error::Io(_)) => Ok(()),
        Err(error) => Err(error),
    }
}

fn update(config: &Config<'_>, reporter: &mut Reporter) -> Result<(), Error> {
    let started = Instant::now();
    let empty = || Values {
        model: None,
        downloaded: None,
        detail: None,
    };
    reporter.update(
        Phase::Checking,
        Values {
            detail: Some("checking model catalog"),
            ..empty()
        },
        true,
        started.elapsed(),
        smoke::wall_time()?,
    )?;
    if active(config.paths)?.is_none() && config.network_wait_seconds > 0.0 {
        writeln!(
            std::io::stdout().lock(),
            "waiting up to {}s for the big model server",
            config.network_wait_seconds
        )?;
        reporter.update(
            Phase::Checking,
            Values {
                detail: Some("waiting for network"),
                ..empty()
            },
            true,
            started.elapsed(),
            smoke::wall_time()?,
        )?;
        if !network::wait(config.manifest_url, config.network_wait_seconds)? {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "big model server unavailable after {}s",
                    config.network_wait_seconds
                ),
            )
            .into());
        }
    }
    let model = fetch_manifest(&super::agent(15), config.manifest_url)?;
    let (path, changed) = ensure_fallible(
        &super::agent(30),
        &model,
        &config.paths.cache,
        &mut |event| {
            let (phase, downloaded, force) = match event {
                Event::Progress { downloaded, total } => {
                    (Phase::Downloading, downloaded, downloaded >= total)
                }
                Event::Verifying => (Phase::Verifying, model.size, true),
            };
            reporter.update(
                phase,
                Values {
                    model: Some(&model),
                    downloaded: Some(downloaded),
                    detail: None,
                },
                force,
                started.elapsed(),
                smoke::wall_time()?,
            )?;
            Ok(())
        },
    )?;
    let active = active(config.paths)?;
    reporter.update(
        if model::active_compiled_path(config.paths).is_some() {
            Phase::Compiled
        } else {
            Phase::Ready
        },
        Values {
            model: active.as_ref(),
            downloaded: active.as_ref().map(|model| model.size),
            detail: None,
        },
        true,
        started.elapsed(),
        smoke::wall_time()?,
    )?;
    writeln!(
        std::io::stdout().lock(),
        "big model {}: {}",
        if changed { "updated" } else { "ready" },
        path.display()
    )?;
    Ok(())
}

pub fn run(config: &Config<'_>) -> Result<(), Error> {
    if config.network_wait_seconds < 0.0 {
        return Err(
            super::Error::Invalid("network wait seconds must be non-negative".into()).into(),
        );
    }
    let present = presence::present(config.devices);
    let compiled = active(config.paths)?.is_some_and(|model| {
        super::precompiled::installed(&model, &config.paths.cache, &config.paths.assets)
            .is_ok_and(|path| path.is_some())
    });
    let known = remembered(config.params)?;
    if !(known || present || compiled) {
        return Ok(());
    }
    if !known {
        remember(config.params)?;
    }
    let mut reporter = Reporter::new(&config.paths.cache, smoke::wall_time()?);
    if let Err(error) = update(config, &mut reporter) {
        reporter.update(
            Phase::Error,
            Values {
                model: None,
                downloaded: None,
                detail: Some(&error.to_string()),
            },
            true,
            std::time::Duration::ZERO,
            smoke::wall_time()?,
        )?;
        writeln!(
            std::io::stderr().lock(),
            "big model update skipped: {error}"
        )?;
    }
    Ok(())
}
