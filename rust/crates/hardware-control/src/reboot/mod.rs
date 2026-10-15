//! Device-only reboot with a bounded best-effort chime; audio stays in its child.
pub mod sound;
use openpilot_process_supervision::{CaptureError, CapturedChild, CapturedCommand};
use std::{
    env,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    time::Duration,
};

pub const SOUND_TIMEOUT: Duration = Duration::from_secs(4);
pub const REBOOT_TIMEOUT: Duration = Duration::from_secs(15);
pub struct Config {
    pub system: PathBuf,
    pub repository: PathBuf,
    pub helper: PathBuf,
    pub launcher: PathBuf,
    pub sudo: PathBuf,
    pub params: PathBuf,
}
impl Config {
    pub fn for_runtime() -> io::Result<Self> {
        let helper = env::current_exe()?.with_file_name("openpilot-reboot");
        let repository = env::var_os("OPENPILOT_ROOT")
            .or_else(|| env::var_os("BASEDIR"))
            .map(PathBuf::from)
            .or_else(|| helper.parent()?.parent()?.parent().map(Path::to_path_buf))
            .ok_or_else(|| io::Error::other("cannot locate reboot assets"))?;
        Ok(Self {
            system: "/".into(),
            launcher: helper.with_file_name("openpilot-process-child"),
            helper,
            repository,
            sudo: "sudo".into(),
            params: "/data/params/d".into(),
        })
    }
    pub fn device_only(&self) -> io::Result<()> {
        if self.system.join("AGNOS").exists() || self.system.join("TICI").exists() {
            Ok(())
        } else {
            Err(io::Error::other(
                "Device reboot is disabled on this computer.",
            ))
        }
    }
    fn request(&self, argv: Vec<OsString>) -> CapturedCommand {
        CapturedCommand {
            launcher: self.launcher.clone(),
            cwd: self.repository.clone(),
            argv,
        }
    }
}
pub trait Commands {
    fn device_only(&mut self) -> io::Result<()>;
    fn sound(&mut self, timeout: Duration) -> Result<ExitStatus, CaptureError>;
    fn reboot(&mut self, timeout: Duration) -> Result<ExitStatus, CaptureError>;
    fn notice(&mut self, text: &str);
}
fn failure(error: CaptureError) -> io::Error {
    io::Error::other(error.to_string())
}
fn sound_failure(result: Result<ExitStatus, CaptureError>) -> Option<&'static str> {
    match result {
        Ok(status) if status.success() => None,
        Ok(_) => Some("CalledProcessError"),
        Err(CaptureError::Timeout { .. }) => Some("TimeoutExpired"),
        Err(CaptureError::Io(e))
        | Err(CaptureError::Launch(openpilot_process_supervision::Error::Io(e))) => {
            Some(match e.kind() {
                io::ErrorKind::NotFound => "FileNotFoundError",
                io::ErrorKind::PermissionDenied => "PermissionError",
                _ => "OSError",
            })
        }
        Err(_) => Some("OSError"),
    }
}
pub fn run(commands: &mut impl Commands) -> io::Result<()> {
    commands.device_only()?;
    if let Some(kind) = sound_failure(commands.sound(SOUND_TIMEOUT)) {
        commands.notice(&format!("[reboot] chime unavailable: {kind}"));
    }
    let status = commands.reboot(REBOOT_TIMEOUT).map_err(failure)?;
    if !status.success() {
        return Err(io::Error::other(format!("reboot command failed: {status}")));
    }
    Ok(())
}
struct Native<'a>(&'a Config);
impl Commands for Native<'_> {
    fn device_only(&mut self) -> io::Result<()> {
        self.0.device_only()
    }
    fn sound(&mut self, timeout: Duration) -> Result<ExitStatus, CaptureError> {
        self.0
            .request(vec![
                self.0.helper.clone().into_os_string(),
                "--sound-only".into(),
            ])
            .capture(timeout, None)
            .map(|o| o.status)
    }
    fn reboot(&mut self, timeout: Duration) -> Result<ExitStatus, CaptureError> {
        self.0
            .request(vec![
                self.0.sudo.clone().into_os_string(),
                "-n".into(),
                "reboot".into(),
            ])
            .capture(timeout, None)
            .map(|o| o.status)
    }
    fn notice(&mut self, text: &str) {
        println!("{text}");
    }
}
pub fn reboot(config: &Config) -> io::Result<()> {
    run(&mut Native(config))
}
pub fn delay(seconds: f64) -> Duration {
    let upper = if seconds < 5.0 { seconds } else { 5.0 };
    Duration::from_secs_f64(if upper > 0.0 { upper } else { 0.0 })
}
pub fn spawn(config: &Config, seconds: f64) -> io::Result<CapturedChild> {
    config.device_only()?;
    config
        .request(vec![
            config.helper.clone().into_os_string(),
            "--delay".into(),
            seconds.to_string().into(),
        ])
        .spawn_session_redirected(Stdio::inherit(), Stdio::inherit())
        .map_err(io::Error::other)
}
pub fn play_sound(config: &Config) -> io::Result<()> {
    let setting = fs::read_to_string(config.params.join("SoundVolumeAdjust")).ok();
    let volume = sound::volume(setting.as_deref())?;
    if volume == 0.0 {
        return Ok(());
    }
    let pcm = sound::load(
        &config
            .repository
            .join("openpilot/selfdrive/assets/sounds_eng/prompt.wav"),
        volume,
    )?;
    openpilot_portaudio::play_blocking(
        Path::new("libportaudio.so.2"),
        &pcm.samples,
        pcm.channels,
        pcm.rate,
    )
    .map_err(io::Error::other)
}
#[cfg(test)]
mod tests;
