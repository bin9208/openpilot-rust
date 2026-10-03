use super::Error;
use dbus::{
    arg::{AppendAll, ReadAll},
    channel::{BusType, Channel, Token},
    nonblock::{NonblockReply, SyncConnection},
    Message,
};
use openpilot_logmessaged::JsonValue;
use std::{sync::Arc, time::Duration};
use tokio::sync::{oneshot, watch};

#[derive(Clone, Copy)]
pub(super) struct Endpoint<'a> {
    pub path: &'a str,
    pub interface: &'a str,
    pub method: &'a str,
    pub seconds: u64,
}

#[derive(Clone)]
pub(super) struct Peer {
    pub connection: Arc<SyncConnection>,
    disconnected: watch::Receiver<bool>,
}

struct Pending {
    connection: Arc<SyncConnection>,
    token: Token,
}
impl Drop for Pending {
    fn drop(&mut self) {
        drop(self.connection.cancel_reply(self.token));
    }
}

impl Peer {
    pub fn open(
        address: Option<&str>,
        disconnected: watch::Receiver<bool>,
    ) -> Result<(Self, dbus_tokio::connection::IOResource<SyncConnection>), Error> {
        let channel = match address {
            Some(address) => {
                let mut channel = Channel::open_private(address)?;
                channel.register()?;
                channel
            }
            None => Channel::get_private(BusType::System)?,
        };
        let (resource, connection) = dbus_tokio::connection::from_channel(channel)?;
        Ok((
            Self {
                connection,
                disconnected,
            },
            resource,
        ))
    }

    pub async fn send(&self, request: Message, timeout: Duration) -> Result<Message, Error> {
        let (sender, receiver) = oneshot::channel();
        let token = self
            .connection
            .send_with_reply(
                request,
                SyncConnection::make_f(move |message, _| {
                    if let Err(undelivered) = sender.send(message) {
                        drop(undelivered);
                    }
                }),
            )
            .map_err(|()| Error::Closed)?;
        let _pending = Pending {
            connection: Arc::clone(&self.connection),
            token,
        };
        let mut disconnected = self.disconnected.clone();
        let received = tokio::time::timeout(timeout, async {
            tokio::select! {
                biased;
                reply = receiver => reply.map_err(|_| Error::Closed),
                _ = disconnected.wait_for(|closed| *closed) => Err(Error::Closed),
            }
        })
        .await
        .map_err(|_| Error::Timeout)??;
        check_reply(received)
    }

    pub async fn message<A: AppendAll>(
        &self,
        endpoint: Endpoint<'_>,
        args: A,
    ) -> Result<Message, Error> {
        let mut message = Message::new_method_call(
            "org.bluez",
            dbus::Path::new(endpoint.path).map_err(Error::Request)?,
            endpoint.interface,
            endpoint.method,
        )
        .map_err(Error::Request)?;
        message.append_all(args);
        self.send(message, Duration::from_secs(endpoint.seconds))
            .await
    }

    pub async fn call<A: AppendAll, R: ReadAll>(
        &self,
        endpoint: Endpoint<'_>,
        args: A,
    ) -> Result<R, Error> {
        Ok(self.message(endpoint, args).await?.read_all()?)
    }

    pub async fn owner(&self) -> Result<String, Error> {
        let message = Message::new_method_call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetNameOwner",
        )
        .map_err(Error::Request)?
        .append1("org.bluez");
        Ok(self.send(message, Duration::from_secs(5)).await?.read1()?)
    }
}

fn check_reply(mut message: Message) -> Result<Message, Error> {
    if let Err(error) = message.as_result() {
        let strings: Vec<String> = message
            .iter_init()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or(Error::Property("error body text"))
            })
            .collect::<Result<_, _>>()?;
        let json = JsonValue::parse(&serde_json::to_string(&strings)?)?;
        let points = openpilot_runtime_version::python_str(&json)?;
        let text: String = points
            .into_iter()
            .map(char::from_u32)
            .collect::<Option<_>>()
            .ok_or_else(|| Error::Request("invalid error text".into()))?;
        let body = text
            .get(1..text.len().saturating_sub(1))
            .ok_or_else(|| Error::Request("invalid error tuple".into()))?;
        return Err(Error::Remote {
            name: error.name().unwrap_or("None").to_owned(),
            body: format!("({}{})", body, if strings.len() == 1 { "," } else { "" }),
        });
    }
    Ok(message)
}
