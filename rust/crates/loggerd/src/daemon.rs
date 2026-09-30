use crate::{
    clock,
    encoder::{Encoder, Stream},
    metadata::Environment,
    rotation::Rotation,
    writer::{route_name, Logger},
    Error,
};
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::services::{Service, SERVICES};
use openpilot_msgq::{MultiSubscriber, Subscription};
use openpilot_params::Params;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

struct ServiceState {
    service: &'static Service,
    counter: u64,
    encoder: Option<Encoder>,
}

pub fn run(environment: Environment) -> Result<(), Error> {
    let signal = Arc::new(AtomicUsize::new(0));
    for value in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        rustix::process::Signal::POWER.as_raw(),
    ] {
        signal_hook::flag::register_usize(value, Arc::clone(&signal), usize::try_from(value)?)?;
    }
    if environment.device != openpilot_cereal::log_capnp::init_data::DeviceType::Pc {
        let mut cores = rustix::thread::CpuSet::new();
        for index in 0..4 {
            cores.set(index);
        }
        rustix::thread::sched_setaffinity(None, &cores).map_err(std::io::Error::from)?;
    }
    let params = Params::for_runtime()?;
    let record_audio = params.get_bool("RecordAudio")?;
    let mut states = Vec::new();
    for service in SERVICES {
        let stream = Stream::from_service(service.name);
        if service.should_log
            || stream.is_some()
            || (service.name == "rawAudioData" && record_audio)
        {
            states.push(ServiceState {
                service,
                counter: 0,
                encoder: stream
                    .map(|stream| Encoder::new(stream, &params))
                    .transpose()?,
            });
        }
    }
    states.sort_unstable_by_key(|state| state.service.name);
    let specifications: Vec<_> = states
        .iter()
        .map(|state| Subscription {
            endpoint: state.service.name,
            capacity: state.service.queue_size,
            polled: true,
        })
        .collect();
    let mut subscribers = MultiSubscriber::queued_for_runtime(&specifications)?;
    let route = route_name(&params)?;
    let mut logger = Logger::new(&environment.log_root, route, environment.init_data()?);
    let mut rotation = Rotation {
        test_mode: std::env::var_os("LOGGERD_TEST").is_some(),
        ..Rotation::default()
    };
    rotate(&mut logger, &mut rotation)?;
    params.put("CurrentRoute", logger.route.as_bytes())?;
    while signal.load(Ordering::Relaxed) == 0 {
        for index in subscribers.poll_ready(Duration::from_millis(1000))? {
            if signal.load(Ordering::Relaxed) != 0 {
                break;
            }
            let name = states
                .get(index)
                .ok_or(Error::Invalid("native service index"))?
                .service
                .name;
            if matches!(name, "userBookmark" | "audioFeedback") {
                logger.preserve(&params)?;
            }
            for _ in 0..200 {
                if signal.load(Ordering::Relaxed) != 0 {
                    break;
                }
                let Some(bytes) = subscribers.receive_one(index)? else {
                    break;
                };
                if name == "rawAudioData" && record_audio {
                    let message = capnp::serialize::read_message_from_flat_slice(
                        &mut bytes.as_slice(),
                        Default::default(),
                    )?;
                    let event = message.get_root::<event::Reader>()?;
                    let event::RawAudioData(data) = event.which()? else {
                        return Err(Error::Invalid("audio event does not match service"));
                    };
                    let data = data?;
                    for state in &mut states {
                        if let Some(encoder) = &mut state.encoder {
                            if encoder.include_audio {
                                if let Some(writer) = &mut encoder.writer {
                                    writer.write_audio(
                                        data.get_data()?,
                                        event.get_log_mono_time() / 1000,
                                        data.get_sample_rate(),
                                    )?;
                                    encoder.audio_initialized = true;
                                }
                            }
                        }
                    }
                }
                let state = states
                    .get_mut(index)
                    .ok_or(Error::Invalid("native service index"))?;
                match &mut state.encoder {
                    Some(encoder) => {
                        rotation.last_camera_ns = clock::now()?;
                        if encoder.handle(&mut logger, bytes)? {
                            rotation.ready += 1;
                        }
                    }
                    None => {
                        let qlog = state.service.decimation.is_some_and(|frequency| {
                            state.counter.is_multiple_of(u64::from(frequency))
                        });
                        state.counter = state.counter.wrapping_add(1);
                        logger.write(&bytes, qlog)?;
                    }
                }
                if rotation.due(clock::now()?) {
                    rotate(&mut logger, &mut rotation)?;
                }
            }
        }
    }
    let signal = i32::try_from(signal.load(Ordering::Relaxed))?;
    if signal == rustix::process::Signal::POWER.as_raw() {
        rustix::fs::sync();
    }
    logger.close(signal, clock::now()?)?;
    for state in &mut states {
        if let Some(encoder) = &mut state.encoder {
            encoder.close()?;
        }
    }
    Ok(())
}

fn rotate(logger: &mut Logger, rotation: &mut Rotation) -> Result<(), Error> {
    rotate_with_clock(logger, rotation, clock::now)
}

fn rotate_with_clock(
    logger: &mut Logger,
    rotation: &mut Rotation,
    mut now: impl FnMut() -> Result<u64, Error>,
) -> Result<(), Error> {
    logger.next(now()?)?;
    rotation.ready = 0;
    rotation.last_rotation_ns = now()?;
    eprintln!("loggerd: logging to {}", logger.path()?.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_rotation_timeout_after_opening_and_writing_the_new_segment() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("route--0/rlog.zst");
        let mut logger = Logger::new(root.path(), "route".into(), Vec::new());
        let mut rotation = Rotation::default();
        let mut calls = 0;
        rotate_with_clock(&mut logger, &mut rotation, || {
            calls += 1;
            assert_eq!(path.exists(), calls == 2);
            Ok(if calls == 1 {
                1_000_000_000
            } else {
                9_000_000_000
            })
        })
        .unwrap();
        assert_eq!(calls, 2);
        assert!(!rotation.due(61_000_000_001));
        assert!(rotation.due(69_000_000_001));
        logger.close(0, 70_000_000_000).unwrap();
    }
}
