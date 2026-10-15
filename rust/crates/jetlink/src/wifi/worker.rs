use super::{collect_with, validate, LIMIT};
use crate::Error;
use rustix::event::{poll, PollFd, PollFlags, Timespec};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn query(args: &[&str]) -> Result<String, Error> {
    let failed = || Error::Contract("NetworkManager private query unavailable");
    let mut child = Command::new("nmcli")
        .args(["--escape", "no"])
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| failed())?;
    let stdout = child.stdout.take().ok_or_else(failed)?;
    // Bound secret-bearing memory and drain concurrently so a full pipe cannot bypass timeout.
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.take(65537).read_to_end(&mut bytes).map(|_| bytes)
    });
    let started = Instant::now();
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if started.elapsed() < Duration::from_secs(2) => {
                std::thread::sleep(Duration::from_millis(10))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let bytes = reader.join().map_err(|_| failed())?.map_err(|_| failed())?;
    if !success || bytes.len() > 65536 {
        return Err(failed());
    }
    String::from_utf8(bytes)
        .map(|value| value.trim_end_matches('\n').to_owned())
        .map_err(|_| failed())
}

/// Private helper entry point. Its inherited stdin is the sole provisioning channel.
pub fn run() -> Result<(), Error> {
    let params = openpilot_params::Params::for_runtime()
        .map_err(|_| Error::Contract("private Params unavailable"))?;
    let owner = format!("{:x}", Sha256::digest(std::fs::read("/etc/machine-id")?));
    let release: serde_json::Value =
        serde_json::from_slice(include_bytes!("../../data/host_release.json"))?;
    let channel = std::io::stdin();
    let mut profiles = None;
    let mut refresh = 0;
    loop {
        let now = crate::now_ns();
        if now >= refresh {
            profiles = collect_with(query).ok();
            refresh = now.saturating_add(if profiles.is_some() {
                30_000_000_000
            } else {
                5_000_000_000
            });
        }
        if let Some(profiles) = &profiles {
            let onroad =
                params
                    .get("IsOnroad")
                    .ok()
                    .flatten()
                    .and_then(|value| match value.as_slice() {
                        b"0" => Some(false),
                        b"1" => Some(true),
                        _ => None,
                    });
            let value = json!({"version":1,"owner":owner,"profiles":profiles,"onroad":onroad,"jetson_release":release});
            validate(&value)?;
            let payload = serde_json::to_vec(&value)?;
            if payload.len() <= LIMIT {
                let mut packet = (crate::now_ns() as f64 / 1e9).to_le_bytes().to_vec();
                packet.extend_from_slice(&payload);
                match rustix::net::send(
                    &channel,
                    &packet,
                    rustix::net::SendFlags::DONTWAIT | rustix::net::SendFlags::NOSIGNAL,
                ) {
                    Ok(_) | Err(rustix::io::Errno::AGAIN) => {}
                    Err(_) => return Ok(()),
                }
            }
        }
        let mut fd = [PollFd::new(&channel, PollFlags::HUP | PollFlags::ERR)];
        match poll(
            &mut fd,
            Some(&Timespec {
                tv_sec: 2,
                tv_nsec: 0,
            }),
        ) {
            Ok(_) if fd[0].revents().intersects(PollFlags::HUP | PollFlags::ERR) => return Ok(()),
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(std::io::Error::from(error).into()),
            _ => {}
        }
    }
}
