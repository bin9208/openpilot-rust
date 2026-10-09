use super::{protocol, session::Context};
use crate::{
    web_sound::{
        clock::{self, Heartbeat},
        socket, Shutdown,
    },
    Error, Value,
};
use futures_util::{stream::SplitStream, SinkExt, StreamExt};
use std::time::Duration;
use tokio::sync::watch;
use tokio_tungstenite::{
    tungstenite::{protocol::frame::coding::CloseCode, Error as WsError, Message},
    WebSocketStream,
};

pub(super) async fn run(
    context: &Context,
    mut stream: SplitStream<WebSocketStream<crate::web_sound::transport::Transport>>,
    mut stop: watch::Receiver<Shutdown>,
) -> Result<socket::Exit, Error> {
    let mut heartbeat = Heartbeat::new(clock::now()?);
    loop {
        if *stop.borrow() == Shutdown::Force {
            return Ok(socket::Exit::Drop);
        }
        let delay = Duration::from_secs_f64((heartbeat.deadline() - clock::now()?).max(0.));
        tokio::select! {
            changed = stop.changed() => if changed.is_err() { return Ok(socket::Exit::Drop); },
            _ = tokio::time::sleep(delay) => match heartbeat {
                Heartbeat::PingAt(_) => {
                    heartbeat.ping_sent(clock::now()?);
                    context.sink.lock().await.send(Message::Ping(Vec::new().into())).await.map_err(|error| Error::Source(error.to_string()))?;
                }
                Heartbeat::PongBefore(_) => return Ok(socket::Exit::Drop),
            },
            packet = stream.next() => {
                heartbeat.activity(clock::now()?);
                match packet {
                    Some(Ok(Message::Text(text))) => {
                        let Ok(value) = Value::parse(&text) else { continue; };
                        if !matches!(value, Value::Object(_)) { return Ok(socket::Exit::Drop); }
                        if let Err(error) = protocol::handle(context, value).await { protocol::error(context, error).await?; }
                    }
                    Some(Ok(Message::Ping(_))) => context.sink.lock().await.flush().await.map_err(|error| Error::Source(error.to_string()))?,
                    Some(Ok(Message::Close(_))) => return Ok(socket::Exit::PeerClosed),
                    Some(Ok(Message::Binary(_) | Message::Pong(_) | Message::Frame(_))) => {}
                    Some(Err(WsError::Capacity(_))) => return Ok(socket::Exit::Protocol(CloseCode::Size)),
                    Some(Err(WsError::Utf8(_))) => return Ok(socket::Exit::Protocol(CloseCode::Invalid)),
                    Some(Err(WsError::Protocol(_))) => return Ok(socket::Exit::Protocol(CloseCode::Protocol)),
                    Some(Err(_)) | None => return Ok(socket::Exit::Drop),
                }
            }
        }
    }
}
