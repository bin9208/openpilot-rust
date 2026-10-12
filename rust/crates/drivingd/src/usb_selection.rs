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
        Ok(Self {
            paths: Paths::for_runtime(
                &std::env::current_dir()?,
                std::env::var_os("USBGPU_ASSETS_ROOT")
                    .map(PathBuf::from)
                    .map_or_else(
                        || std::env::current_exe().map(|path| path.with_file_name("usbgpu-assets")),
                        Ok,
                    )?,
            )?,
            devices: hardware::SYSFS.into(),
            worker: std::env::current_exe()?.with_file_name("openpilot-usbgpu-worker"),
            cancelled,
        })
    }
    pub fn present(&self) -> Result<bool, Error> {
        Ok(openpilot_usbgpu::model_delivery::presence::present(
            &self.devices,
        ))
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
        model::compiled_path(&config.paths).is_some(),
    )?;
    Ok(())
}

pub fn start(
    config: &Configuration,
    params: &Params,
    camera: [u32; 2],
) -> Result<Option<UsbModel>, Error> {
    let path = model::compiled_path(&config.paths);
    let compiled = path.is_some();
    let mut present = config.present()?;
    if !present && compiled && params.get_bool("UsbGpuHardwareSeen")? {
        let started = Instant::now();
        while !present && started.elapsed() < Duration::from_secs(5) {
            if config.cancelled.load(Ordering::Relaxed) {
                clear_activity(params)?;
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
    let binding = match openpilot_usbgpu::worker_artifact::bind(&path, &config.paths.assets) {
        Ok(binding) => binding,
        Err(error) => {
            fail(params)?;
            return Err(
                openpilot_usbgpu::Error::Protocol(format!("native model assets: {error}")).into(),
            );
        }
    };
    let started = Instant::now();
    for attempt in 0..6 {
        let remaining = Duration::from_secs(120).saturating_sub(started.elapsed());
        if remaining.is_zero() {
            break;
        }
        match UsbModel::launch_with_assets(
            Launch {
                worker: &config.worker,
                model: &path,
                camera,
                timeout: remaining.min(Duration::from_secs(110)),
                cancelled: Arc::clone(&config.cancelled),
            },
            &binding,
        ) {
            Ok(model) => {
                params.put_bool("UsbGpuActive", true)?;
                return Ok(Some(model));
            }
            Err(error) => {
                if config.cancelled.load(Ordering::Relaxed) {
                    clear_activity(params)?;
                    return Ok(None);
                }
                if attempt < 5
                    && openpilot_usbgpu::model_delivery::failure::pcie_not_ready(&error.to_string())
                {
                    let delay = Instant::now();
                    while delay.elapsed() < Duration::from_secs(2) {
                        if config.cancelled.load(Ordering::Relaxed) {
                            clear_activity(params)?;
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

fn clear_activity(params: &Params) -> Result<(), Error> {
    params.put_bool("UsbGpuActive", false)?;
    params.put_bool("UsbGpuLoading", false)?;
    Ok(())
}

pub fn fail(params: &Params) -> Result<(), Error> {
    clear_activity(params)?;
    params.put_bool("UsbGpuStartupFailed", true)?;
    Ok(())
}

pub fn published(params: &Params) -> Result<(), Error> {
    params.put_bool("UsbGpuLoading", false)?;
    Ok(())
}
