//! Owning FIFO for source put_nonblocking and synchronous setting mutations.
use super::Read;
use crate::Error;
use openpilot_params::Params;
use std::{
    cell::RefCell,
    sync::{mpsc, Arc},
    thread::{self, JoinHandle},
};
#[derive(Clone, Debug, serde::Serialize)]
pub struct Mutation {
    pub key: String,
    pub value: Vec<u8>,
}
struct Worker {
    sender: mpsc::Sender<Mutation>,
    join: JoinHandle<()>,
}
pub struct Store {
    pub raw: Arc<Params>,
    worker: RefCell<Option<Worker>>,
}
impl Store {
    pub fn new(params: Params) -> Self {
        Self {
            raw: Arc::new(params),
            worker: RefCell::new(None),
        }
    }
    pub fn put(&self, key: &str, value: &[u8]) -> Result<(), Error> {
        Ok(self.raw.put(key, value)?)
    }
    pub fn put_bool(&self, key: &str, value: bool) -> Result<(), Error> {
        self.put(key, if value { b"1" } else { b"0" })
    }
    pub fn put_int(&self, key: &str, value: i32) -> Result<(), Error> {
        self.put(key, value.to_string().as_bytes())
    }
    pub fn remove(&self, key: &str) -> Result<(), Error> {
        match self.raw.remove(key) {
            Err(openpilot_params::Error::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(())
            }
            result => Ok(result?),
        }
    }
    pub fn put_nonblocking(&self, mutation: Mutation) -> Result<(), Error> {
        if openpilot_params::metadata(&mutation.key).is_none() {
            return Err(Error::Parameter(mutation.key));
        }
        let mut worker = self.worker.borrow_mut();
        if worker.is_none() {
            let (sender, receiver) = mpsc::channel::<Mutation>();
            let params = self.raw.clone();
            let join = thread::Builder::new()
                .name("ui-params".into())
                .spawn(move || {
                    for mutation in receiver {
                        if let Err(error) = params.put(&mutation.key, &mutation.value) {
                            openpilot_startup_ui::logging::emit(
                                openpilot_logging::record::Level::Error,
                                format!("UI Params write failed: {}: {error}", mutation.key),
                            );
                        }
                    }
                })?;
            *worker = Some(Worker { sender, join });
        }
        worker
            .as_ref()
            .ok_or(Error::Contract("Params writer missing"))?
            .sender
            .send(mutation)
            .map_err(|_| Error::Contract("Params writer stopped"))
    }
    pub fn put_bool_nonblocking(&self, key: &str, value: bool) -> Result<(), Error> {
        self.put_nonblocking(Mutation {
            key: key.into(),
            value: if value { b"1".to_vec() } else { b"0".to_vec() },
        })
    }
    pub fn flush(&self) -> Result<(), Error> {
        if let Some(Worker { sender, join }) = self.worker.borrow_mut().take() {
            drop(sender);
            join.join()
                .map_err(|_| Error::Contract("Params writer panicked"))?;
        }
        Ok(())
    }
}
impl Read for Store {
    fn bytes(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        self.raw.bytes(key)
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        if let Err(error) = self.flush() {
            openpilot_startup_ui::logging::emit(
                openpilot_logging::record::Level::Error,
                format!("UI Params close failed: {error}"),
            );
        }
    }
}
