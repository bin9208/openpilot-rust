use super::{
    context::{Audience, Context, Event},
    model::{Client, Pending, Session},
    policy::{self, Permission},
    random,
};
use crate::{Error, Value};

pub(super) fn named(kind: &str, id: &str) -> Value {
    Value::object([("type", Value::text(kind)), ("id", Value::text(id))])
}
fn error(message: &str) -> Value {
    Value::object([("ok", Value::Bool(false)), ("error", Value::text(message))])
}
impl Context {
    pub async fn queue(
        &self,
        session: &Session,
        client: &Client,
        line: Value,
        control: String,
    ) -> Result<Value, Error> {
        if session.borrow().owners.is_empty() {
            self.send(
                client,
                Value::object([("type", Value::text("owner_absent"))]),
            )
            .await?;
            return Ok(error("owner not present"));
        }
        let command = Pending {
            id: random::token::<8>()?,
            line: policy::clipped(line, 4000),
            created: policy::now(),
            control,
        };
        if session.borrow().options.permission == Permission::AllowAll {
            self.send(client, named("command_approved", &command.id))
                .await?;
            let result = self.run_command(&command).await;
            self.broadcast(
                Audience::Owners,
                Value::object([
                    ("type", Value::text("command_auto_run")),
                    ("id", Value::text(&command.id)),
                    ("line", command.line),
                    ("created_at", Value::Float(command.created)),
                ]),
            )
            .await;
            return Ok(result);
        }
        let payload = command.request();
        let id = command.id.clone();
        session.borrow_mut().pending.push(command);
        self.broadcast(Audience::Owners, payload.clone()).await;
        self.send(client, named("command_waiting_approval", &id))
            .await?;
        self.event(Event::CommandExpiry(session.clone(), id))?;
        Ok(Value::object([
            ("ok", Value::Bool(true)),
            ("command", payload),
        ]))
    }
    pub async fn decision(&self, id: &str, approve: bool) -> Value {
        let Some(session) = self.current() else {
            return error("no active session");
        };
        let command = {
            let mut session = session.borrow_mut();
            let Some(index) = session.pending.iter().position(|command| command.id == id) else {
                return error("command not pending");
            };
            session.pending.remove(index)
        };
        self.broadcast(
            Audience::All,
            named(
                if approve {
                    "command_approved"
                } else {
                    "command_rejected"
                },
                &command.id,
            ),
        )
        .await;
        if approve {
            self.run_command(&command).await
        } else {
            Value::object([("ok", Value::Bool(true))])
        }
    }
    async fn run_command(&self, command: &Pending) -> Value {
        let run = async {
            let bytes = match command.control.as_str() {
                "ctrl_c" => vec![3],
                "clear" => {
                    self.config.terminal.clear_history().await?;
                    b"clear\r".to_vec()
                }
                _ => {
                    let text = replacement(&command.line)?;
                    format!("{}\r", self.config.terminal.translate_support(&text)).into_bytes()
                }
            };
            self.config.terminal.write(bytes).await
        }
        .await;
        match run {
            Ok(()) => {
                self.broadcast(Audience::All, named("command_running", &command.id))
                    .await;
                Value::object([("ok", Value::Bool(true))])
            }
            Err(failure) => {
                self.broadcast(
                    Audience::All,
                    Value::object([
                        ("type", Value::text("command_failed")),
                        ("id", Value::text(&command.id)),
                        ("message", Value::text(&failure.to_string())),
                    ]),
                )
                .await;
                error(&failure.to_string())
            }
        }
    }
    pub async fn expire_command(&self, session: Session, id: String) -> Result<(), Error> {
        let delay = session.borrow().options.command_timeout;
        let mut stop = self.stop.clone();
        if *stop.borrow() != crate::web_sound::Shutdown::Running {
            return Ok(());
        }
        tokio::select! { _ = tokio::time::sleep(std::time::Duration::from_secs(delay)) => {}, _ = stop.changed() => return Ok(()) }
        let removed = {
            let mut session = session.borrow_mut();
            session
                .pending
                .iter()
                .position(|command| command.id == id)
                .map(|index| session.pending.remove(index))
        };
        if removed.is_some() {
            self.broadcast(Audience::All, named("command_expired", &id))
                .await;
        }
        Ok(())
    }
}
pub(super) fn replacement(value: &Value) -> Result<String, Error> {
    match value {
        Value::Text(points) => Ok(points
            .iter()
            .map(|point| char::from_u32(*point).unwrap_or('?'))
            .collect()),
        other => Ok(other.string()?),
    }
}
