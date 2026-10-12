use super::{Command, Worker};
use crate::h264::{startup, state, Config, Output};
use std::{
    fs::File,
    io,
    os::unix::net::UnixStream,
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
};

impl Worker {
    pub fn start<S: Output + 'static>(
        config: Config,
        state: state::Shared,
        factory: impl FnOnce() -> io::Result<S> + Send + 'static,
        configure: impl FnOnce(&mut Config, Option<[u32; 2]>) -> io::Result<()>,
    ) -> io::Result<(Self, Config)> {
        let (commands, receiver) = mpsc::channel();
        let (wake, wake_reader) = UnixStream::pair()?;
        wake_reader.set_nonblocking(true)?;
        let (ready, initialized) = mpsc::sync_channel(1);
        let (main, setup) = startup::channel();
        let thread = thread::Builder::new()
            .name("cluster-h264-out".into())
            .spawn(move || {
                if let Some((output, config)) = setup.open(factory()) {
                    execute(output, &config, &state, &receiver, wake_reader, &ready);
                }
            })?;
        let mut worker = Self {
            chunk_size: 1,
            commands,
            wake,
            thread: Some(thread),
        };
        let config = match main.configure(config, configure) {
            Ok(config) => config,
            Err(error) => {
                worker.join()?;
                return Err(error);
            }
        };
        let result = initialized
            .recv()
            .map_err(|_| io::Error::other("H264 output initialization worker exited"))
            .and_then(|result| result);
        match result {
            Ok(size) => {
                worker.chunk_size = size;
                Ok((worker, config))
            }
            Err(error) => {
                worker.join()?;
                Err(error)
            }
        }
    }
}
fn execute(
    mut output: impl Output,
    config: &Config,
    state: &state::Shared,
    receiver: &Receiver<Command>,
    wake: UnixStream,
    ready: &SyncSender<io::Result<usize>>,
) {
    let size = match output.start(config.requested_chunk_size.max(0)) {
        Ok(size) => size.max(1),
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let dump = config
        .dump
        .as_ref()
        .map(|path| {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            File::create(path)
        })
        .transpose();
    let dump = match dump {
        Ok(dump) => dump,
        Err(error) => {
            output.stop();
            let _ = ready.send(Err(error));
            return;
        }
    };
    let _ = ready.send(Ok(size));
    if let Err(error) =
        crate::h264::stream::run(&mut output, size, config, receiver, wake, state, dump)
    {
        state::failure(state, &error);
        while !state::lock(state).is_ok_and(|state| state.closing) {
            let Ok(command) = receiver.recv() else {
                break;
            };
            match command {
                Command::Stop(reply) => {
                    let _ = reply.send(());
                    break;
                }
                Command::Control(_, reply) => {
                    let _ = reply.send(Err(io::Error::other("H264 stdout worker failed")));
                }
                Command::Source(_) => {}
            }
        }
    }
    output.stop();
}
