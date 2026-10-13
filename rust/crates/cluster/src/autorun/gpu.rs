use super::params::Reader;
use std::path::Path;
use std::time::{Duration, Instant};

/// # Errors
/// Returns the shared artifact-location error after matching a `SuperSpeed` bridge.
pub fn startup_expected(root: &Path, assets: &Path, sysfs: &Path) -> Result<bool, crate::Error> {
    if !openpilot_usbgpu::model_delivery::presence::present(sysfs) {
        return Ok(false);
    }
    let paths = openpilot_usbgpu::model::Paths::for_runtime(root, assets.to_owned())?;
    Ok(openpilot_usbgpu::model::compiled_path(&paths).is_some())
}

const DISCOVERY_GRACE: Duration = Duration::from_secs(3);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
const STABILIZE: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(100);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cluster GPU startup interrupted")]
    Interrupted,
    #[error(transparent)]
    Params(#[from] super::params::Error),
    #[error("GPU stabilization deadline passed before sleep")]
    NegativeSleep,
}

fn expected(probe: &mut impl FnMut() -> Result<bool, crate::Error>) -> bool {
    match probe() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("[cluster_autorun] could not inspect eGPU startup state: {error}");
            false
        }
    }
}

/// # Errors
/// Returns a Params failure or the source negative-sleep failure at a crossed stabilization deadline.
pub fn wait_for_usbgpu_startup(
    params: &Reader,
    probe: impl FnMut() -> Result<bool, crate::Error>,
) -> Result<(), Error> {
    wait_for_usbgpu_startup_until(params, probe, || false)
}
/// # Errors
/// Preserves the source startup deadlines and returns cooperative outer managed interruption.
pub fn wait_for_usbgpu_startup_until(
    params: &Reader,
    mut probe: impl FnMut() -> Result<bool, crate::Error>,
    interrupted: impl Fn() -> bool,
) -> Result<(), Error> {
    let remembered = params.boolean("UsbGpuHardwareSeen")?;
    let mut expected = expected(&mut probe);
    let discovery_deadline = Instant::now() + DISCOVERY_GRACE;
    while remembered && !expected && Instant::now() < discovery_deadline {
        if interrupted() {
            return Err(Error::Interrupted);
        }
        if params.boolean("UsbGpuLoading")?
            || params.boolean("UsbGpuActive")?
            || params.boolean("UsbGpuStartupFailed")?
        {
            expected = true;
            break;
        }
        std::thread::sleep(POLL);
        expected = self::expected(&mut probe);
    }
    if !expected {
        return Ok(());
    }
    println!("[cluster_autorun] waiting for eGPU startup before opening USB display");
    let startup_deadline = Instant::now() + STARTUP_TIMEOUT;
    while Instant::now() < startup_deadline {
        if interrupted() {
            return Err(Error::Interrupted);
        }
        let loading = params.boolean("UsbGpuLoading")?;
        let active = params.boolean("UsbGpuActive")?;
        let failed = params.boolean("UsbGpuStartupFailed")?;
        if !loading && active {
            println!("[cluster_autorun] eGPU startup complete; keeping USB display idle for 10s");
            let stabilize_deadline = Instant::now() + STABILIZE;
            while Instant::now() < stabilize_deadline {
                if interrupted() {
                    return Err(Error::Interrupted);
                }
                if !params.boolean("UsbGpuActive")? {
                    println!(
                        "[cluster_autorun] eGPU became inactive during stabilization; starting USB display"
                    );
                    return Ok(());
                }
                let remaining = stabilize_deadline
                    .checked_duration_since(Instant::now())
                    .ok_or(Error::NegativeSleep)?;
                std::thread::sleep(POLL.min(remaining));
            }
            println!("[cluster_autorun] eGPU stable; starting USB display at 5 fps");
            return Ok(());
        }
        if !loading && failed {
            println!("[cluster_autorun] eGPU startup failed; starting USB display after fallback");
            return Ok(());
        }
        std::thread::sleep(POLL);
    }
    println!("[cluster_autorun] eGPU startup wait timed out after 60s; starting USB display");
    Ok(())
}
