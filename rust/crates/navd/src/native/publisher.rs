use super::{diagnostics, timestamp};
use crate::{
    geometry::{limit_route_points, Coordinate},
    route::Diagnostic,
    wire, Error,
};
use openpilot_logging::producer::{Factory, Logger};
use openpilot_messaging::runtime::PubMaster;
use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

enum Command {
    Message {
        service: &'static str,
        bytes: Vec<u8>,
        done: SyncSender<Result<(), Error>>,
    },
    Geometry(Result<Vec<Coordinate>, String>),
    Resend(Instant),
}

pub struct Publisher {
    sender: Option<SyncSender<Command>>,
    worker: Option<JoinHandle<()>>,
}

impl Publisher {
    pub fn new(factory: Factory) -> Result<Self, Error> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let (ready, readiness) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("navd-publisher".into())
            .spawn(move || {
                let mut publisher =
                    match PubMaster::for_runtime(&["navInstruction", "navRouteNavd"]) {
                        Ok(publisher) => publisher,
                        Err(error) => {
                            if ready.send(Err(Error::from(error))).is_err() {
                                eprintln!("navd publisher startup receiver closed");
                            }
                            return;
                        }
                    };
                if ready.send(Ok(())).is_err() {
                    return;
                }
                run(&mut publisher, &mut factory.logger(), receiver);
            })?;
        readiness
            .recv()
            .map_err(|_| Error::Runtime("publisher initialization stopped"))??;
        Ok(Self {
            sender: Some(sender),
            worker: Some(worker),
        })
    }

    fn command(&self, command: Command) -> Result<(), Error> {
        self.sender
            .as_ref()
            .ok_or(Error::Runtime("publisher closed"))?
            .send(command)
            .map_err(|_| Error::Runtime("publisher worker stopped"))
    }

    pub fn send(&self, service: &'static str, bytes: Vec<u8>) -> Result<(), Error> {
        let (done, result) = mpsc::sync_channel(1);
        self.command(Command::Message {
            service,
            bytes,
            done,
        })?;
        result
            .recv()
            .map_err(|_| Error::Runtime("publisher acknowledgement closed"))?
    }

    pub fn geometry(&self, coordinates: Result<Vec<Coordinate>, Error>) -> Result<(), Error> {
        self.command(Command::Geometry(
            coordinates.map_err(|error| error.to_string()),
        ))
    }

    pub fn resend(&self) -> Result<(), Error> {
        self.command(Command::Resend(Instant::now() + Duration::from_secs(5)))
    }
}

impl Drop for Publisher {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                eprintln!("navd publisher thread panicked");
            }
        }
    }
}

fn send_route(
    publisher: &mut PubMaster,
    logger: &mut Logger,
    geometry: &Result<Vec<Coordinate>, String>,
) -> Result<(), Error> {
    let geometry = match geometry {
        Ok(geometry) => geometry,
        Err(error) => {
            eprintln!("navd route timer: {error}");
            return Ok(());
        }
    };
    let limited = limit_route_points(geometry, 4096)?;
    if geometry.len() > limited.len() {
        diagnostics::event(
            logger,
            Diagnostic::RouteLimited {
                original: geometry.len(),
                sent: limited.len(),
            },
        );
    }
    publisher.send("navRouteNavd", &wire::route(&limited, timestamp()?)?)?;
    Ok(())
}

fn run(publisher: &mut PubMaster, logger: &mut Logger, receiver: Receiver<Command>) {
    let mut geometry = Ok(Vec::new());
    let mut deadlines: Vec<Instant> = Vec::new();
    let mut connected = true;
    loop {
        let next = deadlines.first().copied();
        let received = if connected {
            match next {
                Some(deadline) => {
                    receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                }
                None => receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
            }
        } else if let Some(deadline) = next {
            thread::sleep(deadline.saturating_duration_since(Instant::now()));
            Err(RecvTimeoutError::Timeout)
        } else {
            break;
        };
        match received {
            Ok(Command::Message {
                service,
                bytes,
                done,
            }) => {
                let result = publisher.send(service, &bytes).map_err(Error::from);
                if done.send(result).is_err() {
                    eprintln!("navd publication receiver closed");
                }
            }
            Ok(Command::Geometry(value)) => geometry = value,
            Ok(Command::Resend(deadline)) => deadlines.push(deadline),
            Err(RecvTimeoutError::Disconnected) => connected = false,
            Err(RecvTimeoutError::Timeout) => {
                if let Err(error) = send_route(publisher, logger, &geometry) {
                    eprintln!("navd route timer: {error}");
                }
                deadlines.remove(0);
            }
        }
    }
}
