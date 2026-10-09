use crate::{usb_model::UsbModel, Error};
use openpilot_params::Params;
use openpilot_usbgpu::{
    client::Launch,
    hardware,
    model::{self, Paths},
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub struct Configuration {
    pub paths: Paths,
    pub devices: PathBuf,
    pub worker: PathBuf,
    pub cancelled: Arc<AtomicBool>,
}
impl Configuration {
    pub fn for_runtime(cancelled: Arc<AtomicBool>) -> Result<Self, Error> {
        let cache = match std::env::var_os("CARROT_BIG_MODEL_DIR") {
            Some(path) => PathBuf::from(path),
            None if std::path::Path::new("/TICI").is_file() => {
                PathBuf::from("/data/media/0/carrot/models")
            }
            None => PathBuf::from(
                std::env::var_os("HOME").ok_or(Error::Contract("missing home directory"))?,
            )
            .join(".comma/models"),
        };
        Ok(Self {
            paths: Paths {
                cache,
                models: std::env::current_dir()?.join("openpilot/selfdrive/modeld/models"),
                assets: std::env::current_exe()?.with_file_name("usbgpu-assets"),
            },
            devices: hardware::SYSFS.into(),
            worker: std::env::current_exe()?.with_file_name("openpilot-usbgpu-worker"),
            cancelled,
        })
    }
    pub fn present(&self) -> Result<bool, Error> {
        Ok(!hardware::devices(&self.devices)?.is_empty())
    }
}

pub fn refresh(config: &Configuration, params: &Params) -> Result<(), Error> {
    let present = config.present()?;
    params.put_bool("UsbGpuPresent", present)?;
    if present {
        params.put_bool("UsbGpuHardwareSeen", true)?;
    }
    params.put_bool(
        "UsbGpuCompiled",
        model::active_compiled_path(&config.paths).is_some(),
    )?;
    Ok(())
}

pub fn start(
    config: &Configuration,
    params: &Params,
    camera: [u32; 2],
) -> Result<Option<UsbModel>, Error> {
    let path = model::active_compiled_path(&config.paths);
    let compiled = path.is_some();
    let mut present = config.present()?;
    if !present && compiled && params.get_bool("UsbGpuHardwareSeen")? {
        let started = Instant::now();
        while !present && started.elapsed() < Duration::from_secs(5) {
            if config.cancelled.load(Ordering::Relaxed) {
                return Ok(None);
            }
            thread::sleep(Duration::from_millis(100));
            present = config.present()?;
        }
    }
    params.put_bool("UsbGpuPresent", present)?;
    params.put_bool("UsbGpuCompiled", compiled)?;
    if present || compiled {
        params.put_bool("UsbGpuHardwareSeen", true)?;
    }
    let requested = present && compiled && !params.get_bool("UsbGpuStartupFailed")?;
    params.put_bool("UsbGpuLoading", requested)?;
    params.put_bool("UsbGpuActive", false)?;
    if !requested {
        return Ok(None);
    }
    let path = path.ok_or(Error::Contract("compiled USB model disappeared"))?;
    let started = Instant::now();
    for attempt in 0..6 {
        let remaining = Duration::from_secs(120).saturating_sub(started.elapsed());
        if remaining.is_zero() {
            break;
        }
        match UsbModel::launch(Launch {
            worker: &config.worker,
            model: &path,
            camera,
            timeout: remaining.min(Duration::from_secs(110)),
            cancelled: Arc::clone(&config.cancelled),
        }) {
            Ok(model) => {
                params.put_bool("UsbGpuActive", true)?;
                return Ok(Some(model));
            }
            Err(error) => {
                if config.cancelled.load(Ordering::Relaxed) {
                    return Ok(None);
                }
                let detail = error.to_string().to_lowercase();
                if attempt < 5
                    && (detail.contains("pcie link not up") || detail.contains("read(0xb450"))
                {
                    let delay = Instant::now();
                    while delay.elapsed() < Duration::from_secs(2) {
                        if config.cancelled.load(Ordering::Relaxed) {
                            return Ok(None);
                        }
                        thread::sleep(Duration::from_millis(100));
                    }
                    continue;
                }
                fail(params)?;
                return Err(error);
            }
        }
    }
    fail(params)?;
    Err(Error::Contract("eGPU model load timed out after 120s"))
}

pub fn fail(params: &Params) -> Result<(), Error> {
    params.put_bool("UsbGpuActive", false)?;
    params.put_bool("UsbGpuStartupFailed", true)?;
    params.put_bool("UsbGpuLoading", false)?;
    Ok(())
}

pub fn published(params: &Params) -> Result<(), Error> {
    params.put_bool("UsbGpuLoading", false)?;
    Ok(())
}
