use super::super::{platform, shared::Shared, Error};
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::runtime::PubMaster;
use std::{sync::mpsc, thread};

struct Publication {
    message: Vec<u8>,
    complete: mpsc::SyncSender<Result<(), Error>>,
}

#[derive(Clone)]
pub struct Publisher(mpsc::SyncSender<Publication>);

impl Publisher {
    pub fn start() -> Result<Self, Error> {
        let (sender, receiver) = mpsc::sync_channel::<Publication>(1);
        let (ready, wait) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("xiaoge-publisher".to_owned())
            .spawn(move || {
                let mut publisher = match PubMaster::for_runtime(&["customReservedRawData0"]) {
                    Ok(publisher) => publisher,
                    Err(error) => {
                        if ready.send(Err(Error::from(error))).is_err() {
                            eprintln!("Xiaoge publisher startup receiver closed");
                        }
                        return;
                    }
                };
                if ready.send(Ok(())).is_err() {
                    return;
                }
                while let Ok(publication) = receiver.recv() {
                    let result = publisher
                        .send("customReservedRawData0", &publication.message)
                        .map_err(Error::from);
                    if publication.complete.send(result).is_err() {
                        eprintln!("Xiaoge publication caller closed");
                    }
                }
            })?;
        wait.recv()
            .map_err(|_| Error::Contract("publisher startup failed"))??;
        Ok(Self(sender))
    }

    pub fn publish(&self, shared: &Shared) -> Result<(), Error> {
        shared.refresh(false)?;
        // Keep selection/result transitions ordered with the actual owner send.
        // An unversioned detached snapshot could otherwise overtake a mode switch
        // and reassert a previous mode's valid result after the cleared state.
        // The owner never acquires state; its channel acknowledgment closes the
        // publication before refresh/settings/other workers can commit a change.
        let state = shared.state()?;
        let payload = crate::wire::compact(
            &state.publication(platform::monotonic()?, platform::timestamp()?)?,
        )?;
        let mut message = capnp::message::Builder::new_default();
        let mut root = message.init_root::<event::Builder>();
        root.set_log_mono_time(platform::timestamp()?);
        root.set_valid(true);
        root.set_custom_reserved_raw_data0(&payload);
        let (complete, wait) = mpsc::sync_channel(1);
        self.0
            .send(Publication {
                message: capnp::serialize::write_message_to_words(&message),
                complete,
            })
            .map_err(|_| Error::Contract("publisher owner stopped"))?;
        let result = wait
            .recv()
            .map_err(|_| Error::Contract("publisher dropped result"))?;
        drop(state);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config,
        service::{Model, State},
    };
    use std::sync::{atomic::AtomicBool, Arc, Condvar, Mutex, TryLockError};
    use std::time::Duration;

    #[test]
    fn publication_completion_excludes_mode_and_result_transitions() {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::new(
                Config::default(),
                ["bsd", "lane"].map(|path| Model {
                    path: path.into(),
                    loaded: true,
                    error: String::new(),
                }),
                platform::monotonic().unwrap(),
            )),
            snapshot: Condvar::new(),
            vasm_operation: Mutex::new(()),
            params: None,
            params_operation: Mutex::new(()),
            config_path: std::path::PathBuf::new(),
            running: AtomicBool::new(true),
        });
        // Substitute only the transport owner so the test can pause precisely
        // after enqueue, before the real send would finish, without IPC or GPU.
        let (send, receive) = mpsc::sync_channel(1);
        let publisher = Publisher(send);
        let caller_shared = Arc::clone(&shared);
        let caller = thread::spawn(move || publisher.publish(&caller_shared));
        let publication = receive.recv_timeout(Duration::from_secs(5)).unwrap();
        let excluded = matches!(shared.state.try_lock(), Err(TryLockError::WouldBlock));
        // Always acknowledge before asserting so a failed regression cannot
        // strand the caller waiting for the substituted owner.
        publication.complete.send(Ok(())).unwrap();
        caller.join().unwrap().unwrap();
        assert!(
            excluded,
            "a mode/result update could overtake a queued publication"
        );
        assert!(
            shared.state.try_lock().is_ok(),
            "state released after actual send acknowledgment"
        );
    }
}
