use openpilot_params::Params;
use std::{
    io,
    sync::{mpsc, Arc},
    thread::{self, JoinHandle},
};

pub struct WriteFailure {
    pub key: String,
    pub error: openpilot_params::Error,
}

struct Task {
    sender: mpsc::Sender<(String, Vec<u8>)>,
    worker: JoinHandle<Vec<WriteFailure>>,
}

fn spawn(
    write: impl Fn(&str, &[u8]) -> Result<(), openpilot_params::Error> + Send + 'static,
) -> io::Result<Task> {
    let (sender, receiver) = mpsc::channel::<(String, Vec<u8>)>();
    let worker = thread::Builder::new()
        .name("card-params".into())
        .spawn(move || {
            let mut failures = Vec::new();
            for (key, value) in receiver {
                if let Err(error) = write(&key, &value) {
                    failures.push(WriteFailure { key, error });
                }
            }
            failures
        })?;
    Ok(Task { sender, worker })
}

/// FIFO parameter writes with the source Params destructor's drain guarantee.
pub struct AsyncParams {
    settings: Arc<Params>,
    task: Option<Task>,
    failures: Vec<WriteFailure>,
}

impl AsyncParams {
    pub fn new(settings: Arc<Params>) -> Self {
        Self {
            settings,
            task: None,
            failures: Vec::new(),
        }
    }

    pub fn put(&mut self, key: &str, value: &[u8]) -> Result<(), crate::core::Error> {
        openpilot_params::metadata(key)
            .ok_or_else(|| openpilot_params::Error::UnknownKey(key.to_owned()))?;
        if self.task.is_none() {
            let settings = Arc::clone(&self.settings);
            self.task = Some(spawn(move |key, value| settings.put(key, value))?);
        }
        let task = self.task.as_ref().ok_or(crate::core::Error::Event(
            "parameter worker was not started",
        ))?;
        task.sender
            .send((key.to_owned(), value.to_vec()))
            .map_err(|_| io::Error::other("parameter worker terminated"))?;
        Ok(())
    }

    pub fn finish(&mut self) {
        if let Some(Task { sender, worker }) = self.task.take() {
            drop(sender);
            match worker.join() {
                Ok(failures) => self.failures.extend(failures),
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
    }

    pub fn failures(&self) -> &[WriteFailure] {
        &self.failures
    }
}

impl Drop for AsyncParams {
    fn drop(&mut self) {
        // Source asyncWriteThread ignores put's result; retain failures for an
        // explicit observer without changing control behavior or adding logs.
        self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_enqueue_returns_while_write_is_blocked_and_keeps_repeated_keys() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (written_tx, written_rx) = mpsc::channel();
        let mut first = true;
        let task = spawn(move |key, bytes| {
            if key == "first" {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            }
            written_tx.send((key.to_owned(), bytes.to_vec())).unwrap();
            if key == "failure" {
                Err(openpilot_params::Error::UnknownKey(key.into()))
            } else {
                Ok(())
            }
        })
        .unwrap();
        for (key, bytes) in [("first", b"1"), ("failure", b"2"), ("first", b"3")] {
            task.sender.send((key.into(), bytes.to_vec())).unwrap();
            if first {
                entered_rx.recv().unwrap();
                first = false;
            }
        }
        release_tx.send(()).unwrap();
        release_tx.send(()).unwrap();
        drop(task.sender);
        let failures = task.worker.join().unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].key, "failure");
        assert_eq!(
            written_rx.into_iter().collect::<Vec<_>>(),
            [
                ("first".into(), b"1".to_vec()),
                ("failure".into(), b"2".to_vec()),
                ("first".into(), b"3".to_vec())
            ]
        );
    }

    #[test]
    fn dropping_drains_every_queued_parameter_write() {
        let directory = tempfile::tempdir().unwrap();
        let settings = Arc::new(Params::open(directory.path(), "d").unwrap());
        {
            let mut writer = AsyncParams::new(Arc::clone(&settings));
            writer.put("CarParamsCache", b"first").unwrap();
            writer.put("CarParamsPersistent", b"persistent").unwrap();
            writer.put("CarParamsCache", b"last").unwrap();
        }
        assert_eq!(settings.get("CarParamsCache").unwrap().unwrap(), b"last");
        assert_eq!(
            settings.get("CarParamsPersistent").unwrap().unwrap(),
            b"persistent"
        );
    }
}
