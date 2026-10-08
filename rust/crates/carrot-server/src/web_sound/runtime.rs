use super::{socket, wire, Input, Policy, Settings};
use crate::{Error, Value};
use futures_util::{SinkExt, StreamExt};
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use openpilot_messaging::{runtime::SubMaster, state::Options};
use openpilot_params::Params;
use std::{sync::Arc, time::Duration};
use tokio::{sync::Mutex, task::JoinSet};
use tokio_tungstenite::{
    tungstenite::protocol::{Role, WebSocketConfig},
    WebSocketStream,
};

pub struct Context {
    pub params: Option<Params>,
    pub tizi: bool,
    pub shutdown: tokio::sync::watch::Receiver<bool>,
}

fn text(value: &Value) -> Result<String, Error> {
    let Value::Object(fields) = value else {
        return Err(Error::Source("expected sound state object".into()));
    };
    let fields = fields
        .iter()
        .map(|(key, value)| {
            Ok(format!(
                "{}: {}",
                Value::Text(key.clone()).encode()?,
                value.encode()?
            ))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(format!("{{{}}}", fields.join(", ")))
}

async fn sender(sink: socket::Sink, params: Option<Params>, tizi: bool) -> Result<(), Error> {
    let mut sm = SubMaster::for_runtime(
        &["selfdriveState", "carrotMan", "carState"],
        Options::default(),
    )
    .map_err(|error| Error::Source(error.to_string()))?;
    let params = params.ok_or_else(|| Error::Source("Params unavailable".into()))?;
    let mut policy = Policy::new(tizi);
    loop {
        sm.update(Duration::ZERO)
            .map_err(|error| Error::Source(error.to_string()))?;
        let state = wire::snapshot(&sm.state)?;
        let settings = if policy.params_due(state.now) {
            let params = params.clone();
            Some(
                tokio::task::spawn_blocking(move || Settings::read(&params))
                    .await
                    .map_err(|error| Error::Source(error.to_string()))?,
            )
        } else {
            None
        };
        let output = policy.step(Input {
            now: state.now,
            received: state.received,
            enabled: state.enabled,
            alert: state.alert,
            valid: state.valid,
            updated: state.updated,
            countdown: state.countdown,
            countdown_valid: state.countdown_valid,
            countdown_updated: state.countdown_updated,
            car_valid: state.car_valid,
            buttons: &state.buttons,
            settings,
        });
        if let Some(output) = output {
            sink.lock()
                .await
                .send(tokio_tungstenite::tungstenite::Message::text(text(
                    &output,
                )?))
                .await
                .map_err(|error| Error::Source(error.to_string()))?;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Run under a LocalSet; the caller owns the HTTP upgrade and shutdown tracking.
pub async fn run(upgraded: Upgraded, context: Context) -> Result<(), Error> {
    let config = WebSocketConfig::default()
        .max_message_size(Some(65535))
        .max_frame_size(None)
        .reply_with_normal_close(true);
    let socket =
        WebSocketStream::from_raw_socket(TokioIo::new(upgraded), Role::Server, Some(config)).await;
    let (sink, stream) = socket.split();
    let sink = Arc::new(Mutex::new(sink));
    let mut tasks = JoinSet::new();
    tasks.spawn_local(sender(Arc::clone(&sink), context.params, context.tizi));
    let result = socket::receive(Arc::clone(&sink), stream, context.shutdown).await;
    tasks.abort_all();
    // Source sender errors are observed only in finally and do not close the receiver.
    while tasks.join_next().await.is_some() {}
    socket::finish(sink, result).await
}
