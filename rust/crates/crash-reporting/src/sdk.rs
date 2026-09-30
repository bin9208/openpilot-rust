//! Native Sentry transport with a narrow raw-envelope adapter for source JSON policy.
use crate::{normalize, Configuration, Error, NativeException, Sdk};
use openpilot_logmessaged::JsonValue;
use sentry::{
    protocol::{Event, Exception, Level},
    Client, ClientOptions, Envelope,
};
use std::sync::Arc;

#[derive(Default)]
pub struct NativeSdk {
    dsn_override: Option<String>,
    client: Option<Client>,
    configuration: Option<Configuration>,
    user: Option<Option<String>>,
    tags: Vec<(String, JsonValue)>,
    extra: Vec<(String, JsonValue)>,
}
impl NativeSdk {
    /// Explicit local capture override; runtime initialization otherwise uses the source project DSN.
    pub fn local_capture(dsn: &str) -> Result<Self, Error> {
        let parsed: sentry::types::Dsn = dsn.parse().map_err(|error| Error::Sdk {
            operation: "dsn",
            detail: format!("{error}"),
        })?;
        if !matches!(parsed.host(), "127.0.0.1" | "localhost" | "[::1]" | "::1") {
            return Err(Error::Sdk {
                operation: "dsn",
                detail: "local capture must use a loopback host".into(),
            });
        }
        Ok(Self {
            dsn_override: Some(dsn.into()),
            ..Self::default()
        })
    }
    fn send(&self, event: Event<'static>) -> Result<(), Error> {
        let (Some(client), Some(configuration)) = (&self.client, &self.configuration) else {
            return Ok(());
        };
        let Some(mut event) = client.prepare_event(event, None) else {
            return Ok(());
        };
        let id = event.event_id;
        let mut extras = std::mem::take(&mut event.extra)
            .into_iter()
            .map(|(key, value)| Ok(format!("{}:{}", serde_json::to_string(&key)?, value)))
            .collect::<Result<Vec<_>, Error>>()?;
        let mut body = serde_json::to_string(&event)?;
        body.pop();
        if let Some(user) = &self.user {
            body.push_str(&format!(",\"user\":{}", serde_json::json!({"id":user})));
        }
        if !self.tags.is_empty() {
            let tags = self
                .tags
                .iter()
                .map(|(key, value)| {
                    Ok(format!(
                        "{}:{}",
                        serde_json::to_string(key)?,
                        value.to_json()?
                    ))
                })
                .collect::<Result<Vec<_>, Error>>()?;
            body.push_str(&format!(",\"tags\":{{{}}}", tags.join(",")));
        }
        if !self.extra.is_empty() || !extras.is_empty() {
            extras.extend(
                self.extra
                    .iter()
                    .map(|(key, value)| {
                        Ok(format!(
                            "{}:{}",
                            serde_json::to_string(key)?,
                            value.to_json()?
                        ))
                    })
                    .collect::<Result<Vec<_>, Error>>()?,
            );
            body.push_str(&format!(",\"extra\":{{{}}}", extras.join(",")));
        }
        body.push('}');
        let body = normalize::serialize(&JsonValue::parse(&body)?, configuration.max_value_length)?;
        let envelope = format!(
            "{}\n{}\n{body}\n",
            serde_json::json!({"event_id":id.simple().to_string()}),
            serde_json::json!({"type":"event","length":body.len()})
        );
        let envelope =
            Envelope::from_bytes_raw(envelope.into_bytes()).map_err(|error| Error::Sdk {
                operation: "envelope",
                detail: error.to_string(),
            })?;
        client.send_envelope(envelope);
        Ok(())
    }
}
fn set<T>(fields: &mut Vec<(String, T)>, key: &str, value: T) {
    if let Some((_, current)) = fields.iter_mut().find(|(name, _)| name == key) {
        *current = value;
    } else {
        fields.push((key.into(), value));
    }
}
impl Sdk for NativeSdk {
    fn init(&mut self, configuration: Configuration) -> Result<(), Error> {
        let dsn = self
            .dsn_override
            .as_deref()
            .unwrap_or(configuration.project.dsn())
            .parse()
            .map_err(|error| Error::Sdk {
                operation: "dsn",
                detail: format!("{error}"),
            })?;
        let mut options = ClientOptions::new()
            .default_integrations(configuration.default_integrations)
            .traces_sample_rate(configuration.traces_sample_rate);
        options.dsn = Some(dsn);
        options.release = Some(configuration.release.clone().into());
        options.environment = Some(configuration.environment.into());
        options.transport = Some(Arc::new(sentry::transports::DefaultTransportFactory));
        self.client = Some(Client::from(options));
        self.configuration = Some(configuration);
        Ok(())
    }
    fn set_user(&mut self, id: Option<String>) -> Result<(), Error> {
        self.user = Some(id);
        Ok(())
    }
    fn set_tag(&mut self, key: &str, value: &JsonValue) -> Result<(), Error> {
        set(&mut self.tags, key, value.clone());
        Ok(())
    }
    fn set_extra(&mut self, key: &str, value: &JsonValue) -> Result<(), Error> {
        set(&mut self.extra, key, value.clone());
        Ok(())
    }
    fn capture_message(&mut self, message: &str) -> Result<(), Error> {
        self.send(Event {
            message: Some(message.into()),
            level: Level::Info,
            ..Event::default()
        })
    }
    fn capture_exception(&mut self, error: &NativeException) -> Result<(), Error> {
        let mut exception: Vec<_> = error
            .causes
            .iter()
            .rev()
            .map(|cause| Exception {
                ty: "RustErrorCause".into(),
                value: Some(cause.clone()),
                ..Exception::default()
            })
            .collect();
        exception.push(Exception {
            ty: error.kind.clone(),
            value: Some(error.message.clone()),
            ..Exception::default()
        });
        let mut event = Event {
            exception: exception.into(),
            level: Level::Error,
            ..Event::default()
        };
        if let Some(backtrace) = &error.backtrace {
            event.extra.insert(
                "rust_backtrace".into(),
                serde_json::Value::String(backtrace.clone()),
            );
        }
        self.send(event)
    }
    fn flush(&mut self) -> Result<(), Error> {
        if let Some(client) = &self.client {
            client.flush(None);
        }
        Ok(())
    }
}
