use openpilot_camera_kernel::{Device, DeviceOperation, Fence, Link, Master, MemoryPool, Session};
use openpilot_camerad::{
    isp::FrameBuffers,
    requests::{
        CameraRequests, Diagnostic, EventResult, FrameEvent, FrameMetadata, RequestError,
        RequestIo, StressPoint,
    },
    sensor::{Register, SensorKind},
    startup::FrameSync,
};
use openpilot_msgq::{VisionLayout, VisionMetadata, VisionServer, VisionStream};

use crate::{
    images::FrameImages, phy::PhyPort, IspConfig, IspPort, IspPortError, OutputMode, SensorError,
    SensorPort,
};

#[derive(Clone, Copy, Debug)]
pub struct CameraConfig {
    pub port: usize,
    pub enabled: bool,
    pub mode: OutputMode,
    pub phy: u32,
    pub vignetting: bool,
    pub depth: usize,
    pub stream: VisionStream,
    pub staggered: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum CameraError {
    #[error(transparent)]
    Kernel(#[from] openpilot_camera_kernel::Error),
    #[error(transparent)]
    Sensor(#[from] SensorError),
    #[error(transparent)]
    Isp(#[from] IspPortError),
    #[error(transparent)]
    Packet(#[from] openpilot_camerad::packet::PacketError),
    #[error(transparent)]
    Vision(#[from] openpilot_msgq::Error),
    #[error(transparent)]
    Depth(#[from] openpilot_camerad::requests::InvalidDepth),
    #[error(transparent)]
    Requests(Box<RequestError<CameraError>>),
    #[error("camera allocation dimensions are invalid")]
    Size,
    #[error("camera buffer depth {0} exceeds the 18-buffer IPC stream")]
    StreamDepth(usize),
    #[error("camera buffer slot {0} is not mapped")]
    Slot(usize),
    #[error("camera devices are not open")]
    Closed,
    #[error("camera slot {0} still owns an IFE fence")]
    Occupied(usize),
    #[error("{variable}: {source}")]
    StressValue {
        variable: &'static str,
        source: openpilot_camera_kernel::DoubleParseError,
    },
    #[error("{0}: invalid environment value")]
    StressEncoding(&'static str),
}

pub trait FrameClock {
    fn now_ns(&mut self) -> u64;
    fn now_ms(&mut self) -> f64;
    fn diagnostic(&mut self, _: Diagnostic<'_>) {}
    fn stress(&mut self, point: StressPoint) -> Result<bool, CameraError>;
    fn sleep_ms(&mut self, millis: u64);
}

pub struct CameraPort<'pool, 'device> {
    requests: Option<CameraRequests>,
    io: CameraIo<'pool, 'device>,
}

struct CameraIo<'pool, 'device> {
    master: &'device Master,
    config: CameraConfig,
    images: Option<FrameImages<'device>>,
    isp: Option<IspPort<'pool, 'device>>,
    phy: Option<PhyPort>,
    sensor: SensorPort<'pool, 'device>,
    link: Option<Link>,
    ife_started: bool,
    bps_started: bool,
    fences: [(Fence, Fence); 20],
    closed: bool,
}

impl<'pool, 'device> CameraPort<'pool, 'device> {
    pub fn open(
        master: &'device Master,
        pool: &'pool MemoryPool<'device>,
        server: &VisionServer,
        config: CameraConfig,
        clock: &mut impl FrameClock,
    ) -> Result<Self, CameraError> {
        Self::open_with(master, pool, server, config, clock, Device::discover)
    }

    pub fn open_with(
        master: &'device Master,
        pool: &'pool MemoryPool<'device>,
        server: &VisionServer,
        config: CameraConfig,
        clock: &mut impl FrameClock,
        mut open: impl FnMut(&str, usize) -> Result<Device, openpilot_camera_kernel::Error>,
    ) -> Result<Self, CameraError> {
        if config.depth > 18 {
            return Err(CameraError::StreamDepth(config.depth));
        }
        if config.port >= 3 {
            return Err(SensorError::Port(config.port).into());
        }
        if config.depth == 0 {
            return Err(openpilot_camerad::requests::InvalidDepth(config.depth).into());
        }
        let sensor = SensorPort::probe(
            &master.request,
            pool,
            open("cam-sensor-driver", config.port)?,
            config.port,
            config.enabled,
        )?;
        let mut output = Self {
            requests: None,
            io: CameraIo {
                master,
                config,
                sensor,
                images: None,
                isp: None,
                phy: None,
                link: None,
                ife_started: false,
                bps_started: false,
                fences: [(Fence(0), Fence(0)); 20],
                closed: false,
            },
        };
        if !output.enabled() {
            return Ok(output);
        }
        let (session, sensor_handle) = output.io.sensor.acquired().ok_or(CameraError::Closed)?;
        let kind = output.kind();
        let mut requests = CameraRequests::new(
            config.port as i32,
            config.depth,
            kind.config().readout_time_ns.into(),
            config.staggered && kind != SensorKind::Ar0231,
        )?;
        output.io.isp = Some(IspPort::new(
            master,
            pool,
            session,
            kind,
            IspConfig {
                camera: config.port,
                mode: config.mode,
                phy: config.phy,
                vignetting: config.vignetting,
                depth: config.depth,
            },
        )?);
        let phy = open("cam-csiphy-driver", config.port)?;
        camera_log!(Debug, "opened csiphy for {}", config.port);
        output.io.phy = Some(PhyPort::new(phy, session, pool)?);
        let isp = output.io.isp.as_ref().ok_or(CameraError::Closed)?;
        camera_log!(Info, "-- Link devices");
        let link = master
            .request
            .link(session, isp.ife_handle(), sensor_handle)?;
        output.io.link = Some(link);
        camera_log!(
            Debug,
            "link: 0 session: 0x{:X} isp: 0x{:X} sensors: 0x{:X} link: 0x{:X}",
            session.0 as u32,
            isp.ife_handle().0 as u32,
            sensor_handle.0 as u32,
            link.0 as u32
        );
        let activated = master.request.activate_link(session, link, true);
        camera_log!(Debug, "link control: {}", activated.code);
        output.io.phy.as_mut().ok_or(CameraError::Closed)?.start()?;
        let started = master
            .isp
            .control_device(DeviceOperation::Start, session, isp.ife_handle());
        camera_log!(Debug, "start isp: {}", started.code);
        started.require_success(0x103)?;
        output.io.ife_started = true;
        if let Some(handle) = isp.bps_handle() {
            let started = master
                .icp
                .control_device(DeviceOperation::Start, session, handle);
            camera_log!(Debug, "start icp: {}", started.code);
            started.require_success(0x103)?;
            output.io.bps_started = true;
        }
        camera_log!(Debug, "camera init {}", config.port);
        output.io.images = Some(FrameImages::new(master, server, config, isp, kind)?);
        requests
            .start(&mut ClockedIo {
                io: &mut output.io,
                clock,
            })
            .map_err(|error| CameraError::Requests(Box::new(error)))?;
        output.requests = Some(requests);
        Ok(output)
    }

    pub fn enabled(&self) -> bool {
        !self.io.closed && self.io.sensor.enabled()
    }
    pub fn kind(&self) -> SensorKind {
        self.io.sensor.kind()
    }
    pub fn session(&self) -> Option<Session> {
        self.io.sensor.acquired().map(|pair| pair.0)
    }
    pub fn layout(&self) -> Option<VisionLayout> {
        self.io.images.as_ref().map(FrameImages::layout)
    }

    pub fn start_sensors(&mut self) -> Result<(), CameraError> {
        if self.io.closed {
            return Err(CameraError::Closed);
        }
        Ok(self.io.sensor.start()?)
    }

    pub fn write_registers(&mut self, registers: &[Register]) -> Result<(), CameraError> {
        if self.io.closed {
            return Err(CameraError::Closed);
        }
        Ok(self.io.sensor.write_registers(registers)?)
    }

    pub fn handle_event(
        &mut self,
        event: FrameEvent,
        sync: &mut FrameSync,
        clock: &mut impl FrameClock,
    ) -> Result<Option<EventResult>, CameraError> {
        if self.io.closed {
            return Err(CameraError::Closed);
        }
        self.requests
            .as_mut()
            .map(|requests| {
                requests
                    .handle_event(
                        event,
                        sync,
                        &mut ClockedIo {
                            io: &mut self.io,
                            clock,
                        },
                    )
                    .map_err(|error| CameraError::Requests(Box::new(error)))
            })
            .transpose()
    }

    pub fn publish(&self, frame: FrameMetadata) -> Result<(), CameraError> {
        let images = self.images()?;
        let layout = images.layout();
        images.publish(
            frame.slot,
            VisionMetadata {
                frame_id: frame.frame_id,
                timestamp_sof: frame.timestamp_sof,
                timestamp_eof: frame.timestamp_eof,
                valid: false,
                width: layout.width,
                height: layout.height,
                stride: layout.stride,
                uv_offset: layout.uv_offset,
                len: layout.len,
                received: true,
                index: frame.slot,
                fd: -1,
            },
        )
    }

    pub fn copy_yuv(&self, slot: usize, destination: &mut [u8]) -> Result<(), CameraError> {
        self.images()?.copy_yuv(slot, destination)
    }

    pub fn copy_raw(&self, slot: usize) -> Result<Option<Vec<u8>>, CameraError> {
        self.images()?.copy_raw(slot)
    }

    fn images(&self) -> Result<&FrameImages<'device>, CameraError> {
        if self.io.closed {
            return Err(CameraError::Closed);
        }
        self.io.images.as_ref().ok_or(CameraError::Closed)
    }

    pub fn shutdown(&mut self) {
        self.io.shutdown();
    }
}

impl CameraIo<'_, '_> {
    fn clear(&mut self) -> Result<(), CameraError> {
        let (session, _) = self.sensor.acquired().ok_or(CameraError::Closed)?;
        let link = self.link.ok_or(CameraError::Closed)?;
        if let Some(handle) = self
            .isp
            .as_ref()
            .and_then(IspPort::bps_handle)
            .filter(|handle| handle.0 > 0)
        {
            self.master
                .icp
                .flush_device(session, handle)
                .require_success(0x108)?;
            camera_log!(Debug, "flushed bps: 0");
        }
        let result = self.master.request.flush_requests(session, link);
        camera_log!(Debug, "flushed all req: {}", result.code);
        for slot in 0..self.fences.len() {
            self.destroy(slot);
        }
        Ok(())
    }

    fn destroy(&mut self, slot: usize) {
        let (ife, bps) = std::mem::replace(&mut self.fences[slot], (Fence(0), Fence(0)));
        for fence in [ife, bps] {
            if fence.0 != 0 {
                let result = self.master.sync.destroy_fence(fence);
                if result.code != 0 {
                    camera_log!(
                        Error,
                        "Failed to destroy sync object: {}, sync_obj: {}",
                        result.code,
                        fence.0
                    );
                }
            }
        }
    }

    fn enqueue(&mut self, request: u64) -> Result<(), CameraError> {
        let slot = (request % self.config.depth as u64) as usize;
        if self.fences[slot].0 .0 != 0 {
            return Err(CameraError::Occupied(slot));
        }
        let bps = self
            .isp
            .as_ref()
            .and_then(IspPort::bps_handle)
            .is_some_and(|handle| handle.0 > 0);
        let created = self.master.sync.create_fences(bps);
        self.fences[slot] = (
            created.ife.1,
            created.bps.as_ref().map_or(Fence(0), |pair| pair.1),
        );
        for (result, fence) in std::iter::once(created.ife).chain(created.bps) {
            if result.code != 0 {
                camera_log!(Error, "failed to create fence: {} {}", result.code, fence.0);
            }
        }
        let (session, _) = self.sensor.acquired().ok_or(CameraError::Closed)?;
        let scheduled =
            self.master
                .request
                .schedule(session, self.link.ok_or(CameraError::Closed)?, request);
        if scheduled.code != 0 {
            camera_log!(
                Error,
                "failed to schedule cam mgr request: {} {}",
                scheduled.code,
                request
            );
        }
        self.sensor.poke(request as i32)?;
        let (raw, yuv) = self
            .images
            .as_ref()
            .ok_or(CameraError::Closed)?
            .handles(slot)?;
        self.isp.as_mut().ok_or(CameraError::Closed)?.configure(
            slot,
            request as i32,
            FrameBuffers {
                raw,
                yuv,
                ife_fence: self.fences[slot].0 .0,
                bps_fence: self.fences[slot].1 .0,
            },
        )?;
        Ok(())
    }

    fn shutdown(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        if self.isp.is_some() {
            camera_log!(Info, "-- Stop devices {}", self.config.port);
        }
        if self.link.is_some() {
            if let Err(error) = self.clear() {
                camera_log!(Error, "camera shutdown: {error}");
            }
        }
        for slot in 0..self.fences.len() {
            self.destroy(slot);
        }
        if let Some((session, _)) = self.sensor.acquired() {
            if let Some(isp) = self.isp.as_ref() {
                if self.ife_started {
                    self.ife_started = false;
                    let result = self.master.isp.control_device(
                        DeviceOperation::Stop,
                        session,
                        isp.ife_handle(),
                    );
                    camera_log!(Debug, "stop isp: {}", result.code);
                }
                if self.bps_started {
                    self.bps_started = false;
                    if let Some(handle) = isp.bps_handle() {
                        let result =
                            self.master
                                .icp
                                .control_device(DeviceOperation::Stop, session, handle);
                        camera_log!(Debug, "stop icp: {}", result.code);
                    }
                }
            }
            if let Some(phy) = self.phy.as_mut() {
                phy.stop();
            }
            if let Some(link) = self.link.take() {
                camera_log!(Info, "-- Stop link control");
                let result = self.master.request.activate_link(session, link, false);
                camera_log!(Debug, "link control stop: {}", result.code);
                camera_log!(Info, "-- Unlink");
                let result = self.master.request.unlink(session, link);
                camera_log!(Debug, "unlink: {}", result.code);
            }
            if let Some(isp) = self.isp.as_mut() {
                camera_log!(Debug, "-- Release devices");
                isp.release();
            }
            if let Some(phy) = self.phy.as_mut() {
                phy.release();
            }
            if let Some(images) = self.images.as_mut() {
                images.release();
                camera_log!(Debug, "released buffers");
            }
        }
        self.sensor.shutdown();
    }
}

impl Drop for CameraIo<'_, '_> {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct ClockedIo<'borrow, 'pool, 'device, Clock> {
    io: &'borrow mut CameraIo<'pool, 'device>,
    clock: &'borrow mut Clock,
}

impl<Clock: FrameClock> RequestIo for ClockedIo<'_, '_, '_, Clock> {
    type Error = CameraError;
    fn now_ns(&mut self) -> u64 {
        self.clock.now_ns()
    }
    fn now_ms(&mut self) -> f64 {
        self.clock.now_ms()
    }
    fn diagnostic(&mut self, value: Diagnostic<'_>) {
        self.clock.diagnostic(value);
    }
    fn clear_req_queue(&mut self) -> Result<(), Self::Error> {
        self.io.clear()
    }
    fn enqueue_frame(&mut self, request: u64) -> Result<(), Self::Error> {
        self.io.enqueue(request)
    }
    fn fences(&self, slot: usize) -> (i32, i32) {
        (self.io.fences[slot].0 .0, self.io.fences[slot].1 .0)
    }
    fn wait_for_sync(&mut self, fence: i32, timeout_ms: u32) -> bool {
        self.io
            .master
            .sync
            .wait_fence(Fence(fence), timeout_ms.into())
            .code
            == 0
    }
    fn destroy_sync(&mut self, slot: usize) -> Result<(), Self::Error> {
        self.io.destroy(slot);
        Ok(())
    }
    fn stress(&mut self, point: StressPoint) -> Result<bool, Self::Error> {
        self.clock.stress(point)
    }
    fn sleep_ms(&mut self, millis: u64) {
        self.clock.sleep_ms(millis);
    }
}
