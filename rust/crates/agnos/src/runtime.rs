use crate::{
    image,
    manifest::{self, Paths},
    Error, Observer,
};
use openpilot_process_supervision::CapturedCommand;
use std::{
    ffi::OsString,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Stdio,
};

pub trait Commands {
    fn abctl(&mut self, args: &[&str], merged: bool) -> Result<(bool, String), Error>;
    fn set_unbootable(&mut self, slot: u32) -> Result<(), Error>;
}
pub struct NativeCommands {
    pub launcher: PathBuf,
    pub abctl: PathBuf,
}
impl NativeCommands {
    fn command(&self, argv: Vec<OsString>) -> Result<CapturedCommand, Error> {
        Ok(CapturedCommand {
            launcher: self.launcher.clone(),
            cwd: std::env::current_dir()?,
            argv,
        })
    }
}
impl Commands for NativeCommands {
    fn abctl(&mut self, args: &[&str], merged: bool) -> Result<(bool, String), Error> {
        let mut output = tempfile::tempfile()?;
        let mut argv = vec![self.abctl.clone().into_os_string()];
        argv.extend(args.iter().map(OsString::from));
        let stderr = if merged {
            output.try_clone()?.into()
        } else {
            Stdio::inherit()
        };
        let mut child = self
            .command(argv)?
            .spawn_redirected(output.try_clone()?.into(), stderr)?;
        let status = child.process.wait()?;
        output.seek(SeekFrom::Start(0))?;
        let mut text = String::new();
        output.read_to_string(&mut text)?;
        Ok((status.success(), text))
    }
    fn set_unbootable(&mut self, slot: u32) -> Result<(), Error> {
        let mut child = self
            .command(vec![
                "/bin/sh".into(),
                "-c".into(),
                "\"$0\" --set_unbootable \"$1\"".into(),
                self.abctl.clone().into_os_string(),
                slot.to_string().into(),
            ])?
            .spawn_inherited()?;
        child.process.wait()?;
        Ok(())
    }
}
pub fn target_slot(commands: &mut dyn Commands) -> Result<u32, Error> {
    let (success, slot) = commands.abctl(&["--boot_slot"], false)?;
    if !success {
        return Err(Error::Contract("abctl --boot_slot failed".into()));
    }
    Ok(if slot.trim() == "_a" { 1 } else { 0 })
}
pub fn verify(paths: &Paths, manifest: &Path, slot: u32) -> Result<bool, Error> {
    for partition in manifest::load(manifest)? {
        if !image::verify(paths, slot, &partition, false)? {
            return Ok(false);
        }
    }
    Ok(true)
}
pub fn flash(
    paths: &Paths,
    manifest: &Path,
    slot: u32,
    standalone: bool,
    retry_network: bool,
    commands: &mut dyn Commands,
    observer: &mut dyn Observer,
) -> Result<(), Error> {
    let partitions = manifest::load(manifest)?;
    observer.log("info", &format!("Target slot {slot}"));
    commands.set_unbootable(slot)?;
    for partition in partitions {
        let mut attempt = 1;
        loop {
            match image::flash(paths, slot, &partition, standalone, observer) {
                Ok(()) => break,
                Err(Error::Request {
                    class,
                    message: _,
                    transient,
                }) => {
                    observer.log("exception", "Failed");
                    let waiting = retry_network && transient;
                    if attempt >= 5 && !waiting {
                        return Err(Error::Contract(format!("Download failed after {attempt} attempts. Check the connection or update server, then Retry.")));
                    }
                    observer.log(
                        "info",
                        &format!(
                            "Failed to download {}, retrying (attempt {attempt}): {class}",
                            partition.name
                        ),
                    );
                    observer.progress(
                        if waiting {
                            "Waiting for internet"
                        } else {
                            "Retrying download"
                        },
                        0,
                    );
                    observer.sleep(10);
                    attempt += 1;
                }
                Err(error) => return Err(error),
            }
        }
    }
    observer.log("info", &format!("AGNOS ready on slot {slot}"));
    Ok(())
}
pub fn swap(
    paths: &Paths,
    manifest: &Path,
    slot: u32,
    commands: &mut dyn Commands,
    observer: &mut dyn Observer,
) -> Result<(), Error> {
    for partition in manifest::load(manifest)? {
        if !partition.full_check {
            image::clear_hash(paths, slot, &partition)?;
        }
    }
    let mut last = String::new();
    for attempt in 1..=5 {
        observer.progress(&format!("Switching boot slot {attempt}/5"), 100);
        let (_, output) = commands.abctl(&["--set_active", &slot.to_string()], true)?;
        last = output;
        if !last.contains("No such file or directory") && last.contains("lun as boot lun") {
            observer.log("info", &format!("Swap successful {last}"));
            return Ok(());
        }
        observer.log("error", &format!("Swap failed ({attempt}/5): {last}"));
        if attempt < 5 {
            observer.sleep(1);
        }
    }
    Err(Error::Contract(format!(
        "Failed to switch boot slot after 5 attempts: {}",
        last.trim()
    )))
}
