use openpilot_camera_kernel::{
    AllocationOptions, DeviceHandle, DeviceOperation, Master, MemoryPool, Session,
};
use openpilot_camerad::{
    isp::{bps, bps_packet, ife_packet, FrameBuffers},
    nv12::{LayoutError, Nv12Layout},
    packet::{COMMAND_SIZE, IO_SIZE, PACKET_SIZE, PATCH_SIZE},
    sensor::SensorKind,
};

use crate::isp_memory::{BpsBuffers, IfeBuffers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputMode {
    Raw,
    Ife,
    Bps,
}

#[derive(Clone, Copy, Debug)]
pub struct IspConfig {
    pub camera: usize,
    pub mode: OutputMode,
    pub phy: u32,
    pub vignetting: bool,
    pub depth: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum IspPortError {
    #[error(transparent)]
    Kernel(#[from] openpilot_camera_kernel::Error),
    #[error(transparent)]
    Program(#[from] openpilot_camerad::isp::IspError),
    #[error(transparent)]
    Layout(#[from] LayoutError),
    #[error("camera allocation dimensions are invalid")]
    Size,
    #[error("camera buffer slot {slot} is outside depth {depth}")]
    Slot { slot: usize, depth: usize },
}

pub struct IspPort<'pool, 'device> {
    master: &'device Master,
    pool: &'pool MemoryPool<'device>,
    session: Session,
    kind: SensorKind,
    config: IspConfig,
    dimensions: (u32, u32),
    ife_handle: DeviceHandle,
    released: bool,
    bps_handle: Option<DeviceHandle>,
    bps: Option<BpsBuffers<'device>>,
    ife: Option<IfeBuffers<'device>>,
}

impl<'pool, 'device> IspPort<'pool, 'device> {
    pub fn new(
        master: &'device Master,
        pool: &'pool MemoryPool<'device>,
        session: Session,
        kind: SensorKind,
        config: IspConfig,
    ) -> Result<Self, IspPortError> {
        if !(1..20).contains(&config.depth) {
            return Err(IspPortError::Size);
        }
        let sensor = kind.config();
        let scale = u32::try_from(sensor.out_scale).map_err(|_| IspPortError::Size)?;
        let height = if sensor.hdr_offset > 0 {
            (sensor.frame_height - sensor.hdr_offset as u32) / 2
        } else {
            sensor.frame_height
        };
        let dimensions = (sensor.frame_width / scale, height / scale);
        let ife_handle = master.isp.acquire_isp(
            session,
            sensor,
            config.phy,
            config.mode != OutputMode::Ife,
            dimensions,
        )?;
        camera_log!(Debug, "acquire isp dev");
        let mut output = Self {
            master,
            pool,
            session,
            kind,
            config,
            dimensions,
            ife_handle,
            released: false,
            bps_handle: None,
            ife: None,
            bps: None,
        };
        output.ife = Some(IfeBuffers::new(
            master,
            kind,
            config.mode == OutputMode::Ife,
            config.depth as u32,
        )?);
        output.configure_ife(
            0,
            1,
            true,
            FrameBuffers {
                raw: 0,
                yuv: 0,
                ife_fence: 0,
                bps_fence: 0,
            },
        )?;
        if config.mode == OutputMode::Bps {
            let sensor_number = kind.config().image_sensor;
            let blob_index = if kind == SensorKind::Ar0231 {
                2
            } else {
                sensor_number
            };
            camera_log!(
                Warning,
                "camera {} sensor {sensor_number}: using BPS blob index {blob_index}",
                config.camera
            );
            let tables = bps::lookup_tables(kind);
            let mut blob = master.request.allocate(AllocationOptions {
                length: tables.config.len(),
                alignment: 1,
                flags: 0x811,
                mmu: [master.mmu.icp, 0],
            })?;
            blob.write(0, tables.config)?;
            output.bps_handle = Some(master.icp.acquire_bps(
                session,
                sensor,
                blob.handle() as i32,
                tables.config.len() as u32,
                dimensions,
            )?);
            camera_log!(Debug, "acquire icp dev");
            blob.close()?;
            output.bps = Some(BpsBuffers::new(master, kind, config.depth as u32)?);
        }
        Ok(output)
    }

    pub fn ife_handle(&self) -> DeviceHandle {
        self.ife_handle
    }
    pub fn bps_handle(&self) -> Option<DeviceHandle> {
        self.bps_handle
    }
    pub fn dimensions(&self) -> (u32, u32) {
        self.dimensions
    }
    pub fn layout(&self) -> Result<Nv12Layout, LayoutError> {
        Nv12Layout::new(self.dimensions.0, self.dimensions.1)
    }

    pub fn configure(
        &mut self,
        slot: usize,
        request: i32,
        buffers: FrameBuffers,
    ) -> Result<(), IspPortError> {
        if slot >= self.config.depth {
            return Err(IspPortError::Slot {
                slot,
                depth: self.config.depth,
            });
        }
        self.configure_ife(slot, request, false, buffers)?;
        if self.config.mode == OutputMode::Bps {
            self.configure_bps(slot, request, buffers)?;
        }
        Ok(())
    }

    fn configure_ife(
        &mut self,
        slot: usize,
        request: i32,
        initial: bool,
        buffers: FrameBuffers,
    ) -> Result<(), IspPortError> {
        let length =
            PACKET_SIZE + 2 * COMMAND_SIZE + 10 * PATCH_SIZE + if initial { 0 } else { IO_SIZE };
        let mut packet = self.pool.lease(length)?;
        let mut generic = self.pool.lease(324)?;
        let memory = self.ife.as_mut().ok_or(IspPortError::Size)?;
        let update = ife_packet::build(
            self.kind.config(),
            &ife_packet::IfeConfig {
                raw: self.config.mode != OutputMode::Ife,
                vignetting: self.config.vignetting,
                width: self.dimensions.0,
                height: self.dimensions.1,
            },
            &memory.references(),
            buffers,
            ife_packet::IfeRequest {
                slot: slot as u32,
                request,
                initial,
                generic_handle: generic.handle() as i32,
            },
        )?;
        memory
            .command
            .write(update.command_offset, &update.program.bytes)?;
        generic.write(0, &update.generic)?;
        packet.write(0, update.packet.bytes())?;
        self.master
            .isp
            .configure(self.session, self.ife_handle, packet.handle())
            .require_success(0x105)?;
        Ok(())
    }

    fn configure_bps(
        &mut self,
        slot: usize,
        request: i32,
        buffers: FrameBuffers,
    ) -> Result<(), IspPortError> {
        let downscale = self.kind.config().out_scale > 1;
        let length = PACKET_SIZE
            + 2 * COMMAND_SIZE
            + if downscale {
                3 * IO_SIZE + 14 * PATCH_SIZE
            } else {
                2 * IO_SIZE + 12 * PATCH_SIZE
            };
        let mut packet = self.pool.lease(length)?;
        let mut generic = self.pool.lease(36)?;
        let memory = self.bps.as_mut().ok_or(IspPortError::Size)?;
        let handle = self.bps_handle.ok_or(IspPortError::Size)?;
        let update = bps_packet::build(
            self.kind.config(),
            &bps_packet::BpsConfig {
                width: self.dimensions.0,
                height: self.dimensions.1,
                device: handle.0,
            },
            &memory.references(),
            buffers,
            bps_packet::BpsRequest {
                slot: slot as u32,
                request,
                generic_handle: generic.handle() as i32,
            },
        )?;
        memory
            .command
            .write(update.command_offset, &update.command)?;
        memory.program.write(0, &update.program.bytes)?;
        generic.write(0, &update.generic)?;
        packet.write(0, update.packet.bytes())?;
        self.master
            .icp
            .configure(self.session, handle, packet.handle())
            .require_success(0x105)?;
        Ok(())
    }
}

impl IspPort<'_, '_> {
    pub(crate) fn release(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        let result =
            self.master
                .isp
                .control_device(DeviceOperation::Release, self.session, self.ife_handle);
        camera_log!(Debug, "release isp: {}", result.code);
        if let Some(handle) = self.bps_handle.take() {
            let result =
                self.master
                    .icp
                    .control_device(DeviceOperation::Release, self.session, handle);
            camera_log!(Debug, "release icp: {}", result.code);
        }
    }
}

impl Drop for IspPort<'_, '_> {
    fn drop(&mut self) {
        self.release();
    }
}
