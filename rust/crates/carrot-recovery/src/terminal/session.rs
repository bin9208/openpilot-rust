use crate::Error;
use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, WaitOptions};
use std::{
    io,
    time::{Duration, Instant},
};

fn members(leader: Pid) -> Result<Vec<Pid>, Error> {
    let session = leader.as_raw_nonzero().get();
    let mut members = Vec::new();
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<i32>().ok())
            .and_then(Pid::from_raw)
        else {
            continue;
        };
        if pid == leader {
            continue;
        }
        let stat = match std::fs::read_to_string(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
                ) =>
            {
                continue
            }
            Err(error) => return Err(error.into()),
        };
        let sid = stat
            .rsplit_once(')')
            .and_then(|(_, fields)| fields.split_whitespace().nth(3))
            .and_then(|value| value.parse::<i32>().ok());
        if sid == Some(session) {
            members.push(pid);
        }
    }
    Ok(members)
}

pub(crate) fn cleanup(leader: Pid) -> Result<(), Error> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let children = members(leader)?;
        if children.is_empty() {
            return Ok(());
        }
        for pid in children {
            // Only adopted direct children are signalled. WNOWAIT reserves an
            // exited child's identity; live children stay ours until reaped.
            let status = match rustix::process::waitid(
                WaitId::Pid(pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(status) => status,
                Err(rustix::io::Errno::CHILD | rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(io::Error::from(error).into()),
            };
            let stat = match std::fs::read_to_string(format!("/proc/{}/stat", pid.as_raw_nonzero()))
            {
                Ok(stat) => stat,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            let sid = stat
                .rsplit_once(')')
                .and_then(|(_, fields)| fields.split_whitespace().nth(3))
                .and_then(|value| value.parse::<i32>().ok());
            if sid != Some(leader.as_raw_nonzero().get()) {
                continue;
            }
            if status.is_none() {
                match rustix::process::kill_process(pid, Signal::KILL) {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                    Err(error) => return Err(io::Error::from(error).into()),
                }
            } else {
                match rustix::process::waitpid(Some(pid), WaitOptions::NOHANG) {
                    Ok(_) | Err(rustix::io::Errno::INTR) => {}
                    Err(error) => return Err(io::Error::from(error).into()),
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(Error::Source(
                "PTY session descendants did not terminate within two seconds".into(),
            ));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
