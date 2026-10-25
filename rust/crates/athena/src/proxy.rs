use crate::{
    logging,
    policy::{proxy_port, SSH_TOS},
    state::{Shared, Stop},
    websocket::{self, Connection},
    Error,
};
use openpilot_logging::producer::Logger;
use socket2::SockRef;
use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::Arc,
    time::Duration,
};
use tungstenite::{
    protocol::{frame::coding::CloseCode, CloseFrame},
    Message,
};

pub fn start(
    shared: Arc<Shared>,
    stop: Stop,
    uri: &str,
    port: i64,
    logger: &mut Logger,
) -> Result<(), Error> {
    let port = proxy_port(port)?;
    let ws = Connection::connect(uri, &websocket::token(&shared, logger)?)?;
    SockRef::from(&ws.tcp).set_tos_v4(SSH_TOS)?;
    let local = TcpStream::connect(("127.0.0.1", port))?;
    local.set_nonblocking(true)?;
    let factory = shared.factory.clone();
    spawn_owned(&shared.proxies, move || {
        if let Err(error) = bridge(ws, local, &stop) {
            logging::failure(&mut factory.logger(), "athenad.ws_proxy.exception", &error);
        }
    })
}
fn spawn_owned(
    registry: &std::sync::Mutex<Vec<std::thread::JoinHandle<()>>>,
    body: impl FnOnce() + Send + 'static,
) -> Result<(), Error> {
    let mut handles = registry
        .lock()
        .map_err(|_| Error::Contract("proxy handles poisoned"))?;
    for index in (0..handles.len()).rev() {
        if handles[index].is_finished() && handles.remove(index).join().is_err() {
            return Err(Error::Contract("proxy worker panicked"));
        }
    }
    let handle = std::thread::Builder::new()
        .name("athena-proxy".into())
        .spawn(body)?;
    handles.push(handle);
    Ok(())
}

pub fn bridge(mut ws: Connection, mut local: TcpStream, stop: &Stop) -> Result<(), Error> {
    ws.bound_proxy_output();
    let mut pending = Vec::new();
    let mut offset = 0;
    let mut buffer = [0; 4096];
    let mut closing = false;
    while !stop.requested() {
        if closing {
            if ws.flush()? {
                break;
            }
            stop.wait(Duration::from_millis(5));
            continue;
        }
        if offset == pending.len() {
            pending.clear();
            offset = 0;
            match ws.read()? {
                Some(Message::Text(text)) => pending.extend_from_slice(text.as_bytes()),
                Some(Message::Binary(bytes)) => pending.extend_from_slice(&bytes),
                Some(Message::Close(_)) => {
                    closing = true;
                    continue;
                }
                Some(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) | None => {}
            }
        }
        if offset < pending.len() {
            match local.write(&pending[offset..]) {
                Ok(0) => break,
                Ok(count) => offset += count,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(error.into()),
            }
        }
        // Do not consume another local frame or EOF while an accepted frame is buffered.
        if ws.flush()? {
            match local.read(&mut buffer) {
                Ok(0) => {
                    ws.send(Message::Close(Some(CloseFrame {
                        code: CloseCode::Normal,
                        reason: "".into(),
                    })))?;
                    closing = true;
                }
                Ok(count) => ws.send(Message::Binary(buffer[..count].to_vec().into()))?,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(error.into()),
            }
        }
        stop.wait(Duration::from_millis(5));
    }
    local.shutdown(std::net::Shutdown::Both)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::spawn_owned;
    use std::{
        sync::{mpsc, Arc, Mutex},
        thread,
        time::{Duration, Instant},
    };

    #[derive(Debug, PartialEq)]
    enum Event {
        Started,
        Dropped,
    }
    struct Probe {
        events: mpsc::Sender<Event>,
        release: mpsc::Receiver<()>,
    }
    impl Probe {
        fn run(self) {
            self.events.send(Event::Started).unwrap();
            let _ = self.release.recv();
        }
    }
    impl Drop for Probe {
        fn drop(&mut self) {
            self.events.send(Event::Dropped).unwrap();
        }
    }
    fn finished(handle: &thread::JoinHandle<()>) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !handle.is_finished() {
            assert!(Instant::now() < deadline, "worker did not finish");
            thread::yield_now();
        }
    }
    fn rejected_worker_never_starts(poison: bool) {
        let registry = Arc::new(Mutex::new(Vec::new()));
        if poison {
            let shared = Arc::clone(&registry);
            assert!(thread::spawn(move || {
                let _locked = shared.lock().unwrap();
                panic!("intentional registry poison");
            })
            .join()
            .is_err());
        } else {
            let worker = thread::spawn(|| panic!("intentional old worker panic"));
            finished(&worker);
            registry.lock().unwrap().push(worker);
        }
        let (events, received) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let probe = Probe {
            events,
            release: wait,
        };
        let error = spawn_owned(&registry, move || probe.run()).unwrap_err();
        let first = received.recv_timeout(Duration::from_secs(2)).unwrap();
        if first == Event::Started {
            // Reap the observation even on the original broken path: a failing
            // assertion must not leave its deliberately blocked worker alive.
            release.send(()).unwrap();
            assert_eq!(
                received.recv_timeout(Duration::from_secs(2)).unwrap(),
                Event::Dropped
            );
        }
        assert_eq!(
            first,
            Event::Dropped,
            "rejected worker started without an owner"
        );
        assert!(error.to_string().contains(if poison {
            "proxy handles poisoned"
        } else {
            "proxy worker panicked"
        }));
    }
    #[test]
    fn poisoned_registry_rejects_before_starting_a_proxy_worker() {
        rejected_worker_never_starts(true);
    }
    #[test]
    fn panicked_previous_proxy_rejects_before_starting_another_worker() {
        rejected_worker_never_starts(false);
    }
    #[test]
    fn finished_workers_are_reaped_while_pending_workers_remain_owned() {
        let registry = Mutex::new(Vec::new());
        let (started, observed) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        spawn_owned(&registry, move || {
            started.send(()).unwrap();
            wait.recv().unwrap();
        })
        .unwrap();
        observed.recv_timeout(Duration::from_secs(2)).unwrap();
        spawn_owned(&registry, || {}).unwrap();
        finished(registry.lock().unwrap().last().unwrap());
        spawn_owned(&registry, || {}).unwrap();
        assert_eq!(registry.lock().unwrap().len(), 2);
        release.send(()).unwrap();
        for worker in registry.into_inner().unwrap() {
            worker.join().unwrap();
        }
    }
}
