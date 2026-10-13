use super::{
    model::{self, event, Guest, Permission, Session},
    Support,
};
use crate::{
    websocket::{self, Sender},
    Error, Value,
};
use std::{
    io::BufReader,
    net::TcpStream,
    sync::{atomic::Ordering, Arc},
};

impl Support {
    pub fn owner_socket(&self, mut reader: BufReader<TcpStream>, key: &str) -> Result<(), Error> {
        let sender = Sender::new(reader.get_ref().try_clone()?)?;
        sender.handshake(key)?;
        let Some(session) = self.current()? else {
            let _sent = sender.send_json(&event("session_status", model::idle()));
            sender.close();
            return Ok(());
        };
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        {
            let mut data = session.data()?;
            if session.stopped.load(Ordering::SeqCst) {
                sender.close();
                return Ok(());
            }
            data.owners.insert(id, Arc::clone(&sender));
        }
        let result = || -> Result<(), Error> {
            let _sent = sender.send_json(&event("session_status", session.snapshot()?));
            let count = session.data()?.guests.len();
            session.owners(&Value::object([
                ("type", Value::text("guest_presence")),
                ("count", Value::integer(count)),
            ]))?;
            session.guests(&Value::object([
                ("type", Value::text("owner_presence")),
                ("active", Value::Bool(true)),
            ]))?;
            while let Ok(message) = websocket::read_message(&mut reader, &sender) {
                let Ok(payload) = Value::parse(&message) else {
                    continue;
                };
                if session.stopped.load(Ordering::SeqCst) {
                    break;
                }
                if payload.get("type").text_eq("refresh") {
                    let _sent = sender.send_json(&event("session_status", session.snapshot()?));
                } else if payload.get("type").text_eq("typing") {
                    session.guests(&Value::object([
                        ("type", Value::text("host_typing")),
                        ("active", Value::Bool(payload.get("active").truth())),
                        (
                            "text",
                            Value::text(&super::commands::typing(payload.get("text"))?),
                        ),
                    ]))?;
                }
            }
            Ok(())
        }();
        session.data()?.owners.remove(&id);
        let active = !session.data()?.owners.is_empty();
        session.guests(&Value::object([
            ("type", Value::text("owner_presence")),
            ("active", Value::Bool(active)),
        ]))?;
        sender.close();
        result
    }
    pub(super) fn guest_socket(
        &self,
        mut reader: BufReader<TcpStream>,
        key: &str,
        sid: &str,
    ) -> Result<(), Error> {
        let sender = Sender::new(reader.get_ref().try_clone()?)?;
        sender.handshake(key)?;
        let Some(session) = self
            .current()?
            .filter(|session| session.id == sid && !session.expired())
        else {
            let _sent = sender.send_json(&Value::object([
                ("type", Value::text("error")),
                ("message", Value::text("session unavailable")),
            ]));
            sender.close();
            return Ok(());
        };
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let mut authenticated = false;
        let result = || -> Result<(), Error> {
            while let Ok(message) = websocket::read_message(&mut reader, &sender) {
                let Ok(payload) = Value::parse(&message) else {
                    continue;
                };
                if session.stopped.load(Ordering::SeqCst) {
                    break;
                }
                if !authenticated {
                    if !payload.get("type").text_eq("auth") {
                        continue;
                    }
                    let pin = model::text(payload.get("pin"))?;
                    if pin.trim() != session.pin {
                        let mut data = session.data()?;
                        data.failures = data.failures.saturating_add(1);
                        let _sent = sender.send_json(&Value::object([
                            ("type", Value::text("auth_failed")),
                            (
                                "remaining",
                                Value::integer(5_u32.saturating_sub(data.failures)),
                            ),
                        ]));
                        if data.failures >= 5 {
                            break;
                        }
                        continue;
                    }
                    {
                        let mut data = session.data()?;
                        if session.stopped.load(Ordering::SeqCst) {
                            break;
                        }
                        data.guests.insert(
                            id,
                            Guest {
                                sender: Arc::clone(&sender),
                                terminal: None,
                            },
                        );
                        if session.permission == Permission::AllowAll
                            && data
                                .controller
                                .and_then(|id| data.guests.get(&id))
                                .is_none_or(|guest| guest.sender.is_closed())
                        {
                            data.controller = Some(id);
                        }
                        let _sent = sender.send_json(&Value::object([
                            ("type", Value::text("auth_ok")),
                            ("permission_mode", Value::text(session.permission.name())),
                            ("command_timeout_seconds", Value::integer(session.timeout)),
                            ("expires_in", session.remaining()),
                            ("owner_present", Value::Bool(!data.owners.is_empty())),
                            (
                                "control_granted",
                                Value::Bool(
                                    session.permission != Permission::AllowAll
                                        || data.controller == Some(id),
                                ),
                            ),
                        ]));
                    }
                    match self.terminal.attach(Arc::clone(&sender), false) {
                        Ok(terminal) => {
                            if let Some(guest) = session.data()?.guests.get_mut(&id) {
                                guest.terminal = Some(terminal);
                            } else {
                                self.terminal.detach(terminal)?;
                                break;
                            }
                        }
                        Err(error) => {
                            let _sent = sender.send_json(&Value::object([
                                ("type", Value::text("error")),
                                ("message", Value::text(&error.to_string())),
                            ]));
                            break;
                        }
                    }
                    authenticated = true;
                    self.presence(&session)?;
                    self.roles(&session)?;
                    continue;
                }
                if !self.guest_message(&session, id, &sender, &payload)? {
                    break;
                }
            }
            Ok(())
        }();
        let removed = session.data()?.guests.remove(&id);
        if let Some(terminal) = removed.and_then(|guest| guest.terminal) {
            self.terminal.detach(terminal)?;
        }
        self.roles(&session)?;
        self.presence(&session)?;
        sender.close();
        result
    }
    fn presence(&self, session: &Session) -> Result<(), Error> {
        let count = session.data()?.guests.len();
        session.owners(&Value::object([
            ("type", Value::text("guest_presence")),
            ("count", Value::integer(count)),
        ]))
    }
    fn roles(&self, session: &Session) -> Result<(), Error> {
        let mut data = session.data()?;
        match session.permission {
            Permission::ApproveEach => data.controller = None,
            Permission::AllowAll => {
                if data
                    .controller
                    .and_then(|id| data.guests.get(&id))
                    .is_none_or(|guest| guest.sender.is_closed())
                {
                    data.controller = data
                        .guests
                        .iter()
                        .find(|(_, guest)| !guest.sender.is_closed())
                        .map(|(id, _)| *id);
                }
            }
        }
        for (id, guest) in &data.guests {
            let _sent = guest.sender.send_json(&Value::object([
                ("type", Value::text("control_role")),
                (
                    "granted",
                    Value::Bool(
                        session.permission != Permission::AllowAll || data.controller == Some(*id),
                    ),
                ),
            ]));
        }
        Ok(())
    }
}
