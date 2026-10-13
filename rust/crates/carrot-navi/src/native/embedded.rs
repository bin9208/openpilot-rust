use super::{
    http,
    publisher::Publisher,
    shared::{Close, Shared},
};
use crate::{
    json::Value,
    manifest::MapConfig,
    receiver::{Dashboard, Receiver},
    Error,
};
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use std::{
    net::{SocketAddr, TcpListener},
    thread::{self, JoinHandle},
    time::Duration,
};
use tokio::sync::oneshot;

pub struct EmbeddedServer {
    address: SocketAddr,
    shared: Shared,
    stop: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<Result<(), Error>>>,
    publisher: Option<Publisher>,
}

impl EmbeddedServer {
    pub fn start(host: &str, port: u16, config: MapConfig, publish: bool) -> Result<Self, Error> {
        let listener = TcpListener::bind((host, port)).map_err(super::io)?;
        listener.set_nonblocking(true).map_err(super::io)?;
        let address = listener.local_addr().map_err(super::io)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(super::io)?;
        let listener = {
            let _guard = runtime.enter();
            tokio::net::TcpListener::from_std(listener).map_err(super::io)?
        };
        let shared = Shared::new(Receiver::new(Value::integer(port), config));
        let owner = shared.clone();
        let (stop, closing) = oneshot::channel();
        let worker = thread::Builder::new()
            .name("cluster-navi-receiver".into())
            .spawn(move || runtime.block_on(serve(listener, owner, closing)))
            .map_err(super::io)?;
        let mut server = Self {
            address,
            shared,
            stop: Some(stop),
            worker: Some(worker),
            publisher: None,
        };
        if publish {
            server.publisher = Some(Publisher::start(server.shared.clone())?);
        }
        Ok(server)
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }

    pub fn snapshot(&self) -> Result<(Dashboard, Value), Error> {
        self.shared
            .with(|receiver| (receiver.dashboard_snapshot(), receiver.cereal_snapshot()))
    }

    pub fn close(&mut self) -> Result<(), Error> {
        if let Some(mut publisher) = self.publisher.take() {
            publisher.stop();
        }
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| {
                Error::typed("RuntimeError", "embedded receiver thread panicked".into())
            })??;
        }
        Ok(())
    }
}

impl Drop for EmbeddedServer {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

async fn serve(
    listener: tokio::net::TcpListener,
    shared: Shared,
    mut stop: oneshot::Receiver<()>,
) -> Result<(), Error> {
    let result = loop {
        tokio::select! {
            _ = &mut stop => break Ok(()),
            accepted = listener.accept() => {
                let (stream, address) = match accepted { Ok(value) => value, Err(error) => break Err(super::io(error)) };
                let owner = shared.clone();
                if let Err(error) = shared.spawn(async move {
                    let target = owner.clone();
                    let handler = service_fn(move |request| http::serve(request, target.clone(), address));
                    let connection = http1::Builder::new().serve_connection(TokioIo::new(stream), handler).with_upgrades();
                    tokio::pin!(connection);
                    let mut closing = owner.closing.subscribe();
                    if *closing.borrow_and_update() == Close::Shutdown { connection.as_mut().graceful_shutdown(); }
                    loop {
                        tokio::select! {
                            _ = &mut connection => break,
                            changed = closing.changed() => {
                                if changed.is_err() || *closing.borrow_and_update() == Close::Shutdown {
                                    connection.as_mut().graceful_shutdown();
                                }
                            }
                        }
                    }
                }) { break Err(error); }
            }
        }
    };
    drop(listener);
    shared.closing.send_replace(Close::Shutdown);
    let mut tasks = {
        let mut tasks = shared
            .tasks
            .lock()
            .map_err(|_| Error::typed("RuntimeError", "task lock poisoned".into()))?;
        std::mem::take(&mut *tasks)
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while !tasks.is_empty() {
        if tokio::time::timeout_at(deadline, tasks.join_next())
            .await
            .is_err()
        {
            tasks.abort_all();
            break;
        }
    }
    while tasks.join_next().await.is_some() {}
    result
}
