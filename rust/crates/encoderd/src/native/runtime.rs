#![allow(unsafe_code)]
use super::{platform, publisher, thumbnail::Thumbnail, video::Video, Mapping};
use crate::{
    config::{Camera, Mode},
    lifecycle::{Frame, Lifecycle},
    profile::{self, CameraInfo, Recording},
    sync::Synchronization,
    Error,
};
use openpilot_logging::{log_site, record::Level};
use openpilot_msgq::{VisionClient, VisionStream};
use openpilot_params::Params;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

pub fn run(mode: Mode, recording: Recording, segment_length: i32) -> Result<(), Error> {
    platform::schedule(mode)?;
    let exit = Arc::new(AtomicBool::new(false));
    for signal in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        rustix::process::Signal::POWER.as_raw(),
    ] {
        signal_hook::flag::register(signal, Arc::clone(&exit))?;
    }
    let streams = loop {
        if exit.load(Ordering::Relaxed) {
            return Ok(());
        }
        let streams = VisionClient::available_streams("camerad")?;
        if !streams.is_empty() {
            break streams;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let cameras = profile::cameras(mode, recording);
    let synchronization = Arc::new(Mutex::new(Synchronization::new(0)));
    let mut threads = Vec::new();
    for stream in streams {
        let Some(info) = cameras
            .iter()
            .find(|info| camera_stream(info.camera) == stream)
            .cloned()
        else {
            continue;
        };
        synchronization
            .lock()
            .map_err(|_| Error::Contract("encoder synchronization poisoned"))?
            .max_waiting += 1;
        let synchronization = Arc::clone(&synchronization);
        let exit = Arc::clone(&exit);
        threads.push(std::thread::spawn(move || {
            if let Err(error) = camera(
                info,
                synchronization,
                exit,
                mode == Mode::CarrotVision,
                segment_length,
            ) {
                fatal(error);
            }
        }));
    }
    for thread in threads {
        thread
            .join()
            .map_err(|_| Error::Contract("encoder thread panicked"))?;
    }
    Ok(())
}

fn camera(
    info: CameraInfo,
    synchronization: Arc<Mutex<Synchronization>>,
    exit: Arc<AtomicBool>,
    on_demand: bool,
    segment_length: i32,
) -> Result<(), Error> {
    platform::name(info.thread)?;
    let mut encoders = Vec::new();
    let mut client = VisionClient::new("camerad", camera_stream(info.camera), false)?;
    let params = Params::for_runtime()?;
    let mut jpeg = None;
    let mut mappings = HashMap::new();
    let mut lifecycle = Lifecycle::new(on_demand, segment_length);
    while !exit.load(Ordering::Relaxed) {
        if !client.connect()? {
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        if encoders.is_empty() {
            let layout = client
                .layout()
                .ok_or(Error::Contract("connected stream has no first buffer"))?;
            platform::emit(
                log_site!(),
                Level::Warning,
                format!(
                    "encoder {} init {}x{}",
                    info.thread, layout.width, layout.height
                ),
            );
            if layout.width == 0 || layout.height == 0 {
                return Err(Error::Contract("source encoder dimensions assertion"));
            }
            for encoder in &info.encoders {
                encoders.push(Video::new(
                    encoder,
                    (i32::try_from(layout.width)?, i32::try_from(layout.height)?),
                )?);
            }
            if let Some(service) = info.encoders[0].thumbnail {
                jpeg = Some((
                    Thumbnail::new(layout.width / 4, layout.height / 4)?,
                    publisher::transport(service)?,
                ));
            }
        }
        lifecycle.lagging = false;
        while !exit.load(Ordering::Relaxed) {
            let Some(frame) = client.receive_retained(Duration::from_millis(100))? else {
                continue;
            };
            let extra = *frame.metadata();
            let descriptor = frame.descriptor()?;
            if descriptor.buffer_frame_id != u64::from(extra.frame_id) {
                if !lifecycle.lagging {
                    platform::emit(
                        log_site!(),
                        Level::Error,
                        format!(
                            "encoder {} lag  buffer id: {} extra id: {}",
                            info.thread, descriptor.buffer_frame_id, extra.frame_id as i32
                        ),
                    );
                }
                lifecycle.lagging = true;
                continue;
            }
            lifecycle.lagging = false;
            let (synced, start_frame_id) = {
                let mut state = synchronization
                    .lock()
                    .map_err(|_| Error::Contract("encoder synchronization poisoned"))?;
                let outcome = state.frame(info.camera, extra.frame_id);
                if let Some(text) = outcome.log {
                    platform::emit(log_site!(), Level::Debug, text);
                }
                (outcome.encode, state.start_frame_id)
            };
            if !synced {
                continue;
            }
            if exit.load(Ordering::Relaxed) {
                break;
            }
            let decision = lifecycle.matching_frame(Frame {
                buffer_frame_id: descriptor.buffer_frame_id,
                frame_id: extra.frame_id,
                synced: true,
                exit: false,
                session_active: !on_demand || params.get_bool("CarrotVisionActive")?,
                start_frame_id,
            });
            for encoder in &encoders {
                encoder.idle(decision.idle.unwrap_or(false));
            }
            if !decision.encode {
                continue;
            }
            if decision.rotate {
                for (encoder, settings) in encoders.iter_mut().zip(&info.encoders) {
                    encoder.rotate(settings)?;
                }
                lifecycle.rotated();
            }
            let key = (descriptor.server_id, descriptor.index);
            let mapping = if let Some(mapping) = mappings.get(&key) {
                Arc::clone(mapping)
            } else {
                let fd = descriptor.fd.try_clone_to_owned()?;
                // SAFETY: validated VisionIPC imported storage retains its declared
                // allocation; a duplicated FD pins it beyond asynchronous dequeue.
                let mapping = Arc::new(unsafe { Mapping::new(fd, descriptor.mmap_len)? });
                mappings.insert(key, Arc::clone(&mapping));
                mapping
            };
            for encoder in &mut encoders {
                if encoder.encode(Arc::clone(&mapping), descriptor.data_len, &extra)? == -1 {
                    platform::emit(
                        log_site!(),
                        Level::Error,
                        format!(
                            "Failed to encode frame. frame_id: {}",
                            extra.frame_id as i32
                        ),
                    );
                }
            }
            lifecycle.encoded();
            if decision.thumbnail {
                if let Some((codec, publisher)) = &mut jpeg {
                    publisher::thumbnail(publisher, &extra, &codec.generate(&mapping, &extra)?)?;
                }
            }
        }
    }
    Ok(())
}

fn camera_stream(camera: Camera) -> VisionStream {
    match camera {
        Camera::Road => VisionStream::Road,
        Camera::Driver => VisionStream::Driver,
        Camera::WideRoad => VisionStream::WideRoad,
    }
}
pub fn fatal(error: Error) -> ! {
    platform::emit(log_site!(), Level::Error, error.to_string());
    eprintln!("{error}");
    std::process::abort()
}
