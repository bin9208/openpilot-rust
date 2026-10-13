use super::{Client, Command, Service};
use crate::Error;
use tokio::sync::oneshot;

impl Service {
    pub(crate) async fn ensure(&self) -> Result<(), Error> {
        let (reply, result) = oneshot::channel();
        self.submit(Command::Ensure(reply))?;
        result
            .await
            .map_err(|_| Error::Source("terminal owner unavailable".into()))?
    }
    pub(crate) async fn attach_guest(&self, client: Client) -> Result<u64, Error> {
        let (reply, result) = oneshot::channel();
        self.submit(Command::GuestAttach { client, reply })?;
        result
            .await
            .map_err(|_| Error::Source("terminal owner unavailable".into()))?
    }
    pub(crate) async fn detach_guest(&self, id: u64) -> Result<(), Error> {
        let (reply, result) = oneshot::channel();
        self.submit(Command::GuestDetach { id, reply })?;
        result
            .await
            .map_err(|_| Error::Source("terminal owner unavailable".into()))
    }
    pub(crate) fn detach_guest_now(&self, id: u64) {
        if let Err(error) = self.submit(Command::Detach(id)) {
            eprintln!("Support terminal detach: {error}");
        }
    }
    pub(crate) async fn clear_history(&self) -> Result<(), Error> {
        let (reply, result) = oneshot::channel();
        self.submit(Command::Clear(reply))?;
        result
            .await
            .map_err(|_| Error::Source("terminal owner unavailable".into()))?
    }
    pub(crate) fn translate_support(&self, line: &str) -> String {
        super::protocol::translate(&self.config, line, false)
    }
}
