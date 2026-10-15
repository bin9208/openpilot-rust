mod network;
use super::{
    ensure_fallible, fetch_manifest, presence, smoke,
    status::{Phase, Reporter, Values},
    Event,
};
use crate::model::{self, Manifest, Paths};
use openpilot_params::Params;
use std::{
    io::Write,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

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

struct DeliveryConfig<'a> {
    paths: &'a Paths,
    manifest_url: &'a str,
    network_wait_seconds: f64,
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

fn update(
    config: &DeliveryConfig<'_>,
    reporter: &mut Reporter,
    retry_network: bool,
    cancelled: &AtomicBool,
) -> Result<bool, Error> {
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
    let packaged = match super::source_manifest(config.manifest_url) {
        Some(model) => super::packaged::seed(config.paths, &model)?,
        None => false,
    };
    if !retry_network
        && !packaged
        && active(config.paths)?.is_none()
        && config.network_wait_seconds > 0.0
    {
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
            if cancelled.load(Ordering::Acquire) {
                return Err(super::Error::Native(crate::Error::Cancelled));
            }
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
    super::bind_manifest(&config.paths.cache, &model, config.manifest_url)?;
    let active = active(config.paths)?
        .ok_or_else(|| super::Error::Invalid("verified active model unavailable".into()))?;
    let complete = || Values {
        model: Some(&active),
        downloaded: Some(active.size),
        detail: None,
    };
    if active.precompiled_only() {
        if super::precompiled::installed(&active, &config.paths.cache, &config.paths.assets)?
            .is_some()
        {
            reporter.update(
                Phase::Compiled,
                complete(),
                true,
                started.elapsed(),
                smoke::wall_time()?,
            )?;
            return Ok(true);
        }
        reporter.update(
            Phase::Installing,
            Values {
                detail: Some("installing verified eGPU runtime package"),
                ..complete()
            },
            true,
            started.elapsed(),
            smoke::wall_time()?,
        )?;
        let target = super::precompiled::ensure_fallible(
            &super::agent(15),
            &super::agent(30),
            &active,
            &config.paths.cache,
            &config.paths.assets,
            &mut |_, _| {
                if cancelled.load(Ordering::Acquire) {
                    Err(super::Error::Native(crate::Error::Cancelled))
                } else {
                    Ok(())
                }
            },
        )?;
        if target.is_none() {
            reporter.error(
                Phase::Error,
                Values {
                    detail: Some(
                        "model rejected after an earlier runtime failure; see diagnostic report",
                    ),
                    ..complete()
                },
                super::delivery_error::Classification {
                    code: "rejected",
                    retryable: false,
                },
                0,
                0,
                smoke::wall_time()?,
            )?;
            return Ok(false);
        }
        if super::precompiled::installed(&active, &config.paths.cache, &config.paths.assets)?
            .is_none()
        {
            return Err(
                super::Error::Invalid("installed runtime package is incomplete".into()).into(),
            );
        }
        reporter.update(
            Phase::Installed,
            Values {
                detail: Some("verified model and runtime installed; used on next model start"),
                ..complete()
            },
            true,
            started.elapsed(),
            smoke::wall_time()?,
        )?;
    } else {
        reporter.update(
            if model::active_compiled_path(config.paths).is_some() {
                Phase::Compiled
            } else {
                Phase::Ready
            },
            complete(),
            true,
            started.elapsed(),
            smoke::wall_time()?,
        )?;
    }
    writeln!(
        std::io::stdout().lock(),
        "big model {}: {}",
        if changed { "updated" } else { "ready" },
        path.display()
    )?;
    Ok(true)
}

pub fn run(config: &Config<'_>) -> Result<(), Error> {
    run_retrying(config, false, &AtomicBool::new(false))
}

pub fn run_retrying(
    config: &Config<'_>,
    retry_network: bool,
    cancelled: &AtomicBool,
) -> Result<(), Error> {
    if config.network_wait_seconds < 0.0 {
        return Err(
            super::Error::Invalid("network wait seconds must be non-negative".into()).into(),
        );
    }
    let present = presence::present(config.devices);
    let compiled = model::compiled_path(config.paths).is_some();
    let known = remembered(config.params)?;
    if !(known || present || compiled) {
        return Ok(());
    }
    if !known {
        remember(config.params)?;
    }
    deliver(
        &DeliveryConfig {
            paths: config.paths,
            manifest_url: config.manifest_url,
            network_wait_seconds: config.network_wait_seconds,
        },
        retry_network,
        cancelled,
    )
    .map(|_| ())
}

/// Startup is explicitly requested by the launcher: no hardware-history gate and no GPU open.
/// Unlike background delivery, terminal failure must return false to the launcher.
pub fn prepare_for_startup(
    paths: &Paths,
    manifest_url: &str,
    retry_network: bool,
    cancelled: &AtomicBool,
) -> Result<bool, Error> {
    deliver(
        &DeliveryConfig {
            paths,
            manifest_url,
            network_wait_seconds: 0.0,
        },
        retry_network,
        cancelled,
    )
}

fn deliver(
    config: &DeliveryConfig<'_>,
    retry_network: bool,
    cancelled: &AtomicBool,
) -> Result<bool, Error> {
    let mut reporter = Reporter::new(&config.paths.cache, smoke::wall_time()?);
    let mut attempts = 0u32;
    while !cancelled.load(Ordering::Acquire) {
        match update(config, &mut reporter, retry_network, cancelled) {
            Ok(delivered) => return Ok(delivered),
            Err(error) => {
                if cancelled.load(Ordering::Acquire) {
                    return Ok(false);
                }
                let class = match &error {
                    Error::Delivery(error) => super::delivery_error::classify(error),
                    Error::Io(error) => super::delivery_error::io(error),
                    Error::Params(openpilot_params::Error::Io(error))
                    | Error::Clock(crate::Error::Io(error)) => super::delivery_error::io(error),
                    _ => super::delivery_error::Classification {
                        code: "install",
                        retryable: false,
                    },
                };
                attempts = attempts.saturating_add(1);
                let retry = retry_network && class.retryable;
                let detail: String = error.to_string().chars().take(2000).collect();
                reporter.error(
                    if retry {
                        Phase::WaitingForNetwork
                    } else {
                        Phase::Error
                    },
                    Values {
                        model: None,
                        downloaded: None,
                        detail: Some(&detail),
                    },
                    class,
                    attempts,
                    if retry { 30 } else { 0 },
                    smoke::wall_time()?,
                )?;
                writeln!(
                    std::io::stderr().lock(),
                    "eGPU delivery {}: {}; {}",
                    class.code,
                    error,
                    if retry {
                        "retrying in 30 seconds"
                    } else {
                        "stopped"
                    }
                )?;
                if !retry {
                    return Ok(false);
                }
                let until = Instant::now() + Duration::from_secs(30);
                while Instant::now() < until && !cancelled.load(Ordering::Acquire) {
                    std::thread::park_timeout(
                        until
                            .saturating_duration_since(Instant::now())
                            .min(Duration::from_millis(200)),
                    );
                }
            }
        }
    }
    Ok(false)
}
