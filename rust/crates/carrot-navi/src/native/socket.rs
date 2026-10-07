use super::{
    session::Session,
    shared::{Close, Shared},
};
use crate::Error;
use futures_util::{SinkExt, StreamExt};
use hyper_util::rt::TokioIo;
use tokio::time::{Duration, Instant};
use tokio_tungstenite::{
    tungstenite::{
        protocol::{frame::coding::CloseCode, CloseFrame},
        Message,
    },
    WebSocketStream,
};

pub type Socket = WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>;

pub async fn send(socket: &mut Socket, value: &crate::json::Value) -> Result<(), Error> {
    socket
        .send(Message::Text(value.encode()?.into()))
        .await
        .map_err(|error| Error::typed("ConnectionError", error.to_string()))
}

pub async fn close(socket: &mut Socket, code: u16, reason: &'static str) {
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code: CloseCode::from(code),
            reason: reason.into(),
        })))
        .await;
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(message) = socket.next().await {
            if matches!(message, Ok(Message::Close(_)) | Err(_)) {
                break;
            }
        }
    })
    .await;
}

pub async fn receive(socket: &mut Socket, session: &Session, shared: &Shared) -> Result<(), Error> {
    let mut closing = shared.closing.subscribe();
    closing.borrow_and_update();
    let heartbeat = Duration::from_secs(if session.control() { 5 } else { 10 });
    let mut deadline = Instant::now() + heartbeat;
    let mut awaiting_pong = false;
    loop {
        tokio::select! {
            changed = closing.changed() => {
                if changed.is_err() { return Ok(()); }
                let state = *closing.borrow_and_update();
                match state {
                    Close::MapChanged => close(socket, 1012, "map configuration changed").await,
                    Close::Shutdown => close(socket, 1001, "").await,
                    Close::Running => continue,
                }
                return Ok(());
            }
            _ = tokio::time::sleep_until(deadline) => {
                if awaiting_pong { return Ok(()); }
                socket.send(Message::Ping(bytes::Bytes::new())).await.map_err(|error| Error::typed("ConnectionError", error.to_string()))?;
                awaiting_pong = true;
                deadline = Instant::now() + heartbeat / 2;
            }
            message = socket.next() => {
                deadline = Instant::now() + heartbeat;
                awaiting_pong = false;
                match message {
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => { socket.flush().await.map_err(|error| Error::typed("ConnectionError", error.to_string()))?; }
                    Some(Ok(Message::Close(_))) | None => { let _ = socket.flush().await; return Ok(()); }
                    Some(Ok(message)) => {
                        match session.message(message, shared) {
                            Ok(Some(reply)) => send(socket, &reply).await?,
                            Ok(None) => (),
                            Err(error) => {
                                if session.recoverable(&error) {
                                    session.reject(socket, shared, &error).await?;
                                    if !session.control() { return Ok(()); }
                                } else { return Err(error); }
                            }
                        }
                    }
                    Some(Err(error)) => return Err(Error::typed("WebSocketError", error.to_string())),
                }
            }
        }
    }
}
