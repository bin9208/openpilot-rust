use crate::health::Health;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Fault {
    #[error("USB device disconnected: {0}")]
    NoDevice(String),
    #[error("USB pipe error: {0}")]
    Pipe(String),
    #[error("Panda protocol mismatch: {0}")]
    Protocol(String),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug)]
pub enum Log {
    Info(String),
    Warning(String),
    Error(&'static str),
    Exception(&'static str, Fault),
    Connect { count: u64 },
    HeartbeatLost { health: Health, serial: String },
    SomReset { health: Health, serial: String },
    DevelopmentBootloader { version: String, internal: bool },
    Found(Vec<String>),
}

pub trait Panda {
    fn bootstub(&self) -> bool;
    fn is_internal(&mut self) -> Result<bool, Fault>;
    fn get_type(&mut self) -> Result<Vec<u8>, Fault>;
    fn serial(&mut self) -> Result<String, Fault>;
    fn version(&mut self) -> Result<String, Fault>;
    fn signature(&mut self) -> Result<Vec<u8>, Fault>;
    fn flash(&mut self) -> Result<(), Fault>;
    fn recover(&mut self, reset: bool) -> Result<(), Fault>;
    fn health(&mut self) -> Result<Health, Fault>;
    fn reset(&mut self) -> Result<(), Fault>;
    fn close(&mut self) -> Result<(), Fault>;
}

pub trait Backend {
    type Device: Panda;
    fn log(&mut self, entry: Log) -> Result<(), Fault>;
    fn remove_signatures(&mut self) -> Result<(), Fault>;
    fn reset_internal(&mut self) -> Result<(), Fault>;
    fn recover_internal(&mut self) -> Result<(), Fault>;
    fn sleep(&mut self, seconds: u64) -> Result<(), Fault>;
    fn dfu_list(&mut self) -> Result<Vec<Option<String>>, Fault>;
    fn dfu_recover(&mut self, serial: Option<&str>) -> Result<(), Fault>;
    fn panda_list(&mut self) -> Result<Vec<String>, Fault>;
    fn connect(&mut self, serial: &str) -> Result<Self::Device, Fault>;
    fn expected_signature(&mut self, panda: &mut Self::Device) -> Result<Vec<u8>, Fault>;
    fn has_internal(&mut self) -> Result<bool, Fault>;
    fn put_signatures(&mut self, signatures: &[u8]) -> Result<(), Fault>;
    fn put_bool(&mut self, key: &str) -> Result<(), Fault>;
    fn run_child(&mut self, serials: &[String]) -> Result<(), Fault>;
}

fn signature_prefix(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn flash_panda<B: Backend>(backend: &mut B, serial: &str) -> Result<B::Device, Fault> {
    let mut panda = match backend.connect(serial) {
        Err(error @ Fault::Protocol(_)) => {
            backend.log(Log::Warning(
                "detected protocol mismatch, reflashing panda".into(),
            ))?;
            backend.recover_internal()?;
            return Err(error);
        }
        result => result?,
    };
    let result = (|| {
        let expected = match backend.expected_signature(&mut panda) {
            Ok(signature) => signature,
            Err(error) => {
                backend.log(Log::Exception("Error computing expected signature", error))?;
                Vec::new()
            }
        };
        let internal = panda.is_internal()?;
        let version = if panda.bootstub() {
            "bootstub".into()
        } else {
            panda.version()?
        };
        let signature = if panda.bootstub() {
            Vec::new()
        } else {
            panda.signature()?
        };
        backend.log(Log::Warning(format!(
            "Panda {serial} connected, version: {version}, signature {}, expected {}",
            signature_prefix(&signature),
            signature_prefix(&expected)
        )))?;
        if panda.bootstub() || signature != expected {
            backend.log(Log::Info(
                "Panda firmware out of date, update required".into(),
            ))?;
            panda.flash()?;
            backend.log(Log::Info("Done flashing".into()))?;
        }
        if panda.bootstub() {
            let version = panda.version()?;
            backend.log(Log::DevelopmentBootloader { version, internal })?;
            if internal {
                backend.recover_internal()?;
            }
            panda.recover(!internal)?;
            backend.log(Log::Info("Done flashing bootstub".into()))?;
        }
        if panda.bootstub() {
            backend.log(Log::Info("Panda still not booting, exiting".into()))?;
            return Err(Fault::Other("Panda still in bootstub".into()));
        }
        if panda.signature()? != expected {
            backend.log(Log::Info("Version mismatch after flashing, exiting".into()))?;
            return Err(Fault::Other("Panda signature mismatch".into()));
        }
        Ok(())
    })();
    if let Err(error) = result {
        panda.close()?;
        return Err(error);
    }
    Ok(panda)
}

pub fn flash_all<B: Backend>(backend: &mut B, serials: &[String]) -> Result<Vec<B::Device>, Fault> {
    let mut pandas = Vec::new();
    let result = (|| {
        for serial in serials {
            pandas.push(flash_panda(backend, serial)?);
        }
        let mut keyed = Vec::with_capacity(pandas.len());
        for (index, panda) in pandas.iter_mut().enumerate() {
            keyed.push((
                (!panda.is_internal()?, panda.get_type()?, panda.serial()?),
                index,
            ));
        }
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(keyed
            .into_iter()
            .map(|(_, index)| index)
            .collect::<Vec<_>>())
    })();
    match result {
        Ok(order) => {
            let mut indexed: Vec<_> = pandas.into_iter().map(Some).collect();
            Ok(order
                .into_iter()
                .map(|index| indexed[index].take().expect("unique sort index"))
                .collect())
        }
        Err(error) => {
            for panda in &mut pandas {
                panda.close()?;
            }
            Err(error)
        }
    }
}

pub struct Supervisor {
    count: u64,
    first_run: bool,
    missing: u64,
}

impl Default for Supervisor {
    fn default() -> Self {
        Self {
            count: 0,
            first_run: true,
            missing: 0,
        }
    }
}

impl Supervisor {
    pub fn step<B: Backend>(&mut self, backend: &mut B) -> Result<(), Fault> {
        let mut pandas = Vec::new();
        let setup = self.setup(backend, &mut pandas);
        let outcome = match setup {
            Ok(serials) => Ok(serials),
            Err(error) => {
                let message = match &error {
                    Fault::NoDevice(_) | Fault::Pipe(_) => "Panda USB exception while setting up",
                    Fault::Protocol(_) => "pandad.protocol_mismatch",
                    Fault::Other(_) => "pandad.uncaught_exception",
                };
                backend.log(Log::Exception(message, error)).map(|()| None)
            }
        };
        // Python's finally runs after logging and propagates a close failure without retrying.
        for panda in &mut pandas {
            panda.close()?;
        }
        if let Some(serials) = outcome? {
            self.first_run = false;
            backend.run_child(&serials)?;
        }
        Ok(())
    }

    fn setup<B: Backend>(
        &mut self,
        backend: &mut B,
        pandas: &mut Vec<B::Device>,
    ) -> Result<Option<Vec<String>>, Fault> {
        self.count += 1;
        backend.log(Log::Connect { count: self.count })?;
        backend.remove_signatures()?;
        if self.missing > 0 {
            if self.missing == 3 {
                backend.log(Log::Info(
                    "No pandas found, putting internal panda into DFU".into(),
                ))?;
                backend.recover_internal()?;
            } else {
                backend.log(Log::Info(
                    "No pandas found, resetting internal panda".into(),
                ))?;
                backend.reset_internal()?;
            }
            backend.sleep(3)?;
        }
        let dfu = backend.dfu_list()?;
        for serial in &dfu {
            backend.log(Log::Info(format!(
                "Panda in DFU mode found, flashing recovery {}",
                serial.as_deref().unwrap_or("None")
            )))?;
            backend.dfu_recover(serial.as_deref())?;
        }
        if !dfu.is_empty() {
            backend.sleep(1)?;
        }
        let serials = backend.panda_list()?;
        if serials.is_empty() {
            self.missing += 1;
            return Ok(None);
        }
        backend.log(Log::Found(serials.clone()))?;
        *pandas = flash_all(backend, &serials)?;
        if backend.has_internal()? {
            let mut included = false;
            for panda in pandas.iter_mut() {
                if panda.is_internal()? {
                    included = true;
                    break;
                }
            }
            if !included {
                backend.log(Log::Error("Internal panda is missing, trying again"))?;
                self.missing += 1;
                return Ok(None);
            }
        }
        self.missing = 0;
        let serials = pandas
            .iter_mut()
            .map(Panda::serial)
            .collect::<Result<Vec<_>, _>>()?;
        let mut signatures = Vec::new();
        for (index, panda) in pandas.iter_mut().enumerate() {
            if index > 0 {
                signatures.push(b',');
            }
            signatures.extend(panda.signature()?);
        }
        backend.put_signatures(&signatures)?;
        for panda in pandas.iter_mut() {
            let health = panda.health()?;
            if health.heartbeat_lost != 0 {
                backend.put_bool("PandaHeartbeatLost")?;
                backend.log(Log::HeartbeatLost {
                    health,
                    serial: panda.serial()?,
                })?;
            }
            if health.som_reset_triggered != 0 {
                backend.put_bool("PandaSomResetTriggered")?;
                backend.log(Log::SomReset {
                    health,
                    serial: panda.serial()?,
                })?;
            }
            if self.first_run {
                backend.log(Log::Info(format!("Resetting panda {}", panda.serial()?)))?;
                panda.reset()?;
            }
        }
        Ok(Some(serials))
    }
}
