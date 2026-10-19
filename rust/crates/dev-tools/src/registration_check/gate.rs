//! Actual subscriber-before-publisher schedule over a private Unix handshake.
//! Snapshots observe the pinned original msgq header ABI at quiescent barriers;
//! no claimed ordering flag is supplied by a canned response.
use super::{process::Interactive, values as v};
use crate::seal::Result;
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::unix::{
        fs::MetadataExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const WORDS: usize = 3 + 3 * 40;
pub(super) struct Gate {
    pub(super) path: PathBuf,
    socket_id: (u64, u64),
    listener: Option<UnixListener>,
    delay: Duration,
}
pub(super) struct Schedule {
    stream: UnixStream,
    delay: Duration,
    started: Instant,
    subscribed: Option<Instant>,
    before: Vec<u64>,
    after: Vec<u64>,
    released: Option<Instant>,
    acknowledged: Option<Instant>,
}
impl Gate {
    pub(super) fn from_environment(root: &Path) -> Result<Option<Self>> {
        let Some(milliseconds) = std::env::var_os("REGISTRATION_COLLECTOR_DELAY_MS") else {
            return Ok(None);
        };
        let delay = Duration::from_millis(
            milliseconds
                .to_str()
                .ok_or("collector delay must be an integer")?
                .parse()?,
        );
        let path = root.join("publisher.sock");
        let listener = UnixListener::bind(&path)?;
        listener.set_nonblocking(true)?;
        let socket = fs::symlink_metadata(&path)?;
        Ok(Some(Self {
            path,
            socket_id: (socket.dev(), socket.ino()),
            listener: Some(listener),
            delay,
        }))
    }
    pub(super) fn accept(&self, collector: &Interactive, deadline: Instant) -> Result<Schedule> {
        let mut stream = loop {
            match self
                .listener
                .as_ref()
                .ok_or("gate listener closed")?
                .accept()
            {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if collector.exited()? {
                        return Err("source collector exited before gate R".into());
                    }
                    if Instant::now() >= deadline {
                        return Err("source collector gate R timeout".into());
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(e) => return Err(e.into()),
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;
        let mut ready = [0];
        stream.read_exact(&mut ready)?;
        if ready != *b"R" {
            return Err("source collector subscriber gate malformed R".into());
        }
        Ok(Schedule {
            stream,
            delay: self.delay,
            started: Instant::now(),
            subscribed: None,
            before: Vec::new(),
            after: Vec::new(),
            released: None,
            acknowledged: None,
        })
    }
}
impl Drop for Gate {
    fn drop(&mut self) {
        self.listener.take();
        if fs::symlink_metadata(&self.path).is_ok_and(|m| (m.dev(), m.ino()) == self.socket_id) {
            let _ = fs::remove_file(&self.path);
        }
    }
}
fn header(path: &Path) -> Result<Vec<u64>> {
    let mut raw = vec![0; WORDS * 8];
    File::open(path)?.read_exact(&mut raw)?;
    Ok(raw
        .chunks_exact(8)
        .map(|bytes| u64::from_ne_bytes(bytes.try_into().expect("eight-byte header word")))
        .collect())
}
impl Schedule {
    pub(super) fn release(&mut self, error_queue: &Path) -> Result<()> {
        self.before = header(error_queue)?;
        self.subscribed = Some(Instant::now());
        if self.before[0] < 2 || self.before[2] != 0 {
            return Err(
                "gate did not observe dummy and real subscribers before publisher initialization"
                    .into(),
            );
        }
        std::thread::sleep(self.delay);
        self.released = Some(Instant::now());
        self.stream.write_all(b"1")?;
        let mut ack = [0];
        self.stream.read_exact(&mut ack)?;
        if ack != *b"1" {
            return Err("source collector publisher gate malformed ACK".into());
        }
        self.acknowledged = Some(Instant::now());
        self.after = header(error_queue)?;
        if self.after[0] != 0 || self.after[2] == 0 || self.after[83..].iter().any(|v| *v != 0) {
            return Err("actual error publisher did not invalidate registered readers".into());
        }
        Ok(())
    }
    pub(super) fn refreshed(&self, error_queue: &Path, output: &Path) -> Result<()> {
        let refreshed = header(error_queue)?;
        if refreshed[0] == 0
            || refreshed[2] != self.after[2]
            || refreshed[83..].iter().all(|v| *v == 0)
        {
            return Err("actual error reader did not refresh after publisher reset".into());
        }
        let at = |instant: Option<Instant>| -> Result<f64> {
            Ok(instant
                .ok_or("missing actual gate event")?
                .duration_since(self.started)
                .as_secs_f64())
        };
        v::write(
            output,
            &v::value(
                serde_json::json!({"requested_delay_ms":self.delay.as_millis(),"subscriber_registered_seconds":at(self.subscribed)?,"release_seconds":at(self.released)?,"publisher_ack_seconds":at(self.acknowledged)?,"reader_refresh_seconds":self.started.elapsed().as_secs_f64(),"before_publisher_header":self.before,"after_publisher_header":self.after,"after_reader_refresh_header":refreshed}),
            )?,
        )
    }
}
