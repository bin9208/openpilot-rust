use std::{
    fs::File,
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::mpsc::Receiver,
    thread::JoinHandle,
};
use tokio::sync::oneshot::Sender;

fn listener_error(probe: PathBuf, released: Receiver<()>) -> io::Result<()> {
    use rustix::process::{getrlimit, setrlimit, Resource, Rlimit};
    let previous = getrlimit(Resource::Nofile);
    setrlimit(
        Resource::Nofile,
        Rlimit {
            current: Some(128),
            maximum: previous.maximum,
        },
    )?;
    let mut files = Vec::new();
    loop {
        match File::open(&probe) {
            Ok(file) => files.push(file),
            Err(error) if error.raw_os_error() == Some(rustix::io::Errno::MFILE.raw_os_error()) => {
                break
            }
            Err(error) => return Err(error),
        }
    }
    println!("{{\"limited\":true}}");
    io::stdout().flush()?;
    let result = released.recv().map_err(io::Error::other);
    drop(files);
    setrlimit(Resource::Nofile, previous)?;
    result
}

pub(super) fn input(
    stop: Sender<()>,
    released: Receiver<()>,
    probe: PathBuf,
) -> JoinHandle<io::Result<()>> {
    std::thread::spawn(move || {
        let mut command = String::new();
        io::stdin().lock().read_line(&mut command)?;
        if command.trim() == "error" {
            listener_error(probe, released)?;
        } else {
            if stop.send(()).is_err() {
                eprintln!("fixture server already stopped");
            }
            released.recv().map_err(io::Error::other)?;
        }
        Ok(())
    })
}
