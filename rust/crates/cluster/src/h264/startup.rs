use super::Output;
use std::{
    io,
    sync::mpsc::{self, Receiver, SyncSender},
};

pub(super) struct Worker<C> {
    opened: SyncSender<io::Result<Option<[u32; 2]>>>,
    configured: Receiver<C>,
}
pub(super) struct Main<C> {
    opened: Receiver<io::Result<Option<[u32; 2]>>>,
    configured: SyncSender<C>,
}

pub(super) fn channel<C>() -> (Main<C>, Worker<C>) {
    let (opened, received) = mpsc::sync_channel(1);
    let (configured, confirmed) = mpsc::sync_channel(1);
    (
        Main {
            opened: received,
            configured,
        },
        Worker {
            opened,
            configured: confirmed,
        },
    )
}
impl<C> Worker<C> {
    pub fn open<S: Output>(self, result: io::Result<S>) -> Option<(S, C)> {
        match result {
            Err(error) => {
                let _ = self.opened.send(Err(error));
                None
            }
            Ok(output) => {
                self.opened.send(Ok(output.dimensions())).ok()?;
                let config = self.configured.recv().ok()?;
                Some((output, config))
            }
        }
    }
}
impl<C: Clone> Main<C> {
    pub fn configure(
        self,
        mut config: C,
        update: impl FnOnce(&mut C, Option<[u32; 2]>) -> io::Result<()>,
    ) -> io::Result<C> {
        let dimensions = self.opened.recv().map_err(io::Error::other)??;
        update(&mut config, dimensions)?;
        self.configured
            .send(config.clone())
            .map_err(|_| io::Error::other("output startup worker exited"))?;
        Ok(config)
    }
}
