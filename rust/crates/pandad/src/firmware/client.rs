use super::{dfu_serial, flash_static_with_log, Error, Mcu, Request, Transport};
use crate::{
    health::Health,
    supervisor::{Fault, Panda},
};
use std::path::{Path, PathBuf};

pub trait Handle: Transport<Error = Fault> {
    fn close(&mut self) -> Result<(), Fault>;
}

pub struct Connection<H> {
    pub handle: H,
    pub serial: String,
    pub bootstub: bool,
    pub bcd: Option<Vec<u8>>,
    pub spi: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum Level {
    Debug,
    Info,
}

pub trait Environment {
    type Device: Handle;
    fn usb_connect(
        &mut self,
        serial: &str,
        claim: bool,
        no_error: bool,
    ) -> Result<Option<Connection<Self::Device>>, Fault>;
    fn spi_connect(&mut self, serial: &str) -> Result<Option<Connection<Self::Device>>, Fault>;
    fn firmware_dir(&self) -> &Path;
    fn file_exists(&mut self, path: &Path) -> Result<bool, Fault>;
    fn file_read(&mut self, path: &Path, tail: Option<usize>) -> Result<Vec<u8>, Fault>;
    fn log(&mut self, level: Level, text: String) -> Result<(), Fault>;
    fn sleep(&mut self, seconds: f64) -> Result<(), Fault>;
    fn monotonic(&mut self) -> Result<f64, Fault>;
    fn dfu_list(&mut self) -> Result<Vec<Option<String>>, Fault>;
    fn dfu_recover(&mut self, serial: Option<&str>) -> Result<(), Fault>;
}

pub struct Client<E: Environment> {
    pub environment: E,
    connection: Option<Connection<E::Device>>,
    serial: String,
    open: bool,
    bcd: Option<Vec<u8>>,
    assume_f4: bool,
    mcu: Option<Mcu>,
    versions: [u8; 3],
}

impl<E: Environment> Client<E> {
    pub fn open(environment: E, serial: String) -> Result<Self, Fault> {
        let mut client = Self {
            environment,
            connection: None,
            serial,
            open: false,
            bcd: None,
            assume_f4: false,
            mcu: None,
            versions: [0; 3],
        };
        client.connect(true, false)?;
        Ok(client)
    }

    pub fn connected(&self) -> bool {
        self.open
    }

    fn connection(&mut self) -> Result<&mut Connection<E::Device>, Fault> {
        self.connection
            .as_mut()
            .ok_or_else(|| Fault::Other("Panda handle is missing".into()))
    }

    fn read(&mut self, request: u8, length: usize) -> Result<Vec<u8>, Fault> {
        self.connection()?
            .handle
            .control_read(Request::new(0xc0, request, 0), length)
    }

    pub fn connect(&mut self, claim: bool, wait: bool) -> Result<(), Fault> {
        self.close()?;
        self.connection = None;
        loop {
            self.connection = self.environment.usb_connect(&self.serial, claim, wait)?;
            if self.connection.is_none() {
                self.connection = self.environment.spi_connect(&self.serial)?;
            }
            if self.connection.is_some() || !wait {
                break;
            }
        }
        if self.connection.is_none() {
            return Err(Fault::Other("failed to connect to panda".into()));
        }
        self.bcd = None;
        let reply = self.read(0xc1, 64)?;
        let missing_type =
            self.bootstub() && reply.starts_with(&[0xff, 0x00, 0xc1, 0x3e, 0xde, 0xad, 0xd0, 0x0d]);
        if missing_type {
            self.bcd = self.connection()?.bcd.clone();
        }
        self.assume_f4 = missing_type && self.bcd.is_none();
        self.serial = self.connection()?.serial.clone();
        self.open = true;
        self.mcu = Some(self.mcu_type()?);
        let versions = self.read(0xdd, 3)?;
        self.versions = versions.try_into().unwrap_or([0; 3]);
        self.environment.log(Level::Debug, "connected".into())?;
        for command in [0xf8, 0xe7, 0xc0] {
            self.connection()?
                .handle
                .control_write(Request::new(0x40, command, 0), &[])?;
        }
        for bus in 0..3 {
            let mut request = Request::new(0x40, 0xde, bus);
            request.index = 5000;
            self.connection()?.handle.control_write(request, &[])?;
        }
        Ok(())
    }

