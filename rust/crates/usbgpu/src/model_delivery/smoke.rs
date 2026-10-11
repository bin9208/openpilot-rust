//! Execute the source smoke inputs through the owned native worker protocol.
use super::failure::{self, Kind};
use crate::{
    client::{Client, Frame, Launch},
    Error,
};
use serde::Serialize;
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Serialize)]
pub struct Report {
    pub camera: [u32; 2],
    pub checkpoint: String,
    pub load_seconds: f64,
    pub inference_seconds: Vec<f64>,
}

pub fn wall_time() -> Result<f64, Error> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64())
        .map_err(|error| std::io::Error::other(error).into())
}

fn kind(error: &Error) -> Kind {
    match error {
        Error::Io(error) => match error.kind() {
            std::io::ErrorKind::TimedOut => Kind::Timeout,
            std::io::ErrorKind::BrokenPipe => Kind::BrokenPipe,
            _ => Kind::Detail,
        },
        #[cfg(feature = "native-skip-miri")]
        Error::ModelRuntime(_) | Error::Library(_) => Kind::Detail,
        Error::Utf8(_)
        | Error::Json(_)
        | Error::UsbApi { .. }
        | Error::Protocol(_)
        | Error::Contract(_)
        | Error::ShortPower(_)
        | Error::Allocation(_)
        | Error::Timeout { .. }
        | Error::Cancelled => Kind::Detail,
    }
}

fn failed(model: &Path, error: Error, phase: &str) -> Result<Error, Error> {
    if !matches!(error, Error::Cancelled) {
        failure::record(model, &error.to_string(), kind(&error), phase, wall_time()?)?;
    }
    Ok(error)
}

/// Run five zero-image/identity-transform frames for each selected camera.
///
/// # Errors
/// Worker load/inference failures are persisted after owned child cleanup.
/// Cancellation releases the worker without rejecting its artifact.
pub fn run(
    worker: &Path,
    model: &Path,
    cameras: &[[u32; 2]],
    cancelled: &Arc<AtomicBool>,
) -> Result<Vec<Report>, Error> {
    run_inner(worker, model, cameras, cancelled, None)
}

pub fn run_with_assets(
    worker: &Path,
    model: &Path,
    cameras: &[[u32; 2]],
    cancelled: &Arc<AtomicBool>,
    binding: &crate::worker_artifact::Binding,
) -> Result<Vec<Report>, Error> {
    run_inner(worker, model, cameras, cancelled, Some(binding))
}

fn run_inner(
    worker: &Path,
    model: &Path,
    cameras: &[[u32; 2]],
    cancelled: &Arc<AtomicBool>,
    binding: Option<&crate::worker_artifact::Binding>,
) -> Result<Vec<Report>, Error> {
    let mut reports = Vec::new();
    for &camera in cameras {
        let started = Instant::now();
        let launch = Launch {
            worker,
            model,
            camera,
            timeout: Duration::from_secs(110),
            cancelled: Arc::clone(cancelled),
        };
        let client = match binding {
            Some(binding) => Client::launch_with_assets(launch, binding),
            None => Client::launch(launch),
        };
        let mut client = match client {
            Ok(client) => client,
            Err(error) => return Err(failed(model, error, "load")?),
        };
        let load_seconds = started.elapsed().as_secs_f64();
        let frames = vec![0; client.info.frame_size];
        let transforms = [[1., 0., 0., 0., 1., 0., 0., 0., 1.]; 2];
        let inputs = [0., 0., 0., 0., 0., 0., 0., 0., 1., 0., 0., 0.];
        let mut timings = Vec::with_capacity(5);
        for _ in 0..5 {
            let started = Instant::now();
            if let Err(error) = client.run(Frame {
                main: &frames,
                extra: &frames,
                transforms,
                inputs: &inputs,
            }) {
                drop(client);
                return Err(failed(model, error, "inference")?);
            }
            timings.push(started.elapsed().as_secs_f64());
        }
        reports.push(Report {
            camera,
            checkpoint: client.info.checkpoint.clone(),
            load_seconds,
            inference_seconds: timings,
        });
    }
    Ok(reports)
}
