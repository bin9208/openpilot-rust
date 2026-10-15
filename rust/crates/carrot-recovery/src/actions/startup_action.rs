//! Isolated action process used only by the failed-startup recovery owner.
use crate::{config::Config, Error};
use openpilot_hardware_control::reboot;
use openpilot_startup_recovery::state::{Progress, Update};
use std::{
    ffi::OsString,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub fn save(path: &Path, progress: &Progress) -> Result<(), Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Source("missing recovery state directory".into()))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(file.as_file_mut(), progress)
        .map_err(|e| Error::Source(e.to_string()))?;
    file.flush()?;
    file.persist(path).map_err(|e| Error::Io(e.error))?;
    Ok(())
}
pub fn read(path: &Path) -> Result<Progress, Error> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(32769).read_to_end(&mut bytes)?;
    if bytes.len() > 32768 {
        return Err(Error::Source("oversized recovery state".into()));
    }
    let progress: Progress =
        serde_json::from_slice(&bytes).map_err(|e| Error::Source(e.to_string()))?;
    if !matches!(
        progress.state.status.as_str(),
        "idle" | "waiting" | "updating" | "failed" | "rebooting" | "cleaning" | "rebuild_rebooting"
    ) || progress.state.detail.chars().count() > 2000
        || progress
            .pending_reboot
            .as_ref()
            .is_some_and(|s| s.len() != 40 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(Error::Source("invalid recovery state".into()));
    }
    Ok(progress)
}
pub(super) fn command(arguments: &[OsString]) -> Result<Option<i32>, Error> {
    if arguments.first().is_none_or(|s| s != "__startup-action") {
        return Ok(None);
    }
    if std::env::var("CARROT_STARTUP_RECOVERY").as_deref() != Ok("1") {
        return Err(Error::Source(
            "Recovery actions are available only from the failed-startup launcher.".into(),
        ));
    }
    let mut config = Config::original()?;
    let mut state_file = None;
    let mut pending = None;
    let mut touch_log = None;
    let mut automatic = false;
    let mut rebuild = false;
    let mut args = arguments[1..].iter();
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--repo") => {
                config.repository = PathBuf::from(
                    args.next()
                        .ok_or_else(|| Error::Source("missing recovery repository".into()))?,
                )
            }
            Some("--state-file") => {
                state_file =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        Error::Source("missing recovery state file".into())
                    })?))
            }
            Some("--pending") => {
                let value = args
                    .next()
                    .and_then(|v| v.to_str())
                    .ok_or_else(|| Error::Source("missing pending revision".into()))?;
                if value.len() != 40 || !value.bytes().all(|v| v.is_ascii_hexdigit()) {
                    return Err(Error::Source("invalid pending revision".into()));
                }
                pending = Some(value.to_owned());
            }
            Some("--touch-log") => {
                touch_log =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        Error::Source("missing recovery touch log".into())
                    })?));
            }
            Some("--automatic") => automatic = true,
            Some("--rebuild") => rebuild = true,
            _ => return Err(Error::Source("unexpected startup action argument".into())),
        }
    }
    if automatic && rebuild {
        return Err(Error::Source("automatic repair is not allowed".into()));
    }
    let path = state_file.ok_or_else(|| Error::Source("missing recovery state file".into()))?;
    // This dispatch occurs before the HTTP server or any worker threads start.
    std::env::set_var("GIT_TERMINAL_PROMPT", "0");
    let update = Update::resume(pending.clone());
    update.begin(rebuild);
    save(&path, &update.progress())?;
    let publish_update = update.clone();
    let publish_path = path.clone();
    let touch_log = touch_log.map(openpilot_startup_recovery::touch_log::TouchLog::for_path);
    let mut reboot_config = reboot::Config::for_runtime()?;
    reboot_config.repository = config.repository.clone();
    let mut backend = super::StartupBackend::new(config, reboot_config, automatic, pending)
        .before_reboot(move || {
            save(&publish_path, &publish_update.progress()).map_err(|e| e.to_string())?;
            if let Some(log) = &touch_log {
                use openpilot_startup_recovery::touch_log::Value;
                // The diagnostic is best effort, including failures to fsync.
                log.record(
                    "reboot",
                    true,
                    &[(
                        "request_exists",
                        Value::Bool(Path::new("/data/rebuild_all.request").exists()),
                    )],
                );
            }
            Ok(())
        });
    if rebuild {
        update.run_rebuild(&mut backend);
    } else {
        update.run_update(&mut backend, automatic);
    }
    save(&path, &update.progress())?;
    Ok(Some(0))
}