    pub fn mcu_type(&mut self) -> Result<Mcu, Fault> {
        let kind = self.get_type()?;
        Mcu::from_hardware(&kind, self.assume_f4)
            .ok_or_else(|| Fault::Other(format!("unknown HW type: {kind:?}")))
    }

    fn cached_mcu(&self) -> Result<Mcu, Fault> {
        self.mcu
            .ok_or_else(|| Fault::Other("Panda MCU is not initialized".into()))
    }

    pub fn expected_signature(&mut self) -> Result<Vec<u8>, Fault> {
        let mcu = self.mcu_type()?;
        let path = self.environment.firmware_dir().join(mcu.app_filename());
        self.environment.file_read(&path, Some(128))
    }

    pub fn up_to_date(&mut self, path: Option<&Path>) -> Result<bool, Fault> {
        let current = self.signature()?;
        let expected = if let Some(path) = path {
            self.environment.file_read(path, Some(128))?
        } else {
            self.expected_signature()?
        };
        Ok(current == expected)
    }

    pub fn reset_to(
        &mut self,
        bootstub: bool,
        bootloader: bool,
        reconnect: bool,
    ) -> Result<(), Fault> {
        let connection = self.connection()?;
        let mut request = Request::new(
            0xc0,
            if bootloader || bootstub { 0xd1 } else { 0xd8 },
            u16::from(!bootloader && bootstub),
        );
        request.timeout_ms = if connection.spi { 5000 } else { 15000 };
        request.expect_disconnect = true;
        let _ = connection.handle.control_write(request, &[]);
        self.close()?;
        if !bootloader && reconnect {
            self.reconnect()?;
        }
        Ok(())
    }

    pub fn reconnect(&mut self) -> Result<(), Fault> {
        if self.open {
            self.close()?;
        }
        for _ in 0..150 {
            if self.connect(false, true).is_ok() {
                return Ok(());
            }
            self.environment.sleep(0.1)?;
        }
        Err(Fault::Other("reconnect failed".into()))
    }

    pub fn flash_file(
        &mut self,
        path: Option<&Path>,
        code: Option<&[u8]>,
        reconnect: bool,
    ) -> Result<(), Fault> {
        if self.up_to_date(path)? {
            return self
                .environment
                .log(Level::Info, "flash: already up to date".into());
        }
        let path: PathBuf = match path.filter(|path| !path.as_os_str().is_empty()) {
            Some(path) => path.into(),
            None => self
                .environment
                .firmware_dir()
                .join(self.cached_mcu()?.app_filename()),
        };
        if !self.environment.file_exists(&path)? {
            return Err(Fault::Other("firmware file missing".into()));
        }
        let version = self.version()?;
        self.environment
            .log(Level::Debug, format!("flash: main version is {version}"))?;
        if !self.bootstub() {
            self.reset_to(true, false, true)?;
        }
        if !self.bootstub() {
            return Err(Fault::Other("Panda did not enter bootstub".into()));
        }
        let owned;
        let code = match code {
            Some(code) => code,
            None => {
                owned = self.environment.file_read(&path, None)?;
                &owned
            }
        };
        let version = self.version()?;
        self.environment.log(
            Level::Debug,
            format!("flash: bootstub version is {version}"),
        )?;
        let mcu = self.cached_mcu()?;
        let handle = &mut self
            .connection
            .as_mut()
            .ok_or_else(|| Fault::Other("Panda handle is missing".into()))?
            .handle;
        let environment = &mut self.environment;
        flash_static_with_log(handle, code, mcu, |text| environment.log(Level::Info, text))
            .map_err(|error| match error {
                Error::Transport(error) => error,
                error => Fault::Other(error.to_string()),
            })?;
        if reconnect {
            self.reconnect()?;
        }
        Ok(())
    }

