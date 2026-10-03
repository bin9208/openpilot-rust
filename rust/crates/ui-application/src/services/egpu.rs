//! Owned asynchronous eGPU checks shared by the two product settings layouts.
pub mod native;
use crate::state::SlowParams;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

pub trait Backend: Send + Sync {
    fn status(&self, state: &SlowParams) -> Result<String, crate::Error>;
    fn link(&self) -> Result<String, crate::Error>;
    fn check(&self, cancelled: &AtomicBool) -> Result<Option<String>, crate::Error>;
    fn remove_compiled_manifest(&self) -> Result<(), crate::Error>;
}

pub struct Check {
    backend: Arc<dyn Backend>,
    worker: Option<JoinHandle<Result<Option<String>, crate::Error>>>,
    cancelled: Arc<AtomicBool>,
    pub result: Option<String>,
}
impl Check {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self {
            backend,
            worker: None,
            cancelled: Arc::default(),
            result: None,
        }
    }
    pub fn running(&self) -> bool {
        self.worker.is_some()
    }
    pub fn start(&mut self) -> Result<(), crate::Error> {
        if self.running() {
            return Ok(());
        }
        let backend = self.backend.clone();
        let cancelled = self.cancelled.clone();
        self.worker = Some(
            std::thread::Builder::new()
                .name("usbgpu-check".into())
                .spawn(move || backend.check(&cancelled))?,
        );
        self.result = None;
        Ok(())
    }
    pub fn poll(&mut self) -> Result<bool, crate::Error> {
        if !self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            return Ok(false);
        }
        let worker = self
            .worker
            .take()
            .ok_or(crate::Error::Contract("finished eGPU worker missing"))?;
        self.result = worker
            .join()
            .map_err(|_| crate::Error::Contract("eGPU check worker panicked"))??;
        Ok(true)
    }
}
impl Drop for Check {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            match worker.join() {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => eprintln!("eGPU check shutdown: {error}"),
                Err(_) => eprintln!("eGPU check worker panicked during shutdown"),
            }
        }
    }
}
