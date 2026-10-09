use super::{legacy, receive, Admission, Client, Command, Config};
use crate::{
    tools::runner::Runner,
    web_sound::{socket, transport::Transport, Shutdown},
    Error,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::{
    sync::{mpsc, oneshot, watch, Mutex, Notify},
    task::JoinSet,
};
use tokio_tungstenite::{
    tungstenite::{
        protocol::{frame::coding::CloseCode, CloseFrame, Role, WebSocketConfig},
        Message,
    },
    WebSocketStream,
};

#[derive(Clone, Copy)]
pub(super) enum Mode {
    Pty,
    Tmux,
}
pub(super) struct Spec {
    pub mode: Mode,
    pub session: String,
    pub reset: bool,
    pub rows: crate::Value,
    pub cols: crate::Value,
}
pub(super) struct Context {
    pub sender: mpsc::UnboundedSender<Command>,
    pub spec: Spec,
    pub sink: socket::Sink,
    pub legacy: legacy::Context,
}
pub(super) struct Start {
    pub upgrade: hyper::upgrade::OnUpgrade,
    pub spec: Spec,
    pub admission: Admission,
    pub id: u64,
}
pub(super) struct Owner {
    pub sender: mpsc::UnboundedSender<Command>,
    pub stop: watch::Receiver<Shutdown>,
    pub config: Config,
    pub runner: Runner,
}
struct Registration {
    id: u64,
    sender: mpsc::UnboundedSender<Command>,
    closed: Arc<AtomicBool>,
}
impl Drop for Registration {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::SeqCst);
        let _result = self.sender.send(Command::Detach(self.id));
    }
}
pub(super) async fn run(start: Start, owner: Owner) -> Result<(), Error> {
    let Start {
        upgrade,
        spec,
        admission,
        id,
    } = start;
    let Owner {
        sender,
        mut stop,
        config,
        runner,
    } = owner;
    let _admission = admission;
    let upgraded = tokio::select! { result = upgrade => result.map_err(|error| Error::Source(error.to_string()))?, _ = stop.changed() => return Ok(()) };
    let websocket = WebSocketStream::from_raw_socket(
        Transport::new(upgraded, stop.clone()),
        Role::Server,
        Some(
            WebSocketConfig::default()
                .max_message_size(Some(4 * 1024 * 1024))
                .max_frame_size(None)
                .reply_with_normal_close(true),
        ),
    )
    .await;
    let (sink, stream) = websocket.split();
    let sink = Arc::new(Mutex::new(sink));
    let mut tasks = JoinSet::new();
    let finished = Arc::new(Notify::new());
    let closed = Arc::new(AtomicBool::new(false));
    let _registration = Registration {
        id,
        sender: sender.clone(),
        closed: Arc::clone(&closed),
    };
    let legacy = legacy::Context::new(config, runner, spec.session.clone(), Arc::clone(&sink));
    match spec.mode {
        Mode::Pty => {
            // Source parses initial geometry after the 101 upgrade and outside
            // its attach error handler; invalid values end the socket.
            spec.rows.int()?;
            spec.cols.int()?;
            let (reply, result) = oneshot::channel();
            sender
                .send(Command::Attach {
                    id,
                    client: Client {
                        sink: Arc::clone(&sink),
                        finished: Arc::clone(&finished),
                        closed,
                    },
                    eligible: true,
                    reset: spec.reset,
                    reply,
                })
                .map_err(|_| Error::Source("terminal owner unavailable".into()))?;
            match result
                .await
                .map_err(|_| Error::Source("terminal owner unavailable".into()))?
            {
                Ok(()) => {}
                Err(error) => {
                    sink.lock()
                        .await
                        .send(Message::text(
                            crate::Value::object([
                                ("type", crate::Value::text("error")),
                                ("error", crate::Value::text(&error.to_string())),
                                ("session", crate::Value::text("login-shell")),
                            ])
                            .encode()?,
                        ))
                        .await
                        .map_err(|error| Error::Source(error.to_string()))?;
                    sink.lock()
                        .await
                        .send(normal_close())
                        .await
                        .map_err(|error| Error::Source(error.to_string()))?;
                    return Ok(());
                }
            }
        }
        Mode::Tmux => {
            if let Err(error) = legacy.initialize().await {
                legacy.error(&error).await?;
                sink.lock()
                    .await
                    .send(normal_close())
                    .await
                    .map_err(|error| Error::Source(error.to_string()))?;
                return Ok(());
            }
            let pump = legacy.clone();
            tasks.spawn_local(async move { pump.pump().await });
            legacy.initial_screen().await?;
        }
    }
    let context = Context {
        sender,
        spec,
        sink: Arc::clone(&sink),
        legacy,
    };
    let result = tokio::select! {
        result = receive::run(&context, stream, stop.clone()) => result?,
        _ = finished.notified() => {
            sink.lock().await.send(normal_close()).await.map_err(|error| Error::Source(error.to_string()))?;
            socket::Exit::Drop
        }
        _ = tasks.join_next(), if !tasks.is_empty() => socket::Exit::Drop,
    };
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    socket::finish(sink, result, stop).await
}
fn normal_close() -> Message {
    Message::Close(Some(CloseFrame {
        code: CloseCode::Normal,
        reason: "".into(),
    }))
}
