use super::{Buffer, Error};
use openpilot_camerad::ioctl::{retry_interrupted, ControlEnvelope, SyscallResult, CONTROL_IOCTL};
use std::ffi::CString;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

#[derive(Clone, Copy, Debug)]
pub struct CallResult {
    pub code: i32,
    pub errno: i32,
}

impl CallResult {
    pub fn require_success(self, opcode: u32) -> Result<(), Error> {
        if self.code == 0 {
            Ok(())
        } else {
            Err(Error::Control {
                opcode,
                code: self.code,
                errno: self.errno,
            })
        }
    }
}

#[derive(Debug)]
pub struct Device {
    fd: OwnedFd,
}

impl Device {
    pub(super) fn raw_fd(&self) -> i32 {
        self.fd.as_raw_fd()
    }

    pub fn open(path: &str) -> Result<Self, Error> {
        let name = CString::new(path).map_err(|_| Error::Path)?;
        let result = retry_interrupted(|| {
            // SAFETY: FFI: name is NUL-terminated and remains live for open; no mode argument is needed.
            let code = unsafe { libc::open(name.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK) };
            SyscallResult {
                code,
                errno: std::io::Error::last_os_error().raw_os_error().unwrap_or(0),
            }
        });
        if result.code < 0 {
            return Err(Error::Open {
                path: path.to_owned(),
                source: std::io::Error::from_raw_os_error(result.errno),
            });
        }
        // SAFETY: FD ownership: a successful open created this descriptor and no other owner exists.
        Ok(Self {
            fd: unsafe { OwnedFd::from_raw_fd(result.code) },
        })
    }

    pub fn discover(name: &str, index: usize) -> Result<Self, Error> {
        let mut remaining = index;
        for number in 0_u32.. {
            let path = format!("/sys/class/video4linux/v4l-subdev{number}/name");
            let contents = super::discovery::read_file(&path)?;
            if contents.is_empty() {
                break;
            }
            if contents.starts_with(name.as_bytes()) {
                if remaining == 0 {
                    return Self::open(&format!("/dev/v4l-subdev{number}"));
                }
                remaining -= 1;
            }
        }
        Err(Error::MissingDevice {
            name: name.to_owned(),
            index,
        })
    }

    // The caller supplies the exact ioctl payload and live writable storage for every nested pointer.
    pub(super) unsafe fn ioctl<const N: usize>(
        &self,
        request: u64,
        data: &mut Buffer<N>,
    ) -> SyscallResult {
        retry_interrupted(|| {
            // SAFETY: FFI and bounds: caller establishes the ABI; Buffer owns N initialized aligned bytes.
            let code = unsafe { libc::ioctl(self.fd.as_raw_fd(), request, data.0.as_mut_ptr()) };
            SyscallResult {
                code,
                errno: std::io::Error::last_os_error().raw_os_error().unwrap_or(0),
            }
        })
    }

    // The opcode must consume N bytes and all nested pointers must remain valid until this call returns.
    pub(super) unsafe fn camera<const N: usize>(
        &self,
        opcode: u32,
        data: &mut Buffer<N>,
    ) -> CallResult {
        let mut envelope = ControlEnvelope::camera(opcode, data.address(), N as i32);
        // SAFETY: FFI: the caller owns the payload contract; its borrow spans the synchronous ioctl.
        unsafe { self.envelope(&mut envelope) }
    }

    // The envelope must contain either a kernel memory handle or live writable userspace storage.
    pub(super) unsafe fn envelope(&self, envelope: &mut ControlEnvelope) -> CallResult {
        let mut wire = Buffer(*envelope.bytes());
        // SAFETY: FFI: wire is the verified 24-byte ABI and caller keeps referenced storage live.
        let result = unsafe { self.ioctl(CONTROL_IOCTL, &mut wire) };
        envelope.set_kernel_result(wire.get32(8));
        if envelope.is_sync() {
            let kernel = wire.get32(8) as i32;
            if result.code < 0 || kernel != 0 {
                super::diagnostics::report(super::KernelDiagnostic::SyncControl {
                    id: wire.get32(0),
                    errno: result.errno,
                    transport: result.code,
                    kernel,
                });
            }
        } else if result.code == -1 {
            super::diagnostics::report(super::KernelDiagnostic::CameraControl {
                opcode: wire.get32(0),
                errno: result.errno,
            });
        }
        CallResult {
            code: envelope.effective_result(result.code),
            errno: result.errno,
        }
    }

    pub fn probe_sensor(&self, packet_handle: u32) -> CallResult {
        let mut envelope = ControlEnvelope::camera(0x10a, u64::from(packet_handle), 0);
        // SAFETY: FFI: this envelope contains a kernel memory handle, never a userspace pointer.
        unsafe { self.envelope(&mut envelope) }
    }

    pub fn query_isp(&self) -> Result<(i32, i32), Error> {
        let mut capability = Buffer::<144>::zeroed();
        self.query_capability(&mut capability)?;
        Ok((
            i32::from_le_bytes(capability.get32(0).to_le_bytes()),
            i32::from_le_bytes(capability.get32(8).to_le_bytes()),
        ))
    }

    pub fn query_icp(&self) -> Result<i32, Error> {
        let mut capability = Buffer::<176>::zeroed();
        self.query_capability(&mut capability)?;
        Ok(i32::from_le_bytes(capability.get32(0).to_le_bytes()))
    }

    fn query_capability<const N: usize>(&self, capability: &mut Buffer<N>) -> Result<(), Error> {
        let mut query = Buffer::<16>::zeroed();
        query.put32(0, N as u32);
        query.put32(4, 1);
        query.put64(8, capability.address());
        // SAFETY: FFI: the 16-byte query references the mutable N-byte capability for this call only.
        unsafe { self.camera(0x101, &mut query) }.require_success(0x101)
    }
}
