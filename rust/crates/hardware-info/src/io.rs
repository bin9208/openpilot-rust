use crate::{numeric, Error};
use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    os::unix::net::UnixDatagram,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct HardwarePaths {
    pub cmdline: PathBuf,
    pub model: PathBuf,
    pub version: PathBuf,
    pub modem: PathBuf,
    pub voltage: PathBuf,
    pub current: PathBuf,
    pub power: PathBuf,
    pub som_voltage: PathBuf,
    pub som_current: PathBuf,
    pub max_brightness: PathBuf,
    pub brightness: PathBuf,
    pub gpu_busy: PathBuf,
    pub route: PathBuf,
    pub thermal: PathBuf,
    pub nm_connections: [PathBuf; 2],
    pub encoder_state: PathBuf,
    pub wpa_control: PathBuf,
}
impl HardwarePaths {
    /// An alternate filesystem root is explicit; defaults always use real paths.
    pub fn under(root: &Path) -> Self {
        let path = |name: &str| root.join(name.trim_start_matches('/'));
        Self {
            cmdline: path("/proc/cmdline"),
            model: path("/sys/firmware/devicetree/base/model"),
            version: path("/VERSION"),
            modem: path("/dev/shm/modem"),
            voltage: path("/sys/class/hwmon/hwmon1/in1_input"),
            current: path("/sys/class/hwmon/hwmon1/curr1_input"),
            power: path("/sys/class/hwmon/hwmon1/power1_input"),
            som_voltage: path("/sys/class/power_supply/bms/voltage_now"),
            som_current: path("/sys/class/power_supply/bms/current_now"),
            max_brightness: path("/sys/class/backlight/panel0-backlight/max_brightness"),
            brightness: path("/sys/class/backlight/panel0-backlight/brightness"),
            gpu_busy: path("/sys/class/kgsl/kgsl-3d0/gpubusy"),
            route: path("/proc/net/route"),
            thermal: path("/sys/devices/virtual/thermal"),
            nm_connections: [
                path("/run/NetworkManager/system-connections"),
                path("/data/etc/NetworkManager/system-connections"),
            ],
            encoder_state: path("/sys/kernel/debug/msm_vidc/core0/info"),
            wpa_control: path("/run/wpa_supplicant/wlan0"),
        }
    }
}
impl Default for HardwarePaths {
    fn default() -> Self {
        Self::under(Path::new("/"))
    }
}

