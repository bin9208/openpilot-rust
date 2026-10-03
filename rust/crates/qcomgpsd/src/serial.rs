use crate::{
    framing::{self, Frames},
    Error,
};
use rustix::{
    event::{poll, PollFd, PollFlags},
    fs::{flock, FlockOperation, OFlags},
    termios::{
        self, ControlModes, InputModes, LocalModes, OptionalActions, OutputModes, QueueSelector,
        SpecialCodeIndex,
    },
};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
pub struct Diagnostic {
    file: File,
    pending: Frames,
}
fn ready(file: &File, events: PollFlags, stop: &AtomicBool) -> Result<(), Error> {
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err(Error::Stopped);
        }
        let mut descriptors = [PollFd::new(file, events)];
        let timeout = rustix::time::Timespec::try_from(Duration::from_millis(50))
            .map_err(|_| Error::Protocol("poll timeout"))?;
        match poll(&mut descriptors, Some(&timeout)) {
            Ok(0) | Err(rustix::io::Errno::INTR) => (),
            Ok(_) => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
}
impl Diagnostic {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let flags = i32::try_from((OFlags::NOCTTY | OFlags::NONBLOCK).bits())
            .map_err(|_| Error::Protocol("serial flags"))?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(flags)
            .open(path)?;
        // pyserial exclusive=True is advisory flock, not TIOCEXCL.
        flock(&file, FlockOperation::NonBlockingLockExclusive)?;
        let mut mode = termios::tcgetattr(&file)?;
        mode.control_modes
            .insert(ControlModes::CLOCAL | ControlModes::CREAD | ControlModes::CRTSCTS);
        mode.control_modes.remove(
            ControlModes::CSIZE
                | ControlModes::CSTOPB
                | ControlModes::PARENB
                | ControlModes::PARODD
                | ControlModes::CMSPAR,
        );
        mode.control_modes.insert(ControlModes::CS8);
        mode.local_modes.remove(
            LocalModes::ICANON
                | LocalModes::ECHO
                | LocalModes::ECHOE
                | LocalModes::ECHOK
                | LocalModes::ECHONL
                | LocalModes::ISIG
                | LocalModes::IEXTEN
                | LocalModes::ECHOCTL
                | LocalModes::ECHOKE,
        );
        mode.output_modes
            .remove(OutputModes::OPOST | OutputModes::ONLCR | OutputModes::OCRNL);
        mode.input_modes.remove(
            InputModes::INLCR
                | InputModes::IGNCR
                | InputModes::ICRNL
                | InputModes::IGNBRK
                | InputModes::IUCLC
                | InputModes::PARMRK
                | InputModes::INPCK
                | InputModes::ISTRIP
                | InputModes::IXON
                | InputModes::IXOFF
                | InputModes::IXANY,
        );
        mode.special_codes[SpecialCodeIndex::VMIN] = 0;
        mode.special_codes[SpecialCodeIndex::VTIME] = 0;
        mode.set_speed(115200)?;
        termios::tcsetattr(&file, OptionalActions::Now, &mode)?;
        termios::tcflush(&file, QueueSelector::IFlush)?;
        termios::tcdrain(&file)?;
        termios::tcflush(&file, QueueSelector::IFlush)?;
        termios::tcflush(&file, QueueSelector::OFlush)?;
        Ok(Self {
            file,
            pending: Frames::default(),
        })
    }
    pub fn recv(&mut self, stop: &AtomicBool) -> Result<(u8, Vec<u8>), Error> {
        loop {
            if let Some(packet) = self.pending.next_frame()? {
                return Ok(packet);
            }
            ready(&self.file, PollFlags::IN, stop)?;
            let mut buffer = [0; 0x10000];
            match (&self.file).read(&mut buffer) {
                Ok(0) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "diagnostic serial disconnected",
                    )
                    .into())
                }
                Ok(count) => self.pending.extend(&buffer[..count]),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    pub fn send(&mut self, opcode: u8, payload: &[u8], stop: &AtomicBool) -> Result<(), Error> {
        let mut message = Vec::with_capacity(payload.len() + 1);
        message.push(opcode);
        message.extend_from_slice(payload);
        let frame = framing::encode(&message);
        let mut remaining = frame.as_slice();
        while !remaining.is_empty() {
            match (&self.file).write(remaining) {
                Ok(0) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "diagnostic serial write",
                    )
                    .into())
                }
                Ok(count) => remaining = &remaining[count..],
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    ready(&self.file, PollFlags::OUT, stop)?
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => (),
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    pub fn exchange(
        &mut self,
        opcode: u8,
        payload: &[u8],
        stop: &AtomicBool,
    ) -> Result<(u8, Vec<u8>), Error> {
        self.send(opcode, payload, stop)?;
        loop {
            let packet = self.recv(stop)?;
            if packet.0 != framing::DIAG_LOG {
                return Ok(packet);
            }
        }
    }
}
