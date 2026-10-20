use super::super::{platform, shared::Shared, Error};
use super::{camera::Camera, publisher::Publisher};
use crate::{
    inference::BlindspotModel,
    service::{state::Timing, Stream},
    vision::{gate, Gate, GateInput, Side},
};
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::{
    runtime::SubMaster,
    state::{Options, State},
};
use std::{
    path::Path,
    sync::{atomic::Ordering, mpsc, Arc},
    time::Duration,
};

fn update_gate(state: &State, configured: [bool; 2], next_side: Side) -> Result<Gate, Error> {
    let valid =
        state.all_alive(&["carState", "modelV2"])? && state.all_valid(&["carState", "modelV2"])?;
    let (left_width, right_width) = if valid {
        let event::ModelV2(model) = state
            .topic("modelV2")?
            .event()?
            .which()
            .map_err(capnp::Error::from)?
        else {
            return Err(Error::Contract("unexpected gate modelV2"));
        };
        let meta = model?.get_meta()?;
        (
            f64::from(meta.get_lane_width_left()),
            f64::from(meta.get_lane_width_right()),
        )
    } else {
        (0.0, 0.0)
    };
    Ok(gate(GateInput {
        alive_valid: valid,
        left_width,
        right_width,
        configured,
        next_side,
    }))
}

pub fn run(
    shared: Arc<Shared>,
    publisher: Publisher,
    ready: mpsc::SyncSender<Result<(), Error>>,
    start: mpsc::Receiver<()>,
) -> Result<(), Error> {
    let (path, config) = {
        let state = shared.state()?;
        (state.models[0].path.clone(), state.config.clone())
    };
    let mut model = BlindspotModel::load(Path::new(&path), config);
    let mut generation = 0;
    let mut subscriber = SubMaster::for_runtime(&["carState", "modelV2"], Options::default())?;
    {
        let mut state = shared.state()?;
        state.models[0].loaded = model.loaded();
        state.models[0].error = model.error().to_owned();
    }
    ready
        .send(Ok(()))
        .map_err(|_| Error::Contract("wide startup receiver closed"))?;
    if start.recv().is_err() {
        return Ok(());
    }
    let mut camera = Camera::new(Stream::Wide);
    while shared.running.load(Ordering::Acquire) {
        let result = (|| {
            shared.refresh(false)?;
            subscriber.update(Duration::ZERO)?;
            let (configured, next_side) = {
                let state = shared.state()?;
                (
                    [
                        state.config.poly_left.len() >= 3,
                        state.config.poly_right.len() >= 3,
                    ],
                    state.next_side,
                )
            };
            let gate = update_gate(&subscriber.state, configured, next_side)?;
            let side = gate.side.filter(|_| gate.active);
            let (infer, interval, settings, snapshot) = {
                let state = shared.state()?;
                let now = platform::monotonic()?;
                let interval = side.map_or(0.0, |side| {
                    if now < state.followup_until[side.index()] {
                        0.15
                    } else {
                        state.settings.base_interval_seconds
                    }
                });
                let status = &state.cameras[0];
                (
                    side.is_some() && model.loaded() && state.metrics[0].ready(now, interval),
                    interval,
                    state.settings,
                    (status.snapshot_response < status.snapshot_request)
                        .then_some(status.snapshot_request),
                )
            };
            camera.receive(&shared, infer || snapshot.is_some(), |camera| {
                let now = platform::monotonic()?;
                let clear = {
                    let mut state = shared.state()?;
                    state.gate = gate;
                    let camera_state = &mut state.cameras[0];
                    camera_state.last_frame = now;
                    camera_state.error.clear();
                    if let Some(request) = snapshot {
                        camera_state.jpeg = Some(camera.snapshot()?);
                        camera_state.snapshot_response = request;
                        shared.snapshot.notify_all();
                    }
                    let mut clear = false;
                    for candidate in [Side::Left, Side::Right] {
                        if !state.gate.eligible_sides.contains(&candidate) {
                            clear |= state.blindspot_active[candidate.index()];
                            state.blindspot_active[candidate.index()] = false;
                            state.blindspot_side_updated[candidate.index()] = 0;
                        }
                    }
                    if side.is_none() {
                        state.blindspot_active = [false; 2];
                        state.blindspot_side = None;
                        state.blindspot_updated = platform::timestamp()?;
                    }
                    clear
                };
                let Some(side) = side else {
                    if clear {
                        publisher.publish(&shared)?;
                    }
                    return Ok(());
                };
                if !infer {
                    return Ok(());
                }
                let operation = shared
                    .vasm_operation
                    .lock()
                    .map_err(|_| Error::Contract("blindspot operation poisoned"))?;
                let previous = {
                    let state = shared.state()?;
                    if generation != state.config_generation {
                        model.load_config(state.config.clone());
                        generation = state.config_generation;
                    }
                    state.side_at[side.index()]
                };
                if !model.configured(side) {
                    return Ok(());
                }
                let frame = camera.frame()?;
                let started = platform::monotonic()?;
                let cpu = platform::thread_cpu()?;
                model.update(
                    &frame,
                    side,
                    &settings,
                    if previous != 0.0 {
                        now - previous
                    } else {
                        interval
                    },
                )?;
                let finished = platform::monotonic()?;
                let thread_cpu_ms = (platform::thread_cpu()? - cpu) * 1000.0;
                let active = model.side(side).active;
                {
                    let mut state = shared.state()?;
                    state.metrics[0].record(Timing {
                        received: now,
                        started,
                        finished,
                        thread_cpu_ms,
                    })?;
                    state.side_at[side.index()] = now;
                    if active {
                        state.followup_until[side.index()] = now + 1.5;
                    }
                    state.sides = [*model.side(Side::Left), *model.side(Side::Right)];
                    state.blindspot_active[side.index()] = active;
                    state.blindspot_side = Some(side);
                    state.blindspot_updated = platform::timestamp()?;
                    state.blindspot_side_updated[side.index()] = state.blindspot_updated;
                    let other = if side == Side::Left {
                        Side::Right
                    } else {
                        Side::Left
                    };
                    state.next_side = if state.gate.eligible_sides.contains(&other) {
                        other
                    } else {
                        side
                    };
                }
                drop(operation);
                publisher.publish(&shared)
            })
        })();
        if let Err(error) = result {
            camera.recover(error, &shared)?;
        }
    }
    Ok(())
}
