use super::{
    camera::CameraHub,
    raw::{Pending, RawHub},
    socket,
    transport::Transport,
    Admission, Mode, Shutdown, Spec,
};
use crate::Error;
use futures_util::{SinkExt, StreamExt};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};
use tokio::{
    sync::{watch, Mutex, Notify},
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
    pub spec: Spec,
    pub admission: Admission,
    pub raw: Rc<RefCell<RawHub>>,
    pub camera: Rc<RefCell<CameraHub>>,
    pub stop: watch::Receiver<Shutdown>,
    pub registrations: Rc<Notify>,
}
pub(super) async fn run(mut launch: Launch) -> Result<(), Error> {
    let _admission = launch.admission;
    if *launch.stop.borrow() == Shutdown::Force {
        return Ok(());
    }
    let upgraded = tokio::select! {result=launch.upgrade=>result.map_err(|error|Error::Source(error.to_string()))?,_=launch.stop.changed()=>return Ok(())};
    let max = match launch.spec.mode {
        Mode::Single | Mode::Multiplex => 8 * 1024 * 1024,
        Mode::Compact => 1024 * 1024,
        Mode::Camera => 2 * 1024 * 1024,
    };
    let config = WebSocketConfig::default()
        .max_message_size(Some(max))
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
    tokio::select! {result=async {sink.lock().await.send(Message::text(launch.spec.hello)).await}=>result.map_err(|error|Error::Source(error.to_string()))?,_=launch.stop.changed()=>return Ok(())}
    let mut tasks = JoinSet::new();
    let failed = Rc::new(Notify::new());
    match launch.spec.mode {
        Mode::Camera => {
            launch
                .camera
                .borrow_mut()
                .register(launch.id, Arc::clone(&sink), Rc::clone(&failed))
        }
        Mode::Single | Mode::Multiplex | Mode::Compact => {
            let pending = Rc::new(Pending::new(launch.spec.mode, launch.spec.services));
            launch
                .raw
                .borrow_mut()
                .register(launch.id, Rc::clone(&pending));
            let sink = Arc::clone(&sink);
            tasks.spawn_local(async move { pending.send(sink).await });
        }
    }
    match launch.spec.mode {
        Mode::Camera => launch.camera.borrow_mut().poll(),
        Mode::Single | Mode::Multiplex | Mode::Compact => launch.raw.borrow_mut().poll(),
    }
    launch.registrations.notify_one();
    let result = tokio::select! {
        result=socket::receive(Arc::clone(&sink),stream,launch.stop.clone())=>result,
        _=tasks.join_next(),if !tasks.is_empty()=>{
            close(&sink,"state_send_timeout").await;socket::Exit::Drop
        }
        _=failed.notified()=>{close(&sink,"camera_send_timeout").await;socket::Exit::Drop}
    };
    launch.raw.borrow_mut().unregister(launch.id);
    launch.camera.borrow_mut().unregister(launch.id);
    launch.registrations.notify_one();
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    socket::finish(sink, result, launch.stop).await
}
async fn close(sink: &socket::Sink, reason: &str) {
    match tokio::time::timeout(Duration::from_millis(750), async {
        sink.lock()
            .await
            .send(Message::Close(Some(CloseFrame {
                code: CloseCode::Error,
                reason: reason.to_owned().into(),
            })))
            .await
    })
    .await
    {
        Ok(Ok(())) | Ok(Err(_)) | Err(_) => {}
    }
}
