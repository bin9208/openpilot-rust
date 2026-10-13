use super::{abi, io, Device};
use crate::Error;
use std::os::fd::AsRawFd;

#[derive(Clone, Copy, Debug)]
pub enum Event {
    PortChanged { width: u32, height: u32 },
    Flush { capture: bool },
    Other(u32),
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Ready {
    pub input: bool,
    pub output: bool,
    pub priority: bool,
    pub error: bool,
}
impl Device {
    /// # Errors
    /// Preserves kernel errors subscribing to the two source VIDC events.
    pub fn subscribe_decoder(&self) -> Result<(), Error> {
        for type_ in [
            abi::V4L2_EVENT_MSM_VIDC_FLUSH_DONE,
            abi::V4L2_EVENT_MSM_VIDC_PORT_SETTINGS_CHANGED_INSUFFICIENT,
        ] {
            let mut subscription = abi::v4l2_event_subscription {
                type_,
                ..Default::default()
            };
            // SAFETY: SUBSCRIBE_EVENT reads its initialized matching ABI value.
            unsafe {
                io::ioctl(
                    &self.fd,
                    abi::ENCODER_VIDIOC_SUBSCRIBE_EVENT,
                    &mut subscription,
                )?;
            }
        }
        Ok(())
    }
    /// # Errors
    /// Preserves event dequeue errors other than EAGAIN/ENOENT absence.
    pub fn event(&self) -> Result<Option<Event>, Error> {
        let mut event = abi::v4l2_event::default();
        // SAFETY: DQEVENT writes the matching initialized event ABI structure.
        if let Err(error) = unsafe { io::ioctl(&self.fd, abi::ENCODER_VIDIOC_DQEVENT, &mut event) }
        {
            return match error {
                Error::Io(error)
                    if matches!(error.raw_os_error(), Some(libc::EAGAIN | libc::ENOENT)) =>
                {
                    Ok(None)
                }
                error => Err(error),
            };
        }
        // SAFETY: the event's opaque vendor data are read as bounded native-endian words.
        let bytes = unsafe { event.u.data };
        let first = u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let second = u32::from_ne_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        Ok(Some(match event.type_ {
            abi::V4L2_EVENT_MSM_VIDC_PORT_SETTINGS_CHANGED_INSUFFICIENT => Event::PortChanged {
                width: second,
                height: first,
            },
            abi::V4L2_EVENT_MSM_VIDC_FLUSH_DONE => Event::Flush {
                capture: first & abi::V4L2_QCOM_CMD_FLUSH_CAPTURE != 0,
            },
            value => Event::Other(value),
        }))
    }
    /// # Errors
    /// Preserves the source capture-flush decoder command error.
    pub fn flush_capture(&self) -> Result<(), Error> {
        let mut command = abi::v4l2_decoder_cmd {
            cmd: abi::V4L2_QCOM_CMD_FLUSH,
            flags: abi::V4L2_QCOM_CMD_FLUSH_CAPTURE,
            ..Default::default()
        };
        // SAFETY: DECODER_CMD reads its matching initialized command structure.
        unsafe { io::ioctl(&self.fd, abi::ENCODER_VIDIOC_DECODER_CMD, &mut command) }
    }
    /// # Errors
    /// Preserves poll errors; interrupted calls retry with the source timeout.
    pub fn poll(&self, timeout_ms: i32) -> Result<Option<Ready>, Error> {
        self.poll_events(timeout_ms, true)
    }
    /// # Errors
    /// Preserves encoder poll errors with only its source input/output/error events.
    pub fn encoder_poll(&self, timeout_ms: i32) -> Result<Option<Ready>, Error> {
        self.poll_events(timeout_ms, false)
    }
    fn poll_events(&self, timeout_ms: i32, priority: bool) -> Result<Option<Ready>, Error> {
        let mut descriptor = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN
                | libc::POLLOUT
                | libc::POLLERR
                | if priority { libc::POLLPRI } else { 0 },
            revents: 0,
        };
        loop {
            // SAFETY: one live pollfd is valid for this synchronous kernel call.
            let count = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
            if count == 0 {
                return Ok(None);
            }
            if count > 0 {
                return Ok(Some(Ready {
                    input: descriptor.revents & libc::POLLIN != 0,
                    output: descriptor.revents & libc::POLLOUT != 0,
                    priority: descriptor.revents & libc::POLLPRI != 0,
                    error: descriptor.revents & libc::POLLERR != 0,
                }));
            }
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::Interrupted {
                return Err(error.into());
            }
        }
    }
}
