use super::Paths;
use crate::Error;
use openpilot_process_supervision::CapturedCommand;
use std::{
    fs::{self, OpenOptions},
    path::Path,
    process::Command,
    thread,
};

fn running(binary: &str) -> bool {
    fs::read_dir("/proc")
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|entry| {
            if entry.file_name().to_string_lossy().parse::<u32>().ok() == Some(std::process::id()) {
                return false;
            }
            fs::read(entry.path().join("cmdline"))
                .ok()
                .is_some_and(|bytes| {
                    let first = bytes.split(|byte| *byte == 0).next().unwrap_or_default();
                    Path::new(std::str::from_utf8(first).unwrap_or_default())
                        .file_name()
                        .is_some_and(|name| name == binary)
                })
        })
}

pub(super) fn background(
    paths: &Paths,
    binary: &str,
    args: Vec<std::ffi::OsString>,
    log: &str,
) -> Result<(), Error> {
    if running(binary) {
        return Ok(());
    }
    let output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.temporary.join(log))?;
    let request = CapturedCommand {
        launcher: paths.binary("openpilot-process-child"),
        cwd: paths.repository.clone(),
        argv: std::iter::once(paths.binary(binary).into_os_string())
            .chain(args)
            .collect(),
    };
    let mut child = request.spawn_session_redirected(output.try_clone()?.into(), output.into())?;
    thread::spawn(move || {
        if let Err(error) = child.process.wait() {
            eprintln!("early service reap: {error}");
        }
    });
    Ok(())
}

pub(super) fn recovery(paths: &Paths) {
    if let Err(error) = background(
        paths,
        "openpilot-carrot-recovery",
        vec!["--port".into(), "6999".into()],
        "carrot_recovery.log",
    ) {
        eprintln!("recovery startup: {error}");
    }
}

pub(super) fn model_update(paths: &Paths) -> Result<(), Error> {
    if running("openpilot-usbgpu-model") {
        return Ok(());
    }
    let mut argv: Vec<std::ffi::OsString> = vec![
        "flock".into(),
        "-n".into(),
        paths
            .temporary
            .join("big_model_update.lock")
            .into_os_string(),
    ];
    if Command::new("ionice").arg("--version").output().is_ok() {
        argv.extend(["ionice".into(), "-c".into(), "3".into()]);
    }
    argv.extend([
        "nice".into(),
        "-n".into(),
        "10".into(),
        paths.binary("openpilot-usbgpu-model").into_os_string(),
        "--ensure-if-egpu".into(),
        "--network-wait-seconds".into(),
        "60".into(),
        "--root".into(),
        paths.repository.clone().into_os_string(),
        "--devices".into(),
        paths.system.join("sys/bus/usb/devices").into_os_string(),
    ]);
    let output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.temporary.join("big_model_update.log"))?;
    let mut child = CapturedCommand {
        launcher: paths.binary("openpilot-process-child"),
        cwd: paths.repository.clone(),
        argv,
    }
    .spawn_session_redirected(output.try_clone()?.into(), output.into())?;
    thread::spawn(move || {
        if let Err(error) = child.process.wait() {
            eprintln!("model update reap: {error}");
        }
    });
    Ok(())
}
