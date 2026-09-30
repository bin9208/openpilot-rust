use crate::Error;
use openpilot_params::Params;
use std::{
    sync::mpsc,
    thread::{self, JoinHandle},
};
enum Write {
    Torque(Vec<u8>),
    Finish,
}

pub struct PendingWrites {
    sender: mpsc::Sender<Write>,
    worker: Option<JoinHandle<()>>,
}

impl PendingWrites {
    pub fn new(params: Params) -> Result<Self, Error> {
        let (sender, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("torque-params".to_owned())
            .spawn(move || {
                while let Ok(command) = receiver.recv() {
                    match command {
                        Write::Torque(bytes) => {
                            if let Err(error) = params.put("LiveTorqueParameters", &bytes) {
                                eprintln!("torqued: error writing LiveTorqueParameters: {error}");
                            }
                        }
                        Write::Finish => break,
                    }
                }
            })?;
        Ok(Self {
            sender,
            worker: Some(worker),
        })
    }

    pub fn put(&self, bytes: Vec<u8>) -> Result<(), Error> {
        self.sender
            .send(Write::Torque(bytes))
            .map_err(|_| Error::Contract("Params writer stopped"))
    }
}

impl Drop for PendingWrites {
    fn drop(&mut self) {
        // An already stopped writer needs no further signal; otherwise drain queued writes like Params::~Params.
        let _ = self.sender.send(Write::Finish);
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            eprintln!("torqued: Params writer panicked");
        }
    }
}
