use super::{Buffer, CallResult, Device};
use openpilot_camerad::ioctl::ControlEnvelope;

#[derive(Clone, Copy, Debug, Default)]
pub struct Fence(pub i32);

#[derive(Debug)]
pub struct CreatedFences {
    pub ife: (CallResult, Fence),
    pub bps: Option<(CallResult, Fence)>,
}

impl Device {
    pub fn create_fences(&self, bps: bool) -> CreatedFences {
        let mut data = Buffer::<68>::zeroed();
        let name = b"NodeOutputPortFence";
        data.0[..name.len()].copy_from_slice(name);
        let ife_result = self.sync_control(0, &mut data);
        let ife = Fence(if ife_result.code == 0 {
            i32::from_le_bytes(data.get32(64).to_le_bytes())
        } else {
            0
        });
        let bps = if bps {
            // SAFETY: FFI: original BPS creation reuses this 68-byte payload through the camera envelope.
            let result = unsafe { self.camera(0, &mut data) };
            Some((
                result,
                Fence(if result.code == 0 {
                    i32::from_le_bytes(data.get32(64).to_le_bytes())
                } else {
                    0
                }),
            ))
        } else {
            None
        };
        CreatedFences {
            ife: (ife_result, ife),
            bps,
        }
    }

    pub fn destroy_fence(&self, fence: Fence) -> CallResult {
        let mut data = Buffer::<68>::zeroed();
        data.put32(64, u32::from_le_bytes(fence.0.to_le_bytes()));
        self.sync_control(1, &mut data)
    }

    pub fn wait_fence(&self, fence: Fence, timeout_ms: u64) -> CallResult {
        let mut data = Buffer::<16>::zeroed();
        data.put32(0, u32::from_le_bytes(fence.0.to_le_bytes()));
        data.put64(8, timeout_ms);
        self.sync_control(6, &mut data)
    }

    fn sync_control<const N: usize>(&self, opcode: u32, data: &mut Buffer<N>) -> CallResult {
        let mut envelope = ControlEnvelope::sync(opcode, data.address(), N as u32);
        // SAFETY: FFI: private callers select matching sync opcodes and keep their sized payload live.
        unsafe { self.envelope(&mut envelope) }
    }
}
