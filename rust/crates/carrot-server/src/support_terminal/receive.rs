use super::session::{normal_close, Connection};
use crate::{
    web_sound::{
        clock::{self, Heartbeat},
        socket,
        transport::Transport,
        Shutdown,
    },
    Error, Value,
};
use futures_util::{stream::SplitStream, SinkExt, StreamExt};
use std::time::Duration;
use tokio::task::JoinSet;
use tokio_tungstenite::{
    tungstenite::{protocol::frame::coding::CloseCode, Error as WsError, Message},
    WebSocketStream,
};

pub(super) async fn run(
    connection: &mut Connection,
    mut stream: SplitStream<WebSocketStream<Transport>>,
) -> Result<socket::Exit, Error> {
    let mut heartbeat = Heartbeat::new(clock::now()?);
    let mut stopped = connection.context.stop.clone();
    let mut controls = JoinSet::new();
    let result = loop {
        if *stopped.borrow() == Shutdown::Force {
            break Ok(socket::Exit::Drop);
        }
        let delay = Duration::from_secs_f64((heartbeat.deadline() - clock::now()?).max(0.));
        tokio::select! {
            changed = stopped.changed() => if changed.is_err() || *stopped.borrow() == Shutdown::Force { break Ok(socket::Exit::Drop); },
            _ = connection.client.finished.notified() => {
                connection.client.sink.lock().await.send(normal_close()).await.map_err(|error| Error::Source(error.to_string()))?;
                break Ok(socket::Exit::Drop);
            }
            joined = controls.join_next(), if !controls.is_empty() => if !matches!(joined, Some(Ok(Ok(())))) { break Ok(socket::Exit::Drop); },
            _ = tokio::time::sleep(delay) => match heartbeat {
                Heartbeat::PingAt(_) => {
                    heartbeat.ping_sent(clock::now()?);
                    let sink = connection.client.sink.clone();
                    controls.spawn_local(async move { sink.lock().await.send(Message::Ping(Vec::new().into())).await });
                }
                Heartbeat::PongBefore(_) => break Ok(socket::Exit::Drop),
            },
            message = stream.next() => {
                heartbeat.activity(clock::now()?);
                match message {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(value) = Value::parse(&text) {
                            if !connection.input(value).await? {
                                connection.client.sink.lock().await.send(normal_close()).await.map_err(|error| Error::Source(error.to_string()))?;
                                if !connection.authed {
                                    let _handshake = tokio::time::timeout(Duration::from_secs(10), stream.next()).await;
                                }
                                break Ok(socket::Exit::Drop);
                            }
                        }
                    }
                    Some(Ok(Message::Ping(_))) => {
                        let sink = connection.client.sink.clone();
                        controls.spawn_local(async move { sink.lock().await.flush().await });
                    }
                    Some(Ok(Message::Close(_))) => break Ok(socket::Exit::PeerClosed),
                    Some(Ok(Message::Binary(_) | Message::Pong(_) | Message::Frame(_))) => {},
                    Some(Err(WsError::Capacity(_))) => break Ok(socket::Exit::Protocol(CloseCode::Size)),
                    Some(Err(WsError::Utf8(_))) => break Ok(socket::Exit::Protocol(CloseCode::Invalid)),
                    Some(Err(WsError::Protocol(_))) => break Ok(socket::Exit::Protocol(CloseCode::Protocol)),
                    Some(Err(_)) | None => break Ok(socket::Exit::Drop),
                }
            }
        }
    };
    controls.abort_all();
    while controls.join_next().await.is_some() {}
    result
}
