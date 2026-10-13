use super::{transport::STREAM_BUFFER, WebDavError};
use crate::{Error, Observer, Progress};
use std::{
    fs::File,
    io::{self, Read},
};

pub(super) struct Stream<'a, 'b> {
    pub(super) file: File,
    pub(super) observer: &'a mut Observer<'b>,
    pub(super) filename: &'a str,
    pub(super) size: u64,
    pub(super) sent: u64,
    pub(super) error: Option<Error>,
}
impl Read for Stream<'_, '_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let result: Result<usize, Error> = (|| {
            self.observer.check()?;
            let remaining =
                usize::try_from(self.size.saturating_sub(self.sent)).unwrap_or(usize::MAX);
            let limit = output.len().min(STREAM_BUFFER).min(remaining);
            if limit == 0 {
                return Ok(0);
            }
            let count = self
                .file
                .read(&mut output[..limit])
                .map_err(|_| WebDavError::File)?;
            if count == 0 {
                return Err(WebDavError::File.into());
            }
            self.sent = self
                .sent
                .checked_add(u64::try_from(count).map_err(|_| WebDavError::File)?)
                .ok_or(WebDavError::File)?;
            self.observer.progress(Progress {
                filename: self.filename,
                sent: self.sent,
                size: self.size,
                chunk: count,
            })?;
            Ok(count)
        })();
        match result {
            Ok(count) => Ok(count),
            Err(error) => {
                let message = error.to_string();
                self.error = Some(error);
                Err(io::Error::other(message))
            }
        }
    }
}
