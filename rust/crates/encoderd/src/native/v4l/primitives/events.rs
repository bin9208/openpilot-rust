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
    /// Preserves decoder poll errors and its original interrupt retry policy.
    pub fn poll(&self, timeout_ms: i32) -> Result<Option<Ready>, Error> {
        self.poll_events(timeout_ms, true)
    }
    /// # Errors
    /// Preserves encoder errors/events and the original deadline across interrupts.
    pub fn encoder_poll(&self, timeout_ms: i32) -> Result<Option<Ready>, Error> {
        self.poll_events(timeout_ms, false)
    }
    fn poll_events(&self, timeout_ms: i32, priority: bool) -> Result<Option<Ready>, Error> {
        let started = std::time::Instant::now();
        let mut remaining_ms = timeout_ms;
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
            let count = unsafe { libc::poll(&mut descriptor, 1, remaining_ms) };
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
            if !priority && timeout_ms >= 0 {
                let duration =
                    std::time::Duration::from_millis(u64::try_from(timeout_ms).unwrap_or(0));
                let Some(remaining) = duration
                    .checked_sub(started.elapsed())
                    .filter(|value| !value.is_zero())
                else {
                    return Ok(None);
                };
                let rounded_ms = remaining.as_millis()
                    + u128::from(!remaining.subsec_nanos().is_multiple_of(1_000_000));
                remaining_ms = i32::try_from(rounded_ms).unwrap_or(i32::MAX);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::fd::FromRawFd,
        process::Command,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };
    extern "C" fn interrupted(_: libc::c_int) {}
    #[test]
    fn repeated_interruptions_do_not_extend_original_poll_deadline() {
        if std::env::var_os("OPENPILOT_POLL_CHILD").is_none() {
            let executable = std::env::current_exe().unwrap();
            let mut command = if let Ok(runner) = std::env::var("OPENPILOT_POLL_TEST_RUNNER") {
                let mut parts = runner.split_whitespace();
                let mut command = Command::new(parts.next().unwrap());
                command.args(parts).arg(executable);
                command
            } else {
                Command::new(executable)
            };
            let result=command.args(["--exact","native::v4l::primitives::events::tests::repeated_interruptions_do_not_extend_original_poll_deadline","--nocapture"])
                .env("OPENPILOT_POLL_CHILD","1").output().unwrap();
            assert!(
                result.status.success(),
                "{}{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let mut fds = [-1; 2];
        // SAFETY: the isolated child owns both returned pipe descriptors.
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        // SAFETY: the read descriptor is exclusively owned after pipe succeeds.
        let device = Device {
            fd: unsafe { std::os::fd::OwnedFd::from_raw_fd(fds[0]) },
        };
        // SAFETY: pipe returned a distinct, exclusively owned write descriptor.
        let _write = unsafe { std::os::fd::OwnedFd::from_raw_fd(fds[1]) };
        // SAFETY: only this isolated test child installs the non-restarting handler.
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = interrupted as *const () as usize;
        assert_eq!(unsafe { libc::sigemptyset(&mut action.sa_mask) }, 0);
        assert_eq!(
            unsafe { libc::sigaction(libc::SIGUSR1, &action, std::ptr::null_mut()) },
            0
        );
        // SAFETY: pthread_self identifies this live test thread until join below.
        let target = unsafe { libc::pthread_self() };
        let stop = Arc::new(AtomicBool::new(false));
        let signal_stop = stop.clone();
        let sender = std::thread::spawn(move || {
            let start = Instant::now();
            let mut sent = 0;
            while !signal_stop.load(Ordering::Relaxed)
                && start.elapsed() < Duration::from_millis(500)
            {
                std::thread::sleep(Duration::from_millis(1));
                // SAFETY: the target test thread is still live and handles SIGUSR1.
                assert_eq!(unsafe { libc::pthread_kill(target, libc::SIGUSR1) }, 0);
                sent += 1;
            }
            sent
        });
        let started = Instant::now();
        let ready = device.encoder_poll(30).unwrap();
        let elapsed = started.elapsed();
        stop.store(true, Ordering::Relaxed);
        let signals = sender.join().unwrap();
        assert!(ready.is_none());
        assert!(signals > 0);
        assert!(
            elapsed < Duration::from_millis(300),
            "interrupts extended30ms poll to{elapsed:?}"
        );
        println!("deadline retained after{signals} real interrupts: {elapsed:?}");
    }
}
