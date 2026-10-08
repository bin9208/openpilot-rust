use super::{
    clock::NativeClock,
    shared::Shared,
    socket::{self, Socket},
};
use crate::{json::Value, receiver::Stream, Error};
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone)]
pub enum Route {
    Control(String),
    Item {
        kind: &'static str,
        session: String,
        name: String,
    },
}

pub struct Session {
    pub route: Route,
    pub peer: String,
}

fn object(text: &str) -> Result<Value, Error> {
    let value = Value::parse(text)?;
    if matches!(value, Value::Object(_)) {
        Ok(value)
    } else {
        Err(Error::value("message must be a JSON object"))
    }
}

fn entropy() -> Result<String, Error> {
    let mut token = [0; 8];
    getrandom::fill(&mut token).map_err(|error| Error::typed("OSError", error.to_string()))?;
    Ok(token.iter().map(|byte| format!("{byte:02x}")).collect())
}

impl Session {
    pub fn control(&self) -> bool {
        matches!(self.route, Route::Control(_))
    }
    pub fn recoverable(&self, error: &Error) -> bool {
        matches!(
            error.kind,
            "TypeError" | "ValueError" | "JSONDecodeError" | "UnicodeEncodeError"
        ) || (matches!(
            self.route,
            Route::Item {
                kind: "image" | "render",
                ..
            }
        ) && error.kind == "KeyError")
    }
    pub fn message(&self, message: Message, shared: &Shared) -> Result<Option<Value>, Error> {
        match &self.route {
            Route::Control(version) => {
                let Message::Text(text) = message else {
                    return Err(Error::value("v2 control accepts JSON text only"));
                };
                let payload = object(&text)?;
                shared.with(|receiver| {
                    if payload.get("type").text_eq("requirements_query") {
                        receiver.negotiate(&payload, version, entropy).map(Some)
                    } else {
                        receiver.record_control(&payload, &self.peer).map(|()| None)
                    }
                })?
            }
            Route::Item {
                kind: "json",
                session,
                name,
            } => {
                let Message::Text(text) = message else {
                    return Err(Error::value("v2 JSON item stream accepts text only"));
                };
                let payload = object(&text)?;
                shared.with(|receiver| {
                    receiver.record_json(
                        session,
                        name,
                        &payload,
                        &self.peer,
                        &mut NativeClock,
                        &mut std::io::stdout().lock(),
                    )
                })??;
                Ok(None)
            }
            Route::Item {
                kind,
                session,
                name,
            } => {
                let Message::Binary(bytes) = message else {
                    return Err(Error::value(
                        "v2 binary item stream accepts binary messages only",
                    ));
                };
                let (metadata, payload) = crate::packet::parse(&bytes)?;
                shared.with(|receiver| {
                    receiver.record_binary(
                        Stream {
                            session,
                            kind,
                            name,
                            peer: &self.peer,
                        },
                        &metadata.value(),
                        payload,
                        &mut NativeClock,
                    )
                })??;
                Ok(None)
            }
        }
    }
    pub async fn reject(
        &self,
        socket: &mut Socket,
        shared: &Shared,
        error: &Error,
    ) -> Result<(), Error> {
        shared.with(|receiver| receiver.fail(&error.message_value(), &self.peer))??;
        let message = match error.message_value() {
            Value::Text(points) => Value::Text(points.into_iter().take(256).collect()),
            _ => return Err(Error::value("invalid protocol error text")),
        };
        let (code, kind, name) = match &self.route {
            Route::Control(_) => ("invalid_control_message", None, None),
            Route::Item {
                kind: "json", name, ..
            } => ("json_stream_error", Some("json"), Some(name.as_str())),
            Route::Item { kind, name, .. } => {
                ("binary_stream_error", Some(*kind), Some(name.as_str()))
            }
        };
        let mut reply = Value::object([
            ("type", Value::text("protocol_error")),
            ("protocol_version", Value::integer(2)),
            ("code", Value::text(code)),
            ("recoverable", Value::Bool(true)),
            ("message", message),
        ]);
        if let (Some(kind), Some(name), Value::Object(fields)) = (kind, name, &mut reply) {
            fields.extend([
                ("kind".chars().map(u32::from).collect(), Value::text(kind)),
                ("name".chars().map(u32::from).collect(), Value::text(name)),
            ]);
        }
        let label = match &self.route {
            Route::Control(_) => "control".into(),
            Route::Item { kind, name, .. } => format!("{kind}:{name}"),
        };
        if !(self.control() && error.message == "v2 control accepts JSON text only") {
            println!(
                "[carrot_navi][WS] {label} rejected peer={}: {error}",
                self.peer
            );
        }
        socket::send(socket, &reply).await?;
        match &self.route {
            Route::Control(_) => (),
            Route::Item { kind: "json", .. } => {
                socket::close(socket, 1008, "invalid JSON stream").await
            }
            Route::Item { .. } => socket::close(socket, 1008, "invalid binary stream").await,
        }
        Ok(())
    }
    pub async fn run(self, mut socket: Socket, shared: Shared) {
        let result = match &self.route {
            Route::Control(version) => {
                let _ = shared.with(|receiver| receiver.control_connected());
                println!(
                    "[carrot_navi][WS] control connected peer={} app={version}",
                    self.peer
                );
                socket::receive(&mut socket, &self, &shared).await
            }
            Route::Item {
                kind,
                session,
                name,
            } => match shared
                .with(|receiver| receiver.stream_config(session, kind, name, &Value::Null))
            {
                Ok(Ok(_)) => socket::receive(&mut socket, &self, &shared).await,
                Ok(Err(error)) | Err(error) => self.reject(&mut socket, &shared, &error).await,
            },
        };
        if let Err(error) = result {
            eprintln!("{}: {error}", error.kind);
        }
        if let Route::Control(version) = &self.route {
            let _ = shared.with(|receiver| receiver.control_disconnected());
            println!(
                "[carrot_navi][WS] control disconnected peer={} app={version}",
                self.peer
            );
        }
    }
}
