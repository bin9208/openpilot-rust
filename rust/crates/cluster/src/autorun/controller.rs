use super::{entry, events::Events, gpu, params::Reader, product_id, Output};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cluster autorun interrupted")]
    Interrupted,
    #[error(transparent)]
    Params(#[from] super::params::Error),
    #[error(transparent)]
    Gpu(#[from] gpu::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub trait Platform {
    fn interrupted(&self) -> bool {
        false
    }
    /// # Errors
    /// Returns an outer managed interrupt while preserving the source retry interval.
    fn sleep(&self, duration: Duration) -> Result<(), Error> {
        std::thread::sleep(duration);
        Ok(())
    }
    fn board(&self) -> bool;
    /// # Errors
    /// Returns failure to restore the source offroad scheduling policy.
    fn offroad(&mut self) -> Result<(), Error>;
    /// # Errors
    /// Returns an unrecovered source startup-wait failure.
    fn startup(&mut self, params: &Reader) -> Result<(), Error>;
    /// # Errors
    /// Returns model startup failure before opening the selected transport.
    fn startup_transport(&mut self, params: &Reader, _shared_usb: bool) -> Result<(), Error> {
        self.startup(params)
    }
    /// # Errors
    /// Returns bounded discovery failure before allocating graphics/encoder resources.
    fn discover_wifi(&mut self) -> Result<(), Error> {
        Ok(())
    }
    /// # Errors
    /// Preserves invalid/unreadable registration failures from the source loader.
    fn wifi_registered(&mut self) -> Result<bool, Error> {
        Ok(false)
    }
    fn find(&mut self, product: u16) -> Option<u16>;
    fn dim(&mut self, product: u16, reason: &str) -> bool;
    fn events(&mut self) -> Events;
}

fn wifi_ready(platform: &mut impl Platform) -> Result<bool, Error> {
    println!("[cluster_autorun] discovering registered S3 Wi-Fi HUD");
    if let Err(error) = platform.discover_wifi() {
        eprintln!("[cluster_autorun] S3 Wi-Fi discovery unavailable ({error}); retrying in 5s");
        platform.sleep(Duration::from_secs(5))?;
        return Ok(false);
    }
    Ok(true)
}

/// # Errors
/// Startup, Params initialization and scheduling failures retain the source outer fatal boundary.
pub fn run(
    params: &Reader,
    platform: &mut impl Platform,
    entrypoint: &mut impl entry::Entrypoint,
) -> Result<(), Error> {
    params.initialize()?;
    loop {
        if platform.interrupted() {
            return Err(Error::Interrupted);
        }
        platform.offroad()?;
        let settings = params.settings();
        let Some(mut expected) = product_id(settings.hud_mode) else {
            println!(
                "[cluster_autorun] ClusterHud={}; stopping cluster HUD",
                settings.hud_mode
            );
            return Ok(());
        };
        if !params.output_allowed() {
            if super::wait::output_allowed(params, platform, expected).is_none() {
                return if platform.interrupted() {
                    Err(Error::Interrupted)
                } else {
                    Ok(())
                };
            }
            continue;
        }
        let wifi = platform.wifi_registered()?;
        if wifi && !wifi_ready(platform)? {
            continue;
        }
        platform.startup_transport(params, !wifi)?;
        if !wifi && platform.find(expected).is_none() {
            if !platform.board() {
                println!(
                    "[cluster_autorun] {} not found on PC; starting window-only HUD",
                    super::usb::label(expected)
                );
                if let Err(error) = entry::run_once(
                    entrypoint,
                    settings.hud_mode,
                    settings.encoder,
                    Output::Window,
                    false,
                    platform.board(),
                ) {
                    eprintln!(
                        "[cluster_autorun] cluster HUD window failed: {error}; retrying in 5s"
                    );
                    eprintln!("{error}");
                    platform.sleep(Duration::from_secs(5))?;
                }
                continue;
            }
            let Some(found) = super::wait::supported_device(params, platform, expected)? else {
                continue;
            };
            expected = found;
        }
        if !wifi {
            println!(
                "[cluster_autorun] found {}; starting cluster HUD",
                super::usb::label(expected)
            );
        }
        let active = params.boolean("UsbGpuActive")?;
        match entry::run_once(
            entrypoint,
            settings.hud_mode,
            settings.encoder,
            Output::Usb,
            active,
            platform.board(),
        ) {
            Ok(()) => {
                let next = params.settings();
                if next.hud_mode != settings.hud_mode
                    || next.encoder.setting() != settings.encoder.setting()
                    || (next.orientation.is_some() && next.orientation != settings.orientation)
                {
                    println!(
                        "[cluster_autorun] HUD setting changed mode {}->{}, encoder {}->{}, orientation {}->{}; rechecking",
                        settings.hud_mode,
                        next.hud_mode,
                        settings.encoder.setting(),
                        next.encoder.setting(),
                        orientation_name(settings.orientation),
                        orientation_name(next.orientation)
                    );
                    continue;
                }
                println!("[cluster_autorun] cluster HUD exited; retrying in 5s");
            }
            Err(error) => {
                log_retry_failure(&error);
            }
        }
        platform.sleep(Duration::from_secs(5))?;
    }
}

fn orientation_name(value: Option<u8>) -> String {
    value.map_or_else(|| "None".into(), |value| value.to_string())
}

fn log_retry_failure(error: &entry::Failure) {
    if let Some(reason) = entry::s3_transport_failure_reason(error) {
        println!("[cluster_autorun] S3 Wi-Fi disconnected ({reason}); retrying in 5s");
    } else if entry::disconnected(error) {
        println!("[cluster_autorun] USB display disconnected; retrying in 5s");
    } else {
        eprintln!("[cluster_autorun] cluster HUD failed: {error}; retrying in 5s");
        eprintln!("{error}");
    }
}
