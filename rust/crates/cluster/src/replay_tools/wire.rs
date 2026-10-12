use super::{queue::Latest, Error, Result, ToolCommand, ToolsState};
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, TryRecvError, TrySendError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Packet {
    State { state: Box<ToolsState> },
    Stop,
    Command { value: ToolCommand },
}
pub enum Outgoing {
    Latest(Arc<Latest<Packet>>),
    Commands(Receiver<ToolCommand>),
}
pub enum Incoming {
    Latest(Arc<Latest<Packet>>),
    Commands(SyncSender<ToolCommand>),
}
impl Outgoing {
    fn take(&self) -> Option<Packet> {
        match self {
            Self::Latest(q) => q.take(),
            Self::Commands(q) => match q.try_recv() {
                Ok(value) => Some(Packet::Command { value }),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
            },
        }
    }
}
impl Incoming {
    fn put(&self, packet: Packet) -> Result<()> {
        match (self, packet) {
            (Self::Latest(q), p @ (Packet::State { .. } | Packet::Stop)) => q.replace(p),
            (Self::Commands(q), Packet::Command { value }) => match q.try_send(value) {
                Ok(()) | Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {}
            },
            (Self::Latest(_), Packet::Command { .. })
            | (Self::Commands(_), Packet::State { .. } | Packet::Stop) => {
                return Err(Error::Contract("replay wire direction mismatch"));
            }
        }
        Ok(())
    }
}
pub struct Pump {
    stream: UnixStream,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<()>>>,
}
impl Pump {
    pub fn is_alive(&self) -> bool {
        self.worker.as_ref().is_some_and(|w| !w.is_finished())
    }

    pub fn spawn(stream: UnixStream, outgoing: Outgoing, incoming: Incoming) -> Result<Self> {
        stream.set_nonblocking(true)?;
        let control = stream.try_clone()?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("replay-tools-ipc".into())
            .spawn(move || pump(stream, &worker_stop, &outgoing, &incoming))?;
        Ok(Self {
            stream: control,
            stop,
            worker: Some(worker),
        })
    }
    pub fn close(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::SeqCst);
        match self.stream.shutdown(std::net::Shutdown::Both) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotConnected => {}
            Err(e) => return Err(e.into()),
        }
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| Error::Contract("replay IPC worker panicked"))??;
        }
        Ok(())
    }
}
impl Drop for Pump {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("replay IPC close: {error}");
        }
    }
}
fn encode(mut packet: Packet) -> Result<Vec<u8>> {
    let rgba = match &mut packet {
        Packet::State { state } => state.video_rgba.take().unwrap_or_default(),
        Packet::Stop | Packet::Command { .. } => Vec::new(),
    };
    let metadata = serde_json::to_vec(&packet)?;
    let length = 12_usize
        .checked_add(metadata.len())
        .and_then(|v| v.checked_add(rgba.len()))
        .ok_or(Error::Contract("wire size overflow"))?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(length)
        .map_err(|_| Error::Contract("wire allocation failed"))?;
    output.extend_from_slice(
        &u32::try_from(metadata.len())
            .map_err(|_| Error::Contract("wire metadata too large"))?
            .to_le_bytes(),
    );
    output.extend_from_slice(
        &u64::try_from(rgba.len())
            .map_err(|_| Error::Contract("wire image too large"))?
            .to_le_bytes(),
    );
    output.extend_from_slice(&metadata);
    output.extend_from_slice(&rgba);
    Ok(output)
}
fn decode(buffer: &mut Vec<u8>) -> Result<Option<Packet>> {
    if buffer.len() < 12 {
        return Ok(None);
    }
    let meta = usize::try_from(u32::from_le_bytes(
        buffer[..4]
            .try_into()
            .map_err(|_| Error::Contract("wire header malformed"))?,
    ))
    .map_err(|_| Error::Contract("wire metadata size overflow"))?;
    let image = usize::try_from(u64::from_le_bytes(
        buffer[4..12]
            .try_into()
            .map_err(|_| Error::Contract("wire header malformed"))?,
    ))
    .map_err(|_| Error::Contract("wire image size overflow"))?;
    let end = 12_usize
        .checked_add(meta)
        .and_then(|v| v.checked_add(image))
        .ok_or(Error::Contract("wire length overflow"))?;
    if buffer.len() < end {
        return Ok(None);
    }
    let mut packet: Packet = serde_json::from_slice(&buffer[12..12 + meta])?;
    match &mut packet {
        Packet::State { state } => {
            state.check_times()?;
            state.video_rgba = (image > 0).then(|| buffer[12 + meta..end].to_vec());
        }
        Packet::Stop | Packet::Command { .. } if image == 0 => {}
        Packet::Stop | Packet::Command { .. } => {
            return Err(Error::Contract("unexpected wire image"));
        }
    }
    buffer.drain(..end);
    Ok(Some(packet))
}
fn pump(
    mut stream: UnixStream,
    stop: &AtomicBool,
    outgoing: &Outgoing,
    incoming: &Incoming,
) -> Result<()> {
    let mut received = Vec::new();
    let mut pending = Vec::new();
    let mut sent = 0;
    let mut chunk = [0_u8; 16384];
    while !stop.load(Ordering::SeqCst) {
        let mut progress = false;
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => return Ok(()),
                Ok(count) => {
                    received
                        .try_reserve(count)
                        .map_err(|_| Error::Contract("wire receive allocation failed"))?;
                    received.extend_from_slice(&chunk[..count]);
                    progress = true;
                    while let Some(packet) = decode(&mut received)? {
                        incoming.put(packet)?;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => {
                    return if stop.load(Ordering::SeqCst) {
                        Ok(())
                    } else {
                        Err(error.into())
                    };
                }
            }
        }
        if sent == pending.len() {
            if let Some(packet) = outgoing.take() {
                pending = encode(packet)?;
                sent = 0;
            }
        }
        if sent < pending.len() {
            match stream.write(&pending[sent..]) {
                Ok(0) => return Err(Error::Contract("replay wire write zero")),
                Ok(count) => {
                    sent += count;
                    progress = true;
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => {
                    return if stop.load(Ordering::SeqCst) {
                        Ok(())
                    } else {
                        Err(error.into())
                    };
                }
            }
        }
        if !progress {
            thread::sleep(Duration::from_millis(2));
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rgba_roundtrip_when_fragmented() {
        let state = ToolsState {
            video_rgba: Some(vec![1, 2, 3, 4]),
            video_width: 1,
            video_height: 1,
            ..ToolsState::default()
        };
        let encoded = encode(Packet::State {
            state: Box::new(state),
        })
        .expect("encode");
        let mut buffer = Vec::new();
        for b in &encoded[..encoded.len() - 1] {
            buffer.push(*b);
            assert!(decode(&mut buffer).expect("decode").is_none());
        }
        buffer.push(encoded[encoded.len() - 1]);
        match decode(&mut buffer).expect("decode").expect("packet") {
            Packet::State { state } => assert_eq!(state.video_rgba, Some(vec![1, 2, 3, 4])),
            Packet::Stop | Packet::Command { .. } => panic!("wrong packet"),
        }
    }
}
