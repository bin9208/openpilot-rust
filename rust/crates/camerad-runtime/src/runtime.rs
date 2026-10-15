use crate::{
    CameraConfig, CameraError, CameraPort, FrameClock, FrameState, FrameStateError, OutputMode,
    SystemClock,
};
use openpilot_camera_kernel::{Master, MemoryPool, Session};
use openpilot_camerad::{
    exposure::{CameraId, ExposureError},
    geometry::{Geometry, SampleIndices},
    requests::{FrameEvent, FrameMetadata},
    startup::FrameSync,
};
use openpilot_msgq::{Publisher, VisionServer, VisionStream};
use openpilot_params::Params;
use std::{
    collections::VecDeque,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Camera(#[from] CameraError),
    #[error(transparent)]
    Kernel(#[from] openpilot_camera_kernel::Error),
    #[error(transparent)]
    FrameState(#[from] FrameStateError),
    #[error(transparent)]
    Messaging(#[from] openpilot_msgq::Error),
    #[error("camera message service is absent: {0}")]
    Service(&'static str),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RuntimeOptions {
    pub debug_frames: bool,
    pub log_raw: bool,
    pub manual_exposure: bool,
}

impl RuntimeOptions {
    pub fn from_environment() -> Self {
        Self {
            debug_frames: std::env::var_os("DEBUG_FRAMES").is_some(),
            log_raw: std::env::var_os("LOG_RAW_FRAMES").is_some(),
            manual_exposure: std::env::var_os("CTRL_EXP_FROM_PARAMS").is_some(),
        }
    }
}

pub struct RuntimeCamera<'pool, 'device> {
    driver: bool,
    queued_events: VecDeque<FrameEvent>,
    publication: Option<Publication>,
    camera: CameraPort<'pool, 'device>,
}

struct Publication {
    state: FrameState,
    publisher: Publisher,
    samples: Vec<u8>,
    sampling: SampleIndices,
}

impl<'pool, 'device> RuntimeCamera<'pool, 'device> {
    pub fn new(
        camera: CameraPort<'pool, 'device>,
        id: CameraId,
        focal_mm: f32,
    ) -> Result<Self, RuntimeError> {
        let publication = if camera.enabled() {
            let layout = camera.layout().ok_or(CameraError::Closed)?;
            let state = FrameState::new(
                camera.kind(),
                id,
                Geometry {
                    width: i32::try_from(layout.width).map_err(|_| CameraError::Size)?,
                    height: i32::try_from(layout.height).map_err(|_| CameraError::Size)?,
                    focal_mm,
                },
            )?;
            let service = state.service();
            let queue = openpilot_messaging::services::lookup(service)
                .ok_or(RuntimeError::Service(service))?;
            let publisher = Publisher::for_runtime(service, queue.queue_size)?;
            let sampling = state
                .sample_indices(layout.len)
                .map_err(FrameStateError::from)?;
            let samples = vec![0; sampling.len()];
            Some(Publication {
                state,
                publisher,
                samples,
                sampling,
            })
        } else {
            None
        };
        Ok(Self {
            driver: matches!(id, CameraId::Driver),
            queued_events: VecDeque::with_capacity(32),
            publication,
            camera,
        })
    }

    pub fn session(&self) -> Option<Session> {
        self.camera.session()
    }

    pub fn start_sensors(&mut self) -> Result<(), CameraError> {
        self.camera.start_sensors()
    }

    pub fn handle_event(
        &mut self,
        event: FrameEvent,
        sync: &mut FrameSync,
        clock: &mut impl FrameClock,
        options: RuntimeOptions,
        params: &mut Option<Params>,
    ) -> Result<(), RuntimeError> {
        if let Some(frame) = self
            .camera
            .handle_event(event, sync, clock)?
            .and_then(|result| result.frame)
        {
            self.send_state(frame, clock, options, params)?;
        }
        Ok(())
    }

    fn complete_wait(
        &mut self,
        sync: &mut FrameSync,
        clock: &mut impl FrameClock,
        options: RuntimeOptions,
        params: &mut Option<Params>,
    ) -> Result<(), RuntimeError> {
        if let Some(frame) = self
            .camera
            .complete_frame_wait(sync, clock)?
            .and_then(|result| result.frame)
        {
            self.send_state(frame, clock, options, params)?;
        }
        Ok(())
    }
    fn receive_event(
        &mut self,
        event: FrameEvent,
        sync: &mut FrameSync,
        clock: &mut impl FrameClock,
        options: RuntimeOptions,
        params: &mut Option<Params>,
    ) -> Result<(), RuntimeError> {
        if self.driver {
            if self.queued_events.len() >= 32 {
                self.queued_events.pop_front();
                camera_log!(Error, "driver camera event backlog overflow");
            }
            self.queued_events.push_back(event);
            Ok(())
        } else {
            self.handle_event(event, sync, clock, options, params)
        }
    }
    fn drain_events(
        &mut self,
        sync: &mut FrameSync,
        clock: &mut impl FrameClock,
        options: RuntimeOptions,
        params: &mut Option<Params>,
    ) -> Result<(), RuntimeError> {
        while !self.camera.frame_wait_pending() {
            let Some(event) = self.queued_events.pop_front() else {
                break;
            };
            self.handle_event(event, sync, clock, options, params)?;
        }
        Ok(())
    }

    fn send_state(
        &mut self,
        frame: FrameMetadata,
        clock: &mut impl FrameClock,
        options: RuntimeOptions,
        params: &mut Option<Params>,
    ) -> Result<(), RuntimeError> {
        self.camera.publish(frame)?;
        let log_time = clock.now_ns();
        let publication = self.publication.as_mut().ok_or(CameraError::Closed)?;
        let raw = if publication.state.wants_raw(frame.frame_id, options.log_raw) {
            self.camera.copy_raw(frame.slot)?
        } else {
            None
        };
        let message = publication
            .state
            .encode(frame, log_time, options.log_raw, raw.as_deref())?;
        self.camera.copy_yuv_samples(
            frame.slot,
            publication.sampling.clone(),
            &mut publication.samples,
        )?;
        let registers = publication.state.adjust_samples_with_manual(
            frame.frame_id,
            &publication.samples,
            self.camera.enabled(),
            || {
                if !options.manual_exposure {
                    return Ok((String::new(), String::new()));
                }
                if params.is_none() {
                    *params = Some(
                        Params::for_runtime()
                            .map_err(|error| ExposureError::ManualInput(Box::new(error)))?,
                    );
                }
                let params = params.as_ref().ok_or_else(|| {
                    ExposureError::ManualInput(Box::new(std::io::Error::other(
                        "Params initialization missing",
                    )))
                })?;
                let gain = params
                    .get("CameraDebugExpGain")
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let time = params
                    .get("CameraDebugExpTime")
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                Ok((
                    String::from_utf8_lossy(&gain).into_owned(),
                    String::from_utf8_lossy(&time).into_owned(),
                ))
            },
        )?;
        if let Some(registers) = registers {
            self.camera.write_registers(registers.as_slice())?;
        }
        let _ = publication.publisher.send(&message);
        Ok(())
    }
}

pub fn run(stop: &AtomicBool) -> Result<(), RuntimeError> {
    let options = RuntimeOptions::from_environment();
    let definitions = [
        (
            "DISABLE_WIDE_ROAD",
            CameraId::Wide,
            VisionStream::WideRoad,
            1.71,
            OutputMode::Ife,
            false,
        ),
        (
            "DISABLE_ROAD",
            CameraId::Road,
            VisionStream::Road,
            8.0,
            OutputMode::Ife,
            true,
        ),
        (
            "DISABLE_DRIVER",
            CameraId::Driver,
            VisionStream::Driver,
            1.71,
            OutputMode::Bps,
            false,
        ),
    ];
    let enabled = definitions.map(|entry| std::env::var_os(entry.0).is_none());
    let server = VisionServer::new("camerad")?;
    let master = Master::open()?;
    let mut pool = MemoryPool::new(&master.request);
    let mut cameras = Vec::with_capacity(3);
    let mut clock = SystemClock::default();
    let mut sync = FrameSync::new(enabled.iter().filter(|value| **value).count());
    let mut params = None;
    for (port, (_, id, stream, focal, mode, vignetting)) in definitions.into_iter().enumerate() {
        clock.set_camera(port);
        let camera = CameraPort::open(
            &master,
            &pool,
            &server,
            CameraConfig {
                port,
                enabled: enabled[port],
                mode,
                phy: 0x4001 + port as u32,
                vignetting,
                depth: 18,
                stream,
                staggered: false,
            },
            &mut clock,
        )?;
        cameras.push(RuntimeCamera::new(camera, id, focal)?);
    }
    server.start_listener()?;
    camera_log!(Info, "-- Starting devices");
    for camera in &mut cameras {
        camera.start_sensors()?;
    }
    camera_log!(Info, "-- Dequeueing Video events");
    while !stop.load(Ordering::Relaxed) {
        let (polled, completed) = {
            let fds: Vec<_> = cameras
                .iter()
                .map(|camera| camera.camera.frame_wait_fd())
                .collect();
            master.request.poll_frames(&fds, 1000)
        };
        if polled.result.code < 0 {
            if polled.retry() {
                continue;
            }
            camera_log!(
                Error,
                "poll failed ({} - {})",
                polled.result.code,
                polled.result.errno
            );
            break;
        }
        for (port, camera) in cameras.iter_mut().enumerate() {
            if completed[port] {
                clock.set_camera(port);
                camera.complete_wait(&mut sync, &mut clock, options, &mut params)?;
            }
        }
        if polled.priority() {
            let (result, event) = master.request.dequeue_event();
            if result.code != 0 {
                camera_log!(Error, "VIDIOC_DQEVENT failed, errno={}", result.errno);
            } else if event.kind != 0x0800_0000 {
                camera_log!(Error, "unhandled event {}\n", event.kind);
            } else {
                if options.debug_frames {
                    println!(
                        "sess_hdl 0x{:6X}, link_hdl 0x{:6X}, frame_id {}, req_id {}, timestamp {:.2} ms, sof_status {}",
                        event.session.0 as u32,
                        event.link as u32,
                        event.frame.frame_id,
                        event.frame.request_id,
                        event.frame.timestamp as f64 / 1e6,
                        event.frame.sof_status
                    );
                    if event.frame.frame_id > 20 {
                        stop.store(true, Ordering::Relaxed);
                    }
                }
                for (port, camera) in cameras.iter_mut().enumerate() {
                    if camera
                        .session()
                        .is_some_and(|session| session.0 == event.session.0)
                    {
                        clock.set_camera(port);
                        camera.receive_event(
                            event.frame,
                            &mut sync,
                            &mut clock,
                            options,
                            &mut params,
                        )?;
                        break;
                    }
                }
            }
        }
        for (port, camera) in cameras.iter_mut().enumerate() {
            clock.set_camera(port);
            camera.drain_events(&mut sync, &mut clock, options, &mut params)?;
        }
    }

    drop(cameras);
    pool.close()?;
    Ok(())
}
