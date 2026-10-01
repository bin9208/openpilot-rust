use crate::{commands::*, Error};

pub trait Platform {
    fn send(&mut self, bytes: &[u8]) -> Result<(), Error>;
    fn receive(&mut self) -> Result<Vec<u8>, Error>;
    fn baud(&mut self, value: u32) -> Result<(), Error>;
    fn power(&mut self, enabled: bool) -> Result<(), Error>;
    fn monotonic(&mut self) -> f64;
    fn sleep(&mut self, seconds: f64) -> Result<(), Error>;
    fn log(&mut self, level: &str, text: &str) -> Result<(), Error>;
    fn current_time(&mut self) -> Result<Option<Vec<u8>>, Error>;
    fn token(&mut self) -> Result<Option<String>, Error>;
    fn assist(&mut self, token: &str) -> Result<Vec<Vec<u8>>, Error>;
}
pub struct Pigeon<P> {
    pub platform: P,
}
fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}
pub fn add_checksum(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.extend(crate::framing::checksum(bytes.get(2..).unwrap_or_default()));
    bytes
}
pub fn assist_messages(mut bytes: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
    let mut messages = Vec::new();
    while !bytes.is_empty() {
        if bytes.get(..2) != Some(&[0xb5, 0x62]) {
            return Err(Error::Malformed("AssistNow preamble"));
        }
        let header = bytes.get(..6).ok_or(Error::Malformed("AssistNow header"))?;
        let length = 8 + usize::from(u16::from_le_bytes([header[4], header[5]]));
        let end = length.min(bytes.len());
        messages.push(bytes[..end].to_vec());
        bytes = &bytes[end..];
    }
    Ok(messages)
}
impl<P: Platform> Pigeon<P> {
    pub fn wait_ack(&mut self, ack: &[u8], nack: &[u8]) -> Result<bool, Error> {
        let mut data = Vec::new();
        let start = self.platform.monotonic();
        loop {
            data.extend(self.platform.receive()?);
            if contains(&data, ack) {
                self.platform.log("debug", "Received ACK from ublox")?;
                return Ok(true);
            }
            if contains(&data, nack) {
                self.platform.log("error", "Received NACK from ublox")?;
                return Ok(false);
            }
            if self.platform.monotonic() - start > 0.5 {
                self.platform.log("error", "No response from ublox")?;
                return Err(Error::Timeout);
            }
            self.platform.sleep(0.001)?;
        }
    }
    pub fn send_ack(&mut self, bytes: &[u8], ack: &[u8]) -> Result<(), Error> {
        self.platform.send(bytes)?;
        self.wait_ack(ack, UBLOX_NACK)?;
        Ok(())
    }
    pub fn backup_status(&mut self) -> Result<u8, Error> {
        let mut data = Vec::new();
        let start = self.platform.monotonic();
        loop {
            data.extend(self.platform.receive()?);
            if let Some(index) = data
                .windows(UBLOX_BACKUP_RESTORE_MSG.len())
                .position(|bytes| bytes == UBLOX_BACKUP_RESTORE_MSG)
            {
                if let Some(value) = data.get(index + 10) {
                    return Ok(*value);
                }
            }
            if self.platform.monotonic() - start > 1. {
                self.platform
                    .log("error", "No backup restore response from ublox")?;
                return Err(Error::Timeout);
            }
            self.platform.sleep(0.001)?;
        }
    }
    pub fn baudrate(&mut self) -> Result<(), Error> {
        self.platform.baud(9600)?;
        self.platform.send(BAUD)?;
        self.platform.sleep(0.1)?;
        self.platform.baud(460800)
    }
    pub fn reset_device(&mut self) -> Result<bool, Error> {
        for _ in 0..5 {
            self.platform.send(COLD_START)?;
            self.platform.sleep(1.)?;
            self.baudrate()?;
            self.send_ack(CLEAR_CONFIG, UBLOX_ACK)?;
            self.send_ack(CLEAR_BACKUP, UBLOX_ACK)?;
            self.platform.send(RESET_RESTORE)?;
            if matches!(self.backup_status()?, 1 | 3) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub fn save_almanac(&mut self) -> Result<(), Error> {
        self.platform.send(SAVE)?;
        match self.wait_ack(UBLOX_SOS_ACK, UBLOX_SOS_NACK) {
            Ok(true) => self.platform.log("info", "Done storing almanac"),
            Ok(false) => self.platform.log("error", "Error storing almanac"),
            Err(Error::Timeout) => Ok(()),
            Err(error) => Err(error),
        }
    }
    fn configure(&mut self) -> Result<(), Error> {
        for command in CONFIG_COMMANDS {
            self.send_ack(command, UBLOX_ACK)?;
        }
        self.platform.log("debug", "pigeon configured")?;
        self.platform.send(RESTORE)?;
        match self.backup_status()? {
            2 => self.platform.log("warning", "almanac backup restored")?,
            3 => self.platform.log("warning", "no almanac backup found")?,
            value => self.platform.log(
                "error",
                &format!("failed to restore almanac backup, status: {value}"),
            )?,
        }
        if let Some(time) = self.platform.current_time()? {
            self.platform
                .log("warning", "Sending current time to ublox")?;
            self.send_ack(&time, UBLOX_ASSIST_ACK)?;
        }
        if let Some(token) = self.platform.token()? {
            let attempt = (|| {
                for message in self.platform.assist(&token)? {
                    self.send_ack(&message, UBLOX_ASSIST_ACK)?;
                }
                self.platform.log("warning", "AssistNow messages sent")
            })();
            match attempt {
                Ok(()) => (),
                Err(Error::Interrupted) => return Err(Error::Interrupted),
                Err(_) => self
                    .platform
                    .log("warning", "failed to get AssistNow messages")?,
            }
        }
        self.platform.log("warning", "Pigeon GPS on!")
    }
    pub fn initialize(&mut self) -> Result<bool, Error> {
        for _ in 0..10 {
            match self.configure() {
                Ok(()) => return Ok(true),
                Err(Error::Timeout) => self
                    .platform
                    .log("warning", "Initialization failed, trying again!")?,
                Err(error) => return Err(error),
            }
        }
        self.platform
            .log("warning", "Failed to initialize pigeon")?;
        Ok(false)
    }
    pub fn init(&mut self) -> Result<(), Error> {
        self.platform.power(false)?;
        self.platform.sleep(0.1)?;
        self.platform.power(true)?;
        self.platform.sleep(0.5)?;
        self.baudrate()?;
        self.initialize()?;
        Ok(())
    }
    pub fn deinitialize(&mut self) -> Result<(), Error> {
        self.platform.send(STOP)?;
        self.platform.power(false)
    }
}
