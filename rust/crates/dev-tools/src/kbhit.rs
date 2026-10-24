//! Linux KBHit port with explicit descriptors and an opt-in stdin adapter.
//! No terminal mode restoration on Drop; explicit reset/atexit retain source order.
//! Copyright (c) 2018, Comma.ai, Inc.; repository MIT license applies.
//! Adapted from openpilot/tools/lib/kbhit.py. Python TextIOWrapper/exit limits
//! are documented in the source candidate's HANDOFF, not full Python parity.
use rustix::termios::{self, LocalModes, OptionalActions, Termios};
use std::{
    fmt,
    io::{self, Read, Write},
    os::fd::{AsFd, BorrowedFd, OwnedFd},
    sync::{Arc, Mutex},
};

#[allow(unsafe_code)] // Checked libc select/atexit and test-only private PTY calls.
mod system;
#[cfg(test)]
mod tests;

#[derive(Default)]
struct Snapshots {
    old: Option<Termios>,
    new: Option<Termios>,
}
struct State {
    fd: OwnedFd,
    settings: Mutex<Snapshots>,
}
impl State {
    fn restore(&self) -> io::Result<()> {
        let settings = self
            .settings
            .lock()
            .map_err(|_| io::Error::other("KBHit settings poisoned"))?;
        let old = settings
            .old
            .as_ref()
            .ok_or_else(|| io::Error::other("KBHit has no saved settings"))?;
        termios::tcsetattr(&self.fd, OptionalActions::Flush, old).map_err(io::Error::from)
    }
}

#[derive(Default)]
struct ExitStack {
    entries: Vec<Arc<State>>,
}
static EXIT_STACK: Mutex<ExitStack> = Mutex::new(ExitStack {
    entries: Vec::new(),
});

fn restore_next(stack: &Mutex<ExitStack>) -> io::Result<()> {
    // Release the stack lock before accessing object state (set uses the opposite order).
    let entry = stack
        .lock()
        .map_err(|_| io::Error::other("KBHit exit stack poisoned"))?
        .entries
        .pop();
    if let Some(state) = entry {
        state.restore()?;
    }
    Ok(())
}
extern "C" fn exit_restore_one() {
    // Every successful registration gets one callback; errors cannot cross C ABI.
    let result = std::panic::catch_unwind(|| restore_next(&EXIT_STACK));
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let _ = writeln!(io::stderr().lock(), "KBHit atexit reset failed: {error}");
        }
        Err(_) => {
            let _ = writeln!(io::stderr().lock(), "KBHit atexit reset panicked");
        }
    }
}
fn register_exit(state: Arc<State>) -> io::Result<()> {
    let mut stack = EXIT_STACK
        .lock()
        .map_err(|_| io::Error::other("KBHit exit stack poisoned"))?;
    stack.entries.push(state);
    if let Err(error) = system::register_exit(exit_restore_one) {
        stack.entries.pop();
        return Err(error);
    }
    Ok(())
}

