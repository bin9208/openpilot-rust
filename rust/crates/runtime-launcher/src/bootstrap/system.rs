use super::{pause, wait, Paths};
use crate::{watchdog::Server, Error};
use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

fn available(name: &str) -> bool {
    env::split_paths(&env::var_os("PATH").unwrap_or_default()).any(|path| {
        fs::metadata(path.join(name))
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    })
}

pub(super) fn cleanup_lfs(paths: &Paths) -> Result<(), Error> {
    if available("git-lfs") {
        return Ok(());
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(&paths.repository)
        .args(["rev-parse", "--absolute-git-dir"])
        .output()?;
    if !output.status.success() {
        return Ok(());
    }
    let directory =
        std::path::PathBuf::from(String::from_utf8_lossy(&output.stdout).trim_end_matches('\n'));
    for hook in ["post-checkout", "post-commit", "post-merge", "pre-push"] {
        let path = directory.join("hooks").join(hook);
        if fs::read_to_string(&path).is_ok_and(|text| {
            text.contains("command -v git-lfs") && text.contains(&format!("git lfs {hook}"))
        }) {
            println!("Removing stale git-lfs hook: {}", path.display());
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

pub(super) fn enable_recovery_ssh(paths: &Paths) -> Result<(), Error> {
    let params = paths.data.join("params/d");
    fs::create_dir_all(&params)?;
    if !params.join("GithubSshKeys").is_file() {
        fs::write(params.join("GithubUsername"), "openpilot")?;
        match fs::read(paths.system.join("usr/comma/setup_keys")) {
            Ok(keys) => fs::write(params.join("GithubSshKeys"), keys)?,
            Err(error) => eprintln!("recovery SSH setup keys: {error}"),
        }
    }
    if fs::read(params.join("SshEnabled")).ok().as_deref() != Some(b"1") {
        fs::write(params.join("SshEnabled"), "1")?;
    }
    Ok(())
}

pub(super) fn agnos(paths: &Paths, stopped: &AtomicBool) -> Result<(), Error> {
    if !paths.system.join("AGNOS").is_file() {
        return Ok(());
    }
    if let Ok(entries) = fs::read_dir(paths.data.join("etc/NetworkManager/system-connections")) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().ends_with(".nmmeta") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    let _ = fs::remove_file(paths.data.join("scons_cache/config.lock"));
    let _ = Command::new("sudo")
        .args(["abctl", "--set_success"])
        .status();
    let devices = [
        paths.system.join("dev/adsprpc-smd"),
        paths.system.join("dev/ion"),
        paths.system.join("dev/kgsl-3d0"),
    ];
    let _ = Command::new("sudo")
        .args(["chgrp", "gpu"])
        .args(&devices)
        .status();
    let _ = Command::new("sudo")
        .args(["chmod", "660"])
        .args(&devices)
        .status();
    let confirmation = env::var_os("AGNOS_UPDATE_CONFIRMATION_FILE")
        .map_or_else(|| paths.data.join("agnos-update-confirmed"), Into::into);
    if fs::read_to_string(paths.system.join("VERSION"))
        .unwrap_or_default()
        .trim_end_matches('\n')
        == "19.8-carrot-bt1"
    {
        let _ = fs::remove_file(confirmation);
        return Ok(());
    }
    let model =
        fs::read(paths.system.join("sys/firmware/devicetree/base/model")).unwrap_or_default();
    let model = String::from_utf8_lossy(&model)
        .replace(['\0', '\r', '\n'], "")
        .to_lowercase();
    let model = model.strip_prefix("comma ").unwrap_or(&model);
    let filename = if matches!(model, "c3" | "tici") {
        "agnos-tici.json"
    } else {
        "agnos.json"
    };
    let manifest = paths
        .repository
        .join("openpilot/system/hardware/tici")
        .join(filename);
    let updater = paths.binary("openpilot-agnos");
    let verify = Command::new(&updater)
        .arg("--verify")
        .arg(&manifest)
        .spawn()?;
    if Server(verify).finish(stopped)?.success() {
        if stopped.load(Ordering::Relaxed) {
            return Ok(());
        }
        if Command::new("sudo").arg("reboot").status()?.success() {
            wait(stopped);
            return Ok(());
        }
        return Err(Error::Configuration(
            "verified AGNOS update is ready, but reboot failed".into(),
        ));
    }
    let log = paths.data.join("agnos-updater-ui.log");
    fs::write(&log, "")?;
    for attempt in 1..=3 {
        if stopped.load(Ordering::Relaxed) {
            return Ok(());
        }
        println!(
            "Starting AGNOS updater UI ({attempt}/3); log {}",
            log.display()
        );
        let output = fs::OpenOptions::new().append(true).open(&log)?;
        let child = Command::new(paths.binary("openpilot-agnos-updater"))
            .arg(&updater)
            .arg(&manifest)
            .current_dir(&paths.repository)
            .stdout(output.try_clone()?)
            .stderr(output)
            .spawn()?;
        let status = Server(child).finish(stopped)?;
        println!("AGNOS updater UI exited: {status}");
        pause(Duration::from_secs(2), stopped);
    }
    Err(Error::Configuration("AGNOS automatic updater stopped three times; inspect agnos-updater-ui.log and use recovery port 6999".into()))
}
