use super::{
    rtmp_api::Api,
    rtmp_handle::Handle,
    tls::{Shared, Tunnel},
};
use crate::Error;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex, TryLockError,
};

pub struct Client {
    api: Arc<Api>,
    url: String,
    owner: Mutex<Owner>,
    bytes: AtomicU64,
    wake: Mutex<Option<Arc<Shared>>>,
}
struct Owner {
    handle: Option<Handle>,
    tunnel: Option<Tunnel>,
}
impl Owner {
    fn close(&mut self) {
        self.handle.take();
        if let Some(mut tunnel) = self.tunnel.take() {
            tunnel.close();
        }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.close();
    }
}
impl Client {
    pub fn new(url: String) -> Result<Self, Error> {
        Ok(Self {
            api: Api::load().map_err(|error| Error::Source(error.to_string()))?,
            url,
            owner: Mutex::new(Owner {
                handle: None,
                tunnel: None,
            }),
            bytes: AtomicU64::new(0),
            wake: Mutex::new(None),
        })
    }
    pub fn connect(&self) -> Result<(), Error> {
        let mut owner = self
            .owner
            .lock()
            .map_err(|_| Error::Source("RTMP owner poisoned".into()))?;
        if owner.handle.is_some() {
            return Ok(());
        }
        let result: Result<(), Error> = (|| {
            let (setup, tc_url) = if self.url.to_ascii_lowercase().starts_with("rtmps:") {
                let source_url = super::rtmp_url::split(&self.url);
                let parsed = url::Url::parse(&source_url)
                    .map_err(|error| Error::Source(error.to_string()))?;
                let host = parsed
                    .host_str()
                    .ok_or_else(|| Error::Source("RTMPS host is missing".into()))?;
                let tunnel = Tunnel::start(host.to_owned(), parsed.port().unwrap_or(443))?;
                if let Ok(mut wake) = self.wake.lock() {
                    *wake = Some(Arc::clone(&tunnel.shared));
                }
                let prepared = super::rtmp_url::prepare(&source_url, tunnel.port)?;
                owner.tunnel = Some(tunnel);
                prepared
            } else {
                (self.url.clone(), String::new())
            };
            let mut handle = Handle::new(Arc::clone(&self.api), &setup, &tc_url)?;
            handle.connect()?;
            owner.handle = Some(handle);
            Ok(())
        })();
        if let Err(error) = result {
            let detail = owner
                .tunnel
                .as_ref()
                .and_then(|t| t.shared.error.lock().ok().map(|v| v.clone()))
                .unwrap_or_default();
            owner.close();
            if !detail.is_empty() && error.to_string() == "YouTube RTMPS connection failed" {
                return Err(Error::Source(format!("{error}: {detail}")));
            }
            return Err(error);
        }
        Ok(())
    }
    pub fn write(&self, payload: &[u8]) -> Result<usize, Error> {
        if payload.is_empty() {
            return Ok(0);
        }
        let mut owner = self
            .owner
            .lock()
            .map_err(|_| Error::Source("RTMP owner poisoned".into()))?;
        let handle = owner
            .handle
            .as_mut()
            .ok_or_else(|| Error::Source("YouTube RTMPS connection is closed".into()))?;
        let count = handle.write(payload)?;
        self.bytes
            .fetch_add(u64::try_from(count).unwrap_or(u64::MAX), Ordering::SeqCst);
        Ok(count)
    }
    pub fn bytes_written(&self) -> u64 {
        self.bytes.load(Ordering::SeqCst)
    }
    pub fn try_connected(&self) -> Result<Option<bool>, Error> {
        match self.owner.try_lock() {
            Ok(owner) => Ok(Some(owner.handle.as_ref().is_some_and(Handle::connected))),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Poisoned(_)) => Err(Error::Source("RTMP owner poisoned".into())),
        }
    }
    pub fn close(&self) {
        if let Ok(mut owner) = self.owner.lock() {
            owner.close();
        }
        if let Ok(mut wake) = self.wake.lock() {
            wake.take();
        }
    }
    pub fn wake_shutdown(&self) {
        if let Ok(wake) = self.wake.lock() {
            if let Some(tunnel) = wake.as_ref() {
                tunnel.shutdown();
            }
        }
    }
}
