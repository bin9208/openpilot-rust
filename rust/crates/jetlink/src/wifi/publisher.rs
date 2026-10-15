use super::LIMIT;
use crate::Error;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fd::OwnedFd,
    net::{socketpair, AddressFamily, RecvFlags, SocketFlags, SocketType},
};
use std::os::unix::process::CommandExt;
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
pub struct Publisher {
    channel: OwnedFd,
    child: Child,
}
impl Publisher {
    pub fn spawn() -> Result<Self, Error> {
        let (channel, child) = socketpair(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::CLOEXEC,
            None,
        )
        .map_err(std::io::Error::from)?;
        let child = Command::new("chrt")
            .args(["--other", "0"])
            .arg(std::env::current_exe()?)
            .arg("--wifi-publisher")
            .process_group(0)
            .stdin(Stdio::from(child))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Self { channel, child })
    }
    pub fn latest(&self, initial: bool) -> Result<Option<Vec<u8>>, Error> {
        if initial {
            let _ = poll(
                &mut [PollFd::new(&self.channel, PollFlags::IN)],
                Some(&Timespec {
                    tv_sec: 4,
                    tv_nsec: 0,
                }),
            );
        }
        let mut latest = None;
        for _ in 0..4 {
            let mut bytes = vec![0; LIMIT + 9];
            match rustix::net::recv(&self.channel, &mut bytes[..], RecvFlags::DONTWAIT) {
                Ok((count, _)) if (9..=LIMIT + 8).contains(&count) => {
                    bytes.truncate(count);
                    latest = Some(bytes);
                }
                Ok(_) => {}
                Err(rustix::io::Errno::AGAIN) => break,
                Err(error) => return Err(std::io::Error::from(error).into()),
            }
        }
        Ok(latest
            .filter(|bytes| {
                let stamp = f64::from_le_bytes(bytes[..8].try_into().unwrap());
                (0.0..3.0).contains(&(crate::now_ns() as f64 / 1e9 - stamp))
            })
            .map(|bytes| bytes[8..].to_vec()))
    }
}
impl Drop for Publisher {
    fn drop(&mut self) {
        let Some(pid) = i32::try_from(self.child.id())
            .ok()
            .and_then(rustix::process::Pid::from_raw)
        else {
            return;
        };
        // Keep the leader unreaped through the final group signal, reserving its PGID.
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::TERM);
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(1) {
            let status = rustix::process::waitid(
                rustix::process::WaitId::Pid(pid),
                rustix::process::WaitIdOptions::EXITED
                    | rustix::process::WaitIdOptions::NOHANG
                    | rustix::process::WaitIdOptions::NOWAIT,
            );
            if matches!(status, Ok(Some(_))) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustix::net::SendFlags;
    #[test]
    fn private_channel_drains_four_only_and_rejects_stale_future_and_oversize_packets() {
        let (channel, sender) = socketpair(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::CLOEXEC,
            None,
        )
        .unwrap();
        let publisher = Publisher {
            channel,
            child: Command::new("sleep")
                .arg("30")
                .process_group(0)
                .spawn()
                .unwrap(),
        };
        let send = |stamp: f64, payload: &[u8]| {
            let mut bytes = stamp.to_le_bytes().to_vec();
            bytes.extend_from_slice(payload);
            rustix::net::send(&sender, &bytes, SendFlags::empty()).unwrap();
        };
        let now = crate::now_ns() as f64 / 1e9;
        for index in 0..5 {
            send(now, &[index]);
        }
        assert_eq!(publisher.latest(false).unwrap(), Some(vec![3]));
        assert_eq!(publisher.latest(false).unwrap(), Some(vec![4]));
        assert!(publisher.latest(false).unwrap().is_none());
        for stamp in [now - 3.1, now + 10.0, f64::NAN] {
            send(stamp, b"fixture");
            assert!(publisher.latest(false).unwrap().is_none());
        }
        send(now, &vec![0; LIMIT + 1]);
        assert!(publisher.latest(false).unwrap().is_none());
    }
}
