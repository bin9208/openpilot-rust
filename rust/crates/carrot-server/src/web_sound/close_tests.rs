use std::io::{self, Cursor, Read, Write};
use tokio_tungstenite::tungstenite::{
    protocol::{CloseFrame, Role, WebSocketConfig},
    Error, Message, WebSocket,
};

struct Stream {
    input: Cursor<Vec<u8>>,
    output: Vec<u8>,
    remaining: Option<usize>,
}

impl Read for Stream {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.input.read(bytes)
    }
}

impl Write for Stream {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let remaining = self.remaining.unwrap_or(bytes.len());
        if remaining == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = remaining.min(bytes.len());
        self.output.extend_from_slice(&bytes[..count]);
        self.remaining = self.remaining.map(|remaining| remaining - count);
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn close(code: u16, reason: &str) -> CloseFrame {
    CloseFrame {
        code: code.into(),
        reason: reason.to_owned().into(),
    }
}

fn peer(role: Role, frame: Option<CloseFrame>) -> Result<Stream, Error> {
    let peer = match role {
        Role::Server => Role::Client,
        Role::Client => Role::Server,
    };
    let mut socket = WebSocket::from_raw_socket(Cursor::new(Vec::new()), peer, None);
    socket.send(Message::Close(frame))?;
    Ok(Stream {
        input: Cursor::new(socket.into_inner().into_inner()),
        output: Vec::new(),
        remaining: None,
    })
}

fn flush(socket: &mut WebSocket<Stream>) {
    assert!(matches!(
        socket.flush(),
        Ok(()) | Err(Error::ConnectionClosed)
    ));
}

#[test]
fn server_reply_is_opt_in_and_preserves_received_close() -> Result<(), Error> {
    for enabled in [false, true] {
        for input in [Some(close(3001, "owned peer reason")), None] {
            let config = WebSocketConfig::default().reply_with_normal_close(enabled);
            let mut socket = WebSocket::from_raw_socket(
                peer(Role::Server, input.clone())?,
                Role::Server,
                Some(config),
            );
            assert_eq!(socket.read()?, Message::Close(input.clone()));
            flush(&mut socket);
            let mut receiver = WebSocket::from_raw_socket(
                Cursor::new(socket.into_inner().output),
                Role::Client,
                None,
            );
            let expected = if enabled {
                Some(close(1000, ""))
            } else {
                input
            };
            assert_eq!(receiver.read()?, Message::Close(expected));
        }
    }
    Ok(())
}

#[test]
fn client_role_and_invalid_peer_close_keep_existing_reply() -> Result<(), Error> {
    for (role, input, expected) in [
        (
            Role::Client,
            close(3001, "owned peer reason"),
            close(3001, "owned peer reason"),
        ),
        (
            Role::Server,
            close(1005, "owned invalid code"),
            close(1002, "Protocol violation"),
        ),
    ] {
        let config = WebSocketConfig::default().reply_with_normal_close(true);
        let mut socket = WebSocket::from_raw_socket(peer(role, Some(input))?, role, Some(config));
        socket.read()?;
        flush(&mut socket);
        let receiver_role = match role {
            Role::Server => Role::Client,
            Role::Client => Role::Server,
        };
        let mut receiver = WebSocket::from_raw_socket(
            Cursor::new(socket.into_inner().output),
            receiver_role,
            None,
        );
        assert_eq!(receiver.read()?, Message::Close(Some(expected)));
    }
    Ok(())
}

#[test]
fn partial_application_frame_completes_before_close_reply() -> Result<(), Error> {
    for enabled in [false, true] {
        let config = WebSocketConfig::default().reply_with_normal_close(enabled);
        let mut socket = WebSocket::from_raw_socket(
            peer(Role::Server, Some(close(3001, "peer")))?,
            Role::Server,
            Some(config),
        );
        socket.get_mut().remaining = Some(3);
        let text = Message::text("owned application frame pending during close");
        socket.write(text.clone())?;
        assert!(
            matches!(socket.flush(), Err(Error::Io(error)) if error.kind() == io::ErrorKind::WouldBlock)
        );
        assert_eq!(socket.get_ref().output.len(), 3);
        assert_eq!(socket.read()?, Message::Close(Some(close(3001, "peer"))));
        socket.get_mut().remaining = None;
        flush(&mut socket);
        let mut receiver =
            WebSocket::from_raw_socket(Cursor::new(socket.into_inner().output), Role::Client, None);
        assert_eq!(receiver.read()?, text);
        let expected = if enabled {
            close(1000, "")
        } else {
            close(3001, "peer")
        };
        assert_eq!(receiver.read()?, Message::Close(Some(expected)));
    }
    Ok(())
}
