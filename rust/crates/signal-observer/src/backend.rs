//! Production backend ownership: internal GPU access is revoked when DM needs it.
use openpilot_plannerd::signal::transport::Mode;
use serde::{Deserialize, Serialize};
use std::{
    io,
    process::{Child, ExitStatus},
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    TrackingCpu,
    Cpu,
    Qcom,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Cpu,
    Qcom,
    Auto,
}
impl Request {
    pub fn parse(value: Option<&str>) -> io::Result<Self> {
        match value
            .unwrap_or("cpu")
            .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        {
            "cpu" => Ok(Self::Cpu),
            "qcom" => Ok(Self::Qcom),
            "auto" => Ok(Self::Auto),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid neural backend selection",
            )),
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Device {
    pub mode: i64,
    pub usb_gpu_active: bool,
    pub dm_enabled: bool,
    pub dm_session_disabled: bool,
    pub driver_view: bool,
}
pub fn automatic(device: Device) -> Backend {
    if device.mode == 2 {
        return Backend::TrackingCpu;
    }
    let dm_needed = (device.dm_enabled && !device.dm_session_disabled) || device.driver_view;
    if device.usb_gpu_active && !dm_needed {
        Backend::Qcom
    } else {
        Backend::TrackingCpu
    }
}
/// Preserve source Params read order and short-circuiting. These are fresh
/// reads, independent of the half-second requested-mode cache.
pub fn automatic_read<E>(
    mode: Mode,
    mut boolean: impl FnMut(&str) -> Result<bool, E>,
) -> Result<Backend, E> {
    if mode == Mode::Prepare {
        return Ok(Backend::TrackingCpu);
    }
    let dm_needed = (boolean("DriverMonitoringEnabled")?
        && !boolean("DriverMonitoringSessionDisabled")?)
        || boolean("IsDriverViewEnabled")?;
    Ok(if boolean("UsbGpuActive")? && !dm_needed {
        Backend::Qcom
    } else {
        Backend::TrackingCpu
    })
}
pub fn selected(mode: Mode, neural: bool, request: Request, device: Device) -> Option<Backend> {
    if mode == Mode::Off {
        return None;
    }
    if mode == Mode::Prepare || !neural {
        return Some(Backend::TrackingCpu);
    }
    Some(if request == Request::Cpu {
        Backend::Cpu
    } else {
        automatic(device)
    })
}
fn join(child: &mut Child, timeout: Duration) -> io::Result<Option<ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(10));
    }
}
/// Terminate/join2s, then kill/join2s. Never start another GPU owner before this
/// succeeds; child exit (even success) during a stable selection is a fault.
pub fn stop(child: &mut Child) -> io::Result<ExitStatus> {
    if let Some(status) = child.try_wait()? {
        return Ok(status);
    }
    let pid = rustix::process::Pid::from_raw(i32::try_from(child.id()).map_err(io::Error::other)?)
        .ok_or_else(|| io::Error::other("invalid backend child PID"))?;
    if let Err(error) = rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
        if error != rustix::io::Errno::SRCH {
            return Err(error.into());
        }
    }
    if let Some(status) = join(child, Duration::from_secs(2))? {
        return Ok(status);
    }
    child.kill()?;
    join(child, Duration::from_secs(2))?
        .ok_or_else(|| io::Error::other("signal backend did not stop"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preparation_and_dm_revoke_gpu_without_cpu_neural_fallback() {
        let mut d = Device {
            mode: 1,
            usb_gpu_active: true,
            dm_enabled: false,
            dm_session_disabled: false,
            driver_view: false,
        };
        assert_eq!(automatic(d), Backend::Qcom);
        d.dm_enabled = true;
        assert_eq!(automatic(d), Backend::TrackingCpu);
        d.dm_session_disabled = true;
        assert_eq!(automatic(d), Backend::Qcom);
        d.driver_view = true;
        assert_eq!(automatic(d), Backend::TrackingCpu);
        assert_eq!(
            selected(Mode::Prepare, true, Request::Cpu, d),
            Some(Backend::TrackingCpu)
        );
        assert_eq!(Request::parse(None).unwrap(), Request::Cpu);
        assert!(Request::parse(Some("unknown")).is_err());
    }
}
