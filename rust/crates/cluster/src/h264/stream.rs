use super::{state, worker::Command, Config, Output};
use crate::usb::H264Chunk;
use rustix::event::{poll, PollFd, PollFlags, Timespec};
use std::{
    fs::File,
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    sync::mpsc::Receiver,
    time::{Duration, Instant},
};

pub(super) fn run(
    output: &mut impl Output,
    size: usize,
    config: &Config,
    commands: &Receiver<Command>,
    mut wake: UnixStream,
    state: &state::Shared,
    mut dump: Option<File>,
) -> io::Result<()> {
    let mut stdout = match commands.recv().map_err(io::Error::other)? {
        Command::Source(stdout) => stdout,
        Command::Stop(reply) => {
            let _ = reply.send(());
            return Ok(());
        }
        Command::Control(_, _) => return Err(io::Error::other("H264 source pipe not initialized")),
    };
    let mut bytes = vec![0; size];
    let mut eof = false;
    let mut stop = None;
    let mut writes = 0_u32;
    loop {
        for command in commands.try_iter() {
            match command {
                Command::Control(value, reply) => {
                    let _ = reply.send(output.control(value));
                }
                Command::Stop(reply) => {
                    stop = Some((reply, Instant::now() + Duration::from_secs(3)));
                }
                Command::Source(_) => return Err(io::Error::other("duplicate H264 stdout pipe")),
            }
        }
        if stop
            .as_ref()
            .is_some_and(|(_, deadline)| eof || Instant::now() >= *deadline)
        {
            if let Some(dump) = &mut dump {
                dump.flush()?;
            }
            if let Some((reply, _)) = stop.take() {
                let _ = reply.send(());
            }
            return Ok(());
        }
        let (wake_ready, data_ready) = {
            let mut descriptors = vec![PollFd::new(&wake, PollFlags::IN)];
            if !eof {
                descriptors.push(PollFd::new(&stdout, PollFlags::IN));
            }
            let timeout = stop
                .as_ref()
                .map(|(_, deadline)| {
                    Timespec::try_from(deadline.saturating_duration_since(Instant::now()))
                })
                .transpose()
                .map_err(io::Error::other)?;
            match poll(&mut descriptors, timeout.as_ref()) {
                Ok(_) => {}
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
            }
            (
                !descriptors[0].revents().is_empty(),
                descriptors
                    .get(1)
                    .is_some_and(|fd| !fd.revents().is_empty()),
            )
        };
        if wake_ready {
            let mut buffer = [0; 64];
            while wake.read(&mut buffer).is_ok_and(|count| count > 0) {}
        }
        if data_ready {
            let count = match stdout.read(&mut bytes) {
                Ok(count) => count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            if count == 0 {
                eof = true;
                continue;
            }
            if let Some(dump) = &mut dump {
                dump.write_all(&bytes[..count])?;
                writes = writes.wrapping_add(1);
                if writes.is_multiple_of(config.fps.max(1)) {
                    dump.flush()?;
                }
            }
            send(output, &bytes[..count], config, state)?;
            let mut state = state::lock(state)?;
            state.bytes_sent += count;
            state.chunks_sent += 1;
        }
    }
}

fn send(
    output: &mut impl Output,
    bytes: &[u8],
    config: &Config,
    state: &state::Shared,
) -> io::Result<()> {
    let monitor = state::lock(state)?.monitor.clone();
    if let Some(monitor) = &monitor {
        monitor.unit(bytes, &[bytes.len()], false)?;
    }
    if let Some(monitor) = &monitor {
        monitor.sending(bytes)?;
    }
    let started = Instant::now();
    output.send(H264Chunk {
        bytes,
        last: false,
        wait_for_ack: config.wait_for_ack,
        require_ack: !config.soft_ack,
    })?;
    if let Some(monitor) = monitor {
        monitor.sent(bytes, started.elapsed().as_secs_f64() * 1000.0)?;
        monitor.samples(output.drain_profile_samples())?;
    }
    Ok(())
}
