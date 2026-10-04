mod camera;
mod publisher;
mod road;
mod wide;
use super::{http, platform, shared::Shared, Error};
use crate::{
    config::Config,
    service::{Model, State},
};
use openpilot_logmessaged::JsonValue;
use openpilot_params::Params;
use std::{
    fs,
    net::SocketAddrV4,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    thread,
    time::Duration,
};

pub struct Options {
    pub root: PathBuf,
    pub assets: PathBuf,
    pub config: PathBuf,
    pub address: SocketAddrV4,
}

fn config(path: &Path) -> Result<Config, Error> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => return Ok(Config::default()),
    };
    let value = match JsonValue::parse(&text) {
        Ok(value) => value,
        Err(_) => return Ok(Config::default()),
    };
    match Config::normalize_json(&value) {
        Ok(config) => Ok(config),
        Err(crate::Error::Invalid(_)) => Ok(Config::default()),
        Err(error) => Err(error.into()),
    }
}

pub fn run(options: Options, parent_running: Arc<AtomicBool>) -> Result<(), Error> {
    platform::background_affinity()?;
    openpilot_opencv_runtime::initialize(2)?;
    let models = ["v_asm_model.onnx", "lane.onnx"].map(|file| Model {
        path: options.assets.join(file).to_string_lossy().into_owned(),
        loaded: false,
        error: String::new(),
    });
    let params = match Params::for_runtime() {
        Ok(params) => Some(params),
        Err(error) => {
            eprintln!("Xiaoge vision parameters unavailable: {error}");
            None
        }
    };
    let shared = Arc::new(Shared {
        state: Mutex::new(State::new(
            config(&options.config)?,
            models,
            platform::monotonic()?,
        )),
        snapshot: Condvar::new(),
        vasm_operation: Mutex::new(()),
        params,
        config_path: options.config,
        running: AtomicBool::new(true),
    });
    let publisher = publisher::Publisher::start()?;
    let (ready, wait) = mpsc::sync_channel(2);
    let mut starters = Vec::new();
    for (name, worker) in [
        ("wide", wide::run as CameraWorker),
        ("road", road::run as CameraWorker),
    ] {
        let (start, started) = mpsc::sync_channel(1);
        starters.push(start);
        let shared = Arc::clone(&shared);
        let publisher = publisher.clone();
        let ready = ready.clone();
        thread::Builder::new()
            .name(format!("xiaoge-{name}"))
            .spawn(move || {
                if let Err(error) = worker(shared, publisher, ready.clone(), started) {
                    eprintln!("Xiaoge {name} camera thread failed: {error}");
                    if let Err(error) = ready.send(Err(error)) {
                        eprintln!("Xiaoge {name} startup receiver closed: {error}");
                    }
                }
            })?;
    }
    drop(ready);
    for _ in 0..2 {
        wait.recv()
            .map_err(|_| Error::Contract("camera owner startup failed"))??;
    }
    shared.refresh(true)?;
    let listener = http::bind(options.address)?;
    for start in starters {
        start
            .send(())
            .map_err(|_| Error::Contract("camera owner stopped before HTTP startup"))?;
    }
    let shutdown = Arc::clone(&shared);
    thread::Builder::new()
        .name("xiaoge-vision-stop".to_owned())
        .spawn(move || {
            while parent_running.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(20));
            }
            shutdown.shutdown();
        })?;
    http::run(
        listener,
        shared,
        options
            .root
            .join("openpilot/selfdrive/carrot/xiaoge/v_asm_web.html"),
    )
}

type CameraWorker = fn(
    Arc<Shared>,
    publisher::Publisher,
    mpsc::SyncSender<Result<(), Error>>,
    mpsc::Receiver<()>,
) -> Result<(), Error>;
