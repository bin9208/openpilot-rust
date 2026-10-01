use crate::{codec, protocol, Error, Result};
use openpilot_process_supervision::CapturedCommand;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::PathBuf,
    time::Duration,
};

pub const ISDR_AID: &str = "A0000005591010FFFFFFFF8900000100";
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub device: PathBuf,
    pub baud: u32,
    pub timeout_ms: u64,
    pub lock: PathBuf,
    pub reset: PathBuf,
    pub launcher: PathBuf,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            device: "/dev/modem_at0".into(),
            baud: 9600,
            timeout_ms: 5000,
            lock: "/dev/shm/modem.lock".into(),
            reset: "/usr/comma/lte/lte.sh".into(),
            launcher: "openpilot-process-child".into(),
        }
    }
}
pub trait Apdu {
    fn send_apdu(&mut self, command: &[u8]) -> Result<(Vec<u8>, u8, u8)>;
}
pub struct AtClient {
    pub config: Config,
    pub channel: Option<String>,
    serial: Option<Box<dyn serialport::SerialPort>>,
}
impl AtClient {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            channel: None,
            serial: None,
        }
    }
    fn ensure(&mut self, reconnect: bool) -> Result<()> {
        if reconnect {
            self.channel = None;
            self.serial = None;
        }
        if self.serial.is_none() {
            self.serial = Some(
                serialport::new(self.config.device.to_string_lossy(), self.config.baud)
                    .timeout(Duration::from_millis(self.config.timeout_ms))
                    .exclusive(false)
                    .dtr_on_open(true)
                    .open()?,
            );
        }
        Ok(())
    }
    fn exchange(&mut self, command: &str) -> Result<Vec<String>> {
        let port = self
            .serial
            .as_mut()
            .ok_or_else(|| protocol("serial not open"))?;
        port.write_all(format!("{command}\r").as_bytes())?;
        let mut lines = Vec::new();
        loop {
            let mut raw = Vec::new();
            loop {
                let mut byte = [0];
                match port.read(&mut byte) {
                    Ok(0) => break,
                    Ok(_) => {
                        raw.push(byte[0]);
                        if byte[0] == b'\n' {
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => break,
                    Err(e) => return Err(e.into()),
                }
            }
            if raw.is_empty() {
                return Err(Error::Timeout);
            }
            let line = codec::utf8_ignore(&raw).trim().to_owned();
            if line.is_empty() {
                continue;
            }
            if line == "OK" {
                return Ok(lines);
            }
            if line == "ERROR" || line.starts_with("+CME ERROR") {
                return Err(protocol(format!("AT command failed: {line}")));
            }
            lines.push(line);
        }
    }
    pub fn query(&mut self, command: &str) -> Result<Vec<String>> {
        self.ensure(false)?;
        match self.exchange(command) {
            Err(Error::Io(_) | Error::Serial(_)) => {
                self.ensure(true)?;
                self.exchange(command)
            }
            result => result,
        }
    }
    pub fn send_raw(&mut self, data: &[u8]) -> Result<()> {
        self.ensure(false)?;
        let port = self
            .serial
            .as_mut()
            .ok_or_else(|| protocol("serial not open"))?;
        port.clear(serialport::ClearBuffer::Input)?;
        port.write_all(data)?;
        port.flush()?;
        Ok(())
    }
    pub fn close_channel(&mut self) -> Result<()> {
        if let Some(ch) = self.channel.take() {
            match self.query(&format!("AT+CCHC={ch}")) {
                Err(Error::Protocol(_) | Error::Timeout) => {}
                other => {
                    other?;
                }
            }
        }
        Ok(())
    }
    pub fn close(&mut self) -> Result<()> {
        let result = self.close_channel();
        self.serial = None;
        result
    }
    fn open_once(&mut self) -> Result<()> {
        if let Some(ch) = self.channel.clone() {
            match self.query(&format!("AT+CCHC={ch}")) {
                Err(Error::Protocol(_)) => {}
                other => {
                    other?;
                }
            }
            self.channel = None;
        }
        if self
            .serial
            .as_ref()
            .is_some_and(|p| p.clear(serialport::ClearBuffer::Input).is_err())
        {
            self.ensure(true)?;
        }
        for line in self.query(&format!("AT+CCHO=\"{ISDR_AID}\""))? {
            if let Some(ch) = line
                .strip_prefix("+CCHO:")
                .map(str::trim)
                .filter(|ch| !ch.is_empty())
            {
                self.channel = Some(ch.into());
                return Ok(());
            }
        }
        Err(protocol("Failed to open ISD-R application"))
    }
    pub fn reset(&mut self) -> Result<()> {
        self.serial = None;
        CapturedCommand {
            launcher: self.config.launcher.clone(),
            cwd: std::env::current_dir()?,
            argv: vec![self.config.reset.clone().into(), "start".into()],
        }
        .spawn_discarded()?
        .process
        .wait()?;
        Ok(())
    }
    pub fn open_isdr(&mut self) -> Result<()> {
        for attempt in 0..10 {
            match self.open_once() {
                Ok(()) => return Ok(()),
                Err(Error::Protocol(_) | Error::Timeout | Error::Serial(_) | Error::Io(_)) => {
                    std::thread::sleep(Duration::from_millis(250));
                    if attempt == 5 {
                        self.reset()?;
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Err(protocol("Failed to open ISD-R after retries"))
    }
    fn apdu_once(&mut self, apdu: &[u8]) -> Result<(Vec<u8>, u8, u8)> {
        if self.channel.is_none() {
            self.open_isdr()?;
        }
        let ch = self
            .channel
            .as_ref()
            .ok_or_else(|| protocol("no ISD-R channel"))?;
        let payload = codec::hex(apdu);
        for line in self.query(&format!("AT+CGLA={ch},{},\"{payload}\"", payload.len()))? {
            if let Some((_, data)) = line.strip_prefix("+CGLA:").and_then(|v| v.split_once(',')) {
                let bytes = codec::unhex(data.trim().trim_matches('"'))?;
                if bytes.len() >= 2 {
                    let end = bytes.len() - 2;
                    return Ok((bytes[..end].to_vec(), bytes[end], bytes[end + 1]));
                }
            }
        }
        Err(protocol("Missing +CGLA response"))
    }
}
impl Apdu for AtClient {
    fn send_apdu(&mut self, apdu: &[u8]) -> Result<(Vec<u8>, u8, u8)> {
        for attempt in 0..3 {
            match self.apdu_once(apdu) {
                Err(e @ (Error::Protocol(_) | Error::Value(_))) => {
                    self.channel = None;
                    if attempt == 2 {
                        return Err(e);
                    }
                }
                other => return other,
            }
        }
        Err(protocol("send_apdu failed"))
    }
}
impl Drop for AtClient {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
