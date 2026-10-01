use super::{
    agent, objects,
    peer::{Endpoint, Peer},
    state::{Pair, Shared},
    Adapter, Device, Error, Prompt, ADAPTER, DEVICE,
};
use crate::Address;
use dbus::Message;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::{sync::watch, task::JoinSet};

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Connect,
    Disconnect,
    Forget,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub adapters: Vec<Adapter>,
    pub devices: Vec<Device>,
    pub pair: Pair,
    pub prompt: Option<Prompt>,
}

pub(super) struct Connection {
    pub(super) peer: Peer,
    agent: agent::Server,
    transport: JoinSet<()>,
}

pub struct Bluez {
    address: Option<String>,
    pub(super) connection: Option<Connection>,
    pub(super) shared: Shared,
    pub(super) registered: bool,
    pub(super) pair: JoinSet<Result<(), Error>>,
    scan: JoinSet<Result<(), Error>>,
    scan_stop: Option<watch::Sender<bool>>,
}

impl Bluez {
    pub fn new(address: Option<String>) -> Self {
        Self {
            address,
            connection: None,
            shared: Shared::default(),
            registered: false,
            pair: JoinSet::new(),
            scan: JoinSet::new(),
            scan_stop: None,
        }
    }

    pub(super) async fn ensure(&mut self) -> Result<Peer, Error> {
        if self.connection.is_none() {
            let (closed, disconnected) = watch::channel(false);
            let address = self.address.clone();
            let (peer, resource) =
                tokio::task::spawn_blocking(move || Peer::open(address.as_deref(), disconnected))
                    .await??;
            let mut transport = JoinSet::new();
            transport.spawn(async move {
                let error = resource.await;
                closed.send_replace(true);
                eprintln!("Bluetooth D-Bus transport: {error}");
            });
            let agent = agent::Server::start(peer.clone(), self.shared.clone())?;
            self.connection = Some(Connection {
                peer,
                agent,
                transport,
            });
        }
        self.connection
            .as_ref()
            .map(|connection| connection.peer.clone())
            .ok_or(Error::Closed)
    }

    async fn objects(&mut self) -> Result<Vec<objects::Object>, Error> {
        let peer = self.ensure().await?;
        let message = Message::new_method_call(
            "org.bluez",
            "/",
            "org.freedesktop.DBus.ObjectManager",
            "GetManagedObjects",
        )
        .map_err(Error::Request)?;
        objects::read(&peer.send(message, Duration::from_secs(10)).await?)
    }

    pub(super) async fn locate(&mut self, address: &Address) -> Result<dbus::Path<'static>, Error> {
        for (path, interfaces) in self.objects().await? {
            if objects::address(&interfaces)?
                .is_some_and(|found| found.to_uppercase() == address.as_str())
            {
                return Ok(path);
            }
        }
        Err(Error::Device)
    }

    pub async fn snapshot(&mut self) -> Result<Snapshot, Error> {
        let (adapters, devices) = objects::snapshot(&self.objects().await?)?;
        let state = self.shared.lock()?;
        Ok(Snapshot {
            adapters,
            devices,
            pair: state.pair.clone(),
            prompt: state.prompt.clone(),
        })
    }

    pub async fn scan(&mut self) -> Result<(), Error> {
        if let Some(done) = self.scan.try_join_next() {
            done??;
        }
        if !self.scan.is_empty() {
            return Ok(());
        }
        let path = self
            .objects()
            .await?
            .into_iter()
            .find(|(_, interfaces)| interfaces.contains_key(ADAPTER))
            .map(|(path, _)| path)
            .ok_or(Error::Adapter)?;
        let peer = self.ensure().await?;
        peer.call::<_, ()>(
            Endpoint {
                path: &path,
                interface: ADAPTER,
                method: "StartDiscovery",
                seconds: 10,
            },
            (),
        )
        .await?;
        let (stop, mut stopping) = watch::channel(false);
        self.scan_stop = Some(stop);
        self.scan.spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(30)) => {},
                _ = stopping.wait_for(|stop| *stop) => {},
            }
            if let Err(error) = peer
                .call::<_, ()>(
                    Endpoint {
                        path: &path,
                        interface: ADAPTER,
                        method: "StopDiscovery",
                        seconds: 10,
                    },
                    (),
                )
                .await
            {
                eprintln!("Bluetooth StopDiscovery: {error}");
            }
            Ok(())
        });
        Ok(())
    }

    pub async fn device_action(&mut self, address: &Address, action: Action) -> Result<(), Error> {
        let path = self.locate(address).await?;
        let peer = self.ensure().await?;
        match action {
            Action::Forget => {
                let parent = path
                    .rsplit_once('/')
                    .map(|(parent, _)| parent)
                    .ok_or(Error::Property("device parent"))?;
                peer.call(
                    Endpoint {
                        path: parent,
                        interface: ADAPTER,
                        method: "RemoveDevice",
                        seconds: 10,
                    },
                    (&path,),
                )
                .await
            }
            Action::Connect | Action::Disconnect => {
                let method = match action {
                    Action::Connect => "Connect",
                    Action::Disconnect => "Disconnect",
                    Action::Forget => unreachable!(),
                };
                peer.call(
                    Endpoint {
                        path: &path,
                        interface: DEVICE,
                        method,
                        seconds: 30,
                    },
                    (),
                )
                .await
            }
        }
    }

    pub async fn close(&mut self) -> Result<(), Error> {
        self.cancel_pair().await?;
        if let Some(stop) = self.scan_stop.take() {
            stop.send_replace(true);
        }
        while let Some(done) = self.scan.join_next().await {
            done??;
        }
        if let Some(mut connection) = self.connection.take() {
            connection.agent.close().await?;
            connection.transport.shutdown().await;
        }
        self.registered = false;
        Ok(())
    }
}
