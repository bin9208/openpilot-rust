use crate::Error;
use parking_lot::{ReentrantMutex, ReentrantMutexGuard};
use rustix::fs::{flock, FlockOperation};
use std::{
    cell::Cell,
    collections::HashMap,
    fs::{File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
};

pub const DEFAULT_PATH: &str = "/tmp/carrot_usbgpu_bus.lock";
struct State {
    file: File,
    depth: Cell<usize>,
}
type Registry = HashMap<(u32, PathBuf), Weak<ReentrantMutex<State>>>;
static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

#[derive(Clone)]
pub struct BusLock(Arc<ReentrantMutex<State>>);
pub struct Guard<'a>(ReentrantMutexGuard<'a, State>);
impl BusLock {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let key = (std::process::id(), path.to_path_buf());
        let mut registry = REGISTRY
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| Error::Contract("USB bus lock registry poisoned"))?;
        if let Some(state) = registry.get(&key).and_then(Weak::upgrade) {
            return Ok(Self(state));
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o666)
            .open(path)?;
        let state = Arc::new(ReentrantMutex::new(State {
            file,
            depth: Cell::new(0),
        }));
        registry.insert(key, Arc::downgrade(&state));
        Ok(Self(state))
    }
    pub fn enter(&self) -> Result<Guard<'_>, Error> {
        let guard = self.0.lock();
        let depth = guard
            .depth
            .get()
            .checked_add(1)
            .ok_or(Error::Contract("USB bus lock nesting overflow"))?;
        if depth == 1 {
            flock(&guard.file, FlockOperation::LockExclusive).map_err(std::io::Error::from)?;
        }
        guard.depth.set(depth);
        Ok(Guard(guard))
    }
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        let next = self.0.depth.get().saturating_sub(1);
        self.0.depth.set(next);
        if next == 0 {
            if let Err(error) = flock(&self.0.file, FlockOperation::Unlock) {
                eprintln!("usbgpu bus unlock: {error}");
            }
        }
    }
}