/// Owned descriptor replacement for the original constructor's stdin.fileno().
/// Exit registrations retain the descriptor/object after a handle is dropped.
/// No Drop restores modes; explicit reset and ordinary C atexit are separate effects.
pub struct KBHit {
    state: Arc<State>,
}
fn duplicate_input(fd: BorrowedFd<'_>) -> io::Result<OwnedFd> {
    rustix::io::fcntl_dupfd_cloexec(fd, 0).map_err(io::Error::from)
}
impl KBHit {
    /// Native callers explicitly opt into the process stdin. Duplicate the descriptor,
    /// never open /dev/tty or change nonblocking flags; retained exit state owns it.
    pub fn from_stdin() -> io::Result<Self> {
        Self::new(duplicate_input(rustix::stdio::stdin())?)
    }
    /// Descriptor-bound methods let migrated callers use the same descriptor for
    /// readiness and character reads, with no TextIOWrapper read-ahead discrepancy.
    pub fn has_input(&self) -> io::Result<bool> {
        Self::kbhit(self.stdin_fd())
    }
    pub fn read_char(&self) -> io::Result<Option<char>> {
        let mut reader = self;
        Self::getch(&mut reader)
    }
    pub fn read_arrow(&self) -> Result<u8, ArrowError> {
        let mut reader = self;
        Self::getarrow(&mut reader)
    }
    pub fn new(fd: OwnedFd) -> io::Result<Self> {
        Self::create(fd, register_exit)
    }
    fn create(
        fd: OwnedFd,
        register: impl FnOnce(Arc<State>) -> io::Result<()>,
    ) -> io::Result<Self> {
        let kb = Self {
            state: Arc::new(State {
                fd,
                settings: Mutex::new(Snapshots::default()),
            }),
        };
        kb.set_kbhit_with(register)?;
        Ok(kb)
    }
    pub fn stdin_fd(&self) -> BorrowedFd<'_> {
        self.state.fd.as_fd()
    }
    pub fn old_term(&self) -> io::Result<Termios> {
        self.state
            .settings
            .lock()
            .map_err(|_| io::Error::other("KBHit settings poisoned"))?
            .old
            .clone()
            .ok_or_else(|| io::Error::other("KBHit has no saved settings"))
    }
    pub fn new_term(&self) -> io::Result<Termios> {
        self.state
            .settings
            .lock()
            .map_err(|_| io::Error::other("KBHit settings poisoned"))?
            .new
            .clone()
            .ok_or_else(|| io::Error::other("KBHit has no new settings"))
    }
    pub fn set_kbhit_terminal(&self) -> io::Result<()> {
        self.set_kbhit_with(register_exit)
    }
    fn set_kbhit_with(
        &self,
        register: impl FnOnce(Arc<State>) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut settings = self
            .state
            .settings
            .lock()
            .map_err(|_| io::Error::other("KBHit settings poisoned"))?;
        let old = termios::tcgetattr(&self.state.fd).map_err(io::Error::from)?;
        let mut new = old.clone();
        new.local_modes
            .remove(LocalModes::ICANON | LocalModes::ECHO);
        // Python assigns both snapshots before tcsetattr; a later failure does not roll back.
        settings.old = Some(old);
        settings.new = Some(new.clone());
        termios::tcsetattr(&self.state.fd, OptionalActions::Flush, &new)
            .map_err(io::Error::from)?;
        // Keep order: tcgetattr -> assigned snapshots -> TCSAFLUSH -> one registration.
        register(Arc::clone(&self.state))
    }
    pub fn set_normal_term(&self) -> io::Result<()> {
        self.state.restore()
    }
    /// Strict UTF8 scalar from an explicit reader; None models Python read(1)=="".
    /// No read-ahead/newline conversion/locale/error-handler or sys.stdin lookup.
    pub fn getch(reader: &mut impl Read) -> io::Result<Option<char>> {
        let Some(first) = read_byte(reader)? else {
            return Ok(None);
        };
        let width = match first {
            0..=0x7f => 1,
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid UTF8 leading byte",
                ));
            }
        };
        let mut bytes = [0; 4];
        bytes[0] = first;
        for byte in &mut bytes[1..width] {
            *byte = read_byte(reader)?.ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "truncated UTF8 character")
            })?;
        }
        let text = std::str::from_utf8(&bytes[..width])
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        Ok(text.chars().next())
    }
    /// Consume three Unicode scalars, then index the third and search A,C,B,D.
    /// Prefix bytes are intentionally NOT checked by the original method.
    pub fn getarrow(reader: &mut impl Read) -> Result<u8, ArrowError> {
        let mut third = None;
        for index in 0..3 {
            third = Self::getch(reader).map_err(ArrowError::Read)?;
            if third.is_none() {
                return Err(ArrowError::IndexError { read: index });
            }
        }
        let third = third.expect("three admitted scalars");
        ['A', 'C', 'B', 'D']
            .iter()
            .position(|&value| value == third)
            .map(|index| index as u8)
            .ok_or(ArrowError::ValueError(third))
    }
    /// Zero-timeout select on an explicitly supplied descriptor, including readable EOF.
    /// Descriptor range errors are reported before FD_SET; select EINTR retries.
    pub fn kbhit(fd: BorrowedFd<'_>) -> io::Result<bool> {
        system::readable(fd)
    }
}
impl Read for &KBHit {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        rustix::io::read(self.stdin_fd(), buffer).map_err(io::Error::from)
    }
}
fn read_byte(reader: &mut impl Read) -> io::Result<Option<u8>> {
    let mut byte = [0];
    loop {
        match reader.read(&mut byte) {
            Ok(0) => return Ok(None),
            Ok(1) => return Ok(Some(byte[0])),
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "reader exceeded byte buffer",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}
#[derive(Debug)]
pub enum ArrowError {
    Read(io::Error),
    IndexError { read: usize },
    ValueError(char),
}
impl fmt::Display for ArrowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(f, "{error}"),
            Self::IndexError { .. } => f.write_str("string index out of range"),
            Self::ValueError(character) => write!(f, "{} is not in list", u32::from(*character)),
        }
    }
}
impl std::error::Error for ArrowError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            _ => None,
        }
    }
}
