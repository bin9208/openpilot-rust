use super::config::Config;
use crate::{serv::TrafficAction, Error};
use openpilot_params::Params;
use std::{sync::mpsc, thread};

pub fn open(config: &Config) -> Result<Params, Error> {
    Ok(match &config.params_root {
        Some(root) => Params::open(root, &config.prefix)?,
        None => Params::for_runtime()?,
    })
}
pub enum Write {
    Traffic(TrafficAction),
    Debug(String),
    Image(String),
    Network(String),
    Exception(String),
    Finish,
}
pub struct Writes {
    sender: mpsc::Sender<Write>,
    worker: Option<thread::JoinHandle<()>>,
    memory: Params,
}
impl Writes {
    pub fn new(config: &Config) -> Result<Self, Error> {
        let memory = Params::open(&config.memory_root, &config.prefix)?;
        let normal = open(config)?;
        let direct_memory = Params::open(&config.memory_root, &config.prefix)?;
        let (sender, receiver) = mpsc::channel();
        let worker = thread::Builder::new().name("carrot-params".into()).spawn(move || {
            while let Ok(write) = receiver.recv() {
                let result = match write {
                    Write::Traffic(TrafficAction::Put { distance, lamp, remain, source, ts }) => memory.put("TrafficLight", serde_json::json!({"distance":distance,"lamp":lamp,"remain":remain,"source":source,"ts":ts}).to_string().as_bytes()),
                    Write::Traffic(TrafficAction::Remove) => memory.remove("TrafficLight"),
                    Write::Debug(value) => memory.put("CarrotNaviDebug", value.as_bytes()), Write::Image(value) => memory.put("CarrotNaviImage", value.as_bytes()),
                    Write::Network(value) => memory.put("NetworkAddress", value.as_bytes()), Write::Exception(value)=>normal.put("CarrotException",value.as_bytes()), Write::Finish => break,
                };
                if let Err(error) = result { if !matches!(error, openpilot_params::Error::Io(ref e) if e.kind()==std::io::ErrorKind::NotFound) { eprintln!("carrot_man Params: {error}"); } }
            }
        })?;
        Ok(Self {
            sender,
            worker: Some(worker),
            memory: direct_memory,
        })
    }
    pub fn send(&self, write: Write) -> Result<(), Error> {
        if matches!(write, Write::Traffic(TrafficAction::Remove)) {
            let _ = self.memory.remove("TrafficLight");
            return Ok(());
        }
        self.sender
            .send(write)
            .map_err(|_| Error::Contract("Params writer stopped"))
    }
}
impl Drop for Writes {
    fn drop(&mut self) {
        let _ = self.sender.send(Write::Finish);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn text(params: &Params, key: &str) -> String {
    params
        .get(key)
        .ok()
        .flatten()
        .map(|b| String::from_utf8_lossy(&b).trim().to_owned())
        .unwrap_or_default()
}
