use crate::config::Config;
use rustix::fs::{flock, FlockOperation};
use std::{
    fs::OpenOptions,
    io::{self, Read, Write},
    os::unix::fs::OpenOptionsExt,
};

pub const INIT: &[&str] = &[
    "ATE0",
    "ATV1",
    "AT+CMEE=1",
    "ATX4",
    "AT&C1",
    "AT+CREG=2",
    "AT+CGREG=2",
];

pub fn command(config: &Config, cmd: &str) -> Vec<String> {
    let result = exchange(config, cmd);
    match result {
        Ok(lines) => lines,
        Err(error) => {
            eprintln!("modem: AT command failed: {cmd}: {error}");
            Vec::new()
        }
    }
}
fn exchange(config: &Config, cmd: &str) -> io::Result<Vec<String>> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o666)
        .open(&config.lock)?;
    if flock(&lock, FlockOperation::NonBlockingLockExclusive).is_err() {
        return Ok(Vec::new());
    }
    // File closure releases the exact flock used by Python's LPA, including on errors.
    let mut port = serialport::new(config.at_port.to_string_lossy(), 9600)
        .timeout(config.serial_timeout())
        .exclusive(false)
        .dtr_on_open(true)
        .open()?;
    port.clear(serialport::ClearBuffer::Input)?;
    port.write_all(format!("{cmd}\r").as_bytes())?;
    let mut lines = Vec::new();
    loop {
        let mut raw = Vec::new();
        loop {
            let mut byte = [0];
            match port.read(&mut byte) {
                Ok(0) => return Err(io::Error::new(io::ErrorKind::TimedOut, "AT timeout")),
                Ok(_) => {
                    raw.push(byte[0]);
                    if byte[0] == b'\n' {
                        break;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::TimedOut && !raw.is_empty() => break,
                Err(error) => return Err(error),
            }
        }
        // Source decodes UTF-8 with errors=ignore (not replacement characters).
        let mut text = String::new();
        let mut remaining = raw.as_slice();
        while !remaining.is_empty() {
            match std::str::from_utf8(remaining) {
                Ok(value) => {
                    text.push_str(value);
                    break;
                }
                Err(error) => {
                    let (valid, tail) = remaining.split_at(error.valid_up_to());
                    if let Ok(value) = std::str::from_utf8(valid) {
                        text.push_str(value);
                    }
                    remaining = &tail[error.error_len().unwrap_or(tail.len())..];
                }
            }
        }
        let line = text.trim().to_owned();
        match line.as_str() {
            "" => continue,
            "OK" => return Ok(lines),
            "ERROR" => return Err(io::Error::other(line)),
            value if value.starts_with("+CME ERROR") => return Err(io::Error::other(line)),
            _ => lines.push(line),
        }
    }
}
pub fn value(config: &Config, cmd: &str, prefix: &str) -> Option<String> {
    command(config, cmd).into_iter().find_map(|line| {
        if line.contains(prefix) {
            line.split_once(':').map(|(_, v)| v.trim().to_owned())
        } else {
            None
        }
    })
}
pub fn reset_data_port(config: &Config) {
    let result = (|| -> Result<(), serialport::Error> {
        let mut port = serialport::new(config.ppp_port.to_string_lossy(), 460800)
            .timeout(std::time::Duration::from_secs(1))
            .exclusive(false)
            .dtr_on_open(true)
            .open()?;
        port.write_data_terminal_ready(false)?;
        std::thread::sleep(std::time::Duration::from_millis(200));
        port.write_data_terminal_ready(true)
    })();
    if let Err(error) = result {
        eprintln!("modem: data port reset failed: {error}");
    }
}
