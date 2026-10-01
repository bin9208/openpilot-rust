use crate::Error;
use dbus::{
    arg::{AppendAll, PropMap, ReadAll, RefArg, Variant},
    channel::{BusType, Channel, Token},
    nonblock::{NonblockReply, SyncConnection},
    Message,
};
use std::sync::Arc;
use tokio::sync::oneshot;

pub const NM: &str = "org.freedesktop.NetworkManager";
pub const NM_PATH: &str = "/org/freedesktop/NetworkManager";
pub const SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
pub const SETTINGS: &str = "org.freedesktop.NetworkManager.Settings";
pub const CONNECTION: &str = "org.freedesktop.NetworkManager.Settings.Connection";
pub const ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
pub const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
pub const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
pub const ACCESS_POINT: &str = "org.freedesktop.NetworkManager.AccessPoint";
pub const IPV4: &str = "org.freedesktop.NetworkManager.IP4Config";
pub const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
pub type Value = Variant<Box<dyn RefArg>>;
pub type Settings = std::collections::HashMap<String, PropMap>;

#[derive(Clone, Copy)]
pub struct Address<'a> {
    pub path: &'a str,
    pub interface: &'a str,
}

#[derive(Clone)]
pub struct Peer {
    pub connection: Arc<SyncConnection>,
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
        Ok((Self { connection }, resource))
    }
    pub async fn call<A: AppendAll, R: ReadAll>(
        &self,
        address: Address<'_>,
        method: &str,
        args: A,
    ) -> Result<R, Error> {
        let mut request = Message::new_method_call(NM, address.path, address.interface, method)
            .map_err(Error::Request)?;
        request.append_all(args);
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
            .map_err(|()| Error::Stopped)?;
        let _pending = Pending {
            connection: Arc::clone(&self.connection),
            token,
        };
        let reply = receiver.await.map_err(|_| Error::Stopped)?;
        reply.read_all().map_err(Error::Reply)
    }
    pub async fn get(&self, address: Address<'_>, property: &str) -> Result<Value, Error> {
        let (value,) = self
            .call(
                Address {
                    path: address.path,
                    interface: PROPERTIES,
                },
                "Get",
                (address.interface, property),
            )
            .await?;
        Ok(value)
    }
    pub async fn properties(&self, address: Address<'_>) -> Result<PropMap, Error> {
        let (properties,) = self
            .call(
                Address {
                    path: address.path,
                    interface: PROPERTIES,
                },
                "GetAll",
                (address.interface,),
            )
            .await?;
        Ok(properties)
    }
}

pub fn text(value: &Value, name: &'static str) -> Result<String, Error> {
    value
        .0
        .as_str()
        .map(str::to_owned)
        .ok_or(Error::Property(name))
}
pub fn unsigned(value: &Value, name: &'static str) -> Result<u32, Error> {
    value
        .0
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(Error::Property(name))
}
pub fn paths(value: &Value, name: &'static str) -> Result<Vec<String>, Error> {
    value
        .0
        .as_iter()
        .ok_or(Error::Property(name))?
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or(Error::Property(name))
        })
        .collect()
}
pub fn bytes(value: &Value, name: &'static str) -> Result<Vec<u8>, Error> {
    value
        .0
        .as_iter()
        .ok_or(Error::Property(name))?
        .map(|item| {
            item.as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .ok_or(Error::Property(name))
        })
        .collect()
}
