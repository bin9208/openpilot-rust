use crate::{
    manifest::{self, Paths},
    runtime::{self, Commands, NativeCommands},
    Error, Observer,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub paths: Paths,
    pub abctl: PathBuf,
    pub launcher: PathBuf,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            paths: Paths::default(),
            abctl: "abctl".into(),
            launcher: "openpilot-process-child".into(),
        }
    }
}
pub struct Console;
impl Observer for Console {
    fn log(&mut self, level: &str, text: &str) {
        eprintln!(
            "{}:root:{text}",
            if level == "exception" {
                "ERROR".into()
            } else {
                level.to_uppercase()
            }
        );
    }
    fn progress(&mut self, stage: &str, progress: i64) {
        use std::io::Write;
        println!("{stage}: {}", progress.clamp(0, 100));
        let _ = std::io::stdout().flush();
    }
    fn sleep(&mut self, seconds: u64) {
        std::thread::sleep(Duration::from_secs(seconds));
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Flash,
    Verify,
    Swap,
}
pub fn execute(
    paths: &Paths,
    manifest: &Path,
    slot: u32,
    mode: Mode,
    retry_network: bool,
    commands: &mut dyn Commands,
    observer: &mut dyn Observer,
) -> Result<bool, Error> {
    match mode {
        Mode::Verify => {
            observer.progress("Verifying update", 0);
            if runtime::verify(paths, manifest, slot)? {
                runtime::swap(paths, manifest, slot, commands, observer)?;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        Mode::Swap => {
            for attempt in 0..=3 {
                observer.progress(&format!("Verifying update {}/4", attempt + 1), 0);
                if runtime::verify(paths, manifest, slot)? {
                    break;
                }
                if attempt >= 3 {
                    return Err(Error::Contract(
                        "AGNOS verification failed after 3 flash attempts".into(),
                    ));
                }
                observer.log(
                    "error",
                    &format!("Verification failed. Flashing AGNOS ({}/3)", attempt + 1),
                );
                runtime::flash(
                    paths,
                    manifest,
                    slot,
                    true,
                    retry_network,
                    commands,
                    observer,
                )?;
            }
            observer.log(
                "warning",
                &format!("Verification succeeded. Swapping to slot {slot}"),
            );
            runtime::swap(paths, manifest, slot, commands, observer)?;
            observer.progress("Update complete; rebooting", 100);
            Ok(true)
        }
        Mode::Flash => {
            runtime::flash(
                paths,
                manifest,
                slot,
                true,
                retry_network,
                commands,
                observer,
            )?;
            Ok(true)
        }
    }
}
pub fn run() -> Result<bool, Error> {
    let mut config = Config::default();
    let mut verify = false;
    let mut swap = false;
    let mut retry = false;
    let mut manifest = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--verify" => verify = true,
            "--swap" => swap = true,
            "--retry-network" => retry = true,
            "--config" => {
                let path = args
                    .next()
                    .ok_or_else(|| Error::Contract("--config requires a JSON path".into()))?;
                config = serde_json::from_slice(&std::fs::read(path)?)?;
            }
            "--help" | "-h" => {
                println!("openpilot-agnos [--verify] [--swap] [--retry-network] [--config PATH] MANIFEST\nFlash and verify AGNOS update. Config overrides support owned offline fixtures; defaults address device partitions.");
                return Ok(true);
            }
            _ if argument.starts_with('-') => {
                return Err(Error::Contract(format!("unknown argument: {argument}")))
            }
            _ if manifest.is_none() => manifest = Some(PathBuf::from(argument)),
            _ => return Err(Error::Contract("unexpected argument".into())),
        }
    }
    let manifest = manifest.ok_or_else(|| Error::Contract("manifest JSON path required".into()))?;
    let _lock = manifest::acquire_lock(&config.paths)?;
    let mut commands = NativeCommands {
        abctl: config.abctl,
        launcher: config.launcher,
    };
    let slot = runtime::target_slot(&mut commands)?;
    let mode = if verify {
        Mode::Verify
    } else if swap {
        Mode::Swap
    } else {
        Mode::Flash
    };
    execute(
        &config.paths,
        &manifest,
        slot,
        mode,
        retry,
        &mut commands,
        &mut Console,
    )
}
