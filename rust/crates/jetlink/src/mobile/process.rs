//! DNS ownership requires executable, exact scoped arguments and process start.
use super::platform::{remove, text, Paths};
use crate::Error;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    pid: u32,
    start: String,
}
#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    start: String,
    args: Vec<Vec<u8>>,
    zombie: bool,
}
fn snapshot(proc: &Path) -> Result<Snapshot, Error> {
    let stat = fs::read_to_string(proc.join("stat"))?;
    let fields = stat
        .rsplit_once(") ")
        .ok_or(Error::Contract("DHCP process stat format"))?
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    let start = fields
        .get(19)
        .ok_or(Error::Contract("DHCP process start time missing"))?
        .to_string();
    if start.parse::<u64>().is_err() {
        return Err(Error::Contract("DHCP start time invalid"));
    }
    Ok(Snapshot {
        start,
        args: fs::read(proc.join("cmdline"))?
            .split(|b| *b == 0)
            .map(<[u8]>::to_vec)
            .collect(),
        zombie: fields.first() == Some(&"Z"),
    })
}
fn identity(paths: &Paths, pid: u32) -> Result<Snapshot, Error> {
    let proc = paths.proc.join(pid.to_string());
    if fs::read_link(proc.join("exe"))?.file_name() != Some(std::ffi::OsStr::new("dnsmasq")) {
        return Err(Error::Contract("PID does not belong to helper dnsmasq"));
    }
    let value = snapshot(&proc)?;
    for required in [
        format!("--pid-file={}", paths.state.join("dnsmasq.pid").display()),
        "--port=0".into(),
        "--conf-file=/dev/null".into(),
        format!(
            "--dhcp-leasefile={}",
            paths.leases.join("dnsmasq.leases").display()
        ),
    ] {
        if !value.args.iter().any(|arg| arg == required.as_bytes()) {
            return Err(Error::Contract("dnsmasq arguments do not belong to helper"));
        }
    }
    Ok(value)
}
fn owner(paths: &Paths) -> Result<Option<Owner>, Error> {
    let pidfile = paths.state.join("dnsmasq.pid");
    let record = paths.state.join("dnsmasq.owner");
    if !pidfile.try_exists()? && !record.try_exists()? {
        return Ok(None);
    }
    let pid = if pidfile.try_exists()? {
        text(&pidfile)?
            .parse()
            .map_err(|_| Error::Contract("invalid DHCP PID"))?
    } else {
        serde_json::from_slice::<Owner>(&fs::read(&record)?)?.pid
    };
    if pid <= 1 {
        return Err(Error::Contract("invalid DHCP PID"));
    }
    if !paths.proc.join(pid.to_string()).try_exists()? {
        remove(&pidfile)?;
        remove(&record)?;
        return Ok(None);
    }
    let current = identity(paths, pid)?;
    Ok(Some(Owner {
        pid,
        start: current.start,
    }))
}
pub(crate) fn record(paths: &Paths) -> Result<(), Error> {
    let owner = owner(paths)?.ok_or(Error::Contract(
        "DHCP process disappeared before ownership record",
    ))?;
    fs::write(
        paths.state.join("dnsmasq.owner"),
        serde_json::to_vec(&owner)?,
    )?;
    Ok(())
}
pub(crate) fn alive(paths: &Paths) -> Result<bool, Error> {
    let Some(current) = owner(paths)? else {
        return Ok(false);
    };
    let saved: Owner = serde_json::from_slice(&fs::read(paths.state.join("dnsmasq.owner"))?)?;
    if saved != current {
        return Err(Error::Contract("DHCP PID reused or not helper-owned"));
    }
    Ok(true)
}
pub(crate) fn stop(paths: &Paths) -> Result<(), Error> {
    let Some(current) = owner(paths)? else {
        return Ok(());
    };
    let saved: Owner = serde_json::from_slice(&fs::read(paths.state.join("dnsmasq.owner"))?)?;
    if current != saved {
        return Err(Error::Contract(
            "refusing DHCP PID reuse/ownership mismatch",
        ));
    }
    let proc = paths.proc.join(current.pid.to_string());
    let before = identity(paths, current.pid)?;
    let pid = rustix::process::Pid::from_raw(
        i32::try_from(current.pid).map_err(|_| Error::Contract("DHCP PID range"))?,
    )
    .ok_or(Error::Contract("DHCP PID range"))?;
    let fd = match rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty()) {
        Ok(fd) => Some(fd),
        Err(rustix::io::Errno::NOSYS | rustix::io::Errno::INVAL) => None,
        Err(error) => return Err(std::io::Error::from(error).into()),
    };
    // Revalidate after pidfd acquisition and immediately before a legacy PID
    // signal. AGNOS4.9 lacks pidfds; never fall back to name-based process kill.
    if identity(paths, current.pid)? != before || before.start != saved.start {
        return Err(Error::Contract("DHCP PID changed before signaling"));
    }
    if let Some(fd) = fd {
        rustix::process::pidfd_send_signal(fd, rustix::process::Signal::TERM)
            .map_err(std::io::Error::from)?;
    } else {
        rustix::process::kill_process(pid, rustix::process::Signal::TERM)
            .map_err(std::io::Error::from)?;
    }
    let until = Instant::now() + Duration::from_secs(2);
    while proc.try_exists()? {
        let state = match snapshot(&proc) {
            Ok(state) => state,
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error),
        };
        if state.start != before.start || state.zombie {
            break;
        }
        if Instant::now() >= until {
            return Err(Error::Contract(
                "helper DHCP did not stop within two seconds",
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    remove(&paths.state.join("dnsmasq.pid"))?;
    remove(&paths.state.join("dnsmasq.owner"))?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    #[test]
    fn ownership_never_comes_from_pidfile_alone_or_another_process() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            gadget: dir.path().join("gadget"),
            net: dir.path().join("net"),
            state: dir.path().join("state"),
            leases: dir.path().join("leases"),
            proc: dir.path().join("proc"),
        };
        fs::create_dir_all(&paths.state).unwrap();
        let proc = paths.proc.join("7890");
        fs::create_dir_all(&proc).unwrap();
        symlink("/usr/sbin/dnsmasq", proc.join("exe")).unwrap();
        let mut fields = vec!["0"; 20];
        fields[0] = "S";
        fields[19] = "123456";
        fs::write(
            proc.join("stat"),
            format!("7890 (dnsmasq) {}", fields.join(" ")),
        )
        .unwrap();
        let arguments = format!(
            "dnsmasq\0--pid-file={}\0--port=0\0--conf-file=/dev/null\0--dhcp-leasefile={}\0",
            paths.state.join("dnsmasq.pid").display(),
            paths.leases.join("dnsmasq.leases").display()
        );
        fs::write(proc.join("cmdline"), arguments).unwrap();
        fs::write(paths.state.join("dnsmasq.pid"), "7890").unwrap();
        assert!(alive(&paths).is_err());
        record(&paths).unwrap();
        assert!(alive(&paths).unwrap());
        fields[19] = "123457";
        fs::write(
            proc.join("stat"),
            format!("7890 (dnsmasq) {}", fields.join(" ")),
        )
        .unwrap();
        assert!(alive(&paths).is_err());
        assert!(stop(&paths).is_err());
        fs::remove_dir_all(&proc).unwrap();
        assert!(!alive(&paths).unwrap());
        assert!(!paths.state.join("dnsmasq.pid").exists());
    }
}
