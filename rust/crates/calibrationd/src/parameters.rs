//! Original Params paths, std::stof-compatible reads and off-thread durable writes.
use crate::Error;
use openpilot_params::Params;
use std::{
    sync::mpsc,
    thread::{self, JoinHandle},
};

pub fn open() -> Result<Params, Error> {
    Ok(Params::for_runtime()?)
}

pub fn parse_float(bytes: &[u8]) -> Result<f64, Error> {
    if bytes.is_empty() {
        return Ok(0.0);
    }
    let mut terminated = bytes.to_vec();
    terminated.push(0);
    let start = terminated.as_ptr().cast();
    let mut end = std::ptr::null_mut();
    // SAFETY: the NUL-terminated buffer and writable end pointer outlive strtof; errno is thread-local.
    let (value, error) = unsafe {
        *libc::__errno_location() = 0;
        let value = libc::strtof(start, &mut end);
        (value, *libc::__errno_location())
    };
    if start == end || error == libc::ERANGE {
        return Err(Error::Contract("invalid float parameter"));
    }
    Ok(f64::from(value))
}

pub fn yaw_trim(params: &Params) -> Result<f64, Error> {
    Ok(parse_float(
        params
            .get("CameraYawTrimDeg")?
            .as_deref()
            .unwrap_or_default(),
    )? * 0.01)
}

enum Write {
    Calibration(Vec<u8>),
    Finish,
}

pub struct PendingWrites {
    sender: mpsc::Sender<Write>,
    worker: Option<JoinHandle<()>>,
}

impl PendingWrites {
    pub fn new(params: Params) -> Result<Self, Error> {
        let (sender, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("calibration-params".to_owned())
            .spawn(move || {
                while let Ok(command) = receiver.recv() {
                    match command {
                        Write::Calibration(bytes) => {
                            if let Err(error) = params.put("CalibrationParams", &bytes) {
                                eprintln!("calibrationd: error writing CalibrationParams: {error}");
                            }
                        }
                        Write::Finish => break,
                    }
                }
            })?;
        Ok(Self {
            sender,
            worker: Some(worker),
        })
    }

    pub fn put(&self, bytes: Vec<u8>) -> Result<(), Error> {
        self.sender
            .send(Write::Calibration(bytes))
            .map_err(|_| Error::Contract("Params writer stopped"))
    }
}

impl Drop for PendingWrites {
    fn drop(&mut self) {
        // An already stopped writer needs no further signal; otherwise drain queued writes like Params::~Params.
        let _ = self.sender.send(Write::Finish);
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            eprintln!("calibrationd: Params writer panicked");
        }
    }
}
