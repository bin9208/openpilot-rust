//! Same-session cleanup uses only direct children reserved by WNOWAIT.
use openpilot_process_supervision::CapturedChild;
use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, WaitOptions};
use std::{
    io,
    time::{Duration, Instant},
};

pub struct Owned {
    pub child: CapturedChild,
    pub pid: Pid,
    pub closed: bool,
    pub cleaned: bool,
    pub deadline: Option<Instant>,
}

pub fn signal(pid: Pid, signal: Signal) -> io::Result<()> {
    match rustix::process::kill_process_group(pid, signal) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[derive(Clone, Copy)]
struct Member {
    pid: Pid,
    starttime: u64,
}
#[derive(Clone, Copy)]
struct Identity {
    session: i32,
    starttime: u64,
}

fn read_identity(pid: Pid) -> io::Result<Option<Identity>> {
    let stat = match std::fs::read(format!("/proc/{}/stat", pid.as_raw_nonzero())) {
        Ok(stat) => stat,
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.raw_os_error() == Some(rustix::io::Errno::SRCH.raw_os_error()) =>
        {
            return Ok(None)
        }
        Err(error) => return Err(error),
    };
    // comm may contain ')' or arbitrary non-UTF-8 bytes. Only the structural
    // numeric fields after the final ')' need ASCII/UTF-8 interpretation.
    let fields: Vec<_> = stat
        .iter()
        .rposition(|byte| *byte == b')')
        .map(|end| {
            stat[end + 1..]
                .split(u8::is_ascii_whitespace)
                .filter(|field| !field.is_empty())
                .collect()
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "malformed proc stat"))?;
    let number = |index: usize| {
        fields
            .get(index)
            .and_then(|field| std::str::from_utf8(field).ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing proc stat field"))
    };
    let session = number(3)?
        .parse()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let starttime = number(19)?
        .parse()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(Some(Identity { session, starttime }))
}

fn members(leader: Pid) -> io::Result<Vec<Member>> {
    let mut members = Vec::new();
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse().ok())
            .and_then(Pid::from_raw)
        else {
            continue;
        };
        if pid == leader {
            continue;
        }
        let identity = match read_identity(pid) {
            Ok(Some(identity)) => identity,
            Ok(None) => continue,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => continue,
            Err(error) => return Err(error),
        };
        if identity.session == leader.as_raw_nonzero().get() {
            members.push(Member {
                pid,
                starttime: identity.starttime,
            });
        }
    }
    Ok(members)
}

fn cleanup_member(leader: Pid, member: Member) -> io::Result<()> {
    // Establish direct-child ownership without releasing the identity. This
    // helper has no concurrent reaper. A census alone does not authorize a PID.
    let status = match rustix::process::waitid(
        WaitId::Pid(member.pid),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    ) {
        Ok(status) => status,
        Err(rustix::io::Errno::CHILD | rustix::io::Errno::INTR) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    // The censused process may have detached (setsid), or the numeric PID may
    // have been reused before it became our child. Re-read AFTER ownership.
    let Some(identity) = read_identity(member.pid)? else {
        return Ok(());
    };
    if identity.session != leader.as_raw_nonzero().get() || identity.starttime != member.starttime {
        return Ok(());
    }
    if status.is_none() {
        match rustix::process::kill_process(member.pid, Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(error.into()),
        }
    } else {
        match rustix::process::waitpid(Some(member.pid), WaitOptions::NOHANG) {
            Ok(_) | Err(rustix::io::Errno::INTR) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

impl Owned {
    pub fn terminate(&mut self) -> io::Result<()> {
        if self.cleaned {
            return Ok(());
        }
        let deadline = *self
            .deadline
            .get_or_insert_with(|| Instant::now() + Duration::from_secs(2));
        for requested in [Signal::HUP, Signal::KILL] {
            signal(self.pid, requested)?;
        }
        // Finish leader exit before the first census: it must no longer be
        // able to fork after an apparently empty /proc scan. Keep it unreaped.
        loop {
            match rustix::process::waitid(
                WaitId::Pid(self.pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(Some(_)) => break,
                Ok(None) | Err(rustix::io::Errno::INTR) => {
                    if Instant::now() >= deadline {
                        return Err(cleanup_timeout());
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(error) => return Err(error.into()),
            }
        }
        loop {
            if Instant::now() >= deadline {
                return Err(cleanup_timeout());
            }
            let children = members(self.pid)?;
            if children.is_empty() {
                break;
            }
            for member in children {
                if Instant::now() >= deadline {
                    return Err(cleanup_timeout());
                }
                cleanup_member(self.pid, member)?;
            }
            if Instant::now() >= deadline {
                return Err(cleanup_timeout());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        self.cleaned = true;
        Ok(())
    }
    pub fn close(&mut self) -> io::Result<()> {
        if !self.closed {
            self.terminate()?;
            self.child.process.wait()?;
            self.closed = true;
        }
        Ok(())
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn cleanup_timeout() -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        "PTY session descendants did not terminate within two seconds",
    )
}

#[cfg(test)]
#[path = "supervisor_session_tests.rs"]
mod tests;
