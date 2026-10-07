use super::{
    clock::{self, NativeClock},
    shared::Shared,
    wire,
};
use crate::{json::Value, record::Record, Error};
use openpilot_messaging::runtime::PubMaster;
use openpilot_params::Params;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub struct Publisher {
    stop: Arc<AtomicBool>,
    shared: Shared,
    worker: Option<JoinHandle<()>>,
}

struct Loop {
    publisher: PubMaster,
    params: Option<Params>,
    shared: Shared,
    stop: Arc<AtomicBool>,
    media_request: String,
    next_poll: f64,
}

impl Loop {
    fn media_request(&mut self) -> String {
        let now = clock::seconds();
        if now >= self.next_poll {
            self.next_poll = now + 0.25;
            if let Some(params) = &self.params {
                match params.get("CarrotNaviWebBootstrapRequest") {
                    Ok(value) => {
                        self.media_request = value
                            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                            .unwrap_or_default()
                    }
                    Err(openpilot_params::Error::Io(_)) => self.media_request.clear(),
                    Err(_) => (),
                }
            } else {
                self.media_request.clear();
            }
        }
        self.media_request.clone()
    }
    fn published(&self, result: Result<(), Error>) -> Result<(), Error> {
        let error = result.err().map(|error| error.message_value());
        self.shared
            .with(|receiver| receiver.record_cereal_publish(error.as_ref(), &mut NativeClock))?
    }
    fn media(&mut self, record: &Record, session: &Value, kind: Option<&str>) -> Result<(), Error> {
        let bytes = wire::media(record, session, kind)?;
        self.publisher
            .send("carrotNaviMedia", &bytes)
            .map_err(|error| Error::typed("IpcError", error.to_string()))?;
        self.published(Ok(()))
    }
    fn updates(&mut self, snapshot: &Value, updates: &[Arc<Record>]) -> Result<(), Error> {
        let before = self.media_request.clone();
        let request = self.media_request();
        let session = snapshot.get("session_id");
        if !request.is_empty() && request != before {
            for record in self.shared.with(|receiver| receiver.media_bootstrap())? {
                if record.kind != "image" && !(record.kind == "render" && record.name == "map_main")
                {
                    continue;
                }
                if updates.iter().any(|update| {
                    (update.kind.as_str(), update.name.as_str(), &update.sequence)
                        == (record.kind.as_str(), record.name.as_str(), &record.sequence)
                }) {
                    continue;
                }
                self.media(
                    &record,
                    session,
                    Some(if record.kind == "image" {
                        "web_image"
                    } else {
                        "web_render"
                    }),
                )?;
            }
        }
        for record in updates {
            self.media(record, session, None)?;
        }
        Ok(())
    }
    fn run(&mut self) -> Result<(), Error> {
        let mut last_generation = Value::integer(-1);
        let mut last_publish = 0.;
        while !self.stop.load(Ordering::Relaxed) {
            let elapsed = clock::seconds() - last_publish;
            self.shared.wait((0.5 - elapsed).max(0.))?;
            if self.stop.load(Ordering::Relaxed) {
                break;
            }
            let elapsed = clock::seconds() - last_publish;
            if elapsed < 0.05 {
                thread::sleep(Duration::from_secs_f64(0.05 - elapsed));
                if self.stop.load(Ordering::Relaxed) {
                    break;
                }
            }
            let snapshot = self.shared.with(|receiver| receiver.cereal_snapshot())?;
            let updates = self
                .shared
                .with(|receiver| receiver.drain_media_updates())?;
            if let Err(error) = self.updates(&snapshot, &updates) {
                self.published(Err(error))?;
            }
            let generation = snapshot.get("generation");
            let elapsed = clock::seconds() - last_publish;
            if generation == &last_generation && elapsed < 0.5 {
                continue;
            }
            let result = wire::state(&snapshot).and_then(|bytes| {
                self.publisher
                    .send("carrotNavi", &bytes)
                    .map_err(|error| Error::typed("IpcError", error.to_string()))
            });
            if result.is_ok() {
                last_generation = Value::Integer(Value::Float(generation.float()?).int()?);
            }
            self.published(result)?;
            last_publish = clock::seconds();
        }
        Ok(())
    }
}

impl Publisher {
    pub fn start(shared: Shared) -> Result<Self, Error> {
        let stop = Arc::new(AtomicBool::new(false));
        let owner = shared.clone();
        let flag = Arc::clone(&stop);
        let (ready, constructed) = std::sync::mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("carrot_navi_cereal".into())
            .spawn(move || {
                let publisher = match PubMaster::for_runtime(&["carrotNavi", "carrotNaviMedia"]) {
                    Ok(publisher) => publisher,
                    Err(error) => {
                        if let Err(error) =
                            ready.send(Err(Error::typed("IpcError", error.to_string())))
                        {
                            eprintln!("publisher construction: {error}");
                        }
                        return;
                    }
                };
                let mut task = Loop {
                    publisher,
                    params: Params::for_runtime().ok(),
                    shared: owner,
                    stop: flag,
                    media_request: String::new(),
                    next_poll: 0.,
                };
                if ready.send(Ok(())).is_err() {
                    return;
                }
                if let Err(error) = task.run() {
                    eprintln!("{}: {error}", error.kind);
                }
            })
            .map_err(super::io)?;
        match constructed
            .recv()
            .map_err(|error| Error::typed("RuntimeError", error.to_string()))?
        {
            Ok(()) => (),
            Err(error) => {
                if worker.join().is_err() {
                    eprintln!("publisher constructor thread panicked");
                }
                return Err(error);
            }
        }
        Ok(Self {
            stop,
            shared,
            worker: Some(worker),
        })
    }
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.shared.wake();
        let until = std::time::Instant::now() + Duration::from_secs(1);
        if let Some(worker) = self.worker.take() {
            while !worker.is_finished() && std::time::Instant::now() < until {
                thread::sleep(Duration::from_millis(1));
            }
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}
impl Drop for Publisher {
    fn drop(&mut self) {
        self.stop();
    }
}
