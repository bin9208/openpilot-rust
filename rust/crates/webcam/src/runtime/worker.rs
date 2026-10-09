use crate::{
    capture::{Capture, Info},
    selection::CameraSpec,
    Error,
};
use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub(super) enum Command {
    Start,
    Published(Result<(), String>),
    Close,
}

pub(super) enum Event {
    Frame {
        index: usize,
        frame_id: u32,
        bytes: Vec<u8>,
    },
    Finished {
        index: usize,
        report: Report,
    },
}

#[derive(serde::Serialize)]
pub struct Report {
    pub service: &'static str,
    pub info: Info,
    pub frame_id: u32,
    pub opened: Option<bool>,
    pub failure: Option<String>,
}

pub(super) struct Worker {
    pub info: Info,
    command: Option<Sender<Command>>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub(super) fn open(
        spec: CameraSpec,
        index: usize,
        events: Sender<Event>,
    ) -> Result<Self, Error> {
        let (command, receiver) = mpsc::channel();
        let (ready, opened) = mpsc::channel();
        let thread = thread::Builder::new()
            .name(spec.kind.service().into())
            .spawn(move || {
                let initialize = || -> Result<(Capture, Info), Error> {
                    let capture = Capture::path(&spec.device)?;
                    let info = capture.info()?;
                    Ok((capture, info))
                };
                let (mut capture, info) = match initialize() {
                    Ok(value) => value,
                    Err(error) => {
                        if ready.send(Err(error.to_string())).is_err() {
                            eprintln!("webcam initializer recipient closed");
                        }
                        return;
                    }
                };
                if ready.send(Ok(info.clone())).is_err() {
                    return;
                }
                if !matches!(receiver.recv(), Ok(Command::Start)) {
                    return;
                }
                let (frame_id, mut failure) = frames(&mut capture, index, &events, &receiver);
                let opened = match capture.opened() {
                    Ok(opened) => Some(opened),
                    Err(error) => {
                        failure = Some(error.to_string());
                        None
                    }
                };
                if let Some(error) = &failure {
                    eprintln!("{}: {error}", spec.kind.service());
                }
                let report = Report {
                    service: spec.kind.service(),
                    info,
                    frame_id,
                    opened,
                    failure,
                };
                if events.send(Event::Finished { index, report }).is_err() {
                    eprintln!("webcam final report recipient closed");
                }
                // A source worker exception ends only that worker. Its camera is
                // still owned by Camerad until caller teardown, so retain it here.
                while let Ok(command) = receiver.recv() {
                    if matches!(command, Command::Close) {
                        break;
                    }
                }
            })?;
        let info = match opened.recv() {
            Ok(Ok(info)) => info,
            result => {
                if thread.join().is_err() {
                    eprintln!("webcam initializer panicked");
                }
                return Err(match result {
                    Ok(Err(error)) => Error::Worker(error),
                    Err(_) => Error::Contract("camera initializer disappeared"),
                    Ok(Ok(_)) => Error::Contract("invalid webcam initializer transition"),
                });
            }
        };
        Ok(Self {
            info,
            command: Some(command),
            thread: Some(thread),
        })
    }

    pub(super) fn send(&self, command: Command) -> Result<(), Error> {
        self.command
            .as_ref()
            .ok_or(Error::Contract("webcam worker closed"))?
            .send(command)
            .map_err(|_| Error::Contract("webcam worker recipient closed"))
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(command) = self.command.take() {
            if command.send(Command::Close).is_err() {
                eprintln!("webcam worker already stopped");
            }
            // Close the sender before joining: a worker interrupted while
            // awaiting publication may consume Close before retaining capture.
            drop(command);
        }
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                eprintln!("webcam worker panicked");
            }
        }
    }
}

fn frames(
    capture: &mut Capture,
    index: usize,
    events: &Sender<Event>,
    commands: &Receiver<Command>,
) -> (u32, Option<String>) {
    let mut frame_id = 0_u32;
    let mut next = None;
    let mut run = || -> Result<(), Error> {
        while let Some(bytes) = capture.read()? {
            events
                .send(Event::Frame {
                    index,
                    frame_id,
                    bytes,
                })
                .map_err(|_| Error::Contract("webcam coordinator closed"))?;
            match commands.recv() {
                Ok(Command::Published(Ok(()))) => {}
                Ok(Command::Published(Err(error))) => return Err(Error::Worker(error)),
                Ok(Command::Close) | Err(_) => return Ok(()),
                Ok(Command::Start) => return Err(Error::Contract("webcam worker started twice")),
            }
            frame_id = frame_id
                .checked_add(1)
                .ok_or(Error::Contract("frame ID exceeds Cereal UInt32"))?;
            let deadline = next.unwrap_or_else(|| Instant::now() + Duration::from_millis(50));
            next = Some(deadline + Duration::from_millis(50));
            thread::sleep(deadline.saturating_duration_since(Instant::now()));
        }
        Ok(())
    };
    let failure = run().err().map(|error| error.to_string());
    (frame_id, failure)
}
