use crate::{
    net,
    state::{self, Shared},
    Error,
};
use openpilot_logging::producer::Logger;
use openpilot_uploader::http::SigningKey;
use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tungstenite::{
    client::IntoClientRequest,
    handshake::HandshakeError,
    protocol::{
        frame::{
            coding::{Data, OpCode},
            Frame,
        },
        WebSocketConfig,
    },
    stream::MaybeTlsStream,
    Message, WebSocket,
};

pub struct Monitored {
    stream: TcpStream,
    read_at: Arc<Mutex<Instant>>,
}
impl Read for Monitored {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let count = self.stream.read(bytes)?;
        if count > 0 {
            *self
                .read_at
                .lock()
                .map_err(|_| std::io::Error::other("read clock poisoned"))? = Instant::now();
        }
        Ok(count)
    }
}
impl Write for Monitored {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.stream.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}
pub struct Connection {
    socket: WebSocket<MaybeTlsStream<Monitored>>,
    pub tcp: TcpStream,
    read_at: Arc<Mutex<Instant>>,
}
impl Connection {
    pub fn connect(uri: &str, token: &str) -> Result<Self, Error> {
        let mut url = url::Url::parse(uri)?;
        for redirects in 0..=3 {
            let stream = net::websocket_tcp(&url, Duration::from_secs(30))?;
            let tcp = stream.try_clone()?;
            let read_at = Arc::new(Mutex::new(Instant::now()));
            let stream = Monitored {
                stream,
                read_at: Arc::clone(&read_at),
            };
            let mut request = url.as_str().into_client_request()?;
            request.headers_mut().insert(
                "Cookie",
                format!("jwt={token}")
                    .parse()
                    .map_err(|_| Error::Contract("invalid JWT cookie"))?,
            );
            let mut origin = url.clone();
            origin
                .set_scheme(if url.scheme() == "wss" {
                    "https"
                } else {
                    "http"
                })
                .map_err(|_| Error::Contract("invalid origin scheme"))?;
            origin.set_path("");
            origin.set_query(None);
            origin.set_fragment(None);
            request.headers_mut().insert(
                "Origin",
                origin
                    .as_str()
                    .trim_end_matches('/')
                    .parse()
                    .map_err(|_| Error::Contract("invalid origin"))?,
            );
            let config = WebSocketConfig::default()
                .max_message_size(None)
                .max_frame_size(None)
                .write_buffer_size(0);
            match tungstenite::client_tls_with_config(request, stream, Some(config), None) {
                Ok((socket, _)) => {
                    tcp.set_nonblocking(true)?;
                    return Ok(Self {
                        socket,
                        tcp,
                        read_at,
                    });
                }
                Err(HandshakeError::Failure(tungstenite::Error::Http(response)))
                    if response.status().is_redirection() && redirects < 3 =>
                {
                    url = url.join(
                        response
                            .headers()
                            .get("Location")
                            .and_then(|value| value.to_str().ok())
                            .ok_or(Error::Contract("redirect location missing"))?,
                    )?;
                }
                Err(HandshakeError::Failure(error)) => return Err(error.into()),
                Err(HandshakeError::Interrupted(_)) => {
                    return Err(Error::Contract("incomplete WebSocket handshake"))
                }
            }
        }
        Err(Error::Contract("WebSocket redirect limit"))
    }
    pub fn read(&mut self) -> Result<Option<Message>, Error> {
        match self.socket.read() {
            Ok(message) => Ok(Some(message)),
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                Ok(None)
            }
            Err(error) => Err(error.into()),
        }
    }
    pub fn send(&mut self, message: Message) -> Result<(), Error> {
        match self.socket.write(message) {
            Ok(()) => Ok(()),
            Err(tungstenite::Error::Io(error))
                if error.kind() == std::io::ErrorKind::WouldBlock =>
            {
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }
    pub fn text(&mut self, text: &str) -> Result<(), Error> {
        let chars: Vec<char> = text.chars().collect();
        for (index, chunk) in chars.chunks(4096).enumerate() {
            self.send(Message::Frame(Frame::message(
                chunk.iter().collect::<String>().into_bytes(),
                OpCode::Data(if index == 0 {
                    Data::Text
                } else {
                    Data::Continue
                }),
                (index + 1) * 4096 >= chars.len(),
            )))?;
        }
        Ok(())
    }
    /// True means all accepted frames have reached the underlying stream.
    pub fn flush(&mut self) -> Result<bool, Error> {
        match self.socket.flush() {
            Ok(()) => Ok(true),
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                Ok(false)
            }
            Err(error) => Err(error.into()),
        }
    }
    pub(crate) fn bound_proxy_output(&mut self) {
        self.socket
            .set_config(|config| config.max_write_buffer_size = 8192);
    }
    pub fn read_timeout(&self) -> Result<bool, Error> {
        let mut last = self
            .read_at
            .lock()
            .map_err(|_| Error::Contract("read clock poisoned"))?;
        if last.elapsed() >= Duration::from_secs(30) {
            *last = Instant::now();
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        if let Err(error) = self.socket.close(None) {
            if !matches!(
                error,
                tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed
            ) {
                eprintln!("athenad: WebSocket close: {error}");
            }
        }
        if let Err(error) = self.tcp.shutdown(std::net::Shutdown::Both) {
            eprintln!("athenad: socket shutdown: {error}");
        }
    }
}
pub fn token(shared: &Shared, logger: &mut Logger) -> Result<String, Error> {
    let id = shared
        .text("DongleId", logger)?
        .ok_or(Error::Contract("DongleId missing"))?;
    let key = SigningKey::load(&shared.config.persist_root)?
        .ok_or(Error::Contract("signing key unavailable"))?;
    Ok(key.token(
        &id,
        u64::try_from(state::now_ms()? / 1000)
            .map_err(|_| Error::Contract("negative wall clock"))?,
    )?)
}
