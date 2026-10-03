use crate::{wire, Error};
use openpilot_logging::{
    producer::Logger,
    record::{Level, Record},
};
use openpilot_params::Params;
use std::{
    sync::mpsc,
    thread::{self, JoinHandle},
};
pub fn read(params: &Params, key: &str) -> Result<Option<Vec<u8>>, Error> {
    match params.get(key) {
        Ok(value) => Ok(value.filter(|bytes| !bytes.is_empty())),
        // C++ read_file returns empty on filesystem errors; Cython maps empty bytes to None.
        Err(openpilot_params::Error::Io(_)) => Ok(None),
        Err(error) => Err(error.into()),
    }
}
pub fn retrieve(
    params: &Params,
    current: &wire::Car,
    logger: &mut Logger,
) -> Result<Option<(f64, i32)>, Error> {
    let saved = read(params, "LiveDelay")?;
    let previous = read(params, "CarParamsPrevRoute")?;
    let Some(bytes) = saved else {
        return Ok(None);
    };
    match wire::saved(&bytes, previous.as_deref().unwrap_or_default(), current) {
        Ok(seed) => Ok(Some(seed)),
        Err(error) => {
            logger.emit(
                openpilot_logging::log_site!(),
                Record::text(
                    Level::Error,
                    format!("Failed to retrieve initial lag: {error}"),
                ),
            )?;
            match params.remove("LiveDelay") {
                // Cython ignores the native remove return code.
                Ok(()) | Err(openpilot_params::Error::Io(_)) => (),
                Err(error) => return Err(error.into()),
            }
            Ok(None)
        }
    }
}
pub struct Writer {
    sender: Option<mpsc::Sender<Vec<u8>>>,
    worker: Option<JoinHandle<()>>,
}
impl Writer {
    pub fn new(params: Params) -> Result<Self, Error> {
        let (sender, receiver) = mpsc::channel::<Vec<u8>>();
        let worker = thread::Builder::new()
            .name("lagd-params".into())
            .spawn(move || {
                for bytes in receiver {
                    if let Err(error) = params.put("LiveDelay", &bytes) {
                        eprintln!("lagd: error writing LiveDelay: {error}");
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            worker: Some(worker),
        })
    }
    pub fn put(&self, bytes: Vec<u8>) -> Result<(), Error> {
        self.sender
            .as_ref()
            .ok_or(Error::Contract("Params writer closed"))?
            .send(bytes)
            .map_err(|_| Error::Contract("Params writer stopped"))
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        self.sender.take();
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            eprintln!("lagd: Params writer panicked");
        }
    }
}
