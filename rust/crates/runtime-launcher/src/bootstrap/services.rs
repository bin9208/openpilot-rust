use super::Paths;
use crate::Error;
use openpilot_process_supervision::CapturedCommand;
use std::{fs::OpenOptions, path::Path, process::Command, thread};

fn running(binary: &Path) -> Result<bool, Error> {
    Ok(openpilot_process_supervision::reconcile_native_service(
        binary,
    )?)
}

pub(super) fn background(
    paths: &Paths,
    binary: &str,
    args: Vec<std::ffi::OsString>,
    log: &str,
) -> Result<(), Error> {
    if running(&paths.binary(binary))? {
        return Ok(());
    }
    if binary == "openpilot-carrot-web-watchdog" {
        // A forcibly retired watchdog may leave its old server behind.
        running(&paths.binary("openpilot-carrot-server"))?;
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
    if running(&paths.binary("openpilot-usbgpu-model"))? {
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
        "--retry-network".into(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::process::CommandExt;

    fn wait_for_exec(child: &std::process::Child, executable: std::path::PathBuf) {
        use std::os::unix::fs::MetadataExt;
        let expected = fs::metadata(executable).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if fs::read(format!("/proc/{}/cmdline", child.id()))
                .is_ok_and(|bytes| !bytes.is_empty())
                && fs::metadata(format!("/proc/{}/exe", child.id())).is_ok_and(|actual| {
                    (actual.dev(), actual.ino()) == (expected.dev(), expected.ino())
                })
            {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "child did not exec");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn misleading_argv_basename_does_not_block_native_service() {
        let binary = format!("openpilot-unrelated-test-{}", std::process::id());
        let mut child = Command::new("/bin/sleep")
            .arg0(&binary)
            .arg("30")
            .spawn()
            .unwrap();
        wait_for_exec(&child, std::path::PathBuf::from("/bin/sleep"));
        let directory = tempfile::tempdir().unwrap();
        let expected = directory.path().join(&binary);
        fs::copy("/bin/true", &expected).unwrap();
        let found = running(&expected).unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(
            !found,
            "unrelated executable blocked the expected native service"
        );
    }
}
