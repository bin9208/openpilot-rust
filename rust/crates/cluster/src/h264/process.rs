use super::state;
use rustix::process::{kill_process, Pid, Signal};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
};
use std::{
    fs::File,
    io::{self, Read},
    os::fd::OwnedFd,
    process::{Child, ChildStderr},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub(super) fn stderr(pipe: ChildStderr, state: state::Shared) -> io::Result<JoinHandle<()>> {
    let pipe: OwnedFd = pipe.into();
    fcntl_setfl(&pipe, fcntl_getfl(&pipe)? | OFlags::NONBLOCK)?;
    thread::Builder::new()
        .name("cluster-h264-err".into())
        .spawn(move || {
            let mut input = File::from(pipe);
            let mut bytes = Vec::new();
            let mut closing = None;
            let timeout = Timespec::try_from(Duration::from_millis(100)).ok();
            loop {
                if state::lock(&state).is_ok_and(|state| state.child_closed) {
                    let started = closing.get_or_insert_with(Instant::now);
                    if started.elapsed() >= Duration::from_secs(1) {
                        break;
                    }
                }
                let mut buffer = [0; 4096];
                match input.read(&mut buffer) {
                    Ok(0) => {
                        line(&state, &bytes);
                        break;
                    }
                    Ok(length) => {
                        bytes.extend_from_slice(&buffer[..length]);
                        while let Some(index) = bytes.iter().position(|byte| *byte == b'\n') {
                            line(&state, &bytes[..index]);
                            bytes.drain(..=index);
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        let _ = poll(&mut [PollFd::new(&input, PollFlags::IN)], timeout.as_ref());
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
        })
}

fn line(state: &state::Shared, bytes: &[u8]) {
    let text = String::from_utf8_lossy(bytes).trim().to_owned();
    if !text.is_empty() {
        if let Ok(mut state) = state::lock(state) {
            if state.stderr_tail.len() == 20 {
                state.stderr_tail.pop_front();
            }
            if let Some(monitor) = &state.monitor {
                if let Err(error) = monitor.stderr(&text) {
                    eprintln!("H264 stderr observation: {error}");
                }
            }
            state.stderr_tail.push_back(text);
        }
    }
}

fn wait(child: &mut Child, timeout: Duration) -> io::Result<bool> {
    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            return Ok(true);
        }
        if started.elapsed() >= timeout {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn close(child: &mut Child) -> io::Result<()> {
    if wait(child, Duration::from_secs(3))? {
        return Ok(());
    }
    let pid = i32::try_from(child.id())
        .ok()
        .and_then(Pid::from_raw)
        .ok_or_else(|| io::Error::other("invalid H264 encoder child pid"))?;
    match kill_process(pid, Signal::TERM) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => {}
        Err(error) => return Err(error.into()),
    }
    if wait(child, Duration::from_secs(2))? {
        return Ok(());
    }
    child.kill()?;
    if !wait(child, Duration::from_secs(2))? {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "H264 encoder did not exit after kill",
        ));
    }
    Ok(())
}
