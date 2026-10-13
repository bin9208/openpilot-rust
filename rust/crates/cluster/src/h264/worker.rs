mod start;
use super::Control;
use std::{
    io::{self, Write},
    os::unix::net::UnixStream,
    process::ChildStdout,
    sync::mpsc::{self, Sender, SyncSender},
    thread::JoinHandle,
};

pub(super) enum Command {
    Source(ChildStdout),
    Control(Control, SyncSender<io::Result<()>>),
    Stop(SyncSender<()>),
}

pub(super) struct Worker {
    pub chunk_size: usize,
    commands: Sender<Command>,
    wake: UnixStream,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn source(&self, stdout: ChildStdout) -> io::Result<()> {
        self.commands
            .send(Command::Source(stdout))
            .map_err(io::Error::other)
    }
    pub fn control(&mut self, control: Control) -> io::Result<()> {
        let (reply, result) = mpsc::sync_channel(1);
        self.commands
            .send(Command::Control(control, reply))
            .map_err(io::Error::other)?;
        self.wake.write_all(&[1])?;
        result.recv().map_err(io::Error::other)?
    }
    pub fn close(&mut self) -> io::Result<()> {
        if self.thread.is_none() {
            return Ok(());
        }
        let (reply, result) = mpsc::sync_channel(1);
        if self.commands.send(Command::Stop(reply)).is_ok() {
            let _ = self.wake.write_all(&[1]);
            let _ = result.recv();
        }
        self.join()
    }
    fn join(&mut self) -> io::Result<()> {
        self.thread.take().map_or(Ok(()), |thread| {
            thread
                .join()
                .map_err(|_| io::Error::other("H264 output worker panicked"))
        })
    }
}
