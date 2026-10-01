use crate::{bus::Peer, engine::Engine, state::State, Command, Error, Event, Snapshot};
use openpilot_logging::{producer::Factory, record::Level};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex},
    thread::JoinHandle,
};
use tokio::{
    sync::{mpsc, watch},
    task::{JoinSet, LocalSet},
};

#[derive(Clone, Debug)]
pub struct Config {
    pub address: Option<String>,
    pub launcher: PathBuf,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            address: None,
            launcher: "openpilot-process-child".into(),
        }
    }
}
pub struct WifiManager {
    state: Arc<Mutex<State>>,
    commands: mpsc::Sender<Command>,
    shutdown: watch::Sender<bool>,
    worker: Option<JoinHandle<Result<(), Error>>>,
}
impl WifiManager {
    pub fn start(config: Config) -> Result<Self, Error> {
        let params = openpilot_params::Params::for_runtime()?;
        let dongle = params.get("DongleId")?.map(String::from_utf8).transpose()?;
        let state = Arc::new(Mutex::new(State::new(dongle.as_deref())));
        let factory = Factory::for_runtime()?;
        let (commands, receiver) = mpsc::channel(256);
        let (shutdown, stopping) = watch::channel(false);
        let shared = Arc::clone(&state);
        let worker = std::thread::Builder::new()
            .name("wifi-manager".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                LocalSet::new().block_on(&runtime, run(config, shared, factory, receiver, stopping))
            })?;
        Ok(Self {
            state,
            commands,
            shutdown,
            worker: Some(worker),
        })
    }
    pub fn snapshot(&self) -> Result<Snapshot, Error> {
        Ok(self.state.lock().map_err(|_| Error::Poisoned)?.snapshot())
    }
    pub fn drain_events(&self) -> Result<Vec<Event>, Error> {
        Ok(std::mem::take(
            &mut self.state.lock().map_err(|_| Error::Poisoned)?.events,
        ))
    }
    pub fn send(&self, command: Command) -> Result<(), Error> {
        if command == Command::Stop {
            return self.shutdown.send(true).map_err(|_| Error::Stopped);
        }
        let permit = self.commands.try_reserve().map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => Error::QueueFull,
            mpsc::error::TrySendError::Closed(_) => Error::Stopped,
        })?;
        {
            let mut state = self.state.lock().map_err(|_| Error::Poisoned)?;
            match &command {
                Command::Connect { ssid, .. } | Command::Activate(ssid) => {
                    state.set_connecting(Some(ssid.clone()))
                }
                Command::SetActive(active) => state.active = *active,
                Command::SetIpv4Forward(enabled) => state.ipv4_forward = *enabled,
                Command::Forget(_)
                | Command::SetTetheringPassword(_)
                | Command::SetTetheringActive(_)
                | Command::SetCurrentNetworkMetered(_)
                | Command::Stop => {}
            }
        }
        permit.send(command);
        Ok(())
    }
    pub fn stop(&mut self) -> Result<(), Error> {
        if let Some(worker) = self.worker.take() {
            self.shutdown.send_replace(true);
            worker.join().map_err(|_| Error::Panicked)??;
        }
        Ok(())
    }
}
impl Drop for WifiManager {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("Wi-Fi shutdown: {error}");
        }
    }
}
async fn run(
    config: Config,
    state: Arc<Mutex<State>>,
    factory: Factory,
    mut commands: mpsc::Receiver<Command>,
    mut stopping: watch::Receiver<bool>,
) -> Result<(), Error> {
    let logger = Rc::new(RefCell::new(factory.logger()));
    let connections = (|| {
        Ok::<_, Error>((
            Peer::open(config.address.as_deref())?,
            Peer::open(config.address.as_deref())?,
        ))
    })();
    let ((main, main_io), (monitor, monitor_io)) = match connections {
        Ok(connections) => connections,
        Err(error) => {
            let record = openpilot_logging::record::Record::text(
                Level::Error,
                "Failed to connect to system D-Bus".into(),
            )
            .with_exception(error.to_string());
            logger
                .borrow_mut()
                .emit(openpilot_logging::log_site!(), record)?;
            return Err(error);
        }
    };
    let engine = Engine {
        state,
        main,
        monitor,
        logger,
        scan_lock: Rc::new(tokio::sync::Mutex::new(())),
        launcher: config.launcher,
    };
    let mut io = JoinSet::new();
    io.spawn_local(main_io);
    io.spawn_local(monitor_io);
    let mut workers = JoinSet::new();
    let startup = engine.clone();
    workers.spawn_local(async move { startup.initialize().await });
    let result = loop {
        if *stopping.borrow() {
            break Ok(());
        }
        tokio::select! {
            change = stopping.changed() => { if change.is_err() || *stopping.borrow() { break Ok(()); } }
            command = commands.recv() => {
                let Some(command) = command else { break Ok(()); };
                let worker = engine.clone();
                workers.spawn_local(async move { worker.execute(command).await });
            }
            completed = workers.join_next(), if !workers.is_empty() => {
                match completed {
                    Some(Ok(Ok(()))) | None => {}
                    Some(Ok(Err(error))) => engine.log(Level::Error, format!("Wi-Fi operation failed: {error}")),
                    Some(Err(error)) => engine.log(Level::Error, format!("Wi-Fi task failed: {error}")),
                }
            }
            ended = io.join_next() => { break Err(Error::Transport(format!("{ended:?}"))); }
        }
    };
    workers.abort_all();
    while let Some(joined) = workers.join_next().await {
        if let Err(error) = joined {
            if !error.is_cancelled() {
                engine.log(Level::Error, format!("Wi-Fi task shutdown: {error}"));
            }
        }
    }
    io.abort_all();
    while let Some(joined) = io.join_next().await {
        if let Err(error) = joined {
            if !error.is_cancelled() {
                engine.log(Level::Error, format!("Wi-Fi I/O shutdown: {error}"));
            }
        }
    }
    result
}
