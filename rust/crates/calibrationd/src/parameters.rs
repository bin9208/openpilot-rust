//! Original Params paths, std::stof-compatible reads and off-thread durable writes.
use crate::Error;
use openpilot_params::Params;
use std::{
    env,
    path::{Path, PathBuf},
    sync::mpsc,
    thread::{self, JoinHandle},
};

pub fn open() -> Result<Params, Error> {
    let prefix = env::var("OPENPILOT_PREFIX").unwrap_or_else(|_| "d".to_owned());
    let root = env::var_os("PARAMS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if Path::new("/TICI").is_file() {
                PathBuf::from("/data/params")
            } else {
                PathBuf::from(env::var_os("HOME").unwrap_or_default())
                    .join(format!(
                        ".comma{}",
                        env::var("OPENPILOT_PREFIX").unwrap_or_default()
                    ))
                    .join("params")
            }
        });
    Ok(Params::open(&root, &prefix)?)
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
