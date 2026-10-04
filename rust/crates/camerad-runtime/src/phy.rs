use openpilot_camera_kernel::{Device, DeviceHandle, DeviceOperation, MemoryPool, Session};
use openpilot_camerad::{
    packet::{COMMAND_SIZE, PACKET_SIZE},
    sensor_packets,
};

use crate::CameraError;

pub(crate) struct PhyPort {
    device: Device,
    session: Session,
    handle: Option<DeviceHandle>,
    started: bool,
}

impl PhyPort {
    pub(crate) fn new(
        device: Device,
        session: Session,
        pool: &MemoryPool<'_>,
    ) -> Result<Self, CameraError> {
        let handle = device.acquire_phy(session)?;
        camera_log!(Debug, "acquire csiphy dev");
        let output = Self {
            device,
            session,
            handle: Some(handle),
            started: false,
        };
        let mut packet = pool.lease(PACKET_SIZE + COMMAND_SIZE)?;
        camera_log!(Info, "-- Config CSI PHY");
        let mut payload = pool.lease(24)?;
        let (bytes, data) = sensor_packets::csiphy(payload.handle() as i32)?;
        packet.write(0, bytes.bytes())?;
        payload.write(0, &data)?;
        output
            .device
            .configure(session, handle, packet.handle())
            .require_success(0x105)?;
        Ok(output)
    }

    pub(crate) fn start(&mut self) -> Result<(), CameraError> {
        let handle = self.handle.ok_or(CameraError::Closed)?;
        let result = self
            .device
            .control_device(DeviceOperation::Start, self.session, handle);
        camera_log!(Debug, "start csiphy: {}", result.code);
        result.require_success(0x103)?;
        self.started = true;
        Ok(())
    }

    pub(crate) fn stop(&mut self) {
        if self.started {
            self.started = false;
            if let Some(handle) = self.handle {
                let result =
                    self.device
                        .control_device(DeviceOperation::Stop, self.session, handle);
                camera_log!(Debug, "stop csiphy: {}", result.code);
            }
        }
    }

    pub(crate) fn release(&mut self) {
        self.stop();
        if let Some(handle) = self.handle.take() {
            let result = self
                .device
                .control_device(DeviceOperation::Release, self.session, handle);
            camera_log!(Debug, "release csiphy: {}", result.code);
        }
    }
}

impl Drop for PhyPort {
    fn drop(&mut self) {
        self.release();
    }
}
