use crate::{
    logging,
    policy::{proxy_port, SSH_TOS},
    state::{Shared, Stop},
    websocket::{self, Connection},
    Error,
};
use openpilot_logging::producer::Logger;
use socket2::SockRef;
use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::Arc,
    time::Duration,
};
use tungstenite::Message;

pub fn start(
    shared: Arc<Shared>,
    stop: Stop,
    uri: &str,
    port: i64,
    logger: &mut Logger,
) -> Result<(), Error> {
    let port = proxy_port(port)?;
    let ws = Connection::connect(uri, &websocket::token(&shared, logger)?)?;
    SockRef::from(&ws.tcp).set_tos_v4(SSH_TOS)?;
    let local = TcpStream::connect(("127.0.0.1", port))?;
    local.set_nonblocking(true)?;
    let factory = shared.factory.clone();
    let handle = std::thread::Builder::new()
        .name("athena-proxy".into())
        .spawn(move || {
            if let Err(error) = bridge(ws, local, &stop) {
                logging::failure(&mut factory.logger(), "athenad.ws_proxy.exception", &error);
            }
        })?;
    let mut handles = shared
        .proxies
        .lock()
        .map_err(|_| Error::Contract("proxy handles poisoned"))?;
    for index in (0..handles.len()).rev() {
        if handles[index].is_finished() && handles.remove(index).join().is_err() {
            return Err(Error::Contract("proxy worker panicked"));
        }
    }
    handles.push(handle);
    Ok(())
}
pub fn bridge(mut ws: Connection, mut local: TcpStream, stop: &Stop) -> Result<(), Error> {
    let mut pending = Vec::new();
    let mut offset = 0;
    let mut buffer = [0; 4096];
    while !stop.requested() {
        if offset == pending.len() {
            pending.clear();
            offset = 0;
            match ws.read()? {
                Some(Message::Text(text)) => pending.extend_from_slice(text.as_bytes()),
                Some(Message::Binary(bytes)) => pending.extend_from_slice(&bytes),
                Some(Message::Close(_)) => break,
                Some(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) | None => {}
            }
        }
        if offset < pending.len() {
            match local.write(&pending[offset..]) {
                Ok(0) => break,
                Ok(count) => offset += count,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(error.into()),
            }
        }
        match local.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => ws.send(Message::Binary(buffer[..count].to_vec().into()))?,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error.into()),
        }
        ws.flush()?;
        stop.wait(Duration::from_millis(5));
    }
    local.shutdown(std::net::Shutdown::Both)?;
    Ok(())
}
