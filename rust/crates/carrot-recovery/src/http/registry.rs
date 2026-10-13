use crate::Error;
use std::{
    collections::BTreeMap,
    net::{Shutdown, TcpStream},
    sync::{Arc, Mutex},
};

pub type Registry = Arc<Mutex<BTreeMap<u64, TcpStream>>>;
pub struct Registration {
    pub id: u64,
    pub sockets: Registry,
}
impl Drop for Registration {
    fn drop(&mut self) {
        match self.sockets.lock() {
            Ok(mut sockets) => {
                sockets.remove(&self.id);
            }
            Err(error) => eprintln!("[recovery] socket registry: {error}"),
        }
    }
}
pub fn close(sockets: &Registry) -> Result<(), Error> {
    let sockets = sockets
        .lock()
        .map_err(|_| Error::Source("socket registry poisoned".into()))?;
    for socket in sockets.values() {
        if let Err(error) = socket.shutdown(Shutdown::Both) {
            eprintln!("[recovery] socket cleanup: {error}");
        }
    }
    Ok(())
}
