use super::{
    bridge::Bridge,
    clients::{Clients, Close},
    runtime::{Admission, Mode},
    wire,
};
use crate::{
    web_sound::{socket, transport::Transport, Shutdown},
    Error,
};
use futures_util::{SinkExt, StreamExt};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};
use tokio::{
    sync::{watch, Mutex},
    task::JoinSet,
};
use tokio_tungstenite::{
    tungstenite::{
        protocol::{frame::coding::CloseCode, CloseFrame, Role, WebSocketConfig},
        Message,
    },
    WebSocketStream,
};

pub(super) struct Launch {
    pub id: u64,
    pub upgrade: hyper::upgrade::OnUpgrade,
    pub identity: String,
    pub mode: Mode,
    pub admission: Admission,
    pub bridge: Rc<RefCell<Bridge>>,
    pub stop: watch::Receiver<Shutdown>,
}
pub(super) async fn run(mut launch: Launch) -> Result<(), Error> {
    let _admission = launch.admission;
    if *launch.stop.borrow() == Shutdown::Force {
        return Ok(());
    }
    let upgraded = tokio::select! {result=launch.upgrade=>result.map_err(|error|Error::Source(error.to_string()))?,_=launch.stop.changed()=>return Ok(())};
    let config = WebSocketConfig::default()
        .max_message_size(Some(8 * 1024 * 1024))
        .max_frame_size(None)
        .reply_with_normal_close(true);
    let websocket = WebSocketStream::from_raw_socket(
        Transport::new(upgraded, launch.stop.clone()),
        Role::Server,
        Some(config),
    )
    .await;
    let (sink, stream) = websocket.split();
    let sink = Arc::new(Mutex::new(sink));
    let (mode, takeover, hud) = match launch.mode {
        Mode::State { takeover } => (None, takeover, false),
        Mode::Media { include_map, hud } => (Some(include_map), false, hud),
    };
    let pending = launch
        .bridge
        .borrow_mut()
        .register(launch.id, launch.identity, mode, takeover);
    let Some(pending) = pending else {
        let status = wire::session("busy", "carrot_navi_busy")?;
        send(&sink, Message::text(status)).await;
        close(
            &sink,
            Close {
                code: 4409,
                reason: "carrot_navi_busy",
            },
        )
        .await;
        let result = closing(socket::receive(
            Arc::clone(&sink),
            stream,
            launch.stop.clone(),
        ))
        .await;
        return socket::finish(sink, result, launch.stop).await;
    };
    let mut closed = pending.closed.subscribe();
    if !send(&sink, Message::text(Clients::accepted()?)).await {
        launch.bridge.borrow_mut().unregister(launch.id);
        return Ok(());
    }
    if hud {
        launch.bridge.borrow_mut().profile(true);
    }
    let mut tasks = JoinSet::new();
    let sender_sink = Arc::clone(&sink);
    tasks.spawn_local(async move {
        pending.send(sender_sink).await;
    });
    let mut receiver = Box::pin(socket::receive(
        Arc::clone(&sink),
        stream,
        launch.stop.clone(),
    ));
    let result = tokio::select! {
        result=&mut receiver=>result,
        _=tasks.join_next()=>{close(&sink,Close{code:1000,reason:""}).await;socket::Exit::Drop},
        _=closed.changed()=>{
            let notice=closed.borrow().clone();
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
            if let Some(notice)=notice {close(&sink,notice).await;}
            socket::Exit::Drop
        },
    };
    launch.bridge.borrow_mut().unregister(launch.id);
    if hud {
        launch.bridge.borrow_mut().profile(false);
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    socket::finish(sink, result, launch.stop).await
}
async fn closing(receiver: impl std::future::Future<Output = socket::Exit>) -> socket::Exit {
    tokio::time::timeout(Duration::from_secs(10), receiver)
        .await
        .unwrap_or(socket::Exit::Drop)
}
async fn send(sink: &socket::Sink, message: Message) -> bool {
    matches!(
        tokio::time::timeout(Duration::from_secs(1), async {
            sink.lock().await.send(message).await
        })
        .await,
        Ok(Ok(()))
    )
}
async fn close(sink: &socket::Sink, notice: Close) {
    send(
        sink,
        Message::Close(Some(CloseFrame {
            code: CloseCode::from(notice.code),
            reason: notice.reason.into(),
        })),
    )
    .await;
}
