use super::{
    model::{self, Command, Permission, Session},
    Support,
};
use crate::{websocket::Sender, Error, Value};
use std::sync::Arc;

impl Support {
    pub fn decide(&self, id: &str, approve: bool) -> Result<Value, Error> {
        let Some(session) = self.current()? else {
            return Ok(model::failure("no active session"));
        };
        let command = {
            let mut data = session.data()?;
            let Some(index) = data.pending.iter().position(|command| command.id == id) else {
                return Ok(model::failure("command not pending"));
            };
            data.pending.remove(index)
        };
        session.all(&Value::object([
            (
                "type",
                Value::text(if approve {
                    "command_approved"
                } else {
                    "command_rejected"
                }),
            ),
            ("id", Value::text(id)),
        ]))?;
        if approve {
            self.run_command(&session, &command)
        } else {
            Ok(Value::object([("ok", Value::Bool(true))]))
        }
    }
    pub(super) fn queue(
        &self,
        session: &Session,
        sender: &Sender,
        line: &str,
        action: &str,
    ) -> Result<(), Error> {
        if session.data()?.owners.is_empty() {
            let _sent = sender.send_json(&Value::object([("type", Value::text("owner_absent"))]));
            return Ok(());
        }
        let command = Command {
            id: model::token::<8>()?,
            line: line.chars().take(4000).collect(),
            created: model::now(),
            action: action.into(),
        };
        match session.permission {
            Permission::AllowAll => {
                let _sent = sender.send_json(&Value::object([
                    ("type", Value::text("command_approved")),
                    ("id", Value::text(&command.id)),
                ]));
                self.run_command(session, &command)?;
                session.owners(&Value::object([
                    ("type", Value::text("command_auto_run")),
                    ("id", Value::text(&command.id)),
                    ("line", Value::text(&command.line)),
                    ("created_at", Value::Float(command.created)),
                ]))?;
            }
            Permission::ApproveEach => {
                session.data()?.pending.push(command.clone());
                session.owners(&Value::object([
                    ("type", Value::text("command_request")),
                    ("id", Value::text(&command.id)),
                    ("line", Value::text(&command.line)),
                    ("created_at", Value::Float(command.created)),
                    ("control_action", Value::text(action)),
                ]))?;
                let _sent = sender.send_json(&Value::object([
                    ("type", Value::text("command_waiting_approval")),
                    ("id", Value::text(&command.id)),
                ]));
                session.wake.notify_all();
            }
        }
        Ok(())
    }
    fn run_command(&self, session: &Session, command: &Command) -> Result<Value, Error> {
        let result = match command.action.as_str() {
            "ctrl_c" => self.terminal.write(b"\x03"),
            "clear" => self
                .terminal
                .clear_history()
                .and_then(|()| self.terminal.write(b"clear\r")),
            _ => self.terminal.write_text(&format!("{}\r", command.line)),
        };
        match result {
            Ok(()) => {
                session.all(&Value::object([
                    ("type", Value::text("command_running")),
                    ("id", Value::text(&command.id)),
                ]))?;
                Ok(Value::object([("ok", Value::Bool(true))]))
            }
            Err(error) => {
                session.all(&Value::object([
                    ("type", Value::text("command_failed")),
                    ("id", Value::text(&command.id)),
                    ("message", Value::text(&error.to_string())),
                ]))?;
                Ok(model::failure(&error.to_string()))
            }
        }
    }
    pub(super) fn guest_message(
        &self,
        session: &Arc<Session>,
        id: u64,
        sender: &Sender,
        payload: &Value,
    ) -> Result<bool, Error> {
        let kind = model::text(payload.get("type"))?;
        match kind.as_str() {
            "typing" => session.owners(&Value::object([
                ("type", Value::text("guest_typing")),
                ("active", Value::Bool(payload.get("active").truth())),
                ("text", Value::text(&typing(payload.get("text"))?)),
            ]))?,
            "close_session" => {
                self.stop("guest")?;
                return Ok(false);
            }
            "disconnect" => return Ok(false),
            "control" | "input" => {
                if session.permission == Permission::AllowAll
                    && session.data()?.controller != Some(id)
                {
                    denied(sender, "viewer only");
                    return Ok(true);
                }
                if kind == "control" {
                    let action = model::text(payload.get("action"))?;
                    let action = action.trim();
                    if matches!(action, "ctrl_c" | "clear") {
                        self.queue(
                            session,
                            sender,
                            if action == "ctrl_c" {
                                "Ctrl+C"
                            } else {
                                "clear"
                            },
                            action,
                        )?;
                    }
                } else {
                    let line = model::text(payload.get("data"))?;
                    if !line.trim().is_empty() {
                        self.queue(session, sender, line.trim(), "")?;
                    }
                }
            }
            "raw" => {
                let data = session.data()?;
                if data.owners.is_empty() {
                    let _sent =
                        sender.send_json(&Value::object([("type", Value::text("owner_absent"))]));
                } else if session.permission != Permission::AllowAll {
                    denied(sender, "approval required");
                } else if data.controller != Some(id) {
                    denied(sender, "viewer only");
                } else {
                    drop(data);
                    let text: String = model::text(payload.get("data"))?
                        .chars()
                        .take(4096)
                        .collect();
                    if let Err(error) = self.terminal.write_text(&text) {
                        let _sent = sender.send_json(&Value::object([
                            ("type", Value::text("error")),
                            ("message", Value::text(&error.to_string())),
                        ]));
                    }
                }
            }
            _ => {}
        }
        Ok(true)
    }
}
fn denied(sender: &Sender, reason: &str) {
    let _sent = sender.send_json(&Value::object([
        ("type", Value::text("input_denied")),
        ("reason", Value::text(reason)),
    ]));
}
pub(super) fn typing(value: &Value) -> Result<String, Error> {
    let points = match value {
        Value::Text(points) => points.clone(),
        _ => model::text(value)?.chars().map(u32::from).collect(),
    };
    Ok(points
        .into_iter()
        .filter(|point| openpilot_carrot_navi::json::is_printable(*point))
        .filter_map(char::from_u32)
        .take(160)
        .collect())
}