pub trait Commands {
    fn output(&self, program: &OsStr, args: &[OsString]) -> Result<String, Error>;
}
#[derive(Default)]
pub struct NativeCommands;
impl Commands for NativeCommands {
    fn output(&self, program: &OsStr, args: &[OsString]) -> Result<String, Error> {
        let output = Command::new(program)
            .args(args)
            .stdin(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()?;
        if !output.status.success() {
            return Err(Error::Command(output.status));
        }
        decode_text(output.stdout)
    }
}
pub(crate) fn sudo_read(commands: &impl Commands, path: &Path) -> String {
    let mut command = OsString::from("sudo cat ");
    command.push(path);
    match commands.output(OsStr::new("/bin/sh"), &[OsString::from("-c"), command]) {
        Ok(value) => numeric::trim(&value).to_owned(),
        Err(_) => String::new(), // common.utils.sudo_read catches every command/decode error.
    }
}
pub(crate) fn read_text(path: &Path) -> Result<String, Error> {
    decode_text(std::fs::read(path)?)
}
fn decode_text(bytes: Vec<u8>) -> Result<String, Error> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
pub(crate) fn read_integer(path: &Path) -> Result<BigInt, Error> {
    numeric::integer(&read_text(path)?, 10)
}
pub(crate) fn read_integer_default(path: &Path) -> BigInt {
    match read_integer(path) {
        Ok(value) => value,
        Err(_) => BigInt::zero(),
    }
}

pub fn get_cmdline(path: &Path) -> Result<BTreeMap<String, String>, Error> {
    let mut fields = BTreeMap::new();
    for item in read_text(path)?.split(' ') {
        let mut pair = item.split('=');
        if let (Some(key), Some(value), None) = (pair.next(), pair.next(), pair.next()) {
            fields.insert(key.into(), value.into());
        }
    }
    Ok(fields)
}
pub fn get_default_route_iface(path: &Path) -> Result<Option<String>, Error> {
    let mut routes = Vec::new();
    for line in read_text(path)?.split_terminator('\n').skip(1) {
        let fields: Vec<_> = line
            .split(numeric::whitespace)
            .filter(|s| !s.is_empty())
            .collect();
        if *fields.get(1).ok_or(Error::Index)? != "00000000" {
            continue;
        }
        if (numeric::integer(fields.get(3).ok_or(Error::Index)?, 16)? & BigInt::from(1)).is_zero() {
            continue;
        }
        routes.push((
            numeric::integer(fields.get(6).ok_or(Error::Index)?, 10)?,
            fields[0].to_owned(),
        ));
    }
    Ok(routes.into_iter().min().map(|(_, interface)| interface))
}
pub(crate) fn monotonic() -> Result<f64, Error> {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    Ok(time.tv_sec.to_f64().ok_or(Error::Overflow)?
        + time.tv_nsec.to_f64().ok_or(Error::Overflow)? / 1e9)
}
fn bind_wpa() -> Result<UnixDatagram, Error> {
    #[cfg(target_os = "linux")]
    {
        use std::os::linux::net::SocketAddrExt;
        let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        let nanos = u128::try_from(time.tv_sec).map_err(|_| Error::Overflow)? * 1_000_000_000
            + u128::try_from(time.tv_nsec).map_err(|_| Error::Overflow)?;
        let name = format!("openpilot-wpa-{}-{nanos}", std::process::id());
        let address = std::os::unix::net::SocketAddr::from_abstract_name(name)?;
        Ok(UnixDatagram::bind_addr(&address)?)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Tici WPA requires Linux abstract Unix sockets",
        )
        .into())
    }
}
fn socket_call<T>(
    socket: &UnixDatagram,
    timeout: Duration,
    read: bool,
    mut call: impl FnMut() -> std::io::Result<T>,
) -> Result<T, Error> {
    let start = Instant::now();
    loop {
        if !timeout.is_zero() {
            let remaining = timeout
                .checked_sub(start.elapsed())
                .filter(|value| !value.is_zero())
                .ok_or(Error::Timeout)?;
            if read {
                socket.set_read_timeout(Some(remaining))?;
            } else {
                socket.set_write_timeout(Some(remaining))?;
            }
        }
        match call() {
            Ok(value) => return Ok(value),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error)
                if !timeout.is_zero()
                    && matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
            {
                return Err(Error::Timeout)
            }
            Err(error) => return Err(error.into()),
        }
    }
}
fn splitlines(text: &str) -> impl Iterator<Item = &str> {
    text.split([
        '\n', '\r', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\u{85}', '\u{2028}', '\u{2029}',
    ])
}
pub fn wpa_supplicant_cmd(
    endpoint: &Path,
    command: &str,
    timeout: Duration,
) -> Result<BTreeMap<String, String>, Error> {
    let socket = bind_wpa()?;
    socket.set_nonblocking(timeout.is_zero())?;
    socket.connect(endpoint)?;
    socket_call(&socket, timeout, false, || socket.send(command.as_bytes()))?;
    let mut bytes = [0_u8; 8192];
    loop {
        let size = socket_call(&socket, timeout, true, || socket.recv(&mut bytes))?;
        let output = String::from_utf8_lossy(&bytes[..size]);
        if output.starts_with('<') {
            continue;
        }
        if output.starts_with("FAIL") {
            return Ok(BTreeMap::new());
        }
        return Ok(splitlines(&output)
            .filter_map(|line| {
                line.split_once('=')
                    .map(|(key, value)| (key.into(), value.into()))
            })
            .collect());
    }
}
