use super::{Buffer, CallResult, Device, DeviceHandle, Error, Session};

#[derive(Clone, Copy, Debug)]
pub struct Link(pub i32);

impl Device {
    pub fn link(
        &self,
        session: Session,
        isp: DeviceHandle,
        sensor: DeviceHandle,
    ) -> Result<Link, Error> {
        let mut data = Buffer::<268>::zeroed();
        for (offset, value) in [(0, session.0), (4, 2), (8, isp.0), (12, sensor.0)] {
            data.put32(offset, u32::from_le_bytes(value.to_le_bytes()));
        }
        // SAFETY: FFI: LINK receives the full 64-device ABI with exactly two initialized handles.
        unsafe { self.camera(0x10d, &mut data) }.require_success(0x10d)?;
        Ok(Link(i32::from_le_bytes(data.get32(264).to_le_bytes())))
    }

    pub fn unlink(&self, session: Session, link: Link) -> CallResult {
        let mut data = Buffer::<8>::zeroed();
        data.put32(0, u32::from_le_bytes(session.0.to_le_bytes()));
        data.put32(4, u32::from_le_bytes(link.0.to_le_bytes()));
        // SAFETY: FFI: UNLINK consumes two 32-bit handles.
        unsafe { self.camera(0x10e, &mut data) }
    }

    pub fn activate_link(&self, session: Session, link: Link, active: bool) -> CallResult {
        let mut data = Buffer::<24>::zeroed();
        data.put32(0, if active { 0 } else { 1 });
        data.put32(4, u32::from_le_bytes(session.0.to_le_bytes()));
        data.put32(8, 1);
        data.put32(16, u32::from_le_bytes(link.0.to_le_bytes()));
        // SAFETY: FFI: LINK_CONTROL consumes a 24-byte ABI with one active handle.
        unsafe { self.camera(0x116, &mut data) }
    }

    pub fn schedule(&self, session: Session, link: Link, request: u64) -> CallResult {
        self.request_operation(0x10f, session, link, request)
    }

    pub fn flush_requests(&self, session: Session, link: Link) -> CallResult {
        self.request_operation(0x110, session, link, 0)
    }

    fn request_operation(
        &self,
        opcode: u32,
        session: Session,
        link: Link,
        request: u64,
    ) -> CallResult {
        let mut data = Buffer::<24>::zeroed();
        data.put32(0, u32::from_le_bytes(session.0.to_le_bytes()));
        data.put32(4, u32::from_le_bytes(link.0.to_le_bytes()));
        data.put64(16, request);
        // SAFETY: FFI: schedule and flush use the verified 24-byte integer-only payload.
        unsafe { self.camera(opcode, &mut data) }
    }
}
