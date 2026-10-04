use super::super::{platform, shared::Shared, Error};
use super::{camera::Camera, publisher::Publisher};
use crate::{
    inference::BlindspotModel,
    service::{state::Timing, Stream},
    vision::{gate, Direction, Gate, GateInput, Side},
};
use openpilot_cereal::log_capnp::{event, LaneChangeDirection};
use openpilot_messaging::{
    runtime::SubMaster,
    state::{Options, State},
};
use std::{
    path::Path,
    sync::{atomic::Ordering, mpsc, Arc},
    time::Duration,
};

fn update_gate(state: &State) -> Result<Gate, Error> {
    let valid =
        state.all_alive(&["carState", "modelV2"])? && state.all_valid(&["carState", "modelV2"])?;
    if !valid {
        return Ok(gate(GateInput {
            alive_valid: false,
            speed: 0.0,
            direction: Direction::None,
            left_width: 0.0,
            right_width: 0.0,
        }));
    }
    let event::CarState(car) = state
        .topic("carState")?
        .event()?
        .which()
        .map_err(capnp::Error::from)?
    else {
        return Err(Error::Contract("unexpected gate carState"));
    };
    let event::ModelV2(model) = state
        .topic("modelV2")?
        .event()?
        .which()
        .map_err(capnp::Error::from)?
    else {
        return Err(Error::Contract("unexpected gate modelV2"));
    };
    let car = car?;
    let meta = model?.get_meta()?;
    let direction = match meta.get_lane_change_direction() {
        Ok(LaneChangeDirection::Left) => Direction::Left,
        Ok(LaneChangeDirection::Right) => Direction::Right,
        Ok(LaneChangeDirection::None) | Err(_) => Direction::None,
    };
    Ok(gate(GateInput {
        alive_valid: true,
        speed: f64::from(car.get_v_ego()),
        direction,
        left_width: f64::from(meta.get_lane_width_left()),
        right_width: f64::from(meta.get_lane_width_right()),
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
            if !camera.receive(&shared)? {
                return Ok(());
            }
            let now = platform::monotonic()?;
            shared.refresh(false)?;
            subscriber.update(Duration::ZERO)?;
            let gate = update_gate(&subscriber.state)?;
            let side = gate.side.filter(|_| gate.active);
            let (clear, interval) = {
                let mut state = shared.state()?;
                state.gate = gate;
                let camera_state = &mut state.cameras[0];
                camera_state.last_frame = now;
                camera_state.error.clear();
                if camera_state.snapshot_response < camera_state.snapshot_request {
                    camera_state.jpeg = Some(camera.snapshot()?);
                    camera_state.snapshot_response = camera_state.snapshot_request;
                    shared.snapshot.notify_all();
                }
                if side.is_none() {
                    let clear = state.blindspot_active.iter().any(|active| *active);
                    state.blindspot_active = [false; 2];
                    state.blindspot_side = None;
                    state.blindspot_updated = platform::timestamp()?;
                    (clear, 0.0)
                } else {
                    let interval = if now < state.followup_until {
                        0.15
                    } else {
                        state.settings.base_interval_seconds
                    };
                    if now - state.metrics[0].last_inference < interval {
                        return Ok(());
                    }
                    (false, interval)
                }
            };
            let Some(side) = side else {
                if clear {
                    publisher.publish(&shared)?;
                }
                return Ok(());
            };
            let operation = shared
                .vasm_operation
                .lock()
                .map_err(|_| Error::Contract("blindspot operation poisoned"))?;
            let (settings, previous) = {
                let state = shared.state()?;
                if generation != state.config_generation {
                    model.load_config(state.config.clone());
                    generation = state.config_generation;
                }
                (state.settings, state.side_at[side.index()])
            };
            if !model.loaded() || !model.configured(side) {
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
                    state.followup_until = now + 1.5;
                }
                state.sides = [*model.side(Side::Left), *model.side(Side::Right)];
                state.blindspot_active = [false; 2];
                state.blindspot_active[side.index()] = active;
                state.blindspot_side = Some(side);
                state.blindspot_updated = platform::timestamp()?;
            }
            drop(operation);
            publisher.publish(&shared)
        })();
        if let Err(error) = result {
            camera.recover(error, &shared)?;
        }
    }
    Ok(())
}