    pub fn wait_for_dfu(
        &mut self,
        serial: Option<&str>,
        timeout: Option<f64>,
    ) -> Result<bool, Fault> {
        let start = self.environment.monotonic()?;
        let mut devices = self.environment.dfu_list()?;
        while serial.map_or(devices.is_empty(), |serial| {
            !devices.iter().any(|value| value.as_deref() == Some(serial))
        }) {
            self.environment
                .log(Level::Debug, "waiting for DFU...".into())?;
            self.environment.sleep(0.1)?;
            if let Some(timeout) = timeout {
                if self.environment.monotonic()? - start > timeout {
                    return Ok(false);
                }
            }
            devices = self.environment.dfu_list()?;
        }
        Ok(true)
    }

    pub fn recover_with_timeout(
        &mut self,
        timeout: Option<f64>,
        reset: bool,
    ) -> Result<bool, Fault> {
        let serial = dfu_serial(&self.serial, self.cached_mcu()?)
            .map_err(|error| Fault::Other(error.to_string()))?;
        if reset {
            self.reset_to(true, false, true)?;
            self.reset_to(false, true, true)?;
        }
        if !self.wait_for_dfu(serial.as_deref(), timeout)? {
            return Ok(false);
        }
        self.environment.dfu_recover(serial.as_deref())?;
        self.connect(true, true)?;
        self.flash_file(None, None, true)?;
        Ok(true)
    }
}

impl<E: Environment> Panda for Client<E> {
    fn bootstub(&self) -> bool {
        self.connection
            .as_ref()
            .is_some_and(|connection| connection.bootstub)
    }
    fn is_internal(&mut self) -> Result<bool, Fault> {
        Ok(matches!(self.get_type()?.as_slice(), [5 | 6 | 9 | 10]))
    }
    fn get_type(&mut self) -> Result<Vec<u8>, Fault> {
        let reply = self.read(0xc1, 64)?;
        Ok(match &self.bcd {
            Some(bcd) if reply.len() != 1 => bcd.clone(),
            _ => reply,
        })
    }
    fn serial(&mut self) -> Result<String, Fault> {
        Ok(self.serial.clone())
    }
    fn version(&mut self) -> Result<String, Fault> {
        String::from_utf8(self.read(0xd6, 64)?).map_err(|error| Fault::Other(error.to_string()))
    }
    fn signature(&mut self) -> Result<Vec<u8>, Fault> {
        let mut signature = self.read(0xd3, 64)?;
        signature.extend(self.read(0xd4, 64)?);
        Ok(signature)
    }
    fn flash(&mut self) -> Result<(), Fault> {
        self.flash_file(None, None, true)
    }
    fn recover(&mut self, reset: bool) -> Result<(), Fault> {
        self.recover_with_timeout(Some(60.0), reset).map(|_| ())
    }
    fn health(&mut self) -> Result<Health, Fault> {
        if self.versions[0] != 16 {
            return Err(Fault::Other(format!(
                "health packet version mismatch: panda's firmware v{}, library v16. Reflash panda.",
                self.versions[0]
            )));
        }
        let bytes = self.read(0xd2, Health::PACKET_SIZE)?;
        let packet = bytes
            .try_into()
            .map_err(|_| Fault::Other("health packet length is not 58".into()))?;
        Ok(Health::from_packet(&packet))
    }
    fn reset(&mut self) -> Result<(), Fault> {
        self.reset_to(false, false, true)
    }
    fn close(&mut self) -> Result<(), Fault> {
        if self.open {
            self.connection()?.handle.close()?;
            self.open = false;
        }
        Ok(())
    }
}
