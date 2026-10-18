use super::{run, Sender, Shared};
use crate::h264::{startup, Output};
use std::{
    io,
    sync::{mpsc, Arc},
    thread,
};

impl Sender {
    /// # Errors
    /// Preserves output/thread initialization and chunk negotiation failures.
    pub fn start<S: Output + 'static>(
        requested: i64,
        wait_for_ack: bool,
        soft_ack: bool,
        factory: impl FnOnce() -> io::Result<S> + Send + 'static,
    ) -> io::Result<Self> {
        Self::start_with(requested, wait_for_ack, soft_ack, factory, |_| Ok(()))
    }
    /// # Errors
    /// Preserves output opening and main-thread configuration before stream negotiation.
    pub fn start_with<S: Output + 'static>(
        requested: i64,
        wait_for_ack: bool,
        soft_ack: bool,
        factory: impl FnOnce() -> io::Result<S> + Send + 'static,
        configure: impl FnOnce(Option<[u32; 2]>) -> io::Result<()>,
    ) -> io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let worker_shared = Arc::clone(&shared);
        let (initialized, ready) = mpsc::sync_channel(1);
        let (done, finished) = mpsc::channel();
        let (main, worker) = startup::channel();
        let thread = thread::Builder::new()
            .name("cluster-usb-h264-native-send".into())
            .spawn(move || {
                if let Some((mut output, ())) = worker.open(factory()) {
                    if let Ok(mut state) = super::lock(&worker_shared) {
                        state.preserve_references = output.preserve_reference_frames();
                    }
                    match output.start(requested.max(0)) {
                        Err(error) => {
                            let _ = initialized.send(Err(error));
                        }
                        Ok(size) => {
                            let _ = initialized.send(Ok(size.max(1)));
                            if let Err(error) =
                                run(&worker_shared, &mut output, wait_for_ack, soft_ack)
                            {
                                if let Ok(mut state) = super::lock(&worker_shared) {
                                    if !state.closing {
                                        state.snapshot.error = Some(error.to_string());
                                        worker_shared.ready.notify_all();
                                    }
                                }
                            }
                            output.stop();
                        }
                    }
                }
                let _ = done.send(());
            })?;
        if let Err(error) = main.configure((), |(), dimensions| configure(dimensions)) {
            let _ = thread.join();
            return Err(error);
        }
        match ready.recv().map_err(io::Error::other)? {
            Ok(chunk_size) => Ok(Self {
                shared,
                thread: Some(thread),
                finished,
                chunk_size,
            }),
            Err(error) => {
                let _ = thread.join();
                Err(error)
            }
        }
    }
}
