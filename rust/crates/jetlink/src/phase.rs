//! Best-effort C4 DM alignment to completion of the current external upload.
use std::{
    io,
    os::{
        linux::net::SocketAddrExt,
        unix::net::{SocketAddr, UnixDatagram},
    },
    time::{Duration, Instant},
};
const ADDRESS: &[u8] = b"carrot-jetlink-dm-phase";
pub const FRESH_NS: u64 = 75_000_000;
pub const CAMERA_SKEW_NS: u64 = 5_000_000;
pub const WAIT: Duration = Duration::from_millis(25);
const MAX_DRAIN: usize = 16;

pub struct Publisher {
    socket: Option<UnixDatagram>,
    address: Option<SocketAddr>,
}
impl Publisher {
    pub fn device() -> Self {
        Self::new(ADDRESS)
    }
    pub fn new(name: &[u8]) -> Self {
        let address = SocketAddr::from_abstract_name(name).ok();
        let socket = UnixDatagram::unbound()
            .and_then(|socket| {
                socket.set_nonblocking(true)?;
                Ok(socket)
            })
            .ok();
        Self { socket, address }
    }
    pub fn sent(&self, source_sof: u64) {
        if source_sof == 0 {
            return;
        }
        let mut packet = [0; 16];
        packet[..8].copy_from_slice(&source_sof.to_le_bytes());
        packet[8..].copy_from_slice(&crate::now_ns().to_le_bytes());
        if let (Some(socket), Some(address)) = (&self.socket, &self.address) {
            let _ = socket.send_to_addr(&packet, address);
        }
    }
}
#[derive(Default, Debug)]
pub struct State {
    pub source_sof: u64,
    pub sent_at: u64,
}
impl State {
    pub fn accept(&mut self, packet: &[u8], now: u64) {
        let Ok(packet): Result<&[u8; 16], _> = packet.try_into() else {
            return;
        };
        let source_sof = u64::from_le_bytes(packet[..8].try_into().expect("fixed SOF bytes"));
        let sent_at = u64::from_le_bytes(packet[8..].try_into().expect("fixed timestamp bytes"));
        if source_sof > 0
            && source_sof <= sent_at
            && sent_at <= now
            && now - sent_at < FRESH_NS
            && sent_at > self.sent_at
        {
            self.source_sof = source_sof;
            self.sent_at = sent_at;
        }
    }
    pub fn fresh(&self, now: u64) -> bool {
        now.checked_sub(self.sent_at)
            .is_some_and(|age| age < FRESH_NS)
    }
    pub fn behind(&self, source_sof: u64) -> bool {
        self.source_sof < source_sof.saturating_sub(CAMERA_SKEW_NS)
    }
}
pub struct Gate {
    socket: Option<UnixDatagram>,
    pub state: State,
}
impl Gate {
    pub fn device() -> Self {
        Self::bind(ADDRESS)
    }
    pub fn bind(name: &[u8]) -> Self {
        let socket = SocketAddr::from_abstract_name(name)
            .and_then(|address| UnixDatagram::bind_addr(&address))
            .ok();
        Self {
            socket,
            state: State::default(),
        }
    }
    pub fn wait(&mut self, source_sof: u64) {
        if source_sof == 0 || self.socket.is_none() {
            return;
        }
        if self.wait_inner(source_sof).is_err() {
            self.socket = None;
        }
    }
    fn wait_inner(&mut self, source_sof: u64) -> io::Result<()> {
        let Some(socket) = &self.socket else {
            return Ok(());
        };
        let started = Instant::now();
        let mut packet = [0; 17];
        socket.set_nonblocking(true)?;
        for _ in 0..MAX_DRAIN {
            match socket.recv(&mut packet) {
                Ok(length) => self.state.accept(&packet[..length], crate::now_ns()),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            }
        }
        if !self.state.fresh(crate::now_ns()) {
            return Ok(());
        }
        socket.set_nonblocking(false)?;
        while self.state.behind(source_sof) {
            let Some(remaining) = WAIT
                .checked_sub(started.elapsed())
                .filter(|value| !value.is_zero())
            else {
                break;
            };
            socket.set_read_timeout(Some(remaining))?;
            match socket.recv(&mut packet) {
                Ok(length) => self.state.accept(&packet[..length], crate::now_ns()),
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    break
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}
