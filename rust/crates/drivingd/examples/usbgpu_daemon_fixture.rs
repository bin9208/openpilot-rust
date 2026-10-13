#[path = "../src/daemon.rs"]
mod daemon;
#[path = "../src/process.rs"]
mod process;

use openpilot_driving_modeld::{usb_selection, Error};
use openpilot_logging::producer::Factory;
use openpilot_usbgpu::model::Paths;
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

pub struct Options {
    catalog: PathBuf,
    frames: Option<u64>,
}

fn main() -> Result<(), Error> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 5 {
        return Err(Error::Contract(
            "expected catalog, fixture root, assets, worker and frames",
        ));
    }
    let root = PathBuf::from(&args[1]);
    let frames = args[4]
        .to_str()
        .and_then(|value| value.parse().ok())
        .filter(|count| *count > 0)
        .ok_or(Error::Contract("frame count"))?;
    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&stop))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stop))?;
    let config = daemon::Configuration {
        options: Options {
            catalog: args[0].clone().into(),
            frames: Some(frames),
        },
        usb: usb_selection::Configuration {
            paths: Paths {
                cache: root.join("cache"),
                models: root.join("models"),
                assets: args[2].clone().into(),
            },
            devices: root.join("sysfs"),
            worker: args[3].clone().into(),
            cancelled: Arc::clone(&stop),
        },
    };
    let mut logger = Factory::for_runtime()?.logger();
    process::configure()?;
    let result = daemon::run(config, &mut logger, &stop);
    logger.close();
    result
}
