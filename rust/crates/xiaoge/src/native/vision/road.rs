use super::super::{platform, shared::Shared, Error};
use super::{camera::Camera, publisher::Publisher};
use crate::{
    inference::LaneModel,
    service::{state::Timing, Stream},
};
use std::{
    path::Path,
    sync::{atomic::Ordering, mpsc, Arc},
};

pub fn run(
    shared: Arc<Shared>,
    publisher: Publisher,
    ready: mpsc::SyncSender<Result<(), Error>>,
    start: mpsc::Receiver<()>,
) -> Result<(), Error> {
    let path = shared.state()?.models[1].path.clone();
    let mut model = LaneModel::load(Path::new(&path));
    {
        let mut state = shared.state()?;
        state.models[1].loaded = model.loaded();
        state.models[1].error = model.error().to_owned();
    }
    ready
        .send(Ok(()))
        .map_err(|_| Error::Contract("road startup receiver closed"))?;
    if start.recv().is_err() {
        return Ok(());
    }
    let mut camera = Camera::new(Stream::Road);
    while shared.running.load(Ordering::Acquire) {
        let result = (|| {
            shared.refresh(false)?;
            let (infer, threshold, snapshot) = {
                let state = shared.state()?;
                let status = &state.cameras[1];
                (
                    model.loaded()
                        && state.metrics[1]
                            .ready(platform::monotonic()?, state.settings.lane_interval_seconds),
                    state.settings.lane_threshold,
                    (status.snapshot_response < status.snapshot_request)
                        .then_some(status.snapshot_request),
                )
            };
            camera.receive(&shared, infer || snapshot.is_some(), |camera| {
                let now = platform::monotonic()?;
                {
                    let mut state = shared.state()?;
                    let status = &mut state.cameras[1];
                    status.last_frame = now;
                    status.error.clear();
                    if let Some(request) = snapshot {
                        status.jpeg = Some(camera.snapshot()?);
                        status.snapshot_response = request;
                        shared.snapshot.notify_all();
                    }
                }
                if !infer {
                    return Ok(());
                }
                let frame = camera.frame()?;
                let started = platform::monotonic()?;
                let cpu = platform::thread_cpu()?;
                let result = model.infer(&frame, threshold);
                let finished = platform::monotonic()?;
                let thread_cpu_ms = (platform::thread_cpu()? - cpu) * 1000.0;
                {
                    let mut state = shared.state()?;
                    state.metrics[1].record(Timing {
                        received: now,
                        started,
                        finished,
                        thread_cpu_ms,
                    })?;
                    state.lane = result;
                    state.lane_updated = platform::timestamp()?;
                }
                publisher.publish(&shared)
            })
        })();
        if let Err(error) = result {
            camera.recover(error, &shared)?;
        }
    }
    Ok(())
}
