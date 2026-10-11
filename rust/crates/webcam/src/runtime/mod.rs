mod publish;
mod worker;

pub use publish::Publication;
pub use worker::Report;

use crate::{selection::CameraSpec, Error};
use openpilot_messaging::runtime::PubMaster;
use openpilot_msgq::VisionServer;
use publish::Publisher;
use std::sync::mpsc::{self, Receiver};
use worker::{Command, Event, Worker};

pub struct Camerad {
    workers: Vec<Worker>,
    publishers: Vec<Publisher>,
    master: PubMaster,
    _server: VisionServer,
    events: Receiver<Event>,
    specs: Vec<CameraSpec>,
    started: bool,
}

impl Camerad {
    /// Open selected cameras in order, allocate storage and start the listener.
    ///
    /// # Errors
    /// Returns capture, allocation, IPC and worker initialization errors.
    pub fn prepare(specs: Vec<CameraSpec>) -> Result<Self, Error> {
        let services: Vec<_> = specs.iter().map(|spec| spec.kind.service()).collect();
        let master = PubMaster::for_runtime(&services)?;
        let server = VisionServer::new("camerad")?;
        let (sender, events) = mpsc::channel();
        let mut workers = Vec::new();
        let mut publishers = Vec::new();
        for (index, spec) in specs.iter().enumerate() {
            println!("Opening {} at {}", spec.kind.service(), spec.device);
            let worker = Worker::open(spec.clone(), index, sender.clone())?;
            publishers.push(Publisher::new(
                &server,
                spec.kind,
                worker.info.width,
                worker.info.height,
            )?);
            workers.push(worker);
        }
        server.start_listener()?;
        Ok(Self {
            workers,
            publishers,
            master,
            _server: server,
            events,
            specs,
            started: false,
        })
    }

    /// Run independent camera workers until every worker reaches EOF or fails.
    /// The observer sees Cereal bytes only after both real publications succeed.
    ///
    /// # Errors
    /// Returns coordinator/observer failures or a repeated invocation. Camera
    /// worker failures are reported independently and do not cancel siblings.
    pub fn run(
        &mut self,
        mut observe: impl FnMut(&Publication<'_>) -> Result<(), Error>,
    ) -> Result<Vec<Report>, Error> {
        if self.started {
            return Err(Error::Contract("Camerad already ran"));
        }
        self.started = true;
        for worker in &self.workers {
            worker.send(Command::Start)?;
        }
        let mut reports: Vec<Option<Report>> = (0..self.workers.len()).map(|_| None).collect();
        let mut remaining = reports.len();
        while remaining != 0 {
            match self
                .events
                .recv()
                .map_err(|_| Error::Contract("webcam workers disappeared"))?
            {
                Event::Frame {
                    index,
                    frame_id,
                    bytes,
                } => {
                    let kind = self.specs[index].kind;
                    let result = self.publishers[index].send(
                        &mut self.master,
                        Publication {
                            kind,
                            frame_id,
                            bytes: &bytes,
                        },
                    );
                    if let Ok(raw) = &result {
                        observe(&Publication {
                            kind,
                            frame_id,
                            bytes: raw,
                        })?;
                    }
                    self.workers[index].send(Command::Published(
                        result.map(|_| ()).map_err(|error| error.to_string()),
                    ))?;
                }
                Event::Finished { index, report } => {
                    reports[index] = Some(report);
                    remaining -= 1;
                }
            }
        }
        reports
            .into_iter()
            .map(|report| report.ok_or(Error::Contract("webcam worker report missing")))
            .collect()
    }
}
