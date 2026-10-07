use crate::{
    joystick::{Gamepad, Keyboard, Profile},
    Error,
};
use rustix::{
    fs::{Mode, OFlags},
    termios::{self, LocalModes, OptionalActions, Termios},
};
use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

pub struct Terminal {
    original: Termios,
    flags: OFlags,
}

impl Terminal {
    fn open() -> Result<Self, Error> {
        let fd = rustix::stdio::stdin();
        let original = termios::tcgetattr(fd).map_err(io::Error::from)?;
        let flags = rustix::fs::fcntl_getfl(fd).map_err(io::Error::from)?;
        let owner = Self { original, flags };
        let mut mode = owner.original.clone();
        mode.local_modes
            .remove(LocalModes::ICANON | LocalModes::ECHO);
        termios::tcsetattr(fd, OptionalActions::Flush, &mode).map_err(io::Error::from)?;
        rustix::fs::fcntl_setfl(fd, flags | OFlags::NONBLOCK).map_err(io::Error::from)?;
        Ok(owner)
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let fd = rustix::stdio::stdin();
        if let Err(error) = termios::tcsetattr(fd, OptionalActions::Flush, &self.original) {
            eprintln!("joystick: terminal restoration failed: {error}");
        }
        if let Err(error) = rustix::fs::fcntl_setfl(fd, self.flags) {
            eprintln!("joystick: stdin flags restoration failed: {error}");
        }
    }
}

pub enum Device {
    Keyboard {
        owner: Keyboard,
        _terminal: Terminal,
        pending: Vec<u8>,
    },
    Gamepad {
        owner: Gamepad,
        path: Option<PathBuf>,
        file: Option<File>,
        pending: [u8; 24],
        filled: usize,
    },
}

impl Device {
    pub fn keyboard() -> Result<Self, Error> {
        Ok(Self::Keyboard {
            owner: Keyboard::default(),
            _terminal: Terminal::open()?,
            pending: Vec::with_capacity(4),
        })
    }

    pub fn gamepad(path: Option<PathBuf>) -> Result<Self, Error> {
        let profile = if Path::new("/TICI").is_file() {
            Profile::Tici
        } else {
            Profile::Pc
        };
        let path = match path {
            Some(path) => Some(path),
            None => discover()?,
        };
        Ok(Self::Gamepad {
            owner: Gamepad::new(profile),
            path,
            file: None,
            pending: [0; 24],
            filled: 0,
        })
    }

    pub fn names(&self) -> [&'static str; 2] {
        match self {
            Self::Keyboard { .. } => ["gb", "steer"],
            Self::Gamepad { owner, .. } => owner.names(),
        }
    }

    pub fn update(&mut self) -> Result<Option<[f64; 2]>, Error> {
        match self {
            Self::Keyboard { owner, pending, .. } => {
                let mut byte = [0];
                match rustix::io::read(rustix::stdio::stdin(), &mut byte).map_err(io::Error::from) {
                    Ok(0) => {
                        owner.update("");
                        Ok(None)
                    }
                    Ok(_) => {
                        pending.push(byte[0]);
                        match std::str::from_utf8(pending) {
                            Ok(key) => {
                                owner.update(key);
                                pending.clear();
                                Ok(Some(owner.axes))
                            }
                            Err(error) if error.error_len().is_none() && pending.len() < 4 => {
                                Ok(None)
                            }
                            Err(_) => Err(Error::Contract("stdin contains invalid UTF-8")),
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) =>
                    {
                        Ok(None)
                    }
                    Err(error) => Err(error.into()),
                }
            }
            Self::Gamepad {
                owner,
                path,
                file,
                pending,
                filled,
            } => {
                if file.is_none() {
                    let Some(path) = path else {
                        return Ok(None);
                    };
                    match rustix::fs::open(
                        path.as_path(),
                        OFlags::RDONLY | OFlags::NONBLOCK,
                        Mode::empty(),
                    ) {
                        Ok(fd) => *file = Some(File::from(fd)),
                        Err(_) => {
                            owner.disconnected();
                            return Ok(Some(owner.axes));
                        }
                    }
                }
                let Some(file) = file else {
                    return Err(Error::Contract("gamepad file was not opened"));
                };
                match file.read(&mut pending[*filled..]) {
                    Ok(0) => Ok(None),
                    Ok(count) => {
                        *filled += count;
                        if *filled < pending.len() {
                            return Ok(Some(owner.axes));
                        }
                        let kind = u16::from_ne_bytes([pending[16], pending[17]]);
                        let code = u16::from_ne_bytes([pending[18], pending[19]]);
                        let value = i32::from_ne_bytes([
                            pending[20],
                            pending[21],
                            pending[22],
                            pending[23],
                        ]);
                        let name = match (kind, code) {
                            (1, 307) => "BTN_NORTH",
                            (3, 2) => "ABS_Z",
                            (3, 3) => "ABS_RX",
                            (3, 4) => "ABS_RY",
                            (3, 5) => "ABS_RZ",
                            _ => "unmapped",
                        };
                        owner.update(name, value)?;
                        *filled = 0;
                        Ok(Some(owner.axes))
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) =>
                    {
                        Ok(None)
                    }
                    Err(_) => {
                        owner.disconnected();
                        *filled = 0;
                        Ok(Some(owner.axes))
                    }
                }
            }
        }
    }
}

fn discover() -> Result<Option<PathBuf>, Error> {
    for directory in ["/dev/input/by-id", "/dev/input/by-path"] {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let path = entry?.path();
            if path
                .file_name()
                .is_some_and(|name| name.as_encoded_bytes().ends_with(b"-event-joystick"))
            {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}
