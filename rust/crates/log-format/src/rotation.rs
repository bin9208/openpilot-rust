use crate::{
    filename::{numeric_suffix, path_order},
    format_record, Error,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy)]
pub struct RotationSettings {
    pub interval: f64,
    pub max_bytes: u64,
    pub backup_count: usize,
}
impl Default for RotationSettings {
    fn default() -> Self {
        Self {
            interval: 60.,
            max_bytes: 256 * 1024,
            backup_count: 2500,
        }
    }
}
pub struct LogFiles<C> {
    clock: C,
    base_filename: PathBuf,
    settings: RotationSettings,
    log_files: Vec<PathBuf>,
    last_index: Option<String>,
    last_rollover: f64,
    stream: Option<BufWriter<File>>,
}
impl<C: FnMut() -> f64> LogFiles<C> {
    pub fn new(base_filename: &Path, settings: RotationSettings, clock: C) -> Result<Self, Error> {
        let directory = base_filename.parent().ok_or(Error::FileIndex)?;
        let mut log_files = Vec::new();
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path
                .as_os_str()
                .as_encoded_bytes()
                .starts_with(base_filename.as_os_str().as_encoded_bytes())
                && path.is_file()
            {
                log_files.push(path);
            }
        }
        // Preserve the original ascending startup list: rollover inserts at the
        // front and retention pops the end, even when that deletes newer files.
        log_files.sort_by_cached_key(|path| path_order(path));
        let mut last_index: Option<String> = None;
        for path in &log_files {
            let name = path.to_string_lossy();
            if let Some(suffix) = name.rsplit('.').next() {
                if let Some(index) = numeric_suffix(suffix)? {
                    if last_index
                        .as_ref()
                        .is_none_or(|last| (index.len(), &index) > (last.len(), last))
                    {
                        last_index = Some(index);
                    }
                }
            }
        }
        let mut handler = Self {
            clock,
            base_filename: base_filename.to_owned(),
            settings,
            log_files,
            last_index,
            last_rollover: 0.,
            stream: None,
        };
        handler.rollover()?;
        Ok(handler)
    }
    fn rollover(&mut self) -> Result<(), Error> {
        // BaseRotatingHandler closes before opening; failed opens leave the
        // handler unusable rather than silently retrying a different filename.
        self.close()?;
        self.last_rollover = (self.clock)();
        let mut next = self.last_index.take().map_or_else(
            || vec![b'0'],
            |value| {
                let mut digits = value.into_bytes();
                let mut carry = true;
                for digit in digits.iter_mut().rev() {
                    if *digit == b'9' {
                        *digit = b'0';
                    } else {
                        *digit += 1;
                        carry = false;
                        break;
                    }
                }
                if carry {
                    digits.insert(0, b'1');
                }
                digits
            },
        );
        let index = String::from_utf8(next.clone()).map_err(|_| Error::FileIndex)?;
        self.last_index = Some(index);
        if next.len() < 10 {
            next.splice(0..0, std::iter::repeat_n(b'0', 10 - next.len()));
        }
        let mut filename = self.base_filename.as_os_str().to_owned();
        filename.push(".");
        filename.push(String::from_utf8(next).map_err(|_| Error::FileIndex)?);
        let filename = PathBuf::from(filename);
        let mut stream = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&filename)?;
        stream.seek(SeekFrom::End(0))?;
        self.log_files.insert(0, filename);
        self.stream = Some(BufWriter::new(stream));
        if self.settings.backup_count > 0 {
            while self.log_files.len() > self.settings.backup_count {
                if let Some(path) = self.log_files.pop() {
                    if path.exists() {
                        fs::remove_file(path)?;
                    }
                }
            }
        }
        Ok(())
    }
    /// Flush on explicit daemon shutdown; source close ignores a previously
    /// closed rollover stream but propagates a pending buffered I/O failure.
    pub fn close(&mut self) -> Result<(), Error> {
        if let Some(mut stream) = self.stream.take() {
            stream.flush()?;
        }
        Ok(())
    }

    /// Errors mirror logging.Handler.emit's caught file/format exceptions. The
    /// daemon reports them and still publishes the original unformatted record.
    pub fn emit(&mut self, record: &str) -> Result<(), Error> {
        let size_exceeded = self.settings.max_bytes > 0
            && self
                .stream
                .as_mut()
                .ok_or(Error::Closed)?
                .stream_position()?
                >= self.settings.max_bytes;
        let time_exceeded = self.settings.interval > 0.
            && self.last_rollover + self.settings.interval <= (self.clock)();
        if size_exceeded || time_exceeded {
            self.rollover()?;
        }
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(Error::Random)?;
        let id = uuid::Builder::from_random_bytes(random).into_uuid();
        let mut formatted = format_record(record, id)?;
        formatted.push('\n');
        let stream = self.stream.as_mut().ok_or(Error::Closed)?;
        stream.write_all(formatted.as_bytes())?;
        stream.flush()?;
        Ok(())
    }
}
