use super::{Buffer, CallResult, Device, Error};
use openpilot_camerad::{isp::acquire, sensor::SensorConfig};

#[derive(Clone, Copy, Debug)]
pub struct Session(pub i32);
#[derive(Clone, Copy, Debug)]
pub struct DeviceHandle(pub i32);
#[derive(Clone, Copy, Debug)]
pub enum DeviceOperation {
    Start,
    Stop,
    Release,
}

impl Device {
    pub fn create_session(&self) -> (CallResult, Session) {
        let mut data = Buffer::<8>::zeroed();
        // SAFETY: FFI: CREATE_SESSION writes an eight-byte integer-only payload.
        let result = unsafe { self.camera(0x10b, &mut data) };
        (
            result,
            Session(i32::from_le_bytes(data.get32(0).to_le_bytes())),
        )
    }

    pub fn destroy_session(&self, session: Session) -> CallResult {
        let mut data = Buffer::<8>::zeroed();
        data.put32(0, u32::from_le_bytes(session.0.to_le_bytes()));
        // SAFETY: FFI: DESTROY_SESSION consumes an eight-byte integer-only payload.
        unsafe { self.camera(0x10c, &mut data) }
    }

    pub fn acquire_sensor(&self, session: Session) -> Result<DeviceHandle, Error> {
        // SAFETY: FFI: zero resource count and null pointer authorize no resource access.
        unsafe { self.acquire(session, 0, 0) }
    }

    pub fn acquire_phy(&self, session: Session) -> Result<DeviceHandle, Error> {
        let mut resource = Buffer::<8>::zeroed();
        // SAFETY: FFI: the eight-byte PHY resource is exclusively borrowed until acquire returns.
        unsafe { self.acquire(session, resource.address(), 1) }
    }

    pub fn acquire_isp(
        &self,
        session: Session,
        sensor: &SensorConfig,
        phy: u32,
        raw: bool,
        dimensions: (u32, u32),
    ) -> Result<DeviceHandle, Error> {
        let mut port = Buffer(acquire::ife_port(
            sensor,
            phy,
            raw,
            dimensions.0,
            dimensions.1,
        ));
        let mut resource = Buffer::<24>::zeroed();
        resource.put32(0, 0);
        resource.put32(4, 132);
        resource.put32(8, 1);
        resource.put64(16, port.address());
        // SAFETY: FFI: both nested resources have verified sizes and remain live through acquire.
        unsafe { self.acquire(session, resource.address(), 1) }
    }

    pub fn acquire_bps(
        &self,
        session: Session,
        sensor: &SensorConfig,
        config_handle: i32,
        config_size: u32,
        dimensions: (u32, u32),
    ) -> Result<DeviceHandle, Error> {
        let mut resource = Buffer(acquire::bps_resource(
            sensor,
            config_handle,
            config_size,
            dimensions.0,
            dimensions.1,
        ));
        // SAFETY: FFI: the packed 60-byte BPS resource contains kernel handles, not nested pointers.
        unsafe { self.acquire(session, resource.address(), 1) }
    }

    // The resource pointer addresses the device-specific writable ABI until this call returns.
    unsafe fn acquire(
        &self,
        session: Session,
        resource: u64,
        count: u32,
    ) -> Result<DeviceHandle, Error> {
        let mut data = Buffer::<24>::zeroed();
        data.put32(0, u32::from_le_bytes(session.0.to_le_bytes()));
        data.put32(8, 1);
        data.put32(12, count);
        data.put64(16, resource);
        // SAFETY: FFI: caller establishes the resource lifetime; data is the 24-byte acquire ABI.
        unsafe { self.camera(0x102, &mut data) }.require_success(0x102)?;
        Ok(DeviceHandle(i32::from_le_bytes(
            data.get32(4).to_le_bytes(),
        )))
    }

    pub fn configure(&self, session: Session, device: DeviceHandle, packet: u32) -> CallResult {
        let mut data = Buffer::<24>::zeroed();
        data.put32(0, u32::from_le_bytes(session.0.to_le_bytes()));
        data.put32(4, u32::from_le_bytes(device.0.to_le_bytes()));
        data.put64(16, u64::from(packet));
        // SAFETY: FFI: the 24-byte config contains only integer handles and a zero offset.
        unsafe { self.camera(0x105, &mut data) }
    }

    pub fn control_device(
        &self,
        operation: DeviceOperation,
        session: Session,
        device: DeviceHandle,
    ) -> CallResult {
        let opcode = match operation {
            DeviceOperation::Start => 0x103,
            DeviceOperation::Stop => 0x104,
            DeviceOperation::Release => 0x106,
        };
        let mut data = Buffer::<8>::zeroed();
        data.put32(0, u32::from_le_bytes(session.0.to_le_bytes()));
        data.put32(4, u32::from_le_bytes(device.0.to_le_bytes()));
        // SAFETY: FFI: all three selected commands use the same eight-byte integer-only ABI.
        unsafe { self.camera(opcode, &mut data) }
    }

    pub fn flush_device(&self, session: Session, device: DeviceHandle) -> CallResult {
        let mut data = Buffer::<32>::zeroed();
        data.put32(8, u32::from_le_bytes(session.0.to_le_bytes()));
        data.put32(12, u32::from_le_bytes(device.0.to_le_bytes()));
        data.put32(16, 1);
        // SAFETY: FFI: FLUSH_REQ consumes a 32-byte integer-only payload.
        unsafe { self.camera(0x108, &mut data) }
    }
}
