use crate::{Address, Event, Seconds};
use indexmap::IndexMap;
use num_traits::ToPrimitive;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    os::{
        fd::{AsFd, BorrowedFd},
        unix::fs::{FileTypeExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const EVENT_BYTES: usize = 24;
const _: () = assert!(size_of::<libc::c_long>() == 8);

#[derive(Debug, thiserror::Error)]
pub enum InputError {
    #[error("Bluetooth input reader stopped")]
    Stopped,
    #[error("HID device disconnected or incomplete event")]
    Disconnected,
    #[error("input timestamp cannot be converted to binary64")]
    Timestamp,
    #[error("{}", io_message(.0, None))]
    Io(#[from] io::Error),
    #[error("{}", io_message(.source, Some(.path.as_path())))]
    Open { path: PathBuf, source: io::Error },
    #[error(transparent)]
    Permission(#[from] crate::input_permissions::PermissionError),
}

pub enum InputBatch {
    Pending,
    Events(Vec<Event>),
}

pub struct Input(File);

impl AsFd for Input {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl Input {
    pub fn open(path: &Path) -> Result<Self, InputError> {
        Self::open_interruptible(path, &AtomicBool::new(false))
    }

    pub fn open_interruptible(path: &Path, stop: &AtomicBool) -> Result<Self, InputError> {
        if stop.load(Ordering::Relaxed) {
            return Err(InputError::Stopped);
        }
        let open = || {
            OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(path)
        };
        let file = match open() {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                let candidate = path.parent() == Some(Path::new("/dev/input"))
                    && path
                        .file_name()
                        .is_some_and(|name| name.as_encoded_bytes().starts_with(b"event"))
                    && fs::metadata(path)
                        .is_ok_and(|metadata| metadata.file_type().is_char_device());
                if !candidate {
                    return Err(InputError::Open {
                        path: path.to_owned(),
                        source: error,
                    });
                }
                crate::input_permissions::grant(path, stop)?;
                open().map_err(|source| InputError::Open {
                    path: path.to_owned(),
                    source,
                })?
            }
            Err(source) => {
                return Err(InputError::Open {
                    path: path.to_owned(),
                    source,
                })
            }
        };
        Self::claim(file, stop)
    }

    fn claim(file: File, stop: &AtomicBool) -> Result<Self, InputError> {
        crate::input_kernel::claim(&file)?;
        let mut owned = Self(file);
        let mut buffer = [0; EVENT_BYTES * 64];
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err(InputError::Stopped);
            }
            match owned.0.read(&mut buffer) {
                Ok(0) => return Ok(owned),
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(owned),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
    }

    pub fn read(&mut self) -> Result<InputBatch, InputError> {
        self.read_interruptible(&AtomicBool::new(false))
    }

    pub fn read_interruptible(&mut self, stop: &AtomicBool) -> Result<InputBatch, InputError> {
        let mut buffer = [0; EVENT_BYTES * 128];
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err(InputError::Stopped);
            }
            match self.0.read(&mut buffer) {
                Ok(length) => return Ok(InputBatch::Events(decode_events(&buffer[..length])?)),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    return Ok(InputBatch::Pending)
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
}

pub fn decode_events(bytes: &[u8]) -> Result<Vec<Event>, InputError> {
    if bytes.is_empty() || !bytes.len().is_multiple_of(EVENT_BYTES) {
        return Err(InputError::Disconnected);
    }
    bytes
        .chunks_exact(EVENT_BYTES)
        .map(|bytes| {
            let field = |start, end| bytes.get(start..end).ok_or(InputError::Disconnected);
            let seconds = i64::from_ne_bytes(
                field(0, 8)?
                    .try_into()
                    .map_err(|_| InputError::Disconnected)?,
            );
            let micros = i64::from_ne_bytes(
                field(8, 16)?
                    .try_into()
                    .map_err(|_| InputError::Disconnected)?,
            );
            Ok(Event {
                at: Seconds(
                    seconds.to_f64().ok_or(InputError::Timestamp)?
                        + micros.to_f64().ok_or(InputError::Timestamp)? / 1e6,
                ),
                kind: u16::from_ne_bytes(
                    field(16, 18)?
                        .try_into()
                        .map_err(|_| InputError::Disconnected)?,
                ),
                code: u16::from_ne_bytes(
                    field(18, 20)?
                        .try_into()
                        .map_err(|_| InputError::Disconnected)?,
                ),
                value: i32::from_ne_bytes(
                    field(20, 24)?
                        .try_into()
                        .map_err(|_| InputError::Disconnected)?,
                ),
            })
        })
        .collect()
}

pub fn enumerate(sysfs: &Path, devices: &Path) -> IndexMap<String, Address> {
    let entries = match fs::read_dir(sysfs) {
        Ok(entries) => entries,
        Err(_) => return IndexMap::new(),
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            if !name.as_encoded_bytes().starts_with(b"event") {
                return None;
            }
            let root = entry.path().join("device");
            let bus = fs::read_to_string(root.join("id/bustype")).ok()?;
            if strip(&bus) != "0005" {
                return None;
            }
            let unique = fs::read_to_string(root.join("uniq")).ok()?;
            let address = Address::parse(strip(&unique)).ok()?;
            fs::read_to_string(root.join("name")).ok()?;
            Some((devices.join(name).to_str()?.to_owned(), address))
        })
        .collect()
}

fn strip(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

pub(crate) fn io_message(error: &io::Error, path: Option<&Path>) -> String {
    let text = error.to_string();
    let Some(code) = error.raw_os_error() else {
        return text;
    };
    let suffix = format!(" (os error {code})");
    let message = text.strip_suffix(&suffix).unwrap_or(&text);
    let mut result = format!("[Errno {code}] {message}");
    if let Some(path) = path {
        result.push_str(&format!(": '{}'", path.display()));
    }
    result
}
