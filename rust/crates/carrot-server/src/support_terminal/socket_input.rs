use super::{
    commands::replacement,
    context::Audience,
    policy::{self, Permission},
    session::Connection,
};
use crate::{Error, Value};

impl Connection {
    pub async fn input(&mut self, value: Value) -> Result<bool, Error> {
        if !matches!(value, Value::Object(_)) {
            return Err(Error::Source(format!(
                "'{}' object has no attribute 'get'",
                value.type_name()
            )));
        }
        let context = &self.context;
        if self.owner {
            if value.get("type").text_eq("refresh") {
                context
                    .send(&self.client, super::context::status(&self.session))
                    .await?;
            } else if value.get("type").text_eq("typing") {
                context
                    .broadcast(
                        Audience::Guests,
                        Value::object([
                            ("type", Value::text("host_typing")),
                            ("active", Value::Bool(value.get("active").truth())),
                            ("text", Value::text(&policy::typing(value.get("text"))?)),
                        ]),
                    )
                    .await;
            }
            return Ok(true);
        }
        if !self.authed {
            if !value.get("type").text_eq("auth") {
                return Ok(true);
            }
            let pin = policy::stripped(value.get("pin"))?;
            let failures = {
                let mut session = self.session.borrow_mut();
                if pin == session.pin {
                    None
                } else {
                    session.failures = session.failures.saturating_add(1);
                    Some(session.failures)
                }
            };
            if let Some(failures) = failures {
                context
                    .send(
                        &self.client,
                        Value::object([
                            ("type", Value::text("auth_failed")),
                            (
                                "remaining",
                                Value::integer(5_usize.saturating_sub(failures)),
                            ),
                        ]),
                    )
                    .await?;
                return Ok(failures < 5);
            }
            self.authed = true;
            let auth = {
                let mut session = self.session.borrow_mut();
                session.guests.insert(self.id, self.client.clone());
                if session.options.permission == Permission::AllowAll
                    && !session.controller_present()
                {
                    session.controller = Some(self.id);
                }
                Value::object([
                    ("type", Value::text("auth_ok")),
                    (
                        "permission_mode",
                        Value::text(session.options.permission.name()),
                    ),
                    (
                        "command_timeout_seconds",
                        Value::integer(session.options.command_timeout),
                    ),
                    ("expires_in", session.remaining()),
                    ("owner_present", Value::Bool(!session.owners.is_empty())),
                    ("control_granted", Value::Bool(session.grant(self.id))),
                ])
            };
            context.send(&self.client, auth).await?;
            match context
                .config
                .terminal
                .attach_guest(crate::terminal::Client {
                    sink: self.client.sink.clone(),
                    finished: self.client.finished.clone(),
                    closed: self.client.closed.clone(),
                })
                .await
            {
                Ok(id) => {
                    self.client.terminal_id = Some(id);
                    self.session
                        .borrow_mut()
                        .guests
                        .insert(self.id, self.client.clone());
                }
                Err(error) => {
                    self.session.borrow_mut().guests.remove(&self.id);
                    context
                        .send(
                            &self.client,
                            Value::object([
                                ("type", Value::text("error")),
                                ("message", Value::text(&error.to_string())),
                            ]),
                        )
                        .await?;
                    return Ok(false);
                }
            }
            let count = self.session.borrow().guests.len();
            context
                .broadcast(
                    Audience::Owners,
                    Value::object([
                        ("type", Value::text("guest_presence")),
                        ("count", Value::integer(count)),
                    ]),
                )
                .await;
            context.roles(&self.session).await;
            return Ok(true);
        }
        let kind = value.get("type");
        if kind.text_eq("typing") {
            context
                .broadcast(
                    Audience::Owners,
                    Value::object([
                        ("type", Value::text("guest_typing")),
                        ("active", Value::Bool(value.get("active").truth())),
                        ("text", Value::text(&policy::typing(value.get("text"))?)),
                    ]),
                )
                .await;
        } else if kind.text_eq("close_session") {
            context.stop_session("guest").await?;
            return Ok(false);
        } else if kind.text_eq("disconnect") {
            return Ok(false);
        } else if kind.text_eq("control") || kind.text_eq("input") {
            if !self.session.borrow().grant(self.id) {
                self.denied("viewer only").await?;
                return Ok(true);
            }
            if kind.text_eq("control") {
                let action = policy::stripped(value.get("action"))?;
                if matches!(action.as_str(), "ctrl_c" | "clear") {
                    context
                        .queue(
                            &self.session,
                            &self.client,
                            Value::text(if action == "ctrl_c" {
                                "Ctrl+C"
                            } else {
                                "clear"
                            }),
                            action,
                        )
                        .await?;
                }
            } else {
                let line = crate::param_changes::text::stripped(value.get("data"), true)?;
                if line.truth() {
                    context
                        .queue(&self.session, &self.client, line, String::new())
                        .await?;
                }
            }
        } else if kind.text_eq("raw") {
            if self.session.borrow().owners.is_empty() {
                context
                    .send(
                        &self.client,
                        Value::object([("type", Value::text("owner_absent"))]),
                    )
                    .await?;
            } else if self.session.borrow().options.permission != Permission::AllowAll {
                self.denied("approval required").await?;
            } else if !self.session.borrow().grant(self.id) {
                self.denied("viewer only").await?;
            } else {
                let data = policy::clipped(policy::string(value.get("data"))?, 4096);
                if data.truth() {
                    if let Err(error) = context
                        .config
                        .terminal
                        .write(replacement(&data)?.into_bytes())
                        .await
                    {
                        context
                            .send(
                                &self.client,
                                Value::object([
                                    ("type", Value::text("error")),
                                    ("message", Value::text(&error.to_string())),
                                ]),
                            )
                            .await?;
                    }
                }
            }
        }
        Ok(true)
    }
    async fn denied(&self, reason: &str) -> Result<(), Error> {
        self.context
            .send(
                &self.client,
                Value::object([
                    ("type", Value::text("input_denied")),
                    ("reason", Value::text(reason)),
                ]),
            )
            .await
    }
}
