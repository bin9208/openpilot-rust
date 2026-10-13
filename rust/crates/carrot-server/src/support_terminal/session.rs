use super::{
    context::{Audience, Context},
    model::{Client, Session},
};
use crate::{
    web_sound::{socket, transport::Transport},
    Error, Value,
};
use futures_util::{SinkExt, StreamExt};
use std::{
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::{Mutex, Notify};
use tokio_tungstenite::{
    tungstenite::{
        protocol::{frame::coding::CloseCode, CloseFrame, Role, WebSocketConfig},
        Message,
    },
    WebSocketStream,
};

pub(super) enum Mode {
    Owner,
    Guest(String),
}
pub(super) struct Connection {
    pub context: Rc<Context>,
    pub session: Session,
    pub id: u64,
    pub client: Client,
    pub owner: bool,
    pub authed: bool,
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.client.closed.store(true, Ordering::SeqCst);
        if let Some(id) = self.client.terminal_id {
            self.context.config.terminal.detach_guest_now(id);
        }
        let mut session = self.session.borrow_mut();
        if self.owner {
            session.owners.remove(&self.id);
        } else {
            session.guests.remove(&self.id);
        }
        if session.controller == Some(self.id) {
            session.controller = None;
        }
    }
}
pub(super) fn normal_close() -> Message {
    Message::Close(Some(CloseFrame {
        code: CloseCode::Normal,
        reason: "".into(),
    }))
}
pub(super) async fn run(
    context: Rc<Context>,
    upgrade: hyper::upgrade::OnUpgrade,
    mode: Mode,
    id: u64,
) -> Result<(), Error> {
    let upgraded = upgrade
        .await
        .map_err(|error| Error::Source(error.to_string()))?;
    let socket = WebSocketStream::from_raw_socket(
        Transport::new(upgraded, context.stop.clone()),
        Role::Server,
        Some(
            WebSocketConfig::default()
                .max_message_size(Some(4 * 1024 * 1024))
                .max_frame_size(None)
                .reply_with_normal_close(true),
        ),
    )
    .await;
    let (sink, mut stream) = socket.split();
    let sink = Arc::new(Mutex::new(sink));
    let client = Client {
        sink: Arc::clone(&sink),
        finished: Arc::new(Notify::new()),
        closed: Arc::new(AtomicBool::new(false)),
        terminal_id: None,
    };
    let current = context.current();
    let available = match &mode {
        Mode::Owner => current.is_some(),
        Mode::Guest(id) => current.as_ref().is_some_and(|session| {
            let session = session.borrow();
            session.id == *id && !session.expired()
        }),
    };
    if !available {
        let value = match mode {
            Mode::Owner => {
                let mut value = Value::object([("type", Value::text("session_status"))]);
                if let (Value::Object(target), Value::Object(status)) =
                    (&mut value, context.snapshot())
                {
                    target.extend(status);
                }
                value
            }
            Mode::Guest(_) => Value::object([
                ("type", Value::text("error")),
                ("message", Value::text("session unavailable")),
            ]),
        };
        context.send(&client, value).await?;
        sink.lock()
            .await
            .send(normal_close())
            .await
            .map_err(|error| Error::Source(error.to_string()))?;
        let _handshake =
            tokio::time::timeout(std::time::Duration::from_secs(10), stream.next()).await;
        return Ok(());
    }
    let session = current.ok_or_else(|| Error::Source("support session missing".into()))?;
    let owner = matches!(mode, Mode::Owner);
    let mut connection = Connection {
        context: Rc::clone(&context),
        session: Rc::clone(&session),
        id,
        client,
        owner,
        authed: owner,
    };
    if owner {
        session
            .borrow_mut()
            .owners
            .insert(id, connection.client.clone());
        context
            .send(&connection.client, super::context::status(&session))
            .await?;
        let count = session.borrow().guests.len();
        context
            .broadcast(
                Audience::Owners,
                Value::object([
                    ("type", Value::text("guest_presence")),
                    ("count", Value::integer(count)),
                ]),
            )
            .await;
        context
            .broadcast(
                Audience::Guests,
                Value::object([
                    ("type", Value::text("owner_presence")),
                    ("active", Value::Bool(true)),
                ]),
            )
            .await;
    }
    let result = super::receive::run(&mut connection, stream).await;
    drop(connection);
    if owner {
        let active = !session.borrow().owners.is_empty();
        context
            .broadcast(
                Audience::Guests,
                Value::object([
                    ("type", Value::text("owner_presence")),
                    ("active", Value::Bool(active)),
                ]),
            )
            .await;
    } else {
        context.roles(&session).await;
        let count = session.borrow().guests.len();
        context
            .broadcast(
                Audience::Owners,
                Value::object([
                    ("type", Value::text("guest_presence")),
                    ("count", Value::integer(count)),
                ]),
            )
            .await;
    }
    let exit = result?;
    socket::finish(sink, exit, context.stop.clone()).await
}
