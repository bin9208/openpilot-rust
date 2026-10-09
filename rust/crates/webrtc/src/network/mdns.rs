use super::{
    dns::{self, Name},
    mdns_socket::Sockets,
};
use crate::Error;
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
use tokio::sync::oneshot;

type Waiters = HashMap<u64, oneshot::Sender<IpAddr>>;

struct State {
    users: usize,
    next: u64,
    sockets: Option<Arc<Sockets>>,
    queries: HashMap<Name, Waiters>,
}

struct Service(Mutex<State>);

pub(crate) struct Lease(Arc<Service>);

struct Waiter {
    service: Arc<Service>,
    name: Name,
    id: u64,
}

pub(crate) struct Resolver {
    recipient: Option<SocketAddr>,
    service: Mutex<Weak<Service>>,
}

impl Resolver {
    #[cfg(test)]
    pub(super) fn pending(&self) -> Result<usize, Error> {
        let shared = self
            .service
            .lock()
            .map_err(|_| Error::Contract("mDNS resolver lock poisoned"))?;
        shared.upgrade().map_or(Ok(0), |service| {
            service
                .0
                .lock()
                .map(|state| state.queries.len())
                .map_err(|_| Error::Contract("mDNS query lock poisoned"))
        })
    }
    pub const fn new(recipient: Option<SocketAddr>) -> Self {
        Self {
            recipient,
            service: Mutex::new(Weak::new()),
        }
    }

    pub fn acquire(&self) -> Result<Lease, Error> {
        let mut shared = self
            .service
            .lock()
            .map_err(|_| Error::Contract("mDNS resolver lock poisoned"))?;
        if let Some(service) = shared.upgrade() {
            let mut state = service
                .0
                .lock()
                .map_err(|_| Error::Contract("mDNS query lock poisoned"))?;
            if state.sockets.is_some() {
                state.users = state
                    .users
                    .checked_add(1)
                    .ok_or(Error::Contract("mDNS owner count overflow"))?;
                drop(state);
                return Ok(Lease(service));
            }
        }
        let service = Arc::new(Service(Mutex::new(State {
            users: 1,
            next: 0,
            sockets: Some(Arc::new(Sockets::new(self.recipient)?)),
            queries: HashMap::new(),
        })));
        *shared = Arc::downgrade(&service);
        Ok(Lease(service))
    }

    pub fn close(&self) -> Result<(), Error> {
        let shared = self
            .service
            .lock()
            .map_err(|_| Error::Contract("mDNS resolver lock poisoned"))?;
        if let Some(service) = shared.upgrade() {
            let mut state = service
                .0
                .lock()
                .map_err(|_| Error::Contract("mDNS query lock poisoned"))?;
            state.queries.clear();
            state.sockets = None;
        }
        Ok(())
    }
}

impl Lease {
    pub async fn resolve(&self, host: &str) -> Result<Option<IpAddr>, Error> {
        let name = Name::hostname(host).ok_or(Error::Contract("invalid mDNS hostname"))?;
        let (sender, mut receiver) = oneshot::channel();
        let (sockets, id, first) = {
            let mut state = self
                .0
                 .0
                .lock()
                .map_err(|_| Error::Contract("mDNS query lock poisoned"))?;
            let Some(sockets) = state.sockets.as_ref().map(Arc::clone) else {
                return Ok(None);
            };
            let id = state.next;
            state.next = id
                .checked_add(1)
                .ok_or(Error::Contract("mDNS waiter count overflow"))?;
            let waiters = state.queries.entry(name.clone()).or_default();
            let first = waiters.is_empty();
            waiters.insert(id, sender);
            (sockets, id, first)
        };
        let _waiter = Waiter {
            service: Arc::clone(&self.0),
            name: name.clone(),
            id,
        };
        if first {
            sockets
                .tx
                .send_to(&dns::query(host)?, sockets.recipient)
                .await?;
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        let mut buffer = vec![0_u8; 65_536].into_boxed_slice();
        loop {
            tokio::select! {
                result = &mut receiver => return Ok(result.ok()),
                () = tokio::time::sleep_until(deadline) => return Ok(None),
                result = sockets.rx.recv_from(&mut buffer) => {
                    let (length, _) = result?;
                    if let Some(answers) = dns::answers(&buffer[..length]) {
                        let mut state = self.0.0.lock().map_err(|_| Error::Contract("mDNS query lock poisoned"))?;
                        for (name, address) in answers {
                            if let Some(waiters) = state.queries.remove(&name) {
                                for sender in waiters.into_values() {
                                    if sender.send(address).is_err() {
                                        eprintln!("WebRTC mDNS answer waiter cancelled");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        match self.0 .0.lock() {
            Ok(mut state) => {
                state.users = state.users.saturating_sub(1);
                if state.users == 0 {
                    state.queries.clear();
                    state.sockets = None;
                }
            }
            Err(error) => eprintln!("WebRTC mDNS owner cleanup failed: {error}"),
        }
    }
}

impl Drop for Waiter {
    fn drop(&mut self) {
        match self.service.0.lock() {
            Ok(mut state) => {
                if let Some(waiters) = state.queries.get_mut(&self.name) {
                    waiters.remove(&self.id);
                    if waiters.is_empty() {
                        state.queries.remove(&self.name);
                    }
                }
            }
            Err(error) => eprintln!("WebRTC mDNS waiter cleanup failed: {error}"),
        }
    }
}
