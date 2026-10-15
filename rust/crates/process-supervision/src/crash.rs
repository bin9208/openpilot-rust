//! Nonblocking local crash notification; no files, Params, tmux or network here.
use openpilot_logging::{Fields, Value};
use std::{
    collections::VecDeque,
    io,
    os::unix::net::UnixDatagram,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

pub const SOCKET_PATH: &str = "/dev/shm/carrot-process-crash.sock";
pub const MAX_PENDING: usize = 32;
pub struct Sender {
    pub pending: VecDeque<Vec<u8>>,
    address: PathBuf,
}
impl Sender {
    pub fn new(address: PathBuf) -> Self {
        Self {
            pending: VecDeque::new(),
            address,
        }
    }
    pub fn report(&mut self, event: Fields) -> io::Result<()> {
        let code = match event.get("exitcode") {
            Some(Value::Integer(code)) => *code,
            _ => return Ok(()),
        };
        let pid = match event.get("pid") {
            Some(Value::Integer(pid)) => *pid,
            _ => return Ok(()),
        };
        if pid == 0 || code == 0 {
            return Ok(());
        }
        let bytes = event.to_json().map_err(io::Error::other)?.into_bytes();
        if self.pending.len() == MAX_PENDING {
            self.pending.pop_front();
        }
        self.pending.push_back(bytes);
        self.flush()
    }
    pub fn flush(&mut self) -> io::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let socket = UnixDatagram::unbound()?;
        socket.set_nonblocking(true)?;
        while let Some(packet) = self.pending.front() {
            socket.send_to(packet, &self.address)?;
            self.pending.pop_front();
        }
        Ok(())
    }
}
fn sender() -> &'static Mutex<Sender> {
    static INSTANCE: OnceLock<Mutex<Sender>> = OnceLock::new();
    INSTANCE.get_or_init(|| Mutex::new(Sender::new(SOCKET_PATH.into())))
}
pub fn flush() {
    if let Ok(mut sender) = sender().try_lock() {
        let _ = sender.flush();
    }
}
pub fn report(name: &str, pid: u32, exitcode: Option<i32>) {
    let Some(exitcode) = exitcode.filter(|code| *code != 0) else {
        return;
    };
    if pid == 0 {
        return;
    }
    let Ok(mut sender) = sender().try_lock() else {
        return;
    };
    let wall = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(v) => v.as_secs_f64(),
        Err(_) => return,
    };
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    let monotonic = i128::from(time.tv_sec) * 1_000_000_000 + i128::from(time.tv_nsec);
    let manager = std::process::id();
    let mut event: Fields = [
        (
            "id".into(),
            Value::Text(format!("{manager}-{pid}-{monotonic}")),
        ),
        ("name".into(), Value::Text(name.into())),
        ("pid".into(), Value::Integer(i128::from(pid))),
        ("exitcode".into(), Value::Integer(i128::from(exitcode))),
        ("time".into(), Value::Float(wall)),
        ("manager_pid".into(), Value::Integer(i128::from(manager))),
    ]
    .into_iter()
    .collect();
    for (key, var) in [
        ("git_branch", "GIT_BRANCH"),
        ("git_commit", "GIT_COMMIT"),
        ("dongle_id", "DONGLE_ID"),
        ("pane", "TMUX_PANE"),
    ] {
        event.insert(
            key.into(),
            Value::Text(std::env::var(var).unwrap_or_default()),
        );
    }
    let _ = sender.report(event);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_datagram_retry_is_bounded_and_keeps_latest_thirty_two() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("socket");
        let mut sender = Sender::new(path.clone());
        let event = |pid: i128, code: i128| {
            [
                ("pid".into(), Value::Integer(pid)),
                ("exitcode".into(), Value::Integer(code)),
            ]
            .into_iter()
            .collect()
        };
        sender.report(event(1, 0)).unwrap();
        sender.report(event(0, -6)).unwrap();
        assert!(sender.pending.is_empty());
        for pid in 1..50 {
            assert!(sender.report(event(pid, -6)).is_err());
        }
        assert_eq!(sender.pending.len(), 32);
        let receiver = UnixDatagram::bind(path).unwrap();
        receiver.set_nonblocking(true).unwrap();
        let mut pids = Vec::new();
        for _ in 0..32 {
            let _ = sender.flush();
            loop {
                let mut data = [0; 8192];
                match receiver.recv(&mut data) {
                    Ok(n) => {
                        let value: serde_json::Value = serde_json::from_slice(&data[..n]).unwrap();
                        pids.push(value["pid"].as_i64().unwrap());
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) => panic!("{e}"),
                }
            }
            if sender.pending.is_empty() {
                break;
            }
        }
        assert!(sender.pending.is_empty());
        assert_eq!(pids, (18..50).collect::<Vec<_>>());
    }
}
