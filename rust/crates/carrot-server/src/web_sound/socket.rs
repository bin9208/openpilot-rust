use super::clock::{self, Heartbeat};
use super::{transport::Transport, Shutdown};
use crate::Error;
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{watch, Mutex},
    task::JoinSet,
};
use tokio_tungstenite::{
    tungstenite::{
        protocol::{frame::coding::CloseCode, CloseFrame},
        Error as WsError, Message,
    },
    WebSocketStream,
};

type Socket = WebSocketStream<Transport>;
pub(crate) type Sink = Arc<Mutex<SplitSink<Socket, Message>>>;

pub(crate) enum Exit {
    PeerClosed,
    Protocol(CloseCode),
    Drop,
}

pub(crate) async fn receive(
    sink: Sink,
    mut stream: SplitStream<Socket>,
    mut shutdown: watch::Receiver<Shutdown>,
) -> Exit {
    let Ok(started) = clock::now() else {
        return Exit::Drop;
    };
    let mut heartbeat = Heartbeat::new(started);
    let mut controls = JoinSet::new();
    let mut flush = false;
    let mut ping = false;
    let result = loop {
        if *shutdown.borrow() == Shutdown::Force {
            break Exit::Drop;
        }
        if controls.is_empty() && (flush || ping) {
            let sink = Arc::clone(&sink);
            let send_ping = ping;
            flush = false;
            ping = false;
            controls.spawn(async move {
                let mut sink = sink.lock().await;
                if send_ping {
                    sink.send(Message::Ping(Vec::new().into())).await
                } else {
                    sink.flush().await
                }
            });
        }
        let Ok(now) = clock::now() else {
            break Exit::Drop;
        };
        let delay = Duration::from_secs_f64((heartbeat.deadline() - now).max(0.));
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() == Shutdown::Force {
                    break Exit::Drop;
                }
            }
            joined = controls.join_next(), if !controls.is_empty() => {
                match joined {
                    Some(Ok(Ok(()))) => {}
                    Some(Ok(Err(_)) | Err(_)) | None => break Exit::Drop,
                }
            }
            _ = tokio::time::sleep(delay) => {
                match heartbeat {
                    Heartbeat::PingAt(_) => {
                        let Ok(now) = clock::now() else { break Exit::Drop; };
                        heartbeat.ping_sent(now);
                        ping = true;
                    }
                    Heartbeat::PongBefore(_) => break Exit::Drop,
                }
            }
            message = stream.next() => {
                let Ok(now) = clock::now() else { break Exit::Drop; };
                heartbeat.activity(now);
                match message {
                    Some(Ok(Message::Text(_) | Message::Binary(_) | Message::Pong(_))) => {}
                    Some(Ok(Message::Ping(_))) => flush = true,
                    Some(Ok(Message::Close(_))) => break Exit::PeerClosed,
                    Some(Ok(Message::Frame(_))) => {}
                    Some(Err(WsError::Capacity(_))) => break Exit::Protocol(CloseCode::Size),
                    Some(Err(WsError::Utf8(_))) => break Exit::Protocol(CloseCode::Invalid),
                    Some(Err(WsError::Protocol(_))) => break Exit::Protocol(CloseCode::Protocol),
                    Some(Err(_)) | None => break Exit::Drop,
                }
            }
        }
    };
    controls.abort_all();
    while controls.join_next().await.is_some() {}
    result
}

pub(crate) async fn finish(
    sink: Sink,
    result: Exit,
    mut shutdown: watch::Receiver<Shutdown>,
) -> Result<(), Error> {
    if *shutdown.borrow() == Shutdown::Force {
        return Ok(());
    }
    let operation = async {
        match result {
            Exit::PeerClosed => sink.lock().await.flush().await,
            Exit::Protocol(code) => {
                sink.lock()
                    .await
                    .send(Message::Close(Some(CloseFrame {
                        code,
                        reason: "".into(),
                    })))
                    .await
            }
            Exit::Drop => Ok(()),
        }
    };
    tokio::pin!(operation);
    let result = loop {
        tokio::select! {
            result = &mut operation => break result,
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() == Shutdown::Force {
                    return Ok(());
                }
            }
        }
    };
    match result {
        Ok(()) | Err(WsError::ConnectionClosed | WsError::AlreadyClosed) => Ok(()),
        Err(error) => Err(Error::Source(error.to_string())),
    }
}
