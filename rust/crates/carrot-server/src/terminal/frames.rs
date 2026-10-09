use super::Client;
use crate::{web_sound::Shutdown, Error, Value};
use base64::Engine;
use futures_util::SinkExt;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;

pub(super) fn output(bytes: &[u8], replay: bool) -> Value {
    let mut fields = vec![
        ("type", Value::text("pty_output")),
        ("session", Value::text("login-shell")),
        (
            "b64",
            Value::text(&base64::engine::general_purpose::STANDARD.encode(bytes)),
        ),
    ];
    if replay {
        fields.push(("replay", Value::Bool(true)));
    }
    Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.chars().map(u32::from).collect(), value))
            .collect(),
    )
}
pub(super) async fn send(
    client: &Client,
    payload: Value,
    stop: &mut watch::Receiver<Shutdown>,
) -> Result<(), Error> {
    let message = Message::text(payload.encode()?);
    let operation = async {
        client
            .sink
            .lock()
            .await
            .send(message)
            .await
            .map_err(|error| Error::Source(error.to_string()))
    };
    tokio::pin!(operation);
    loop {
        if *stop.borrow() == Shutdown::Force {
            return Err(Error::Source("terminal owner stopping".into()));
        }
        tokio::select! { result = &mut operation => return result, changed = stop.changed() => if changed.is_err() { return Err(Error::Source("terminal owner stopping".into())); } }
    }
}
