use super::{Buffer, CallResult, Device, Session};
use openpilot_camerad::{ioctl::DEQUEUE_EVENT_IOCTL, requests::FrameEvent};

#[derive(Debug)]
pub struct PollResult {
    pub result: CallResult,
    pub revents: i16,
}

impl PollResult {
    pub fn priority(&self) -> bool {
        self.revents & libc::POLLPRI != 0
    }
    pub fn retry(&self) -> bool {
        self.result.code < 0 && matches!(self.result.errno, libc::EINTR | libc::EAGAIN)
    }
}

#[derive(Debug)]
pub struct CameraEvent {
    pub kind: u32,
    pub id: u32,
    pub session: Session,
    pub link: i32,
    pub frame: FrameEvent,
}

impl Device {
    pub fn poll_priority(&self, timeout_ms: i32) -> PollResult {
        let mut descriptor = libc::pollfd {
            fd: self.raw_fd(),
            events: libc::POLLPRI,
            revents: 0,
        };
        // SAFETY: FFI: poll receives exactly one initialized mutable descriptor for this call.
        let code = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
        PollResult {
            result: CallResult {
                code,
                errno: std::io::Error::last_os_error().raw_os_error().unwrap_or(0),
            },
            revents: descriptor.revents,
        }
    }

    pub fn dequeue_event(&self) -> (CallResult, CameraEvent) {
        let mut data = Buffer::<136>::zeroed();
        // SAFETY: FFI: DQEVENT writes the verified LP64 136-byte V4L2 event with no nested pointers.
        let result = unsafe { self.ioctl(DEQUEUE_EVENT_IOCTL, &mut data) };
        (
            CallResult {
                code: result.code,
                errno: result.errno,
            },
            decode(data),
        )
    }
}

fn decode(data: Buffer<136>) -> CameraEvent {
    CameraEvent {
        kind: data.get32(0),
        id: data.get32(96),
        session: Session(i32::from_le_bytes(data.get32(8).to_le_bytes())),
        link: i32::from_le_bytes(data.get32(40).to_le_bytes()),
        frame: FrameEvent {
            request_id: data.get64(16),
            frame_id: data.get64(24),
            timestamp: data.get64(32),
            sof_status: data.get32(44),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn event_preserves_full_request_timestamp_and_separate_link() {
        let mut data = Buffer::<136>::zeroed();
        data.put32(0, 0x0800_0000);
        data.put32(96, 2);
        data.put32(8, 0x8000_0001);
        data.put64(16, 0x1020_3040_5060_7080);
        data.put64(24, u64::MAX);
        data.put64(32, 0xfedc_ba98_7654_3210);
        data.put32(40, 0xffff_fffd);
        data.put32(44, 7);
        let event = decode(data);
        assert_eq!(
            (event.kind, event.id, event.session.0, event.link),
            (0x0800_0000, 2, i32::MIN + 1, -3)
        );
        assert_eq!(
            (
                event.frame.request_id,
                event.frame.frame_id,
                event.frame.timestamp,
                event.frame.sof_status
            ),
            (0x1020_3040_5060_7080, u64::MAX, 0xfedc_ba98_7654_3210, 7)
        );
    }
}
