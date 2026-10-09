use super::rtmp::Client;
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

pub struct Sink {
    client: Arc<Client>,
    pending: Vec<u8>,
    pub bytes_accepted: u64,
    pub drain_calls: u64,
    pub partial_writes: u64,
    observed: Arc<Mutex<Snapshot>>,
}
#[derive(Clone, Default)]
pub struct Snapshot {
    pub bytes_accepted: u64,
    pub pending_bytes: usize,
    pub drain_calls: u64,
    pub partial_writes: u64,
}
impl Sink {
    pub fn new(client: Arc<Client>) -> Self {
        Self::observed(client, Arc::new(Mutex::new(Snapshot::default())))
    }
    pub(super) fn observed(client: Arc<Client>, observed: Arc<Mutex<Snapshot>>) -> Self {
        Self {
            client,
            pending: Vec::new(),
            bytes_accepted: 0,
            drain_calls: 0,
            partial_writes: 0,
            observed,
        }
    }
    pub fn pending_bytes(&self) -> usize {
        self.pending.len()
    }
    fn publish(&self) -> io::Result<()> {
        let mut snapshot = self
            .observed
            .lock()
            .map_err(|_| io::Error::other("RTMP sink snapshot poisoned"))?;
        *snapshot = Snapshot {
            bytes_accepted: self.bytes_accepted,
            pending_bytes: self.pending.len(),
            drain_calls: self.drain_calls,
            partial_writes: self.partial_writes,
        };
        Ok(())
    }
    fn drain(&mut self, floor: usize) -> io::Result<()> {
        while !self.pending.is_empty() && self.pending.len() >= floor {
            let before = self.pending.len();
            let written = self.client.write(&self.pending).map_err(io::Error::other)?;
            if written == 0 {
                break;
            }
            self.drain_calls = self.drain_calls.saturating_add(1);
            if written < before {
                self.partial_writes = self.partial_writes.saturating_add(1);
            }
            self.pending.drain(..written);
            self.publish()?;
        }
        Ok(())
    }
}
impl Write for Sink {
    fn write(&mut self, payload: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(payload);
        self.bytes_accepted = self
            .bytes_accepted
            .saturating_add(u64::try_from(payload.len()).unwrap_or(u64::MAX));
        self.publish()?;
        self.drain(16 * 1024)?;
        Ok(payload.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.drain(1)
    }
}
