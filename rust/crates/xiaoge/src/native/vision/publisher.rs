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
        let payload = crate::wire::compact(
            &shared
                .state()?
                .publication(platform::monotonic()?, platform::timestamp()?)?,
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
        wait.recv()
            .map_err(|_| Error::Contract("publisher dropped result"))?
    }
}
